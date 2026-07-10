use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nalgebra::Vector3;
use vulkano::command_buffer::CommandBufferExecFuture;
use vulkano::sync::future::{FenceSignalFuture, NowFuture};

use crate::atmosphere;
use crate::block_update::BlockUpdateQueue;
use crate::chunk_loader::ChunkLoader;
use crate::config::WorldGenType;
use crate::falling_block::FallingBlockSystem;
use crate::lava::LavaGrid;
use crate::particles::ParticleSystem;
use crate::pictures::PictureLibrary;
use crate::player::Player;
use crate::render_mode::RenderMode;
use crate::storage;
use crate::sub_voxel::ModelRegistry;
use crate::terrain_gen::TerrainGenerator;
use crate::utils::{ChunkStats, Profiler};
use crate::water::WaterGrid;
use crate::world::{ChunkPos, World};
use crate::world_streaming::MetadataState;

/// Number of chunks persisted per steady-state auto-save tick. Raised from 10
/// so dirty chunks drain faster in active play; the unload path
/// ([`WorldSim::unload_chunk`]) is the primary guard against data loss.
const AUTO_SAVE_CHUNK_BUDGET: usize = 64;

/// Type alias for the fence future returned by texture clear commands.
pub type ClearFence = FenceSignalFuture<CommandBufferExecFuture<NowFuture>>;

/// GPU texture streaming state extracted from [`WorldSim`].
///
/// These fields drive the GPU texture streaming pipeline and have no place in
/// the simulation layer.  Keeping them separate makes it clear that they are
/// touched only by `world_streaming` and the GPU upload path, not by gameplay
/// logic.
pub struct StreamingState {
    /// Incremental reupload queue after origin shifts to avoid stalls.
    pub reupload_queue: std::collections::VecDeque<Vector3<i32>>,
    /// Deferred chunk uploads when too many complete in one frame.
    /// Stores (position, chunk) pairs to be inserted and uploaded next frame.
    pub deferred_uploads: std::collections::VecDeque<crate::chunk_loader::ChunkResult>,
    /// Pending texture clear fence from async origin shift.
    /// Uploads are delayed until this fence signals completion.
    pub pending_clear_fence: Option<ClearFence>,
}

impl StreamingState {
    pub fn new() -> Self {
        Self {
            reupload_queue: std::collections::VecDeque::new(),
            deferred_uploads: std::collections::VecDeque::new(),
            pending_clear_fence: None,
        }
    }

    /// Clears all queued GPU work.  Call when the world is reset (e.g. when
    /// connecting to a server with a different seed).
    pub fn clear(&mut self) {
        self.reupload_queue.clear();
        self.deferred_uploads.clear();
        // Drop (and therefore wait for) any outstanding fence.
        self.pending_clear_fence = None;
    }
}

impl Default for StreamingState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct WorldSim {
    pub world: World,
    pub model_registry: ModelRegistry,
    pub terrain_generator: TerrainGenerator,
    pub player: Player,
    pub world_extent: [u32; 3],
    pub texture_origin: Vector3<i32>,
    pub last_player_chunk: Vector3<i32>,
    pub chunk_stats: ChunkStats,
    pub chunk_loader: ChunkLoader,
    pub storage: Arc<storage::worker::StorageSystem>,

    pub particles: ParticleSystem,
    pub falling_blocks: FallingBlockSystem,
    pub block_updates: BlockUpdateQueue,
    pub water_grid: WaterGrid,
    pub lava_grid: LavaGrid,

    pub time_of_day: f32,
    pub day_cycle_paused: bool,
    pub atmosphere: atmosphere::AtmosphereSettings,
    pub animation_time: f32,

    pub render_mode: RenderMode,
    pub view_distance: i32,
    pub load_distance: i32,
    pub unload_distance: i32,

    pub profiler: Profiler,

    pub metadata_state: MetadataState,
    /// GPU texture streaming state.  Extracted into its own struct to isolate
    /// GPU concerns from simulation logic.
    pub streaming: StreamingState,
    /// Most recent texture origin shift positions for HUD/debug.
    pub last_origin_shift: Option<Vector3<i32>>,
    /// Count of origin shifts in this session.
    pub origin_shift_count: u32,
    pub last_save: Instant,
    pub world_dir: PathBuf,
    pub world_name: String,
    pub seed: u32,
    pub world_gen: WorldGenType,

    /// True once this world has genuine local player edits (block placement /
    /// breaking / shape tools / console edits). Mirrors the persisted
    /// `WorldMetadata::player_modified` flag and is flipped on by the
    /// `save_dirty` / `save_all` / `unload_chunk` paths the first time a
    /// locally-dirty chunk is actually persisted (network-received chunks have
    /// `persistence_dirty = false`, so dirty chunks are by definition player
    /// edits). Used together with `remote_client` to gate client-side saves.
    pub player_modified: bool,
    /// True when this `WorldSim` is a remote client streaming another host's
    /// world. Set when the client applies the server's seed via
    /// `set_world_seed`. Remote clients suppress local saves until
    /// `player_modified` is true so a cached/downloaded server world does not
    /// overwrite this player's own local save. Host and single-player leave
    /// this `false`; saves then proceed unconditionally.
    pub remote_client: bool,

    /// Picture library for storing user-created artwork.
    pub picture_library: PictureLibrary,
}

impl WorldSim {
    pub fn auto_save(
        &mut self,
        measurement_markers: &[Vector3<i32>],
        stencil_manager: &crate::stencils::StencilManager,
    ) {
        let now = Instant::now();
        if now.duration_since(self.last_save) > Duration::from_secs(30) {
            self.save_dirty(AUTO_SAVE_CHUNK_BUDGET);

            // STOR-M06: persist fluid sources + stencil state on the autosave
            // path so a crash between autosaves no longer loses every
            // water/lava source and active stencil (previously these were only
            // saved by save_all on clean exit). Same STOR-004 remote-client
            // gate as save_all: a non-editing client must not overwrite its own
            // local fluids/stencils with state streamed from the server.
            if !self.remote_client || self.player_modified {
                let fluid_sources = storage::fluid_sources::FluidSources {
                    water: self.water_grid.get_source_positions(),
                    lava: self.lava_grid.get_source_positions(),
                };
                if let Err(e) = fluid_sources.save(&self.world_dir) {
                    log::error!("[Storage] Failed to save fluid sources: {}", e);
                }

                let stencil_state =
                    storage::stencil_state::StencilState::from_manager(stencil_manager);
                if let Err(e) = stencil_state.save(&self.world_dir) {
                    log::error!("[Storage] Failed to save stencil state: {}", e);
                }
            }

            self.save_metadata(measurement_markers);
            // Update last_save even if nothing was saved, to wait for the next interval
            self.last_save = now;
        }
    }

    pub fn save_metadata(&self, measurement_markers: &[Vector3<i32>]) {
        // STOR-004 client gate: a remote client that has not made local edits
        // must NOT overwrite its own level.dat with the server's seed/world_gen
        // (that would corrupt the local world's metadata for a different save).
        // Once `player_modified` is flipped by save_dirty/save_all/unload_chunk,
        // metadata saves proceed so the player's edits are recorded.
        if self.remote_client && !self.player_modified {
            log::debug!(
                "[Storage] Remote client skipping metadata save (no local edits; \
                 refusing to overwrite local level.dat)"
            );
            return;
        }

        let player_pos = self.player.feet_pos(self.world_extent, self.texture_origin);

        let meta = storage::metadata::WorldMetadata {
            seed: self.seed,
            spawn_pos: [player_pos.x, player_pos.y, player_pos.z], // Legacy field, keeping updated
            version: 2,
            time_of_day: self.time_of_day,
            day_cycle_paused: self.day_cycle_paused,
            world_gen: self.world_gen,
            measurement_markers: measurement_markers
                .iter()
                .map(|v| [v.x, v.y, v.z])
                .collect(),
            player_modified: self.player_modified,
        };

        if let Err(e) = meta.save(self.world_dir.join("level.dat")) {
            log::error!("[Storage] Failed to save metadata: {}", e);
        }
    }

    pub fn save_dirty(&mut self, limit: usize) {
        let mut saved_count = 0;
        for (pos, chunk) in self.world.chunks_mut() {
            if chunk.persistence_dirty {
                let serialized = storage::format::SerializedChunk::from(&*chunk);
                self.storage.save_chunk(*pos, serialized);
                chunk.persistence_dirty = false;
                // Dirty chunks are exactly local player edits (network-received
                // chunks have persistence_dirty = false), so persisting one
                // means this world is now player-modified. STOR-004.
                self.player_modified = true;
                saved_count += 1;
                if saved_count >= limit {
                    break;
                }
            }
        }
        if saved_count > 0 && limit < 1000 {
            log::debug!("[Storage] Auto-saved {} chunks", saved_count);
        }
    }

    /// Removes a chunk from the loaded set, persisting it first if it has
    /// unsaved player edits. Mirrors the persist idiom in [`WorldSim::save_dirty`].
    ///
    /// Returns `true` if a chunk was actually removed. This is the STOR-002
    /// fix: previously the streaming unload path dropped the returned `Chunk`
    /// without checking `persistence_dirty`, silently losing any player edits
    /// in chunks unloaded at the view-distance boundary.
    pub fn unload_chunk<S: Into<ChunkPos>>(&mut self, pos: S) -> bool {
        let pos: ChunkPos = pos.into();
        self.chunk_loader.cancel_chunk(pos.0);
        if let Some(chunk) = self.world.remove_chunk(pos) {
            if chunk.persistence_dirty {
                let serialized = storage::format::SerializedChunk::from(&chunk);
                self.storage.save_chunk(pos, serialized);
                // Same STOR-004 invariant as save_dirty: a dirty chunk being
                // persisted on unload is a local player edit, so the world is
                // now player-modified. Without this the flag would never flip
                // for edits in chunks unloaded at the view-distance boundary.
                self.player_modified = true;
            }
            true
        } else {
            false
        }
    }

    pub fn save_all(
        &mut self,
        measurement_markers: &[Vector3<i32>],
        stencil_manager: &crate::stencils::StencilManager,
    ) {
        let mut saved_count = 0;
        for (pos, chunk) in self.world.chunks_mut() {
            if chunk.persistence_dirty {
                let serialized = storage::format::SerializedChunk::from(&*chunk);
                self.storage.save_chunk(*pos, serialized);
                chunk.persistence_dirty = false;
                // Same STOR-004 invariant as save_dirty: persisting a dirty
                // chunk means this world is now player-modified.
                self.player_modified = true;
                saved_count += 1;
            }
        }
        log::debug!("[Storage] Saved {} chunks to disk", saved_count);

        // STOR-004 client gate: the dirty-chunk flush above always runs (no
        // data loss for player edits), but a remote client that has not made
        // local edits must not overwrite its own world's fluids/stencils/
        // models/metadata with state streamed from the server. Once any dirty
        // chunk was persisted this tick, `player_modified` is true and the
        // remaining state is written too.
        if self.remote_client && !self.player_modified {
            log::debug!(
                "[Storage] Remote client skipping save_all sidecar/metadata writes \
                 (no local edits; refusing to overwrite local world state)"
            );
            return;
        }

        // Save fluid sources (water/lava with is_source=true)
        let fluid_sources = storage::fluid_sources::FluidSources {
            water: self.water_grid.get_source_positions(),
            lava: self.lava_grid.get_source_positions(),
        };
        if let Err(e) = fluid_sources.save(&self.world_dir) {
            log::error!("[Storage] Failed to save fluid sources: {}", e);
        } else {
            let total = fluid_sources.water.len() + fluid_sources.lava.len();
            if total > 0 {
                log::debug!(
                    "[Storage] Saved {} fluid sources ({} water, {} lava)",
                    total,
                    fluid_sources.water.len(),
                    fluid_sources.lava.len()
                );
            }
        }

        // Save stencil state (active stencils in world)
        let stencil_state = storage::stencil_state::StencilState::from_manager(stencil_manager);
        if let Err(e) = stencil_state.save(&self.world_dir) {
            log::error!("[Storage] Failed to save stencil state: {}", e);
        } else if !stencil_manager.active_stencils.is_empty() {
            log::debug!(
                "[Storage] Saved {} active stencils",
                stencil_manager.active_stencils.len()
            );
        }

        // Persist custom models so their IDs survive library churn between
        // sessions. Models are snapshotted in ID order (see
        // [`ModelRegistry::to_world_store`]) so stored index i maps exactly to
        // registry ID FIRST_CUSTOM_MODEL_ID + i. On reload, models.dat is loaded
        // before the library so saved custom-model IDs stay stable.
        let model_store = self.model_registry.to_world_store();
        if let Err(e) = model_store.save(&self.world_dir) {
            log::error!("[Storage] Failed to save model store: {}", e);
        } else {
            let count = model_store.len();
            if count > 0 {
                log::debug!("[Storage] Saved {} custom models", count);
            }
        }

        self.save_metadata(measurement_markers);
    }

    /// Redirects subsequent saves to a per-server cache directory (STOR-005).
    ///
    /// Called once when a pure client applies the server's seed. Swaps both
    /// [`WorldSim::world_dir`] and [`WorldSim::storage`] so every save path
    /// (`save_metadata` / `save_all` / `save_dirty` / `unload_chunk`) targets
    /// the cache dir for this server. Must be called BEFORE
    /// [`WorldSim::set_world_seed`]: that recreates the chunk loader against
    /// `self.world_dir`, and we want local generation / reload on reconnect to
    /// read from the cache dir, not the local world dir.
    ///
    /// On failure (cache dir cannot be created) the existing `world_dir` is
    /// left intact so saves still land somewhere durable rather than being
    /// silently dropped. Host and single-player never call this; their saves
    /// stay on the local world dir byte-identical to pre-STOR-005.
    pub fn enter_remote_client_mode(&mut self, cache_dir: PathBuf) {
        if cache_dir == self.world_dir {
            return;
        }
        if let Err(e) = std::fs::create_dir_all(&cache_dir) {
            log::error!(
                "[Storage] Failed to create remote client cache dir {}: {}; \
                 saves will continue against the existing world dir",
                cache_dir.display(),
                e
            );
            return;
        }
        log::debug!(
            "[Storage] Remote client redirecting saves to cache dir: {}",
            cache_dir.display()
        );
        self.world_dir = cache_dir;
        // Replace the storage worker so chunks are persisted to the cache dir's
        // region/ subtree. The old worker is dropped (Shutdown + join) when this
        // is the last Arc reference — no other code holds a clone.
        self.storage = Arc::new(storage::worker::StorageSystem::new(self.world_dir.clone()));
    }

    /// Updates the terrain generator and chunk loader with a new seed.
    /// Used when a client connects to a server and needs to use the server's world seed.
    /// Also clears the current world to start fresh with the server's world.
    pub fn set_world_seed(&mut self, seed: u32, world_gen: WorldGenType) {
        log::debug!(
            "[WorldSim] Updating world seed to {} (world_gen: {:?})",
            seed,
            world_gen
        );

        // Update seed and world_gen
        self.seed = seed;
        self.world_gen = world_gen;

        // Swapping in a fresh server world means no local edits survive — reset
        // the STOR-004 flag so client-side saves stay gated until the player
        // edits this new world. The caller (client seed-apply path) sets
        // `remote_client = true` separately to arm the gate.
        self.player_modified = false;

        // Clear the current world - we're loading a new world from the server
        let chunk_count = self.world.chunk_count();
        self.world.clear();
        log::debug!("[WorldSim] Cleared {} chunks from local world", chunk_count);

        // Clear fluid grids
        self.water_grid.clear();
        self.lava_grid.clear();

        // Clear block update queue
        self.block_updates.clear();

        // Clear falling blocks
        self.falling_blocks.clear();

        // Clear GPU streaming state
        self.streaming.clear();

        // Create new terrain generator with the new seed
        self.terrain_generator = TerrainGenerator::new(seed);

        // Recreate chunk loader with new terrain generator
        let terrain = self.terrain_generator.clone();
        let benchmark_terrain = match world_gen {
            WorldGenType::Benchmark => crate::config::BenchmarkTerrain::Hills,
            _ => crate::config::BenchmarkTerrain::Flat,
        };
        let world_dir = self.world_dir.clone();

        self.chunk_loader = ChunkLoader::new(
            move |pos| {
                // Generate chunk with overflow blocks for cross-chunk structures
                crate::terrain_gen::generate_chunk_terrain(
                    &terrain,
                    pos,
                    world_gen,
                    benchmark_terrain,
                )
            },
            Some(world_dir),
        );

        log::debug!("[WorldSim] Chunk loader updated with new seed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{BlockType, Chunk};
    use crate::storage::worker::StorageSystem;
    use std::path::PathBuf;
    use tempfile::tempdir;

    /// Regression test for STOR-002.
    ///
    /// Proves the storage round-trip portion of [`WorldSim::unload_chunk`]: a
    /// chunk that was dirty at unload time is recoverable from disk afterward.
    /// Before the fix, the unload path dropped the returned `Chunk` without
    /// inspecting `persistence_dirty`, so dirty chunks were silently lost.
    ///
    /// Covered:
    ///   - The real `StorageSystem` worker thread, region file format, and
    ///     compress/decompress path (no mock).
    ///   - The exact persist idiom used by `unload_chunk`:
    ///     `SerializedChunk::from(&chunk)` then `storage.save_chunk(pos, _)`.
    ///   - Flush strategy: `drop(StorageSystem)` sends `Shutdown` and joins the
    ///     worker thread ([`StorageWorker::run`] returns on `Shutdown`), which
    ///     guarantees every prior `Save` command is fully written and flushed
    ///     to the region file before the system goes away. We then build a
    ///     *fresh* `StorageSystem` against the same temp dir and read the chunk
    ///     back, proving the bytes are actually on disk (not just in the
    ///     worker's in-memory region cache).
    ///   - The dirty-decision: a non-dirty chunk is NOT persisted.
    ///
    /// Not covered (requires full GPU/Vulkan init via `src/app/init.rs`):
    ///   - The `WorldSim` orchestration (cancel_chunk + world.remove_chunk +
    ///     dirty branch). `WorldSim` cannot be constructed in a `#[cfg(test)]`
    ///     fixture without spinning up the whole renderer; the storage path is
    ///     the load-bearing part for data loss and is fully exercised here.
    #[test]
    fn dirty_chunk_survives_unload_persist_round_trip() {
        let dir: PathBuf = tempdir().expect("tempdir").keep();
        // Region files only address chunk Y in [0, 16); use an in-range slot.
        let pos = ChunkPos::new(7, 3, 12);

        // --- Phase A: a dirty chunk goes through the unload_chunk persist step. ---
        {
            let storage = StorageSystem::new(dir.clone());
            let mut chunk = Chunk::new();
            // Simulate a generated chunk (no player edits yet): not dirty.
            chunk.persistence_dirty = false;
            // Player places a block — set_block flips persistence_dirty to true
            // (mark_mutated funnel) exactly as real edits do.
            chunk.set_block(1, 2, 3, BlockType::Stone);
            assert!(chunk.persistence_dirty, "set_block must mark dirty");

            // Mirror the unload_chunk persist branch exactly.
            if chunk.persistence_dirty {
                let serialized = storage::format::SerializedChunk::from(&chunk);
                storage.save_chunk(pos, serialized);
            }
            // Chunk is dropped here (mirrors the unload path dropping the
            // removed chunk after persisting). StorageSystem drops below,
            // joining the worker thread and guaranteeing the save is on disk.
        }

        // --- Phase B: reload from a fresh StorageSystem pointed at the same dir. ---
        let reloaded = {
            let storage = StorageSystem::new(dir.clone());
            storage
                .load_chunk(pos)
                .expect("storage load should succeed")
                .expect("dirty chunk should have been persisted to disk")
        };
        assert_eq!(
            reloaded.get_block(1, 2, 3),
            BlockType::Stone,
            "edited block must survive unload+reload (the STOR-002 regression)"
        );

        // --- Phase C: a non-dirty chunk is NOT persisted (no half-generated data). ---
        // Procedural generation writes via set_block_generated, which does NOT
        // flip persistence_dirty. The unload_chunk guard only saves when dirty,
        // so a freshly-generated chunk unloaded without edits writes nothing.
        let mut generated = Chunk::new();
        generated.set_block_generated(4, 5, 6, BlockType::Dirt);
        generated.persistence_dirty = false;
        assert!(
            !generated.persistence_dirty,
            "non-dirty chunk must skip the persist branch in unload_chunk"
        );

        // Confirm the slot the non-dirty chunk would have occupied is empty.
        let none_loaded = {
            let storage = StorageSystem::new(dir.clone());
            let other = ChunkPos::new(8, 3, 12);
            storage
                .load_chunk(other)
                .expect("storage load should succeed")
        };
        assert!(
            none_loaded.is_none(),
            "non-dirty (purely generated) chunk must not be persisted"
        );
    }
}

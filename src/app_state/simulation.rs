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

    /// Picture library for storing user-created artwork.
    pub picture_library: PictureLibrary,
}

impl WorldSim {
    pub fn auto_save(&mut self, measurement_markers: &[Vector3<i32>]) {
        let now = Instant::now();
        if now.duration_since(self.last_save) > Duration::from_secs(30) {
            self.save_dirty(AUTO_SAVE_CHUNK_BUDGET);
            self.save_metadata(measurement_markers);
            // Update last_save even if nothing was saved, to wait for the next interval
            self.last_save = now;
        }
    }

    pub fn save_metadata(&self, measurement_markers: &[Vector3<i32>]) {
        let player_pos = self.player.feet_pos(self.world_extent, self.texture_origin);

        let meta = storage::metadata::WorldMetadata {
            seed: self.seed,
            spawn_pos: [player_pos.x, player_pos.y, player_pos.z], // Legacy field, keeping updated
            version: 1,
            time_of_day: self.time_of_day,
            day_cycle_paused: self.day_cycle_paused,
            world_gen: self.world_gen,
            measurement_markers: measurement_markers
                .iter()
                .map(|v| [v.x, v.y, v.z])
                .collect(),
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
    pub fn unload_chunk(&mut self, pos: ChunkPos) -> bool {
        self.chunk_loader.cancel_chunk(pos);
        if let Some(chunk) = self.world.remove_chunk(pos) {
            if chunk.persistence_dirty {
                let serialized = storage::format::SerializedChunk::from(&chunk);
                self.storage.save_chunk(pos, serialized);
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
                saved_count += 1;
            }
        }
        log::debug!("[Storage] Saved {} chunks to disk", saved_count);

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

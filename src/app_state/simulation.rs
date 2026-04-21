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
use crate::world::World;
use crate::world_streaming::MetadataState;

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
            self.save_dirty(10);
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

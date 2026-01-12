use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nalgebra::Vector3;

use crate::atmosphere;
use crate::block_update::BlockUpdateQueue;
use crate::chunk_loader::ChunkLoader;
use crate::config::WorldGenType;
use crate::falling_block::FallingBlockSystem;
use crate::lava::LavaGrid;
use crate::particles::ParticleSystem;
use crate::player::Player;
use crate::render_mode::RenderMode;
use crate::storage;
use crate::sub_voxel::ModelRegistry;
use crate::terrain_gen::TerrainGenerator;
use crate::utils::{ChunkStats, Profiler};
use crate::water::WaterGrid;
use crate::world::World;
use crate::world_streaming::MetadataState;

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
    pub last_save: Instant,
    pub world_dir: PathBuf,
    pub world_name: String,
    pub seed: u32,
    pub world_gen: WorldGenType,
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
            eprintln!("[Storage] Failed to save metadata: {}", e);
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
            println!("[Storage] Auto-saved {} chunks", saved_count);
        }
    }

    pub fn save_all(&mut self, measurement_markers: &[Vector3<i32>]) {
        let mut saved_count = 0;
        for (pos, chunk) in self.world.chunks_mut() {
            if chunk.persistence_dirty {
                let serialized = storage::format::SerializedChunk::from(&*chunk);
                self.storage.save_chunk(*pos, serialized);
                chunk.persistence_dirty = false;
                saved_count += 1;
            }
        }
        println!("[Storage] Saved {} chunks to disk", saved_count);

        // Save fluid sources (water/lava with is_source=true)
        let fluid_sources = storage::fluid_sources::FluidSources {
            water: self.water_grid.get_source_positions(),
            lava: self.lava_grid.get_source_positions(),
        };
        if let Err(e) = fluid_sources.save(&self.world_dir) {
            eprintln!("[Storage] Failed to save fluid sources: {}", e);
        } else {
            let total = fluid_sources.water.len() + fluid_sources.lava.len();
            if total > 0 {
                println!(
                    "[Storage] Saved {} fluid sources ({} water, {} lava)",
                    total,
                    fluid_sources.water.len(),
                    fluid_sources.lava.len()
                );
            }
        }

        self.save_metadata(measurement_markers);
    }
}

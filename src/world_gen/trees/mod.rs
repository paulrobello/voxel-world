//! Tree generation system.
//!
//! Generates various tree types based on biome.

mod cactus;
mod oak;
mod pine;
mod snow;
mod willow;

use crate::chunk::{CHUNK_SIZE, Chunk};
use crate::world_gen::SEA_LEVEL;
use crate::world_gen::biome::BiomeType;
use crate::world_gen::terrain::TerrainGenerator;
use crate::world_gen::utils::OverflowBlock;

pub use cactus::generate_cactus;
pub use oak::generate_oak;
pub use pine::generate_pine;
pub use snow::{generate_dead_tree, generate_snow_pine};
pub use willow::generate_willow;

/// Generates trees for a chunk based on biome.
pub fn generate_trees(
    chunk: &mut Chunk,
    terrain: &TerrainGenerator,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    // Trees can now span chunks freely with overflow support
    // No boundary guards - trees can spawn at chunk edges and overflow into neighbors
    for lx in (0..CHUNK_SIZE).step_by(4) {
        for lz in (0..CHUNK_SIZE).step_by(4) {
            let world_x = chunk_world_x + lx as i32;
            let world_z = chunk_world_z + lz as i32;
            let height = terrain.get_height(world_x, world_z);
            // Use primary biome for tree type selection to avoid blending issues
            // Swamps blend heavily with grasslands, causing inconsistent tree spawning
            let biome_info = terrain.get_biome_info(world_x, world_z);
            let biome = biome_info.biome;
            let local_base_y = height - chunk_world_y;

            // Check if tree base is in this chunk
            // Buffer at top ensures enough of tree is in this chunk for proper generation
            // (overflow handles canopy extending into next chunk)
            if local_base_y < 0 || local_base_y >= (CHUNK_SIZE as i32 - 5) {
                continue;
            }

            // Don't spawn trees underwater (only prevent if below sea level)
            if height < SEA_LEVEL {
                continue;
            }

            // Check terrain slope - prevent trees on steep terrain
            let height_n = terrain.get_height(world_x, world_z + 4);
            let height_s = terrain.get_height(world_x, world_z - 4);
            let height_e = terrain.get_height(world_x + 4, world_z);
            let height_w = terrain.get_height(world_x - 4, world_z);
            let max_height_diff = (height_n - height)
                .abs()
                .max((height_s - height).abs())
                .max((height_e - height).abs())
                .max((height_w - height).abs());

            // Skip if terrain is too steep (more than 3 blocks difference in 4 block radius)
            if max_height_diff > 3 {
                continue;
            }

            // Randomness
            let hash = terrain.hash(world_x, world_z);

            #[allow(deprecated)]
            match biome {
                // Plains and grassland - sparse oak trees
                BiomeType::Plains | BiomeType::Grassland => {
                    if hash % 100 < 5 {
                        generate_oak(
                            chunk,
                            lx as i32,
                            local_base_y,
                            lz as i32,
                            hash,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    }
                }

                // Meadow - very sparse trees
                BiomeType::Meadow => {
                    if hash % 100 < 3 {
                        generate_oak(
                            chunk,
                            lx as i32,
                            local_base_y,
                            lz as i32,
                            hash,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    }
                }

                // Forest - dense oak trees
                BiomeType::Forest => {
                    if hash % 100 < 25 {
                        generate_oak(
                            chunk,
                            lx as i32,
                            local_base_y,
                            lz as i32,
                            hash,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    }
                }

                // Birch forest - oak trees (would need birch tree type for proper implementation)
                BiomeType::BirchForest => {
                    if hash % 100 < 20 {
                        generate_oak(
                            chunk,
                            lx as i32,
                            local_base_y,
                            lz as i32,
                            hash,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    }
                }

                // Dark forest - very dense
                BiomeType::DarkForest => {
                    if hash % 100 < 35 {
                        generate_oak(
                            chunk,
                            lx as i32,
                            local_base_y,
                            lz as i32,
                            hash,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    }
                }

                // Taiga - pine trees
                BiomeType::Taiga => {
                    if hash % 100 < 18 {
                        generate_pine(
                            chunk,
                            lx as i32,
                            local_base_y,
                            lz as i32,
                            hash,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    }
                }

                // Snowy taiga - dense snow-covered pines
                BiomeType::SnowyTaiga => {
                    if hash % 100 < 20 {
                        generate_snow_pine(
                            chunk,
                            lx as i32,
                            local_base_y,
                            lz as i32,
                            hash,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    }
                }

                // Snowy plains - sparse trees
                BiomeType::SnowyPlains | BiomeType::Snow => {
                    let tree_roll = hash % 100;
                    if tree_roll < 6 {
                        generate_snow_pine(
                            chunk,
                            lx as i32,
                            local_base_y,
                            lz as i32,
                            hash,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    } else if tree_roll < 14 {
                        generate_dead_tree(
                            chunk,
                            lx as i32,
                            local_base_y,
                            lz as i32,
                            hash,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    }
                }

                // Jungle - very dense trees (use oak for now)
                BiomeType::Jungle => {
                    if hash % 100 < 40 {
                        generate_oak(
                            chunk,
                            lx as i32,
                            local_base_y,
                            lz as i32,
                            hash,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    }
                }

                // Savanna - sparse trees
                BiomeType::Savanna => {
                    if hash % 100 < 6 {
                        generate_oak(
                            chunk,
                            lx as i32,
                            local_base_y,
                            lz as i32,
                            hash,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    }
                }

                // Mountains - low density pines below snow line
                BiomeType::Mountains => {
                    if height < 80 && hash % 100 < 3 {
                        generate_pine(
                            chunk,
                            lx as i32,
                            local_base_y,
                            lz as i32,
                            hash,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    }
                }

                // Swamp - willow trees
                BiomeType::Swamp => {
                    if hash % 100 < 12 {
                        generate_willow(
                            chunk,
                            lx as i32,
                            local_base_y,
                            lz as i32,
                            hash,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    }
                }

                // Desert - sparse cacti
                BiomeType::Desert => {
                    if hash % 100 < 2 {
                        generate_cactus(
                            chunk,
                            lx as i32,
                            local_base_y,
                            lz as i32,
                            hash,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    }
                }

                // Ocean, Beach - no trees
                BiomeType::Ocean | BiomeType::Beach => {}

                // Underground biomes - no surface trees
                BiomeType::LushCaves | BiomeType::DripstoneCaves | BiomeType::DeepDark => {}
            }
        }
    }
}

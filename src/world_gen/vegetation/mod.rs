//! Ground cover and vegetation generation.
//!
//! Handles grass, flowers, mushrooms, cave decorations, and other ground cover.

use crate::chunk::{BlockType, CHUNK_SIZE, Chunk};
use crate::world_gen::SEA_LEVEL;
use crate::world_gen::biome::BiomeType;
use crate::world_gen::terrain::TerrainGenerator;
use crate::world_gen::utils::OverflowBlock;

// Model IDs for ground cover
const MODEL_TALL_GRASS: u8 = 100;
const MODEL_FLOWER_RED: u8 = 101;
const MODEL_FLOWER_YELLOW: u8 = 102;
const MODEL_LILY_PAD: u8 = 103;
const MODEL_MUSHROOM_BROWN: u8 = 104;
// const MODEL_MUSHROOM_RED: u8 = 105;

// Cave vegetation model IDs
const MODEL_MOSS_CARPET: u8 = 110;
const MODEL_GLOW_LICHEN: u8 = 111;
const MODEL_HANGING_ROOTS: u8 = 112;
const MODEL_GLOW_BERRY_VINES: u8 = 113;
const MODEL_GLOW_MUSHROOM: u8 = 114;

/// Generates ground cover (grass, flowers, etc.) based on biome.
pub fn generate_ground_cover(
    chunk: &mut Chunk,
    terrain: &TerrainGenerator,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    _overflow_blocks: &mut Vec<OverflowBlock>,
) {
    for lx in 0..CHUNK_SIZE {
        for lz in 0..CHUNK_SIZE {
            let world_x = chunk_world_x + lx as i32;
            let world_z = chunk_world_z + lz as i32;
            let height = terrain.get_height(world_x, world_z);
            let biome = terrain.get_biome(world_x, world_z);
            let local_y = height - chunk_world_y;

            // Check if surface is in this chunk
            if local_y >= 0 && local_y < (CHUNK_SIZE as i32 - 1) {
                let y = (local_y + 1) as usize;
                let surface_block = chunk.get_block(lx, local_y as usize, lz);

                // Skip if already occupied
                if chunk.get_block(lx, y, lz) != BlockType::Air {
                    continue;
                }

                let hash = terrain.hash(world_x, world_z);

                #[allow(deprecated)]
                match biome {
                    // Plains and temperate grasslands
                    BiomeType::Plains | BiomeType::Grassland => {
                        if surface_block == BlockType::Grass {
                            if hash % 100 < 10 {
                                chunk.set_model_block(lx, y, lz, MODEL_TALL_GRASS, 0, false);
                            } else if hash % 100 < 12 {
                                let flower = if hash % 2 == 0 {
                                    MODEL_FLOWER_RED
                                } else {
                                    MODEL_FLOWER_YELLOW
                                };
                                chunk.set_model_block(lx, y, lz, flower, 0, false);
                            }
                        }
                    }

                    // Meadow - lots of flowers
                    BiomeType::Meadow => {
                        if surface_block == BlockType::Grass && hash % 100 < 15 {
                            let flower = if hash % 3 == 0 {
                                MODEL_FLOWER_RED
                            } else if hash % 3 == 1 {
                                MODEL_FLOWER_YELLOW
                            } else {
                                MODEL_TALL_GRASS
                            };
                            chunk.set_model_block(lx, y, lz, flower, 0, false);
                        }
                    }

                    // Forest biomes - moderate grass, some flowers
                    BiomeType::Forest | BiomeType::BirchForest => {
                        if surface_block == BlockType::Grass {
                            if hash % 100 < 8 {
                                chunk.set_model_block(lx, y, lz, MODEL_TALL_GRASS, 0, false);
                            } else if hash % 100 < 10 {
                                chunk.set_model_block(lx, y, lz, MODEL_FLOWER_RED, 0, false);
                            }
                        }
                    }

                    // Dark forest - mushrooms and less grass
                    BiomeType::DarkForest => {
                        if surface_block == BlockType::Grass || surface_block == BlockType::Dirt {
                            if hash % 100 < 12 {
                                chunk.set_model_block(lx, y, lz, MODEL_MUSHROOM_BROWN, 0, false);
                            } else if hash % 100 < 16 {
                                chunk.set_model_block(lx, y, lz, MODEL_TALL_GRASS, 0, false);
                            }
                        }
                    }

                    // Swamp - tall grass, mushrooms, lily pads
                    BiomeType::Swamp => {
                        if surface_block == BlockType::Grass
                            || surface_block == BlockType::Dirt
                            || surface_block == BlockType::Mud
                        {
                            if hash % 100 < 15 {
                                chunk.set_model_block(lx, y, lz, MODEL_TALL_GRASS, 0, false);
                            } else if hash % 100 < 20 {
                                chunk.set_model_block(lx, y, lz, MODEL_MUSHROOM_BROWN, 0, false);
                            }
                        }
                        // Lily pads on water
                        if surface_block == BlockType::Water
                            && height == SEA_LEVEL
                            && hash % 100 < 8
                        {
                            chunk.set_model_block(lx, y, lz, MODEL_LILY_PAD, 0, false);
                        }
                    }

                    // Jungle - dense vegetation
                    BiomeType::Jungle => {
                        if surface_block == BlockType::Grass {
                            if hash % 100 < 20 {
                                chunk.set_model_block(lx, y, lz, MODEL_TALL_GRASS, 0, false);
                            } else if hash % 100 < 25 {
                                let flower = if hash % 2 == 0 {
                                    MODEL_FLOWER_RED
                                } else {
                                    MODEL_FLOWER_YELLOW
                                };
                                chunk.set_model_block(lx, y, lz, flower, 0, false);
                            }
                        }
                    }

                    // Savanna - sparse grass
                    BiomeType::Savanna => {
                        if surface_block == BlockType::Grass && hash % 100 < 8 {
                            chunk.set_model_block(lx, y, lz, MODEL_TALL_GRASS, 0, false);
                        }
                    }

                    // Taiga and cold forests
                    BiomeType::Taiga | BiomeType::SnowyTaiga => {
                        if (surface_block == BlockType::Grass || surface_block == BlockType::Snow)
                            && hash % 100 < 6
                        {
                            chunk.set_model_block(lx, y, lz, MODEL_TALL_GRASS, 0, false);
                        }
                    }

                    // Snowy biomes - very sparse
                    BiomeType::SnowyPlains | BiomeType::Snow => {
                        if surface_block == BlockType::Snow && hash % 100 < 2 {
                            chunk.set_model_block(lx, y, lz, MODEL_TALL_GRASS, 0, false);
                        }
                    }

                    // Mountains - sparse grass
                    BiomeType::Mountains => {
                        if surface_block == BlockType::Grass && hash % 100 < 5 {
                            chunk.set_model_block(lx, y, lz, MODEL_TALL_GRASS, 0, false);
                        }
                    }

                    // Desert, Ocean, Beach - no ground cover
                    BiomeType::Desert | BiomeType::Ocean | BiomeType::Beach => {}

                    // Underground biomes - no surface vegetation
                    BiomeType::LushCaves | BiomeType::DripstoneCaves | BiomeType::DeepDark => {}
                }
            }
        }
    }
}

/// Generates cave decorations (stalactites/stalagmites, vegetation) in underground caves.
///
/// Uses 3D biome selection to place appropriate decorations based on underground biome type:
/// - Standard caves: Stone stalactites/stalagmites
/// - Snow biome caves: Ice stalactites/stalagmites
/// - Lush caves: Moss carpet, hanging roots, glow berries, glow lichen
/// - Dripstone caves: Denser stalactite/stalagmite formations
/// - Deep dark: Sparse decorations, glow mushrooms
pub fn generate_cave_decorations(
    chunk: &mut Chunk,
    terrain: &TerrainGenerator,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    _overflow_blocks: &mut Vec<OverflowBlock>,
) {
    // Cave decorations can now span chunks with overflow support
    if chunk_world_y > SEA_LEVEL {
        return; // Above sea level, unlikely to have deep caves
    }

    for lx in 0..CHUNK_SIZE {
        for lz in 0..CHUNK_SIZE {
            let world_x = chunk_world_x + lx as i32;
            let world_z = chunk_world_z + lz as i32;

            for ly in 0..CHUNK_SIZE {
                let world_y = chunk_world_y + ly as i32;
                let block = chunk.get_block(lx, ly, lz);

                // Use 3D biome for underground biome-aware decoration
                let biome = terrain.get_biome_3d(world_x, world_y, world_z);
                let hash = terrain.hash(world_x + world_y * 17, world_z);

                // Check for cave ceiling (solid block with air below)
                if (block == BlockType::Stone || block == BlockType::Deepslate) && ly > 0 {
                    let below = chunk.get_block(lx, ly - 1, lz);
                    if below == BlockType::Air {
                        // This is a cave ceiling - try biome-specific decorations first
                        if let Some(model_id) =
                            get_ceiling_decoration(biome, hash, terrain, world_x, world_y, world_z)
                        {
                            chunk.set_model_block(lx, ly - 1, lz, model_id, 0, false);
                        }
                    }
                }

                // Check for cave floor (solid block with air above)
                if (block == BlockType::Stone || block == BlockType::Deepslate)
                    && ly < CHUNK_SIZE - 1
                {
                    let above = chunk.get_block(lx, ly + 1, lz);
                    if above == BlockType::Air {
                        // This is a cave floor - try biome-specific decorations first
                        if let Some(model_id) =
                            get_floor_decoration(biome, hash, terrain, world_x, world_y, world_z)
                        {
                            chunk.set_model_block(lx, ly + 1, lz, model_id, 0, false);
                        }
                    }
                }
            }
        }
    }
}

/// Get ceiling decoration model based on biome.
fn get_ceiling_decoration(
    biome: BiomeType,
    hash: i32,
    terrain: &TerrainGenerator,
    world_x: i32,
    world_y: i32,
    world_z: i32,
) -> Option<u8> {
    #[allow(deprecated)]
    match biome {
        // Lush caves - hanging roots, glow berries, glow lichen
        BiomeType::LushCaves => {
            let roll = hash % 100;
            if roll < 8 {
                Some(MODEL_HANGING_ROOTS)
            } else if roll < 14 {
                Some(MODEL_GLOW_BERRY_VINES)
            } else if roll < 22 {
                Some(MODEL_GLOW_LICHEN)
            } else {
                // Still place some stalactites
                terrain
                    .cave_generator()
                    .should_place_stalactite(world_x, world_y, world_z, biome)
            }
        }

        // Dripstone caves - extra stalactites
        BiomeType::DripstoneCaves => terrain
            .cave_generator()
            .should_place_stalactite(world_x, world_y, world_z, biome),

        // Deep dark - sparse glow lichen
        BiomeType::DeepDark => {
            if hash % 100 < 3 {
                Some(MODEL_GLOW_LICHEN)
            } else {
                terrain
                    .cave_generator()
                    .should_place_stalactite(world_x, world_y, world_z, biome)
            }
        }

        // Standard caves - stalactites
        _ => terrain
            .cave_generator()
            .should_place_stalactite(world_x, world_y, world_z, biome),
    }
}

/// Get floor decoration model based on biome.
fn get_floor_decoration(
    biome: BiomeType,
    hash: i32,
    terrain: &TerrainGenerator,
    world_x: i32,
    world_y: i32,
    world_z: i32,
) -> Option<u8> {
    #[allow(deprecated)]
    match biome {
        // Lush caves - moss carpet, glow mushrooms
        BiomeType::LushCaves => {
            let roll = hash % 100;
            if roll < 15 {
                Some(MODEL_MOSS_CARPET)
            } else if roll < 20 {
                Some(MODEL_GLOW_MUSHROOM)
            } else if roll < 25 {
                Some(MODEL_MUSHROOM_BROWN)
            } else {
                // Still place some stalagmites
                terrain
                    .cave_generator()
                    .should_place_stalagmite(world_x, world_y, world_z, biome)
            }
        }

        // Dripstone caves - extra stalagmites
        BiomeType::DripstoneCaves => terrain
            .cave_generator()
            .should_place_stalagmite(world_x, world_y, world_z, biome),

        // Deep dark - sparse glow mushrooms
        BiomeType::DeepDark => {
            if hash % 100 < 5 {
                Some(MODEL_GLOW_MUSHROOM)
            } else {
                terrain
                    .cave_generator()
                    .should_place_stalagmite(world_x, world_y, world_z, biome)
            }
        }

        // Standard caves - stalagmites
        _ => terrain
            .cave_generator()
            .should_place_stalagmite(world_x, world_y, world_z, biome),
    }
}

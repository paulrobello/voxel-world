//! Terrain generation facade.
//!
//! This module provides backward compatibility by re-exporting types from
//! the new modular world_gen system.
//!
//! The actual implementation has been split into:
//! - `world_gen::biome` - Biome types and selection
//! - `world_gen::terrain` - TerrainGenerator and height calculation
//! - `world_gen::trees` - Tree generation
//! - `world_gen::vegetation` - Ground cover and cave decorations
//! - `world_gen::utils` - Helper functions and overflow blocks

// Cross-chunk terrain generation requires many parameters (chunk coords, overflow blocks)
// which exceeds clippy's default limit. This is intentional for the overflow block system.
#![allow(clippy::too_many_arguments)]
#![allow(clippy::ptr_arg)]

use crate::cave_gen::CaveFillType;
use crate::chunk::{BlockType, CHUNK_SIZE, Chunk};
use crate::config::WorldGenType;
use crate::world_gen;
use nalgebra::Vector3;

// Re-export types for backward compatibility
pub use crate::world_gen::SEA_LEVEL;
pub use crate::world_gen::biome::{BiomeInfo, BiomeType};
pub use crate::world_gen::terrain::TerrainGenerator;
pub use crate::world_gen::utils::{ChunkGenerationResult, OverflowBlock};

/// Generates terrain for a single chunk at the given position.
pub fn generate_chunk_terrain(
    terrain: &TerrainGenerator,
    chunk_pos: Vector3<i32>,
    world_gen_type: WorldGenType,
) -> ChunkGenerationResult {
    match world_gen_type {
        WorldGenType::Normal => generate_normal_chunk(terrain, chunk_pos),
        WorldGenType::Flat => generate_flat_chunk(chunk_pos),
    }
}

/// Generates a flat world chunk (2 chunks = 64 blocks high).
fn generate_flat_chunk(chunk_pos: Vector3<i32>) -> ChunkGenerationResult {
    let mut chunk = Chunk::new();
    let chunk_world_y = chunk_pos.y * CHUNK_SIZE as i32;

    const FLAT_HEIGHT: i32 = 63;
    const GRASS_LAYERS: i32 = 1;
    const DIRT_LAYERS: i32 = 7;

    if chunk_pos.y >= 2 {
        chunk.update_metadata();
        chunk.persistence_dirty = false;
        return ChunkGenerationResult {
            chunk,
            overflow_blocks: Vec::new(),
        };
    }

    for lx in 0..CHUNK_SIZE {
        for lz in 0..CHUNK_SIZE {
            for ly in 0..CHUNK_SIZE {
                let world_y = chunk_world_y + ly as i32;

                let block_type = if world_y > FLAT_HEIGHT {
                    BlockType::Air
                } else if world_y == 0 {
                    BlockType::Bedrock
                } else if world_y == FLAT_HEIGHT {
                    BlockType::Grass
                } else if world_y > FLAT_HEIGHT - GRASS_LAYERS - DIRT_LAYERS {
                    BlockType::Dirt
                } else {
                    BlockType::Stone
                };

                chunk.set_block(lx, ly, lz, block_type);
            }
        }
    }

    chunk.update_metadata();
    chunk.persistence_dirty = false;
    ChunkGenerationResult {
        chunk,
        overflow_blocks: Vec::new(),
    }
}

/// Generates normal terrain with biomes, caves, and trees.
fn generate_normal_chunk(
    terrain: &TerrainGenerator,
    chunk_pos: Vector3<i32>,
) -> ChunkGenerationResult {
    let mut chunk = Chunk::new();
    let mut overflow_blocks = Vec::new();
    let chunk_world_x = chunk_pos.x * CHUNK_SIZE as i32;
    let chunk_world_y = chunk_pos.y * CHUNK_SIZE as i32;
    let chunk_world_z = chunk_pos.z * CHUNK_SIZE as i32;

    // Generate terrain for this chunk
    for lx in 0..CHUNK_SIZE {
        for lz in 0..CHUNK_SIZE {
            let world_x = chunk_world_x + lx as i32;
            let world_z = chunk_world_z + lz as i32;
            let height = terrain.get_height(world_x, world_z);
            let biome = terrain.get_biome(world_x, world_z);

            for ly in 0..CHUNK_SIZE {
                let world_y = chunk_world_y + ly as i32;

                // Check if this is a cave first
                let is_cave = terrain
                    .cave_generator()
                    .is_cave(world_x, world_y, world_z, height, biome);

                let block_type = if world_y == 0 {
                    BlockType::Bedrock
                } else if world_y > height && world_y > SEA_LEVEL {
                    BlockType::Air
                } else if world_y > height && world_y <= SEA_LEVEL {
                    BlockType::Water
                } else if is_cave {
                    match terrain
                        .cave_generator()
                        .get_cave_fill(biome, world_y, SEA_LEVEL)
                    {
                        CaveFillType::Air => BlockType::Air,
                        CaveFillType::Water(water_type) => {
                            chunk.set_water_block(lx, ly, lz, water_type);
                            continue;
                        }
                        CaveFillType::Lava => BlockType::Lava,
                    }
                } else if world_y == height {
                    // Surface block selection based on biome
                    #[allow(deprecated)]
                    match biome {
                        // Snowy biomes - snow on top
                        BiomeType::SnowyPlains | BiomeType::SnowyTaiga | BiomeType::Snow => {
                            BlockType::Snow
                        }
                        // Desert - sand surface
                        BiomeType::Desert => BlockType::Sand,
                        // Mountains - exposed stone
                        BiomeType::Mountains => BlockType::Stone,
                        // Swamp - mud surface
                        BiomeType::Swamp => {
                            chunk.set_block(lx, ly, lz, BlockType::Mud);
                            continue;
                        }
                        // Beach - always sand
                        BiomeType::Beach => BlockType::Sand,
                        // Ocean floor - sand or gravel
                        BiomeType::Ocean => {
                            if terrain.hash(world_x, world_z) % 3 == 0 {
                                BlockType::Gravel
                            } else {
                                BlockType::Sand
                            }
                        }
                        // Savanna - dry grass (regular grass block)
                        BiomeType::Savanna => BlockType::Grass,
                        // Taiga - grass (cold but not frozen)
                        BiomeType::Taiga => BlockType::Grass,
                        // All other biomes - grass or sand near water
                        BiomeType::Plains
                        | BiomeType::Grassland
                        | BiomeType::Forest
                        | BiomeType::DarkForest
                        | BiomeType::BirchForest
                        | BiomeType::Meadow
                        | BiomeType::Jungle => {
                            if world_y <= SEA_LEVEL + 2 {
                                BlockType::Sand
                            } else {
                                BlockType::Grass
                            }
                        }
                        // Underground biomes - stone at surface (shouldn't normally be visible)
                        BiomeType::LushCaves | BiomeType::DripstoneCaves | BiomeType::DeepDark => {
                            BlockType::Stone
                        }
                    }
                } else if world_y > height - 4 {
                    // Subsurface layer (1-4 blocks below surface)
                    #[allow(deprecated)]
                    match biome {
                        // Desert - sandstone subsurface
                        BiomeType::Desert => {
                            chunk.set_block(lx, ly, lz, BlockType::Sandstone);
                            continue;
                        }
                        // Mountains - stone all the way
                        BiomeType::Mountains => BlockType::Stone,
                        // Snowy biomes - ice subsurface
                        BiomeType::SnowyPlains | BiomeType::SnowyTaiga | BiomeType::Snow => {
                            BlockType::Ice
                        }
                        // Ocean/Beach - sand under surface
                        BiomeType::Ocean | BiomeType::Beach => BlockType::Sand,
                        // All other biomes - dirt or sand near water
                        _ => {
                            if height <= SEA_LEVEL + 2 {
                                BlockType::Sand
                            } else {
                                BlockType::Dirt
                            }
                        }
                    }
                } else {
                    // Deep underground
                    #[allow(deprecated)]
                    match biome {
                        // Snowy biomes have ice below surface
                        BiomeType::SnowyPlains | BiomeType::SnowyTaiga | BiomeType::Snow => {
                            BlockType::Ice
                        }
                        // Everything else is stone
                        _ => BlockType::Stone,
                    }
                };

                if block_type == BlockType::Water {
                    chunk.set_water_block(lx, ly, lz, biome.water_type());
                } else {
                    chunk.set_block(lx, ly, lz, block_type);
                }
            }
        }
    }

    // Generate trees
    world_gen::generate_trees(
        &mut chunk,
        terrain,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        &mut overflow_blocks,
    );

    // Generate ground cover
    world_gen::generate_ground_cover(
        &mut chunk,
        terrain,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        &mut overflow_blocks,
    );

    // Generate cave decorations
    world_gen::generate_cave_decorations(
        &mut chunk,
        terrain,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        &mut overflow_blocks,
    );

    chunk.update_metadata();
    chunk.persistence_dirty = false;
    ChunkGenerationResult {
        chunk,
        overflow_blocks,
    }
}

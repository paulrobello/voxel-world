use nalgebra::{Vector3, vector};

use crate::chunk::{BlockType, CHUNK_SIZE};
use crate::config::WorldGenType;
use crate::constants::{
    LOADED_CHUNKS_X, LOADED_CHUNKS_Z, TEXTURE_SIZE_X, TEXTURE_SIZE_Y, TEXTURE_SIZE_Z,
    VIEW_DISTANCE, WORLD_CHUNKS_Y,
};
use crate::storage;
use crate::terrain_gen::{TerrainGenerator, generate_chunk_terrain};
use crate::world::World;

/// Creates a world with only chunks near the spawn point loaded.
/// Additional chunks are loaded dynamically as the player moves.
pub fn create_initial_world_with_seed(
    spawn_chunk: Vector3<i32>,
    seed: u32,
    world_gen_type: WorldGenType,
    storage: Option<&storage::worker::StorageSystem>,
) -> World {
    let mut world = World::new();
    let terrain = TerrainGenerator::new(seed);

    // Load chunks within horizontal view distance, all Y levels
    // Uses circular distance to match runtime loading behavior
    for dx in -VIEW_DISTANCE..=VIEW_DISTANCE {
        for dz in -VIEW_DISTANCE..=VIEW_DISTANCE {
            // Check horizontal distance (circular)
            let dist_sq = dx * dx + dz * dz;
            if dist_sq > VIEW_DISTANCE * VIEW_DISTANCE {
                continue;
            }

            let cx = spawn_chunk.x + dx;
            let cz = spawn_chunk.z + dz;

            // No horizontal bounds check - world is infinite in X/Z
            // Load ALL Y levels within this horizontal range
            for cy in 0..WORLD_CHUNKS_Y {
                let chunk_pos = vector![cx, cy, cz];

                // Try to load from storage first
                let mut loaded = false;
                if let Some(storage) = storage {
                    if let Ok(Some(mut chunk)) = storage.load_chunk(chunk_pos) {
                        chunk.update_metadata();
                        chunk.mark_dirty();
                        chunk.persistence_dirty = false;
                        world.insert_chunk(chunk_pos, chunk);
                        loaded = true;
                    }
                }

                if !loaded {
                    let chunk = generate_chunk_terrain(&terrain, chunk_pos, world_gen_type);
                    world.insert_chunk(chunk_pos, chunk);
                }
            }
        }
    }

    println!("Initial world created with {} chunks", world.chunk_count());
    world
}

/// Legacy function - kept for reference but no longer used
#[allow(dead_code)]
pub fn create_game_world_full() -> World {
    let mut world = World::new();
    let terrain = TerrainGenerator::new(42); // Fixed seed for reproducibility

    // Generate chunks within the loaded area (centered at origin for legacy mode)
    for cx in 0..LOADED_CHUNKS_X {
        for cy in 0..WORLD_CHUNKS_Y {
            for cz in 0..LOADED_CHUNKS_Z {
                let chunk_pos = vector![cx, cy, cz];
                let chunk = generate_chunk_terrain(&terrain, chunk_pos, WorldGenType::Normal);
                world.insert_chunk(chunk_pos, chunk);
            }
        }
    }

    // Count non-air blocks
    let mut count = 0;
    for cx in 0..LOADED_CHUNKS_X {
        for cy in 0..WORLD_CHUNKS_Y {
            for cz in 0..LOADED_CHUNKS_Z {
                if let Some(chunk) = world.get_chunk(vector![cx, cy, cz]) {
                    for x in 0..CHUNK_SIZE {
                        for y in 0..CHUNK_SIZE {
                            for z in 0..CHUNK_SIZE {
                                if chunk.get_block(x, y, z) != BlockType::Air {
                                    count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    println!(
        "Created world: {}x{}x{} blocks ({} chunks), {} non-air blocks",
        TEXTURE_SIZE_X,
        TEXTURE_SIZE_Y,
        TEXTURE_SIZE_Z,
        LOADED_CHUNKS_X * WORLD_CHUNKS_Y * LOADED_CHUNKS_Z,
        count
    );

    world
}

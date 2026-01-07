//! World generation methods.

use super::World;
use crate::chunk::{BlockType, CHUNK_SIZE, Chunk};
use nalgebra::vector;

impl World {
    /// Creates a simple test world with a flat terrain.
    pub fn create_flat_world(size: i32, height: i32) -> Self {
        let mut world = Self::new();

        // Calculate how many chunks we need
        let chunks_xz = (size + CHUNK_SIZE as i32 - 1) / CHUNK_SIZE as i32;
        let chunks_y = (height + CHUNK_SIZE as i32 - 1) / CHUNK_SIZE as i32;

        for cx in 0..chunks_xz {
            for cz in 0..chunks_xz {
                for cy in 0..chunks_y {
                    let chunk_pos = vector![cx, cy, cz];
                    let mut chunk = Chunk::new();

                    for lx in 0..CHUNK_SIZE {
                        for lz in 0..CHUNK_SIZE {
                            for ly in 0..CHUNK_SIZE {
                                let world_y = cy * CHUNK_SIZE as i32 + ly as i32;

                                let block = if world_y == height - 1 {
                                    BlockType::Grass
                                } else if world_y >= height - 4 {
                                    BlockType::Dirt
                                } else if world_y >= 0 {
                                    BlockType::Stone
                                } else {
                                    BlockType::Air
                                };

                                if block != BlockType::Air {
                                    chunk.set_block(lx, ly, lz, block);
                                }
                            }
                        }
                    }

                    if !chunk.is_empty() {
                        world.insert_chunk(chunk_pos, chunk);
                    }
                }
            }
        }

        world
    }

    /// Creates a world from a single chunk (for testing/compatibility).
    pub fn from_single_chunk(chunk: Chunk) -> Self {
        let mut world = Self::new();
        world.insert_chunk(vector![0, 0, 0], chunk);
        world
    }
}

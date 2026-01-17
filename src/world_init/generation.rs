use nalgebra::{Vector3, vector};
use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::chunk::{BlockType, CHUNK_SIZE};
use crate::config::{BenchmarkTerrain, WorldGenType};
use crate::constants::{
    LOADED_CHUNKS_X, LOADED_CHUNKS_Z, TEXTURE_SIZE_X, TEXTURE_SIZE_Y, TEXTURE_SIZE_Z,
    VIEW_DISTANCE, WORLD_CHUNKS_Y,
};
use crate::storage;
use crate::terrain_gen::{ChunkGenerationResult, TerrainGenerator, generate_chunk_terrain};
use crate::world::World;

/// Creates a world with only chunks near the spawn point loaded.
/// Additional chunks are loaded dynamically as the player moves.
///
/// Uses parallel chunk generation for significant speedup on multi-core systems.
pub fn create_initial_world_with_seed(
    spawn_chunk: Vector3<i32>,
    seed: u32,
    world_gen_type: WorldGenType,
    benchmark_terrain: BenchmarkTerrain,
    storage: Option<&storage::worker::StorageSystem>,
) -> World {
    use std::io::{Write, stdout};
    use std::time::Instant;

    let start_time = Instant::now();
    let mut world = World::new();

    println!(
        "[World Gen] Initializing terrain generator (seed: {})...",
        seed
    );
    let terrain = TerrainGenerator::new(seed);

    // Build list of all chunk positions to load/generate
    let mut chunk_positions: Vec<Vector3<i32>> = Vec::new();
    for dx in -VIEW_DISTANCE..=VIEW_DISTANCE {
        for dz in -VIEW_DISTANCE..=VIEW_DISTANCE {
            // Check horizontal distance (circular)
            let dist_sq = dx * dx + dz * dz;
            if dist_sq > VIEW_DISTANCE * VIEW_DISTANCE {
                continue;
            }

            let cx = spawn_chunk.x + dx;
            let cz = spawn_chunk.z + dz;

            // Load ALL Y levels within this horizontal range
            for cy in 0..WORLD_CHUNKS_Y {
                chunk_positions.push(vector![cx, cy, cz]);
            }
        }
    }

    let total_chunks = chunk_positions.len();
    let total_columns = total_chunks / WORLD_CHUNKS_Y as usize;

    println!(
        "[World Gen] Generating {} chunks ({} columns × {} Y levels)...",
        total_chunks, total_columns, WORLD_CHUNKS_Y
    );

    // === PHASE 1: Try to load chunks from storage (single-threaded) ===
    let mut chunks_to_generate: Vec<Vector3<i32>> = Vec::new();
    let mut chunks_loaded = 0;

    if let Some(storage) = storage {
        print!("[World Gen] Checking storage for existing chunks...");
        let _ = stdout().flush();

        for chunk_pos in &chunk_positions {
            if let Ok(Some(mut chunk)) = storage.load_chunk(*chunk_pos) {
                chunk.update_metadata();
                chunk.mark_dirty();
                chunk.persistence_dirty = false;
                world.insert_chunk(*chunk_pos, chunk);
                chunks_loaded += 1;
            } else {
                chunks_to_generate.push(*chunk_pos);
            }
        }
        println!(
            " {} found, {} to generate",
            chunks_loaded,
            chunks_to_generate.len()
        );
    } else {
        chunks_to_generate = chunk_positions;
    }

    // === PHASE 2: Generate remaining chunks in parallel ===
    if !chunks_to_generate.is_empty() {
        let chunks_to_gen_count = chunks_to_generate.len();
        let progress_counter = AtomicUsize::new(0);
        let progress_interval = (chunks_to_gen_count / 20).max(1); // Report every ~5%

        println!(
            "[World Gen] Generating {} chunks in parallel ({} threads)...",
            chunks_to_gen_count,
            rayon::current_num_threads()
        );

        // Generate chunks in parallel - each chunk is independent
        let results: Vec<(Vector3<i32>, ChunkGenerationResult)> = chunks_to_generate
            .par_iter()
            .map(|&chunk_pos| {
                let result =
                    generate_chunk_terrain(&terrain, chunk_pos, world_gen_type, benchmark_terrain);

                // Update progress (atomic)
                let count = progress_counter.fetch_add(1, Ordering::Relaxed) + 1;
                if count % progress_interval == 0 || count == chunks_to_gen_count {
                    let percent = (count * 100) / chunks_to_gen_count;
                    print!(
                        "\r[World Gen] Generating: {}% ({}/{})",
                        percent, count, chunks_to_gen_count
                    );
                    let _ = stdout().flush();
                }

                (chunk_pos, result)
            })
            .collect();

        println!(); // New line after progress

        // === PHASE 3: Insert chunks sequentially (handles overflow blocks) ===
        print!("[World Gen] Inserting chunks into world...");
        let _ = stdout().flush();

        for (chunk_pos, result) in results {
            // Apply overflow blocks (immediate if chunk exists, pending if not)
            world.apply_overflow_blocks(result.overflow_blocks);

            // Insert chunk into world (will also apply any pending overflow for this chunk)
            world.insert_chunk(chunk_pos, result.chunk);
        }

        println!(" done");
    }

    let elapsed = start_time.elapsed();
    let chunks_generated = total_chunks - chunks_loaded;
    println!(
        "[World Gen] Complete! {} chunks in {:.2}s ({} generated, {} loaded from storage)",
        world.chunk_count(),
        elapsed.as_secs_f32(),
        chunks_generated,
        chunks_loaded
    );

    // Report throughput
    if elapsed.as_secs_f32() > 0.0 {
        let chunks_per_sec = chunks_generated as f32 / elapsed.as_secs_f32();
        println!("[World Gen] Throughput: {:.0} chunks/sec", chunks_per_sec);
    }

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
                let result = generate_chunk_terrain(
                    &terrain,
                    chunk_pos,
                    WorldGenType::Normal,
                    BenchmarkTerrain::default(),
                );

                // Apply overflow blocks (immediate if chunk exists, pending if not)
                world.apply_overflow_blocks(result.overflow_blocks);

                // Insert chunk into world (will also apply any pending overflow for this chunk)
                world.insert_chunk(chunk_pos, result.chunk);
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

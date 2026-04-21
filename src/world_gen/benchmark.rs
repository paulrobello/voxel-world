//! Benchmark terrain generator for controlled profiling scenarios.
//!
//! Creates predictable terrain with:
//! - Flat or hills terrain styles
//! - Dense showcase area near spawn with all feature types
//! - Regular torch placement for point light testing
//! - Glowstone pillars and crystal clusters
//! - Glass walls for transparency stress testing

use crate::chunk::{BlockType, CHUNK_SIZE, Chunk};
use crate::config::BenchmarkTerrain;
use crate::world_gen::utils::ChunkGenerationResult;
use nalgebra::Vector3;

/// Model ID for torch (from sub_voxel registry)
const TORCH_MODEL_ID: u8 = 1;

/// Base height for flat terrain
const BASE_HEIGHT: i32 = 100;

/// Showcase area radius (dense features within this distance of spawn)
const SHOWCASE_RADIUS: i32 = 48;

/// Torch placement interval in showcase area
const SHOWCASE_TORCH_INTERVAL: i32 = 8;

/// Torch placement interval (every N blocks on grid)
const TORCH_INTERVAL: i32 = 16;

/// Glowstone pillar interval
const GLOWSTONE_INTERVAL: i32 = 64;

/// Crystal cluster interval
const CRYSTAL_INTERVAL: i32 = 128;

/// Glass wall interval
const GLASS_WALL_INTERVAL: i32 = 96;

/// Glass wall height
const GLASS_WALL_HEIGHT: i32 = 8;

/// Generates a benchmark chunk with controlled features.
///
/// # Arguments
/// * `chunk_pos` - The chunk position in chunk coordinates
/// * `terrain_style` - Whether to use flat or hills terrain
///
/// # Returns
/// A ChunkGenerationResult with the generated chunk and overflow blocks
pub fn generate_benchmark_chunk(
    chunk_pos: Vector3<i32>,
    terrain_style: BenchmarkTerrain,
) -> ChunkGenerationResult {
    let mut chunk = Chunk::new();
    let chunk_world_x = chunk_pos.x * CHUNK_SIZE as i32;
    let chunk_world_y = chunk_pos.y * CHUNK_SIZE as i32;
    let chunk_world_z = chunk_pos.z * CHUNK_SIZE as i32;

    // Only generate terrain for chunks at Y=0-3 (covers Y=0-127, terrain at Y=100)
    if chunk_pos.y > 3 {
        chunk.update_metadata();
        chunk.persistence_dirty = false;
        return ChunkGenerationResult {
            chunk,
            overflow_blocks: Vec::new(),
        };
    }

    // Generate base terrain
    for lx in 0..CHUNK_SIZE {
        for lz in 0..CHUNK_SIZE {
            let world_x = chunk_world_x + lx as i32;
            let world_z = chunk_world_z + lz as i32;

            // Calculate surface height based on terrain style
            let surface_height = match terrain_style {
                BenchmarkTerrain::Flat => BASE_HEIGHT,
                BenchmarkTerrain::Hills => {
                    // Sine wave hills: Y = 90-110
                    let x_wave = (world_x as f64 / 20.0).sin();
                    let z_wave = (world_z as f64 / 20.0).sin();
                    (BASE_HEIGHT as f64 + x_wave * z_wave * 10.0) as i32
                }
            };

            for ly in 0..CHUNK_SIZE {
                let world_y = chunk_world_y + ly as i32;

                let block_type = if world_y == 0 {
                    BlockType::Bedrock
                } else if world_y > surface_height {
                    BlockType::Air
                } else if world_y == surface_height {
                    // Surface layer - stone for visibility
                    BlockType::Stone
                } else if world_y > surface_height - 4 {
                    // Subsurface
                    BlockType::Dirt
                } else {
                    // Deep underground
                    BlockType::Stone
                };

                chunk.set_block(lx, ly, lz, block_type);
            }
        }
    }

    // Place features (torches, glowstone, crystals, glass walls)
    place_benchmark_features(
        &mut chunk,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        terrain_style,
    );

    chunk.update_metadata();
    chunk.persistence_dirty = false;
    ChunkGenerationResult {
        chunk,
        overflow_blocks: Vec::new(),
    }
}

/// Places benchmark features (torches, glowstone pillars, crystals, glass walls)
/// Creates a dense showcase area near spawn (0,0) and regular features elsewhere.
fn place_benchmark_features(
    chunk: &mut Chunk,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    terrain_style: BenchmarkTerrain,
) {
    for lx in 0..CHUNK_SIZE {
        for lz in 0..CHUNK_SIZE {
            let world_x = chunk_world_x + lx as i32;
            let world_z = chunk_world_z + lz as i32;

            // Calculate surface height for this column
            let surface_height = match terrain_style {
                BenchmarkTerrain::Flat => BASE_HEIGHT,
                BenchmarkTerrain::Hills => {
                    let x_wave = (world_x as f64 / 20.0).sin();
                    let z_wave = (world_z as f64 / 20.0).sin();
                    (BASE_HEIGHT as f64 + x_wave * z_wave * 10.0) as i32
                }
            };

            // Check if we're in the showcase area (dense features near spawn)
            let in_showcase = world_x.abs() <= SHOWCASE_RADIUS && world_z.abs() <= SHOWCASE_RADIUS;

            // Determine feature placement based on location
            let (is_torch_pos, is_glowstone_pos, is_crystal_pos, is_glass_wall_pos) = if in_showcase
            {
                // Dense showcase area: more torches, explicit feature positions
                let torch = world_x.rem_euclid(SHOWCASE_TORCH_INTERVAL) == 0
                    && world_z.rem_euclid(SHOWCASE_TORCH_INTERVAL) == 0;

                // Glowstone pillars at specific positions in showcase
                // Cardinal directions and corners
                let glowstone = (world_x.abs() == 16 && world_z == 0)
                    || (world_x == 0 && world_z.abs() == 16)
                    || (world_x.abs() == 24 && world_z.abs() == 24);

                // Crystal clusters at different positions with different tints
                let crystal = (world_x.abs() == 32 && world_z == 0)
                    || (world_x == 0 && world_z.abs() == 32)
                    || (world_x.abs() == 40 && world_z == 8);

                // Glass walls at specific Z positions (not blocking forward view)
                let glass = (world_z == 20 || world_z == -20) && (8..=40).contains(&world_x.abs());

                (torch, glowstone, crystal, glass)
            } else {
                // Regular sparse placement outside showcase
                let torch = world_x.rem_euclid(TORCH_INTERVAL) == 0
                    && world_z.rem_euclid(TORCH_INTERVAL) == 0;
                let glowstone = world_x.rem_euclid(GLOWSTONE_INTERVAL) == 0
                    && world_z.rem_euclid(GLOWSTONE_INTERVAL) == 0;
                let crystal = world_x.rem_euclid(CRYSTAL_INTERVAL) == 0
                    && world_z.rem_euclid(CRYSTAL_INTERVAL) == 0;
                let glass = world_x.rem_euclid(GLASS_WALL_INTERVAL) == 0;

                (torch, glowstone, crystal, glass)
            };

            // Process each Y level in this chunk
            for ly in 0..CHUNK_SIZE {
                let world_y = chunk_world_y + ly as i32;
                let height_above_surface = world_y - surface_height;

                // Skip positions that would have multiple features
                let skip_for_glowstone = is_glowstone_pos;
                let skip_for_crystal = is_crystal_pos && !is_glowstone_pos;

                // Torch on surface (height_above_surface == 1)
                if is_torch_pos
                    && height_above_surface == 1
                    && !skip_for_glowstone
                    && !skip_for_crystal
                {
                    place_torch(chunk, lx, ly, lz);
                }

                // Glowstone pillar (3 blocks tall, starting at surface + 1)
                if is_glowstone_pos && (1..=3).contains(&height_above_surface) {
                    chunk.set_block(lx, ly, lz, BlockType::GlowStone);
                }

                // Crystal cluster at surface + 1, with varied tints
                if is_crystal_pos && height_above_surface == 1 && !is_glowstone_pos {
                    // Use position hash for tint variation (0-31)
                    let tint = ((world_x.abs() + world_z.abs()) % 32) as u8;
                    place_crystal(chunk, lx, ly, lz, tint);
                }

                // Glass wall (8 blocks tall, starting at surface + 1)
                if is_glass_wall_pos
                    && (1..=GLASS_WALL_HEIGHT).contains(&height_above_surface)
                    && !is_glowstone_pos
                    && !is_crystal_pos
                {
                    chunk.set_block(lx, ly, lz, BlockType::Glass);
                }
            }
        }
    }
}

/// Places a torch model at the given local coordinates
fn place_torch(chunk: &mut Chunk, lx: usize, ly: usize, lz: usize) {
    chunk.set_model_block(
        lx,
        ly,
        lz,
        TORCH_MODEL_ID,
        0,    // rotation
        true, // emits_light
    );
}

/// Places a crystal block at the given local coordinates with a specific tint
fn place_crystal(chunk: &mut Chunk, lx: usize, ly: usize, lz: usize, tint: u8) {
    chunk.set_crystal_block(lx, ly, lz, tint);
}

//! Birch tree generation.
//!
//! Birch trees are characterized by:
//! - Tall, thin trunks
//! - Small, rounded canopy
//! - White bark (uses BirchLog block)
//! - Light green leaves (uses BirchLeaves block)

use crate::chunk::{BlockType, Chunk};
use crate::world_gen::utils::{OverflowBlock, get_block_safe, set_block_safe};

/// Generate a birch tree.
#[allow(clippy::too_many_arguments)]
pub fn generate_birch(
    chunk: &mut Chunk,
    x: i32,
    y: i32,
    z: i32,
    hash: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    // Check if there's solid ground below
    for check_y in (y.saturating_sub(2))..=y {
        if let Some(block) = get_block_safe(chunk, x, check_y, z) {
            if !block.is_solid() {
                return;
            }
        } else {
            return;
        }
    }

    // Birch trees are tall and thin
    let height = 5 + (hash % 4); // 5-8 blocks tall
    let canopy_start = height - 3;

    // Thin trunk (single block wide)
    for dy in 1..height {
        set_block_safe(
            chunk,
            x,
            y + dy,
            z,
            BlockType::BirchLog,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }

    // Small rounded canopy
    generate_birch_canopy(
        chunk,
        x,
        y + canopy_start,
        z,
        height - canopy_start + 1,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        overflow_blocks,
    );
}

#[allow(clippy::too_many_arguments)]
fn generate_birch_canopy(
    chunk: &mut Chunk,
    x: i32,
    base_y: i32,
    z: i32,
    canopy_height: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    // Birch canopy is small and oval-shaped
    for dy in 0..canopy_height {
        // Radius varies to create rounded shape
        let radius = if dy == 0 || dy == canopy_height - 1 {
            1 // Top and bottom are small
        } else {
            2 // Middle is wider
        };

        for dx in -radius..=radius {
            for dz in -radius..=radius {
                let dist_sq = dx * dx + dz * dz;
                let radius_sq = radius * radius;

                // Skip corners for rounder appearance
                if dist_sq > radius_sq {
                    continue;
                }

                // Don't overwrite trunk in lower layers
                if dx == 0 && dz == 0 && dy < canopy_height - 2 {
                    continue;
                }

                set_block_safe(
                    chunk,
                    x + dx,
                    base_y + dy,
                    z + dz,
                    BlockType::BirchLeaves,
                    chunk_world_x,
                    chunk_world_y,
                    chunk_world_z,
                    overflow_blocks,
                );
            }
        }
    }

    // Add top leaf
    set_block_safe(
        chunk,
        x,
        base_y + canopy_height,
        z,
        BlockType::BirchLeaves,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        overflow_blocks,
    );
}

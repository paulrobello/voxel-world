//! Acacia tree generation.
//!
//! Acacia trees are characterized by:
//! - Distinctive bent/diagonal trunk
//! - Flat, spreading canopy (umbrella shape)
//! - Common in savanna biomes

use crate::chunk::{BlockType, Chunk};
use crate::world_gen::utils::{OverflowBlock, get_block_safe, set_block_safe};

/// Generate an acacia tree with characteristic bent trunk and flat canopy.
#[allow(clippy::too_many_arguments)]
pub fn generate_acacia(
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

    let height = 5 + (hash % 3); // 5-7 blocks tall
    let bend_direction = hash % 4; // 0=+x, 1=-x, 2=+z, 3=-z
    let bend_amount = 1 + (hash / 13) % 2; // 1-2 blocks offset

    // Calculate bend offset
    let (bend_dx, bend_dz) = match bend_direction {
        0 => (bend_amount, 0),
        1 => (-bend_amount, 0),
        2 => (0, bend_amount),
        _ => (0, -bend_amount),
    };

    // Generate bent trunk
    let bend_point = height / 2;
    let mut trunk_x = x;
    let mut trunk_z = z;

    for dy in 1..=height {
        set_block_safe(
            chunk,
            trunk_x,
            y + dy,
            trunk_z,
            BlockType::Log,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );

        // Apply bend at midpoint
        if dy == bend_point {
            trunk_x += bend_dx;
            trunk_z += bend_dz;

            // Add diagonal connector block
            set_block_safe(
                chunk,
                trunk_x,
                y + dy,
                trunk_z,
                BlockType::Log,
                chunk_world_x,
                chunk_world_y,
                chunk_world_z,
                overflow_blocks,
            );
        }
    }

    // Generate flat umbrella canopy at top
    generate_acacia_canopy(
        chunk,
        trunk_x,
        y + height,
        trunk_z,
        hash,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        overflow_blocks,
    );
}

#[allow(clippy::too_many_arguments)]
fn generate_acacia_canopy(
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
    // Acacia canopy is flat and wide (umbrella shape)
    let canopy_radius = 3 + (hash / 7) % 2; // 3-4 blocks

    // Main flat layer
    for dx in -canopy_radius..=canopy_radius {
        for dz in -canopy_radius..=canopy_radius {
            let dist_sq = dx * dx + dz * dz;
            let radius_sq = canopy_radius * canopy_radius;

            // Create irregular edges for natural look
            let edge_variation = ((hash + dx * 7 + dz * 11) % 3) - 1;
            let effective_radius_sq = radius_sq + edge_variation;

            if dist_sq > effective_radius_sq {
                continue;
            }

            set_block_safe(
                chunk,
                x + dx,
                y,
                z + dz,
                BlockType::Leaves,
                chunk_world_x,
                chunk_world_y,
                chunk_world_z,
                overflow_blocks,
            );
        }
    }

    // Add a second thinner layer on top for thickness
    let upper_radius = canopy_radius - 1;
    for dx in -upper_radius..=upper_radius {
        for dz in -upper_radius..=upper_radius {
            let dist_sq = dx * dx + dz * dz;
            let radius_sq = upper_radius * upper_radius;

            if dist_sq > radius_sq {
                continue;
            }

            // Sparse upper layer
            if (dx + dz + hash) % 3 == 0 {
                set_block_safe(
                    chunk,
                    x + dx,
                    y + 1,
                    z + dz,
                    BlockType::Leaves,
                    chunk_world_x,
                    chunk_world_y,
                    chunk_world_z,
                    overflow_blocks,
                );
            }
        }
    }

    // Add center top leaf
    set_block_safe(
        chunk,
        x,
        y + 1,
        z,
        BlockType::Leaves,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        overflow_blocks,
    );
}

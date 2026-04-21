//! Jungle tree generation.
//!
//! Jungle trees are characterized by:
//! - Very tall trunks (normal and giant variants)
//! - Large, spreading canopy
//! - Vines (represented by hanging leaves)

use crate::chunk::{BlockType, Chunk};
use crate::world_gen::utils::{OverflowBlock, get_block_safe, set_block_safe};

/// Generate a jungle tree (normal or giant based on hash).
#[allow(clippy::too_many_arguments)]
pub fn generate_jungle(
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
    let is_giant = (hash % 8) == 0; // 12.5% chance of giant

    if is_giant {
        generate_giant_jungle(
            chunk,
            x,
            y,
            z,
            hash,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    } else {
        generate_normal_jungle(
            chunk,
            x,
            y,
            z,
            hash,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn generate_normal_jungle(
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

    // Jungle trees are tall
    let height = 8 + (hash % 6); // 8-13 blocks tall
    let canopy_start = height - 4;
    let canopy_radius = 3 + (hash / 17) % 2;

    // Trunk
    for dy in 1..height {
        set_block_safe(
            chunk,
            x,
            y + dy,
            z,
            BlockType::Log,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }

    // Large spreading canopy
    generate_jungle_canopy(
        chunk,
        x,
        y + canopy_start,
        z,
        canopy_radius,
        height - canopy_start,
        y + height - 1,
        hash,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        overflow_blocks,
    );
}

#[allow(clippy::too_many_arguments)]
fn generate_giant_jungle(
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

    // Giant jungle trees are very tall with thick trunk
    let height = 18 + (hash % 10); // 18-27 blocks tall
    let canopy_start = height - 8;
    let canopy_radius = 5 + (hash / 29) % 2;

    // 2x2 thick trunk
    for dy in 1..height {
        for dx in 0..=1 {
            for dz in 0..=1 {
                set_block_safe(
                    chunk,
                    x + dx,
                    y + dy,
                    z + dz,
                    BlockType::Log,
                    chunk_world_x,
                    chunk_world_y,
                    chunk_world_z,
                    overflow_blocks,
                );
            }
        }
    }

    // Massive canopy
    generate_jungle_canopy(
        chunk,
        x,
        y + canopy_start,
        z,
        canopy_radius,
        height - canopy_start,
        y + height - 1,
        hash,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        overflow_blocks,
    );

    // Add hanging vines (leaves extending down)
    add_jungle_vines(
        chunk,
        x,
        y + canopy_start,
        z,
        canopy_radius,
        hash,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        overflow_blocks,
    );
}

#[allow(clippy::too_many_arguments)]
fn generate_jungle_canopy(
    chunk: &mut Chunk,
    x: i32,
    base_y: i32,
    z: i32,
    max_radius: i32,
    canopy_height: i32,
    trunk_top_y: i32,
    _hash: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    // Jungle canopy is wide and somewhat flat
    for dy in 0..canopy_height {
        // Radius starts large, stays large, then tapers at top
        let radius = if dy == canopy_height - 1 {
            1
        } else if dy == canopy_height - 2 {
            max_radius / 2
        } else {
            max_radius
        };

        for dx in -radius..=radius {
            for dz in -radius..=radius {
                let dist_sq = dx * dx + dz * dz;
                let radius_sq = radius * radius;

                if dist_sq > radius_sq {
                    continue;
                }

                let ly = base_y + dy;
                // Don't overwrite trunk
                if (dx == 0 || dx == 1) && (dz == 0 || dz == 1) && ly <= trunk_top_y {
                    continue;
                }

                set_block_safe(
                    chunk,
                    x + dx,
                    ly,
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
}

#[allow(clippy::too_many_arguments)]
fn add_jungle_vines(
    chunk: &mut Chunk,
    base_x: i32,
    base_y: i32,
    base_z: i32,
    canopy_radius: i32,
    hash: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    // Add vines hanging from canopy edges
    let vine_positions = [
        (-canopy_radius, 0),
        (canopy_radius, 0),
        (0, -canopy_radius),
        (0, canopy_radius),
        (-canopy_radius + 1, -canopy_radius + 1),
        (canopy_radius - 1, canopy_radius - 1),
    ];

    for (i, (dx, dz)) in vine_positions.iter().enumerate() {
        let vine_length = 2 + ((hash / (i as i32 + 1)) % 4);

        for dy in 0..vine_length {
            set_block_safe(
                chunk,
                base_x + dx,
                base_y - dy - 1,
                base_z + dz,
                BlockType::Leaves,
                chunk_world_x,
                chunk_world_y,
                chunk_world_z,
                overflow_blocks,
            );
        }
    }
}

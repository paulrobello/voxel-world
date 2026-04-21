//! Pine tree generation.

use crate::chunk::{BlockType, Chunk};
use crate::world_gen::utils::{OverflowBlock, get_block_safe, set_block_safe};

/// Generate a pine tree (normal or giant based on hash).
#[allow(clippy::too_many_arguments)]
pub fn generate_pine(
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
    let is_giant = (hash % 10) == 0;

    if is_giant {
        generate_giant_pine(
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
        generate_normal_pine(
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
fn generate_normal_pine(
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

    let height = 6 + (hash % 8);
    let cone_width = (hash / 11) % 3;
    let start_leaves = 2 + (height / 4);
    let cone_height = height - start_leaves + 2;

    let max_radius = match cone_width {
        0 => (cone_height / 3).max(2),
        1 => ((cone_height * 2) / 5).max(2),
        _ => (cone_height / 2).max(3),
    };

    // Trunk
    for dy in 1..height {
        set_block_safe(
            chunk,
            x,
            y + dy,
            z,
            BlockType::PineLog,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }

    generate_pine_cone(
        chunk,
        x,
        y + start_leaves,
        z,
        max_radius,
        cone_height,
        y + height - 1,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        overflow_blocks,
    );
}

#[allow(clippy::too_many_arguments)]
fn generate_giant_pine(
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

    let height = 15 + ((hash / 19) % 8);
    let cone_width = (hash / 23) % 2;
    let start_leaves = 4 + (height / 6);
    let cone_height = height - start_leaves + 2;

    let max_radius = if cone_width == 0 {
        ((cone_height * 2) / 5).max(3)
    } else {
        (cone_height / 2).max(4)
    };

    // Trunk
    for dy in 1..height {
        set_block_safe(
            chunk,
            x,
            y + dy,
            z,
            BlockType::PineLog,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }

    generate_pine_cone(
        chunk,
        x,
        y + start_leaves,
        z,
        max_radius,
        cone_height,
        y + height - 1,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        overflow_blocks,
    );
}

#[allow(clippy::too_many_arguments)]
fn generate_pine_cone(
    chunk: &mut Chunk,
    x: i32,
    base_y: i32,
    z: i32,
    max_radius: i32,
    cone_height: i32,
    trunk_top_y: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    for dy in 0..cone_height {
        let radius: i32 = {
            let t = 1.0 - (dy as f32 / cone_height as f32);
            let calculated = (t * max_radius as f32) as i32;

            if calculated == 0 && dy < cone_height - 1 {
                1
            } else {
                calculated
            }
        };

        for dx in -radius..=radius {
            for dz in -radius..=radius {
                let dist_sq = dx * dx + dz * dz;
                let radius_sq = radius * radius;

                if dist_sq > radius_sq {
                    continue;
                }

                let ly = base_y + dy;
                if dx == 0 && dz == 0 && ly <= trunk_top_y {
                    continue;
                }
                set_block_safe(
                    chunk,
                    x + dx,
                    ly,
                    z + dz,
                    BlockType::PineLeaves,
                    chunk_world_x,
                    chunk_world_y,
                    chunk_world_z,
                    overflow_blocks,
                );
            }
        }
    }

    // Place tip leaf above trunk
    set_block_safe(
        chunk,
        x,
        trunk_top_y + 1,
        z,
        BlockType::PineLeaves,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        overflow_blocks,
    );
}

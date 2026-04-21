//! Snow biome tree generation (dead trees and snow-covered pines).

use crate::chunk::{BlockType, Chunk};
use crate::world_gen::utils::{OverflowBlock, get_block_safe, set_block_safe};

/// Generates a dead tree with bare branches and snow coverage for snow biome.
#[allow(clippy::too_many_arguments)]
pub fn generate_dead_tree(
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
    // Check if there's solid ground below (ice counts as solid for snow biome trees)
    for check_y in (y.saturating_sub(2))..=y {
        if let Some(block) = get_block_safe(chunk, x, check_y, z) {
            if !block.is_solid() && block != BlockType::Ice {
                return;
            }
        } else {
            return;
        }
    }

    let height = 6 + (hash % 7);

    // Pre-check: verify branch positions won't collide with existing trees
    // This prevents partial trees when two dead trees have branches pointing at each other
    let num_branches = 2 + (hash % 3);
    for branch_idx in 0..num_branches {
        let branch_y = y + (height * 2 / 5) + branch_idx;
        if branch_y >= y + height {
            break;
        }

        let direction = (hash.wrapping_add(branch_idx * 13)) % 4;
        let length = 1 + ((hash.wrapping_add(branch_idx * 17)) % 4);

        let (dx, dz) = match direction {
            0 => (1, 0),
            1 => (-1, 0),
            2 => (0, 1),
            _ => (0, -1),
        };

        // Check if branch endpoint has existing logs
        if let Some(block) = get_block_safe(chunk, x + dx * length, branch_y, z + dz * length)
            && (block == BlockType::PineLog || block == BlockType::Log)
        {
            return; // Another tree is in the way, skip this tree entirely
        }
    }

    // Build the trunk
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

    // Add 2-4 horizontal branches at different heights
    let num_branches = 2 + (hash % 3);
    for branch_idx in 0..num_branches {
        let branch_y = y + (height * 2 / 5) + branch_idx;
        if branch_y >= y + height {
            break;
        }

        let direction = (hash.wrapping_add(branch_idx * 13)) % 4;
        let length = 1 + ((hash.wrapping_add(branch_idx * 17)) % 4);

        let (dx, dz) = match direction {
            0 => (1, 0),
            1 => (-1, 0),
            2 => (0, 1),
            _ => (0, -1),
        };

        for dist in 1..=length {
            set_block_safe(
                chunk,
                x + dx * dist,
                branch_y,
                z + dz * dist,
                BlockType::PineLog,
                chunk_world_x,
                chunk_world_y,
                chunk_world_z,
                overflow_blocks,
            );

            // Place snow on top of each branch segment (80% chance)
            if (hash.wrapping_add(dist * 11 + branch_idx * 7) % 10) < 8 {
                set_block_safe(
                    chunk,
                    x + dx * dist,
                    branch_y + 1,
                    z + dz * dist,
                    BlockType::Snow,
                    chunk_world_x,
                    chunk_world_y,
                    chunk_world_z,
                    overflow_blocks,
                );
            }
        }
    }

    // Snow cap on top of trunk
    set_block_safe(
        chunk,
        x,
        y + height,
        z,
        BlockType::Snow,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        overflow_blocks,
    );

    // Add snow drifts around base (60% chance)
    if (hash % 10) < 6 {
        for dx in -1..=1 {
            for dz in -1..=1 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                if (hash.wrapping_add(dx * 5 + dz * 3) % 10) < 7 {
                    set_block_safe(
                        chunk,
                        x + dx,
                        y + 1,
                        z + dz,
                        BlockType::Snow,
                        chunk_world_x,
                        chunk_world_y,
                        chunk_world_z,
                        overflow_blocks,
                    );
                }
            }
        }
    }

    // Occasional snow accumulation on trunk sides (20% chance per level)
    for dy in 2..(height - 1) {
        if (hash.wrapping_add(dy * 19) % 10) < 2 {
            let side = (hash.wrapping_add(dy * 7)) % 4;
            let (dx, dz) = match side {
                0 => (1, 0),
                1 => (-1, 0),
                2 => (0, 1),
                _ => (0, -1),
            };
            set_block_safe(
                chunk,
                x + dx,
                y + dy,
                z + dz,
                BlockType::Snow,
                chunk_world_x,
                chunk_world_y,
                chunk_world_z,
                overflow_blocks,
            );
        }
    }
}

/// Generates a snow-covered pine tree for snow biome.
#[allow(clippy::too_many_arguments)]
pub fn generate_snow_pine(
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
    // Check if there's solid ground below (ice counts as solid for snow biome trees)
    for check_y in (y.saturating_sub(2))..=y {
        if let Some(block) = get_block_safe(chunk, x, check_y, z) {
            if !block.is_solid() && block != BlockType::Ice {
                return;
            }
        } else {
            return;
        }
    }

    let height = 8 + (hash % 7);

    // Pine trunk
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

    // Conical shape with layers of pine leaves and snow
    let layers = height / 2;

    for layer in 0..layers {
        let layer_y = y + height - 1 - layer;
        let radius = 1 + (layer / 2);

        for dx in -radius..=radius {
            for dz in -radius..=radius {
                if dx == 0 && dz == 0 {
                    continue;
                }

                let dist_sq = dx * dx + dz * dz;
                let max_dist_sq = radius * radius + radius;

                if dist_sq <= max_dist_sq {
                    set_block_safe(
                        chunk,
                        x + dx,
                        layer_y,
                        z + dz,
                        BlockType::PineLeaves,
                        chunk_world_x,
                        chunk_world_y,
                        chunk_world_z,
                        overflow_blocks,
                    );

                    // Heavy snow coverage on top of leaves (60% chance)
                    if (hash.wrapping_add(dx * 13 + dz * 17 + layer * 5) % 10) < 6 {
                        set_block_safe(
                            chunk,
                            x + dx,
                            layer_y + 1,
                            z + dz,
                            BlockType::Snow,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    }
                }
            }
        }
    }

    // Snow cap at the very top
    set_block_safe(
        chunk,
        x,
        y + height,
        z,
        BlockType::Snow,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        overflow_blocks,
    );

    // Snow drifts around base (70% chance)
    if hash % 10 < 7 {
        for dx in -2..=2 {
            for dz in -2..=2 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                let dist_sq = dx * dx + dz * dz;
                if dist_sq <= 4 && (hash.wrapping_add(dx + dz) % 3) != 0 {
                    set_block_safe(
                        chunk,
                        x + dx,
                        y + 1,
                        z + dz,
                        BlockType::Snow,
                        chunk_world_x,
                        chunk_world_y,
                        chunk_world_z,
                        overflow_blocks,
                    );
                }
            }
        }
    }
}

//! Willow tree generation for swamp biomes.

use crate::chunk::{BlockType, Chunk};
use crate::world_gen::utils::{OverflowBlock, set_block_safe};

/// Generate a willow tree (small, medium, or large based on hash).
#[allow(clippy::too_many_arguments)]
pub fn generate_willow(
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
    // Size variation: 60% small, 30% medium, 10% large
    let size_roll = hash % 10;
    let tree_size = if size_roll < 6 {
        0
    } else if size_roll < 9 {
        1
    } else {
        2
    };

    match tree_size {
        0 => generate_willow_small(
            chunk,
            x,
            y,
            z,
            hash,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        ),
        1 => generate_willow_medium(
            chunk,
            x,
            y,
            z,
            hash,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        ),
        _ => generate_willow_large(
            chunk,
            x,
            y,
            z,
            hash,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        ),
    }
}

/// Small willow tree.
#[allow(clippy::too_many_arguments)]
fn generate_willow_small(
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
    let height = 4 + (hash % 3);

    // Trunk
    for dy in 1..=height {
        set_block_safe(
            chunk,
            x,
            y + dy,
            z,
            BlockType::WillowLog,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }

    let canopy_y = y + height;

    // Bottom layer - widest (radius 3)
    for dx in -3i32..=3 {
        for dz in -3i32..=3 {
            let dist_sq = dx * dx + dz * dz;
            if dist_sq <= 9 {
                set_block_safe(
                    chunk,
                    x + dx,
                    canopy_y,
                    z + dz,
                    BlockType::WillowLeaves,
                    chunk_world_x,
                    chunk_world_y,
                    chunk_world_z,
                    overflow_blocks,
                );

                // Hanging vines from outer edge
                let dist = (dist_sq as f32).sqrt();
                if dist > 1.5 && (hash.wrapping_add(dx * 30 + dz) % 3 == 0) {
                    let vine_len = 1 + (hash % 3);
                    for v in 1..=vine_len {
                        set_block_safe(
                            chunk,
                            x + dx,
                            canopy_y - v,
                            z + dz,
                            BlockType::WillowLeaves,
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

    // Middle layer
    for dx in -2i32..=2 {
        for dz in -2i32..=2 {
            let dist_sq = dx * dx + dz * dz;
            if dist_sq <= 6 {
                set_block_safe(
                    chunk,
                    x + dx,
                    canopy_y + 1,
                    z + dz,
                    BlockType::WillowLeaves,
                    chunk_world_x,
                    chunk_world_y,
                    chunk_world_z,
                    overflow_blocks,
                );
            }
        }
    }

    // Top layer
    for dx in -1i32..=1 {
        for dz in -1i32..=1 {
            let dist_sq = dx * dx + dz * dz;
            if dist_sq <= 2 {
                set_block_safe(
                    chunk,
                    x + dx,
                    canopy_y + 2,
                    z + dz,
                    BlockType::WillowLeaves,
                    chunk_world_x,
                    chunk_world_y,
                    chunk_world_z,
                    overflow_blocks,
                );
            }
        }
    }
}

/// Medium willow tree with larger hollow canopy.
#[allow(clippy::too_many_arguments)]
fn generate_willow_medium(
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
    let height = 7 + (hash % 3);

    // Trunk
    for dy in 1..=height {
        set_block_safe(
            chunk,
            x,
            y + dy,
            z,
            BlockType::WillowLog,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }

    let canopy_y = y + height;

    // Hollow shell - only outer ring of leaves
    for dx in -5i32..=5 {
        for dz in -5i32..=5 {
            let dist_sq = dx * dx + dz * dz;
            let dist = (dist_sq as f32).sqrt();
            if (3.5..=5.0).contains(&dist) {
                set_block_safe(
                    chunk,
                    x + dx,
                    canopy_y,
                    z + dz,
                    BlockType::WillowLeaves,
                    chunk_world_x,
                    chunk_world_y,
                    chunk_world_z,
                    overflow_blocks,
                );

                // Draping vines that hang all the way to the ground
                if hash.wrapping_add(dx * 30 + dz) % 2 == 0 {
                    let vine_len = height;
                    for v in 1..=vine_len {
                        set_block_safe(
                            chunk,
                            x + dx,
                            canopy_y - v,
                            z + dz,
                            BlockType::WillowLeaves,
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

    // Upper layers - gradually smaller hollow shells
    for dx in -3i32..=3 {
        for dz in -3i32..=3 {
            let dist_sq = dx * dx + dz * dz;
            let dist = (dist_sq as f32).sqrt();
            if (2.0..=3.5).contains(&dist) {
                set_block_safe(
                    chunk,
                    x + dx,
                    canopy_y + 1,
                    z + dz,
                    BlockType::WillowLeaves,
                    chunk_world_x,
                    chunk_world_y,
                    chunk_world_z,
                    overflow_blocks,
                );
            }
        }
    }

    // Top cap
    for dx in -2i32..=2 {
        for dz in -2i32..=2 {
            if dx * dx + dz * dz <= 4 {
                set_block_safe(
                    chunk,
                    x + dx,
                    canopy_y + 2,
                    z + dz,
                    BlockType::WillowLeaves,
                    chunk_world_x,
                    chunk_world_y,
                    chunk_world_z,
                    overflow_blocks,
                );
            }
        }
    }
}

/// Large willow tree with dramatic hollow draping canopy.
#[allow(clippy::too_many_arguments)]
fn generate_willow_large(
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
    let height = 10 + (hash % 4);

    // Trunk
    for dy in 1..=height {
        set_block_safe(
            chunk,
            x,
            y + dy,
            z,
            BlockType::WillowLog,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }

    let canopy_y = y + height;

    // Large hollow shell
    for dx in -7i32..=7 {
        for dz in -7i32..=7 {
            let dist_sq = dx * dx + dz * dz;
            let dist = (dist_sq as f32).sqrt();
            if (5.0..=7.0).contains(&dist) {
                set_block_safe(
                    chunk,
                    x + dx,
                    canopy_y,
                    z + dz,
                    BlockType::WillowLeaves,
                    chunk_world_x,
                    chunk_world_y,
                    chunk_world_z,
                    overflow_blocks,
                );

                // Draping vines
                if hash.wrapping_add(dx * 30 + dz) % 2 == 0 {
                    let vine_len = height;
                    for v in 1..=vine_len {
                        set_block_safe(
                            chunk,
                            x + dx,
                            canopy_y - v,
                            z + dz,
                            BlockType::WillowLeaves,
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

    // Middle layer - hollow shell
    for dx in -5i32..=5 {
        for dz in -5i32..=5 {
            let dist_sq = dx * dx + dz * dz;
            let dist = (dist_sq as f32).sqrt();
            if (3.5..=5.0).contains(&dist) {
                set_block_safe(
                    chunk,
                    x + dx,
                    canopy_y + 1,
                    z + dz,
                    BlockType::WillowLeaves,
                    chunk_world_x,
                    chunk_world_y,
                    chunk_world_z,
                    overflow_blocks,
                );
            }
        }
    }

    // Upper layer
    for dx in -3i32..=3 {
        for dz in -3i32..=3 {
            let dist_sq = dx * dx + dz * dz;
            let dist = (dist_sq as f32).sqrt();
            if (2.0..=3.5).contains(&dist) {
                set_block_safe(
                    chunk,
                    x + dx,
                    canopy_y + 2,
                    z + dz,
                    BlockType::WillowLeaves,
                    chunk_world_x,
                    chunk_world_y,
                    chunk_world_z,
                    overflow_blocks,
                );
            }
        }
    }

    // Top cap
    for dx in -2i32..=2 {
        for dz in -2i32..=2 {
            if dx * dx + dz * dz <= 4 {
                set_block_safe(
                    chunk,
                    x + dx,
                    canopy_y + 3,
                    z + dz,
                    BlockType::WillowLeaves,
                    chunk_world_x,
                    chunk_world_y,
                    chunk_world_z,
                    overflow_blocks,
                );
            }
        }
    }
}

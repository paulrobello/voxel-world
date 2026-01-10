//! Oak tree generation.

use crate::chunk::{BlockType, Chunk};
use crate::world_gen::utils::{OverflowBlock, get_block_safe, set_block_safe};

/// Generate an oak tree (normal or giant based on hash).
#[allow(clippy::too_many_arguments)]
pub fn generate_oak(
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
    // Check if this should be a giant multi-deck tree (rare: ~10% chance)
    let is_giant = (hash % 10) == 0;

    if is_giant {
        generate_giant_oak(
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
        generate_normal_oak(
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
fn generate_normal_oak(
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
    // Check if there's solid ground below (no floating trees over caves)
    for check_y in (y.saturating_sub(2))..=y {
        if let Some(block) = get_block_safe(chunk, x, check_y, z) {
            if !block.is_solid() {
                return;
            }
        } else {
            return;
        }
    }

    // More variation: height 4-9, with different canopy sizes
    let height = 4 + (hash % 6);
    let canopy_size = (hash / 7) % 3;
    let trunk_offset = (hash / 13) % 2;
    let canopy_shape = (hash / 17) % 4;

    // Canopy layers - varies by size
    let layers = match canopy_size {
        0 => 3,
        1 => 4,
        _ => 5,
    };

    // Canopy placement
    let canopy_base = if height <= 5 {
        y + height - 2
    } else {
        y + height - 2 - trunk_offset
    };

    let trunk_top = canopy_base;

    // Trunk
    for dy in 1..trunk_top {
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

    // Add branches for taller trees with large canopies
    if height >= 7 && canopy_size == 2 && (hash % 3) == 0 {
        let branch_y = trunk_top - 3;
        let num_branches = 1 + ((hash / 43) % 2);

        for branch_idx in 0..num_branches {
            let branch_dir = (hash / (47 + branch_idx * 11)) % 4;
            let branch_len = 2 + ((hash / (53 + branch_idx * 7)) % 2);

            let (dx, dz) = match branch_dir {
                0 => (1, 0),
                1 => (-1, 0),
                2 => (0, 1),
                _ => (0, -1),
            };

            for i in 1..=branch_len {
                set_block_safe(
                    chunk,
                    x + dx * i,
                    branch_y,
                    z + dz * i,
                    BlockType::Log,
                    chunk_world_x,
                    chunk_world_y,
                    chunk_world_z,
                    overflow_blocks,
                );
            }

            let tip_x = x + dx * branch_len;
            let tip_z = z + dz * branch_len;
            let branch_shape = (hash / (59 + branch_idx * 13)) % 4;
            generate_oak_canopy(
                chunk,
                tip_x,
                branch_y,
                tip_z,
                0,
                3,
                branch_y,
                branch_shape,
                chunk_world_x,
                chunk_world_y,
                chunk_world_z,
                overflow_blocks,
            );
        }
    }

    generate_oak_canopy(
        chunk,
        x,
        canopy_base,
        z,
        canopy_size,
        layers,
        trunk_top - 1,
        canopy_shape,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        overflow_blocks,
    );
}

#[allow(clippy::too_many_arguments)]
fn generate_giant_oak(
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

    // Giant trees: 2-3 decks
    let num_decks = 2 + ((hash / 19) % 2);

    let mut deck_positions = Vec::new();
    let mut current_y = y;

    for deck_idx in 0..num_decks {
        let trunk_section = 3 + ((hash / (23 + deck_idx)) % 3);
        current_y += trunk_section;

        let canopy_size = if deck_idx == 0 {
            2 + ((hash / (29 + deck_idx)) % 2)
        } else if deck_idx == num_decks - 1 {
            1 + ((hash / (31 + deck_idx)) % 2)
        } else {
            1 + ((hash / (37 + deck_idx)) % 3)
        };

        let layers = match canopy_size {
            3 => 6,
            2 => 5,
            _ => 4,
        };

        deck_positions.push((current_y, canopy_size, layers, trunk_section, deck_idx));
        current_y += layers;
    }

    let highest_canopy_base = deck_positions
        .iter()
        .map(|(canopy_y, _, _, _, _)| *canopy_y)
        .max()
        .unwrap_or(y);

    let trunk_top = highest_canopy_base;

    // Build continuous trunk
    for dy in 1..trunk_top {
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

    // Place canopies at each deck position
    for &(canopy_y, canopy_size, layers, trunk_section, deck_idx) in &deck_positions {
        let deck_shape = (hash / (71 + deck_idx)) % 4;
        generate_oak_canopy(
            chunk,
            x,
            canopy_y,
            z,
            canopy_size,
            layers,
            trunk_top - 1,
            deck_shape,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );

        // Add branches from trunk section (not on the top deck)
        if deck_idx < num_decks - 1 {
            let max_branches = if deck_idx == 0 { 4 } else { 3 };
            let num_branches = 2 + ((hash / (31 + deck_idx)) % max_branches);

            let mut vertical_supports: Vec<(i32, i32, i32, i32)> = Vec::new();

            for branch_idx in 0..num_branches {
                let branch_dir = (hash / (37 + branch_idx * 7)) % 4;

                let min_len = if deck_idx == 0 { 3 } else { 1 };
                let max_len = if deck_idx == 0 { 7 } else { 5 };
                let branch_len =
                    min_len + ((hash / (41 + branch_idx * 5)) % (max_len - min_len + 1));

                let height_offset =
                    ((hash / (59 + branch_idx * 3)) % trunk_section.max(3)).min(trunk_section - 1);
                let branch_y = canopy_y - trunk_section + height_offset;

                let (dx, dz) = match branch_dir {
                    0 => (1, 0),
                    1 => (-1, 0),
                    2 => (0, 1),
                    _ => (0, -1),
                };

                for i in 1..=branch_len {
                    set_block_safe(
                        chunk,
                        x + dx * i,
                        branch_y,
                        z + dz * i,
                        BlockType::Log,
                        chunk_world_x,
                        chunk_world_y,
                        chunk_world_z,
                        overflow_blocks,
                    );
                }

                let tip_x = x + dx * branch_len;
                let tip_z = z + dz * branch_len;

                let has_vertical = (hash / (67 + branch_idx * 11)) % 10 < 7;
                if has_vertical {
                    let vertical_height = 4 + ((hash / (79 + branch_idx * 13)) % 7);
                    for vy in 1..=vertical_height {
                        set_block_safe(
                            chunk,
                            tip_x,
                            branch_y + vy,
                            tip_z,
                            BlockType::Log,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    }

                    vertical_supports.push((tip_x, branch_y, tip_z, vertical_height));

                    let vert_top_y = branch_y + vertical_height;
                    let branch_canopy_size = 1 + ((hash / (89 + branch_idx * 7)) % 2);
                    let branch_layers = 3 + ((hash / (97 + branch_idx * 5)) % 2);
                    let branch_shape = (hash / (73 + branch_idx * 17)) % 4;
                    generate_oak_canopy(
                        chunk,
                        tip_x,
                        vert_top_y,
                        tip_z,
                        branch_canopy_size,
                        branch_layers,
                        vert_top_y,
                        branch_shape,
                        chunk_world_x,
                        chunk_world_y,
                        chunk_world_z,
                        overflow_blocks,
                    );
                } else {
                    let branch_canopy_size = (hash / (67 + branch_idx * 11)) % 3;
                    let branch_layers = match branch_canopy_size {
                        0 => 3,
                        1 => 4,
                        _ => 5,
                    };
                    let branch_shape = (hash / (73 + branch_idx * 17)) % 4;
                    generate_oak_canopy(
                        chunk,
                        tip_x,
                        branch_y,
                        tip_z,
                        branch_canopy_size,
                        branch_layers,
                        branch_y,
                        branch_shape,
                        chunk_world_x,
                        chunk_world_y,
                        chunk_world_z,
                        overflow_blocks,
                    );
                }
            }

            // Add horizontal cross-bracing between nearby vertical supports
            for i in 0..vertical_supports.len() {
                for j in (i + 1)..vertical_supports.len() {
                    let (x1, y1, z1, h1) = vertical_supports[i];
                    let (x2, y2, z2, h2) = vertical_supports[j];

                    let dx = (x2 - x1).abs();
                    let dz = (z2 - z1).abs();

                    if (dx == 0 && dz > 0 && dz <= 12) || (dz == 0 && dx > 0 && dx <= 12) {
                        let min_height = h1.min(h2);
                        let brace_height_offset = min_height / 3
                            + ((hash / (101 + i as i32 * 7)) % (min_height / 3).max(1));
                        let brace_y = y1.max(y2) + brace_height_offset;

                        if dx == 0 {
                            let z_start = z1.min(z2);
                            let z_end = z1.max(z2);
                            for bz in z_start..=z_end {
                                set_block_safe(
                                    chunk,
                                    x1,
                                    brace_y,
                                    bz,
                                    BlockType::Log,
                                    chunk_world_x,
                                    chunk_world_y,
                                    chunk_world_z,
                                    overflow_blocks,
                                );
                            }
                        } else {
                            let x_start = x1.min(x2);
                            let x_end = x1.max(x2);
                            for bx in x_start..=x_end {
                                set_block_safe(
                                    chunk,
                                    bx,
                                    brace_y,
                                    z1,
                                    BlockType::Log,
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
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn generate_oak_canopy(
    chunk: &mut Chunk,
    x: i32,
    base_y: i32,
    z: i32,
    canopy_size: i32,
    layers: i32,
    trunk_top_y: i32,
    shape: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    let max_radius = match canopy_size {
        0 => 2.5,
        1 => 3.0,
        2 => 4.0,
        _ => 5.0,
    };

    let height = layers as f32;

    for dx in -(max_radius as i32)..=(max_radius as i32) {
        for dz in -(max_radius as i32)..=(max_radius as i32) {
            for dy in 0..layers {
                let dist_xz_squared = (dx * dx + dz * dz) as f32;
                let y_norm = dy as f32 / height;

                let radius_at_height = match shape {
                    0 => {
                        let t = y_norm - 0.5;
                        max_radius * (1.0 - 4.0 * t * t).max(0.3)
                    }
                    1 => max_radius * (1.0 - 0.3 * y_norm.abs()),
                    2 => max_radius * (1.0 - 0.7 * y_norm),
                    _ => max_radius * (0.4 + 0.6 * y_norm),
                };

                let r_squared_at_height = radius_at_height * radius_at_height;

                if dist_xz_squared <= r_squared_at_height {
                    let hash =
                        ((x + dx) * 73856093) ^ ((base_y + dy) * 19349663) ^ ((z + dz) * 83492791);
                    if (hash.abs() % 10) == 0 {
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
}

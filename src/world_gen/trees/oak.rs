//! Oak tree generation.

use crate::chunk::{BlockType, Chunk};
use crate::world_gen::utils::{OverflowBlock, get_block_safe, set_block_safe};

/// Cross-shaped trunk pattern offsets (plus sign when viewed from above).
/// ```text
///  #
/// ###
///  #
/// ```
const TRUNK_CROSS_OFFSETS: [(i32, i32); 5] = [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)];

/// Place a cross-shaped trunk section at the given position.
#[allow(clippy::too_many_arguments)]
fn place_trunk_cross(
    chunk: &mut Chunk,
    x: i32,
    y: i32,
    z: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    for &(dx, dz) in &TRUNK_CROSS_OFFSETS {
        set_block_safe(
            chunk,
            x + dx,
            y,
            z + dz,
            BlockType::Log,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }
}

/// Place a thick (2-wide) branch log at the given position.
/// Thickens perpendicular to the branch direction.
#[allow(clippy::too_many_arguments)]
fn place_thick_branch_log(
    chunk: &mut Chunk,
    px: i32,
    py: i32,
    pz: i32,
    dir_x: i32,
    dir_z: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    // Place main log
    set_block_safe(
        chunk,
        px,
        py,
        pz,
        BlockType::Log,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        overflow_blocks,
    );

    // Calculate perpendicular direction for thickness
    // Perpendicular to (dir_x, dir_z) is (-dir_z, dir_x)
    let perp_x = if dir_z != 0 {
        if dir_z > 0 { -1 } else { 1 }
    } else {
        0
    };
    let perp_z = if dir_x != 0 {
        if dir_x > 0 { 1 } else { -1 }
    } else {
        0
    };

    // If branch is diagonal, thicken in both perpendicular directions
    if dir_x != 0 && dir_z != 0 {
        // Place logs to fill the diagonal gap
        set_block_safe(
            chunk,
            px + 1,
            py,
            pz,
            BlockType::Log,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
        set_block_safe(
            chunk,
            px,
            py,
            pz + 1,
            BlockType::Log,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    } else if perp_x != 0 || perp_z != 0 {
        // Place perpendicular log for axis-aligned branches
        set_block_safe(
            chunk,
            px + perp_x,
            py,
            pz + perp_z,
            BlockType::Log,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }
}

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
    // Check if this should be a majestic old-growth oak (rare: ~10% chance)
    let is_majestic = (hash % 10) == 0;

    if is_majestic {
        generate_majestic_oak(
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

    // Calculate relative trunk height (canopy_base is absolute, convert to relative)
    let trunk_height = canopy_base - y;

    // Trunk
    for dy in 1..trunk_height {
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
        let branch_y = canopy_base - 3;
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
        canopy_base - 1,
        canopy_shape,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        overflow_blocks,
    );
}

/// Majestic oak - large old-growth style oak with organic branching.
///
/// Features two trunk variants (forking and single) with curved branches
/// that exhibit the signature oak "droop then rise" silhouette.
#[allow(clippy::too_many_arguments)]
fn generate_majestic_oak(
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
    // Check if there's solid ground below (no floating trees)
    for check_y in (y.saturating_sub(2))..=y {
        if let Some(block) = get_block_safe(chunk, x, check_y, z) {
            if !block.is_solid() {
                return;
            }
        } else {
            return;
        }
    }

    // Height: 12-24 blocks
    let height = 12 + (hash % 13);

    // 50% chance of forking trunk vs single trunk
    let is_forking = (hash / 17) % 2 == 0;

    if is_forking {
        generate_forking_trunk_oak(
            chunk,
            x,
            y,
            z,
            height,
            hash,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    } else {
        generate_single_trunk_oak(
            chunk,
            x,
            y,
            z,
            height,
            hash,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }
}

/// Forking trunk variant: trunk splits into 2-4 major limbs.
#[allow(clippy::too_many_arguments)]
fn generate_forking_trunk_oak(
    chunk: &mut Chunk,
    x: i32,
    y: i32,
    z: i32,
    height: i32,
    hash: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    // Fork height: 40-60% of total height
    let fork_ratio = 40 + (hash / 19) % 21; // 40-60
    let fork_height = (height * fork_ratio) / 100;

    // Number of major limbs: 2-4
    let num_limbs = 2 + (hash / 23) % 3;

    // Pre-check: verify limb positions won't collide with existing trees
    // This prevents partial trees when two forking oaks grow toward each other
    for limb_idx in 0..num_limbs {
        let angle_offset = (limb_idx * 360 / num_limbs) + (hash / (29 + limb_idx)) % 45;
        let angle_rad = (angle_offset as f32) * std::f32::consts::PI / 180.0;
        let limb_rise = 3 + (hash / (31 + limb_idx * 7)) % 4;
        let limb_spread = 2 + (hash / (37 + limb_idx * 5)) % 3;

        let dx = (angle_rad.cos() * limb_spread as f32).round() as i32;
        let dz = (angle_rad.sin() * limb_spread as f32).round() as i32;
        let limb_end_y = y + fork_height + limb_rise;

        // Check if limb endpoint has existing logs
        if let Some(block) = get_block_safe(chunk, x + dx, limb_end_y, z + dz) {
            if block == BlockType::Log {
                return; // Another tree is in the way, skip this tree entirely
            }
        }
    }

    // Build main trunk up to fork point with cross-shaped pattern
    for dy in 1..=fork_height {
        place_trunk_cross(
            chunk,
            x,
            y + dy,
            z,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }

    // Generate major limbs from fork point
    let mut limb_endpoints: Vec<(i32, i32, i32)> = Vec::new();

    for limb_idx in 0..num_limbs {
        // Distribute limbs around the trunk (not all same direction)
        let angle_offset = (limb_idx * 360 / num_limbs) + (hash / (29 + limb_idx)) % 45;
        let angle_rad = (angle_offset as f32) * std::f32::consts::PI / 180.0;

        // Major limb rises at 30-60 degrees, extends 3-6 blocks
        let limb_rise = 3 + (hash / (31 + limb_idx * 7)) % 4; // 3-6 blocks up
        let limb_spread = 2 + (hash / (37 + limb_idx * 5)) % 3; // 2-4 blocks out

        let dx = (angle_rad.cos() * limb_spread as f32).round() as i32;
        let dz = (angle_rad.sin() * limb_spread as f32).round() as i32;

        // Generate the major limb with slight curve
        let limb_end_x = x + dx;
        let limb_end_y = y + fork_height + limb_rise;
        let limb_end_z = z + dz;

        // Place logs along the limb (diagonal rise) with thick branches
        let steps = limb_rise.max(limb_spread.abs().max(1));
        let dir_x = dx.signum();
        let dir_z = dz.signum();
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let lx = x + (dx as f32 * t).round() as i32;
            let ly = y + fork_height + (limb_rise as f32 * t).round() as i32;
            let lz = z + (dz as f32 * t).round() as i32;

            place_thick_branch_log(
                chunk,
                lx,
                ly,
                lz,
                dir_x,
                dir_z,
                chunk_world_x,
                chunk_world_y,
                chunk_world_z,
                overflow_blocks,
            );
        }

        limb_endpoints.push((limb_end_x, limb_end_y, limb_end_z));
    }

    // Generate secondary branches from each major limb
    let crown_center_y = y + fork_height + (height - fork_height) / 2;

    for (limb_idx, &(lx, ly, lz)) in limb_endpoints.iter().enumerate() {
        // 2-4 secondary branches per limb
        let num_branches = 2 + (hash / (41 + limb_idx as i32 * 11)) % 3;

        for branch_idx in 0..num_branches {
            let branch_hash = hash
                .wrapping_mul(73 + limb_idx as i32)
                .wrapping_add(branch_idx);

            // Branch endpoint within crown sphere
            let angle = (branch_hash % 360) as f32 * std::f32::consts::PI / 180.0;
            let branch_len = 3 + (branch_hash / 7) % 5; // 3-7 blocks
            let end_rise = (branch_hash / 11) % 4; // 0-3 blocks up

            let end_x = lx + (angle.cos() * branch_len as f32).round() as i32;
            let end_y = ly + end_rise;
            let end_z = lz + (angle.sin() * branch_len as f32).round() as i32;

            // Generate curved branch with droop-rise pattern
            generate_curved_branch(
                chunk,
                lx,
                ly,
                lz,
                end_x,
                end_y,
                end_z,
                branch_hash,
                chunk_world_x,
                chunk_world_y,
                chunk_world_z,
                overflow_blocks,
            );

            // Leaf cluster at branch endpoint
            generate_leaf_cluster(
                chunk,
                end_x,
                end_y,
                end_z,
                3 + (branch_hash / 13) % 2, // radius 3-4
                branch_hash,
                chunk_world_x,
                chunk_world_y,
                chunk_world_z,
                overflow_blocks,
            );
        }

        // Leaf cluster at major limb endpoint too
        generate_leaf_cluster(
            chunk,
            lx,
            ly,
            lz,
            4,
            hash.wrapping_mul(limb_idx as i32 + 1),
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }

    // Add a crown cluster at top center
    generate_leaf_cluster(
        chunk,
        x,
        crown_center_y + 2,
        z,
        5,
        hash,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        overflow_blocks,
    );
}

/// Single trunk variant: continuous trunk with radiating crown branches.
#[allow(clippy::too_many_arguments)]
fn generate_single_trunk_oak(
    chunk: &mut Chunk,
    x: i32,
    y: i32,
    z: i32,
    height: i32,
    hash: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    // Trunk extends to ~70% of height
    let trunk_top = (height * 70) / 100;

    // Build continuous trunk with cross-shaped pattern
    for dy in 1..=trunk_top {
        place_trunk_cross(
            chunk,
            x,
            y + dy,
            z,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }

    // Generate 8-12 branches radiating from upper trunk
    let num_branches = 8 + (hash / 19) % 5;

    for branch_idx in 0..num_branches {
        let branch_hash = hash.wrapping_mul(59).wrapping_add(branch_idx);

        // Attachment point on trunk (upper 40%)
        let attach_offset = (trunk_top * 60) / 100 + (branch_hash % (trunk_top * 40 / 100)).max(1);
        let attach_y = y + attach_offset;

        // Branch direction - distribute around trunk
        let angle = ((branch_idx * 360 / num_branches) + (branch_hash / 7) % 30) as f32
            * std::f32::consts::PI
            / 180.0;

        // Branch length: 4-8 blocks
        let branch_len = 4 + (branch_hash / 11) % 5;

        // End point with upward rise
        let end_rise = 1 + (branch_hash / 13) % 4; // Rise 1-4 blocks
        let end_x = x + (angle.cos() * branch_len as f32).round() as i32;
        let end_y = attach_y + end_rise;
        let end_z = z + (angle.sin() * branch_len as f32).round() as i32;

        // Generate curved branch
        generate_curved_branch(
            chunk,
            x,
            attach_y,
            z,
            end_x,
            end_y,
            end_z,
            branch_hash,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );

        // Leaf cluster at endpoint
        generate_leaf_cluster(
            chunk,
            end_x,
            end_y,
            end_z,
            3 + (branch_hash / 17) % 2,
            branch_hash,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }

    // Central crown cluster at top
    generate_leaf_cluster(
        chunk,
        x,
        y + trunk_top + 2,
        z,
        5,
        hash,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        overflow_blocks,
    );
}

/// Generate a curved branch with droop-then-rise pattern.
///
/// Uses 3 control points to create organic curved branches:
/// 1. Start point (attachment to trunk/limb)
/// 2. Mid point (droops down slightly)
/// 3. End point (rises up)
#[allow(clippy::too_many_arguments)]
fn generate_curved_branch(
    chunk: &mut Chunk,
    start_x: i32,
    start_y: i32,
    start_z: i32,
    end_x: i32,
    end_y: i32,
    end_z: i32,
    hash: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    // Calculate midpoint with droop
    let mid_x = (start_x + end_x) / 2;
    let mid_z = (start_z + end_z) / 2;

    // Droop amount: 1-3 blocks down at midpoint
    let droop = 1 + (hash / 23) % 3;
    let mid_y = ((start_y + end_y) / 2) - droop;

    // Calculate total path length for step count
    let dx1 = (mid_x - start_x).abs();
    let dy1 = (mid_y - start_y).abs();
    let dz1 = (mid_z - start_z).abs();
    let dx2 = (end_x - mid_x).abs();
    let dy2 = (end_y - mid_y).abs();
    let dz2 = (end_z - mid_z).abs();

    let seg1_len = dx1.max(dy1).max(dz1).max(1);
    let seg2_len = dx2.max(dy2).max(dz2).max(1);

    // Direction for first segment (start to mid)
    let dir1_x = (mid_x - start_x).signum();
    let dir1_z = (mid_z - start_z).signum();

    // First segment: start to mid (droop) with thick branches
    for step in 1..=seg1_len {
        let t = step as f32 / seg1_len as f32;
        let px = start_x + ((mid_x - start_x) as f32 * t).round() as i32;
        let py = start_y + ((mid_y - start_y) as f32 * t).round() as i32;
        let pz = start_z + ((mid_z - start_z) as f32 * t).round() as i32;

        place_thick_branch_log(
            chunk,
            px,
            py,
            pz,
            dir1_x,
            dir1_z,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }

    // Direction for second segment (mid to end)
    let dir2_x = (end_x - mid_x).signum();
    let dir2_z = (end_z - mid_z).signum();

    // Second segment: mid to end (rise) with thick branches
    for step in 1..=seg2_len {
        let t = step as f32 / seg2_len as f32;
        let px = mid_x + ((end_x - mid_x) as f32 * t).round() as i32;
        let py = mid_y + ((end_y - mid_y) as f32 * t).round() as i32;
        let pz = mid_z + ((end_z - mid_z) as f32 * t).round() as i32;

        place_thick_branch_log(
            chunk,
            px,
            py,
            pz,
            dir2_x,
            dir2_z,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }
}

/// Generate an ellipsoid leaf cluster at a branch endpoint.
#[allow(clippy::too_many_arguments)]
fn generate_leaf_cluster(
    chunk: &mut Chunk,
    cx: i32,
    cy: i32,
    cz: i32,
    radius: i32,
    hash: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    // Slightly flattened ellipsoid (more horizontal spread)
    let h_radius = radius as f32;
    let v_radius = (radius as f32 * 0.7).max(2.0); // Vertical is 70% of horizontal

    for dx in -radius..=radius {
        for dy in -(radius - 1)..=radius {
            for dz in -radius..=radius {
                // Ellipsoid distance check
                let dist_h = (dx * dx + dz * dz) as f32 / (h_radius * h_radius);
                let dist_v = (dy * dy) as f32 / (v_radius * v_radius);

                if dist_h + dist_v <= 1.0 {
                    // Random removal for organic look (10-15%)
                    let leaf_hash = ((cx + dx) * 73856093)
                        ^ ((cy + dy) * 19349663)
                        ^ ((cz + dz) * 83492791)
                        ^ hash;
                    if (leaf_hash.abs() % 10) == 0 {
                        continue;
                    }

                    // Bias toward more leaves above center (top-heavy)
                    if dy < -1 && (leaf_hash.abs() % 4) == 0 {
                        continue;
                    }

                    set_block_safe(
                        chunk,
                        cx + dx,
                        cy + dy,
                        cz + dz,
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

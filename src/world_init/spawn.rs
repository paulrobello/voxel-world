use nalgebra::vector;

use crate::chunk::BlockType;
use crate::constants::TEXTURE_SIZE_Y;
use crate::world::World;

/// Finds the ground level (highest non-air block) at the given world coordinates.
pub fn find_ground_level(world: &World, world_x: i32, world_z: i32) -> i32 {
    // Search from top of world downward (Y dimension is still bounded)
    for y in (0..TEXTURE_SIZE_Y as i32).rev() {
        let pos = vector![world_x, y, world_z];
        if let Some(block) = world.get_block(pos) {
            if block != BlockType::Air && block != BlockType::Water {
                return y;
            }
        }
    }
    // Fallback to base height if nothing found (raised terrain)
    128
}

/// Checks if a position is safe to spawn (solid ground, not in water/lava).
fn is_safe_spawn(world: &World, world_x: i32, world_z: i32) -> bool {
    // Check from near surface height downward
    for y in (60..TEXTURE_SIZE_Y as i32).rev() {
        let pos = vector![world_x, y, world_z];
        if let Some(block) = world.get_block(pos) {
            match block {
                BlockType::Air => continue,
                BlockType::Water | BlockType::Lava => return false,
                _ => {
                    // Found solid ground - check block above is air (not underwater)
                    let above = vector![world_x, y + 1, world_z];
                    if let Some(above_block) = world.get_block(above) {
                        return above_block == BlockType::Air;
                    }
                    return true;
                }
            }
        }
    }
    false
}

/// Finds a safe spawn point starting from the given origin.
/// Searches in an expanding spiral pattern to find dry land.
/// Returns (x, z) of a safe spawn point, or the origin if none found.
pub fn find_safe_spawn(world: &World, origin_x: i32, origin_z: i32, max_radius: i32) -> (i32, i32) {
    // First check the origin
    if is_safe_spawn(world, origin_x, origin_z) {
        return (origin_x, origin_z);
    }

    // Search in expanding square spiral
    for radius in 1..=max_radius {
        // Check positions on the perimeter of the square at this radius
        // Top and bottom edges
        for dx in -radius..=radius {
            let x = origin_x + dx;
            // Top edge
            let z = origin_z - radius;
            if is_safe_spawn(world, x, z) {
                return (x, z);
            }
            // Bottom edge
            let z = origin_z + radius;
            if is_safe_spawn(world, x, z) {
                return (x, z);
            }
        }
        // Left and right edges (excluding corners already checked)
        for dz in (-radius + 1)..radius {
            let z = origin_z + dz;
            // Left edge
            let x = origin_x - radius;
            if is_safe_spawn(world, x, z) {
                return (x, z);
            }
            // Right edge
            let x = origin_x + radius;
            if is_safe_spawn(world, x, z) {
                return (x, z);
            }
        }
    }

    // Fallback to origin if no safe spawn found
    println!(
        "[SPAWN] Warning: No safe spawn found within {} blocks of origin, using origin",
        max_radius
    );
    (origin_x, origin_z)
}

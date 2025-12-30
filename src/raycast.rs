//! CPU-based raycasting for voxel world interaction.
//!
//! This module provides DDA (Digital Differential Analyzer) based
//! raycasting to find which block the player is looking at.

use crate::world::World;
use nalgebra::Vector3;

/// Maximum distance to raycast (in blocks) - 1 chunk.
pub const MAX_RAYCAST_DISTANCE: f32 = 32.0;

/// Result of a raycast operation.
#[derive(Debug, Clone, Copy)]
pub struct RaycastHit {
    /// World position of the hit block.
    pub block_pos: Vector3<i32>,
    /// Normal of the face that was hit (direction from which the ray entered).
    pub normal: Vector3<i32>,
    /// Distance from the ray origin to the hit point.
    pub distance: f32,
}

/// Performs a raycast through the voxel world using the DDA algorithm.
///
/// Returns the first solid block hit and the face normal, or None if no hit.
///
/// # Arguments
/// * `world` - The voxel world to raycast through
/// * `origin` - Starting position of the ray (in world coordinates)
/// * `direction` - Direction of the ray (will be normalized)
/// * `max_distance` - Maximum distance to search
pub fn raycast(
    world: &World,
    origin: Vector3<f32>,
    direction: Vector3<f32>,
    max_distance: f32,
) -> Option<RaycastHit> {
    let dir = direction.normalize();

    // Handle edge case of zero direction
    if dir.x.is_nan() || dir.y.is_nan() || dir.z.is_nan() {
        return None;
    }

    // Current voxel position
    let mut pos = Vector3::new(
        origin.x.floor() as i32,
        origin.y.floor() as i32,
        origin.z.floor() as i32,
    );

    // Step direction for each axis (+1 or -1)
    let step = Vector3::new(
        if dir.x >= 0.0 { 1 } else { -1 },
        if dir.y >= 0.0 { 1 } else { -1 },
        if dir.z >= 0.0 { 1 } else { -1 },
    );

    // Distance along ray to next voxel boundary for each axis
    // t_max = distance to the next grid line in each direction
    let mut t_max = Vector3::new(
        if dir.x != 0.0 {
            let next_x = if dir.x > 0.0 {
                (pos.x + 1) as f32
            } else {
                pos.x as f32
            };
            (next_x - origin.x) / dir.x
        } else {
            f32::INFINITY
        },
        if dir.y != 0.0 {
            let next_y = if dir.y > 0.0 {
                (pos.y + 1) as f32
            } else {
                pos.y as f32
            };
            (next_y - origin.y) / dir.y
        } else {
            f32::INFINITY
        },
        if dir.z != 0.0 {
            let next_z = if dir.z > 0.0 {
                (pos.z + 1) as f32
            } else {
                pos.z as f32
            };
            (next_z - origin.z) / dir.z
        } else {
            f32::INFINITY
        },
    );

    // Distance along ray to cross one voxel for each axis
    let t_delta = Vector3::new(
        if dir.x != 0.0 {
            (1.0 / dir.x).abs()
        } else {
            f32::INFINITY
        },
        if dir.y != 0.0 {
            (1.0 / dir.y).abs()
        } else {
            f32::INFINITY
        },
        if dir.z != 0.0 {
            (1.0 / dir.z).abs()
        } else {
            f32::INFINITY
        },
    );

    // Track which face we entered from
    let mut normal = Vector3::new(0, 0, 0);
    let mut distance = 0.0f32;

    // DDA loop
    while distance < max_distance {
        // Check if current voxel is solid (skip air, water, and other non-solid blocks)
        if let Some(block) = world.get_block(pos) {
            if block.is_solid() {
                return Some(RaycastHit {
                    block_pos: pos,
                    normal,
                    distance,
                });
            }
        }

        // Step to next voxel
        if t_max.x < t_max.y && t_max.x < t_max.z {
            distance = t_max.x;
            t_max.x += t_delta.x;
            pos.x += step.x;
            normal = Vector3::new(-step.x, 0, 0);
        } else if t_max.y < t_max.z {
            distance = t_max.y;
            t_max.y += t_delta.y;
            pos.y += step.y;
            normal = Vector3::new(0, -step.y, 0);
        } else {
            distance = t_max.z;
            t_max.z += t_delta.z;
            pos.z += step.z;
            normal = Vector3::new(0, 0, -step.z);
        }
    }

    None
}

/// Calculates the position where a new block would be placed.
///
/// This is the block position adjacent to the hit block, in the direction
/// of the hit normal.
pub fn get_place_position(hit: &RaycastHit) -> Vector3<i32> {
    hit.block_pos + hit.normal
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{BlockType, Chunk};
    use nalgebra::vector;

    fn create_test_world() -> World {
        let mut world = World::new();
        let mut chunk = Chunk::new();

        // Create a solid floor at y=0
        for x in 0..32 {
            for z in 0..32 {
                chunk.set_block(x, 0, z, BlockType::Stone);
            }
        }

        // Add a single block at (5, 1, 5)
        chunk.set_block(5, 1, 5, BlockType::Stone);

        world.insert_chunk(vector![0, 0, 0], chunk);
        world
    }

    #[test]
    fn test_raycast_hit_floor() {
        let world = create_test_world();

        // Cast ray straight down
        let origin = Vector3::new(5.5, 10.0, 5.5);
        let direction = Vector3::new(0.0, -1.0, 0.0);

        let hit = raycast(&world, origin, direction, 20.0);
        assert!(hit.is_some());

        let hit = hit.unwrap();
        // Should hit the block at (5, 1, 5) first
        assert_eq!(hit.block_pos, Vector3::new(5, 1, 5));
        assert_eq!(hit.normal, Vector3::new(0, 1, 0)); // Hit from above
    }

    #[test]
    fn test_raycast_miss() {
        let world = create_test_world();

        // Cast ray that misses everything
        let origin = Vector3::new(100.0, 100.0, 100.0);
        let direction = Vector3::new(1.0, 0.0, 0.0);

        let hit = raycast(&world, origin, direction, 20.0);
        assert!(hit.is_none());
    }

    #[test]
    fn test_place_position() {
        let hit = RaycastHit {
            block_pos: Vector3::new(5, 1, 5),
            normal: Vector3::new(0, 1, 0),
            distance: 5.0,
        };

        let place_pos = get_place_position(&hit);
        assert_eq!(place_pos, Vector3::new(5, 2, 5));
    }

    #[test]
    fn test_raycast_diagonal() {
        let world = create_test_world();

        // Cast ray diagonally toward the floor
        let origin = Vector3::new(0.5, 5.0, 0.5);
        let direction = Vector3::new(0.1, -1.0, 0.1).normalize();

        let hit = raycast(&world, origin, direction, 20.0);
        assert!(hit.is_some());

        let hit = hit.unwrap();
        // Should hit the floor
        assert_eq!(hit.block_pos.y, 0);
    }
}

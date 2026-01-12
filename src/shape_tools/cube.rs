//! Cube generation algorithm for the cube placement tool.
//!
//! This module provides functions to generate cube/box block positions
//! with support for hollow and dome modes.

use nalgebra::Vector3;

use super::PlacementMode;

/// Generate all block positions for a cube/box.
///
/// # Arguments
/// * `center` - Center position of the cube in world coordinates
/// * `size_x` - Half-size in X direction (full width = size_x * 2 + 1)
/// * `size_y` - Half-size in Y direction (full height = size_y * 2 + 1)
/// * `size_z` - Half-size in Z direction (full depth = size_z * 2 + 1)
/// * `hollow` - If true, only generate shell positions (walls, floor, ceiling)
/// * `dome` - If true, only generate the top half (y >= center.y)
///
/// # Returns
/// Vector of all block positions that make up the cube.
pub fn generate_cube_positions(
    center: Vector3<i32>,
    size_x: i32,
    size_y: i32,
    size_z: i32,
    hollow: bool,
    dome: bool,
) -> Vec<Vector3<i32>> {
    let mut positions = Vec::new();

    if size_x <= 0 || size_y <= 0 || size_z <= 0 {
        return positions;
    }

    // For dome mode, start at center.y instead of center.y - size_y
    let y_start = if dome { center.y } else { center.y - size_y };
    let y_end = center.y + size_y;

    let x_start = center.x - size_x;
    let x_end = center.x + size_x;
    let z_start = center.z - size_z;
    let z_end = center.z + size_z;

    for x in x_start..=x_end {
        for y in y_start..=y_end {
            for z in z_start..=z_end {
                if hollow {
                    // Only place on faces (shell)
                    let on_x_face = x == x_start || x == x_end;
                    let on_y_face = y == y_start || y == y_end;
                    let on_z_face = z == z_start || z == z_end;

                    if on_x_face || on_y_face || on_z_face {
                        positions.push(Vector3::new(x, y, z));
                    }
                } else {
                    positions.push(Vector3::new(x, y, z));
                }
            }
        }
    }

    positions
}

/// Calculate effective center position based on placement mode.
///
/// # Arguments
/// * `target` - Target position (where user is aiming)
/// * `size_y` - Half-size in Y direction
/// * `mode` - Placement mode (Center or Base)
///
/// # Returns
/// The actual center position for the cube.
pub fn calculate_center(target: Vector3<i32>, size_y: i32, mode: PlacementMode) -> Vector3<i32> {
    match mode {
        PlacementMode::Center => target,
        PlacementMode::Base => Vector3::new(target.x, target.y + size_y, target.z),
    }
}

/// Estimate cube volume.
///
/// # Arguments
/// * `size_x` - Half-size in X direction
/// * `size_y` - Half-size in Y direction
/// * `size_z` - Half-size in Z direction
/// * `hollow` - If true, calculate shell volume only
/// * `dome` - If true, calculate half volume
///
/// # Returns
/// Estimated number of blocks.
#[allow(dead_code)]
pub fn estimate_volume(size_x: i32, size_y: i32, size_z: i32, hollow: bool, dome: bool) -> u64 {
    let width = (size_x * 2 + 1) as u64;
    let height = (size_y * 2 + 1) as u64;
    let depth = (size_z * 2 + 1) as u64;

    let mut volume = if hollow {
        // Shell volume = outer - inner
        let outer = width * height * depth;
        let inner_width = width.saturating_sub(2);
        let inner_height = height.saturating_sub(2);
        let inner_depth = depth.saturating_sub(2);
        let inner = inner_width * inner_height * inner_depth;
        outer - inner
    } else {
        width * height * depth
    };

    if dome {
        volume /= 2;
    }

    volume
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_solid_cube() {
        let center = Vector3::new(0, 0, 0);
        let positions = generate_cube_positions(center, 1, 1, 1, false, false);
        // 3x3x3 = 27 blocks
        assert_eq!(positions.len(), 27);
        assert!(positions.contains(&center));
    }

    #[test]
    fn test_hollow_cube() {
        let center = Vector3::new(10, 10, 10);
        let solid = generate_cube_positions(center, 2, 2, 2, false, false);
        let hollow = generate_cube_positions(center, 2, 2, 2, true, false);

        // Hollow should have fewer blocks than solid
        assert!(hollow.len() < solid.len());
        // Both should contain some blocks
        assert!(!solid.is_empty());
        assert!(!hollow.is_empty());

        // Hollow should not contain interior blocks
        // Interior is from -1 to +1 in each axis (center)
        assert!(!hollow.contains(&center));
    }

    #[test]
    fn test_dome_mode() {
        let center = Vector3::new(10, 10, 10);
        let full_cube = generate_cube_positions(center, 3, 3, 3, false, false);
        let dome = generate_cube_positions(center, 3, 3, 3, false, true);

        // Dome should have roughly half the blocks
        assert!(dome.len() < full_cube.len());

        // Dome should not contain any blocks below center
        for pos in &dome {
            assert!(
                pos.y >= center.y,
                "Dome should not have blocks below center"
            );
        }

        // Dome should contain the center
        assert!(dome.contains(&center));
    }

    #[test]
    fn test_hollow_dome() {
        let center = Vector3::new(10, 10, 10);
        let hollow_dome = generate_cube_positions(center, 3, 3, 3, true, true);

        // Should have blocks
        assert!(!hollow_dome.is_empty());

        // No blocks below center
        for pos in &hollow_dome {
            assert!(pos.y >= center.y);
        }
    }

    #[test]
    fn test_calculate_center_modes() {
        let target = Vector3::new(10, 20, 30);
        let size_y = 5;

        let center_mode = calculate_center(target, size_y, PlacementMode::Center);
        assert_eq!(center_mode, target);

        let base_mode = calculate_center(target, size_y, PlacementMode::Base);
        assert_eq!(base_mode, Vector3::new(10, 25, 30));
    }

    #[test]
    fn test_estimate_volume() {
        // Solid cube 5x5x5 = 125 blocks
        let solid_vol = estimate_volume(2, 2, 2, false, false);
        assert_eq!(solid_vol, 125);

        // Hollow cube 5x5x5 - 3x3x3 = 125 - 27 = 98 blocks
        let hollow_vol = estimate_volume(2, 2, 2, true, false);
        assert_eq!(hollow_vol, 98);
    }

    #[test]
    fn test_asymmetric_cube() {
        let center = Vector3::new(0, 0, 0);
        // 5 wide, 3 tall, 7 deep
        let positions = generate_cube_positions(center, 2, 1, 3, false, false);
        // 5 * 3 * 7 = 105 blocks
        assert_eq!(positions.len(), 105);
    }
}

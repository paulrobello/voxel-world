//! Wall generation algorithm for the wall placement tool.
//!
//! This module provides functions to generate wall block positions
//! from two corner points, with configurable thickness.

use nalgebra::Vector3;

/// Generate all block positions for a wall between two corners.
///
/// The wall is generated as a vertical rectangle connecting the two corner positions.
/// The wall's primary axis (X or Z) is automatically determined by which direction
/// has the greater distance between the corners.
///
/// # Arguments
/// * `start` - First corner position (set on first click)
/// * `end` - Second corner position (set on second click)
/// * `thickness` - Wall thickness in blocks (1-5)
/// * `manual_height` - If Some, override height; if None, use Y difference between corners
///
/// # Returns
/// Vector of all block positions that make up the wall.
pub fn generate_wall_positions(
    start: Vector3<i32>,
    end: Vector3<i32>,
    thickness: i32,
    manual_height: Option<i32>,
) -> Vec<Vector3<i32>> {
    let mut positions = Vec::new();

    let thickness = thickness.clamp(1, 5);

    // Determine primary direction (X or Z based on which has larger distance)
    let dx = (end.x - start.x).abs();
    let dz = (end.z - start.z).abs();

    // Calculate height from corners or use manual override
    let (min_y, max_y) = if let Some(h) = manual_height {
        (start.y.min(end.y), start.y.min(end.y) + h - 1)
    } else {
        (start.y.min(end.y), start.y.max(end.y))
    };

    // Ensure at least 1 block height
    let height = (max_y - min_y + 1).max(1);

    if dx >= dz {
        // Primary axis is X, thickness extends in Z
        let x_min = start.x.min(end.x);
        let x_max = start.x.max(end.x);
        let z_base = (start.z + end.z) / 2; // Center thickness around midpoint

        let half_thick = thickness / 2;
        let z_start = z_base - half_thick;
        let z_end = z_base + half_thick;

        for x in x_min..=x_max {
            for y in min_y..=min_y + height - 1 {
                for z in z_start..=z_end {
                    positions.push(Vector3::new(x, y, z));
                }
            }
        }
    } else {
        // Primary axis is Z, thickness extends in X
        let z_min = start.z.min(end.z);
        let z_max = start.z.max(end.z);
        let x_base = (start.x + end.x) / 2; // Center thickness around midpoint

        let half_thick = thickness / 2;
        let x_start = x_base - half_thick;
        let x_end = x_base + half_thick;

        for z in z_min..=z_max {
            for y in min_y..=min_y + height - 1 {
                for x in x_start..=x_end {
                    positions.push(Vector3::new(x, y, z));
                }
            }
        }
    }

    positions
}

/// Calculate wall dimensions for display.
///
/// # Returns
/// (length, height, thickness) tuple
pub fn calculate_dimensions(
    start: Vector3<i32>,
    end: Vector3<i32>,
    thickness: i32,
    manual_height: Option<i32>,
) -> (i32, i32, i32) {
    let dx = (end.x - start.x).abs() + 1;
    let dz = (end.z - start.z).abs() + 1;
    let length = dx.max(dz);

    let height = if let Some(h) = manual_height {
        h
    } else {
        (start.y - end.y).abs() + 1
    };

    (length, height.max(1), thickness)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_wall_x_axis() {
        let start = Vector3::new(0, 0, 5);
        let end = Vector3::new(10, 5, 5);

        let positions = generate_wall_positions(start, end, 1, None);

        // Wall should be 11 blocks long (0-10), 6 blocks tall (0-5), 1 thick
        assert_eq!(positions.len(), 11 * 6 * 1);

        // Check that all positions are in expected range
        for pos in &positions {
            assert!(pos.x >= 0 && pos.x <= 10);
            assert!(pos.y >= 0 && pos.y <= 5);
            assert_eq!(pos.z, 5);
        }
    }

    #[test]
    fn test_simple_wall_z_axis() {
        let start = Vector3::new(5, 0, 0);
        let end = Vector3::new(5, 5, 10);

        let positions = generate_wall_positions(start, end, 1, None);

        // Wall should be 11 blocks long (0-10 in Z), 6 blocks tall (0-5), 1 thick
        assert_eq!(positions.len(), 11 * 6 * 1);

        // Check that all positions are in expected range
        for pos in &positions {
            assert_eq!(pos.x, 5);
            assert!(pos.y >= 0 && pos.y <= 5);
            assert!(pos.z >= 0 && pos.z <= 10);
        }
    }

    #[test]
    fn test_wall_with_thickness() {
        let start = Vector3::new(0, 0, 5);
        let end = Vector3::new(10, 0, 5);

        let thin = generate_wall_positions(start, end, 1, None);
        let thick = generate_wall_positions(start, end, 3, None);

        // Thick wall should have more blocks
        assert!(thick.len() > thin.len());
    }

    #[test]
    fn test_wall_manual_height() {
        let start = Vector3::new(0, 10, 5);
        let end = Vector3::new(5, 10, 5); // Same Y, so auto-height would be 1

        // With manual height of 5
        let positions = generate_wall_positions(start, end, 1, Some(5));

        // Should have 6 blocks wide * 5 tall * 1 thick = 30 blocks
        assert_eq!(positions.len(), 6 * 5 * 1);
    }

    #[test]
    fn test_calculate_dimensions() {
        let start = Vector3::new(0, 0, 0);
        let end = Vector3::new(10, 5, 0);

        let (length, height, thick) = calculate_dimensions(start, end, 2, None);

        assert_eq!(length, 11); // 0 to 10 inclusive
        assert_eq!(height, 6); // 0 to 5 inclusive
        assert_eq!(thick, 2);
    }

    #[test]
    fn test_wall_reversed_corners() {
        // Start and end can be in any order
        let start = Vector3::new(10, 5, 5);
        let end = Vector3::new(0, 0, 5);

        let positions = generate_wall_positions(start, end, 1, None);

        // Should still produce correct wall
        assert_eq!(positions.len(), 11 * 6);
    }
}

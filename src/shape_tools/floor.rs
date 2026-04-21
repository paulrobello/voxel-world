//! Floor/Platform generation algorithm for the floor placement tool.
//!
//! This module provides functions to generate floor block positions
//! from two corner points, with configurable thickness and direction.

use nalgebra::Vector3;

/// Direction for floor/platform building.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FloorDirection {
    /// Floor mode: builds thickness downward from Y level
    #[default]
    Floor,
    /// Ceiling mode: builds thickness upward from Y level
    Ceiling,
}

impl FloorDirection {
    /// Get display name for the direction.
    pub fn name(&self) -> &'static str {
        match self {
            FloorDirection::Floor => "Floor",
            FloorDirection::Ceiling => "Ceiling",
        }
    }
}

/// Generate all block positions for a floor between two corners.
///
/// The floor is generated as a horizontal rectangle at the Y level of the first corner.
/// Thickness extends downward (Floor mode) or upward (Ceiling mode).
///
/// # Arguments
/// * `start` - First corner position (sets Y level)
/// * `end` - Second corner position (X/Z extent only, Y ignored)
/// * `thickness` - Floor thickness in blocks (1-5)
/// * `direction` - Whether to build downward (Floor) or upward (Ceiling)
///
/// # Returns
/// Vector of all block positions that make up the floor.
pub fn generate_floor_positions(
    start: Vector3<i32>,
    end: Vector3<i32>,
    thickness: i32,
    direction: FloorDirection,
) -> Vec<Vector3<i32>> {
    let mut positions = Vec::new();

    let thickness = thickness.clamp(1, 5);

    // Floor Y level from start position
    let base_y = start.y;

    // Calculate Y range based on direction
    let (min_y, max_y) = match direction {
        FloorDirection::Floor => (base_y - thickness + 1, base_y),
        FloorDirection::Ceiling => (base_y, base_y + thickness - 1),
    };

    // X and Z bounds from both corners
    let x_min = start.x.min(end.x);
    let x_max = start.x.max(end.x);
    let z_min = start.z.min(end.z);
    let z_max = start.z.max(end.z);

    for x in x_min..=x_max {
        for z in z_min..=z_max {
            for y in min_y..=max_y {
                positions.push(Vector3::new(x, y, z));
            }
        }
    }

    positions
}

/// Calculate floor dimensions for display.
///
/// # Returns
/// (length, width, thickness) tuple
pub fn calculate_dimensions(
    start: Vector3<i32>,
    end: Vector3<i32>,
    thickness: i32,
) -> (i32, i32, i32) {
    let length = (end.x - start.x).abs() + 1;
    let width = (end.z - start.z).abs() + 1;

    (length, width, thickness)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_floor() {
        let start = Vector3::new(0, 10, 0);
        let end = Vector3::new(5, 10, 5);

        let positions = generate_floor_positions(start, end, 1, FloorDirection::Floor);

        // Floor should be 6x6 blocks, 1 thick = 36 blocks
        assert_eq!(positions.len(), 6 * 6);

        // All blocks should be at Y=10
        for pos in &positions {
            assert_eq!(pos.y, 10);
            assert!(pos.x >= 0 && pos.x <= 5);
            assert!(pos.z >= 0 && pos.z <= 5);
        }
    }

    #[test]
    fn test_floor_with_thickness() {
        let start = Vector3::new(0, 10, 0);
        let end = Vector3::new(2, 10, 2);

        let positions = generate_floor_positions(start, end, 3, FloorDirection::Floor);

        // 3x3 area, 3 thick = 27 blocks
        assert_eq!(positions.len(), 3 * 3 * 3);

        // Y should range from 8 to 10 (thickness downward)
        let min_y = positions.iter().map(|p| p.y).min().unwrap();
        let max_y = positions.iter().map(|p| p.y).max().unwrap();
        assert_eq!(min_y, 8);
        assert_eq!(max_y, 10);
    }

    #[test]
    fn test_ceiling_mode() {
        let start = Vector3::new(0, 10, 0);
        let end = Vector3::new(2, 10, 2);

        let positions = generate_floor_positions(start, end, 3, FloorDirection::Ceiling);

        // 3x3 area, 3 thick = 27 blocks
        assert_eq!(positions.len(), 3 * 3 * 3);

        // Y should range from 10 to 12 (thickness upward)
        let min_y = positions.iter().map(|p| p.y).min().unwrap();
        let max_y = positions.iter().map(|p| p.y).max().unwrap();
        assert_eq!(min_y, 10);
        assert_eq!(max_y, 12);
    }

    #[test]
    fn test_floor_reversed_corners() {
        let start = Vector3::new(5, 10, 5);
        let end = Vector3::new(0, 10, 0);

        let positions = generate_floor_positions(start, end, 1, FloorDirection::Floor);

        // Should still produce correct floor
        assert_eq!(positions.len(), 6 * 6);
    }

    #[test]
    fn test_calculate_dimensions() {
        let start = Vector3::new(0, 10, 0);
        let end = Vector3::new(10, 10, 5);

        let (length, width, thick) = calculate_dimensions(start, end, 2);

        assert_eq!(length, 11); // 0 to 10 inclusive
        assert_eq!(width, 6); // 0 to 5 inclusive
        assert_eq!(thick, 2);
    }

    #[test]
    fn test_end_y_ignored() {
        // End Y should be ignored - floor uses start Y
        let start = Vector3::new(0, 10, 0);
        let end = Vector3::new(2, 50, 2); // Y=50 should be ignored

        let positions = generate_floor_positions(start, end, 1, FloorDirection::Floor);

        // All blocks should be at Y=10 (start Y)
        for pos in &positions {
            assert_eq!(pos.y, 10);
        }
    }
}

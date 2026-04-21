//! Bridge (line) generation algorithm for the bridge placement tool.
//!
//! This module provides functions to generate a line of block positions
//! between two points using the 3D Bresenham algorithm.

use nalgebra::Vector3;

/// Generate all block positions for a line between two points.
///
/// Uses 3D Bresenham's line algorithm to generate a continuous line
/// of blocks from start to end.
///
/// # Arguments
/// * `start` - Starting position of the line in world coordinates
/// * `end` - Ending position of the line in world coordinates
///
/// # Returns
/// Vector of all block positions that make up the line, including start and end.
pub fn generate_line_positions(start: Vector3<i32>, end: Vector3<i32>) -> Vec<Vector3<i32>> {
    let mut positions = Vec::new();

    // Calculate deltas
    let dx = (end.x - start.x).abs();
    let dy = (end.y - start.y).abs();
    let dz = (end.z - start.z).abs();

    // Calculate step directions
    let sx = if end.x > start.x { 1 } else { -1 };
    let sy = if end.y > start.y { 1 } else { -1 };
    let sz = if end.z > start.z { 1 } else { -1 };

    // Determine which axis is dominant
    let dm = dx.max(dy).max(dz);

    // Handle single point case
    if dm == 0 {
        positions.push(start);
        return positions;
    }

    // Initialize position
    let mut x = start.x;
    let mut y = start.y;
    let mut z = start.z;

    // Initialize error terms
    let mut err_x = dm / 2;
    let mut err_y = dm / 2;
    let mut err_z = dm / 2;

    // Draw line using 3D Bresenham
    for _ in 0..=dm {
        positions.push(Vector3::new(x, y, z));

        // Update error terms and step
        err_x -= dx;
        if err_x < 0 {
            x += sx;
            err_x += dm;
        }

        err_y -= dy;
        if err_y < 0 {
            y += sy;
            err_y += dm;
        }

        err_z -= dz;
        if err_z < 0 {
            z += sz;
            err_z += dm;
        }
    }

    positions
}

/// Calculate the length of a line in blocks.
///
/// # Arguments
/// * `start` - Starting position
/// * `end` - Ending position
///
/// # Returns
/// The number of blocks in the line.
#[allow(dead_code)]
pub fn line_length(start: Vector3<i32>, end: Vector3<i32>) -> usize {
    let dx = (end.x - start.x).abs();
    let dy = (end.y - start.y).abs();
    let dz = (end.z - start.z).abs();
    (dx.max(dy).max(dz) + 1) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_point() {
        let start = Vector3::new(0, 0, 0);
        let positions = generate_line_positions(start, start);
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0], start);
    }

    #[test]
    fn test_horizontal_line_x() {
        let start = Vector3::new(0, 0, 0);
        let end = Vector3::new(5, 0, 0);
        let positions = generate_line_positions(start, end);
        assert_eq!(positions.len(), 6);
        assert!(positions.contains(&start));
        assert!(positions.contains(&end));
        // All Y and Z should be 0
        for pos in &positions {
            assert_eq!(pos.y, 0);
            assert_eq!(pos.z, 0);
        }
    }

    #[test]
    fn test_vertical_line_y() {
        let start = Vector3::new(10, 5, 10);
        let end = Vector3::new(10, 15, 10);
        let positions = generate_line_positions(start, end);
        assert_eq!(positions.len(), 11);
        assert!(positions.contains(&start));
        assert!(positions.contains(&end));
        // All X and Z should be 10
        for pos in &positions {
            assert_eq!(pos.x, 10);
            assert_eq!(pos.z, 10);
        }
    }

    #[test]
    fn test_diagonal_line() {
        let start = Vector3::new(0, 0, 0);
        let end = Vector3::new(5, 5, 5);
        let positions = generate_line_positions(start, end);
        // Diagonal should have 6 blocks
        assert_eq!(positions.len(), 6);
        assert!(positions.contains(&start));
        assert!(positions.contains(&end));
    }

    #[test]
    fn test_negative_direction() {
        let start = Vector3::new(10, 10, 10);
        let end = Vector3::new(5, 5, 5);
        let positions = generate_line_positions(start, end);
        assert_eq!(positions.len(), 6);
        assert!(positions.contains(&start));
        assert!(positions.contains(&end));
    }

    #[test]
    fn test_line_length() {
        assert_eq!(line_length(Vector3::new(0, 0, 0), Vector3::new(0, 0, 0)), 1);
        assert_eq!(line_length(Vector3::new(0, 0, 0), Vector3::new(5, 0, 0)), 6);
        assert_eq!(line_length(Vector3::new(0, 0, 0), Vector3::new(5, 5, 5)), 6);
    }
}

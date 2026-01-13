//! Stairs generation algorithm.
//!
//! This module provides functions to generate staircase positions
//! between two points with configurable width.

use nalgebra::Vector3;

/// Generate positions for a staircase between two points.
///
/// # Arguments
/// * `start` - Starting position (typically lower end)
/// * `end` - Ending position (typically upper end)
/// * `width` - Width of the staircase in blocks (perpendicular to travel direction)
///
/// # Returns
/// Vector of block positions forming the staircase. Each step is 1 block high.
pub fn generate_stair_positions(
    start: Vector3<i32>,
    end: Vector3<i32>,
    width: i32,
) -> Vec<Vector3<i32>> {
    let mut positions = Vec::new();

    if width <= 0 {
        return positions;
    }

    let height_diff = end.y - start.y;
    let dx = end.x - start.x;
    let dz = end.z - start.z;

    // If no height difference, no stairs needed
    if height_diff == 0 {
        return positions;
    }

    // Determine the step count (absolute height difference)
    let step_count = height_diff.abs();

    // Calculate horizontal distance
    let horizontal_dist = ((dx * dx + dz * dz) as f64).sqrt();

    // Run per step (how far horizontally each step travels)
    let run_per_step = if step_count > 0 {
        horizontal_dist / step_count as f64
    } else {
        0.0
    };

    // Direction of travel (normalized)
    let dir_x = if horizontal_dist > 0.0 {
        dx as f64 / horizontal_dist
    } else {
        0.0
    };
    let dir_z = if horizontal_dist > 0.0 {
        dz as f64 / horizontal_dist
    } else {
        0.0
    };

    // Perpendicular direction for width
    let perp_x = -dir_z;
    let perp_z = dir_x;

    // Determine if going up or down
    let going_up = height_diff > 0;
    let (lower, _upper) = if going_up { (start, end) } else { (end, start) };

    // Generate each step - fill in all blocks from this step to the next
    for step in 0..step_count {
        // Calculate start and end positions along the path for this step
        let progress_start = step as f64 * run_per_step;
        let progress_end = (step + 1) as f64 * run_per_step;
        let base_y = lower.y + step;

        // Calculate how many blocks we need to place along the travel direction
        let blocks_in_run = (run_per_step.ceil() as i32).max(1);

        // Place blocks along the entire run of this step
        for r in 0..=blocks_in_run {
            let t = if blocks_in_run > 0 {
                r as f64 / blocks_in_run as f64
            } else {
                0.0
            };
            let progress = progress_start + t * (progress_end - progress_start);

            // Don't go past the total horizontal distance
            if progress > horizontal_dist {
                break;
            }

            let base_x = lower.x as f64 + dir_x * progress;
            let base_z = lower.z as f64 + dir_z * progress;

            // Add blocks for the width
            let half_width = (width - 1) as f64 / 2.0;
            for w in 0..width {
                let offset = w as f64 - half_width;
                let block_x = (base_x + perp_x * offset).round() as i32;
                let block_z = (base_z + perp_z * offset).round() as i32;
                let block_pos = Vector3::new(block_x, base_y, block_z);

                if !positions.contains(&block_pos) {
                    positions.push(block_pos);
                }
            }
        }
    }

    positions
}

/// Estimate the number of blocks in a staircase.
#[allow(dead_code)]
pub fn estimate_block_count(start: Vector3<i32>, end: Vector3<i32>, width: i32) -> usize {
    let height_diff = (end.y - start.y).abs();
    (height_diff * width) as usize
}

/// Calculate staircase dimensions for display.
pub fn calculate_dimensions(start: Vector3<i32>, end: Vector3<i32>) -> (i32, i32, i32) {
    let height = (end.y - start.y).abs();
    let dx = (end.x - start.x).abs();
    let dz = (end.z - start.z).abs();
    let horizontal = ((dx * dx + dz * dz) as f64).sqrt().ceil() as i32;
    (height, horizontal, height) // height, horizontal distance, step count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_stair() {
        let start = Vector3::new(0, 0, 0);
        let end = Vector3::new(5, 5, 0);
        let positions = generate_stair_positions(start, end, 1);

        // 5 steps high, each step has 2 blocks (fills horizontal run)
        assert_eq!(positions.len(), 10);

        // Check that Y coordinates are 0, 1, 2, 3, 4
        for i in 0..5 {
            assert!(positions.iter().any(|p| p.y == i));
        }
    }

    #[test]
    fn test_stair_with_width() {
        let start = Vector3::new(0, 0, 0);
        let end = Vector3::new(3, 3, 0);
        let positions = generate_stair_positions(start, end, 3);

        // 3 steps high, each step has 2 blocks in run, 3 wide = 18 blocks
        assert_eq!(positions.len(), 18);
    }

    #[test]
    fn test_descending_stair() {
        let start = Vector3::new(0, 5, 0);
        let end = Vector3::new(5, 0, 0);
        let positions = generate_stair_positions(start, end, 1);

        // 5 steps, each with 2 blocks = 10 blocks
        assert_eq!(positions.len(), 10);
    }

    #[test]
    fn test_diagonal_stair() {
        let start = Vector3::new(0, 0, 0);
        let end = Vector3::new(4, 4, 4);
        let positions = generate_stair_positions(start, end, 1);

        // 4 steps, diagonal run fills 2 blocks per step
        assert_eq!(positions.len(), 8);
    }

    #[test]
    fn test_no_height_difference() {
        let start = Vector3::new(0, 5, 0);
        let end = Vector3::new(10, 5, 0);
        let positions = generate_stair_positions(start, end, 1);

        // No stairs if no height difference
        assert!(positions.is_empty());
    }

    #[test]
    fn test_zero_width() {
        let start = Vector3::new(0, 0, 0);
        let end = Vector3::new(5, 5, 0);
        let positions = generate_stair_positions(start, end, 0);

        assert!(positions.is_empty());
    }

    #[test]
    fn test_estimate_block_count() {
        let start = Vector3::new(0, 0, 0);
        let end = Vector3::new(5, 10, 0);
        let count = estimate_block_count(start, end, 2);

        assert_eq!(count, 20); // 10 steps * 2 width
    }
}

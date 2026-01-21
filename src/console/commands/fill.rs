//! Fill command implementation.
//!
//! Fills a rectangular region with the specified block type.

use crate::chunk::BlockType;
use crate::console::{
    CommandResult, parse_coordinate, validate_y_bounds, volume_confirm_threshold,
};
use crate::world::World;
use nalgebra::Vector3;

/// Execute the fill command.
///
/// Syntax: fill <block> <x1> <y1> <z1> <x2> <y2> <z2> [hollow]
pub fn fill(
    args: &[&str],
    world: &mut World,
    player_pos: Vector3<i32>,
    confirmed: bool,
) -> CommandResult {
    // Parse arguments
    if args.len() < 7 {
        return CommandResult::Error(
            "Usage: fill <block> <x1> <y1> <z1> <x2> <y2> <z2> [hollow]".to_string(),
        );
    }

    // Parse block name
    let block_name = args[0].to_lowercase();
    let block = match BlockType::from_name(&block_name) {
        Some(b) => b,
        None => {
            return CommandResult::Error(format!(
                "Unknown block type: '{}'. Valid types: {}",
                block_name,
                BlockType::all_block_names().join(", ")
            ));
        }
    };

    // Parse coordinates
    let x1 = match parse_coordinate(args[1], player_pos.x) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let y1 = match parse_coordinate(args[2], player_pos.y) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let z1 = match parse_coordinate(args[3], player_pos.z) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let x2 = match parse_coordinate(args[4], player_pos.x) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let y2 = match parse_coordinate(args[5], player_pos.y) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let z2 = match parse_coordinate(args[6], player_pos.z) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };

    // Check for hollow flag
    let hollow = args.len() > 7 && args[7].to_lowercase() == "hollow";

    // Normalize coordinates (min/max)
    let min_x = x1.min(x2);
    let max_x = x1.max(x2);
    let min_y = y1.min(y2);
    let max_y = y1.max(y2);
    let min_z = z1.min(z2);
    let max_z = z1.max(z2);

    // Validate Y bounds
    if let Some(error) = validate_y_bounds(min_y) {
        return CommandResult::Error(error);
    }
    if let Some(error) = validate_y_bounds(max_y) {
        return CommandResult::Error(error);
    }

    // Calculate volume
    let width = (max_x - min_x + 1) as u64;
    let height = (max_y - min_y + 1) as u64;
    let depth = (max_z - min_z + 1) as u64;
    let volume = width * height * depth;

    // Calculate actual blocks to fill (for hollow, only outer shell)
    let fill_count = if hollow {
        calculate_hollow_volume(width, height, depth)
    } else {
        volume
    };

    // Check volume threshold
    if !confirmed && fill_count > volume_confirm_threshold() {
        let original_cmd = args.join(" ");
        return CommandResult::NeedsConfirmation {
            message: format!("This will modify {} blocks. Are you sure?", fill_count),
            command: format!("fill {}", original_cmd),
        };
    }

    // Execute the fill
    let mut count = 0u64;
    for x in min_x..=max_x {
        for y in min_y..=max_y {
            for z in min_z..=max_z {
                // For hollow, only fill if on the boundary
                let is_boundary = x == min_x
                    || x == max_x
                    || y == min_y
                    || y == max_y
                    || z == min_z
                    || z == max_z;
                if hollow && !is_boundary {
                    // Fill interior with air
                    world.set_block(Vector3::new(x, y, z), BlockType::Air);
                } else {
                    world.set_block(Vector3::new(x, y, z), block);
                }
                count += 1;
            }
        }
    }

    let mode = if hollow { " (hollow)" } else { "" };
    CommandResult::Success(format!("Filled {} blocks with {:?}{}", count, block, mode))
}

/// Calculate the number of blocks in a hollow box shell.
fn calculate_hollow_volume(width: u64, height: u64, depth: u64) -> u64 {
    if width <= 2 || height <= 2 || depth <= 2 {
        // Box is too small to be hollow - all blocks are on boundary
        width * height * depth
    } else {
        // Total volume minus interior
        let total = width * height * depth;
        let interior = (width - 2) * (height - 2) * (depth - 2);
        total - interior
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper for testing boundary detection.
    fn is_boundary_test(
        x: i32,
        y: i32,
        z: i32,
        min_x: i32,
        max_x: i32,
        min_y: i32,
        max_y: i32,
        min_z: i32,
        max_z: i32,
    ) -> bool {
        x == min_x || x == max_x || y == min_y || y == max_y || z == min_z || z == max_z
    }

    #[test]
    fn test_is_boundary() {
        // Corners are boundaries
        assert!(is_boundary_test(0, 0, 0, 0, 2, 0, 2, 0, 2));
        assert!(is_boundary_test(2, 2, 2, 0, 2, 0, 2, 0, 2));

        // Center is not a boundary (for 3x3x3 box)
        assert!(!is_boundary_test(1, 1, 1, 0, 2, 0, 2, 0, 2));

        // Edges are boundaries
        assert!(is_boundary_test(0, 1, 1, 0, 2, 0, 2, 0, 2));
        assert!(is_boundary_test(1, 0, 1, 0, 2, 0, 2, 0, 2));
        assert!(is_boundary_test(1, 1, 0, 0, 2, 0, 2, 0, 2));
    }

    #[test]
    fn test_hollow_volume() {
        // 3x3x3 box: 27 total - 1 interior = 26
        assert_eq!(calculate_hollow_volume(3, 3, 3), 26);

        // 4x4x4 box: 64 total - 8 interior = 56
        assert_eq!(calculate_hollow_volume(4, 4, 4), 56);

        // 2x2x2 box: all boundary (no interior)
        assert_eq!(calculate_hollow_volume(2, 2, 2), 8);

        // 1x1x1 box: single block
        assert_eq!(calculate_hollow_volume(1, 1, 1), 1);
    }
}

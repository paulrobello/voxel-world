//! Teleport command implementation.
//!
//! Teleports the player to specified coordinates.

use crate::console::{CommandResult, parse_coordinate, validate_y_bounds};
use nalgebra::Vector3;

/// Execute the tp (teleport) command.
///
/// Syntax: tp <x> <y> <z>
/// Coordinates support relative values with ~ prefix.
pub fn tp(args: &[&str], player_pos: Vector3<i32>) -> CommandResult {
    if args.len() < 3 {
        return CommandResult::Error("Usage: tp <x> <y> <z>".to_string());
    }

    // Parse coordinates
    let x = match parse_coordinate(args[0], player_pos.x) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let y = match parse_coordinate(args[1], player_pos.y) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let z = match parse_coordinate(args[2], player_pos.z) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };

    // Validate Y bounds
    if let Some(error) = validate_y_bounds(y) {
        return CommandResult::Error(error);
    }

    CommandResult::Teleport {
        x: x as f64 + 0.5, // Center of block
        y: y as f64,
        z: z as f64 + 0.5, // Center of block
    }
}

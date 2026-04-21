//! Boxme command implementation.
//!
//! Creates a hollow box around the player - a convenience macro for fill.

use super::fill;
use crate::console::CommandResult;
use crate::world::World;
use nalgebra::Vector3;

/// Execute the boxme command.
///
/// Syntax: boxme <block> <size>
/// Equivalent to: fill <block> ~-size ~ ~-size ~size ~size ~size hollow
pub fn boxme(
    args: &[&str],
    world: &mut World,
    player_pos: Vector3<i32>,
    confirmed: bool,
) -> CommandResult {
    if args.len() < 2 {
        return CommandResult::Error("Usage: boxme <block> <size>".to_string());
    }

    let block_name = args[0];
    let size_str = args[1];

    // Parse size
    let size: i32 = match size_str.parse() {
        Ok(s) if s > 0 => s,
        Ok(_) => return CommandResult::Error("Size must be a positive integer".to_string()),
        Err(_) => return CommandResult::Error(format!("Invalid size: '{}'", size_str)),
    };

    // Build the fill arguments: block ~-size ~ ~-size ~size ~size ~size hollow
    let neg_size = format!("~-{}", size);
    let pos_size = format!("~{}", size);

    let fill_args: Vec<&str> = vec![
        block_name, &neg_size, // x1: ~-size
        "~",       // y1: ~ (player y)
        &neg_size, // z1: ~-size
        &pos_size, // x2: ~size
        &pos_size, // y2: ~size
        &pos_size, // z2: ~size
        "hollow",
    ];

    fill::fill(&fill_args, world, player_pos, confirmed)
}

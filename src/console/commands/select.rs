//! Selection command implementation for template regions.

use crate::console::CommandResult;
use crate::templates::TemplateSelection;
use nalgebra::Vector3;

/// Handles /select command with subcommands: pos1, pos2, clear
#[allow(dead_code)] // TODO: Remove once integrated with main.rs
pub fn select(
    args: &[&str],
    player_pos: Vector3<f64>,
    selection: &mut TemplateSelection,
) -> CommandResult {
    if args.is_empty() {
        return CommandResult::Error("Usage: /select pos1|pos2|clear [x] [y] [z]".to_string());
    }

    let subcommand = args[0];

    match subcommand {
        "pos1" => {
            let pos = if args.len() >= 4 {
                // Parse explicit coordinates
                match parse_coords(&args[1..4], player_pos) {
                    Ok(p) => p,
                    Err(e) => return CommandResult::Error(e),
                }
            } else {
                // Use player position (block at feet level, not the block standing on)
                Vector3::new(
                    player_pos.x.floor() as i32,
                    player_pos.y.floor() as i32,
                    player_pos.z.floor() as i32,
                )
            };

            selection.set_pos1(pos);
            CommandResult::Success(format!(
                "Position 1 set to ({}, {}, {})",
                pos.x, pos.y, pos.z
            ))
        }

        "pos2" => {
            let pos = if args.len() >= 4 {
                // Parse explicit coordinates
                match parse_coords(&args[1..4], player_pos) {
                    Ok(p) => p,
                    Err(e) => return CommandResult::Error(e),
                }
            } else {
                // Use player position (block at feet level, not the block standing on)
                Vector3::new(
                    player_pos.x.floor() as i32,
                    player_pos.y.floor() as i32,
                    player_pos.z.floor() as i32,
                )
            };

            selection.set_pos2(pos);

            // Show selection info if both positions are set
            if let Some((_min, _max)) = selection.bounds() {
                let dims = selection.dimensions().unwrap();
                CommandResult::Success(format!(
                    "Position 2 set to ({}, {}, {}). Selection: {}×{}×{} ({} blocks)",
                    pos.x,
                    pos.y,
                    pos.z,
                    dims.0,
                    dims.1,
                    dims.2,
                    selection.volume().unwrap()
                ))
            } else {
                CommandResult::Success(format!(
                    "Position 2 set to ({}, {}, {})",
                    pos.x, pos.y, pos.z
                ))
            }
        }

        "clear" => {
            selection.clear();
            CommandResult::Success("Selection cleared".to_string())
        }

        _ => CommandResult::Error(format!(
            "Unknown subcommand '{}'. Use: pos1, pos2, or clear",
            subcommand
        )),
    }
}

/// Parses coordinates with support for relative positioning (~)
fn parse_coords(args: &[&str], player_pos: Vector3<f64>) -> Result<Vector3<i32>, String> {
    if args.len() != 3 {
        return Err("Expected 3 coordinates (x y z)".to_string());
    }

    let x = parse_coordinate(args[0], player_pos.x.floor() as i32)?;
    let y = parse_coordinate(args[1], player_pos.y.floor() as i32)?;
    let z = parse_coordinate(args[2], player_pos.z.floor() as i32)?;

    Ok(Vector3::new(x, y, z))
}

/// Parses a single coordinate, supporting relative positioning with ~
fn parse_coordinate(s: &str, relative_to: i32) -> Result<i32, String> {
    if let Some(offset_str) = s.strip_prefix('~') {
        if offset_str.is_empty() {
            Ok(relative_to)
        } else {
            match offset_str.parse::<i32>() {
                Ok(offset) => Ok(relative_to + offset),
                Err(_) => Err(format!("Invalid offset: {}", offset_str)),
            }
        }
    } else {
        s.parse::<i32>()
            .map_err(|_| format!("Invalid coordinate: {}", s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_coordinate() {
        // Absolute
        assert_eq!(parse_coordinate("100", 50).unwrap(), 100);
        assert_eq!(parse_coordinate("-50", 50).unwrap(), -50);

        // Relative
        assert_eq!(parse_coordinate("~", 50).unwrap(), 50);
        assert_eq!(parse_coordinate("~10", 50).unwrap(), 60);
        assert_eq!(parse_coordinate("~-10", 50).unwrap(), 40);

        // Errors
        assert!(parse_coordinate("abc", 50).is_err());
        assert!(parse_coordinate("~abc", 50).is_err());
    }

    #[test]
    fn test_parse_coords() {
        let player_pos = Vector3::new(100.5, 65.0, 200.5);

        // Absolute
        let result = parse_coords(&["10", "20", "30"], player_pos).unwrap();
        assert_eq!(result, Vector3::new(10, 20, 30));

        // Relative (now uses feet level, not block below)
        let result = parse_coords(&["~", "~", "~"], player_pos).unwrap();
        assert_eq!(result, Vector3::new(100, 65, 200));

        // Mixed
        let result = parse_coords(&["~5", "65", "~-10"], player_pos).unwrap();
        assert_eq!(result, Vector3::new(105, 65, 190));
    }
}

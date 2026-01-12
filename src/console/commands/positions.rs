//! Position saving console commands.
//!
//! Manages named saved positions per world.

use crate::console::CommandResult;

/// Execute the save_pos command.
///
/// Syntax: save_pos <name>
/// Saves the player's current position with the given name.
pub fn save_pos(args: &[&str]) -> CommandResult {
    if args.is_empty() {
        return CommandResult::Error(
            "Usage: save_pos <name>\nSaves your current position with the given name.".to_string(),
        );
    }

    let name = args[0].to_string();
    if name.is_empty() {
        return CommandResult::Error("Position name cannot be empty.".to_string());
    }

    // Validate name: no special characters that could cause issues
    if name.contains(|c: char| !c.is_alphanumeric() && c != '_' && c != '-') {
        return CommandResult::Error(
            "Position name can only contain letters, numbers, underscores, and hyphens."
                .to_string(),
        );
    }

    CommandResult::SavePosition { name }
}

/// Execute the delete_pos command.
///
/// Syntax: delete_pos <name>
/// Deletes the saved position with the given name.
pub fn delete_pos(args: &[&str]) -> CommandResult {
    if args.is_empty() {
        return CommandResult::Error(
            "Usage: delete_pos <name>\nDeletes the saved position with the given name.".to_string(),
        );
    }

    let name = args[0].to_string();
    if name.is_empty() {
        return CommandResult::Error("Position name cannot be empty.".to_string());
    }

    CommandResult::DeletePosition { name }
}

/// Execute the list_pos command.
///
/// Syntax: list_pos
/// Lists all saved positions for the current world.
pub fn list_pos() -> CommandResult {
    CommandResult::ListPositions
}

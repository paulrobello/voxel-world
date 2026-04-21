//! Measurement tool console commands.
//!
//! Manages measurement markers in the world.

use crate::console::CommandResult;

/// Execute the measure command.
///
/// Syntax: measure <subcommand>
/// Subcommands:
///   clear - Remove all measurement markers
pub fn measure(args: &[&str]) -> CommandResult {
    if args.is_empty() {
        return CommandResult::Error("Usage: measure <subcommand>\nSubcommands: clear".to_string());
    }

    match args[0].to_lowercase().as_str() {
        "clear" => CommandResult::ClearMeasurementMarkers,
        _ => CommandResult::Error(format!(
            "Unknown subcommand '{}'. Available: clear",
            args[0]
        )),
    }
}

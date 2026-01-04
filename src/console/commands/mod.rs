//! Console command implementations.
//!
//! Each command function takes arguments and returns a `CommandResult`.

mod fill;
mod tp;

pub use fill::fill;
pub use tp::tp;

use super::CommandResult;

/// Help command - list available commands.
pub fn help() -> CommandResult {
    let help_text = r#"Available commands:
  fill <block> <x1> <y1> <z1> <x2> <y2> <z2> [hollow]
    Fill a region with blocks. Use 'air' to clear.
    Coordinates support ~ for relative (e.g., ~5 = player + 5)
    'hollow' flag creates a shell with air inside

  tp <x> <y> <z>
    Teleport to coordinates. Y must be 0-255.

  clear
    Clear console output

  help, ?
    Show this help message

Examples:
  fill stone 0 0 0 10 5 10
  fill air ~-5 ~ ~-5 ~5 ~10 ~5
  fill brick ~ ~-1 ~ ~10 ~3 ~10 hollow
  tp 100 64 200
  tp ~ ~10 ~"#;

    CommandResult::Success(help_text.to_string())
}

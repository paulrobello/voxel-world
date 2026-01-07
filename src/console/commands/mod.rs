//! Console command implementations.
//!
//! Each command function takes arguments and returns a `CommandResult`.

mod boxme;
mod fill;
mod select;
mod sphere;
mod template;
mod tp;

pub use boxme::boxme;
pub use fill::fill;
#[allow(unused_imports)] // TODO: Remove once integrated with main.rs
pub use select::select;
pub use sphere::sphere;
#[allow(unused_imports)] // TODO: Remove once integrated with main.rs
pub use template::template;
pub use tp::tp;

use super::CommandResult;

/// Help command - list available commands.
pub fn help() -> CommandResult {
    let help_text = r#"Available commands:
  fill <block> <x1> <y1> <z1> <x2> <y2> <z2> [hollow]
    Fill a region with blocks. Use 'air' to clear.
    Coordinates support ~ for relative (e.g., ~5 = player + 5)
    'hollow' flag creates a shell with air inside

  sphere <block> <cx> <cy> <cz> <radius> [hollow]
    Create a sphere of blocks at center (cx, cy, cz).
    Coordinates support ~ for relative positions.
    'hollow' flag creates a shell with air inside.

  boxme <block> <size>
    Create a hollow box around you (shortcut for fill hollow).
    Equivalent to: fill <block> ~-size ~ ~-size ~size ~size ~size hollow

  tp <x> <y> <z>
    Teleport to coordinates. Y must be 0-255.

  select pos1|pos2|clear [x] [y] [z]
    Manage template region selection.
    pos1/pos2: Set selection corners (use current position if no coords)
    clear: Clear the current selection

  template save|load|list|delete|info <name> [tags...]
    Manage world templates.
    save <name> [tags]: Save current selection as template
    load <name>: Load template for placement
    list: Show all saved templates
    delete <name>: Delete a template
    info <name>: Show template details

  waterdebug, wd
    Show water/lava simulation debug info.

  waterforce, wf
    Force ALL water cells to become active (unstick water).

  wateranalyze, wa
    Analyze water flow at player position (debug).

  clear
    Clear console output

  help, ?
    Show this help message

Examples:
  fill stone 0 0 0 10 5 10
  fill air ~-5 ~ ~-5 ~5 ~10 ~5
  fill brick ~ ~-1 ~ ~10 ~3 ~10 hollow
  sphere stone ~ ~5 ~ 10
  sphere glass ~ ~ ~ 15 hollow
  boxme brick 5
  tp 100 64 200
  tp ~ ~10 ~
  select pos1
  select pos2 100 64 200
  select clear
  template save my_house building decorated
  template load my_house
  template list
  template info my_house"#;

    CommandResult::Success(help_text.to_string())
}

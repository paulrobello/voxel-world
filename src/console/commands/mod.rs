//! Console command implementations.
//!
//! Each command function takes arguments and returns a `CommandResult`.

mod boxme;
mod copy;
mod fill;
mod locate;
mod locate_async;
mod select;
mod sphere;
mod template;
mod tp;

pub use boxme::boxme;
pub use copy::copy;
pub use fill::fill;
pub use locate::locate;
pub use locate_async::update_locate_search;
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

  copy <x1> <y1> <z1> <x2> <y2> <z2> <dx> <dy> <dz> [rotate_90|rotate_180|rotate_270]
    Copy a region from source to destination with optional rotation.
    Coordinates support ~ for relative positions.
    Rotation flags: rotate_90, rotate_180, rotate_270

  tp <x> <y> <z>
    Teleport to coordinates. Y must be 0-511.

  locate <biome|block|cave> [range|size] [range] [tp]
    Find the nearest biome, block type, or cave.
    Biomes: grassland, mountains, desert, swamp, snow
    Blocks: stone, water, lava, etc. (any block type)
    Cave: locate cave [min_size] [range] (default size: 50 blocks)
    Add 'tp' flag to teleport to location when found.
    Reports coordinates, distance, and direction.
    Searches run in background and show progress updates.

  cancel, cancellocate
    Cancel an active locate search.

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
  copy 0 0 0 10 5 10 20 0 0
  copy ~ ~ ~ ~10 ~5 ~10 ~20 ~ ~ rotate_90
  tp 100 64 200
  tp ~ ~10 ~
  locate mountains
  locate desert 4096
  locate lava 1024 tp
  locate cave 100 2048
  locate mountains tp
  select pos1
  select pos2 100 64 200
  select clear
  template save my_house building decorated
  template load my_house
  template list
  template info my_house"#;

    CommandResult::Success(help_text.to_string())
}

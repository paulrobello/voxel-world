//! Console command implementations.
//!
//! Each command function takes arguments and returns a `CommandResult`.

mod boxme;
mod copy;
mod fill;
mod floodfill;
mod locate;
mod locate_async;
mod measure;
mod picture;
mod positions;
mod select;
mod sphere;
mod stencil;
mod template;
mod texture;
mod tp;

pub use boxme::boxme;
pub use copy::copy;
pub use fill::fill;
pub use floodfill::floodfill;
pub use locate::locate;
pub use locate_async::update_locate_search;
pub use measure::measure;
pub use picture::picture;
pub use positions::{delete_pos, list_pos, save_pos};
#[allow(unused_imports)] // TODO: Remove once integrated with main.rs
pub use select::select;
pub use sphere::sphere;
#[allow(unused_imports)] // TODO: Remove once integrated with main.rs
pub use stencil::stencil;
#[allow(unused_imports)] // TODO: Remove once integrated with main.rs
pub use template::template;
pub use texture::{texture_add, texture_info, texture_list, texture_remove};
pub use tp::tp;

use super::CommandResult;

/// Help command - list available commands.
pub fn help() -> CommandResult {
    let help_text = r#"Available commands:
  fill <block> <x1> <y1> <z1> <x2> <y2> <z2> [hollow]
    Fill a region with blocks. Use 'air' to clear.
    Coordinates support ~ for relative (e.g., ~5 = player + 5)
    'hollow' flag creates a shell with air inside

  floodfill <block> [x] [y] [z]
    Replace connected blocks of the same type with target block.
    If no coordinates, uses crosshair target.
    Smart matching: painted blocks match texture+tint, water matches type.
    Model blocks cannot be flood filled (prevents accidents).

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

  stencil create|load|list|delete|active|clear|opacity|mode|remove <args>
    Manage holographic building guides.
    create <name> [tags]: Create stencil from selection (non-air positions only)
    load <name>: Load stencil at crosshair position
    list: Show all saved stencils
    delete <name>: Delete a stencil
    active: Show active stencils with IDs
    clear: Remove all active stencils
    opacity <0.3-0.8>: Set global stencil opacity
    mode <wireframe|solid>: Set render mode
    remove <id>: Remove a specific active stencil

  frame picture list|set|clear|debug [id]
    Manage picture frame selection.
    list: Show all saved pictures with cluster recommendations
    set <id>: Select picture for frame placement
    clear: Deselect (place empty frames)
    debug: Show cluster picture size guide

  texture_add <filepath> <name>, texadd
    Add a custom texture from a PNG file (host only).
    The texture will be synced to all connected clients.
    Max file size: 1MB, recommended size: 64x64 pixels.

  texture_list, texlist, textures
    List all custom textures with their slot numbers.

  texture_remove <slot>, texremove, texdel
    Remove a custom texture by slot number (host only).

  texture_info <name|slot>
    Show detailed info about a specific texture.

  measure clear
    Clear all measurement markers from the world.

  save_pos <name>, savepos, sp
    Save your current position with a custom name.
    Overwrites if name already exists.

  delete_pos <name>, deletepos, delpos, dp
    Delete a saved position by name.

  list_pos, listpos, lp, positions
    List all saved positions for the current world.

  waterdebug, wd
    Show water/lava simulation debug info (includes profiling if enabled).

  waterforce, wf
    Force ALL water cells to become active (unstick water).

  wateranalyze, wa
    Analyze water flow at player position (debug).

  waterprofile, wp [on|off]
    Enable/disable water simulation performance profiling.
    Shows timing breakdown in waterdebug output.

  clear
    Clear console output

  help, ?
    Show this help message

Examples:
  fill stone 0 0 0 10 5 10
  fill air ~-5 ~ ~-5 ~5 ~10 ~5
  fill brick ~ ~-1 ~ ~10 ~3 ~10 hollow
  floodfill stone
  floodfill air ~ ~ ~
  floodfill cobblestone 100 64 200
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
  template info my_house
  save_pos home
  save_pos mining_base
  delete_pos old_base
  list_pos
  stencil create wall_guide building
  stencil load wall_guide
  stencil active
  stencil opacity 0.6
  stencil mode wireframe
  stencil remove 2
  stencil clear
  frame picture list
  frame picture set 1
  frame picture clear
  frame picture debug"#;

    CommandResult::Success(help_text.to_string())
}

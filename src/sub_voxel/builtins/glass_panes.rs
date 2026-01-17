//! Framed glass pane models.
//!
//! Glass panes are flat panels with decorative frames that connect to neighbors.
//! - Horizontal panes (XZ plane) for floors/ceilings
//! - Vertical panes (XY/YZ planes) for walls, rotatable for orientation
//!
//! Connection bitmask: N=1, S=2, E=4, W=8
//! - For horizontal: N=-Z, S=+Z, E=+X, W=-X
//! - For vertical: N=+Y (up), S=-Y (down), E/W=horizontal in pane plane

use super::basic::fill_scaled;
use crate::sub_voxel::{Color, LightBlocking, ModelResolution, SubVoxelModel};

/// Frame color (dark wood/metal)
const FRAME_COLOR: Color = Color::rgb(60, 50, 45);

/// Glass color (light blue tinted, matches door glass)
const GLASS_COLOR: Color = Color::rgba(200, 220, 255, 160);

/// Creates a horizontal glass pane (XZ plane) with the specified connections.
///
/// The pane lies flat on the XZ plane at Y=3 (middle of block, 1 voxel thick).
/// Frame edges are hidden when connected to neighbors, and glass extends to fill the gap.
///
/// # Arguments
/// * `connections` - Bitmask: N=1 (-Z), S=2 (+Z), E=4 (+X), W=8 (-X)
pub fn create_horizontal_glass_pane(connections: u8) -> SubVoxelModel {
    let name = format!("glass_pane_horizontal_{}", connections);
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, &name);

    model.palette[1] = FRAME_COLOR;
    model.palette[2] = GLASS_COLOR;

    // Glass bounds extend to edges when connected (to merge with adjacent pane's glass)
    let z_min = if connections & 1 != 0 { 0 } else { 1 }; // North connected
    let z_max = if connections & 2 != 0 { 7 } else { 6 }; // South connected
    let x_min = if connections & 8 != 0 { 0 } else { 1 }; // West connected
    let x_max = if connections & 4 != 0 { 7 } else { 6 }; // East connected

    // Fill glass (1 voxel thick at Y=3, extends to edge when connected)
    fill_scaled(&mut model, x_min, 3, z_min, x_max, 3, z_max, 2);

    // Frame edges (only if not connected, 1 voxel thick)
    // North edge (-Z, z=0)
    if connections & 1 == 0 {
        fill_scaled(&mut model, x_min, 3, 0, x_max, 3, 0, 1);
    }
    // South edge (+Z, z=7)
    if connections & 2 == 0 {
        fill_scaled(&mut model, x_min, 3, 7, x_max, 3, 7, 1);
    }
    // East edge (+X, x=7)
    if connections & 4 == 0 {
        fill_scaled(&mut model, 7, 3, z_min, 7, 3, z_max, 1);
    }
    // West edge (-X, x=0)
    if connections & 8 == 0 {
        fill_scaled(&mut model, 0, 3, z_min, 0, 3, z_max, 1);
    }

    // Corners (only if both adjacent edges are visible)
    // NW corner (x=0, z=0) - visible if N and W edges visible
    if connections & 1 == 0 && connections & 8 == 0 {
        fill_scaled(&mut model, 0, 3, 0, 0, 3, 0, 1);
    }
    // NE corner (x=7, z=0) - visible if N and E edges visible
    if connections & 1 == 0 && connections & 4 == 0 {
        fill_scaled(&mut model, 7, 3, 0, 7, 3, 0, 1);
    }
    // SW corner (x=0, z=7) - visible if S and W edges visible
    if connections & 2 == 0 && connections & 8 == 0 {
        fill_scaled(&mut model, 0, 3, 7, 0, 3, 7, 1);
    }
    // SE corner (x=7, z=7) - visible if S and E edges visible
    if connections & 2 == 0 && connections & 4 == 0 {
        fill_scaled(&mut model, 7, 3, 7, 7, 3, 7, 1);
    }

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = false; // Connections handle orientation
    model.requires_ground_support = false;
    model.compute_collision_mask();
    model
}

/// Creates a vertical glass pane (XY plane, facing +Z/-Z) with the specified connections.
///
/// The pane stands vertically on the XY plane at Z=3 (middle of block, 1 voxel thick).
/// Use rotation to orient to YZ plane.
///
/// # Arguments
/// * `connections` - Bitmask: N=1 (+Y up), S=2 (-Y down), E=4 (+X), W=8 (-X)
pub fn create_vertical_glass_pane(connections: u8) -> SubVoxelModel {
    let name = format!("glass_pane_vertical_{}", connections);
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, &name);

    model.palette[1] = FRAME_COLOR;
    model.palette[2] = GLASS_COLOR;

    // Glass bounds extend to edges when connected (to merge with adjacent pane's glass)
    let y_min = if connections & 2 != 0 { 0 } else { 1 }; // Bottom connected
    let y_max = if connections & 1 != 0 { 7 } else { 6 }; // Top connected
    let x_min = if connections & 8 != 0 { 0 } else { 1 }; // West connected
    let x_max = if connections & 4 != 0 { 7 } else { 6 }; // East connected

    // Fill glass (1 voxel thick at Z=3, extends to edge when connected)
    fill_scaled(&mut model, x_min, y_min, 3, x_max, y_max, 3, 2);

    // Frame edges (only if not connected, 1 voxel thick)
    // Top edge (+Y, y=7)
    if connections & 1 == 0 {
        fill_scaled(&mut model, x_min, 7, 3, x_max, 7, 3, 1);
    }
    // Bottom edge (-Y, y=0)
    if connections & 2 == 0 {
        fill_scaled(&mut model, x_min, 0, 3, x_max, 0, 3, 1);
    }
    // East edge (+X, x=7)
    if connections & 4 == 0 {
        fill_scaled(&mut model, 7, y_min, 3, 7, y_max, 3, 1);
    }
    // West edge (-X, x=0)
    if connections & 8 == 0 {
        fill_scaled(&mut model, 0, y_min, 3, 0, y_max, 3, 1);
    }

    // Corners (only if both adjacent edges are visible)
    // Top-West corner (x=0, y=7)
    if connections & 1 == 0 && connections & 8 == 0 {
        fill_scaled(&mut model, 0, 7, 3, 0, 7, 3, 1);
    }
    // Top-East corner (x=7, y=7)
    if connections & 1 == 0 && connections & 4 == 0 {
        fill_scaled(&mut model, 7, 7, 3, 7, 7, 3, 1);
    }
    // Bottom-West corner (x=0, y=0)
    if connections & 2 == 0 && connections & 8 == 0 {
        fill_scaled(&mut model, 0, 0, 3, 0, 0, 3, 1);
    }
    // Bottom-East corner (x=7, y=0)
    if connections & 2 == 0 && connections & 4 == 0 {
        fill_scaled(&mut model, 7, 0, 3, 7, 0, 3, 1);
    }

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true; // Rotate for XY vs YZ orientation
    model.requires_ground_support = false;
    model.compute_collision_mask();
    model
}

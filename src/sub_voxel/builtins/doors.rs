use super::basic::{fill_scaled, set_scaled};
use crate::sub_voxel::{Color, LightBlocking, ModelResolution, SubVoxelModel};

// ============================================================================
// DOORS
// ============================================================================

/// Creates a door lower half (closed, hinge on left when facing +Z).
/// Door is 2 voxels thick in Z, full width in X.
pub fn create_door_lower_closed_left() -> SubVoxelModel {
    let mut model =
        SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "door_lower_closed_left");

    model.palette[1] = Color::rgb(100, 70, 40); // Hinge (darker wood/metal)
    model.palette[2] = Color::rgb(139, 90, 43); // Wood
    model.palette[3] = Color::rgb(0, 0, 0); // Handle (black)

    // Hinge column at x=0 (1 voxel thick at z=0)
    fill_scaled(&mut model, 7, 0, 0, 7, 7, 0, 1);
    // Wood body at x=1-7 (1 voxel thick at z=0)
    fill_scaled(&mut model, 0, 0, 0, 6, 7, 0, 2);
    // Handle at right edge, top of lower door
    set_scaled(&mut model, 0, 7, 0, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a door lower half (closed, hinge on right when facing +Z).
pub fn create_door_lower_closed_right() -> SubVoxelModel {
    let mut model =
        SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "door_lower_closed_right");

    model.palette[1] = Color::rgb(100, 70, 40); // Hinge (darker wood/metal)
    model.palette[2] = Color::rgb(139, 90, 43); // Wood
    model.palette[3] = Color::rgb(0, 0, 0); // Handle (black)

    // Hinge column at x=7 (1 voxel thick at z=0)
    fill_scaled(&mut model, 0, 0, 0, 0, 7, 0, 1);
    // Wood body at x=0-6 (1 voxel thick at z=0)
    fill_scaled(&mut model, 1, 0, 0, 7, 7, 0, 2);
    // Handle at left edge, top of lower door
    set_scaled(&mut model, 7, 7, 0, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a door upper half (closed, hinge on left).
pub fn create_door_upper_closed_left() -> SubVoxelModel {
    let mut model =
        SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "door_upper_closed_left");

    model.palette[1] = Color::rgb(100, 70, 40); // Hinge (darker wood/metal)
    model.palette[2] = Color::rgb(139, 90, 43); // Wood
    model.palette[3] = Color::rgb(0, 0, 0); // Handle (black)

    // Hinge column at x=0 (1 voxel thick at z=0)
    fill_scaled(&mut model, 7, 0, 0, 7, 7, 0, 1);
    // Wood body at x=1-7 (1 voxel thick at z=0)
    fill_scaled(&mut model, 0, 0, 0, 6, 7, 0, 2);
    // Handle at right edge, bottom of upper door
    set_scaled(&mut model, 0, 0, 0, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a door upper half (closed, hinge on right).
pub fn create_door_upper_closed_right() -> SubVoxelModel {
    let mut model =
        SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "door_upper_closed_right");

    model.palette[1] = Color::rgb(100, 70, 40); // Hinge (darker wood/metal)
    model.palette[2] = Color::rgb(139, 90, 43); // Wood
    model.palette[3] = Color::rgb(0, 0, 0); // Handle (black)

    // Hinge column at x=7 (1 voxel thick at z=0)
    fill_scaled(&mut model, 0, 0, 0, 0, 7, 0, 1);
    // Wood body at x=0-6 (1 voxel thick at z=0)
    fill_scaled(&mut model, 1, 0, 0, 7, 7, 0, 2);
    // Handle at left edge, bottom of upper door
    set_scaled(&mut model, 7, 0, 0, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a door lower half (open, hinge on left - door swings to +Z side).
pub fn create_door_lower_open_left() -> SubVoxelModel {
    let mut model =
        SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "door_lower_open_left");

    model.palette[1] = Color::rgb(100, 70, 40); // Hinge (darker wood/metal)
    model.palette[2] = Color::rgb(139, 90, 43); // Wood
    model.palette[3] = Color::rgb(0, 0, 0); // Handle (black)

    // Hinge at x=0, z=0 (pivot point - 1 voxel)
    fill_scaled(&mut model, 7, 0, 0, 7, 7, 0, 1);
    // Wood body at x=0, z=1-7 (1 voxel thick in x)
    fill_scaled(&mut model, 7, 0, 1, 7, 7, 7, 2);
    // Handle at swung end (x=0, z=7, top of lower door)
    set_scaled(&mut model, 7, 7, 7, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a door lower half (open, hinge on right - door swings to +Z side).
pub fn create_door_lower_open_right() -> SubVoxelModel {
    let mut model =
        SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "door_lower_open_right");

    model.palette[1] = Color::rgb(100, 70, 40); // Hinge (darker wood/metal)
    model.palette[2] = Color::rgb(139, 90, 43); // Wood
    model.palette[3] = Color::rgb(0, 0, 0); // Handle (black)

    // Hinge at x=7, z=0 (pivot point - 1 voxel)
    fill_scaled(&mut model, 0, 0, 0, 0, 7, 0, 1);
    // Wood body at x=7, z=1-7 (1 voxel thick in x)
    fill_scaled(&mut model, 0, 0, 1, 0, 7, 7, 2);
    // Handle at swung end (x=7, z=7, top of lower door)
    set_scaled(&mut model, 0, 7, 7, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a door upper half (open, hinge on left).
pub fn create_door_upper_open_left() -> SubVoxelModel {
    let mut model =
        SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "door_upper_open_left");

    model.palette[1] = Color::rgb(100, 70, 40); // Hinge (darker wood/metal)
    model.palette[2] = Color::rgb(139, 90, 43); // Wood
    model.palette[3] = Color::rgb(0, 0, 0); // Handle (black)

    // Hinge at x=0, z=0 (pivot point - 1 voxel)
    fill_scaled(&mut model, 7, 0, 0, 7, 7, 0, 1);
    // Wood body at x=0, z=1-7 (1 voxel thick in x)
    fill_scaled(&mut model, 7, 0, 1, 7, 7, 7, 2);
    // Handle at swung end (x=0, z=7, bottom of upper door)
    set_scaled(&mut model, 7, 0, 7, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a door upper half (open, hinge on right).
pub fn create_door_upper_open_right() -> SubVoxelModel {
    let mut model =
        SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "door_upper_open_right");

    model.palette[1] = Color::rgb(100, 70, 40); // Hinge (darker wood/metal)
    model.palette[2] = Color::rgb(139, 90, 43); // Wood
    model.palette[3] = Color::rgb(0, 0, 0); // Handle (black)

    // Hinge at x=7, z=0 (pivot point - 1 voxel)
    fill_scaled(&mut model, 0, 0, 0, 0, 7, 0, 1);
    // Wood body at x=7, z=1-7 (1 voxel thick in x)
    fill_scaled(&mut model, 0, 0, 1, 0, 7, 7, 2);
    // Handle at swung end (x=7, z=7, bottom of upper door)
    set_scaled(&mut model, 0, 0, 7, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

// ============================================================================
// TRAPDOORS
// ============================================================================

/// Creates a trapdoor (closed, attached to floor).
/// Fills bottom 1 voxel of the block.
pub fn create_trapdoor_floor_closed() -> SubVoxelModel {
    let mut model =
        SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "trapdoor_floor_closed");

    model.palette[1] = Color::rgb(139, 90, 43); // Wood brown
    model.palette[2] = Color::rgb(160, 110, 60); // Lighter brown
    model.palette[3] = Color::rgb(60, 60, 65); // Iron

    // Main panel (1 voxel thick at y=0)
    fill_scaled(&mut model, 0, 0, 0, 7, 0, 7, 1);
    // Inner panel detail
    fill_scaled(&mut model, 1, 0, 1, 6, 0, 6, 2);
    // Handle
    set_scaled(&mut model, 3, 1, 3, 3);
    set_scaled(&mut model, 4, 1, 3, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a trapdoor (closed, attached to ceiling).
/// Fills top 1 voxel of the block.
pub fn create_trapdoor_ceiling_closed() -> SubVoxelModel {
    let mut model =
        SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "trapdoor_ceiling_closed");

    model.palette[1] = Color::rgb(139, 90, 43);
    model.palette[2] = Color::rgb(160, 110, 60);
    model.palette[3] = Color::rgb(60, 60, 65);

    // Main panel (1 voxel thick at y=7)
    fill_scaled(&mut model, 0, 7, 0, 7, 7, 7, 1);
    fill_scaled(&mut model, 1, 7, 1, 6, 7, 6, 2);
    set_scaled(&mut model, 3, 6, 3, 3);
    set_scaled(&mut model, 4, 6, 3, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a trapdoor (open, hinged at -Z, panel now vertical).
pub fn create_trapdoor_floor_open() -> SubVoxelModel {
    let mut model =
        SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "trapdoor_floor_open");

    model.palette[1] = Color::rgb(139, 90, 43);
    model.palette[2] = Color::rgb(160, 110, 60);
    model.palette[3] = Color::rgb(60, 60, 65);

    // Vertical panel at z=0 (1 voxel thick, hinged at near edge)
    fill_scaled(&mut model, 0, 0, 0, 7, 7, 0, 1);
    fill_scaled(&mut model, 1, 1, 0, 6, 6, 0, 2);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a trapdoor (open, hinged at -Z from ceiling, panel now vertical).
pub fn create_trapdoor_ceiling_open() -> SubVoxelModel {
    let mut model =
        SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "trapdoor_ceiling_open");

    model.palette[1] = Color::rgb(139, 90, 43);
    model.palette[2] = Color::rgb(160, 110, 60);
    model.palette[3] = Color::rgb(60, 60, 65);

    // Vertical panel at z=0 (1 voxel thick, hinged at near edge)
    fill_scaled(&mut model, 0, 0, 0, 7, 7, 0, 1);
    fill_scaled(&mut model, 1, 1, 0, 6, 6, 0, 2);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

// ============================================================================
// WINDOWS (Glass Panes)
// ============================================================================

/// Creates a glass pane window with the specified connection mask.
/// Connection bitmask: N=1, S=2, E=4, W=8 (same as fences).
pub fn create_window(connections: u8) -> SubVoxelModel {
    let name = format!("window_{}", connections);
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, &name);

    model.palette[1] = Color::rgb(80, 80, 85); // Frame (dark gray)
    model.palette[2] = Color::rgba(180, 210, 255, 160); // Glass (light blue tinted)

    // Center post (only if no connections, or if multiple directions)
    let has_ns = (connections & 0x03) != 0;
    let has_ew = (connections & 0x0C) != 0;

    if connections == 0 || (has_ns && has_ew) {
        // Center post for isolated pane or cross
        fill_scaled(&mut model, 3, 0, 3, 4, 7, 4, 1);
    }

    // Glass panes based on connections (thin, 1 voxel thick)
    // North (-Z)
    if connections & 1 != 0 {
        fill_scaled(&mut model, 3, 0, 0, 4, 7, 3, 1); // Frame edges
        fill_scaled(&mut model, 3, 1, 1, 4, 6, 2, 2); // Glass
    }
    // South (+Z)
    if connections & 2 != 0 {
        fill_scaled(&mut model, 3, 0, 4, 4, 7, 7, 1);
        fill_scaled(&mut model, 3, 1, 5, 4, 6, 6, 2);
    }
    // East (+X)
    if connections & 4 != 0 {
        fill_scaled(&mut model, 4, 0, 3, 7, 7, 4, 1);
        fill_scaled(&mut model, 5, 1, 3, 6, 6, 4, 2);
    }
    // West (-X)
    if connections & 8 != 0 {
        fill_scaled(&mut model, 0, 0, 3, 3, 7, 4, 1);
        fill_scaled(&mut model, 1, 1, 3, 2, 6, 4, 2);
    }

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = false;
    model.requires_ground_support = false;
    model.compute_collision_mask();
    model
}

// ============================================================================
// WINDOWED DOORS (IDs 67-74)
// ============================================================================

/// Creates a windowed door lower half (closed, hinge on left).
pub fn create_windowed_door_lower_closed_left() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "windowed_door_lower_closed_left",
    );

    model.palette[1] = Color::rgb(100, 70, 40); // Hinge
    model.palette[2] = Color::rgb(139, 90, 43); // Wood
    model.palette[3] = Color::rgb(0, 0, 0); // Handle
    model.palette[4] = Color::rgb(90, 70, 50); // Darker wood panels

    // Hinge column at x=0
    fill_scaled(&mut model, 7, 0, 0, 7, 7, 0, 1);
    // Wood body at x=1-7
    fill_scaled(&mut model, 0, 0, 0, 6, 7, 0, 2);
    // Decorative panels (lower half)
    fill_scaled(&mut model, 4, 1, 0, 5, 2, 0, 4);
    fill_scaled(&mut model, 1, 1, 0, 2, 2, 0, 4);
    fill_scaled(&mut model, 4, 4, 0, 5, 5, 0, 4);
    fill_scaled(&mut model, 1, 4, 0, 2, 5, 0, 4);
    // Handle
    set_scaled(&mut model, 0, 7, 0, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a windowed door lower half (closed, hinge on right).
pub fn create_windowed_door_lower_closed_right() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "windowed_door_lower_closed_right",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgb(90, 70, 50);

    fill_scaled(&mut model, 0, 0, 0, 0, 7, 0, 1);
    fill_scaled(&mut model, 1, 0, 0, 7, 7, 0, 2);
    fill_scaled(&mut model, 5, 1, 0, 6, 2, 0, 4);
    fill_scaled(&mut model, 2, 1, 0, 3, 2, 0, 4);
    fill_scaled(&mut model, 5, 4, 0, 6, 5, 0, 4);
    fill_scaled(&mut model, 2, 4, 0, 3, 5, 0, 4);
    set_scaled(&mut model, 7, 7, 0, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a windowed door upper half (closed, hinge on left).
pub fn create_windowed_door_upper_closed_left() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "windowed_door_upper_closed_left",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgba(200, 220, 255, 160); // Glass

    // Hinge column at x=0
    fill_scaled(&mut model, 7, 0, 0, 7, 7, 0, 1);
    // Wood body at x=1-7
    fill_scaled(&mut model, 0, 0, 0, 6, 7, 0, 2);
    // Glass windows (upper half)
    fill_scaled(&mut model, 4, 3, 0, 5, 5, 0, 4);
    fill_scaled(&mut model, 1, 3, 0, 2, 5, 0, 4);
    // Handle
    set_scaled(&mut model, 0, 0, 0, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a windowed door upper half (closed, hinge on right).
pub fn create_windowed_door_upper_closed_right() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "windowed_door_upper_closed_right",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgba(200, 220, 255, 160);

    fill_scaled(&mut model, 0, 0, 0, 0, 7, 0, 1);
    fill_scaled(&mut model, 1, 0, 0, 7, 7, 0, 2);
    fill_scaled(&mut model, 5, 3, 0, 6, 5, 0, 4);
    fill_scaled(&mut model, 2, 3, 0, 3, 5, 0, 4);
    set_scaled(&mut model, 7, 0, 0, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a windowed door lower half (open, hinge on left).
pub fn create_windowed_door_lower_open_left() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "windowed_door_lower_open_left",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgb(90, 70, 50);

    fill_scaled(&mut model, 7, 0, 0, 7, 7, 0, 1);
    fill_scaled(&mut model, 7, 0, 1, 7, 7, 7, 2);
    fill_scaled(&mut model, 7, 1, 1, 7, 2, 2, 4);
    fill_scaled(&mut model, 7, 1, 4, 7, 2, 5, 4);
    fill_scaled(&mut model, 7, 4, 1, 7, 5, 2, 4);
    fill_scaled(&mut model, 7, 4, 4, 7, 5, 5, 4);
    set_scaled(&mut model, 7, 7, 7, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a windowed door lower half (open, hinge on right).
pub fn create_windowed_door_lower_open_right() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "windowed_door_lower_open_right",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgb(90, 70, 50);

    fill_scaled(&mut model, 0, 0, 0, 0, 7, 0, 1);
    fill_scaled(&mut model, 0, 0, 1, 0, 7, 7, 2);
    fill_scaled(&mut model, 0, 1, 1, 0, 2, 2, 4);
    fill_scaled(&mut model, 0, 1, 4, 0, 2, 5, 4);
    fill_scaled(&mut model, 0, 4, 1, 0, 5, 2, 4);
    fill_scaled(&mut model, 0, 4, 4, 0, 5, 5, 4);
    set_scaled(&mut model, 0, 7, 7, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a windowed door upper half (open, hinge on left).
pub fn create_windowed_door_upper_open_left() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "windowed_door_upper_open_left",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgba(200, 220, 255, 160);

    fill_scaled(&mut model, 7, 0, 0, 7, 7, 0, 1);
    fill_scaled(&mut model, 7, 0, 1, 7, 7, 7, 2);
    fill_scaled(&mut model, 7, 3, 1, 7, 5, 2, 4);
    fill_scaled(&mut model, 7, 3, 4, 7, 5, 5, 4);
    set_scaled(&mut model, 7, 0, 7, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a windowed door upper half (open, hinge on right).
pub fn create_windowed_door_upper_open_right() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "windowed_door_upper_open_right",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgba(200, 220, 255, 160);

    fill_scaled(&mut model, 0, 0, 0, 0, 7, 0, 1);
    fill_scaled(&mut model, 0, 0, 1, 0, 7, 7, 2);
    fill_scaled(&mut model, 0, 3, 1, 0, 5, 2, 4);
    fill_scaled(&mut model, 0, 3, 4, 0, 5, 5, 4);
    set_scaled(&mut model, 0, 0, 7, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

// ============================================================================
// PANELED DOORS (no windows, decorative panels)
// ============================================================================

/// Creates a paneled door lower half (closed, hinge on left).
pub fn create_paneled_door_lower_closed_left() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "paneled_door_lower_closed_left",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgb(110, 75, 35); // Panel detail

    fill_scaled(&mut model, 7, 0, 0, 7, 7, 0, 1);
    fill_scaled(&mut model, 0, 0, 0, 6, 7, 0, 2);
    // Decorative panels (lower door)
    fill_scaled(&mut model, 4, 1, 0, 5, 3, 0, 4);
    fill_scaled(&mut model, 1, 1, 0, 2, 3, 0, 4);
    fill_scaled(&mut model, 4, 4, 0, 5, 6, 0, 4);
    fill_scaled(&mut model, 1, 4, 0, 2, 6, 0, 4);
    set_scaled(&mut model, 0, 7, 0, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a paneled door lower half (closed, hinge on right).
pub fn create_paneled_door_lower_closed_right() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "paneled_door_lower_closed_right",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgb(110, 75, 35);

    fill_scaled(&mut model, 0, 0, 0, 0, 7, 0, 1);
    fill_scaled(&mut model, 1, 0, 0, 7, 7, 0, 2);
    fill_scaled(&mut model, 5, 1, 0, 6, 3, 0, 4);
    fill_scaled(&mut model, 2, 1, 0, 3, 3, 0, 4);
    fill_scaled(&mut model, 5, 4, 0, 6, 6, 0, 4);
    fill_scaled(&mut model, 2, 4, 0, 3, 6, 0, 4);
    set_scaled(&mut model, 7, 7, 0, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a paneled door upper half (closed, hinge on left).
pub fn create_paneled_door_upper_closed_left() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "paneled_door_upper_closed_left",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgb(110, 75, 35);

    fill_scaled(&mut model, 7, 0, 0, 7, 7, 0, 1);
    fill_scaled(&mut model, 0, 0, 0, 6, 7, 0, 2);
    // Decorative panels (upper door - solid wood, no glass)
    fill_scaled(&mut model, 4, 1, 0, 5, 3, 0, 4);
    fill_scaled(&mut model, 1, 1, 0, 2, 3, 0, 4);
    fill_scaled(&mut model, 4, 4, 0, 5, 6, 0, 4);
    fill_scaled(&mut model, 1, 4, 0, 2, 6, 0, 4);
    set_scaled(&mut model, 0, 0, 0, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a paneled door upper half (closed, hinge on right).
pub fn create_paneled_door_upper_closed_right() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "paneled_door_upper_closed_right",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgb(110, 75, 35);

    fill_scaled(&mut model, 0, 0, 0, 0, 7, 0, 1);
    fill_scaled(&mut model, 1, 0, 0, 7, 7, 0, 2);
    fill_scaled(&mut model, 5, 1, 0, 6, 3, 0, 4);
    fill_scaled(&mut model, 2, 1, 0, 3, 3, 0, 4);
    fill_scaled(&mut model, 5, 4, 0, 6, 6, 0, 4);
    fill_scaled(&mut model, 2, 4, 0, 3, 6, 0, 4);
    set_scaled(&mut model, 7, 0, 0, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a paneled door lower half (open, hinge on left).
pub fn create_paneled_door_lower_open_left() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "paneled_door_lower_open_left",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgb(110, 75, 35);

    fill_scaled(&mut model, 7, 0, 0, 7, 7, 0, 1);
    fill_scaled(&mut model, 7, 0, 1, 7, 7, 7, 2);
    fill_scaled(&mut model, 7, 1, 1, 7, 3, 2, 4);
    fill_scaled(&mut model, 7, 1, 4, 7, 3, 5, 4);
    fill_scaled(&mut model, 7, 4, 1, 7, 6, 2, 4);
    fill_scaled(&mut model, 7, 4, 4, 7, 6, 5, 4);
    set_scaled(&mut model, 7, 7, 7, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a paneled door lower half (open, hinge on right).
pub fn create_paneled_door_lower_open_right() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "paneled_door_lower_open_right",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgb(110, 75, 35);

    fill_scaled(&mut model, 0, 0, 0, 0, 7, 0, 1);
    fill_scaled(&mut model, 0, 0, 1, 0, 7, 7, 2);
    fill_scaled(&mut model, 0, 1, 1, 0, 3, 2, 4);
    fill_scaled(&mut model, 0, 1, 4, 0, 3, 5, 4);
    fill_scaled(&mut model, 0, 4, 1, 0, 6, 2, 4);
    fill_scaled(&mut model, 0, 4, 4, 0, 6, 5, 4);
    set_scaled(&mut model, 0, 7, 7, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a paneled door upper half (open, hinge on left).
pub fn create_paneled_door_upper_open_left() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "paneled_door_upper_open_left",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgb(110, 75, 35);

    fill_scaled(&mut model, 7, 0, 0, 7, 7, 0, 1);
    fill_scaled(&mut model, 7, 0, 1, 7, 7, 7, 2);
    fill_scaled(&mut model, 7, 1, 1, 7, 3, 2, 4);
    fill_scaled(&mut model, 7, 1, 4, 7, 3, 5, 4);
    fill_scaled(&mut model, 7, 4, 1, 7, 6, 2, 4);
    fill_scaled(&mut model, 7, 4, 4, 7, 6, 5, 4);
    set_scaled(&mut model, 7, 0, 7, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a paneled door upper half (open, hinge on right).
pub fn create_paneled_door_upper_open_right() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "paneled_door_upper_open_right",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgb(110, 75, 35);

    fill_scaled(&mut model, 0, 0, 0, 0, 7, 0, 1);
    fill_scaled(&mut model, 0, 0, 1, 0, 7, 7, 2);
    fill_scaled(&mut model, 0, 1, 1, 0, 3, 2, 4);
    fill_scaled(&mut model, 0, 1, 4, 0, 3, 5, 4);
    fill_scaled(&mut model, 0, 4, 1, 0, 6, 2, 4);
    fill_scaled(&mut model, 0, 4, 4, 0, 6, 5, 4);
    set_scaled(&mut model, 0, 0, 7, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

// ============================================================================
// WINDOWED+PANELED DOORS (decorative panels on lower, glass on upper)
// ============================================================================

/// Creates a windowed+paneled door lower half (closed, hinge on left).
pub fn create_fancy_door_lower_closed_left() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "fancy_door_lower_closed_left",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgb(110, 75, 35); // Panel detail

    fill_scaled(&mut model, 7, 0, 0, 7, 7, 0, 1);
    fill_scaled(&mut model, 0, 0, 0, 6, 7, 0, 2);
    fill_scaled(&mut model, 4, 1, 0, 5, 3, 0, 4);
    fill_scaled(&mut model, 1, 1, 0, 2, 3, 0, 4);
    fill_scaled(&mut model, 4, 4, 0, 5, 6, 0, 4);
    fill_scaled(&mut model, 1, 4, 0, 2, 6, 0, 4);
    set_scaled(&mut model, 0, 7, 0, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a windowed+paneled door lower half (closed, hinge on right).
pub fn create_fancy_door_lower_closed_right() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "fancy_door_lower_closed_right",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgb(110, 75, 35);

    fill_scaled(&mut model, 0, 0, 0, 0, 7, 0, 1);
    fill_scaled(&mut model, 1, 0, 0, 7, 7, 0, 2);
    fill_scaled(&mut model, 5, 1, 0, 6, 2, 0, 4);
    fill_scaled(&mut model, 2, 1, 0, 3, 2, 0, 4);
    fill_scaled(&mut model, 5, 4, 0, 6, 5, 0, 4);
    fill_scaled(&mut model, 2, 4, 0, 3, 5, 0, 4);
    set_scaled(&mut model, 7, 7, 0, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a windowed+paneled door upper half (closed, hinge on left).
pub fn create_fancy_door_upper_closed_left() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "fancy_door_upper_closed_left",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgba(200, 220, 255, 160); // Glass

    fill_scaled(&mut model, 7, 0, 0, 7, 7, 0, 1);
    fill_scaled(&mut model, 0, 0, 0, 6, 7, 0, 2);
    // Glass windows (upper half)
    fill_scaled(&mut model, 4, 3, 0, 5, 5, 0, 4);
    fill_scaled(&mut model, 1, 3, 0, 2, 5, 0, 4);
    set_scaled(&mut model, 0, 0, 0, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a windowed+paneled door upper half (closed, hinge on right).
pub fn create_fancy_door_upper_closed_right() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "fancy_door_upper_closed_right",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgba(200, 220, 255, 160);

    fill_scaled(&mut model, 0, 0, 0, 0, 7, 0, 1);
    fill_scaled(&mut model, 1, 0, 0, 7, 7, 0, 2);
    fill_scaled(&mut model, 5, 3, 0, 6, 5, 0, 4);
    fill_scaled(&mut model, 2, 3, 0, 3, 5, 0, 4);
    set_scaled(&mut model, 7, 0, 0, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a windowed+paneled door lower half (open, hinge on left).
pub fn create_fancy_door_lower_open_left() -> SubVoxelModel {
    let mut model =
        SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "fancy_door_lower_open_left");

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgb(110, 75, 35);

    fill_scaled(&mut model, 7, 0, 0, 7, 7, 0, 1);
    fill_scaled(&mut model, 7, 0, 1, 7, 7, 7, 2);
    fill_scaled(&mut model, 7, 1, 1, 7, 3, 2, 4);
    fill_scaled(&mut model, 7, 1, 4, 7, 3, 5, 4);
    fill_scaled(&mut model, 7, 4, 1, 7, 6, 2, 4);
    fill_scaled(&mut model, 7, 4, 4, 7, 6, 5, 4);
    set_scaled(&mut model, 7, 7, 7, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a windowed+paneled door lower half (open, hinge on right).
pub fn create_fancy_door_lower_open_right() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "fancy_door_lower_open_right",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgb(110, 75, 35);

    fill_scaled(&mut model, 0, 0, 0, 0, 7, 0, 1);
    fill_scaled(&mut model, 0, 0, 1, 0, 7, 7, 2);
    fill_scaled(&mut model, 0, 1, 1, 0, 3, 2, 4);
    fill_scaled(&mut model, 0, 1, 4, 0, 3, 5, 4);
    fill_scaled(&mut model, 0, 4, 1, 0, 6, 2, 4);
    fill_scaled(&mut model, 0, 4, 4, 0, 6, 5, 4);
    set_scaled(&mut model, 0, 7, 7, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a windowed+paneled door upper half (open, hinge on left).
pub fn create_fancy_door_upper_open_left() -> SubVoxelModel {
    let mut model =
        SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "fancy_door_upper_open_left");

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgba(200, 220, 255, 160);

    fill_scaled(&mut model, 7, 0, 0, 7, 7, 0, 1);
    fill_scaled(&mut model, 7, 0, 1, 7, 7, 7, 2);
    fill_scaled(&mut model, 7, 3, 1, 7, 5, 2, 4);
    fill_scaled(&mut model, 7, 3, 4, 7, 5, 5, 4);
    set_scaled(&mut model, 7, 0, 7, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a windowed+paneled door upper half (open, hinge on right).
pub fn create_fancy_door_upper_open_right() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "fancy_door_upper_open_right",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgba(200, 220, 255, 160);

    fill_scaled(&mut model, 0, 0, 0, 0, 7, 0, 1);
    fill_scaled(&mut model, 0, 0, 1, 0, 7, 7, 2);
    fill_scaled(&mut model, 0, 3, 1, 0, 5, 2, 4);
    fill_scaled(&mut model, 0, 3, 4, 0, 5, 5, 4);
    set_scaled(&mut model, 0, 0, 7, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

// ============================================================================
// FULL GLASS DOORS (mostly glass with wood frame)
// ============================================================================

/// Creates a full glass door lower half (closed, hinge on left).
pub fn create_glass_door_lower_closed_left() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "glass_door_lower_closed_left",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgba(200, 220, 255, 160); // Glass

    fill_scaled(&mut model, 7, 0, 0, 7, 7, 0, 1);
    // Wood frame edges
    fill_scaled(&mut model, 0, 0, 0, 6, 0, 0, 2);
    fill_scaled(&mut model, 0, 7, 0, 6, 7, 0, 2);
    fill_scaled(&mut model, 6, 1, 0, 6, 6, 0, 2);
    fill_scaled(&mut model, 0, 1, 0, 0, 6, 0, 2);
    // Glass center
    fill_scaled(&mut model, 1, 1, 0, 5, 6, 0, 4);
    set_scaled(&mut model, 0, 7, 0, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a full glass door lower half (closed, hinge on right).
pub fn create_glass_door_lower_closed_right() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "glass_door_lower_closed_right",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgba(200, 220, 255, 160);

    fill_scaled(&mut model, 0, 0, 0, 0, 7, 0, 1);
    fill_scaled(&mut model, 1, 0, 0, 7, 0, 0, 2);
    fill_scaled(&mut model, 1, 7, 0, 7, 7, 0, 2);
    fill_scaled(&mut model, 7, 1, 0, 7, 6, 0, 2);
    fill_scaled(&mut model, 1, 1, 0, 1, 6, 0, 2);
    fill_scaled(&mut model, 2, 1, 0, 6, 6, 0, 4);
    set_scaled(&mut model, 7, 7, 0, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a full glass door upper half (closed, hinge on left).
pub fn create_glass_door_upper_closed_left() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "glass_door_upper_closed_left",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgba(200, 220, 255, 160);

    fill_scaled(&mut model, 7, 0, 0, 7, 7, 0, 1);
    fill_scaled(&mut model, 0, 0, 0, 6, 0, 0, 2);
    fill_scaled(&mut model, 0, 7, 0, 6, 7, 0, 2);
    fill_scaled(&mut model, 6, 1, 0, 6, 6, 0, 2);
    fill_scaled(&mut model, 0, 1, 0, 0, 6, 0, 2);
    fill_scaled(&mut model, 1, 1, 0, 5, 6, 0, 4);
    set_scaled(&mut model, 0, 0, 0, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a full glass door upper half (closed, hinge on right).
pub fn create_glass_door_upper_closed_right() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "glass_door_upper_closed_right",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgba(200, 220, 255, 160);

    fill_scaled(&mut model, 0, 0, 0, 0, 7, 0, 1);
    fill_scaled(&mut model, 1, 0, 0, 7, 0, 0, 2);
    fill_scaled(&mut model, 1, 7, 0, 7, 7, 0, 2);
    fill_scaled(&mut model, 7, 1, 0, 7, 6, 0, 2);
    fill_scaled(&mut model, 1, 1, 0, 1, 6, 0, 2);
    fill_scaled(&mut model, 2, 1, 0, 6, 6, 0, 4);
    set_scaled(&mut model, 7, 0, 0, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a full glass door lower half (open, hinge on left).
pub fn create_glass_door_lower_open_left() -> SubVoxelModel {
    let mut model =
        SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "glass_door_lower_open_left");

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgba(200, 220, 255, 160);

    fill_scaled(&mut model, 7, 0, 0, 7, 7, 0, 1);
    fill_scaled(&mut model, 7, 0, 1, 7, 0, 7, 2);
    fill_scaled(&mut model, 7, 7, 1, 7, 7, 7, 2);
    fill_scaled(&mut model, 7, 1, 1, 7, 6, 1, 2);
    fill_scaled(&mut model, 7, 1, 7, 7, 6, 7, 2);
    fill_scaled(&mut model, 7, 1, 2, 7, 6, 6, 4);
    set_scaled(&mut model, 7, 7, 7, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a full glass door lower half (open, hinge on right).
pub fn create_glass_door_lower_open_right() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "glass_door_lower_open_right",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgba(200, 220, 255, 160);

    fill_scaled(&mut model, 0, 0, 0, 0, 7, 0, 1);
    fill_scaled(&mut model, 0, 0, 1, 0, 0, 7, 2);
    fill_scaled(&mut model, 0, 7, 1, 0, 7, 7, 2);
    fill_scaled(&mut model, 0, 1, 1, 0, 6, 1, 2);
    fill_scaled(&mut model, 0, 1, 7, 0, 6, 7, 2);
    fill_scaled(&mut model, 0, 1, 2, 0, 6, 6, 4);
    set_scaled(&mut model, 0, 7, 7, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a full glass door upper half (open, hinge on left).
pub fn create_glass_door_upper_open_left() -> SubVoxelModel {
    let mut model =
        SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "glass_door_upper_open_left");

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgba(200, 220, 255, 160);

    fill_scaled(&mut model, 7, 0, 0, 7, 7, 0, 1);
    fill_scaled(&mut model, 7, 0, 1, 7, 0, 7, 2);
    fill_scaled(&mut model, 7, 7, 1, 7, 7, 7, 2);
    fill_scaled(&mut model, 7, 1, 1, 7, 6, 1, 2);
    fill_scaled(&mut model, 7, 1, 7, 7, 6, 7, 2);
    fill_scaled(&mut model, 7, 1, 2, 7, 6, 6, 4);
    set_scaled(&mut model, 7, 0, 7, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a full glass door upper half (open, hinge on right).
pub fn create_glass_door_upper_open_right() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(
        ModelResolution::Low,
        "glass_door_upper_open_right",
    );

    model.palette[1] = Color::rgb(100, 70, 40);
    model.palette[2] = Color::rgb(139, 90, 43);
    model.palette[3] = Color::rgb(0, 0, 0);
    model.palette[4] = Color::rgba(200, 220, 255, 160);

    fill_scaled(&mut model, 0, 0, 0, 0, 7, 0, 1);
    fill_scaled(&mut model, 0, 0, 1, 0, 0, 7, 2);
    fill_scaled(&mut model, 0, 7, 1, 0, 7, 7, 2);
    fill_scaled(&mut model, 0, 1, 1, 0, 6, 1, 2);
    fill_scaled(&mut model, 0, 1, 7, 0, 6, 7, 2);
    fill_scaled(&mut model, 0, 1, 2, 0, 6, 6, 4);
    set_scaled(&mut model, 0, 0, 7, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

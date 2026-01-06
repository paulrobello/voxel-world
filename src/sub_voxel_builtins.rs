use crate::sub_voxel::{Color, LightBlocking, ModelResolution, SubVoxelModel};

/// The design space size (8³) - models are designed in this space.
/// All built-in models use Low (8³) resolution for optimal performance.
const DESIGN_SIZE: usize = 8;

/// Scale factor - now 1 since we render at native 8³ resolution.
const SCALE: usize = 1;

/// Places a scaled voxel. In 8³ mode (SCALE=1), places a single voxel.
/// In 16³ mode (SCALE=2), places a 2×2×2 block at scaled coordinates.
fn set_scaled(model: &mut SubVoxelModel, x: usize, y: usize, z: usize, v: u8) {
    let sx = x * SCALE;
    let sy = y * SCALE;
    let sz = z * SCALE;
    for dx in 0..SCALE {
        for dy in 0..SCALE {
            for dz in 0..SCALE {
                model.set_voxel(sx + dx, sy + dy, sz + dz, v);
            }
        }
    }
}

/// Fills a scaled box. Coordinates are in 8³ space and get scaled up.
/// Note: The max coordinates are inclusive (as in fill_box), and scaling
/// is applied to make the box proportionally larger.
#[allow(clippy::too_many_arguments)]
fn fill_scaled(
    model: &mut SubVoxelModel,
    x0: usize,
    y0: usize,
    z0: usize,
    x1: usize,
    y1: usize,
    z1: usize,
    v: u8,
) {
    // Scale and adjust for inclusive bounds: (x1+1)*SCALE - 1 = scaled inclusive max
    let sx0 = x0 * SCALE;
    let sy0 = y0 * SCALE;
    let sz0 = z0 * SCALE;
    let sx1 = (x1 + 1) * SCALE - 1;
    let sy1 = (y1 + 1) * SCALE - 1;
    let sz1 = (z1 + 1) * SCALE - 1;
    model.fill_box(sx0, sy0, sz0, sx1, sy1, sz1, v);
}

/// Creates an inverted (flipped on Y) copy of a model with a new name.
fn inverted_copy(base: &SubVoxelModel, name: &str) -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(base.resolution, name);
    model.palette = base.palette;

    let size = base.resolution.size();
    for x in 0..size {
        for y in 0..size {
            for z in 0..size {
                let v = base.get_voxel(x, y, z);
                if v != 0 {
                    model.set_voxel(x, size - 1 - y, z, v);
                }
            }
        }
    }

    model.light_blocking = base.light_blocking;
    model.rotatable = base.rotatable;
    model.emission = base.emission;
    model.requires_ground_support = base.requires_ground_support;
    model.compute_collision_mask();
    model
}

/// Creates an empty model (placeholder, id 0).
pub fn create_empty() -> SubVoxelModel {
    SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "empty")
}

/// Creates a torch model with stick and flame.
pub fn create_torch() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "torch");

    // Palette
    model.palette[1] = Color::rgb(101, 67, 33); // Dark wood brown
    model.palette[2] = Color::rgb(139, 90, 43); // Wood brown
    model.palette[3] = Color::rgb(255, 200, 50); // Flame yellow
    model.palette[4] = Color::rgb(255, 100, 20); // Flame orange

    // Stick (center, bottom 5 voxels) - 2×2 cross-section
    for y in 0..5 {
        set_scaled(&mut model, 3, y, 3, 1);
        set_scaled(&mut model, 4, y, 3, 2);
        set_scaled(&mut model, 3, y, 4, 2);
        set_scaled(&mut model, 4, y, 4, 1);
    }

    // Flame core (voxels 5-7)
    for y in 5..8 {
        for dx in 3..5 {
            for dz in 3..5 {
                set_scaled(&mut model, dx, y, dz, 3);
            }
        }
    }

    // Flame outer (y=5,6 expanded)
    for y in 5..7 {
        set_scaled(&mut model, 2, y, 3, 4);
        set_scaled(&mut model, 5, y, 3, 4);
        set_scaled(&mut model, 3, y, 2, 4);
        set_scaled(&mut model, 3, y, 5, 4);
        set_scaled(&mut model, 4, y, 2, 4);
        set_scaled(&mut model, 4, y, 5, 4);
        set_scaled(&mut model, 2, y, 4, 4);
        set_scaled(&mut model, 5, y, 4, 4);
    }

    model.emission = Some(Color::rgb(255, 180, 80));
    model.light_blocking = LightBlocking::Partial;
    model.rotatable = false;
    model.requires_ground_support = true;

    model.compute_collision_mask();
    model
}

/// Creates a bottom slab (half-block on bottom).
pub fn create_slab_bottom() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "slab_bottom");
    model.palette[1] = Color::rgb(128, 128, 128); // Stone gray
    fill_scaled(&mut model, 0, 0, 0, 7, 3, 7, 1);
    model.light_blocking = LightBlocking::Partial;
    model.rotatable = false;
    model.compute_collision_mask();
    model
}

/// Creates a top slab (half-block on top).
pub fn create_slab_top() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "slab_top");
    model.palette[1] = Color::rgb(128, 128, 128); // Stone gray
    fill_scaled(&mut model, 0, 4, 0, 7, 7, 7, 1);
    model.light_blocking = LightBlocking::Partial;
    model.rotatable = false;
    model.compute_collision_mask();
    model
}

/// Creates a fence with the specified connection mask.
pub fn create_fence(connections: u8) -> SubVoxelModel {
    let name = format!("fence_{}", connections);
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, &name);

    model.palette[1] = Color::rgb(139, 90, 43); // Wood brown (post)
    model.palette[2] = Color::rgb(160, 110, 60); // Lighter brown (rails)

    // Center post (2×8×2 at center)
    fill_scaled(&mut model, 3, 0, 3, 4, 7, 4, 1);

    // Add rails based on connections
    let rail_y_ranges = [(2, 3), (5, 6)];

    for &(y0, y1) in &rail_y_ranges {
        if connections & 1 != 0 {
            fill_scaled(&mut model, 3, y0, 0, 4, y1, 2, 2);
        }
        if connections & 2 != 0 {
            fill_scaled(&mut model, 3, y0, 5, 4, y1, 7, 2);
        }
        if connections & 4 != 0 {
            fill_scaled(&mut model, 5, y0, 3, 7, y1, 4, 2);
        }
        if connections & 8 != 0 {
            fill_scaled(&mut model, 0, y0, 3, 2, y1, 4, 2);
        }
    }

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = false;
    model.requires_ground_support = true;
    model.compute_collision_mask();
    model
}

/// Creates a fence gate with connection mask (closed state).
pub fn create_gate_closed(connections: u8) -> SubVoxelModel {
    let name = format!("gate_closed_{}", connections);
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, &name);

    model.palette[1] = Color::rgb(139, 90, 43); // Wood brown (posts)
    model.palette[2] = Color::rgb(160, 110, 60); // Lighter brown (door)
    model.palette[3] = Color::rgb(60, 60, 65); // Iron gray (hardware)

    fill_scaled(&mut model, 0, 0, 3, 1, 7, 4, 1);
    fill_scaled(&mut model, 6, 0, 3, 7, 7, 4, 1);

    fill_scaled(&mut model, 2, 2, 3, 3, 3, 4, 2);
    fill_scaled(&mut model, 2, 5, 3, 3, 6, 4, 2);
    fill_scaled(&mut model, 3, 4, 3, 3, 4, 4, 2);

    fill_scaled(&mut model, 4, 2, 3, 5, 3, 4, 2);
    fill_scaled(&mut model, 4, 5, 3, 5, 6, 4, 2);
    fill_scaled(&mut model, 4, 4, 3, 4, 4, 4, 2);

    set_scaled(&mut model, 1, 3, 2, 3);
    set_scaled(&mut model, 1, 5, 2, 3);
    set_scaled(&mut model, 6, 3, 2, 3);
    set_scaled(&mut model, 6, 5, 2, 3);
    set_scaled(&mut model, 3, 4, 2, 3);
    set_scaled(&mut model, 4, 4, 2, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.requires_ground_support = true;
    model.compute_collision_mask();
    model
}

/// Creates a fence gate with connection mask (open state).
pub fn create_gate_open(connections: u8) -> SubVoxelModel {
    let name = format!("gate_open_{}", connections);
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, &name);

    model.palette[1] = Color::rgb(139, 90, 43); // Wood brown (posts)
    model.palette[2] = Color::rgb(160, 110, 60); // Lighter brown (door)
    model.palette[3] = Color::rgb(60, 60, 65); // Iron gray (hardware)

    fill_scaled(&mut model, 0, 0, 3, 1, 7, 4, 1);
    fill_scaled(&mut model, 6, 0, 3, 7, 7, 4, 1);

    fill_scaled(&mut model, 0, 2, 0, 1, 3, 2, 2);
    fill_scaled(&mut model, 0, 5, 0, 1, 6, 2, 2);
    fill_scaled(&mut model, 0, 4, 0, 1, 4, 0, 2);

    fill_scaled(&mut model, 6, 2, 0, 7, 3, 2, 2);
    fill_scaled(&mut model, 6, 5, 0, 7, 6, 2, 2);
    fill_scaled(&mut model, 6, 4, 0, 7, 4, 0, 2);

    set_scaled(&mut model, 1, 3, 2, 3);
    set_scaled(&mut model, 1, 5, 2, 3);
    set_scaled(&mut model, 6, 3, 2, 3);
    set_scaled(&mut model, 6, 5, 2, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.requires_ground_support = true;
    model.compute_collision_mask();
    model
}

/// Creates stairs facing north (step in back/+Z).
pub fn create_stairs_north() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "stairs_north");
    model.palette[1] = Color::rgb(128, 128, 128); // Stone gray
    fill_scaled(&mut model, 0, 0, 0, 7, 3, 7, 1);
    fill_scaled(&mut model, 0, 4, 4, 7, 7, 7, 1);
    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates an upside-down variant of the stairs (ceiling mounted).
pub fn create_stairs_north_inverted() -> SubVoxelModel {
    let base = create_stairs_north();
    inverted_copy(&base, "stairs_north_inverted")
}

/// Creates a ladder (thin vertical rungs against wall).
pub fn create_ladder() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "ladder");
    model.palette[1] = Color::rgb(139, 90, 43); // Wood brown
    for y in 0..8 {
        set_scaled(&mut model, 1, y, 7, 1);
        set_scaled(&mut model, 6, y, 7, 1);
    }
    for y in [1, 3, 5, 7] {
        for x in 2..6 {
            set_scaled(&mut model, x, y, 7, 1);
        }
    }
    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.requires_ground_support = true;
    model.compute_collision_mask();
    model
}

/// Creates an inner-corner stairs (concave), missing front-left quadrant (relative to facing).
pub fn create_stairs_inner_left() -> SubVoxelModel {
    let mut model =
        SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "stairs_inner_left");
    model.palette[1] = Color::rgb(128, 128, 128); // Stone gray

    // Bottom half solid
    fill_scaled(&mut model, 0, 0, 0, 7, 3, 7, 1);

    // Upper L: high where z>=4 OR x>=4 (concave interior) leaving front-left void
    for z in 0..DESIGN_SIZE {
        for y in 4..DESIGN_SIZE {
            for x in 0..DESIGN_SIZE {
                if x >= 4 || z >= 4 {
                    set_scaled(&mut model, x, y, z, 1);
                }
            }
        }
    }

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Inner-corner stairs missing front-right quadrant (relative to facing).
pub fn create_stairs_inner_right() -> SubVoxelModel {
    let mut model =
        SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "stairs_inner_right");
    model.palette[1] = Color::rgb(128, 128, 128); // Stone gray

    // Bottom half solid
    fill_scaled(&mut model, 0, 0, 0, 7, 3, 7, 1);

    // Upper L: high where z>=4 OR x<=3 (mirror)
    for z in 0..DESIGN_SIZE {
        for y in 4..DESIGN_SIZE {
            for x in 0..DESIGN_SIZE {
                if x <= 3 || z >= 4 {
                    set_scaled(&mut model, x, y, z, 1);
                }
            }
        }
    }

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates an outer-corner stairs (convex), filled back-left (relative to facing).
pub fn create_stairs_outer_left() -> SubVoxelModel {
    let mut model =
        SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "stairs_outer_left");
    model.palette[1] = Color::rgb(128, 128, 128); // Stone gray

    // Bottom half solid
    fill_scaled(&mut model, 0, 0, 0, 7, 3, 7, 1);

    // Upper quarter: only back-left corner (x<=3 AND z>=4)
    for z in 4..DESIGN_SIZE {
        for y in 4..DESIGN_SIZE {
            for x in 0..=3 {
                set_scaled(&mut model, x, y, z, 1);
            }
        }
    }

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates an outer-corner stairs (convex), filled back-right (relative to facing).
pub fn create_stairs_outer_right() -> SubVoxelModel {
    let mut model =
        SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "stairs_outer_right");
    model.palette[1] = Color::rgb(128, 128, 128); // Stone gray

    // Bottom half solid
    fill_scaled(&mut model, 0, 0, 0, 7, 3, 7, 1);

    // Upper quarter: only back-right corner (x>=4 AND z>=4)
    for z in 4..DESIGN_SIZE {
        for y in 4..DESIGN_SIZE {
            for x in 4..DESIGN_SIZE {
                set_scaled(&mut model, x, y, z, 1);
            }
        }
    }

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Inner-corner stairs flipped for ceiling placement (left).
pub fn create_stairs_inner_left_inverted() -> SubVoxelModel {
    let base = create_stairs_inner_left();
    inverted_copy(&base, "stairs_inner_left_inverted")
}

/// Inner-corner stairs flipped for ceiling placement (right).
pub fn create_stairs_inner_right_inverted() -> SubVoxelModel {
    let base = create_stairs_inner_right();
    inverted_copy(&base, "stairs_inner_right_inverted")
}

/// Outer-corner stairs flipped for ceiling placement (left).
pub fn create_stairs_outer_left_inverted() -> SubVoxelModel {
    let base = create_stairs_outer_left();
    inverted_copy(&base, "stairs_outer_left_inverted")
}

/// Outer-corner stairs flipped for ceiling placement (right).
pub fn create_stairs_outer_right_inverted() -> SubVoxelModel {
    let base = create_stairs_outer_right();
    inverted_copy(&base, "stairs_outer_right_inverted")
}

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
    model.compute_collision_mask();
    model
}

/// Creates a crystal cluster model with the given color.
/// The model consists of multiple pointed crystal spires of varying heights.
/// Used by ModelRegistry for crystal blocks (tinted by shader based on block metadata).
pub fn create_crystal(color: Color) -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "crystal");

    // Palette: darker base, main crystal color, bright highlight
    let (r, g, b) = (color.r, color.g, color.b);
    model.palette[1] = Color::rgb(r / 2, g / 2, b / 2); // Dark base
    model.palette[2] = color; // Main crystal
    model.palette[3] = Color::rgb(
        // Bright highlight
        (r as u16 + 128).min(255) as u8,
        (g as u16 + 128).min(255) as u8,
        (b as u16 + 128).min(255) as u8,
    );

    // Central tall crystal spire (tallest, center)
    // Base (2x2)
    fill_scaled(&mut model, 3, 0, 3, 4, 1, 4, 1);
    // Body tapers up
    fill_scaled(&mut model, 3, 2, 3, 4, 4, 4, 2);
    set_scaled(&mut model, 3, 5, 3, 2);
    set_scaled(&mut model, 4, 5, 4, 2);
    set_scaled(&mut model, 3, 6, 4, 2);
    set_scaled(&mut model, 4, 6, 3, 2);
    // Tip
    set_scaled(&mut model, 3, 7, 3, 3);
    set_scaled(&mut model, 4, 7, 4, 3);

    // Front-left crystal (medium height)
    fill_scaled(&mut model, 1, 0, 1, 2, 0, 2, 1);
    fill_scaled(&mut model, 1, 1, 1, 2, 3, 2, 2);
    set_scaled(&mut model, 1, 4, 2, 2);
    set_scaled(&mut model, 2, 4, 1, 2);
    set_scaled(&mut model, 1, 5, 1, 3);

    // Back-right crystal (medium height)
    fill_scaled(&mut model, 5, 0, 5, 6, 0, 6, 1);
    fill_scaled(&mut model, 5, 1, 5, 6, 3, 6, 2);
    set_scaled(&mut model, 5, 4, 6, 2);
    set_scaled(&mut model, 6, 4, 5, 2);
    set_scaled(&mut model, 6, 5, 6, 3);

    // Front-right small crystal
    fill_scaled(&mut model, 5, 0, 1, 6, 0, 2, 1);
    fill_scaled(&mut model, 5, 1, 1, 6, 2, 2, 2);
    set_scaled(&mut model, 5, 3, 2, 3);

    // Back-left small crystal
    fill_scaled(&mut model, 1, 0, 5, 2, 0, 6, 1);
    fill_scaled(&mut model, 1, 1, 5, 2, 2, 6, 2);
    set_scaled(&mut model, 2, 3, 5, 3);

    // Tiny accent crystals
    set_scaled(&mut model, 0, 0, 3, 1);
    set_scaled(&mut model, 0, 1, 3, 2);
    set_scaled(&mut model, 7, 0, 4, 1);
    set_scaled(&mut model, 7, 1, 4, 2);
    set_scaled(&mut model, 3, 0, 7, 1);
    set_scaled(&mut model, 3, 1, 7, 2);
    set_scaled(&mut model, 4, 0, 0, 1);
    set_scaled(&mut model, 4, 1, 0, 2);

    model.emission = Some(color);
    model.light_blocking = LightBlocking::Partial;
    model.rotatable = false;
    model.requires_ground_support = false;
    model.compute_collision_mask();
    model
}

// ============================================================================
// GROUND COVER (Grass, Flowers, Mushrooms, etc.)
// ============================================================================

/// Creates tall grass (cross pattern).
pub fn create_tall_grass() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "tall_grass");

    model.palette[1] = Color::rgb(50, 150, 50); // Grass Green

    // Cross pattern (x=z and x=7-z)
    for i in 1..7 {
        // Diagonal 1
        set_scaled(&mut model, i, i, i, 1);
        set_scaled(&mut model, i, i - 1, i, 1); // Thicken slightly vertically

        // Diagonal 2
        set_scaled(&mut model, i, i, 7 - i, 1);
        set_scaled(&mut model, i, i - 1, 7 - i, 1);
    }

    // Base
    set_scaled(&mut model, 3, 0, 3, 1);
    set_scaled(&mut model, 4, 0, 4, 1);
    set_scaled(&mut model, 3, 0, 4, 1);
    set_scaled(&mut model, 4, 0, 3, 1);

    model.light_blocking = LightBlocking::None;
    model.rotatable = false;
    model.requires_ground_support = true;
    model.no_collision = true;
    model.compute_collision_mask();
    model
}

/// Creates a red flower.
pub fn create_flower_red() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "flower_red");

    model.palette[1] = Color::rgb(50, 150, 50); // Stem Green
    model.palette[2] = Color::rgb(220, 50, 50); // Red Petals
    model.palette[3] = Color::rgb(255, 200, 50); // Yellow Center

    // Stem
    fill_scaled(&mut model, 3, 0, 3, 4, 4, 4, 1);

    // Petals
    fill_scaled(&mut model, 2, 5, 2, 5, 5, 5, 2);
    fill_scaled(&mut model, 3, 6, 3, 4, 6, 4, 3); // Center

    model.light_blocking = LightBlocking::None;
    model.rotatable = false;
    model.requires_ground_support = true;
    model.no_collision = true;
    model.compute_collision_mask();
    model
}

/// Creates a yellow flower.
pub fn create_flower_yellow() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "flower_yellow");

    model.palette[1] = Color::rgb(50, 150, 50); // Stem Green
    model.palette[2] = Color::rgb(255, 220, 50); // Yellow Petals
    model.palette[3] = Color::rgb(200, 150, 50); // Orange Center

    // Stem
    fill_scaled(&mut model, 3, 0, 3, 4, 4, 4, 1);

    // Petals (slightly different shape)
    fill_scaled(&mut model, 2, 5, 3, 5, 6, 4, 2);
    fill_scaled(&mut model, 3, 5, 2, 4, 6, 5, 2);
    fill_scaled(&mut model, 3, 5, 3, 4, 5, 4, 3); // Center

    model.light_blocking = LightBlocking::None;
    model.rotatable = false;
    model.requires_ground_support = true;
    model.no_collision = true;
    model.compute_collision_mask();
    model
}

/// Creates a lily pad.
pub fn create_lily_pad() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "lily_pad");

    model.palette[1] = Color::rgb(40, 140, 60); // Dark Green

    // Flat pad at y=0, with notch
    for x in 0..8 {
        for z in 0..8 {
            // Circle-ish approximation
            let dx = x as f32 - 3.5;
            let dz = z as f32 - 3.5;
            if dx * dx + dz * dz <= 16.0 {
                // Notch cut out
                if x < 4 || (z != 3 && z != 4) {
                    set_scaled(&mut model, x, 0, z, 1);
                }
            }
        }
    }

    model.light_blocking = LightBlocking::None;
    model.rotatable = true; // Can rotate on water
    model.compute_collision_mask();
    model
}

/// Creates a brown mushroom.
pub fn create_mushroom_brown() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "mushroom_brown");

    model.palette[1] = Color::rgb(220, 200, 180); // White/Beige Stalk
    model.palette[2] = Color::rgb(140, 100, 70); // Brown Cap

    // Stalk
    fill_scaled(&mut model, 3, 0, 3, 4, 2, 4, 1);

    // Flat Cap
    fill_scaled(&mut model, 1, 3, 1, 6, 3, 6, 2);
    fill_scaled(&mut model, 2, 4, 2, 5, 4, 5, 2);

    model.light_blocking = LightBlocking::None;
    model.rotatable = false;
    model.requires_ground_support = true;
    model.no_collision = true;
    model.emission = Some(Color::rgb(10, 5, 0)); // Very faint glow? No.
    model.compute_collision_mask();
    model
}

/// Creates a red mushroom.
pub fn create_mushroom_red() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "mushroom_red");

    model.palette[1] = Color::rgb(220, 200, 180); // White/Beige Stalk
    model.palette[2] = Color::rgb(220, 40, 40); // Red Cap
    model.palette[3] = Color::rgb(255, 255, 255); // White Dots

    // Stalk
    fill_scaled(&mut model, 3, 0, 3, 4, 2, 4, 1);

    // Domed Cap
    fill_scaled(&mut model, 1, 3, 1, 6, 3, 6, 2); // Base rim
    fill_scaled(&mut model, 2, 4, 2, 5, 4, 5, 2); // Dome
    fill_scaled(&mut model, 3, 5, 3, 4, 5, 4, 2); // Top

    // Dots
    set_scaled(&mut model, 2, 4, 2, 3);
    set_scaled(&mut model, 5, 4, 5, 3);
    set_scaled(&mut model, 3, 5, 3, 3);

    model.light_blocking = LightBlocking::None;
    model.rotatable = false;
    model.requires_ground_support = true;
    model.no_collision = true;
    model.compute_collision_mask();
    model
}

use super::basic::{DESIGN_SIZE, fill_scaled, inverted_copy, set_scaled};
use crate::sub_voxel::{Color, LightBlocking, ModelResolution, SubVoxelModel};

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

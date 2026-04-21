use super::basic::{fill_scaled, set_scaled};
use crate::sub_voxel::{Color, LightBlocking, ModelResolution, SubVoxelModel};

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

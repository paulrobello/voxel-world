use super::basic::{fill_scaled, set_scaled};
use crate::sub_voxel::{Color, LightBlocking, ModelResolution, SubVoxelModel};

/// Creates a stalactite (hangs from ceiling).
/// Tapered cone shape, 6 blocks tall.
pub fn create_stalactite() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "stalactite");

    model.palette[1] = Color::rgb(150, 150, 150); // Stone gray

    // Build from top (attached to ceiling) to bottom (pointed tip)
    // Layer 7 (top, attached to ceiling): 4x4 base
    fill_scaled(&mut model, 2, 7, 2, 5, 7, 5, 1);

    // Layer 6: 4x4
    fill_scaled(&mut model, 2, 6, 2, 5, 6, 5, 1);

    // Layer 5: 3x3
    fill_scaled(&mut model, 2, 5, 2, 5, 5, 5, 1);

    // Layer 4: 3x3
    fill_scaled(&mut model, 2, 4, 2, 5, 4, 5, 1);

    // Layer 3: 2x2
    fill_scaled(&mut model, 3, 3, 3, 4, 3, 4, 1);

    // Layer 2: 2x2
    fill_scaled(&mut model, 3, 2, 3, 4, 2, 4, 1);

    // Layer 1: Single voxel tip
    set_scaled(&mut model, 3, 1, 3, 1);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = false;
    model.no_collision = true; // Can walk through
    model.compute_collision_mask();
    model
}

/// Creates a stalagmite (grows from floor).
/// Tapered cone shape, 6 blocks tall.
pub fn create_stalagmite() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "stalagmite");

    model.palette[1] = Color::rgb(150, 150, 150); // Stone gray

    // Build from bottom (attached to floor) to top (pointed tip)
    // Layer 0 (bottom, attached to floor): 4x4 base
    fill_scaled(&mut model, 2, 0, 2, 5, 0, 5, 1);

    // Layer 1: 4x4
    fill_scaled(&mut model, 2, 1, 2, 5, 1, 5, 1);

    // Layer 2: 3x3
    fill_scaled(&mut model, 2, 2, 2, 5, 2, 5, 1);

    // Layer 3: 3x3
    fill_scaled(&mut model, 2, 3, 2, 5, 3, 5, 1);

    // Layer 4: 2x2
    fill_scaled(&mut model, 3, 4, 3, 4, 4, 4, 1);

    // Layer 5: 2x2
    fill_scaled(&mut model, 3, 5, 3, 4, 5, 4, 1);

    // Layer 6: Single voxel tip
    set_scaled(&mut model, 3, 6, 3, 1);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = false;
    model.no_collision = true; // Can walk through
    model.requires_ground_support = true;
    model.compute_collision_mask();
    model
}

/// Creates an ice stalactite (hangs from ceiling).
/// Identical shape to stone stalactite but with ice blue color.
pub fn create_ice_stalactite() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "ice_stalactite");

    model.palette[1] = Color::rgb(180, 220, 255); // Ice blue

    // Build from top (attached to ceiling) to bottom (pointed tip)
    // Layer 7 (top, attached to ceiling): 4x4 base
    fill_scaled(&mut model, 2, 7, 2, 5, 7, 5, 1);

    // Layer 6: 4x4
    fill_scaled(&mut model, 2, 6, 2, 5, 6, 5, 1);

    // Layer 5: 3x3
    fill_scaled(&mut model, 2, 5, 2, 5, 5, 5, 1);

    // Layer 4: 3x3
    fill_scaled(&mut model, 2, 4, 2, 5, 4, 5, 1);

    // Layer 3: 2x2
    fill_scaled(&mut model, 3, 3, 3, 4, 3, 4, 1);

    // Layer 2: 2x2
    fill_scaled(&mut model, 3, 2, 3, 4, 2, 4, 1);

    // Layer 1: Single voxel tip
    set_scaled(&mut model, 3, 1, 3, 1);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = false;
    model.no_collision = true; // Can walk through
    model.compute_collision_mask();
    model
}

/// Creates an ice stalagmite (grows from floor).
/// Identical shape to stone stalagmite but with ice blue color.
pub fn create_ice_stalagmite() -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "ice_stalagmite");

    model.palette[1] = Color::rgb(180, 220, 255); // Ice blue

    // Build from bottom (attached to floor) to top (pointed tip)
    // Layer 0 (bottom, attached to floor): 4x4 base
    fill_scaled(&mut model, 2, 0, 2, 5, 0, 5, 1);

    // Layer 1: 4x4
    fill_scaled(&mut model, 2, 1, 2, 5, 1, 5, 1);

    // Layer 2: 3x3
    fill_scaled(&mut model, 2, 2, 2, 5, 2, 5, 1);

    // Layer 3: 3x3
    fill_scaled(&mut model, 2, 3, 2, 5, 3, 5, 1);

    // Layer 4: 2x2
    fill_scaled(&mut model, 3, 4, 3, 4, 4, 4, 1);

    // Layer 5: 2x2
    fill_scaled(&mut model, 3, 5, 3, 4, 5, 4, 1);

    // Layer 6: Single voxel tip
    set_scaled(&mut model, 3, 6, 3, 1);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = false;
    model.no_collision = true; // Can walk through
    model.requires_ground_support = true;
    model.compute_collision_mask();
    model
}

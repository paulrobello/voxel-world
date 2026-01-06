use super::basic::{fill_scaled, set_scaled};
use crate::sub_voxel::{Color, LightBlocking, ModelResolution, SubVoxelModel};

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

use super::basic::{fill_scaled, set_scaled};
use crate::sub_voxel::{Color, LightBlocking, ModelResolution, SubVoxelModel};

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

    // Debug: Print torch model info
    let voxel_count = model.voxels.iter().filter(|&&v| v != 0).count();
    println!("[DEBUG] Torch model created:");
    println!("  - Voxel count: {}/{}", voxel_count, model.voxels.len());
    println!("  - Collision mask: 0x{:016x}", model.collision_mask);
    println!("  - Resolution: {:?}", model.resolution);

    model
}

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

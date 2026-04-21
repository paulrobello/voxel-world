use crate::sub_voxel::{Color, LightBlocking, ModelResolution, SubVoxelModel};

/// The design space size (8³) - models are designed in this space.
/// All built-in models use Low (8³) resolution for optimal performance.
pub const DESIGN_SIZE: usize = 8;

/// Scale factor - now 1 since we render at native 8³ resolution.
pub const SCALE: usize = 1;

/// Places a scaled voxel. In 8³ mode (SCALE=1), places a single voxel.
/// In 16³ mode (SCALE=2), places a 2×2×2 block at scaled coordinates.
pub fn set_scaled(model: &mut SubVoxelModel, x: usize, y: usize, z: usize, v: u8) {
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
pub fn fill_scaled(
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
pub fn inverted_copy(base: &SubVoxelModel, name: &str) -> SubVoxelModel {
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

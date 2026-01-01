use crate::sub_voxel::{Color, LightBlocking, SubVoxelModel};

/// Creates an empty model (placeholder, id 0).
pub fn create_empty() -> SubVoxelModel {
    SubVoxelModel::new("empty")
}

/// Creates a torch model with stick and flame.
pub fn create_torch() -> SubVoxelModel {
    let mut model = SubVoxelModel::new("torch");

    // Palette
    model.palette[1] = Color::rgb(101, 67, 33); // Dark wood brown
    model.palette[2] = Color::rgb(139, 90, 43); // Wood brown
    model.palette[3] = Color::rgb(255, 200, 50); // Flame yellow
    model.palette[4] = Color::rgb(255, 100, 20); // Flame orange

    // Stick (center, bottom 5 voxels) - 2×2 cross-section
    for y in 0..5 {
        model.set_voxel(3, y, 3, 1);
        model.set_voxel(4, y, 3, 2);
        model.set_voxel(3, y, 4, 2);
        model.set_voxel(4, y, 4, 1);
    }

    // Flame core (voxels 5-7)
    for y in 5..8 {
        for dx in 3..5 {
            for dz in 3..5 {
                model.set_voxel(dx, y, dz, 3);
            }
        }
    }

    // Flame outer (y=5,6 expanded)
    for y in 5..7 {
        model.set_voxel(2, y, 3, 4);
        model.set_voxel(5, y, 3, 4);
        model.set_voxel(3, y, 2, 4);
        model.set_voxel(3, y, 5, 4);
        model.set_voxel(4, y, 2, 4);
        model.set_voxel(4, y, 5, 4);
        model.set_voxel(2, y, 4, 4);
        model.set_voxel(5, y, 4, 4);
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
    let mut model = SubVoxelModel::new("slab_bottom");
    model.palette[1] = Color::rgb(128, 128, 128); // Stone gray
    model.fill_box(0, 0, 0, 7, 3, 7, 1);
    model.light_blocking = LightBlocking::Full;
    model.rotatable = false;
    model.compute_collision_mask();
    model
}

/// Creates a top slab (half-block on top).
pub fn create_slab_top() -> SubVoxelModel {
    let mut model = SubVoxelModel::new("slab_top");
    model.palette[1] = Color::rgb(128, 128, 128); // Stone gray
    model.fill_box(0, 4, 0, 7, 7, 7, 1);
    model.light_blocking = LightBlocking::Full;
    model.rotatable = false;
    model.compute_collision_mask();
    model
}

/// Creates a fence with the specified connection mask.
pub fn create_fence(connections: u8) -> SubVoxelModel {
    let name = format!("fence_{}", connections);
    let mut model = SubVoxelModel::new(&name);

    model.palette[1] = Color::rgb(139, 90, 43); // Wood brown (post)
    model.palette[2] = Color::rgb(160, 110, 60); // Lighter brown (rails)

    // Center post (2×8×2 at center)
    model.fill_box(3, 0, 3, 4, 7, 4, 1);

    // Add rails based on connections
    let rail_y_ranges = [(2, 3), (5, 6)];

    for &(y0, y1) in &rail_y_ranges {
        if connections & 1 != 0 {
            model.fill_box(3, y0, 0, 4, y1, 2, 2);
        }
        if connections & 2 != 0 {
            model.fill_box(3, y0, 5, 4, y1, 7, 2);
        }
        if connections & 4 != 0 {
            model.fill_box(5, y0, 3, 7, y1, 4, 2);
        }
        if connections & 8 != 0 {
            model.fill_box(0, y0, 3, 2, y1, 4, 2);
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
    let mut model = SubVoxelModel::new(&name);

    model.palette[1] = Color::rgb(139, 90, 43); // Wood brown (posts)
    model.palette[2] = Color::rgb(160, 110, 60); // Lighter brown (door)
    model.palette[3] = Color::rgb(60, 60, 65); // Iron gray (hardware)

    model.fill_box(0, 0, 3, 1, 7, 4, 1);
    model.fill_box(6, 0, 3, 7, 7, 4, 1);

    model.fill_box(2, 2, 3, 3, 3, 4, 2);
    model.fill_box(2, 5, 3, 3, 6, 4, 2);
    model.fill_box(3, 4, 3, 3, 4, 4, 2);

    model.fill_box(4, 2, 3, 5, 3, 4, 2);
    model.fill_box(4, 5, 3, 5, 6, 4, 2);
    model.fill_box(4, 4, 3, 4, 4, 4, 2);

    model.set_voxel(1, 3, 2, 3);
    model.set_voxel(1, 5, 2, 3);
    model.set_voxel(6, 3, 2, 3);
    model.set_voxel(6, 5, 2, 3);
    model.set_voxel(3, 4, 2, 3);
    model.set_voxel(4, 4, 2, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.requires_ground_support = true;
    model.compute_collision_mask();
    model
}

/// Creates a fence gate with connection mask (open state).
pub fn create_gate_open(connections: u8) -> SubVoxelModel {
    let name = format!("gate_open_{}", connections);
    let mut model = SubVoxelModel::new(&name);

    model.palette[1] = Color::rgb(139, 90, 43); // Wood brown (posts)
    model.palette[2] = Color::rgb(160, 110, 60); // Lighter brown (door)
    model.palette[3] = Color::rgb(60, 60, 65); // Iron gray (hardware)

    model.fill_box(0, 0, 3, 1, 7, 4, 1);
    model.fill_box(6, 0, 3, 7, 7, 4, 1);

    model.fill_box(0, 2, 0, 1, 3, 2, 2);
    model.fill_box(0, 5, 0, 1, 6, 2, 2);
    model.fill_box(0, 4, 0, 1, 4, 0, 2);

    model.fill_box(6, 2, 0, 7, 3, 2, 2);
    model.fill_box(6, 5, 0, 7, 6, 2, 2);
    model.fill_box(6, 4, 0, 7, 4, 0, 2);

    model.set_voxel(1, 3, 2, 3);
    model.set_voxel(1, 5, 2, 3);
    model.set_voxel(6, 3, 2, 3);
    model.set_voxel(6, 5, 2, 3);

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.requires_ground_support = true;
    model.compute_collision_mask();
    model
}

/// Creates stairs facing north (step in back/+Z).
pub fn create_stairs_north() -> SubVoxelModel {
    let mut model = SubVoxelModel::new("stairs_north");
    model.palette[1] = Color::rgb(128, 128, 128); // Stone gray
    model.fill_box(0, 0, 0, 7, 3, 7, 1);
    model.fill_box(0, 4, 4, 7, 7, 7, 1);
    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.compute_collision_mask();
    model
}

/// Creates a ladder (thin vertical rungs against wall).
pub fn create_ladder() -> SubVoxelModel {
    let mut model = SubVoxelModel::new("ladder");
    model.palette[1] = Color::rgb(139, 90, 43); // Wood brown
    for y in 0..8 {
        model.set_voxel(1, y, 7, 1);
        model.set_voxel(6, y, 7, 1);
    }
    for y in [1, 3, 5, 7] {
        for x in 2..6 {
            model.set_voxel(x, y, 7, 1);
        }
    }
    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.requires_ground_support = true;
    model.compute_collision_mask();
    model
}

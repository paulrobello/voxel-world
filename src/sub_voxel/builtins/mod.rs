pub mod basic;
pub mod caves;
pub mod doors;
pub mod fences;
pub mod frames;
pub mod glass_panes;
pub mod lighting;
pub mod stairs;
pub mod vegetation;

use crate::sub_voxel::ModelRegistry;
use crate::sub_voxel::types::Color;

use self::basic::{create_empty, create_slab_bottom, create_slab_top};
use self::caves::{
    create_ice_stalactite, create_ice_stalagmite, create_stalactite, create_stalagmite,
};
use self::doors::{
    create_door_lower_closed_left, create_door_lower_closed_right, create_door_lower_open_left,
    create_door_lower_open_right, create_door_upper_closed_left, create_door_upper_closed_right,
    create_door_upper_open_left, create_door_upper_open_right, create_fancy_door_lower_closed_left,
    create_fancy_door_lower_closed_right, create_fancy_door_lower_open_left,
    create_fancy_door_lower_open_right, create_fancy_door_upper_closed_left,
    create_fancy_door_upper_closed_right, create_fancy_door_upper_open_left,
    create_fancy_door_upper_open_right, create_glass_door_lower_closed_left,
    create_glass_door_lower_closed_right, create_glass_door_lower_open_left,
    create_glass_door_lower_open_right, create_glass_door_upper_closed_left,
    create_glass_door_upper_closed_right, create_glass_door_upper_open_left,
    create_glass_door_upper_open_right, create_paneled_door_lower_closed_left,
    create_paneled_door_lower_closed_right, create_paneled_door_lower_open_left,
    create_paneled_door_lower_open_right, create_paneled_door_upper_closed_left,
    create_paneled_door_upper_closed_right, create_paneled_door_upper_open_left,
    create_paneled_door_upper_open_right, create_trapdoor_ceiling_closed,
    create_trapdoor_ceiling_open, create_trapdoor_floor_closed, create_trapdoor_floor_open,
    create_window, create_windowed_door_lower_closed_left, create_windowed_door_lower_closed_right,
    create_windowed_door_lower_open_left, create_windowed_door_lower_open_right,
    create_windowed_door_upper_closed_left, create_windowed_door_upper_closed_right,
    create_windowed_door_upper_open_left, create_windowed_door_upper_open_right,
};
use self::fences::{create_fence, create_gate_closed, create_gate_open};
use self::frames::register_all_frame_variants;
use self::glass_panes::{create_horizontal_glass_pane, create_vertical_glass_pane};
use self::lighting::{create_crystal, create_torch};
use self::stairs::{
    create_ladder, create_stairs_inner_left, create_stairs_inner_left_inverted,
    create_stairs_inner_right, create_stairs_inner_right_inverted, create_stairs_north,
    create_stairs_north_inverted, create_stairs_outer_left, create_stairs_outer_left_inverted,
    create_stairs_outer_right, create_stairs_outer_right_inverted,
};
use self::vegetation::{
    create_dead_bush, create_fern, create_flower_blue, create_flower_red, create_flower_yellow,
    create_glow_berry_vines, create_glow_lichen, create_glow_mushroom_model, create_hanging_roots,
    create_lily_pad, create_moss_carpet, create_mushroom_brown, create_mushroom_red,
    create_seagrass, create_tall_grass,
};

/// Registers built-in models.
pub fn register_builtins(registry: &mut ModelRegistry) {
    // ID 0: Empty/placeholder (no model)
    registry
        .register(create_empty())
        .expect("builtin model registry overflow");

    // ID 1: Torch
    registry
        .register(create_torch())
        .expect("builtin model registry overflow");

    // ID 2-3: Slabs
    registry
        .register(create_slab_bottom())
        .expect("builtin model registry overflow");
    registry
        .register(create_slab_top())
        .expect("builtin model registry overflow");

    // ID 4-19: Fence variants (16 connection combinations)
    // Connection bitmask: N=1, S=2, E=4, W=8
    for connections in 0..16u8 {
        registry
            .register(create_fence(connections))
            .expect("builtin model registry overflow");
    }

    // ID 20-23: Closed gate variants (4 connection combinations)
    // Connection bitmask: W=1, E=2
    for connections in 0..4u8 {
        registry
            .register(create_gate_closed(connections))
            .expect("builtin model registry overflow");
    }

    // ID 24-27: Open gate variants (4 connection combinations)
    for connections in 0..4u8 {
        registry
            .register(create_gate_open(connections))
            .expect("builtin model registry overflow");
    }

    // ID 28: Stairs
    registry
        .register(create_stairs_north())
        .expect("builtin model registry overflow");

    // ID 29: Ladder
    registry
        .register(create_ladder())
        .expect("builtin model registry overflow");

    // ID 30: Upside-down stairs
    registry
        .register(create_stairs_north_inverted())
        .expect("builtin model registry overflow");

    // ID 31-34: Inner/outer stairs (upright)
    registry
        .register(create_stairs_inner_left())
        .expect("builtin model registry overflow");
    registry
        .register(create_stairs_inner_right())
        .expect("builtin model registry overflow");
    registry
        .register(create_stairs_outer_left())
        .expect("builtin model registry overflow");
    registry
        .register(create_stairs_outer_right())
        .expect("builtin model registry overflow");

    // ID 35-38: Inner/outer stairs (inverted)
    registry
        .register(create_stairs_inner_left_inverted())
        .expect("builtin model registry overflow");
    registry
        .register(create_stairs_inner_right_inverted())
        .expect("builtin model registry overflow");
    registry
        .register(create_stairs_outer_left_inverted())
        .expect("builtin model registry overflow");
    registry
        .register(create_stairs_outer_right_inverted())
        .expect("builtin model registry overflow");

    // ID 39-46: Doors (8 variants)
    // Order: lower closed left, lower closed right, upper closed left, upper closed right,
    //        lower open left, lower open right, upper open left, upper open right
    registry
        .register(create_door_lower_closed_left())
        .expect("builtin model registry overflow");
    registry
        .register(create_door_lower_closed_right())
        .expect("builtin model registry overflow");
    registry
        .register(create_door_upper_closed_left())
        .expect("builtin model registry overflow");
    registry
        .register(create_door_upper_closed_right())
        .expect("builtin model registry overflow");
    registry
        .register(create_door_lower_open_left())
        .expect("builtin model registry overflow");
    registry
        .register(create_door_lower_open_right())
        .expect("builtin model registry overflow");
    registry
        .register(create_door_upper_open_left())
        .expect("builtin model registry overflow");
    registry
        .register(create_door_upper_open_right())
        .expect("builtin model registry overflow");

    // ID 47-50: Trapdoors (4 variants)
    registry
        .register(create_trapdoor_floor_closed())
        .expect("builtin model registry overflow");
    registry
        .register(create_trapdoor_ceiling_closed())
        .expect("builtin model registry overflow");
    registry
        .register(create_trapdoor_floor_open())
        .expect("builtin model registry overflow");
    registry
        .register(create_trapdoor_ceiling_open())
        .expect("builtin model registry overflow");

    // ID 51-66: Windows (16 connection variants)
    for connections in 0..16u8 {
        registry
            .register(create_window(connections))
            .expect("builtin model registry overflow");
    }

    // ID 67-74: Windowed Doors (8 variants)
    registry
        .register(create_windowed_door_lower_closed_left())
        .expect("builtin model registry overflow");
    registry
        .register(create_windowed_door_lower_closed_right())
        .expect("builtin model registry overflow");
    registry
        .register(create_windowed_door_upper_closed_left())
        .expect("builtin model registry overflow");
    registry
        .register(create_windowed_door_upper_closed_right())
        .expect("builtin model registry overflow");
    registry
        .register(create_windowed_door_lower_open_left())
        .expect("builtin model registry overflow");
    registry
        .register(create_windowed_door_lower_open_right())
        .expect("builtin model registry overflow");
    registry
        .register(create_windowed_door_upper_open_left())
        .expect("builtin model registry overflow");
    registry
        .register(create_windowed_door_upper_open_right())
        .expect("builtin model registry overflow");

    // ID 75-82: Paneled Doors (8 variants)
    registry
        .register(create_paneled_door_lower_closed_left())
        .expect("builtin model registry overflow");
    registry
        .register(create_paneled_door_lower_closed_right())
        .expect("builtin model registry overflow");
    registry
        .register(create_paneled_door_upper_closed_left())
        .expect("builtin model registry overflow");
    registry
        .register(create_paneled_door_upper_closed_right())
        .expect("builtin model registry overflow");
    registry
        .register(create_paneled_door_lower_open_left())
        .expect("builtin model registry overflow");
    registry
        .register(create_paneled_door_lower_open_right())
        .expect("builtin model registry overflow");
    registry
        .register(create_paneled_door_upper_open_left())
        .expect("builtin model registry overflow");
    registry
        .register(create_paneled_door_upper_open_right())
        .expect("builtin model registry overflow");

    // ID 83-90: Windowed+Paneled Doors (8 variants)
    registry
        .register(create_fancy_door_lower_closed_left())
        .expect("builtin model registry overflow");
    registry
        .register(create_fancy_door_lower_closed_right())
        .expect("builtin model registry overflow");
    registry
        .register(create_fancy_door_upper_closed_left())
        .expect("builtin model registry overflow");
    registry
        .register(create_fancy_door_upper_closed_right())
        .expect("builtin model registry overflow");
    registry
        .register(create_fancy_door_lower_open_left())
        .expect("builtin model registry overflow");
    registry
        .register(create_fancy_door_lower_open_right())
        .expect("builtin model registry overflow");
    registry
        .register(create_fancy_door_upper_open_left())
        .expect("builtin model registry overflow");
    registry
        .register(create_fancy_door_upper_open_right())
        .expect("builtin model registry overflow");

    // ID 91-98: Full Glass Doors (8 variants)
    registry
        .register(create_glass_door_lower_closed_left())
        .expect("builtin model registry overflow");
    registry
        .register(create_glass_door_lower_closed_right())
        .expect("builtin model registry overflow");
    registry
        .register(create_glass_door_upper_closed_left())
        .expect("builtin model registry overflow");
    registry
        .register(create_glass_door_upper_closed_right())
        .expect("builtin model registry overflow");
    registry
        .register(create_glass_door_lower_open_left())
        .expect("builtin model registry overflow");
    registry
        .register(create_glass_door_lower_open_right())
        .expect("builtin model registry overflow");
    registry
        .register(create_glass_door_upper_open_left())
        .expect("builtin model registry overflow");
    registry
        .register(create_glass_door_upper_open_right())
        .expect("builtin model registry overflow");

    // ID 99: Crystal (neutral color, tinted by shader based on tint_index)
    registry
        .register(create_crystal(Color::rgb(220, 220, 220)))
        .expect("builtin model registry overflow");

    // ID 100: Tall Grass
    registry
        .register(create_tall_grass())
        .expect("builtin model registry overflow");
    // ID 101: Red Flower
    registry
        .register(create_flower_red())
        .expect("builtin model registry overflow");
    // ID 102: Yellow Flower
    registry
        .register(create_flower_yellow())
        .expect("builtin model registry overflow");
    // ID 103: Lily Pad
    registry
        .register(create_lily_pad())
        .expect("builtin model registry overflow");
    // ID 104: Brown Mushroom
    registry
        .register(create_mushroom_brown())
        .expect("builtin model registry overflow");
    // ID 105: Red Mushroom
    registry
        .register(create_mushroom_red())
        .expect("builtin model registry overflow");

    // ID 106: Stalactite (stone)
    registry
        .register(create_stalactite())
        .expect("builtin model registry overflow");
    // ID 107: Stalagmite (stone)
    registry
        .register(create_stalagmite())
        .expect("builtin model registry overflow");
    // ID 108: Ice Stalactite
    registry
        .register(create_ice_stalactite())
        .expect("builtin model registry overflow");
    // ID 109: Ice Stalagmite
    registry
        .register(create_ice_stalagmite())
        .expect("builtin model registry overflow");

    // === Cave Vegetation (IDs 110-114) ===
    // ID 110: Moss Carpet (lush caves floor)
    registry
        .register(create_moss_carpet())
        .expect("builtin model registry overflow");
    // ID 111: Glow Lichen (ceiling/wall with emission)
    registry
        .register(create_glow_lichen())
        .expect("builtin model registry overflow");
    // ID 112: Hanging Roots (lush caves ceiling)
    registry
        .register(create_hanging_roots())
        .expect("builtin model registry overflow");
    // ID 113: Glow Berry Vines (lush caves ceiling with emission)
    registry
        .register(create_glow_berry_vines())
        .expect("builtin model registry overflow");
    // ID 114: Glow Mushroom Model (floor with emission)
    registry
        .register(create_glow_mushroom_model())
        .expect("builtin model registry overflow");

    // === Additional Surface Vegetation (IDs 115-118) ===
    // ID 115: Fern (taiga/jungle)
    registry
        .register(create_fern())
        .expect("builtin model registry overflow");
    // ID 116: Dead Bush (desert/savanna)
    registry
        .register(create_dead_bush())
        .expect("builtin model registry overflow");
    // ID 117: Seagrass (underwater)
    registry
        .register(create_seagrass())
        .expect("builtin model registry overflow");
    // ID 118: Blue Flower (cornflower)
    registry
        .register(create_flower_blue())
        .expect("builtin model registry overflow");

    // === Glass Panes (IDs 119-150) ===
    // ID 119-134: Horizontal glass panes (16 connection variants)
    // Connection bitmask: N=1, S=2, E=4, W=8
    for connections in 0..16u8 {
        registry
            .register(create_horizontal_glass_pane(connections))
            .expect("builtin model registry overflow");
    }
    // ID 135-150: Vertical glass panes (16 connection variants)
    // Rotatable: rotation 0=XY plane, rotation 1=YZ plane
    for connections in 0..16u8 {
        registry
            .register(create_vertical_glass_pane(connections))
            .expect("builtin model registry overflow");
    }

    // === Reserved/Placeholder (IDs 151-159) ===
    // These IDs are reserved for future use. We fill them with empty models
    // to ensure frame models get IDs 160-175 (16 edge mask variants).
    for i in 151..=159 {
        let mut placeholder = create_empty();
        placeholder.name = format!("reserved_{}", i);
        registry
            .register(placeholder)
            .expect("builtin model registry overflow");
    }

    // === Picture Frames (IDs 160-175) ===
    // 16 variants for different edge mask combinations
    register_all_frame_variants(registry);
}

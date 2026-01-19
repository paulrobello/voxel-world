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
use self::frames::{
    create_frame_1x1, create_frame_1x2, create_frame_1x3, create_frame_2x1, create_frame_2x2,
    create_frame_2x3, create_frame_3x1, create_frame_3x2, create_frame_3x3,
};
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
    registry.register(create_empty());

    // ID 1: Torch
    registry.register(create_torch());

    // ID 2-3: Slabs
    registry.register(create_slab_bottom());
    registry.register(create_slab_top());

    // ID 4-19: Fence variants (16 connection combinations)
    // Connection bitmask: N=1, S=2, E=4, W=8
    for connections in 0..16u8 {
        registry.register(create_fence(connections));
    }

    // ID 20-23: Closed gate variants (4 connection combinations)
    // Connection bitmask: W=1, E=2
    for connections in 0..4u8 {
        registry.register(create_gate_closed(connections));
    }

    // ID 24-27: Open gate variants (4 connection combinations)
    for connections in 0..4u8 {
        registry.register(create_gate_open(connections));
    }

    // ID 28: Stairs
    registry.register(create_stairs_north());

    // ID 29: Ladder
    registry.register(create_ladder());

    // ID 30: Upside-down stairs
    registry.register(create_stairs_north_inverted());

    // ID 31-34: Inner/outer stairs (upright)
    registry.register(create_stairs_inner_left());
    registry.register(create_stairs_inner_right());
    registry.register(create_stairs_outer_left());
    registry.register(create_stairs_outer_right());

    // ID 35-38: Inner/outer stairs (inverted)
    registry.register(create_stairs_inner_left_inverted());
    registry.register(create_stairs_inner_right_inverted());
    registry.register(create_stairs_outer_left_inverted());
    registry.register(create_stairs_outer_right_inverted());

    // ID 39-46: Doors (8 variants)
    // Order: lower closed left, lower closed right, upper closed left, upper closed right,
    //        lower open left, lower open right, upper open left, upper open right
    registry.register(create_door_lower_closed_left());
    registry.register(create_door_lower_closed_right());
    registry.register(create_door_upper_closed_left());
    registry.register(create_door_upper_closed_right());
    registry.register(create_door_lower_open_left());
    registry.register(create_door_lower_open_right());
    registry.register(create_door_upper_open_left());
    registry.register(create_door_upper_open_right());

    // ID 47-50: Trapdoors (4 variants)
    registry.register(create_trapdoor_floor_closed());
    registry.register(create_trapdoor_ceiling_closed());
    registry.register(create_trapdoor_floor_open());
    registry.register(create_trapdoor_ceiling_open());

    // ID 51-66: Windows (16 connection variants)
    for connections in 0..16u8 {
        registry.register(create_window(connections));
    }

    // ID 67-74: Windowed Doors (8 variants)
    registry.register(create_windowed_door_lower_closed_left());
    registry.register(create_windowed_door_lower_closed_right());
    registry.register(create_windowed_door_upper_closed_left());
    registry.register(create_windowed_door_upper_closed_right());
    registry.register(create_windowed_door_lower_open_left());
    registry.register(create_windowed_door_lower_open_right());
    registry.register(create_windowed_door_upper_open_left());
    registry.register(create_windowed_door_upper_open_right());

    // ID 75-82: Paneled Doors (8 variants)
    registry.register(create_paneled_door_lower_closed_left());
    registry.register(create_paneled_door_lower_closed_right());
    registry.register(create_paneled_door_upper_closed_left());
    registry.register(create_paneled_door_upper_closed_right());
    registry.register(create_paneled_door_lower_open_left());
    registry.register(create_paneled_door_lower_open_right());
    registry.register(create_paneled_door_upper_open_left());
    registry.register(create_paneled_door_upper_open_right());

    // ID 83-90: Windowed+Paneled Doors (8 variants)
    registry.register(create_fancy_door_lower_closed_left());
    registry.register(create_fancy_door_lower_closed_right());
    registry.register(create_fancy_door_upper_closed_left());
    registry.register(create_fancy_door_upper_closed_right());
    registry.register(create_fancy_door_lower_open_left());
    registry.register(create_fancy_door_lower_open_right());
    registry.register(create_fancy_door_upper_open_left());
    registry.register(create_fancy_door_upper_open_right());

    // ID 91-98: Full Glass Doors (8 variants)
    registry.register(create_glass_door_lower_closed_left());
    registry.register(create_glass_door_lower_closed_right());
    registry.register(create_glass_door_upper_closed_left());
    registry.register(create_glass_door_upper_closed_right());
    registry.register(create_glass_door_lower_open_left());
    registry.register(create_glass_door_lower_open_right());
    registry.register(create_glass_door_upper_open_left());
    registry.register(create_glass_door_upper_open_right());

    // ID 99: Crystal (neutral color, tinted by shader based on tint_index)
    registry.register(create_crystal(Color::rgb(220, 220, 220)));

    // ID 100: Tall Grass
    registry.register(create_tall_grass());
    // ID 101: Red Flower
    registry.register(create_flower_red());
    // ID 102: Yellow Flower
    registry.register(create_flower_yellow());
    // ID 103: Lily Pad
    registry.register(create_lily_pad());
    // ID 104: Brown Mushroom
    registry.register(create_mushroom_brown());
    // ID 105: Red Mushroom
    registry.register(create_mushroom_red());

    // ID 106: Stalactite (stone)
    registry.register(create_stalactite());
    // ID 107: Stalagmite (stone)
    registry.register(create_stalagmite());
    // ID 108: Ice Stalactite
    registry.register(create_ice_stalactite());
    // ID 109: Ice Stalagmite
    registry.register(create_ice_stalagmite());

    // === Cave Vegetation (IDs 110-114) ===
    // ID 110: Moss Carpet (lush caves floor)
    registry.register(create_moss_carpet());
    // ID 111: Glow Lichen (ceiling/wall with emission)
    registry.register(create_glow_lichen());
    // ID 112: Hanging Roots (lush caves ceiling)
    registry.register(create_hanging_roots());
    // ID 113: Glow Berry Vines (lush caves ceiling with emission)
    registry.register(create_glow_berry_vines());
    // ID 114: Glow Mushroom Model (floor with emission)
    registry.register(create_glow_mushroom_model());

    // === Additional Surface Vegetation (IDs 115-118) ===
    // ID 115: Fern (taiga/jungle)
    registry.register(create_fern());
    // ID 116: Dead Bush (desert/savanna)
    registry.register(create_dead_bush());
    // ID 117: Seagrass (underwater)
    registry.register(create_seagrass());
    // ID 118: Blue Flower (cornflower)
    registry.register(create_flower_blue());

    // === Glass Panes (IDs 119-150) ===
    // ID 119-134: Horizontal glass panes (16 connection variants)
    // Connection bitmask: N=1, S=2, E=4, W=8
    for connections in 0..16u8 {
        registry.register(create_horizontal_glass_pane(connections));
    }
    // ID 135-150: Vertical glass panes (16 connection variants)
    // Rotatable: rotation 0=XY plane, rotation 1=YZ plane
    for connections in 0..16u8 {
        registry.register(create_vertical_glass_pane(connections));
    }

    // === Reserved/Placeholder (IDs 151-159) ===
    // These IDs are reserved for future use. We fill them with empty models
    // to ensure frame models get IDs 160-168.
    for i in 151..=159 {
        let mut placeholder = create_empty();
        placeholder.name = format!("reserved_{}", i);
        registry.register(placeholder);
    }

    // === Picture Frames (IDs 160-168) ===
    // 9 frame sizes from 1×1 to 3×3 blocks.
    // Each block of a multi-block frame uses the same model ID.
    // The shader uses block metadata to sample the correct picture region.
    registry.register(create_frame_1x1()); // ID 160: 1×1
    registry.register(create_frame_1x2()); // ID 161: 1×2
    registry.register(create_frame_1x3()); // ID 162: 1×3
    registry.register(create_frame_2x1()); // ID 163: 2×1
    registry.register(create_frame_2x2()); // ID 164: 2×2
    registry.register(create_frame_2x3()); // ID 165: 2×3
    registry.register(create_frame_3x1()); // ID 166: 3×1
    registry.register(create_frame_3x2()); // ID 167: 3×2
    registry.register(create_frame_3x3()); // ID 168: 3×3
}

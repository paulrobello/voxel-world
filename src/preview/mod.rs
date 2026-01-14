//! Preview module for shape tool preview updates.
//!
//! This module consolidates the preview update logic for all shape tools,
//! reducing code duplication and making it easier to add new tools.

use crate::app_state::UiState;
use crate::raycast::get_place_position;
use crate::shape_tools::PlacementMode;
use crate::world::World;
use nalgebra::Vector3;

/// Update all active shape tool previews based on the current raycast hit.
///
/// This is called each frame to update tool previews. Each tool checks if it's active
/// and updates its preview positions accordingly.
pub fn update_all_tool_previews(ui: &mut UiState, world: &World) {
    let current_hit = ui.current_hit;

    // Update stencil placement preview
    if let Some(ref mut placement) = ui.active_stencil_placement {
        if let Some(hit) = current_hit {
            let place_pos = get_place_position(&hit);
            placement.update_position_from_raycast(place_pos);
        }
    }

    // Update sphere tool preview
    if ui.sphere_tool.active {
        if let Some(hit) = current_hit {
            let target = calculate_placement_mode_target(&hit, ui.sphere_tool.placement_mode);
            ui.sphere_tool.update_preview(target);
        } else {
            ui.sphere_tool.clear_preview();
        }
    }

    // Update cube tool preview
    if ui.cube_tool.active {
        if let Some(hit) = current_hit {
            let target = calculate_placement_mode_target(&hit, ui.cube_tool.placement_mode);
            ui.cube_tool.update_preview(target);
        } else {
            ui.cube_tool.clear_preview();
        }
    }

    // Update cylinder tool preview
    if ui.cylinder_tool.active {
        if let Some(hit) = current_hit {
            let target = calculate_placement_mode_target(&hit, ui.cylinder_tool.placement_mode);
            ui.cylinder_tool.update_preview(target);
        } else {
            ui.cylinder_tool.clear_preview();
        }
    }

    // Update bridge tool preview (only when start position is set)
    if ui.bridge_tool.active && ui.bridge_tool.start_position.is_some() {
        if let Some(hit) = current_hit {
            let target = get_place_position(&hit);
            ui.bridge_tool.update_preview(target);
        } else {
            ui.bridge_tool.clear_preview();
        }
    }

    // Update wall tool preview (only when start position is set)
    if ui.wall_tool.active && ui.wall_tool.start_position.is_some() {
        if let Some(hit) = current_hit {
            let target = get_place_position(&hit);
            ui.wall_tool.update_preview(target);
        } else {
            ui.wall_tool.clear_preview();
        }
    }

    // Update floor tool preview (only when start position is set)
    if ui.floor_tool.active && ui.floor_tool.start_position.is_some() {
        if let Some(hit) = current_hit {
            let target = get_place_position(&hit);
            ui.floor_tool.update_preview(target);
        } else {
            ui.floor_tool.clear_preview();
        }
    }

    // Update stairs tool preview (only when start position is set)
    if ui.stairs_tool.active && ui.stairs_tool.start_pos.is_some() {
        if let Some(hit) = current_hit {
            let target = get_place_position(&hit);
            ui.stairs_tool.update_preview(target);
        } else {
            ui.stairs_tool.clear_preview();
        }
    }

    // Update circle tool preview
    if ui.circle_tool.active {
        if let Some(hit) = current_hit {
            let target = get_place_position(&hit);
            ui.circle_tool.update_preview(target);
        } else {
            ui.circle_tool.clear_preview();
        }
    }

    // Update arch tool preview
    if ui.arch_tool.active {
        if let Some(hit) = current_hit {
            let target = get_place_position(&hit);
            ui.arch_tool.update_preview(target);
        } else {
            ui.arch_tool.clear_preview();
        }
    }

    // Update cone tool preview
    if ui.cone_tool.active {
        if let Some(hit) = current_hit {
            let target = get_place_position(&hit);
            ui.cone_tool.update_preview(target);
        } else {
            ui.cone_tool.clear_preview();
        }
    }

    // Update torus tool preview
    if ui.torus_tool.active {
        if let Some(hit) = current_hit {
            let target = get_place_position(&hit);
            ui.torus_tool.update_preview(target);
        } else {
            ui.torus_tool.clear_preview();
        }
    }

    // Update helix tool preview
    if ui.helix_tool.active {
        if let Some(hit) = current_hit {
            let target = get_place_position(&hit);
            ui.helix_tool.update_preview(target);
        } else {
            ui.helix_tool.clear_preview();
        }
    }

    // Handle replace tool preview (requires world and selection)
    if ui.replace_tool.active && ui.replace_tool.preview_requested {
        ui.replace_tool.preview_requested = false;
        ui.replace_tool
            .update_preview(world, &ui.template_selection);
    }

    // Handle clone tool preview (requires selection and world)
    if ui.clone_tool.active {
        ui.clone_tool.update_preview(&ui.template_selection, world);
    }
}

/// Calculate target position for tools with PlacementMode.
///
/// In Base mode, the shape sits on top of the hit block.
/// In Center mode, the shape is centered at the placement position.
fn calculate_placement_mode_target(
    hit: &crate::raycast::RaycastHit,
    placement_mode: PlacementMode,
) -> Vector3<i32> {
    if placement_mode == PlacementMode::Base {
        // Base mode: shape bottom rests on top of hit block
        hit.block_pos + Vector3::new(0, 1, 0)
    } else {
        get_place_position(hit)
    }
}

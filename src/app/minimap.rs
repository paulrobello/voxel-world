use crate::app_state::{UiState, WorldSim};
use egui_winit_vulkano::egui;
use nalgebra::Vector3;
use std::time::Instant;

pub fn prepare_minimap_image(
    ui: &mut UiState,
    sim: &mut WorldSim,
    player_world_pos: Vector3<f64>,
    camera_yaw: f32,
) -> Option<egui::ColorImage> {
    if !ui.minimap_ui.show_minimap {
        return None;
    }

    let current_pos = Vector3::new(
        player_world_pos.x.floor() as i32,
        player_world_pos.y.floor() as i32,
        player_world_pos.z.floor() as i32,
    );
    // Check if player moved at least 1 block
    let moved = (current_pos.x - ui.minimap_ui.minimap_last_pos.x).abs() >= 1
        || (current_pos.z - ui.minimap_ui.minimap_last_pos.z).abs() >= 1;
    // Check if player rotated significantly (5 degrees) - only matters when rotate mode is on
    let yaw_changed =
        ui.minimap_ui.minimap.rotate && (camera_yaw - ui.minimap_ui.minimap_last_yaw).abs() > 0.087; // ~5 degrees
    // Check if enough time has passed (0.1 seconds for rotation, 0.5 for position)
    let time_elapsed = ui.minimap_ui.minimap_last_update.elapsed().as_secs_f32();
    let time_ok = if ui.minimap_ui.minimap.rotate {
        time_elapsed >= 0.1
    } else {
        time_elapsed >= 0.5
    };

    if ((moved || yaw_changed) && time_ok) || ui.minimap_ui.minimap_cached_image.is_none() {
        // Update last position/time/yaw and regenerate
        ui.minimap_ui.minimap_last_pos = current_pos;
        ui.minimap_ui.minimap_last_update = Instant::now();
        ui.minimap_ui.minimap_last_yaw = camera_yaw;
        let image = sim.world.generate_minimap_image(
            player_world_pos,
            camera_yaw,
            &ui.minimap_ui.minimap,
            &sim.terrain_generator,
        );
        ui.minimap_ui.minimap_cached_image = Some(image.clone());
        Some(image)
    } else {
        // Use cached image
        ui.minimap_ui.minimap_cached_image.clone()
    }
}

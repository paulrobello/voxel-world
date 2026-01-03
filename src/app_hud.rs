use crate::chunk::BlockType;
use crate::gpu_resources::RenderContext;
use crate::hud_render::{HUDRenderer, HudInputs};
use crate::{UiState, WorldSim};
use egui_winit_vulkano::egui;
use nalgebra::Vector3;

/// Render the HUD; returns true if render targets were recreated (matching HUDRenderer contract).
pub fn render_hud(
    rcx: &mut RenderContext,
    ui: &mut UiState,
    sim: &mut WorldSim,
    selected_block: BlockType,
    minimap_image: Option<egui::ColorImage>,
    camera_yaw: f32,
    player_world_pos: Vector3<f64>,
) -> bool {
    HUDRenderer.render(
        &mut rcx.gui,
        HudInputs {
            fps: ui.fps,
            chunk_stats: &sim.chunk_stats,
            player: &mut sim.player,
            world: &mut sim.world,
            settings: &mut ui.settings,
            render_mode: &mut sim.render_mode,
            current_hit: &ui.current_hit,
            selected_block,
            hotbar_index: &mut ui.hotbar_index,
            hotbar_blocks: &mut ui.hotbar_blocks,
            hotbar_model_ids: &mut ui.hotbar_model_ids,
            minimap_image,
            atlas_texture_id: rcx.atlas_texture_id,
            sprite_icons: Some(&rcx.sprite_icons),
            camera_yaw,
            player_world_pos,
            time_of_day: &mut sim.time_of_day,
            day_cycle_paused: &mut sim.day_cycle_paused,
            atmosphere: &mut sim.atmosphere,
            view_distance: &mut sim.view_distance,
            unload_distance: &mut sim.unload_distance,
            block_updates: &mut sim.block_updates,
            show_minimap: &mut ui.show_minimap,
            minimap: &mut ui.minimap,
            minimap_cached_image: &mut ui.minimap_cached_image,
            palette_open: &mut ui.palette_open,
            palette_tab: &mut ui.palette_tab,
            dragging_item: &mut ui.dragging_item,
            model_registry: &sim.model_registry,
        },
    )
}

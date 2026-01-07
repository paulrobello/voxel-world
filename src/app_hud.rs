use crate::chunk::BlockType;
use crate::editor::EditorAction;
use crate::editor::rasterizer::generate_model_sprite;
use crate::gpu_resources::RenderContext;
use crate::ui::{FluidStats, HUDRenderer, HudInputs};
use crate::{UiState, WorldSim};
use egui_winit_vulkano::egui;
use nalgebra::Vector3;
use std::path::Path;

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
    // Gather fluid stats for debug display
    let fluid_stats = FluidStats {
        water_cells: sim.water_grid.cell_count(),
        water_active: sim.water_grid.active_count(),
        lava_cells: sim.lava_grid.cell_count(),
        lava_active: sim.lava_grid.active_count(),
    };

    let (scale_changed, editor_action) = HUDRenderer.render(
        &mut rcx.gui,
        HudInputs {
            fps: ui.fps,
            chunk_stats: &sim.chunk_stats,
            fluid_stats,
            player: &mut sim.player,
            world: &mut sim.world,
            terrain_generator: &sim.terrain_generator,
            settings: &mut ui.settings,
            render_mode: &mut sim.render_mode,
            current_hit: &ui.current_hit,
            selected_block,
            hotbar_index: &mut ui.hotbar_index,
            hotbar_blocks: &mut ui.hotbar_blocks,
            hotbar_model_ids: &mut ui.hotbar_model_ids,
            hotbar_tint_indices: &mut ui.hotbar_tint_indices,
            hotbar_paint_textures: &mut ui.hotbar_paint_textures,
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
            editor: &mut ui.editor,
            console: &mut ui.console,
        },
    );

    // Handle editor action
    match editor_action {
        EditorAction::PlaceInWorld => {
            if let Some(pos) = ui.editor.saved_target_pos {
                // Register the model in the world's registry
                let model_id = sim.model_registry.register(ui.editor.scratch_pad.clone());

                // Calculate rotation to face player based on camera yaw
                let rot = (camera_yaw / std::f32::consts::FRAC_PI_2).round() as i32;
                let rotation = rot.rem_euclid(4) as u8;

                // Place the model block in the world
                sim.world.set_model_block(pos, model_id, rotation, false);
                sim.world.invalidate_minimap_cache(pos.x, pos.z);

                println!(
                    "[Editor] Placed model '{}' (ID {}) at {:?} with rotation {}",
                    ui.editor.scratch_pad.name, model_id, pos, rotation
                );

                // Close the editor
                ui.editor.active = false;
            }
        }
        EditorAction::ModelSaved => {
            // Register or update the model in the registry so it's available in palette
            let model_id = sim
                .model_registry
                .update_or_register(ui.editor.scratch_pad.clone());
            println!(
                "[Editor] Registered model '{}' as ID {} in palette",
                ui.editor.scratch_pad.name, model_id
            );

            // Generate sprite with the model ID and reload it in the HUD
            let sprites_dir = Path::new("textures/rendered");
            if let Err(e) = std::fs::create_dir_all(sprites_dir) {
                eprintln!("[Editor] Failed to create sprites directory: {}", e);
            } else {
                let sprite_path = sprites_dir.join(format!("model_{}.png", model_id));
                if let Err(e) = generate_model_sprite(&ui.editor.scratch_pad, &sprite_path) {
                    eprintln!("[Editor] Failed to generate sprite: {}", e);
                } else {
                    println!("[Editor] Generated sprite: {}", sprite_path.display());
                    // Reload the sprite in the HUD
                    let ctx = rcx.gui.context();
                    if rcx
                        .sprite_icons
                        .reload_model_sprite(&ctx, model_id, &sprite_path)
                    {
                        println!("[Editor] Reloaded HUD sprite for model {}", model_id);
                    } else {
                        eprintln!(
                            "[Editor] Failed to reload HUD sprite for model {}",
                            model_id
                        );
                    }
                }
            }
        }
        EditorAction::ModelDeleted => {
            // Reload custom models from library to update the palette
            let library_path = crate::user_prefs::user_models_dir();
            let old_count = sim.model_registry.custom_model_count();

            // Clear existing custom models and reload from library
            // We need to rebuild the registry with built-ins + library models
            sim.model_registry = crate::sub_voxel::ModelRegistry::new();
            match sim.model_registry.load_library_models(&library_path) {
                Ok(count) => {
                    println!(
                        "[Editor] Reloaded model registry: {} custom models ({} -> {})",
                        count, old_count, count
                    );
                }
                Err(e) => {
                    eprintln!("[Editor] Failed to reload library models: {}", e);
                }
            }

            // GPU resources will be updated automatically next frame since registry is dirty
        }
        EditorAction::ModelLoaded | EditorAction::None => {}
    }

    scale_changed
}

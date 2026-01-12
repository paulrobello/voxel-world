use crate::app_state::{UiState, WorldSim};
use crate::chunk::BlockType;
use crate::editor::EditorAction;
use crate::editor::rasterizer::generate_model_sprite;
use crate::gpu_resources::RenderContext;
use crate::stencils::{StencilBrowserAction, draw_stencil_browser};
use crate::templates::{TemplateBrowserAction, draw_save_template_dialog, draw_template_browser};
use crate::ui::{FluidStats, HUDRenderer, HudInputs, ToolAction};
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

    let (scale_changed, editor_action, tool_action) = HUDRenderer.render(
        &mut rcx.gui,
        HudInputs {
            fps: ui.fps,
            chunk_stats: &sim.chunk_stats,
            fluid_stats,
            player: &mut sim.player,
            world: &mut sim.world,
            terrain_generator: &sim.terrain_generator,
            cave_generator: sim.terrain_generator.cave_generator(),
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
            load_distance: &mut sim.load_distance,
            unload_distance: &mut sim.unload_distance,
            block_updates: &mut sim.block_updates,
            show_minimap: &mut ui.show_minimap,
            minimap: &mut ui.minimap,
            minimap_cached_image: &mut ui.minimap_cached_image,
            palette_open: &mut ui.palette_open,
            palette_tab: &mut ui.palette_tab,
            palette_search: &mut ui.palette_search,
            dragging_item: &mut ui.dragging_item,
            model_registry: &sim.model_registry,
            editor: &mut ui.editor,
            console: &mut ui.console,
            template_selection: &mut ui.template_selection,
            template_library: &ui.template_library,
            stencil_library: &ui.stencil_library,
            water_grid: &sim.water_grid,
            active_placement: &mut ui.active_placement,
            rangefinder_active: ui.rangefinder_active,
            flood_fill_active: ui.flood_fill_active,
            measurement_markers: &mut ui.measurement_markers,
            tools_palette: &mut ui.tools_palette,
            stencil_browser_open: ui.stencil_ui.browser_open,
        },
    );

    // Handle tool palette actions
    match tool_action {
        ToolAction::ToggleTemplateBrowser => {
            ui.template_ui.browser_open = !ui.template_ui.browser_open;
            if ui.template_ui.browser_open {
                ui.template_ui.refresh_templates(&ui.template_library);
            }
            // Close tools palette when opening template browser
            ui.tools_palette.open = false;
        }
        ToolAction::ToggleRangefinder => {
            ui.rangefinder_active = !ui.rangefinder_active;
            println!(
                "Rangefinder: {}",
                if ui.rangefinder_active { "ON" } else { "OFF" }
            );
            // Close tools palette and grab cursor when activating rangefinder
            if ui.rangefinder_active {
                ui.tools_palette.open = false;
                ui.request_cursor_grab = true;
            }
        }
        ToolAction::ToggleStencilBrowser => {
            ui.stencil_ui.browser_open = !ui.stencil_ui.browser_open;
            if ui.stencil_ui.browser_open {
                ui.stencil_ui.refresh_stencils(&ui.stencil_library);
            }
            // Close tools palette when opening stencil browser
            ui.tools_palette.open = false;
        }
        ToolAction::ToggleFloodFill => {
            ui.flood_fill_active = !ui.flood_fill_active;
            println!(
                "Flood Fill Mode: {}",
                if ui.flood_fill_active { "ON" } else { "OFF" }
            );
            // Close tools palette and grab cursor when activating fill mode
            if ui.flood_fill_active {
                ui.tools_palette.open = false;
                ui.request_cursor_grab = true;
            }
        }
        ToolAction::None => {}
    }

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

    // Render template browser UI
    let ctx = rcx.gui.context();
    let browser_action = draw_template_browser(
        &ctx,
        &mut ui.template_ui,
        &ui.template_selection,
        &ui.template_library,
    );

    // Handle template browser actions
    match browser_action {
        TemplateBrowserAction::OpenSaveDialog => {
            ui.template_ui.open_save_dialog("my_template");
        }
        TemplateBrowserAction::ClearSelection => {
            ui.template_selection.clear();
        }
        TemplateBrowserAction::LoadTemplate(name) => {
            match ui.template_library.load_template(&name) {
                Ok(template) => {
                    println!(
                        "Loaded template '{}' ({}×{}×{}, {} blocks)",
                        template.name,
                        template.width,
                        template.height,
                        template.depth,
                        template.block_count()
                    );

                    // Create placement at player position
                    let placement_pos = Vector3::new(
                        player_world_pos.x.floor() as i32,
                        (player_world_pos.y - 1.0).floor() as i32,
                        player_world_pos.z.floor() as i32,
                    );

                    let placement =
                        crate::templates::TemplatePlacement::new(template, placement_pos);
                    ui.active_placement = Some(placement);

                    // Close template browser after loading
                    ui.template_ui.browser_open = false;

                    // Request cursor grab to begin placement
                    ui.request_cursor_grab = true;

                    println!(
                        "Template placement ready. Use R to rotate, Right Click to confirm placement."
                    );
                }
                Err(e) => {
                    eprintln!("Failed to load template '{}': {}", name, e);
                }
            }
        }
        TemplateBrowserAction::DeleteTemplate(name) => {
            match ui.template_library.delete_template(&name) {
                Ok(_) => {
                    println!("Deleted template '{}'", name);
                    ui.template_ui.error_message = Some(format!("✓ Deleted template '{}'", name));
                    ui.template_ui.refresh_templates(&ui.template_library);
                }
                Err(e) => {
                    eprintln!("Failed to delete template '{}': {}", name, e);
                    ui.template_ui.error_message =
                        Some(format!("Failed to delete template: {}", e));
                }
            }
        }
        TemplateBrowserAction::SaveTemplate { name, tags } => {
            if let Some((min, max)) = ui.template_selection.bounds() {
                match ui.template_selection.validate_size() {
                    Ok(_) => {
                        let author = "Player".to_string(); // TODO: Get from user prefs
                        match crate::templates::VxtFile::from_world_region(
                            &sim.world,
                            &sim.water_grid,
                            name.clone(),
                            author,
                            min,
                            max,
                        ) {
                            Ok(mut template) => {
                                template.tags = tags;
                                match ui.template_library.save_template(&template) {
                                    Ok(_) => {
                                        // Generate thumbnail
                                        let thumbnail_path =
                                            ui.template_library.get_thumbnail_path(&name);
                                        if let Err(e) =
                                            crate::templates::rasterizer::generate_template_thumbnail(
                                                &template,
                                                &thumbnail_path,
                                            )
                                        {
                                            eprintln!(
                                                "[Template] Warning: Failed to generate thumbnail: {}",
                                                e
                                            );
                                        }

                                        println!("Saved template '{}'", name);
                                        ui.template_ui.error_message = Some(format!(
                                            "✓ Successfully saved template '{}' ({} blocks)",
                                            name,
                                            template.block_count()
                                        ));
                                        ui.template_ui.clear_thumbnail_cache(&name);
                                        ui.template_ui.refresh_templates(&ui.template_library);
                                    }
                                    Err(e) => {
                                        eprintln!("Failed to save template: {}", e);
                                        ui.template_ui.error_message =
                                            Some(format!("Failed to save template: {}", e));
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to create template: {}", e);
                                ui.template_ui.error_message =
                                    Some(format!("Failed to create template: {}", e));
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Invalid selection: {}", e);
                        ui.template_ui.error_message = Some(format!("Invalid selection: {}", e));
                    }
                }
            }
        }
        TemplateBrowserAction::RegenerateThumbnail(name) => {
            match ui.template_library.regenerate_thumbnail(&name) {
                Ok(_) => {
                    println!("Regenerated thumbnail for template '{}'", name);
                    ui.template_ui.error_message =
                        Some(format!("✓ Regenerated thumbnail for '{}'", name));
                    ui.template_ui.clear_thumbnail_cache(&name);
                    // Refresh template list to pick up new thumbnail path
                    ui.template_ui.refresh_templates(&ui.template_library);
                }
                Err(e) => {
                    eprintln!("Failed to regenerate thumbnail for '{}': {}", name, e);
                    ui.template_ui.error_message =
                        Some(format!("Failed to regenerate thumbnail: {}", e));
                }
            }
        }
        TemplateBrowserAction::ToStencil(name) => {
            // Load template and convert to stencil
            match ui.template_library.load_template(&name) {
                Ok(template) => {
                    let stencil = crate::stencils::StencilFile::from_template(&template, None);
                    let stencil_name = stencil.name.clone();
                    let position_count = stencil.position_count();

                    // Generate thumbnail
                    let thumbnail_path = ui.stencil_library.get_thumbnail_path(&stencil_name);
                    if let Err(e) = crate::stencils::rasterizer::generate_stencil_thumbnail(
                        &stencil,
                        &thumbnail_path,
                    ) {
                        eprintln!("[Stencil] Warning: Failed to generate thumbnail: {}", e);
                    }

                    // Save stencil
                    match ui.stencil_library.save_stencil(&stencil) {
                        Ok(_) => {
                            println!(
                                "Created stencil '{}' from template '{}' ({} positions)",
                                stencil_name, name, position_count
                            );
                            ui.template_ui.error_message = Some(format!(
                                "✓ Created stencil '{}' ({} positions)",
                                stencil_name, position_count
                            ));
                        }
                        Err(e) => {
                            eprintln!("Failed to save stencil '{}': {}", stencil_name, e);
                            ui.template_ui.error_message =
                                Some(format!("Failed to save stencil: {}", e));
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to load template '{}': {}", name, e);
                    ui.template_ui.error_message = Some(format!("Failed to load template: {}", e));
                }
            }
        }
        TemplateBrowserAction::None => {}
    }

    // Render save template dialog
    if let Some((name, tags)) = draw_save_template_dialog(&ctx, &mut ui.template_ui) {
        // User confirmed save in the dialog - trigger the actual save
        if let Some((min, max)) = ui.template_selection.bounds() {
            match ui.template_selection.validate_size() {
                Ok(_) => {
                    let author = "Player".to_string(); // TODO: Get from user prefs
                    match crate::templates::VxtFile::from_world_region(
                        &sim.world,
                        &sim.water_grid,
                        name.clone(),
                        author,
                        min,
                        max,
                    ) {
                        Ok(mut template) => {
                            template.tags = tags;
                            match ui.template_library.save_template(&template) {
                                Ok(_) => {
                                    // Generate thumbnail
                                    let thumbnail_path =
                                        ui.template_library.get_thumbnail_path(&name);
                                    if let Err(e) =
                                        crate::templates::rasterizer::generate_template_thumbnail(
                                            &template,
                                            &thumbnail_path,
                                        )
                                    {
                                        eprintln!(
                                            "[Template] Warning: Failed to generate thumbnail: {}",
                                            e
                                        );
                                    }

                                    println!("Saved template '{}'", name);
                                    ui.template_ui.error_message = Some(format!(
                                        "✓ Successfully saved template '{}' ({} blocks)",
                                        name,
                                        template.block_count()
                                    ));
                                    ui.template_ui.clear_thumbnail_cache(&name);
                                    ui.template_ui.refresh_templates(&ui.template_library);
                                }
                                Err(e) => {
                                    eprintln!("Failed to save template: {}", e);
                                    ui.template_ui.error_message =
                                        Some(format!("Failed to save template: {}", e));
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to create template: {}", e);
                            ui.template_ui.error_message =
                                Some(format!("Failed to create template: {}", e));
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Invalid selection: {}", e);
                }
            }
        }
    }

    // Render stencil browser UI
    let stencil_action = draw_stencil_browser(
        &ctx,
        &mut ui.stencil_ui,
        &ui.template_selection,
        &ui.stencil_library,
        &ui.stencil_manager,
    );

    // Handle stencil browser actions
    match stencil_action {
        StencilBrowserAction::OpenSaveDialog => {
            ui.stencil_ui.open_save_dialog("my_stencil");
        }
        StencilBrowserAction::ClearSelection => {
            ui.template_selection.clear();
        }
        StencilBrowserAction::SaveStencil { name, tags } => {
            if let Some((min, max)) = ui.template_selection.bounds() {
                match ui.template_selection.validate_size() {
                    Ok(_) => {
                        let author = "Player".to_string();
                        match crate::stencils::StencilFile::from_world_region(
                            &sim.world,
                            name.clone(),
                            author,
                            min,
                            max,
                        ) {
                            Ok(mut stencil) => {
                                stencil.tags = tags;
                                match ui.stencil_library.save_stencil(&stencil) {
                                    Ok(_) => {
                                        // Generate thumbnail
                                        let thumbnail_path =
                                            ui.stencil_library.get_thumbnail_path(&name);
                                        if let Err(e) =
                                            crate::stencils::rasterizer::generate_stencil_thumbnail(
                                                &stencil,
                                                &thumbnail_path,
                                            )
                                        {
                                            eprintln!(
                                                "[Stencil] Warning: Failed to generate thumbnail: {}",
                                                e
                                            );
                                        }

                                        println!("Saved stencil '{}'", name);
                                        ui.stencil_ui.error_message = Some(format!(
                                            "✓ Successfully saved stencil '{}' ({} positions)",
                                            name,
                                            stencil.positions.len()
                                        ));
                                        ui.stencil_ui.clear_thumbnail_cache(&name);
                                        ui.stencil_ui.refresh_stencils(&ui.stencil_library);
                                    }
                                    Err(e) => {
                                        eprintln!("Failed to save stencil: {}", e);
                                        ui.stencil_ui.error_message =
                                            Some(format!("Failed to save stencil: {}", e));
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to create stencil: {}", e);
                                ui.stencil_ui.error_message =
                                    Some(format!("Failed to create stencil: {}", e));
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Invalid selection: {}", e);
                        ui.stencil_ui.error_message = Some(format!("Invalid selection: {}", e));
                    }
                }
            }
        }
        StencilBrowserAction::LoadStencil(name) => {
            match ui.stencil_library.load_stencil(&name) {
                Ok(stencil) => {
                    println!(
                        "Loaded stencil '{}' ({}×{}×{}, {} positions)",
                        stencil.name,
                        stencil.width,
                        stencil.height,
                        stencil.depth,
                        stencil.positions.len()
                    );

                    // Start at player position (Y=0 sits ON the ground)
                    let placement_pos = nalgebra::Vector3::new(
                        player_world_pos.x.floor() as i32,
                        player_world_pos.y.floor() as i32,
                        player_world_pos.z.floor() as i32,
                    );

                    // Enter stencil placement mode
                    let placement =
                        crate::stencils::StencilPlacementMode::new(stencil, placement_pos);
                    ui.active_stencil_placement = Some(placement);

                    // Close stencil browser after loading
                    ui.stencil_ui.browser_open = false;

                    // Request cursor grab to begin placement
                    ui.request_cursor_grab = true;

                    println!(
                        "Stencil placement ready. Use R to rotate, Right Click to place, Escape to cancel."
                    );
                }
                Err(e) => {
                    eprintln!("Failed to load stencil '{}': {}", name, e);
                }
            }
        }
        StencilBrowserAction::DeleteStencil(name) => {
            match ui.stencil_library.delete_stencil(&name) {
                Ok(_) => {
                    println!("Deleted stencil '{}'", name);
                    ui.stencil_ui.error_message = Some(format!("✓ Deleted stencil '{}'", name));
                    ui.stencil_ui.refresh_stencils(&ui.stencil_library);
                }
                Err(e) => {
                    eprintln!("Failed to delete stencil '{}': {}", name, e);
                    ui.stencil_ui.error_message = Some(format!("Failed to delete stencil: {}", e));
                }
            }
        }
        StencilBrowserAction::RemoveActiveStencil(id) => {
            ui.stencil_manager.remove_stencil(id);
            println!("Removed active stencil {}", id);
        }
        StencilBrowserAction::RegenerateThumbnail(name) => {
            match ui.stencil_library.regenerate_thumbnail(&name) {
                Ok(_) => {
                    println!("Regenerated thumbnail for stencil '{}'", name);
                    ui.stencil_ui.error_message =
                        Some(format!("✓ Regenerated thumbnail for '{}'", name));
                    ui.stencil_ui.clear_thumbnail_cache(&name);
                    ui.stencil_ui.refresh_stencils(&ui.stencil_library);
                }
                Err(e) => {
                    eprintln!("Failed to regenerate thumbnail for '{}': {}", name, e);
                    ui.stencil_ui.error_message =
                        Some(format!("Failed to regenerate thumbnail: {}", e));
                }
            }
        }
        StencilBrowserAction::ClearAllActive => {
            let count = ui.stencil_manager.active_stencils.len();
            ui.stencil_manager.clear();
            println!("Cleared {} active stencils", count);
        }
        StencilBrowserAction::SetGlobalOpacity(opacity) => {
            ui.stencil_manager.set_global_opacity(opacity);
        }
        StencilBrowserAction::ToggleRenderMode => {
            ui.stencil_manager.toggle_render_mode();
        }
        StencilBrowserAction::None => {}
    }

    scale_changed
}

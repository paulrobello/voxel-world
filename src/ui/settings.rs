//! Settings window UI.

use super::time::{format_time, parse_time};
use crate::block_update::BlockUpdateQueue;
use crate::chunk::BlockType;
use crate::config::Settings;
use crate::player::Player;
use crate::raycast::RaycastHit;
use crate::render_mode::RenderMode;
use crate::sub_voxel::ModelRegistry;
use egui_winit_vulkano::egui;

pub struct SettingsUI;

impl SettingsUI {
    #[allow(clippy::too_many_arguments)]
    pub fn draw_settings_window(
        ctx: &egui::Context,
        settings: &mut Settings,
        render_mode: &mut RenderMode,
        current_hit: &Option<RaycastHit>,
        player: &mut Player,
        world: &mut crate::world::World,
        selected_block: BlockType,
        time_of_day: &mut f32,
        day_cycle_paused: &mut bool,
        atmosphere: &mut crate::atmosphere::AtmosphereSettings,
        view_distance: &mut i32,
        load_distance: &mut i32,
        unload_distance: &mut i32,
        block_updates: &mut BlockUpdateQueue,
        _model_registry: &ModelRegistry,
        minimap: &mut crate::hud::Minimap,
        show_minimap: &mut bool,
        minimap_cached_image: &mut Option<egui_winit_vulkano::egui::ColorImage>,
    ) -> bool {
        let mut scale_changed = false;

        egui::Window::new("Settings")
            .default_open(false)
            .default_pos(egui::pos2(10.0, 40.0))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(500.0)
                    .show(ui, |ui| {
                        ui.collapsing("Controls", |ui| {
                            ui.collapsing("Movement", |ui| {
                                ui.label("  WASD - Move");
                                ui.label("  Space - Jump");
                                ui.label("  Space/Shift - Up/Down (fly, swim & climb)");
                                ui.label("  Ctrl - Toggle sprint");
                                ui.label("  F - Toggle fly mode");
                                ui.label("  Mouse - Look around");
                            });

                            ui.collapsing("Building", |ui| {
                                ui.label("  Left Click - Break block");
                                ui.label("  Right Click - Place block");
                                ui.label("  Scroll - Select block");
                                ui.label("  1-9 - Select hotbar slot");
                                ui.label("  E - Open block/model palette");
                                ui.label("  N - Open model editor");
                                ui.label("  P - Repaint painted block");
                                ui.label("  [ ] - Cycle paint texture/stencil opacity");
                                ui.label("  , . - Cycle tint color");
                            });

                            ui.collapsing("Tools (T key)", |ui| {
                                ui.label("  T - Toggle tools palette");
                                ui.label("  V - Toggle selection mode");
                                ui.label("  G - Toggle rangefinder (distance)");
                                ui.label("  L - Open template library");
                                ui.label("  K - Open stencil browser");
                                ui.label("  R - Rotate template/stencil");
                                ui.label("  \\ - Toggle stencil render mode");
                            });

                            ui.collapsing("Measurements", |ui| {
                                ui.label("  G - Toggle rangefinder mode");
                                ui.label("  LClick - Place marker (up to 4)");
                                ui.label("  RClick - Remove last marker");
                                ui.label("  /measure clear - Remove all");
                            });

                            ui.collapsing("Templates & Stencils", |ui| {
                                ui.label("  V - Toggle selection mode");
                                ui.label("  L - Open template library");
                                ui.label("  K - Open stencil browser");
                                ui.label("  R - Rotate placement");
                                ui.label("  RClick - Confirm placement");
                                ui.label("  Esc - Cancel placement");
                                ui.label("Console commands:");
                                ui.label("  /select pos1|pos2 - Set corners");
                                ui.label("  /template save|load|list");
                                ui.label("  /stencil create|load|list");
                                ui.label("  /floodfill <block> - Mass replace");
                            });

                            ui.collapsing("UI & System", |ui| {
                                ui.label("  / - Open command console");
                                ui.label("  J - Toggle torch light");
                                ui.label("  M - Toggle minimap");
                                ui.label("  B - Toggle chunk boundaries");
                                ui.label("  C - Toggle debug cutaway");
                                ui.label("  Esc - Release cursor/close panels");
                                ui.label("  F12 - Screenshot");
                            });
                        });
                        ui.separator();

                        ui.label(format!("Chunks: {}", world.chunk_count()));
                        if player.in_water {
                            ui.colored_label(
                                egui::Color32::from_rgb(100, 150, 255),
                                "🌊 UNDERWATER",
                            );
                        }

                        ui.separator();

                        // Block selection
                        ui.label(format!("Selected: {:?}", selected_block));
                        if let Some(hit) = current_hit {
                            let block_type = world.get_block(hit.block_pos);
                            let block_name = block_type
                                .map(|b| format!("{:?}", b))
                                .unwrap_or_else(|| "Unknown".to_string());
                            ui.label(format!(
                                "Looking at: {} ({}, {}, {})",
                                block_name, hit.block_pos.x, hit.block_pos.y, hit.block_pos.z
                            ));
                            ui.label(format!("Distance: {:.1}", hit.distance));
                        } else {
                            ui.label("Looking at: (nothing)");
                        }

                        ui.separator();

                        // Debug render mode
                        ui.label("Render Mode:");
                        ui.horizontal(|ui| {
                            for &mode in RenderMode::ALL {
                                ui.selectable_value(render_mode, mode, format!("{:?}", mode));
                            }
                        });

                        ui.separator();

                        ui.add(egui::Slider::new(&mut player.camera.fov, 20.0..=120.0).text("FOV"));

                        if ui
                            .add(
                                egui::Slider::new(&mut settings.render_scale, 0.25..=1.5)
                                    .text("Render Scale"),
                            )
                            .changed()
                        {
                            scale_changed = true;
                        }

                        ui.separator();

                        // Day/night cycle controls
                        ui.label("Day/Night Cycle:");
                        ui.checkbox(day_cycle_paused, "Pause cycle");
                        let hours = (*time_of_day * 24.0) % 24.0;
                        let time_label = if hours < 6.0 {
                            "Night"
                        } else if hours < 9.0 {
                            "Sunrise"
                        } else if hours < 17.0 {
                            "Day"
                        } else if hours < 20.0 {
                            "Sunset"
                        } else {
                            "Night"
                        };
                        ui.add(
                            egui::Slider::new(time_of_day, 0.0..=1.0)
                                .text(time_label)
                                .custom_formatter(|v, _| format_time(v))
                                .custom_parser(parse_time),
                        );
                        ui.add(
                            egui::Slider::new(&mut atmosphere.ambient_light, 0.0..=1.0)
                                .text("Ambient Light"),
                        );

                        // Cloud settings in collapsible section
                        ui.collapsing("Clouds", |ui| {
                            ui.checkbox(&mut atmosphere.clouds_enabled, "Enable Clouds");

                            ui.add_enabled(
                                atmosphere.clouds_enabled,
                                egui::Slider::new(&mut atmosphere.cloud_speed, 0.0..=3.0)
                                    .text("Speed")
                                    .suffix("x"),
                            );

                            ui.add_enabled(
                                atmosphere.clouds_enabled,
                                egui::Slider::new(&mut atmosphere.cloud_coverage, 0.0..=1.0)
                                    .text("Coverage"),
                            );

                            ui.horizontal(|ui| {
                                ui.label("Cloud Color:");
                                if atmosphere.clouds_enabled {
                                    egui::color_picker::color_edit_button_rgb(
                                        ui,
                                        &mut atmosphere.cloud_color,
                                    );
                                } else {
                                    ui.add_enabled(false, egui::Button::new("   "));
                                }
                            });
                        });

                        // Fog settings in collapsible section
                        ui.collapsing("Fog", |ui| {
                            ui.add(
                                egui::Slider::new(&mut atmosphere.fog_density, 0.0..=0.1)
                                    .text("Density"),
                            );
                            ui.add(
                                egui::Slider::new(&mut atmosphere.fog_start, 0.0..=128.0)
                                    .text("Start Distance"),
                            );
                            ui.add(
                                egui::Slider::new(&mut atmosphere.fog_overlay_scale, 0.0..=2.0)
                                    .text("Overlay Scale"),
                            );
                        });

                        // Sky color settings in collapsible section
                        ui.collapsing("Sky Colors", |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Zenith (overhead):");
                                egui::color_picker::color_edit_button_rgb(
                                    ui,
                                    &mut atmosphere.sky_color_zenith,
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label("Horizon:");
                                egui::color_picker::color_edit_button_rgb(
                                    ui,
                                    &mut atmosphere.sky_color_horizon,
                                );
                            });
                            if ui.button("Reset to Default").clicked() {
                                atmosphere.sky_color_zenith = [0.25, 0.45, 0.85];
                                atmosphere.sky_color_horizon = [0.6, 0.75, 0.95];
                            }
                        });
                        if ui
                            .add(
                                egui::Slider::new(&mut settings.max_ray_steps, 128..=1024)
                                    .text("Ray Steps"),
                            )
                            .changed()
                        {
                            println!("[SETTING] Ray Steps: {}", settings.max_ray_steps);
                        }
                        if ui
                            .add(
                                egui::Slider::new(&mut settings.shadow_max_steps, 64..=256)
                                    .text("Shadow Steps"),
                            )
                            .changed()
                        {
                            println!("[SETTING] Shadow Steps: {}", settings.shadow_max_steps);
                        }
                        if ui
                            .add(egui::Slider::new(view_distance, 2..=10).text("View Distance"))
                            .on_hover_text("Distance at which chunks are rendered")
                            .changed()
                        {
                            println!("[SETTING] View Distance: {} chunks", *view_distance);
                            // Ensure load distance is at least view distance
                            if *load_distance < *view_distance {
                                *load_distance = *view_distance;
                            }
                            // Ensure unload distance is greater than load distance
                            if *unload_distance <= *load_distance {
                                *unload_distance = *load_distance + 2;
                            }
                        }
                        if ui
                            .add(egui::Slider::new(load_distance, 2..=12).text("Load Distance"))
                            .on_hover_text("Distance at which chunks are loaded (preload buffer)")
                            .changed()
                        {
                            println!("[SETTING] Load Distance: {} chunks", *load_distance);
                            // Ensure load distance is at least view distance
                            if *load_distance < *view_distance {
                                *load_distance = *view_distance;
                            }
                            // Ensure unload distance is greater than load distance
                            if *unload_distance <= *load_distance {
                                *unload_distance = *load_distance + 2;
                            }
                        }
                        if ui
                            .add(egui::Slider::new(unload_distance, 3..=14).text("Unload Distance"))
                            .on_hover_text("Distance at which chunks are unloaded")
                            .changed()
                        {
                            println!("[SETTING] Unload Distance: {} chunks", *unload_distance);
                            // Ensure unload distance is greater than load distance
                            if *unload_distance <= *load_distance {
                                *unload_distance = *load_distance + 2;
                            }
                        }

                        ui.separator();
                        ui.label("Feature Toggles:");
                        if ui
                            .checkbox(&mut settings.enable_ao, "Ambient Occlusion")
                            .changed()
                        {
                            println!(
                                "[TOGGLE] Ambient Occlusion: {}",
                                if settings.enable_ao { "ON" } else { "OFF" }
                            );
                        }
                        if ui
                            .checkbox(&mut settings.enable_shadows, "Sun Shadows")
                            .changed()
                        {
                            println!(
                                "[TOGGLE] Sun Shadows: {}",
                                if settings.enable_shadows { "ON" } else { "OFF" }
                            );
                        }
                        if ui
                            .checkbox(&mut settings.enable_model_shadows, "Model Sun Shadows")
                            .changed()
                        {
                            println!(
                                "[TOGGLE] Model Sun Shadows: {}",
                                if settings.enable_model_shadows {
                                    "ON"
                                } else {
                                    "OFF"
                                }
                            );
                        }
                        if ui
                            .checkbox(&mut settings.enable_point_lights, "Point Lights (torches)")
                            .changed()
                        {
                            println!(
                                "[TOGGLE] Point Lights: {}",
                                if settings.enable_point_lights {
                                    "ON"
                                } else {
                                    "OFF"
                                }
                            );
                        }
                        if ui
                            .checkbox(&mut settings.enable_tinted_shadows, "Tinted Glass Shadows")
                            .changed()
                        {
                            println!(
                                "[TOGGLE] Tinted Glass Shadows: {}",
                                if settings.enable_tinted_shadows {
                                    "ON"
                                } else {
                                    "OFF"
                                }
                            );
                        }
                        if ui
                            .checkbox(&mut settings.hide_ground_cover, "Hide Ground Cover")
                            .on_hover_text("Hide grass, flowers, mushrooms, and other small vegetation in the main view")
                            .changed()
                        {
                            println!(
                                "[TOGGLE] Hide Ground Cover: {}",
                                if settings.hide_ground_cover {
                                    "ON"
                                } else {
                                    "OFF"
                                }
                            );
                        }
                        if ui
                            .checkbox(
                                &mut settings.water_simulation_enabled,
                                "Water Flow Simulation",
                            )
                            .changed()
                        {
                            println!(
                                "[TOGGLE] Water Flow Simulation: {}",
                                if settings.water_simulation_enabled {
                                    "ON"
                                } else {
                                    "OFF"
                                }
                            );
                        }

                        ui.separator();

                        // LOD settings in collapsible section
                        ui.collapsing("LOD Distances (lower = faster)", |ui| {
                            ui.horizontal(|ui| {
                                ui.label("AO:");
                                if ui
                                    .add(
                                        egui::Slider::new(&mut settings.lod_ao_distance, 8.0..=128.0)
                                            .suffix(" blocks"),
                                    )
                                    .changed()
                                {
                                    println!("[LOD] AO distance: {:.0}", settings.lod_ao_distance);
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("Shadows:");
                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut settings.lod_shadow_distance,
                                            16.0..=128.0,
                                        )
                                        .suffix(" blocks"),
                                    )
                                    .changed()
                                {
                                    println!(
                                        "[LOD] Shadow distance: {:.0}",
                                        settings.lod_shadow_distance
                                    );
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("Lights:");
                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut settings.lod_point_light_distance,
                                            8.0..=48.0,
                                        )
                                        .suffix(" blocks"),
                                    )
                                    .changed()
                                {
                                    println!(
                                        "[LOD] Point light distance: {:.0}",
                                        settings.lod_point_light_distance
                                    );
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("Models:");
                                if ui
                                    .add(
                                        egui::Slider::new(&mut settings.lod_model_distance, 8.0..=128.0)
                                            .suffix(" blocks"),
                                    )
                                    .changed()
                                {
                                    println!(
                                        "[LOD] Model detail distance: {:.0}",
                                        settings.lod_model_distance
                                    );
                                }
                            });
                        });

                        // Light culling settings (CPU-side optimization)
                        ui.collapsing("Light Culling (CPU)", |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Cull Radius:");
                                if ui
                                    .add(
                                        egui::Slider::new(&mut settings.light_cull_radius, 16.0..=128.0)
                                            .suffix(" blocks"),
                                    )
                                    .on_hover_text("Lights beyond this distance are not sent to GPU")
                                    .changed()
                                {
                                    println!(
                                        "[LIGHTS] Cull radius: {:.0}",
                                        settings.light_cull_radius
                                    );
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("Max Active:");
                                if ui
                                    .add(
                                        egui::Slider::new(&mut settings.max_active_lights, 8..=256)
                                            .logarithmic(true),
                                    )
                                    .on_hover_text("Maximum lights sent to GPU per frame (closest prioritized)")
                                    .changed()
                                {
                                    println!(
                                        "[LIGHTS] Max active: {}",
                                        settings.max_active_lights
                                    );
                                }
                            });
                        });

                        ui.separator();

                        // Gameplay options
                        ui.checkbox(&mut player.auto_jump, "Auto-jump");
                        ui.checkbox(&mut settings.instant_break, "Instant block break");
                        ui.checkbox(&mut settings.instant_place, "Instant block place");
                        ui.checkbox(&mut settings.show_block_preview, "Block placement preview");
                        ui.checkbox(&mut settings.show_target_outline, "Target block outline");
                        if ui
                            .checkbox(&mut player.light_enabled, "Player torch light")
                            .changed()
                        {
                            println!(
                                "[TOGGLE] Player Light: {}",
                                if player.light_enabled { "ON" } else { "OFF" }
                            );
                        }

                        ui.add(
                            egui::Slider::new(&mut settings.break_cooldown_duration, 0.05..=0.5)
                                .text("Break cooldown")
                                .suffix("s"),
                        );
                        ui.add(
                            egui::Slider::new(&mut settings.place_cooldown_duration, 0.05..=1.0)
                                .text("Place cooldown")
                                .suffix("s"),
                        );

                        // Block physics updates per frame (higher = faster cascades, more CPU)
                        let mut max_updates = block_updates.max_per_frame as u32;
                        if ui
                            .add(
                                egui::Slider::new(&mut max_updates, 16..=128)
                                    .text("Physics updates/frame")
                                    .logarithmic(true),
                            )
                            .changed()
                        {
                            block_updates.max_per_frame = max_updates as usize;
                        }

                        ui.separator();

                        // Collision settings
                        ui.checkbox(&mut settings.collision_enabled_fly, "Collision (fly mode)");

                        ui.separator();

                        // HUD visibility
                        ui.checkbox(&mut settings.show_position, "Show position");
                        ui.checkbox(&mut settings.show_stats, "Show FPS/stats");

                        ui.separator();

                        // Minimap settings
                        ui.collapsing("Minimap (Toggle: M)", |ui| {
                        ui.checkbox(show_minimap, "Show minimap");
                        ui.checkbox(&mut settings.show_compass, "Show compass");

                        // Track previous values to detect changes
                        let prev_mode = minimap.mode;
                        let prev_zoom = minimap.zoom;

                        ui.horizontal(|ui| {
                            ui.label("Mode:");
                            ui.selectable_value(
                                &mut minimap.mode,
                                crate::hud::MinimapMode::Blocks,
                                "Blocks",
                            );
                            ui.selectable_value(
                                &mut minimap.mode,
                                crate::hud::MinimapMode::Height,
                                "Height",
                            );
                            ui.selectable_value(
                                &mut minimap.mode,
                                crate::hud::MinimapMode::Combined,
                                "Combined",
                            );
                        });

                        // Clear cache if mode changed
                        if minimap.mode != prev_mode {
                            *minimap_cached_image = None;
                        }

                        ui.checkbox(&mut minimap.rotate, "Rotate with player");
                        ui.add(
                            egui::Slider::new(&mut minimap.zoom, 0.25..=2.0)
                                .text("Zoom")
                                .logarithmic(true),
                        );

                        // Clear cache if zoom changed
                        if (minimap.zoom - prev_zoom).abs() > 0.001 {
                            *minimap_cached_image = None;
                        }

                        if ui
                            .checkbox(
                                &mut minimap.skip_decorative,
                                "Hide ground clutter (flowers, grass, torches)",
                            )
                            .on_hover_text("Improves performance by showing terrain under decorative models. Tree leaves remain visible as navigation landmarks.")
                            .changed()
                        {
                            // Clear cache when this setting changes
                            *minimap_cached_image = None;
                            world.clear_minimap_cache();
                            println!(
                                "[MINIMAP] Skip ground clutter: {}",
                                if minimap.skip_decorative { "ON" } else { "OFF" }
                            );
                        }
                        });

                        ui.separator();

                        // Camera position debug
                        ui.label(format!(
                            "Position: ({:.1}, {:.1}, {:.1})",
                            player.camera.position.x,
                            player.camera.position.y,
                            player.camera.position.z
                        ));

                        ui.separator();

                        // Window size
                        let screen = ui.ctx().screen_rect();
                        ui.label(format!(
                            "Window: {}x{}",
                            screen.width() as u32,
                            screen.height() as u32
                        ));
                    }); // end ScrollArea
            });

        scale_changed
    }
}

use crate::block_update::BlockUpdateQueue;
use crate::chunk::BlockType;
use crate::config::Settings;
use crate::hud::Minimap;
use crate::player::Player;
use crate::raycast::RaycastHit;
use crate::render_mode::RenderMode;
use crate::utils::ChunkStats;
use egui_winit_vulkano::{Gui, egui};
use nalgebra::Vector3;

/// Bundles HUD inputs to avoid an oversized render signature.
pub struct HudInputs<'a> {
    pub fps: u32,
    pub chunk_stats: &'a ChunkStats,
    pub player: &'a mut Player,
    pub world: &'a mut crate::world::World,
    pub settings: &'a mut Settings,
    pub render_mode: &'a mut RenderMode,
    pub current_hit: &'a Option<RaycastHit>,
    pub selected_block: BlockType,
    pub hotbar_index: usize,
    pub hotbar_blocks: &'a [BlockType; 9],
    pub hotbar_model_ids: &'a [u8; 9],
    pub minimap_image: Option<egui::ColorImage>,
    pub atlas_texture_id: egui::TextureId,
    pub camera_yaw: f32,
    pub player_world_pos: Vector3<f64>,
    pub time_of_day: &'a mut f32,
    pub day_cycle_paused: &'a mut bool,
    pub atmosphere: &'a mut crate::atmosphere::AtmosphereSettings,
    pub view_distance: &'a mut i32,
    pub unload_distance: &'a mut i32,
    pub block_updates: &'a mut BlockUpdateQueue,
    pub show_minimap: &'a mut bool,
    pub minimap: &'a mut Minimap,
    pub minimap_cached_image: &'a mut Option<egui::ColorImage>,
}

pub struct HUDRenderer;

impl HUDRenderer {
    fn overlay_frame() -> egui::Frame {
        egui::Frame::new()
            .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180))
            .corner_radius(egui::CornerRadius::same(4))
            .inner_margin(egui::Margin::symmetric(8, 4))
    }

    fn draw_stats_overlay(ctx: &egui::Context, fps: u32, chunk_stats: &ChunkStats) {
        egui::Area::new(egui::Id::new("fps_overlay"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 10.0))
            .show(ctx, |ui| {
                Self::overlay_frame().show(ui, |ui| {
                    ui.set_min_width(100.0);
                    ui.label(
                        egui::RichText::new(format!("FPS: {}", fps))
                            .color(egui::Color32::WHITE)
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new(format!("Chunks: {}", chunk_stats.loaded_count))
                            .color(egui::Color32::LIGHT_GRAY)
                            .small(),
                    );
                    if chunk_stats.dirty_count > 0 {
                        ui.label(
                            egui::RichText::new(format!("Dirty: {}", chunk_stats.dirty_count))
                                .color(egui::Color32::YELLOW)
                                .small(),
                        );
                    }
                    if chunk_stats.in_flight_count > 0 {
                        ui.label(
                            egui::RichText::new(format!(
                                "Generating: {}",
                                chunk_stats.in_flight_count
                            ))
                            .color(egui::Color32::LIGHT_GREEN)
                            .small(),
                        );
                    }
                    ui.label(
                        egui::RichText::new(format!("GPU: {:.1} MB", chunk_stats.memory_mb))
                            .color(egui::Color32::LIGHT_GRAY)
                            .small(),
                    );
                });
            });
    }

    fn draw_position_overlay(ctx: &egui::Context, player_world_pos: Vector3<f64>) {
        egui::Area::new(egui::Id::new("position_overlay"))
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 10.0))
            .show(ctx, |ui| {
                Self::overlay_frame()
                    .inner_margin(egui::Margin::symmetric(12, 6))
                    .show(ui, |ui| {
                        let pos_text = format!(
                            "Pos: {:.1}, {:.1}, {:.1}",
                            player_world_pos.x, player_world_pos.y, player_world_pos.z
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(pos_text)
                                    .color(egui::Color32::WHITE)
                                    .strong()
                                    .monospace(),
                            )
                            .wrap_mode(egui::TextWrapMode::Extend),
                        );
                    });
            });
    }

    pub fn render(&self, gui: &mut Gui, input: HudInputs<'_>) -> bool {
        let HudInputs {
            fps,
            chunk_stats,
            player,
            world,
            settings,
            render_mode,
            current_hit,
            selected_block,
            hotbar_index,
            hotbar_blocks,
            hotbar_model_ids,
            minimap_image,
            atlas_texture_id,
            camera_yaw,
            player_world_pos,
            time_of_day,
            day_cycle_paused,
            atmosphere,
            view_distance,
            unload_distance,
            block_updates,
            show_minimap,
            minimap,
            minimap_cached_image,
        } = input;
        let mut scale_changed = false;
        gui.immediate_ui(|gui| {
            let ctx = gui.context();

            Self::draw_stats_overlay(&ctx, fps, chunk_stats);
            Self::draw_position_overlay(&ctx, player_world_pos);

            egui::Window::new("Voxel Game")
                .default_open(false)
                .default_pos(egui::pos2(10.0, 40.0))
                .show(&ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(500.0)
                        .show(ui, |ui| {
                            ui.collapsing("Controls", |ui| {
                                ui.label("  WASD - Move");
                                ui.label("  Space - Jump");
                                ui.label("  Space/Shift - Up/Down (fly, swim & climb)");
                                ui.label("  Mouse - Look around");
                                ui.label("  Scroll - Select block");
                                ui.label("  Ctrl - Toggle sprint");
                                ui.label("  F - Toggle fly mode");
                                ui.label("  B - Toggle chunk boundaries");
                                ui.label("  Left Click - Break block");
                                ui.label("  Right Click - Place block");
                                ui.label("  1-9 - Select block type (8=Ladder, 9=Torch)");
                                ui.label("  Escape - Release cursor");
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

                            ui.add(
                                egui::Slider::new(&mut player.camera.fov, 20.0..=120.0).text("FOV"),
                            );

                            if ui
                                .add(
                                    egui::Slider::new(&mut settings.render_scale, 0.25..=2.0)
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
                            let time_label = match (*time_of_day * 4.0) as u32 {
                                0 => "Night",
                                1 => "Sunrise",
                                2 => "Day",
                                3 => "Sunset",
                                _ => "Day",
                            };
                            ui.add(
                                egui::Slider::new(time_of_day, 0.0..=1.0)
                                    .text(time_label)
                                    .custom_formatter(|v, _| {
                                        let hours = ((v * 24.0) + 6.0) % 24.0; // 0.0 = 6am, 0.5 = 6pm
                                        let h = hours as u32;
                                        let m = ((hours - h as f64) * 60.0) as u32;
                                        format!("{:02}:{:02}", h, m)
                                    }),
                            );
                            ui.add(
                                egui::Slider::new(&mut atmosphere.ambient_light, 0.0..=1.0)
                                    .text("Ambient Light"),
                            );
                            ui.add(
                                egui::Slider::new(&mut atmosphere.fog_density, 0.0..=0.1)
                                    .text("Fog Density"),
                            );
                            ui.add(
                                egui::Slider::new(&mut atmosphere.fog_start, 0.0..=128.0)
                                    .text("Fog Start"),
                            );
                            ui.add(
                                egui::Slider::new(&mut atmosphere.fog_overlay_scale, 0.0..=2.0)
                                    .text("Fog Overlay Scale"),
                            );
                            ui.checkbox(&mut atmosphere.fog_affects_sky, "Fog Affects Sky");
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
                                .add(egui::Slider::new(view_distance, 2..=10).text("View Distance"))
                                .changed()
                            {
                                println!("[SETTING] View Distance: {} chunks", *view_distance);
                                // Ensure unload distance is at least view distance + 1
                                if *unload_distance <= *view_distance {
                                    *unload_distance = *view_distance + 2;
                                }
                            }
                            if ui
                                .add(
                                    egui::Slider::new(unload_distance, 3..=12)
                                        .text("Unload Distance"),
                                )
                                .changed()
                            {
                                println!("[SETTING] Unload Distance: {} chunks", *unload_distance);
                                // Ensure unload distance is greater than view distance
                                if *unload_distance <= *view_distance {
                                    *unload_distance = *view_distance + 2;
                                }
                            }

                            ui.separator();
                            ui.label("Feature Toggles (for FPS profiling):");
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
                                .checkbox(
                                    &mut settings.enable_point_lights,
                                    "Point Lights (torches)",
                                )
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

                            ui.separator();
                            ui.label("LOD Distances (lower = faster):");
                            ui.horizontal(|ui| {
                                ui.label("AO:");
                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut settings.lod_ao_distance,
                                            8.0..=64.0,
                                        )
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

                            ui.separator();

                            // Gameplay options
                            ui.checkbox(&mut settings.instant_break, "Instant block break");
                            ui.checkbox(
                                &mut settings.show_block_preview,
                                "Block placement preview",
                            );
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
                                egui::Slider::new(
                                    &mut settings.break_cooldown_duration,
                                    0.05..=0.5,
                                )
                                .text("Break cooldown")
                                .suffix("s"),
                            );
                            ui.add(
                                egui::Slider::new(
                                    &mut settings.place_cooldown_duration,
                                    0.05..=1.0,
                                )
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

                            // Movement settings
                            ui.checkbox(&mut player.auto_jump, "Auto-jump");
                            ui.checkbox(&mut settings.show_compass, "Show compass");

                            ui.separator();

                            // Minimap settings
                            ui.label("Minimap");
                            if ui.checkbox(show_minimap, "Show minimap (M)").changed() {
                                println!("Minimap: {}", if *show_minimap { "ON" } else { "OFF" });
                            }

                            ui.horizontal(|ui| {
                                ui.label("Size:");
                                if ui.selectable_label(minimap.size == 128, "Small").clicked() {
                                    minimap.size = 128;
                                    *minimap_cached_image = None; // Force refresh
                                }
                                if ui.selectable_label(minimap.size == 192, "Medium").clicked() {
                                    minimap.size = 192;
                                    *minimap_cached_image = None; // Force refresh
                                }
                                if ui.selectable_label(minimap.size == 256, "Large").clicked() {
                                    minimap.size = 256;
                                    *minimap_cached_image = None; // Force refresh
                                }
                            });

                            ui.horizontal(|ui| {
                                ui.label("Colors:");
                                if ui
                                    .selectable_label(minimap.color_mode == 0, "Blocks")
                                    .clicked()
                                {
                                    minimap.color_mode = 0;
                                    *minimap_cached_image = None; // Force refresh
                                }
                                if ui
                                    .selectable_label(minimap.color_mode == 1, "Height")
                                    .clicked()
                                {
                                    minimap.color_mode = 1;
                                    *minimap_cached_image = None; // Force refresh
                                }
                                if ui
                                    .selectable_label(minimap.color_mode == 2, "Both")
                                    .clicked()
                                {
                                    minimap.color_mode = 2;
                                    *minimap_cached_image = None; // Force refresh
                                }
                            });

                            if ui
                                .add(
                                    egui::Slider::new(&mut minimap.zoom, 0.5..=3.0)
                                        .text("Zoom")
                                        .logarithmic(true),
                                )
                                .changed()
                            {
                                *minimap_cached_image = None; // Force refresh
                            }

                            if ui
                                .checkbox(&mut minimap.rotate, "Rotate with player")
                                .changed()
                            {
                                // Force minimap refresh when rotation mode changes
                                *minimap_cached_image = None;
                            }

                            ui.separator();

                            // Camera position debug
                            ui.label(format!(
                                "Position: ({:.1}, {:.1}, {:.1})",
                                player.camera.position.x,
                                player.camera.position.y,
                                player.camera.position.z
                            ));
                        }); // end ScrollArea
                });

            // Draw crosshair at screen center
            // Changes appearance when targeting a block
            let screen_rect = ctx.screen_rect();
            let center = screen_rect.center();
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("crosshair"),
            ));

            let targeting_block = current_hit.is_some();
            let (crosshair_size, crosshair_gap, crosshair_color) = if targeting_block {
                (12.0, 4.0, egui::Color32::from_rgb(100, 255, 100)) // Green, larger, with gap
            } else {
                (8.0, 0.0, egui::Color32::WHITE) // White, smaller, no gap
            };
            let stroke = egui::Stroke::new(2.0, crosshair_color);

            // Horizontal lines (with gap when targeting)
            painter.line_segment(
                [
                    egui::pos2(center.x - crosshair_size, center.y),
                    egui::pos2(center.x - crosshair_gap, center.y),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x + crosshair_gap, center.y),
                    egui::pos2(center.x + crosshair_size, center.y),
                ],
                stroke,
            );
            // Vertical lines (with gap when targeting)
            painter.line_segment(
                [
                    egui::pos2(center.x, center.y - crosshair_size),
                    egui::pos2(center.x, center.y - crosshair_gap),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x, center.y + crosshair_gap),
                    egui::pos2(center.x, center.y + crosshair_size),
                ],
                stroke,
            );

            // Minimap HUD (top-left)
            if *show_minimap {
                if let Some(image) = minimap_image {
                    // Load the pre-generated image as texture
                    let texture = ctx.load_texture("minimap", image, egui::TextureOptions::NEAREST);

                    egui::Area::new(egui::Id::new("minimap_hud"))
                        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-10.0, -60.0))
                        .show(&ctx, |ui| {
                            egui::Frame::new()
                                .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 200))
                                .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(60, 60, 60)))
                                .corner_radius(egui::CornerRadius::same(4))
                                .inner_margin(egui::Margin::same(4))
                                .show(ui, |ui| {
                                    let size = minimap.size as f32;
                                    let image_response = ui.add(
                                        egui::Image::new(egui::load::SizedTexture::new(
                                            texture.id(),
                                            egui::vec2(size, size),
                                        ))
                                        .fit_to_exact_size(egui::vec2(size, size)),
                                    );

                                    // Draw player indicator (triangle pointing in direction)
                                    let center = image_response.rect.center();
                                    let tri_size = 6.0;

                                    // Calculate triangle rotation angle
                                    let angle = if minimap.rotate {
                                        0.0 // Always point up when map rotates
                                    } else {
                                        -camera_yaw // Point in player's direction
                                    };

                                    // Triangle vertices: tip at front, two corners at back
                                    let (sin_a, cos_a) = (angle.sin(), angle.cos());
                                    let tip = egui::pos2(
                                        center.x - sin_a * tri_size,
                                        center.y - cos_a * tri_size,
                                    );
                                    let left = egui::pos2(
                                        center.x + cos_a * tri_size * 0.6 + sin_a * tri_size * 0.5,
                                        center.y - sin_a * tri_size * 0.6 + cos_a * tri_size * 0.5,
                                    );
                                    let right = egui::pos2(
                                        center.x - cos_a * tri_size * 0.6 + sin_a * tri_size * 0.5,
                                        center.y + sin_a * tri_size * 0.6 + cos_a * tri_size * 0.5,
                                    );

                                    ui.painter().add(egui::Shape::convex_polygon(
                                        vec![tip, left, right],
                                        egui::Color32::RED,
                                        egui::Stroke::new(1.0, egui::Color32::WHITE),
                                    ));
                                });
                        });
                }
            }

            // Compass HUD (bottom-left)
            if settings.show_compass {
                egui::Area::new(egui::Id::new("compass_hud"))
                    .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(10.0, -60.0))
                    .show(&ctx, |ui| {
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 200))
                            .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(60, 60, 60)))
                            .corner_radius(egui::CornerRadius::same(4))
                            .inner_margin(egui::Margin::same(8))
                            .show(ui, |ui| {
                                let compass_size = 60.0;
                                let (response, painter) = ui.allocate_painter(
                                    egui::vec2(compass_size, compass_size),
                                    egui::Sense::hover(),
                                );
                                let center = response.rect.center();
                                let radius = compass_size / 2.0 - 4.0;

                                // Draw compass circle
                                painter.circle_stroke(
                                    center,
                                    radius,
                                    egui::Stroke::new(1.5, egui::Color32::from_rgb(100, 100, 100)),
                                );

                                // Cardinal direction positions (N=-Z, S=+Z, E=+X, W=-X)
                                // In our coordinate system: yaw=0 looks at -Z (North)
                                let directions = [
                                    ("N", 0.0_f32, egui::Color32::RED), // North at yaw=0
                                    ("E", std::f32::consts::FRAC_PI_2, egui::Color32::WHITE), // East at yaw=90°
                                    ("S", std::f32::consts::PI, egui::Color32::WHITE), // South at yaw=180°
                                    ("W", -std::f32::consts::FRAC_PI_2, egui::Color32::WHITE), // West at yaw=-90°
                                ];

                                for (label, dir_angle, color) in directions {
                                    // Calculate angle relative to player's view
                                    // Player yaw: 0 = looking North (-Z)
                                    let relative_angle = dir_angle - camera_yaw;
                                    let (sin_a, cos_a) = relative_angle.sin_cos();

                                    // Position on compass (up = forward direction in player's view)
                                    let label_pos = egui::pos2(
                                        center.x + sin_a * (radius - 8.0),
                                        center.y - cos_a * (radius - 8.0),
                                    );

                                    painter.text(
                                        label_pos,
                                        egui::Align2::CENTER_CENTER,
                                        label,
                                        egui::FontId::proportional(12.0),
                                        color,
                                    );
                                }

                                // Draw direction indicator (line pointing up = forward)
                                painter.line_segment(
                                    [
                                        egui::pos2(center.x, center.y),
                                        egui::pos2(center.x, center.y - radius + 12.0),
                                    ],
                                    egui::Stroke::new(2.0, egui::Color32::YELLOW),
                                );
                                // Arrow head
                                painter.line_segment(
                                    [
                                        egui::pos2(center.x - 4.0, center.y - radius + 18.0),
                                        egui::pos2(center.x, center.y - radius + 12.0),
                                    ],
                                    egui::Stroke::new(2.0, egui::Color32::YELLOW),
                                );
                                painter.line_segment(
                                    [
                                        egui::pos2(center.x + 4.0, center.y - radius + 18.0),
                                        egui::pos2(center.x, center.y - radius + 12.0),
                                    ],
                                    egui::Stroke::new(2.0, egui::Color32::YELLOW),
                                );
                            });
                    });
            }

            // Hotbar HUD at bottom center - 9 slots
            const ATLAS_TILE_COUNT: f32 = 19.0;
            const SLOT_SIZE: f32 = 40.0;

            egui::Area::new(egui::Id::new("hotbar_hud"))
                .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -10.0))
                .show(&ctx, |ui| {
                    // Background frame for the whole hotbar
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180))
                        .corner_radius(egui::CornerRadius::same(4))
                        .inner_margin(egui::Margin::same(6))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);

                                for (i, block) in hotbar_blocks.iter().enumerate() {
                                    let is_selected = i == hotbar_index;

                                    // Calculate UV for this block
                                    // For Model blocks, use texture based on model type
                                    let block_idx = if *block == BlockType::Model {
                                        match hotbar_model_ids[i] {
                                            1 => 11.0,      // Torch
                                            4..=19 => 4.0,  // Fence -> use planks texture
                                            20..=27 => 4.0, // Gate -> use planks texture
                                            29 => 4.0,      // Ladder -> use planks texture
                                            _ => 11.0,      // Default to torch
                                        }
                                    } else {
                                        *block as u8 as f32
                                    };
                                    let uv_left = block_idx / ATLAS_TILE_COUNT;
                                    let uv_right = (block_idx + 1.0) / ATLAS_TILE_COUNT;
                                    let uv_rect = egui::Rect::from_min_max(
                                        egui::pos2(uv_left, 0.0),
                                        egui::pos2(uv_right, 1.0),
                                    );

                                    // Slot border color
                                    let border_color = if is_selected {
                                        egui::Color32::from_rgb(100, 255, 100)
                                    } else {
                                        egui::Color32::from_rgb(60, 60, 60)
                                    };
                                    let border_width = if is_selected { 3.0 } else { 1.0 };

                                    // Allocate space for slot
                                    let (rect, _response) = ui.allocate_exact_size(
                                        egui::vec2(SLOT_SIZE + 4.0, SLOT_SIZE + 16.0),
                                        egui::Sense::hover(),
                                    );

                                    // Draw slot background
                                    ui.painter().rect_filled(
                                        rect,
                                        egui::CornerRadius::same(2),
                                        egui::Color32::from_rgb(40, 40, 40),
                                    );

                                    // Draw texture
                                    let texture_rect = egui::Rect::from_min_size(
                                        rect.min + egui::vec2(2.0, 2.0),
                                        egui::vec2(SLOT_SIZE, SLOT_SIZE),
                                    );
                                    ui.painter().image(
                                        atlas_texture_id,
                                        texture_rect,
                                        uv_rect,
                                        egui::Color32::WHITE,
                                    );

                                    // Draw border
                                    ui.painter().rect_stroke(
                                        rect,
                                        egui::CornerRadius::same(2),
                                        egui::Stroke::new(border_width, border_color),
                                        egui::StrokeKind::Outside,
                                    );

                                    // Draw number label
                                    let text_pos = egui::pos2(rect.center().x, rect.max.y - 8.0);
                                    ui.painter().text(
                                        text_pos,
                                        egui::Align2::CENTER_CENTER,
                                        format!("{}", i + 1),
                                        egui::FontId::proportional(10.0),
                                        egui::Color32::WHITE,
                                    );
                                }
                            });

                            // Selected block name below hotbar
                            ui.vertical_centered(|ui| {
                                ui.add_space(4.0);
                                // For Model blocks, show the model type name
                                let block_name = if selected_block == BlockType::Model {
                                    match hotbar_model_ids[hotbar_index] {
                                        1 => "Torch".to_string(),
                                        4..=19 => "Fence".to_string(),
                                        20..=23 => "Gate (Closed)".to_string(),
                                        24..=27 => "Gate (Open)".to_string(),
                                        29 => "Ladder".to_string(),
                                        _ => format!("{:?}", selected_block),
                                    }
                                } else {
                                    format!("{:?}", selected_block)
                                };
                                ui.label(
                                    egui::RichText::new(block_name)
                                        .color(egui::Color32::WHITE)
                                        .strong(),
                                );
                            });
                        });
                });
        });
        scale_changed
    }
}

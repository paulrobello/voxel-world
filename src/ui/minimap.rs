//! Minimap and compass UI rendering.

use super::helpers::HudHelpers;
use crate::hud::Minimap;
use egui_winit_vulkano::egui;

pub struct MinimapUI;

impl MinimapUI {
    pub fn draw_minimap_and_compass(
        ctx: &egui::Context,
        show_minimap: &bool,
        minimap: &Minimap,
        minimap_image: Option<egui::ColorImage>,
        minimap_cached_image: &Option<egui::ColorImage>,
        camera_yaw: f32,
        show_compass: bool,
    ) {
        // Draw minimap
        if *show_minimap {
            if let Some(image) = minimap_image {
                // Load the pre-generated image as texture
                let texture = ctx.load_texture("minimap", image, egui::TextureOptions::NEAREST);

                egui::Area::new(egui::Id::new("minimap_hud"))
                    .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -12.0))
                    .show(ctx, |ui| {
                        egui::Frame::none()
                            .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 200))
                            .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(60, 60, 60)))
                            .rounding(egui::Rounding::same(4.0))
                            .inner_margin(egui::Margin::same(4.0))
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

        // Draw compass (independent of minimap settings)
        if show_compass {
            Self::draw_compass(ctx, camera_yaw);
        }
    }

    fn draw_compass(ctx: &egui::Context, camera_yaw: f32) {
        egui::Area::new(egui::Id::new("compass_hud"))
            .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(12.0, -12.0))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 200))
                    .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(60, 60, 60)))
                    .rounding(egui::Rounding::same(4.0))
                    .inner_margin(egui::Margin::same(8.0))
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
}

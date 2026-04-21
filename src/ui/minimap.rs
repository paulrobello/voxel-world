//! Minimap and compass UI rendering.

use crate::hud::Minimap;
use egui_winit_vulkano::egui;

/// Player colors for minimap markers.
/// Host (player_id 0) is always RED, then clients get Blue, Green, Purple, etc.
const PLAYER_COLORS: [egui::Color32; 8] = [
    egui::Color32::from_rgb(255, 50, 50),   // Red - Host
    egui::Color32::from_rgb(50, 100, 255),  // Blue - 1st client
    egui::Color32::from_rgb(50, 200, 50),   // Green - 2nd client
    egui::Color32::from_rgb(180, 50, 255),  // Purple - 3rd client
    egui::Color32::from_rgb(255, 200, 50),  // Gold - 4th client
    egui::Color32::from_rgb(255, 150, 200), // Pink - 5th client
    egui::Color32::from_rgb(50, 200, 200),  // Cyan - 6th client
    egui::Color32::from_rgb(255, 150, 50),  // Orange - 7th client
];

/// Remote player marker for minimap display.
#[allow(dead_code)]
pub struct RemotePlayerMarker {
    /// Player display name.
    pub name: String,
    /// World position (x, z).
    pub position: (f32, f32),
    /// Player ID (0 = host).
    pub player_id: u64,
}

/// Remote player data for 3D name label rendering.
#[allow(dead_code)]
pub struct RemotePlayerLabel {
    /// Player display name.
    pub name: String,
    /// World position (x, y, z).
    pub position: [f32; 3],
    /// Color index for this player (0-7).
    pub color_index: usize,
}

pub struct MinimapUI;

impl MinimapUI {
    #[allow(clippy::too_many_arguments)]
    pub fn draw_minimap_and_compass(
        ctx: &egui::Context,
        show_minimap: &bool,
        minimap: &Minimap,
        minimap_image: Option<egui::ColorImage>,
        _minimap_cached_image: &Option<egui::ColorImage>,
        camera_yaw: f32,
        show_compass: bool,
        biome_name: &str,
        player_world_pos: (f32, f32),
        remote_players: &[RemotePlayerMarker],
    ) {
        // Draw minimap
        if *show_minimap && let Some(image) = minimap_image {
            // Load the pre-generated image as texture
            let texture = ctx.load_texture("minimap", image, egui::TextureOptions::NEAREST);

            egui::Area::new(egui::Id::new("minimap_hud"))
                .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -12.0))
                .show(ctx, |ui| {
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

                            // Draw remote player markers as colored dots
                            let minimap_radius = size / 2.0;
                            let edge_offset = 8.0; // How far from the edge to draw perimeter markers
                            let inner_radius = minimap_radius - edge_offset;

                            for player in remote_players {
                                // Calculate relative position from local player
                                // Note: In this game, -Z is forward, but minimaps show forward at top
                                // So we negate dz to map -Z (forward) to +screen_y (top)
                                let dx = player.position.0 - player_world_pos.0;
                                let dz = -(player.position.1 - player_world_pos.1); // Negate for correct direction

                                // Scale based on minimap zoom
                                // At zoom 1.0, the minimap shows ~64 blocks radius
                                // Higher zoom = fewer blocks visible = larger scale
                                let base_range = 64.0;
                                let view_distance = base_range / minimap.zoom;
                                let scale = minimap_radius / view_distance;

                                // Rotate offset if minimap rotates
                                let (rel_x, rel_y) = if minimap.rotate {
                                    // When minimap rotates with camera, we need to rotate markers
                                    // in the opposite direction to keep world positions correct
                                    // This is equivalent to rotating by -camera_yaw
                                    let (sin_a, cos_a) = camera_yaw.sin_cos();
                                    // Rotate by -yaw: swap signs on sin terms
                                    let rx = dx * cos_a + dz * sin_a;
                                    let ry = -dx * sin_a + dz * cos_a;
                                    (rx, ry)
                                } else {
                                    (dx, dz)
                                };

                                // Convert to minimap coordinates
                                let marker_x = center.x + rel_x * scale;
                                let marker_y = center.y - rel_y * scale; // Y is inverted in screen coords

                                // Calculate distance from center in screen pixels
                                let screen_dist = ((marker_x - center.x).powi(2)
                                    + (marker_y - center.y).powi(2))
                                .sqrt();

                                let (final_x, final_y, is_perimeter) = if screen_dist < inner_radius
                                {
                                    // Player is within visible range - draw normally
                                    (marker_x, marker_y, false)
                                } else {
                                    // Player is outside visible range - clamp to perimeter
                                    let angle = (marker_y - center.y).atan2(marker_x - center.x);
                                    let perimeter_x = center.x + angle.cos() * inner_radius;
                                    let perimeter_y = center.y + angle.sin() * inner_radius;
                                    (perimeter_x, perimeter_y, true)
                                };

                                // Assign color based on player_id:
                                // Host (player_id 0) always gets red (index 0)
                                // Other players get colors 1-7 based on their player_id hash
                                let color_index = if player.player_id == 0 {
                                    0 // Host is always red
                                } else {
                                    // Use player_id hash to get a consistent color (1-7)
                                    ((player.player_id.wrapping_mul(0x5851F42E4C957F2D) % 7) + 1)
                                        as usize
                                };
                                let color = PLAYER_COLORS[color_index % PLAYER_COLORS.len()];

                                if is_perimeter {
                                    // Draw as a small arrow/triangle pointing outward for perimeter markers
                                    let angle = (final_y - center.y).atan2(final_x - center.x);
                                    let arrow_size = 5.0;

                                    // Triangle pointing away from center
                                    let tip = egui::pos2(
                                        final_x + angle.cos() * arrow_size,
                                        final_y + angle.sin() * arrow_size,
                                    );
                                    let left = egui::pos2(
                                        final_x + (angle + 2.5).cos() * arrow_size * 0.7,
                                        final_y + (angle + 2.5).sin() * arrow_size * 0.7,
                                    );
                                    let right = egui::pos2(
                                        final_x + (angle - 2.5).cos() * arrow_size * 0.7,
                                        final_y + (angle - 2.5).sin() * arrow_size * 0.7,
                                    );

                                    ui.painter().add(egui::Shape::convex_polygon(
                                        vec![tip, left, right],
                                        color,
                                        egui::Stroke::new(1.0, egui::Color32::WHITE),
                                    ));
                                } else {
                                    // Draw as a circle for in-range players
                                    let dot_radius = 4.0;

                                    // Draw colored dot with outline
                                    ui.painter().circle_filled(
                                        egui::pos2(final_x, final_y),
                                        dot_radius,
                                        color,
                                    );
                                    ui.painter().circle_stroke(
                                        egui::pos2(final_x, final_y),
                                        dot_radius,
                                        egui::Stroke::new(1.0, egui::Color32::WHITE),
                                    );
                                }
                            }

                            // Biome name label below the minimap
                            ui.add_space(2.0);
                            ui.label(
                                egui::RichText::new(biome_name)
                                    .color(egui::Color32::from_gray(220))
                                    .size(11.0),
                            );
                        });
                });
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
}

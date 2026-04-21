//! Bezier curve placement tool UI.
//!
//! Provides a settings window for configuring bezier curve parameters
//! and showing control point status.

use egui_winit_vulkano::egui;

use crate::shape_tools::bezier::BezierToolState;

/// UI for the bezier curve placement tool.
pub struct BezierToolUI;

impl BezierToolUI {
    /// Draw the bezier tool settings window.
    pub fn draw(ctx: &egui::Context, state: &mut BezierToolState) {
        if !state.active {
            return;
        }

        egui::Window::new("Bezier Curve Tool")
            .default_pos(egui::pos2(ctx.screen_rect().width() - 240.0, 100.0))
            .default_size(egui::vec2(220.0, 320.0))
            .resizable(false)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.heading("Bezier Settings");
                ui.add_space(8.0);

                // Tube radius input
                ui.horizontal(|ui| {
                    ui.label("Tube Radius:");
                    ui.add(
                        egui::DragValue::new(&mut state.tube_radius)
                            .range(1..=10)
                            .suffix(" blocks"),
                    );
                });
                ui.add(egui::Slider::new(&mut state.tube_radius, 1..=10).show_value(false));

                ui.add_space(4.0);

                // Resolution input
                ui.horizontal(|ui| {
                    ui.label("Resolution:");
                    ui.add(egui::DragValue::new(&mut state.resolution).range(1..=5));
                });
                ui.add(egui::Slider::new(&mut state.resolution, 1..=5).show_value(false));
                ui.small("Higher = smoother curve");

                ui.add_space(12.0);

                // Separator
                ui.separator();

                // Control points status
                ui.label(
                    egui::RichText::new("Control Points")
                        .strong()
                        .color(egui::Color32::from_rgb(200, 200, 255)),
                );
                ui.add_space(4.0);

                // Show control points
                for (i, point) in state.control_points.iter().enumerate() {
                    let color = match i {
                        0 => egui::Color32::from_rgb(0, 255, 255),  // Cyan
                        1 => egui::Color32::from_rgb(255, 77, 255), // Magenta
                        2 => egui::Color32::from_rgb(255, 255, 77), // Yellow
                        3 => egui::Color32::from_rgb(255, 128, 51), // Orange
                        _ => egui::Color32::WHITE,
                    };
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("P{}", i + 1))
                                .color(color)
                                .strong(),
                        );
                        ui.label(format!("({}, {}, {})", point.x, point.y, point.z));
                    });
                }

                // Show status message
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(state.status_message())
                        .color(egui::Color32::from_rgb(200, 255, 200))
                        .size(12.0),
                );

                ui.add_space(8.0);

                // Preview info
                ui.horizontal(|ui| {
                    ui.label("Blocks:");
                    ui.label(format!("{}", state.total_blocks));
                });

                // Truncation warning
                if state.preview_truncated {
                    ui.colored_label(egui::Color32::YELLOW, "Preview truncated (>4096 blocks)");
                }

                ui.add_space(8.0);

                // Buttons
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !state.control_points.is_empty(),
                            egui::Button::new("Undo Point"),
                        )
                        .on_hover_text("Remove last control point (Backspace)")
                        .clicked()
                    {
                        state.remove_last_point();
                    }

                    if ui
                        .button("Clear")
                        .on_hover_text("Clear all points")
                        .clicked()
                    {
                        state.clear();
                    }
                });

                ui.add_space(4.0);

                // Instructions
                ui.label(
                    egui::RichText::new("Right-click to place points")
                        .size(11.0)
                        .color(egui::Color32::from_gray(180)),
                );
                if state.has_curve() {
                    ui.label(
                        egui::RichText::new("Enter to confirm and place")
                            .size(11.0)
                            .color(egui::Color32::from_rgb(100, 255, 100)),
                    );
                }
                ui.label(
                    egui::RichText::new("Left-click to cancel")
                        .size(11.0)
                        .color(egui::Color32::from_gray(180)),
                );
                ui.small("Uses selected hotbar block");
            });
    }
}

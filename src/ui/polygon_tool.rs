//! Polygon (n-gon) placement tool UI.
//!
//! Provides a settings window for configuring regular polygon placement
//! parameters including number of sides, radius, height, hollow mode, and rotation.

use egui_winit_vulkano::egui;

use crate::shape_tools::polygon::PolygonToolState;

/// UI for the polygon placement tool.
pub struct PolygonToolUI;

impl PolygonToolUI {
    /// Draw the polygon tool settings window.
    ///
    /// Placement happens via right-click, not through UI buttons.
    pub fn draw(ctx: &egui::Context, state: &mut PolygonToolState) {
        if !state.active {
            return;
        }

        egui::Window::new("Polygon Tool")
            .default_pos(egui::pos2(ctx.screen_rect().width() - 240.0, 100.0))
            .default_size(egui::vec2(220.0, 340.0))
            .resizable(false)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.heading(format!("{} Settings", state.polygon_name()));
                ui.add_space(8.0);

                // Number of sides
                ui.horizontal(|ui| {
                    ui.label("Sides:");
                    ui.add(egui::DragValue::new(&mut state.sides).range(3..=12));
                });
                ui.add(egui::Slider::new(&mut state.sides, 3..=12).show_value(false));

                ui.add_space(4.0);

                // Radius input
                ui.horizontal(|ui| {
                    ui.label("Radius:");
                    ui.add(
                        egui::DragValue::new(&mut state.radius)
                            .range(2..=50)
                            .suffix(" blocks"),
                    );
                });
                ui.add(egui::Slider::new(&mut state.radius, 2..=50).show_value(false));

                ui.add_space(4.0);

                // Height input (for prism)
                ui.horizontal(|ui| {
                    ui.label("Height:");
                    ui.add(
                        egui::DragValue::new(&mut state.height)
                            .range(1..=100)
                            .suffix(" blocks"),
                    );
                });
                ui.add(egui::Slider::new(&mut state.height, 1..=100).show_value(false));

                ui.add_space(4.0);

                // Rotation input
                ui.horizontal(|ui| {
                    ui.label("Rotation:");
                    ui.add(
                        egui::DragValue::new(&mut state.rotation)
                            .range(0..=360)
                            .suffix("\u{00B0}"),
                    );
                });
                ui.add(egui::Slider::new(&mut state.rotation, 0..=360).show_value(false));

                ui.add_space(8.0);

                // Hollow checkbox
                ui.checkbox(&mut state.hollow, "Hollow");
                ui.small(if state.height == 1 {
                    "Outline only"
                } else {
                    "Shell (walls + caps)"
                });

                ui.add_space(12.0);

                // Separator
                ui.separator();

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

                // Cancel button
                ui.horizontal(|ui| {
                    if ui.button("Cancel (Esc)").clicked() {
                        state.deactivate();
                    }
                });

                ui.add_space(4.0);

                // Instructions
                ui.label("Right-click to place polygon");
                ui.small("Uses selected hotbar block");
            });
    }
}

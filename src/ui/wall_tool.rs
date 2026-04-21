//! Wall placement tool UI.
//!
//! Provides a settings window for configuring wall placement parameters
//! including thickness and manual height override.

use egui_winit_vulkano::egui;

use crate::shape_tools::WallToolState;

/// UI for the wall placement tool.
pub struct WallToolUI;

impl WallToolUI {
    /// Draw the wall tool settings window.
    ///
    /// The wall uses a two-click workflow: first click sets start, second click places wall.
    pub fn draw(ctx: &egui::Context, state: &mut WallToolState) {
        if !state.active {
            return;
        }

        egui::Window::new("Wall Tool")
            .default_pos(egui::pos2(ctx.screen_rect().width() - 220.0, 100.0))
            .default_size(egui::vec2(200.0, 220.0))
            .resizable(false)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.heading("Wall Settings");
                ui.add_space(8.0);

                // Thickness input
                ui.horizontal(|ui| {
                    ui.label("Thickness:");
                    ui.add(
                        egui::DragValue::new(&mut state.thickness)
                            .range(1..=5)
                            .suffix(" blocks"),
                    );
                });
                ui.add(egui::Slider::new(&mut state.thickness, 1..=5).show_value(false));

                ui.add_space(8.0);

                // Manual height toggle
                ui.checkbox(&mut state.use_manual_height, "Manual height");

                // Height slider (only enabled when manual height is on)
                ui.add_enabled_ui(state.use_manual_height, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Height:");
                        ui.add(
                            egui::DragValue::new(&mut state.height_value)
                                .range(1..=100)
                                .suffix(" blocks"),
                        );
                    });
                    ui.add(egui::Slider::new(&mut state.height_value, 1..=100).show_value(false));
                });

                ui.add_space(8.0);
                ui.separator();

                // Status info
                if state.start_position.is_some() {
                    ui.colored_label(egui::Color32::GREEN, "Start corner set");
                    ui.small("Right-click to place wall at end corner");

                    // Show preview dimensions
                    if let (Some(start), Some(end)) = (state.start_position, state.preview_end) {
                        let (length, height, thick) =
                            crate::shape_tools::wall::calculate_dimensions(
                                start,
                                end,
                                state.thickness,
                                state.effective_manual_height(),
                            );
                        ui.horizontal(|ui| {
                            ui.label("Dimensions:");
                            ui.label(format!("{}L × {}H × {}T", length, height, thick));
                        });
                    }
                } else {
                    ui.small("Right-click to set start corner");
                }

                ui.add_space(4.0);

                // Block count
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
                    if state.start_position.is_some() && ui.button("Clear Start").clicked() {
                        state.cancel();
                    }
                    if ui.button("Cancel (Esc)").clicked() {
                        state.deactivate();
                    }
                });

                ui.add_space(4.0);

                // Instructions
                ui.small("Uses selected hotbar block");
            });
    }
}

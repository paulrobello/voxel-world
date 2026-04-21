//! Stairs tool settings UI.
//!
//! Provides a settings window for configuring the stairs generator parameters
//! including width and displaying staircase dimensions.

use egui_winit_vulkano::egui;

use crate::shape_tools::StairsToolState;

/// Stairs tool UI renderer.
pub struct StairsToolUI;

impl StairsToolUI {
    /// Draw the stairs tool settings window.
    ///
    /// Two-click workflow: first right-click sets start, second right-click places stairs.
    pub fn draw(ctx: &egui::Context, state: &mut StairsToolState) {
        if !state.active {
            return;
        }

        egui::Window::new("Stairs Tool")
            .default_pos(egui::pos2(ctx.screen_rect().width() - 220.0, 100.0))
            .default_size(egui::vec2(200.0, 200.0))
            .resizable(false)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.heading("Staircase Generator");
                ui.add_space(8.0);

                // Width slider
                ui.horizontal(|ui| {
                    ui.label("Width:");
                    ui.add(
                        egui::DragValue::new(&mut state.width)
                            .range(1..=5)
                            .suffix(" blocks"),
                    );
                });
                ui.add(egui::Slider::new(&mut state.width, 1..=5).show_value(false));

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // Status display
                if let Some(start) = state.start_pos {
                    ui.horizontal(|ui| {
                        ui.label("Start:");
                        ui.label(format!("({}, {}, {})", start.x, start.y, start.z));
                    });

                    if state.total_blocks > 0 {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label("Steps:");
                            ui.label(format!("{}", state.step_count));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Blocks:");
                            ui.label(format!("{}", state.total_blocks));
                        });
                        if state.preview_truncated {
                            ui.colored_label(egui::Color32::YELLOW, "Preview truncated (>4096)");
                        }
                    }

                    ui.add_space(4.0);
                    ui.small("Right-click to place stairs");
                    ui.small("Escape to clear start");
                } else {
                    ui.colored_label(egui::Color32::YELLOW, "Click to set start");
                    ui.add_space(4.0);
                    ui.small("Right-click on ground to set start point");
                }

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                // Buttons
                ui.horizontal(|ui| {
                    if state.start_pos.is_some() && ui.button("Clear Start").clicked() {
                        state.reset();
                    }
                    if ui.button("Cancel (Esc)").clicked() {
                        state.deactivate();
                    }
                });

                ui.add_space(8.0);

                // Instructions
                ui.heading("Usage");
                ui.add_space(4.0);
                ui.small("1. Right-click to set start (bottom)");
                ui.small("2. Right-click to set end (top)");
                ui.small("3. Stairs placed automatically");
            });
    }
}

//! Floor placement tool UI.
//!
//! Provides a settings window for configuring floor/platform placement parameters
//! including thickness and direction (floor vs ceiling).

use egui_winit_vulkano::egui;

use crate::shape_tools::FloorToolState;
use crate::shape_tools::floor::FloorDirection;

/// UI for the floor placement tool.
pub struct FloorToolUI;

impl FloorToolUI {
    /// Draw the floor tool settings window.
    ///
    /// The floor uses a two-click workflow: first click sets start corner, second click places floor.
    pub fn draw(ctx: &egui::Context, state: &mut FloorToolState) {
        if !state.active {
            return;
        }

        egui::Window::new("Floor Tool")
            .default_pos(egui::pos2(ctx.screen_rect().width() - 220.0, 100.0))
            .default_size(egui::vec2(200.0, 200.0))
            .resizable(false)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.heading("Floor Settings");
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

                // Direction selection
                ui.horizontal(|ui| {
                    ui.label("Direction:");
                    egui::ComboBox::from_id_salt("floor_direction")
                        .selected_text(state.direction.name())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut state.direction,
                                FloorDirection::Floor,
                                "Floor (builds down)",
                            );
                            ui.selectable_value(
                                &mut state.direction,
                                FloorDirection::Ceiling,
                                "Ceiling (builds up)",
                            );
                        });
                });

                ui.add_space(8.0);
                ui.separator();

                // Status info
                if state.start_position.is_some() {
                    ui.colored_label(egui::Color32::GREEN, "Start corner set");
                    ui.small("Right-click to place floor at end corner");

                    // Show preview dimensions
                    if let (Some(start), Some(end)) = (state.start_position, state.preview_end) {
                        let (length, width, thick) =
                            crate::shape_tools::floor::calculate_dimensions(
                                start,
                                end,
                                state.thickness,
                            );
                        ui.horizontal(|ui| {
                            ui.label("Dimensions:");
                            ui.label(format!("{}L × {}W × {}T", length, width, thick));
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

//! Bridge (line) placement tool UI.
//!
//! Provides a status window for the bridge tool showing start position
//! and block count preview.

use egui_winit_vulkano::egui;

use crate::shape_tools::BridgeToolState;

/// UI for the bridge placement tool.
pub struct BridgeToolUI;

impl BridgeToolUI {
    /// Draw the bridge tool status window.
    ///
    /// First right-click sets start position, second right-click draws the line.
    pub fn draw(ctx: &egui::Context, state: &mut BridgeToolState) {
        if !state.active {
            return;
        }

        egui::Window::new("Bridge Tool")
            .default_pos(egui::pos2(ctx.screen_rect().width() - 220.0, 100.0))
            .default_size(egui::vec2(200.0, 160.0))
            .resizable(false)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.heading("Bridge (Line)");
                ui.add_space(8.0);

                // Show start position status
                if let Some(start) = state.start_position {
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::GREEN, "Start:");
                        ui.label(format!("({}, {}, {})", start.x, start.y, start.z));
                    });

                    // Show block count if we have a preview
                    if state.total_blocks > 0 {
                        ui.horizontal(|ui| {
                            ui.label("Blocks:");
                            ui.label(format!("{}", state.total_blocks));
                        });
                    }

                    ui.add_space(8.0);
                    ui.label("Right-click to set end point");
                    ui.small("and place the bridge");
                } else {
                    ui.colored_label(egui::Color32::YELLOW, "No start point set");
                    ui.add_space(8.0);
                    ui.label("Right-click to set start point");
                }

                ui.add_space(12.0);

                // Separator
                ui.separator();

                // Cancel button
                ui.horizontal(|ui| {
                    if ui.button("Cancel (Esc)").clicked() {
                        state.deactivate();
                    }
                    if state.start_position.is_some() && ui.button("Clear Start").clicked() {
                        state.cancel();
                    }
                });

                ui.add_space(4.0);
                ui.small("Uses selected hotbar block");
            });
    }
}

//! Cube placement tool UI.
//!
//! Provides a settings window for configuring cube placement parameters
//! including size, hollow mode, dome mode, and placement mode.

use egui_winit_vulkano::egui;

use crate::shape_tools::{CubeToolState, PlacementMode};

/// UI for the cube placement tool.
pub struct CubeToolUI;

impl CubeToolUI {
    /// Draw the cube tool settings window.
    ///
    /// Placement happens via right-click, not through UI buttons.
    pub fn draw(ctx: &egui::Context, state: &mut CubeToolState) {
        if !state.active {
            return;
        }

        egui::Window::new("Cube Tool")
            .default_pos(egui::pos2(ctx.screen_rect().width() - 220.0, 100.0))
            .default_size(egui::vec2(200.0, 300.0))
            .resizable(false)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.heading("Cube Settings");
                ui.add_space(8.0);

                // Size inputs with drag values and sliders
                ui.label("Size (half-extents):");
                ui.add_space(4.0);

                // X size
                ui.horizontal(|ui| {
                    ui.label("X:");
                    ui.add(
                        egui::DragValue::new(&mut state.size_x)
                            .range(1..=50)
                            .suffix(" blocks"),
                    );
                });
                ui.add(egui::Slider::new(&mut state.size_x, 1..=50).show_value(false));

                // Y size
                ui.horizontal(|ui| {
                    ui.label("Y:");
                    ui.add(
                        egui::DragValue::new(&mut state.size_y)
                            .range(1..=50)
                            .suffix(" blocks"),
                    );
                });
                ui.add(egui::Slider::new(&mut state.size_y, 1..=50).show_value(false));

                // Z size
                ui.horizontal(|ui| {
                    ui.label("Z:");
                    ui.add(
                        egui::DragValue::new(&mut state.size_z)
                            .range(1..=50)
                            .suffix(" blocks"),
                    );
                });
                ui.add(egui::Slider::new(&mut state.size_z, 1..=50).show_value(false));

                ui.add_space(8.0);

                // Actual dimensions display
                let width = state.size_x * 2 + 1;
                let height = state.size_y * 2 + 1;
                let depth = state.size_z * 2 + 1;
                ui.label(format!("Dimensions: {}x{}x{}", width, height, depth));

                ui.add_space(8.0);

                // Hollow checkbox
                ui.checkbox(&mut state.hollow, "Hollow (shell)");

                // Dome checkbox
                ui.checkbox(&mut state.dome, "Dome (top half only)");

                ui.add_space(8.0);

                // Placement mode
                ui.label("Placement Mode:");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut state.placement_mode, PlacementMode::Center, "Center")
                        .on_hover_text("Cube center at target position");
                    ui.selectable_value(&mut state.placement_mode, PlacementMode::Base, "Base")
                        .on_hover_text("Cube bottom rests on target surface");
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
                ui.label("Right-click to place cube");
                ui.small("Uses selected hotbar block");
            });
    }
}

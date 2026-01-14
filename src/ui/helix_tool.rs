//! Helix (spiral) placement tool UI.
//!
//! Provides a settings window for configuring helix placement parameters
//! including radius, height, turns, tube thickness, and direction.

use egui_winit_vulkano::egui;

use crate::shape_tools::helix::{HelixDirection, HelixToolState};

/// UI for the helix placement tool.
pub struct HelixToolUI;

impl HelixToolUI {
    /// Draw the helix tool settings window.
    ///
    /// Placement happens via right-click, not through UI buttons.
    pub fn draw(ctx: &egui::Context, state: &mut HelixToolState) {
        if !state.active {
            return;
        }

        egui::Window::new("Helix Tool")
            .default_pos(egui::pos2(ctx.screen_rect().width() - 240.0, 100.0))
            .default_size(egui::vec2(220.0, 380.0))
            .resizable(false)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.heading("Helix Settings");
                ui.add_space(8.0);

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

                // Height input
                ui.horizontal(|ui| {
                    ui.label("Height:");
                    ui.add(
                        egui::DragValue::new(&mut state.height)
                            .range(5..=200)
                            .suffix(" blocks"),
                    );
                });
                ui.add(egui::Slider::new(&mut state.height, 5..=200).show_value(false));

                ui.add_space(4.0);

                // Turns input
                ui.horizontal(|ui| {
                    ui.label("Turns:");
                    ui.add(
                        egui::DragValue::new(&mut state.turns)
                            .range(0.5..=20.0)
                            .speed(0.1)
                            .fixed_decimals(1),
                    );
                });
                ui.add(egui::Slider::new(&mut state.turns, 0.5..=20.0).show_value(false));

                ui.add_space(4.0);

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

                ui.add_space(8.0);

                // Direction
                ui.horizontal(|ui| {
                    ui.label("Direction:");
                    if ui
                        .selectable_label(state.direction == HelixDirection::Clockwise, "CW")
                        .on_hover_text("Clockwise")
                        .clicked()
                    {
                        state.direction = HelixDirection::Clockwise;
                    }
                    if ui
                        .selectable_label(
                            state.direction == HelixDirection::CounterClockwise,
                            "CCW",
                        )
                        .on_hover_text("Counter-clockwise")
                        .clicked()
                    {
                        state.direction = HelixDirection::CounterClockwise;
                    }
                });

                ui.add_space(4.0);

                // Start angle input
                ui.horizontal(|ui| {
                    ui.label("Start Angle:");
                    ui.add(
                        egui::DragValue::new(&mut state.start_angle)
                            .range(0..=360)
                            .suffix("\u{00B0}"),
                    );
                });
                ui.add(egui::Slider::new(&mut state.start_angle, 0..=360).show_value(false));

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

                // Instructions
                ui.label("Right-click to place helix");
                ui.label("Left-click to cancel");
                ui.small("Uses selected hotbar block");
            });
    }
}

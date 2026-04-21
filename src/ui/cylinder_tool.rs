//! Cylinder placement tool UI.
//!
//! Provides a settings window for configuring cylinder placement parameters
//! including radius, height, hollow mode, axis orientation, and placement mode.

use egui_winit_vulkano::egui;

use crate::shape_tools::cylinder::CylinderAxis;
use crate::shape_tools::{CylinderToolState, PlacementMode};

/// UI for the cylinder placement tool.
pub struct CylinderToolUI;

impl CylinderToolUI {
    /// Draw the cylinder tool settings window.
    ///
    /// Placement happens via right-click, not through UI buttons.
    pub fn draw(ctx: &egui::Context, state: &mut CylinderToolState) {
        if !state.active {
            return;
        }

        egui::Window::new("Cylinder Tool")
            .default_pos(egui::pos2(ctx.screen_rect().width() - 220.0, 100.0))
            .default_size(egui::vec2(200.0, 300.0))
            .resizable(false)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.heading("Cylinder Settings");
                ui.add_space(8.0);

                // Radius input with drag value and slider
                ui.horizontal(|ui| {
                    ui.label("Radius:");
                    ui.add(
                        egui::DragValue::new(&mut state.radius)
                            .range(1..=50)
                            .suffix(" blocks"),
                    );
                });
                ui.add(egui::Slider::new(&mut state.radius, 1..=50).show_value(false));

                ui.add_space(4.0);

                // Height input with drag value and slider
                ui.horizontal(|ui| {
                    ui.label("Height:");
                    ui.add(
                        egui::DragValue::new(&mut state.height)
                            .range(1..=100)
                            .suffix(" blocks"),
                    );
                });
                ui.add(egui::Slider::new(&mut state.height, 1..=100).show_value(false));

                ui.add_space(8.0);

                // Hollow checkbox
                ui.checkbox(&mut state.hollow, "Hollow (tube)");

                ui.add_space(8.0);

                // Axis orientation
                ui.label("Orientation:");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut state.axis, CylinderAxis::Y, "Vertical")
                        .on_hover_text("Cylinder stands upright (Y axis)");
                    ui.selectable_value(&mut state.axis, CylinderAxis::X, "X-axis")
                        .on_hover_text("Cylinder lies east-west");
                    ui.selectable_value(&mut state.axis, CylinderAxis::Z, "Z-axis")
                        .on_hover_text("Cylinder lies north-south");
                });

                ui.add_space(8.0);

                // Placement mode
                ui.label("Placement Mode:");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut state.placement_mode, PlacementMode::Center, "Center")
                        .on_hover_text("Cylinder center at target position");
                    ui.selectable_value(&mut state.placement_mode, PlacementMode::Base, "Base")
                        .on_hover_text("Cylinder bottom rests on target surface");
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
                ui.label("Right-click to place cylinder");
                ui.small("Uses selected hotbar block");
            });
    }
}

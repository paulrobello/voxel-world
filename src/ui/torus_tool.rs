//! Torus (ring/donut) placement tool UI.
//!
//! Provides a settings window for configuring torus placement parameters
//! including major/minor radius, orientation plane, arc angle, and hollow mode.

use egui_winit_vulkano::egui;

use crate::shape_tools::PlacementMode;
use crate::shape_tools::torus::{TorusPlane, TorusToolState};

/// UI for the torus placement tool.
pub struct TorusToolUI;

impl TorusToolUI {
    /// Draw the torus tool settings window.
    ///
    /// Placement happens via right-click, not through UI buttons.
    pub fn draw(ctx: &egui::Context, state: &mut TorusToolState) {
        if !state.active {
            return;
        }

        egui::Window::new("Torus Tool")
            .default_pos(egui::pos2(ctx.screen_rect().width() - 240.0, 100.0))
            .default_size(egui::vec2(220.0, 320.0))
            .resizable(false)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.heading("Torus Settings");
                ui.add_space(8.0);

                // Major radius input
                ui.horizontal(|ui| {
                    ui.label("Major Radius:");
                    ui.add(
                        egui::DragValue::new(&mut state.major_radius)
                            .range(2..=50)
                            .suffix(" blocks"),
                    );
                });
                ui.add(egui::Slider::new(&mut state.major_radius, 2..=50).show_value(false));

                ui.add_space(4.0);

                // Minor radius input
                ui.horizontal(|ui| {
                    ui.label("Minor Radius:");
                    ui.add(
                        egui::DragValue::new(&mut state.minor_radius)
                            .range(1..=20)
                            .suffix(" blocks"),
                    );
                });
                ui.add(egui::Slider::new(&mut state.minor_radius, 1..=20).show_value(false));

                ui.add_space(8.0);

                // Orientation plane
                ui.label("Orientation:");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut state.plane, TorusPlane::XZ, "XZ")
                        .on_hover_text("Horizontal ring (like a table)");
                    ui.selectable_value(&mut state.plane, TorusPlane::XY, "XY")
                        .on_hover_text("Vertical ring facing Z");
                    ui.selectable_value(&mut state.plane, TorusPlane::YZ, "YZ")
                        .on_hover_text("Vertical ring facing X");
                });

                ui.add_space(8.0);

                // Arc angle
                ui.horizontal(|ui| {
                    ui.label("Arc Angle:");
                    ui.add(
                        egui::DragValue::new(&mut state.arc_angle)
                            .range(1..=360)
                            .suffix("\u{00B0}"),
                    );
                });
                ui.add(egui::Slider::new(&mut state.arc_angle, 1..=360).show_value(false));

                ui.add_space(8.0);

                // Placement mode
                ui.label("Placement Mode:");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut state.placement_mode, PlacementMode::Center, "Center")
                        .on_hover_text("Torus centered at target");
                    ui.selectable_value(&mut state.placement_mode, PlacementMode::Base, "Base")
                        .on_hover_text("Torus bottom rests on target");
                });

                ui.add_space(8.0);

                // Hollow checkbox
                ui.checkbox(&mut state.hollow, "Hollow tube");

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
                ui.label("Right-click to place torus");
                ui.label("Left-click to cancel");
                ui.small("Uses selected hotbar block");
            });
    }
}

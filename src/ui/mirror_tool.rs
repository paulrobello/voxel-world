//! Mirror tool settings UI.
//!
//! Provides a settings window for configuring the mirror/symmetry tool parameters
//! including axis selection and mirror plane position.

use egui_winit_vulkano::egui;

use crate::shape_tools::MirrorToolState;
use crate::shape_tools::mirror::MirrorAxis;

/// Mirror tool UI renderer.
pub struct MirrorToolUI;

impl MirrorToolUI {
    /// Draw the mirror tool settings window.
    ///
    /// The mirror plane is set by right-clicking while the tool is active.
    pub fn draw(ctx: &egui::Context, state: &mut MirrorToolState) {
        if !state.active {
            return;
        }

        egui::Window::new("Mirror Tool")
            .default_pos(egui::pos2(ctx.screen_rect().width() - 220.0, 100.0))
            .default_size(egui::vec2(200.0, 200.0))
            .resizable(false)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.heading("Symmetric Building");
                ui.add_space(8.0);

                // Axis selection with buttons
                ui.label("Mirror Axis:");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut state.axis, MirrorAxis::X, "X (E-W)")
                        .on_hover_text("Mirror across X axis (East-West symmetry)");
                    ui.selectable_value(&mut state.axis, MirrorAxis::Z, "Z (N-S)")
                        .on_hover_text("Mirror across Z axis (North-South symmetry)");
                    ui.selectable_value(&mut state.axis, MirrorAxis::Both, "Both")
                        .on_hover_text("Mirror across both axes (4-way symmetry)");
                });

                ui.add_space(8.0);

                // Show plane toggle
                ui.checkbox(&mut state.show_plane, "Show mirror plane");

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // Mirror plane status
                if state.plane_set {
                    ui.colored_label(egui::Color32::LIGHT_GREEN, "Mirror active");
                    ui.horizontal(|ui| {
                        ui.label("Plane at:");
                        ui.label(format!(
                            "({}, {}, {})",
                            state.plane_position.x, state.plane_position.y, state.plane_position.z
                        ));
                    });

                    ui.add_space(4.0);
                    ui.small("Place/break blocks to mirror them");

                    ui.add_space(4.0);

                    if ui.button("Clear plane").clicked() {
                        state.clear_plane();
                    }
                } else {
                    ui.colored_label(egui::Color32::YELLOW, "No plane set");
                    ui.small("Right-click to set mirror plane");
                }

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                // Cancel button
                if ui.button("Cancel (Esc)").clicked() {
                    state.deactivate();
                }

                ui.add_space(8.0);

                // Instructions
                ui.heading("Usage");
                ui.add_space(4.0);
                ui.small("1. Select mirror axis above");
                ui.small("2. Right-click to set mirror plane");
                ui.small("3. Place/break blocks normally");
                ui.small("4. Actions are mirrored automatically");
            });
    }
}

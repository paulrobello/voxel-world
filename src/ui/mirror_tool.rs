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

                // Axis selection dropdown
                ui.horizontal(|ui| {
                    ui.label("Axis:");
                    egui::ComboBox::from_id_salt("mirror_axis")
                        .selected_text(state.axis.name())
                        .show_ui(ui, |ui| {
                            for axis in MirrorAxis::all() {
                                ui.selectable_value(&mut state.axis, *axis, axis.name());
                            }
                        });
                });

                ui.add_space(8.0);

                // Show plane toggle
                ui.checkbox(&mut state.show_plane, "Show mirror plane");

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // Mirror plane status
                if state.plane_set {
                    ui.horizontal(|ui| {
                        ui.label("Plane at:");
                        ui.label(format!(
                            "({}, {}, {})",
                            state.plane_position.x, state.plane_position.y, state.plane_position.z
                        ));
                    });

                    ui.add_space(4.0);

                    if ui.button("Clear plane").clicked() {
                        state.clear_plane();
                    }
                } else {
                    ui.colored_label(egui::Color32::YELLOW, "No plane set");
                    ui.small("Right-click to set plane");
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
                ui.small("1. Right-click to set mirror plane");
                ui.small("2. Place blocks normally");
                ui.small("3. Blocks are mirrored automatically");
                ui.add_space(4.0);
                ui.small("Tab: Cycle axis");
            });
    }
}

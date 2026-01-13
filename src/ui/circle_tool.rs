//! Circle/Ellipse tool settings UI.
//!
//! Provides a settings window for configuring the circle/ellipse tool parameters
//! including radius, ellipse mode, fill mode, and orientation plane.

use egui_winit_vulkano::egui;

use crate::shape_tools::CircleToolState;
use crate::shape_tools::PlacementMode;
use crate::shape_tools::circle::CirclePlane;

/// Circle/Ellipse tool UI renderer.
pub struct CircleToolUI;

impl CircleToolUI {
    /// Draw the circle/ellipse tool settings window.
    ///
    /// Placement happens via right-click, not through UI buttons.
    pub fn draw(ctx: &egui::Context, state: &mut CircleToolState) {
        if !state.active {
            return;
        }

        egui::Window::new("Circle Tool")
            .default_pos(egui::pos2(ctx.screen_rect().width() - 220.0, 100.0))
            .default_size(egui::vec2(200.0, 280.0))
            .resizable(false)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.heading("Circle / Ellipse");
                ui.add_space(8.0);

                // Mode toggle: Circle vs Ellipse
                ui.horizontal(|ui| {
                    ui.label("Mode:");
                    ui.selectable_value(&mut state.ellipse_mode, false, "Circle");
                    ui.selectable_value(&mut state.ellipse_mode, true, "Ellipse");
                });

                ui.add_space(8.0);

                // Radius A (primary)
                let radius_label = if state.ellipse_mode {
                    "Radius A:"
                } else {
                    "Radius:"
                };
                ui.horizontal(|ui| {
                    ui.label(radius_label);
                    ui.add(
                        egui::DragValue::new(&mut state.radius_a)
                            .range(1..=50)
                            .suffix(" blocks"),
                    );
                });
                ui.add(egui::Slider::new(&mut state.radius_a, 1..=50).show_value(false));

                // Radius B (secondary, only in ellipse mode)
                if state.ellipse_mode {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("Radius B:");
                        ui.add(
                            egui::DragValue::new(&mut state.radius_b)
                                .range(1..=50)
                                .suffix(" blocks"),
                        );
                    });
                    ui.add(egui::Slider::new(&mut state.radius_b, 1..=50).show_value(false));
                }

                ui.add_space(8.0);

                // Fill mode toggle
                ui.horizontal(|ui| {
                    ui.label("Fill:");
                    ui.selectable_value(&mut state.filled, true, "Filled");
                    ui.selectable_value(&mut state.filled, false, "Outline");
                });

                ui.add_space(8.0);

                // Orientation plane dropdown
                ui.horizontal(|ui| {
                    ui.label("Plane:");
                    egui::ComboBox::from_id_salt("circle_plane")
                        .selected_text(state.plane.name())
                        .show_ui(ui, |ui| {
                            for plane in CirclePlane::all() {
                                ui.selectable_value(&mut state.plane, *plane, plane.name());
                            }
                        });
                });

                // Placement mode (only shown for wall modes)
                if state.is_wall_mode() {
                    ui.add_space(8.0);
                    ui.label("Placement:");
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut state.placement_mode,
                            PlacementMode::Center,
                            "Center",
                        )
                        .on_hover_text("Circle center at target position");
                        ui.selectable_value(&mut state.placement_mode, PlacementMode::Base, "Base")
                            .on_hover_text("Circle bottom rests on target surface");
                    });
                }

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                // Info display
                if state.total_blocks > 0 {
                    ui.horizontal(|ui| {
                        ui.label("Blocks:");
                        ui.label(format!("{}", state.total_blocks));
                    });
                    if state.preview_truncated {
                        ui.colored_label(egui::Color32::YELLOW, "Preview truncated (>4096)");
                    }
                }

                ui.add_space(8.0);

                // Cancel button
                if ui.button("Cancel (Esc)").clicked() {
                    state.deactivate();
                }

                ui.add_space(4.0);
                ui.small("Right-click to place");
            });
    }
}

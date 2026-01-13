//! Cone/Pyramid tool settings UI.
//!
//! Provides a settings window for configuring cone and pyramid placement
//! parameters including base size, height, shape type, hollow, and inverted modes.

use egui_winit_vulkano::egui;

use crate::shape_tools::ConeToolState;
use crate::shape_tools::cone::ConeShape;

/// Cone tool UI renderer.
pub struct ConeToolUI;

impl ConeToolUI {
    /// Draw the cone tool settings window.
    pub fn draw(ctx: &egui::Context, state: &mut ConeToolState) {
        if !state.active {
            return;
        }

        egui::Window::new("Cone/Pyramid Tool")
            .default_pos(egui::pos2(ctx.screen_rect().width() - 220.0, 100.0))
            .default_size(egui::vec2(200.0, 320.0))
            .resizable(false)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.heading("Cone/Pyramid Builder");
                ui.add_space(8.0);

                // Shape selection
                ui.horizontal(|ui| {
                    ui.label("Shape:");
                    egui::ComboBox::from_id_salt("cone_shape")
                        .selected_text(state.shape.name())
                        .show_ui(ui, |ui| {
                            for shape in ConeShape::all() {
                                ui.selectable_value(&mut state.shape, *shape, shape.name());
                            }
                        });
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // Base size slider
                let size_label = if state.shape == ConeShape::Cone {
                    "Radius:"
                } else {
                    "Half-side:"
                };
                ui.horizontal(|ui| {
                    ui.label(size_label);
                    ui.add(
                        egui::DragValue::new(&mut state.base_size)
                            .range(1..=50)
                            .suffix(" blocks"),
                    );
                });
                ui.add(egui::Slider::new(&mut state.base_size, 1..=50).show_value(false));

                ui.add_space(4.0);

                // Height slider
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
                ui.separator();
                ui.add_space(8.0);

                // Options
                ui.checkbox(&mut state.hollow, "Hollow shell");
                ui.checkbox(&mut state.inverted, "Inverted (point down)");

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // Status display
                if state.total_blocks > 0 {
                    ui.horizontal(|ui| {
                        ui.label("Blocks:");
                        ui.label(format!("{}", state.total_blocks));
                    });
                    if state.preview_truncated {
                        ui.colored_label(egui::Color32::YELLOW, "Preview truncated (>4096)");
                    }
                    ui.add_space(4.0);
                    ui.small("Right-click to place");
                } else {
                    ui.colored_label(egui::Color32::YELLOW, "Aim at surface to preview");
                }

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                // Instructions
                ui.heading("Usage");
                ui.add_space(4.0);
                ui.small("1. Select shape (cone/pyramid)");
                ui.small("2. Adjust base size and height");
                ui.small("3. Toggle hollow/inverted if desired");
                ui.small("4. Aim at placement surface");
                ui.small("5. Right-click to place");
            });
    }
}

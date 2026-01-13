//! Arch tool settings UI.
//!
//! Provides a settings window for configuring the arch placement parameters
//! including width, height, thickness, style, orientation, and hollow mode.

use egui_winit_vulkano::egui;

use crate::shape_tools::ArchToolState;
use crate::shape_tools::arch::{ArchOrientation, ArchStyle};

/// Arch tool UI renderer.
pub struct ArchToolUI;

impl ArchToolUI {
    /// Draw the arch tool settings window.
    pub fn draw(ctx: &egui::Context, state: &mut ArchToolState) {
        if !state.active {
            return;
        }

        egui::Window::new("Arch Tool")
            .default_pos(egui::pos2(ctx.screen_rect().width() - 220.0, 100.0))
            .default_size(egui::vec2(200.0, 320.0))
            .resizable(false)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.heading("Arch Builder");
                ui.add_space(8.0);

                // Width slider
                ui.horizontal(|ui| {
                    ui.label("Width:");
                    ui.add(
                        egui::DragValue::new(&mut state.width)
                            .range(2..=50)
                            .suffix(" blocks"),
                    );
                });
                ui.add(egui::Slider::new(&mut state.width, 2..=50).show_value(false));

                ui.add_space(4.0);

                // Height slider
                ui.horizontal(|ui| {
                    ui.label("Height:");
                    ui.add(
                        egui::DragValue::new(&mut state.height)
                            .range(1..=50)
                            .suffix(" blocks"),
                    );
                });
                ui.add(egui::Slider::new(&mut state.height, 1..=50).show_value(false));

                ui.add_space(4.0);

                // Thickness slider
                ui.horizontal(|ui| {
                    ui.label("Thickness:");
                    ui.add(
                        egui::DragValue::new(&mut state.thickness)
                            .range(1..=10)
                            .suffix(" blocks"),
                    );
                });
                ui.add(egui::Slider::new(&mut state.thickness, 1..=10).show_value(false));

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // Arch style selection
                ui.horizontal(|ui| {
                    ui.label("Style:");
                    egui::ComboBox::from_id_salt("arch_style")
                        .selected_text(state.style.name())
                        .show_ui(ui, |ui| {
                            for style in ArchStyle::all() {
                                ui.selectable_value(&mut state.style, *style, style.name());
                            }
                        });
                });

                ui.add_space(4.0);

                // Orientation selection
                ui.horizontal(|ui| {
                    ui.label("Facing:");
                    egui::ComboBox::from_id_salt("arch_orientation")
                        .selected_text(state.orientation.name())
                        .show_ui(ui, |ui| {
                            for orientation in ArchOrientation::all() {
                                ui.selectable_value(
                                    &mut state.orientation,
                                    *orientation,
                                    orientation.name(),
                                );
                            }
                        });
                });

                ui.add_space(4.0);

                // Hollow toggle
                ui.checkbox(&mut state.hollow, "Hollow (passageway)");

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
                    ui.small("Right-click to place arch");
                } else {
                    ui.colored_label(egui::Color32::YELLOW, "Aim at surface to preview");
                }

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                // Instructions
                ui.heading("Usage");
                ui.add_space(4.0);
                ui.small("1. Adjust width, height, thickness");
                ui.small("2. Select arch style");
                ui.small("3. Aim at placement surface");
                ui.small("4. Right-click to place");
            });
    }
}

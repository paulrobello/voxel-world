//! Clone/Array tool settings UI.
//!
//! Provides a settings window for configuring clone/array operations
//! including mode (linear/grid), counts, spacing, and axis.

use egui_winit_vulkano::egui;

use crate::shape_tools::CloneToolState;
use crate::shape_tools::clone::{CloneAxis, CloneMode};
use crate::templates::TemplateSelection;

/// Clone tool UI renderer.
pub struct CloneToolUI;

impl CloneToolUI {
    /// Draw the clone tool settings window.
    pub fn draw(ctx: &egui::Context, state: &mut CloneToolState, selection: &TemplateSelection) {
        if !state.active {
            return;
        }

        egui::Window::new("Clone/Array Tool")
            .default_pos(egui::pos2(ctx.screen_rect().width() - 220.0, 100.0))
            .default_size(egui::vec2(200.0, 380.0))
            .resizable(false)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.heading("Clone Builder");
                ui.add_space(8.0);

                // Check if selection is valid
                let has_selection = selection.pos1.is_some() && selection.pos2.is_some();

                if !has_selection {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        "No selection! Press V to enter selection mode,\nthen set pos1 and pos2.",
                    );
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);
                }

                // Mode selection
                ui.horizontal(|ui| {
                    ui.label("Mode:");
                    egui::ComboBox::from_id_salt("clone_mode")
                        .selected_text(state.mode.name())
                        .show_ui(ui, |ui| {
                            for mode in CloneMode::all() {
                                ui.selectable_value(&mut state.mode, *mode, mode.name());
                            }
                        });
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                match state.mode {
                    CloneMode::Linear => {
                        Self::draw_linear_settings(ui, state);
                    }
                    CloneMode::Grid => {
                        Self::draw_grid_settings(ui, state);
                    }
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // Status display
                if state.total_blocks > 0 {
                    ui.horizontal(|ui| {
                        ui.label("Copies:");
                        ui.label(format!("{}", state.copy_count));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Total blocks:");
                        ui.label(format!("{}", state.total_blocks));
                    });
                    if state.preview_truncated {
                        ui.colored_label(egui::Color32::YELLOW, "Preview truncated (>4096)");
                    }
                    ui.add_space(8.0);

                    // Execute button
                    if ui.button("Clone Blocks").clicked() {
                        state.execute_requested = true;
                    }
                } else if has_selection {
                    ui.colored_label(egui::Color32::YELLOW, "Selection contains no blocks");
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
                ui.small("1. Press V to enter selection mode");
                ui.small("2. Set pos1 and pos2 corners");
                ui.small("3. Choose clone mode and settings");
                ui.small("4. Click 'Clone Blocks' to execute");
            });
    }

    /// Draw linear clone mode settings.
    fn draw_linear_settings(ui: &mut egui::Ui, state: &mut CloneToolState) {
        ui.label(egui::RichText::new("Linear Mode").strong());
        ui.add_space(4.0);

        // Axis selection
        ui.horizontal(|ui| {
            ui.label("Axis:");
            egui::ComboBox::from_id_salt("clone_axis")
                .selected_text(state.axis.name())
                .show_ui(ui, |ui| {
                    for axis in CloneAxis::all() {
                        ui.selectable_value(&mut state.axis, *axis, axis.name());
                    }
                });
        });

        ui.add_space(4.0);

        // Count slider
        ui.horizontal(|ui| {
            ui.label("Copies:");
            ui.add(egui::DragValue::new(&mut state.count).range(1..=20));
        });
        ui.add(egui::Slider::new(&mut state.count, 1..=20).show_value(false));

        ui.add_space(4.0);

        // Spacing slider
        ui.horizontal(|ui| {
            ui.label("Spacing:");
            ui.add(egui::DragValue::new(&mut state.spacing).range(0..=50));
        });
        ui.add(egui::Slider::new(&mut state.spacing, 0..=50).show_value(false));
    }

    /// Draw grid clone mode settings.
    fn draw_grid_settings(ui: &mut egui::Ui, state: &mut CloneToolState) {
        ui.label(egui::RichText::new("Grid Mode (XZ)").strong());
        ui.add_space(4.0);

        // X count
        ui.horizontal(|ui| {
            ui.label("X copies:");
            ui.add(egui::DragValue::new(&mut state.grid_count_x).range(1..=10));
        });
        ui.add(egui::Slider::new(&mut state.grid_count_x, 1..=10).show_value(false));

        // X spacing
        ui.horizontal(|ui| {
            ui.label("X spacing:");
            ui.add(egui::DragValue::new(&mut state.grid_spacing_x).range(0..=20));
        });

        ui.add_space(8.0);

        // Z count
        ui.horizontal(|ui| {
            ui.label("Z copies:");
            ui.add(egui::DragValue::new(&mut state.grid_count_z).range(1..=10));
        });
        ui.add(egui::Slider::new(&mut state.grid_count_z, 1..=10).show_value(false));

        // Z spacing
        ui.horizontal(|ui| {
            ui.label("Z spacing:");
            ui.add(egui::DragValue::new(&mut state.grid_spacing_z).range(0..=20));
        });
    }
}

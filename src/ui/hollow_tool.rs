//! Hollow tool UI settings window.
//!
//! This module provides the egui interface for configuring the hollow tool.

use egui_winit_vulkano::egui::{self, Align2, Context, Slider, Window};

use crate::shape_tools::HollowToolState;

/// Hollow tool settings window.
pub struct HollowToolUI;

impl HollowToolUI {
    /// Draw the hollow tool settings window.
    ///
    /// # Arguments
    /// * `ctx` - The egui context
    /// * `state` - The hollow tool state to modify
    /// * `has_selection` - Whether a valid selection exists
    pub fn draw(ctx: &Context, state: &mut HollowToolState, has_selection: bool) {
        if !state.active {
            return;
        }

        Window::new("⬜ Hollow Tool")
            .resizable(false)
            .collapsible(true)
            .anchor(Align2::RIGHT_TOP, [-10.0, 310.0])
            .show(ctx, |ui| {
                ui.set_width(200.0);

                // Thickness slider
                ui.horizontal(|ui| {
                    ui.label("Thickness:");
                    ui.add(Slider::new(&mut state.thickness, 1..=5).suffix(" blocks"));
                });

                ui.add_space(8.0);
                ui.separator();

                // Status
                if !has_selection {
                    ui.colored_label(egui::Color32::YELLOW, "⚠ No selection");
                    ui.label("Use V to make a selection first");
                } else {
                    ui.label(format!("Selection: {} blocks", state.total_blocks));

                    if state.hollow_count == 0 {
                        ui.colored_label(egui::Color32::YELLOW, "Selection too small to hollow");
                    } else {
                        ui.label(format!("Interior: {} blocks", state.hollow_count));
                        ui.label(format!(
                            "Remaining: {} blocks",
                            state.total_blocks - state.hollow_count
                        ));

                        if state.preview_truncated {
                            ui.colored_label(egui::Color32::YELLOW, "Preview truncated");
                        }

                        ui.add_space(4.0);
                        ui.label("Right-click to hollow out");
                    }
                }

                ui.add_space(4.0);
                ui.colored_label(egui::Color32::GRAY, "ESC to cancel");
            });
    }
}

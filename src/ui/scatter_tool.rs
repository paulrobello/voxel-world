//! Scatter Brush tool UI settings window.
//!
//! This module provides the egui interface for configuring the scatter brush tool.

use egui_winit_vulkano::egui::{self, Align2, Context, Slider, Window};

use crate::shape_tools::ScatterToolState;

/// Scatter Brush tool settings window.
pub struct ScatterToolUI;

impl ScatterToolUI {
    /// Draw the scatter brush tool settings window.
    ///
    /// # Arguments
    /// * `ctx` - The egui context
    /// * `state` - The scatter tool state to modify
    pub fn draw(ctx: &Context, state: &mut ScatterToolState) {
        if !state.active {
            return;
        }

        Window::new("🎨 Scatter Brush")
            .resizable(false)
            .collapsible(true)
            .anchor(Align2::RIGHT_TOP, [-10.0, 310.0])
            .show(ctx, |ui| {
                ui.set_width(200.0);

                // Radius slider
                ui.horizontal(|ui| {
                    ui.label("Radius:");
                    ui.add(Slider::new(&mut state.radius, 1..=20));
                });

                ui.add_space(4.0);

                // Density slider
                ui.horizontal(|ui| {
                    ui.label("Density:");
                    ui.add(Slider::new(&mut state.density, 1..=100).suffix("%"));
                });

                ui.add_space(4.0);

                // Surface only checkbox
                ui.checkbox(&mut state.surface_only, "Surface only");

                ui.add_space(4.0);

                // Height variation slider
                if !state.surface_only {
                    ui.horizontal(|ui| {
                        ui.label("Height var:");
                        ui.add(Slider::new(&mut state.height_variation, 0..=5));
                    });
                }

                ui.add_space(8.0);
                ui.separator();

                // Block info
                ui.label("Block: Hotbar slot 1");

                ui.add_space(8.0);
                ui.separator();

                // Status and instructions
                if state.painting {
                    ui.colored_label(egui::Color32::GREEN, "Painting...");
                } else {
                    ui.label("Hold right-click to paint");
                }

                ui.add_space(4.0);
                ui.colored_label(egui::Color32::GRAY, "ESC to cancel");
            });
    }
}

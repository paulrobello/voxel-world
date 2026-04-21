//! UI for the terrain brush tool.

use egui_winit_vulkano::egui::{Color32, RichText, Slider, Ui};

use crate::shape_tools::terrain_brush::{TerrainBrushMode, TerrainBrushState};

/// UI component for terrain brush tool settings.
pub struct TerrainBrushToolUI;

impl TerrainBrushToolUI {
    /// Draw the terrain brush tool UI.
    pub fn draw(ui: &mut Ui, state: &mut TerrainBrushState) {
        ui.heading("Terrain Brush");
        ui.separator();

        // Mode selection
        ui.horizontal(|ui: &mut Ui| {
            ui.label("Mode:");
            let mode_text = state.mode.name();
            if ui.button(mode_text).clicked() {
                state.mode = state.mode.next();
            }
        });

        // Mode description
        ui.label(
            RichText::new(state.mode.description())
                .italics()
                .color(Color32::GRAY),
        );

        ui.add_space(8.0);

        // Brush shape toggle
        ui.horizontal(|ui: &mut Ui| {
            ui.label("Shape:");
            let shape_text = state.shape.name();
            if ui.button(shape_text).clicked() {
                state.shape = state.shape.toggle();
            }
        });

        ui.add_space(8.0);

        // Radius slider
        ui.horizontal(|ui: &mut Ui| {
            ui.label("Radius:");
            ui.add(Slider::new(&mut state.radius, 1..=20));
        });

        // Strength slider (not used for flatten mode)
        if state.mode != TerrainBrushMode::Flatten {
            ui.horizontal(|ui: &mut Ui| {
                ui.label("Strength:");
                ui.add(Slider::new(&mut state.strength, 1..=10));
            });
        }

        // Target Y for flatten mode
        if state.mode == TerrainBrushMode::Flatten {
            ui.horizontal(|ui: &mut Ui| {
                ui.label("Target Y:");
                ui.add(Slider::new(&mut state.target_y, 1..=511));
            });
        }

        // Cooldown slider
        ui.horizontal(|ui: &mut Ui| {
            ui.label("Cooldown:");
            ui.add(Slider::new(&mut state.cooldown, 0.1..=2.0).suffix("s"));
        });

        ui.add_space(8.0);
        ui.separator();

        // Status
        let status_text = if state.painting {
            RichText::new("Painting...").color(Color32::YELLOW)
        } else {
            RichText::new("Right-click and drag to paint").color(Color32::LIGHT_GRAY)
        };
        ui.label(status_text);

        ui.add_space(4.0);

        // Cancel instruction
        ui.label(RichText::new("Left-click to cancel").color(Color32::GRAY));
    }
}

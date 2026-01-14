//! Pattern Fill tool UI settings window.
//!
//! This module provides the egui interface for configuring the pattern fill tool.

use egui_winit_vulkano::egui::{self, Align2, Context, Slider, Window};

use crate::shape_tools::{PatternFillState, PatternType};

/// Pattern Fill tool settings window.
pub struct PatternToolUI;

impl PatternToolUI {
    /// Draw the pattern fill tool settings window.
    ///
    /// # Arguments
    /// * `ctx` - The egui context
    /// * `state` - The pattern fill tool state to modify
    /// * `has_selection` - Whether a valid selection exists
    pub fn draw(ctx: &Context, state: &mut PatternFillState, has_selection: bool) {
        if !state.active {
            return;
        }

        Window::new("🔲 Pattern Fill")
            .resizable(false)
            .collapsible(true)
            .anchor(Align2::RIGHT_TOP, [-10.0, 310.0])
            .show(ctx, |ui| {
                ui.set_width(200.0);

                // Pattern type selector
                ui.horizontal(|ui| {
                    ui.label("Pattern:");
                    egui::ComboBox::from_id_salt("pattern_type")
                        .selected_text(state.pattern_type.name())
                        .show_ui(ui, |ui| {
                            for pattern in PatternType::ALL {
                                ui.selectable_value(
                                    &mut state.pattern_type,
                                    *pattern,
                                    pattern.name(),
                                );
                            }
                        });
                });

                ui.add_space(4.0);

                // Period slider (not used for Random)
                if state.pattern_type != PatternType::Random {
                    ui.horizontal(|ui| {
                        ui.label("Period:");
                        ui.add(Slider::new(&mut state.period, 1..=10));
                    });
                }

                // Random percentage (only for Random pattern)
                if state.pattern_type == PatternType::Random {
                    ui.horizontal(|ui| {
                        ui.label("Block A %:");
                        ui.add(Slider::new(&mut state.random_percent, 1..=99).suffix("%"));
                    });
                }

                ui.add_space(8.0);
                ui.separator();

                // Block info
                ui.label("Block A: Hotbar slot 1");
                ui.label("Block B: Hotbar slot 2 (or Air)");

                ui.add_space(8.0);
                ui.separator();

                // Status
                if !has_selection {
                    ui.colored_label(egui::Color32::YELLOW, "⚠ No selection");
                    ui.label("Use V to make a selection first");
                } else {
                    ui.label(format!("Total blocks: {}", state.total_blocks));
                    if state.preview_truncated {
                        ui.colored_label(egui::Color32::YELLOW, "Preview truncated");
                    }

                    ui.add_space(4.0);
                    ui.label("Right-click to apply pattern");
                }

                ui.add_space(4.0);
                ui.colored_label(egui::Color32::GRAY, "ESC to cancel");
            });
    }
}

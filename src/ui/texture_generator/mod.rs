//! Texture generator UI panel with tabs for Generate, Paint, and Import.
//!
//! This is a pure directory-module split of the former `texture_generator.rs`
//! god file. The public API (`TextureGeneratorState`, `TextureTab`,
//! `TextureGeneratorUI`, `TexturePickerUI`) is re-exported unchanged so
//! external callers keep working. Tab drawing lives in one submodule per
//! cohesive concern:
//! - [`state`] — `TextureGeneratorState` and `TextureTab`.
//! - [`generate`] — procedural Generate tab.
//! - [`paint`] — pixel Paint tab (decomposed from a single 420-line function).
//! - [`import`] — image Import tab.
//! - [`picker`] — quick texture picker dropdown.

mod generate;
mod import;
mod paint;
mod picker;
mod state;

use crate::textures::{CanvasSize, TextureLibrary};
use egui_winit_vulkano::egui;

#[allow(unused_imports)] // reason: WIP texture generator UI — kept on public API surface
pub use picker::TexturePickerUI;
pub use state::{TextureGeneratorState, TextureTab};

/// Texture generator UI drawing entry point.
pub struct TextureGeneratorUI;

impl TextureGeneratorUI {
    /// Draws the texture generator window.
    pub fn draw(
        ctx: &egui::Context,
        state: &mut TextureGeneratorState,
        library: &mut TextureLibrary,
        picture_library: &mut crate::pictures::PictureLibrary,
    ) {
        if !state.open {
            return;
        }

        let mut open = state.open;
        egui::Window::new("Texture Generator")
            .open(&mut open)
            .default_size(egui::vec2(500.0, 600.0))
            .min_size(egui::vec2(450.0, 500.0))
            .resizable(true)
            .show(ctx, |ui| {
                // Tab bar
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut state.active_tab, TextureTab::Generate, "Generate");
                    ui.selectable_value(&mut state.active_tab, TextureTab::Paint, "Paint");
                    ui.selectable_value(&mut state.active_tab, TextureTab::Import, "Import");
                });

                ui.separator();

                // Content based on active tab
                match state.active_tab {
                    TextureTab::Generate => generate::draw_generate_tab(ui, state, library),
                    TextureTab::Paint => paint::draw_paint_tab(ui, state, library, picture_library),
                    TextureTab::Import => import::draw_import_tab(ui, state),
                }

                // Show status message if any
                if let Some(status) = state.get_status() {
                    ui.add_space(4.0);
                    ui.colored_label(egui::Color32::from_rgb(100, 200, 255), status);
                }
            });
        state.open = open;

        // Custom size dialog (drawn outside the main window)
        if state.show_size_dialog {
            let mut should_close = false;
            let mut should_apply = false;
            let mut new_width = state.custom_width;
            let mut new_height = state.custom_height;

            egui::Window::new("Custom Canvas Size")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Enter custom canvas dimensions (1-128):");
                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        ui.label("Width:");
                        ui.add(egui::DragValue::new(&mut new_width).range(1..=128).speed(1));
                        ui.label("px");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Height:");
                        ui.add(
                            egui::DragValue::new(&mut new_height)
                                .range(1..=128)
                                .speed(1),
                        );
                        ui.label("px");
                    });

                    ui.add_space(8.0);
                    ui.separator();

                    ui.horizontal(|ui| {
                        if ui.button("Apply").clicked() {
                            should_apply = true;
                            should_close = true;
                        }

                        if ui.button("Cancel").clicked() {
                            should_close = true;
                        }
                    });
                });

            // Apply changes outside the closure
            if should_apply {
                state.custom_width = new_width;
                state.custom_height = new_height;
                let new_size = CanvasSize::new(new_width, new_height);
                state.change_canvas_size(new_size);
            }
            if should_close {
                state.show_size_dialog = false;
            }
        }
    }
}

//! Import tab: image import and resize/filter UI.

use super::state::TextureGeneratorState;
use crate::textures::{ResizeMode, SampleFilter, open_image_dialog};
use egui_winit_vulkano::egui;

/// Draws the Import tab content.
pub(super) fn draw_import_tab(ui: &mut egui::Ui, state: &mut TextureGeneratorState) {
    ui.horizontal(|ui| {
        // Left: Controls
        ui.vertical(|ui| {
            ui.set_min_width(200.0);
            ui.set_max_width(200.0);

            ui.label("Source Image");
            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Browse...").clicked()
                    && let Some(path) = open_image_dialog()
                {
                    state.import.load_image(path);
                }
                if state.import.has_image() && ui.button("Clear").clicked() {
                    state.import.clear();
                }
            });

            if !state.import.file_name().is_empty() {
                ui.label(format!("File: {}", state.import.file_name()));
                ui.label(format!("Size: {}", state.import.source_size_string()));
            }

            if let Some(error) = &state.import.error {
                ui.colored_label(egui::Color32::RED, error);
            }

            ui.add_space(16.0);
            ui.label("Resize Mode");
            ui.separator();

            for mode in ResizeMode::all() {
                if ui
                    .radio_value(&mut state.import.resize_mode, mode, mode.display_name())
                    .on_hover_text(mode.description())
                    .changed()
                {
                    state.import.update_preview();
                }
            }

            // Crop offset controls (for Crop mode)
            if state.import.resize_mode == ResizeMode::Crop && state.import.has_image() {
                ui.add_space(8.0);
                ui.label("Crop Offset");
                let (max_x, max_y) = state.import.max_crop_offset();

                let mut changed = false;
                ui.horizontal(|ui| {
                    ui.label("X:");
                    if ui
                        .add(egui::DragValue::new(&mut state.import.crop_offset.0).range(0..=max_x))
                        .changed()
                    {
                        changed = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Y:");
                    if ui
                        .add(egui::DragValue::new(&mut state.import.crop_offset.1).range(0..=max_y))
                        .changed()
                    {
                        changed = true;
                    }
                });
                if changed {
                    state.import.update_preview();
                }
            }

            ui.add_space(16.0);
            ui.label("Sample Filter");
            ui.separator();

            for filter in SampleFilter::all() {
                if ui
                    .radio_value(
                        &mut state.import.sample_filter,
                        filter,
                        filter.display_name(),
                    )
                    .on_hover_text(filter.description())
                    .changed()
                {
                    state.import.update_preview();
                }
            }
        });

        ui.separator();

        // Right: Preview
        ui.vertical(|ui| {
            ui.label("Preview (64x64)");
            ui.separator();

            // Draw preview at 2x scale
            let preview_size = 128.0;
            let (rect, _response) = ui
                .allocate_exact_size(egui::vec2(preview_size, preview_size), egui::Sense::hover());

            let painter = ui.painter_at(rect);

            // Draw checkerboard background
            let check_size = 8.0;
            for cy in 0..16 {
                for cx in 0..16 {
                    let check_color = if (cx + cy) % 2 == 0 {
                        egui::Color32::from_gray(200)
                    } else {
                        egui::Color32::from_gray(160)
                    };
                    painter.rect_filled(
                        egui::Rect::from_min_size(
                            rect.min + egui::vec2(cx as f32 * check_size, cy as f32 * check_size),
                            egui::vec2(check_size, check_size),
                        ),
                        0.0,
                        check_color,
                    );
                }
            }

            // Draw preview pixels at 2x scale
            let pixels = state.import.get_result();
            let cell_size = preview_size / 64.0;
            for y in 0..64 {
                for x in 0..64 {
                    let idx = (y * 64 + x) * 4;
                    let r = pixels[idx];
                    let g = pixels[idx + 1];
                    let b = pixels[idx + 2];
                    let a = pixels[idx + 3];

                    if a > 0 {
                        let color = egui::Color32::from_rgba_unmultiplied(r, g, b, a);
                        let pixel_rect = egui::Rect::from_min_size(
                            rect.min + egui::vec2(x as f32 * cell_size, y as f32 * cell_size),
                            egui::vec2(cell_size, cell_size),
                        );
                        painter.rect_filled(pixel_rect, 0.0, color);
                    }
                }
            }

            // Border
            painter.rect_stroke(
                rect,
                0.0,
                egui::Stroke::new(1.0_f32, egui::Color32::GRAY),
                egui::StrokeKind::Outside,
            );

            ui.add_space(16.0);

            // Apply button
            let can_apply = state.import.has_image();
            if ui
                .add_enabled(can_apply, egui::Button::new("Apply to Canvas"))
                .clicked()
            {
                state.copy_import_to_canvas();
            }
        });
    });
}

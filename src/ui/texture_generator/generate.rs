//! Generate tab: procedural texture generation UI.

use super::state::TextureGeneratorState;
use crate::textures::{CustomTexture, TextureColor, TextureLibrary, TexturePattern};
use egui_winit_vulkano::egui;
use std::time::{SystemTime, UNIX_EPOCH};

/// Draws the Generate tab content.
pub(super) fn draw_generate_tab(
    ui: &mut egui::Ui,
    state: &mut TextureGeneratorState,
    library: &mut TextureLibrary,
) {
    ui.horizontal(|ui| {
        // Left panel: texture list
        ui.vertical(|ui| {
            ui.set_min_width(100.0);
            ui.set_max_width(100.0);
            ui.label("Custom Textures");
            ui.separator();

            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    let mut select_slot = None;
                    for (slot, name) in library.names() {
                        let selected = state.selected_slot == Some(slot);
                        if ui.selectable_label(selected, &name).clicked() {
                            select_slot = Some(slot);
                        }
                    }
                    if let Some(slot) = select_slot
                        && let Some(tex) = library.get(slot)
                    {
                        state.edit_texture(tex);
                    }
                });

            ui.separator();
            let can_add = !library.is_full();
            if ui
                .add_enabled(can_add, egui::Button::new("+ New"))
                .clicked()
            {
                state.new_texture();
            }
            if !can_add {
                ui.small("(Max 16 textures)");
            }
        });

        ui.separator();

        // Right panel: editor
        ui.vertical(|ui| {
            draw_pattern_editor(ui, state);
        });
    });

    ui.separator();

    // Bottom: action buttons
    ui.horizontal(|ui| {
        if ui.button("Save").clicked() {
            save_texture(state, library);
        }
        if state.selected_slot.is_some()
            && ui.button("Delete").clicked()
            && let Some(slot) = state.selected_slot
        {
            let _ = library.remove(slot);
            state.set_status("Deleted texture");
            state.new_texture();
            state.needs_gpu_sync = true;
        }
        if ui.button("Save Library").clicked() {
            if let Err(e) = library.save() {
                log::warn!("Failed to save texture library: {}", e);
                state.set_status(format!("Save failed: {}", e));
            } else {
                state.set_status("Library saved to disk");
            }
        }
        ui.separator();
        if ui.button("Copy to Canvas").clicked() {
            state.copy_generate_to_canvas();
        }
    });
}

/// Draws the pattern editor panel.
fn draw_pattern_editor(ui: &mut egui::Ui, state: &mut TextureGeneratorState) {
    let mut changed = false;

    // Show editing mode indicator
    if let Some(slot) = state.selected_slot {
        ui.colored_label(
            egui::Color32::from_rgb(100, 150, 255),
            format!("Editing Slot {}", slot),
        );
    } else {
        ui.colored_label(
            egui::Color32::from_rgb(100, 200, 100),
            "Creating New Texture",
        );
    }
    ui.add_space(4.0);

    // Name
    ui.horizontal(|ui| {
        ui.label("Name:");
        ui.text_edit_singleline(&mut state.editing.name);
    });

    // Pattern selector
    ui.horizontal(|ui| {
        ui.label("Pattern:");
        egui::ComboBox::from_id_salt("pattern_combo")
            .selected_text(state.editing.pattern.display_name())
            .show_ui(ui, |ui| {
                for pattern in TexturePattern::all() {
                    if ui
                        .selectable_value(
                            &mut state.editing.pattern,
                            pattern,
                            pattern.display_name(),
                        )
                        .on_hover_text(pattern.description())
                        .changed()
                    {
                        changed = true;
                    }
                }
            });
    });

    // Color pickers
    ui.horizontal(|ui| {
        ui.label("Color 1:");
        let mut rgb = [
            state.editing.color1.r as f32 / 255.0,
            state.editing.color1.g as f32 / 255.0,
            state.editing.color1.b as f32 / 255.0,
        ];
        if ui.color_edit_button_rgb(&mut rgb).changed() {
            state.editing.color1 = TextureColor::new(
                (rgb[0] * 255.0) as u8,
                (rgb[1] * 255.0) as u8,
                (rgb[2] * 255.0) as u8,
            );
            changed = true;
        }
    });
    // Color 1 presets
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (color, name) in color_presets() {
            let btn_color = egui::Color32::from_rgb(color.r, color.g, color.b);
            let btn = egui::Button::new("")
                .fill(btn_color)
                .min_size(egui::vec2(16.0, 16.0));
            if ui.add(btn).on_hover_text(name).clicked() {
                state.editing.color1 = color;
                changed = true;
            }
        }
    });

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label("Color 2:");
        let mut rgb = [
            state.editing.color2.r as f32 / 255.0,
            state.editing.color2.g as f32 / 255.0,
            state.editing.color2.b as f32 / 255.0,
        ];
        if ui.color_edit_button_rgb(&mut rgb).changed() {
            state.editing.color2 = TextureColor::new(
                (rgb[0] * 255.0) as u8,
                (rgb[1] * 255.0) as u8,
                (rgb[2] * 255.0) as u8,
            );
            changed = true;
        }
    });
    // Color 2 presets
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (color, name) in color_presets() {
            let btn_color = egui::Color32::from_rgb(color.r, color.g, color.b);
            let btn = egui::Button::new("")
                .fill(btn_color)
                .min_size(egui::vec2(16.0, 16.0));
            if ui.add(btn).on_hover_text(name).clicked() {
                state.editing.color2 = color;
                changed = true;
            }
        }
    });

    // Scale slider
    ui.horizontal(|ui| {
        ui.label("Scale:");
        if ui
            .add(egui::Slider::new(&mut state.editing.scale, 0.25..=4.0).logarithmic(true))
            .changed()
        {
            changed = true;
        }
    });

    // Rotation
    ui.horizontal(|ui| {
        ui.label("Rotation:");
        for rot in 0..4 {
            let label = match rot {
                0 => "0",
                1 => "90",
                2 => "180",
                3 => "270",
                _ => unreachable!(),
            };
            if ui
                .selectable_label(state.editing.rotation == rot, label)
                .clicked()
            {
                state.editing.rotation = rot;
                changed = true;
            }
        }
    });

    // Seed (for noise patterns)
    if state.editing.pattern == TexturePattern::Noise {
        ui.horizontal(|ui| {
            ui.label("Seed:");
            if ui
                .add(egui::DragValue::new(&mut state.editing.seed).range(0..=9999))
                .changed()
            {
                changed = true;
            }
            if ui.button("Random").on_hover_text("Random seed").clicked() {
                let nanos = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.subsec_nanos())
                    .unwrap_or(0);
                state.editing.seed = nanos % 10000;
                changed = true;
            }
        });
    }

    // Regenerate preview if needed
    if changed {
        state.editing.regenerate();
    }

    ui.separator();

    // Preview
    ui.label("Preview:");
    draw_preview(ui, &state.editing);
}

/// Draws a simple preview of the texture.
fn draw_preview(ui: &mut egui::Ui, texture: &CustomTexture) {
    let preview_size = 128.0;
    let (rect, _response) =
        ui.allocate_exact_size(egui::vec2(preview_size, preview_size), egui::Sense::hover());

    let painter = ui.painter_at(rect);

    // Draw texture preview using colored rectangles (2x2 pixel blocks)
    let cell_size = preview_size / 32.0;
    for gy in 0..32 {
        for gx in 0..32 {
            let x = gx * 2;
            let y = gy * 2;
            let idx = (y * 64 + x) * 4;
            if idx + 2 < texture.pixels.len() {
                let r = texture.pixels[idx];
                let g = texture.pixels[idx + 1];
                let b = texture.pixels[idx + 2];
                let color = egui::Color32::from_rgb(r, g, b);
                let cell_rect = egui::Rect::from_min_size(
                    rect.min + egui::vec2(gx as f32 * cell_size, gy as f32 * cell_size),
                    egui::vec2(cell_size, cell_size),
                );
                painter.rect_filled(cell_rect, 0.0, color);
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
}

/// Color presets for quick selection.
fn color_presets() -> [(TextureColor, &'static str); 8] {
    [
        (TextureColor::WHITE, "White"),
        (TextureColor::BLACK, "Black"),
        (TextureColor::RED, "Red"),
        (TextureColor::GREEN, "Green"),
        (TextureColor::BLUE, "Blue"),
        (TextureColor::YELLOW, "Yellow"),
        (TextureColor::STONE, "Stone"),
        (TextureColor::WOOD, "Wood"),
    ]
}

/// Saves the current texture to the library.
fn save_texture(state: &mut TextureGeneratorState, library: &mut TextureLibrary) {
    let success = if let Some(slot) = state.selected_slot {
        // Update existing
        match library.update(slot, state.editing.clone()) {
            Ok(()) => {
                log::debug!(
                    "[Texture] Updated custom texture '{}' in slot {}",
                    state.editing.name,
                    slot
                );
                state.set_status(format!("Updated '{}'", state.editing.name));
                true
            }
            Err(e) => {
                log::warn!("Failed to update texture: {}", e);
                state.set_status(format!("Error: {}", e));
                false
            }
        }
    } else {
        // Add new
        match library.add(state.editing.clone()) {
            Ok(slot) => {
                log::debug!(
                    "[Texture] Added custom texture '{}' to slot {}",
                    state.editing.name,
                    slot
                );
                state.selected_slot = Some(slot);
                state.editing.id = slot;
                state.set_status(format!("Created '{}' in slot {}", state.editing.name, slot));
                true
            }
            Err(e) => {
                log::warn!("Failed to add texture: {}", e);
                state.set_status(format!("Error: {}", e));
                false
            }
        }
    };

    // Signal that GPU sync is needed
    if success {
        state.needs_gpu_sync = true;
        // Set flag for multiplayer sync
        state.pending_multiplayer_upload = state.selected_slot;
    }
}

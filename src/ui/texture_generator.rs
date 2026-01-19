//! Texture generator UI panel.
#![allow(dead_code)] // Will be integrated into main UI later

use crate::textures::{CustomTexture, TextureColor, TextureLibrary, TexturePattern};
use egui_winit_vulkano::egui;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Duration to show status messages.
const STATUS_DURATION_SECS: f32 = 2.0;

/// State for the texture generator UI panel.
pub struct TextureGeneratorState {
    /// Whether the panel is open.
    pub open: bool,
    /// Current texture being edited.
    pub editing: CustomTexture,
    /// Selected slot for editing (None = new texture).
    pub selected_slot: Option<u8>,
    /// Preview texture ID for egui.
    pub preview_texture_id: Option<egui::TextureId>,
    /// Color picker state for color1.
    pub color1_picker_open: bool,
    /// Color picker state for color2.
    pub color2_picker_open: bool,
    /// Flag indicating custom textures need GPU sync.
    pub needs_gpu_sync: bool,
    /// Status message to display.
    status_message: Option<String>,
    /// When the status message was set.
    status_time: Option<Instant>,
}

impl Default for TextureGeneratorState {
    fn default() -> Self {
        Self::new()
    }
}

impl TextureGeneratorState {
    /// Creates new state with default values.
    pub fn new() -> Self {
        let mut editing = CustomTexture {
            name: "New Texture".to_string(),
            ..CustomTexture::default()
        };
        editing.regenerate();

        Self {
            open: false,
            editing,
            selected_slot: None,
            preview_texture_id: None,
            color1_picker_open: false,
            color2_picker_open: false,
            needs_gpu_sync: false,
            status_message: None,
            status_time: None,
        }
    }

    /// Sets a status message that will display briefly.
    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
        self.status_time = Some(Instant::now());
    }

    /// Gets the current status message if it hasn't expired.
    pub fn get_status(&self) -> Option<&str> {
        if let (Some(msg), Some(time)) = (&self.status_message, self.status_time) {
            if time.elapsed().as_secs_f32() < STATUS_DURATION_SECS {
                return Some(msg.as_str());
            }
        }
        None
    }

    /// Starts editing a new texture.
    pub fn new_texture(&mut self) {
        self.editing = CustomTexture::default();
        self.editing.name = "New Texture".to_string();
        self.editing.regenerate();
        self.selected_slot = None;
        self.set_status("Started new texture");
    }

    /// Starts editing an existing texture.
    pub fn edit_texture(&mut self, texture: &CustomTexture) {
        self.editing = texture.clone();
        self.selected_slot = Some(texture.id);
    }
}

/// Texture generator UI drawing functions.
pub struct TextureGeneratorUI;

impl TextureGeneratorUI {
    /// Draws the texture generator window.
    pub fn draw(
        ctx: &egui::Context,
        state: &mut TextureGeneratorState,
        library: &mut TextureLibrary,
    ) {
        if !state.open {
            return;
        }

        let mut open = state.open;
        egui::Window::new("Texture Generator")
            .open(&mut open)
            .default_size(egui::vec2(400.0, 500.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Left panel: texture list
                    ui.vertical(|ui| {
                        ui.set_min_width(120.0);
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
                                if let Some(slot) = select_slot {
                                    if let Some(tex) = library.get(slot) {
                                        state.edit_texture(tex);
                                    }
                                }
                            });

                        ui.separator();
                        let can_add = !library.is_full();
                        if ui
                            .add_enabled(can_add, egui::Button::new("➕ New"))
                            .clicked()
                        {
                            state.new_texture();
                            println!("[Texture] Started new texture");
                        }
                        if !can_add {
                            ui.small("(Max 16 textures)");
                        }
                    });

                    ui.separator();

                    // Right panel: editor
                    ui.vertical(|ui| {
                        Self::draw_editor(ui, state);
                    });
                });

                ui.separator();

                // Bottom: action buttons
                ui.horizontal(|ui| {
                    if ui.button("💾 Save").clicked() {
                        Self::save_texture(state, library);
                    }
                    if state.selected_slot.is_some() && ui.button("🗑 Delete").clicked() {
                        if let Some(slot) = state.selected_slot {
                            let _ = library.remove(slot);
                            state.set_status("Deleted texture");
                            state.new_texture();
                        }
                    }
                    if ui.button("📋 Save Library").clicked() {
                        if let Err(e) = library.save() {
                            eprintln!("Failed to save texture library: {}", e);
                            state.set_status(format!("Save failed: {}", e));
                        } else {
                            state.set_status("Library saved to disk");
                        }
                    }
                });

                // Show status message if any
                if let Some(status) = state.get_status() {
                    ui.add_space(4.0);
                    ui.colored_label(egui::Color32::from_rgb(100, 200, 255), status);
                }
            });
        state.open = open;
    }

    /// Draws the texture editor panel.
    fn draw_editor(ui: &mut egui::Ui, state: &mut TextureGeneratorState) {
        let mut changed = false;

        // Show editing mode indicator
        if state.selected_slot.is_none() {
            ui.colored_label(
                egui::Color32::from_rgb(100, 200, 100),
                "✨ Creating New Texture",
            );
        } else {
            ui.colored_label(
                egui::Color32::from_rgb(100, 150, 255),
                format!("✏ Editing Slot {}", state.selected_slot.unwrap()),
            );
        }
        ui.add_space(4.0);

        // Name
        ui.horizontal(|ui| {
            ui.label("Name:");
            if ui.text_edit_singleline(&mut state.editing.name).changed() {
                // Name doesn't affect preview
            }
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
            // Preset colors
            for (color, name) in Self::color_presets() {
                if ui.small_button("●").on_hover_text(name).clicked() {
                    state.editing.color1 = color;
                    changed = true;
                }
                ui.painter().rect_filled(
                    ui.cursor().shrink(2.0),
                    0.0,
                    egui::Color32::from_rgb(color.r, color.g, color.b),
                );
            }
        });

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
                    0 => "0°",
                    1 => "90°",
                    2 => "180°",
                    3 => "270°",
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
                if ui.button("🎲").on_hover_text("Random seed").clicked() {
                    // Simple time-based random seed
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

        // Preview (simple colored rectangles showing the pattern)
        ui.label("Preview:");
        Self::draw_preview(ui, &state.editing);
    }

    /// Draws a simple preview of the texture.
    fn draw_preview(ui: &mut egui::Ui, texture: &CustomTexture) {
        let preview_size = 128.0;
        let (rect, _response) =
            ui.allocate_exact_size(egui::vec2(preview_size, preview_size), egui::Sense::hover());

        let painter = ui.painter_at(rect);

        // Draw texture preview using colored rectangles (2x2 pixel blocks)
        let cell_size = preview_size / 32.0; // 32x32 grid for 64x64 texture
        for gy in 0..32 {
            for gx in 0..32 {
                let x = gx * 2;
                let y = gy * 2;
                // Sample at 2x2 grid to reduce draw calls
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
            egui::Stroke::new(1.0, egui::Color32::GRAY),
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
                    println!(
                        "[Texture] Updated custom texture '{}' in slot {}",
                        state.editing.name, slot
                    );
                    state.set_status(format!("Updated '{}'", state.editing.name));
                    true
                }
                Err(e) => {
                    eprintln!("Failed to update texture: {}", e);
                    state.set_status(format!("Error: {}", e));
                    false
                }
            }
        } else {
            // Add new
            match library.add(state.editing.clone()) {
                Ok(slot) => {
                    println!(
                        "[Texture] Added custom texture '{}' to slot {}",
                        state.editing.name, slot
                    );
                    state.selected_slot = Some(slot);
                    state.editing.id = slot;
                    state.set_status(format!("Created '{}' in slot {}", state.editing.name, slot));
                    true
                }
                Err(e) => {
                    eprintln!("Failed to add texture: {}", e);
                    state.set_status(format!("Error: {}", e));
                    false
                }
            }
        };

        // Signal that GPU sync is needed
        if success {
            state.needs_gpu_sync = true;
        }
    }
}

/// Quick texture picker for selecting custom textures.
pub struct TexturePickerUI;

impl TexturePickerUI {
    /// Draws a texture picker dropdown.
    /// Returns Some(slot) if a texture was selected.
    pub fn draw(
        ui: &mut egui::Ui,
        library: &TextureLibrary,
        current: Option<u8>,
        id_source: &str,
    ) -> Option<u8> {
        let mut selected = None;
        let label = current
            .and_then(|s| library.get(s))
            .map(|t| t.name.as_str())
            .unwrap_or("(None)");

        egui::ComboBox::from_id_salt(id_source)
            .selected_text(label)
            .show_ui(ui, |ui| {
                if ui.selectable_label(current.is_none(), "(None)").clicked() {
                    selected = Some(None);
                }
                for (slot, name) in library.names() {
                    if ui.selectable_label(current == Some(slot), name).clicked() {
                        selected = Some(Some(slot));
                    }
                }
            });

        selected.flatten()
    }
}

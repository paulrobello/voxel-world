//! Texture generator UI panel with tabs for Generate, Paint, and Import.
#![allow(dead_code)] // Will be integrated into main UI later

use crate::textures::{
    CanvasState, CustomTexture, ImportState, PaintTool, ResizeMode, SampleFilter, ShapeMode,
    TEXTURE_SIZE, TextureColor, TextureLibrary, TexturePattern, open_image_dialog,
};
use egui_winit_vulkano::egui;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Duration to show status messages.
const STATUS_DURATION_SECS: f32 = 2.0;

/// Active tab in the texture generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextureTab {
    /// Procedural pattern generation.
    #[default]
    Generate,
    /// Pixel painting canvas.
    Paint,
    /// Image import.
    Import,
}

/// State for the texture generator UI panel.
pub struct TextureGeneratorState {
    /// Whether the panel is open.
    pub open: bool,
    /// Current active tab.
    pub active_tab: TextureTab,
    /// Current texture being edited (procedural).
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
    /// Canvas state for paint tab.
    pub canvas: CanvasState,
    /// Import state for import tab.
    pub import: ImportState,
    /// Whether mouse is currently dragging on canvas.
    canvas_dragging: bool,
    /// Start position for shape dragging.
    drag_start: Option<(u32, u32)>,
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
            active_tab: TextureTab::default(),
            editing,
            selected_slot: None,
            preview_texture_id: None,
            color1_picker_open: false,
            color2_picker_open: false,
            needs_gpu_sync: false,
            status_message: None,
            status_time: None,
            canvas: CanvasState::new(),
            import: ImportState::new(),
            canvas_dragging: false,
            drag_start: None,
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

    /// Copies generated texture to the paint canvas.
    pub fn copy_generate_to_canvas(&mut self) {
        if !self.editing.pixels.is_empty() {
            self.canvas.copy_from(&self.editing.pixels);
            self.active_tab = TextureTab::Paint;
            self.set_status("Copied to canvas");
        }
    }

    /// Copies import result to the paint canvas.
    pub fn copy_import_to_canvas(&mut self) {
        if self.import.has_image() {
            self.canvas.copy_from(self.import.get_result());
            self.active_tab = TextureTab::Paint;
            self.set_status("Imported to canvas");
        }
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
                    TextureTab::Generate => Self::draw_generate_tab(ui, state, library),
                    TextureTab::Paint => Self::draw_paint_tab(ui, state, library),
                    TextureTab::Import => Self::draw_import_tab(ui, state),
                }

                // Show status message if any
                if let Some(status) = state.get_status() {
                    ui.add_space(4.0);
                    ui.colored_label(egui::Color32::from_rgb(100, 200, 255), status);
                }
            });
        state.open = open;
    }

    /// Draws the Generate tab content.
    fn draw_generate_tab(
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
                        if let Some(slot) = select_slot {
                            if let Some(tex) = library.get(slot) {
                                state.edit_texture(tex);
                            }
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
                Self::draw_pattern_editor(ui, state);
            });
        });

        ui.separator();

        // Bottom: action buttons
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                Self::save_texture(state, library);
            }
            if state.selected_slot.is_some() && ui.button("Delete").clicked() {
                if let Some(slot) = state.selected_slot {
                    let _ = library.remove(slot);
                    state.set_status("Deleted texture");
                    state.new_texture();
                    state.needs_gpu_sync = true;
                }
            }
            if ui.button("Save Library").clicked() {
                if let Err(e) = library.save() {
                    eprintln!("Failed to save texture library: {}", e);
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
        if state.selected_slot.is_none() {
            ui.colored_label(
                egui::Color32::from_rgb(100, 200, 100),
                "Creating New Texture",
            );
        } else {
            ui.colored_label(
                egui::Color32::from_rgb(100, 150, 255),
                format!("Editing Slot {}", state.selected_slot.unwrap()),
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
            for (color, name) in Self::color_presets() {
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
            for (color, name) in Self::color_presets() {
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
        Self::draw_preview(ui, &state.editing);
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
            egui::Stroke::new(1.0, egui::Color32::GRAY),
            egui::StrokeKind::Outside,
        );
    }

    /// Draws the Paint tab content.
    fn draw_paint_tab(
        ui: &mut egui::Ui,
        state: &mut TextureGeneratorState,
        library: &mut TextureLibrary,
    ) {
        ui.horizontal(|ui| {
            // Left: Tools panel
            ui.vertical(|ui| {
                ui.set_min_width(120.0);
                ui.set_max_width(120.0);

                ui.label("Tools");
                ui.separator();

                // Tool buttons in grid
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
                    for tool in PaintTool::all() {
                        let selected = state.canvas.tool == tool;
                        let label = format!("{} {}", tool.icon(), tool.display_name());
                        if ui.selectable_label(selected, label).clicked() {
                            state.canvas.tool = tool;
                            // Clear shape start when switching tools
                            state.canvas.shape_start = None;
                            state.drag_start = None;
                        }
                    }
                });

                ui.add_space(8.0);

                // Brush size (for brush/eraser)
                if matches!(state.canvas.tool, PaintTool::Brush | PaintTool::Eraser) {
                    ui.horizontal(|ui| {
                        ui.label("Size:");
                        ui.add(
                            egui::DragValue::new(&mut state.canvas.brush_size)
                                .range(1..=8)
                                .speed(0.1),
                        );
                    });
                }

                // Shape mode (for rect/circle)
                if matches!(state.canvas.tool, PaintTool::Rectangle | PaintTool::Circle) {
                    ui.horizontal(|ui| {
                        ui.label("Mode:");
                        ui.selectable_value(
                            &mut state.canvas.shape_mode,
                            ShapeMode::Filled,
                            "Fill",
                        );
                        ui.selectable_value(
                            &mut state.canvas.shape_mode,
                            ShapeMode::Outline,
                            "Outline",
                        );
                    });
                }

                ui.add_space(8.0);
                ui.label("Mirror");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut state.canvas.mirror_x, "X");
                    ui.checkbox(&mut state.canvas.mirror_y, "Y");
                });

                ui.add_space(8.0);
                ui.label("View");
                ui.horizontal(|ui| {
                    ui.label("Zoom:");
                    for z in [1, 2, 4, 8] {
                        if ui
                            .selectable_label(state.canvas.zoom == z, format!("{}x", z))
                            .clicked()
                        {
                            state.canvas.zoom = z;
                        }
                    }
                });
                ui.checkbox(&mut state.canvas.show_grid, "Grid");

                ui.add_space(8.0);
                ui.separator();

                // Undo/Redo
                ui.horizontal(|ui| {
                    let undo_enabled = state.canvas.history.can_undo();
                    if ui
                        .add_enabled(undo_enabled, egui::Button::new("Undo"))
                        .clicked()
                    {
                        state.canvas.undo();
                    }
                    let redo_enabled = state.canvas.history.can_redo();
                    if ui
                        .add_enabled(redo_enabled, egui::Button::new("Redo"))
                        .clicked()
                    {
                        state.canvas.redo();
                    }
                });
                ui.small(format!(
                    "({}/{})",
                    state.canvas.history.undo_count(),
                    state.canvas.history.redo_count()
                ));

                if ui.button("Clear").clicked() {
                    state.canvas.clear();
                }
            });

            ui.separator();

            // Center: Canvas
            ui.vertical(|ui| {
                Self::draw_canvas(ui, state);
            });

            ui.separator();

            // Right: Palette
            ui.vertical(|ui| {
                ui.set_min_width(80.0);
                ui.set_max_width(80.0);

                ui.label("Palette");
                ui.separator();

                // 32-color palette grid (4x8)
                let cell_size = 16.0;
                for row in 0..8 {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
                        for col in 0..4 {
                            let idx = row * 4 + col;
                            let color = state.canvas.palette[idx];
                            let is_selected = state.canvas.selected_color == idx;

                            let btn_color = egui::Color32::from_rgba_unmultiplied(
                                color[0], color[1], color[2], color[3],
                            );

                            // Use checkerboard for transparent
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(cell_size, cell_size),
                                egui::Sense::click(),
                            );

                            let painter = ui.painter_at(rect);

                            // Draw transparency checkerboard if alpha < 255
                            if color[3] < 255 {
                                let check_size = cell_size / 4.0;
                                for cy in 0..4 {
                                    for cx in 0..4 {
                                        let check_color = if (cx + cy) % 2 == 0 {
                                            egui::Color32::from_gray(200)
                                        } else {
                                            egui::Color32::from_gray(150)
                                        };
                                        painter.rect_filled(
                                            egui::Rect::from_min_size(
                                                rect.min
                                                    + egui::vec2(
                                                        cx as f32 * check_size,
                                                        cy as f32 * check_size,
                                                    ),
                                                egui::vec2(check_size, check_size),
                                            ),
                                            0.0,
                                            check_color,
                                        );
                                    }
                                }
                            }

                            // Draw the color
                            painter.rect_filled(rect, 0.0, btn_color);

                            // Selection border
                            let stroke = if is_selected {
                                egui::Stroke::new(2.0, egui::Color32::WHITE)
                            } else {
                                egui::Stroke::new(1.0, egui::Color32::DARK_GRAY)
                            };
                            painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Outside);

                            if response.clicked() {
                                state.canvas.selected_color = idx;
                            }

                            // Right-click to edit color
                            if response.secondary_clicked() {
                                // Could open color picker here
                            }
                        }
                    });
                }

                ui.add_space(8.0);

                // Current color display
                let current = state.canvas.palette[state.canvas.selected_color];
                ui.horizontal(|ui| {
                    ui.label("Color:");
                    let mut rgb = [
                        current[0] as f32 / 255.0,
                        current[1] as f32 / 255.0,
                        current[2] as f32 / 255.0,
                    ];
                    if ui.color_edit_button_rgb(&mut rgb).changed() {
                        state.canvas.palette[state.canvas.selected_color] = [
                            (rgb[0] * 255.0) as u8,
                            (rgb[1] * 255.0) as u8,
                            (rgb[2] * 255.0) as u8,
                            255,
                        ];
                    }
                });
            });
        });

        ui.separator();

        // Bottom: Save buttons
        ui.horizontal(|ui| {
            if ui.button("Save as Texture").clicked() {
                // Create a raw CustomTexture from canvas pixels
                let name = if state.editing.name.is_empty() || state.editing.name == "New Texture" {
                    format!("Canvas {}", library.count() + 1)
                } else {
                    state.editing.name.clone()
                };
                let tex = CustomTexture::from_pixels(name, state.canvas.pixels.clone());

                if let Some(slot) = state.selected_slot {
                    // Update existing
                    let mut tex = tex;
                    tex.id = slot;
                    if library.update(slot, tex.clone()).is_ok() {
                        state.editing = tex;
                        state.set_status("Updated texture");
                        state.needs_gpu_sync = true;
                    }
                } else {
                    // Add new
                    match library.add(tex.clone()) {
                        Ok(slot) => {
                            state.selected_slot = Some(slot);
                            let mut tex = tex;
                            tex.id = slot;
                            state.editing = tex;
                            state.set_status(format!("Saved to slot {}", slot));
                            state.needs_gpu_sync = true;
                        }
                        Err(e) => {
                            state.set_status(format!("Error: {}", e));
                        }
                    }
                }
            }

            if ui.button("Save Library").clicked() {
                if let Err(e) = library.save() {
                    state.set_status(format!("Save failed: {}", e));
                } else {
                    state.set_status("Library saved");
                }
            }
        });
    }

    /// Draws the paint canvas with interaction handling.
    fn draw_canvas(ui: &mut egui::Ui, state: &mut TextureGeneratorState) {
        let zoom = state.canvas.zoom as f32;
        let canvas_size = TEXTURE_SIZE as f32 * zoom;

        egui::ScrollArea::both().max_height(400.0).show(ui, |ui| {
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(canvas_size, canvas_size),
                egui::Sense::click_and_drag(),
            );

            let painter = ui.painter_at(rect);

            // Draw canvas pixels
            let pixels_to_draw = if let Some(ref preview) = state.canvas.preview_pixels {
                preview
            } else {
                &state.canvas.pixels
            };

            for y in 0..TEXTURE_SIZE {
                for x in 0..TEXTURE_SIZE {
                    let idx = ((y * TEXTURE_SIZE + x) * 4) as usize;
                    let r = pixels_to_draw[idx];
                    let g = pixels_to_draw[idx + 1];
                    let b = pixels_to_draw[idx + 2];
                    let a = pixels_to_draw[idx + 3];

                    let pixel_rect = egui::Rect::from_min_size(
                        rect.min + egui::vec2(x as f32 * zoom, y as f32 * zoom),
                        egui::vec2(zoom, zoom),
                    );

                    // Draw checkerboard for transparent pixels
                    if a < 255 {
                        let check = if (x + y) % 2 == 0 { 220 } else { 180 };
                        painter.rect_filled(pixel_rect, 0.0, egui::Color32::from_gray(check));
                    }

                    if a > 0 {
                        let color = egui::Color32::from_rgba_unmultiplied(r, g, b, a);
                        painter.rect_filled(pixel_rect, 0.0, color);
                    }
                }
            }

            // Draw grid if enabled
            if state.canvas.show_grid && zoom >= 2.0 {
                let grid_color = egui::Color32::from_rgba_unmultiplied(128, 128, 128, 80);
                for i in 0..=TEXTURE_SIZE {
                    let x = rect.min.x + i as f32 * zoom;
                    let y = rect.min.y + i as f32 * zoom;
                    painter.line_segment(
                        [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                        egui::Stroke::new(1.0, grid_color),
                    );
                    painter.line_segment(
                        [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
                        egui::Stroke::new(1.0, grid_color),
                    );
                }
            }

            // Border
            painter.rect_stroke(
                rect,
                0.0,
                egui::Stroke::new(1.0, egui::Color32::GRAY),
                egui::StrokeKind::Outside,
            );

            // Handle mouse interaction
            if let Some(pos) = response.interact_pointer_pos() {
                let local_x = ((pos.x - rect.min.x) / zoom).floor() as i32;
                let local_y = ((pos.y - rect.min.y) / zoom).floor() as i32;

                if local_x >= 0
                    && local_x < TEXTURE_SIZE as i32
                    && local_y >= 0
                    && local_y < TEXTURE_SIZE as i32
                {
                    let x = local_x as u32;
                    let y = local_y as u32;

                    state.canvas.hover_pos = Some((x, y));

                    // Handle tool actions
                    if response.drag_started() {
                        state.canvas_dragging = true;

                        // Save state for tools that need it
                        match state.canvas.tool {
                            PaintTool::Pencil | PaintTool::Brush | PaintTool::Eraser => {
                                state.canvas.save_state();
                            }
                            PaintTool::Line | PaintTool::Rectangle | PaintTool::Circle => {
                                state.drag_start = Some((x, y));
                                state.canvas.save_state();
                            }
                            PaintTool::Fill => {
                                state.canvas.flood_fill(x, y);
                            }
                            PaintTool::Eyedropper => {
                                state.canvas.eyedropper(x, y);
                                // Switch to pencil after picking
                                state.canvas.tool = PaintTool::Pencil;
                            }
                        }
                    }

                    if state.canvas_dragging {
                        match state.canvas.tool {
                            PaintTool::Pencil => {
                                state.canvas.draw_pencil(x, y);
                            }
                            PaintTool::Brush => {
                                state.canvas.draw_brush(x, y);
                            }
                            PaintTool::Eraser => {
                                state.canvas.erase(x, y);
                            }
                            PaintTool::Line | PaintTool::Rectangle | PaintTool::Circle => {
                                // Update preview
                                if let Some((sx, sy)) = state.drag_start {
                                    state.canvas.preview_pixels =
                                        Some(state.canvas.generate_preview(sx, sy, x, y));
                                }
                            }
                            _ => {}
                        }
                    }

                    if response.drag_stopped() {
                        state.canvas_dragging = false;

                        // Apply shape tools
                        match state.canvas.tool {
                            PaintTool::Line => {
                                if let Some((sx, sy)) = state.drag_start {
                                    state.canvas.draw_line(sx, sy, x, y);
                                }
                            }
                            PaintTool::Rectangle => {
                                if let Some((sx, sy)) = state.drag_start {
                                    state.canvas.draw_rectangle(sx, sy, x, y);
                                }
                            }
                            PaintTool::Circle => {
                                if let Some((sx, sy)) = state.drag_start {
                                    state.canvas.draw_circle(sx, sy, x, y);
                                }
                            }
                            _ => {}
                        }

                        state.drag_start = None;
                        state.canvas.preview_pixels = None;
                    }
                }
            } else {
                state.canvas.hover_pos = None;
            }

            // Show hover info
            if let Some((x, y)) = state.canvas.hover_pos {
                let color = state.canvas.get_pixel(x, y);
                ui.label(format!(
                    "({}, {}) #{:02X}{:02X}{:02X}",
                    x, y, color[0], color[1], color[2]
                ));
            }
        });
    }

    /// Draws the Import tab content.
    fn draw_import_tab(ui: &mut egui::Ui, state: &mut TextureGeneratorState) {
        ui.horizontal(|ui| {
            // Left: Controls
            ui.vertical(|ui| {
                ui.set_min_width(200.0);
                ui.set_max_width(200.0);

                ui.label("Source Image");
                ui.separator();

                ui.horizontal(|ui| {
                    if ui.button("Browse...").clicked() {
                        if let Some(path) = open_image_dialog() {
                            state.import.load_image(path);
                        }
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
                            .add(
                                egui::DragValue::new(&mut state.import.crop_offset.0)
                                    .range(0..=max_x),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Y:");
                        if ui
                            .add(
                                egui::DragValue::new(&mut state.import.crop_offset.1)
                                    .range(0..=max_y),
                            )
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
                let (rect, _response) = ui.allocate_exact_size(
                    egui::vec2(preview_size, preview_size),
                    egui::Sense::hover(),
                );

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
                                rect.min
                                    + egui::vec2(cx as f32 * check_size, cy as f32 * check_size),
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
                    egui::Stroke::new(1.0, egui::Color32::GRAY),
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

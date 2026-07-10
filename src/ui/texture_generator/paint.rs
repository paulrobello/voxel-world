//! Paint tab: pixel canvas painting UI.

use super::state::TextureGeneratorState;
use crate::textures::{CustomTexture, PaintTool, ShapeMode, TextureLibrary};
use egui_winit_vulkano::egui;
use std::time::Instant;

/// Draws the Paint tab content.
pub(super) fn draw_paint_tab(
    ui: &mut egui::Ui,
    state: &mut TextureGeneratorState,
    library: &mut TextureLibrary,
    picture_library: &mut crate::pictures::PictureLibrary,
) {
    update_text_cursor(state);

    // Canvas size selector at the top
    draw_canvas_size_selector(ui, state);

    ui.separator();

    ui.horizontal(|ui| {
        // Left: Tools panel
        ui.vertical(|ui| {
            draw_tools_panel(ui, state);
        });

        ui.separator();

        // Center: Canvas
        ui.vertical(|ui| {
            draw_canvas(ui, state);
        });

        ui.separator();

        // Right: Palette
        ui.vertical(|ui| {
            draw_palette_panel(ui, state);
        });
    });

    ui.separator();

    // Bottom: Save buttons
    draw_paint_action_buttons(ui, state, library, picture_library);
}

/// Toggles the text cursor and refreshes the text preview while the text tool is active.
fn update_text_cursor(state: &mut TextureGeneratorState) {
    if state.canvas.tool == PaintTool::Text {
        let now = Instant::now();
        if now.duration_since(state.last_cursor_toggle).as_millis() >= 500 {
            state.canvas.toggle_text_cursor();
            state.last_cursor_toggle = now;
        }

        // Update text preview when typing
        state.canvas.preview_pixels = state.canvas.generate_text_preview();
    } else {
        // Clear text preview when not in text tool
        state.canvas.preview_pixels = None;
    }
}

/// Draws the canvas size selector row (presets + Custom... button).
fn draw_canvas_size_selector(ui: &mut egui::Ui, state: &mut TextureGeneratorState) {
    ui.horizontal(|ui| {
        ui.label("Canvas Size:");

        // Show current size
        ui.weak(format!(
            "{}×{}",
            state.canvas.size.width, state.canvas.size.height
        ));

        ui.separator();

        // Preset size buttons (show first few)
        // Clone the current canvas size for comparison
        let current_width = state.canvas.size.width;
        let current_height = state.canvas.size.height;
        let presets_to_show: Vec<_> = state.available_sizes.iter().take(7).copied().collect();

        for size in presets_to_show {
            let label = size.size_label();
            let is_selected = current_width == size.width && current_height == size.height;

            if is_selected {
                ui.weak(label);
            } else if ui.button(label).clicked() {
                state.change_canvas_size(size);
            }
        }

        // Custom size button
        if ui.button("Custom...").clicked() {
            state.show_size_dialog = true;
            state.custom_width = state.canvas.size.width;
            state.custom_height = state.canvas.size.height;
        }
    });
}

/// Draws the left-hand tools panel (tool selection and per-tool options).
fn draw_tools_panel(ui: &mut egui::Ui, state: &mut TextureGeneratorState) {
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
        draw_brush_size_control(ui, state);
    }

    // Text tool input
    if state.canvas.tool == PaintTool::Text {
        draw_text_tool_controls(ui, state);
    }

    // Shape mode (for rect/circle)
    if matches!(state.canvas.tool, PaintTool::Rectangle | PaintTool::Circle) {
        draw_shape_mode_control(ui, state);
    }

    ui.add_space(8.0);
    draw_mirror_controls(ui, state);

    ui.add_space(8.0);
    draw_view_controls(ui, state);

    ui.add_space(8.0);
    ui.separator();

    draw_undo_redo_controls(ui, state);
}

/// Draws the brush/eraser size slider.
fn draw_brush_size_control(ui: &mut egui::Ui, state: &mut TextureGeneratorState) {
    ui.horizontal(|ui| {
        ui.label("Size:");
        ui.add(
            egui::DragValue::new(&mut state.canvas.brush_size)
                .range(1..=8)
                .speed(0.1),
        );
    });
}

/// Draws the text tool controls (font size, text input, cursor info, instructions).
fn draw_text_tool_controls(ui: &mut egui::Ui, state: &mut TextureGeneratorState) {
    // Font size selector
    ui.horizontal(|ui| {
        ui.label("Size:");
        for size in [1, 2, 3] {
            let label = match size {
                1 => "S",
                2 => "M",
                3 => "L",
                _ => "?",
            };
            if ui
                .selectable_label(state.canvas.text_font_size == size, label)
                .clicked()
            {
                state.canvas.text_font_size = size;
            }
        }
    });

    ui.horizontal(|ui| {
        ui.label("Text:");
        let response = ui.text_edit_singleline(&mut state.canvas.text_input);
        let text_entered = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

        // Render text when Enter is pressed
        if text_entered
            && !state.canvas.text_input.is_empty()
            && let Some((cx, cy)) = state.canvas.text_cursor
        {
            state.canvas.save_state();
            let (new_x, new_y) = state
                .canvas
                .draw_text(cx, cy, &state.canvas.text_input.clone());
            // Clear input but keep cursor at new position for next text
            state.canvas.text_input.clear();
            state.canvas.text_cursor = Some((new_x, new_y));
        }
    });

    // Show cursor position
    if let Some((cx, cy)) = state.canvas.text_cursor {
        ui.label(format!("Cursor: ({}, {})", cx, cy));
    } else {
        ui.label("Click canvas to set cursor");
    }

    // Instructions
    ui.label("Type text and press Enter to render");
    ui.label("Click multiple times for multiple locations");
    ui.label("Supported: 0-9, A-Z, .,!?,:-");
}

/// Draws the filled/outline mode selector for shape tools.
fn draw_shape_mode_control(ui: &mut egui::Ui, state: &mut TextureGeneratorState) {
    ui.horizontal(|ui| {
        ui.label("Mode:");
        ui.selectable_value(&mut state.canvas.shape_mode, ShapeMode::Filled, "Fill");
        ui.selectable_value(&mut state.canvas.shape_mode, ShapeMode::Outline, "Outline");
    });
}

/// Draws the mirror axis checkboxes.
fn draw_mirror_controls(ui: &mut egui::Ui, state: &mut TextureGeneratorState) {
    ui.label("Mirror");
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.canvas.mirror_x, "X");
        ui.checkbox(&mut state.canvas.mirror_y, "Y");
    });
}

/// Draws the view controls (zoom selector + grid toggle).
fn draw_view_controls(ui: &mut egui::Ui, state: &mut TextureGeneratorState) {
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
}

/// Draws the undo/redo buttons, history counts, and Clear button.
fn draw_undo_redo_controls(ui: &mut egui::Ui, state: &mut TextureGeneratorState) {
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
}

/// Draws the right-hand palette panel (color grid + current color editor).
fn draw_palette_panel(ui: &mut egui::Ui, state: &mut TextureGeneratorState) {
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

                let btn_color =
                    egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);

                // Use checkerboard for transparent
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(cell_size, cell_size), egui::Sense::click());

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
                    egui::Stroke::new(2.0_f32, egui::Color32::WHITE)
                } else {
                    egui::Stroke::new(1.0_f32, egui::Color32::DARK_GRAY)
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
}

/// Draws the bottom action buttons (save as texture, export as picture, save library).
fn draw_paint_action_buttons(
    ui: &mut egui::Ui,
    state: &mut TextureGeneratorState,
    library: &mut TextureLibrary,
    picture_library: &mut crate::pictures::PictureLibrary,
) {
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

        ui.separator();

        // Export to Picture Library button
        let width = state.canvas.size.width;
        let height = state.canvas.size.height;
        let export_label =
            egui::RichText::new(format!("📷 Export as Picture ({}×{})", width, height))
                .color(egui::Color32::from_rgb(100, 200, 255));
        if ui.button(export_label).clicked() {
            // Export canvas to picture library at actual size
            let name = if state.editing.name.is_empty() || state.editing.name == "New Texture" {
                format!("Picture {}", picture_library.len() + 1)
            } else {
                state.editing.name.clone()
            };

            match picture_library.import_rgba(
                &name,
                width as u32,
                height as u32,
                &state.canvas.pixels,
            ) {
                Some(id) => {
                    state.set_status(format!(
                        "Exported as {}×{} picture ID {}",
                        width, height, id
                    ));
                    // Mark picture library as needing save
                    let _ = picture_library.save();
                }
                None => {
                    state.set_status("Failed to export picture".to_string());
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
    let canvas_width = state.canvas.size.width as f32 * zoom;
    let canvas_height = state.canvas.size.height as f32 * zoom;

    egui::ScrollArea::both().max_height(400.0).show(ui, |ui| {
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(canvas_width, canvas_height),
            egui::Sense::click_and_drag(),
        );

        let painter = ui.painter_at(rect);

        // Draw canvas pixels
        let pixels_to_draw = if let Some(ref preview) = state.canvas.preview_pixels {
            preview
        } else {
            &state.canvas.pixels
        };

        let width = state.canvas.size.width;
        let height = state.canvas.size.height;

        for y in 0..height {
            for x in 0..width {
                let idx = (y as usize * width as usize + x as usize) * 4;
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
            for i in 0..=width {
                let x = rect.min.x + i as f32 * zoom;
                painter.line_segment(
                    [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                    egui::Stroke::new(1.0_f32, grid_color),
                );
            }
            for i in 0..=height {
                let y = rect.min.y + i as f32 * zoom;
                painter.line_segment(
                    [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
                    egui::Stroke::new(1.0_f32, grid_color),
                );
            }
        }

        // Draw mirror axes when mirroring is enabled
        let mirror_color = egui::Color32::from_rgba_unmultiplied(255, 100, 100, 180);

        // Mirror X axis (horizontal line at center - mirrors vertically across this line)
        if state.canvas.mirror_x && height > 1 {
            let center_y = rect.min.y + (height as f32 / 2.0) * zoom;
            painter.line_segment(
                [
                    egui::pos2(rect.min.x, center_y),
                    egui::pos2(rect.max.x, center_y),
                ],
                egui::Stroke::new(2.0_f32, mirror_color),
            );
        }

        // Mirror Y axis (vertical line at center - mirrors horizontally across this line)
        if state.canvas.mirror_y && width > 1 {
            let center_x = rect.min.x + (width as f32 / 2.0) * zoom;
            painter.line_segment(
                [
                    egui::pos2(center_x, rect.min.y),
                    egui::pos2(center_x, rect.max.y),
                ],
                egui::Stroke::new(2.0_f32, mirror_color),
            );
        }

        // Draw text cursor if text tool is active
        if state.canvas.tool == PaintTool::Text
            && let Some((cx, cy)) = state.canvas.text_cursor
        {
            let scale = state.canvas.text_font_size as f32;
            let cursor_width = 6.0 * scale;
            let cursor_height = 7.0 * scale;
            let cursor_color = egui::Color32::YELLOW;

            // Only draw if cursor is visible
            if state.canvas.text_cursor_visible {
                let cursor_min =
                    egui::pos2(rect.min.x + cx as f32 * zoom, rect.min.y + cy as f32 * zoom);
                let cursor_max = egui::pos2(
                    cursor_min.x + cursor_width * zoom,
                    cursor_min.y + cursor_height * zoom,
                );
                let cursor_rect = egui::Rect::from_min_max(cursor_min, cursor_max);

                // Draw hollow cursor outline
                painter.rect_stroke(
                    cursor_rect,
                    0.0,
                    egui::Stroke::new(2.0_f32, cursor_color),
                    egui::StrokeKind::Outside,
                );
            }
        }

        // Border
        painter.rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(1.0_f32, egui::Color32::GRAY),
            egui::StrokeKind::Outside,
        );

        // Handle mouse interaction
        if let Some(pos) = response.interact_pointer_pos() {
            let local_x = ((pos.x - rect.min.x) / zoom).floor() as i32;
            let local_y = ((pos.y - rect.min.y) / zoom).floor() as i32;

            if local_x >= 0 && local_x < width as i32 && local_y >= 0 && local_y < height as i32 {
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
                        PaintTool::Text => {
                            // Set text cursor position
                            state.canvas.text_cursor = Some((x, y));
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
                        PaintTool::Text
                        | PaintTool::Pencil
                        | PaintTool::Brush
                        | PaintTool::Eraser
                        | PaintTool::Fill
                        | PaintTool::Eyedropper => {}
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

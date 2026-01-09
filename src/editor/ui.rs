//! Egui UI for the in-game model editor.

use super::rasterizer::render_model;
use super::{EditorState, EditorTool, MirrorAxis};
use crate::storage::model_format::LibraryManager;
use crate::sub_voxel::{Color, ModelResolution, PALETTE_SIZE};
use egui_winit_vulkano::egui;

/// Draws all editor UI panels.
/// Returns true if a model was saved or loaded (for potential registry updates).
pub fn draw_editor_ui(
    ctx: &egui::Context,
    editor: &mut EditorState,
    library: &LibraryManager,
    author_name: &str,
) -> EditorAction {
    if !editor.active {
        return EditorAction::None;
    }

    let mut action = EditorAction::None;

    // Main editor window with tools and model info
    egui::Window::new("Model Editor")
        .default_pos(egui::pos2(10.0, 10.0))
        .default_size(egui::vec2(250.0, 400.0))
        .show(ctx, |ui| {
            // Model name (max 32 characters)
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.add(egui::TextEdit::singleline(&mut editor.scratch_pad.name).char_limit(32));
            });

            ui.separator();

            // Tools
            ui.label("Tools:");
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(editor.tool == EditorTool::Pencil, "🖊 Pencil")
                    .clicked()
                {
                    editor.tool = EditorTool::Pencil;
                }
                if ui
                    .selectable_label(editor.tool == EditorTool::Eraser, "🧹 Eraser")
                    .clicked()
                {
                    editor.tool = EditorTool::Eraser;
                }
                if ui
                    .selectable_label(editor.tool == EditorTool::Eyedropper, "💧 Pick")
                    .clicked()
                {
                    editor.tool = EditorTool::Eyedropper;
                }
            });
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(editor.tool == EditorTool::Cube, "⬜ Cube")
                    .on_hover_text("Place filled cube shapes")
                    .clicked()
                {
                    editor.tool = EditorTool::Cube;
                }
                if ui
                    .selectable_label(editor.tool == EditorTool::Sphere, "⚪ Sphere")
                    .on_hover_text("Place filled sphere shapes")
                    .clicked()
                {
                    editor.tool = EditorTool::Sphere;
                }
            });
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(editor.tool == EditorTool::ColorChange, "🎨 Color")
                    .on_hover_text("Change color of existing voxels")
                    .clicked()
                {
                    editor.tool = EditorTool::ColorChange;
                }
                if ui
                    .selectable_label(editor.tool == EditorTool::PaintBucket, "🪣 Fill")
                    .on_hover_text("Flood fill connected voxels")
                    .clicked()
                {
                    editor.tool = EditorTool::PaintBucket;
                }
            });
            // Shape size slider (only shown for Cube/Sphere tools)
            if editor.tool == EditorTool::Cube || editor.tool == EditorTool::Sphere {
                let model_size = editor.scratch_pad.size() as i32;
                ui.horizontal(|ui| {
                    ui.label("Size:");
                    let mut size = editor.shape_size as i32;
                    if ui
                        .add(egui::Slider::new(&mut size, 1..=model_size).suffix(" voxels"))
                        .changed()
                    {
                        editor.shape_size = size as usize;
                    }
                });
            }

            ui.separator();

            // Mirror mode toggles
            ui.label("Mirror Mode:");
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(editor.mirror_axes[0], "X")
                    .on_hover_text("Mirror edits across X axis (left/right)")
                    .clicked()
                {
                    editor.toggle_mirror(MirrorAxis::X);
                }
                if ui
                    .selectable_label(editor.mirror_axes[1], "Y")
                    .on_hover_text("Mirror edits across Y axis (up/down)")
                    .clicked()
                {
                    editor.toggle_mirror(MirrorAxis::Y);
                }
                if ui
                    .selectable_label(editor.mirror_axes[2], "Z")
                    .on_hover_text("Mirror edits across Z axis (front/back)")
                    .clicked()
                {
                    editor.toggle_mirror(MirrorAxis::Z);
                }
                // Show count when mirroring is active
                if editor.is_mirror_enabled() {
                    let count = 1 << editor.mirror_axes.iter().filter(|&&x| x).count();
                    ui.label(format!("({}x)", count));
                }
            });

            ui.separator();

            // Instructions
            ui.label("Controls:");
            ui.label("  Left Click: Place/Pick");
            ui.label("  Right Click: Erase");
            ui.label("  Middle Click: Pick Color");
            ui.label("  Drag: Rotate View");
            ui.label("  Scroll: Zoom");
            ui.label("  Cmd/Ctrl+Z: Undo");
            ui.label("  Cmd/Ctrl+Shift+Z: Redo");
            ui.label("  N or Esc: Close Editor");

            ui.separator();

            // Model properties
            ui.collapsing("Properties", |ui| {
                // Resolution selector (triggers confirmation dialog)
                ui.horizontal(|ui| {
                    ui.label("Resolution:");
                    let current_res = editor.scratch_pad.resolution;
                    egui::ComboBox::from_id_salt("res_combo")
                        .selected_text(format!(
                            "{}³ ({} voxels)",
                            current_res.size(),
                            current_res.volume()
                        ))
                        .show_ui(ui, |ui| {
                            for res in [
                                ModelResolution::Low,
                                ModelResolution::Medium,
                                ModelResolution::High,
                            ] {
                                let label = format!("{}³ ({} voxels)", res.size(), res.volume());
                                if ui.selectable_label(current_res == res, label).clicked()
                                    && res != current_res
                                {
                                    // Store pending change, show confirmation dialog
                                    editor.pending_resolution_change = Some(res);
                                }
                            }
                        });
                });

                ui.checkbox(&mut editor.scratch_pad.rotatable, "Rotatable");
                ui.checkbox(
                    &mut editor.scratch_pad.requires_ground_support,
                    "Requires Ground",
                );
                ui.checkbox(&mut editor.scratch_pad.no_collision, "No Collision")
                    .on_hover_text("Disable physics collision (walk-through)");

                // Light emission toggle
                let has_emission = editor.scratch_pad.emission.is_some();
                let mut emit_enabled = has_emission;
                if ui.checkbox(&mut emit_enabled, "Emits Light").changed() {
                    if emit_enabled && !has_emission {
                        editor.scratch_pad.emission = Some(Color::rgb(255, 200, 100));
                    } else if !emit_enabled {
                        editor.scratch_pad.emission = None;
                    }
                }

                if let Some(ref mut emission) = editor.scratch_pad.emission {
                    ui.horizontal(|ui| {
                        ui.label("Color:");
                        let mut color = [
                            emission.r as f32 / 255.0,
                            emission.g as f32 / 255.0,
                            emission.b as f32 / 255.0,
                        ];
                        if ui.color_edit_button_rgb(&mut color).changed() {
                            emission.r = (color[0] * 255.0) as u8;
                            emission.g = (color[1] * 255.0) as u8;
                            emission.b = (color[2] * 255.0) as u8;
                        }
                    });
                }

                // Light source settings
                ui.separator();
                ui.checkbox(&mut editor.scratch_pad.is_light_source, "Light Source");
                if editor.scratch_pad.is_light_source {
                    // Light mode selector
                    ui.horizontal(|ui| {
                        ui.label("Mode:");
                        egui::ComboBox::from_id_salt("light_mode")
                            .selected_text(format!("{:?}", editor.scratch_pad.light_mode))
                            .show_ui(ui, |ui| {
                                use crate::sub_voxel::LightMode;
                                ui.selectable_value(
                                    &mut editor.scratch_pad.light_mode,
                                    LightMode::Steady,
                                    "Steady",
                                );
                                ui.selectable_value(
                                    &mut editor.scratch_pad.light_mode,
                                    LightMode::Pulse,
                                    "Pulse",
                                );
                                ui.selectable_value(
                                    &mut editor.scratch_pad.light_mode,
                                    LightMode::Flicker,
                                    "Flicker",
                                );
                                ui.selectable_value(
                                    &mut editor.scratch_pad.light_mode,
                                    LightMode::Candle,
                                    "Candle",
                                );
                                ui.selectable_value(
                                    &mut editor.scratch_pad.light_mode,
                                    LightMode::Strobe,
                                    "Strobe",
                                );
                                ui.selectable_value(
                                    &mut editor.scratch_pad.light_mode,
                                    LightMode::Breathe,
                                    "Breathe",
                                );
                                ui.selectable_value(
                                    &mut editor.scratch_pad.light_mode,
                                    LightMode::Sparkle,
                                    "Sparkle",
                                );
                                ui.selectable_value(
                                    &mut editor.scratch_pad.light_mode,
                                    LightMode::Wave,
                                    "Wave",
                                );
                                ui.selectable_value(
                                    &mut editor.scratch_pad.light_mode,
                                    LightMode::WarmUp,
                                    "Warm Up",
                                );
                                ui.selectable_value(
                                    &mut editor.scratch_pad.light_mode,
                                    LightMode::Arc,
                                    "Arc",
                                );
                            });
                    });
                    ui.add(
                        egui::Slider::new(&mut editor.scratch_pad.light_radius, 1.0..=32.0)
                            .text("Radius"),
                    );
                    ui.add(
                        egui::Slider::new(&mut editor.scratch_pad.light_intensity, 0.1..=2.0)
                            .text("Intensity"),
                    );
                }
            });

            ui.separator();

            // Actions
            // Undo/Redo buttons
            ui.horizontal(|ui| {
                let undo_label = format!("↩ Undo ({})", editor.history.undo_count());
                if ui
                    .add_enabled(editor.can_undo(), egui::Button::new(&undo_label))
                    .on_hover_text("Undo last change (Cmd+Z / Ctrl+Z)")
                    .clicked()
                {
                    editor.undo();
                }
                let redo_label = format!("↪ Redo ({})", editor.history.redo_count());
                if ui
                    .add_enabled(editor.can_redo(), egui::Button::new(&redo_label))
                    .on_hover_text("Redo last undone change (Cmd+Shift+Z / Ctrl+Shift+Z)")
                    .clicked()
                {
                    editor.redo();
                }
            });

            ui.separator();

            // Actions
            ui.horizontal(|ui| {
                if ui.button("New...").clicked() {
                    editor.show_new_model_dialog = true;
                }
                if ui.button("Clear").clicked() {
                    editor.clear_voxels();
                }
                if ui
                    .button("⟳ Rotate")
                    .on_hover_text("Rotate model 90° clockwise around Y axis")
                    .clicked()
                {
                    editor.rotate_model_y90();
                }
            });

            ui.horizontal(|ui| {
                if ui.button("Save to Library").clicked() {
                    // Check if model already exists
                    if library.model_exists(&editor.scratch_pad.name) {
                        editor.show_overwrite_confirm = true;
                    } else {
                        // New model - save directly
                        action = save_model_to_library(editor, library, author_name);
                    }
                }
            });

            // Voxel count - use model's actual volume
            let voxel_count: usize = editor
                .scratch_pad
                .voxels
                .iter()
                .filter(|&&v| v != 0)
                .count();
            let max_voxels = editor.scratch_pad.resolution.volume();
            ui.label(format!("Voxels: {}/{}", voxel_count, max_voxels));
        });

    // Palette window
    egui::Window::new("Palette")
        .default_pos(egui::pos2(10.0, 420.0))
        .default_size(egui::vec2(250.0, 200.0))
        .show(ctx, |ui| {
            draw_palette_grid(ui, editor);
        });

    // Library window
    egui::Window::new("Library")
        .default_pos(egui::pos2(270.0, 10.0))
        .default_size(egui::vec2(200.0, 300.0))
        .show(ctx, |ui| {
            if let Some(loaded_action) = draw_library_list(ui, editor, library) {
                action = loaded_action;
            }
        });

    // Overwrite confirmation dialog
    if editor.show_overwrite_confirm {
        egui::Window::new("Confirm Overwrite")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(format!(
                    "A model named '{}' already exists.",
                    editor.scratch_pad.name
                ));
                ui.label("Do you want to overwrite it?");
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Overwrite").clicked() {
                        editor.show_overwrite_confirm = false;
                        action = save_model_to_library(editor, library, author_name);
                    }
                    if ui.button("Cancel").clicked() {
                        editor.show_overwrite_confirm = false;
                    }
                });
            });
    }

    // Delete confirmation dialog
    if let Some(ref model_name) = editor.pending_delete_model.clone() {
        egui::Window::new("Confirm Delete")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(format!("Delete model '{}'?", model_name));
                ui.label("This action cannot be undone.");
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        editor.pending_delete_model = None;
                        if let Err(e) = library.delete_model(model_name) {
                            eprintln!("[Editor] Failed to delete model '{}': {}", model_name, e);
                        } else {
                            println!("[Editor] Deleted model '{}'", model_name);
                            action = EditorAction::ModelDeleted;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        editor.pending_delete_model = None;
                    }
                });
            });
    }

    // New model dialog with resolution selection
    if editor.show_new_model_dialog {
        egui::Window::new("New Model")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label("Select resolution for new model:");
                ui.add_space(10.0);

                // Resolution radio buttons
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut editor.new_model_resolution,
                        ModelResolution::Low,
                        "8³ (512 voxels)",
                    );
                });
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut editor.new_model_resolution,
                        ModelResolution::Medium,
                        "16³ (4,096 voxels)",
                    );
                });
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut editor.new_model_resolution,
                        ModelResolution::High,
                        "32³ (32,768 voxels)",
                    );
                });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Create").clicked() {
                        editor.show_new_model_dialog = false;
                        editor.new_model_with_resolution("untitled", editor.new_model_resolution);
                    }
                    if ui.button("Cancel").clicked() {
                        editor.show_new_model_dialog = false;
                    }
                });
            });
    }

    // Resolution change confirmation dialog
    if let Some(new_res) = editor.pending_resolution_change {
        let current_res = editor.scratch_pad.resolution;
        let is_upscale = new_res.size() > current_res.size();

        egui::Window::new("Change Resolution?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                if is_upscale {
                    ui.label("Upscaling will subdivide each voxel.");
                    ui.label("Your model will be preserved at higher detail.");
                } else {
                    ui.label("Downscaling will sample the model.");
                    ui.label("Some detail may be lost.");
                }
                ui.add_space(5.0);
                ui.label(format!("{}³ → {}³", current_res.size(), new_res.size()));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let button_text = if is_upscale { "Upscale" } else { "Downscale" };
                    if ui.button(button_text).clicked() {
                        editor.change_resolution(new_res);
                        editor.pending_resolution_change = None;
                    }
                    if ui.button("Cancel").clicked() {
                        editor.pending_resolution_change = None;
                    }
                });
            });
    }

    action
}

/// Draws the 32-color palette grid with color editing.
fn draw_palette_grid(ui: &mut egui::Ui, editor: &mut EditorState) {
    ui.label("Select Color (click to select, right-click to edit):");

    // 4x8 grid of palette colors (32 total)
    for row in 0..8 {
        ui.horizontal(|ui| {
            for col in 0..4 {
                let idx = row * 4 + col;
                let color = &mut editor.scratch_pad.palette[idx];
                let is_selected = editor.selected_palette_index == idx as u8;

                // Draw color swatch
                let size = egui::vec2(32.0, 32.0);
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

                // Background for transparency
                if color.a < 255 {
                    ui.painter().rect_filled(
                        rect,
                        egui::CornerRadius::ZERO,
                        egui::Color32::from_gray(40),
                    );
                }

                // The color itself
                let egui_color =
                    egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a);
                ui.painter()
                    .rect_filled(rect, egui::CornerRadius::ZERO, egui_color);

                // Selection border
                if is_selected {
                    ui.painter().rect_stroke(
                        rect,
                        egui::CornerRadius::ZERO,
                        egui::Stroke::new(2.0, egui::Color32::WHITE),
                        egui::StrokeKind::Inside,
                    );
                } else {
                    ui.painter().rect_stroke(
                        rect,
                        egui::CornerRadius::ZERO,
                        egui::Stroke::new(1.0, egui::Color32::from_gray(80)),
                        egui::StrokeKind::Inside,
                    );
                }

                // Index label for slot 0 (air)
                if idx == 0 {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "AIR",
                        egui::FontId::proportional(8.0),
                        egui::Color32::WHITE,
                    );
                }

                // Click to select
                if response.clicked() && idx > 0 {
                    editor.selected_palette_index = idx as u8;
                }

                // Right-click to edit color
                if response.secondary_clicked() && idx > 0 {
                    // Open color picker popup
                }

                // Show tooltip with index
                response.on_hover_text(format!("Color {}", idx));
            }
        });
    }

    // Color editor for selected color
    if editor.selected_palette_index > 0 && (editor.selected_palette_index as usize) < PALETTE_SIZE
    {
        ui.separator();
        ui.label(format!("Edit Color {}:", editor.selected_palette_index));

        let idx = editor.selected_palette_index as usize;
        let color = &mut editor.scratch_pad.palette[idx];

        // RGBA sliders
        let mut rgba = [
            color.r as f32 / 255.0,
            color.g as f32 / 255.0,
            color.b as f32 / 255.0,
            color.a as f32 / 255.0,
        ];

        if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
            color.r = (rgba[0] * 255.0) as u8;
            color.g = (rgba[1] * 255.0) as u8;
            color.b = (rgba[2] * 255.0) as u8;
            color.a = (rgba[3] * 255.0) as u8;
        }
    }
}

/// Draws the library file list with load/delete options.
/// Returns Some(action) if a model was loaded.
fn draw_library_list(
    ui: &mut egui::Ui,
    editor: &mut EditorState,
    library: &LibraryManager,
) -> Option<EditorAction> {
    let mut action = None;

    ui.label("Saved Models:");

    match library.list_models() {
        Ok(models) if models.is_empty() => {
            ui.label("(No models saved yet)");
        }
        Ok(models) => {
            // Show scrollbar if more than 20 items (roughly 20 * 18px per row)
            let max_height = if models.len() > 20 {
                360.0
            } else {
                f32::INFINITY
            };
            egui::ScrollArea::vertical()
                .max_height(max_height)
                .show(ui, |ui| {
                    for name in models {
                        ui.horizontal(|ui| {
                            if ui.button("Load").clicked() {
                                match library.load_model(&name) {
                                    Ok(model) => {
                                        editor.load_model(&model);
                                        println!("[Editor] Loaded model '{}'", name);
                                        action = Some(EditorAction::ModelLoaded);
                                    }
                                    Err(e) => {
                                        eprintln!("[Editor] Failed to load '{}': {}", name, e);
                                    }
                                }
                            }
                            if ui
                                .button("🗑")
                                .on_hover_text("Delete this model from library")
                                .clicked()
                            {
                                editor.pending_delete_model = Some(name.clone());
                            }
                            // Truncate display name to 32 characters
                            let display_name = if name.len() > 32 {
                                format!("{}...", &name[..29])
                            } else {
                                name.clone()
                            };
                            ui.label(&display_name);
                        });
                    }
                });
        }
        Err(e) => {
            ui.colored_label(egui::Color32::RED, format!("Error: {}", e));
        }
    }

    action
}

/// Actions that can result from editor UI interactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAction {
    None,
    ModelSaved,
    ModelLoaded,
    ModelDeleted,
    /// Place the edited model in the world and close the editor.
    PlaceInWorld,
}

/// Draws the interactive 3D model editor viewport using software rasterizer with z-buffer.
pub fn draw_model_preview(ctx: &egui::Context, editor: &mut EditorState) {
    if !editor.active {
        return;
    }

    // 3D Viewport window - larger default size to accommodate 16³ models
    egui::Window::new("3D Viewport")
        .default_pos(egui::pos2(270.0, 320.0))
        .default_size(egui::vec2(600.0, 600.0))
        .resizable(true)
        .show(ctx, |ui| {
            let available = ui.available_size();
            // Reserve space for info text at bottom (about 50 pixels)
            let viewport_height = (available.y - 50.0).max(200.0);
            let viewport_size = egui::vec2(available.x, viewport_height);

            // Calculate render dimensions (use integer pixel sizes)
            let render_width = viewport_size.x as usize;
            let render_height = viewport_size.y as usize;

            // Handle camera rotation with drag (check before allocating space)
            let (rect, response) =
                ui.allocate_exact_size(viewport_size, egui::Sense::click_and_drag());

            if response.dragged_by(egui::PointerButton::Secondary)
                || response.dragged_by(egui::PointerButton::Middle)
            {
                let delta = response.drag_delta();
                editor.orbit_yaw += delta.x * 0.01;
            }

            // Handle scroll to zoom
            if response.hovered() {
                let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
                if scroll_delta != 0.0 {
                    // Zoom in/out based on scroll direction
                    editor.orbit_distance -= scroll_delta * 0.05;
                    // Clamp to reasonable range based on model resolution
                    let (min_zoom, max_zoom) = match editor.scratch_pad.resolution {
                        ModelResolution::Low => (8.0, 50.0),
                        ModelResolution::Medium => (12.0, 80.0),
                        ModelResolution::High => (20.0, 150.0),
                    };
                    editor.orbit_distance = editor.orbit_distance.clamp(min_zoom, max_zoom);
                }
            }

            // Get hovered voxel/normal for highlight rendering
            let hovered_voxel = editor.hovered_voxel.map(|v| [v.x, v.y, v.z]);
            let hovered_normal = editor.hovered_normal.map(|n| [n.x, n.y, n.z]);

            // Render the model using the software rasterizer
            let render_result = render_model(
                &editor.scratch_pad,
                render_width,
                render_height,
                editor.orbit_yaw,
                editor.orbit_distance,
                hovered_voxel,
                hovered_normal,
                editor.mirror_axes,
            );

            // Create texture from rendered image
            let texture_id = ctx.load_texture(
                "editor_viewport",
                render_result.image,
                egui::TextureOptions::NEAREST,
            );

            // Draw the rendered image
            let painter = ui.painter_at(rect);
            painter.image(
                texture_id.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );

            // Draw axis labels on top of the rendered image
            draw_axis_labels(
                ui,
                &painter,
                rect,
                editor.orbit_yaw,
                editor.orbit_distance,
                editor.scratch_pad.size(),
                editor.scratch_pad.resolution.center_f32(),
            );

            // Handle mouse interaction using hit map
            if let Some(pointer_pos) = response.hover_pos() {
                // Convert screen position to image coordinates
                let local_x = (pointer_pos.x - rect.min.x) as usize;
                let local_y = (pointer_pos.y - rect.min.y) as usize;

                if local_x < render_result.width && local_y < render_result.height {
                    let hit_idx = local_y * render_result.width + local_x;
                    if let Some(hit_info) = render_result.hit_map.get(hit_idx).and_then(|h| *h) {
                        editor.hovered_voxel = Some(nalgebra::Vector3::new(
                            hit_info.voxel[0],
                            hit_info.voxel[1],
                            hit_info.voxel[2],
                        ));
                        if hit_info.is_floor {
                            // Floor tiles: place at the tile position, normal is +Y
                            editor.hovered_normal = Some(nalgebra::Vector3::new(0, 1, 0));
                        } else {
                            editor.hovered_normal = Some(nalgebra::Vector3::new(
                                hit_info.normal[0],
                                hit_info.normal[1],
                                hit_info.normal[2],
                            ));
                        }
                    } else {
                        editor.hovered_voxel = None;
                        editor.hovered_normal = None;
                    }
                } else {
                    editor.hovered_voxel = None;
                    editor.hovered_normal = None;
                }
            } else {
                editor.hovered_voxel = None;
                editor.hovered_normal = None;
            }

            // Handle clicks
            if response.clicked() {
                editor.on_left_click();
            }
            if response.secondary_clicked() {
                editor.on_right_click();
            }
            if response.middle_clicked() {
                editor.on_middle_click();
            }

            // Info panel
            ui.separator();
            ui.horizontal(|ui| {
                if let Some(voxel) = editor.hovered_voxel {
                    ui.label(format!("Pos: ({}, {}, {})", voxel.x, voxel.y, voxel.z));
                    let idx = editor.scratch_pad.get_voxel(
                        voxel.x as usize,
                        voxel.y as usize,
                        voxel.z as usize,
                    );
                    if idx > 0 {
                        ui.label(format!("| Color: {}", idx));
                    } else {
                        ui.label("| (empty - click to place)");
                    }
                } else {
                    ui.label("Hover over grid to select");
                }
            });
            ui.label("Left: Place | Right: Erase | Middle: Pick Color");
        });
}

/// Draws axis labels on top of the rendered viewport.
fn draw_axis_labels(
    _ui: &egui::Ui,
    painter: &egui::Painter,
    rect: egui::Rect,
    orbit_yaw: f32,
    orbit_distance: f32,
    model_size: usize,
    model_center: f32,
) {
    use super::rasterizer::DEFAULT_ORBIT_DISTANCE;

    // Calculate where axis endpoints would be in screen space
    // Must match the projection in render_model exactly
    let size = rect.width().min(rect.height()) - 20.0;
    let zoom_factor = DEFAULT_ORBIT_DISTANCE / orbit_distance;
    let cell_size = (size / 16.0) * zoom_factor;
    let center_x = rect.center().x;
    let center_y = rect.center().y;

    let cos_yaw = orbit_yaw.cos();
    let sin_yaw = orbit_yaw.sin();

    let base_x = [cell_size * 0.866, cell_size * 0.5];
    let base_z = [-cell_size * 0.866, cell_size * 0.5];

    let iso_x = [
        base_x[0] * cos_yaw - base_z[0] * sin_yaw,
        base_x[1] * cos_yaw - base_z[1] * sin_yaw,
    ];
    let iso_z = [
        base_x[0] * sin_yaw + base_z[0] * cos_yaw,
        base_x[1] * sin_yaw + base_z[1] * cos_yaw,
    ];
    let iso_y = [0.0, -cell_size];

    // Project function for label positions
    let project = |x: f32, y: f32, z: f32| -> egui::Pos2 {
        let cx = x - model_center;
        let cy = y - model_center;
        let cz = z - model_center;
        egui::pos2(
            center_x + iso_x[0] * cx + iso_z[0] * cz,
            center_y + iso_x[1] * cx + iso_z[1] * cz + iso_y[1] * cy,
        )
    };

    // Labels at end of axis lines (scale with model size ~12.5%)
    let axis_length = model_size as f32 * 0.125;
    let x_end = project(axis_length, 0.0, 0.0);
    let y_end = project(0.0, axis_length, 0.0);
    let z_end = project(0.0, 0.0, axis_length);

    painter.text(
        x_end + egui::vec2(5.0, 0.0),
        egui::Align2::LEFT_CENTER,
        "X",
        egui::FontId::proportional(12.0),
        egui::Color32::RED,
    );
    painter.text(
        y_end + egui::vec2(0.0, -5.0),
        egui::Align2::CENTER_BOTTOM,
        "Y",
        egui::FontId::proportional(12.0),
        egui::Color32::GREEN,
    );
    painter.text(
        z_end + egui::vec2(-5.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        "Z",
        egui::FontId::proportional(12.0),
        egui::Color32::BLUE,
    );
}

/// Saves the model to the library and generates its sprite.
///
/// Returns `EditorAction::ModelSaved` on success.
fn save_model_to_library(
    editor: &mut EditorState,
    library: &LibraryManager,
    author_name: &str,
) -> EditorAction {
    editor.finalize_model();

    if let Err(e) = library.save_model(&editor.scratch_pad, author_name) {
        eprintln!("[Editor] Failed to save model: {}", e);
        return EditorAction::None;
    }

    println!(
        "[Editor] Saved model '{}' to library",
        editor.scratch_pad.name
    );

    // Sprite generation is now handled in app_hud.rs after model ID is assigned
    EditorAction::ModelSaved
}

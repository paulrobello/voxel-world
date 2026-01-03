//! Egui UI for the in-game model editor.

use super::{EditorState, EditorTool};
use crate::storage::model_format::LibraryManager;
use crate::sub_voxel::{Color, PALETTE_SIZE, SUB_VOXEL_SIZE};
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
            // Model name
            ui.horizontal(|ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut editor.scratch_pad.name);
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

            ui.separator();

            // Instructions
            ui.label("Controls:");
            ui.label("  Left Click: Place/Pick");
            ui.label("  Right Click: Erase");
            ui.label("  Middle Click: Pick Color");
            ui.label("  Drag: Rotate View");
            ui.label("  Scroll: Zoom");
            ui.label("  N or Esc: Close Editor");

            ui.separator();

            // Model properties
            ui.collapsing("Properties", |ui| {
                ui.checkbox(&mut editor.scratch_pad.rotatable, "Rotatable");
                ui.checkbox(
                    &mut editor.scratch_pad.requires_ground_support,
                    "Requires Ground",
                );

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
            });

            ui.separator();

            // Actions
            ui.horizontal(|ui| {
                if ui.button("New").clicked() {
                    editor.new_model("untitled");
                }
                if ui.button("Clear").clicked() {
                    editor.scratch_pad.voxels =
                        [0; SUB_VOXEL_SIZE * SUB_VOXEL_SIZE * SUB_VOXEL_SIZE];
                }
            });

            ui.horizontal(|ui| {
                if ui.button("Save to Library").clicked() {
                    editor.finalize_model();
                    if let Err(e) = library.save_model(&editor.scratch_pad, author_name) {
                        eprintln!("[Editor] Failed to save model: {}", e);
                    } else {
                        println!(
                            "[Editor] Saved model '{}' to library",
                            editor.scratch_pad.name
                        );
                        action = EditorAction::ModelSaved;
                    }
                }
            });

            // Voxel count
            let voxel_count: usize = editor
                .scratch_pad
                .voxels
                .iter()
                .filter(|&&v| v != 0)
                .count();
            ui.label(format!("Voxels: {}/512", voxel_count));
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

    action
}

/// Draws the 16-color palette grid with color editing.
fn draw_palette_grid(ui: &mut egui::Ui, editor: &mut EditorState) {
    ui.label("Select Color (click to select, right-click to edit):");

    // 4x4 grid of palette colors
    for row in 0..4 {
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
            egui::ScrollArea::vertical()
                .max_height(200.0)
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
                            ui.label(&name);
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
}

/// Draws the 3D model preview in the editor.
/// This is a simple 2D representation of the 8x8x8 grid.
pub fn draw_model_preview(ctx: &egui::Context, editor: &EditorState) {
    if !editor.active {
        return;
    }

    // Center preview window
    egui::Window::new("Preview")
        .default_pos(egui::pos2(500.0, 100.0))
        .default_size(egui::vec2(300.0, 300.0))
        .show(ctx, |ui| {
            let available = ui.available_size();
            let size = available.x.min(available.y).min(280.0);
            let (rect, _response) =
                ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());

            let painter = ui.painter();

            // Background
            painter.rect_filled(rect, egui::CornerRadius::ZERO, egui::Color32::from_gray(30));

            // Simple top-down view for now
            let grid_offset = rect.min + egui::vec2(size * 0.1, size * 0.1);
            let grid_cell = (size * 0.8) / SUB_VOXEL_SIZE as f32;

            // Draw from back to front for proper layering
            for y in (0..SUB_VOXEL_SIZE).rev() {
                for z in 0..SUB_VOXEL_SIZE {
                    for x in 0..SUB_VOXEL_SIZE {
                        let idx = editor.scratch_pad.get_voxel(x, y, z);
                        if idx == 0 {
                            continue;
                        }

                        let color = &editor.scratch_pad.palette[idx as usize];
                        let egui_color = egui::Color32::from_rgba_unmultiplied(
                            color.r, color.g, color.b, color.a,
                        );

                        // Simple 3D projection (isometric-like)
                        let px = grid_offset.x + (x as f32 - z as f32 * 0.3) * grid_cell;
                        let py = grid_offset.y + (z as f32 * 0.5 - y as f32 * 0.7) * grid_cell;

                        let voxel_rect = egui::Rect::from_min_size(
                            egui::pos2(px, py),
                            egui::vec2(grid_cell * 0.9, grid_cell * 0.9),
                        );

                        painter.rect_filled(voxel_rect, egui::CornerRadius::ZERO, egui_color);
                    }
                }
            }

            // Draw grid border
            painter.rect_stroke(
                rect,
                egui::CornerRadius::ZERO,
                egui::Stroke::new(1.0, egui::Color32::from_gray(60)),
                egui::StrokeKind::Inside,
            );

            // Hover info
            if let Some(voxel) = editor.hovered_voxel {
                ui.label(format!("Hover: ({}, {}, {})", voxel.x, voxel.y, voxel.z));
            }
        });
}

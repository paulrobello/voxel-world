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

/// Draws the interactive 3D model editor viewport.
/// Supports clicking to place/erase voxels and dragging to rotate view.
pub fn draw_model_preview(ctx: &egui::Context, editor: &mut EditorState) {
    if !editor.active {
        return;
    }

    // 3D Viewport window
    egui::Window::new("3D Viewport")
        .default_pos(egui::pos2(270.0, 320.0))
        .default_size(egui::vec2(400.0, 400.0))
        .resizable(true)
        .show(ctx, |ui| {
            let available = ui.available_size();
            let size = available.x.min(available.y).max(200.0);
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click_and_drag());

            let painter = ui.painter_at(rect);
            let center = rect.center();
            let scale = size / 16.0; // Scale factor for voxels

            // Dark background
            painter.rect_filled(rect, egui::CornerRadius::ZERO, egui::Color32::from_gray(25));

            // Handle camera orbit with right-drag
            if response.dragged_by(egui::PointerButton::Secondary) {
                let delta = response.drag_delta();
                editor.orbit_yaw += delta.x * 0.01;
                editor.orbit_pitch = (editor.orbit_pitch + delta.y * 0.01).clamp(
                    -std::f32::consts::FRAC_PI_2 + 0.1,
                    std::f32::consts::FRAC_PI_2 - 0.1,
                );
            }

            // Handle zoom with scroll
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll.abs() > 0.1 {
                editor.orbit_distance = (editor.orbit_distance - scroll * 0.05).clamp(6.0, 24.0);
            }

            // Use orbit camera angles for projection
            let cos_yaw = editor.orbit_yaw.cos();
            let sin_yaw = editor.orbit_yaw.sin();
            let cos_pitch = editor.orbit_pitch.cos();
            let sin_pitch = editor.orbit_pitch.sin();

            // Project a 3D point to 2D using current camera orientation
            let project = |x: f32, y: f32, z: f32| -> egui::Pos2 {
                // Center the model at origin
                let cx = x - 4.0;
                let cy = y - 4.0;
                let cz = z - 4.0;

                // Rotate around Y axis (yaw)
                let rx = cx * cos_yaw - cz * sin_yaw;
                let rz = cx * sin_yaw + cz * cos_yaw;

                // Rotate around X axis (pitch)
                let ry = cy * cos_pitch - rz * sin_pitch;
                let _final_z = cy * sin_pitch + rz * cos_pitch;

                // Simple perspective (orthographic for now)
                let px = rx * scale;
                let py = -ry * scale; // Flip Y for screen coords

                egui::pos2(center.x + px, center.y + py)
            };

            // Draw grid floor (Y=0 plane) with lines
            let grid_color = egui::Color32::from_rgba_unmultiplied(80, 80, 80, 100);
            for i in 0..=SUB_VOXEL_SIZE {
                let i_f = i as f32;
                // X-axis lines
                let p1 = project(i_f, 0.0, 0.0);
                let p2 = project(i_f, 0.0, 8.0);
                painter.line_segment([p1, p2], egui::Stroke::new(0.5, grid_color));
                // Z-axis lines
                let p1 = project(0.0, 0.0, i_f);
                let p2 = project(8.0, 0.0, i_f);
                painter.line_segment([p1, p2], egui::Stroke::new(0.5, grid_color));
            }

            // Draw bounding box edges
            let box_color = egui::Color32::from_rgba_unmultiplied(100, 100, 100, 80);
            let corners = [
                (0.0, 0.0, 0.0),
                (8.0, 0.0, 0.0),
                (8.0, 8.0, 0.0),
                (0.0, 8.0, 0.0),
                (0.0, 0.0, 8.0),
                (8.0, 0.0, 8.0),
                (8.0, 8.0, 8.0),
                (0.0, 8.0, 8.0),
            ];
            let edges = [
                (0, 1),
                (1, 2),
                (2, 3),
                (3, 0), // Front face
                (4, 5),
                (5, 6),
                (6, 7),
                (7, 4), // Back face
                (0, 4),
                (1, 5),
                (2, 6),
                (3, 7), // Connecting edges
            ];
            for (i, j) in edges {
                let p1 = project(corners[i].0, corners[i].1, corners[i].2);
                let p2 = project(corners[j].0, corners[j].1, corners[j].2);
                painter.line_segment([p1, p2], egui::Stroke::new(0.5, box_color));
            }

            // Collect voxels with their depths for proper sorting
            let mut voxels_to_draw: Vec<(f32, usize, usize, usize, u8)> = Vec::new();
            for y in 0..SUB_VOXEL_SIZE {
                for z in 0..SUB_VOXEL_SIZE {
                    for x in 0..SUB_VOXEL_SIZE {
                        let idx = editor.scratch_pad.get_voxel(x, y, z);
                        if idx != 0 {
                            // Calculate depth for sorting (further = drawn first)
                            let cx = x as f32 - 4.0;
                            let cy = y as f32 - 4.0;
                            let cz = z as f32 - 4.0;
                            let rz = cx * sin_yaw + cz * cos_yaw;
                            let depth = cy * sin_pitch + rz * cos_pitch;
                            voxels_to_draw.push((depth, x, y, z, idx));
                        }
                    }
                }
            }

            // Sort by depth (furthest first)
            voxels_to_draw
                .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

            // Draw voxels
            let voxel_size = scale * 0.9;
            for (_depth, x, y, z, idx) in voxels_to_draw {
                let color = &editor.scratch_pad.palette[idx as usize];
                let base_color =
                    egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a);

                // Draw as a small square at projected position
                let p = project(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                let voxel_rect =
                    egui::Rect::from_center_size(p, egui::vec2(voxel_size, voxel_size));
                painter.rect_filled(voxel_rect, egui::CornerRadius::same(1), base_color);

                // Subtle border for depth
                painter.rect_stroke(
                    voxel_rect,
                    egui::CornerRadius::same(1),
                    egui::Stroke::new(0.5, egui::Color32::from_rgba_unmultiplied(0, 0, 0, 100)),
                    egui::StrokeKind::Outside,
                );
            }

            // Handle mouse interaction for placing/erasing voxels
            if let Some(pointer_pos) = response.interact_pointer_pos() {
                // Convert screen position back to approximate voxel position
                // This is a simplified hit test - find the closest grid cell at Y=0 first
                let _rel_pos = pointer_pos - center;

                // Reverse the projection (approximate)
                // For a simpler approach, we'll project all potential voxel positions and find closest
                let mut best_dist = f32::MAX;
                let mut best_voxel: Option<(i32, i32, i32)> = None;

                // Check existing voxels first (for erase/pick)
                for y in 0..SUB_VOXEL_SIZE {
                    for z in 0..SUB_VOXEL_SIZE {
                        for x in 0..SUB_VOXEL_SIZE {
                            let p = project(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                            let dist = (p - pointer_pos).length();
                            if dist < best_dist && dist < scale * 1.5 {
                                best_dist = dist;
                                best_voxel = Some((x as i32, y as i32, z as i32));
                            }
                        }
                    }
                }

                // Update hovered voxel
                if let Some((x, y, z)) = best_voxel {
                    editor.hovered_voxel = Some(nalgebra::Vector3::new(x, y, z));

                    // Draw hover highlight
                    let p = project(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
                    let hover_rect = egui::Rect::from_center_size(
                        p,
                        egui::vec2(voxel_size * 1.2, voxel_size * 1.2),
                    );
                    painter.rect_stroke(
                        hover_rect,
                        egui::CornerRadius::same(2),
                        egui::Stroke::new(2.0, egui::Color32::YELLOW),
                        egui::StrokeKind::Outside,
                    );
                } else {
                    editor.hovered_voxel = None;
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
            } else {
                editor.hovered_voxel = None;
            }

            // Draw axis indicators
            let _axis_len = scale * 2.0;
            let origin = project(0.0, 0.0, 0.0);
            let x_end = project(2.0, 0.0, 0.0);
            let y_end = project(0.0, 2.0, 0.0);
            let z_end = project(0.0, 0.0, 2.0);

            painter.line_segment([origin, x_end], egui::Stroke::new(2.0, egui::Color32::RED));
            painter.line_segment(
                [origin, y_end],
                egui::Stroke::new(2.0, egui::Color32::GREEN),
            );
            painter.line_segment([origin, z_end], egui::Stroke::new(2.0, egui::Color32::BLUE));

            painter.text(
                x_end,
                egui::Align2::LEFT_CENTER,
                "X",
                egui::FontId::proportional(10.0),
                egui::Color32::RED,
            );
            painter.text(
                y_end,
                egui::Align2::CENTER_BOTTOM,
                "Y",
                egui::FontId::proportional(10.0),
                egui::Color32::GREEN,
            );
            painter.text(
                z_end,
                egui::Align2::RIGHT_CENTER,
                "Z",
                egui::FontId::proportional(10.0),
                egui::Color32::BLUE,
            );

            // Info panel below viewport
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
                        ui.label(format!("Color: {}", idx));
                    } else {
                        ui.label("(empty)");
                    }
                } else {
                    ui.label("Hover over grid to select voxel");
                }
            });
            ui.horizontal(|ui| {
                ui.label("Left: Place | Right: Erase | Middle: Pick | R-Drag: Rotate");
            });
        });
}

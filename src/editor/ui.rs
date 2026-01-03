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

/// Draws the interactive 3D model editor viewport with isometric cubes.
pub fn draw_model_preview(ctx: &egui::Context, editor: &mut EditorState) {
    if !editor.active {
        return;
    }

    // 3D Viewport window
    egui::Window::new("3D Viewport")
        .default_pos(egui::pos2(270.0, 320.0))
        .default_size(egui::vec2(450.0, 500.0))
        .resizable(true)
        .show(ctx, |ui| {
            let available = ui.available_size();
            let size = (available.x.min(available.y) - 60.0).max(200.0);
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click_and_drag());

            let painter = ui.painter_at(rect);
            let center = rect.center() + egui::vec2(0.0, size * 0.1); // Offset down slightly

            // Isometric projection parameters
            let cell_size = size / 14.0;
            let iso_x = egui::vec2(cell_size * 0.866, cell_size * 0.5); // cos(30), sin(30)
            let iso_z = egui::vec2(-cell_size * 0.866, cell_size * 0.5); // -cos(30), sin(30)
            let iso_y = egui::vec2(0.0, -cell_size); // straight up

            // Dark background
            painter.rect_filled(rect, egui::CornerRadius::ZERO, egui::Color32::from_gray(30));

            // Project isometric coordinates to screen
            let project = |x: f32, y: f32, z: f32| -> egui::Pos2 {
                let offset = iso_x * x + iso_z * z + iso_y * y;
                center + offset - egui::vec2(0.0, size * 0.25) // Center the grid
            };

            // Draw floor grid (Y=0 plane) - these are clickable cells
            let floor_color = egui::Color32::from_rgba_unmultiplied(60, 70, 80, 200);
            let floor_line_color = egui::Color32::from_rgba_unmultiplied(80, 90, 100, 255);

            for z in 0..SUB_VOXEL_SIZE {
                for x in 0..SUB_VOXEL_SIZE {
                    let x_f = x as f32;
                    let z_f = z as f32;

                    // Draw floor tile as a diamond
                    let p0 = project(x_f, 0.0, z_f);
                    let p1 = project(x_f + 1.0, 0.0, z_f);
                    let p2 = project(x_f + 1.0, 0.0, z_f + 1.0);
                    let p3 = project(x_f, 0.0, z_f + 1.0);

                    // Checkerboard pattern
                    let checker = if (x + z) % 2 == 0 {
                        egui::Color32::from_rgba_unmultiplied(50, 55, 65, 180)
                    } else {
                        floor_color
                    };

                    painter.add(egui::Shape::convex_polygon(
                        vec![p0, p1, p2, p3],
                        checker,
                        egui::Stroke::new(0.5, floor_line_color),
                    ));
                }
            }

            // Collect voxels for depth-sorted rendering
            struct VoxelDraw {
                x: usize,
                y: usize,
                z: usize,
                idx: u8,
                depth: f32,
            }
            let mut voxels: Vec<VoxelDraw> = Vec::new();

            for y in 0..SUB_VOXEL_SIZE {
                for z in 0..SUB_VOXEL_SIZE {
                    for x in 0..SUB_VOXEL_SIZE {
                        let idx = editor.scratch_pad.get_voxel(x, y, z);
                        if idx != 0 {
                            // Depth: further back = drawn first (painter's algorithm)
                            let depth = -(x as f32) - (z as f32) + (y as f32) * 0.01;
                            voxels.push(VoxelDraw {
                                x,
                                y,
                                z,
                                idx,
                                depth,
                            });
                        }
                    }
                }
            }

            // Sort by depth (back to front)
            voxels.sort_by(|a, b| {
                a.depth
                    .partial_cmp(&b.depth)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            // Draw each voxel as an isometric cube with 3 visible faces
            for voxel in &voxels {
                let x = voxel.x as f32;
                let y = voxel.y as f32;
                let z = voxel.z as f32;
                let color = &editor.scratch_pad.palette[voxel.idx as usize];

                // Base color and shaded variants
                let top_color =
                    egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a);
                let left_color = egui::Color32::from_rgba_unmultiplied(
                    (color.r as f32 * 0.7) as u8,
                    (color.g as f32 * 0.7) as u8,
                    (color.b as f32 * 0.7) as u8,
                    color.a,
                );
                let right_color = egui::Color32::from_rgba_unmultiplied(
                    (color.r as f32 * 0.85) as u8,
                    (color.g as f32 * 0.85) as u8,
                    (color.b as f32 * 0.85) as u8,
                    color.a,
                );

                // 8 corners of the cube
                let p000 = project(x, y, z);
                let p100 = project(x + 1.0, y, z);
                let p110 = project(x + 1.0, y + 1.0, z);
                let p010 = project(x, y + 1.0, z);
                let p001 = project(x, y, z + 1.0);
                let p101 = project(x + 1.0, y, z + 1.0);
                let p111 = project(x + 1.0, y + 1.0, z + 1.0);
                let p011 = project(x, y + 1.0, z + 1.0);

                let outline =
                    egui::Stroke::new(0.5, egui::Color32::from_rgba_unmultiplied(0, 0, 0, 100));

                // Top face (brightest)
                painter.add(egui::Shape::convex_polygon(
                    vec![p010, p110, p111, p011],
                    top_color,
                    outline,
                ));

                // Left face (darkest)
                painter.add(egui::Shape::convex_polygon(
                    vec![p000, p010, p011, p001],
                    left_color,
                    outline,
                ));

                // Right face (medium)
                painter.add(egui::Shape::convex_polygon(
                    vec![p100, p101, p111, p110],
                    right_color,
                    outline,
                ));
            }

            // Handle mouse interaction
            if let Some(pointer_pos) = response.hover_pos() {
                let mut best_dist = f32::MAX;
                let mut best_voxel: Option<(i32, i32, i32)> = None;
                let mut best_is_floor = false;

                // First check existing voxels (prioritize these for erase/pick)
                for voxel in &voxels {
                    let x = voxel.x as f32;
                    let y = voxel.y as f32;
                    let z = voxel.z as f32;

                    // Check if pointer is inside the top face
                    let p010 = project(x, y + 1.0, z);
                    let p110 = project(x + 1.0, y + 1.0, z);
                    let p111 = project(x + 1.0, y + 1.0, z + 1.0);
                    let p011 = project(x, y + 1.0, z + 1.0);

                    if point_in_quad(pointer_pos, p010, p110, p111, p011) {
                        let center_p = project(x + 0.5, y + 0.5, z + 0.5);
                        let dist = (center_p - pointer_pos).length();
                        if dist < best_dist {
                            best_dist = dist;
                            best_voxel = Some((voxel.x as i32, voxel.y as i32, voxel.z as i32));
                            best_is_floor = false;
                        }
                    }
                }

                // Then check floor tiles for placing new voxels
                for z in 0..SUB_VOXEL_SIZE {
                    for x in 0..SUB_VOXEL_SIZE {
                        let x_f = x as f32;
                        let z_f = z as f32;

                        let p0 = project(x_f, 0.0, z_f);
                        let p1 = project(x_f + 1.0, 0.0, z_f);
                        let p2 = project(x_f + 1.0, 0.0, z_f + 1.0);
                        let p3 = project(x_f, 0.0, z_f + 1.0);

                        if point_in_quad(pointer_pos, p0, p1, p2, p3) {
                            // Only use floor if no voxel is already there
                            if editor.scratch_pad.get_voxel(x, 0, z) == 0 {
                                let center_p = project(x_f + 0.5, 0.0, z_f + 0.5);
                                let dist = (center_p - pointer_pos).length();
                                if dist < best_dist {
                                    best_dist = dist;
                                    best_voxel = Some((x as i32, 0, z as i32));
                                    best_is_floor = true;
                                }
                            }
                        }
                    }
                }

                // Update hovered voxel and draw highlight
                if let Some((x, y, z)) = best_voxel {
                    editor.hovered_voxel = Some(nalgebra::Vector3::new(x, y, z));

                    // Draw highlight on the hovered cell
                    let x_f = x as f32;
                    let y_f = y as f32;
                    let z_f = z as f32;

                    if best_is_floor {
                        // Highlight floor tile
                        let p0 = project(x_f, 0.0, z_f);
                        let p1 = project(x_f + 1.0, 0.0, z_f);
                        let p2 = project(x_f + 1.0, 0.0, z_f + 1.0);
                        let p3 = project(x_f, 0.0, z_f + 1.0);
                        painter.add(egui::Shape::convex_polygon(
                            vec![p0, p1, p2, p3],
                            egui::Color32::from_rgba_unmultiplied(255, 255, 0, 60),
                            egui::Stroke::new(2.0, egui::Color32::YELLOW),
                        ));
                    } else {
                        // Highlight top of existing voxel
                        let p010 = project(x_f, y_f + 1.0, z_f);
                        let p110 = project(x_f + 1.0, y_f + 1.0, z_f);
                        let p111 = project(x_f + 1.0, y_f + 1.0, z_f + 1.0);
                        let p011 = project(x_f, y_f + 1.0, z_f + 1.0);
                        painter.add(egui::Shape::convex_polygon(
                            vec![p010, p110, p111, p011],
                            egui::Color32::from_rgba_unmultiplied(255, 255, 0, 80),
                            egui::Stroke::new(2.0, egui::Color32::YELLOW),
                        ));
                    }
                } else {
                    editor.hovered_voxel = None;
                }
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

            // Draw axis labels
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

/// Check if a point is inside a quadrilateral (for hit testing).
fn point_in_quad(
    p: egui::Pos2,
    a: egui::Pos2,
    b: egui::Pos2,
    c: egui::Pos2,
    d: egui::Pos2,
) -> bool {
    // Use cross product signs to determine if point is on the same side of all edges
    fn sign(p1: egui::Pos2, p2: egui::Pos2, p3: egui::Pos2) -> f32 {
        (p1.x - p3.x) * (p2.y - p3.y) - (p2.x - p3.x) * (p1.y - p3.y)
    }

    // Check if point is in triangle ABC
    let in_abc = {
        let d1 = sign(p, a, b);
        let d2 = sign(p, b, c);
        let d3 = sign(p, c, a);
        let has_neg = (d1 < 0.0) || (d2 < 0.0) || (d3 < 0.0);
        let has_pos = (d1 > 0.0) || (d2 > 0.0) || (d3 > 0.0);
        !(has_neg && has_pos)
    };

    // Check if point is in triangle ACD
    let in_acd = {
        let d1 = sign(p, a, c);
        let d2 = sign(p, c, d);
        let d3 = sign(p, d, a);
        let has_neg = (d1 < 0.0) || (d2 < 0.0) || (d3 < 0.0);
        let has_pos = (d1 > 0.0) || (d2 > 0.0) || (d3 > 0.0);
        !(has_neg && has_pos)
    };

    in_abc || in_acd
}

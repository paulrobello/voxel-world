//! Replace tool UI.
//!
//! Provides a settings window for finding and replacing blocks within
//! the current selection region.

use egui_winit_vulkano::egui;

use crate::chunk::BlockType;
use crate::shape_tools::ReplaceToolState;
use crate::templates::TemplateSelection;

/// Common block types available for replacement.
const BLOCK_OPTIONS: &[(BlockType, &str)] = &[
    (BlockType::Stone, "Stone"),
    (BlockType::Dirt, "Dirt"),
    (BlockType::Grass, "Grass"),
    (BlockType::Sand, "Sand"),
    (BlockType::Gravel, "Gravel"),
    (BlockType::Cobblestone, "Cobblestone"),
    (BlockType::Brick, "Brick"),
    (BlockType::Planks, "Planks"),
    (BlockType::Log, "Log"),
    (BlockType::Leaves, "Leaves"),
    (BlockType::Glass, "Glass"),
    (BlockType::Snow, "Snow"),
    (BlockType::Ice, "Ice"),
    (BlockType::Iron, "Iron"),
    (BlockType::Bedrock, "Bedrock"),
    (BlockType::Concrete, "Concrete"),
    (BlockType::Deepslate, "Deepslate"),
    (BlockType::Moss, "Moss"),
    (BlockType::MossyCobblestone, "Mossy Cobblestone"),
    (BlockType::Clay, "Clay"),
    (BlockType::Terracotta, "Terracotta"),
    (BlockType::Air, "Air"),
];

/// UI for the replace tool.
pub struct ReplaceToolUI;

impl ReplaceToolUI {
    /// Draw the replace tool settings window.
    ///
    /// Button clicks set flags in the state (preview_requested, execute_requested)
    /// which are processed by the input handler.
    pub fn draw(ctx: &egui::Context, state: &mut ReplaceToolState, selection: &TemplateSelection) {
        if !state.active {
            return;
        }

        egui::Window::new("Replace Tool")
            .default_pos(egui::pos2(ctx.screen_rect().width() - 250.0, 100.0))
            .default_size(egui::vec2(230.0, 300.0))
            .resizable(false)
            .collapsible(true)
            .show(ctx, |ui| {
                ui.heading("Find & Replace Blocks");
                ui.add_space(8.0);

                // Check if selection exists
                let has_selection = selection.pos1.is_some() && selection.pos2.is_some();

                if !has_selection {
                    ui.colored_label(egui::Color32::YELLOW, "No selection active");
                    ui.small("Use V key to enter selection mode");
                    ui.small("Then set pos1 and pos2 corners");
                    ui.add_space(8.0);
                }

                // Source block selector
                ui.label("Source Block (find):");
                egui::ComboBox::from_id_salt("source_block")
                    .selected_text(block_name(state.source_block))
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        for (block, name) in BLOCK_OPTIONS {
                            ui.selectable_value(&mut state.source_block, *block, *name);
                        }
                    });

                ui.add_space(8.0);

                // Target block selector
                ui.label("Target Block (replace with):");
                egui::ComboBox::from_id_salt("target_block")
                    .selected_text(block_name(state.target_block))
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        for (block, name) in BLOCK_OPTIONS {
                            ui.selectable_value(&mut state.target_block, *block, *name);
                        }
                    });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                // Preview section
                if has_selection {
                    if let Some((min, max)) = selection.bounds() {
                        let width = (max.x - min.x).abs() + 1;
                        let height = (max.y - min.y).abs() + 1;
                        let depth = (max.z - min.z).abs() + 1;
                        let total = width * height * depth;
                        ui.horizontal(|ui| {
                            ui.label("Selection:");
                            ui.label(format!("{}×{}×{} ({} blocks)", width, height, depth, total));
                        });
                    }

                    ui.add_space(4.0);

                    // Match count
                    if state.match_count > 0 {
                        ui.horizontal(|ui| {
                            ui.label("Matches:");
                            ui.colored_label(
                                egui::Color32::LIGHT_GREEN,
                                format!("{} blocks", state.match_count),
                            );
                        });
                        if state.preview_truncated {
                            ui.colored_label(
                                egui::Color32::YELLOW,
                                "Preview truncated (>4096 blocks)",
                            );
                        }
                    } else {
                        ui.label("Matches: (click Preview to scan)");
                    }

                    ui.add_space(8.0);

                    // Action buttons
                    ui.horizontal(|ui| {
                        if ui.button("Preview").clicked() {
                            state.preview_requested = true;
                        }
                        let replace_enabled = state.match_count > 0;
                        ui.add_enabled_ui(replace_enabled, |ui| {
                            if ui.button("Replace All").clicked() {
                                state.execute_requested = true;
                            }
                        });
                    });
                } else {
                    ui.colored_label(egui::Color32::GRAY, "Select a region first");
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // Cancel button
                if ui.button("Cancel (Esc)").clicked() {
                    state.deactivate();
                }

                ui.add_space(4.0);
                ui.small("Replaces ALL matching blocks in selection");
            });
    }
}

/// Get display name for a block type.
fn block_name(block: BlockType) -> &'static str {
    for (b, name) in BLOCK_OPTIONS {
        if *b == block {
            return name;
        }
    }
    "Unknown"
}

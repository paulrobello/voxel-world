//! Picture browser UI for selecting pictures for frame placement.

use super::{Picture, PictureLibrary};
use egui_winit_vulkano::egui;
use std::collections::HashMap;

/// Picture UI state and dialog management.
pub struct PictureUi {
    /// Whether the picture browser is currently open.
    pub browser_open: bool,

    /// Currently selected picture ID (for frame placement).
    pub selected_picture_id: Option<u32>,

    /// Cached picture list for display.
    pub pictures: Vec<PictureInfo>,

    /// Error message to display in browser.
    pub error_message: Option<String>,

    /// Search/filter text for pictures.
    pub search_text: String,

    /// Cached thumbnail textures (picture ID -> texture handle).
    pub thumbnail_cache: HashMap<u32, egui::TextureHandle>,
}

/// Display information for a picture.
#[derive(Clone, Debug)]
pub struct PictureInfo {
    pub id: u32,
    pub name: String,
    pub width: u16,
    pub height: u16,
}

impl Default for PictureUi {
    fn default() -> Self {
        Self::new()
    }
}

impl PictureUi {
    /// Creates a new picture UI state.
    pub fn new() -> Self {
        Self {
            browser_open: false,
            selected_picture_id: None,
            pictures: Vec::new(),
            error_message: None,
            search_text: String::new(),
            thumbnail_cache: HashMap::new(),
        }
    }

    /// Toggles the picture browser.
    pub fn toggle_browser(&mut self) {
        self.browser_open = !self.browser_open;
        if self.browser_open {
            self.error_message = None;
        }
    }

    /// Refreshes the picture list from the library.
    pub fn refresh_pictures(&mut self, library: &PictureLibrary) {
        self.pictures.clear();
        self.error_message = None;

        for picture in library.iter() {
            self.pictures.push(PictureInfo {
                id: picture.id,
                name: picture.name.clone(),
                width: picture.width,
                height: picture.height,
            });
        }

        // Sort by name
        self.pictures.sort_by(|a, b| a.name.cmp(&b.name));
    }

    /// Clears the cached thumbnail for a picture (forces reload on next display).
    pub fn clear_thumbnail_cache(&mut self, id: u32) {
        self.thumbnail_cache.remove(&id);
    }
}

/// Draws the picture browser window.
pub fn draw_picture_browser(
    ctx: &egui::Context,
    ui_state: &mut PictureUi,
    current_selection: Option<u32>,
    library: &PictureLibrary,
) -> Option<PictureBrowserAction> {
    let mut action = None;

    if !ui_state.browser_open {
        return action;
    }

    // Refresh picture list when opening
    if ui_state.pictures.is_empty() && ui_state.error_message.is_none() {
        ui_state.refresh_pictures(library);
    }

    // Track whether we need to refresh after the window
    let mut needs_refresh = false;

    let browser_open = &mut ui_state.browser_open;

    egui::Window::new("Picture Browser")
        .default_pos(egui::pos2(10.0, 10.0))
        .default_size(egui::vec2(350.0, 550.0))
        .open(browser_open)
        .show(ctx, |ui| {
            // Current selection info
            ui.heading("Current Selection");

            if let Some(id) = current_selection {
                if let Some(picture) = library.get(id) {
                    ui.label(format!("📷 {} ({}×{})", picture.name, picture.width, picture.height));
                    ui.horizontal(|ui| {
                        if ui.button("✖ Clear").clicked() {
                            action = Some(PictureBrowserAction::ClearSelection);
                        }
                    });
                } else {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 100, 100),
                        "⚠ Selected picture not found",
                    );
                }
            } else {
                ui.colored_label(
                    egui::Color32::from_gray(180),
                    "No picture selected (frames will be empty)",
                );
            }

            ui.separator();

            // Error message display
            if let Some(ref error) = ui_state.error_message {
                ui.colored_label(egui::Color32::from_rgb(255, 100, 100), error);
                ui.separator();
            }

            // Pictures list
            ui.heading("Pictures");

            ui.horizontal(|ui| {
                ui.label("🔍 Search:");
                ui.text_edit_singleline(&mut ui_state.search_text);
                if ui.button("🔄 Refresh").clicked() {
                    needs_refresh = true;
                }
            });

            ui.separator();

            // Filter pictures based on search text
            let search_lower = ui_state.search_text.to_lowercase();
            let filtered_pictures: Vec<&PictureInfo> = if search_lower.is_empty() {
                ui_state.pictures.iter().collect()
            } else {
                ui_state
                    .pictures
                    .iter()
                    .filter(|info| info.name.to_lowercase().contains(&search_lower))
                    .collect()
            };

            if filtered_pictures.is_empty() {
                if ui_state.pictures.is_empty() {
                    ui.colored_label(
                        egui::Color32::from_gray(150),
                        "No pictures. Create one in the Texture Editor (P key).",
                    );
                } else {
                    ui.label("No pictures match your search.");
                }
            } else {
                egui::ScrollArea::vertical()
                    .max_height(400.0)
                    .show(ui, |ui| {
                        for info in &filtered_pictures {
                            let is_selected = current_selection == Some(info.id);

                            // Picture item
                            egui::Grid::new(format!("picture_{}", info.id))
                                .num_columns(2)
                                .spacing([10.0, 4.0])
                                .show(ui, |ui| {
                                    // Selection indicator
                                    if is_selected {
                                        ui.colored_label(
                                            egui::Color32::from_rgb(100, 255, 100),
                                            "✓",
                                        );
                                    } else {
                                        ui.label("  ");
                                    }

                                    // Picture info
                                    ui.vertical(|ui| {
                                        ui.label(format!("📷 {}", info.name));
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{}×{} pixels",
                                                info.width, info.height
                                            ))
                                            .size(12.0)
                                            .color(egui::Color32::from_gray(150)),
                                        );
                                    });

                                    ui.end_row();

                                    // Action buttons
                                    ui.label("  ");
                                    ui.horizontal(|ui| {
                                        if ui.button("Select").clicked() {
                                            action = Some(PictureBrowserAction::SelectPicture(info.id));
                                        }
                                        if is_selected {
                                            if ui.button("✖ Clear").clicked() {
                                                action = Some(PictureBrowserAction::ClearSelection);
                                            }
                                        }
                                    });
                                    ui.end_row();
                                });
                            ui.separator();
                        }
                    });
            }

            ui.separator();

            // Instructions
            ui.label(
                egui::RichText::new(
                    "Select a picture to use when placing frames. \
                    Press P to open the Texture Editor and create new pictures.",
                )
                .size(13.0)
                .color(egui::Color32::from_gray(150)),
            );
        });

    // Refresh after window closes if requested
    if needs_refresh {
        ui_state.refresh_pictures(library);
    }

    action
}

/// Result of picture browser interaction.
#[derive(Debug, Clone, Copy)]
pub enum PictureBrowserAction {
    /// User selected a picture for frame placement.
    SelectPicture(u32),
    /// User cleared the picture selection.
    ClearSelection,
}

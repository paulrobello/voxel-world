//! Stencil browser UI for managing saved and active stencils.

use super::{StencilInfo, StencilLibrary, StencilManager};
use crate::templates::TemplateSelection;
use egui_winit_vulkano::egui;
use std::collections::HashMap;

/// Stencil UI state and dialog management.
#[allow(dead_code)]
pub struct StencilUi {
    /// Whether the stencil browser is currently open.
    pub browser_open: bool,

    /// Whether the save stencil dialog is open.
    pub save_dialog_open: bool,

    /// Stencil name being edited in the save dialog.
    pub edit_name: String,

    /// Stencil tags being edited in the save dialog (comma-separated).
    pub edit_tags: String,

    /// Currently selected stencil in the browser (for info display).
    pub selected_stencil: Option<String>,

    /// Cached stencil infos for display.
    pub stencil_infos: Vec<StencilInfo>,

    /// Error message to display in browser.
    pub error_message: Option<String>,

    /// Search/filter text for stencils.
    pub search_text: String,

    /// Cached thumbnail textures (stencil name -> texture handle).
    pub thumbnail_cache: HashMap<String, egui::TextureHandle>,
}

impl Default for StencilUi {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl StencilUi {
    /// Creates a new stencil UI state.
    pub fn new() -> Self {
        Self {
            browser_open: false,
            save_dialog_open: false,
            edit_name: String::new(),
            edit_tags: String::new(),
            selected_stencil: None,
            stencil_infos: Vec::new(),
            error_message: None,
            search_text: String::new(),
            thumbnail_cache: HashMap::new(),
        }
    }

    /// Toggles the stencil browser.
    pub fn toggle_browser(&mut self) {
        self.browser_open = !self.browser_open;
        if self.browser_open {
            self.error_message = None;
        }
    }

    /// Opens the save stencil dialog.
    pub fn open_save_dialog(&mut self, default_name: &str) {
        self.save_dialog_open = true;
        self.edit_name = default_name.to_string();
        self.edit_tags.clear();
        self.error_message = None;
    }

    /// Closes the save stencil dialog.
    pub fn close_save_dialog(&mut self) {
        self.save_dialog_open = false;
        self.edit_name.clear();
        self.edit_tags.clear();
        self.error_message = None;
    }

    /// Refreshes the stencil list from the library.
    pub fn refresh_stencils(&mut self, library: &StencilLibrary) {
        self.stencil_infos.clear();
        self.error_message = None;

        match library.list_stencils() {
            Ok(names) => {
                for name in names {
                    match library.get_stencil_info(&name) {
                        Ok(info) => self.stencil_infos.push(info),
                        Err(e) => {
                            self.error_message =
                                Some(format!("Failed to load info for '{}': {}", name, e));
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to list stencils: {}", e));
            }
        }
    }

    /// Clears the cached thumbnail for a stencil (forces reload on next display).
    pub fn clear_thumbnail_cache(&mut self, name: &str) {
        self.thumbnail_cache.remove(name);
    }
}

/// Result of stencil browser interaction.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum StencilBrowserAction {
    /// No action.
    None,
    /// User clicked "Create Stencil" button.
    OpenSaveDialog,
    /// User confirmed save with name and tags.
    SaveStencil { name: String, tags: Vec<String> },
    /// User clicked load button for a stencil.
    LoadStencil(String),
    /// User clicked delete button for a saved stencil.
    DeleteStencil(String),
    /// User clicked remove button for an active stencil.
    RemoveActiveStencil(u64),
    /// User requested to clear the selection.
    ClearSelection,
    /// User clicked regenerate thumbnail button for a stencil.
    RegenerateThumbnail(String),
    /// User requested to clear all active stencils.
    ClearAllActive,
    /// User changed global opacity.
    SetGlobalOpacity(f32),
    /// User toggled render mode.
    ToggleRenderMode,
}

/// Draws the stencil browser window.
/// Returns the action taken by the user.
#[allow(dead_code)]
pub fn draw_stencil_browser(
    ctx: &egui::Context,
    ui_state: &mut StencilUi,
    selection: &TemplateSelection,
    library: &StencilLibrary,
    manager: &StencilManager,
) -> StencilBrowserAction {
    let mut action = StencilBrowserAction::None;

    if !ui_state.browser_open {
        return action;
    }

    // Refresh stencil list when opening
    if ui_state.stencil_infos.is_empty() && ui_state.error_message.is_none() {
        ui_state.refresh_stencils(library);
    }

    // Track whether we need to refresh after the window
    let mut needs_refresh = false;

    let browser_open = &mut ui_state.browser_open;

    egui::Window::new("Stencil Browser")
        .default_pos(egui::pos2(370.0, 10.0)) // Offset from template browser
        .default_size(egui::vec2(380.0, 600.0))
        .open(browser_open)
        .show(ctx, |ui| {
            // === ACTIVE STENCILS SECTION ===
            ui.heading("Active Stencils");

            // Render mode and opacity controls
            ui.horizontal(|ui| {
                ui.label("Mode:");
                if ui.button(manager.render_mode.display_name()).clicked() {
                    action = StencilBrowserAction::ToggleRenderMode;
                }
                ui.label("Opacity:");
                let mut opacity = manager.global_opacity;
                if ui.add(egui::Slider::new(&mut opacity, 0.3..=0.8).step_by(0.1)).changed() {
                    action = StencilBrowserAction::SetGlobalOpacity(opacity);
                }
            });

            if manager.is_empty() {
                ui.colored_label(
                    egui::Color32::from_gray(150),
                    "No active stencils. Load one from the library below.",
                );
            } else {
                ui.label(format!("{} active stencil(s)", manager.count()));

                egui::ScrollArea::vertical()
                    .id_salt("active_stencils")
                    .max_height(120.0)
                    .show(ui, |ui| {
                        for (id, name) in manager.list_active() {
                            ui.horizontal(|ui| {
                                ui.label(format!("#{} - {}", id, name));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.button("✕ Remove").clicked() {
                                            action = StencilBrowserAction::RemoveActiveStencil(id);
                                        }
                                    },
                                );
                            });
                        }
                    });

                if ui.button("🗑 Clear All Active").clicked() {
                    action = StencilBrowserAction::ClearAllActive;
                }
            }

            ui.separator();

            // === CREATE FROM SELECTION ===
            ui.heading("Create from Selection");

            // Selection mode status indicator
            ui.horizontal(|ui| {
                if selection.visual_mode {
                    ui.colored_label(
                        egui::Color32::from_rgb(100, 255, 100),
                        "● Selection Mode: ON",
                    );
                    ui.label("(Left-click: pos1, Right-click: pos2, V: exit)");
                } else {
                    ui.colored_label(egui::Color32::from_gray(150), "○ Selection Mode: OFF");
                    ui.label("(Press V to enter)");
                }
            });

            if let Some((min, max)) = selection.bounds() {
                ui.label(format!("Position 1: ({}, {}, {})", min.x, min.y, min.z));
                ui.label(format!("Position 2: ({}, {}, {})", max.x, max.y, max.z));

                if let Ok((w, h, d)) = selection.validate_size() {
                    ui.label(format!("Dimensions: {}×{}×{}", w, h, d));
                    if let Some(vol) = selection.volume() {
                        ui.label(format!("Volume: {} blocks", vol));
                    }

                    ui.horizontal(|ui| {
                        if ui.button("💾 Create Stencil").clicked() {
                            action = StencilBrowserAction::OpenSaveDialog;
                        }
                        if ui.button("🗑 Clear Selection").clicked() {
                            action = StencilBrowserAction::ClearSelection;
                        }
                    });
                } else {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 200, 100),
                        "⚠ Selection too large (max 128×128×128)",
                    );
                }
            } else {
                ui.colored_label(
                    egui::Color32::from_gray(180),
                    "No selection. Press V and click to select.",
                );
            }

            ui.separator();

            // Error message display
            if let Some(ref error) = ui_state.error_message {
                ui.colored_label(egui::Color32::from_rgb(255, 100, 100), error);
                ui.separator();
            }

            // === SAVED STENCILS LIBRARY ===
            ui.heading("Saved Stencils");

            ui.horizontal(|ui| {
                ui.label("🔍 Search:");
                ui.text_edit_singleline(&mut ui_state.search_text);
                if ui.button("🔄 Refresh").clicked() {
                    needs_refresh = true;
                }
            });

            ui.separator();

            // Filter stencils based on search text
            let search_lower = ui_state.search_text.to_lowercase();
            let filtered_stencils: Vec<&StencilInfo> = if search_lower.is_empty() {
                ui_state.stencil_infos.iter().collect()
            } else {
                ui_state
                    .stencil_infos
                    .iter()
                    .filter(|info| {
                        // Search in name
                        if info.name.to_lowercase().contains(&search_lower) {
                            return true;
                        }
                        // Search in tags
                        for tag in &info.tags {
                            if tag.to_lowercase().contains(&search_lower) {
                                return true;
                            }
                        }
                        // Search in dimensions (e.g., "16x16")
                        let dim_str = format!("{}x{}x{}", info.width, info.height, info.depth);
                        if dim_str.contains(&search_lower) {
                            return true;
                        }
                        false
                    })
                    .collect()
            };

            if ui_state.stencil_infos.is_empty() {
                ui.label("No stencils found");
            } else if filtered_stencils.is_empty() {
                ui.label(format!("No stencils match '{}'", ui_state.search_text));
            } else {
                ui.label(format!(
                    "Showing {} of {} stencils",
                    filtered_stencils.len(),
                    ui_state.stencil_infos.len()
                ));

                egui::ScrollArea::vertical()
                    .id_salt("saved_stencils")
                    .max_height(250.0)
                    .show(ui, |ui| {
                        for info in filtered_stencils {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    // Display thumbnail
                                    if let Some(ref thumb_path) = info.thumbnail_path {
                                        // Try to get from cache first
                                        let texture = if let Some(cached) =
                                            ui_state.thumbnail_cache.get(&info.name)
                                        {
                                            Some(cached.clone())
                                        } else {
                                            // Load and cache
                                            if let Some(tex) =
                                                load_thumbnail_texture(ctx, thumb_path, &info.name)
                                            {
                                                ui_state
                                                    .thumbnail_cache
                                                    .insert(info.name.clone(), tex.clone());
                                                Some(tex)
                                            } else {
                                                None
                                            }
                                        };

                                        if let Some(tex) = texture {
                                            ui.image(&tex);
                                        } else {
                                            show_placeholder_thumbnail(ui);
                                        }
                                    } else {
                                        show_placeholder_thumbnail(ui);
                                    }

                                    // Stencil info and buttons
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.heading(&info.name);
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if ui.button("🗑 Delete").clicked() {
                                                        action =
                                                            StencilBrowserAction::DeleteStencil(
                                                                info.name.clone(),
                                                            );
                                                    }
                                                    if ui
                                                        .button("🔄 Thumb")
                                                        .on_hover_text(
                                                            "Regenerate thumbnail image",
                                                        )
                                                        .clicked()
                                                    {
                                                        action = StencilBrowserAction::RegenerateThumbnail(
                                                            info.name.clone(),
                                                        );
                                                    }
                                                    if ui.button("📥 Load").clicked() {
                                                        action =
                                                            StencilBrowserAction::LoadStencil(
                                                                info.name.clone(),
                                                            );
                                                    }
                                                },
                                            );
                                        });

                                        ui.label(format!(
                                            "Size: {} ({} positions)",
                                            info.dimensions_str(),
                                            info.position_count_str()
                                        ));

                                        if !info.tags.is_empty() {
                                            ui.label(format!("Tags: {}", info.tags.join(", ")));
                                        }
                                    });
                                });
                            });
                        }
                    });
            }
        });

    // Handle refresh after the window to avoid borrow conflicts
    if needs_refresh {
        ui_state.refresh_stencils(library);
    }

    action
}

/// Draws the save stencil dialog.
/// Returns Some((name, tags)) if the user confirmed the save.
#[allow(dead_code)]
pub fn draw_save_stencil_dialog(
    ctx: &egui::Context,
    ui_state: &mut StencilUi,
) -> Option<(String, Vec<String>)> {
    if !ui_state.save_dialog_open {
        return None;
    }

    let mut result = None;

    egui::Window::new("Create Stencil")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label("Stencil Name:");
            let name_edit = egui::TextEdit::singleline(&mut ui_state.edit_name).char_limit(32);
            let name_response = ui.add(name_edit);

            // Auto-focus the name field when dialog opens
            if ui_state.save_dialog_open {
                name_response.request_focus();
            }

            ui.label("Tags (comma-separated):");
            ui.add(
                egui::TextEdit::singleline(&mut ui_state.edit_tags)
                    .hint_text("building, wall, arch"),
            );

            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("✓ Create").clicked() && !ui_state.edit_name.trim().is_empty() {
                    let tags: Vec<String> = ui_state
                        .edit_tags
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();

                    result = Some((ui_state.edit_name.clone(), tags));
                    ui_state.close_save_dialog();
                }

                if ui.button("✗ Cancel").clicked() {
                    ui_state.close_save_dialog();
                }
            });

            if ui_state.edit_name.trim().is_empty() {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 200, 100),
                    "⚠ Name cannot be empty",
                );
            }
        });

    result
}

/// Loads a thumbnail image as an egui texture.
#[allow(dead_code)]
fn load_thumbnail_texture(
    ctx: &egui::Context,
    path: &std::path::Path,
    name: &str,
) -> Option<egui::TextureHandle> {
    // Try to load the image
    let img = match image::open(path) {
        Ok(img) => img,
        Err(_) => return None,
    };

    let rgba_img = img.to_rgba8();
    let size = [rgba_img.width() as usize, rgba_img.height() as usize];
    let pixels = rgba_img.into_raw();

    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
    let texture = ctx.load_texture(
        format!("stencil_thumbnail_{}", name),
        color_image,
        egui::TextureOptions::LINEAR,
    );

    Some(texture)
}

/// Shows a placeholder for missing thumbnails.
#[allow(dead_code)]
fn show_placeholder_thumbnail(ui: &mut egui::Ui) {
    ui.allocate_ui_with_layout(
        egui::vec2(64.0, 64.0),
        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
        |ui| {
            ui.colored_label(egui::Color32::from_rgb(0, 200, 200), "◇");
        },
    );
}

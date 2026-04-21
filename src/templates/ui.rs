//! Template browser UI for managing saved templates.

use super::{TemplateInfo, TemplateLibrary, TemplateSelection};
use egui_winit_vulkano::egui;
use std::collections::HashMap;

/// Template UI state and dialog management.
pub struct TemplateUi {
    /// Whether the template browser is currently open.
    pub browser_open: bool,

    /// Whether the save template dialog is open.
    pub save_dialog_open: bool,

    /// Template name being edited in the save dialog.
    pub edit_name: String,

    /// Template tags being edited in the save dialog (comma-separated).
    pub edit_tags: String,

    /// Currently selected template in the browser (for info display).
    pub selected_template: Option<String>,

    /// Cached template infos for display.
    pub template_infos: Vec<TemplateInfo>,

    /// Error message to display in browser.
    pub error_message: Option<String>,

    /// Search/filter text for templates.
    pub search_text: String,

    /// Cached thumbnail textures (template name -> texture handle).
    pub thumbnail_cache: HashMap<String, egui::TextureHandle>,
}

impl Default for TemplateUi {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateUi {
    /// Creates a new template UI state.
    pub fn new() -> Self {
        Self {
            browser_open: false,
            save_dialog_open: false,
            edit_name: String::new(),
            edit_tags: String::new(),
            selected_template: None,
            template_infos: Vec::new(),
            error_message: None,
            search_text: String::new(),
            thumbnail_cache: HashMap::new(),
        }
    }

    /// Toggles the template browser.
    pub fn toggle_browser(&mut self) {
        self.browser_open = !self.browser_open;
        if self.browser_open {
            self.error_message = None;
        }
    }

    /// Opens the save template dialog.
    pub fn open_save_dialog(&mut self, default_name: &str) {
        self.save_dialog_open = true;
        self.edit_name = default_name.to_string();
        self.edit_tags.clear();
        self.error_message = None;
    }

    /// Closes the save template dialog.
    pub fn close_save_dialog(&mut self) {
        self.save_dialog_open = false;
        self.edit_name.clear();
        self.edit_tags.clear();
        self.error_message = None;
    }

    /// Refreshes the template list from the library.
    pub fn refresh_templates(&mut self, library: &TemplateLibrary) {
        self.template_infos.clear();
        self.error_message = None;

        match library.list_templates() {
            Ok(names) => {
                for name in names {
                    match library.get_template_info(&name) {
                        Ok(info) => self.template_infos.push(info),
                        Err(e) => {
                            self.error_message =
                                Some(format!("Failed to load info for '{}': {}", name, e));
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to list templates: {}", e));
            }
        }
    }

    /// Clears the cached thumbnail for a template (forces reload on next display).
    pub fn clear_thumbnail_cache(&mut self, name: &str) {
        self.thumbnail_cache.remove(name);
    }
}

/// Result of template browser interaction.
#[derive(Debug, Clone)]
pub enum TemplateBrowserAction {
    /// No action.
    None,
    /// User clicked "Save Template" button.
    OpenSaveDialog,
    /// User confirmed save with name and tags.
    SaveTemplate { name: String, tags: Vec<String> },
    /// User clicked load button for a template.
    LoadTemplate(String),
    /// User clicked delete button for a template.
    DeleteTemplate(String),
    /// User requested to clear the selection.
    ClearSelection,
    /// User clicked regenerate thumbnail button for a template.
    RegenerateThumbnail(String),
    /// User clicked "To Stencil" button to convert template to stencil.
    ToStencil(String),
}

/// Draws the template browser window.
/// Returns the action taken by the user.
pub fn draw_template_browser(
    ctx: &egui::Context,
    ui_state: &mut TemplateUi,
    selection: &TemplateSelection,
    library: &TemplateLibrary,
) -> TemplateBrowserAction {
    let mut action = TemplateBrowserAction::None;

    if !ui_state.browser_open {
        return action;
    }

    // Refresh template list when opening
    if ui_state.template_infos.is_empty() && ui_state.error_message.is_none() {
        ui_state.refresh_templates(library);
    }

    // Track whether we need to refresh after the window
    let mut needs_refresh = false;

    let browser_open = &mut ui_state.browser_open;

    egui::Window::new("Template Browser")
        .default_pos(egui::pos2(10.0, 10.0))
        .default_size(egui::vec2(350.0, 550.0))
        .open(browser_open)
        .show(ctx, |ui| {
            // Current selection info
            ui.heading("Current Selection");

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
            ui.separator();

            if let Some((min, max)) = selection.bounds() {
                ui.label(format!("Position 1: ({}, {}, {})", min.x, min.y, min.z));
                ui.label(format!("Position 2: ({}, {}, {})", max.x, max.y, max.z));

                if let Ok((w, h, d)) = selection.validate_size() {
                    ui.label(format!("Dimensions: {}×{}×{}", w, h, d));
                    if let Some(vol) = selection.volume() {
                        ui.label(format!("Volume: {} blocks", vol));
                    }

                    ui.horizontal(|ui| {
                        if ui.button("💾 Save as Template").clicked() {
                            action = TemplateBrowserAction::OpenSaveDialog;
                        }
                        if ui.button("🗑 Clear Selection").clicked() {
                            action = TemplateBrowserAction::ClearSelection;
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
                    "No selection. Use /select pos1 and /select pos2",
                );
            }

            ui.separator();

            // Error message display
            if let Some(ref error) = ui_state.error_message {
                ui.colored_label(egui::Color32::from_rgb(255, 100, 100), error);
                ui.separator();
            }

            // Saved templates list
            ui.heading("Saved Templates");

            ui.horizontal(|ui| {
                ui.label("🔍 Search:");
                ui.text_edit_singleline(&mut ui_state.search_text);
                if ui.button("🔄 Refresh").clicked() {
                    needs_refresh = true;
                }
            });

            ui.separator();

            // Filter templates based on search text
            let search_lower = ui_state.search_text.to_lowercase();
            let filtered_templates: Vec<&TemplateInfo> = if search_lower.is_empty() {
                ui_state.template_infos.iter().collect()
            } else {
                ui_state
                    .template_infos
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

            if ui_state.template_infos.is_empty() {
                ui.label("No templates found");
            } else if filtered_templates.is_empty() {
                ui.label(format!("No templates match '{}'", ui_state.search_text));
            } else {
                ui.label(format!(
                    "Showing {} of {} templates",
                    filtered_templates.len(),
                    ui_state.template_infos.len()
                ));

                egui::ScrollArea::vertical()
                    .max_height(350.0)
                    .show(ui, |ui| {
                        for info in filtered_templates {
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

                                    // Template info and buttons
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.heading(&info.name);
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if ui.button("🗑 Delete").clicked() {
                                                        action =
                                                            TemplateBrowserAction::DeleteTemplate(
                                                                info.name.clone(),
                                                            );
                                                    }
                                                    if ui
                                                        .button("🔄 Thumbnail")
                                                        .on_hover_text(
                                                            "Regenerate thumbnail image",
                                                        )
                                                        .clicked()
                                                    {
                                                        action = TemplateBrowserAction::RegenerateThumbnail(
                                                            info.name.clone(),
                                                        );
                                                    }
                                                    if ui
                                                        .button("📐 Stencil")
                                                        .on_hover_text(
                                                            "Convert to stencil guide",
                                                        )
                                                        .clicked()
                                                    {
                                                        action = TemplateBrowserAction::ToStencil(
                                                            info.name.clone(),
                                                        );
                                                    }
                                                    if ui.button("📥 Load").clicked() {
                                                        action =
                                                            TemplateBrowserAction::LoadTemplate(
                                                                info.name.clone(),
                                                            );
                                                    }
                                                },
                                            );
                                        });

                                        ui.label(format!("Author: {}", info.author));
                                        ui.label(format!(
                                            "Size: {} ({} blocks)",
                                            info.dimensions_str(),
                                            info.block_count_str()
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
        ui_state.refresh_templates(library);
    }

    action
}

/// Draws the save template dialog.
/// Returns Some((name, tags)) if the user confirmed the save.
pub fn draw_save_template_dialog(
    ctx: &egui::Context,
    ui_state: &mut TemplateUi,
) -> Option<(String, Vec<String>)> {
    if !ui_state.save_dialog_open {
        return None;
    }

    let mut result = None;

    egui::Window::new("Save Template")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label("Template Name:");
            let name_edit = egui::TextEdit::singleline(&mut ui_state.edit_name).char_limit(32);
            let name_response = ui.add(name_edit);

            // Auto-focus the name field when dialog opens
            if ui_state.save_dialog_open {
                name_response.request_focus();
            }

            ui.label("Tags (comma-separated):");
            ui.add(
                egui::TextEdit::singleline(&mut ui_state.edit_tags)
                    .hint_text("building, decorated, castle"),
            );

            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("✓ Save").clicked() && !ui_state.edit_name.trim().is_empty() {
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
        format!("thumbnail_{}", name),
        color_image,
        egui::TextureOptions::LINEAR,
    );

    Some(texture)
}

/// Shows a placeholder for missing thumbnails.
fn show_placeholder_thumbnail(ui: &mut egui::Ui) {
    ui.allocate_ui_with_layout(
        egui::vec2(64.0, 64.0),
        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
        |ui| {
            ui.colored_label(egui::Color32::from_gray(100), "📦");
        },
    );
}

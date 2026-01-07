//! Template browser UI (L key).
//!
//! This module will implement the template browser window with:
//! - Current selection info
//! - Save template dialog
//! - Template list with Load/Delete buttons
//! - Confirmation dialogs

// TODO: Implement UI in Day 5
// Will use egui similar to editor/ui.rs
pub struct TemplateUi {
    pub show_save_dialog: bool,
    pub show_delete_confirm: bool,
    pub show_overwrite_confirm: bool,
    pub pending_template_name: String,
    pub pending_tags: String,
}

impl TemplateUi {
    pub fn new() -> Self {
        Self {
            show_save_dialog: false,
            show_delete_confirm: false,
            show_overwrite_confirm: false,
            pending_template_name: String::new(),
            pending_tags: String::new(),
        }
    }
}

impl Default for TemplateUi {
    fn default() -> Self {
        Self::new()
    }
}

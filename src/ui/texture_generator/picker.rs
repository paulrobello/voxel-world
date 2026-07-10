//! Quick texture picker dropdown.

use crate::textures::TextureLibrary;
use egui_winit_vulkano::egui;

/// Quick texture picker for selecting custom textures.
#[allow(dead_code)] // reason: WIP texture generator UI — not yet integrated
pub struct TexturePickerUI;

impl TexturePickerUI {
    /// Draws a texture picker dropdown.
    /// Returns Some(slot) if a texture was selected.
    #[allow(dead_code)] // reason: WIP texture generator UI — not yet integrated
    pub fn draw(
        ui: &mut egui::Ui,
        library: &TextureLibrary,
        current: Option<u8>,
        id_source: &str,
    ) -> Option<u8> {
        let mut selected = None;
        let label = current
            .and_then(|s| library.get(s))
            .map(|t| t.name.as_str())
            .unwrap_or("(None)");

        egui::ComboBox::from_id_salt(id_source)
            .selected_text(label)
            .show_ui(ui, |ui| {
                if ui.selectable_label(current.is_none(), "(None)").clicked() {
                    selected = Some(None);
                }
                for (slot, name) in library.names() {
                    if ui.selectable_label(current == Some(slot), name).clicked() {
                        selected = Some(Some(slot));
                    }
                }
            });

        selected.flatten()
    }
}

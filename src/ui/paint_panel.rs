//! Paint customization panel UI with HSV sliders, blend modes, and presets.
//!
//! Press Y to open the paint customization panel.
//! Provides HSV adjustment, blend modes, and preset management.

use crate::paint::{BlendMode, HsvAdjustment, PaintConfig, PaintPreset, PaintPresetLibrary};
use egui_winit_vulkano::egui;

/// State for the paint panel UI.
#[derive(Debug)]
pub struct PaintPanelState {
    /// Whether the paint panel window is open.
    pub open: bool,
    /// Current paint configuration being edited.
    pub current_config: PaintConfig,
    /// Whether the paint panel is expanded.
    pub expanded: bool,
    /// Paint preset library.
    pub presets: PaintPresetLibrary,
    /// New preset name input.
    pub new_preset_name: String,
    /// Whether to show the preset save dialog.
    pub show_save_dialog: bool,
}

impl Default for PaintPanelState {
    fn default() -> Self {
        Self {
            open: false,
            current_config: PaintConfig::default(),
            expanded: false,
            presets: PaintPresetLibrary::load(),
            new_preset_name: String::new(),
            show_save_dialog: false,
        }
    }
}

impl PaintPanelState {
    /// Creates a new paint panel state with loaded presets.
    pub fn new() -> Self {
        Self::default()
    }

    /// Updates the paint config from external sources (hotbar, etc.).
    pub fn set_texture_and_tint(&mut self, texture_idx: u8, tint_idx: u8) {
        self.current_config.texture_idx = texture_idx;
        self.current_config.tint_idx = tint_idx;
    }

    /// Applies a preset to the current configuration.
    pub fn apply_preset(&mut self, preset: &PaintPreset) {
        if let Some(config) = preset.primary_config() {
            self.current_config = *config;
        }
    }

    /// Saves the current configuration as a new preset.
    pub fn save_as_preset(&mut self, name: String) -> bool {
        if name.is_empty() {
            return false;
        }
        let preset = PaintPreset::new(name, self.current_config);
        self.presets.add(preset).is_some()
    }

    /// Saves presets to disk.
    pub fn save_presets(&self) {
        if let Err(e) = self.presets.save() {
            eprintln!("Failed to save paint presets: {}", e);
        }
    }
}

pub struct PaintPanelUI;

impl PaintPanelUI {
    /// Draws the paint panel as a standalone window.
    /// Call this from the HUD rendering code.
    pub fn draw_window(ctx: &egui::Context, state: &mut PaintPanelState) {
        if !state.open {
            return;
        }

        let texture_names = get_texture_names();
        let mut window_open = state.open;

        egui::Window::new("🎨 Paint Customization")
            .open(&mut window_open)
            .default_size(egui::vec2(280.0, 400.0))
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                Self::draw_inner(ui, state, &texture_names);
            });

        state.open = window_open;
    }

    /// Draws the paint panel within a parent UI.
    /// Returns true if the paint config was changed.
    pub fn draw(ui: &mut egui::Ui, state: &mut PaintPanelState, texture_names: &[&str]) -> bool {
        let mut changed = false;

        // Collapsible header
        let header_response = ui.collapsing("🎨 Paint Customization", |ui| {
            changed |= Self::draw_inner(ui, state, texture_names);
        });

        state.expanded = header_response.fully_open();
        changed
    }

    /// Draws the full paint panel (when in a dedicated window).
    pub fn draw_full(
        ui: &mut egui::Ui,
        state: &mut PaintPanelState,
        texture_names: &[&str],
    ) -> bool {
        Self::draw_inner(ui, state, texture_names)
    }

    fn draw_inner(ui: &mut egui::Ui, state: &mut PaintPanelState, texture_names: &[&str]) -> bool {
        let mut changed = false;

        ui.add_space(4.0);

        // Texture and Tint selectors (basic)
        changed |= Self::draw_basic_selectors(ui, state, texture_names);

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);

        // HSV Adjustments
        changed |= Self::draw_hsv_controls(ui, state);

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);

        // Blend Mode selector
        changed |= Self::draw_blend_mode(ui, state);

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);

        // Presets section
        Self::draw_presets(ui, state);

        changed
    }

    fn draw_basic_selectors(
        ui: &mut egui::Ui,
        state: &mut PaintPanelState,
        texture_names: &[&str],
    ) -> bool {
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.label("Texture:");

            let current_name = texture_names
                .get(state.current_config.texture_idx as usize)
                .unwrap_or(&"Unknown");

            egui::ComboBox::from_id_salt("paint_texture")
                .selected_text(*current_name)
                .width(120.0)
                .show_ui(ui, |ui| {
                    for (idx, name) in texture_names.iter().enumerate() {
                        if ui
                            .selectable_value(
                                &mut state.current_config.texture_idx,
                                idx as u8,
                                *name,
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });
        });

        ui.horizontal(|ui| {
            ui.label("Tint:");

            let tint_idx = state.current_config.tint_idx;
            let tint_color = tint_to_color32(tint_idx);

            // Color preview
            let (rect, _) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(2), tint_color);

            // Tint index slider
            if ui
                .add(egui::Slider::new(&mut state.current_config.tint_idx, 0..=31).show_value(true))
                .changed()
            {
                changed = true;
            }
        });

        changed
    }

    fn draw_hsv_controls(ui: &mut egui::Ui, state: &mut PaintPanelState) -> bool {
        let mut changed = false;
        let hsv = &mut state.current_config.hsv;

        ui.label(egui::RichText::new("HSV Adjustments").strong().size(13.0));

        // Hue shift
        ui.horizontal(|ui| {
            ui.label("Hue:");
            if ui
                .add(
                    egui::Slider::new(&mut hsv.hue_shift, -180.0..=180.0)
                        .suffix("°")
                        .fixed_decimals(0),
                )
                .changed()
            {
                changed = true;
            }
            if ui.small_button("↺").on_hover_text("Reset").clicked() {
                hsv.hue_shift = 0.0;
                changed = true;
            }
        });

        // Saturation
        ui.horizontal(|ui| {
            ui.label("Sat:");
            if ui
                .add(egui::Slider::new(&mut hsv.saturation_mult, 0.0..=2.0).fixed_decimals(2))
                .changed()
            {
                changed = true;
            }
            if ui.small_button("↺").on_hover_text("Reset").clicked() {
                hsv.saturation_mult = 1.0;
                changed = true;
            }
        });

        // Value (brightness)
        ui.horizontal(|ui| {
            ui.label("Val:");
            if ui
                .add(egui::Slider::new(&mut hsv.value_mult, 0.0..=2.0).fixed_decimals(2))
                .changed()
            {
                changed = true;
            }
            if ui.small_button("↺").on_hover_text("Reset").clicked() {
                hsv.value_mult = 1.0;
                changed = true;
            }
        });

        // Reset all button
        if !hsv.is_identity() {
            ui.horizontal(|ui| {
                if ui.button("Reset All HSV").clicked() {
                    *hsv = HsvAdjustment::default();
                    changed = true;
                }
            });
        }

        changed
    }

    fn draw_blend_mode(ui: &mut egui::Ui, state: &mut PaintPanelState) -> bool {
        let mut changed = false;

        ui.label(egui::RichText::new("Blend Mode").strong().size(13.0));

        let current_mode = state.current_config.blend_mode;

        egui::ComboBox::from_id_salt("paint_blend_mode")
            .selected_text(current_mode.display_name())
            .show_ui(ui, |ui| {
                for mode in BlendMode::ALL {
                    let response = ui.selectable_value(
                        &mut state.current_config.blend_mode,
                        mode,
                        mode.display_name(),
                    );
                    if response.changed() {
                        changed = true;
                    }
                    response.on_hover_text(mode.description());
                }
            });

        // Show description of current mode
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(current_mode.description())
                .italics()
                .size(11.0)
                .color(egui::Color32::from_gray(150)),
        );

        changed
    }

    fn draw_presets(ui: &mut egui::Ui, state: &mut PaintPanelState) {
        ui.label(egui::RichText::new("Presets").strong().size(13.0));

        // Preset buttons in a grid
        egui::Grid::new("paint_presets_grid")
            .num_columns(3)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                let mut apply_idx: Option<usize> = None;
                let mut delete_idx: Option<usize> = None;

                for (idx, preset) in state.presets.iter().enumerate() {
                    // Preset button
                    let button_text = egui::RichText::new(&preset.name).size(11.0);
                    let is_selected = state.presets.selected() == Some(idx);

                    let button = egui::Button::new(button_text)
                        .min_size(egui::vec2(80.0, 24.0))
                        .fill(if is_selected {
                            egui::Color32::from_rgb(60, 80, 120)
                        } else {
                            egui::Color32::from_gray(50)
                        });

                    if ui.add(button).clicked() {
                        apply_idx = Some(idx);
                    }

                    // Delete button (only for non-builtin)
                    if !preset.builtin {
                        if ui
                            .small_button("✖")
                            .on_hover_text("Delete preset")
                            .clicked()
                        {
                            delete_idx = Some(idx);
                        }
                    } else {
                        ui.label(""); // Placeholder for alignment
                    }

                    if (idx + 1) % 2 == 0 {
                        ui.end_row();
                    }
                }

                // Apply preset
                if let Some(idx) = apply_idx {
                    if let Some(preset) = state.presets.get(idx) {
                        state.apply_preset(&preset.clone());
                        state.presets.select(Some(idx));
                    }
                }

                // Delete preset
                if let Some(idx) = delete_idx {
                    state.presets.remove(idx);
                    state.save_presets();
                }
            });

        ui.add_space(8.0);

        // Save new preset
        ui.horizontal(|ui| {
            let response = ui.text_edit_singleline(&mut state.new_preset_name);
            response.on_hover_text("Enter preset name");

            let can_save = !state.new_preset_name.is_empty()
                && state.presets.get_by_name(&state.new_preset_name).is_none();

            if ui
                .add_enabled(can_save, egui::Button::new("Save"))
                .on_hover_text("Save current settings as a new preset")
                .clicked()
            {
                let name = std::mem::take(&mut state.new_preset_name);
                if state.save_as_preset(name) {
                    state.save_presets();
                }
            }
        });
    }
}

/// Convert tint index to egui Color32.
fn tint_to_color32(tint_idx: u8) -> egui::Color32 {
    // Mirror the TINT_PALETTE from common.glsl
    const TINT_PALETTE: [[f32; 3]; 32] = [
        [1.0, 0.2, 0.2],    // 0: Red
        [1.0, 0.5, 0.2],    // 1: Orange
        [1.0, 1.0, 0.2],    // 2: Yellow
        [0.5, 1.0, 0.2],    // 3: Lime
        [0.2, 1.0, 0.2],    // 4: Green
        [0.2, 1.0, 0.5],    // 5: Teal
        [0.2, 1.0, 1.0],    // 6: Cyan
        [0.2, 0.5, 1.0],    // 7: Sky blue
        [0.2, 0.2, 1.0],    // 8: Blue
        [0.5, 0.2, 1.0],    // 9: Purple
        [1.0, 0.2, 1.0],    // 10: Magenta
        [1.0, 0.2, 0.5],    // 11: Pink
        [0.95, 0.95, 0.95], // 12: White
        [0.6, 0.6, 0.6],    // 13: Light gray
        [0.3, 0.3, 0.3],    // 14: Dark gray
        [0.4, 0.25, 0.1],   // 15: Brown
        [0.8, 0.4, 0.4],    // 16: Light red
        [0.8, 0.6, 0.4],    // 17: Peach
        [0.8, 0.8, 0.4],    // 18: Light yellow
        [0.6, 0.8, 0.4],    // 19: Light lime
        [0.4, 0.8, 0.4],    // 20: Light green
        [0.4, 0.8, 0.6],    // 21: Light teal
        [0.4, 0.8, 0.8],    // 22: Light cyan
        [0.4, 0.6, 0.8],    // 23: Light sky
        [0.4, 0.4, 0.8],    // 24: Light blue
        [0.6, 0.4, 0.8],    // 25: Light purple
        [0.8, 0.4, 0.8],    // 26: Light magenta
        [0.8, 0.4, 0.6],    // 27: Light pink
        [0.2, 0.15, 0.1],   // 28: Dark brown
        [0.1, 0.2, 0.1],    // 29: Dark green
        [0.1, 0.1, 0.2],    // 30: Dark blue
        [0.2, 0.1, 0.2],    // 31: Dark purple
    ];

    let idx = (tint_idx as usize).min(31);
    let [r, g, b] = TINT_PALETTE[idx];
    egui::Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

/// Get texture names for the UI dropdown.
pub fn get_texture_names() -> Vec<&'static str> {
    vec![
        "Air",
        "Stone",
        "Dirt",
        "Grass",
        "Planks",
        "Leaves",
        "Sand",
        "Gravel",
        "Water",
        "Glass",
        "Log",
        "Model",
        "Brick",
        "Snow",
        "Cobblestone",
        "Iron",
        "Bedrock",
        "Grass Side",
        "Log Top",
        "Lava",
        "GlowStone",
        "GlowMushroom",
        "Crystal",
        "Cactus",
        "Mud",
        "Sandstone",
        "Ice",
        "Pine Leaves",
        "Decorative Stone",
        "Willow Leaves",
        "Concrete",
        "Deepslate",
        "Moss",
        "Mossy Cobble",
        "Clay",
        "Dripstone",
        "Calcite",
        "Terracotta",
        "Packed Ice",
        "Podzol",
        "Mycelium",
        "Coarse Dirt",
        "Rooted Dirt",
        "Birch Log",
        "Birch Leaves",
    ]
}

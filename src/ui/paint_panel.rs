//! Paint customization panel UI with HSV sliders, blend modes, and presets.
//!
//! Press Y to open the paint customization panel.
//! Provides HSV adjustment, blend modes, and preset management.
//! Supports both standard atlas textures and custom textures from the texture generator.
#![allow(dead_code)] // Many features will be used once paint panel is fully integrated

use crate::paint::{
    BlendMode, HsvAdjustment, PaintConfig, PaintPreset, PaintPresetLibrary, apply_blend_mode,
    apply_hsv_adjustment,
};
use crate::textures::{
    TextureLibrary, is_custom_texture, slot_to_texture_index, texture_index_to_slot,
};
use egui_winit_vulkano::egui;
use std::path::PathBuf;

/// Number of textures in the atlas (must match ATLAS_TILE_COUNT in shaders).
const ATLAS_TEXTURE_COUNT: usize = crate::constants::ATLAS_TILE_COUNT;
/// Size of each texture in the atlas.
const TEXTURE_SIZE: usize = 64;
/// Maximum number of custom textures.
const MAX_CUSTOM_TEXTURES: usize = 16;

/// Pre-loaded texture atlas data for CPU-side preview rendering.
pub struct TextureAtlasData {
    /// RGBA pixels for each texture (45 textures × 64×64×4 bytes).
    textures: Vec<Vec<u8>>,
    /// Custom texture pixel cache (up to 16 textures × 64×64×4 bytes).
    /// Updated from TextureLibrary when needed.
    custom_textures: Vec<Option<Vec<u8>>>,
}

impl TextureAtlasData {
    /// Loads the texture atlas from disk and splits it into individual textures.
    pub fn load() -> Option<Self> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let atlas_path = root.join("textures").join("texture_atlas.png");

        let img = image::open(&atlas_path).ok()?.to_rgba8();
        let (width, height) = img.dimensions();

        // Atlas should be (ATLAS_TEXTURE_COUNT * 64) x 64
        let expected_width = (ATLAS_TEXTURE_COUNT * TEXTURE_SIZE) as u32;
        if width != expected_width || height != TEXTURE_SIZE as u32 {
            log::warn!(
                "Texture atlas dimensions mismatch: expected {}x{}, got {}x{}",
                expected_width,
                TEXTURE_SIZE,
                width,
                height
            );
            return None;
        }

        let raw = img.into_raw();

        // Split into individual 64x64 textures
        let mut textures = Vec::with_capacity(ATLAS_TEXTURE_COUNT);
        for i in 0..ATLAS_TEXTURE_COUNT {
            let mut tex_data = vec![0u8; TEXTURE_SIZE * TEXTURE_SIZE * 4];
            for y in 0..TEXTURE_SIZE {
                let src_start = (y * width as usize + i * TEXTURE_SIZE) * 4;
                let dst_start = y * TEXTURE_SIZE * 4;
                tex_data[dst_start..dst_start + TEXTURE_SIZE * 4]
                    .copy_from_slice(&raw[src_start..src_start + TEXTURE_SIZE * 4]);
            }
            textures.push(tex_data);
        }

        // Initialize custom texture slots as empty
        let custom_textures = (0..MAX_CUSTOM_TEXTURES).map(|_| None).collect();

        Some(Self {
            textures,
            custom_textures,
        })
    }

    /// Updates custom texture cache from the texture library.
    pub fn update_custom_textures(&mut self, library: &TextureLibrary) {
        // Clear all slots first
        for slot in &mut self.custom_textures {
            *slot = None;
        }

        // Copy pixels from library textures
        for (slot, _name) in library.names() {
            if let Some(tex) = library.get(slot)
                && !tex.pixels.is_empty()
                && (slot as usize) < MAX_CUSTOM_TEXTURES
            {
                self.custom_textures[slot as usize] = Some(tex.pixels.clone());
            }
        }
    }

    /// Checks if a custom texture slot has data.
    pub fn has_custom_texture(&self, slot: u8) -> bool {
        (slot as usize) < MAX_CUSTOM_TEXTURES && self.custom_textures[slot as usize].is_some()
    }

    /// Gets the RGB pixel at the given position in a texture.
    /// Returns (r, g, b) as floats in 0.0-1.0 range.
    /// Supports both standard atlas textures (0-127) and custom textures (128-143).
    pub fn get_pixel(&self, texture_idx: u8, x: u32, y: u32) -> (f32, f32, f32) {
        let pixel_idx = ((y as usize) * TEXTURE_SIZE + (x as usize)) * 4;

        // Check if this is a custom texture
        if is_custom_texture(texture_idx) {
            let slot = texture_index_to_slot(texture_idx) as usize;
            if slot < self.custom_textures.len()
                && let Some(tex) = &self.custom_textures[slot]
                && pixel_idx + 2 < tex.len()
            {
                return (
                    tex[pixel_idx] as f32 / 255.0,
                    tex[pixel_idx + 1] as f32 / 255.0,
                    tex[pixel_idx + 2] as f32 / 255.0,
                );
            }
            return (0.5, 0.5, 0.5); // Gray for missing custom texture
        }

        // Standard atlas texture
        let idx = texture_idx as usize;
        if idx >= self.textures.len() {
            return (1.0, 0.0, 1.0); // Magenta for missing
        }

        let tex = &self.textures[idx];
        if pixel_idx + 2 >= tex.len() {
            return (1.0, 0.0, 1.0); // Magenta for out of bounds
        }

        (
            tex[pixel_idx] as f32 / 255.0,
            tex[pixel_idx + 1] as f32 / 255.0,
            tex[pixel_idx + 2] as f32 / 255.0,
        )
    }
}

/// State for the paint panel UI.
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
    /// Cached preview pixels (64×64 RGBA = 16,384 bytes).
    pub preview_pixels: Vec<u8>,
    /// Texture atlas data for previewing (loaded once).
    pub atlas_data: Option<TextureAtlasData>,
    /// Flag to regenerate preview.
    pub preview_dirty: bool,
}

impl Default for PaintPanelState {
    fn default() -> Self {
        let atlas_data = TextureAtlasData::load();
        if atlas_data.is_none() {
            log::warn!("[PaintPanel] Warning: Failed to load texture atlas for preview");
        }

        let mut state = Self {
            open: false,
            current_config: PaintConfig::default(),
            expanded: false,
            presets: PaintPresetLibrary::load(),
            new_preset_name: String::new(),
            show_save_dialog: false,
            preview_pixels: vec![0u8; TEXTURE_SIZE * TEXTURE_SIZE * 4],
            atlas_data,
            preview_dirty: true,
        };

        // Generate initial preview
        state.regenerate_preview();
        state
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
        self.preview_dirty = true;
    }

    /// Applies the current paint configuration to the given hotbar slot.
    pub fn apply_to_hotbar_slot(
        &self,
        hotbar_index: usize,
        hotbar_paint_textures: &mut [u8; 9],
        hotbar_tint_indices: &mut [u8; 9],
    ) {
        if hotbar_index < hotbar_paint_textures.len() {
            hotbar_paint_textures[hotbar_index] = self.current_config.texture_idx;
            hotbar_tint_indices[hotbar_index] = self.current_config.tint_idx;
        }
    }

    /// Applies a preset to the current configuration.
    pub fn apply_preset(&mut self, preset: &PaintPreset) {
        if let Some(config) = preset.primary_config() {
            self.current_config = *config;
            self.preview_dirty = true;
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
            log::warn!("Failed to save paint presets: {}", e);
        }
    }

    /// Regenerates the preview pixels based on current paint configuration.
    pub fn regenerate_preview(&mut self) {
        let Some(atlas) = &self.atlas_data else {
            return;
        };

        self.preview_pixels
            .resize(TEXTURE_SIZE * TEXTURE_SIZE * 4, 0);

        let tint = get_tint_color_f32(self.current_config.tint_idx);

        for y in 0..TEXTURE_SIZE {
            for x in 0..TEXTURE_SIZE {
                // Get texture color
                let tex = atlas.get_pixel(self.current_config.texture_idx, x as u32, y as u32);

                // Apply blend mode
                let blended = apply_blend_mode(tex, tint, self.current_config.blend_mode);

                // Apply HSV adjustment
                let (r, g, b): (f32, f32, f32) =
                    apply_hsv_adjustment(blended.0, blended.1, blended.2, &self.current_config.hsv);

                // Store result
                let idx = (y * TEXTURE_SIZE + x) * 4;
                self.preview_pixels[idx] = (r.clamp(0.0, 1.0) * 255.0) as u8;
                self.preview_pixels[idx + 1] = (g.clamp(0.0, 1.0) * 255.0) as u8;
                self.preview_pixels[idx + 2] = (b.clamp(0.0, 1.0) * 255.0) as u8;
                self.preview_pixels[idx + 3] = 255;
            }
        }

        self.preview_dirty = false;
    }
}

pub struct PaintPanelUI;

impl PaintPanelUI {
    /// Draws the paint panel as a standalone window.
    /// Call this from the HUD rendering code.
    pub fn draw_window(
        ctx: &egui::Context,
        state: &mut PaintPanelState,
        texture_library: &TextureLibrary,
        hotbar_paint_textures: &mut [u8; 9],
        hotbar_tint_indices: &mut [u8; 9],
        hotbar_index: &mut usize,
    ) {
        if !state.open {
            return;
        }

        // Update custom texture cache if library changed
        if let Some(atlas) = &mut state.atlas_data {
            atlas.update_custom_textures(texture_library);
        }

        let texture_names = get_texture_names();
        let mut window_open = state.open;

        egui::Window::new("🎨 Paint Customization")
            .open(&mut window_open)
            .default_size(egui::vec2(320.0, 480.0))
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| {
                // Quick apply to current hotbar slot
                ui.horizontal(|ui| {
                    ui.label(format!("Hotbar slot {}", *hotbar_index + 1));
                    if ui.button("Apply to slot").clicked() {
                        state.apply_to_hotbar_slot(
                            *hotbar_index,
                            hotbar_paint_textures,
                            hotbar_tint_indices,
                        );
                    }
                });
                ui.add_space(6.0);
                Self::draw_inner(ui, state, &texture_names, texture_library);
            });

        state.open = window_open;
    }

    /// Draws the paint panel within a parent UI.
    /// Returns true if the paint config was changed.
    pub fn draw(
        ui: &mut egui::Ui,
        state: &mut PaintPanelState,
        texture_names: &[&str],
        texture_library: &TextureLibrary,
    ) -> bool {
        let mut changed = false;

        // Collapsible header
        let header_response = ui.collapsing("🎨 Paint Customization", |ui| {
            changed |= Self::draw_inner(ui, state, texture_names, texture_library);
        });

        state.expanded = header_response.fully_open();
        changed
    }

    /// Draws the full paint panel (when in a dedicated window).
    pub fn draw_full(
        ui: &mut egui::Ui,
        state: &mut PaintPanelState,
        texture_names: &[&str],
        texture_library: &TextureLibrary,
    ) -> bool {
        Self::draw_inner(ui, state, texture_names, texture_library)
    }

    fn draw_inner(
        ui: &mut egui::Ui,
        state: &mut PaintPanelState,
        texture_names: &[&str],
        texture_library: &TextureLibrary,
    ) -> bool {
        let mut changed = false;

        ui.add_space(4.0);

        // Texture and Tint selectors (basic)
        changed |= Self::draw_basic_selectors(ui, state, texture_names, texture_library);

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

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);

        // Preview section
        Self::draw_preview_section(ui, state);

        // Regenerate preview if settings changed
        if changed {
            state.preview_dirty = true;
        }

        // Regenerate preview if dirty
        if state.preview_dirty {
            state.regenerate_preview();
        }

        changed
    }

    fn draw_preview_section(ui: &mut egui::Ui, state: &PaintPanelState) {
        ui.label(egui::RichText::new("Preview").strong().size(13.0));
        ui.add_space(4.0);

        if state.atlas_data.is_some() {
            draw_preview(ui, &state.preview_pixels);
        } else {
            ui.colored_label(
                egui::Color32::from_rgb(200, 100, 100),
                "Preview unavailable - atlas not loaded",
            );
        }
    }

    fn draw_basic_selectors(
        ui: &mut egui::Ui,
        state: &mut PaintPanelState,
        texture_names: &[&str],
        texture_library: &TextureLibrary,
    ) -> bool {
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.label("Texture:");

            // Determine current texture name
            let current_idx = state.current_config.texture_idx;
            let current_name = if is_custom_texture(current_idx) {
                let slot = texture_index_to_slot(current_idx);
                texture_library
                    .get(slot)
                    .map(|t| t.name.as_str())
                    .unwrap_or("Custom (missing)")
            } else {
                texture_names
                    .get(current_idx as usize)
                    .copied()
                    .unwrap_or("Unknown")
            };

            egui::ComboBox::from_id_salt("paint_texture")
                .selected_text(current_name)
                .width(140.0)
                .show_ui(ui, |ui| {
                    // Standard atlas textures
                    ui.label(egui::RichText::new("Standard Textures").strong().size(11.0));
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

                    // Custom textures section (if any exist)
                    let custom_names = texture_library.names();
                    if !custom_names.is_empty() {
                        ui.separator();
                        ui.label(egui::RichText::new("Custom Textures").strong().size(11.0));
                        for (slot, name) in custom_names {
                            let tex_idx = slot_to_texture_index(slot);
                            if ui
                                .selectable_value(
                                    &mut state.current_config.texture_idx,
                                    tex_idx,
                                    format!("★ {}", name),
                                )
                                .changed()
                            {
                                changed = true;
                            }
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
                if let Some(idx) = apply_idx
                    && let Some(preset) = state.presets.get(idx)
                {
                    state.apply_preset(&preset.clone());
                    state.presets.select(Some(idx));
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

/// Tint palette colors (mirrors TINT_PALETTE from common.glsl).
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

/// Convert tint index to egui Color32.
fn tint_to_color32(tint_idx: u8) -> egui::Color32 {
    let idx = (tint_idx as usize).min(31);
    let [r, g, b] = TINT_PALETTE[idx];
    egui::Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

/// Convert tint index to f32 tuple for blend operations.
fn get_tint_color_f32(tint_idx: u8) -> (f32, f32, f32) {
    let idx = (tint_idx as usize).min(31);
    let [r, g, b] = TINT_PALETTE[idx];
    (r, g, b)
}

/// Draws a preview of the paint effect using precomputed pixels.
fn draw_preview(ui: &mut egui::Ui, pixels: &[u8]) {
    let preview_size = 128.0;
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(preview_size, preview_size), egui::Sense::hover());
    let painter = ui.painter_at(rect);

    // Draw as 32x32 grid sampling every 2nd pixel for performance
    let cell_size = preview_size / 32.0;
    for gy in 0..32 {
        for gx in 0..32 {
            let x = gx * 2;
            let y = gy * 2;
            let idx = (y * TEXTURE_SIZE + x) * 4;
            if idx + 2 < pixels.len() {
                let color = egui::Color32::from_rgb(pixels[idx], pixels[idx + 1], pixels[idx + 2]);
                let cell_rect = egui::Rect::from_min_size(
                    rect.min + egui::vec2(gx as f32 * cell_size, gy as f32 * cell_size),
                    egui::vec2(cell_size, cell_size),
                );
                painter.rect_filled(cell_rect, 0.0, color);
            }
        }
    }

    // Border
    painter.rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(1.0, egui::Color32::GRAY),
        egui::StrokeKind::Outside,
    );
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

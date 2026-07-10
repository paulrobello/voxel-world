//! State for the texture generator UI panel.

use crate::textures::{CanvasSize, CanvasState, CustomTexture, ImportState};
use egui_winit_vulkano::egui;
use std::time::Instant;

/// Duration to show status messages.
const STATUS_DURATION_SECS: f32 = 2.0;

/// Active tab in the texture generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextureTab {
    /// Procedural pattern generation.
    #[default]
    Generate,
    /// Pixel painting canvas.
    Paint,
    /// Image import.
    Import,
}

/// State for the texture generator UI panel.
pub struct TextureGeneratorState {
    /// Whether the panel is open.
    pub open: bool,
    /// Current active tab.
    pub active_tab: TextureTab,
    /// Current texture being edited (procedural).
    pub editing: CustomTexture,
    /// Selected slot for editing (None = new texture).
    pub selected_slot: Option<u8>,
    /// Preview texture ID for egui.
    #[allow(dead_code)] // reason: WIP texture generator UI — not yet integrated
    pub preview_texture_id: Option<egui::TextureId>,
    /// Color picker state for color1.
    #[allow(dead_code)] // reason: WIP texture generator UI — not yet integrated
    pub color1_picker_open: bool,
    /// Color picker state for color2.
    #[allow(dead_code)] // reason: WIP texture generator UI — not yet integrated
    pub color2_picker_open: bool,
    /// Flag indicating custom textures need GPU sync.
    pub needs_gpu_sync: bool,
    /// Slot of texture that needs multiplayer sync (set when saved).
    pub pending_multiplayer_upload: Option<u8>,
    /// Status message to display.
    status_message: Option<String>,
    /// When the status message was set.
    status_time: Option<Instant>,
    /// Canvas state for paint tab.
    pub canvas: CanvasState,
    /// Selected canvas size for size selector.
    pub selected_canvas_size: CanvasSize,
    /// Available canvas size presets.
    pub available_sizes: Vec<CanvasSize>,
    /// Whether custom size dialog is open.
    pub show_size_dialog: bool,
    /// Custom width input.
    pub custom_width: u16,
    /// Custom height input.
    pub custom_height: u16,
    /// Import state for import tab.
    pub import: ImportState,
    /// Whether mouse is currently dragging on canvas.
    pub(super) canvas_dragging: bool,
    /// Start position for shape dragging.
    pub(super) drag_start: Option<(u32, u32)>,
    /// Last time the text cursor was toggled (for blinking effect).
    pub(super) last_cursor_toggle: Instant,
}

impl Default for TextureGeneratorState {
    fn default() -> Self {
        Self::new()
    }
}

impl TextureGeneratorState {
    /// Creates new state with default values.
    pub fn new() -> Self {
        let mut editing = CustomTexture {
            name: "New Texture".to_string(),
            ..CustomTexture::default()
        };
        editing.regenerate();

        let available_sizes = CanvasSize::all_presets().to_vec();
        let default_size = CanvasSize::default();

        Self {
            open: false,
            active_tab: TextureTab::default(),
            editing,
            selected_slot: None,
            preview_texture_id: None,
            color1_picker_open: false,
            color2_picker_open: false,
            needs_gpu_sync: false,
            pending_multiplayer_upload: None,
            status_message: None,
            status_time: None,
            canvas: CanvasState::with_size(default_size),
            selected_canvas_size: default_size,
            available_sizes,
            show_size_dialog: false,
            custom_width: 64,
            custom_height: 64,
            import: ImportState::new(),
            canvas_dragging: false,
            drag_start: None,
            last_cursor_toggle: Instant::now(),
        }
    }

    /// Sets a status message that will display briefly.
    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
        self.status_time = Some(Instant::now());
    }

    /// Gets the current status message if it hasn't expired.
    pub fn get_status(&self) -> Option<&str> {
        if let (Some(msg), Some(time)) = (&self.status_message, self.status_time)
            && time.elapsed().as_secs_f32() < STATUS_DURATION_SECS
        {
            return Some(msg.as_str());
        }
        None
    }

    /// Starts editing a new texture.
    pub fn new_texture(&mut self) {
        self.editing = CustomTexture::default();
        self.editing.name = "New Texture".to_string();
        self.editing.regenerate();
        self.selected_slot = None;
        self.set_status("Started new texture");
    }

    /// Starts editing an existing texture.
    pub fn edit_texture(&mut self, texture: &CustomTexture) {
        self.editing = texture.clone();
        self.selected_slot = Some(texture.id);
    }

    /// Copies generated texture to the paint canvas.
    pub fn copy_generate_to_canvas(&mut self) {
        if !self.editing.pixels.is_empty() {
            self.canvas.copy_from(&self.editing.pixels);
            self.active_tab = TextureTab::Paint;
            self.set_status("Copied to canvas");
        }
    }

    /// Copies import result to the paint canvas.
    pub fn copy_import_to_canvas(&mut self) {
        if self.import.has_image() {
            self.canvas.copy_from(self.import.get_result());
            self.active_tab = TextureTab::Paint;
            self.set_status("Imported to canvas");
        }
    }

    /// Changes the canvas size, preserving existing pixels where possible.
    pub fn change_canvas_size(&mut self, new_size: CanvasSize) {
        // Save current pixels
        let old_pixels = self.canvas.pixels.clone();
        let old_size = self.canvas.size;

        // Create new canvas with new size
        self.canvas = CanvasState::with_size(new_size);
        self.selected_canvas_size = new_size;

        // Copy existing pixels (clip if smaller, center if larger)
        let copy_width = old_size.width.min(new_size.width);
        let copy_height = old_size.height.min(new_size.height);

        // Calculate offset to center when resizing up
        let offset_x = if new_size.width > old_size.width {
            (new_size.width - old_size.width) / 2
        } else {
            0
        };
        let offset_y = if new_size.height > old_size.height {
            (new_size.height - old_size.height) / 2
        } else {
            0
        };

        for y in 0..copy_height {
            for x in 0..copy_width {
                let src_idx = (y as usize * old_size.width as usize + x as usize) * 4;
                let dst_x = x + offset_x;
                let dst_y = y + offset_y;
                let dst_idx = (dst_y as usize * new_size.width as usize + dst_x as usize) * 4;

                if src_idx + 4 <= old_pixels.len() && dst_idx + 4 <= self.canvas.pixels.len() {
                    self.canvas.pixels[dst_idx..dst_idx + 4]
                        .copy_from_slice(&old_pixels[src_idx..src_idx + 4]);
                }
            }
        }

        // Clear undo history when changing size
        self.canvas.history.clear();

        self.set_status(format!("Canvas resized to {}", new_size.size_label()));
    }
}

use std::time::Instant;

use egui_winit_vulkano::egui;
use nalgebra::Vector3;

use crate::chunk::BlockType;
use crate::raycast::RaycastHit;

use super::{AutoProfileFeature, PaletteItem, PaletteTab};

// ---------------------------------------------------------------------------
// HotbarState
// ---------------------------------------------------------------------------

/// Selected hotbar slot and per-slot block/model/tint data.
pub struct HotbarState {
    pub hotbar_index: usize,
    pub hotbar_blocks: [BlockType; 9],
    pub hotbar_model_ids: [u8; 9],
    pub hotbar_tint_indices: [u8; 9],
    pub hotbar_paint_textures: [u8; 9],
}

// ---------------------------------------------------------------------------
// MinimapUiState
// ---------------------------------------------------------------------------

/// Minimap visibility toggle and cached render data.
pub struct MinimapUiState {
    pub show_minimap: bool,
    pub minimap: crate::hud::Minimap,
    pub minimap_cached_image: Option<egui::ColorImage>,
    pub minimap_last_pos: Vector3<i32>,
    pub minimap_last_update: Instant,
    pub minimap_last_yaw: f32,
}

// ---------------------------------------------------------------------------
// PaletteUiState
// ---------------------------------------------------------------------------

/// Block/model palette panel state.
#[derive(Default)]
pub struct PaletteUiState {
    pub palette_open: bool,
    pub palette_tab: PaletteTab,
    pub palette_previously_focused: bool,
    pub palette_search: String,
    pub dragging_item: Option<PaletteItem>,
}

// ---------------------------------------------------------------------------
// PlacementState
// ---------------------------------------------------------------------------

/// Block breaking/placing interaction state, raycasting and tool-mode flags.
pub struct PlacementState {
    /// Currently targeted block hit by the crosshair raycast.
    pub current_hit: Option<RaycastHit>,

    // Breaking
    pub breaking_block: Option<Vector3<i32>>,
    pub break_progress: f32,
    pub break_cooldown: f32,
    pub skip_break_until_release: bool,

    // Placing
    pub last_place_pos: Option<Vector3<i32>>,
    pub place_cooldown: f32,
    pub place_needs_reclick: bool,
    pub model_needs_reclick: bool,
    pub gate_needs_reclick: bool,
    pub custom_rotate_needs_reclick: bool,

    // Line-drawing
    pub line_start_pos: Option<Vector3<i32>>,
    pub line_locked_axis: Option<u8>,

    // Active tool modes
    pub rangefinder_active: bool,
    pub flood_fill_active: bool,
    pub measurement_markers: Vec<Vector3<i32>>,
}

impl Default for PlacementState {
    fn default() -> Self {
        Self {
            current_hit: None,
            breaking_block: None,
            break_progress: 0.0,
            break_cooldown: 0.0,
            skip_break_until_release: false,
            last_place_pos: None,
            place_cooldown: 0.0,
            place_needs_reclick: false,
            model_needs_reclick: false,
            gate_needs_reclick: false,
            custom_rotate_needs_reclick: false,
            line_start_pos: None,
            line_locked_axis: None,
            rangefinder_active: false,
            flood_fill_active: false,
            measurement_markers: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// FrameState
// ---------------------------------------------------------------------------

/// Per-frame counters, FPS tracking, and render scale change signalling.
pub struct FrameState {
    pub window_size: [u32; 2],
    pub last_second: Instant,
    pub frames_since_last_second: u32,
    pub fps: u32,
    /// Smoothed FPS for dynamic render scale (exponential moving average).
    pub smoothed_fps: f32,
    /// Flag set when dynamic render scale changes (triggers render target recreation).
    pub pending_scale_change: bool,
    pub total_frames: u64,
    pub screenshot_taken: bool,
}

// ---------------------------------------------------------------------------
// ProfileState
// ---------------------------------------------------------------------------

/// Auto-profile state machine and CSV log path.
pub struct ProfileState {
    pub start_time: Instant,
    pub profile_log_path: Option<String>,
    pub profile_log_header_written: bool,
    pub auto_profile_enabled: bool,
    pub auto_profile_feature: AutoProfileFeature,
    /// true = feature is OFF, false = feature is ON.
    pub auto_profile_feature_off: bool,
    pub auto_profile_phase_start: Instant,
}

// ---------------------------------------------------------------------------
// PictureUiState
// ---------------------------------------------------------------------------

/// Picture-frame placement and GPU atlas upload state.
pub struct PictureUiState {
    /// Selected picture ID for frame placement (None = empty frame).
    pub selected_picture_id: Option<u32>,
    /// Pending picture ID to upload to GPU atlas (Some = needs upload).
    pub pending_picture_upload: Option<u32>,
    /// Flag indicating pictures need to be uploaded to GPU (e.g., after world load).
    pub pictures_need_upload: bool,
}

impl Default for PictureUiState {
    fn default() -> Self {
        Self {
            selected_picture_id: None,
            pending_picture_upload: None,
            pictures_need_upload: true,
        }
    }
}

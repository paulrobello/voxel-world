use std::time::Instant;

use egui_winit_vulkano::egui;
use nalgebra::Vector3;

use crate::chunk::BlockType;
use crate::config::Settings;
use crate::console::ConsoleState;
use crate::editor::EditorState;
use crate::hud::Minimap;
use crate::raycast::RaycastHit;
use crate::shape_tools::{BridgeToolState, CubeToolState, SphereToolState};
use crate::templates::{TemplateLibrary, TemplatePlacement, TemplateSelection, TemplateUi};
use crate::ui::tools::ToolsPaletteState;

use super::{AutoProfileFeature, PaletteItem, PaletteTab};

pub struct UiState {
    pub settings: Settings,
    pub window_size: [u32; 2],
    pub start_time: Instant,
    pub profile_log_path: Option<String>,
    pub profile_log_header_written: bool,

    // Auto-profile state machine
    pub auto_profile_enabled: bool,
    pub auto_profile_feature: AutoProfileFeature,
    pub auto_profile_feature_off: bool, // true = feature is OFF, false = feature is ON
    pub auto_profile_phase_start: Instant,

    pub show_minimap: bool,
    pub minimap: Minimap,
    pub minimap_cached_image: Option<egui::ColorImage>,
    pub minimap_last_pos: Vector3<i32>,
    pub minimap_last_update: Instant,
    pub minimap_last_yaw: f32,

    pub palette_open: bool,
    pub palette_tab: PaletteTab,
    pub palette_previously_focused: bool,
    pub palette_search: String,
    pub dragging_item: Option<PaletteItem>,

    pub hotbar_index: usize,
    pub hotbar_blocks: [BlockType; 9],
    pub hotbar_model_ids: [u8; 9],
    pub hotbar_tint_indices: [u8; 9],
    pub hotbar_paint_textures: [u8; 9],
    pub current_hit: Option<RaycastHit>,

    pub breaking_block: Option<Vector3<i32>>,
    pub break_progress: f32,
    pub break_cooldown: f32,
    pub skip_break_until_release: bool,

    pub last_place_pos: Option<Vector3<i32>>,
    pub place_cooldown: f32,
    pub place_needs_reclick: bool,
    pub model_needs_reclick: bool,
    pub gate_needs_reclick: bool,
    pub custom_rotate_needs_reclick: bool,
    pub line_start_pos: Option<Vector3<i32>>,
    pub line_locked_axis: Option<u8>,

    pub last_second: Instant,
    pub frames_since_last_second: u32,
    pub fps: u32,
    pub total_frames: u64,
    pub screenshot_taken: bool,

    /// In-game model editor state.
    pub editor: EditorState,
    /// Whether we were focused before opening the editor.
    pub editor_previously_focused: bool,

    /// In-game command console state.
    pub console: ConsoleState,
    /// Whether we were focused before opening the console.
    pub console_previously_focused: bool,

    /// Template browser UI state.
    pub template_ui: TemplateUi,
    /// Template region selection state.
    pub template_selection: TemplateSelection,
    /// Template library manager.
    pub template_library: TemplateLibrary,
    /// Stencil library manager.
    pub stencil_library: crate::stencils::StencilLibrary,
    /// Stencil manager for active stencils.
    pub stencil_manager: crate::stencils::StencilManager,
    /// Stencil browser UI state.
    pub stencil_ui: crate::stencils::StencilUi,
    /// Whether we were focused before opening the stencil browser.
    pub stencil_previously_focused: bool,
    /// Active stencil placement preview (when loading a stencil for positioning).
    pub active_stencil_placement: Option<crate::stencils::StencilPlacementMode>,
    /// Active template placement (when loading a template).
    #[allow(dead_code)] // TODO: Will be used for template placement handlers
    pub active_placement: Option<TemplatePlacement>,
    /// Whether we were focused before opening the template browser.
    pub template_previously_focused: bool,
    /// Request cursor grab (set when loading template for placement).
    pub request_cursor_grab: bool,

    /// Laser rangefinder mode active (shows distance to targeted block).
    pub rangefinder_active: bool,

    /// Flood fill mode active (right-click to fill connected blocks).
    pub flood_fill_active: bool,

    /// Measurement marker positions (up to 8 markers for dimension display).
    pub measurement_markers: Vec<Vector3<i32>>,

    /// Tools palette UI state (passive overlay, doesn't capture cursor).
    pub tools_palette: ToolsPaletteState,

    /// Sphere placement tool state.
    pub sphere_tool: SphereToolState,

    /// Cube placement tool state.
    pub cube_tool: CubeToolState,

    /// Bridge (line) placement tool state.
    pub bridge_tool: BridgeToolState,
}

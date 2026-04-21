use crate::config::Settings;
use crate::console::ConsoleState;
use crate::editor::EditorState;
use crate::pictures::PictureUi;
use crate::shape_tools::{
    ArchToolState, BezierToolState, BridgeToolState, CircleToolState, CloneToolState,
    ConeToolState, CubeToolState, CylinderToolState, FloorToolState, HelixToolState,
    HollowToolState, MirrorToolState, PatternFillState, PolygonToolState, ReplaceToolState,
    ScatterToolState, SphereToolState, StairsToolState, TerrainBrushState, TorusToolState,
    WallToolState,
};
use crate::templates::{TemplateLibrary, TemplatePlacement, TemplateSelection, TemplateUi};
use crate::textures::TextureLibrary;
use crate::ui::multiplayer::MultiplayerPanelState;
use crate::ui::paint_panel::PaintPanelState;
use crate::ui::texture_generator::TextureGeneratorState;
use crate::ui::tools::ToolsPaletteState;

use super::ui_substates::{
    FrameState, HotbarState, MinimapUiState, PaletteUiState, PictureUiState, PlacementState,
    ProfileState,
};

pub struct UiState {
    pub settings: Settings,

    // --- Domain sub-states ---
    pub hotbar: HotbarState,
    pub minimap_ui: MinimapUiState,
    pub palette_ui: PaletteUiState,
    pub placement: PlacementState,
    pub frame: FrameState,
    pub profile: ProfileState,
    pub picture_state: PictureUiState,

    // --- Already-structured sub-states (unchanged) ---
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

    /// Tools palette UI state (passive overlay, doesn't capture cursor).
    pub tools_palette: ToolsPaletteState,

    // --- Shape tool states ---
    pub sphere_tool: SphereToolState,
    pub cube_tool: CubeToolState,
    pub bridge_tool: BridgeToolState,
    pub cylinder_tool: CylinderToolState,
    pub wall_tool: WallToolState,
    pub floor_tool: FloorToolState,
    pub replace_tool: ReplaceToolState,
    pub circle_tool: CircleToolState,
    pub mirror_tool: MirrorToolState,
    pub stairs_tool: StairsToolState,
    pub arch_tool: ArchToolState,
    pub cone_tool: ConeToolState,
    pub clone_tool: CloneToolState,
    pub torus_tool: TorusToolState,
    pub helix_tool: HelixToolState,
    pub polygon_tool: PolygonToolState,
    pub bezier_tool: BezierToolState,
    pub pattern_fill: PatternFillState,
    pub scatter_tool: ScatterToolState,
    pub hollow_tool: HollowToolState,
    pub terrain_brush: TerrainBrushState,

    // --- Texture / picture library ---
    /// Texture generator UI state.
    pub texture_generator: TextureGeneratorState,
    /// Custom texture library.
    pub texture_library: TextureLibrary,

    /// Picture browser UI state.
    pub picture_ui: PictureUi,

    /// Paint panel UI state.
    pub paint_panel: PaintPanelState,

    /// Multiplayer panel UI state.
    pub multiplayer_panel: MultiplayerPanelState,
}

use crate::config::Settings;
use crate::console::ConsoleState;
use crate::editor::EditorState;
use crate::pictures::PictureUi;
use crate::shape_tools::{
    ArchToolState, BezierToolState, BridgeToolState, CircleToolState, CloneToolState,
    ConeToolState, CubeToolState, CylinderToolState, FloorToolState, HelixToolState,
    HollowToolState, MirrorToolState, PatternFillState, PolygonToolState, ReplaceToolState,
    ScatterToolState, ShapeTool, SphereToolState, StairsToolState, TerrainBrushState,
    TorusToolState, WallToolState,
};
use crate::templates::{TemplateLibrary, TemplatePlacement, TemplateSelection, TemplateUi};
use crate::textures::TextureLibrary;
use crate::ui::multiplayer::MultiplayerPanelState;
use crate::ui::paint_panel::PaintPanelState;
use crate::ui::texture_generator::TextureGeneratorState;
use crate::ui::tools::{ActiveTool, ToolsPaletteState};

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

impl UiState {
    /// Iterate the placement tools that carry a standard holographic preview as
    /// `(ActiveTool, &dyn ShapeTool)` pairs, so render / dispatch sites can loop
    /// over the registry instead of re-encoding per tool.
    ///
    /// Colour and any extra markers stay keyed off the `ActiveTool` at the call
    /// site (the trait is preview-state only — rendering concerns don't belong
    /// on it). Excluded: `Mirror` (a placement modifier), and `Bridge` /
    /// `Replace` / `PatternFill` / `Scatter` / `TerrainBrush` (modifiers or lack
    /// the standard preview fields) — those are handled directly at their call
    /// sites.
    pub fn shape_tools(&self) -> Vec<(ActiveTool, &dyn ShapeTool)> {
        vec![
            (ActiveTool::Sphere, &self.sphere_tool as &dyn ShapeTool),
            (ActiveTool::Cube, &self.cube_tool as &dyn ShapeTool),
            (ActiveTool::Cylinder, &self.cylinder_tool as &dyn ShapeTool),
            (ActiveTool::Wall, &self.wall_tool as &dyn ShapeTool),
            (ActiveTool::Floor, &self.floor_tool as &dyn ShapeTool),
            (ActiveTool::Circle, &self.circle_tool as &dyn ShapeTool),
            (ActiveTool::Cone, &self.cone_tool as &dyn ShapeTool),
            (ActiveTool::Arch, &self.arch_tool as &dyn ShapeTool),
            (ActiveTool::Stairs, &self.stairs_tool as &dyn ShapeTool),
            (ActiveTool::Clone, &self.clone_tool as &dyn ShapeTool),
            (ActiveTool::Torus, &self.torus_tool as &dyn ShapeTool),
            (ActiveTool::Helix, &self.helix_tool as &dyn ShapeTool),
            (ActiveTool::Polygon, &self.polygon_tool as &dyn ShapeTool),
            (ActiveTool::Bezier, &self.bezier_tool as &dyn ShapeTool),
            (ActiveTool::Hollow, &self.hollow_tool as &dyn ShapeTool),
        ]
    }

    /// Deactivate every mutually-exclusive placement tool. Called when one is
    /// turned ON so that two placement tools can never be active at once — each
    /// `ToolAction::Toggle*Tool` handler otherwise flips only its own flag,
    /// leaving the previously-active tool running.
    ///
    /// `Mirror` is excluded on purpose: it is a placement modifier (block
    /// placement is mirrored across its plane when active and the plane is set)
    /// and is meant to combine with an active placement tool. The browser /
    /// measurement toggles (Template / Stencil / Rangefinder / FloodFill) are a
    /// separate UX category and are not placement tools, so they're excluded too.
    pub fn deactivate_all_placement_tools(&mut self) {
        self.sphere_tool.active = false;
        self.cube_tool.active = false;
        self.cylinder_tool.active = false;
        self.wall_tool.active = false;
        self.floor_tool.active = false;
        self.circle_tool.active = false;
        self.cone_tool.active = false;
        self.arch_tool.active = false;
        self.stairs_tool.active = false;
        self.clone_tool.active = false;
        self.torus_tool.active = false;
        self.helix_tool.active = false;
        self.polygon_tool.active = false;
        self.bezier_tool.active = false;
        self.hollow_tool.active = false;
        self.bridge_tool.active = false;
        self.replace_tool.active = false;
        self.pattern_fill.active = false;
        self.scatter_tool.active = false;
        self.terrain_brush.active = false;
    }
}

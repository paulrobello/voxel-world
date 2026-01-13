//! Tools Palette UI - provides quick access to building tools.
//!
//! Tools available:
//! - Template (L key): Copy/paste building templates
//! - Measurement (G key): Place markers and measure distances
//! - Stencil (K key): Holographic building guides
//! - Flood Fill: Mass block replacement (tools palette/console only)
//! - Sphere: Place solid or hollow spheres (tools palette only)
//! - Cube: Place solid or hollow cubes/boxes (tools palette only)
//! - Bridge: Draw lines between two points (tools palette only)

use egui_winit_vulkano::egui;
use serde::{Deserialize, Serialize};

use crate::stencils::StencilRenderMode;

/// Action requested by clicking a tool button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolAction {
    #[default]
    None,
    ToggleTemplateBrowser,
    ToggleRangefinder,
    ToggleStencilBrowser,
    ToggleFloodFill,
    ToggleSphereTool,
    ToggleCubeTool,
    ToggleBridgeTool,
    ToggleCylinderTool,
    ToggleWallTool,
    ToggleFloorTool,
    ToggleReplaceTool,
    ToggleCircleTool,
    ToggleMirrorTool,
    ToggleStairsTool,
    ToggleArchTool,
    ToggleConeTool,
    ToggleCloneTool,
}

/// Which tool is currently active/highlighted in the palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveTool {
    #[default]
    None,
    Template,
    Measurement,
    Stencil,
    FloodFill,
    Sphere,
    Cube,
    Bridge,
    Cylinder,
    Wall,
    Floor,
    Replace,
    Circle,
    Mirror,
    Stairs,
    Arch,
    Cone,
    Clone,
}

impl ActiveTool {
    /// Returns the display name for the tool.
    pub fn name(&self) -> &'static str {
        match self {
            ActiveTool::None => "None",
            ActiveTool::Template => "Template",
            ActiveTool::Measurement => "Measurement",
            ActiveTool::Stencil => "Stencil",
            ActiveTool::FloodFill => "Flood Fill",
            ActiveTool::Sphere => "Sphere",
            ActiveTool::Cube => "Cube",
            ActiveTool::Bridge => "Bridge",
            ActiveTool::Cylinder => "Cylinder",
            ActiveTool::Wall => "Wall",
            ActiveTool::Floor => "Floor",
            ActiveTool::Replace => "Replace",
            ActiveTool::Circle => "Circle",
            ActiveTool::Mirror => "Mirror",
            ActiveTool::Stairs => "Stairs",
            ActiveTool::Arch => "Arch",
            ActiveTool::Cone => "Cone/Pyramid",
            ActiveTool::Clone => "Clone/Array",
        }
    }

    /// Returns the icon character for the tool.
    pub fn icon(&self) -> &'static str {
        match self {
            ActiveTool::None => "",
            ActiveTool::Template => "📋",    // Blueprint/clipboard
            ActiveTool::Measurement => "📏", // Ruler
            ActiveTool::Stencil => "👻",     // Ghost block
            ActiveTool::FloodFill => "🪣",   // Paint bucket
            ActiveTool::Sphere => "🔵",      // Blue circle
            ActiveTool::Cube => "🟦",        // Blue square
            ActiveTool::Bridge => "📍",      // Pin/marker for line endpoints
            ActiveTool::Cylinder => "⬤",     // Filled circle for column/cylinder
            ActiveTool::Wall => "▮",         // Vertical rectangle for walls
            ActiveTool::Floor => "▬",        // Horizontal rectangle for floors
            ActiveTool::Replace => "↔",      // Swap/exchange symbol for replace
            ActiveTool::Circle => "◯",       // Circle outline
            ActiveTool::Mirror => "⟷",       // Symmetric reflection arrows
            ActiveTool::Stairs => "📶",      // Step pattern
            ActiveTool::Arch => "∩",         // Arch shape
            ActiveTool::Cone => "△",         // Triangle for cone/pyramid
            ActiveTool::Clone => "⧉",        // Grid pattern for clone/array
        }
    }

    /// Returns the hotkey hint for the tool.
    pub fn hotkey(&self) -> &'static str {
        match self {
            ActiveTool::None => "",
            ActiveTool::Template => "L",
            ActiveTool::Measurement => "G",
            ActiveTool::Stencil => "K",
            ActiveTool::FloodFill => "", // No dedicated hotkey, tools palette/console only
            ActiveTool::Sphere => "",    // No dedicated hotkey, button only
            ActiveTool::Cube => "",      // No dedicated hotkey, button only
            ActiveTool::Bridge => "",    // No dedicated hotkey, button only
            ActiveTool::Cylinder => "",  // No dedicated hotkey, button only
            ActiveTool::Wall => "",      // No dedicated hotkey, button only
            ActiveTool::Floor => "",     // No dedicated hotkey, button only
            ActiveTool::Replace => "",   // No dedicated hotkey, button only
            ActiveTool::Circle => "",    // No dedicated hotkey, button only
            ActiveTool::Mirror => "",    // No dedicated hotkey, button only
            ActiveTool::Stairs => "",    // No dedicated hotkey, button only
            ActiveTool::Arch => "",      // No dedicated hotkey, button only
            ActiveTool::Cone => "",      // No dedicated hotkey, button only
            ActiveTool::Clone => "",     // No dedicated hotkey, button only
        }
    }

    /// Returns a description of what the tool does.
    pub fn description(&self) -> &'static str {
        match self {
            ActiveTool::None => "",
            ActiveTool::Template => "Copy and paste building regions",
            ActiveTool::Measurement => "Place markers and measure distances",
            ActiveTool::Stencil => "Create holographic building guides",
            ActiveTool::FloodFill => "Right-click to fill connected blocks",
            ActiveTool::Sphere => "Place solid or hollow spheres",
            ActiveTool::Cube => "Place solid or hollow cubes/boxes",
            ActiveTool::Bridge => "Draw line between two points",
            ActiveTool::Cylinder => "Place solid or hollow cylinders",
            ActiveTool::Wall => "Build walls with two-click corners",
            ActiveTool::Floor => "Build floors with two-click corners",
            ActiveTool::Replace => "Find and replace blocks in selection",
            ActiveTool::Circle => "Place circles or ellipses",
            ActiveTool::Mirror => "Mirror block placements symmetrically",
            ActiveTool::Stairs => "Build staircases between two points",
            ActiveTool::Arch => "Create architectural arches and doorways",
            ActiveTool::Cone => "Place cones or pyramids",
            ActiveTool::Clone => "Clone selection in patterns (linear/grid)",
        }
    }
}

// ============================================================================
// Tool Settings
// ============================================================================

/// Preset colors for the measurement laser.
pub const LASER_COLOR_PRESETS: &[([f32; 3], &str)] = &[
    ([1.0, 0.2, 0.2], "Red"),
    ([0.2, 1.0, 0.2], "Green"),
    ([0.2, 0.6, 1.0], "Blue"),
    ([1.0, 1.0, 0.2], "Yellow"),
    ([1.0, 0.5, 0.0], "Orange"),
    ([0.8, 0.2, 1.0], "Purple"),
    ([0.0, 1.0, 1.0], "Cyan"),
    ([1.0, 1.0, 1.0], "White"),
];

/// Settings for the measurement/rangefinder tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeasurementSettings {
    /// Color for the laser rangefinder display (RGB 0-1).
    pub laser_color: [f32; 3],
    /// Index of the selected color preset (for UI).
    pub color_preset_index: usize,
}

impl Default for MeasurementSettings {
    fn default() -> Self {
        Self {
            laser_color: LASER_COLOR_PRESETS[0].0, // Default: Red
            color_preset_index: 0,
        }
    }
}

/// Settings for the stencil tool (references StencilManager for actual values).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StencilSettings {
    /// Global opacity for stencils (0.3-0.8).
    pub opacity: f32,
    /// Render mode (wireframe or solid).
    pub render_mode: StencilRenderMode,
}

impl Default for StencilSettings {
    fn default() -> Self {
        Self {
            opacity: 0.5,
            render_mode: StencilRenderMode::Solid,
        }
    }
}

/// Settings for the flood fill tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloodFillSettings {
    /// Whether to show a preview of affected blocks before filling.
    pub preview_mode: bool,
    /// Maximum blocks to preview (performance limit).
    pub preview_limit: usize,
}

impl Default for FloodFillSettings {
    fn default() -> Self {
        Self {
            preview_mode: false,
            preview_limit: 5000,
        }
    }
}

/// All tool settings combined.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolSettings {
    pub measurement: MeasurementSettings,
    pub stencil: StencilSettings,
    pub flood_fill: FloodFillSettings,
}

/// Tools palette UI state.
#[derive(Debug, Clone, Default)]
pub struct ToolsPaletteState {
    /// Whether the tools palette window is open.
    pub open: bool,
    /// Which tool is currently highlighted/active.
    pub active_tool: ActiveTool,
    /// Whether the settings panel is expanded.
    pub show_settings: bool,
    /// Per-tool settings.
    pub settings: ToolSettings,
    /// Whether we were focused before opening the palette (to restore on close).
    pub previously_focused: bool,
}

impl ToolsPaletteState {
    /// Toggle the palette open/closed.
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }
}

/// Tools palette UI renderer.
pub struct ToolsPaletteUI;

/// Result of drawing the tools palette.
#[derive(Debug, Clone, Default)]
pub struct ToolsPaletteResult {
    /// Action requested by clicking a tool button.
    pub action: ToolAction,
    /// Whether stencil opacity was changed (value to sync).
    pub stencil_opacity_changed: Option<f32>,
    /// Whether stencil render mode was changed.
    pub stencil_render_mode_changed: Option<StencilRenderMode>,
}

impl ToolsPaletteUI {
    /// Draw the tools palette window.
    ///
    /// Returns a `ToolsPaletteResult` with any actions or setting changes.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_tools_window(
        ctx: &egui::Context,
        state: &mut ToolsPaletteState,
        template_browser_open: bool,
        rangefinder_active: bool,
        stencil_browser_open: bool,
        selection_mode_active: bool,
        flood_fill_active: bool,
        sphere_tool_active: bool,
        cube_tool_active: bool,
        bridge_tool_active: bool,
        cylinder_tool_active: bool,
        wall_tool_active: bool,
        floor_tool_active: bool,
        replace_tool_active: bool,
        circle_tool_active: bool,
        mirror_tool_active: bool,
        stairs_tool_active: bool,
        arch_tool_active: bool,
        cone_tool_active: bool,
        clone_tool_active: bool,
        stencil_opacity: f32,
        stencil_render_mode: StencilRenderMode,
    ) -> ToolsPaletteResult {
        // Sync stencil settings from manager on each frame
        state.settings.stencil.opacity = stencil_opacity;
        state.settings.stencil.render_mode = stencil_render_mode;

        if !state.open {
            return ToolsPaletteResult::default();
        }

        let mut result = ToolsPaletteResult::default();

        // Determine active tool based on current state
        let active = if template_browser_open || selection_mode_active {
            ActiveTool::Template
        } else if rangefinder_active {
            ActiveTool::Measurement
        } else if stencil_browser_open {
            ActiveTool::Stencil
        } else if flood_fill_active {
            ActiveTool::FloodFill
        } else if sphere_tool_active {
            ActiveTool::Sphere
        } else if cube_tool_active {
            ActiveTool::Cube
        } else if bridge_tool_active {
            ActiveTool::Bridge
        } else if cylinder_tool_active {
            ActiveTool::Cylinder
        } else if wall_tool_active {
            ActiveTool::Wall
        } else if floor_tool_active {
            ActiveTool::Floor
        } else if replace_tool_active {
            ActiveTool::Replace
        } else if circle_tool_active {
            ActiveTool::Circle
        } else if mirror_tool_active {
            ActiveTool::Mirror
        } else if stairs_tool_active {
            ActiveTool::Stairs
        } else if arch_tool_active {
            ActiveTool::Arch
        } else if cone_tool_active {
            ActiveTool::Cone
        } else if clone_tool_active {
            ActiveTool::Clone
        } else {
            ActiveTool::None
        };
        state.active_tool = active;

        // Extract values we need to avoid borrow conflicts
        let active_tool = state.active_tool;
        let show_settings = state.show_settings;

        let mut window_open = state.open;
        let mut toggle_settings = false;

        egui::Window::new("Tools")
            .default_pos(egui::pos2(ctx.screen_rect().width() - 200.0, 100.0))
            .default_size(egui::vec2(180.0, 380.0))
            .resizable(false)
            .collapsible(true)
            .open(&mut window_open)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    // Tool buttons
                    let tools = [
                        ActiveTool::Template,
                        ActiveTool::Measurement,
                        ActiveTool::Stencil,
                        ActiveTool::FloodFill,
                        ActiveTool::Sphere,
                        ActiveTool::Cube,
                        ActiveTool::Circle,
                        ActiveTool::Cylinder,
                        ActiveTool::Wall,
                        ActiveTool::Floor,
                        ActiveTool::Replace,
                        ActiveTool::Mirror,
                        ActiveTool::Stairs,
                        ActiveTool::Arch,
                        ActiveTool::Cone,
                        ActiveTool::Clone,
                        ActiveTool::Bridge,
                    ];

                    for tool in tools {
                        let is_active = active_tool == tool;
                        let response = Self::draw_tool_button(ui, tool, is_active);

                        if response.clicked() {
                            result.action = match tool {
                                ActiveTool::Template => ToolAction::ToggleTemplateBrowser,
                                ActiveTool::Measurement => ToolAction::ToggleRangefinder,
                                ActiveTool::Stencil => ToolAction::ToggleStencilBrowser,
                                ActiveTool::FloodFill => ToolAction::ToggleFloodFill,
                                ActiveTool::Sphere => ToolAction::ToggleSphereTool,
                                ActiveTool::Cube => ToolAction::ToggleCubeTool,
                                ActiveTool::Cylinder => ToolAction::ToggleCylinderTool,
                                ActiveTool::Wall => ToolAction::ToggleWallTool,
                                ActiveTool::Floor => ToolAction::ToggleFloorTool,
                                ActiveTool::Replace => ToolAction::ToggleReplaceTool,
                                ActiveTool::Circle => ToolAction::ToggleCircleTool,
                                ActiveTool::Mirror => ToolAction::ToggleMirrorTool,
                                ActiveTool::Stairs => ToolAction::ToggleStairsTool,
                                ActiveTool::Arch => ToolAction::ToggleArchTool,
                                ActiveTool::Cone => ToolAction::ToggleConeTool,
                                ActiveTool::Clone => ToolAction::ToggleCloneTool,
                                ActiveTool::Bridge => ToolAction::ToggleBridgeTool,
                                ActiveTool::None => ToolAction::None,
                            };
                        }

                        // Tooltip on hover
                        response.on_hover_ui(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{} {}", tool.icon(), tool.name()))
                                        .strong()
                                        .size(14.0),
                                );
                                ui.label(
                                    egui::RichText::new(tool.description())
                                        .color(egui::Color32::from_gray(180))
                                        .size(12.0),
                                );
                                if !tool.hotkey().is_empty() {
                                    ui.add_space(4.0);
                                    ui.label(
                                        egui::RichText::new(format!("Hotkey: {}", tool.hotkey()))
                                            .color(egui::Color32::from_rgb(255, 255, 100))
                                            .size(11.0),
                                    );
                                }
                            });
                        });
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // Settings toggle header
                    let settings_header = if show_settings {
                        "▼ Settings"
                    } else {
                        "▶ Settings"
                    };
                    if ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new(settings_header)
                                    .color(egui::Color32::from_gray(180))
                                    .size(12.0),
                            )
                            .sense(egui::Sense::click()),
                        )
                        .clicked()
                    {
                        toggle_settings = true;
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // Help text
                    ui.label(
                        egui::RichText::new("Press T to toggle")
                            .color(egui::Color32::from_gray(120))
                            .size(11.0),
                    );
                });
            });

        // Update state after window closes
        state.open = window_open;
        if toggle_settings {
            state.show_settings = !state.show_settings;
        }

        // Draw settings panel in a separate window if expanded
        if state.open && state.show_settings {
            Self::draw_settings_window(ctx, state, &mut result);
        }

        result
    }

    /// Draw the settings panel as a separate attached window.
    fn draw_settings_window(
        ctx: &egui::Context,
        state: &mut ToolsPaletteState,
        result: &mut ToolsPaletteResult,
    ) {
        egui::Window::new("Tool Settings")
            .default_pos(egui::pos2(ctx.screen_rect().width() - 200.0, 500.0))
            .default_size(egui::vec2(180.0, 150.0))
            .resizable(false)
            .collapsible(false)
            .title_bar(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgba_unmultiplied(30, 30, 40, 220))
                    .inner_margin(egui::Margin::same(8))
                    .corner_radius(egui::CornerRadius::same(4))
                    .show(ui, |ui| {
                        Self::draw_settings_panel(ui, state, result);
                    });
            });
    }

    /// Draw the settings panel content based on active tool.
    fn draw_settings_panel(
        ui: &mut egui::Ui,
        state: &mut ToolsPaletteState,
        result: &mut ToolsPaletteResult,
    ) {
        match state.active_tool {
            ActiveTool::Measurement => {
                Self::draw_measurement_settings(ui, &mut state.settings.measurement);
            }
            ActiveTool::Stencil => {
                Self::draw_stencil_settings(ui, &mut state.settings.stencil, result);
            }
            ActiveTool::FloodFill => {
                Self::draw_flood_fill_settings(ui, &mut state.settings.flood_fill);
            }
            ActiveTool::Template | ActiveTool::None => {
                ui.label(
                    egui::RichText::new("Select a tool to configure")
                        .color(egui::Color32::from_gray(140))
                        .size(11.0)
                        .italics(),
                );
            }
            ActiveTool::Sphere => {
                // Sphere tool has its own settings window (SphereToolUI)
                ui.label(
                    egui::RichText::new("Sphere settings in separate window")
                        .color(egui::Color32::from_gray(140))
                        .size(11.0)
                        .italics(),
                );
            }
            ActiveTool::Cube => {
                // Cube tool has its own settings window (CubeToolUI)
                ui.label(
                    egui::RichText::new("Cube settings in separate window")
                        .color(egui::Color32::from_gray(140))
                        .size(11.0)
                        .italics(),
                );
            }
            ActiveTool::Bridge => {
                // Bridge tool has its own status window (BridgeToolUI)
                ui.label(
                    egui::RichText::new("Bridge status in separate window")
                        .color(egui::Color32::from_gray(140))
                        .size(11.0)
                        .italics(),
                );
            }
            ActiveTool::Cylinder => {
                // Cylinder tool has its own settings window (CylinderToolUI)
                ui.label(
                    egui::RichText::new("Cylinder settings in separate window")
                        .color(egui::Color32::from_gray(140))
                        .size(11.0)
                        .italics(),
                );
            }
            ActiveTool::Wall => {
                // Wall tool has its own settings window (WallToolUI)
                ui.label(
                    egui::RichText::new("Wall settings in separate window")
                        .color(egui::Color32::from_gray(140))
                        .size(11.0)
                        .italics(),
                );
            }
            ActiveTool::Floor => {
                // Floor tool has its own settings window (FloorToolUI)
                ui.label(
                    egui::RichText::new("Floor settings in separate window")
                        .color(egui::Color32::from_gray(140))
                        .size(11.0)
                        .italics(),
                );
            }
            ActiveTool::Replace => {
                // Replace tool has its own settings window (ReplaceToolUI)
                ui.label(
                    egui::RichText::new("Replace settings in separate window")
                        .color(egui::Color32::from_gray(140))
                        .size(11.0)
                        .italics(),
                );
            }
            ActiveTool::Circle => {
                // Circle tool has its own settings window (CircleToolUI)
                ui.label(
                    egui::RichText::new("Circle settings in separate window")
                        .color(egui::Color32::from_gray(140))
                        .size(11.0)
                        .italics(),
                );
            }
            ActiveTool::Mirror => {
                // Mirror tool has its own settings window (MirrorToolUI)
                ui.label(
                    egui::RichText::new("Mirror settings in separate window")
                        .color(egui::Color32::from_gray(140))
                        .size(11.0)
                        .italics(),
                );
            }
            ActiveTool::Stairs => {
                // Stairs tool has its own settings window (StairsToolUI)
                ui.label(
                    egui::RichText::new("Stairs settings in separate window")
                        .color(egui::Color32::from_gray(140))
                        .size(11.0)
                        .italics(),
                );
            }
            ActiveTool::Arch => {
                // Arch tool has its own settings window (ArchToolUI)
                ui.label(
                    egui::RichText::new("Arch settings in separate window")
                        .color(egui::Color32::from_gray(140))
                        .size(11.0)
                        .italics(),
                );
            }
            ActiveTool::Cone => {
                // Cone tool has its own settings window (ConeToolUI)
                ui.label(
                    egui::RichText::new("Cone settings in separate window")
                        .color(egui::Color32::from_gray(140))
                        .size(11.0)
                        .italics(),
                );
            }
            ActiveTool::Clone => {
                // Clone tool has its own settings window (CloneToolUI)
                ui.label(
                    egui::RichText::new("Clone settings in separate window")
                        .color(egui::Color32::from_gray(140))
                        .size(11.0)
                        .italics(),
                );
            }
        }
    }

    /// Draw measurement tool settings.
    fn draw_measurement_settings(ui: &mut egui::Ui, settings: &mut MeasurementSettings) {
        ui.label(
            egui::RichText::new("📏 Measurement")
                .strong()
                .size(12.0)
                .color(egui::Color32::from_rgb(200, 200, 255)),
        );
        ui.add_space(6.0);

        // Laser color presets
        ui.label(
            egui::RichText::new("Laser Color")
                .size(11.0)
                .color(egui::Color32::from_gray(180)),
        );

        // Use a grid for reliable button layout
        egui::Grid::new("laser_color_grid")
            .spacing([4.0, 4.0])
            .show(ui, |ui| {
                for (i, (color, name)) in LASER_COLOR_PRESETS.iter().enumerate() {
                    let is_selected = settings.color_preset_index == i;
                    let color32 = egui::Color32::from_rgb(
                        (color[0] * 255.0) as u8,
                        (color[1] * 255.0) as u8,
                        (color[2] * 255.0) as u8,
                    );

                    let size = egui::vec2(20.0, 20.0);

                    // Use a Button with custom fill color for reliable click handling
                    let button = egui::Button::new("")
                        .fill(color32)
                        .min_size(size)
                        .stroke(if is_selected {
                            egui::Stroke::new(2.0, egui::Color32::WHITE)
                        } else {
                            egui::Stroke::new(1.0, egui::Color32::from_gray(60))
                        })
                        .corner_radius(egui::CornerRadius::same(3));

                    let response = ui.add(button);

                    if response.clicked() {
                        settings.color_preset_index = i;
                        settings.laser_color = *color;
                    }

                    response.on_hover_text(*name);

                    // 4 colors per row
                    if (i + 1) % 4 == 0 {
                        ui.end_row();
                    }
                }
            });
    }

    /// Draw stencil tool settings.
    fn draw_stencil_settings(
        ui: &mut egui::Ui,
        settings: &mut StencilSettings,
        result: &mut ToolsPaletteResult,
    ) {
        ui.label(
            egui::RichText::new("👻 Stencil")
                .strong()
                .size(12.0)
                .color(egui::Color32::from_rgb(200, 255, 200)),
        );
        ui.add_space(6.0);

        // Opacity slider
        ui.label(
            egui::RichText::new("Opacity")
                .size(11.0)
                .color(egui::Color32::from_gray(180)),
        );

        let old_opacity = settings.opacity;
        ui.add(
            egui::Slider::new(&mut settings.opacity, 0.3..=0.8)
                .show_value(true)
                .custom_formatter(|n, _| format!("{:.0}%", n * 100.0)),
        );
        if (settings.opacity - old_opacity).abs() > 0.001 {
            result.stencil_opacity_changed = Some(settings.opacity);
        }

        ui.add_space(6.0);

        // Render mode toggle
        ui.label(
            egui::RichText::new("Render Mode")
                .size(11.0)
                .color(egui::Color32::from_gray(180)),
        );

        ui.horizontal(|ui| {
            let is_solid = settings.render_mode == StencilRenderMode::Solid;

            if ui.selectable_label(is_solid, "Solid").clicked() && !is_solid {
                settings.render_mode = StencilRenderMode::Solid;
                result.stencil_render_mode_changed = Some(StencilRenderMode::Solid);
            }

            if ui.selectable_label(!is_solid, "Wireframe").clicked() && is_solid {
                settings.render_mode = StencilRenderMode::Wireframe;
                result.stencil_render_mode_changed = Some(StencilRenderMode::Wireframe);
            }
        });

        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("[ ] keys: adjust opacity")
                .size(10.0)
                .color(egui::Color32::from_gray(120)),
        );
    }

    /// Draw flood fill tool settings.
    fn draw_flood_fill_settings(ui: &mut egui::Ui, settings: &mut FloodFillSettings) {
        ui.label(
            egui::RichText::new("🪣 Flood Fill")
                .strong()
                .size(12.0)
                .color(egui::Color32::from_rgb(255, 200, 150)),
        );
        ui.add_space(6.0);

        // Preview mode checkbox
        ui.checkbox(&mut settings.preview_mode, "Preview Mode");
        ui.label(
            egui::RichText::new("Show affected blocks before filling")
                .size(10.0)
                .color(egui::Color32::from_gray(140)),
        );

        if settings.preview_mode {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Preview Limit")
                    .size(11.0)
                    .color(egui::Color32::from_gray(180)),
            );
            ui.add(
                egui::Slider::new(&mut settings.preview_limit, 1000..=10000)
                    .logarithmic(true)
                    .suffix(" blocks"),
            );
        }
    }

    /// Draw a single tool button with icon, label, and hotkey.
    fn draw_tool_button(ui: &mut egui::Ui, tool: ActiveTool, is_active: bool) -> egui::Response {
        let button_height = 48.0;
        let button_width = ui.available_width();

        // Colors
        let (bg_color, border_color, text_color) = if is_active {
            (
                egui::Color32::from_rgba_unmultiplied(60, 120, 60, 200),
                egui::Color32::from_rgb(100, 255, 100),
                egui::Color32::WHITE,
            )
        } else {
            (
                egui::Color32::from_rgba_unmultiplied(40, 40, 50, 180),
                egui::Color32::from_gray(80),
                egui::Color32::from_gray(200),
            )
        };

        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(button_width, button_height),
            egui::Sense::click(),
        );

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();

            // Hover effect
            let bg = if response.hovered() && !is_active {
                egui::Color32::from_rgba_unmultiplied(50, 50, 65, 200)
            } else {
                bg_color
            };

            // Background
            painter.rect(
                rect,
                egui::CornerRadius::same(6),
                bg,
                egui::Stroke::new(if is_active { 2.0 } else { 1.0 }, border_color),
                egui::StrokeKind::Outside,
            );

            // Icon on the left
            let icon_pos = egui::pos2(rect.left() + 12.0, rect.center().y);
            painter.text(
                icon_pos,
                egui::Align2::LEFT_CENTER,
                tool.icon(),
                egui::FontId::proportional(20.0),
                text_color,
            );

            // Tool name
            let name_pos = egui::pos2(rect.left() + 40.0, rect.center().y - 6.0);
            painter.text(
                name_pos,
                egui::Align2::LEFT_CENTER,
                tool.name(),
                egui::FontId::proportional(14.0),
                text_color,
            );

            // Hotkey badge
            if !tool.hotkey().is_empty() {
                let hotkey_pos = egui::pos2(rect.left() + 40.0, rect.center().y + 10.0);
                painter.text(
                    hotkey_pos,
                    egui::Align2::LEFT_CENTER,
                    tool.hotkey(),
                    egui::FontId::proportional(11.0),
                    egui::Color32::from_rgb(255, 200, 100),
                );
            }

            // Active indicator dot
            if is_active {
                let dot_pos = egui::pos2(rect.right() - 12.0, rect.center().y);
                painter.circle_filled(dot_pos, 5.0, egui::Color32::from_rgb(100, 255, 100));
            }
        }

        response
    }
}

//! Tools Palette UI - provides quick access to building tools.
//!
//! Tools available:
//! - Template (L key): Copy/paste building templates
//! - Measurement (G key): Place markers and measure distances
//! - Stencil (K key): Holographic building guides
//! - Flood Fill: Mass block replacement (console only for now)

use egui_winit_vulkano::egui;

/// Which tool is currently active/highlighted in the palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveTool {
    #[default]
    None,
    Template,
    Measurement,
    Stencil,
    FloodFill,
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
        }
    }

    /// Returns the hotkey hint for the tool.
    pub fn hotkey(&self) -> &'static str {
        match self {
            ActiveTool::None => "",
            ActiveTool::Template => "L",
            ActiveTool::Measurement => "G",
            ActiveTool::Stencil => "K",
            ActiveTool::FloodFill => "/ff",
        }
    }

    /// Returns a description of what the tool does.
    pub fn description(&self) -> &'static str {
        match self {
            ActiveTool::None => "",
            ActiveTool::Template => "Copy and paste building regions",
            ActiveTool::Measurement => "Place markers and measure distances",
            ActiveTool::Stencil => "Create holographic building guides",
            ActiveTool::FloodFill => "Replace connected blocks (console)",
        }
    }
}

/// Tools palette UI state.
#[derive(Debug, Clone, Default)]
pub struct ToolsPaletteState {
    /// Whether the tools palette window is open.
    pub open: bool,
    /// Which tool is currently highlighted/active.
    pub active_tool: ActiveTool,
}

impl ToolsPaletteState {
    /// Toggle the palette open/closed.
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }
}

/// Tools palette UI renderer.
pub struct ToolsPaletteUI;

impl ToolsPaletteUI {
    /// Draw the tools palette window.
    ///
    /// Returns true if a tool was clicked and the window should trigger that tool's action.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_tools_window(
        ctx: &egui::Context,
        state: &mut ToolsPaletteState,
        template_browser_open: bool,
        rangefinder_active: bool,
        stencil_browser_open: bool,
        selection_mode_active: bool,
    ) -> Option<ActiveTool> {
        if !state.open {
            return None;
        }

        let mut clicked_tool: Option<ActiveTool> = None;

        // Determine active tool based on current state
        let active = if template_browser_open || selection_mode_active {
            ActiveTool::Template
        } else if rangefinder_active {
            ActiveTool::Measurement
        } else if stencil_browser_open {
            ActiveTool::Stencil
        } else {
            ActiveTool::None
        };
        state.active_tool = active;

        egui::Window::new("Tools")
            .default_pos(egui::pos2(ctx.screen_rect().width() - 180.0, 100.0))
            .default_size(egui::vec2(160.0, 280.0))
            .resizable(false)
            .collapsible(true)
            .open(&mut state.open)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    // Tool buttons
                    let tools = [
                        ActiveTool::Template,
                        ActiveTool::Measurement,
                        ActiveTool::Stencil,
                        ActiveTool::FloodFill,
                    ];

                    for tool in tools {
                        let is_active = state.active_tool == tool;
                        let response = Self::draw_tool_button(ui, tool, is_active);

                        if response.clicked() {
                            clicked_tool = Some(tool);
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

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Help text
                    ui.label(
                        egui::RichText::new("Press T to toggle")
                            .color(egui::Color32::from_gray(120))
                            .size(11.0),
                    );
                });
            });

        clicked_tool
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

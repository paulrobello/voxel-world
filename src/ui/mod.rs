//! User interface rendering for the voxel game.
//!
//! This module contains all HUD and UI components including:
//! - Performance stats and debug overlays
//! - Block/model palette
//! - Settings menus
//! - Hotbar and inventory
//! - Minimap and compass
//! - Console

use crate::app_state::{PaletteItem, PaletteTab};
use crate::block_update::BlockUpdateQueue;
use crate::chunk::BlockType;
use crate::config::Settings;
use crate::console::ConsoleState;
use crate::editor::{EditorAction, EditorState};
use crate::gpu_resources::SpriteIcons;
use crate::hud::Minimap;
use crate::player::Player;
use crate::raycast::RaycastHit;
use crate::render_mode::RenderMode;
use crate::sub_voxel::ModelRegistry;
use crate::terrain_gen::TerrainGenerator;
use crate::utils::ChunkStats;
use egui_winit_vulkano::egui;
use nalgebra::Vector3;

pub mod arch_tool;
pub mod bridge_tool;
pub mod circle_tool;
pub mod clone_tool;
pub mod cone_tool;
pub mod console;
pub mod cube_tool;
pub mod cylinder_tool;
pub mod floor_tool;
pub mod helpers;
pub mod hotbar;
pub mod minimap;
pub mod mirror_tool;
pub mod palette;
pub mod replace_tool;
pub mod settings;
pub mod sphere_tool;
pub mod stairs_tool;
pub mod stats;
pub mod time;
pub mod tools;
pub mod torus_tool;
pub mod wall_tool;

use arch_tool::ArchToolUI;
use bridge_tool::BridgeToolUI;
use clone_tool::CloneToolUI;
use cone_tool::ConeToolUI;
use console::ConsoleUI;
use cube_tool::CubeToolUI;
use cylinder_tool::CylinderToolUI;
use floor_tool::FloorToolUI;
use hotbar::HotbarUI;
use minimap::MinimapUI;
use mirror_tool::MirrorToolUI;
use palette::PaletteUI;
use replace_tool::ReplaceToolUI;
use settings::SettingsUI;
use sphere_tool::SphereToolUI;
use stairs_tool::StairsToolUI;
use stats::StatsUI;
pub use tools::ToolAction;
use tools::{ToolsPaletteState, ToolsPaletteUI};
use torus_tool::TorusToolUI;
use wall_tool::WallToolUI;

/// Water/lava simulation stats for debug display.
#[derive(Debug, Clone, Copy, Default)]
pub struct FluidStats {
    /// Total water cells in grid.
    pub water_cells: usize,
    /// Active (potentially flowing) water cells.
    pub water_active: usize,
    /// Dirty (pending update) water cells.
    pub water_dirty: usize,
    /// Total lava cells in grid.
    pub lava_cells: usize,
    /// Active (potentially flowing) lava cells.
    pub lava_active: usize,
}

/// Bundles HUD inputs to avoid an oversized render signature.
pub struct HudInputs<'a> {
    pub fps: u32,
    pub chunk_stats: &'a ChunkStats,
    pub fluid_stats: FluidStats,
    pub player: &'a mut Player,
    pub world: &'a mut crate::world::World,
    pub terrain_generator: &'a TerrainGenerator,
    pub cave_generator: &'a crate::cave_gen::CaveGenerator,
    pub settings: &'a mut Settings,
    pub render_mode: &'a mut RenderMode,
    pub current_hit: &'a Option<RaycastHit>,
    pub selected_block: BlockType,
    pub hotbar_index: &'a mut usize,
    pub hotbar_blocks: &'a mut [BlockType; 9],
    pub hotbar_model_ids: &'a mut [u8; 9],
    pub hotbar_tint_indices: &'a mut [u8; 9],
    pub hotbar_paint_textures: &'a mut [u8; 9],
    pub minimap_image: Option<egui::ColorImage>,
    pub atlas_texture_id: egui::TextureId,
    pub sprite_icons: Option<&'a SpriteIcons>,
    pub camera_yaw: f32,
    pub player_world_pos: Vector3<f64>,
    pub time_of_day: &'a mut f32,
    pub day_cycle_paused: &'a mut bool,
    pub atmosphere: &'a mut crate::atmosphere::AtmosphereSettings,
    pub view_distance: &'a mut i32,
    pub load_distance: &'a mut i32,
    pub unload_distance: &'a mut i32,
    pub block_updates: &'a mut BlockUpdateQueue,
    pub show_minimap: &'a mut bool,
    pub minimap: &'a mut Minimap,
    pub minimap_cached_image: &'a mut Option<egui::ColorImage>,
    pub palette_open: &'a mut bool,
    pub palette_tab: &'a mut PaletteTab,
    pub palette_search: &'a mut String,
    pub dragging_item: &'a mut Option<PaletteItem>,
    pub model_registry: &'a ModelRegistry,
    pub editor: &'a mut EditorState,
    pub console: &'a mut ConsoleState,
    pub template_selection: &'a mut crate::templates::TemplateSelection,
    pub template_library: &'a crate::templates::TemplateLibrary,
    pub stencil_library: &'a crate::stencils::StencilLibrary,
    pub stencil_manager: &'a mut crate::stencils::StencilManager,
    pub water_grid: &'a crate::water::WaterGrid,
    pub active_placement: &'a mut Option<crate::templates::TemplatePlacement>,
    pub rangefinder_active: bool,
    pub flood_fill_active: bool,
    pub measurement_markers: &'a mut Vec<Vector3<i32>>,
    pub tools_palette: &'a mut ToolsPaletteState,
    pub stencil_browser_open: bool,
    pub sphere_tool: &'a mut crate::shape_tools::SphereToolState,
    pub cube_tool: &'a mut crate::shape_tools::CubeToolState,
    pub bridge_tool: &'a mut crate::shape_tools::BridgeToolState,
    pub cylinder_tool: &'a mut crate::shape_tools::CylinderToolState,
    pub wall_tool: &'a mut crate::shape_tools::WallToolState,
    pub floor_tool: &'a mut crate::shape_tools::FloorToolState,
    pub replace_tool: &'a mut crate::shape_tools::ReplaceToolState,
    pub circle_tool: &'a mut crate::shape_tools::CircleToolState,
    pub mirror_tool: &'a mut crate::shape_tools::MirrorToolState,
    pub stairs_tool: &'a mut crate::shape_tools::StairsToolState,
    pub arch_tool: &'a mut crate::shape_tools::ArchToolState,
    pub cone_tool: &'a mut crate::shape_tools::ConeToolState,
    pub clone_tool: &'a mut crate::shape_tools::CloneToolState,
    pub torus_tool: &'a mut crate::shape_tools::TorusToolState,
}

pub struct HUDRenderer;

impl HUDRenderer {
    pub fn render(
        &self,
        gui: &mut egui_winit_vulkano::Gui,
        input: HudInputs<'_>,
    ) -> (bool, EditorAction, ToolAction) {
        let HudInputs {
            fps,
            chunk_stats,
            fluid_stats,
            player,
            world,
            terrain_generator,
            cave_generator,
            settings,
            render_mode,
            current_hit,
            selected_block,
            hotbar_index,
            hotbar_blocks,
            hotbar_model_ids,
            hotbar_tint_indices,
            hotbar_paint_textures,
            minimap_image,
            atlas_texture_id,
            sprite_icons,
            camera_yaw,
            player_world_pos,
            time_of_day,
            day_cycle_paused,
            atmosphere,
            view_distance,
            load_distance,
            unload_distance,
            block_updates,
            show_minimap,
            minimap,
            minimap_cached_image,
            palette_open,
            palette_tab,
            palette_search,
            dragging_item,
            model_registry,
            editor,
            console,
            template_selection,
            template_library,
            stencil_library,
            stencil_manager,
            water_grid,
            active_placement,
            rangefinder_active,
            flood_fill_active,
            measurement_markers,
            tools_palette,
            stencil_browser_open,
            sphere_tool,
            cube_tool,
            bridge_tool,
            cylinder_tool,
            wall_tool,
            floor_tool,
            replace_tool,
            circle_tool,
            mirror_tool,
            stairs_tool,
            arch_tool,
            cone_tool,
            clone_tool,
            torus_tool,
        } = input;
        let mut scale_changed = false;
        let mut editor_action = EditorAction::None;
        let mut tool_action = ToolAction::None;
        gui.immediate_ui(|gui| {
            let ctx = gui.context();

            if settings.show_stats {
                StatsUI::draw_stats_overlay(&ctx, fps, chunk_stats, fluid_stats);
            }
            if settings.show_position {
                StatsUI::draw_position_overlay(&ctx, player_world_pos);
            }
            if settings.show_biome_debug {
                StatsUI::draw_biome_debug_overlay(&ctx, terrain_generator, player_world_pos);
            }
            PaletteUI::draw_palette_window(
                &ctx,
                atlas_texture_id,
                sprite_icons,
                palette_open,
                palette_tab,
                palette_search,
                dragging_item,
                model_registry,
                hotbar_blocks,
                hotbar_model_ids,
                hotbar_tint_indices,
                hotbar_paint_textures,
                hotbar_index,
            );

            // Drag preview near cursor
            if let Some(item) = dragging_item.as_ref() {
                HotbarUI::draw_drag_preview(&ctx, *item, atlas_texture_id, sprite_icons);
            }

            scale_changed = SettingsUI::draw_settings_window(
                &ctx,
                settings,
                render_mode,
                current_hit,
                player,
                world,
                selected_block,
                time_of_day,
                day_cycle_paused,
                atmosphere,
                view_distance,
                load_distance,
                unload_distance,
                block_updates,
                model_registry,
                minimap,
                show_minimap,
                minimap_cached_image,
            );

            // Tools palette (T key)
            let tools_result = ToolsPaletteUI::draw_tools_window(
                &ctx,
                tools_palette,
                template_selection.visual_mode,
                rangefinder_active,
                stencil_browser_open,
                template_selection.visual_mode,
                flood_fill_active,
                sphere_tool.active,
                cube_tool.active,
                bridge_tool.active,
                cylinder_tool.active,
                wall_tool.active,
                floor_tool.active,
                replace_tool.active,
                circle_tool.active,
                mirror_tool.active,
                stairs_tool.active,
                arch_tool.active,
                cone_tool.active,
                clone_tool.active,
                torus_tool.active,
                stencil_manager.global_opacity,
                stencil_manager.render_mode,
            );
            tool_action = tools_result.action;

            // Apply stencil setting changes from the tools palette
            if let Some(opacity) = tools_result.stencil_opacity_changed {
                stencil_manager.set_global_opacity(opacity);
                stencil_manager.apply_global_opacity_to_all();
            }
            if let Some(render_mode) = tools_result.stencil_render_mode_changed {
                stencil_manager.set_render_mode(render_mode);
            }

            // Sphere tool settings window
            SphereToolUI::draw(&ctx, sphere_tool);

            // Cube tool settings window
            CubeToolUI::draw(&ctx, cube_tool);

            // Bridge tool status window
            BridgeToolUI::draw(&ctx, bridge_tool);

            // Cylinder tool settings window
            CylinderToolUI::draw(&ctx, cylinder_tool);

            // Wall tool settings window
            WallToolUI::draw(&ctx, wall_tool);

            // Floor tool settings window
            FloorToolUI::draw(&ctx, floor_tool);

            // Replace tool settings window
            ReplaceToolUI::draw(&ctx, replace_tool, template_selection);

            // Circle tool settings window
            circle_tool::CircleToolUI::draw(&ctx, circle_tool);

            // Mirror tool settings window
            MirrorToolUI::draw(&ctx, mirror_tool);

            // Stairs tool settings window
            StairsToolUI::draw(&ctx, stairs_tool);

            // Arch tool settings window
            ArchToolUI::draw(&ctx, arch_tool);

            // Cone tool settings window
            ConeToolUI::draw(&ctx, cone_tool);

            // Clone tool settings window
            CloneToolUI::draw(&ctx, clone_tool, template_selection);

            // Torus tool settings window
            TorusToolUI::draw(&ctx, torus_tool);

            // Crosshair (hide when editor or console is open)
            if !editor.active && !console.active {
                Self::draw_crosshair(&ctx, current_hit);
            }

            // Rangefinder distance display (when active and targeting a block)
            if rangefinder_active && !editor.active && !console.active {
                Self::draw_rangefinder_overlay(
                    &ctx,
                    current_hit,
                    measurement_markers,
                    tools_palette.settings.measurement.laser_color,
                );
            }

            // Flood fill mode indicator
            if flood_fill_active && !editor.active && !console.active {
                Self::draw_flood_fill_overlay(&ctx, current_hit, &selected_block);
            }

            // Minimap settings panel integration
            if *show_minimap {
                Self::draw_minimap_settings(&ctx, show_minimap, minimap, minimap_cached_image);
            }

            // Get current biome name at player position
            let biome_name = terrain_generator
                .get_biome_info(player_world_pos.x as i32, player_world_pos.z as i32)
                .biome
                .display_name();

            MinimapUI::draw_minimap_and_compass(
                &ctx,
                show_minimap,
                minimap,
                minimap_image,
                minimap_cached_image,
                camera_yaw,
                settings.show_compass,
                biome_name,
            );

            HotbarUI::draw_hotbar(
                &ctx,
                hotbar_blocks,
                hotbar_model_ids,
                hotbar_tint_indices,
                hotbar_paint_textures,
                hotbar_index,
                atlas_texture_id,
                sprite_icons,
                dragging_item,
            );

            // Editor
            if editor.active {
                let library = crate::storage::model_format::LibraryManager::new("user_models");
                let _ = library.init();
                editor_action = crate::editor::draw_editor_ui(&ctx, editor, &library, "Player");
                crate::editor::draw_model_preview(&ctx, editor);
            }

            // Set raycast hit for commands that use crosshair targeting (e.g., floodfill)
            console.raycast_hit = current_hit.as_ref().map(|hit| hit.block_pos);

            ConsoleUI::draw_console(
                &ctx,
                console,
                world,
                player_world_pos,
                fluid_stats,
                template_selection,
                template_library,
                stencil_library,
                water_grid,
                active_placement,
                terrain_generator,
                cave_generator,
                measurement_markers,
            );

            // Template placement overlay
            if let Some(placement) = active_placement {
                Self::draw_template_placement_overlay(&ctx, placement);
            }

            // Selection mode indicator
            if template_selection.visual_mode {
                Self::draw_selection_mode_overlay(&ctx, template_selection);
            }
        });

        (scale_changed, editor_action, tool_action)
    }

    fn draw_selection_mode_overlay(
        ctx: &egui::Context,
        selection: &crate::templates::TemplateSelection,
    ) {
        let screen_rect = ctx.screen_rect();

        // Draw overlay at top center
        egui::Area::new(egui::Id::new("selection_mode_overlay"))
            .fixed_pos(egui::pos2(screen_rect.center().x, 40.0))
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgba_unmultiplied(30, 30, 40, 220))
                    .stroke(egui::Stroke::new(
                        2.0,
                        egui::Color32::from_rgb(100, 255, 100),
                    ))
                    .inner_margin(12.0)
                    .corner_radius(6.0)
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new("✏ SELECTION MODE")
                                    .color(egui::Color32::from_rgb(100, 255, 100))
                                    .size(16.0)
                                    .strong(),
                            );

                            // Show selection info if we have any markers
                            if let Some((min, max)) = selection.bounds() {
                                if let Some((w, h, d)) = selection.dimensions() {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "Selection: {}×{}×{} blocks",
                                            w, h, d
                                        ))
                                        .color(egui::Color32::from_gray(220))
                                        .size(13.0),
                                    );
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "From ({},{},{}) to ({},{},{})",
                                            min.x, min.y, min.z, max.x, max.y, max.z
                                        ))
                                        .color(egui::Color32::from_gray(180))
                                        .size(11.0),
                                    );
                                }
                            } else if selection.pos1.is_some() {
                                let pos = selection.pos1.unwrap();
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Pos1: ({},{},{}) • Right-click to set Pos2",
                                        pos.x, pos.y, pos.z
                                    ))
                                    .color(egui::Color32::from_gray(220))
                                    .size(13.0),
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new("Left-click to set Pos1")
                                        .color(egui::Color32::from_gray(220))
                                        .size(13.0),
                                );
                            }

                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new(
                                    "Left-click: Pos1 • Right-click: Pos2 • V: Exit",
                                )
                                .color(egui::Color32::from_rgb(255, 255, 100))
                                .size(14.0),
                            );
                        });
                    });
            });
    }

    fn draw_template_placement_overlay(
        ctx: &egui::Context,
        placement: &crate::templates::TemplatePlacement,
    ) {
        let screen_rect = ctx.screen_rect();

        // Draw overlay at top center
        egui::Area::new(egui::Id::new("template_placement_overlay"))
            .fixed_pos(egui::pos2(screen_rect.center().x, 40.0))
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgba_unmultiplied(30, 30, 30, 220))
                    .stroke(egui::Stroke::new(
                        2.0,
                        egui::Color32::from_rgb(100, 255, 100),
                    ))
                    .inner_margin(12.0)
                    .corner_radius(6.0)
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "📦 Template: {}",
                                    placement.template.name
                                ))
                                .color(egui::Color32::from_rgb(100, 255, 100))
                                .size(16.0)
                                .strong(),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}×{}×{} ({} blocks) • Rotation: {}°",
                                    placement.template.width,
                                    placement.template.height,
                                    placement.template.depth,
                                    placement.template.block_count(),
                                    placement.rotation * 90
                                ))
                                .color(egui::Color32::from_gray(220))
                                .size(13.0),
                            );
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new(
                                    "R - Rotate  •  Right Click - Place  •  Esc - Cancel",
                                )
                                .color(egui::Color32::from_rgb(255, 255, 100))
                                .size(14.0),
                            );
                        });
                    });
            });
    }

    fn draw_crosshair(ctx: &egui::Context, current_hit: &Option<RaycastHit>) {
        let screen_rect = ctx.screen_rect();
        let center = screen_rect.center();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("crosshair"),
        ));

        let targeting_block = current_hit.is_some();
        let (crosshair_size, crosshair_gap, crosshair_color) = if targeting_block {
            (12.0, 4.0, egui::Color32::from_rgb(100, 255, 100)) // Green, larger, with gap
        } else {
            (8.0, 0.0, egui::Color32::WHITE) // White, smaller, no gap
        };
        let stroke = egui::Stroke::new(2.0, crosshair_color);

        // Horizontal lines (with gap when targeting)
        painter.line_segment(
            [
                egui::pos2(center.x - crosshair_size, center.y),
                egui::pos2(center.x - crosshair_gap, center.y),
            ],
            stroke,
        );
        painter.line_segment(
            [
                egui::pos2(center.x + crosshair_gap, center.y),
                egui::pos2(center.x + crosshair_size, center.y),
            ],
            stroke,
        );
        // Vertical lines (with gap when targeting)
        painter.line_segment(
            [
                egui::pos2(center.x, center.y - crosshair_size),
                egui::pos2(center.x, center.y - crosshair_gap),
            ],
            stroke,
        );
        painter.line_segment(
            [
                egui::pos2(center.x, center.y + crosshair_gap),
                egui::pos2(center.x, center.y + crosshair_size),
            ],
            stroke,
        );
    }

    /// Draws laser rangefinder distance display below the crosshair.
    fn draw_rangefinder_overlay(
        ctx: &egui::Context,
        current_hit: &Option<RaycastHit>,
        measurement_markers: &[Vector3<i32>],
        laser_color: [f32; 3],
    ) {
        let screen_rect = ctx.screen_rect();
        let center = screen_rect.center();

        // Convert laser color from f32 RGB to egui Color32
        let laser_color32 = egui::Color32::from_rgb(
            (laser_color[0] * 255.0) as u8,
            (laser_color[1] * 255.0) as u8,
            (laser_color[2] * 255.0) as u8,
        );

        // Position the distance display below the crosshair (screen center)
        // Use anchor from CENTER_CENTER with positive Y offset to place below crosshair
        egui::Area::new(egui::Id::new("rangefinder_overlay"))
            .anchor(
                egui::Align2::CENTER_TOP,
                egui::vec2(0.0, screen_rect.height() * 0.5 + 50.0),
            )
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgba_unmultiplied(20, 20, 30, 200))
                    .stroke(egui::Stroke::new(1.5, laser_color32))
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .corner_radius(4.0)
                    .show(ui, |ui| {
                        if let Some(hit) = current_hit {
                            // Show distance with 1 decimal place
                            ui.label(
                                egui::RichText::new(format!("📏 {:.1} blocks", hit.distance))
                                    .color(laser_color32)
                                    .size(14.0)
                                    .strong(),
                            );
                        } else {
                            // No target
                            ui.label(
                                egui::RichText::new("📏 --.- blocks")
                                    .color(egui::Color32::from_gray(120))
                                    .size(14.0),
                            );
                        }

                        // Show marker count and instructions
                        if !measurement_markers.is_empty() {
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "Markers: {}/4",
                                    measurement_markers.len()
                                ))
                                .color(egui::Color32::from_rgb(100, 200, 255))
                                .size(12.0),
                            );
                        } else {
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new("Left-click to place markers")
                                    .color(egui::Color32::from_gray(150))
                                    .size(11.0),
                            );
                        }
                    });
            });

        // Draw measurement panel on the left side when markers exist
        if measurement_markers.len() >= 2 {
            egui::Area::new(egui::Id::new("measurement_panel"))
                .fixed_pos(egui::pos2(10.0, screen_rect.center().y - 50.0))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgba_unmultiplied(20, 20, 30, 220))
                        .stroke(egui::Stroke::new(
                            1.5,
                            egui::Color32::from_rgb(100, 200, 255),
                        ))
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .corner_radius(6.0)
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("📐 Measurements")
                                    .color(egui::Color32::from_rgb(100, 200, 255))
                                    .size(14.0)
                                    .strong(),
                            );
                            ui.add_space(4.0);

                            // Show distances between consecutive markers
                            for i in 0..measurement_markers.len() - 1 {
                                let p1 = measurement_markers[i];
                                let p2 = measurement_markers[i + 1];

                                let dx = (p2.x - p1.x).abs();
                                let dy = (p2.y - p1.y).abs();
                                let dz = (p2.z - p1.z).abs();

                                let dist = ((p2.x - p1.x).pow(2)
                                    + (p2.y - p1.y).pow(2)
                                    + (p2.z - p1.z).pow(2))
                                    as f32;
                                let dist = dist.sqrt();

                                // Marker colors matching shader
                                let colors = [
                                    egui::Color32::from_rgb(0, 255, 255),  // Cyan
                                    egui::Color32::from_rgb(255, 77, 255), // Magenta
                                    egui::Color32::from_rgb(255, 255, 77), // Yellow
                                    egui::Color32::from_rgb(255, 128, 51), // Orange
                                ];

                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{}", i + 1))
                                            .color(colors[i % 4])
                                            .size(12.0),
                                    );
                                    ui.label(
                                        egui::RichText::new("→")
                                            .color(egui::Color32::WHITE)
                                            .size(12.0),
                                    );
                                    ui.label(
                                        egui::RichText::new(format!("{}", i + 2))
                                            .color(colors[(i + 1) % 4])
                                            .size(12.0),
                                    );
                                    ui.label(
                                        egui::RichText::new(format!(": {:.1}", dist))
                                            .color(egui::Color32::WHITE)
                                            .size(12.0)
                                            .strong(),
                                    );
                                });

                                // Show axis breakdown
                                if dx > 0 || dy > 0 || dz > 0 {
                                    ui.horizontal(|ui| {
                                        ui.add_space(20.0);
                                        if dx > 0 {
                                            ui.label(
                                                egui::RichText::new(format!("X:{}", dx))
                                                    .color(egui::Color32::from_rgb(255, 100, 100))
                                                    .size(10.0),
                                            );
                                        }
                                        if dy > 0 {
                                            ui.label(
                                                egui::RichText::new(format!("Y:{}", dy))
                                                    .color(egui::Color32::from_rgb(100, 255, 100))
                                                    .size(10.0),
                                            );
                                        }
                                        if dz > 0 {
                                            ui.label(
                                                egui::RichText::new(format!("Z:{}", dz))
                                                    .color(egui::Color32::from_rgb(100, 100, 255))
                                                    .size(10.0),
                                            );
                                        }
                                    });
                                }
                            }

                            // Show total distance if 3+ markers
                            if measurement_markers.len() >= 3 {
                                ui.add_space(4.0);
                                let mut total = 0.0f32;
                                for i in 0..measurement_markers.len() - 1 {
                                    let p1 = measurement_markers[i];
                                    let p2 = measurement_markers[i + 1];
                                    let dist = ((p2.x - p1.x).pow(2)
                                        + (p2.y - p1.y).pow(2)
                                        + (p2.z - p1.z).pow(2))
                                        as f32;
                                    total += dist.sqrt();
                                }
                                ui.label(
                                    egui::RichText::new(format!("Total: {:.1} blocks", total))
                                        .color(egui::Color32::from_rgb(255, 200, 100))
                                        .size(12.0)
                                        .strong(),
                                );
                            }
                        });
                });
        }

        // Draw a laser beam line from crosshair extending outward when targeting
        if current_hit.is_some() {
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Background,
                egui::Id::new("rangefinder_laser"),
            ));

            // Draw a subtle red laser line extending from crosshair
            // This is a visual indicator - the actual 3D line would require projection
            let laser_color = egui::Color32::from_rgba_unmultiplied(255, 50, 50, 100);
            let laser_stroke = egui::Stroke::new(1.5, laser_color);

            // Draw decorative laser brackets around the crosshair
            let bracket_size = 20.0;
            let bracket_offset = 18.0;

            // Top-left bracket
            painter.line_segment(
                [
                    egui::pos2(center.x - bracket_offset, center.y - bracket_offset),
                    egui::pos2(
                        center.x - bracket_offset + bracket_size * 0.4,
                        center.y - bracket_offset,
                    ),
                ],
                laser_stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x - bracket_offset, center.y - bracket_offset),
                    egui::pos2(
                        center.x - bracket_offset,
                        center.y - bracket_offset + bracket_size * 0.4,
                    ),
                ],
                laser_stroke,
            );

            // Top-right bracket
            painter.line_segment(
                [
                    egui::pos2(center.x + bracket_offset, center.y - bracket_offset),
                    egui::pos2(
                        center.x + bracket_offset - bracket_size * 0.4,
                        center.y - bracket_offset,
                    ),
                ],
                laser_stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x + bracket_offset, center.y - bracket_offset),
                    egui::pos2(
                        center.x + bracket_offset,
                        center.y - bracket_offset + bracket_size * 0.4,
                    ),
                ],
                laser_stroke,
            );

            // Bottom-left bracket
            painter.line_segment(
                [
                    egui::pos2(center.x - bracket_offset, center.y + bracket_offset),
                    egui::pos2(
                        center.x - bracket_offset + bracket_size * 0.4,
                        center.y + bracket_offset,
                    ),
                ],
                laser_stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x - bracket_offset, center.y + bracket_offset),
                    egui::pos2(
                        center.x - bracket_offset,
                        center.y + bracket_offset - bracket_size * 0.4,
                    ),
                ],
                laser_stroke,
            );

            // Bottom-right bracket
            painter.line_segment(
                [
                    egui::pos2(center.x + bracket_offset, center.y + bracket_offset),
                    egui::pos2(
                        center.x + bracket_offset - bracket_size * 0.4,
                        center.y + bracket_offset,
                    ),
                ],
                laser_stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x + bracket_offset, center.y + bracket_offset),
                    egui::pos2(
                        center.x + bracket_offset,
                        center.y + bracket_offset - bracket_size * 0.4,
                    ),
                ],
                laser_stroke,
            );
        }
    }

    fn draw_minimap_settings(
        _ctx: &egui::Context,
        _show_minimap: &mut bool,
        _minimap: &mut crate::hud::Minimap,
        _minimap_cached_image: &mut Option<egui::ColorImage>,
    ) {
        // This is now integrated in settings window
        // Keeping this empty stub for potential future use
    }

    /// Draw flood fill mode indicator.
    fn draw_flood_fill_overlay(
        ctx: &egui::Context,
        current_hit: &Option<RaycastHit>,
        selected_block: &crate::chunk::BlockType,
    ) {
        let screen_rect = ctx.screen_rect();
        let center = screen_rect.center();

        // Position the mode indicator below the crosshair (consistent with rangefinder)
        egui::Area::new(egui::Id::new("flood_fill_overlay"))
            .anchor(
                egui::Align2::CENTER_TOP,
                egui::vec2(0.0, screen_rect.height() * 0.5 + 50.0),
            )
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgba_unmultiplied(20, 30, 50, 200))
                    .stroke(egui::Stroke::new(
                        1.5,
                        egui::Color32::from_rgb(100, 180, 255),
                    ))
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .corner_radius(4.0)
                    .show(ui, |ui| {
                        // Mode indicator
                        ui.label(
                            egui::RichText::new("🪣 FILL MODE")
                                .color(egui::Color32::from_rgb(100, 180, 255))
                                .size(14.0)
                                .strong(),
                        );

                        ui.add_space(4.0);

                        // Show what will be filled
                        if current_hit.is_some() {
                            ui.label(
                                egui::RichText::new(format!("Fill with: {:?}", selected_block))
                                    .color(egui::Color32::WHITE)
                                    .size(12.0),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("Aim at a block to fill")
                                    .color(egui::Color32::from_gray(150))
                                    .size(12.0),
                            );
                        }

                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("Right-click to fill | Esc to cancel")
                                .color(egui::Color32::from_gray(120))
                                .size(11.0),
                        );
                    });
            });

        // Draw paint bucket cursor overlay when targeting
        if current_hit.is_some() {
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Background,
                egui::Id::new("flood_fill_cursor"),
            ));

            // Draw decorative brackets with blue color
            let bracket_color = egui::Color32::from_rgba_unmultiplied(100, 180, 255, 150);
            let bracket_stroke = egui::Stroke::new(2.0, bracket_color);
            let bracket_size = 24.0;
            let bracket_offset = 16.0;

            // Top-left bracket
            painter.line_segment(
                [
                    egui::pos2(center.x - bracket_offset, center.y - bracket_offset),
                    egui::pos2(
                        center.x - bracket_offset + bracket_size * 0.5,
                        center.y - bracket_offset,
                    ),
                ],
                bracket_stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x - bracket_offset, center.y - bracket_offset),
                    egui::pos2(
                        center.x - bracket_offset,
                        center.y - bracket_offset + bracket_size * 0.5,
                    ),
                ],
                bracket_stroke,
            );

            // Top-right bracket
            painter.line_segment(
                [
                    egui::pos2(center.x + bracket_offset, center.y - bracket_offset),
                    egui::pos2(
                        center.x + bracket_offset - bracket_size * 0.5,
                        center.y - bracket_offset,
                    ),
                ],
                bracket_stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x + bracket_offset, center.y - bracket_offset),
                    egui::pos2(
                        center.x + bracket_offset,
                        center.y - bracket_offset + bracket_size * 0.5,
                    ),
                ],
                bracket_stroke,
            );

            // Bottom-left bracket
            painter.line_segment(
                [
                    egui::pos2(center.x - bracket_offset, center.y + bracket_offset),
                    egui::pos2(
                        center.x - bracket_offset + bracket_size * 0.5,
                        center.y + bracket_offset,
                    ),
                ],
                bracket_stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x - bracket_offset, center.y + bracket_offset),
                    egui::pos2(
                        center.x - bracket_offset,
                        center.y + bracket_offset - bracket_size * 0.5,
                    ),
                ],
                bracket_stroke,
            );

            // Bottom-right bracket
            painter.line_segment(
                [
                    egui::pos2(center.x + bracket_offset, center.y + bracket_offset),
                    egui::pos2(
                        center.x + bracket_offset - bracket_size * 0.5,
                        center.y + bracket_offset,
                    ),
                ],
                bracket_stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x + bracket_offset, center.y + bracket_offset),
                    egui::pos2(
                        center.x + bracket_offset,
                        center.y + bracket_offset - bracket_size * 0.5,
                    ),
                ],
                bracket_stroke,
            );
        }
    }
}

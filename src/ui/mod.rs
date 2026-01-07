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

pub mod console;
pub mod helpers;
pub mod hotbar;
pub mod minimap;
pub mod palette;
pub mod settings;
pub mod stats;
pub mod time;

use console::ConsoleUI;
use hotbar::HotbarUI;
use minimap::MinimapUI;
use palette::PaletteUI;
use settings::SettingsUI;
use stats::StatsUI;

/// Water/lava simulation stats for debug display.
#[derive(Debug, Clone, Copy, Default)]
pub struct FluidStats {
    /// Total water cells in grid.
    pub water_cells: usize,
    /// Active (potentially flowing) water cells.
    pub water_active: usize,
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
    pub unload_distance: &'a mut i32,
    pub block_updates: &'a mut BlockUpdateQueue,
    pub show_minimap: &'a mut bool,
    pub minimap: &'a mut Minimap,
    pub minimap_cached_image: &'a mut Option<egui::ColorImage>,
    pub palette_open: &'a mut bool,
    pub palette_tab: &'a mut PaletteTab,
    pub dragging_item: &'a mut Option<PaletteItem>,
    pub model_registry: &'a ModelRegistry,
    pub editor: &'a mut EditorState,
    pub console: &'a mut ConsoleState,
}

pub struct HUDRenderer;

impl HUDRenderer {
    pub fn render(
        &self,
        gui: &mut egui_winit_vulkano::Gui,
        input: HudInputs<'_>,
    ) -> (bool, EditorAction) {
        let HudInputs {
            fps,
            chunk_stats,
            fluid_stats,
            player,
            world,
            terrain_generator,
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
            unload_distance,
            block_updates,
            show_minimap,
            minimap,
            minimap_cached_image,
            palette_open,
            palette_tab,
            dragging_item,
            model_registry,
            editor,
            console,
        } = input;
        let mut scale_changed = false;
        let mut editor_action = EditorAction::None;
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
                unload_distance,
                block_updates,
                model_registry,
                minimap,
                show_minimap,
                minimap_cached_image,
            );

            // Crosshair (hide when editor or console is open)
            if !editor.active && !console.active {
                Self::draw_crosshair(&ctx, current_hit);
            }

            // Minimap settings panel integration
            if *show_minimap {
                Self::draw_minimap_settings(&ctx, show_minimap, minimap, minimap_cached_image);
            }

            MinimapUI::draw_minimap_and_compass(
                &ctx,
                show_minimap,
                minimap,
                minimap_image,
                minimap_cached_image,
                camera_yaw,
                settings.show_compass,
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

            ConsoleUI::draw_console(&ctx, console, world, player_world_pos, fluid_stats);
        });

        (scale_changed, editor_action)
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

    fn draw_minimap_settings(
        _ctx: &egui::Context,
        _show_minimap: &mut bool,
        _minimap: &mut crate::hud::Minimap,
        _minimap_cached_image: &mut Option<egui::ColorImage>,
    ) {
        // This is now integrated in settings window
        // Keeping this empty stub for potential future use
    }
}

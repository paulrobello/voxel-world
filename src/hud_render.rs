use crate::block_update::BlockUpdateQueue;
use crate::chunk::BlockType;
use crate::config::Settings;
use crate::console::ConsoleState;
use crate::editor::{EditorAction, EditorState, draw_editor_ui, draw_model_preview};
use crate::gpu_resources::SpriteIcons;
use crate::hud::Minimap;
use crate::player::Player;
use crate::raycast::RaycastHit;
use crate::render_mode::RenderMode;
use crate::storage::model_format::LibraryManager;
use crate::sub_voxel::ModelRegistry;
use crate::utils::ChunkStats;
use crate::{PaletteItem, PaletteTab};
use egui_winit_vulkano::{Gui, egui};
use nalgebra::Vector3;

/// Bundles HUD inputs to avoid an oversized render signature.
pub struct HudInputs<'a> {
    pub fps: u32,
    pub chunk_stats: &'a ChunkStats,
    pub player: &'a mut Player,
    pub world: &'a mut crate::world::World,
    pub settings: &'a mut Settings,
    pub render_mode: &'a mut RenderMode,
    pub current_hit: &'a Option<RaycastHit>,
    pub selected_block: BlockType,
    pub hotbar_index: &'a mut usize,
    pub hotbar_blocks: &'a mut [BlockType; 9],
    pub hotbar_model_ids: &'a mut [u8; 9],
    pub hotbar_tint_indices: &'a mut [u8; 9],
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
    /// Convert tint_index to egui::Color32 for UI display.
    /// Matches TINT_PALETTE in shaders/common.glsl.
    fn tint_color(tint_index: u8) -> egui::Color32 {
        match tint_index {
            0 => egui::Color32::from_rgb(255, 51, 51),    // Red
            1 => egui::Color32::from_rgb(255, 128, 51),   // Orange
            2 => egui::Color32::from_rgb(255, 255, 51),   // Yellow
            3 => egui::Color32::from_rgb(128, 255, 51),   // Lime
            4 => egui::Color32::from_rgb(51, 255, 51),    // Green
            5 => egui::Color32::from_rgb(51, 255, 128),   // Teal
            6 => egui::Color32::from_rgb(51, 255, 255),   // Cyan
            7 => egui::Color32::from_rgb(51, 128, 255),   // Sky blue
            8 => egui::Color32::from_rgb(51, 51, 255),    // Blue
            9 => egui::Color32::from_rgb(128, 51, 255),   // Purple
            10 => egui::Color32::from_rgb(255, 51, 255),  // Magenta
            11 => egui::Color32::from_rgb(255, 51, 128),  // Pink
            12 => egui::Color32::from_rgb(242, 242, 242), // White
            13 => egui::Color32::from_rgb(153, 153, 153), // Light gray
            14 => egui::Color32::from_rgb(77, 77, 77),    // Dark gray
            15 => egui::Color32::from_rgb(102, 64, 26),   // Brown
            _ => egui::Color32::from_rgb(200, 200, 200),  // Default gray
        }
    }

    fn sprite_for_item(item: PaletteItem, icons: Option<&SpriteIcons>) -> Option<egui::TextureId> {
        let set = icons?;
        match item.block {
            BlockType::Model => set.model.get(&item.model_id).copied().or(Some(set.missing)),
            BlockType::TintedGlass => set
                .tinted_glass
                .get(&item.tint_index)
                .copied()
                .or(Some(set.missing)),
            BlockType::Air => None,
            _ => set.block.get(&item.block).copied().or(Some(set.missing)),
        }
    }

    fn atlas_tile_for(block: BlockType, model_id: u8) -> f32 {
        if block == BlockType::Model {
            match model_id {
                1 => 11.0,     // Torch
                4..=27 => 4.0, // Wood-based models use planks texture
                29 => 4.0,     // Ladder
                _ => 11.0,
            }
        } else {
            block as u8 as f32
        }
    }

    fn apply_item_to_slot(
        item: PaletteItem,
        slot: usize,
        hotbar_blocks: &mut [BlockType; 9],
        hotbar_model_ids: &mut [u8; 9],
        hotbar_tint_indices: &mut [u8; 9],
    ) {
        hotbar_blocks[slot] = item.block;
        hotbar_model_ids[slot] = if item.block == BlockType::Model {
            item.model_id
        } else {
            0
        };
        hotbar_tint_indices[slot] = if item.block == BlockType::TintedGlass {
            item.tint_index
        } else {
            0
        };
    }

    fn fill_or_replace_hotbar(
        item: PaletteItem,
        hotbar_blocks: &mut [BlockType; 9],
        hotbar_model_ids: &mut [u8; 9],
        hotbar_tint_indices: &mut [u8; 9],
        hotbar_index: &mut usize,
    ) {
        if let Some(empty_slot) = hotbar_blocks.iter().position(|b| *b == BlockType::Air) {
            Self::apply_item_to_slot(
                item,
                empty_slot,
                hotbar_blocks,
                hotbar_model_ids,
                hotbar_tint_indices,
            );
            *hotbar_index = empty_slot;
        } else {
            let idx = *hotbar_index;
            Self::apply_item_to_slot(
                item,
                idx,
                hotbar_blocks,
                hotbar_model_ids,
                hotbar_tint_indices,
            );
        }
    }

    fn palette_items_for_tab(
        tab: PaletteTab,
        registry: &ModelRegistry,
    ) -> Vec<(PaletteItem, String)> {
        // Regular blocks (excluding TintedGlass which has separate color entries)
        const BLOCK_PALETTE: [BlockType; 15] = [
            BlockType::Stone,
            BlockType::Dirt,
            BlockType::Grass,
            BlockType::Planks,
            BlockType::Leaves,
            BlockType::Sand,
            BlockType::Gravel,
            BlockType::Water,
            BlockType::Glass,
            BlockType::Log,
            BlockType::Brick,
            BlockType::Snow,
            BlockType::Cobblestone,
            BlockType::Iron,
            BlockType::Bedrock,
        ];

        // Tinted glass colors: (tint_index, name)
        const TINTED_GLASS_COLORS: [(u8, &str); 7] = [
            (0, "Red Glass"),
            (4, "Green Glass"),
            (8, "Blue Glass"),
            (6, "Cyan Glass"),
            (2, "Yellow Glass"),
            (1, "Orange Glass"),
            (9, "Purple Glass"),
        ];

        let mut items = Vec::new();

        if matches!(tab, PaletteTab::Blocks | PaletteTab::All) {
            for &block in BLOCK_PALETTE.iter() {
                items.push((
                    PaletteItem {
                        block,
                        model_id: 0,
                        tint_index: 0,
                    },
                    format!("{:?}", block),
                ));
            }
            // Add tinted glass colors
            for &(tint_index, name) in TINTED_GLASS_COLORS.iter() {
                items.push((
                    PaletteItem {
                        block: BlockType::TintedGlass,
                        model_id: 0,
                        tint_index,
                    },
                    name.to_string(),
                ));
            }
        }

        if matches!(tab, PaletteTab::Models | PaletteTab::All) {
            let mut push_if = |id: u8, label: &str| {
                if registry.get(id).is_some() {
                    items.push((
                        PaletteItem {
                            block: BlockType::Model,
                            model_id: id,
                            tint_index: 0,
                        },
                        label.to_string(),
                    ));
                }
            };

            // Curated list: single fence entry (auto-connects), representative gate, etc.
            push_if(1, "Torch");
            push_if(2, "Slab (Bottom)");
            push_if(3, "Slab (Top)");
            push_if(4, "Fence");
            push_if(20, "Gate");
            push_if(28, "Stairs");
            push_if(29, "Ladder");
            push_if(39, "Door");
            push_if(67, "Windowed Door");
            push_if(75, "Paneled Door");
            push_if(83, "Fancy Door");
            push_if(91, "Glass Door");
            push_if(47, "Trapdoor");
            push_if(51, "Window");

            // Add custom/user models from the registry
            for model in registry.iter_custom_models() {
                items.push((
                    PaletteItem {
                        block: BlockType::Model,
                        model_id: model.id,
                        tint_index: 0,
                    },
                    model.name.clone(),
                ));
            }
        }

        items
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_palette_item(
        ui: &mut egui::Ui,
        atlas_texture_id: egui::TextureId,
        sprite_icons: Option<&SpriteIcons>,
        item: PaletteItem,
        label: &str,
        hotbar_blocks: &mut [BlockType; 9],
        hotbar_model_ids: &mut [u8; 9],
        hotbar_tint_indices: &mut [u8; 9],
        hotbar_index: &mut usize,
        dragging_item: &mut Option<PaletteItem>,
    ) {
        const ATLAS_TILE_COUNT: f32 = 19.0;
        let block_idx = Self::atlas_tile_for(item.block, item.model_id);
        let uv_left = block_idx / ATLAS_TILE_COUNT;
        let uv_right = (block_idx + 1.0) / ATLAS_TILE_COUNT;
        let uv_rect = egui::Rect::from_min_max(egui::pos2(uv_left, 0.0), egui::pos2(uv_right, 1.0));
        let sprite_id = Self::sprite_for_item(item, sprite_icons);

        ui.vertical(|ui| {
            let button = if let Some(id) = sprite_id {
                egui::ImageButton::new((id, egui::vec2(48.0, 48.0)))
                    .uv(egui::Rect::from_min_max(
                        egui::pos2(0.0, 0.0),
                        egui::pos2(1.0, 1.0),
                    ))
                    .frame(true)
                    .sense(egui::Sense::click_and_drag())
            } else {
                egui::ImageButton::new((atlas_texture_id, egui::vec2(48.0, 48.0)))
                    .uv(uv_rect)
                    .frame(true)
                    .sense(egui::Sense::click_and_drag())
            };
            let resp = ui.add(button);
            let clicked = resp.clicked();
            let middle = resp.middle_clicked();
            let drag_started = resp.drag_started();

            if drag_started {
                *dragging_item = Some(item);
            }
            if clicked {
                Self::apply_item_to_slot(
                    item,
                    *hotbar_index,
                    hotbar_blocks,
                    hotbar_model_ids,
                    hotbar_tint_indices,
                );
            }
            if middle {
                Self::fill_or_replace_hotbar(
                    item,
                    hotbar_blocks,
                    hotbar_model_ids,
                    hotbar_tint_indices,
                    hotbar_index,
                );
            }

            let hover_rect = resp.rect;
            resp.on_hover_text("Drag to a hotbar slot, left-click to set current slot, middle-click to fill/replace hotbar");
            if item.model_id == 2 {
                ui.painter().text(
                    hover_rect.left_top() + egui::vec2(6.0, 6.0),
                    egui::Align2::LEFT_TOP,
                    "B",
                    egui::FontId::proportional(12.0),
                    egui::Color32::YELLOW,
                );
            } else if item.model_id == 3 {
                ui.painter().text(
                    hover_rect.left_top() + egui::vec2(6.0, 6.0),
                    egui::Align2::LEFT_TOP,
                    "T",
                    egui::FontId::proportional(12.0),
                    egui::Color32::YELLOW,
                );
            }
            // Draw colored border for TintedGlass items
            if item.block == BlockType::TintedGlass {
                let tint_color = Self::tint_color(item.tint_index);
                ui.painter().rect_stroke(
                    hover_rect.shrink(2.0),
                    egui::CornerRadius::same(3),
                    egui::Stroke::new(3.0, tint_color),
                    egui::StrokeKind::Inside,
                );
            }
            ui.add(
                egui::Label::new(
                    egui::RichText::new(label)
                        .color(egui::Color32::WHITE)
                        .small(),
                )
                .wrap(),
            );
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_palette_window(
        ctx: &egui::Context,
        atlas_texture_id: egui::TextureId,
        sprite_icons: Option<&SpriteIcons>,
        palette_open: &mut bool,
        palette_tab: &mut PaletteTab,
        dragging_item: &mut Option<PaletteItem>,
        model_registry: &ModelRegistry,
        hotbar_blocks: &mut [BlockType; 9],
        hotbar_model_ids: &mut [u8; 9],
        hotbar_tint_indices: &mut [u8; 9],
        hotbar_index: &mut usize,
    ) {
        if !*palette_open {
            return;
        }

        egui::Window::new("Block & Model Palette")
            .open(palette_open)
            .default_size(egui::vec2(520.0, 360.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(palette_tab, PaletteTab::All, "All");
                    ui.selectable_value(palette_tab, PaletteTab::Blocks, "Blocks");
                    ui.selectable_value(palette_tab, PaletteTab::Models, "Models");
                });
                ui.label("Drag items to the hotbar, left-click to set current slot, middle-click to fill (or replace if full).");
                ui.separator();

                let items = Self::palette_items_for_tab(*palette_tab, model_registry);
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        for row in items.chunks(12) {
                            ui.horizontal(|ui| {
                                for (item, label) in row.iter() {
                                    Self::draw_palette_item(
                                        ui,
                                        atlas_texture_id,
                                        sprite_icons,
                                        *item,
                                        label,
                                        hotbar_blocks,
                                        hotbar_model_ids,
                                        hotbar_tint_indices,
                                        hotbar_index,
                                        dragging_item,
                                    );
                                    ui.add_space(6.0);
                                }
                            });
                            ui.add_space(6.0);
                        }
                    });
            });
    }

    fn overlay_frame() -> egui::Frame {
        egui::Frame::new()
            .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180))
            .corner_radius(egui::CornerRadius::same(4))
            .inner_margin(egui::Margin::symmetric(8, 4))
    }

    fn draw_stats_overlay(ctx: &egui::Context, fps: u32, chunk_stats: &ChunkStats) {
        egui::Area::new(egui::Id::new("fps_overlay"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 10.0))
            .show(ctx, |ui| {
                Self::overlay_frame().show(ui, |ui| {
                    ui.set_min_width(100.0);
                    ui.label(
                        egui::RichText::new(format!("FPS: {}", fps))
                            .color(egui::Color32::WHITE)
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new(format!("Chunks: {}", chunk_stats.loaded_count))
                            .color(egui::Color32::LIGHT_GRAY)
                            .small(),
                    );
                    if chunk_stats.dirty_count > 0 {
                        ui.label(
                            egui::RichText::new(format!("Dirty: {}", chunk_stats.dirty_count))
                                .color(egui::Color32::YELLOW)
                                .small(),
                        );
                    }
                    if chunk_stats.in_flight_count > 0 {
                        ui.label(
                            egui::RichText::new(format!(
                                "Generating: {}",
                                chunk_stats.in_flight_count
                            ))
                            .color(egui::Color32::LIGHT_GREEN)
                            .small(),
                        );
                    }
                    ui.label(
                        egui::RichText::new(format!("GPU: {:.1} MB", chunk_stats.memory_mb))
                            .color(egui::Color32::LIGHT_GRAY)
                            .small(),
                    );
                });
            });
    }

    fn draw_position_overlay(ctx: &egui::Context, player_world_pos: Vector3<f64>) {
        egui::Area::new(egui::Id::new("position_overlay"))
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 10.0))
            .show(ctx, |ui| {
                Self::overlay_frame()
                    .inner_margin(egui::Margin::symmetric(12, 6))
                    .show(ui, |ui| {
                        let pos_text = format!(
                            "Pos: {:.1}, {:.1}, {:.1}",
                            player_world_pos.x, player_world_pos.y, player_world_pos.z
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(pos_text)
                                    .color(egui::Color32::WHITE)
                                    .strong()
                                    .monospace(),
                            )
                            .wrap_mode(egui::TextWrapMode::Extend),
                        );
                    });
            });
    }

    pub fn render(&self, gui: &mut Gui, input: HudInputs<'_>) -> (bool, EditorAction) {
        let HudInputs {
            fps,
            chunk_stats,
            player,
            world,
            settings,
            render_mode,
            current_hit,
            selected_block,
            hotbar_index,
            hotbar_blocks,
            hotbar_model_ids,
            hotbar_tint_indices,
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
                Self::draw_stats_overlay(&ctx, fps, chunk_stats);
            }
            if settings.show_position {
                Self::draw_position_overlay(&ctx, player_world_pos);
            }
            Self::draw_palette_window(
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
                hotbar_index,
            );

            // Drag preview near cursor
            if let Some(item) = dragging_item.as_ref() {
                if let Some(pointer_pos) = ctx.input(|i| i.pointer.latest_pos()) {
                    let (texture_id, uv_rect) = if let Some(tex) =
                        Self::sprite_for_item(*item, sprite_icons)
                    {
                        (
                            tex,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        )
                    } else {
                        const ATLAS_TILE_COUNT: f32 = 19.0;
                        let block_idx = Self::atlas_tile_for(item.block, item.model_id);
                        let uv_left = block_idx / ATLAS_TILE_COUNT;
                        let uv_right = (block_idx + 1.0) / ATLAS_TILE_COUNT;
                        (
                            atlas_texture_id,
                            egui::Rect::from_min_max(
                                egui::pos2(uv_left, 0.0),
                                egui::pos2(uv_right, 1.0),
                            ),
                        )
                    };

                    let size = egui::vec2(48.0, 48.0);
                    let rect = egui::Rect::from_min_size(pointer_pos - size * 0.5, size);
                    let painter = ctx.layer_painter(egui::LayerId::new(
                        egui::Order::Tooltip,
                        egui::Id::new("drag_preview"),
                    ));
                    painter.image(texture_id, rect, uv_rect, egui::Color32::WHITE);
                    let label = if item.model_id == 2 {
                        "B"
                    } else if item.model_id == 3 {
                        "T"
                    } else {
                        ""
                    };
                    if !label.is_empty() {
                        painter.text(
                            rect.left_top() + egui::vec2(4.0, 4.0),
                            egui::Align2::LEFT_TOP,
                            label,
                            egui::FontId::proportional(12.0),
                            egui::Color32::YELLOW,
                        );
                    }
                }
            }

            egui::Window::new("Settings")
                .default_open(false)
                .default_pos(egui::pos2(10.0, 40.0))
                .show(&ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(500.0)
                        .show(ui, |ui| {
                            ui.collapsing("Controls", |ui| {
                                ui.label("  WASD - Move");
                                ui.label("  Space - Jump");
                                ui.label("  Space/Shift - Up/Down (fly, swim & climb)");
                                ui.label("  Mouse - Look around");
                                ui.label("  Scroll - Select block");
                                ui.label("  Ctrl - Toggle sprint");
                                ui.label("  F - Toggle fly mode");
                                ui.label("  B - Toggle chunk boundaries");
                                ui.label("  Left Click - Break block");
                                ui.label("  Right Click - Place block");
                                ui.label("  1-9 - Select block type (8=Ladder, 9=Torch)");
                                ui.label("  / - Open command console");
                                ui.label("  Escape - Release cursor");
                            });
                            ui.separator();

                            ui.label(format!("Chunks: {}", world.chunk_count()));
                            if player.in_water {
                                ui.colored_label(
                                    egui::Color32::from_rgb(100, 150, 255),
                                    "🌊 UNDERWATER",
                                );
                            }

                            ui.separator();

                            // Block selection
                            ui.label(format!("Selected: {:?}", selected_block));
                            if let Some(hit) = current_hit {
                                let block_type = world.get_block(hit.block_pos);
                                let block_name = block_type
                                    .map(|b| format!("{:?}", b))
                                    .unwrap_or_else(|| "Unknown".to_string());
                                ui.label(format!(
                                    "Looking at: {} ({}, {}, {})",
                                    block_name, hit.block_pos.x, hit.block_pos.y, hit.block_pos.z
                                ));
                                ui.label(format!("Distance: {:.1}", hit.distance));
                            } else {
                                ui.label("Looking at: (nothing)");
                            }

                            ui.separator();

                            // Debug render mode
                            ui.label("Render Mode:");
                            ui.horizontal(|ui| {
                                for &mode in RenderMode::ALL {
                                    ui.selectable_value(render_mode, mode, format!("{:?}", mode));
                                }
                            });

                            ui.separator();

                            ui.add(
                                egui::Slider::new(&mut player.camera.fov, 20.0..=120.0).text("FOV"),
                            );

                            if ui
                                .add(
                                    egui::Slider::new(&mut settings.render_scale, 0.25..=1.5)
                                        .text("Render Scale"),
                                )
                                .changed()
                            {
                                scale_changed = true;
                            }

                            ui.separator();

                            // Day/night cycle controls
                            ui.label("Day/Night Cycle:");
                            ui.checkbox(day_cycle_paused, "Pause cycle");
                            let hours = (*time_of_day * 24.0) % 24.0;
                            let time_label = if hours < 6.0 {
                                "Night"
                            } else if hours < 9.0 {
                                "Sunrise"
                            } else if hours < 17.0 {
                                "Day"
                            } else if hours < 20.0 {
                                "Sunset"
                            } else {
                                "Night"
                            };
                            ui.add(
                                egui::Slider::new(time_of_day, 0.0..=1.0)
                                    .text(time_label)
                                    .custom_formatter(|v, _| format_time_of_day(v))
                                    .custom_parser(parse_time_of_day),
                            );
                            ui.add(
                                egui::Slider::new(&mut atmosphere.ambient_light, 0.0..=1.0)
                                    .text("Ambient Light"),
                            );
                            ui.add(
                                egui::Slider::new(&mut atmosphere.cloud_speed, 0.0..=3.0)
                                    .text("Cloud Speed")
                                    .suffix("x"),
                            );
                            ui.add(
                                egui::Slider::new(&mut atmosphere.fog_density, 0.0..=0.1)
                                    .text("Fog Density"),
                            );
                            ui.add(
                                egui::Slider::new(&mut atmosphere.fog_start, 0.0..=128.0)
                                    .text("Fog Start"),
                            );
                            ui.add(
                                egui::Slider::new(&mut atmosphere.fog_overlay_scale, 0.0..=2.0)
                                    .text("Fog Overlay Scale"),
                            );
                            if ui
                                .add(
                                    egui::Slider::new(&mut settings.max_ray_steps, 128..=1024)
                                        .text("Ray Steps"),
                                )
                                .changed()
                            {
                                println!("[SETTING] Ray Steps: {}", settings.max_ray_steps);
                            }
                            if ui
                                .add(
                                    egui::Slider::new(&mut settings.shadow_max_steps, 64..=256)
                                        .text("Shadow Steps"),
                                )
                                .changed()
                            {
                                println!("[SETTING] Shadow Steps: {}", settings.shadow_max_steps);
                            }
                            if ui
                                .add(egui::Slider::new(view_distance, 2..=10).text("View Distance"))
                                .changed()
                            {
                                println!("[SETTING] View Distance: {} chunks", *view_distance);
                                // Ensure unload distance is at least view distance + 1
                                if *unload_distance <= *view_distance {
                                    *unload_distance = *view_distance + 2;
                                }
                            }
                            if ui
                                .add(
                                    egui::Slider::new(unload_distance, 3..=12)
                                        .text("Unload Distance"),
                                )
                                .changed()
                            {
                                println!("[SETTING] Unload Distance: {} chunks", *unload_distance);
                                // Ensure unload distance is greater than view distance
                                if *unload_distance <= *view_distance {
                                    *unload_distance = *view_distance + 2;
                                }
                            }

                            ui.separator();
                            ui.label("Feature Toggles:");
                            if ui
                                .checkbox(&mut settings.enable_ao, "Ambient Occlusion")
                                .changed()
                            {
                                println!(
                                    "[TOGGLE] Ambient Occlusion: {}",
                                    if settings.enable_ao { "ON" } else { "OFF" }
                                );
                            }
                            if ui
                                .checkbox(&mut settings.enable_shadows, "Sun Shadows")
                                .changed()
                            {
                                println!(
                                    "[TOGGLE] Sun Shadows: {}",
                                    if settings.enable_shadows { "ON" } else { "OFF" }
                                );
                            }
                            if ui
                                .checkbox(&mut settings.enable_model_shadows, "Model Sun Shadows")
                                .changed()
                            {
                                println!(
                                    "[TOGGLE] Model Sun Shadows: {}",
                                    if settings.enable_model_shadows {
                                        "ON"
                                    } else {
                                        "OFF"
                                    }
                                );
                            }
                            if ui
                                .checkbox(
                                    &mut settings.enable_point_lights,
                                    "Point Lights (torches)",
                                )
                                .changed()
                            {
                                println!(
                                    "[TOGGLE] Point Lights: {}",
                                    if settings.enable_point_lights {
                                        "ON"
                                    } else {
                                        "OFF"
                                    }
                                );
                            }
                            if ui
                                .checkbox(
                                    &mut settings.enable_tinted_shadows,
                                    "Tinted Glass Shadows",
                                )
                                .changed()
                            {
                                println!(
                                    "[TOGGLE] Tinted Glass Shadows: {}",
                                    if settings.enable_tinted_shadows {
                                        "ON"
                                    } else {
                                        "OFF"
                                    }
                                );
                            }
                            if ui
                                .checkbox(
                                    &mut settings.water_simulation_enabled,
                                    "Water Flow Simulation",
                                )
                                .changed()
                            {
                                println!(
                                    "[TOGGLE] Water Flow Simulation: {}",
                                    if settings.water_simulation_enabled {
                                        "ON"
                                    } else {
                                        "OFF"
                                    }
                                );
                            }

                            ui.separator();
                            ui.label("LOD Distances (lower = faster):");
                            ui.horizontal(|ui| {
                                ui.label("AO:");
                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut settings.lod_ao_distance,
                                            8.0..=64.0,
                                        )
                                        .suffix(" blocks"),
                                    )
                                    .changed()
                                {
                                    println!("[LOD] AO distance: {:.0}", settings.lod_ao_distance);
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("Shadows:");
                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut settings.lod_shadow_distance,
                                            16.0..=128.0,
                                        )
                                        .suffix(" blocks"),
                                    )
                                    .changed()
                                {
                                    println!(
                                        "[LOD] Shadow distance: {:.0}",
                                        settings.lod_shadow_distance
                                    );
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("Lights:");
                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut settings.lod_point_light_distance,
                                            8.0..=48.0,
                                        )
                                        .suffix(" blocks"),
                                    )
                                    .changed()
                                {
                                    println!(
                                        "[LOD] Point light distance: {:.0}",
                                        settings.lod_point_light_distance
                                    );
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("Models:");
                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut settings.lod_model_distance,
                                            8.0..=64.0,
                                        )
                                        .suffix(" blocks"),
                                    )
                                    .changed()
                                {
                                    println!(
                                        "[LOD] Model detail distance: {:.0}",
                                        settings.lod_model_distance
                                    );
                                }
                            });

                            ui.separator();

                            // Gameplay options
                            ui.checkbox(&mut player.auto_jump, "Auto-jump");
                            ui.checkbox(&mut settings.instant_break, "Instant block break");
                            ui.checkbox(&mut settings.instant_place, "Instant block place");
                            ui.checkbox(
                                &mut settings.show_block_preview,
                                "Block placement preview",
                            );
                            ui.checkbox(&mut settings.show_target_outline, "Target block outline");
                            if ui
                                .checkbox(&mut player.light_enabled, "Player torch light")
                                .changed()
                            {
                                println!(
                                    "[TOGGLE] Player Light: {}",
                                    if player.light_enabled { "ON" } else { "OFF" }
                                );
                            }

                            ui.add(
                                egui::Slider::new(
                                    &mut settings.break_cooldown_duration,
                                    0.05..=0.5,
                                )
                                .text("Break cooldown")
                                .suffix("s"),
                            );
                            ui.add(
                                egui::Slider::new(
                                    &mut settings.place_cooldown_duration,
                                    0.05..=1.0,
                                )
                                .text("Place cooldown")
                                .suffix("s"),
                            );

                            // Block physics updates per frame (higher = faster cascades, more CPU)
                            let mut max_updates = block_updates.max_per_frame as u32;
                            if ui
                                .add(
                                    egui::Slider::new(&mut max_updates, 16..=128)
                                        .text("Physics updates/frame")
                                        .logarithmic(true),
                                )
                                .changed()
                            {
                                block_updates.max_per_frame = max_updates as usize;
                            }

                            ui.separator();

                            // HUD visibility
                            ui.checkbox(&mut settings.show_compass, "Show compass");
                            ui.checkbox(&mut settings.show_position, "Show position");
                            ui.checkbox(&mut settings.show_stats, "Show FPS/stats");

                            ui.separator();

                            // Minimap settings
                            ui.label("Minimap");
                            if ui.checkbox(show_minimap, "Show minimap (M)").changed() {
                                println!("Minimap: {}", if *show_minimap { "ON" } else { "OFF" });
                            }

                            ui.horizontal(|ui| {
                                ui.label("Size:");
                                if ui.selectable_label(minimap.size == 128, "Small").clicked() {
                                    minimap.size = 128;
                                    *minimap_cached_image = None; // Force refresh
                                }
                                if ui.selectable_label(minimap.size == 192, "Medium").clicked() {
                                    minimap.size = 192;
                                    *minimap_cached_image = None; // Force refresh
                                }
                                if ui.selectable_label(minimap.size == 256, "Large").clicked() {
                                    minimap.size = 256;
                                    *minimap_cached_image = None; // Force refresh
                                }
                            });

                            ui.horizontal(|ui| {
                                ui.label("Colors:");
                                if ui
                                    .selectable_label(minimap.color_mode == 0, "Blocks")
                                    .clicked()
                                {
                                    minimap.color_mode = 0;
                                    *minimap_cached_image = None; // Force refresh
                                }
                                if ui
                                    .selectable_label(minimap.color_mode == 1, "Height")
                                    .clicked()
                                {
                                    minimap.color_mode = 1;
                                    *minimap_cached_image = None; // Force refresh
                                }
                                if ui
                                    .selectable_label(minimap.color_mode == 2, "Both")
                                    .clicked()
                                {
                                    minimap.color_mode = 2;
                                    *minimap_cached_image = None; // Force refresh
                                }
                            });

                            if ui
                                .add(
                                    egui::Slider::new(&mut minimap.zoom, 0.5..=3.0)
                                        .text("Zoom")
                                        .logarithmic(true),
                                )
                                .changed()
                            {
                                *minimap_cached_image = None; // Force refresh
                            }

                            if ui
                                .checkbox(&mut minimap.rotate, "Rotate with player")
                                .changed()
                            {
                                // Force minimap refresh when rotation mode changes
                                *minimap_cached_image = None;
                            }

                            ui.separator();

                            // Camera position debug
                            ui.label(format!(
                                "Position: ({:.1}, {:.1}, {:.1})",
                                player.camera.position.x,
                                player.camera.position.y,
                                player.camera.position.z
                            ));

                            ui.separator();

                            // Window size
                            let screen = ui.ctx().screen_rect();
                            ui.label(format!(
                                "Window: {}x{}",
                                screen.width() as u32,
                                screen.height() as u32
                            ));
                        }); // end ScrollArea
                });

            // Draw crosshair at screen center (hide when editor is open)
            // Changes appearance when targeting a block
            if !editor.active {
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

            // Minimap HUD (bottom-right)
            if *show_minimap {
                if let Some(image) = minimap_image {
                    // Load the pre-generated image as texture
                    let texture = ctx.load_texture("minimap", image, egui::TextureOptions::NEAREST);

                    egui::Area::new(egui::Id::new("minimap_hud"))
                        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -12.0))
                        .show(&ctx, |ui| {
                            egui::Frame::new()
                                .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 200))
                                .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(60, 60, 60)))
                                .corner_radius(egui::CornerRadius::same(4))
                                .inner_margin(egui::Margin::same(4))
                                .show(ui, |ui| {
                                    let size = minimap.size as f32;
                                    let image_response = ui.add(
                                        egui::Image::new(egui::load::SizedTexture::new(
                                            texture.id(),
                                            egui::vec2(size, size),
                                        ))
                                        .fit_to_exact_size(egui::vec2(size, size)),
                                    );

                                    // Draw player indicator (triangle pointing in direction)
                                    let center = image_response.rect.center();
                                    let tri_size = 6.0;

                                    // Calculate triangle rotation angle
                                    let angle = if minimap.rotate {
                                        0.0 // Always point up when map rotates
                                    } else {
                                        -camera_yaw // Point in player's direction
                                    };

                                    // Triangle vertices: tip at front, two corners at back
                                    let (sin_a, cos_a) = (angle.sin(), angle.cos());
                                    let tip = egui::pos2(
                                        center.x - sin_a * tri_size,
                                        center.y - cos_a * tri_size,
                                    );
                                    let left = egui::pos2(
                                        center.x + cos_a * tri_size * 0.6 + sin_a * tri_size * 0.5,
                                        center.y - sin_a * tri_size * 0.6 + cos_a * tri_size * 0.5,
                                    );
                                    let right = egui::pos2(
                                        center.x - cos_a * tri_size * 0.6 + sin_a * tri_size * 0.5,
                                        center.y + sin_a * tri_size * 0.6 + cos_a * tri_size * 0.5,
                                    );

                                    ui.painter().add(egui::Shape::convex_polygon(
                                        vec![tip, left, right],
                                        egui::Color32::RED,
                                        egui::Stroke::new(1.0, egui::Color32::WHITE),
                                    ));
                                });
                        });
                }
            }

            // Compass HUD (bottom-left)
            if settings.show_compass {
                egui::Area::new(egui::Id::new("compass_hud"))
                    .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(12.0, -12.0))
                    .show(&ctx, |ui| {
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 200))
                            .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(60, 60, 60)))
                            .corner_radius(egui::CornerRadius::same(4))
                            .inner_margin(egui::Margin::same(8))
                            .show(ui, |ui| {
                                let compass_size = 60.0;
                                let (response, painter) = ui.allocate_painter(
                                    egui::vec2(compass_size, compass_size),
                                    egui::Sense::hover(),
                                );
                                let center = response.rect.center();
                                let radius = compass_size / 2.0 - 4.0;

                                // Draw compass circle
                                painter.circle_stroke(
                                    center,
                                    radius,
                                    egui::Stroke::new(1.5, egui::Color32::from_rgb(100, 100, 100)),
                                );

                                // Cardinal direction positions (N=-Z, S=+Z, E=+X, W=-X)
                                // In our coordinate system: yaw=0 looks at -Z (North)
                                let directions = [
                                    ("N", 0.0_f32, egui::Color32::RED), // North at yaw=0
                                    ("E", std::f32::consts::FRAC_PI_2, egui::Color32::WHITE), // East at yaw=90°
                                    ("S", std::f32::consts::PI, egui::Color32::WHITE), // South at yaw=180°
                                    ("W", -std::f32::consts::FRAC_PI_2, egui::Color32::WHITE), // West at yaw=-90°
                                ];

                                for (label, dir_angle, color) in directions {
                                    // Calculate angle relative to player's view
                                    // Player yaw: 0 = looking North (-Z)
                                    let relative_angle = dir_angle - camera_yaw;
                                    let (sin_a, cos_a) = relative_angle.sin_cos();

                                    // Position on compass (up = forward direction in player's view)
                                    let label_pos = egui::pos2(
                                        center.x + sin_a * (radius - 8.0),
                                        center.y - cos_a * (radius - 8.0),
                                    );

                                    painter.text(
                                        label_pos,
                                        egui::Align2::CENTER_CENTER,
                                        label,
                                        egui::FontId::proportional(12.0),
                                        color,
                                    );
                                }

                                // Draw direction indicator (line pointing up = forward)
                                painter.line_segment(
                                    [
                                        egui::pos2(center.x, center.y),
                                        egui::pos2(center.x, center.y - radius + 12.0),
                                    ],
                                    egui::Stroke::new(2.0, egui::Color32::YELLOW),
                                );
                                // Arrow head
                                painter.line_segment(
                                    [
                                        egui::pos2(center.x - 4.0, center.y - radius + 18.0),
                                        egui::pos2(center.x, center.y - radius + 12.0),
                                    ],
                                    egui::Stroke::new(2.0, egui::Color32::YELLOW),
                                );
                                painter.line_segment(
                                    [
                                        egui::pos2(center.x + 4.0, center.y - radius + 18.0),
                                        egui::pos2(center.x, center.y - radius + 12.0),
                                    ],
                                    egui::Stroke::new(2.0, egui::Color32::YELLOW),
                                );
                            });
                    });
            }

            // Hotbar HUD at bottom center - 9 slots
            const ATLAS_TILE_COUNT: f32 = 19.0;
            const SLOT_SIZE: f32 = 40.0;
            let pointer_released =
                ctx.input(|i| i.pointer.button_released(egui::PointerButton::Primary));

            egui::Area::new(egui::Id::new("hotbar_hud"))
                .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -10.0))
                .show(&ctx, |ui| {
                    // Background frame for the whole hotbar
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180))
                        .corner_radius(egui::CornerRadius::same(4))
                        .inner_margin(egui::Margin::same(6))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);

                                let len = hotbar_blocks.len();
                                for i in 0..len {
                                    let block = hotbar_blocks[i];
                                    let is_selected = i == *hotbar_index;

                                    // Texture source: prefer generated sprite, fallback to atlas UV.
                                    let palette_item = PaletteItem {
                                        block,
                                        model_id: hotbar_model_ids[i],
                                        tint_index: hotbar_tint_indices[i],
                                    };
                                    let (texture_id, uv_rect) = if let Some(tex) =
                                        Self::sprite_for_item(palette_item, sprite_icons)
                                    {
                                        (
                                            tex,
                                            egui::Rect::from_min_max(
                                                egui::pos2(0.0, 0.0),
                                                egui::pos2(1.0, 1.0),
                                            ),
                                        )
                                    } else {
                                        let block_idx =
                                            Self::atlas_tile_for(block, hotbar_model_ids[i]);
                                        let uv_left = block_idx / ATLAS_TILE_COUNT;
                                        let uv_right = (block_idx + 1.0) / ATLAS_TILE_COUNT;
                                        (
                                            atlas_texture_id,
                                            egui::Rect::from_min_max(
                                                egui::pos2(uv_left, 0.0),
                                                egui::pos2(uv_right, 1.0),
                                            ),
                                        )
                                    };

                                    // Slot border color
                                    let border_color = if is_selected {
                                        egui::Color32::from_rgb(100, 255, 100)
                                    } else {
                                        egui::Color32::from_rgb(60, 60, 60)
                                    };
                                    let border_width = if is_selected { 3.0 } else { 1.0 };

                                    // Allocate space for slot
                                    let (rect, response) = ui.allocate_exact_size(
                                        egui::vec2(SLOT_SIZE + 4.0, SLOT_SIZE + 16.0),
                                        egui::Sense::click_and_drag(),
                                    );

                                    // Draw slot background
                                    ui.painter().rect_filled(
                                        rect,
                                        egui::CornerRadius::same(2),
                                        egui::Color32::from_rgb(40, 40, 40),
                                    );

                                    // Draw texture
                                    let texture_rect = egui::Rect::from_min_size(
                                        rect.min + egui::vec2(2.0, 2.0),
                                        egui::vec2(SLOT_SIZE, SLOT_SIZE),
                                    );
                                    ui.painter().image(
                                        texture_id,
                                        texture_rect,
                                        uv_rect,
                                        egui::Color32::WHITE,
                                    );
                                    if hotbar_model_ids[i] == 2 {
                                        ui.painter().text(
                                            texture_rect.left_top() + egui::vec2(4.0, 4.0),
                                            egui::Align2::LEFT_TOP,
                                            "B",
                                            egui::FontId::proportional(11.0),
                                            egui::Color32::YELLOW,
                                        );
                                    } else if hotbar_model_ids[i] == 3 {
                                        ui.painter().text(
                                            texture_rect.left_top() + egui::vec2(4.0, 4.0),
                                            egui::Align2::LEFT_TOP,
                                            "T",
                                            egui::FontId::proportional(11.0),
                                            egui::Color32::YELLOW,
                                        );
                                    }
                                    // Draw colored inner border for TintedGlass
                                    if block == BlockType::TintedGlass {
                                        let tint_color = Self::tint_color(hotbar_tint_indices[i]);
                                        ui.painter().rect_stroke(
                                            texture_rect.shrink(1.0),
                                            egui::CornerRadius::same(2),
                                            egui::Stroke::new(2.0, tint_color),
                                            egui::StrokeKind::Inside,
                                        );
                                    }

                                    // Draw border
                                    ui.painter().rect_stroke(
                                        rect,
                                        egui::CornerRadius::same(2),
                                        egui::Stroke::new(border_width, border_color),
                                        egui::StrokeKind::Outside,
                                    );

                                    // Draw number label
                                    let text_pos = egui::pos2(rect.center().x, rect.max.y - 8.0);
                                    ui.painter().text(
                                        text_pos,
                                        egui::Align2::CENTER_CENTER,
                                        format!("{}", i + 1),
                                        egui::FontId::proportional(10.0),
                                        egui::Color32::WHITE,
                                    );

                                    // Interactions: click selects slot, drop assigns dragged item.
                                    if response.clicked() {
                                        *hotbar_index = i;
                                    }
                                    if pointer_released && response.hovered() {
                                        if let Some(item) = dragging_item.take() {
                                            Self::apply_item_to_slot(
                                                item,
                                                i,
                                                hotbar_blocks,
                                                hotbar_model_ids,
                                                hotbar_tint_indices,
                                            );
                                            *hotbar_index = i;
                                        }
                                    }
                                }
                            });

                            if pointer_released && dragging_item.is_some() {
                                // Drop cancelled (released outside hotbar).
                                *dragging_item = None;
                            }

                            // Selected block name below hotbar
                            ui.vertical_centered(|ui| {
                                ui.add_space(4.0);
                                // For Model blocks, show the model type name
                                let block_name = if selected_block == BlockType::Model {
                                    match hotbar_model_ids[*hotbar_index] {
                                        1 => "Torch".to_string(),
                                        2 | 3 => "Slab".to_string(),
                                        4..=19 => "Fence".to_string(),
                                        20..=27 => "Gate".to_string(),
                                        29 => "Ladder".to_string(),
                                        28 | 30..=38 => "Stairs".to_string(),
                                        _ => "Model".to_string(),
                                    }
                                } else {
                                    format!("{:?}", selected_block)
                                };
                                ui.label(
                                    egui::RichText::new(block_name)
                                        .color(egui::Color32::WHITE)
                                        .strong(),
                                );
                            });
                        });
                });

            // Draw editor UI if active
            if editor.active {
                let library = LibraryManager::new("user_models");
                let _ = library.init();
                editor_action = draw_editor_ui(&ctx, editor, &library, "Player");
                draw_model_preview(&ctx, editor);
            }

            // Draw console if active
            if console.active {
                Self::draw_console(&ctx, console, world, player_world_pos);
            }
        });
        (scale_changed, editor_action)
    }

    /// Draw the command console UI.
    fn draw_console(
        ctx: &egui::Context,
        console: &mut ConsoleState,
        world: &mut crate::world::World,
        player_world_pos: Vector3<f64>,
    ) {
        let screen_rect = ctx.screen_rect();
        let console_height = screen_rect.height() * 0.6;
        let console_width = screen_rect.width().min(800.0);

        // Position at bottom center of screen
        let console_pos = egui::pos2(
            (screen_rect.width() - console_width) / 2.0,
            screen_rect.height() - console_height - 10.0,
        );

        egui::Window::new("Console")
            .fixed_pos(console_pos)
            .fixed_size([console_width, console_height])
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(egui::Color32::from_rgba_unmultiplied(20, 20, 20, 230)),
            )
            .show(ctx, |ui| {
                // Output history with scroll - use full width
                let output_height = console_height - 40.0;
                let available_width = ui.available_width();
                egui::ScrollArea::vertical()
                    .max_height(output_height)
                    .stick_to_bottom(true)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_min_width(available_width);
                        for entry in &console.output {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(entry.text())
                                        .color(entry.color())
                                        .monospace(),
                                )
                                .wrap_mode(egui::TextWrapMode::Wrap),
                            );
                        }
                    });

                ui.separator();

                // Input field
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(">")
                            .monospace()
                            .color(egui::Color32::from_rgb(100, 255, 100)),
                    );

                    let response = ui.add(
                        egui::TextEdit::singleline(&mut console.input)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(console_width - 30.0)
                            .frame(false)
                            .hint_text("Type a command... (help for list)"),
                    );

                    // Request focus if needed
                    if console.request_focus {
                        response.request_focus();
                        console.request_focus = false;
                    }

                    // Handle keyboard input
                    if response.lost_focus() {
                        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            // Submit command
                            let player_pos = Vector3::new(
                                player_world_pos.x.floor() as i32,
                                player_world_pos.y.floor() as i32,
                                player_world_pos.z.floor() as i32,
                            );
                            console.submit(world, player_pos);
                            // Re-focus the input
                            console.request_focus = true;
                        } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            // Close console
                            console.close();
                        }
                    }

                    // History navigation (check while focused)
                    if response.has_focus() {
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                            console.history_up();
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                            console.history_down();
                        }
                    }
                });
            });
    }
}

/// Parses a time string in "HH:MM" or "HH" format to a 0.0-1.0 day fraction.
///
/// Examples:
/// - "00:00" -> 0.0 (midnight)
/// - "12:00" -> 0.5 (noon)
/// - "14:30" -> ~0.604 (2:30 PM)
/// - "24" or "24:00" -> 1.0 (end of day, clamped)
pub fn parse_time_of_day(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Some((h, m)) = s.split_once(':') {
        let hours: f64 = h.trim().parse().ok()?;
        let minutes: f64 = m.trim().parse().unwrap_or(0.0);
        let total_hours = hours + minutes / 60.0;
        Some((total_hours / 24.0).clamp(0.0, 1.0))
    } else {
        // Just hours, no colon
        let hours: f64 = s.parse().ok()?;
        Some((hours / 24.0).clamp(0.0, 1.0))
    }
}

/// Formats a 0.0-1.0 day fraction as "HH:MM" string.
pub fn format_time_of_day(v: f64) -> String {
    let hours = (v * 24.0) % 24.0;
    let h = hours as u32;
    let m = ((hours - h as f64) * 60.0) as u32;
    format!("{:02}:{:02}", h, m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_time_midnight() {
        assert_eq!(parse_time_of_day("00:00"), Some(0.0));
        assert_eq!(parse_time_of_day("0:00"), Some(0.0));
        assert_eq!(parse_time_of_day("0"), Some(0.0));
    }

    #[test]
    fn test_parse_time_noon() {
        assert_eq!(parse_time_of_day("12:00"), Some(0.5));
        assert_eq!(parse_time_of_day("12"), Some(0.5));
    }

    #[test]
    fn test_parse_time_afternoon() {
        let result = parse_time_of_day("14:00").unwrap();
        assert!((result - 14.0 / 24.0).abs() < 0.0001);

        let result = parse_time_of_day("14:30").unwrap();
        assert!((result - 14.5 / 24.0).abs() < 0.0001);
    }

    #[test]
    fn test_parse_time_with_whitespace() {
        assert_eq!(parse_time_of_day("  12:00  "), Some(0.5));
        assert_eq!(parse_time_of_day(" 12 : 00 "), Some(0.5));
    }

    #[test]
    fn test_parse_time_minutes_zero() {
        // This was the reported bug - "00" minutes not working
        let result = parse_time_of_day("14:00").unwrap();
        assert!((result - 14.0 / 24.0).abs() < 0.0001);

        let result = parse_time_of_day("6:00").unwrap();
        assert!((result - 6.0 / 24.0).abs() < 0.0001);
    }

    #[test]
    fn test_parse_time_clamped() {
        // Values beyond 24 should be clamped to 1.0
        assert_eq!(parse_time_of_day("24:00"), Some(1.0));
        assert_eq!(parse_time_of_day("30"), Some(1.0));
    }

    #[test]
    fn test_parse_time_invalid() {
        assert_eq!(parse_time_of_day(""), None);
        assert_eq!(parse_time_of_day("abc"), None);
        assert_eq!(parse_time_of_day("12:abc"), Some(0.5)); // Invalid minutes default to 0
    }

    #[test]
    fn test_format_time() {
        assert_eq!(format_time_of_day(0.0), "00:00");
        assert_eq!(format_time_of_day(0.5), "12:00");
        assert_eq!(format_time_of_day(14.0 / 24.0), "14:00");
        assert_eq!(format_time_of_day(14.5 / 24.0), "14:30");
    }

    #[test]
    fn test_roundtrip() {
        // Parse then format should give same result
        for time_str in ["00:00", "06:00", "12:00", "14:30", "18:45", "23:59"] {
            let parsed = parse_time_of_day(time_str).unwrap();
            let formatted = format_time_of_day(parsed);
            assert_eq!(formatted, time_str, "Roundtrip failed for {}", time_str);
        }
    }
}

//! Block and model palette UI.

use super::helpers::HudHelpers;
use crate::app_state::{PaletteItem, PaletteTab};
use crate::chunk::{BlockType, WaterType};
use crate::gpu_resources::SpriteIcons;
use crate::sub_voxel::ModelRegistry;
use egui_winit_vulkano::egui;

pub struct PaletteUI;

impl PaletteUI {
    fn palette_items_for_tab(
        tab: PaletteTab,
        registry: &ModelRegistry,
    ) -> Vec<(PaletteItem, String)> {
        // Regular blocks (excluding TintedGlass and Crystal which have separate color entries)
        const BLOCK_PALETTE: [BlockType; 40] = [
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
            BlockType::Ice,
            BlockType::Cobblestone,
            BlockType::Iron,
            BlockType::Bedrock,
            // Emissive blocks
            BlockType::Lava,
            BlockType::GlowStone,
            BlockType::GlowMushroom,
            // Tree variants
            BlockType::PineLog,
            BlockType::WillowLog,
            BlockType::PineLeaves,
            BlockType::WillowLeaves,
            // Terrain blocks
            BlockType::Mud,
            BlockType::Sandstone,
            BlockType::Cactus,
            BlockType::DecorativeStone,
            BlockType::Concrete,
            // Cave/biome blocks
            BlockType::Deepslate,
            BlockType::Moss,
            BlockType::MossyCobblestone,
            BlockType::Clay,
            BlockType::Dripstone,
            BlockType::Calcite,
            BlockType::Terracotta,
            BlockType::PackedIce,
            BlockType::Podzol,
            BlockType::Mycelium,
            BlockType::CoarseDirt,
            BlockType::RootedDirt,
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
                // Skip base Water block as we'll add typed versions
                if block == BlockType::Water {
                    continue;
                }
                items.push((
                    PaletteItem {
                        block,
                        model_id: 0,
                        tint_index: 0,
                        paint_texture_idx: 0,
                        water_type: WaterType::Ocean,
                    },
                    format!("{:?}", block),
                ));
            }

            // Add water variants
            const WATER_VARIANTS: [(WaterType, &str); 5] = [
                (WaterType::Ocean, "Ocean Water"),
                (WaterType::Lake, "Lake Water"),
                (WaterType::River, "River Water"),
                (WaterType::Swamp, "Swamp Water"),
                (WaterType::Spring, "Spring Water"),
            ];
            for &(water_type, name) in WATER_VARIANTS.iter() {
                items.push((
                    PaletteItem {
                        block: BlockType::Water,
                        model_id: 0,
                        tint_index: 0,
                        paint_texture_idx: 0,
                        water_type,
                    },
                    name.to_string(),
                ));
            }

            // Add tinted glass colors
            for &(tint_index, name) in TINTED_GLASS_COLORS.iter() {
                items.push((
                    PaletteItem {
                        block: BlockType::TintedGlass,
                        model_id: 0,
                        tint_index,
                        paint_texture_idx: 0,
                        water_type: WaterType::Ocean,
                    },
                    name.to_string(),
                ));
            }
            // Add crystal colors (emissive with tint)
            const CRYSTAL_COLORS: [(u8, &str); 8] = [
                (0, "Red Crystal"),
                (4, "Green Crystal"),
                (8, "Blue Crystal"),
                (6, "Cyan Crystal"),
                (2, "Yellow Crystal"),
                (1, "Orange Crystal"),
                (9, "Purple Crystal"),
                (12, "White Crystal"),
            ];
            for &(tint_index, name) in CRYSTAL_COLORS.iter() {
                items.push((
                    PaletteItem {
                        block: BlockType::Crystal,
                        model_id: 0,
                        tint_index,
                        paint_texture_idx: 0,
                        water_type: WaterType::Ocean,
                    },
                    name.to_string(),
                ));
            }
            // Add paintable block entry (defaults: planks texture, no tint)
            items.push((
                PaletteItem {
                    block: BlockType::Painted,
                    model_id: 0,
                    tint_index: 0,
                    paint_texture_idx: BlockType::Planks as u8,
                    water_type: WaterType::Ocean,
                },
                "Painted Block".to_string(),
            ));
        }

        if matches!(tab, PaletteTab::Models | PaletteTab::All) {
            let mut push_if = |id: u8, label: &str| {
                if registry.get(id).is_some() {
                    items.push((
                        PaletteItem {
                            block: BlockType::Model,
                            model_id: id,
                            tint_index: 0,
                            paint_texture_idx: 0,
                            water_type: WaterType::Ocean,
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
            push_if(100, "Tall Grass");
            push_if(101, "Red Flower");
            push_if(102, "Yellow Flower");
            push_if(103, "Lily Pad");
            push_if(104, "Brown Mushroom");
            push_if(105, "Red Mushroom");

            // Add custom/user models from the registry
            for model in registry.iter_custom_models() {
                items.push((
                    PaletteItem {
                        block: BlockType::Model,
                        model_id: model.id,
                        tint_index: 0,
                        paint_texture_idx: 0,
                        water_type: WaterType::Ocean,
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
        hotbar_paint_textures: &mut [u8; 9],
        hotbar_index: &mut usize,
        dragging_item: &mut Option<PaletteItem>,
    ) {
        const ATLAS_TILE_COUNT: f32 = 43.0;
        let block_idx =
            HudHelpers::atlas_tile_for(item.block, item.model_id, item.paint_texture_idx);
        let uv_left = block_idx / ATLAS_TILE_COUNT;
        let uv_right = (block_idx + 1.0) / ATLAS_TILE_COUNT;
        let uv_rect = egui::Rect::from_min_max(egui::pos2(uv_left, 0.0), egui::pos2(uv_right, 1.0));
        let sprite_id = HudHelpers::sprite_for_item(item, sprite_icons);

        ui.vertical(|ui| {
            // Calculate tint color for Painted blocks
            let tint_color = if item.block == BlockType::Painted {
                HudHelpers::tint_color(item.tint_index)
            } else {
                egui::Color32::WHITE
            };

            let button = if let Some(id) = sprite_id {
                egui::ImageButton::new((id, egui::vec2(48.0, 48.0)))
                    .uv(egui::Rect::from_min_max(
                        egui::pos2(0.0, 0.0),
                        egui::pos2(1.0, 1.0),
                    ))
                    .tint(tint_color)
                    .frame(true)
                    .sense(egui::Sense::click_and_drag())
            } else {
                egui::ImageButton::new((atlas_texture_id, egui::vec2(48.0, 48.0)))
                    .uv(uv_rect)
                    .tint(tint_color)
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
                HudHelpers::apply_item_to_slot(
                    item,
                    *hotbar_index,
                    hotbar_blocks,
                    hotbar_model_ids,
                    hotbar_tint_indices,
                    hotbar_paint_textures,
                );
            }
            if middle {
                HudHelpers::fill_or_replace_hotbar(
                    item,
                    hotbar_blocks,
                    hotbar_model_ids,
                    hotbar_tint_indices,
                    hotbar_paint_textures,
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
            if item.block == BlockType::TintedGlass || item.block == BlockType::Painted {
                let tint_color = HudHelpers::tint_color(item.tint_index);
                ui.painter().rect_stroke(
                    hover_rect.shrink(2.0),
                    egui::CornerRadius::same(3),
                    egui::Stroke::new(3.0, tint_color),
                    egui::StrokeKind::Outside,
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
    pub fn draw_palette_window(
        ctx: &egui::Context,
        atlas_texture_id: egui::TextureId,
        sprite_icons: Option<&SpriteIcons>,
        palette_open: &mut bool,
        palette_tab: &mut PaletteTab,
        palette_search: &mut String,
        dragging_item: &mut Option<PaletteItem>,
        model_registry: &ModelRegistry,
        hotbar_blocks: &mut [BlockType; 9],
        hotbar_model_ids: &mut [u8; 9],
        hotbar_tint_indices: &mut [u8; 9],
        hotbar_paint_textures: &mut [u8; 9],
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

                // Search bar
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    let response = ui.text_edit_singleline(palette_search);
                    if response.changed() {
                        // Convert to lowercase for case-insensitive search
                        *palette_search = palette_search.to_lowercase();
                    }
                    if ui.button("✖").clicked() {
                        palette_search.clear();
                    }
                });

                ui.label("Drag items to the hotbar, left-click to set current slot, middle-click to fill (or replace if full).");
                ui.separator();

                let items = Self::palette_items_for_tab(*palette_tab, model_registry);

                // Filter items based on search
                let search_lower = palette_search.to_lowercase();
                let filtered_items: Vec<_> = if search_lower.is_empty() {
                    items
                } else {
                    items
                        .into_iter()
                        .filter(|(_, label)| label.to_lowercase().contains(&search_lower))
                        .collect()
                };
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        for row in filtered_items.chunks(12) {
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
                                        hotbar_paint_textures,
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
}

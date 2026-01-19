//! Hotbar UI rendering.

use super::helpers::HudHelpers;
use crate::app_state::PaletteItem;
use crate::chunk::{BlockType, WaterType};
use crate::gpu_resources::SpriteIcons;
use egui_winit_vulkano::egui;

pub struct HotbarUI;

impl HotbarUI {
    pub fn draw_drag_preview(
        ctx: &egui::Context,
        item: PaletteItem,
        atlas_texture_id: egui::TextureId,
        sprite_icons: Option<&SpriteIcons>,
    ) {
        if let Some(pointer_pos) = ctx.input(|i| i.pointer.latest_pos()) {
            let (texture_id, uv_rect) = if let Some(tex) =
                HudHelpers::sprite_for_item(item, sprite_icons)
            {
                (
                    tex,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                )
            } else {
                let atlas_tile_count = HudHelpers::ATLAS_TILE_COUNT;
                let block_idx =
                    HudHelpers::atlas_tile_for(item.block, item.model_id, item.paint_texture_idx);
                let uv_left = block_idx / atlas_tile_count;
                let uv_right = (block_idx + 1.0) / atlas_tile_count;
                (
                    atlas_texture_id,
                    egui::Rect::from_min_max(egui::pos2(uv_left, 0.0), egui::pos2(uv_right, 1.0)),
                )
            };

            let size = egui::vec2(48.0, 48.0);
            let rect = egui::Rect::from_min_size(pointer_pos - size * 0.5, size);
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Tooltip,
                egui::Id::new("drag_preview"),
            ));

            // Calculate tint color for drag preview
            let texture_tint = if item.block == BlockType::Painted {
                HudHelpers::tint_color(item.tint_index)
            } else if item.block == BlockType::Water {
                HudHelpers::water_type_color(item.water_type)
            } else {
                egui::Color32::WHITE
            };

            painter.image(texture_id, rect, uv_rect, texture_tint);
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

    #[allow(clippy::too_many_arguments)]
    pub fn draw_hotbar(
        ctx: &egui::Context,
        hotbar_blocks: &mut [BlockType; 9],
        hotbar_model_ids: &mut [u8; 9],
        hotbar_tint_indices: &mut [u8; 9],
        hotbar_paint_textures: &mut [u8; 9],
        hotbar_index: &mut usize,
        atlas_texture_id: egui::TextureId,
        sprite_icons: Option<&SpriteIcons>,
        dragging_item: &mut Option<PaletteItem>,
    ) {
        let atlas_tile_count = HudHelpers::ATLAS_TILE_COUNT;
        const SLOT_SIZE: f32 = 40.0;
        let pointer_released =
            ctx.input(|i| i.pointer.button_released(egui::PointerButton::Primary));

        egui::Area::new(egui::Id::new("hotbar_hud"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -10.0))
            .show(ctx, |ui| {
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
                                    paint_texture_idx: hotbar_paint_textures[i],
                                    water_type: if block == BlockType::Water {
                                        WaterType::from_u8(hotbar_tint_indices[i])
                                    } else {
                                        WaterType::Ocean
                                    },
                                };
                                let (texture_id, uv_rect) = if let Some(tex) =
                                    HudHelpers::sprite_for_item(palette_item, sprite_icons)
                                {
                                    (
                                        tex,
                                        egui::Rect::from_min_max(
                                            egui::pos2(0.0, 0.0),
                                            egui::pos2(1.0, 1.0),
                                        ),
                                    )
                                } else {
                                    let block_idx = HudHelpers::atlas_tile_for(
                                        block,
                                        hotbar_model_ids[i],
                                        hotbar_paint_textures[i],
                                    );
                                    let uv_left = block_idx / atlas_tile_count;
                                    let uv_right = (block_idx + 1.0) / atlas_tile_count;
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
                                // Apply tint color for Painted blocks, white for others
                                let texture_tint = if block == BlockType::Painted {
                                    HudHelpers::tint_color(hotbar_tint_indices[i])
                                } else if block == BlockType::Water {
                                    // Apply water tint based on type
                                    let water_type = WaterType::from_u8(hotbar_tint_indices[i]);
                                    HudHelpers::water_type_color(water_type)
                                } else {
                                    egui::Color32::WHITE
                                };
                                ui.painter()
                                    .image(texture_id, texture_rect, uv_rect, texture_tint);
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
                                // Draw colored inner border for TintedGlass only
                                if block == BlockType::TintedGlass {
                                    let tint_color = HudHelpers::tint_color(hotbar_tint_indices[i]);
                                    ui.painter().rect_stroke(
                                        texture_rect.shrink(1.0),
                                        egui::CornerRadius::same(2),
                                        egui::Stroke::new(2.0, tint_color),
                                        egui::StrokeKind::Outside,
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
                                        HudHelpers::apply_item_to_slot(
                                            item,
                                            i,
                                            hotbar_blocks,
                                            hotbar_model_ids,
                                            hotbar_tint_indices,
                                            hotbar_paint_textures,
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
                            let selected_block = hotbar_blocks[*hotbar_index];
                            // For Model blocks, show the model type name
                            let block_name = if selected_block == BlockType::Model {
                                match hotbar_model_ids[*hotbar_index] {
                                    1 => "Torch".to_string(),
                                    2 | 3 => "Slab".to_string(),
                                    4..=19 => "Fence".to_string(),
                                    20..=27 => "Gate".to_string(),
                                    29 => "Ladder".to_string(),
                                    28 | 30..=38 => "Stairs".to_string(),
                                    39..=46 => "Door".to_string(),
                                    47..=50 => "Trapdoor".to_string(),
                                    51..=66 => "Window".to_string(),
                                    67..=74 => "Windowed Door".to_string(),
                                    75..=82 => "Paneled Door".to_string(),
                                    83..=90 => "Fancy Door".to_string(),
                                    91..=98 => "Glass Door".to_string(),
                                    99 => "Crystal".to_string(),
                                    100 => "Grass Tuft".to_string(),
                                    101 => "Flower".to_string(),
                                    102 => "Lily Pad".to_string(),
                                    103 | 104 => "Mushroom".to_string(),
                                    105 => "Tall Grass".to_string(),
                                    106 | 108 => "Stalactite".to_string(),
                                    107 | 109 => "Stalagmite".to_string(),
                                    110..=118 => "Model".to_string(),
                                    119..=134 => "Glass Pane (H)".to_string(),
                                    135..=150 => "Glass Pane (V)".to_string(),
                                    _ => "Model".to_string(),
                                }
                            } else if hotbar_blocks[*hotbar_index] == BlockType::Water {
                                match WaterType::from_u8(hotbar_tint_indices[*hotbar_index]) {
                                    WaterType::Ocean => "Ocean Water".to_string(),
                                    WaterType::Lake => "Lake Water".to_string(),
                                    WaterType::River => "River Water".to_string(),
                                    WaterType::Swamp => "Swamp Water".to_string(),
                                    WaterType::Spring => "Spring Water".to_string(),
                                }
                            } else if selected_block == BlockType::Painted {
                                let tex = hotbar_paint_textures[*hotbar_index];
                                let tint = hotbar_tint_indices[*hotbar_index];
                                if HudHelpers::is_custom_texture(tex) {
                                    // Custom texture (128+)
                                    let custom_idx = tex - HudHelpers::CUSTOM_TEXTURE_FLAG;
                                    format!("Custom #{} (tint {})", custom_idx, tint)
                                } else {
                                    match tex {
                                        23 => "Cactus".to_string(),
                                        24 => "Mud".to_string(),
                                        25 => "Sandstone".to_string(),
                                        26 => "Ice".to_string(),
                                        _ => format!("Painted (tex {}, tint {})", tex, tint),
                                    }
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
    }
}

//! Helper functions for UI rendering.

use crate::PaletteItem;
use crate::chunk::{BlockType, WaterType};
use crate::gpu_resources::SpriteIcons;
use egui_winit_vulkano::egui;

pub struct HudHelpers;

impl HudHelpers {
    /// Convert tint_index to egui::Color32 for UI display.
    /// Matches TINT_PALETTE in shaders/common.glsl.
    pub fn tint_color(tint_index: u8) -> egui::Color32 {
        // Use the same palette as the shader (from chunk.rs)
        let color = crate::chunk::tint_color(tint_index);
        egui::Color32::from_rgb(
            (color[0] * 255.0) as u8,
            (color[1] * 255.0) as u8,
            (color[2] * 255.0) as u8,
        )
    }

    pub fn sprite_for_item(
        item: PaletteItem,
        icons: Option<&SpriteIcons>,
    ) -> Option<egui::TextureId> {
        let set = icons?;
        match item.block {
            BlockType::Model => set.model.get(&item.model_id).copied().or(Some(set.missing)),
            BlockType::TintedGlass => set
                .tinted_glass
                .get(&item.tint_index)
                .copied()
                .or(Some(set.missing)),
            BlockType::Crystal => set
                .crystal
                .get(&item.tint_index)
                .copied()
                .or(Some(set.missing)),
            BlockType::Painted => None, // Use atlas texture + tint instead of sprite
            BlockType::Air => None,
            _ => set.block.get(&item.block).copied().or(Some(set.missing)),
        }
    }

    pub fn atlas_tile_for(block: BlockType, model_id: u8, paint_texture_idx: u8) -> f32 {
        if block == BlockType::Painted {
            paint_texture_idx as f32
        } else if block == BlockType::Model {
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

    pub fn apply_item_to_slot(
        item: PaletteItem,
        slot: usize,
        hotbar_blocks: &mut [BlockType; 9],
        hotbar_model_ids: &mut [u8; 9],
        hotbar_tint_indices: &mut [u8; 9],
        hotbar_paint_textures: &mut [u8; 9],
    ) {
        hotbar_blocks[slot] = item.block;
        hotbar_model_ids[slot] = if item.block == BlockType::Model {
            item.model_id
        } else {
            0
        };
        hotbar_tint_indices[slot] = if item.block == BlockType::TintedGlass
            || item.block == BlockType::Painted
            || item.block == BlockType::Crystal
        {
            item.tint_index
        } else if item.block == BlockType::Water {
            item.water_type as u8
        } else {
            0
        };
        hotbar_paint_textures[slot] = if item.block == BlockType::Painted {
            item.paint_texture_idx
        } else {
            0
        };
    }

    pub fn fill_or_replace_hotbar(
        item: PaletteItem,
        hotbar_blocks: &mut [BlockType; 9],
        hotbar_model_ids: &mut [u8; 9],
        hotbar_tint_indices: &mut [u8; 9],
        hotbar_paint_textures: &mut [u8; 9],
        hotbar_index: &mut usize,
    ) {
        if let Some(empty_slot) = hotbar_blocks.iter().position(|b| *b == BlockType::Air) {
            Self::apply_item_to_slot(
                item,
                empty_slot,
                hotbar_blocks,
                hotbar_model_ids,
                hotbar_tint_indices,
                hotbar_paint_textures,
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
                hotbar_paint_textures,
            );
        }
    }

    pub fn water_type_color(water_type: WaterType) -> egui::Color32 {
        match water_type {
            WaterType::Lake => egui::Color32::from_rgb(178, 255, 229),
            WaterType::River => egui::Color32::from_rgb(229, 255, 255),
            WaterType::Swamp => egui::Color32::from_rgb(153, 178, 102),
            WaterType::Spring => egui::Color32::from_rgb(255, 255, 255),
            WaterType::Ocean => egui::Color32::WHITE,
        }
    }

    pub fn overlay_frame() -> egui::Frame {
        egui::Frame::new()
            .fill(egui::Color32::from_black_alpha(180))
            .corner_radius(egui::CornerRadius::same(4))
            .inner_margin(egui::Margin::same(6))
    }
}

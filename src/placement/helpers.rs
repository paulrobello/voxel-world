//! Shared helper functions for block placement.
//!
//! This module contains the common block placement logic that handles different
//! block types (TintedGlass, Crystal, Painted, Water, Lava, etc.) consistently
//! across all shape tools.

use crate::chunk::{BlockType, WaterType};
use crate::constants::TEXTURE_SIZE_Y;
use crate::lava::LavaGrid;
use crate::water::WaterGrid;
use crate::world::World;
use nalgebra::Vector3;

/// Parameters for block placement from the hotbar.
#[derive(Clone, Copy, Debug)]
pub struct BlockPlacementParams {
    /// The block type to place.
    pub block_type: BlockType,
    /// Tint index for tinted blocks (TintedGlass, Crystal) or water type.
    pub tint_index: u8,
    /// Paint texture index for painted blocks.
    pub paint_texture: u8,
}

impl BlockPlacementParams {
    /// Create new placement parameters from hotbar state.
    pub fn new(block_type: BlockType, tint_index: u8, paint_texture: u8) -> Self {
        Self {
            block_type,
            tint_index,
            paint_texture,
        }
    }
}

/// Place blocks at the given positions using the specified block type and metadata.
///
/// This is the shared implementation used by all shape tools. It handles:
/// - Y bounds checking (X/Z are infinite)
/// - TintedGlass with tint index
/// - Crystal with tint index
/// - Painted blocks with texture + tint
/// - Water with water type and source placement
/// - Lava with source placement
/// - Skipping Model and Air blocks
/// - Regular blocks
///
/// Returns the number of blocks actually placed.
pub fn place_blocks_at_positions(
    positions: &[Vector3<i32>],
    params: BlockPlacementParams,
    world: &mut World,
    water_grid: &mut WaterGrid,
    lava_grid: &mut LavaGrid,
) -> usize {
    let mut placed_count = 0;

    for pos in positions {
        // Skip if out of Y bounds (X/Z are infinite)
        if pos.y < 0 || pos.y >= TEXTURE_SIZE_Y as i32 {
            continue;
        }

        match params.block_type {
            BlockType::TintedGlass => {
                world.set_tinted_glass_block(*pos, params.tint_index);
            }
            BlockType::Crystal => {
                world.set_crystal_block(*pos, params.tint_index);
            }
            BlockType::Painted => {
                world.set_painted_block(*pos, params.paint_texture, params.tint_index);
            }
            BlockType::Water => {
                let water_type = WaterType::from_u8(params.tint_index);
                water_grid.place_source(*pos, water_type);
                world.set_water_block(*pos, water_type);
            }
            BlockType::Lava => {
                lava_grid.place_source(*pos);
                world.set_block(*pos, BlockType::Lava);
            }
            BlockType::Model | BlockType::Air => {
                // Skip model and air blocks - don't make sense for shape fill
                continue;
            }
            _ => {
                world.set_block(*pos, params.block_type);
            }
        }
        placed_count += 1;
    }

    placed_count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_placement_params() {
        let params = BlockPlacementParams::new(BlockType::Stone, 5, 3);
        assert_eq!(params.block_type, BlockType::Stone);
        assert_eq!(params.tint_index, 5);
        assert_eq!(params.paint_texture, 3);
    }
}

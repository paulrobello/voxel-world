//! Shared helper functions for block placement.
//!
//! This module contains the common block placement logic that handles different
//! block types (TintedGlass, Crystal, Painted, Water, Lava, etc.) consistently
//! across all shape tools.

use crate::chunk::{BlockPaintData, BlockType, WaterType};
use crate::constants::TEXTURE_SIZE_Y;
use crate::lava::LavaGrid;
use crate::net::protocol::BlockData;
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

/// Builds the `BlockData` representing what [`place_blocks_at_positions`]
/// writes for `params`, or `None` for Model/Air (which placement skips).
///
/// Branch-per-type to mirror `place_blocks_at_positions` exactly:
/// - TintedGlass/Crystal carry `tint_index`
/// - Painted carries texture+tint via `BlockPaintData::simple`
/// - Water carries `water_type` (so receivers place the source via their
///   existing Water apply arm in `apply_remote_block_changes`)
/// - Lava and any plain block use `BlockData::from(block_type)`
/// - Model/Air return `None` (placement skips them, so sync skips them too)
///
/// Shared by the shape-tool sync path (`block_interaction::sync_shape_blocks`)
/// and the console (`fill`/`sphere`) so both produce identical `BlockData` for
/// the same `BlockPlacementParams`.
pub fn block_data_for_params(params: BlockPlacementParams) -> Option<BlockData> {
    let block = match params.block_type {
        BlockType::TintedGlass | BlockType::Crystal => BlockData {
            block_type: params.block_type,
            tint_index: Some(params.tint_index),
            ..Default::default()
        },
        BlockType::Painted => BlockData {
            block_type: params.block_type,
            paint_data: Some(BlockPaintData::simple(
                params.paint_texture,
                params.tint_index,
            )),
            ..Default::default()
        },
        BlockType::Water => BlockData {
            block_type: params.block_type,
            water_type: Some(WaterType::from_u8(params.tint_index)),
            ..Default::default()
        },
        BlockType::Model | BlockType::Air => return None,
        other => BlockData::from(other),
    };
    Some(block)
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

    // ── block_data_for_params: params → BlockData conversion ──────────────
    //
    // Exercises every per-type branch so the conversion cannot silently drift
    // from `place_blocks_at_positions`. Shared with the console fill/sphere
    // path, so a regression here would also break multiplayer console sync.

    #[test]
    fn test_block_data_for_plain_block() {
        let bd = block_data_for_params(BlockPlacementParams::new(BlockType::Stone, 0, 0))
            .expect("plain block yields BlockData");
        assert_eq!(bd, BlockData::from(BlockType::Stone));
        assert!(bd.tint_index.is_none() && bd.paint_data.is_none() && bd.water_type.is_none());
    }

    #[test]
    fn test_block_data_for_tinted_glass_and_crystal() {
        for ty in [BlockType::TintedGlass, BlockType::Crystal] {
            let bd = block_data_for_params(BlockPlacementParams::new(ty, 7, 0))
                .expect("tinted block yields BlockData");
            assert_eq!(bd.block_type, ty);
            assert_eq!(bd.tint_index, Some(7));
            assert!(bd.paint_data.is_none() && bd.water_type.is_none());
        }
    }

    #[test]
    fn test_block_data_for_painted() {
        let bd = block_data_for_params(BlockPlacementParams::new(BlockType::Painted, 4, 9))
            .expect("painted block yields BlockData");
        assert_eq!(bd.block_type, BlockType::Painted);
        // paint_data mirrors BlockPaintData::simple(texture, tint).
        assert_eq!(bd.paint_data, Some(BlockPaintData::simple(9, 4)));
        assert!(bd.tint_index.is_none() && bd.water_type.is_none());
    }

    #[test]
    fn test_block_data_for_water_carries_source_type() {
        // tint_index selects the WaterType; the receiver's apply path places
        // the source from this field, so carrying it is load-bearing.
        let bd = block_data_for_params(BlockPlacementParams::new(BlockType::Water, 2, 0))
            .expect("water yields BlockData");
        assert_eq!(bd.block_type, BlockType::Water);
        assert_eq!(bd.water_type, Some(WaterType::River)); // from_u8(2) == River
    }

    #[test]
    fn test_block_data_for_lava_is_plain() {
        // Lava falls through to the plain-block branch (the client-side apply
        // path has no Lava-specific source placement, matching single-block).
        let bd = block_data_for_params(BlockPlacementParams::new(BlockType::Lava, 0, 0))
            .expect("lava yields BlockData");
        assert_eq!(bd, BlockData::from(BlockType::Lava));
    }

    #[test]
    fn test_block_data_for_model_and_air_is_none() {
        // place_blocks_at_positions skips Model and Air; sync must skip them
        // too so they never appear in a BlocksChanged batch.
        assert!(block_data_for_params(BlockPlacementParams::new(BlockType::Model, 0, 0)).is_none());
        assert!(block_data_for_params(BlockPlacementParams::new(BlockType::Air, 0, 0)).is_none());
    }
}

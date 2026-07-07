//! Fill command implementation.
//!
//! Fills a rectangular region with the specified block type.

use crate::chunk::BlockType;
use crate::console::{
    CommandResult, parse_coordinate, validate_y_bounds, volume_confirm_threshold,
};
use crate::lava::LavaGrid;
use crate::net::protocol::BlockData;
use crate::placement::{BlockPlacementParams, block_data_for_params, place_blocks_at_positions};
use crate::water::WaterGrid;
use crate::world::World;
use nalgebra::Vector3;

/// Number of coordinate args a `fill` takes (x1 y1 z1 x2 y2 z2).
const FILL_COORD_ARG_COUNT: usize = 6;

const USAGE: &str = "Usage: fill <block> [meta...] <x1> <y1> <z1> <x2> <y2> <z2> [hollow]\n  meta: <tint> for tintedglass/crystal/water/lava; <texture> <tint> for painted";

/// Execute the fill command.
///
/// Syntax:
/// - `fill <block> <x1> <y1> <z1> <x2> <y2> <z2> [hollow]`
/// - `fill tintedglass|crystal|water|lava <tint> <x1> <y1> <z1> <x2> <y2> <z2> [hollow]`
/// - `fill painted <texture> <tint> <x1> <y1> <z1> <x2> <y2> <z2> [hollow]`
///
/// The optional metadata args are disambiguated from coordinates purely by
/// count: coordinates are always exactly 6, so 7 positional args after the
/// block means one leading tint arg, 8 means two leading paint args (texture,
/// tint), 6 means no metadata (defaults to 0). `hollow` is a trailing flag.
#[allow(clippy::too_many_arguments)]
pub fn fill(
    args: &[&str],
    world: &mut World,
    water_grid: &mut WaterGrid,
    lava_grid: &mut LavaGrid,
    player_pos: Vector3<i32>,
    confirmed: bool,
) -> CommandResult {
    if args.is_empty() {
        return CommandResult::Error(USAGE.to_string());
    }

    // Parse block name
    let block_name = args[0].to_lowercase();
    let block = match BlockType::from_name(&block_name) {
        Some(b) => b,
        None => {
            return CommandResult::Error(format!(
                "Unknown block type: '{}'. Valid types: {}",
                block_name,
                BlockType::all_block_names().join(", ")
            ));
        }
    };

    // `hollow` is a trailing flag that may appear anywhere after the block name.
    let hollow = args
        .iter()
        .skip(1)
        .any(|a| a.eq_ignore_ascii_case("hollow"));
    // Positional args (everything after the block name, minus the flag).
    let positional: Vec<&str> = args[1..]
        .iter()
        .copied()
        .filter(|a| !a.eq_ignore_ascii_case("hollow"))
        .collect();

    // Disambiguate optional per-type metadata from the 6 coordinate args by
    // positional count (see fn doc).
    let (tint_index, paint_texture, coord_args): (u8, u8, &[&str]) = match positional.len() {
        n if n == FILL_COORD_ARG_COUNT => (0, 0, &positional[..FILL_COORD_ARG_COUNT]),
        n if n == FILL_COORD_ARG_COUNT + 1
            && matches!(
                block,
                BlockType::TintedGlass | BlockType::Crystal | BlockType::Water | BlockType::Lava
            ) =>
        {
            let tint = match parse_u8_meta(positional[0], "tint") {
                Ok(v) => v,
                Err(e) => return CommandResult::Error(e),
            };
            (tint, 0, &positional[1..FILL_COORD_ARG_COUNT + 1])
        }
        n if n == FILL_COORD_ARG_COUNT + 2 && block == BlockType::Painted => {
            let texture = match parse_u8_meta(positional[0], "paint texture") {
                Ok(v) => v,
                Err(e) => return CommandResult::Error(e),
            };
            let tint = match parse_u8_meta(positional[1], "paint tint") {
                Ok(v) => v,
                Err(e) => return CommandResult::Error(e),
            };
            (tint, texture, &positional[2..FILL_COORD_ARG_COUNT + 2])
        }
        _ => return CommandResult::Error(USAGE.to_string()),
    };

    // Parse coordinates
    let x1 = match parse_coordinate(coord_args[0], player_pos.x) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let y1 = match parse_coordinate(coord_args[1], player_pos.y) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let z1 = match parse_coordinate(coord_args[2], player_pos.z) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let x2 = match parse_coordinate(coord_args[3], player_pos.x) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let y2 = match parse_coordinate(coord_args[4], player_pos.y) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let z2 = match parse_coordinate(coord_args[5], player_pos.z) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };

    // Normalize coordinates (min/max)
    let min_x = x1.min(x2);
    let max_x = x1.max(x2);
    let min_y = y1.min(y2);
    let max_y = y1.max(y2);
    let min_z = z1.min(z2);
    let max_z = z1.max(z2);

    // Validate Y bounds
    if let Some(error) = validate_y_bounds(min_y) {
        return CommandResult::Error(error);
    }
    if let Some(error) = validate_y_bounds(max_y) {
        return CommandResult::Error(error);
    }

    // Calculate volume
    let width = (max_x - min_x + 1) as u64;
    let height = (max_y - min_y + 1) as u64;
    let depth = (max_z - min_z + 1) as u64;
    let volume = width * height * depth;

    // Calculate actual blocks to fill (for hollow, only outer shell)
    let fill_count = if hollow {
        calculate_hollow_volume(width, height, depth)
    } else {
        volume
    };

    // Check volume threshold
    if !confirmed && fill_count > volume_confirm_threshold() {
        let original_cmd = args.join(" ");
        return CommandResult::NeedsConfirmation {
            message: format!("This will modify {} blocks. Are you sure?", fill_count),
            command: format!("fill {}", original_cmd),
        };
    }

    // Partition into boundary positions (the block being filled) and, for
    // hollow, interior positions (cleared to Air).
    let mut boundary_positions: Vec<Vector3<i32>> = Vec::new();
    let mut interior_positions: Vec<Vector3<i32>> = Vec::new();
    for x in min_x..=max_x {
        for y in min_y..=max_y {
            for z in min_z..=max_z {
                let pos = Vector3::new(x, y, z);
                let is_boundary = x == min_x
                    || x == max_x
                    || y == min_y
                    || y == max_y
                    || z == min_z
                    || z == max_z;
                if hollow && !is_boundary {
                    interior_positions.push(pos);
                } else {
                    boundary_positions.push(pos);
                }
            }
        }
    }

    let params = BlockPlacementParams::new(block, tint_index, paint_texture);
    execute_fill(
        block,
        params,
        &boundary_positions,
        &interior_positions,
        world,
        water_grid,
        lava_grid,
    )
}

/// Apply a fill: place boundary blocks via the shared placement pipeline (so
/// tint/paint/water/lava sources are handled identically to shape tools), clear
/// hollow interior to Air, and build the `(position, BlockData)` sync list with
/// full metadata via [`block_data_for_params`].
#[allow(clippy::too_many_arguments)]
fn execute_fill(
    block: BlockType,
    params: BlockPlacementParams,
    boundary_positions: &[Vector3<i32>],
    interior_positions: &[Vector3<i32>],
    world: &mut World,
    water_grid: &mut WaterGrid,
    lava_grid: &mut LavaGrid,
) -> CommandResult {
    let air_data = BlockData::from(BlockType::Air);
    let placed_data = block_data_for_params(params);
    let mut changed_blocks: Vec<(Vector3<i32>, BlockData)> =
        Vec::with_capacity(boundary_positions.len() + interior_positions.len());
    let mut count = 0u64;

    if block == BlockType::Air {
        // placement skips Air; clear directly so `/fill air` still empties a region.
        for pos in boundary_positions {
            world.set_block(*pos, BlockType::Air);
            changed_blocks.push((*pos, air_data.clone()));
            count += 1;
        }
    } else {
        count += place_blocks_at_positions(boundary_positions, params, world, water_grid, lava_grid)
            as u64;
        // `place_blocks_at_positions` skips Model (nonsensical for fill); in that
        // case `placed_data` is None and there is nothing to sync.
        if let Some(data) = placed_data {
            for pos in boundary_positions {
                changed_blocks.push((*pos, data.clone()));
            }
        }
    }

    // Clear interior for hollow.
    for pos in interior_positions {
        world.set_block(*pos, BlockType::Air);
        changed_blocks.push((*pos, air_data.clone()));
        count += 1;
    }

    let mode = if !interior_positions.is_empty() {
        " (hollow)"
    } else {
        ""
    };
    CommandResult::success_with_blocks(
        format!("Filled {} blocks with {:?}{}", count, block, mode),
        changed_blocks,
    )
}

/// Parse a metadata arg (tint/paint) as a `u8`.
fn parse_u8_meta(s: &str, label: &str) -> Result<u8, String> {
    s.parse::<u8>()
        .map_err(|_| format!("Invalid {} value: '{}'", label, s))
}

/// Calculate the number of blocks in a hollow box shell.
fn calculate_hollow_volume(width: u64, height: u64, depth: u64) -> u64 {
    if width <= 2 || height <= 2 || depth <= 2 {
        // Box is too small to be hollow - all blocks are on boundary
        width * height * depth
    } else {
        // Total volume minus interior
        let total = width * height * depth;
        let interior = (width - 2) * (height - 2) * (depth - 2);
        total - interior
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::WaterType;
    use crate::lava::LavaGrid;
    use crate::water::WaterGrid;
    use crate::world::World;

    /// Helper: run a `/fill ...` command string and return the result.
    fn run_fill(cmd: &str, world: &mut World) -> CommandResult {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        let args = &parts[1..];
        let mut water = WaterGrid::new();
        let mut lava = LavaGrid::new();
        fill(
            args,
            world,
            &mut water,
            &mut lava,
            Vector3::new(0, 32, 0),
            false,
        )
    }

    /// `/fill stone` over a 2x1x2 region places Stone and reports BlockData::Stone.
    #[test]
    fn test_fill_stone_uses_placement_pipeline() {
        let mut world = World::new();
        let result = run_fill("fill stone 0 32 0 1 32 1", &mut world);
        let (msg, blocks) = match result {
            CommandResult::Success {
                message,
                changed_blocks,
            } => (message, changed_blocks),
            CommandResult::Error(e) => panic!("expected success, got Error: {}", e),
            _ => panic!("expected success, got non-Success CommandResult"),
        };
        assert!(msg.starts_with("Filled 4 blocks"));
        assert_eq!(blocks.len(), 4);
        for (pos, bd) in &blocks {
            assert_eq!(world.get_block(*pos), Some(BlockType::Stone));
            assert_eq!(*bd, BlockData::from(BlockType::Stone));
            assert!(bd.tint_index.is_none() && bd.paint_data.is_none() && bd.water_type.is_none());
        }
    }

    /// `/fill water` places actual water sources (proved via WaterGrid state)
    /// and the sync list carries `water_type`.
    #[test]
    fn test_fill_water_places_source_and_carries_water_type() {
        let mut world = World::new();
        let mut water = WaterGrid::new();
        let mut lava = LavaGrid::new();
        let args: Vec<&str> = vec!["water", "0", "32", "0", "1", "32", "1"];
        let result = fill(
            &args,
            &mut world,
            &mut water,
            &mut lava,
            Vector3::new(0, 32, 0),
            false,
        );
        let blocks = match result {
            CommandResult::Success { changed_blocks, .. } => changed_blocks,
            CommandResult::Error(e) => panic!("expected success, got Error: {}", e),
            _ => panic!("expected success, got non-Success CommandResult"),
        };
        // WaterGrid gained active source cells (one per position).
        assert_eq!(water.cell_count(), 4, "water source cells should be placed");
        for (_, bd) in &blocks {
            assert_eq!(bd.block_type, BlockType::Water);
            assert_eq!(bd.water_type, Some(WaterType::from_u8(0)));
        }
    }

    /// `/fill tintedglass 5` parses the leading tint arg and the world/sync
    /// list both carry tint index 5.
    #[test]
    fn test_fill_tintedglass_with_tint_arg() {
        let mut world = World::new();
        let result = run_fill("fill tintedglass 5 0 32 0 0 32 0", &mut world);
        let blocks = match result {
            CommandResult::Success { changed_blocks, .. } => changed_blocks,
            CommandResult::Error(e) => panic!("expected success, got Error: {}", e),
            _ => panic!("expected success, got non-Success CommandResult"),
        };
        assert_eq!(blocks.len(), 1);
        let (pos, bd) = &blocks[0];
        assert_eq!(*pos, Vector3::new(0, 32, 0));
        assert_eq!(bd.block_type, BlockType::TintedGlass);
        assert_eq!(bd.tint_index, Some(5));
        assert_eq!(world.get_tint_index(*pos), Some(5));
    }

    /// `/fill painted <tex> <tint> ...` parses both metadata args.
    #[test]
    fn test_fill_painted_with_texture_and_tint() {
        let mut world = World::new();
        let result = run_fill("fill painted 3 9 0 32 0 0 32 0", &mut world);
        let blocks = match result {
            CommandResult::Success { changed_blocks, .. } => changed_blocks,
            CommandResult::Error(e) => panic!("expected success, got Error: {}", e),
            _ => panic!("expected success, got non-Success CommandResult"),
        };
        assert_eq!(blocks.len(), 1);
        let bd = &blocks[0].1;
        assert_eq!(bd.block_type, BlockType::Painted);
        // block_data_for_params packs (paint_texture=3, tint_index=9) -> simple(3,9).
        assert_eq!(
            bd.paint_data,
            Some(crate::chunk::BlockPaintData::simple(3, 9))
        );
    }

    /// Tint arg is optional; absent it defaults to 0 (backward compat).
    #[test]
    fn test_fill_tintedglass_default_tint_when_absent() {
        let mut world = World::new();
        let result = run_fill("fill tintedglass 0 32 0 0 32 0", &mut world);
        let blocks = match result {
            CommandResult::Success { changed_blocks, .. } => changed_blocks,
            CommandResult::Error(e) => panic!("expected success, got Error: {}", e),
            _ => panic!("expected success, got non-Success CommandResult"),
        };
        assert_eq!(blocks[0].1.tint_index, Some(0));
    }

    /// Hollow fill clears interior to Air and the sync list reflects it.
    #[test]
    fn test_fill_hollow_clears_interior() {
        let mut world = World::new();
        // 3x3x3 cube (y 32..34): 27 total, 1 interior Air, 26 boundary Stone.
        let result = run_fill("fill stone 0 32 0 2 34 2 hollow", &mut world);
        let blocks = match result {
            CommandResult::Success { changed_blocks, .. } => changed_blocks,
            CommandResult::Error(e) => panic!("expected success, got Error: {}", e),
            _ => panic!("expected success, got non-Success CommandResult"),
        };
        let (stones, air) = blocks
            .iter()
            .fold((0, 0), |(s, a), (_, bd)| match bd.block_type {
                BlockType::Stone => (s + 1, a),
                BlockType::Air => (s, a + 1),
                _ => (s, a),
            });
        assert_eq!(stones, 26);
        assert_eq!(air, 1);
    }

    /// Unknown block errors cleanly.
    #[test]
    fn test_fill_unknown_block() {
        let mut world = World::new();
        let result = run_fill("fill notablock 0 32 0 0 32 0", &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    /// Too few args returns usage error.
    #[test]
    fn test_fill_too_few_args() {
        let mut world = World::new();
        let result = run_fill("fill stone 0 32 0", &mut world);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    /// Boundary detection helper.
    #[allow(clippy::too_many_arguments)]
    fn is_boundary_test(
        x: i32,
        y: i32,
        z: i32,
        min_x: i32,
        max_x: i32,
        min_y: i32,
        max_y: i32,
        min_z: i32,
        max_z: i32,
    ) -> bool {
        x == min_x || x == max_x || y == min_y || y == max_y || z == min_z || z == max_z
    }

    #[test]
    fn test_is_boundary() {
        assert!(is_boundary_test(0, 0, 0, 0, 2, 0, 2, 0, 2));
        assert!(is_boundary_test(2, 2, 2, 0, 2, 0, 2, 0, 2));
        assert!(!is_boundary_test(1, 1, 1, 0, 2, 0, 2, 0, 2));
        assert!(is_boundary_test(0, 1, 1, 0, 2, 0, 2, 0, 2));
    }

    #[test]
    fn test_hollow_volume() {
        assert_eq!(calculate_hollow_volume(3, 3, 3), 26);
        assert_eq!(calculate_hollow_volume(4, 4, 4), 56);
        assert_eq!(calculate_hollow_volume(2, 2, 2), 8);
        assert_eq!(calculate_hollow_volume(1, 1, 1), 1);
    }
}

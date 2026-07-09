//! Sphere command implementation.
//!
//! Creates a sphere of blocks at the specified center with given radius.

use crate::chunk::BlockType;
use crate::console::{
    CommandResult, parse_coordinate, validate_y_bounds, volume_confirm_threshold,
};
use crate::lava::LavaGrid;
use crate::net::protocol::BlockData;
use crate::placement::{BlockPlacementParams, block_data_for_params, place_blocks_at_positions};
use crate::shape_tools::sphere::{estimate_volume, generate_sphere_positions};
use crate::water::WaterGrid;
use crate::world::World;
use nalgebra::Vector3;
use std::collections::HashSet;

/// Number of coordinate args a `sphere` takes (cx cy cz radius).
const SPHERE_COORD_ARG_COUNT: usize = 4;

const USAGE: &str = "Usage: sphere <block> [meta...] <cx> <cy> <cz> <radius> [hollow] [dome]\n  meta: <tint> for tintedglass/crystal/water/lava; <texture> <tint> for painted";

/// Execute the sphere command.
///
/// Syntax:
/// - `sphere <block> <cx> <cy> <cz> <radius> [hollow] [dome]`
/// - `sphere tintedglass|crystal|water|lava <tint> <cx> <cy> <cz> <radius> ...`
/// - `sphere painted <texture> <tint> <cx> <cy> <cz> <radius> ...`
///
/// Optional metadata is disambiguated from coordinates by count (coordinates
/// are always exactly 4): 5 positional args after the block means one leading
/// tint, 6 means two paint args (texture, tint), 4 means no metadata (default
/// 0). `hollow`/`dome` are trailing flags in any order.
#[allow(clippy::too_many_arguments)]
pub fn sphere(
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

    // `hollow`/`dome` are trailing flags that may appear in any order after the
    // block name. Positional args are the rest.
    let mut hollow = false;
    let mut dome = false;
    let mut positional: Vec<&str> = Vec::with_capacity(args.len() - 1);
    for arg in args[1..].iter().copied() {
        match arg.to_lowercase().as_str() {
            "hollow" => hollow = true,
            "dome" => dome = true,
            _ => positional.push(arg),
        }
    }

    // Disambiguate optional per-type metadata from the 4 coordinate args by
    // positional count (see fn doc).
    let (tint_index, paint_texture, coord_args): (u8, u8, &[&str]) = match positional.len() {
        n if n == SPHERE_COORD_ARG_COUNT => (0, 0, &positional[..SPHERE_COORD_ARG_COUNT]),
        n if n == SPHERE_COORD_ARG_COUNT + 1
            && matches!(
                block,
                BlockType::TintedGlass | BlockType::Crystal | BlockType::Water | BlockType::Lava
            ) =>
        {
            let tint = match parse_u8_meta(positional[0], "tint") {
                Ok(v) => v,
                Err(e) => return CommandResult::Error(e),
            };
            (tint, 0, &positional[1..SPHERE_COORD_ARG_COUNT + 1])
        }
        n if n == SPHERE_COORD_ARG_COUNT + 2 && block == BlockType::Painted => {
            let texture = match parse_u8_meta(positional[0], "paint texture") {
                Ok(v) => v,
                Err(e) => return CommandResult::Error(e),
            };
            let tint = match parse_u8_meta(positional[1], "paint tint") {
                Ok(v) => v,
                Err(e) => return CommandResult::Error(e),
            };
            (tint, texture, &positional[2..SPHERE_COORD_ARG_COUNT + 2])
        }
        _ => return CommandResult::Error(USAGE.to_string()),
    };

    // Parse center coordinates
    let cx = match parse_coordinate(coord_args[0], player_pos.x) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let cy = match parse_coordinate(coord_args[1], player_pos.y) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let cz = match parse_coordinate(coord_args[2], player_pos.z) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };

    // Parse radius
    let radius: i32 = match coord_args[3].parse() {
        Ok(r) if r > 0 => r,
        Ok(_) => return CommandResult::Error("Radius must be positive".to_string()),
        Err(_) => return CommandResult::Error(format!("Invalid radius: '{}'", coord_args[3])),
    };

    // Validate Y bounds for sphere extent
    let min_y = if dome { cy } else { cy - radius };
    let max_y = cy + radius;
    if let Some(error) = validate_y_bounds(min_y) {
        return CommandResult::Error(error);
    }
    if let Some(error) = validate_y_bounds(max_y) {
        return CommandResult::Error(error);
    }

    // Estimate volume for confirmation (shared formula with the sphere tool).
    let mut estimated_volume = estimate_volume(radius, hollow);
    if dome {
        estimated_volume /= 2;
    }

    // Check volume threshold
    if !confirmed && estimated_volume > volume_confirm_threshold() {
        let original_cmd = args.join(" ");
        return CommandResult::NeedsConfirmation {
            message: format!(
                "This will modify approximately {} blocks. Are you sure?",
                estimated_volume
            ),
            command: format!("sphere {}", original_cmd),
        };
    }

    // Use the shared sphere geometry (ARC-TOOL-001). Split shell vs interior by
    // differencing a solid sphere of radius-1 out of the full sphere: this matches
    // the console's hollow semantics exactly, including the floor ring at y == cy
    // for hollow domes (the shared hollow path drops that ring, the console never did).
    let center = Vector3::new(cx, cy, cz);
    let outer = generate_sphere_positions(center, radius, false, dome);
    let (shell_positions, interior_positions): (Vec<Vector3<i32>>, Vec<Vector3<i32>>) =
        if hollow && radius > 1 {
            let inner: HashSet<Vector3<i32>> =
                generate_sphere_positions(center, radius - 1, false, dome)
                    .into_iter()
                    .collect();
            let mut shell = Vec::with_capacity(outer.len());
            let mut interior = Vec::new();
            for pos in outer {
                if inner.contains(&pos) {
                    interior.push(pos);
                } else {
                    shell.push(pos);
                }
            }
            (shell, interior)
        } else {
            (outer, Vec::new())
        };

    let params = BlockPlacementParams::new(block, tint_index, paint_texture);
    execute_sphere(
        block,
        params,
        &shell_positions,
        &interior_positions,
        hollow,
        dome,
        world,
        water_grid,
        lava_grid,
    )
}

/// Apply a sphere: place shell blocks via the shared placement pipeline (so
/// tint/paint/water/lava sources are handled identically to shape tools), clear
/// hollow interior to Air, and build the `(position, BlockData)` sync list with
/// full metadata via [`block_data_for_params`]. The reported count excludes
/// hollow-interior Air clears (matching the original behavior).
#[allow(clippy::too_many_arguments)]
fn execute_sphere(
    block: BlockType,
    params: BlockPlacementParams,
    shell_positions: &[Vector3<i32>],
    interior_positions: &[Vector3<i32>],
    hollow: bool,
    dome: bool,
    world: &mut World,
    water_grid: &mut WaterGrid,
    lava_grid: &mut LavaGrid,
) -> CommandResult {
    let air_data = BlockData::from(BlockType::Air);
    let placed_data = block_data_for_params(params);
    let mut changed_blocks: Vec<(Vector3<i32>, BlockData)> =
        Vec::with_capacity(shell_positions.len() + interior_positions.len());
    let mut count = 0u64;

    if block == BlockType::Air {
        for pos in shell_positions {
            world.set_block(*pos, BlockType::Air);
            changed_blocks.push((*pos, air_data.clone()));
            count += 1;
        }
    } else {
        count +=
            place_blocks_at_positions(shell_positions, params, world, water_grid, lava_grid) as u64;
        if let Some(data) = placed_data {
            for pos in shell_positions {
                changed_blocks.push((*pos, data.clone()));
            }
        }
    }

    // Clear interior for hollow (does not increment count, matching old behavior).
    for pos in interior_positions {
        world.set_block(*pos, BlockType::Air);
        changed_blocks.push((*pos, air_data.clone()));
    }

    let hollow_str = if hollow { " hollow" } else { "" };
    let dome_str = if dome { " dome" } else { "" };
    CommandResult::success_with_blocks(
        format!(
            "Created{}{} sphere of {} blocks with {:?}",
            hollow_str, dome_str, count, block
        ),
        changed_blocks,
    )
}

/// Parse a metadata arg (tint/paint) as a `u8`.
fn parse_u8_meta(s: &str, label: &str) -> Result<u8, String> {
    s.parse::<u8>()
        .map_err(|_| format!("Invalid {} value: '{}'", label, s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sphere_volume_estimate() {
        let radius = 5.0_f64;
        let volume = (4.0 / 3.0) * std::f64::consts::PI * radius.powi(3);
        assert!((volume - 523.6).abs() < 1.0);
    }

    #[test]
    fn test_hollow_sphere_shell_volume() {
        let outer = (4.0 / 3.0) * std::f64::consts::PI * 5.0_f64.powi(3);
        let inner = (4.0 / 3.0) * std::f64::consts::PI * 4.0_f64.powi(3);
        let shell = outer - inner;
        assert!((shell - 255.5).abs() < 1.0);
    }

    #[test]
    fn test_distance_squared() {
        let dx: i64 = 3;
        let dy: i64 = 4;
        let dz: i64 = 0;
        let dist_sq = dx * dx + dy * dy + dz * dz;
        assert_eq!(dist_sq, 25);
    }

    /// `/sphere tintedglass <tint> ...` parses the leading tint arg.
    #[test]
    fn test_sphere_tintedglass_with_tint_arg() {
        let mut world = World::new();
        let mut water = crate::water::WaterGrid::new();
        let mut lava = crate::lava::LavaGrid::new();
        let args: Vec<&str> = vec!["tintedglass", "5", "0", "40", "0", "2"];
        let result = sphere(
            &args,
            &mut world,
            &mut water,
            &mut lava,
            Vector3::new(0, 40, 0),
            false,
        );
        let blocks = match result {
            CommandResult::Success { changed_blocks, .. } => changed_blocks,
            CommandResult::Error(e) => panic!("expected success, got Error: {}", e),
            _ => panic!("expected success, got non-Success CommandResult"),
        };
        assert!(
            blocks
                .iter()
                .all(|(_, bd)| bd.block_type == BlockType::TintedGlass && bd.tint_index == Some(5))
        );
    }

    /// Too few args returns usage error.
    #[test]
    fn test_sphere_too_few_args() {
        let mut world = World::new();
        let mut water = crate::water::WaterGrid::new();
        let mut lava = crate::lava::LavaGrid::new();
        let args: Vec<&str> = vec!["stone", "0", "40"];
        let result = sphere(
            &args,
            &mut world,
            &mut water,
            &mut lava,
            Vector3::zeros(),
            false,
        );
        assert!(matches!(result, CommandResult::Error(_)));
    }
}

//! Copy command implementation.
//!
//! Copies a rectangular region from one location to another with optional rotation.

use crate::chunk::{BlockModelData, BlockPaintData, BlockType, WaterType};
use crate::console::{
    CommandResult, parse_coordinate, validate_y_bounds, volume_confirm_threshold,
};
use crate::world::World;
use nalgebra::Vector3;

/// Execute the copy command.
///
/// Syntax: copy <x1> <y1> <z1> <x2> <y2> <z2> <dx> <dy> <dz> [rotate_90|rotate_180|rotate_270]
pub fn copy(
    args: &[&str],
    world: &mut World,
    player_pos: Vector3<i32>,
    confirmed: bool,
) -> CommandResult {
    // Parse arguments
    if args.len() < 9 {
        return CommandResult::Error(
            "Usage: copy <x1> <y1> <z1> <x2> <y2> <z2> <dx> <dy> <dz> [rotate_90|rotate_180|rotate_270]"
                .to_string(),
        );
    }

    // Parse source coordinates
    let x1 = match parse_coordinate(args[0], player_pos.x) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let y1 = match parse_coordinate(args[1], player_pos.y) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let z1 = match parse_coordinate(args[2], player_pos.z) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let x2 = match parse_coordinate(args[3], player_pos.x) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let y2 = match parse_coordinate(args[4], player_pos.y) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let z2 = match parse_coordinate(args[5], player_pos.z) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };

    // Parse destination coordinates
    let dx = match parse_coordinate(args[6], player_pos.x) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let dy = match parse_coordinate(args[7], player_pos.y) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let dz = match parse_coordinate(args[8], player_pos.z) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };

    // Parse optional rotation
    let rotation = if args.len() > 9 {
        match args[9].to_lowercase().as_str() {
            "rotate_90" => 1,
            "rotate_180" => 2,
            "rotate_270" => 3,
            _ => {
                return CommandResult::Error(format!(
                    "Invalid rotation flag: '{}'. Valid options: rotate_90, rotate_180, rotate_270",
                    args[9]
                ));
            }
        }
    } else {
        0 // No rotation
    };

    // Normalize source coordinates (min/max)
    let min_x = x1.min(x2);
    let max_x = x1.max(x2);
    let min_y = y1.min(y2);
    let max_y = y1.max(y2);
    let min_z = z1.min(z2);
    let max_z = z1.max(z2);

    // Validate Y bounds for source
    if let Some(error) = validate_y_bounds(min_y) {
        return CommandResult::Error(error);
    }
    if let Some(error) = validate_y_bounds(max_y) {
        return CommandResult::Error(error);
    }

    // Calculate dimensions
    let width = (max_x - min_x + 1) as u64;
    let height = (max_y - min_y + 1) as u64;
    let depth = (max_z - min_z + 1) as u64;
    let volume = width * height * depth;

    // Check volume threshold
    if !confirmed && volume > volume_confirm_threshold() {
        let original_cmd = args.join(" ");
        return CommandResult::NeedsConfirmation {
            message: format!(
                "Copy {} blocks? This is a large operation.\nType '/confirm' to proceed, or '/{}' to cancel.",
                volume, original_cmd
            ),
            command: format!("copy {}", original_cmd),
        };
    }

    // Validate destination Y bounds
    let dest_min_y = dy;
    let dest_max_y = dy + (max_y - min_y);
    if let Some(error) = validate_y_bounds(dest_min_y) {
        return CommandResult::Error(format!("Destination {}", error));
    }
    if let Some(error) = validate_y_bounds(dest_max_y) {
        return CommandResult::Error(format!("Destination {}", error));
    }

    // Copy blocks
    let mut copied_count = 0;

    // Read all blocks from source region first
    let mut blocks = Vec::new();
    for y in min_y..=max_y {
        for z in min_z..=max_z {
            for x in min_x..=max_x {
                let pos = Vector3::new(x, y, z);

                // Get block type, default to Air if chunk doesn't exist
                let block_type = world.get_block(pos).unwrap_or(BlockType::Air);

                // Store block data
                let local_x = (x - min_x) as u8;
                let local_y = (y - min_y) as u8;
                let local_z = (z - min_z) as u8;

                blocks.push(BlockData {
                    local_pos: (local_x, local_y, local_z),
                    block_type,
                    model_data: if block_type == BlockType::Model {
                        world.get_model_data(pos)
                    } else {
                        None
                    },
                    tint_data: if block_type == BlockType::TintedGlass
                        || block_type == BlockType::Crystal
                    {
                        world.get_tint_index(pos)
                    } else {
                        None
                    },
                    paint_data: if block_type == BlockType::Painted {
                        world.get_paint_data(pos)
                    } else {
                        None
                    },
                    water_data: if block_type == BlockType::Water {
                        world.get_water_type(pos)
                    } else {
                        None
                    },
                });
            }
        }
    }

    // Apply rotation and place blocks at destination
    let w = max_x - min_x + 1;
    let h = max_y - min_y + 1;
    let d = max_z - min_z + 1;

    let mut changed_blocks = Vec::new();

    for block_data in blocks {
        let (x, y, z) = block_data.local_pos;

        // Apply rotation to position
        let dest_offset = apply_rotation(x, y, z, w, h, d, rotation);
        let dest_pos = Vector3::new(dx, dy, dz) + dest_offset;

        // Place block
        world.set_block(dest_pos, block_data.block_type);
        changed_blocks.push((dest_pos, block_data.block_type));
        copied_count += 1;

        // Place metadata
        if let Some(model_data) = block_data.model_data {
            // Apply rotation to model rotation
            let final_rotation = (model_data.rotation + rotation) % 4;
            world.set_model_block(
                dest_pos,
                model_data.model_id,
                final_rotation,
                model_data.waterlogged,
            );
        }

        if let Some(tint_index) = block_data.tint_data {
            if block_data.block_type == BlockType::TintedGlass {
                world.set_tinted_glass_block(dest_pos, tint_index);
            } else if block_data.block_type == BlockType::Crystal {
                world.set_crystal_block(dest_pos, tint_index);
            }
        }

        if let Some(paint_data) = block_data.paint_data {
            world.set_painted_block(dest_pos, paint_data.texture_idx, paint_data.tint_idx);
        }

        if let Some(water_type) = block_data.water_data {
            world.set_water_block(dest_pos, water_type);
        }
    }

    let rotation_str = match rotation {
        1 => " (rotated 90°)",
        2 => " (rotated 180°)",
        3 => " (rotated 270°)",
        _ => "",
    };

    CommandResult::success_with_blocks(
        format!(
            "Copied {} blocks from ({},{},{}) to ({},{},{}) to ({},{},{}){}",
            copied_count, min_x, min_y, min_z, max_x, max_y, max_z, dx, dy, dz, rotation_str
        ),
        changed_blocks,
    )
}

/// Helper struct to store block data during copy.
struct BlockData {
    local_pos: (u8, u8, u8),
    block_type: BlockType,
    model_data: Option<BlockModelData>,
    tint_data: Option<u8>,
    paint_data: Option<BlockPaintData>,
    water_data: Option<WaterType>,
}

/// Apply Y-axis rotation to a position (same logic as template placement).
fn apply_rotation(x: u8, y: u8, z: u8, w: i32, _h: i32, d: i32, rotation: u8) -> Vector3<i32> {
    // Calculate center of rotation
    let cx = w / 2;
    let cz = d / 2;

    // Position relative to center
    let rx = x as i32 - cx;
    let rz = z as i32 - cz;

    // Apply Y-axis rotation (clockwise when viewed from above)
    let (tx, tz) = match rotation {
        0 => (rx, rz),   // 0°
        1 => (rz, -rx),  // 90° clockwise
        2 => (-rx, -rz), // 180°
        3 => (-rz, rx),  // 270° clockwise
        _ => (rx, rz),   // Invalid, default to 0°
    };

    // Convert back to offset from destination origin
    Vector3::new(tx + cx, y as i32, tz + cz)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_rotation_0() {
        // 3x3x3 region, no rotation
        let result = apply_rotation(0, 0, 0, 3, 3, 3, 0);
        assert_eq!(result, Vector3::new(0, 0, 0));

        let result = apply_rotation(2, 1, 2, 3, 3, 3, 0);
        assert_eq!(result, Vector3::new(2, 1, 2));
    }

    #[test]
    fn test_apply_rotation_90() {
        // 3x3x3 region, 90° rotation
        // Center is at (1, 1)
        // Corner (0, 0) relative to center is (-1, -1)
        // After 90° clockwise: (z, -x) = (-1, 1)
        // Back to coords: (1 + (-1), 1 + 1) = (0, 2)
        let result = apply_rotation(0, 0, 0, 3, 3, 3, 1);
        assert_eq!(result.x, 0);
        assert_eq!(result.z, 2);
    }

    #[test]
    fn test_apply_rotation_180() {
        // 3x3x3 region, 180° rotation
        let result = apply_rotation(0, 0, 0, 3, 3, 3, 2);
        assert_eq!(result.x, 2);
        assert_eq!(result.z, 2);
    }

    #[test]
    fn test_apply_rotation_270() {
        // 3x3x3 region, 270° rotation
        let result = apply_rotation(0, 0, 0, 3, 3, 3, 3);
        assert_eq!(result.x, 2);
        assert_eq!(result.z, 0);
    }
}

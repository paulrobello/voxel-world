//! Flood fill command implementation.
//!
//! Fills a connected region of matching blocks with the specified block type.
//! Uses BFS algorithm with smart block matching for painted blocks, water types,
//! and tinted blocks.

use crate::chunk::{BlockType, WaterType};
use crate::console::{CommandResult, parse_coordinate, validate_y_bounds};
use crate::world::World;
use nalgebra::Vector3;
use std::collections::{HashSet, VecDeque};

/// Confirmation threshold for flood fill operations.
/// Operations affecting more than this many blocks will require confirmation.
const FLOODFILL_CONFIRM_THRESHOLD: u64 = 10_000;

/// Hard limit for flood fill operations.
/// Operations affecting more than this many blocks will be rejected.
const FLOODFILL_MAX_BLOCKS: usize = 1_000_000;

/// Neighbor offsets for 6-connected flood fill (cardinal directions only).
const NEIGHBOR_OFFSETS: [(i32, i32, i32); 6] = [
    (1, 0, 0),
    (-1, 0, 0),
    (0, 1, 0),
    (0, -1, 0),
    (0, 0, 1),
    (0, 0, -1),
];

/// Block identity for matching purposes.
/// This captures all the properties that make a block "the same" as another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum BlockIdentity {
    /// Simple block type (not special).
    Simple(BlockType),
    /// Painted block with specific texture and tint.
    Painted { texture_idx: u8, tint_idx: u8 },
    /// Tinted glass with specific tint.
    TintedGlass { tint_idx: u8 },
    /// Crystal with specific tint.
    Crystal { tint_idx: u8 },
    /// Water with specific type.
    Water { water_type: WaterType },
    /// Model block (we don't fill these to prevent accidental overwrites).
    Model,
}

impl BlockIdentity {
    /// Get the block identity at a world position.
    fn at_position(world: &World, pos: Vector3<i32>) -> Option<Self> {
        let block = world.get_block(pos)?;
        Some(Self::from_block(world, pos, block))
    }

    /// Create identity from block type and world metadata.
    fn from_block(world: &World, pos: Vector3<i32>, block: BlockType) -> Self {
        match block {
            BlockType::Painted => {
                if let Some(paint_data) = world.get_paint_data(pos) {
                    BlockIdentity::Painted {
                        texture_idx: paint_data.texture_idx,
                        tint_idx: paint_data.tint_idx,
                    }
                } else {
                    // Fallback if no paint data
                    BlockIdentity::Painted {
                        texture_idx: 0,
                        tint_idx: 0,
                    }
                }
            }
            BlockType::TintedGlass => {
                let tint_idx = world.get_tint_index(pos).unwrap_or(0);
                BlockIdentity::TintedGlass { tint_idx }
            }
            BlockType::Crystal => {
                let tint_idx = world.get_tint_index(pos).unwrap_or(0);
                BlockIdentity::Crystal { tint_idx }
            }
            BlockType::Water => {
                let water_type = world.get_water_type(pos).unwrap_or(WaterType::Ocean);
                BlockIdentity::Water { water_type }
            }
            BlockType::Model => BlockIdentity::Model,
            other => BlockIdentity::Simple(other),
        }
    }
}

/// Execute the floodfill command.
///
/// Syntax: floodfill <target_block> [x] [y] [z]
///
/// If coordinates are omitted, uses the block at player's crosshair (raycast hit).
/// This is a pre-scan only - the actual filling will be done by the caller
/// using frame-distributed updates.
pub fn floodfill(
    args: &[&str],
    world: &mut World,
    player_pos: Vector3<i32>,
    raycast_hit: Option<Vector3<i32>>,
    confirmed: bool,
) -> CommandResult {
    // Parse arguments
    if args.is_empty() {
        return CommandResult::Error(
            "Usage: floodfill <target_block> [x] [y] [z]\n\
             If coordinates omitted, uses crosshair target."
                .to_string(),
        );
    }

    // Parse target block name
    let block_name = args[0].to_lowercase();
    let target_block = match BlockType::from_name(&block_name) {
        Some(b) => b,
        None => {
            return CommandResult::Error(format!(
                "Unknown block type: '{}'. Valid types: air, stone, dirt, grass, planks, leaves, sand, gravel, water, glass, log, brick, snow, cobblestone, iron, bedrock, etc.",
                block_name
            ));
        }
    };

    // Get start position
    let start_pos = if args.len() >= 4 {
        // Explicit coordinates provided
        let x = match parse_coordinate(args[1], player_pos.x) {
            Ok(v) => v,
            Err(e) => return CommandResult::Error(e),
        };
        let y = match parse_coordinate(args[2], player_pos.y) {
            Ok(v) => v,
            Err(e) => return CommandResult::Error(e),
        };
        let z = match parse_coordinate(args[3], player_pos.z) {
            Ok(v) => v,
            Err(e) => return CommandResult::Error(e),
        };

        // Validate Y bounds
        if let Some(error) = validate_y_bounds(y) {
            return CommandResult::Error(error);
        }

        Vector3::new(x, y, z)
    } else {
        // Use raycast hit position
        match raycast_hit {
            Some(pos) => pos,
            None => {
                return CommandResult::Error(
                    "No block targeted. Aim at a block or provide coordinates.".to_string(),
                );
            }
        }
    };

    // Get source block identity
    let source_identity = match BlockIdentity::at_position(world, start_pos) {
        Some(id) => id,
        None => {
            return CommandResult::Error(format!(
                "No block at position ({}, {}, {})",
                start_pos.x, start_pos.y, start_pos.z
            ));
        }
    };

    // Don't fill model blocks
    if matches!(source_identity, BlockIdentity::Model) {
        return CommandResult::Error(
            "Cannot flood fill model blocks (doors, fences, etc.). \
             Remove them manually to prevent accidental overwrites."
                .to_string(),
        );
    }

    // Don't fill with model blocks
    if target_block == BlockType::Model {
        return CommandResult::Error(
            "Cannot flood fill with Model block type. \
             Place models individually."
                .to_string(),
        );
    }

    // Check if target is same as source
    let source_block = world.get_block(start_pos).unwrap_or(BlockType::Air);
    if source_block == target_block
        && !matches!(
            source_block,
            BlockType::Painted | BlockType::TintedGlass | BlockType::Crystal | BlockType::Water
        )
    {
        return CommandResult::Error(
            "Target block is the same as source block. Nothing to fill.".to_string(),
        );
    }

    // Pre-scan to count affected blocks
    let affected = scan_flood_fill_region(world, start_pos, &source_identity);

    if affected.is_empty() {
        return CommandResult::Error("No blocks to fill at this location.".to_string());
    }

    // Check hard limit
    if affected.len() > FLOODFILL_MAX_BLOCKS {
        return CommandResult::Error(format!(
            "Flood fill region too large: {} blocks (max {} blocks). \
             Try a smaller area or use the fill command for large regions.",
            affected.len(),
            FLOODFILL_MAX_BLOCKS
        ));
    }

    // Check confirmation threshold
    if !confirmed && affected.len() as u64 > FLOODFILL_CONFIRM_THRESHOLD {
        let original_cmd = args.join(" ");
        return CommandResult::NeedsConfirmation {
            message: format!("This will replace {} blocks. Are you sure?", affected.len()),
            command: format!("floodfill {}", original_cmd),
        };
    }

    // Execute the fill
    let mut count = 0u64;
    for pos in &affected {
        world.set_block(*pos, target_block);
        count += 1;
    }

    CommandResult::Success(format!(
        "Flood filled {} blocks with {:?}",
        count, target_block
    ))
}

/// Pre-scan the flood fill region to count affected blocks.
/// Uses BFS with 6-connected neighbors.
fn scan_flood_fill_region(
    world: &World,
    start: Vector3<i32>,
    source_identity: &BlockIdentity,
) -> Vec<Vector3<i32>> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let mut affected = Vec::new();

    queue.push_back(start);
    visited.insert(start);

    while let Some(pos) = queue.pop_front() {
        // Early exit if we've hit the hard limit
        if affected.len() >= FLOODFILL_MAX_BLOCKS {
            break;
        }

        // Get identity at this position
        let identity = match BlockIdentity::at_position(world, pos) {
            Some(id) => id,
            None => continue,
        };

        // Check if this block matches the source
        if &identity != source_identity {
            continue;
        }

        // This block is part of the fill region
        affected.push(pos);

        // Add unvisited neighbors to queue
        for (dx, dy, dz) in NEIGHBOR_OFFSETS {
            let neighbor = Vector3::new(pos.x + dx, pos.y + dy, pos.z + dz);

            // Skip if already visited
            if visited.contains(&neighbor) {
                continue;
            }

            // Skip if Y is out of bounds
            if neighbor.y < 0 || validate_y_bounds(neighbor.y).is_some() {
                continue;
            }

            visited.insert(neighbor);
            queue.push_back(neighbor);
        }
    }

    affected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_identity_simple() {
        // Simple blocks should have Simple identity
        assert!(matches!(
            BlockIdentity::Simple(BlockType::Stone),
            BlockIdentity::Simple(BlockType::Stone)
        ));

        // Different simple blocks are not equal
        assert_ne!(
            BlockIdentity::Simple(BlockType::Stone),
            BlockIdentity::Simple(BlockType::Dirt)
        );
    }

    #[test]
    fn test_block_identity_painted() {
        // Same painted blocks are equal
        assert_eq!(
            BlockIdentity::Painted {
                texture_idx: 1,
                tint_idx: 5
            },
            BlockIdentity::Painted {
                texture_idx: 1,
                tint_idx: 5
            }
        );

        // Different texture = not equal
        assert_ne!(
            BlockIdentity::Painted {
                texture_idx: 1,
                tint_idx: 5
            },
            BlockIdentity::Painted {
                texture_idx: 2,
                tint_idx: 5
            }
        );

        // Different tint = not equal
        assert_ne!(
            BlockIdentity::Painted {
                texture_idx: 1,
                tint_idx: 5
            },
            BlockIdentity::Painted {
                texture_idx: 1,
                tint_idx: 6
            }
        );
    }

    #[test]
    fn test_block_identity_water() {
        // Same water type = equal
        assert_eq!(
            BlockIdentity::Water {
                water_type: WaterType::Ocean
            },
            BlockIdentity::Water {
                water_type: WaterType::Ocean
            }
        );

        // Different water type = not equal
        assert_ne!(
            BlockIdentity::Water {
                water_type: WaterType::Ocean
            },
            BlockIdentity::Water {
                water_type: WaterType::Swamp
            }
        );
    }
}

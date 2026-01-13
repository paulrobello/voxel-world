//! Block replacement algorithm for the replace tool.
//!
//! This module provides functions to find and replace blocks within
//! a selection region.

use nalgebra::Vector3;

use crate::chunk::BlockType;
use crate::templates::TemplateSelection;
use crate::world::World;

/// Represents a block identity for matching during replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum BlockIdentity {
    /// Match a specific block type (ignores metadata).
    Type(BlockType),
    /// Match any non-air block.
    AnyNonAir,
    /// Match painted blocks with specific texture and tint.
    Painted { texture: u8, tint: u8 },
    /// Match tinted glass with specific tint.
    TintedGlass { tint: u8 },
    /// Match crystal with specific tint.
    Crystal { tint: u8 },
}

impl BlockIdentity {
    /// Create a BlockIdentity from a block type and optional metadata.
    #[allow(dead_code)]
    pub fn from_block_type(block_type: BlockType, metadata: Option<(u8, u8)>) -> Self {
        match block_type {
            BlockType::Painted => {
                if let Some((texture, tint)) = metadata {
                    BlockIdentity::Painted { texture, tint }
                } else {
                    BlockIdentity::Type(BlockType::Painted)
                }
            }
            BlockType::TintedGlass => {
                if let Some((_, tint)) = metadata {
                    BlockIdentity::TintedGlass { tint }
                } else {
                    BlockIdentity::Type(BlockType::TintedGlass)
                }
            }
            BlockType::Crystal => {
                if let Some((_, tint)) = metadata {
                    BlockIdentity::Crystal { tint }
                } else {
                    BlockIdentity::Type(BlockType::Crystal)
                }
            }
            _ => BlockIdentity::Type(block_type),
        }
    }

    /// Check if a position in the world matches this identity.
    pub fn matches(&self, world: &World, pos: Vector3<i32>) -> bool {
        let block_type = world.get_block(pos);

        match self {
            BlockIdentity::Type(target) => block_type == Some(*target),
            BlockIdentity::AnyNonAir => block_type.is_some_and(|bt| bt != BlockType::Air),
            BlockIdentity::Painted { texture, tint } => {
                if block_type != Some(BlockType::Painted) {
                    return false;
                }
                if let Some(paint_data) = world.get_paint_data(pos) {
                    paint_data.texture_idx == *texture && paint_data.tint_idx == *tint
                } else {
                    false
                }
            }
            BlockIdentity::TintedGlass { tint } => {
                if block_type != Some(BlockType::TintedGlass) {
                    return false;
                }
                world.get_tint_index(pos) == Some(*tint)
            }
            BlockIdentity::Crystal { tint } => {
                if block_type != Some(BlockType::Crystal) {
                    return false;
                }
                world.get_tint_index(pos) == Some(*tint)
            }
        }
    }
}

/// Count matching blocks within a selection.
///
/// # Arguments
/// * `world` - World to search in
/// * `selection` - Region to search within
/// * `source` - Block identity to match
///
/// # Returns
/// Number of matching blocks found
pub fn count_matching_blocks(
    world: &World,
    selection: &TemplateSelection,
    source: &BlockIdentity,
) -> usize {
    if selection.pos1.is_none() || selection.pos2.is_none() {
        return 0;
    }

    let (min, max) = selection.bounds().unwrap();
    let mut count = 0;

    for x in min.x..=max.x {
        for y in min.y..=max.y {
            for z in min.z..=max.z {
                let pos = Vector3::new(x, y, z);
                if source.matches(world, pos) {
                    count += 1;
                }
            }
        }
    }

    count
}

/// Find all positions matching a block identity within a selection.
///
/// # Arguments
/// * `world` - World to search in
/// * `selection` - Region to search within
/// * `source` - Block identity to match
/// * `limit` - Maximum positions to return (0 = no limit)
///
/// # Returns
/// Vector of matching positions (may be truncated if limit > 0)
pub fn find_matching_blocks(
    world: &World,
    selection: &TemplateSelection,
    source: &BlockIdentity,
    limit: usize,
) -> Vec<Vector3<i32>> {
    if selection.pos1.is_none() || selection.pos2.is_none() {
        return Vec::new();
    }

    let (min, max) = selection.bounds().unwrap();
    let mut positions = Vec::new();

    for x in min.x..=max.x {
        for y in min.y..=max.y {
            for z in min.z..=max.z {
                if limit > 0 && positions.len() >= limit {
                    return positions;
                }
                let pos = Vector3::new(x, y, z);
                if source.matches(world, pos) {
                    positions.push(pos);
                }
            }
        }
    }

    positions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_identity_type() {
        let id = BlockIdentity::from_block_type(BlockType::Stone, None);
        assert_eq!(id, BlockIdentity::Type(BlockType::Stone));
    }

    #[test]
    fn test_block_identity_painted() {
        let id = BlockIdentity::from_block_type(BlockType::Painted, Some((5, 10)));
        assert_eq!(
            id,
            BlockIdentity::Painted {
                texture: 5,
                tint: 10
            }
        );
    }

    #[test]
    fn test_block_identity_tinted_glass() {
        let id = BlockIdentity::from_block_type(BlockType::TintedGlass, Some((0, 15)));
        assert_eq!(id, BlockIdentity::TintedGlass { tint: 15 });
    }

    #[test]
    fn test_block_identity_crystal() {
        let id = BlockIdentity::from_block_type(BlockType::Crystal, Some((0, 20)));
        assert_eq!(id, BlockIdentity::Crystal { tint: 20 });
    }
}

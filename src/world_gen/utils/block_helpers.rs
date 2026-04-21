//! Helper functions for safe block access during generation.

use crate::chunk::{BlockType, CHUNK_SIZE, Chunk};
use nalgebra::Vector3;

use super::OverflowBlock;

/// Safely get a block within chunk bounds.
///
/// Returns `None` if coordinates are outside the chunk.
pub fn get_block_safe(chunk: &Chunk, x: i32, y: i32, z: i32) -> Option<BlockType> {
    if x >= 0
        && x < CHUNK_SIZE as i32
        && y >= 0
        && y < CHUNK_SIZE as i32
        && z >= 0
        && z < CHUNK_SIZE as i32
    {
        Some(chunk.get_block(x as usize, y as usize, z as usize))
    } else {
        None
    }
}

/// Safely set a block, adding to overflow if outside chunk bounds.
///
/// # Arguments
/// * `chunk` - The chunk to modify
/// * `x`, `y`, `z` - Local coordinates within chunk
/// * `block` - Block type to place
/// * `chunk_world_x/y/z` - World coordinates of chunk origin
/// * `overflow_blocks` - Collection for out-of-bounds blocks
#[allow(clippy::too_many_arguments)]
pub fn set_block_safe(
    chunk: &mut Chunk,
    x: i32,
    y: i32,
    z: i32,
    block: BlockType,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    if x >= 0
        && x < CHUNK_SIZE as i32
        && y >= 0
        && y < CHUNK_SIZE as i32
        && z >= 0
        && z < CHUNK_SIZE as i32
    {
        // Within chunk bounds - place directly
        let existing = chunk.get_block(x as usize, y as usize, z as usize);
        // Tree structure (logs/leaves) can replace surface terrain for proper generation
        let can_replace = existing == BlockType::Air
            || existing.is_transparent()
            || (block.is_tree_structure() && existing.is_replaceable_terrain());
        if can_replace {
            chunk.set_block(x as usize, y as usize, z as usize, block);
        }
    } else {
        // Out of bounds - add to overflow for neighboring chunk
        let world_pos = Vector3::new(chunk_world_x + x, chunk_world_y + y, chunk_world_z + z);
        overflow_blocks.push(OverflowBlock {
            world_pos,
            block_type: block,
        });
    }
}

//! Overflow block system for cross-chunk feature generation.
//!
//! When trees or other features extend beyond chunk boundaries,
//! overflow blocks are collected and applied to neighboring chunks.

use crate::chunk::{BlockType, Chunk};
use nalgebra::Vector3;

/// Represents a block that should be placed outside the current chunk.
#[derive(Clone, Debug)]
pub struct OverflowBlock {
    pub world_pos: Vector3<i32>,
    pub block_type: BlockType,
}

/// Result of chunk terrain generation including overflow blocks.
pub struct ChunkGenerationResult {
    pub chunk: Chunk,
    pub overflow_blocks: Vec<OverflowBlock>,
}

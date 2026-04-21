//! Utility types and functions for world generation.

mod block_helpers;
mod overflow;

pub use block_helpers::{get_block_safe, set_block_safe};
pub use overflow::{ChunkGenerationResult, OverflowBlock};

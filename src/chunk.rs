//! Chunk data structure for voxel storage.
//!
//! Each chunk is a 32³ grid of blocks. Blocks are stored as u8 values
//! where 0 = air and other values represent different block types.

#![allow(dead_code)]

use std::sync::Arc;
use vulkano::image::view::ImageView;

/// Size of a chunk in each dimension (32³ = 32,768 blocks per chunk).
pub const CHUNK_SIZE: usize = 32;

/// Total number of blocks in a chunk.
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

/// Block types that can exist in the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum BlockType {
    #[default]
    Air = 0,
    Stone = 1,
    Dirt = 2,
    Grass = 3,
    Planks = 4,
    Leaves = 5,
    Sand = 6,
    Gravel = 7,
    Water = 8,
    Glass = 9,
    Log = 10,
    Torch = 11,
    Brick = 12,
    Snow = 13,
    Cobblestone = 14,
    Iron = 15,
}

impl BlockType {
    /// Returns true if this block type is solid (not air, water, or torch).
    #[inline]
    pub fn is_solid(self) -> bool {
        !matches!(self, BlockType::Air | BlockType::Water | BlockType::Torch)
    }

    /// Returns true if this block type is affected by gravity (sand, gravel).
    #[inline]
    pub fn is_affected_by_gravity(self) -> bool {
        matches!(self, BlockType::Sand | BlockType::Gravel)
    }

    /// Returns true if this block type is transparent.
    #[inline]
    pub fn is_transparent(self) -> bool {
        matches!(
            self,
            BlockType::Air
                | BlockType::Water
                | BlockType::Glass
                | BlockType::Leaves
                | BlockType::Torch
        )
    }

    /// Returns true if this block type emits light.
    #[inline]
    pub fn is_light_source(self) -> bool {
        matches!(self, BlockType::Torch)
    }

    /// Returns the light color and intensity for light-emitting blocks.
    /// Returns (color RGB, intensity) or None if not a light source.
    #[inline]
    pub fn light_properties(self) -> Option<([f32; 3], f32)> {
        match self {
            BlockType::Torch => Some(([1.0, 0.8, 0.4], 8.0)), // Warm orange light, radius ~8 blocks
            _ => None,
        }
    }

    /// Returns the color for this block type (RGB, 0-1 range).
    #[inline]
    pub fn color(self) -> [f32; 3] {
        match self {
            BlockType::Air => [0.0, 0.0, 0.0],
            BlockType::Stone => [0.5, 0.5, 0.5],
            BlockType::Dirt => [0.6, 0.4, 0.2],
            BlockType::Grass => [0.3, 0.7, 0.2],
            BlockType::Planks => [0.6, 0.4, 0.2],
            BlockType::Leaves => [0.2, 0.6, 0.1],
            BlockType::Sand => [0.9, 0.8, 0.5],
            BlockType::Gravel => [0.4, 0.4, 0.4],
            BlockType::Water => [0.2, 0.4, 0.8],
            BlockType::Glass => [0.8, 0.9, 1.0],
            BlockType::Log => [0.4, 0.3, 0.2],
            BlockType::Torch => [0.9, 0.7, 0.3], // Warm torch color
            BlockType::Brick => [0.7, 0.35, 0.3],
            BlockType::Snow => [0.95, 0.95, 0.98],
            BlockType::Cobblestone => [0.45, 0.45, 0.45],
            BlockType::Iron => [0.75, 0.75, 0.78],
        }
    }

    /// Returns the time in seconds to break this block type.
    /// Higher values = takes longer to break.
    #[inline]
    pub fn break_time(self) -> f32 {
        match self {
            BlockType::Air => 0.0,
            // Very fast (instant)
            BlockType::Leaves | BlockType::Torch => 0.15,
            // Fast
            BlockType::Dirt | BlockType::Sand | BlockType::Gravel | BlockType::Snow => 0.3,
            // Normal
            BlockType::Grass | BlockType::Planks | BlockType::Log | BlockType::Glass => 0.5,
            // Slow
            BlockType::Stone | BlockType::Cobblestone | BlockType::Brick => 0.8,
            // Very slow
            BlockType::Iron => 1.2,
            // Special (can't break or shouldn't)
            BlockType::Water => 0.0,
        }
    }
}

impl From<u8> for BlockType {
    fn from(value: u8) -> Self {
        match value {
            0 => BlockType::Air,
            1 => BlockType::Stone,
            2 => BlockType::Dirt,
            3 => BlockType::Grass,
            4 => BlockType::Planks,
            5 => BlockType::Leaves,
            6 => BlockType::Sand,
            7 => BlockType::Gravel,
            8 => BlockType::Water,
            9 => BlockType::Glass,
            10 => BlockType::Log,
            11 => BlockType::Torch,
            12 => BlockType::Brick,
            13 => BlockType::Snow,
            14 => BlockType::Cobblestone,
            15 => BlockType::Iron,
            _ => BlockType::Air,
        }
    }
}

/// A chunk of blocks in the voxel world.
///
/// Chunks are 32³ grids of blocks that can be individually loaded,
/// modified, and uploaded to the GPU.
pub struct Chunk {
    /// Block data stored as a flat array.
    /// Index = x + y * CHUNK_SIZE + z * CHUNK_SIZE * CHUNK_SIZE
    blocks: Box<[BlockType; CHUNK_VOLUME]>,

    /// Whether this chunk has been modified since last GPU upload.
    pub dirty: bool,

    /// Cached GPU texture for this chunk (if uploaded).
    pub gpu_texture: Option<Arc<ImageView>>,

    /// Cached: true if all blocks are air (for ray skip optimization).
    cached_is_empty: bool,

    /// Cached: true if all blocks are solid (for ray skip optimization).
    cached_is_fully_solid: bool,

    /// Whether cached_is_empty/cached_is_fully_solid need recalculation.
    metadata_dirty: bool,
}

impl Default for Chunk {
    fn default() -> Self {
        Self::new()
    }
}

impl Chunk {
    /// Creates a new empty chunk (all air).
    pub fn new() -> Self {
        Self {
            blocks: Box::new([BlockType::Air; CHUNK_VOLUME]),
            dirty: true,
            gpu_texture: None,
            cached_is_empty: true,
            cached_is_fully_solid: false,
            metadata_dirty: false,
        }
    }

    /// Creates a chunk filled with a single block type.
    pub fn filled(block_type: BlockType) -> Self {
        let is_empty = block_type == BlockType::Air;
        let is_solid = block_type.is_solid();
        Self {
            blocks: Box::new([block_type; CHUNK_VOLUME]),
            dirty: true,
            gpu_texture: None,
            cached_is_empty: is_empty,
            cached_is_fully_solid: is_solid,
            metadata_dirty: false,
        }
    }

    /// Converts local coordinates to a flat array index.
    #[inline]
    fn index(x: usize, y: usize, z: usize) -> usize {
        debug_assert!(x < CHUNK_SIZE && y < CHUNK_SIZE && z < CHUNK_SIZE);
        x + y * CHUNK_SIZE + z * CHUNK_SIZE * CHUNK_SIZE
    }

    /// Gets the block at the given local coordinates.
    #[inline]
    pub fn get_block(&self, x: usize, y: usize, z: usize) -> BlockType {
        self.blocks[Self::index(x, y, z)]
    }

    /// Sets the block at the given local coordinates.
    #[inline]
    pub fn set_block(&mut self, x: usize, y: usize, z: usize, block: BlockType) {
        let idx = Self::index(x, y, z);
        if self.blocks[idx] != block {
            self.blocks[idx] = block;
            self.dirty = true;
            self.metadata_dirty = true;
        }
    }

    /// Checks if a block is solid at the given local coordinates.
    #[inline]
    pub fn is_solid(&self, x: usize, y: usize, z: usize) -> bool {
        self.get_block(x, y, z).is_solid()
    }

    /// Converts the chunk to bit-packed format for GPU upload.
    ///
    /// This matches the format expected by the current traverse.comp shader:
    /// - Each u128 represents a 4×4×8 block of voxels
    /// - Each bit indicates whether a block is solid (1) or air (0)
    ///
    /// The output size is CHUNK_SIZE³ / 128 = 256 u128 values for a 32³ chunk.
    pub fn to_bit_packed(&self) -> Vec<u128> {
        let packed_size = CHUNK_VOLUME / 128;
        let mut packed = vec![0u128; packed_size];

        for z in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    if self.get_block(x, y, z).is_solid() {
                        // Match the bit layout from voxelize.rs
                        let texel = (x + ((y + (z / 8) * CHUNK_SIZE) / 4) * CHUNK_SIZE) / 4;
                        let bit = (x % 4) * 32 + (y % 4) + (z % 8) * 4;
                        packed[texel] |= 1u128 << bit;
                    }
                }
            }
        }

        packed
    }

    /// Converts the chunk to a format that includes block type information.
    ///
    /// This returns a Vec<u8> with one byte per block, suitable for
    /// uploading to an R8_UINT 3D texture.
    pub fn to_block_data(&self) -> Vec<u8> {
        self.blocks.iter().map(|&b| b as u8).collect()
    }

    /// Returns the number of non-air blocks in the chunk.
    pub fn block_count(&self) -> usize {
        self.blocks.iter().filter(|&&b| b != BlockType::Air).count()
    }

    /// Returns true if the chunk is completely empty (all air).
    /// Uses cached value if available, otherwise recomputes.
    pub fn is_empty(&self) -> bool {
        if self.metadata_dirty {
            // Recompute if dirty (but don't cache in immutable method)
            self.blocks.iter().all(|&b| b == BlockType::Air)
        } else {
            self.cached_is_empty
        }
    }

    /// Returns true if the chunk is completely solid (no air/transparent blocks).
    /// Uses cached value if available, otherwise recomputes.
    pub fn is_fully_solid(&self) -> bool {
        if self.metadata_dirty {
            self.blocks.iter().all(|&b| b.is_solid())
        } else {
            self.cached_is_fully_solid
        }
    }

    /// Updates the cached metadata (is_empty, is_fully_solid).
    /// Call this after bulk modifications to avoid repeated recalculation.
    pub fn update_metadata(&mut self) {
        if self.metadata_dirty {
            self.cached_is_empty = self.blocks.iter().all(|&b| b == BlockType::Air);
            self.cached_is_fully_solid = self.blocks.iter().all(|&b| b.is_solid());
            self.metadata_dirty = false;
        }
    }

    /// Returns the cached is_empty flag directly (for GPU upload).
    /// Call update_metadata() first to ensure accuracy.
    #[inline]
    pub fn cached_is_empty(&self) -> bool {
        self.cached_is_empty
    }

    /// Returns the cached is_fully_solid flag directly (for GPU upload).
    /// Call update_metadata() first to ensure accuracy.
    #[inline]
    pub fn cached_is_fully_solid(&self) -> bool {
        self.cached_is_fully_solid
    }

    /// Marks the chunk as needing GPU re-upload.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Marks the chunk as synced with GPU.
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_new() {
        let chunk = Chunk::new();
        assert!(chunk.is_empty());
        assert!(chunk.dirty);
    }

    #[test]
    fn test_chunk_set_get() {
        let mut chunk = Chunk::new();
        chunk.set_block(5, 10, 15, BlockType::Stone);
        assert_eq!(chunk.get_block(5, 10, 15), BlockType::Stone);
        assert_eq!(chunk.get_block(0, 0, 0), BlockType::Air);
    }

    #[test]
    fn test_chunk_bit_packed() {
        let mut chunk = Chunk::new();
        chunk.set_block(0, 0, 0, BlockType::Stone);
        chunk.set_block(1, 0, 0, BlockType::Dirt);

        let packed = chunk.to_bit_packed();
        assert!(!packed.is_empty());

        // First two bits should be set
        assert!(packed[0] & 1 != 0); // (0,0,0)
        assert!(packed[0] & (1 << 32) != 0); // (1,0,0) - x % 4 = 1, so bit 32
    }

    #[test]
    fn test_block_count() {
        let mut chunk = Chunk::new();
        assert_eq!(chunk.block_count(), 0);

        chunk.set_block(0, 0, 0, BlockType::Stone);
        chunk.set_block(1, 1, 1, BlockType::Dirt);
        assert_eq!(chunk.block_count(), 2);
    }
}

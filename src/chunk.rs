//! Chunk data structure for voxel storage.
//!
//! Each chunk is a 32³ grid of blocks. Blocks are stored as u8 values
//! where 0 = air and other values represent different block types.
//!
//! Blocks of type `Model` use sparse metadata storage to associate
//! a model_id and rotation with each model block.

#![allow(dead_code)]

use std::cell::{Cell, Ref, RefCell};
use std::collections::HashMap;
use std::slice;
use std::sync::Arc;
use vulkano::image::view::ImageView;

/// Size of a chunk in each dimension (32³ = 32,768 blocks per chunk).
pub const CHUNK_SIZE: usize = 32;

/// Total number of blocks in a chunk.
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

/// Block types that can exist in the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
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
    /// Sub-voxel model block. Use BlockModelData to get model_id and rotation.
    Model = 11,
    Brick = 12,
    Snow = 13,
    Cobblestone = 14,
    Iron = 15,
    Bedrock = 16,
}

/// Metadata for a block that uses a sub-voxel model.
///
/// This is stored sparsely in chunks - only blocks of type `Model` have metadata.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BlockModelData {
    /// Model ID from the model registry (1 = torch, 2 = slab_bottom, etc.).
    pub model_id: u8,

    /// Rotation around Y axis (0-3 = 0°/90°/180°/270°).
    pub rotation: u8,

    /// Whether this block is waterlogged (contains water in the same space).
    pub waterlogged: bool,
}

impl BlockType {
    /// Returns true if this block type is solid (not air, water, or model blocks).
    /// Note: Model blocks may have sub-voxel collision, but are not solid at block level.
    #[inline]
    pub fn is_solid(self) -> bool {
        !matches!(self, BlockType::Air | BlockType::Water | BlockType::Model)
    }

    /// Returns true if this block can be targeted by raycast for breaking/interaction.
    /// Includes Model blocks which are not solid but can still be broken.
    #[inline]
    pub fn is_targetable(self) -> bool {
        !matches!(self, BlockType::Air | BlockType::Water)
    }

    /// Returns true if this block type is affected by gravity (sand, gravel).
    #[inline]
    pub fn is_affected_by_gravity(self) -> bool {
        matches!(self, BlockType::Sand | BlockType::Gravel)
    }

    /// Returns true if this block is a log (tree trunk).
    #[inline]
    pub fn is_log(self) -> bool {
        matches!(self, BlockType::Log)
    }

    /// Returns true if this block is part of a tree (log or leaves).
    #[inline]
    pub fn is_tree_part(self) -> bool {
        matches!(self, BlockType::Log | BlockType::Leaves)
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
                | BlockType::Model
        )
    }

    /// Returns true if this block type emits light.
    /// Note: For Model blocks, check the model's emission property instead.
    #[inline]
    pub fn is_light_source(self) -> bool {
        // Model blocks handle light emission via their model data
        false
    }

    /// Returns the light color and intensity for light-emitting blocks.
    /// Returns (color RGB, intensity) or None if not a light source.
    /// Note: For Model blocks, use the model registry to get emission properties.
    #[inline]
    pub fn light_properties(self) -> Option<([f32; 3], f32)> {
        // Model blocks get light properties from their model data
        None
    }

    /// Returns the color for this block type (RGB, 0-1 range).
    /// Note: Model blocks use their sub-voxel palette for coloring.
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
            BlockType::Model => [0.5, 0.5, 0.5], // Fallback gray (uses sub-voxel colors)
            BlockType::Brick => [0.7, 0.35, 0.3],
            BlockType::Snow => [0.95, 0.95, 0.98],
            BlockType::Cobblestone => [0.45, 0.45, 0.45],
            BlockType::Iron => [0.75, 0.75, 0.78],
            BlockType::Bedrock => [0.2, 0.2, 0.2], // Dark gray, nearly black
        }
    }

    /// Returns the time in seconds to break this block type.
    /// Higher values = takes longer to break.
    #[inline]
    pub fn break_time(self) -> f32 {
        match self {
            BlockType::Air => 0.0,
            // Very fast (instant)
            BlockType::Leaves | BlockType::Model => 0.15,
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
            // Indestructible
            BlockType::Bedrock => 0.0,
        }
    }

    /// Returns true if this block type uses sub-voxel model rendering.
    #[inline]
    pub fn is_model(self) -> bool {
        matches!(self, BlockType::Model)
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
            11 => BlockType::Model,
            12 => BlockType::Brick,
            13 => BlockType::Snow,
            14 => BlockType::Cobblestone,
            15 => BlockType::Iron,
            16 => BlockType::Bedrock,
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

    /// Sparse storage for sub-voxel model metadata.
    /// Only blocks of type `Model` have entries here.
    /// Key: block index, Value: model_id and rotation.
    model_data: HashMap<usize, BlockModelData>,

    /// Reusable RG8 buffer for model metadata uploads (len = CHUNK_VOLUME * 2).
    model_metadata_buf: RefCell<Vec<u8>>,
    /// Whether the cached model metadata buffer needs recomputing.
    model_metadata_dirty: Cell<bool>,

    /// Count of non-model light-emitting block types (for quick skip).
    light_block_count: usize,

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
            model_data: HashMap::new(),
            model_metadata_buf: RefCell::new(vec![0u8; CHUNK_VOLUME * 2]),
            model_metadata_dirty: Cell::new(false),
            light_block_count: 0,
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
        let light_block_count = if block_type.is_light_source() {
            CHUNK_VOLUME
        } else {
            0
        };
        Self {
            blocks: Box::new([block_type; CHUNK_VOLUME]),
            model_data: HashMap::new(),
            model_metadata_buf: RefCell::new(vec![0u8; CHUNK_VOLUME * 2]),
            model_metadata_dirty: Cell::new(false),
            light_block_count,
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

    /// Converts a flat array index back to local coordinates.
    #[inline]
    pub fn index_to_coords(idx: usize) -> (usize, usize, usize) {
        debug_assert!(idx < CHUNK_VOLUME);
        let x = idx % CHUNK_SIZE;
        let y = (idx / CHUNK_SIZE) % CHUNK_SIZE;
        let z = idx / (CHUNK_SIZE * CHUNK_SIZE);
        (x, y, z)
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
        let old = self.blocks[idx];
        if old != block {
            // Maintain light block count
            if old.is_light_source() && self.light_block_count > 0 {
                self.light_block_count -= 1;
            }
            if block.is_light_source() {
                self.light_block_count += 1;
            }

            self.blocks[idx] = block;
            self.dirty = true;
            self.metadata_dirty = true;

            // Clean up model data if block is no longer a Model
            if block != BlockType::Model {
                self.model_data.remove(&idx);
                self.model_metadata_dirty.set(true);
            }
        } else if block.is_light_source() {
            // No change, keep counts stable
        }
    }

    /// Sets a model block with its metadata at the given local coordinates.
    #[inline]
    pub fn set_model_block(
        &mut self,
        x: usize,
        y: usize,
        z: usize,
        model_id: u8,
        rotation: u8,
        waterlogged: bool,
    ) {
        let idx = Self::index(x, y, z);
        self.blocks[idx] = BlockType::Model;
        self.model_data.insert(
            idx,
            BlockModelData {
                model_id,
                rotation,
                waterlogged,
            },
        );
        self.dirty = true;
        self.metadata_dirty = true;
        self.model_metadata_dirty.set(true);
    }

    /// Gets the model data for a block at the given local coordinates.
    /// Returns None if the block is not a Model type.
    #[inline]
    pub fn get_model_data(&self, x: usize, y: usize, z: usize) -> Option<BlockModelData> {
        let idx = Self::index(x, y, z);
        self.model_data.get(&idx).copied()
    }

    /// Sets the model data for a block at the given local coordinates.
    /// The block should already be of type Model.
    #[inline]
    pub fn set_model_data(&mut self, x: usize, y: usize, z: usize, data: BlockModelData) {
        let idx = Self::index(x, y, z);
        self.model_data.insert(idx, data);
        self.dirty = true;
        self.model_metadata_dirty.set(true);
    }

    /// Returns the number of model blocks in this chunk.
    #[inline]
    pub fn model_count(&self) -> usize {
        self.model_data.len()
    }

    /// Returns true if this chunk may contain non-model light sources.
    #[inline]
    pub fn light_block_count(&self) -> usize {
        self.light_block_count
    }

    /// Iterates over all model block entries (index -> metadata).
    #[inline]
    pub fn model_entries(&self) -> impl Iterator<Item = (&usize, &BlockModelData)> {
        self.model_data.iter()
    }

    /// Iterates over all blocks with their flat index.
    #[inline]
    pub fn iter_blocks(&self) -> impl Iterator<Item = (usize, BlockType)> + '_ {
        self.blocks.iter().copied().enumerate()
    }

    /// Checks if a block is solid at the given local coordinates.
    #[inline]
    pub fn is_solid(&self, x: usize, y: usize, z: usize) -> bool {
        self.get_block(x, y, z).is_solid()
    }

    /// Converts the chunk to bit-packed format.
    ///
    /// LEGACY: This method is currently unused. The actual GPU acceleration structure
    /// is built using the `svt` module (Sparse Voxel Tree), which generates a
    /// 64-bit brick mask (split into two u32s) per chunk, not this u128 format.
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

    /// Returns a zero-copy view of the chunk blocks as raw u8 bytes.
    #[inline]
    pub fn block_bytes(&self) -> &[u8] {
        // SAFETY: BlockType is #[repr(u8)] and blocks is a contiguous array.
        unsafe { slice::from_raw_parts(self.blocks.as_ptr() as *const u8, CHUNK_VOLUME) }
    }

    /// Converts the chunk's model metadata to GPU format.
    ///
    /// This returns a Vec<u8> with 2 bytes per block (RG8 format):
    /// - R channel: model_id (0 for non-model blocks)
    /// - G channel: rotation (0 for non-model blocks)
    ///
    /// Suitable for uploading to an RG8_UINT 3D texture.
    pub fn to_model_metadata(&self) -> Vec<u8> {
        self.model_metadata_bytes().to_vec()
    }

    /// Returns a cached RG8 view of the model metadata (2 bytes per voxel).
    /// The buffer is rebuilt only when model data changes.
    #[inline]
    pub fn model_metadata_bytes(&self) -> Ref<'_, [u8]> {
        if self.model_metadata_dirty.get() {
            {
                let mut buf = self.model_metadata_buf.borrow_mut();
                buf.fill(0);
                for (idx, data) in &self.model_data {
                    let offset = idx * 2;
                    buf[offset] = data.model_id;
                    // Pack rotation (bits 0-1) and waterlogged (bit 2)
                    let mut packed_meta = data.rotation & 0x03;
                    if data.waterlogged {
                        packed_meta |= 0x04;
                    }
                    buf[offset + 1] = packed_meta;
                }
            }
            self.model_metadata_dirty.set(false);
        }
        Ref::map(self.model_metadata_buf.borrow(), |v| v.as_slice())
    }

    /// Returns the number of non-air blocks in the chunk.
    pub fn block_count(&self) -> usize {
        self.blocks.iter().filter(|&&b| b != BlockType::Air).count()
    }

    /// Returns an immutable view of the chunk's block storage.
    #[inline]
    pub fn block_slice(&self) -> &[BlockType; CHUNK_VOLUME] {
        &self.blocks
    }

    /// Clones the chunk's block storage into a new boxed array.
    /// Useful for off-thread processing without borrowing the chunk.
    pub fn clone_blocks(&self) -> Box<[BlockType; CHUNK_VOLUME]> {
        self.blocks.clone()
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

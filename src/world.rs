//! World management for the voxel game.
//!
//! The World struct manages a collection of chunks and provides
//! methods for accessing and modifying blocks at world coordinates.

#![allow(dead_code)]

use crate::chunk::{BlockType, CHUNK_SIZE, Chunk};
use nalgebra::{Vector3, vector};
use std::collections::HashMap;

/// A position in chunk coordinates (each unit = one chunk).
pub type ChunkPos = Vector3<i32>;

/// A position in world/block coordinates.
pub type WorldPos = Vector3<i32>;

/// The voxel world, containing all loaded chunks.
pub struct World {
    /// All currently loaded chunks, keyed by chunk position.
    chunks: HashMap<ChunkPos, Chunk>,

    /// Queue of chunk positions that need GPU re-upload.
    dirty_chunks: Vec<ChunkPos>,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    /// Creates a new empty world.
    pub fn new() -> Self {
        Self {
            chunks: HashMap::new(),
            dirty_chunks: Vec::new(),
        }
    }

    /// Converts world coordinates to chunk coordinates.
    #[inline]
    pub fn world_to_chunk(world_pos: WorldPos) -> ChunkPos {
        vector![
            world_pos.x.div_euclid(CHUNK_SIZE as i32),
            world_pos.y.div_euclid(CHUNK_SIZE as i32),
            world_pos.z.div_euclid(CHUNK_SIZE as i32)
        ]
    }

    /// Converts world coordinates to local chunk coordinates.
    #[inline]
    pub fn world_to_local(world_pos: WorldPos) -> (usize, usize, usize) {
        (
            world_pos.x.rem_euclid(CHUNK_SIZE as i32) as usize,
            world_pos.y.rem_euclid(CHUNK_SIZE as i32) as usize,
            world_pos.z.rem_euclid(CHUNK_SIZE as i32) as usize,
        )
    }

    /// Converts chunk position to world position (bottom-left-back corner).
    #[inline]
    pub fn chunk_to_world(chunk_pos: ChunkPos) -> WorldPos {
        vector![
            chunk_pos.x * CHUNK_SIZE as i32,
            chunk_pos.y * CHUNK_SIZE as i32,
            chunk_pos.z * CHUNK_SIZE as i32
        ]
    }

    /// Gets a reference to a chunk if it exists.
    pub fn get_chunk(&self, chunk_pos: ChunkPos) -> Option<&Chunk> {
        self.chunks.get(&chunk_pos)
    }

    /// Gets a mutable reference to a chunk if it exists.
    pub fn get_chunk_mut(&mut self, chunk_pos: ChunkPos) -> Option<&mut Chunk> {
        self.chunks.get_mut(&chunk_pos)
    }

    /// Checks if a chunk exists at the given position.
    pub fn has_chunk(&self, chunk_pos: ChunkPos) -> bool {
        self.chunks.contains_key(&chunk_pos)
    }

    /// Inserts a chunk at the given position.
    pub fn insert_chunk(&mut self, chunk_pos: ChunkPos, chunk: Chunk) {
        self.chunks.insert(chunk_pos, chunk);
        self.dirty_chunks.push(chunk_pos);
    }

    /// Removes and returns a chunk at the given position.
    pub fn remove_chunk(&mut self, chunk_pos: ChunkPos) -> Option<Chunk> {
        self.chunks.remove(&chunk_pos)
    }

    /// Gets the block at world coordinates.
    pub fn get_block(&self, world_pos: WorldPos) -> Option<BlockType> {
        let chunk_pos = Self::world_to_chunk(world_pos);
        let (lx, ly, lz) = Self::world_to_local(world_pos);

        self.chunks
            .get(&chunk_pos)
            .map(|chunk| chunk.get_block(lx, ly, lz))
    }

    /// Sets the block at world coordinates.
    ///
    /// If the chunk doesn't exist, it will be created.
    pub fn set_block(&mut self, world_pos: WorldPos, block: BlockType) {
        let chunk_pos = Self::world_to_chunk(world_pos);
        let (lx, ly, lz) = Self::world_to_local(world_pos);

        let is_new_chunk = !self.chunks.contains_key(&chunk_pos);
        let chunk = self.chunks.entry(chunk_pos).or_default();
        let was_dirty = chunk.dirty;
        chunk.set_block(lx, ly, lz, block);

        // Add to dirty queue if this is a new chunk or the modification made it dirty
        if is_new_chunk || (chunk.dirty && !was_dirty) {
            self.dirty_chunks.push(chunk_pos);
        }
    }

    /// Checks if a block is solid at world coordinates.
    pub fn is_solid(&self, world_pos: WorldPos) -> bool {
        self.get_block(world_pos)
            .map(|b| b.is_solid())
            .unwrap_or(false)
    }

    /// Returns an iterator over all loaded chunks.
    pub fn chunks(&self) -> impl Iterator<Item = (&ChunkPos, &Chunk)> {
        self.chunks.iter()
    }

    /// Returns a mutable iterator over all loaded chunks.
    pub fn chunks_mut(&mut self) -> impl Iterator<Item = (&ChunkPos, &mut Chunk)> {
        self.chunks.iter_mut()
    }

    /// Returns the number of loaded chunks.
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Drains the dirty chunks queue.
    ///
    /// Returns all chunk positions that need GPU re-upload.
    pub fn drain_dirty_chunks(&mut self) -> Vec<ChunkPos> {
        std::mem::take(&mut self.dirty_chunks)
    }

    /// Returns all dirty chunk positions without draining.
    pub fn dirty_chunks(&self) -> &[ChunkPos] {
        &self.dirty_chunks
    }

    /// Creates a simple test world with a flat terrain.
    pub fn create_flat_world(size: i32, height: i32) -> Self {
        let mut world = Self::new();

        // Calculate how many chunks we need
        let chunks_xz = (size + CHUNK_SIZE as i32 - 1) / CHUNK_SIZE as i32;
        let chunks_y = (height + CHUNK_SIZE as i32 - 1) / CHUNK_SIZE as i32;

        for cx in 0..chunks_xz {
            for cz in 0..chunks_xz {
                for cy in 0..chunks_y {
                    let chunk_pos = vector![cx, cy, cz];
                    let mut chunk = Chunk::new();

                    for lx in 0..CHUNK_SIZE {
                        for lz in 0..CHUNK_SIZE {
                            for ly in 0..CHUNK_SIZE {
                                let world_y = cy * CHUNK_SIZE as i32 + ly as i32;

                                let block = if world_y == height - 1 {
                                    BlockType::Grass
                                } else if world_y >= height - 4 {
                                    BlockType::Dirt
                                } else if world_y >= 0 {
                                    BlockType::Stone
                                } else {
                                    BlockType::Air
                                };

                                if block != BlockType::Air {
                                    chunk.set_block(lx, ly, lz, block);
                                }
                            }
                        }
                    }

                    if !chunk.is_empty() {
                        world.insert_chunk(chunk_pos, chunk);
                    }
                }
            }
        }

        world
    }

    /// Creates a world from a single chunk (for testing/compatibility).
    pub fn from_single_chunk(chunk: Chunk) -> Self {
        let mut world = Self::new();
        world.insert_chunk(vector![0, 0, 0], chunk);
        world
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_world_to_chunk() {
        assert_eq!(World::world_to_chunk(vector![0, 0, 0]), vector![0, 0, 0]);
        assert_eq!(World::world_to_chunk(vector![31, 31, 31]), vector![0, 0, 0]);
        assert_eq!(World::world_to_chunk(vector![32, 0, 0]), vector![1, 0, 0]);
        assert_eq!(World::world_to_chunk(vector![-1, 0, 0]), vector![-1, 0, 0]);
        assert_eq!(World::world_to_chunk(vector![-32, 0, 0]), vector![-1, 0, 0]);
        assert_eq!(World::world_to_chunk(vector![-33, 0, 0]), vector![-2, 0, 0]);
    }

    #[test]
    fn test_world_to_local() {
        assert_eq!(World::world_to_local(vector![0, 0, 0]), (0, 0, 0));
        assert_eq!(World::world_to_local(vector![5, 10, 15]), (5, 10, 15));
        assert_eq!(World::world_to_local(vector![32, 0, 0]), (0, 0, 0));
        assert_eq!(World::world_to_local(vector![-1, 0, 0]), (31, 0, 0));
    }

    #[test]
    fn test_set_get_block() {
        let mut world = World::new();

        world.set_block(vector![10, 20, 30], BlockType::Stone);
        assert_eq!(world.get_block(vector![10, 20, 30]), Some(BlockType::Stone));
        assert_eq!(world.get_block(vector![0, 0, 0]), Some(BlockType::Air));

        // Test negative coordinates
        world.set_block(vector![-5, -10, -15], BlockType::Dirt);
        assert_eq!(
            world.get_block(vector![-5, -10, -15]),
            Some(BlockType::Dirt)
        );
    }

    #[test]
    fn test_dirty_chunks() {
        let mut world = World::new();

        world.set_block(vector![0, 0, 0], BlockType::Stone);
        world.set_block(vector![32, 0, 0], BlockType::Dirt);

        let dirty = world.drain_dirty_chunks();
        assert_eq!(dirty.len(), 2);
        assert!(world.dirty_chunks().is_empty());
    }
}

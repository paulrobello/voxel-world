//! World storage and chunk management.

use super::{ChunkPos, WorldPos};
use crate::chunk::{BlockModelData, BlockType, CHUNK_SIZE, Chunk};
use crate::terrain_gen::OverflowBlock;
use nalgebra::vector;
use std::collections::{HashMap, HashSet};

/// The voxel world, containing all loaded chunks.
pub struct World {
    /// All currently loaded chunks, keyed by chunk position.
    pub(super) chunks: HashMap<ChunkPos, Chunk>,

    /// Queue of chunk positions that need GPU re-upload.
    pub(super) dirty_chunks: Vec<ChunkPos>,
    /// Membership set to keep dirty_chunks unique.
    pub(super) dirty_set: HashSet<ChunkPos>,

    /// Height cache for minimap: (x, z) -> (block_type, height)
    pub(super) minimap_height_cache: HashMap<(i32, i32), (BlockType, i32)>,

    /// Pending overflow blocks waiting for their target chunks to be loaded.
    /// Key: target chunk position, Value: blocks to place in that chunk
    pub(super) pending_overflow: HashMap<ChunkPos, Vec<OverflowBlock>>,
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
            dirty_set: HashSet::new(),
            minimap_height_cache: HashMap::new(),
            pending_overflow: HashMap::new(),
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
    pub fn insert_chunk(&mut self, chunk_pos: ChunkPos, mut chunk: Chunk) {
        // Apply any pending overflow blocks for this chunk position
        if let Some(overflow_blocks) = self.pending_overflow.remove(&chunk_pos) {
            for overflow in overflow_blocks {
                let local = Self::world_to_local(overflow.world_pos);
                let existing_block = chunk.get_block(local.0, local.1, local.2);
                // Tree structure (logs/leaves) can replace surface terrain for proper cross-chunk trees
                let can_replace = existing_block == BlockType::Air
                    || existing_block.is_transparent()
                    || (overflow.block_type.is_tree_structure()
                        && existing_block.is_replaceable_terrain());
                if can_replace {
                    chunk.set_block(local.0, local.1, local.2, overflow.block_type);
                }
            }
            // CRITICAL: Update metadata after applying overflow blocks
            chunk.update_metadata();
        }

        self.chunks.insert(chunk_pos, chunk);
        self.push_dirty(chunk_pos);

        // Invalidate minimap cache for this chunk's column to ensure new data is visible
        let world_x_base = chunk_pos.x * CHUNK_SIZE as i32;
        let world_z_base = chunk_pos.z * CHUNK_SIZE as i32;
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                self.minimap_height_cache
                    .remove(&(world_x_base + x as i32, world_z_base + z as i32));
            }
        }
    }

    /// Removes and returns a chunk at the given position.
    pub fn remove_chunk(&mut self, chunk_pos: ChunkPos) -> Option<Chunk> {
        // Invalidate minimap cache for this chunk's column
        let world_x_base = chunk_pos.x * CHUNK_SIZE as i32;
        let world_z_base = chunk_pos.z * CHUNK_SIZE as i32;
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                self.minimap_height_cache
                    .remove(&(world_x_base + x as i32, world_z_base + z as i32));
            }
        }
        self.chunks.remove(&chunk_pos)
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
        self.dirty_set.clear();
        std::mem::take(&mut self.dirty_chunks)
    }

    /// Removes the given positions from the dirty chunk queue, if present.
    pub fn remove_dirty_positions(&mut self, positions: &[ChunkPos]) {
        if self.dirty_chunks.is_empty() || positions.is_empty() {
            return;
        }

        let remove: HashSet<_> = positions.iter().copied().collect();
        self.dirty_chunks.retain(|pos| !remove.contains(pos));
        for pos in &remove {
            self.dirty_set.remove(pos);
        }
    }

    /// Pushes a chunk position onto the dirty queue if not already present.
    pub(super) fn push_dirty(&mut self, pos: ChunkPos) {
        if self.dirty_set.insert(pos) {
            self.dirty_chunks.push(pos);
        }
    }

    /// Returns all dirty chunk positions without draining.
    pub fn dirty_chunks(&self) -> &[ChunkPos] {
        &self.dirty_chunks
    }

    /// Returns the count of chunks with dirty flag set.
    pub fn dirty_chunk_count(&self) -> usize {
        self.chunks.values().filter(|c| c.dirty).count()
    }

    /// Gets chunk positions that should be loaded based on player position.
    ///
    /// Returns chunks within the given view distance that are not yet loaded.
    /// Chunks are sorted by distance to the center (closest first).
    pub fn get_chunks_to_load(
        &self,
        center: ChunkPos,
        view_distance: i32,
        world_bounds: (ChunkPos, ChunkPos), // (min_chunk, max_chunk) inclusive
    ) -> Vec<ChunkPos> {
        let mut to_load = Vec::new();
        let (min_chunk, max_chunk) = world_bounds;

        // Use horizontal distance only - load ALL Y levels within horizontal range
        // This prevents floating chunks when viewing from mountaintops
        for dx in -view_distance..=view_distance {
            for dz in -view_distance..=view_distance {
                // Check horizontal distance (circular, not square)
                let dist_sq = dx * dx + dz * dz;
                if dist_sq > view_distance * view_distance {
                    continue;
                }

                // Load all Y levels within horizontal range
                for cy in min_chunk.y..=max_chunk.y {
                    let chunk_pos = vector![center.x + dx, cy, center.z + dz];

                    // Check horizontal world bounds
                    if chunk_pos.x < min_chunk.x
                        || chunk_pos.x > max_chunk.x
                        || chunk_pos.z < min_chunk.z
                        || chunk_pos.z > max_chunk.z
                    {
                        continue;
                    }

                    // Only add if not already loaded
                    if !self.has_chunk(chunk_pos) {
                        to_load.push(chunk_pos);
                    }
                }
            }
        }

        // Sort by horizontal distance to center (closest first)
        to_load.sort_by_key(|pos| {
            let dx = pos.x - center.x;
            let dz = pos.z - center.z;
            dx * dx + dz * dz
        });

        to_load
    }

    /// Gets chunk positions that should be unloaded based on player position.
    ///
    /// Returns loaded chunks that are beyond the given unload distance (horizontal only).
    pub fn get_chunks_to_unload(&self, center: ChunkPos, unload_distance: i32) -> Vec<ChunkPos> {
        let unload_dist_sq = unload_distance * unload_distance;
        self.chunks
            .keys()
            .filter(|pos| {
                // Use horizontal distance only (matching load behavior)
                let dx = pos.x - center.x;
                let dz = pos.z - center.z;
                dx * dx + dz * dz > unload_dist_sq
            })
            .cloned()
            .collect()
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

        if is_new_chunk || (chunk.dirty && !was_dirty) {
            self.push_dirty(chunk_pos);
        }
    }

    /// Sets a water block at world coordinates with the given water type.
    ///
    /// This sets the block type to Water and stores the water type metadata.
    /// If the chunk doesn't exist, it will be created.
    pub fn set_water_block(&mut self, world_pos: WorldPos, water_type: crate::chunk::WaterType) {
        let chunk_pos = Self::world_to_chunk(world_pos);
        let (lx, ly, lz) = Self::world_to_local(world_pos);

        let is_new_chunk = !self.chunks.contains_key(&chunk_pos);
        let chunk = self.chunks.entry(chunk_pos).or_default();
        let was_dirty = chunk.dirty;
        chunk.set_water_block(lx, ly, lz, water_type);

        if is_new_chunk || (chunk.dirty && !was_dirty) {
            self.push_dirty(chunk_pos);
        }
    }

    /// Gets the water type for a block at world coordinates.
    ///
    /// Returns None if the chunk doesn't exist or the block has no water data.
    pub fn get_water_type(&self, world_pos: WorldPos) -> Option<crate::chunk::WaterType> {
        let chunk_pos = Self::world_to_chunk(world_pos);
        let (lx, ly, lz) = Self::world_to_local(world_pos);

        self.chunks
            .get(&chunk_pos)
            .and_then(|chunk| chunk.get_water_type(lx, ly, lz))
    }

    /// Sets a model block at world coordinates with the given model_id and rotation.
    ///
    /// This sets the block type to Model and stores the model metadata.
    /// If the chunk doesn't exist, it will be created.
    pub fn set_model_block(
        &mut self,
        world_pos: WorldPos,
        model_id: u8,
        rotation: u8,
        waterlogged: bool,
    ) {
        let chunk_pos = Self::world_to_chunk(world_pos);
        let (lx, ly, lz) = Self::world_to_local(world_pos);

        let is_new_chunk = !self.chunks.contains_key(&chunk_pos);
        let chunk = self.chunks.entry(chunk_pos).or_default();
        let was_dirty = chunk.dirty;
        chunk.set_model_block(lx, ly, lz, model_id, rotation, waterlogged);

        if is_new_chunk || (chunk.dirty && !was_dirty) {
            self.push_dirty(chunk_pos);
        }
    }

    /// Gets model data for a block at world coordinates.
    ///
    /// Returns None if the chunk doesn't exist or the block has no model data.
    pub fn get_model_data(&self, world_pos: WorldPos) -> Option<BlockModelData> {
        let chunk_pos = Self::world_to_chunk(world_pos);
        let (lx, ly, lz) = Self::world_to_local(world_pos);

        self.chunks
            .get(&chunk_pos)
            .and_then(|chunk| chunk.get_model_data(lx, ly, lz))
    }

    /// Sets a tinted glass block at world coordinates with the given tint color index.
    ///
    /// This sets the block type to TintedGlass and stores the tint metadata.
    /// If the chunk doesn't exist, it will be created.
    pub fn set_tinted_glass_block(&mut self, world_pos: WorldPos, tint_index: u8) {
        let chunk_pos = Self::world_to_chunk(world_pos);
        let (lx, ly, lz) = Self::world_to_local(world_pos);

        let is_new_chunk = !self.chunks.contains_key(&chunk_pos);
        let chunk = self.chunks.entry(chunk_pos).or_default();
        let was_dirty = chunk.dirty;
        chunk.set_tinted_glass_block(lx, ly, lz, tint_index);

        if is_new_chunk || (chunk.dirty && !was_dirty) {
            self.push_dirty(chunk_pos);
        }
    }

    /// Sets a crystal block at world coordinates with the given tint color index.
    ///
    /// This sets the block type to Crystal and stores the tint metadata.
    /// If the chunk doesn't exist, it will be created.
    pub fn set_crystal_block(&mut self, world_pos: WorldPos, tint_index: u8) {
        let chunk_pos = Self::world_to_chunk(world_pos);
        let (lx, ly, lz) = Self::world_to_local(world_pos);

        let is_new_chunk = !self.chunks.contains_key(&chunk_pos);
        let chunk = self.chunks.entry(chunk_pos).or_default();
        let was_dirty = chunk.dirty;
        chunk.set_crystal_block(lx, ly, lz, tint_index);

        if is_new_chunk || (chunk.dirty && !was_dirty) {
            self.push_dirty(chunk_pos);
        }
    }

    /// Gets the tint color index for a tinted glass block at world coordinates.
    ///
    /// Returns None if the chunk doesn't exist or the block has no tint data.
    pub fn get_tint_index(&self, world_pos: WorldPos) -> Option<u8> {
        let chunk_pos = Self::world_to_chunk(world_pos);
        let (lx, ly, lz) = Self::world_to_local(world_pos);

        self.chunks
            .get(&chunk_pos)
            .and_then(|chunk| chunk.get_tint_index(lx, ly, lz))
    }

    /// Sets a painted block at world coordinates with the given texture + tint.
    ///
    /// This sets the block type to Painted and stores the paint metadata.
    /// If the chunk doesn't exist, it will be created.
    pub fn set_painted_block(&mut self, world_pos: WorldPos, texture_idx: u8, tint_idx: u8) {
        let chunk_pos = Self::world_to_chunk(world_pos);
        let (lx, ly, lz) = Self::world_to_local(world_pos);

        let is_new_chunk = !self.chunks.contains_key(&chunk_pos);
        let chunk = self.chunks.entry(chunk_pos).or_default();
        let was_dirty = chunk.dirty;
        chunk.set_painted_block(lx, ly, lz, texture_idx, tint_idx);

        if is_new_chunk || (chunk.dirty && !was_dirty) {
            self.push_dirty(chunk_pos);
        }
    }

    /// Gets the paint metadata for a painted block at world coordinates.
    ///
    /// Returns None if the chunk doesn't exist or the block has no paint data.
    pub fn get_paint_data(&self, world_pos: WorldPos) -> Option<crate::chunk::BlockPaintData> {
        let chunk_pos = Self::world_to_chunk(world_pos);
        let (lx, ly, lz) = Self::world_to_local(world_pos);

        self.chunks
            .get(&chunk_pos)
            .and_then(|chunk| chunk.get_paint_data(lx, ly, lz))
    }

    /// Checks if a block is solid at world coordinates.
    pub fn is_solid(&self, world_pos: WorldPos) -> bool {
        self.get_block(world_pos)
            .map(|b| b.is_solid())
            .unwrap_or(false)
    }

    /// Applies overflow blocks to the world.
    /// - If target chunk exists: applies immediately and marks chunk dirty
    /// - If target chunk doesn't exist: stores in pending_overflow for later application
    pub fn apply_overflow_blocks(&mut self, overflow_blocks: Vec<OverflowBlock>) {
        for overflow in overflow_blocks {
            let chunk_pos = Self::world_to_chunk(overflow.world_pos);

            if self.has_chunk(chunk_pos) {
                // Target chunk exists - apply immediately
                if let Some(existing_block) = self.get_block(overflow.world_pos) {
                    // Tree structure (logs/leaves) can replace surface terrain for proper cross-chunk trees
                    let can_replace = existing_block == BlockType::Air
                        || existing_block.is_transparent()
                        || (overflow.block_type.is_tree_structure()
                            && existing_block.is_replaceable_terrain());
                    if can_replace {
                        self.set_block(overflow.world_pos, overflow.block_type);
                    }
                }
            } else {
                // Target chunk doesn't exist yet - store for later
                self.pending_overflow
                    .entry(chunk_pos)
                    .or_default()
                    .push(overflow);
            }
        }
    }
}

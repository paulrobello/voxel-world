//! World management for the voxel game.
//!
//! The World struct manages a collection of chunks and provides
//! methods for accessing and modifying blocks at world coordinates.

#![allow(dead_code)]

use crate::chunk::{BlockModelData, BlockType, CHUNK_SIZE, Chunk};
use nalgebra::{Vector3, vector};
use std::collections::{HashMap, HashSet};

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
    /// Membership set to keep dirty_chunks unique.
    dirty_set: HashSet<ChunkPos>,

    /// Height cache for minimap: (x, z) -> (block_type, height)
    minimap_height_cache: HashMap<(i32, i32), (BlockType, i32)>,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

use crate::gpu_resources::GpuLight;
use crate::player::PLAYER_EYE_HEIGHT;
use crate::sub_voxel::{ModelRegistry, StairShape};

impl World {
    /// Encodes light mode and intensity for the shader.
    /// Mode: 0 = steady, 1 = slow pulse, 2 = torch flicker
    /// Encoded as: mode + (intensity / 2.0) where intensity is clamped to 0-2 range
    #[inline]
    fn encode_light_intensity(mode: u8, intensity: f32) -> f32 {
        mode as f32 + (intensity.clamp(0.0, 2.0) / 2.0)
    }

    /// Collects all light-emitting blocks (including model blocks like torches)
    /// and returns them as GPU light data.
    pub fn collect_torch_lights(
        &self,
        player_light_enabled: bool,
        player_pos: Vector3<f64>,
        texture_origin: Vector3<i32>,
        model_registry: &ModelRegistry,
        _world_extent: [u32; 3],
    ) -> Vec<GpuLight> {
        let mut lights = Vec::new();

        // Add player light if enabled (like holding a torch)
        if player_light_enabled {
            // Light is at player's hand/chest level, convert to texture coordinates for shader
            let tex_x = (player_pos.x - texture_origin.x as f64) as f32;
            let tex_y = (player_pos.y + PLAYER_EYE_HEIGHT * 0.7 - texture_origin.y as f64) as f32;
            let tex_z = (player_pos.z - texture_origin.z as f64) as f32;
            lights.push(GpuLight {
                pos_radius: [tex_x, tex_y, tex_z, 12.0],
                color_intensity: [1.0, 0.8, 0.5, Self::encode_light_intensity(2, 1.5)], // Flicker mode
            });
        }

        // Iterate over all loaded chunks
        for (chunk_pos, chunk) in self.chunks() {
            // Skip chunks that cannot contribute any light.
            if chunk.is_empty() && chunk.model_count() == 0 && chunk.light_block_count() == 0 {
                continue;
            }

            // Fast path: iterate only model blocks that have metadata.
            if chunk.model_count() > 0 {
                for (idx, model_data) in chunk.model_entries() {
                    if let Some(model) = model_registry.get(model_data.model_id) {
                        if let Some(emission) = &model.emission {
                            let (lx, ly, lz) = crate::chunk::Chunk::index_to_coords(*idx);
                            let world_x = chunk_pos.x * CHUNK_SIZE as i32 + lx as i32;
                            let world_y = chunk_pos.y * CHUNK_SIZE as i32 + ly as i32;
                            let world_z = chunk_pos.z * CHUNK_SIZE as i32 + lz as i32;

                            let tex_x = (world_x - texture_origin.x) as f32 + 0.5;
                            let tex_y = (world_y - texture_origin.y) as f32 + 0.5;
                            let tex_z = (world_z - texture_origin.z) as f32 + 0.5;

                            let r = emission.r as f32 / 255.0;
                            let g = emission.g as f32 / 255.0;
                            let b = emission.b as f32 / 255.0;

                            lights.push(GpuLight {
                                pos_radius: [tex_x, tex_y, tex_z, 10.0],
                                color_intensity: [r, g, b, Self::encode_light_intensity(2, 1.2)], // Flicker mode for torches
                            });

                            if lights.len() >= crate::gpu_resources::MAX_LIGHTS {
                                return lights;
                            }
                        }
                    }
                }
            }

            // Optional scan for non-model light sources (if any).
            if chunk.light_block_count() > 0 {
                for (idx, block) in chunk.iter_blocks() {
                    if !block.is_light_source() {
                        continue;
                    }
                    // light_properties returns (color, intensity), light_radius returns actual radius
                    if let Some((color, intensity)) = block.light_properties() {
                        let radius = block.light_radius();
                        let mode = block.light_mode();
                        let (lx, ly, lz) = crate::chunk::Chunk::index_to_coords(idx);
                        let world_x = chunk_pos.x * CHUNK_SIZE as i32 + lx as i32;
                        let world_y = chunk_pos.y * CHUNK_SIZE as i32 + ly as i32;
                        let world_z = chunk_pos.z * CHUNK_SIZE as i32 + lz as i32;

                        let tex_x = (world_x - texture_origin.x) as f32 + 0.5;
                        let tex_y = (world_y - texture_origin.y) as f32 + 0.5;
                        let tex_z = (world_z - texture_origin.z) as f32 + 0.5;

                        lights.push(GpuLight {
                            pos_radius: [tex_x, tex_y, tex_z, radius],
                            color_intensity: [
                                color[0],
                                color[1],
                                color[2],
                                Self::encode_light_intensity(mode, intensity),
                            ],
                        });

                        if lights.len() >= crate::gpu_resources::MAX_LIGHTS {
                            return lights;
                        }
                    }
                }
            }
        }

        lights
    }

    /// Creates a new empty world.
    pub fn new() -> Self {
        Self {
            chunks: HashMap::new(),
            dirty_chunks: Vec::new(),
            dirty_set: HashSet::new(),
            minimap_height_cache: HashMap::new(),
        }
    }

    /// Invalidates the minimap height cache for a given (x, z) position.
    pub fn invalidate_minimap_cache(&mut self, world_x: i32, world_z: i32) {
        self.minimap_height_cache.remove(&(world_x, world_z));
    }

    /// Gets the minimap height cache.
    pub fn minimap_height_cache(&self) -> &HashMap<(i32, i32), (BlockType, i32)> {
        &self.minimap_height_cache
    }

    /// Gets a mutable reference to the minimap height cache.
    pub fn minimap_height_cache_mut(&mut self) -> &mut HashMap<(i32, i32), (BlockType, i32)> {
        &mut self.minimap_height_cache
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
        self.push_dirty(chunk_pos);
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

        if is_new_chunk || (chunk.dirty && !was_dirty) {
            self.push_dirty(chunk_pos);
        }
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
    fn push_dirty(&mut self, pos: ChunkPos) {
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

    /// Finds all leaves connected to the starting leaf, and checks if any connect to a log.
    /// Returns (leaf_positions, has_log_connection).
    pub fn find_leaf_cluster_and_check_log(
        &self,
        start: Vector3<i32>,
    ) -> (Vec<(Vector3<i32>, BlockType)>, bool) {
        use std::collections::{HashSet, VecDeque};

        let mut leaves = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut found_log = false;

        // Verify starting block is leaves
        if let Some(block) = self.get_block(start) {
            if block != BlockType::Leaves {
                return (leaves, true); // Not leaves, assume connected
            }
        } else {
            return (leaves, true);
        }

        queue.push_back(start);
        visited.insert(start);

        // 26-directional for leaf-to-leaf, 6-directional for leaf-to-log check
        let mut neighbors_26 = Vec::with_capacity(26);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if dx != 0 || dy != 0 || dz != 0 {
                        neighbors_26.push(Vector3::new(dx, dy, dz));
                    }
                }
            }
        }

        while let Some(pos) = queue.pop_front() {
            if let Some(block) = self.get_block(pos) {
                if block == BlockType::Leaves {
                    leaves.push((pos, block));

                    for offset in &neighbors_26 {
                        let neighbor = pos + offset;
                        let is_cardinal = (offset.x != 0) as i32
                            + (offset.y != 0) as i32
                            + (offset.z != 0) as i32
                            == 1;

                        if let Some(neighbor_block) = self.get_block(neighbor) {
                            // Check for log connection (orthogonal only)
                            if neighbor_block.is_log() && is_cardinal {
                                found_log = true;
                            }

                            // Add unvisited leaves to queue (any direction)
                            if neighbor_block == BlockType::Leaves && !visited.contains(&neighbor) {
                                visited.insert(neighbor);
                                queue.push_back(neighbor);
                            }
                        }
                    }
                }
            }
        }

        (leaves, found_log)
    }

    /// Flood-fill to find all connected tree blocks (logs and leaves) starting from a log.
    /// Returns a vector of (position, block_type) for all connected blocks.
    pub fn find_connected_tree(&self, start: Vector3<i32>) -> Vec<(Vector3<i32>, BlockType)> {
        use std::collections::{HashSet, VecDeque};

        let mut tree_blocks = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        // Verify starting block is a log
        if let Some(block) = self.get_block(start) {
            if !block.is_log() {
                return tree_blocks;
            }
        } else {
            return tree_blocks;
        }

        queue.push_back(start);
        visited.insert(start);

        // 26-directional neighbors (including diagonals)
        let mut neighbors_26 = Vec::with_capacity(26);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if dx != 0 || dy != 0 || dz != 0 {
                        neighbors_26.push(Vector3::new(dx, dy, dz));
                    }
                }
            }
        }

        while let Some(pos) = queue.pop_front() {
            if let Some(block) = self.get_block(pos) {
                if block.is_tree_part() {
                    tree_blocks.push((pos, block));

                    // Connectivity rules to prevent merging separate trees:
                    // - Logs: only connect orthogonally (6-dir) to logs and leaves
                    // - Leaves: connect diagonally (26-dir) to OTHER leaves,
                    //           but only orthogonally (6-dir) to logs
                    for offset in &neighbors_26 {
                        let neighbor = pos + offset;
                        if !visited.contains(&neighbor) {
                            if let Some(neighbor_block) = self.get_block(neighbor) {
                                if !neighbor_block.is_tree_part() {
                                    continue;
                                }

                                // is_cardinal: exactly one axis is non-zero (orthogonal neighbor)
                                let is_cardinal = (offset.x != 0) as i32
                                    + (offset.y != 0) as i32
                                    + (offset.z != 0) as i32
                                    == 1;

                                let should_connect = if block.is_log() {
                                    // Logs only connect orthogonally (6-dir)
                                    is_cardinal
                                } else {
                                    // Leaves: connect to other leaves diagonally (26-dir),
                                    // but only connect to logs orthogonally (6-dir)
                                    if neighbor_block.is_log() {
                                        is_cardinal
                                    } else {
                                        true // leaf-to-leaf: any direction
                                    }
                                };

                                if should_connect {
                                    visited.insert(neighbor);
                                    queue.push_back(neighbor);
                                }
                            }
                        }
                    }
                }
            }
        }

        tree_blocks
    }

    /// Checks if any log in the tree has ground support.
    /// A log has ground support if the block below it is solid and NOT a log.
    pub fn tree_has_ground_support(&self, tree_blocks: &[(Vector3<i32>, BlockType)]) -> bool {
        for (pos, block) in tree_blocks {
            if block.is_log() {
                let below_pos = pos + Vector3::new(0, -1, 0);
                if let Some(below_block) = self.get_block(below_pos) {
                    // Supported if block below is solid and NOT part of the tree
                    // (leaves don't count as support either!)
                    if below_block.is_solid() && !below_block.is_tree_part() {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Calculates fence connection bitmask based on neighboring fences/gates.
    /// Returns N=1, S=2, E=4, W=8 bitmask.
    /// Note: North is -Z, South is +Z (matching model definition)
    pub fn calculate_fence_connections(&self, pos: Vector3<i32>) -> u8 {
        let mut connections = 0u8;

        // Check north (-Z)
        if self.is_fence_connectable(pos + Vector3::new(0, 0, -1)) {
            connections |= 1;
        }
        // Check south (+Z)
        if self.is_fence_connectable(pos + Vector3::new(0, 0, 1)) {
            connections |= 2;
        }
        // Check east (+X)
        if self.is_fence_connectable(pos + Vector3::new(1, 0, 0)) {
            connections |= 4;
        }
        // Check west (-X)
        if self.is_fence_connectable(pos + Vector3::new(-1, 0, 0)) {
            connections |= 8;
        }

        connections
    }

    /// Calculates gate connection bitmask based on neighboring fences/gates.
    /// Returns W=1, E=2 bitmask (gates only connect east-west).
    pub fn calculate_gate_connections(&self, pos: Vector3<i32>) -> u8 {
        let mut connections = 0u8;

        // Check west (-X)
        if self.is_fence_connectable(pos + Vector3::new(-1, 0, 0)) {
            connections |= 1;
        }
        // Check east (+X)
        if self.is_fence_connectable(pos + Vector3::new(1, 0, 0)) {
            connections |= 2;
        }

        connections
    }

    /// Returns true if the block at pos can connect to fences/gates.
    pub fn is_fence_connectable(&self, pos: Vector3<i32>) -> bool {
        use crate::sub_voxel::ModelRegistry;
        if let Some(block) = self.get_block(pos) {
            match block {
                BlockType::Model => {
                    // Check if it's a fence or gate model
                    if let Some(data) = self.get_model_data(pos) {
                        ModelRegistry::is_fence_or_gate(data.model_id)
                    } else {
                        false
                    }
                }
                // Solid blocks also connect to fences
                b if b.is_solid() => true,
                _ => false,
            }
        } else {
            false
        }
    }

    /// Returns the facing direction vector for a stair rotation (0-3).
    #[inline]
    fn stair_facing(rotation: u8) -> Vector3<i32> {
        match rotation & 3 {
            // Base stairs model has low side toward -Z; rotate clockwise for subsequent states.
            0 => Vector3::new(0, 0, -1), // facing -Z (front)
            1 => Vector3::new(1, 0, 0),  // facing +X
            2 => Vector3::new(0, 0, 1),  // facing +Z
            _ => Vector3::new(-1, 0, 0), // facing -X
        }
    }

    #[inline]
    fn rotate_left(dir: Vector3<i32>) -> Vector3<i32> {
        // Left = up x dir
        Vector3::new(dir.z, 0, -dir.x)
    }

    #[inline]
    fn rotate_right(dir: Vector3<i32>) -> Vector3<i32> {
        // Right = dir x up
        Vector3::new(-dir.z, 0, dir.x)
    }

    /// Returns facing for neighboring stair if it matches the inverted flag.
    fn stair_neighbor_facing(&self, pos: Vector3<i32>, inverted: bool) -> Option<Vector3<i32>> {
        if let Some(BlockType::Model) = self.get_block(pos) {
            if let Some(data) = self.get_model_data(pos) {
                if ModelRegistry::is_stairs_model(data.model_id)
                    && ModelRegistry::is_stairs_inverted(data.model_id) == inverted
                {
                    return Some(Self::stair_facing(data.rotation));
                }
            }
        }
        None
    }

    /// Recomputes stair corner shape only for the given position.
    pub fn update_stair_shape_at(&mut self, pos: Vector3<i32>) {
        let Some(BlockType::Model) = self.get_block(pos) else {
            return;
        };
        let Some(data) = self.get_model_data(pos) else {
            return;
        };
        if !ModelRegistry::is_stairs_model(data.model_id) {
            return;
        }

        let inverted = ModelRegistry::is_stairs_inverted(data.model_id);
        let rotation = data.rotation & 3;
        let facing = Self::stair_facing(rotation);

        let left_dir = Self::rotate_left(facing);
        let right_dir = Self::rotate_right(facing);

        // Minecraft wiki stair corner logic:
        // - Inner corner: Our HALF-BLOCK (low/front) side adjacent to SIDE of another stair
        // - Outer corner: Our FULL-BLOCK (high/back) side adjacent to SIDE of another stair
        //
        // facing = direction of LOW side, so:
        //   front_pos (pos + facing) = toward our low side
        //   back_pos (pos - facing) = toward our high side
        let front_pos = pos + facing; // Low side direction - check for INNER corners
        let back_pos = pos - facing; // High side direction - check for OUTER corners

        let front_neighbor = self.stair_neighbor_facing(front_pos, inverted);
        let back_neighbor = self.stair_neighbor_facing(back_pos, inverted);

        let mut shape = StairShape::Straight;

        // 1. Outer Corner Check (Priority) - check FRONT neighbor (at our low side)
        // When neighbor is at our front and faces perpendicular, we get an outer corner
        // (single raised quadrant connecting to neighbor's high side)
        if let Some(ff) = front_neighbor {
            if ff == left_dir {
                shape = StairShape::OuterLeft;
            } else if ff == right_dir {
                shape = StairShape::OuterRight;
            }
        }

        // 2. Inner Corner Check - check BACK neighbor (at our high side)
        // When neighbor is at our back and faces perpendicular, we get an inner corner
        // (L-shaped top with pocket)
        if shape == StairShape::Straight {
            if let Some(bf) = back_neighbor {
                if bf == left_dir {
                    shape = StairShape::InnerRight;
                } else if bf == right_dir {
                    shape = StairShape::InnerLeft;
                }
            }
        }

        // 3. Check LEFT neighbor - stair to our left that's perpendicular
        if shape == StairShape::Straight {
            let left_pos = pos + left_dir;
            if let Some(lf) = self.stair_neighbor_facing(left_pos, inverted) {
                if lf == right_dir {
                    shape = StairShape::InnerRight;
                } else if lf == -left_dir {
                    shape = StairShape::OuterRight;
                }
            }
        }

        // 4. Check RIGHT neighbor - stair to our right that's perpendicular
        if shape == StairShape::Straight {
            let right_pos = pos + right_dir;
            if let Some(rf) = self.stair_neighbor_facing(right_pos, inverted) {
                if rf == left_dir {
                    shape = StairShape::InnerLeft;
                } else if rf == -right_dir {
                    shape = StairShape::OuterLeft;
                }
            }
        }

        // For inverted (ceiling) stairs, swap both Inner↔Outer AND Left↔Right
        // since geometry is flipped vertically, changing both relationships
        if inverted && shape != StairShape::Straight {
            shape = match shape {
                StairShape::InnerLeft => StairShape::OuterRight,
                StairShape::InnerRight => StairShape::OuterLeft,
                StairShape::OuterLeft => StairShape::InnerRight,
                StairShape::OuterRight => StairShape::InnerLeft,
                StairShape::Straight => StairShape::Straight,
            };
        }

        let target_model = ModelRegistry::stairs_model_id(shape, inverted);
        if target_model != data.model_id {
            self.set_model_block(pos, target_model, rotation, data.waterlogged);
        }
    }

    /// Recompute shapes for four horizontal neighbors.
    pub fn update_adjacent_stair_shapes(&mut self, center: Vector3<i32>) {
        let neighbors = [
            Vector3::new(1, 0, 0),
            Vector3::new(-1, 0, 0),
            Vector3::new(0, 0, 1),
            Vector3::new(0, 0, -1),
        ];
        for n in neighbors {
            self.update_stair_shape_at(center + n);
        }
    }

    /// Recompute shape for a newly placed stair (only the placed stair adapts, not neighbors).
    pub fn update_stair_and_neighbors(&mut self, pos: Vector3<i32>) {
        self.update_stair_shape_at(pos);
    }

    /// Updates fence/gate connections for a position and its neighbors.
    pub fn update_fence_connections(&mut self, center_pos: Vector3<i32>) {
        use crate::sub_voxel::ModelRegistry;
        // Update neighbors in all 4 horizontal directions
        let neighbors = [
            Vector3::new(0, 0, 1),  // North
            Vector3::new(0, 0, -1), // South
            Vector3::new(1, 0, 0),  // East
            Vector3::new(-1, 0, 0), // West
        ];

        for offset in &neighbors {
            let neighbor_pos = center_pos + offset;
            if let Some(BlockType::Model) = self.get_block(neighbor_pos) {
                if let Some(data) = self.get_model_data(neighbor_pos) {
                    if ModelRegistry::is_fence_model(data.model_id) {
                        // Update fence connections
                        let connections = self.calculate_fence_connections(neighbor_pos);
                        let new_model_id = ModelRegistry::fence_model_id(connections);
                        if new_model_id != data.model_id {
                            println!(
                                "[DEBUG] Updating fence at {:?} to ID {}",
                                neighbor_pos, new_model_id
                            );
                            // Force rotation 0 for fences as their orientation is in the model_id
                            self.set_model_block(neighbor_pos, new_model_id, 0, data.waterlogged);
                        }
                    } else if ModelRegistry::is_gate_model(data.model_id) {
                        // Update gate connections
                        let connections = self.calculate_gate_connections(neighbor_pos);
                        let is_open = ModelRegistry::is_gate_open_model(data.model_id);
                        let new_model_id = if is_open {
                            ModelRegistry::gate_open_model_id(connections)
                        } else {
                            ModelRegistry::gate_closed_model_id(connections)
                        };
                        if new_model_id != data.model_id {
                            self.set_model_block(
                                neighbor_pos,
                                new_model_id,
                                data.rotation,
                                data.waterlogged,
                            );
                        }
                    }
                }
            }
        }
    }

    /// Calculates window connection bitmask based on neighboring windows/solid blocks.
    /// Returns N=1, S=2, E=4, W=8 bitmask (same as fences).
    pub fn calculate_window_connections(&self, pos: Vector3<i32>) -> u8 {
        let mut connections = 0u8;

        // Check north (-Z)
        if self.is_window_connectable(pos + Vector3::new(0, 0, -1)) {
            connections |= 1;
        }
        // Check south (+Z)
        if self.is_window_connectable(pos + Vector3::new(0, 0, 1)) {
            connections |= 2;
        }
        // Check east (+X)
        if self.is_window_connectable(pos + Vector3::new(1, 0, 0)) {
            connections |= 4;
        }
        // Check west (-X)
        if self.is_window_connectable(pos + Vector3::new(-1, 0, 0)) {
            connections |= 8;
        }

        connections
    }

    /// Returns true if the block at pos can connect to windows.
    pub fn is_window_connectable(&self, pos: Vector3<i32>) -> bool {
        use crate::sub_voxel::ModelRegistry;
        if let Some(block) = self.get_block(pos) {
            match block {
                BlockType::Model => {
                    // Check if it's a window model
                    if let Some(data) = self.get_model_data(pos) {
                        ModelRegistry::is_window_model(data.model_id)
                    } else {
                        false
                    }
                }
                // Solid blocks also connect to windows
                b if b.is_solid() => true,
                // Glass blocks connect too
                BlockType::Glass | BlockType::TintedGlass => true,
                _ => false,
            }
        } else {
            false
        }
    }

    /// Updates window connections for a position and its neighbors.
    pub fn update_window_connections(&mut self, center_pos: Vector3<i32>) {
        use crate::sub_voxel::ModelRegistry;
        // Update neighbors in all 4 horizontal directions
        let neighbors = [
            Vector3::new(0, 0, 1),  // South
            Vector3::new(0, 0, -1), // North
            Vector3::new(1, 0, 0),  // East
            Vector3::new(-1, 0, 0), // West
        ];

        for offset in &neighbors {
            let neighbor_pos = center_pos + offset;
            if let Some(BlockType::Model) = self.get_block(neighbor_pos) {
                if let Some(data) = self.get_model_data(neighbor_pos) {
                    if ModelRegistry::is_window_model(data.model_id) {
                        // Update window connections
                        let connections = self.calculate_window_connections(neighbor_pos);
                        let new_model_id = ModelRegistry::window_model_id(connections);
                        if new_model_id != data.model_id {
                            // Force rotation 0 for windows as their orientation is in the model_id
                            self.set_model_block(neighbor_pos, new_model_id, 0, data.waterlogged);
                        }
                    }
                }
            }
        }
    }

    pub fn generate_minimap_image(
        &mut self,
        player_pos: Vector3<f64>,
        yaw: f32,
        minimap: &crate::hud::Minimap,
    ) -> egui_winit_vulkano::egui::ColorImage {
        use egui_winit_vulkano::egui;
        let display_size = minimap.size as usize;
        let center_x = player_pos.x as f32;
        let center_z = player_pos.z as f32;

        // Base sample radius adjusted by zoom (higher zoom = larger area = zoomed out)
        // When rotating, multiply by sqrt(2) ≈ 1.42 to fill corners
        let base_radius = (display_size as f32 / 2.0) * minimap.zoom;
        let sample_radius = if minimap.rotate {
            base_radius * 1.42
        } else {
            base_radius
        };

        let mut pixels = vec![egui::Color32::BLACK; display_size * display_size];

        // Precompute rotation (rotate world coords to align with player facing direction)
        let (sin_yaw, cos_yaw) = if minimap.rotate {
            (yaw.sin(), yaw.cos())
        } else {
            (0.0, 1.0) // No rotation
        };

        let half = display_size as f32 / 2.0;

        for dz in 0..display_size {
            for dx in 0..display_size {
                // Screen-space offset from center (-half to +half)
                let sx = dx as f32 - half;
                let sz = dz as f32 - half;

                // Scale to sample radius
                let scale = sample_radius / half;
                let scaled_x = sx * scale;
                let scaled_z = sz * scale;

                // Apply rotation to get world-space offset
                let world_offset_x = scaled_x * cos_yaw + scaled_z * sin_yaw;
                let world_offset_z = -scaled_x * sin_yaw + scaled_z * cos_yaw;

                let world_x = (center_x + world_offset_x).floor() as i32;
                let world_z = (center_z + world_offset_z).floor() as i32;

                // Find surface block (top-down) with caching
                let (block_type, height) =
                    if let Some(&cached) = self.minimap_height_cache.get(&(world_x, world_z)) {
                        cached
                    } else {
                        let mut res = (BlockType::Air, 0);
                        for y in (0..crate::constants::TEXTURE_SIZE_Y as i32).rev() {
                            if let Some(block) = self.get_block(Vector3::new(world_x, y, world_z)) {
                                if block != BlockType::Air {
                                    res = (block, y);
                                    break;
                                }
                            }
                        }
                        self.minimap_height_cache.insert((world_x, world_z), res);
                        res
                    };

                // Calculate color based on mode
                let color = minimap.get_color(block_type, height);

                pixels[dz * display_size + dx] = color;
            }
        }

        egui::ColorImage {
            size: [display_size, display_size],
            pixels,
        }
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
        // Setting the same block again should not duplicate entries
        world.set_block(vector![0, 0, 0], BlockType::Stone);

        let dirty = world.drain_dirty_chunks();
        assert_eq!(dirty.len(), 2);
        assert!(world.dirty_chunks().is_empty());
    }

    #[test]
    fn test_remove_dirty_positions() {
        let mut world = World::new();
        let pos_a = vector![0, 0, 0];
        let pos_b = vector![32, 0, 0];
        let chunk_b = World::world_to_chunk(pos_b);

        world.set_block(pos_a, BlockType::Stone);
        world.set_block(pos_b, BlockType::Dirt);
        assert_eq!(world.dirty_chunks().len(), 2);

        // Remove one entry
        let chunk_a = World::world_to_chunk(pos_a);
        world.remove_dirty_positions(&[chunk_a]);
        let mut remaining: Vec<_> = world
            .dirty_chunks()
            .iter()
            .map(|v| (v.x, v.y, v.z))
            .collect();
        remaining.sort();
        assert_eq!(remaining, vec![(chunk_b.x, chunk_b.y, chunk_b.z)]);

        // Removing again is a no-op
        world.remove_dirty_positions(&[chunk_a]);
        let mut remaining: Vec<_> = world
            .dirty_chunks()
            .iter()
            .map(|v| (v.x, v.y, v.z))
            .collect();
        remaining.sort();
        assert_eq!(remaining, vec![(chunk_b.x, chunk_b.y, chunk_b.z)]);

        // Remove remaining
        world.remove_dirty_positions(&[chunk_b]);
        let remaining: Vec<_> = world
            .dirty_chunks()
            .iter()
            .map(|v| (v.x, v.y, v.z))
            .collect();
        assert!(
            remaining.is_empty(),
            "dirty_chunks should be empty, found: {:?}",
            remaining
        );
    }

    #[test]
    fn test_stair_shapes_front_back_neighbors() {
        use crate::sub_voxel::{ModelRegistry, StairShape};
        let mut world = World::new();

        // Stair corner detection logic:
        // - Outer corner (single raised quadrant): Neighbor at our LOW/front side, facing perpendicular
        // - Inner corner (L-shaped top with pocket): Neighbor at our HIGH/back side, facing perpendicular
        //
        // Rotation 0: facing -Z, left_dir = -X, right_dir = +X
        // Rotation 1: facing +X, left_dir = -Z, right_dir = +Z
        // Rotation 2: facing +Z, left_dir = +X, right_dir = -X
        // Rotation 3: facing -X, left_dir = +Z, right_dir = -Z

        let straight_id = ModelRegistry::stairs_model_id(StairShape::Straight, false);

        // Case 1: INNER corner - neighbor at our HIGH/back side, neighbor faces our left
        // Our stair at (0,0,0): rotation 0 → facing (-Z), left_dir = (-X)
        // Neighbor at (0,0,1): rotation 3 → facing (-X) = our left_dir
        // back_neighbor == left_dir → InnerRight
        world.set_model_block(vector![0, 0, 0], straight_id, 0, false);
        world.set_model_block(vector![0, 0, 1], straight_id, 3, false);

        world.update_stair_shape_at(vector![0, 0, 0]);

        let data = world.get_model_data(vector![0, 0, 0]).unwrap();
        let expected_id = ModelRegistry::stairs_model_id(StairShape::InnerRight, false);
        assert_eq!(
            data.model_id, expected_id,
            "Back neighbor facing left → InnerRight"
        );

        // Case 2: OUTER corner - neighbor at our LOW/front side, neighbor faces our left
        // Our stair at (10,0,0): rotation 0 → facing (-Z), left_dir = (-X)
        // Neighbor at (10,0,-1): rotation 3 → facing (-X) = our left_dir
        // front_neighbor == left_dir → OuterLeft
        world.set_model_block(vector![10, 0, 0], straight_id, 0, false);
        world.set_model_block(vector![10, 0, -1], straight_id, 3, false);

        world.update_stair_shape_at(vector![10, 0, 0]);

        let data = world.get_model_data(vector![10, 0, 0]).unwrap();
        let expected_id = ModelRegistry::stairs_model_id(StairShape::OuterLeft, false);
        assert_eq!(
            data.model_id, expected_id,
            "Front neighbor facing left → OuterLeft"
        );

        // Case 3: OUTER corner - neighbor at our LOW/front side, neighbor faces our right
        // Our stair at (20,0,0): rotation 0 → facing (-Z), right_dir = (+X)
        // Neighbor at (20,0,-1): rotation 1 → facing (+X) = our right_dir
        // front_neighbor == right_dir → OuterRight
        world.set_model_block(vector![20, 0, 0], straight_id, 0, false);
        world.set_model_block(vector![20, 0, -1], straight_id, 1, false);

        world.update_stair_shape_at(vector![20, 0, 0]);

        let data = world.get_model_data(vector![20, 0, 0]).unwrap();
        let expected_id = ModelRegistry::stairs_model_id(StairShape::OuterRight, false);
        assert_eq!(
            data.model_id, expected_id,
            "Front neighbor facing right → OuterRight"
        );

        // Case 4: INNER corner - neighbor at our HIGH/back side, neighbor faces our right
        // Our stair at (30,0,0): rotation 0 → facing (-Z), right_dir = (+X)
        // Neighbor at (30,0,1): rotation 1 → facing (+X) = our right_dir
        // back_neighbor == right_dir → InnerLeft
        world.set_model_block(vector![30, 0, 0], straight_id, 0, false);
        world.set_model_block(vector![30, 0, 1], straight_id, 1, false);

        world.update_stair_shape_at(vector![30, 0, 0]);

        let data = world.get_model_data(vector![30, 0, 0]).unwrap();
        let expected_id = ModelRegistry::stairs_model_id(StairShape::InnerLeft, false);
        assert_eq!(
            data.model_id, expected_id,
            "Back neighbor facing right → InnerLeft"
        );
    }

    #[test]
    fn test_stair_shapes_left_right_neighbors() {
        use crate::sub_voxel::{ModelRegistry, StairShape};
        let mut world = World::new();

        let straight_id = ModelRegistry::stairs_model_id(StairShape::Straight, false);

        // Case 1: Left neighbor - neighbor faces away (our right_dir) → InnerRight
        // Our stair at (0,0,0): rotation 0 → facing (-Z), left_dir = (-X), right_dir = (+X)
        // Left neighbor at (-1,0,0): rotation 1 → facing (+X) = our right_dir
        world.set_model_block(vector![0, 0, 0], straight_id, 0, false);
        world.set_model_block(vector![-1, 0, 0], straight_id, 1, false);

        world.update_stair_shape_at(vector![0, 0, 0]);

        let data = world.get_model_data(vector![0, 0, 0]).unwrap();
        let expected_id = ModelRegistry::stairs_model_id(StairShape::InnerRight, false);
        assert_eq!(
            data.model_id, expected_id,
            "Left neighbor facing away (right_dir) → InnerRight"
        );

        // Case 2: Left neighbor facing our left_dir (parallel, no corner)
        // For rotation 0: left_dir = -X, neighbor facing -X = rotation 3
        world.set_model_block(vector![20, 0, 0], straight_id, 0, false);
        world.set_model_block(vector![19, 0, 0], straight_id, 3, false);

        world.update_stair_shape_at(vector![20, 0, 0]);

        let data = world.get_model_data(vector![20, 0, 0]).unwrap();
        assert_eq!(
            data.model_id, straight_id,
            "Left neighbor facing same direction as our left_dir → stays Straight"
        );

        // Case 3: Right neighbor - neighbor faces away (our left_dir) → InnerLeft
        // Our stair at (30,0,0): rotation 0 → facing (-Z), left_dir = (-X), right_dir = (+X)
        // Right neighbor at (31,0,0): rotation 3 → facing (-X) = our left_dir
        world.set_model_block(vector![30, 0, 0], straight_id, 0, false);
        world.set_model_block(vector![31, 0, 0], straight_id, 3, false);

        world.update_stair_shape_at(vector![30, 0, 0]);

        let data = world.get_model_data(vector![30, 0, 0]).unwrap();
        let expected_id = ModelRegistry::stairs_model_id(StairShape::InnerLeft, false);
        assert_eq!(
            data.model_id, expected_id,
            "Right neighbor facing away (left_dir) → InnerLeft"
        );

        // Case 4: Right neighbor facing our right_dir (parallel, no corner)
        // For rotation 0: right_dir = +X, neighbor facing +X = rotation 1
        world.set_model_block(vector![40, 0, 0], straight_id, 0, false);
        world.set_model_block(vector![41, 0, 0], straight_id, 1, false);

        world.update_stair_shape_at(vector![40, 0, 0]);

        let data = world.get_model_data(vector![40, 0, 0]).unwrap();
        assert_eq!(
            data.model_id, straight_id,
            "Right neighbor facing same direction as our right_dir → stays Straight"
        );
    }

    #[test]
    fn test_stair_shapes_parallel_neighbors_stay_straight() {
        use crate::sub_voxel::{ModelRegistry, StairShape};
        let mut world = World::new();

        let straight_id = ModelRegistry::stairs_model_id(StairShape::Straight, false);

        // Two stairs side by side facing the same direction should both stay straight
        // Our stair at (0,0,0): rotation 0 (facing -Z)
        // Neighbor at (1,0,0): rotation 0 (facing -Z) - parallel, not perpendicular
        world.set_model_block(vector![0, 0, 0], straight_id, 0, false);
        world.set_model_block(vector![1, 0, 0], straight_id, 0, false);

        world.update_stair_shape_at(vector![0, 0, 0]);

        let data = world.get_model_data(vector![0, 0, 0]).unwrap();
        assert_eq!(
            data.model_id, straight_id,
            "Parallel neighbors should stay straight"
        );

        // Also test the other neighbor
        world.update_stair_shape_at(vector![1, 0, 0]);
        let data = world.get_model_data(vector![1, 0, 0]).unwrap();
        assert_eq!(
            data.model_id, straight_id,
            "Parallel neighbors should stay straight"
        );

        // Test a row of 3 stairs all facing same direction
        world.set_model_block(vector![10, 0, 0], straight_id, 0, false);
        world.set_model_block(vector![11, 0, 0], straight_id, 0, false);
        world.set_model_block(vector![12, 0, 0], straight_id, 0, false);

        world.update_stair_shape_at(vector![11, 0, 0]); // Middle one
        let data = world.get_model_data(vector![11, 0, 0]).unwrap();
        assert_eq!(
            data.model_id, straight_id,
            "Middle of row should stay straight"
        );
    }

    #[test]
    fn test_stair_shapes_different_rotations() {
        use crate::sub_voxel::{ModelRegistry, StairShape};
        let mut world = World::new();

        let straight_id = ModelRegistry::stairs_model_id(StairShape::Straight, false);

        // Test with rotation 2 (facing +Z)
        // Our stair at (0,0,0): rotation 2 → facing (+Z), left_dir = (+X), right_dir = (-X)
        // Neighbor at (0,0,-1): this is at our HIGH/back side (opposite of +Z)
        // Neighbor with rotation 1 → facing (+X) = our left_dir
        // back_neighbor == left_dir → InnerRight
        world.set_model_block(vector![0, 0, 0], straight_id, 2, false);
        world.set_model_block(vector![0, 0, -1], straight_id, 1, false);

        world.update_stair_shape_at(vector![0, 0, 0]);

        let data = world.get_model_data(vector![0, 0, 0]).unwrap();
        let expected_id = ModelRegistry::stairs_model_id(StairShape::InnerRight, false);
        assert_eq!(
            data.model_id, expected_id,
            "Rotation 2 inner corner should work"
        );

        // Test with rotation 1 (facing +X)
        // Our stair at (10,0,0): rotation 1 → facing (+X), left_dir = (-Z), right_dir = (+Z)
        // Neighbor at (9,0,0): this is at our HIGH/back side (opposite of +X)
        // Neighbor with rotation 0 → facing (-Z) = our left_dir
        // back_neighbor == left_dir → InnerRight
        world.set_model_block(vector![10, 0, 0], straight_id, 1, false);
        world.set_model_block(vector![9, 0, 0], straight_id, 0, false);

        world.update_stair_shape_at(vector![10, 0, 0]);

        let data = world.get_model_data(vector![10, 0, 0]).unwrap();
        let expected_id = ModelRegistry::stairs_model_id(StairShape::InnerRight, false);
        assert_eq!(
            data.model_id, expected_id,
            "Rotation 1 inner corner should work"
        );
    }

    #[test]
    fn test_stair_inner_priority_over_outer() {
        use crate::sub_voxel::{ModelRegistry, StairShape};
        let mut world = World::new();

        let straight_id = ModelRegistry::stairs_model_id(StairShape::Straight, false);

        // When a stair has both front and back neighbors that could trigger corners,
        // outer (front) takes priority over inner (back)
        // Our stair at (0,0,0): rotation 0 → facing (-Z)
        // Front neighbor at (0,0,-1): rotation 3 → facing (-X) = left_dir → OuterLeft
        // Back neighbor at (0,0,1): rotation 1 → facing (+X) = right_dir → would be InnerLeft
        // Outer should win (front is checked first)
        world.set_model_block(vector![0, 0, 0], straight_id, 0, false);
        world.set_model_block(vector![0, 0, -1], straight_id, 3, false); // Front - triggers outer
        world.set_model_block(vector![0, 0, 1], straight_id, 1, false); // Back - would trigger inner

        world.update_stair_shape_at(vector![0, 0, 0]);

        let data = world.get_model_data(vector![0, 0, 0]).unwrap();
        let expected_id = ModelRegistry::stairs_model_id(StairShape::OuterLeft, false);
        assert_eq!(
            data.model_id, expected_id,
            "Outer corner takes priority over inner"
        );
    }

    #[test]
    fn test_stair_no_corner_with_opposite_facing() {
        use crate::sub_voxel::{ModelRegistry, StairShape};
        let mut world = World::new();

        let straight_id = ModelRegistry::stairs_model_id(StairShape::Straight, false);

        // Stairs facing opposite directions (180° apart) should not form corners
        // Our stair at (0,0,0): rotation 0 (facing -Z)
        // Neighbor at (0,0,1): rotation 2 (facing +Z) - directly opposite, not perpendicular
        world.set_model_block(vector![0, 0, 0], straight_id, 0, false);
        world.set_model_block(vector![0, 0, 1], straight_id, 2, false);

        world.update_stair_shape_at(vector![0, 0, 0]);

        let data = world.get_model_data(vector![0, 0, 0]).unwrap();
        assert_eq!(
            data.model_id, straight_id,
            "Opposite facing neighbors stay straight"
        );

        // Same test for front neighbor with opposite facing
        world.set_model_block(vector![10, 0, 0], straight_id, 0, false);
        world.set_model_block(vector![10, 0, -1], straight_id, 2, false); // Facing toward us

        world.update_stair_shape_at(vector![10, 0, 0]);

        let data = world.get_model_data(vector![10, 0, 0]).unwrap();
        assert_eq!(
            data.model_id, straight_id,
            "Opposite facing front neighbor stays straight"
        );
    }

    #[test]
    fn test_stair_shapes_inverted_stairs() {
        use crate::sub_voxel::{ModelRegistry, StairShape};
        let mut world = World::new();

        let straight_inverted = ModelRegistry::stairs_model_id(StairShape::Straight, true);

        // Inverted stairs should also form corners with other inverted stairs
        // For inverted stairs, both Inner↔Outer AND Left↔Right are flipped
        // Case 1: Back neighbor → OuterLeft (inverted) - would be InnerRight if not inverted
        world.set_model_block(vector![0, 0, 0], straight_inverted, 0, false);
        world.set_model_block(vector![0, 0, 1], straight_inverted, 3, false);

        world.update_stair_shape_at(vector![0, 0, 0]);

        let data = world.get_model_data(vector![0, 0, 0]).unwrap();
        let expected_id = ModelRegistry::stairs_model_id(StairShape::OuterLeft, true);
        assert_eq!(
            data.model_id, expected_id,
            "Inverted: back neighbor facing left → OuterLeft (flipped from InnerRight)"
        );

        // Case 2: Front neighbor → InnerRight (inverted) - would be OuterLeft if not inverted
        world.set_model_block(vector![10, 0, 0], straight_inverted, 0, false);
        world.set_model_block(vector![10, 0, -1], straight_inverted, 3, false);

        world.update_stair_shape_at(vector![10, 0, 0]);

        let data = world.get_model_data(vector![10, 0, 0]).unwrap();
        let expected_id = ModelRegistry::stairs_model_id(StairShape::InnerRight, true);
        assert_eq!(
            data.model_id, expected_id,
            "Inverted: front neighbor facing left → InnerRight (flipped from OuterLeft)"
        );

        // Case 3: Inverted stair should NOT form corner with non-inverted stair
        let straight_normal = ModelRegistry::stairs_model_id(StairShape::Straight, false);
        world.set_model_block(vector![20, 0, 0], straight_inverted, 0, false);
        world.set_model_block(vector![20, 0, 1], straight_normal, 3, false); // Non-inverted neighbor

        world.update_stair_shape_at(vector![20, 0, 0]);

        let data = world.get_model_data(vector![20, 0, 0]).unwrap();
        assert_eq!(
            data.model_id, straight_inverted,
            "Inverted stair should not corner with non-inverted neighbor"
        );

        // Case 4: Non-inverted stair should NOT form corner with inverted stair
        world.set_model_block(vector![30, 0, 0], straight_normal, 0, false);
        world.set_model_block(vector![30, 0, 1], straight_inverted, 3, false); // Inverted neighbor

        world.update_stair_shape_at(vector![30, 0, 0]);

        let data = world.get_model_data(vector![30, 0, 0]).unwrap();
        assert_eq!(
            data.model_id, straight_normal,
            "Non-inverted stair should not corner with inverted neighbor"
        );
    }

    #[test]
    fn test_stair_shapes_all_rotations_outer_corners() {
        use crate::sub_voxel::{ModelRegistry, StairShape};
        let mut world = World::new();

        let straight_id = ModelRegistry::stairs_model_id(StairShape::Straight, false);

        // Test outer corners for all 4 rotations with front neighbors
        // Rotation 0: facing -Z, front at -Z, left = -X, right = +X
        // Rotation 1: facing +X, front at +X, left = -Z, right = +Z
        // Rotation 2: facing +Z, front at +Z, left = +X, right = -X
        // Rotation 3: facing -X, front at -X, left = +Z, right = -Z

        struct TestCase {
            rotation: u8,
            front_offset: [i32; 3],
            neighbor_rot_for_left: u8, // neighbor rotation to face our left_dir
            neighbor_rot_for_right: u8, // neighbor rotation to face our right_dir
        }

        let cases = [
            TestCase {
                rotation: 0,
                front_offset: [0, 0, -1],
                neighbor_rot_for_left: 3,  // faces -X
                neighbor_rot_for_right: 1, // faces +X
            },
            TestCase {
                rotation: 1,
                front_offset: [1, 0, 0],
                neighbor_rot_for_left: 0,  // faces -Z
                neighbor_rot_for_right: 2, // faces +Z
            },
            TestCase {
                rotation: 2,
                front_offset: [0, 0, 1],
                neighbor_rot_for_left: 1,  // faces +X
                neighbor_rot_for_right: 3, // faces -X
            },
            TestCase {
                rotation: 3,
                front_offset: [-1, 0, 0],
                neighbor_rot_for_left: 2,  // faces +Z
                neighbor_rot_for_right: 0, // faces -Z
            },
        ];

        for (i, case) in cases.iter().enumerate() {
            let base_x = (i as i32) * 20;

            // Test OuterLeft: front neighbor faces our left_dir
            let pos = vector![base_x, 0, 0];
            let front_pos = vector![
                base_x + case.front_offset[0],
                case.front_offset[1],
                case.front_offset[2]
            ];

            world.set_model_block(pos, straight_id, case.rotation, false);
            world.set_model_block(front_pos, straight_id, case.neighbor_rot_for_left, false);

            world.update_stair_shape_at(pos);

            let data = world.get_model_data(pos).unwrap();
            let expected_id = ModelRegistry::stairs_model_id(StairShape::OuterLeft, false);
            assert_eq!(
                data.model_id, expected_id,
                "Rotation {}: front neighbor facing left → OuterLeft",
                case.rotation
            );

            // Test OuterRight: front neighbor faces our right_dir
            let pos = vector![base_x + 10, 0, 0];
            let front_pos = vector![
                base_x + 10 + case.front_offset[0],
                case.front_offset[1],
                case.front_offset[2]
            ];

            world.set_model_block(pos, straight_id, case.rotation, false);
            world.set_model_block(front_pos, straight_id, case.neighbor_rot_for_right, false);

            world.update_stair_shape_at(pos);

            let data = world.get_model_data(pos).unwrap();
            let expected_id = ModelRegistry::stairs_model_id(StairShape::OuterRight, false);
            assert_eq!(
                data.model_id, expected_id,
                "Rotation {}: front neighbor facing right → OuterRight",
                case.rotation
            );
        }
    }

    #[test]
    fn test_stair_shapes_all_rotations_inner_corners() {
        use crate::sub_voxel::{ModelRegistry, StairShape};
        let mut world = World::new();

        let straight_id = ModelRegistry::stairs_model_id(StairShape::Straight, false);

        // Test inner corners for all 4 rotations with back neighbors
        struct TestCase {
            rotation: u8,
            back_offset: [i32; 3],
            neighbor_rot_for_left: u8,
            neighbor_rot_for_right: u8,
        }

        let cases = [
            TestCase {
                rotation: 0,
                back_offset: [0, 0, 1],
                neighbor_rot_for_left: 3,
                neighbor_rot_for_right: 1,
            },
            TestCase {
                rotation: 1,
                back_offset: [-1, 0, 0],
                neighbor_rot_for_left: 0,
                neighbor_rot_for_right: 2,
            },
            TestCase {
                rotation: 2,
                back_offset: [0, 0, -1],
                neighbor_rot_for_left: 1,
                neighbor_rot_for_right: 3,
            },
            TestCase {
                rotation: 3,
                back_offset: [1, 0, 0],
                neighbor_rot_for_left: 2,
                neighbor_rot_for_right: 0,
            },
        ];

        for (i, case) in cases.iter().enumerate() {
            let base_x = (i as i32) * 20;

            // Test InnerRight: back neighbor faces our left_dir
            let pos = vector![base_x, 0, 0];
            let back_pos = vector![
                base_x + case.back_offset[0],
                case.back_offset[1],
                case.back_offset[2]
            ];

            world.set_model_block(pos, straight_id, case.rotation, false);
            world.set_model_block(back_pos, straight_id, case.neighbor_rot_for_left, false);

            world.update_stair_shape_at(pos);

            let data = world.get_model_data(pos).unwrap();
            let expected_id = ModelRegistry::stairs_model_id(StairShape::InnerRight, false);
            assert_eq!(
                data.model_id, expected_id,
                "Rotation {}: back neighbor facing left → InnerRight",
                case.rotation
            );

            // Test InnerLeft: back neighbor faces our right_dir
            let pos = vector![base_x + 10, 0, 0];
            let back_pos = vector![
                base_x + 10 + case.back_offset[0],
                case.back_offset[1],
                case.back_offset[2]
            ];

            world.set_model_block(pos, straight_id, case.rotation, false);
            world.set_model_block(back_pos, straight_id, case.neighbor_rot_for_right, false);

            world.update_stair_shape_at(pos);

            let data = world.get_model_data(pos).unwrap();
            let expected_id = ModelRegistry::stairs_model_id(StairShape::InnerLeft, false);
            assert_eq!(
                data.model_id, expected_id,
                "Rotation {}: back neighbor facing right → InnerLeft",
                case.rotation
            );
        }
    }

    #[test]
    fn test_stair_neighbor_removal_resets_shape() {
        use crate::sub_voxel::{ModelRegistry, StairShape};
        let mut world = World::new();

        let straight_id = ModelRegistry::stairs_model_id(StairShape::Straight, false);

        // Place two stairs that form a corner
        world.set_model_block(vector![0, 0, 0], straight_id, 0, false);
        world.set_model_block(vector![0, 0, -1], straight_id, 3, false);

        world.update_stair_shape_at(vector![0, 0, 0]);

        let data = world.get_model_data(vector![0, 0, 0]).unwrap();
        let outer_left = ModelRegistry::stairs_model_id(StairShape::OuterLeft, false);
        assert_eq!(data.model_id, outer_left, "Should form OuterLeft corner");

        // Remove the neighbor
        world.set_block(vector![0, 0, -1], BlockType::Air);

        // Update the stair shape
        world.update_stair_shape_at(vector![0, 0, 0]);

        let data = world.get_model_data(vector![0, 0, 0]).unwrap();
        assert_eq!(
            data.model_id, straight_id,
            "Should reset to Straight after neighbor removal"
        );
    }

    /// Test ceiling (inverted) stairs form correct outer corners at all rotations.
    /// For inverted stairs, the shape mapping is flipped: InnerLeft↔OuterRight, InnerRight↔OuterLeft
    #[test]
    fn test_ceiling_stair_shapes_all_rotations_outer_corners() {
        use crate::sub_voxel::{ModelRegistry, StairShape};
        let mut world = World::new();

        let straight_inv = ModelRegistry::stairs_model_id(StairShape::Straight, true);

        // Test configurations: (main_rotation, neighbor_rotation, neighbor_offset, expected_shape)
        // For inverted stairs, OuterLeft/OuterRight are produced where floor stairs would get InnerRight/InnerLeft
        let test_cases = [
            // Rotation 0: faces -Z. Front neighbor at -Z facing perpendicular creates outer corner
            // Floor: front neighbor facing left (rot 3) → OuterLeft
            // Ceiling: flipped → InnerRight
            (
                0u8,
                3u8,
                vector![0i32, 0, -1],
                StairShape::InnerRight,
                "rot0 front-left",
            ),
            (
                0,
                1,
                vector![0, 0, -1],
                StairShape::InnerLeft,
                "rot0 front-right",
            ),
            // Rotation 1: faces +X
            (
                1,
                0,
                vector![1, 0, 0],
                StairShape::InnerRight,
                "rot1 front-left",
            ),
            (
                1,
                2,
                vector![1, 0, 0],
                StairShape::InnerLeft,
                "rot1 front-right",
            ),
            // Rotation 2: faces +Z
            (
                2,
                1,
                vector![0, 0, 1],
                StairShape::InnerRight,
                "rot2 front-left",
            ),
            (
                2,
                3,
                vector![0, 0, 1],
                StairShape::InnerLeft,
                "rot2 front-right",
            ),
            // Rotation 3: faces -X
            (
                3,
                2,
                vector![-1, 0, 0],
                StairShape::InnerRight,
                "rot3 front-left",
            ),
            (
                3,
                0,
                vector![-1, 0, 0],
                StairShape::InnerLeft,
                "rot3 front-right",
            ),
        ];

        for (i, (main_rot, neighbor_rot, offset, expected_shape, desc)) in
            test_cases.iter().enumerate()
        {
            let base = vector![i as i32 * 10, 0, 0];
            let neighbor_pos = base + offset;

            world.set_model_block(base, straight_inv, *main_rot, false);
            world.set_model_block(neighbor_pos, straight_inv, *neighbor_rot, false);

            world.update_stair_shape_at(base);

            let data = world.get_model_data(base).unwrap();
            let expected_id = ModelRegistry::stairs_model_id(*expected_shape, true);
            assert_eq!(
                data.model_id, expected_id,
                "Ceiling outer corner {}: expected {:?}",
                desc, expected_shape
            );
        }
    }

    /// Test ceiling (inverted) stairs form correct inner corners at all rotations.
    #[test]
    fn test_ceiling_stair_shapes_all_rotations_inner_corners() {
        use crate::sub_voxel::{ModelRegistry, StairShape};
        let mut world = World::new();

        let straight_inv = ModelRegistry::stairs_model_id(StairShape::Straight, true);

        // Test configurations for inner corners (back neighbor creates inner corner)
        // For inverted stairs, InnerLeft/InnerRight are produced where floor stairs would get OuterRight/OuterLeft
        let test_cases = [
            // Rotation 0: faces -Z. Back neighbor at +Z facing perpendicular creates inner corner
            // Floor: back neighbor facing left (rot 3) → InnerRight
            // Ceiling: flipped → OuterLeft
            (
                0u8,
                3u8,
                vector![0i32, 0, 1],
                StairShape::OuterLeft,
                "rot0 back-left",
            ),
            (
                0,
                1,
                vector![0, 0, 1],
                StairShape::OuterRight,
                "rot0 back-right",
            ),
            // Rotation 1: faces +X
            (
                1,
                0,
                vector![-1, 0, 0],
                StairShape::OuterLeft,
                "rot1 back-left",
            ),
            (
                1,
                2,
                vector![-1, 0, 0],
                StairShape::OuterRight,
                "rot1 back-right",
            ),
            // Rotation 2: faces +Z
            (
                2,
                1,
                vector![0, 0, -1],
                StairShape::OuterLeft,
                "rot2 back-left",
            ),
            (
                2,
                3,
                vector![0, 0, -1],
                StairShape::OuterRight,
                "rot2 back-right",
            ),
            // Rotation 3: faces -X
            (
                3,
                2,
                vector![1, 0, 0],
                StairShape::OuterLeft,
                "rot3 back-left",
            ),
            (
                3,
                0,
                vector![1, 0, 0],
                StairShape::OuterRight,
                "rot3 back-right",
            ),
        ];

        for (i, (main_rot, neighbor_rot, offset, expected_shape, desc)) in
            test_cases.iter().enumerate()
        {
            let base = vector![i as i32 * 10, 0, 0];
            let neighbor_pos = base + offset;

            world.set_model_block(base, straight_inv, *main_rot, false);
            world.set_model_block(neighbor_pos, straight_inv, *neighbor_rot, false);

            world.update_stair_shape_at(base);

            let data = world.get_model_data(base).unwrap();
            let expected_id = ModelRegistry::stairs_model_id(*expected_shape, true);
            assert_eq!(
                data.model_id, expected_id,
                "Ceiling inner corner {}: expected {:?}",
                desc, expected_shape
            );
        }
    }

    /// Test that floor and ceiling stairs don't form corners with each other
    #[test]
    fn test_floor_ceiling_stairs_dont_mix() {
        use crate::sub_voxel::{ModelRegistry, StairShape};
        let mut world = World::new();

        let straight_floor = ModelRegistry::stairs_model_id(StairShape::Straight, false);
        let straight_ceiling = ModelRegistry::stairs_model_id(StairShape::Straight, true);

        // Place floor stair with ceiling neighbor that would form corner if same type
        world.set_model_block(vector![0, 0, 0], straight_floor, 0, false);
        world.set_model_block(vector![0, 0, -1], straight_ceiling, 3, false);

        world.update_stair_shape_at(vector![0, 0, 0]);

        let data = world.get_model_data(vector![0, 0, 0]).unwrap();
        assert_eq!(
            data.model_id, straight_floor,
            "Floor stair should stay straight when neighbor is ceiling stair"
        );

        // Place ceiling stair with floor neighbor
        world.set_model_block(vector![10, 0, 0], straight_ceiling, 0, false);
        world.set_model_block(vector![10, 0, -1], straight_floor, 3, false);

        world.update_stair_shape_at(vector![10, 0, 0]);

        let data = world.get_model_data(vector![10, 0, 0]).unwrap();
        assert_eq!(
            data.model_id, straight_ceiling,
            "Ceiling stair should stay straight when neighbor is floor stair"
        );
    }
}

//! World management for the voxel game.
//!
//! The World struct manages a collection of chunks and provides
//! methods for accessing and modifying blocks at world coordinates.

#![allow(dead_code)]

use crate::chunk::{BlockModelData, BlockType, CHUNK_SIZE, Chunk};
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
use crate::sub_voxel::ModelRegistry;

impl World {
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
                pos_radius: [tex_x, tex_y, tex_z, 12.0], // Torch-like radius
                color_intensity: [1.0, 0.8, 0.5, 1.5],   // Warm torch color
            });
        }

        // Iterate over all loaded chunks
        for (chunk_pos, chunk) in self.chunks() {
            // Scan chunk for light-emitting blocks
            for lx in 0..CHUNK_SIZE {
                for ly in 0..CHUNK_SIZE {
                    for lz in 0..CHUNK_SIZE {
                        let block = chunk.get_block(lx, ly, lz);

                        // Check for Model blocks with emission
                        if block == BlockType::Model {
                            if let Some(model_data) = chunk.get_model_data(lx, ly, lz) {
                                if let Some(model) = model_registry.get(model_data.model_id) {
                                    if let Some(emission) = &model.emission {
                                        // Calculate world position (center of block)
                                        let world_x = chunk_pos.x * CHUNK_SIZE as i32 + lx as i32;
                                        let world_y = chunk_pos.y * CHUNK_SIZE as i32 + ly as i32;
                                        let world_z = chunk_pos.z * CHUNK_SIZE as i32 + lz as i32;

                                        // Convert to texture coordinates
                                        let tex_x = (world_x - texture_origin.x) as f32 + 0.5;
                                        let tex_y = (world_y - texture_origin.y) as f32 + 0.5;
                                        let tex_z = (world_z - texture_origin.z) as f32 + 0.5;

                                        let r = emission.r as f32 / 255.0;
                                        let g = emission.g as f32 / 255.0;
                                        let b = emission.b as f32 / 255.0;

                                        lights.push(GpuLight {
                                            pos_radius: [tex_x, tex_y, tex_z, 10.0], // Torch radius
                                            color_intensity: [r, g, b, 1.2],
                                        });

                                        if lights.len() >= crate::gpu_resources::MAX_LIGHTS {
                                            return lights;
                                        }
                                    }
                                }
                            }
                        }
                        // Also check regular block light properties (for future non-model lights)
                        else if let Some((color, radius)) = block.light_properties() {
                            // Calculate world position (center of block)
                            let world_x = chunk_pos.x * CHUNK_SIZE as i32 + lx as i32;
                            let world_y = chunk_pos.y * CHUNK_SIZE as i32 + ly as i32;
                            let world_z = chunk_pos.z * CHUNK_SIZE as i32 + lz as i32;

                            // Convert to texture coordinates (shader operates in texture space)
                            let tex_x = (world_x - texture_origin.x) as f32 + 0.5;
                            let tex_y = (world_y - texture_origin.y) as f32 + 0.5;
                            let tex_z = (world_z - texture_origin.z) as f32 + 0.5;

                            lights.push(GpuLight {
                                pos_radius: [tex_x, tex_y, tex_z, radius],
                                color_intensity: [color[0], color[1], color[2], 1.2],
                            });

                            if lights.len() >= crate::gpu_resources::MAX_LIGHTS {
                                return lights;
                            }
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

    /// Sets a model block at world coordinates with the given model_id and rotation.
    ///
    /// This sets the block type to Model and stores the model metadata.
    /// If the chunk doesn't exist, it will be created.
    pub fn set_model_block(&mut self, world_pos: WorldPos, model_id: u8, rotation: u8) {
        let chunk_pos = Self::world_to_chunk(world_pos);
        let (lx, ly, lz) = Self::world_to_local(world_pos);

        let is_new_chunk = !self.chunks.contains_key(&chunk_pos);
        let chunk = self.chunks.entry(chunk_pos).or_default();
        let was_dirty = chunk.dirty;
        chunk.set_model_block(lx, ly, lz, model_id, rotation);

        // Add to dirty queue if this is a new chunk or the modification made it dirty
        if is_new_chunk || (chunk.dirty && !was_dirty) {
            self.dirty_chunks.push(chunk_pos);
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
                            self.set_model_block(neighbor_pos, new_model_id, 0);
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
                            self.set_model_block(neighbor_pos, new_model_id, data.rotation);
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

        let dirty = world.drain_dirty_chunks();
        assert_eq!(dirty.len(), 2);
        assert!(world.dirty_chunks().is_empty());
    }
}

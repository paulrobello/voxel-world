//! World storage and chunk management.

use super::{ChunkPos, WorldPos};
use crate::chunk::{BlockModelData, BlockType, CHUNK_SIZE, Chunk};
use crate::terrain_gen::OverflowBlock;
use nalgebra::{Vector3, vector};
use std::collections::{HashMap, HashSet};

/// The voxel world, containing all loaded chunks.
pub struct World {
    /// All currently loaded chunks, keyed by chunk position.
    pub(super) chunks: HashMap<ChunkPos, Chunk>,

    // ---- Dual dirty representation (ARC-M11) ---------------------------------
    // "Which chunks need GPU re-upload" is tracked two ways, kept in sync by
    // convention (there is no compiler link between them):
    //
    // 1. `Chunk.dirty: bool` (in `chunk.rs`) — the per-chunk source of truth.
    // 2. `dirty_chunks` + `dirty_set` here — a deduped work queue of positions
    //    drained each frame by `world_streaming::upload_world_to_gpu`.
    //
    // Convention: every `World` mutator (`set_block`, `set_water_block`,
    // `set_model_block`, ...) calls `push_dirty(pos)` exactly on the
    // clean→dirty transition (guarded by the `was_dirty` snapshot), so the
    // queue mirrors the bools on the happy path. The upload path drains the
    // queue (`drain_dirty_chunks_limit`), then clears each chunk's bool via
    // `chunk.mark_clean()` ONLY after a successful upload; on failure the bool
    // stays true and the queue entry is dropped, to be re-pushed by the next
    // mutation or an origin-shift `requeue_dirty`, or caught by the one-shot
    // full scan `upload_all_dirty_chunks` (which reads `Chunk.dirty` directly
    // and bypasses this queue entirely — invoked from `main.rs` at startup).
    // `remove_dirty_positions` tidies any position re-pushed between drain and
    // upload.
    //
    // Invariant: the bool is authoritative; the queue is a hint. A chunk with
    // `dirty == true` may transiently be absent from the queue (e.g. right
    // after a failed incremental upload), but a position in the queue always
    // has `Chunk.dirty == true`.
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
    pub fn world_to_chunk<S: Into<WorldPos>>(world_pos: S) -> ChunkPos {
        let world_pos = world_pos.into();
        ChunkPos(vector![
            world_pos.x.div_euclid(CHUNK_SIZE as i32),
            world_pos.y.div_euclid(CHUNK_SIZE as i32),
            world_pos.z.div_euclid(CHUNK_SIZE as i32)
        ])
    }

    /// Converts world coordinates to local chunk coordinates.
    #[inline]
    pub fn world_to_local<S: Into<WorldPos>>(world_pos: S) -> (usize, usize, usize) {
        let world_pos = world_pos.into();
        (
            world_pos.x.rem_euclid(CHUNK_SIZE as i32) as usize,
            world_pos.y.rem_euclid(CHUNK_SIZE as i32) as usize,
            world_pos.z.rem_euclid(CHUNK_SIZE as i32) as usize,
        )
    }

    /// Converts chunk position to world position (bottom-left-back corner).
    ///
    /// Returns a plain `Vector3<i32>` (the historical block-coordinate type) so
    /// the many callers that consume the result as a `Vector3` keep compiling
    /// unchanged. The input remains type-safe: passing a `WorldPos` is a
    /// compile error because there is intentionally no `Into<ChunkPos>` for it.
    #[inline]
    pub fn chunk_to_world<S: Into<ChunkPos>>(chunk_pos: S) -> Vector3<i32> {
        let chunk_pos = chunk_pos.into();
        vector![
            chunk_pos.x * CHUNK_SIZE as i32,
            chunk_pos.y * CHUNK_SIZE as i32,
            chunk_pos.z * CHUNK_SIZE as i32
        ]
    }

    /// Gets a reference to a chunk if it exists.
    pub fn get_chunk<S: Into<ChunkPos>>(&self, chunk_pos: S) -> Option<&Chunk> {
        let chunk_pos = chunk_pos.into();
        self.chunks.get(&chunk_pos)
    }

    /// Gets a mutable reference to a chunk if it exists.
    pub fn get_chunk_mut<S: Into<ChunkPos>>(&mut self, chunk_pos: S) -> Option<&mut Chunk> {
        let chunk_pos = chunk_pos.into();
        self.chunks.get_mut(&chunk_pos)
    }

    /// Checks if a chunk exists at the given position.
    pub fn has_chunk<S: Into<ChunkPos>>(&self, chunk_pos: S) -> bool {
        let chunk_pos = chunk_pos.into();
        self.chunks.contains_key(&chunk_pos)
    }

    /// Inserts a chunk at the given position.
    pub fn insert_chunk<S: Into<ChunkPos>>(&mut self, chunk_pos: S, mut chunk: Chunk) {
        let chunk_pos = chunk_pos.into();
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
                    // CRITICAL: Use set_block (NOT set_block_generated) to mark persistence_dirty.
                    // Overflow blocks are procedural, but for multiplayer sync we MUST send
                    // the actual chunk data instead of ChunkGenerateLocal. If we don't mark
                    // dirty, the server will tell clients to generate locally, but clients
                    // generate chunks in different orders, causing cross-chunk trees to be
                    // cut off at boundaries.
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
    pub fn remove_chunk<S: Into<ChunkPos>>(&mut self, chunk_pos: S) -> Option<Chunk> {
        let chunk_pos = chunk_pos.into();
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
    pub fn chunks(&self) -> impl Iterator<Item = (&Vector3<i32>, &Chunk)> {
        self.chunks.iter().map(|(k, v)| (k.as_ref(), v))
    }

    /// Returns a mutable iterator over all loaded chunks.
    pub fn chunks_mut(&mut self) -> impl Iterator<Item = (&Vector3<i32>, &mut Chunk)> {
        self.chunks.iter_mut().map(|(k, v)| (k.as_ref(), v))
    }

    /// Returns the number of loaded chunks.
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Clears all chunks and cached data from the world.
    /// Used when a client connects to a server and needs to load a new world.
    pub fn clear(&mut self) {
        self.chunks.clear();
        self.dirty_chunks.clear();
        self.dirty_set.clear();
        self.minimap_height_cache.clear();
        self.pending_overflow.clear();
    }

    /// Drains the dirty chunks queue.
    ///
    /// Returns all chunk positions that need GPU re-upload.
    #[allow(dead_code)] // reason: world API — kept for future use
    pub fn drain_dirty_chunks(&mut self) -> Vec<Vector3<i32>> {
        self.dirty_set.clear();
        std::mem::take(&mut self.dirty_chunks)
            .into_iter()
            .map(|c| c.0)
            .collect()
    }

    /// Drains up to `limit` dirty chunk positions, leaving the rest queued.
    pub fn drain_dirty_chunks_limit(&mut self, limit: usize) -> Vec<Vector3<i32>> {
        if limit == 0 || self.dirty_chunks.is_empty() {
            return Vec::new();
        }
        let len = self.dirty_chunks.len();
        let take = len.min(limit);
        let start = len - take;
        let taken: Vec<_> = self.dirty_chunks.drain(start..).collect();
        for pos in &taken {
            self.dirty_set.remove(pos);
        }
        taken.into_iter().map(|c| c.0).collect()
    }

    /// Removes the given positions from the dirty chunk queue, if present.
    ///
    /// Generic over the slice element so both raw `Vector3<i32>` callers (the
    /// streaming/upload path) and typed `ChunkPos` callers (tests) keep working.
    pub fn remove_dirty_positions<T: Into<ChunkPos> + Copy>(&mut self, positions: &[T]) {
        if self.dirty_chunks.is_empty() || positions.is_empty() {
            return;
        }

        let remove: HashSet<ChunkPos> = positions.iter().map(|p| (*p).into()).collect();
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
    #[allow(dead_code)] // reason: world API — kept for future use
    pub fn dirty_chunks(&self) -> &[ChunkPos] {
        &self.dirty_chunks
    }

    /// Returns the count of chunks with dirty flag set.
    pub fn dirty_chunk_count(&self) -> usize {
        self.chunks.values().filter(|c| c.dirty).count()
    }

    /// Marks all loaded chunks as dirty for GPU upload.
    #[allow(dead_code)] // reason: world API — kept for future use
    pub fn mark_all_dirty(&mut self) {
        let positions: Vec<Vector3<i32>> = self.chunks.keys().map(|p| p.0).collect();
        self.requeue_dirty(&positions);
    }

    /// Re-queues a list of dirty chunk positions (deduped).
    pub fn requeue_dirty(&mut self, positions: &[Vector3<i32>]) {
        for &pos in positions {
            self.push_dirty(pos.into());
        }
    }

    /// Gets chunk positions that should be loaded based on player position and view direction.
    ///
    /// Returns chunks within the given view distance that are not yet loaded.
    /// Chunks are sorted by a score that prioritizes:
    /// 1. Chunks in the viewing direction (lower score = higher priority)
    /// 2. Closer chunks over farther chunks
    ///
    /// `view_dir` is the normalized XZ viewing direction (from camera yaw).
    pub fn get_chunks_to_load(
        &self,
        center: Vector3<i32>,
        view_distance: i32,
        world_bounds: (Vector3<i32>, Vector3<i32>), // (min_chunk, max_chunk) inclusive
        view_dir: Option<(f32, f32)>,               // Optional (dir_x, dir_z) normalized
        y_band: Option<i32>,                        // Optional vertical band half-extent
    ) -> Vec<Vector3<i32>> {
        let mut to_load = Vec::new();
        let (min_chunk, max_chunk) = world_bounds;

        let (y_min, y_max) = if let Some(radius) = y_band {
            (
                (center.y - radius).max(min_chunk.y),
                (center.y + radius).min(max_chunk.y),
            )
        } else {
            (min_chunk.y, max_chunk.y)
        };

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
                for cy in y_min..=y_max {
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

        // Sort by priority score: combines distance and view direction alignment
        // Lower score = higher priority (loaded first)
        to_load.sort_by(|a, b| {
            let score_a = Self::chunk_load_priority(center, *a, view_dir);
            let score_b = Self::chunk_load_priority(center, *b, view_dir);
            score_a
                .partial_cmp(&score_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        to_load
    }

    /// Calculate load priority for a chunk.
    /// Lower score = higher priority.
    /// Combines distance with view direction alignment.
    pub fn chunk_load_priority(
        center: Vector3<i32>,
        chunk: Vector3<i32>,
        view_dir: Option<(f32, f32)>,
    ) -> f32 {
        let dx = (chunk.x - center.x) as f32;
        let dz = (chunk.z - center.z) as f32;
        let dist_sq = dx * dx + dz * dz;
        let dist = dist_sq.sqrt();

        // Base priority is distance
        let mut score = dist;

        // If we have view direction, adjust score based on alignment
        if let Some((vx, vz)) = view_dir
            && dist > 0.1
        {
            // Normalize chunk direction
            let cx = dx / dist;
            let cz = dz / dist;

            // Dot product: 1.0 = same direction, -1.0 = opposite
            let dot = cx * vx + cz * vz;

            // Convert to multiplier:
            // - Looking at chunk (dot=1): multiply by 0.9 (slightly higher priority)
            // - Perpendicular (dot=0): multiply by 1.0 (normal priority)
            // - Looking away (dot=-1): multiply by 1.1 (slightly lower priority)
            // Keep the adjustment small so distance remains the dominant factor.
            // This ensures nearby chunks always load before distant ones.
            let dir_multiplier = 1.0 - dot * 0.1;
            score *= dir_multiplier;
        }

        score
    }

    /// Gets chunk positions that should be unloaded based on player position.
    ///
    /// Returns loaded chunks that are beyond the given unload distance (horizontal only).
    pub fn get_chunks_to_unload(
        &self,
        center: Vector3<i32>,
        unload_distance: i32,
    ) -> Vec<Vector3<i32>> {
        let unload_dist_sq = unload_distance * unload_distance;
        let mut to_unload: Vec<_> = self
            .chunks
            .keys()
            .filter(|pos| {
                // Use horizontal distance only (matching load behavior)
                let dx = pos.x - center.x;
                let dz = pos.z - center.z;
                dx * dx + dz * dz > unload_dist_sq
            })
            .map(|p| p.0)
            .collect();

        // Prefer unloading farthest chunks first to reduce thrash near the boundary.
        to_unload.sort_by(|a, b| {
            let da = (a.x - center.x).pow(2) + (a.z - center.z).pow(2);
            let db = (b.x - center.x).pow(2) + (b.z - center.z).pow(2);
            db.cmp(&da)
        });

        to_unload
    }

    /// Gets the block at world coordinates.
    pub fn get_block<S: Into<WorldPos>>(&self, world_pos: S) -> Option<BlockType> {
        let world_pos = world_pos.into();
        let chunk_pos = Self::world_to_chunk(world_pos);
        let (lx, ly, lz) = Self::world_to_local(world_pos);

        self.chunks
            .get(&chunk_pos)
            .map(|chunk| chunk.get_block(lx, ly, lz))
    }

    /// Sets the block at world coordinates.
    ///
    /// If the chunk doesn't exist, it will be created.
    pub fn set_block<S: Into<WorldPos>>(&mut self, world_pos: S, block: BlockType) {
        let world_pos = world_pos.into();
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
    pub fn set_water_block<S: Into<WorldPos>>(
        &mut self,
        world_pos: S,
        water_type: crate::chunk::WaterType,
    ) {
        let world_pos = world_pos.into();
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
    pub fn get_water_type<S: Into<WorldPos>>(
        &self,
        world_pos: S,
    ) -> Option<crate::chunk::WaterType> {
        let world_pos = world_pos.into();
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
    pub fn set_model_block<S: Into<WorldPos>>(
        &mut self,
        world_pos: S,
        model_id: u8,
        rotation: u8,
        waterlogged: bool,
    ) {
        let world_pos = world_pos.into();
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

    /// Sets the custom_data for an existing model block at world coordinates.
    #[allow(dead_code)] // reason: world API — kept for future use
    pub fn set_model_custom_data<S: Into<WorldPos>>(&mut self, world_pos: S, custom_data: u32) {
        let world_pos = world_pos.into();
        let chunk_pos = Self::world_to_chunk(world_pos);
        let (lx, ly, lz) = Self::world_to_local(world_pos);

        if let Some(chunk) = self.chunks.get_mut(&chunk_pos) {
            let was_dirty = chunk.dirty;
            chunk.set_model_custom_data(lx, ly, lz, custom_data);
            if chunk.dirty && !was_dirty {
                self.push_dirty(chunk_pos);
            }
        }
    }

    /// Sets a model block with full metadata (including custom_data) at world coordinates.
    ///
    /// This is required for blocks that rely on per-block custom data (e.g., picture frames).
    pub fn set_model_block_with_data<S: Into<WorldPos>>(
        &mut self,
        world_pos: S,
        model_id: u8,
        rotation: u8,
        waterlogged: bool,
        custom_data: u32,
    ) {
        let world_pos = world_pos.into();
        let chunk_pos = Self::world_to_chunk(world_pos);
        let (lx, ly, lz) = Self::world_to_local(world_pos);

        let is_new_chunk = !self.chunks.contains_key(&chunk_pos);
        let chunk = self.chunks.entry(chunk_pos).or_default();
        let was_dirty = chunk.dirty;
        chunk.set_model_block_with_data(lx, ly, lz, model_id, rotation, waterlogged, custom_data);

        if is_new_chunk || (chunk.dirty && !was_dirty) {
            self.push_dirty(chunk_pos);
        }
    }

    /// Gets model data for a block at world coordinates.
    ///
    /// Returns None if the chunk doesn't exist or the block has no model data.
    pub fn get_model_data<S: Into<WorldPos>>(&self, world_pos: S) -> Option<BlockModelData> {
        let world_pos = world_pos.into();
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
    pub fn set_tinted_glass_block<S: Into<WorldPos>>(&mut self, world_pos: S, tint_index: u8) {
        let world_pos = world_pos.into();
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
    pub fn set_crystal_block<S: Into<WorldPos>>(&mut self, world_pos: S, tint_index: u8) {
        let world_pos = world_pos.into();
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
    pub fn get_tint_index<S: Into<WorldPos>>(&self, world_pos: S) -> Option<u8> {
        let world_pos = world_pos.into();
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
    /// Uses default multiply blend mode.
    pub fn set_painted_block<S: Into<WorldPos>>(
        &mut self,
        world_pos: S,
        texture_idx: u8,
        tint_idx: u8,
    ) {
        let world_pos = world_pos.into();
        self.set_painted_block_full(world_pos, texture_idx, tint_idx, 0);
    }

    /// Sets a painted block at world coordinates with full metadata including blend mode.
    ///
    /// This sets the block type to Painted and stores the paint metadata.
    /// If the chunk doesn't exist, it will be created.
    pub fn set_painted_block_full<S: Into<WorldPos>>(
        &mut self,
        world_pos: S,
        texture_idx: u8,
        tint_idx: u8,
        blend_mode: u8,
    ) {
        let world_pos = world_pos.into();
        let chunk_pos = Self::world_to_chunk(world_pos);
        let (lx, ly, lz) = Self::world_to_local(world_pos);

        let is_new_chunk = !self.chunks.contains_key(&chunk_pos);
        let chunk = self.chunks.entry(chunk_pos).or_default();
        let was_dirty = chunk.dirty;
        chunk.set_painted_block_full(lx, ly, lz, texture_idx, tint_idx, blend_mode);

        if is_new_chunk || (chunk.dirty && !was_dirty) {
            self.push_dirty(chunk_pos);
        }
    }

    /// Gets the paint metadata for a painted block at world coordinates.
    ///
    /// Returns None if the chunk doesn't exist or the block has no paint data.
    pub fn get_paint_data<S: Into<WorldPos>>(
        &self,
        world_pos: S,
    ) -> Option<crate::chunk::BlockPaintData> {
        let world_pos = world_pos.into();
        let chunk_pos = Self::world_to_chunk(world_pos);
        let (lx, ly, lz) = Self::world_to_local(world_pos);

        self.chunks
            .get(&chunk_pos)
            .and_then(|chunk| chunk.get_paint_data(lx, ly, lz))
    }

    /// Checks if a block is solid at world coordinates.
    #[allow(dead_code)] // reason: world API — kept for future use
    pub fn is_solid<S: Into<WorldPos>>(&self, world_pos: S) -> bool {
        let world_pos = world_pos.into();
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

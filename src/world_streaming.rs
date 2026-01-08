use crate::App;
use crate::chunk::{BlockType, CHUNK_SIZE, CHUNK_VOLUME};
use crate::constants::{
    CHUNKS_PER_FRAME, EMPTY_CHUNK_DATA, EMPTY_MODEL_METADATA, LOADED_CHUNKS_X, LOADED_CHUNKS_Z,
    TEXTURE_SIZE_X, TEXTURE_SIZE_Y, TEXTURE_SIZE_Z, WORLD_CHUNKS_Y,
};
use crate::gpu_resources::{
    BRICK_DIST_WORDS, BRICK_MASK_WORDS, CHUNK_METADATA_WORDS, TOTAL_CHUNKS, upload_chunks_batched,
};
use crate::svt::ChunkSVT;
use crate::utils::ChunkStats;
use nalgebra::{Vector3, vector};
use rayon::prelude::*;
use std::cell::Ref;
use std::collections::{HashSet, VecDeque};
use std::sync::OnceLock;
use std::time::Instant;
use vulkano::command_buffer::{
    AutoCommandBufferBuilder, ClearColorImageInfo, CommandBufferUsage, PrimaryCommandBufferAbstract,
};
use vulkano::sync::GpuFuture;

const METADATA_DEFAULT_BUDGET: usize = 192;
const METADATA_MIN_BUDGET: usize = 64;

fn metadata_chunks_per_frame() -> usize {
    static BUDGET: OnceLock<usize> = OnceLock::new();
    *BUDGET.get_or_init(|| {
        std::env::var("METADATA_CHUNKS_PER_FRAME")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .map(|v| v.clamp(METADATA_MIN_BUDGET, TOTAL_CHUNKS))
            .unwrap_or(METADATA_DEFAULT_BUDGET)
    })
}

/// Maintains CPU-side metadata buffers and scheduling for amortized uploads.
pub(crate) struct MetadataState {
    pub chunk_bits: Vec<u32>,
    pub brick_masks: Vec<u32>,
    pub brick_distances: Vec<u32>,
    pending: VecDeque<usize>,
    pending_set: HashSet<usize>,
    cursor: usize,
    full_refresh: bool,
    dirty: bool,
    last_origin: Vector3<i32>,
}

impl MetadataState {
    pub fn new(texture_origin: Vector3<i32>) -> Self {
        Self {
            chunk_bits: vec![u32::MAX; CHUNK_METADATA_WORDS],
            brick_masks: vec![0; BRICK_MASK_WORDS],
            brick_distances: vec![0xFFFF_FFFF; BRICK_DIST_WORDS],
            pending: VecDeque::new(),
            pending_set: HashSet::new(),
            cursor: 0,
            full_refresh: true,
            dirty: true,
            last_origin: texture_origin,
        }
    }

    pub fn reset_for_origin(&mut self, new_origin: Vector3<i32>) {
        self.last_origin = new_origin;
        self.chunk_bits.fill(u32::MAX);
        self.brick_masks.fill(0);
        self.brick_distances.fill(0xFFFF_FFFF);
        self.pending.clear();
        self.pending_set.clear();
        self.cursor = 0;
        self.full_refresh = true;
        self.dirty = true;
    }

    pub fn queue_world_chunk(&mut self, texture_origin: Vector3<i32>, chunk_pos: Vector3<i32>) {
        if texture_origin != self.last_origin {
            self.reset_for_origin(texture_origin);
        }
        if let Some(idx) = world_pos_to_chunk_index(texture_origin, chunk_pos) {
            self.queue_index(idx);
        }
    }

    pub fn queue_many(
        &mut self,
        texture_origin: Vector3<i32>,
        positions: impl IntoIterator<Item = Vector3<i32>>,
    ) {
        for pos in positions {
            self.queue_world_chunk(texture_origin, pos);
        }
    }

    #[allow(dead_code)]
    pub fn request_full_refresh(&mut self, texture_origin: Vector3<i32>) {
        if texture_origin != self.last_origin {
            self.reset_for_origin(texture_origin);
            return;
        }
        self.cursor = 0;
        self.full_refresh = true;
        self.dirty = true;
    }

    #[inline]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    #[inline]
    pub fn should_seed_buffers(&self) -> bool {
        self.full_refresh && self.cursor == 0
    }

    /// Returns up to `budget` chunk indices to refresh, preferring explicit dirty chunks.
    pub fn take_work(&mut self, budget: usize) -> Vec<usize> {
        let mut work = Vec::with_capacity(budget);

        while work.len() < budget {
            if let Some(idx) = self.pending.pop_front() {
                self.pending_set.remove(&idx);
                work.push(idx);
            } else {
                break;
            }
        }

        while work.len() < budget && self.full_refresh && self.cursor < TOTAL_CHUNKS {
            work.push(self.cursor);
            self.cursor += 1;
        }

        work
    }

    pub fn mark_results_applied(&mut self) {
        if self.pending.is_empty() && (!self.full_refresh || self.cursor >= TOTAL_CHUNKS) {
            self.full_refresh = false;
            self.dirty = false;
        } else {
            self.dirty = true;
        }
    }

    fn queue_index(&mut self, idx: usize) {
        if self.pending_set.insert(idx) {
            self.pending.push_back(idx);
        }
        self.dirty = true;
    }
}

#[derive(Clone)]
enum ChunkWork {
    Missing,
    Empty,
    FullSolid,
    Blocks(Box<[BlockType; CHUNK_VOLUME]>),
}

struct ChunkMetaResult {
    idx: usize,
    is_empty: bool,
    mask_low: u32,
    mask_high: u32,
    dist: [u32; 16],
}

fn chunk_index_to_world_pos(chunk_idx: usize, texture_origin: Vector3<i32>) -> Vector3<i32> {
    let cx = (chunk_idx % LOADED_CHUNKS_X as usize) as i32;
    let cz = (chunk_idx / LOADED_CHUNKS_X as usize) % LOADED_CHUNKS_Z as usize;
    let cy = chunk_idx / (LOADED_CHUNKS_X as usize * LOADED_CHUNKS_Z as usize);

    Vector3::new(
        texture_origin.x / CHUNK_SIZE as i32 + cx,
        cy as i32,
        texture_origin.z / CHUNK_SIZE as i32 + cz as i32,
    )
}

fn world_pos_to_chunk_index(
    texture_origin: Vector3<i32>,
    chunk_pos: Vector3<i32>,
) -> Option<usize> {
    let origin_chunk_x = texture_origin.x / CHUNK_SIZE as i32;
    let origin_chunk_z = texture_origin.z / CHUNK_SIZE as i32;

    let rel_x = chunk_pos.x - origin_chunk_x;
    let rel_y = chunk_pos.y;
    let rel_z = chunk_pos.z - origin_chunk_z;

    if !(0..LOADED_CHUNKS_X).contains(&rel_x)
        || !(0..LOADED_CHUNKS_Z).contains(&rel_z)
        || !(0..WORLD_CHUNKS_Y).contains(&rel_y)
    {
        return None;
    }

    let idx = rel_x as usize
        + rel_z as usize * LOADED_CHUNKS_X as usize
        + rel_y as usize * LOADED_CHUNKS_X as usize * LOADED_CHUNKS_Z as usize;
    Some(idx)
}

fn pack_distances(distances: &[u8]) -> [u32; 16] {
    let mut out = [0u32; 16];
    for (i, chunk) in distances.chunks(4).enumerate() {
        let b0 = chunk.first().copied().unwrap_or(0) as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let b3 = *chunk.get(3).unwrap_or(&0) as u32;
        out[i] = b0 | (b1 << 8) | (b2 << 16) | (b3 << 24);
    }
    out
}

impl App {
    pub fn check_and_shift_texture_origin(&mut self) -> bool {
        let player_chunk = self
            .sim
            .player
            .get_chunk_pos(self.sim.world_extent, self.sim.texture_origin);

        // Calculate texture center in chunk coordinates
        let texture_center_chunk = Vector3::new(
            self.sim.texture_origin.x / CHUNK_SIZE as i32 + LOADED_CHUNKS_X / 2,
            0, // Y doesn't shift
            self.sim.texture_origin.z / CHUNK_SIZE as i32 + LOADED_CHUNKS_Z / 2,
        );

        // Distance from player to texture center (in chunks)
        let dx = player_chunk.x - texture_center_chunk.x;
        let dz = player_chunk.z - texture_center_chunk.z;

        // Shift threshold: when player is more than 1/4 of texture size from center
        let shift_threshold_x = LOADED_CHUNKS_X / 4;
        let shift_threshold_z = LOADED_CHUNKS_Z / 4;

        if dx.abs() <= shift_threshold_x && dz.abs() <= shift_threshold_z {
            return false; // No shift needed
        }

        // Calculate new texture origin centered on player
        let new_origin = Vector3::new(
            (player_chunk.x - LOADED_CHUNKS_X / 2) * CHUNK_SIZE as i32,
            0, // Y origin stays at 0
            (player_chunk.z - LOADED_CHUNKS_Z / 2) * CHUNK_SIZE as i32,
        );

        println!(
            "Shifting texture origin from ({}, {}) to ({}, {}) - player at chunk ({}, {})",
            self.sim.texture_origin.x,
            self.sim.texture_origin.z,
            new_origin.x,
            new_origin.z,
            player_chunk.x,
            player_chunk.z
        );

        // Save old origin to adjust camera position
        let old_origin = self.sim.texture_origin;
        self.sim.texture_origin = new_origin;
        self.sim
            .metadata_state
            .reset_for_origin(self.sim.texture_origin);

        // Adjust camera position to maintain the same world position
        let origin_delta = old_origin - new_origin;
        let scale = Vector3::new(
            self.sim.world_extent[0] as f64,
            self.sim.world_extent[1] as f64,
            self.sim.world_extent[2] as f64,
        );
        self.sim.player.camera.position.x += origin_delta.x as f64 / scale.x;
        self.sim.player.camera.position.y += origin_delta.y as f64 / scale.y;
        self.sim.player.camera.position.z += origin_delta.z as f64 / scale.z;

        // Re-upload all loaded chunks to their new texture positions (slice-backed, no alloc).
        struct Upload<'a> {
            pos: Vector3<i32>,
            block: &'a [u8],
            meta: Ref<'a, [u8]>,
        }

        let mut uploads: Vec<Upload> = Vec::new();
        for (pos, chunk) in self.sim.world.chunks() {
            // Skip empty chunks entirely (they contain no visible data)
            if chunk.is_empty() {
                continue;
            }
            uploads.push(Upload {
                pos: *pos,
                block: chunk.block_bytes(),
                meta: chunk.model_metadata_bytes(),
            });
        }

        if !uploads.is_empty() {
            // Clear the texture first (set all to air)
            self.clear_voxel_texture();
            // Upload chunks at new positions - convert to slice references
            let upload_slices: Vec<_> =
                uploads.iter().map(|u| (u.pos, u.block, &*u.meta)).collect();
            self.upload_chunk_refs(&upload_slices);
        }
        // Drop metadata refs and chunk borrows before any mutable world operations
        let uploaded_positions: Vec<_> = uploads.iter().map(|u| u.pos).collect();
        drop(uploads);

        if !uploaded_positions.is_empty() {
            self.sim.world.remove_dirty_positions(&uploaded_positions);
        }

        true
    }

    pub fn clear_voxel_texture(&self) {
        let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
            self.graphics.command_buffer_allocator.clone(),
            self.graphics.queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )
        .unwrap();

        command_buffer_builder
            .clear_color_image(ClearColorImageInfo::image(
                self.graphics.voxel_image.clone(),
            ))
            .unwrap();

        command_buffer_builder
            .clear_color_image(ClearColorImageInfo::image(
                self.graphics.model_metadata.clone(),
            ))
            .unwrap();

        command_buffer_builder
            .build()
            .unwrap()
            .execute(self.graphics.queue.clone())
            .unwrap()
            .then_signal_fence_and_flush()
            .unwrap()
            .wait(None)
            .unwrap();
    }

    pub fn update_chunk_loading(&mut self) -> (usize, usize) {
        // Check if we need to shift the texture origin first
        let shifted = self.check_and_shift_texture_origin();
        if shifted {
            println!(
                "Texture origin shifted to ({}, {})",
                self.sim.texture_origin.x, self.sim.texture_origin.z
            );
        }

        let player_chunk = self
            .sim
            .player
            .get_chunk_pos(self.sim.world_extent, self.sim.texture_origin);

        // Infinite world in X/Z, bounded in Y (0 to WORLD_CHUNKS_Y-1)
        let min_chunk = vector![i32::MIN, 0, i32::MIN];
        let max_chunk = vector![i32::MAX, WORLD_CHUNKS_Y - 1, i32::MAX];

        // === STEP 1: Receive completed chunks from background threads ===
        let completed = self.sim.chunk_loader.receive_chunks();
        let mut chunks_to_upload: Vec<(Vector3<i32>, Vec<u8>, Vec<u8>)> = Vec::new();
        let mut loaded = 0;

        for result in completed {
            // Apply overflow blocks (immediate if chunk exists, pending if not)
            self.sim.world.apply_overflow_blocks(result.overflow_blocks);

            // Insert chunk into world (will also apply any pending overflow for this chunk)
            self.sim.world.insert_chunk(result.position, result.chunk);

            // CRITICAL: Recompute block_data and model_metadata AFTER insert_chunk
            // because pending overflow may have modified the chunk
            let chunk = self
                .sim
                .world
                .get_chunk(result.position)
                .expect("Chunk should exist after insert");
            let block_data = chunk.to_block_data();
            let model_metadata = chunk.to_model_metadata();

            chunks_to_upload.push((result.position, block_data, model_metadata));
            loaded += 1;
        }

        // Batch upload completed chunks to GPU
        if !chunks_to_upload.is_empty() {
            // Convert to slice references for upload
            self.upload_owned_chunks(&chunks_to_upload);
            self.mark_chunks_clean(&chunks_to_upload);
            // Already uploaded this frame; avoid a second upload in upload_world_to_gpu
            self.sim.world.remove_dirty_positions(
                &chunks_to_upload
                    .iter()
                    .map(|(pos, _, _)| *pos)
                    .collect::<Vec<_>>(),
            );
        }

        // === STEP 2: Queue new chunks for generation ===
        let to_load = self.sim.world.get_chunks_to_load(
            player_chunk,
            self.sim.view_distance,
            (min_chunk, max_chunk),
        );

        // Queue chunks for async generation (ChunkLoader handles deduplication)
        let max_to_queue = CHUNKS_PER_FRAME * 4;
        let queued = self
            .sim
            .chunk_loader
            .request_chunks(&to_load.into_iter().take(max_to_queue).collect::<Vec<_>>());

        if queued > 20 {
            println!(
                "Queued {} chunks for generation around ({}, {}, {})",
                queued, player_chunk.x, player_chunk.y, player_chunk.z
            );
        }

        // === STEP 3: Unload distant chunks ===
        let to_unload = self
            .sim
            .world
            .get_chunks_to_unload(player_chunk, self.sim.unload_distance);

        let mut unloaded = 0;
        let positions_to_clear: Vec<_> = to_unload
            .iter()
            .take(CHUNKS_PER_FRAME)
            .map(|pos| {
                // Cancel pending generation for this chunk if queued
                self.sim.chunk_loader.cancel_chunk(*pos);
                self.sim.world.remove_chunk(*pos);
                unloaded += 1;
                *pos
            })
            .collect();

        // Batch clear all unloaded chunks using static empty data (no allocation)
        if !positions_to_clear.is_empty() {
            let chunks_to_clear: Vec<_> = positions_to_clear
                .iter()
                .map(|pos| {
                    (
                        *pos,
                        EMPTY_CHUNK_DATA.as_slice(),
                        EMPTY_MODEL_METADATA.as_slice(),
                    )
                })
                .collect();
            self.upload_chunk_refs(&chunks_to_clear);
        }

        // Update chunk metadata if any chunks were loaded or unloaded
        if !chunks_to_upload.is_empty() {
            let positions = chunks_to_upload.iter().map(|(pos, _, _)| *pos);
            self.sim
                .metadata_state
                .queue_many(self.sim.texture_origin, positions);
        }
        if !positions_to_clear.is_empty() {
            self.sim
                .metadata_state
                .queue_many(self.sim.texture_origin, positions_to_clear.iter().copied());
        }

        // Update chunk stats
        self.sim.chunk_stats = ChunkStats {
            loaded_count: self.sim.world.chunk_count(),
            dirty_count: self.sim.world.dirty_chunk_count(),
            in_flight_count: self.sim.chunk_loader.in_flight_count(),
            memory_mb: (TEXTURE_SIZE_X * TEXTURE_SIZE_Y * TEXTURE_SIZE_Z) as f32
                / (1024.0 * 1024.0),
        };

        // Update last player chunk
        self.sim.last_player_chunk = player_chunk;

        (loaded, unloaded)
    }

    pub fn upload_world_to_gpu(&mut self) {
        // Drain dirty chunk positions from world
        let dirty_positions = self.sim.world.drain_dirty_chunks();
        if dirty_positions.is_empty() {
            return;
        }

        struct Upload<'a> {
            pos: Vector3<i32>,
            block: &'a [u8],
            meta: Ref<'a, [u8]>,
        }

        let mut uploads: Vec<Upload> = Vec::new();
        for &pos in &dirty_positions {
            if let Some(chunk) = self.sim.world.get_chunk(pos) {
                uploads.push(Upload {
                    pos,
                    block: chunk.block_bytes(),
                    meta: chunk.model_metadata_bytes(),
                });
            }
        }

        if !uploads.is_empty() {
            self.sim.profiler.chunks_uploaded += uploads.len() as u32;
            let upload_slices: Vec<_> =
                uploads.iter().map(|u| (u.pos, u.block, &*u.meta)).collect();
            self.upload_chunk_refs(&upload_slices);

            // Release borrows before marking chunks clean
            let uploaded_positions: Vec<_> = uploads.iter().map(|u| u.pos).collect();
            drop(uploads);

            for pos in dirty_positions {
                if let Some(chunk) = self.sim.world.get_chunk_mut(pos) {
                    chunk.mark_clean();
                }
            }
            // Refresh metadata for the chunks we just uploaded (amortized later this frame)
            self.sim
                .metadata_state
                .queue_many(self.sim.texture_origin, uploaded_positions.iter().copied());
            // Avoid re-upload if any positions remain queued
            if !uploaded_positions.is_empty() {
                self.sim.world.remove_dirty_positions(&uploaded_positions);
            }
        }
    }

    pub fn upload_all_dirty_chunks(&mut self) {
        let dirty_positions: Vec<_> = self
            .sim
            .world
            .chunks()
            .filter(|(_, chunk)| chunk.dirty)
            .map(|(pos, _)| *pos)
            .collect();

        if dirty_positions.is_empty() {
            return;
        }

        struct Upload<'a> {
            pos: Vector3<i32>,
            block: &'a [u8],
            meta: Ref<'a, [u8]>,
        }

        let mut uploads: Vec<Upload> = Vec::new();

        for pos in &dirty_positions {
            if let Some(chunk) = self.sim.world.get_chunk(*pos) {
                uploads.push(Upload {
                    pos: *pos,
                    block: chunk.block_bytes(),
                    meta: chunk.model_metadata_bytes(),
                });
            }
        }

        if uploads.is_empty() {
            return;
        }

        self.sim.profiler.chunks_uploaded += uploads.len() as u32;
        let upload_slices: Vec<_> = uploads.iter().map(|u| (u.pos, u.block, &*u.meta)).collect();
        self.upload_chunk_refs(&upload_slices);

        let uploaded_positions: Vec<_> = uploads.iter().map(|u| u.pos).collect();
        drop(uploads);

        for pos in dirty_positions {
            if let Some(chunk) = self.sim.world.get_chunk_mut(pos) {
                chunk.mark_clean();
            }
        }

        if !uploaded_positions.is_empty() {
            self.sim
                .metadata_state
                .queue_many(self.sim.texture_origin, uploaded_positions.iter().copied());
            self.sim.world.remove_dirty_positions(&uploaded_positions);
        }
    }

    /// Uploads owned chunk buffers (Vec-backed) to GPU by creating borrowed views.
    fn upload_owned_chunks(&self, uploads: &[(Vector3<i32>, Vec<u8>, Vec<u8>)]) {
        let upload_refs: Vec<_> = uploads
            .iter()
            .map(|(pos, block_data, model_metadata)| {
                (*pos, block_data.as_slice(), model_metadata.as_slice())
            })
            .collect();
        self.upload_chunk_refs(&upload_refs);
    }

    /// Uploads chunk data that is already slice-backed to GPU.
    fn upload_chunk_refs(&self, uploads: &[(Vector3<i32>, &[u8], &[u8])]) {
        if uploads.is_empty() {
            return;
        }
        upload_chunks_batched(
            &self.graphics.memory_allocator,
            &self.graphics.command_buffer_allocator,
            &self.graphics.queue,
            &self.graphics.voxel_image,
            &self.graphics.model_metadata,
            self.sim.texture_origin,
            uploads,
        );
    }

    /// Marks chunks referenced in the upload list as clean if they exist in the world.
    fn mark_chunks_clean(&mut self, uploads: &[(Vector3<i32>, Vec<u8>, Vec<u8>)]) {
        for (pos, _, _) in uploads {
            if let Some(chunk) = self.sim.world.get_chunk_mut(*pos) {
                chunk.mark_clean();
            }
        }
    }

    /// Refreshes chunk and brick metadata buffers and records profiling time.
    pub(crate) fn update_metadata_buffers(&mut self) {
        let mut reset_buffers = false;
        if self.sim.metadata_state.last_origin != self.sim.texture_origin {
            self.sim
                .metadata_state
                .reset_for_origin(self.sim.texture_origin);
            reset_buffers = true;
        } else if self.sim.metadata_state.should_seed_buffers() {
            reset_buffers = true;
        }

        if !self.sim.metadata_state.is_dirty() && !reset_buffers {
            return;
        }

        let t_meta = Instant::now();

        if reset_buffers {
            let mut chunk_meta_write = self.graphics.chunk_metadata_buffer.write().unwrap();
            chunk_meta_write.copy_from_slice(&self.sim.metadata_state.chunk_bits);

            let mut brick_mask_write = self.graphics.brick_mask_buffer.write().unwrap();
            brick_mask_write.copy_from_slice(&self.sim.metadata_state.brick_masks);

            let mut brick_dist_write = self.graphics.brick_dist_buffer.write().unwrap();
            brick_dist_write.copy_from_slice(&self.sim.metadata_state.brick_distances);
        }

        if !self.sim.metadata_state.is_dirty() {
            self.sim.profiler.metadata_update_us += t_meta.elapsed().as_micros() as u64;
            return;
        }

        // After a texture-origin shift we must rebuild all chunk/brick metadata in one frame
        // to avoid a “world is empty” flash. Otherwise, keep the amortized per-frame budget.
        let budget = if reset_buffers {
            TOTAL_CHUNKS
        } else {
            metadata_chunks_per_frame()
        };

        let work_indices = self.sim.metadata_state.take_work(budget);

        if work_indices.is_empty() {
            self.sim.metadata_state.mark_results_applied();
            self.sim.profiler.metadata_update_us += t_meta.elapsed().as_micros() as u64;
            return;
        }

        let mut tasks = Vec::with_capacity(work_indices.len());
        for idx in work_indices {
            let world_pos = chunk_index_to_world_pos(idx, self.sim.texture_origin);
            let work = if let Some(chunk) = self.sim.world.get_chunk_mut(world_pos) {
                chunk.update_metadata();
                if chunk.cached_is_empty() {
                    ChunkWork::Empty
                } else if chunk.cached_is_fully_solid() {
                    ChunkWork::FullSolid
                } else {
                    ChunkWork::Blocks(chunk.clone_blocks())
                }
            } else {
                ChunkWork::Missing
            };
            tasks.push((idx, work));
        }

        let results: Vec<ChunkMetaResult> = tasks
            .into_par_iter()
            .map(|(idx, work)| match work {
                ChunkWork::Missing | ChunkWork::Empty => ChunkMetaResult {
                    idx,
                    is_empty: true,
                    mask_low: 0,
                    mask_high: 0,
                    dist: [0xFFFF_FFFF; 16],
                },
                ChunkWork::FullSolid => ChunkMetaResult {
                    idx,
                    is_empty: false,
                    mask_low: u32::MAX,
                    mask_high: u32::MAX,
                    dist: [0; 16],
                },
                ChunkWork::Blocks(blocks) => {
                    let svt = ChunkSVT::from_block_data(&blocks);
                    ChunkMetaResult {
                        idx,
                        is_empty: svt.brick_mask == 0,
                        mask_low: svt.brick_mask as u32,
                        mask_high: (svt.brick_mask >> 32) as u32,
                        dist: pack_distances(&svt.brick_distances),
                    }
                }
            })
            .collect();

        {
            let mut chunk_meta_write = self.graphics.chunk_metadata_buffer.write().unwrap();
            let mut brick_mask_write = self.graphics.brick_mask_buffer.write().unwrap();
            let mut brick_dist_write = self.graphics.brick_dist_buffer.write().unwrap();

            for res in &results {
                let word_idx = res.idx / 32;
                let bit_idx = res.idx % 32;
                if res.is_empty {
                    self.sim.metadata_state.chunk_bits[word_idx] |= 1u32 << bit_idx;
                } else {
                    self.sim.metadata_state.chunk_bits[word_idx] &= !(1u32 << bit_idx);
                }
                chunk_meta_write[word_idx] = self.sim.metadata_state.chunk_bits[word_idx];

                let mask_offset = res.idx * 2;
                self.sim.metadata_state.brick_masks[mask_offset] = res.mask_low;
                self.sim.metadata_state.brick_masks[mask_offset + 1] = res.mask_high;
                brick_mask_write[mask_offset] = res.mask_low;
                brick_mask_write[mask_offset + 1] = res.mask_high;

                let dist_offset = res.idx * 16;
                for (i, word) in res.dist.iter().enumerate() {
                    self.sim.metadata_state.brick_distances[dist_offset + i] = *word;
                    brick_dist_write[dist_offset + i] = *word;
                }
            }
        }

        self.sim.metadata_state.mark_results_applied();
        self.sim.profiler.metadata_update_us += t_meta.elapsed().as_micros() as u64;
    }
}

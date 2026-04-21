//! Chunk streaming: loading, unloading, and GPU upload of chunks around the player.
//!
//! Drives the chunk lifecycle: requests new chunks from `chunk_loader` as the player
//! moves, evicts distant chunks, applies pending network chunks from multiplayer,
//! and batches GPU uploads so only modified 32KB chunks are transferred each frame.

use crate::App;
use crate::chunk::{BlockType, CHUNK_SIZE, CHUNK_VOLUME, Chunk};
use crate::chunk_loader::RequestStats;
use crate::constants::{
    CHUNKS_PER_FRAME, EMPTY_CHUNK_DATA, EMPTY_CUSTOM_DATA, EMPTY_MODEL_METADATA, LOADED_CHUNKS_X,
    LOADED_CHUNKS_Z, MAX_COMPLETED_UPLOADS_PER_FRAME, TEXTURE_SIZE_X, TEXTURE_SIZE_Y,
    TEXTURE_SIZE_Z, WORLD_CHUNKS_Y,
};
use crate::gpu_resources::{
    BRICK_DIST_WORDS, BRICK_MASK_WORDS, CHUNK_METADATA_WORDS, ChunkDataSlice, ChunkUploadConfig,
    TOTAL_CHUNKS, upload_chunks_batched,
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

const METADATA_DEFAULT_BUDGET: usize = 128;
const METADATA_MIN_BUDGET: usize = 64;
const REUPLOAD_DEFAULT_PER_FRAME: usize = 256;
const UPLOAD_DEFAULT_PER_FRAME: usize = 256;
const METADATA_RESET_DEFAULT_BUDGET: usize = 256;

/// Radius (in chunks) of the synchronous near-chunk upload on origin shift.
/// Must cover the player's visible area or chunks will pop in. Tunable via
/// `ORIGIN_SHIFT_NEAR_RADIUS` (clamped to view_distance). `0` = no sync bulk,
/// rely entirely on reupload_queue (causes visible flashes).
const ORIGIN_SHIFT_NEAR_RADIUS_DEFAULT: i32 = i32::MAX;

fn shift_profile_enabled() -> bool {
    static VAL: OnceLock<bool> = OnceLock::new();
    *VAL.get_or_init(|| std::env::var("ORIGIN_SHIFT_PROFILE").is_ok())
}

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

fn reupload_per_frame() -> usize {
    static VAL: OnceLock<usize> = OnceLock::new();
    *VAL.get_or_init(|| {
        std::env::var("REUPLOAD_PER_FRAME")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(REUPLOAD_DEFAULT_PER_FRAME)
    })
}

fn uploads_per_frame() -> usize {
    static VAL: OnceLock<usize> = OnceLock::new();
    *VAL.get_or_init(|| {
        std::env::var("UPLOADS_PER_FRAME")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(UPLOAD_DEFAULT_PER_FRAME)
    })
}

fn metadata_reset_budget() -> usize {
    static VAL: OnceLock<usize> = OnceLock::new();
    *VAL.get_or_init(|| {
        std::env::var("METADATA_RESET_BUDGET")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(METADATA_RESET_DEFAULT_BUDGET)
    })
}

/// Radius (in chunks) of the synchronous bulk upload at origin shift.
///
/// The old behavior used `view_distance` (6), which produced ~1584-chunk
/// syncronous uploads (~346 MB, ~40 ms stall). A small radius keeps the
/// player's immediate surroundings visible without pop-in while letting the
/// `reupload_queue` fill the rest gradually over the following frames.
fn origin_shift_near_radius(fallback: i32) -> i32 {
    static VAL: OnceLock<i32> = OnceLock::new();
    let configured = *VAL.get_or_init(|| {
        std::env::var("ORIGIN_SHIFT_NEAR_RADIUS")
            .ok()
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(ORIGIN_SHIFT_NEAR_RADIUS_DEFAULT)
            .max(0)
    });
    // Never exceed view_distance (no point uploading invisible chunks bulk).
    configured.min(fallback.max(0))
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

    /// Returns chunk indices to refresh, prioritizing explicit dirty chunks.
    ///
    /// Pending dirty chunks are bounded by the budget to avoid frame stalls;
    /// any remaining pending items stay queued for the next frame.
    pub fn take_work(&mut self, budget: usize) -> Vec<usize> {
        let mut work = Vec::with_capacity(budget);
        let mut remaining = budget;

        // Drain pending up to budget
        while remaining > 0 {
            if let Some(idx) = self.pending.pop_front() {
                self.pending_set.remove(&idx);
                work.push(idx);
                remaining -= 1;
            } else {
                break;
            }
        }

        // Background sweep uses any leftover budget
        while remaining > 0 && self.full_refresh && self.cursor < TOTAL_CHUNKS {
            work.push(self.cursor);
            self.cursor += 1;
            remaining -= 1;
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

    #[inline]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
}

enum ChunkWork<'a> {
    Missing,
    Borrow(&'a [BlockType; CHUNK_VOLUME]),
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

        // Predictive shifting: if player is moving toward an edge, shift earlier.
        // This reduces the number of chunks dropped when the shift occurs because
        // we start loading chunks at the new origin before reaching the boundary.
        let velocity = self.sim.player.velocity;
        let vel_x = velocity.x.signum() as i32;
        let vel_z = velocity.z.signum() as i32;

        // Check if player is moving toward the current offset direction
        let moving_toward_edge_x = vel_x != 0 && vel_x == dx.signum();
        let moving_toward_edge_z = vel_z != 0 && vel_z == dz.signum();

        // Use smaller threshold (1/6) when moving toward edge, otherwise 1/4
        let shift_threshold_x = if moving_toward_edge_x {
            LOADED_CHUNKS_X / 6
        } else {
            LOADED_CHUNKS_X / 4
        };
        let shift_threshold_z = if moving_toward_edge_z {
            LOADED_CHUNKS_Z / 6
        } else {
            LOADED_CHUNKS_Z / 4
        };

        if dx.abs() <= shift_threshold_x && dz.abs() <= shift_threshold_z {
            return false; // No shift needed
        }

        // Calculate new texture origin centered on player
        let new_origin = Vector3::new(
            (player_chunk.x - LOADED_CHUNKS_X / 2) * CHUNK_SIZE as i32,
            0, // Y origin stays at 0
            (player_chunk.z - LOADED_CHUNKS_Z / 2) * CHUNK_SIZE as i32,
        );

        log::debug!(
            "Shifting texture origin from ({}, {}) to ({}, {}) - player at chunk ({}, {})",
            self.sim.texture_origin.x,
            self.sim.texture_origin.z,
            new_origin.x,
            new_origin.z,
            player_chunk.x,
            player_chunk.z
        );

        self.sim.last_origin_shift = Some(new_origin / CHUNK_SIZE as i32);
        self.sim.origin_shift_count = self.sim.origin_shift_count.saturating_add(1);

        let t_shift_start = Instant::now();

        // Save old origin to adjust camera position
        let old_origin = self.sim.texture_origin;
        self.sim.texture_origin = new_origin;
        self.sim
            .metadata_state
            .reset_for_origin(self.sim.texture_origin);

        // Cancel all in-flight chunk generation requests - they were requested for the old
        // texture origin and may complete at positions outside the new texture bounds.
        // They'll be re-requested with the correct origin in the next frame.
        self.sim.chunk_loader.reset_epoch_and_clear();

        // Also clear deferred uploads - they were for the old texture origin
        self.sim.streaming.deferred_uploads.clear();

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

        // Split chunks into near (uploaded immediately) and far (queued for gradual re-upload).
        // This avoids the 20-50ms stall from uploading all ~2500 chunks at once.
        // Near chunks fill the area around the player so there's no visual gap.
        self.sim.streaming.reupload_queue.clear();

        let t_setup = t_shift_start.elapsed();

        // Start GPU texture clear FIRST (async) - runs on GPU while CPU collects data
        let t_clear_issue = Instant::now();
        let clear_fence = self.clear_voxel_texture_async();
        let clear_issue_us = t_clear_issue.elapsed();

        // Partition chunks: near player = immediate upload, far = deferred via reupload_queue.
        // Use view_distance as the radius so the player never sees un-filled chunks within their
        // visible area. Tunable via `ORIGIN_SHIFT_NEAR_RADIUS`; smaller values cause pop-in.
        let t_partition = Instant::now();
        let texture_origin = self.sim.texture_origin;
        let near_radius = origin_shift_near_radius(self.sim.view_distance);
        let near_radius_sq = near_radius * near_radius;
        let mut near_positions: Vec<Vector3<i32>> = Vec::new();
        let mut far_positions: Vec<Vector3<i32>> = Vec::new();

        let total_world_chunks = self.sim.world.chunks().count();
        for (pos, _) in self.sim.world.chunks() {
            if world_pos_to_chunk_index(texture_origin, *pos).is_some() {
                let dx = pos.x - player_chunk.x;
                let dz = pos.z - player_chunk.z;
                let dist_sq = dx * dx + dz * dz;
                if dist_sq <= near_radius_sq {
                    near_positions.push(*pos);
                } else {
                    far_positions.push(*pos);
                }
            }
        }

        // Queue far chunks for gradual re-upload over subsequent frames.
        // Sort by distance so nearest chunks fill in first.
        far_positions.sort_by_key(|pos| {
            let dx = pos.x - player_chunk.x;
            let dz = pos.z - player_chunk.z;
            dx * dx + dz * dz
        });
        self.sim
            .streaming
            .reupload_queue
            .extend(far_positions.iter());
        let partition_us = t_partition.elapsed();

        // Collect near chunk refs for immediate upload (zero-copy borrows).
        let t_svt = Instant::now();
        let near_chunk_refs: Vec<_> = near_positions
            .iter()
            .filter_map(|pos| self.sim.world.get_chunk(*pos).map(|c| (*pos, c)))
            .collect();

        // Parallel SVT for near chunks only.
        let block_slices: Vec<&[u8]> = near_chunk_refs
            .iter()
            .map(|(_, chunk)| chunk.block_bytes())
            .collect();
        let svt_data: Vec<ChunkSVT> = block_slices
            .par_iter()
            .map(|bytes| ChunkSVT::from_bytes(bytes))
            .collect();

        // Build upload refs with zero-copy borrows.
        let meta_guards: Vec<_> = near_chunk_refs
            .iter()
            .map(|(_, chunk)| chunk.model_metadata_bytes())
            .collect();
        let custom_guards: Vec<_> = near_chunk_refs
            .iter()
            .map(|(_, chunk)| chunk.custom_data_bytes())
            .collect();

        let upload_refs: Vec<ChunkDataSlice<'_>> = near_chunk_refs
            .iter()
            .enumerate()
            .map(|(i, (pos, chunk))| {
                (
                    *pos,
                    chunk.block_bytes(),
                    &*meta_guards[i],
                    &*custom_guards[i],
                )
            })
            .collect();
        let svt_us = t_svt.elapsed();

        // NOW wait for texture clear (should be done by now since CPU work overlapped)
        let t_clear_wait = Instant::now();
        if let Err(e) = clear_fence.wait(None) {
            log::warn!("[GPU] Origin shift texture clear error: {:?}", e);
        }
        let clear_wait_us = t_clear_wait.elapsed();

        let mut upload_us = std::time::Duration::ZERO;
        let mut lock_acquire_us = std::time::Duration::ZERO;
        let mut meta_write_us = std::time::Duration::ZERO;

        // Upload near chunks to GPU immediately.
        // `skip_zero_slices` is safe here because the texture was just cleared.
        if !upload_refs.is_empty() {
            let t_upload = Instant::now();
            self.upload_chunk_refs_with(&upload_refs, true);
            upload_us = t_upload.elapsed();

            // Update metadata buffers for near chunks
            {
                let t_lock = Instant::now();
                let mut chunk_meta_write = self.graphics.chunk_metadata_buffer.write().unwrap();
                let mut brick_mask_write = self.graphics.brick_mask_buffer.write().unwrap();
                let mut brick_dist_write = self.graphics.brick_dist_buffer.write().unwrap();
                lock_acquire_us = t_lock.elapsed();

                let t_write = Instant::now();
                for ((pos, _), svt) in near_chunk_refs.iter().zip(svt_data.iter()) {
                    if let Some(idx) = world_pos_to_chunk_index(self.sim.texture_origin, *pos) {
                        let word_idx = idx / 32;
                        let bit_idx = idx % 32;
                        if svt.brick_mask == 0 {
                            self.sim.metadata_state.chunk_bits[word_idx] |= 1u32 << bit_idx;
                        } else {
                            self.sim.metadata_state.chunk_bits[word_idx] &= !(1u32 << bit_idx);
                        }
                        chunk_meta_write[word_idx] = self.sim.metadata_state.chunk_bits[word_idx];

                        let mask_offset = idx * 2;
                        self.sim.metadata_state.brick_masks[mask_offset] = svt.brick_mask as u32;
                        self.sim.metadata_state.brick_masks[mask_offset + 1] =
                            (svt.brick_mask >> 32) as u32;
                        brick_mask_write[mask_offset] = svt.brick_mask as u32;
                        brick_mask_write[mask_offset + 1] = (svt.brick_mask >> 32) as u32;

                        let dist_offset = idx * 16;
                        let packed_dist = pack_distances(&svt.brick_distances);
                        for (i, word) in packed_dist.iter().enumerate() {
                            self.sim.metadata_state.brick_distances[dist_offset + i] = *word;
                            brick_dist_write[dist_offset + i] = *word;
                        }
                    }
                }
                meta_write_us = t_write.elapsed();
            }
        }

        let total_us = t_shift_start.elapsed();
        if shift_profile_enabled() {
            log::warn!(
                "[ShiftProfile] total={:.2}ms setup={:.2}ms clear_issue={:.2}ms partition={:.2}ms (world_chunks={} near={} far={}) svt={:.2}ms clear_wait={:.2}ms upload={:.2}ms lock_acq={:.2}ms meta_write={:.2}ms",
                total_us.as_secs_f64() * 1000.0,
                t_setup.as_secs_f64() * 1000.0,
                clear_issue_us.as_secs_f64() * 1000.0,
                partition_us.as_secs_f64() * 1000.0,
                total_world_chunks,
                near_positions.len(),
                far_positions.len(),
                svt_us.as_secs_f64() * 1000.0,
                clear_wait_us.as_secs_f64() * 1000.0,
                upload_us.as_secs_f64() * 1000.0,
                lock_acquire_us.as_secs_f64() * 1000.0,
                meta_write_us.as_secs_f64() * 1000.0,
            );
        }

        true
    }

    /// Clears the voxel and model metadata textures asynchronously.
    /// Returns a fence that signals when the clear is complete.
    /// Uploads should be delayed until this fence signals.
    pub fn clear_voxel_texture_async(&self) -> crate::app_state::ClearFence {
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

        // Clear custom data too, so upload path can skip zero-filled slices without
        // leaving stale bytes behind from the previous texture origin.
        command_buffer_builder
            .clear_color_image(ClearColorImageInfo::image(
                self.graphics.block_custom_data.clone(),
            ))
            .unwrap();

        command_buffer_builder
            .build()
            .unwrap()
            .execute(self.graphics.queue.clone())
            .unwrap()
            .then_signal_fence_and_flush()
            .unwrap()
    }

    pub fn update_chunk_loading(&mut self) -> (Vec<Vector3<i32>>, usize, usize) {
        // Check if we need to shift the texture origin first
        let shifted = self.check_and_shift_texture_origin();
        if shifted {
            log::debug!(
                "Texture origin shifted to ({}, {})",
                self.sim.texture_origin.x,
                self.sim.texture_origin.z
            );
        }

        let player_chunk = self
            .sim
            .player
            .get_chunk_pos(self.sim.world_extent, self.sim.texture_origin);

        // Bounds limited to texture pool - chunks outside cannot be stored in GPU texture
        // This prevents infinite re-requesting of chunks that complete but can't be inserted
        let origin_chunk_x = self.sim.texture_origin.x / CHUNK_SIZE as i32;
        let origin_chunk_z = self.sim.texture_origin.z / CHUNK_SIZE as i32;
        let min_chunk = vector![origin_chunk_x, 0, origin_chunk_z];
        let max_chunk = vector![
            origin_chunk_x + LOADED_CHUNKS_X - 1,
            WORLD_CHUNKS_Y - 1,
            origin_chunk_z + LOADED_CHUNKS_Z - 1
        ];

        // === STEP 1: Receive completed chunks from background threads ===
        // First, drain any deferred uploads from previous frames
        let mut completed: Vec<_> = self.sim.streaming.deferred_uploads.drain(..).collect();

        // Then receive newly completed chunks
        let completed_new = self.sim.chunk_loader.receive_chunks();
        let (mut completed_in_bounds, dropped_oob): (Vec<_>, Vec<_>) = completed_new
            .into_iter()
            .partition(|r| world_pos_to_chunk_index(self.sim.texture_origin, r.position).is_some());

        // If any completed chunks are now out-of-bounds, re-request them at the new origin.
        if !dropped_oob.is_empty() {
            let mut retry_positions = Vec::with_capacity(dropped_oob.len());
            for c in dropped_oob {
                retry_positions.push(c.position);
            }
            // Drop silent errors; just requeue.
            let _ = self.sim.chunk_loader.request_chunks(&retry_positions);
        }

        // Append new completions to deferred (which we already drained)
        completed.append(&mut completed_in_bounds);

        // === STEP 1b: Receive network chunks (multiplayer mode) ===
        // In multiplayer client mode, we also receive chunks from the server
        let network_chunks = self.apply_network_chunks();

        // Sort completed chunks by distance to player (closer chunks processed first)
        // This ensures nearby chunks become visible before distant chunks, even when
        // workers complete chunks out of request order (due to varying terrain complexity)
        completed.sort_by(|a, b| {
            let dist_sq_a = (a.position.x - player_chunk.x).pow(2)
                + (a.position.y - player_chunk.y).pow(2)
                + (a.position.z - player_chunk.z).pow(2);
            let dist_sq_b = (b.position.x - player_chunk.x).pow(2)
                + (b.position.y - player_chunk.y).pow(2)
                + (b.position.z - player_chunk.z).pow(2);
            dist_sq_a.cmp(&dist_sq_b)
        });

        // Budget: only process up to MAX_COMPLETED_UPLOADS_PER_FRAME chunks this frame.
        // Defer the rest to the next frame to prevent GPU upload spikes.
        let deferred_count = completed
            .len()
            .saturating_sub(MAX_COMPLETED_UPLOADS_PER_FRAME);
        if deferred_count > 0 {
            // Move excess chunks to deferred queue (they're already sorted by distance,
            // so we defer the farthest chunks)
            let to_defer = completed.split_off(MAX_COMPLETED_UPLOADS_PER_FRAME);
            self.sim.streaming.deferred_uploads.extend(to_defer);
        }

        // Wait for any pending texture clear before uploading
        self.wait_for_pending_clear();

        // Metadata updates computed during chunk processing (avoids 32KB clone per chunk)
        struct MetadataUpdate {
            idx: usize,
            word_idx: usize,
            bit_idx: usize,
            is_empty: bool,
            mask_low: u32,
            mask_high: u32,
            packed_dist: [u32; 16],
        }
        let mut metadata_updates: Vec<MetadataUpdate> = Vec::new();
        let mut loaded = 0;
        let mut loaded_positions: Vec<Vector3<i32>> = Vec::new();
        {
            struct Upload {
                pos: Vector3<i32>,
                block: Vec<u8>,
                meta: Vec<u8>,
                custom: Vec<u8>,
            }

            let mut uploads: Vec<Upload> = Vec::new();

            // CRITICAL: Two-pass processing to fix tree overflow race condition.
            // If we process chunks in order (apply overflow, insert, extract), a chunk
            // processed early in the batch may be modified by a later chunk's overflow
            // AFTER its block_data was already extracted, causing stale GPU data.
            //
            // Fix: First pass applies ALL overflow, second pass inserts and extracts.
            // This ensures all cross-chunk modifications happen before any extraction.

            // First pass: Apply all overflow blocks from the batch
            // This handles both immediate application (target exists) and pending (target doesn't exist yet)
            for result in &completed {
                self.sim
                    .world
                    .apply_overflow_blocks(result.overflow_blocks.clone());
            }

            // Second pass: Insert chunks and extract block_data
            // Now all overflow targeting these chunks has been applied
            for result in completed {
                // Skip chunks that are outside the current texture bounds.
                // This can happen if texture origin shifted while chunks were in-flight.
                // These chunks will be re-requested next frame at their new positions.
                if world_pos_to_chunk_index(self.sim.texture_origin, result.position).is_none() {
                    continue;
                }

                // Insert chunk into world (will also apply any pending overflow for this chunk)
                self.sim.world.insert_chunk(result.position, result.chunk);

                // Extract block_data AFTER insert and AFTER all overflow has been applied
                let chunk = self
                    .sim
                    .world
                    .get_chunk(result.position)
                    .expect("Chunk should exist after insert");
                uploads.push(Upload {
                    pos: result.position,
                    block: chunk.to_block_data(),
                    meta: chunk.to_model_metadata(),
                    custom: chunk.custom_data_bytes().to_vec(),
                });
                loaded_positions.push(result.position);
                loaded += 1;
            }

            // Batch upload completed chunks to GPU and compute metadata updates
            // Computing SVT here avoids cloning block_data (32KB per chunk savings)
            if !uploads.is_empty() {
                let uploaded_positions: Vec<_> = uploads.iter().map(|u| u.pos).collect();

                // Compute metadata updates BEFORE upload (uses block_data without clone)
                for upload in &uploads {
                    if let Some(idx) = world_pos_to_chunk_index(self.sim.texture_origin, upload.pos)
                    {
                        let svt = ChunkSVT::from_bytes(&upload.block);
                        let word_idx = idx / 32;
                        let bit_idx = idx % 32;
                        metadata_updates.push(MetadataUpdate {
                            idx,
                            word_idx,
                            bit_idx,
                            is_empty: svt.brick_mask == 0,
                            mask_low: svt.brick_mask as u32,
                            mask_high: (svt.brick_mask >> 32) as u32,
                            packed_dist: pack_distances(&svt.brick_distances),
                        });
                    }
                }

                // Convert to slice references for upload
                let upload_slices: Vec<_> = uploads
                    .iter()
                    .map(|u| {
                        (
                            u.pos,
                            u.block.as_slice(),
                            u.meta.as_slice(),
                            u.custom.as_slice(),
                        )
                    })
                    .collect();
                self.upload_chunk_refs(&upload_slices);

                // Release uploads (block_data no longer needed - metadata already computed)
                drop(uploads);

                for pos in &uploaded_positions {
                    if let Some(chunk) = self.sim.world.get_chunk_mut(*pos) {
                        chunk.mark_clean();
                    }
                }

                // Already uploaded this frame; avoid a second upload in upload_world_to_gpu
                self.sim.world.remove_dirty_positions(&uploaded_positions);
            }

            // === Process network chunks (multiplayer mode) ===
            // Network chunks are simpler - no overflow handling needed
            if !network_chunks.is_empty() {
                let mut network_uploads: Vec<Upload> = Vec::new();

                for (pos, chunk) in network_chunks {
                    // Skip chunks outside texture bounds
                    if world_pos_to_chunk_index(self.sim.texture_origin, pos).is_none() {
                        continue;
                    }

                    // Insert chunk into world
                    self.sim.world.insert_chunk(pos, chunk);

                    // Extract data for upload
                    let chunk = self
                        .sim
                        .world
                        .get_chunk(pos)
                        .expect("Chunk should exist after insert");
                    network_uploads.push(Upload {
                        pos,
                        block: chunk.to_block_data(),
                        meta: chunk.to_model_metadata(),
                        custom: chunk.custom_data_bytes().to_vec(),
                    });
                    loaded_positions.push(pos);
                    loaded += 1;
                }

                // Upload network chunks to GPU
                if !network_uploads.is_empty() {
                    let uploaded_positions: Vec<_> =
                        network_uploads.iter().map(|u| u.pos).collect();

                    // Compute metadata updates
                    for upload in &network_uploads {
                        if let Some(idx) =
                            world_pos_to_chunk_index(self.sim.texture_origin, upload.pos)
                        {
                            let svt = ChunkSVT::from_bytes(&upload.block);
                            let word_idx = idx / 32;
                            let bit_idx = idx % 32;
                            metadata_updates.push(MetadataUpdate {
                                idx,
                                word_idx,
                                bit_idx,
                                is_empty: svt.brick_mask == 0,
                                mask_low: svt.brick_mask as u32,
                                mask_high: (svt.brick_mask >> 32) as u32,
                                packed_dist: pack_distances(&svt.brick_distances),
                            });
                        }
                    }

                    // Upload to GPU
                    let upload_slices: Vec<_> = network_uploads
                        .iter()
                        .map(|u| {
                            (
                                u.pos,
                                u.block.as_slice(),
                                u.meta.as_slice(),
                                u.custom.as_slice(),
                            )
                        })
                        .collect();
                    self.upload_chunk_refs(&upload_slices);

                    // Mark chunks clean
                    for pos in &uploaded_positions {
                        if let Some(chunk) = self.sim.world.get_chunk_mut(*pos) {
                            chunk.mark_clean();
                        }
                    }

                    // Avoid duplicate upload
                    self.sim.world.remove_dirty_positions(&uploaded_positions);
                }
            }
        }

        // === STEP 2: Queue new chunks for generation ===
        // First load visible chunks (view_distance), then preload chunks if capacity allows
        let yaw = self.sim.player.camera.rotation.y as f32;
        let view_dir = Some((yaw.sin(), -yaw.cos())); // XZ direction player is looking

        // Snapshot loader state for budgeting new requests.
        let loader_stats_before = self.sim.chunk_loader.stats();
        let queue_capacity = self.sim.chunk_loader.queue_capacity();
        let available_slots = queue_capacity
            .saturating_sub(loader_stats_before.queue_len + loader_stats_before.in_flight);

        // Get visible chunks first (highest priority)
        let visible_chunks = self.sim.world.get_chunks_to_load(
            player_chunk,
            self.sim.view_distance,
            (min_chunk, max_chunk),
            view_dir,
            None,
        );

        // Request visible chunks first
        let max_to_queue = (CHUNKS_PER_FRAME * 4).min(available_slots);
        let mut queued_visible = 0;
        let mut failed_visible = 0;
        if max_to_queue > 0 {
            let visible_to_request: Vec<_> =
                visible_chunks.into_iter().take(max_to_queue).collect();
            let RequestStats {
                queued,
                failed_full,
            } = self.sim.chunk_loader.request_chunks(&visible_to_request);
            queued_visible = queued;
            failed_visible = failed_full;
        }

        // Only request preload chunks if we have spare capacity
        let mut queued_preload = 0;
        let mut failed_preload = 0;
        // If we already hit a full queue on visible requests, skip preloading this frame.
        let queue_len_after_vis = self.sim.chunk_loader.stats().queue_len;
        let remaining_capacity = queue_capacity.saturating_sub(queue_len_after_vis);
        let near_full = queue_len_after_vis >= queue_capacity.saturating_mul(8) / 10;
        if !near_full && remaining_capacity > 0 && self.sim.load_distance > self.sim.view_distance {
            let preload_chunks = self.sim.world.get_chunks_to_load(
                player_chunk,
                self.sim.load_distance,
                (min_chunk, max_chunk),
                view_dir,
                Some(1),
            );
            // Filter to only chunks beyond view distance
            let preload_only: Vec<_> = preload_chunks
                .into_iter()
                .filter(|pos| {
                    let dx = (pos.x - player_chunk.x).abs();
                    let dz = (pos.z - player_chunk.z).abs();
                    dx > self.sim.view_distance || dz > self.sim.view_distance
                })
                .take(remaining_capacity.min(CHUNKS_PER_FRAME * 2))
                .collect();
            let RequestStats {
                queued,
                failed_full,
            } = self.sim.chunk_loader.request_chunks(&preload_only);
            queued_preload = queued;
            failed_preload = failed_full;
        }

        let queued = queued_visible + queued_preload;
        let failed_full = failed_visible + failed_preload;

        // Throttle noisy logs: only print occasionally or when verbose requested.
        let log_spam_frame = self.ui.frame.total_frames.is_multiple_of(60);
        if self.args.verbose && (queued > 0 || failed_full > 0) {
            log::debug!(
                "Queued {} chunks ({} failed: queue full) around ({}, {}, {})",
                queued,
                failed_full,
                player_chunk.x,
                player_chunk.y,
                player_chunk.z
            );
        } else if log_spam_frame && failed_full > 0 {
            log::debug!(
                "[chunk-load] queue_full: queued={} failed={} pos=({}, {}, {})",
                queued,
                failed_full,
                player_chunk.x,
                player_chunk.y,
                player_chunk.z
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
                        EMPTY_CUSTOM_DATA.as_slice(),
                    )
                })
                .collect();
            self.upload_chunk_refs(&chunks_to_clear);
        }

        // Apply pre-computed metadata updates to GPU buffers
        // These were computed during chunk processing to avoid 32KB clone per chunk
        if !metadata_updates.is_empty() {
            let mut chunk_meta_write = self.graphics.chunk_metadata_buffer.write().unwrap();
            let mut brick_mask_write = self.graphics.brick_mask_buffer.write().unwrap();
            let mut brick_dist_write = self.graphics.brick_dist_buffer.write().unwrap();

            for update in &metadata_updates {
                // Update chunk empty bit
                if update.is_empty {
                    self.sim.metadata_state.chunk_bits[update.word_idx] |= 1u32 << update.bit_idx;
                } else {
                    self.sim.metadata_state.chunk_bits[update.word_idx] &=
                        !(1u32 << update.bit_idx);
                }
                chunk_meta_write[update.word_idx] =
                    self.sim.metadata_state.chunk_bits[update.word_idx];

                // Update brick mask
                let mask_offset = update.idx * 2;
                self.sim.metadata_state.brick_masks[mask_offset] = update.mask_low;
                self.sim.metadata_state.brick_masks[mask_offset + 1] = update.mask_high;
                brick_mask_write[mask_offset] = update.mask_low;
                brick_mask_write[mask_offset + 1] = update.mask_high;

                // Update brick distances
                let dist_offset = update.idx * 16;
                for (i, word) in update.packed_dist.iter().enumerate() {
                    self.sim.metadata_state.brick_distances[dist_offset + i] = *word;
                    brick_dist_write[dist_offset + i] = *word;
                }
            }

            // NOTE: We intentionally do NOT queue for background refresh here.
            // The immediate update above already wrote to GPU buffers directly,
            // so queueing would cause duplicate SVT computation in update_metadata_buffers().
        }
        if !positions_to_clear.is_empty() {
            self.sim
                .metadata_state
                .queue_many(self.sim.texture_origin, positions_to_clear.iter().copied());
        }

        // Update chunk stats
        let loader_stats = self.sim.chunk_loader.stats();
        self.sim.chunk_stats = ChunkStats {
            loaded_count: self.sim.world.chunk_count(),
            dirty_count: self.sim.world.dirty_chunk_count(),
            in_flight_count: loader_stats.in_flight,
            queued_count: loader_stats.queue_len,
            queue_full_events: loader_stats.queue_full_events,
            dropped_results: loader_stats.dropped_stale_results,
            reupload_pending: self.sim.streaming.reupload_queue.len(),
            deferred_uploads: self.sim.streaming.deferred_uploads.len(),
            metadata_pending: self.sim.metadata_state.pending_len(),
            upload_budget: uploads_per_frame(),
            reupload_budget: reupload_per_frame(),
            metadata_budget: metadata_chunks_per_frame(),
            origin_chunk_x: self.sim.texture_origin.x / CHUNK_SIZE as i32,
            origin_chunk_z: self.sim.texture_origin.z / CHUNK_SIZE as i32,
            origin_shift_count: self.sim.origin_shift_count,
            memory_mb: (TEXTURE_SIZE_X * TEXTURE_SIZE_Y * TEXTURE_SIZE_Z) as f32
                / (1024.0 * 1024.0),
        };

        // Update last player chunk
        self.sim.last_player_chunk = player_chunk;

        (loaded_positions, loaded, unloaded)
    }

    pub fn upload_world_to_gpu(&mut self) {
        // Ensure any pending texture clear is complete before uploading
        self.wait_for_pending_clear();

        // Gradually mark chunks dirty after origin shifts to avoid stalls.
        let reupload_budget = reupload_per_frame();
        for _ in 0..reupload_budget {
            if let Some(pos) = self.sim.streaming.reupload_queue.pop_front() {
                if let Some(chunk) = self.sim.world.get_chunk_mut(pos) {
                    chunk.mark_dirty();
                }
                self.sim
                    .metadata_state
                    .queue_world_chunk(self.sim.texture_origin, pos);
                self.sim.world.requeue_dirty(&[pos]);
            } else {
                break;
            }
        }

        // Drain a bounded number of dirty chunk positions from world to avoid frame stalls
        let max_uploads = uploads_per_frame();
        let dirty_positions = self.sim.world.drain_dirty_chunks_limit(max_uploads);
        if dirty_positions.is_empty() {
            return;
        }

        struct Upload<'a> {
            pos: Vector3<i32>,
            block: &'a [u8],
            meta: Ref<'a, [u8]>,
            custom: Ref<'a, [u8]>,
        }

        let mut uploads: Vec<Upload> = Vec::new();
        for &pos in &dirty_positions {
            if let Some(chunk) = self.sim.world.get_chunk(pos) {
                uploads.push(Upload {
                    pos,
                    block: chunk.block_bytes(),
                    meta: chunk.model_metadata_bytes(),
                    custom: chunk.custom_data_bytes(),
                });
            }
        }

        if !uploads.is_empty() {
            self.sim.profiler.chunks_uploaded += uploads.len() as u32;
            let upload_slices: Vec<_> = uploads
                .iter()
                .map(|u| (u.pos, u.block, &*u.meta, &*u.custom))
                .collect();
            self.upload_chunk_refs(&upload_slices);

            // Immediate metadata update to prevent visibility gaps.
            // Parallelize SVT computation across uploaded chunks, then apply results sequentially.
            if !upload_slices.is_empty() {
                let svt_results: Vec<(Vector3<i32>, ChunkSVT)> = upload_slices
                    .par_iter()
                    .map(|(pos, block_data, _, _)| (*pos, ChunkSVT::from_bytes(block_data)))
                    .collect();

                let mut chunk_meta_write = self.graphics.chunk_metadata_buffer.write().unwrap();
                let mut brick_mask_write = self.graphics.brick_mask_buffer.write().unwrap();
                let mut brick_dist_write = self.graphics.brick_dist_buffer.write().unwrap();

                for (pos, svt) in &svt_results {
                    if let Some(idx) = world_pos_to_chunk_index(self.sim.texture_origin, *pos) {
                        let word_idx = idx / 32;
                        let bit_idx = idx % 32;
                        if svt.brick_mask == 0 {
                            self.sim.metadata_state.chunk_bits[word_idx] |= 1u32 << bit_idx;
                        } else {
                            self.sim.metadata_state.chunk_bits[word_idx] &= !(1u32 << bit_idx);
                        }
                        chunk_meta_write[word_idx] = self.sim.metadata_state.chunk_bits[word_idx];

                        let mask_offset = idx * 2;
                        self.sim.metadata_state.brick_masks[mask_offset] = svt.brick_mask as u32;
                        self.sim.metadata_state.brick_masks[mask_offset + 1] =
                            (svt.brick_mask >> 32) as u32;
                        brick_mask_write[mask_offset] = svt.brick_mask as u32;
                        brick_mask_write[mask_offset + 1] = (svt.brick_mask >> 32) as u32;

                        let dist_offset = idx * 16;
                        let packed_dist = pack_distances(&svt.brick_distances);
                        for (i, word) in packed_dist.iter().enumerate() {
                            self.sim.metadata_state.brick_distances[dist_offset + i] = *word;
                            brick_dist_write[dist_offset + i] = *word;
                        }
                    }
                }
            }

            // Release borrows before marking chunks clean
            let uploaded_positions: Vec<_> = uploads.iter().map(|u| u.pos).collect();
            drop(uploads);

            for pos in dirty_positions {
                if let Some(chunk) = self.sim.world.get_chunk_mut(pos) {
                    chunk.mark_clean();
                }
            }
            // NOTE: We intentionally do NOT queue for background metadata refresh here.
            // The immediate update above (lines 852-884) already computed SVT and updated
            // both CPU metadata_state and GPU buffers. Calling queue_many would cause
            // update_metadata_buffers() to recompute SVT redundantly.

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
            custom: Ref<'a, [u8]>,
        }

        let mut uploads: Vec<Upload> = Vec::new();

        for pos in &dirty_positions {
            if let Some(chunk) = self.sim.world.get_chunk(*pos) {
                uploads.push(Upload {
                    pos: *pos,
                    block: chunk.block_bytes(),
                    meta: chunk.model_metadata_bytes(),
                    custom: chunk.custom_data_bytes(),
                });
            }
        }

        if uploads.is_empty() {
            return;
        }

        self.sim.profiler.chunks_uploaded += uploads.len() as u32;
        let upload_slices: Vec<_> = uploads
            .iter()
            .map(|u| (u.pos, u.block, &*u.meta, &*u.custom))
            .collect();
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

    /// Waits for any pending texture clear to complete before uploading.
    /// This blocks until the GPU finishes the clear operation, but since we only
    /// wait when uploads are ready (not immediately after issuing the clear),
    /// CPU work can overlap with the GPU clear in the meantime.
    /// Call this before any GPU upload operations.
    fn wait_for_pending_clear(&mut self) {
        if let Some(fence) = self.sim.streaming.pending_clear_fence.take() {
            // Wait for the clear to complete - typically 1-5ms
            // We must wait fully because vulkano's FenceSignalFuture requires
            // complete cleanup before the command buffer can be released.
            if let Err(e) = fence.wait(None) {
                log::warn!("[GPU] Texture clear fence error: {:?}", e);
            }
        }
    }

    /// Uploads chunk data that is already slice-backed to GPU.
    fn upload_chunk_refs(&self, uploads: &[ChunkDataSlice<'_>]) {
        self.upload_chunk_refs_with(uploads, false);
    }

    /// Like [`Self::upload_chunk_refs`] but allows the caller to enable the
    /// zero-slice skip optimization. Must only be used when the destination
    /// texture region is guaranteed zero (i.e., right after a full texture
    /// clear during an origin shift).
    fn upload_chunk_refs_with(&self, uploads: &[ChunkDataSlice<'_>], skip_zero_slices: bool) {
        if uploads.is_empty() {
            return;
        }
        upload_chunks_batched(
            &self.graphics.memory_allocator,
            &self.graphics.command_buffer_allocator,
            ChunkUploadConfig {
                transfer_queue: &self.graphics.transfer_queue,
                graphics_queue_family: self.graphics.graphics_queue_family,
                separate_transfer_queue: self.graphics.separate_transfer_queue,
                voxel_image: &self.graphics.voxel_image,
                model_metadata_image: &self.graphics.model_metadata,
                block_custom_data_image: &self.graphics.block_custom_data,
                texture_origin: self.sim.texture_origin,
                skip_zero_slices,
            },
            uploads,
        );
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

        // After a texture-origin shift we rebuild gradually to avoid stalls.
        let budget = if reset_buffers {
            metadata_reset_budget()
        } else {
            metadata_chunks_per_frame()
        };

        let work_indices = self.sim.metadata_state.take_work(budget);

        // Process chunks first to update CPU metadata state
        // This ensures metadata reflects actual chunk data before any GPU sync
        if !work_indices.is_empty() {
            let mut tasks = Vec::with_capacity(work_indices.len());
            // Collect immutable borrows first to avoid overlapping mutable borrows.
            let mut borrows: Vec<(usize, &Chunk)> = Vec::with_capacity(work_indices.len());
            for idx in &work_indices {
                let world_pos = chunk_index_to_world_pos(*idx, self.sim.texture_origin);
                if let Some(chunk) = self.sim.world.get_chunk(world_pos) {
                    borrows.push((*idx, chunk));
                } else {
                    tasks.push((*idx, ChunkWork::Missing));
                }
            }

            for (idx, chunk) in borrows {
                // We only need immutable access for block_slice; metadata is kept up to date elsewhere.
                tasks.push((idx, ChunkWork::Borrow(chunk.block_slice())));
            }

            let results: Vec<ChunkMetaResult> = tasks
                .into_par_iter()
                .map(|(idx, work)| match work {
                    ChunkWork::Missing => ChunkMetaResult {
                        idx,
                        is_empty: true,
                        mask_low: 0,
                        mask_high: 0,
                        dist: [0xFFFF_FFFF; 16],
                    },
                    ChunkWork::Borrow(blocks) => {
                        let svt = ChunkSVT::from_block_data(blocks);
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

            // Update CPU metadata state with computed values
            for res in &results {
                let word_idx = res.idx / 32;
                let bit_idx = res.idx % 32;
                if res.is_empty {
                    self.sim.metadata_state.chunk_bits[word_idx] |= 1u32 << bit_idx;
                } else {
                    self.sim.metadata_state.chunk_bits[word_idx] &= !(1u32 << bit_idx);
                }

                let mask_offset = res.idx * 2;
                self.sim.metadata_state.brick_masks[mask_offset] = res.mask_low;
                self.sim.metadata_state.brick_masks[mask_offset + 1] = res.mask_high;

                let dist_offset = res.idx * 16;
                for (i, word) in res.dist.iter().enumerate() {
                    self.sim.metadata_state.brick_distances[dist_offset + i] = *word;
                }
            }
        }

        // Now sync to GPU. On origin-shift reset the old 288 KiB full-buffer
        // copy was ~5-10 ms; it's redundant because the GLSL traversal (see
        // shaders/accel.glsl `isChunkEmpty` / `isBrickEmptyFast`) short-
        // circuits on `chunk_bits == 1` for empty chunks and never reads
        // `brick_masks` / `brick_distances` for them. So on reset we only
        // push the tiny `chunk_bits` buffer (all "empty" placeholder,
        // < 1 KiB) and let the incremental path fill in brick data as
        // chunks get processed.
        //
        // Ordering invariant for the incremental path: a chunk must be
        // visible as "not empty" only *after* its brick_mask +
        // brick_distances have been written. The loop below writes brick
        // data first, then flips the chunk_bit.
        if reset_buffers {
            let mut chunk_meta_write = self.graphics.chunk_metadata_buffer.write().unwrap();
            chunk_meta_write.copy_from_slice(&self.sim.metadata_state.chunk_bits);
        }

        if !work_indices.is_empty() {
            let mut chunk_meta_write = self.graphics.chunk_metadata_buffer.write().unwrap();
            let mut brick_mask_write = self.graphics.brick_mask_buffer.write().unwrap();
            let mut brick_dist_write = self.graphics.brick_dist_buffer.write().unwrap();

            for idx in &work_indices {
                // 1) Brick mask + distances first so they're visible before
                //    the chunk_bit is flipped to "not empty".
                let mask_offset = idx * 2;
                brick_mask_write[mask_offset] = self.sim.metadata_state.brick_masks[mask_offset];
                brick_mask_write[mask_offset + 1] =
                    self.sim.metadata_state.brick_masks[mask_offset + 1];

                let dist_offset = idx * 16;
                for i in 0..16 {
                    brick_dist_write[dist_offset + i] =
                        self.sim.metadata_state.brick_distances[dist_offset + i];
                }

                // 2) Now flip the chunk_bit. For non-empty chunks this
                //    publishes the brick data above; for empty chunks it
                //    keeps the "empty" flag that was seeded on reset.
                let word_idx = idx / 32;
                chunk_meta_write[word_idx] = self.sim.metadata_state.chunk_bits[word_idx];
            }
        }

        self.sim.metadata_state.mark_results_applied();
        self.sim.profiler.metadata_update_us += t_meta.elapsed().as_micros() as u64;
    }
}

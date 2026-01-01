use crate::App;
use crate::chunk::{CHUNK_SIZE, EMPTY_CHUNK_DATA, EMPTY_MODEL_METADATA};
use crate::constants::{
    CHUNKS_PER_FRAME, LOADED_CHUNKS_X, LOADED_CHUNKS_Z, TEXTURE_SIZE_X, TEXTURE_SIZE_Y,
    TEXTURE_SIZE_Z, WORLD_CHUNKS_Y,
};
use crate::gpu_resources::{
    upload_chunks_batched, update_brick_metadata, update_chunk_metadata,
};
use crate::utils::ChunkStats;
use nalgebra::{Vector3, vector};
use std::time::Instant;
use vulkano::buffer::{Buffer, BufferCreateInfo, BufferUsage};
use vulkano::command_buffer::{
    AutoCommandBufferBuilder, BufferImageCopy, CommandBufferUsage, CopyBufferToImageInfo,
    PrimaryCommandBufferAbstract,
};
use vulkano::memory::allocator::{AllocationCreateInfo, MemoryTypeFilter};
use vulkano::sync::GpuFuture;

impl App {
    pub fn check_and_shift_texture_origin(&mut self) -> bool {
        let player_chunk = self
            .player
            .get_chunk_pos(self.world_extent, self.texture_origin);

        // Calculate texture center in chunk coordinates
        let texture_center_chunk = Vector3::new(
            self.texture_origin.x / CHUNK_SIZE as i32 + LOADED_CHUNKS_X / 2,
            0, // Y doesn't shift
            self.texture_origin.z / CHUNK_SIZE as i32 + LOADED_CHUNKS_Z / 2,
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
            self.texture_origin.x,
            self.texture_origin.z,
            new_origin.x,
            new_origin.z,
            player_chunk.x,
            player_chunk.z
        );

        // Save old origin to adjust camera position
        let old_origin = self.texture_origin;
        self.texture_origin = new_origin;

        // Adjust camera position to maintain the same world position
        let origin_delta = old_origin - new_origin;
        let scale = Vector3::new(
            self.world_extent[0] as f64,
            self.world_extent[1] as f64,
            self.world_extent[2] as f64,
        );
        self.player.camera.position.x += origin_delta.x as f64 / scale.x;
        self.player.camera.position.y += origin_delta.y as f64 / scale.y;
        self.player.camera.position.z += origin_delta.z as f64 / scale.z;

        // Re-upload all loaded chunks to their new texture positions
        let chunks_to_upload: Vec<(Vector3<i32>, Vec<u8>, Vec<u8>)> = self
            .world
            .chunks()
            .map(|(pos, chunk)| (*pos, chunk.to_block_data(), chunk.to_model_metadata()))
            .collect();

        if !chunks_to_upload.is_empty() {
            // Clear the texture first (set all to air)
            self.clear_voxel_texture();
            // Upload chunks at new positions - convert to slice references
            let upload_refs: Vec<_> = chunks_to_upload
                .iter()
                .map(|(pos, block_data, model_metadata)| {
                    (*pos, block_data.as_slice(), model_metadata.as_slice())
                })
                .collect();
            upload_chunks_batched(
                &self.memory_allocator,
                &self.command_buffer_allocator,
                &self.queue,
                &self.voxel_image,
                &self.model_metadata,
                self.texture_origin,
                &upload_refs,
            );
        }

        true
    }

    pub fn clear_voxel_texture(&self) {
        let total_size = TEXTURE_SIZE_X * TEXTURE_SIZE_Y * TEXTURE_SIZE_Z;
        let empty_data = vec![0u8; total_size];

        let src_buffer = Buffer::from_iter(
            self.memory_allocator.clone(),
            BufferCreateInfo {
                usage: BufferUsage::TRANSFER_SRC,
                ..Default::default()
            },
            AllocationCreateInfo {
                memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                    | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                ..Default::default()
            },
            empty_data,
        )
        .unwrap();

        let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
            self.command_buffer_allocator.clone(),
            self.queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )
        .unwrap();

        command_buffer_builder
            .copy_buffer_to_image(CopyBufferToImageInfo {
                regions: [BufferImageCopy {
                    buffer_offset: 0,
                    buffer_row_length: TEXTURE_SIZE_X as u32,
                    buffer_image_height: TEXTURE_SIZE_Y as u32,
                    image_subresource: self.voxel_image.subresource_layers(),
                    image_offset: [0, 0, 0],
                    image_extent: [
                        TEXTURE_SIZE_X as u32,
                        TEXTURE_SIZE_Y as u32,
                        TEXTURE_SIZE_Z as u32,
                    ],
                    ..Default::default()
                }]
                .into(),
                ..CopyBufferToImageInfo::buffer_image(src_buffer, self.voxel_image.clone())
            })
            .unwrap();

        command_buffer_builder
            .build()
            .unwrap()
            .execute(self.queue.clone())
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
                self.texture_origin.x, self.texture_origin.z
            );
        }

        let player_chunk = self
            .player
            .get_chunk_pos(self.world_extent, self.texture_origin);

        // Infinite world in X/Z, bounded in Y (0 to WORLD_CHUNKS_Y-1)
        let min_chunk = vector![i32::MIN, 0, i32::MIN];
        let max_chunk = vector![i32::MAX, WORLD_CHUNKS_Y - 1, i32::MAX];

        // === STEP 1: Receive completed chunks from background threads ===
        let completed = self.chunk_loader.receive_chunks();
        let mut chunks_to_upload: Vec<(Vector3<i32>, Vec<u8>, Vec<u8>)> = Vec::new();
        let mut loaded = 0;

        for result in completed {
            // Get model metadata before inserting chunk
            let model_metadata = result.chunk.to_model_metadata();
            // Insert chunk into world
            self.world.insert_chunk(result.position, result.chunk);
            chunks_to_upload.push((result.position, result.block_data, model_metadata));
            loaded += 1;
        }

        // Batch upload completed chunks to GPU
        if !chunks_to_upload.is_empty() {
            // Convert to slice references for upload
            let upload_refs: Vec<_> = chunks_to_upload
                .iter()
                .map(|(pos, block_data, model_metadata)| {
                    (*pos, block_data.as_slice(), model_metadata.as_slice())
                })
                .collect();
            upload_chunks_batched(
                &self.memory_allocator,
                &self.command_buffer_allocator,
                &self.queue,
                &self.voxel_image,
                &self.model_metadata,
                self.texture_origin,
                &upload_refs,
            );

            // Mark chunks as clean
            for (pos, _, _) in &chunks_to_upload {
                if let Some(chunk) = self.world.get_chunk_mut(*pos) {
                    chunk.mark_clean();
                }
            }
        }

        // === STEP 2: Queue new chunks for generation ===
        let to_load =
            self.world
                .get_chunks_to_load(player_chunk, self.view_distance, (min_chunk, max_chunk));

        // Queue chunks for async generation (ChunkLoader handles deduplication)
        let max_to_queue = CHUNKS_PER_FRAME * 4;
        let queued = self
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
            .world
            .get_chunks_to_unload(player_chunk, self.unload_distance);

        let mut unloaded = 0;
        let positions_to_clear: Vec<_> = to_unload
            .iter()
            .take(CHUNKS_PER_FRAME)
            .map(|pos| {
                // Cancel pending generation for this chunk if queued
                self.chunk_loader.cancel_chunk(*pos);
                self.world.remove_chunk(*pos);
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
            upload_chunks_batched(
                &self.memory_allocator,
                &self.command_buffer_allocator,
                &self.queue,
                &self.voxel_image,
                &self.model_metadata,
                self.texture_origin,
                &chunks_to_clear,
            );
        }

        // Update chunk metadata if any chunks were loaded or unloaded
        if !chunks_to_upload.is_empty() || !positions_to_clear.is_empty() {
            let t_meta = Instant::now();
            update_chunk_metadata(
                &mut self.world,
                &self.chunk_metadata_buffer,
                self.texture_origin,
            );
            update_brick_metadata(
                &self.world,
                &self.brick_mask_buffer,
                &self.brick_dist_buffer,
                self.texture_origin,
            );
            self.profiler.metadata_update_us += t_meta.elapsed().as_micros() as u64;
        }

        // Update chunk stats
        self.chunk_stats = ChunkStats {
            loaded_count: self.world.chunk_count(),
            dirty_count: self.world.dirty_chunk_count(),
            in_flight_count: self.chunk_loader.in_flight_count(),
            memory_mb: (TEXTURE_SIZE_X * TEXTURE_SIZE_Y * TEXTURE_SIZE_Z) as f32
                / (1024.0 * 1024.0),
        };

        // Update last player chunk
        self.last_player_chunk = player_chunk;

        (loaded, unloaded)
    }

    pub fn upload_world_to_gpu(&mut self) {
        // Drain dirty chunk positions from world
        let dirty_positions = self.world.drain_dirty_chunks();
        if dirty_positions.is_empty() {
            return;
        }

        // Collect chunk data for all dirty chunks
        let chunks_to_upload: Vec<(Vector3<i32>, Vec<u8>, Vec<u8>)> = dirty_positions
            .iter()
            .filter_map(|&pos| {
                self.world.get_chunk_mut(pos).map(|chunk| {
                    let block_data = chunk.to_block_data();
                    let model_metadata = chunk.to_model_metadata();
                    chunk.mark_clean();
                    (pos, block_data, model_metadata)
                })
            })
            .collect();

        if !chunks_to_upload.is_empty() {
            self.profiler.chunks_uploaded += chunks_to_upload.len() as u32;
            // Convert to slice references for upload
            let upload_refs: Vec<_> = chunks_to_upload
                .iter()
                .map(|(pos, block_data, model_metadata)| {
                    (*pos, block_data.as_slice(), model_metadata.as_slice())
                })
                .collect();
            upload_chunks_batched(
                &self.memory_allocator,
                &self.command_buffer_allocator,
                &self.queue,
                &self.voxel_image,
                &self.model_metadata,
                self.texture_origin,
                &upload_refs,
            );

            // Update metadata so shader doesn't skip newly non-empty bricks
            let t_meta = Instant::now();
            update_chunk_metadata(
                &mut self.world,
                &self.chunk_metadata_buffer,
                self.texture_origin,
            );
            update_brick_metadata(
                &self.world,
                &self.brick_mask_buffer,
                &self.brick_dist_buffer,
                self.texture_origin,
            );
            self.profiler.metadata_update_us += t_meta.elapsed().as_micros() as u64;
        }
    }

    pub fn upload_all_dirty_chunks(&mut self) {
        let chunks_to_upload: Vec<(Vector3<i32>, Vec<u8>, Vec<u8>)> = self
            .world
            .chunks()
            .filter(|(_, chunk)| chunk.dirty)
            .map(|(pos, chunk)| (*pos, chunk.to_block_data(), chunk.to_model_metadata()))
            .collect();

        if chunks_to_upload.is_empty() {
            return;
        }

        self.profiler.chunks_uploaded += chunks_to_upload.len() as u32;
        let upload_refs: Vec<_> = chunks_to_upload
            .iter()
            .map(|(pos, block_data, model_metadata)| {
                (*pos, block_data.as_slice(), model_metadata.as_slice())
            })
            .collect();

        upload_chunks_batched(
            &self.memory_allocator,
            &self.command_buffer_allocator,
            &self.queue,
            &self.voxel_image,
            &self.model_metadata,
            self.texture_origin,
            &upload_refs,
        );

        for (pos, _, _) in &chunks_to_upload {
            if let Some(chunk) = self.world.get_chunk_mut(*pos) {
                chunk.mark_clean();
            }
        }

        update_chunk_metadata(
            &mut self.world,
            &self.chunk_metadata_buffer,
            self.texture_origin,
        );
        update_brick_metadata(
            &self.world,
            &self.brick_mask_buffer,
            &self.brick_dist_buffer,
            self.texture_origin,
        );
    }
}

//! Chunk-upload pipeline: per-frame batched DMA of voxel data into the resident
//! GPU texture window, plus the chunk-metadata and SVT brick-mask buffers that
//! drive empty-space skipping in the ray-march compute shader.

use super::*;

use std::cell::RefCell;
use std::sync::Arc;

use nalgebra::Vector3;
use vulkano::{
    buffer::{Buffer, BufferCreateInfo, BufferUsage, Subbuffer},
    command_buffer::{
        AutoCommandBufferBuilder, BufferImageCopy, CommandBufferUsage, CopyBufferToImageInfo,
        allocator::StandardCommandBufferAllocator,
    },
    descriptor_set::{
        DescriptorSet, WriteDescriptorSet, allocator::StandardDescriptorSetAllocator,
    },
    device::Queue,
    image::Image,
    memory::allocator::{AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator},
    pipeline::ComputePipeline,
};

use crate::chunk::CHUNK_SIZE;
use crate::constants::{LOADED_CHUNKS_X, LOADED_CHUNKS_Z, WORLD_CHUNKS_Y};

pub const TOTAL_CHUNKS: usize =
    LOADED_CHUNKS_X as usize * WORLD_CHUNKS_Y as usize * LOADED_CHUNKS_Z as usize;
/// Number of u32 words needed to store 1 bit per chunk
pub const CHUNK_METADATA_WORDS: usize = TOTAL_CHUNKS.div_ceil(32);

thread_local! {
    // Reusable scratch buffers to avoid per-frame allocations during streaming.
    static CHUNK_META_SCRATCH: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
    static BRICK_MASK_SCRATCH: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
    static BRICK_DIST_SCRATCH: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

/// Creates a storage buffer and descriptor set for chunk metadata (empty/solid flags).
/// Uses HOST_COHERENT memory for immediate GPU visibility without sync stalls.
pub fn get_chunk_metadata_set(
    memory_allocator: Arc<StandardMemoryAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
) -> (Subbuffer<[u32]>, Arc<DescriptorSet>) {
    // Create a coherent storage buffer for chunk metadata (bit-packed flags)
    // HOST_COHERENT eliminates per-frame sync stalls when updating metadata
    let chunk_metadata_buffer =
        make_coherent_storage_buffer::<u32>(&memory_allocator, CHUNK_METADATA_WORDS as u64);

    // Create descriptor set at set index 5
    let descriptor_set = make_set(
        &descriptor_set_allocator,
        render_pipeline,
        5,
        [WriteDescriptorSet::buffer(0, chunk_metadata_buffer.clone())],
    );

    (chunk_metadata_buffer, descriptor_set)
}

pub struct ChunkUploadConfig<'a> {
    /// Queue to use for DMA transfers (dedicated transfer or graphics fallback).
    pub transfer_queue: &'a Arc<Queue>,
    /// Graphics queue-family index (for ownership-transfer barriers).
    pub graphics_queue_family: u32,
    /// `true` when the transfer queue belongs to a different family than the
    /// graphics queue (enables explicit ownership transfers on discrete GPUs).
    pub separate_transfer_queue: bool,
    /// Main 3D block-type voxel texture.
    pub voxel_image: &'a Arc<Image>,
    /// Per-block model metadata (model_id + rotation).
    pub model_metadata_image: &'a Arc<Image>,
    /// Per-block custom data (e.g. picture_id).
    pub block_custom_data_image: &'a Arc<Image>,
    /// World-space block origin of the currently resident texture window.
    pub texture_origin: Vector3<i32>,
    /// If `true`, skip uploading model_metadata / custom_data slices that are
    /// entirely zero. Only safe when the destination texture region is
    /// guaranteed zero (e.g., directly after `clear_voxel_texture_async`).
    /// For regular dirty-chunk uploads this MUST be `false` because the GPU
    /// image may still hold non-zero bytes from a previous upload.
    pub skip_zero_slices: bool,
}

/// Boxed-error result used by per-frame GPU upload paths. Errors are propagated
/// rather than panicked so the caller can log and retry the batch next frame
/// instead of crashing mid-frame. Covers both staging-buffer allocation
/// (`AllocateBufferError`) and host-visible mapping writes (`HostAccessError`)
/// via the boxed trait object.
pub type GpuResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Unit-returning specialization for upload-style operations.
pub type GpuUploadResult = GpuResult<()>;

/// Uploads chunk data to GPU textures using async DMA transfers.
///
/// On discrete GPUs with separate transfer queues this allows PCIe transfers
/// to run in parallel with graphics workloads for better performance.
pub fn upload_chunks_batched(
    memory_allocator: &Arc<StandardMemoryAllocator>,
    command_buffer_allocator: &Arc<StandardCommandBufferAllocator>,
    config: ChunkUploadConfig<'_>,
    chunks: &[ChunkDataSlice<'_>],
) -> GpuUploadResult {
    let ChunkUploadConfig {
        transfer_queue,
        graphics_queue_family,
        separate_transfer_queue,
        voxel_image,
        model_metadata_image,
        block_custom_data_image,
        texture_origin,
        skip_zero_slices,
    } = config;
    if chunks.is_empty() {
        return Ok(());
    }

    // Filter uploads that fit into the current texture window and collect offsets.
    struct Upload<'a> {
        offset: [u32; 3],
        block_data: &'a [u8],
        model_metadata: &'a [u8],
        custom_data: &'a [u8],
        /// True if `model_metadata` has any non-zero bytes.
        /// False entries skip memcpy and region emission — the destination image
        /// is already zero (cleared on origin shift and zero-initialized at startup).
        upload_meta: bool,
        /// True if `custom_data` has any non-zero bytes. See `upload_meta`.
        upload_custom: bool,
    }
    let mut uploads: Vec<Upload> = Vec::with_capacity(chunks.len());

    for (chunk_pos, block_data, model_metadata, custom_data) in chunks {
        // Convert world chunk position to texture position
        // World block position = chunk_pos * CHUNK_SIZE
        // Texture block position = world_block_pos - texture_origin
        let world_block_x = chunk_pos.x * CHUNK_SIZE as i32;
        let world_block_y = chunk_pos.y * CHUNK_SIZE as i32;
        let world_block_z = chunk_pos.z * CHUNK_SIZE as i32;

        let texture_x = world_block_x - texture_origin.x;
        let texture_y = world_block_y - texture_origin.y;
        let texture_z = world_block_z - texture_origin.z;

        // Skip chunks outside texture bounds
        if texture_x < 0
            || texture_y < 0
            || texture_z < 0
            || texture_x + CHUNK_SIZE as i32 > crate::constants::TEXTURE_SIZE_X as i32
            || texture_y + CHUNK_SIZE as i32 > crate::constants::TEXTURE_SIZE_Y as i32
            || texture_z + CHUNK_SIZE as i32 > crate::constants::TEXTURE_SIZE_Z as i32
        {
            continue;
        }

        uploads.push(Upload {
            offset: [texture_x as u32, texture_y as u32, texture_z as u32],
            block_data,
            model_metadata,
            custom_data,
            upload_meta: true,
            upload_custom: true,
        });
    }

    if uploads.is_empty() {
        return Ok(());
    }

    // When skip_zero_slices is enabled (origin-shift path only), detect
    // all-zero model_metadata / custom_data slices in parallel. Most fresh
    // terrain chunks have no model_metadata and no custom_data, so ~90%+ of
    // these slices can be skipped entirely. Safe only when the destination
    // texture region is guaranteed zero.
    if skip_zero_slices {
        use rayon::prelude::*;
        uploads.par_iter_mut().for_each(|u| {
            u.upload_meta = u.model_metadata.iter().any(|&b| b != 0);
            u.upload_custom = u.custom_data.iter().any(|&b| b != 0);
        });
    }

    // Sort uploads by (y, x, z) so consecutive entries with the same (y, x)
    // and sequential z form runs that can be merged into a single
    // BufferImageCopy with image_extent.z = 32 * run_len. MoltenVK's
    // `vkQueueSubmit` cost scales linearly with the number of copy regions
    // (~6 μs/region), so collapsing 1584 chunks into ~200 runs is the
    // largest remaining win after zero-slice skipping.
    uploads.sort_unstable_by_key(|u| (u.offset[1], u.offset[0], u.offset[2]));

    let total_block_bytes: usize = uploads.iter().map(|u| u.block_data.len()).sum();
    let total_meta_bytes: usize = uploads
        .iter()
        .filter(|u| u.upload_meta)
        .map(|u| u.model_metadata.len())
        .sum();
    let total_custom_bytes: usize = uploads
        .iter()
        .filter(|u| u.upload_custom)
        .map(|u| u.custom_data.len())
        .sum();

    // Profiling: big-batch origin-shift uploads log phase breakdown when
    // ORIGIN_SHIFT_PROFILE env var is set.
    let profile = uploads.len() >= 256 && std::env::var_os("ORIGIN_SHIFT_PROFILE").is_some();
    let t_total = std::time::Instant::now();

    // Acquire a slot in the ring buffer (may block if all slots busy)
    // Also reclaims completed transfers and their staging buffers
    let t_ring = std::time::Instant::now();
    let (slot_index, reclaimed) = TRANSFER_RING.with(|ring| ring.borrow_mut().acquire_slot());
    let ring_us = t_ring.elapsed();

    // Return reclaimed staging buffers to the pool
    STAGING_POOL.with(|pool| {
        let mut p = pool.borrow_mut();
        for buffers in reclaimed {
            if p.len() < STAGING_POOL_MAX {
                p.push(buffers);
            }
        }
    });

    // Get staging buffers - prefer reusing from pool
    let t_staging = std::time::Instant::now();
    let mut staging_allocated = false;
    let (block_staging, meta_staging, custom_staging) =
        STAGING_POOL.with(|pool| -> GpuResult<StagingBufferPair> {
            let mut p = pool.borrow_mut();

            // Find a triple with sufficient sizes
            let idx_opt = p.iter().position(|(b, m, c)| {
                b.size() as usize >= total_block_bytes
                    && m.size() as usize >= total_meta_bytes
                    && c.size() as usize >= total_custom_bytes
            });

            if let Some(idx) = idx_opt {
                Ok(p.swap_remove(idx))
            } else {
                staging_allocated = true;
                // Allocate new staging buffers
                let block_buf = Buffer::new_slice::<u8>(
                    memory_allocator.clone(),
                    BufferCreateInfo {
                        usage: BufferUsage::TRANSFER_SRC,
                        ..Default::default()
                    },
                    AllocationCreateInfo {
                        memory_type_filter: MemoryTypeFilter::PREFER_HOST
                            | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                        ..Default::default()
                    },
                    total_block_bytes as u64,
                )?;

                // Allocate at least 1 byte so the staging buffer is always valid
                // even when the first shift happens to have no meta/custom to upload.
                let meta_buf = Buffer::new_slice::<u8>(
                    memory_allocator.clone(),
                    BufferCreateInfo {
                        usage: BufferUsage::TRANSFER_SRC,
                        ..Default::default()
                    },
                    AllocationCreateInfo {
                        memory_type_filter: MemoryTypeFilter::PREFER_HOST
                            | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                        ..Default::default()
                    },
                    total_meta_bytes.max(1) as u64,
                )?;

                let custom_buf = Buffer::new_slice::<u8>(
                    memory_allocator.clone(),
                    BufferCreateInfo {
                        usage: BufferUsage::TRANSFER_SRC,
                        ..Default::default()
                    },
                    AllocationCreateInfo {
                        memory_type_filter: MemoryTypeFilter::PREFER_HOST
                            | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                        ..Default::default()
                    },
                    total_custom_bytes.max(1) as u64,
                )?;

                Ok((block_buf, meta_buf, custom_buf))
            }
        })?;
    let staging_us = t_staging.elapsed();

    // Write data to staging buffers (skip zero slices entirely).
    let t_memcpy = std::time::Instant::now();
    {
        let mut block_write = block_staging.write()?;
        let mut block_cursor = 0usize;
        for upload in &uploads {
            let blen = upload.block_data.len();
            block_write[block_cursor..block_cursor + blen].copy_from_slice(upload.block_data);
            block_cursor += blen;
        }

        if total_meta_bytes > 0 {
            let mut meta_write = meta_staging.write()?;
            let mut meta_cursor = 0usize;
            for upload in &uploads {
                if !upload.upload_meta {
                    continue;
                }
                let mlen = upload.model_metadata.len();
                meta_write[meta_cursor..meta_cursor + mlen].copy_from_slice(upload.model_metadata);
                meta_cursor += mlen;
            }
        }

        if total_custom_bytes > 0 {
            let mut custom_write = custom_staging.write()?;
            let mut custom_cursor = 0usize;
            for upload in &uploads {
                if !upload.upload_custom {
                    continue;
                }
                let clen = upload.custom_data.len();
                custom_write[custom_cursor..custom_cursor + clen]
                    .copy_from_slice(upload.custom_data);
                custom_cursor += clen;
            }
        }
    }
    let memcpy_us = t_memcpy.elapsed();

    // Build copy regions, merging Z-adjacent chunks into a single region per run.
    // Two chunks can be merged when they share (y, x) and their z differs by exactly
    // CHUNK_SIZE, AND they are consecutive in the staging buffer (no skipped chunk
    // between them for that slice). The block slice uploads every chunk so runs
    // form naturally; meta / custom slices filter out `upload_*=false` chunks and
    // must track staging adjacency separately.
    let t_cmd = std::time::Instant::now();
    let t_regions = std::time::Instant::now();
    let meta_upload_count = uploads.iter().filter(|u| u.upload_meta).count();
    let custom_upload_count = uploads.iter().filter(|u| u.upload_custom).count();
    let mut block_regions: Vec<BufferImageCopy> = Vec::with_capacity(uploads.len());
    let mut metadata_regions: Vec<BufferImageCopy> = Vec::with_capacity(meta_upload_count);
    let mut custom_regions: Vec<BufferImageCopy> = Vec::with_capacity(custom_upload_count);

    /// Emits merged Z-run regions for a single slice. `included(u)` selects the
    /// chunks that participate. `len(u)` returns the slice byte length per chunk.
    /// Adjacency is detected between *consecutive included* chunks only, so a
    /// skipped chunk breaks the run.
    #[allow(clippy::too_many_arguments)]
    fn build_runs<'a>(
        uploads: &[Upload<'a>],
        subresource: vulkano::image::ImageSubresourceLayers,
        out: &mut Vec<BufferImageCopy>,
        mut included: impl FnMut(&Upload<'a>) -> bool,
        mut len: impl FnMut(&Upload<'a>) -> u64,
    ) {
        let chunk_size = CHUNK_SIZE as u32;
        let mut buffer_offset = 0u64;
        let mut run_start: Option<(u64, [u32; 3], u32)> = None; // (buf_off, offset, run_depth)

        for u in uploads {
            if !included(u) {
                continue;
            }
            let u_offset = u.offset;
            let u_len = len(u);

            let extend_run = match run_start {
                Some((_, start_off, run_depth)) => {
                    // Adjacent in texture iff same (y, x) and z == start_z + run_depth.
                    u_offset[0] == start_off[0]
                        && u_offset[1] == start_off[1]
                        && u_offset[2] == start_off[2] + run_depth
                }
                None => false,
            };

            if extend_run {
                if let Some((_, _, run_depth)) = run_start.as_mut() {
                    *run_depth += chunk_size;
                }
            } else {
                if let Some((buf_off, start_off, run_depth)) = run_start.take() {
                    out.push(BufferImageCopy {
                        buffer_offset: buf_off,
                        buffer_row_length: chunk_size,
                        buffer_image_height: chunk_size,
                        image_subresource: subresource.clone(),
                        image_offset: start_off,
                        image_extent: [chunk_size, chunk_size, run_depth],
                        ..Default::default()
                    });
                }
                run_start = Some((buffer_offset, u_offset, chunk_size));
            }
            buffer_offset += u_len;
        }

        if let Some((buf_off, start_off, run_depth)) = run_start {
            out.push(BufferImageCopy {
                buffer_offset: buf_off,
                buffer_row_length: chunk_size,
                buffer_image_height: chunk_size,
                image_subresource: subresource,
                image_offset: start_off,
                image_extent: [chunk_size, chunk_size, run_depth],
                ..Default::default()
            });
        }
    }

    build_runs(
        &uploads,
        voxel_image.subresource_layers(),
        &mut block_regions,
        |_| true,
        |u| u.block_data.len() as u64,
    );
    build_runs(
        &uploads,
        model_metadata_image.subresource_layers(),
        &mut metadata_regions,
        |u| u.upload_meta,
        |u| u.model_metadata.len() as u64,
    );
    build_runs(
        &uploads,
        block_custom_data_image.subresource_layers(),
        &mut custom_regions,
        |u| u.upload_custom,
        |u| u.custom_data.len() as u64,
    );

    // Build single command buffer with all copies
    // Uses transfer queue (may be same as graphics on unified memory systems)
    //
    // Note: On discrete GPUs with separate transfer queues, this enables parallel
    // DMA transfers over PCIe while the graphics queue is busy rendering.
    // The images use GENERAL layout which allows concurrent access.
    // Explicit queue family ownership transfers are not needed because:
    // 1. VK_SHARING_MODE_EXCLUSIVE with GENERAL layout allows cross-queue access
    // 2. The fence ensures transfer completion before graphics reads the data
    let _ = (graphics_queue_family, separate_transfer_queue); // Suppress unused warnings
    let regions_us = t_regions.elapsed();
    let block_region_count = block_regions.len();
    let meta_region_count = metadata_regions.len();
    let custom_region_count = custom_regions.len();

    let t_builder = std::time::Instant::now();
    let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
        command_buffer_allocator.clone(),
        transfer_queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();
    let builder_us = t_builder.elapsed();

    let t_copy_calls = std::time::Instant::now();
    command_buffer_builder
        .copy_buffer_to_image(CopyBufferToImageInfo {
            regions: block_regions.into(),
            ..CopyBufferToImageInfo::buffer_image(block_staging.clone(), voxel_image.clone())
        })
        .unwrap();

    if !metadata_regions.is_empty() {
        command_buffer_builder
            .copy_buffer_to_image(CopyBufferToImageInfo {
                regions: metadata_regions.into(),
                ..CopyBufferToImageInfo::buffer_image(
                    meta_staging.clone(),
                    model_metadata_image.clone(),
                )
            })
            .unwrap();
    }

    if !custom_regions.is_empty() {
        command_buffer_builder
            .copy_buffer_to_image(CopyBufferToImageInfo {
                regions: custom_regions.into(),
                ..CopyBufferToImageInfo::buffer_image(
                    custom_staging.clone(),
                    block_custom_data_image.clone(),
                )
            })
            .unwrap();
    }
    let copy_calls_us = t_copy_calls.elapsed();

    let t_build = std::time::Instant::now();
    let cb = command_buffer_builder.build().unwrap();
    let build_us = t_build.elapsed();

    // Submit to transfer queue and get fence (non-blocking)
    let t_submit = std::time::Instant::now();
    let fence = cb
        .execute(transfer_queue.clone())
        .unwrap()
        .then_signal_fence_and_flush()
        .unwrap();

    // Submit the transfer to the ring buffer (keeps staging buffers alive until GPU completes)
    TRANSFER_RING.with(|ring| {
        ring.borrow_mut().submit(
            slot_index,
            TransferSlot::new(fence, block_staging, meta_staging, custom_staging),
        );
    });
    let submit_us = t_submit.elapsed();
    let cmd_us = t_cmd.elapsed();

    if profile {
        log::warn!(
            "[UploadProfile] chunks={} regions=b{}+m{}+c{} total={:.2}ms ring={:.2}ms staging={:.2}ms ({}block={}MB meta={}MB/{} custom={}MB/{}) memcpy={:.2}ms cmd={:.2}ms [regions={:.2}ms builder={:.2}ms copy_calls={:.2}ms build={:.2}ms submit={:.2}ms]",
            uploads.len(),
            block_region_count,
            meta_region_count,
            custom_region_count,
            t_total.elapsed().as_secs_f64() * 1000.0,
            ring_us.as_secs_f64() * 1000.0,
            staging_us.as_secs_f64() * 1000.0,
            if staging_allocated { "ALLOC " } else { "" },
            total_block_bytes / (1024 * 1024),
            total_meta_bytes / (1024 * 1024),
            meta_upload_count,
            total_custom_bytes / (1024 * 1024),
            custom_upload_count,
            memcpy_us.as_secs_f64() * 1000.0,
            cmd_us.as_secs_f64() * 1000.0,
            regions_us.as_secs_f64() * 1000.0,
            builder_us.as_secs_f64() * 1000.0,
            copy_calls_us.as_secs_f64() * 1000.0,
            build_us.as_secs_f64() * 1000.0,
            submit_us.as_secs_f64() * 1000.0,
        );
    }

    // Note: We do NOT wait here! The fence is polled on the next upload call.
    // Staging buffers are kept alive in the ring buffer until the transfer completes.

    Ok(())
}

/// Flushes all pending chunk transfers, waiting for GPU completion.
/// Call this before shutdown or when you need to ensure all uploads are done.
#[allow(dead_code)]
pub fn flush_chunk_transfers() {
    TRANSFER_RING.with(|ring| ring.borrow_mut().flush());
}

#[allow(dead_code)]
pub fn update_chunk_metadata(
    world: &mut crate::world::World,
    chunk_metadata_buffer: &Subbuffer<[u32]>,
    texture_origin: Vector3<i32>,
) -> GpuUploadResult {
    CHUNK_META_SCRATCH.with(|scratch| {
        let mut metadata = scratch.borrow_mut();
        metadata.clear();
        metadata.resize(CHUNK_METADATA_WORDS, 0);

        // Iterate over texture-relative chunk positions
        for cy in 0..WORLD_CHUNKS_Y {
            for cz in 0..LOADED_CHUNKS_Z {
                for cx in 0..LOADED_CHUNKS_X {
                    // Convert texture-relative chunk position to world chunk position
                    let world_chunk_x = texture_origin.x / CHUNK_SIZE as i32 + cx;
                    let world_chunk_y = cy;
                    let world_chunk_z = texture_origin.z / CHUNK_SIZE as i32 + cz;
                    let world_chunk_pos = Vector3::new(world_chunk_x, world_chunk_y, world_chunk_z);

                    // Calculate flat chunk index
                    let chunk_idx = cx as usize
                        + cz as usize * LOADED_CHUNKS_X as usize
                        + cy as usize * LOADED_CHUNKS_X as usize * LOADED_CHUNKS_Z as usize;

                    if let Some(chunk) = world.get_chunk_mut(world_chunk_pos) {
                        chunk.update_metadata();
                        if chunk.is_empty() {
                            let word_idx = chunk_idx / 32;
                            let bit_idx = chunk_idx % 32;
                            metadata[word_idx] |= 1u32 << bit_idx;
                        }
                    } else {
                        let word_idx = chunk_idx / 32;
                        let bit_idx = chunk_idx % 32;
                        metadata[word_idx] |= 1u32 << bit_idx;
                    }
                }
            }
        }

        let mut buffer_write = chunk_metadata_buffer.write()?;
        buffer_write.copy_from_slice(&metadata);
        Ok(())
    })
}

#[allow(dead_code)]
pub fn update_brick_metadata(
    world: &crate::world::World,
    brick_mask_buffer: &Subbuffer<[u32]>,
    brick_dist_buffer: &Subbuffer<[u32]>,
    texture_origin: Vector3<i32>,
) -> GpuUploadResult {
    use crate::svt::ChunkSVT;

    BRICK_MASK_SCRATCH.with(|mask_scratch| {
        BRICK_DIST_SCRATCH.with(|dist_scratch| {
            let mut brick_masks = mask_scratch.borrow_mut();
            let mut brick_distances = dist_scratch.borrow_mut();
            brick_masks.clear();
            brick_masks.resize(BRICK_MASK_WORDS, 0);
            brick_distances.clear();
            brick_distances.resize(BRICK_DIST_WORDS, 0xFFFFFFFF);

            for cy in 0..WORLD_CHUNKS_Y {
                for cz in 0..LOADED_CHUNKS_Z {
                    for cx in 0..LOADED_CHUNKS_X {
                        let world_chunk_x = texture_origin.x / CHUNK_SIZE as i32 + cx;
                        let world_chunk_y = cy;
                        let world_chunk_z = texture_origin.z / CHUNK_SIZE as i32 + cz;
                        let world_chunk_pos =
                            Vector3::new(world_chunk_x, world_chunk_y, world_chunk_z);

                        let chunk_idx = cx as usize
                            + cz as usize * LOADED_CHUNKS_X as usize
                            + cy as usize * LOADED_CHUNKS_X as usize * LOADED_CHUNKS_Z as usize;

                        if let Some(chunk) = world.get_chunk(world_chunk_pos) {
                            let svt = ChunkSVT::from_chunk(chunk);

                            let mask_offset = chunk_idx * 2;
                            brick_masks[mask_offset] = svt.brick_mask as u32;
                            brick_masks[mask_offset + 1] = (svt.brick_mask >> 32) as u32;

                            let dist_offset = chunk_idx * 16;
                            for (i, chunk_distances) in svt.brick_distances.chunks(4).enumerate() {
                                let word = (chunk_distances[0] as u32)
                                    | ((chunk_distances[1] as u32) << 8)
                                    | ((chunk_distances[2] as u32) << 16)
                                    | ((chunk_distances[3] as u32) << 24);
                                brick_distances[dist_offset + i] = word;
                            }
                        }
                    }
                }
            }

            {
                let mut mask_write = brick_mask_buffer.write()?;
                mask_write.copy_from_slice(&brick_masks);
            }
            {
                let mut dist_write = brick_dist_buffer.write()?;
                dist_write.copy_from_slice(&brick_distances);
            }
            Ok(())
        })
    })
}

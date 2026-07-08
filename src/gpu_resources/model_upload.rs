//! Brick-mask / SVT and sub-voxel model atlas resources and upload paths.
//!
//! Owns descriptor set 7's brick-and-model backing (brick masks/distances,
//! the three resolution-tier model atlases, palettes, emission, metadata,
//! custom data, and the model properties SSBO) plus the full and incremental
//! model-registry upload routines.

use super::*;

use crate::sub_voxel::{MAX_MODELS, ModelRegistry, PALETTE_SIZE};

pub const BRICK_MASK_WORDS: usize = TOTAL_CHUNKS * 2;
/// Number of u32 words for brick distances (16 words = 64 bytes per chunk).
pub const BRICK_DIST_WORDS: usize = TOTAL_CHUNKS * 16;

/// Creates combined descriptor set 7 containing brick metadata AND model resources.
/// This merges brick metadata with model resources to stay within the 8 descriptor set limit.
///
/// Layout:
/// - Binding 0: Brick masks - 64 bits per chunk (2 u32 words per chunk)
/// - Binding 1: Brick distances - 64 bytes per chunk (distance to nearest solid brick)
/// - Binding 2: Model atlas (8³) - 128×8×128, R8_UINT palette indices
/// - Binding 3: Model atlas (16³) - 256×16×256, R8_UINT palette indices
/// - Binding 4: Model atlas (32³) - 512×32×512, R8_UINT palette indices
/// - Binding 5: Model palettes - 256×32 (256 models × 32 colors), RGBA8
/// - Binding 6: Model metadata - model_id (R) + rotation (G) per block
/// - Binding 7: Model properties - collision mask, emission, flags, resolution per model
/// - Binding 8: Model palette emission - emission intensity per palette entry
/// - Binding 9: Block custom data - per-block custom data (e.g., picture_id for frames)
pub struct BrickAndModelResources {
    pub brick_mask_buffer: Subbuffer<[u32]>,
    pub brick_dist_buffer: Subbuffer<[u32]>,
    pub model_atlas_8: Arc<Image>,
    pub model_atlas_16: Arc<Image>,
    pub model_atlas_32: Arc<Image>,
    pub model_palettes: Arc<Image>,
    pub model_palette_emission: Arc<Image>,
    pub model_metadata: Arc<Image>,
    pub block_custom_data: Arc<Image>,
    pub model_properties_buffer: Subbuffer<[GpuModelProperties]>,
    pub descriptor_set: Arc<DescriptorSet>,
}

pub fn get_brick_and_model_set(
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
    queue: &Arc<Queue>,
    world_extent: [u32; 3],
    model_registry: &ModelRegistry,
) -> BrickAndModelResources {
    // === Brick metadata resources (bindings 0-1) ===

    // Create coherent buffers for brick metadata to eliminate per-frame sync stalls.
    // HOST_COHERENT allows CPU writes to be immediately visible to GPU.
    let brick_mask_buffer =
        make_coherent_storage_buffer::<u32>(&memory_allocator, BRICK_MASK_WORDS as u64);

    // Create coherent buffer for brick distances (64 bytes per chunk)
    let brick_dist_buffer =
        make_coherent_storage_buffer::<u32>(&memory_allocator, BRICK_DIST_WORDS as u64);

    // === Model resources (bindings 2-7) ===

    // Create three tiered model atlas 3D textures (R8_UINT)
    // Tier 0: 8³ resolution (128×8×128)
    let model_atlas_8 = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim3d,
            format: Format::R8_UINT,
            extent: [
                MODEL_ATLAS_8_WIDTH,
                MODEL_ATLAS_8_HEIGHT,
                MODEL_ATLAS_8_DEPTH,
            ],
            mip_levels: 1,
            array_layers: 1,
            usage: ImageUsage::STORAGE | ImageUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        },
    )
    .unwrap();

    // Tier 1: 16³ resolution (256×16×256)
    let model_atlas_16 = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim3d,
            format: Format::R8_UINT,
            extent: [
                MODEL_ATLAS_16_WIDTH,
                MODEL_ATLAS_16_HEIGHT,
                MODEL_ATLAS_16_DEPTH,
            ],
            mip_levels: 1,
            array_layers: 1,
            usage: ImageUsage::STORAGE | ImageUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        },
    )
    .unwrap();

    // Tier 2: 32³ resolution (512×32×512)
    let model_atlas_32 = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim3d,
            format: Format::R8_UINT,
            extent: [
                MODEL_ATLAS_32_WIDTH,
                MODEL_ATLAS_32_HEIGHT,
                MODEL_ATLAS_32_DEPTH,
            ],
            mip_levels: 1,
            array_layers: 1,
            usage: ImageUsage::STORAGE | ImageUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        },
    )
    .unwrap();

    // Create model palette 2D texture (RGBA8, 256×32)
    let model_palettes = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: Format::R8G8B8A8_UNORM,
            extent: [MAX_MODELS as u32, PALETTE_SIZE as u32, 1],
            usage: ImageUsage::SAMPLED | ImageUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo::default(),
    )
    .unwrap();

    // Create model palette emission 2D texture (R8, 256×32)
    let model_palette_emission = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: Format::R8_UNORM,
            extent: [MAX_MODELS as u32, PALETTE_SIZE as u32, 1],
            usage: ImageUsage::SAMPLED | ImageUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo::default(),
    )
    .unwrap();

    // Create model metadata 3D texture (RG8_UINT, same extent as blocks)
    let model_metadata = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim3d,
            format: Format::R8G8_UINT,
            extent: world_extent,
            usage: ImageUsage::STORAGE | ImageUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo::default(),
    )
    .unwrap();

    // Create custom_data 3D texture (R32_UINT, same extent as blocks)
    // Stores per-block custom data (e.g., picture_id, offset_x, offset_y for frames)
    let block_custom_data = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim3d,
            format: Format::R32_UINT,
            extent: world_extent,
            usage: ImageUsage::STORAGE | ImageUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo::default(),
    )
    .unwrap();

    // Create model properties buffer (SSBO)
    let model_properties_buffer = Buffer::new_slice::<GpuModelProperties>(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::STORAGE_BUFFER | BufferUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        MAX_MODELS as u64,
    )
    .unwrap();

    // Upload model registry data to GPU (all three atlas tiers)
    upload_model_registry(
        memory_allocator.clone(),
        command_buffer_allocator.clone(),
        queue,
        model_registry,
        ModelAtlasTargets {
            atlas_8: &model_atlas_8,
            atlas_16: &model_atlas_16,
            atlas_32: &model_atlas_32,
            palettes: &model_palettes,
            palette_emission: &model_palette_emission,
            properties_buffer: &model_properties_buffer,
        },
    );

    // Clear metadata to all zeros (no models placed yet)
    let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
        command_buffer_allocator,
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    command_buffer_builder
        .clear_color_image(ClearColorImageInfo::image(model_metadata.clone()))
        .unwrap();

    command_buffer_builder
        .clear_color_image(ClearColorImageInfo::image(block_custom_data.clone()))
        .unwrap();

    command_buffer_builder
        .build()
        .unwrap()
        .execute(queue.clone())
        .unwrap()
        .then_signal_fence_and_flush()
        .unwrap()
        .wait(None)
        .unwrap();

    // Create image views for all three resolution tiers
    let atlas_8_view = ImageView::new(
        model_atlas_8.clone(),
        ImageViewCreateInfo::from_image(&model_atlas_8),
    )
    .unwrap();

    let atlas_16_view = ImageView::new(
        model_atlas_16.clone(),
        ImageViewCreateInfo::from_image(&model_atlas_16),
    )
    .unwrap();

    let atlas_32_view = ImageView::new(
        model_atlas_32.clone(),
        ImageViewCreateInfo::from_image(&model_atlas_32),
    )
    .unwrap();

    let palette_view = ImageView::new(
        model_palettes.clone(),
        ImageViewCreateInfo::from_image(&model_palettes),
    )
    .unwrap();

    let emission_view = ImageView::new(
        model_palette_emission.clone(),
        ImageViewCreateInfo::from_image(&model_palette_emission),
    )
    .unwrap();

    let metadata_view = ImageView::new(
        model_metadata.clone(),
        ImageViewCreateInfo::from_image(&model_metadata),
    )
    .unwrap();

    let custom_data_view = ImageView::new(
        block_custom_data.clone(),
        ImageViewCreateInfo::from_image(&block_custom_data),
    )
    .unwrap();

    // Create sampler for palette texture
    let palette_sampler = Sampler::new(
        memory_allocator.device().clone(),
        SamplerCreateInfo {
            mag_filter: Filter::Nearest,
            min_filter: Filter::Nearest,
            address_mode: [SamplerAddressMode::ClampToEdge; 3],
            ..Default::default()
        },
    )
    .unwrap();

    // === Create combined descriptor set at set index 7 ===
    let descriptor_set = make_set(
        &descriptor_set_allocator,
        render_pipeline,
        7,
        [
            // Brick metadata (bindings 0-1)
            WriteDescriptorSet::buffer(0, brick_mask_buffer.clone()),
            WriteDescriptorSet::buffer(1, brick_dist_buffer.clone()),
            // Model atlases at native resolutions (bindings 2-4)
            WriteDescriptorSet::image_view(2, atlas_8_view),
            WriteDescriptorSet::image_view(3, atlas_16_view),
            WriteDescriptorSet::image_view(4, atlas_32_view),
            // Model resources (bindings 5-8)
            WriteDescriptorSet::image_view_sampler(
                5,
                palette_view.clone(),
                palette_sampler.clone(),
            ),
            WriteDescriptorSet::image_view(6, metadata_view),
            WriteDescriptorSet::buffer(7, model_properties_buffer.clone()),
            WriteDescriptorSet::image_view_sampler(8, emission_view, palette_sampler.clone()),
            // Per-block custom data (binding 9)
            WriteDescriptorSet::image_view(9, custom_data_view),
        ],
    );

    BrickAndModelResources {
        brick_mask_buffer,
        brick_dist_buffer,
        model_atlas_8,
        model_atlas_16,
        model_atlas_32,
        model_palettes,
        model_palette_emission,
        model_metadata,
        block_custom_data,
        model_properties_buffer,
        descriptor_set,
    }
}

/// GPU-side model properties for sub-voxel rendering.
/// Must match the shader struct layout.
#[derive(Debug, Clone, Copy, Default, BufferContents)]
#[repr(C)]
pub struct GpuModelProperties {
    /// 64-bit collision mask (4×4×4 grid) stored as two u32s.
    pub collision_mask: [u32; 2],
    /// Packed AABB min (x, y, z bytes).
    pub aabb_min: u32,
    /// Packed AABB max (x, y, z bytes).
    pub aabb_max: u32,
    /// Light emission color (RGB) and intensity (A).
    pub emission: [f32; 4],
    /// Flags: bit 0 = rotatable, bit 1-2 = light_blocking, bit 3 = is_light_source, bits 4-7 = light_mode.
    pub flags: u32,
    /// Model resolution (8, 16, or 32).
    pub resolution: u32,
    /// Light radius in blocks.
    pub light_radius: f32,
    /// Light intensity multiplier.
    pub light_intensity: f32,
}

/// Model atlas dimensions for each resolution tier.
/// Each tier holds up to 256 models in a 16×16 grid.
/// Tier 0 (8³): 128×8×128
pub const MODEL_ATLAS_8_WIDTH: u32 = 16 * 8;
pub const MODEL_ATLAS_8_HEIGHT: u32 = 8;
pub const MODEL_ATLAS_8_DEPTH: u32 = 16 * 8;

/// Tier 1 (16³): 256×16×256
pub const MODEL_ATLAS_16_WIDTH: u32 = 16 * 16;
pub const MODEL_ATLAS_16_HEIGHT: u32 = 16;
pub const MODEL_ATLAS_16_DEPTH: u32 = 16 * 16;

/// Tier 2 (32³): 512×32×512
pub const MODEL_ATLAS_32_WIDTH: u32 = 16 * 32;
pub const MODEL_ATLAS_32_HEIGHT: u32 = 32;
pub const MODEL_ATLAS_32_DEPTH: u32 = 16 * 32;

/// Destination GPU images for a model-registry upload.
pub struct ModelAtlasTargets<'a> {
    pub atlas_8: &'a Arc<Image>,
    pub atlas_16: &'a Arc<Image>,
    pub atlas_32: &'a Arc<Image>,
    pub palettes: &'a Arc<Image>,
    pub palette_emission: &'a Arc<Image>,
    pub properties_buffer: &'a Subbuffer<[GpuModelProperties]>,
}

/// Uploads model registry data (atlas, palettes, properties) to GPU.
/// Dispatches to a full or incremental path based on the registry's dirty tracking.
pub fn upload_model_registry(
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    queue: &Arc<Queue>,
    registry: &ModelRegistry,
    targets: ModelAtlasTargets<'_>,
) {
    if registry.needs_full_resync() {
        upload_model_registry_full(
            memory_allocator,
            command_buffer_allocator,
            queue,
            registry,
            targets,
        );
    } else {
        upload_model_registry_incremental(
            memory_allocator,
            command_buffer_allocator,
            queue,
            registry,
            targets,
        );
    }
}

/// Full upload: repacks every atlas, palette, emission, and property entry.
/// Used on first upload and whenever `ModelRegistry::needs_full_resync()` is set.
fn upload_model_registry_full(
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    queue: &Arc<Queue>,
    registry: &ModelRegistry,
    targets: ModelAtlasTargets<'_>,
) {
    let ModelAtlasTargets {
        atlas_8,
        atlas_16,
        atlas_32,
        palettes,
        palette_emission,
        properties_buffer,
    } = targets;
    // Pack models by resolution tier (native resolution, no downsampling)
    let atlas_data_8 = registry.pack_voxels_for_tier(0); // Tier 0: 8³
    let atlas_data_16 = registry.pack_voxels_for_tier(1); // Tier 1: 16³
    let atlas_data_32 = registry.pack_voxels_for_tier(2); // Tier 2: 32³
    let palette_data = registry.pack_palettes_for_gpu();
    let emission_data = registry.pack_palette_emission_for_gpu();
    let properties_data = registry.pack_properties_for_gpu();

    // Reuse host-visible staging buffers
    thread_local! {
        static ATLAS_POOL_8: std::cell::RefCell<Vec<Subbuffer<[u8]>>> = const { std::cell::RefCell::new(Vec::new()) };
        static ATLAS_POOL_16: std::cell::RefCell<Vec<Subbuffer<[u8]>>> = const { std::cell::RefCell::new(Vec::new()) };
        static ATLAS_POOL_32: std::cell::RefCell<Vec<Subbuffer<[u8]>>> = const { std::cell::RefCell::new(Vec::new()) };
        static PALETTE_POOL: std::cell::RefCell<Vec<Subbuffer<[u8]>>> = const { std::cell::RefCell::new(Vec::new()) };
        static EMISSION_POOL: std::cell::RefCell<Vec<Subbuffer<[u8]>>> = const { std::cell::RefCell::new(Vec::new()) };
    }
    const HOST_POOL_MAX_BUFFERS: usize = 4;

    fn take_or_alloc_host(
        pool: &std::cell::RefCell<Vec<Subbuffer<[u8]>>>,
        needed: usize,
        memory_allocator: &Arc<StandardMemoryAllocator>,
    ) -> Subbuffer<[u8]> {
        let idx_opt = {
            let borrow = pool.borrow();
            borrow.iter().position(|b| b.size() as usize >= needed)
        };
        if let Some(idx) = idx_opt {
            return pool.borrow_mut().swap_remove(idx);
        }

        Buffer::new_slice::<u8>(
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
            needed as u64,
        )
        .unwrap()
    }

    // Allocate staging buffers for all three atlas tiers
    let atlas_staging_8 =
        ATLAS_POOL_8.with(|pool| take_or_alloc_host(pool, atlas_data_8.len(), &memory_allocator));
    let atlas_staging_16 =
        ATLAS_POOL_16.with(|pool| take_or_alloc_host(pool, atlas_data_16.len(), &memory_allocator));
    let atlas_staging_32 =
        ATLAS_POOL_32.with(|pool| take_or_alloc_host(pool, atlas_data_32.len(), &memory_allocator));
    let palette_staging =
        PALETTE_POOL.with(|pool| take_or_alloc_host(pool, palette_data.len(), &memory_allocator));
    let emission_staging =
        EMISSION_POOL.with(|pool| take_or_alloc_host(pool, emission_data.len(), &memory_allocator));

    // Write atlas data to staging buffers
    {
        let mut write = atlas_staging_8.write().unwrap();
        write[..atlas_data_8.len()].copy_from_slice(&atlas_data_8);
    }
    {
        let mut write = atlas_staging_16.write().unwrap();
        write[..atlas_data_16.len()].copy_from_slice(&atlas_data_16);
    }
    {
        let mut write = atlas_staging_32.write().unwrap();
        write[..atlas_data_32.len()].copy_from_slice(&atlas_data_32);
    }
    log::debug!(
        "[DEBUG] Uploaded {} bytes (8³) + {} bytes (16³) + {} bytes (32³) of atlas data to GPU",
        atlas_data_8.len(),
        atlas_data_16.len(),
        atlas_data_32.len()
    );

    {
        let mut write = palette_staging.write().unwrap();
        write[..palette_data.len()].copy_from_slice(&palette_data);
    }

    {
        let mut write = emission_staging.write().unwrap();
        write[..emission_data.len()].copy_from_slice(&emission_data);
    }

    // Convert properties data to GpuModelProperties
    let gpu_properties: Vec<GpuModelProperties> = properties_data
        .chunks(48)
        .map(|chunk: &[u8]| {
            let mut props = GpuModelProperties::default();
            if chunk.len() >= 48 {
                // collision_mask (8 bytes)
                props.collision_mask[0] =
                    u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                props.collision_mask[1] =
                    u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);

                // aabb (8 bytes)
                props.aabb_min = u32::from_le_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]);
                props.aabb_max = u32::from_le_bytes([chunk[12], chunk[13], chunk[14], chunk[15]]);

                // emission (16 bytes as 4 floats)
                props.emission[0] =
                    f32::from_le_bytes([chunk[16], chunk[17], chunk[18], chunk[19]]);
                props.emission[1] =
                    f32::from_le_bytes([chunk[20], chunk[21], chunk[22], chunk[23]]);
                props.emission[2] =
                    f32::from_le_bytes([chunk[24], chunk[25], chunk[26], chunk[27]]);
                props.emission[3] =
                    f32::from_le_bytes([chunk[28], chunk[29], chunk[30], chunk[31]]);

                // flags (4 bytes)
                props.flags = u32::from_le_bytes([chunk[32], chunk[33], chunk[34], chunk[35]]);

                // resolution (4 bytes)
                props.resolution = u32::from_le_bytes([chunk[36], chunk[37], chunk[38], chunk[39]]);

                // light_radius (4 bytes)
                props.light_radius =
                    f32::from_le_bytes([chunk[40], chunk[41], chunk[42], chunk[43]]);

                // light_intensity (4 bytes)
                props.light_intensity =
                    f32::from_le_bytes([chunk[44], chunk[45], chunk[46], chunk[47]]);
            }
            props
        })
        .collect();
    // Write properties directly to mapped buffer
    {
        let mut write_guard = properties_buffer.write().unwrap();
        for (i, prop) in gpu_properties.iter().enumerate() {
            if i < write_guard.len() {
                write_guard[i] = *prop;
            }
        }
    }

    // Build command buffer to copy staging data to images
    let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
        command_buffer_allocator,
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    // Copy atlas data for all three resolution tiers
    command_buffer_builder
        .copy_buffer_to_image(CopyBufferToImageInfo {
            regions: [BufferImageCopy {
                image_subresource: atlas_8.subresource_layers(),
                image_extent: atlas_8.extent(),
                ..Default::default()
            }]
            .into(),
            ..CopyBufferToImageInfo::buffer_image(atlas_staging_8.clone(), atlas_8.clone())
        })
        .unwrap();

    command_buffer_builder
        .copy_buffer_to_image(CopyBufferToImageInfo {
            regions: [BufferImageCopy {
                image_subresource: atlas_16.subresource_layers(),
                image_extent: atlas_16.extent(),
                ..Default::default()
            }]
            .into(),
            ..CopyBufferToImageInfo::buffer_image(atlas_staging_16.clone(), atlas_16.clone())
        })
        .unwrap();

    command_buffer_builder
        .copy_buffer_to_image(CopyBufferToImageInfo {
            regions: [BufferImageCopy {
                image_subresource: atlas_32.subresource_layers(),
                image_extent: atlas_32.extent(),
                ..Default::default()
            }]
            .into(),
            ..CopyBufferToImageInfo::buffer_image(atlas_staging_32.clone(), atlas_32.clone())
        })
        .unwrap();

    // Copy palette data
    command_buffer_builder
        .copy_buffer_to_image(CopyBufferToImageInfo {
            regions: [BufferImageCopy {
                image_subresource: palettes.subresource_layers(),
                image_extent: palettes.extent(),
                ..Default::default()
            }]
            .into(),
            ..CopyBufferToImageInfo::buffer_image(palette_staging.clone(), palettes.clone())
        })
        .unwrap();

    // Copy emission data
    command_buffer_builder
        .copy_buffer_to_image(CopyBufferToImageInfo {
            regions: [BufferImageCopy {
                image_subresource: palette_emission.subresource_layers(),
                image_extent: palette_emission.extent(),
                ..Default::default()
            }]
            .into(),
            ..CopyBufferToImageInfo::buffer_image(
                emission_staging.clone(),
                palette_emission.clone(),
            )
        })
        .unwrap();

    command_buffer_builder
        .build()
        .unwrap()
        .execute(queue.clone())
        .unwrap()
        .then_signal_fence_and_flush()
        .unwrap()
        .wait(None)
        .unwrap();

    // Return staging buffers to pools with cap
    ATLAS_POOL_8.with(|pool| {
        let mut p = pool.borrow_mut();
        if p.len() < HOST_POOL_MAX_BUFFERS {
            p.push(atlas_staging_8);
        }
    });
    ATLAS_POOL_16.with(|pool| {
        let mut p = pool.borrow_mut();
        if p.len() < HOST_POOL_MAX_BUFFERS {
            p.push(atlas_staging_16);
        }
    });
    ATLAS_POOL_32.with(|pool| {
        let mut p = pool.borrow_mut();
        if p.len() < HOST_POOL_MAX_BUFFERS {
            p.push(atlas_staging_32);
        }
    });
    PALETTE_POOL.with(|pool| {
        let mut p = pool.borrow_mut();
        if p.len() < HOST_POOL_MAX_BUFFERS {
            p.push(palette_staging);
        }
    });
}

/// Incremental upload: only updates models listed in `registry.dirty_model_ids()`.
///
/// Coalesces all dirty models' voxel/palette/emission bytes into three staging
/// buffers with per-model `BufferImageCopy` regions, then writes property rows
/// directly into the mapped properties buffer. Avoids the 1MB full-atlas repack
/// that the full upload does on every model edit.
fn upload_model_registry_incremental(
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    queue: &Arc<Queue>,
    registry: &ModelRegistry,
    targets: ModelAtlasTargets<'_>,
) {
    let ModelAtlasTargets {
        atlas_8,
        atlas_16,
        atlas_32,
        palettes,
        palette_emission,
        properties_buffer,
    } = targets;

    // Model-keyed dirty set (voxels + properties).
    let mut dirty_ids: Vec<u8> = registry.dirty_model_ids().iter().copied().collect();
    dirty_ids.sort_unstable();

    // Palette-keyed dirty set (palette + emission columns in the shared palette atlas).
    let mut dirty_palette_ids: Vec<u8> = registry.dirty_palette_ids().iter().copied().collect();
    dirty_palette_ids.sort_unstable();

    if dirty_ids.is_empty() && dirty_palette_ids.is_empty() {
        return;
    }

    // Build coalesced staging buffers and per-model copy descriptors.
    struct VoxelCopy {
        tier: usize,
        buffer_offset: u64,
        image_offset: [u32; 3],
        image_extent: [u32; 3],
    }

    let mut voxel_staging_bytes: Vec<u8> = Vec::new();
    let mut voxel_copies: Vec<VoxelCopy> = Vec::with_capacity(dirty_ids.len());

    let mut palette_staging_bytes: Vec<u8> =
        Vec::with_capacity(dirty_palette_ids.len() * PALETTE_SIZE * 4);
    let mut palette_copies: Vec<(u64, u32)> = Vec::with_capacity(dirty_palette_ids.len()); // (buffer_offset, palette_id)

    let mut emission_staging_bytes: Vec<u8> =
        Vec::with_capacity(dirty_palette_ids.len() * PALETTE_SIZE);
    let mut emission_copies: Vec<(u64, u32)> = Vec::with_capacity(dirty_palette_ids.len());

    for &id in &dirty_ids {
        if let Some((image_offset, image_extent, data)) = registry.pack_model_voxel_region(id) {
            let tier = registry.model_tier(id).unwrap_or(1);
            let buffer_offset = voxel_staging_bytes.len() as u64;
            voxel_staging_bytes.extend_from_slice(&data);
            voxel_copies.push(VoxelCopy {
                tier,
                buffer_offset,
                image_offset,
                image_extent,
            });
        }
    }

    for &palette_id in &dirty_palette_ids {
        if let Some(palette) = registry.pack_palette_column(palette_id) {
            palette_copies.push((palette_staging_bytes.len() as u64, palette_id as u32));
            palette_staging_bytes.extend_from_slice(&palette);
        }
        if let Some(emission) = registry.pack_palette_emission_column(palette_id) {
            emission_copies.push((emission_staging_bytes.len() as u64, palette_id as u32));
            emission_staging_bytes.extend_from_slice(&emission);
        }
    }

    // Allocate staging buffers (one per category) sized to the coalesced payload.
    fn make_staging(
        memory_allocator: &Arc<StandardMemoryAllocator>,
        bytes: &[u8],
    ) -> Option<Subbuffer<[u8]>> {
        if bytes.is_empty() {
            return None;
        }
        let buf = Buffer::new_slice::<u8>(
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
            bytes.len() as u64,
        )
        .ok()?;
        {
            let mut w = buf.write().ok()?;
            w[..bytes.len()].copy_from_slice(bytes);
        }
        Some(buf)
    }

    let voxel_staging = make_staging(&memory_allocator, &voxel_staging_bytes);
    let palette_staging = make_staging(&memory_allocator, &palette_staging_bytes);
    let emission_staging = make_staging(&memory_allocator, &emission_staging_bytes);

    // Update properties buffer directly (host-visible, mapped).
    {
        let mut write_guard = properties_buffer.write().unwrap();
        for &id in &dirty_ids {
            if let Some(bytes) = registry.pack_model_properties(id) {
                let idx = id as usize;
                if idx < write_guard.len() {
                    let mut props = GpuModelProperties::default();
                    props.collision_mask[0] =
                        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                    props.collision_mask[1] =
                        u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
                    props.aabb_min = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
                    props.aabb_max =
                        u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
                    props.emission[0] =
                        f32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
                    props.emission[1] =
                        f32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
                    props.emission[2] =
                        f32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
                    props.emission[3] =
                        f32::from_le_bytes([bytes[28], bytes[29], bytes[30], bytes[31]]);
                    props.flags = u32::from_le_bytes([bytes[32], bytes[33], bytes[34], bytes[35]]);
                    props.resolution =
                        u32::from_le_bytes([bytes[36], bytes[37], bytes[38], bytes[39]]);
                    props.light_radius =
                        f32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]);
                    props.light_intensity =
                        f32::from_le_bytes([bytes[44], bytes[45], bytes[46], bytes[47]]);
                    write_guard[idx] = props;
                }
            }
        }
    }

    // If nothing to copy via staging (unlikely — would imply empty models), we're done.
    if voxel_staging.is_none() && palette_staging.is_none() && emission_staging.is_none() {
        return;
    }

    let mut cb = AutoCommandBufferBuilder::primary(
        command_buffer_allocator,
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    // Voxel atlas copies: group by tier (each tier's copies share a dest image).
    if let Some(staging) = voxel_staging.as_ref() {
        for tier in 0..3 {
            let mut regions: Vec<BufferImageCopy> = Vec::new();
            for c in voxel_copies.iter().filter(|c| c.tier == tier) {
                let dest = match tier {
                    0 => atlas_8,
                    1 => atlas_16,
                    _ => atlas_32,
                };
                regions.push(BufferImageCopy {
                    buffer_offset: c.buffer_offset,
                    buffer_row_length: 0,   // tight (equals image_extent width)
                    buffer_image_height: 0, // tight (equals image_extent height)
                    image_subresource: dest.subresource_layers(),
                    image_offset: c.image_offset,
                    image_extent: c.image_extent,
                    ..Default::default()
                });
            }
            if regions.is_empty() {
                continue;
            }
            let dest = match tier {
                0 => atlas_8,
                1 => atlas_16,
                _ => atlas_32,
            };
            cb.copy_buffer_to_image(CopyBufferToImageInfo {
                regions: regions.into(),
                ..CopyBufferToImageInfo::buffer_image(staging.clone(), (*dest).clone())
            })
            .unwrap();
        }
    }

    // Palette copies: 1-wide × 32-tall column per palette slot at x = palette_id.
    if let Some(staging) = palette_staging.as_ref() {
        let regions: Vec<BufferImageCopy> = palette_copies
            .iter()
            .map(|&(buf_off, palette_id)| BufferImageCopy {
                buffer_offset: buf_off,
                buffer_row_length: 0, // tight: row length = extent.width = 1
                buffer_image_height: 0,
                image_subresource: palettes.subresource_layers(),
                image_offset: [palette_id, 0, 0],
                image_extent: [1, PALETTE_SIZE as u32, 1],
                ..Default::default()
            })
            .collect();
        cb.copy_buffer_to_image(CopyBufferToImageInfo {
            regions: regions.into(),
            ..CopyBufferToImageInfo::buffer_image(staging.clone(), (*palettes).clone())
        })
        .unwrap();
    }

    // Emission copies: same layout as palette but R8 single channel.
    if let Some(staging) = emission_staging.as_ref() {
        let regions: Vec<BufferImageCopy> = emission_copies
            .iter()
            .map(|&(buf_off, palette_id)| BufferImageCopy {
                buffer_offset: buf_off,
                buffer_row_length: 0,
                buffer_image_height: 0,
                image_subresource: palette_emission.subresource_layers(),
                image_offset: [palette_id, 0, 0],
                image_extent: [1, PALETTE_SIZE as u32, 1],
                ..Default::default()
            })
            .collect();
        cb.copy_buffer_to_image(CopyBufferToImageInfo {
            regions: regions.into(),
            ..CopyBufferToImageInfo::buffer_image(staging.clone(), (*palette_emission).clone())
        })
        .unwrap();
    }

    cb.build()
        .unwrap()
        .execute(queue.clone())
        .unwrap()
        .then_signal_fence_and_flush()
        .unwrap()
        .wait(None)
        .unwrap();
}

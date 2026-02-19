use egui_winit_vulkano::{Gui, egui};
use nalgebra::{Matrix4, Vector3};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use vulkano::{
    buffer::{Buffer, BufferContents, BufferCreateInfo, BufferUsage, Subbuffer},
    command_buffer::{
        AutoCommandBufferBuilder, BufferImageCopy, ClearColorImageInfo, CommandBufferUsage,
        CopyBufferToImageInfo, PrimaryCommandBufferAbstract,
        allocator::StandardCommandBufferAllocator,
    },
    descriptor_set::{
        DescriptorSet, WriteDescriptorSet, allocator::StandardDescriptorSetAllocator,
    },
    device::{Device, DeviceOwned, Queue},
    format::Format,
    image::{
        Image, ImageCreateInfo, ImageType, ImageUsage,
        sampler::{Filter, Sampler, SamplerAddressMode, SamplerCreateInfo},
        view::{ImageView, ImageViewCreateInfo},
    },
    memory::allocator::{AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator},
    pipeline::{ComputePipeline, Pipeline},
    swapchain::{PresentMode, Surface, Swapchain, SwapchainCreateInfo},
    sync::{
        GpuFuture,
        future::{FenceSignalFuture, NowFuture},
    },
};
use winit::window::{Icon, Window};

use crate::chunk::{BlockType, CHUNK_SIZE};
use crate::constants::{LOADED_CHUNKS_X, LOADED_CHUNKS_Z, WORLD_CHUNKS_Y};
use crate::falling_block::{GpuFallingBlock, MAX_FALLING_BLOCKS};
use crate::particles;
use crate::remote_player::{GpuRemotePlayer, MAX_REMOTE_PLAYERS};
use crate::sub_voxel::{MAX_MODELS, ModelRegistry, PALETTE_SIZE};

/// Helper to allocate a storage buffer with the common flags used across GPU resources.
fn make_storage_buffer<T: BufferContents>(
    memory_allocator: &Arc<StandardMemoryAllocator>,
    len: u64,
) -> Subbuffer<[T]> {
    Buffer::new_slice::<T>(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::STORAGE_BUFFER,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        len,
    )
    .unwrap()
}

/// Helper to allocate a host-local storage buffer for metadata that is updated frequently.
/// Uses PREFER_HOST | HOST_RANDOM_ACCESS for optimal CPU write performance.
/// On unified memory systems (like M4 Max), this eliminates sync stalls when
/// the GPU reads metadata that was just written by the CPU.
fn make_coherent_storage_buffer<T: BufferContents>(
    memory_allocator: &Arc<StandardMemoryAllocator>,
    len: u64,
) -> Subbuffer<[T]> {
    Buffer::new_slice::<T>(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::STORAGE_BUFFER,
            ..Default::default()
        },
        AllocationCreateInfo {
            // PREFER_HOST places the buffer in host-local (system) memory, which is:
            // 1. Optimal for frequent CPU writes (no PCIe transfer overhead)
            // 2. On unified memory systems, still directly accessible by GPU
            // HOST_RANDOM_ACCESS allows efficient per-element updates via HOST_CACHED.
            memory_type_filter: MemoryTypeFilter::PREFER_HOST
                | MemoryTypeFilter::HOST_RANDOM_ACCESS,
            ..Default::default()
        },
        len,
    )
    .unwrap()
}

/// Type alias for the fence future returned by chunk upload commands.
/// CommandBuffer execute -> then_signal_fence_and_flush produces this type.
type ChunkTransferFence =
    FenceSignalFuture<vulkano::command_buffer::CommandBufferExecFuture<NowFuture>>;

/// Type alias for a triple of staging buffers (block data + model metadata + custom data).
type StagingBufferPair = (Subbuffer<[u8]>, Subbuffer<[u8]>, Subbuffer<[u8]>);

/// A slot in the transfer ring buffer holding an in-flight transfer's fence and staging buffers.
pub(crate) struct TransferSlot {
    fence: ChunkTransferFence,
    block_staging: Subbuffer<[u8]>,
    meta_staging: Subbuffer<[u8]>,
    custom_staging: Subbuffer<[u8]>,
}

/// Ring buffer for async GPU chunk uploads.
/// Tracks N in-flight transfers, allowing CPU and GPU to work in parallel.
/// Only blocks when all slots are busy (rare with 3+ slots).
pub struct TransferRingBuffer {
    slots: Vec<Option<TransferSlot>>,
    capacity: usize,
}

impl TransferRingBuffer {
    /// Creates a new ring buffer with the specified number of slots.
    /// More slots = less blocking, but more staging memory usage.
    pub fn new(capacity: usize) -> Self {
        let mut slots = Vec::with_capacity(capacity);
        slots.resize_with(capacity, || None);
        Self { slots, capacity }
    }

    /// Polls all slots and reclaims completed transfers.
    /// Returns staging buffers from completed slots (block, meta) for reuse.
    pub fn poll_completed(&mut self) -> Vec<StagingBufferPair> {
        let mut reclaimed = Vec::new();

        for slot in &mut self.slots {
            if let Some(transfer) = slot.as_ref() {
                // Poll the fence without blocking (is_signaled returns immediately)
                if transfer.fence.is_signaled().unwrap_or(false) {
                    // Transfer completed, reclaim staging buffers
                    let transfer = slot.take().unwrap();
                    reclaimed.push((
                        transfer.block_staging,
                        transfer.meta_staging,
                        transfer.custom_staging,
                    ));
                }
            }
        }

        reclaimed
    }

    /// Finds an empty slot or waits for the oldest transfer to complete.
    /// Returns the slot index and any reclaimed staging buffers.
    pub fn acquire_slot(&mut self) -> (usize, Vec<StagingBufferPair>) {
        // First, poll all slots to reclaim completed transfers
        let mut reclaimed = self.poll_completed();

        // Find first empty slot
        for (i, slot) in self.slots.iter().enumerate() {
            if slot.is_none() {
                return (i, reclaimed);
            }
        }

        // All slots busy - must wait for the oldest (slot 0)
        // Rotate the ring: wait on slot 0, then shift all slots left
        if let Some(transfer) = self.slots[0].take() {
            transfer.fence.wait(None).unwrap();
            reclaimed.push((
                transfer.block_staging,
                transfer.meta_staging,
                transfer.custom_staging,
            ));
        }

        // Shift all slots left
        for i in 1..self.capacity {
            self.slots[i - 1] = self.slots[i].take();
        }

        // Return the last slot (now empty)
        (self.capacity - 1, reclaimed)
    }

    /// Submits a transfer to the specified slot.
    pub fn submit(&mut self, slot_index: usize, transfer: TransferSlot) {
        self.slots[slot_index] = Some(transfer);
    }

    /// Wait for all in-flight transfers to complete.
    /// Call this before destroying the ring buffer.
    #[allow(dead_code)]
    pub fn flush(&mut self) {
        for slot in &mut self.slots {
            if let Some(transfer) = slot.take() {
                transfer.fence.wait(None).unwrap();
            }
        }
    }
}

impl Default for TransferRingBuffer {
    fn default() -> Self {
        // 6 slots provides good CPU/GPU overlap with reasonable memory usage.
        // Each frame can have up to 3 upload calls (completed chunks, unloaded chunks, dirty chunks).
        // With only 3 slots, if any previous transfer is still in-flight, we block.
        // 6 slots gives 2 frames of headroom before blocking occurs.
        Self::new(6)
    }
}

/// Helper to create a descriptor set for a given pipeline set index.
fn make_set(
    descriptor_set_allocator: &Arc<StandardDescriptorSetAllocator>,
    pipeline: &ComputePipeline,
    set_idx: usize,
    writes: impl IntoIterator<Item = WriteDescriptorSet>,
) -> Arc<DescriptorSet> {
    let layout = pipeline
        .layout()
        .set_layouts()
        .get(set_idx)
        .unwrap()
        .clone();
    DescriptorSet::new(descriptor_set_allocator.clone(), layout, writes, []).unwrap()
}

pub struct RenderContext {
    pub window: Arc<Window>,
    pub swapchain: Arc<Swapchain>,
    pub image_views: Vec<Arc<ImageView>>,

    pub render_image: Arc<Image>,
    pub render_set: Arc<DescriptorSet>,
    pub resample_image: Arc<Image>,
    pub resample_set: Arc<DescriptorSet>,

    /// Distance buffer for two-pass beam optimization (1/4 resolution)
    pub distance_image: Arc<Image>,
    pub distance_set: Arc<DescriptorSet>,

    pub gui: Gui,
    /// Texture ID for the atlas in egui.
    pub atlas_texture_id: egui::TextureId,
    /// Optional per-block/model sprite textures loaded from disk.
    pub sprite_icons: SpriteIcons,

    /// Picture atlas for frame pictures.
    #[allow(dead_code)]
    pub picture_atlas: Arc<Image>,
    /// Picture atlas image view for shader access.
    #[allow(dead_code)]
    pub picture_atlas_view: Arc<ImageView>,

    pub recreate_swapchain: bool,
}

/// Sprite icons loaded for blocks and models, kept alive by owning texture handles.
#[derive(Default)]
pub struct SpriteIcons {
    pub block: HashMap<BlockType, egui::TextureId>,
    pub tinted_glass: HashMap<u8, egui::TextureId>, // tint_index -> texture
    pub crystal: HashMap<u8, egui::TextureId>,      // tint_index -> texture for Crystal blocks
    pub model: HashMap<u8, egui::TextureId>,
    pub missing: egui::TextureId,
    handles: Vec<egui::TextureHandle>,
}

impl SpriteIcons {
    /// Reloads or adds a single model sprite from the given path.
    /// Returns true if the sprite was successfully loaded.
    pub fn reload_model_sprite(&mut self, ctx: &egui::Context, model_id: u8, path: &Path) -> bool {
        if let Some(image) = load_color_image(path) {
            let handle = ctx.load_texture(
                format!("sprite_model_{}", model_id),
                image,
                egui::TextureOptions::NEAREST,
            );
            self.model.insert(model_id, handle.id());
            self.handles.push(handle);
            true
        } else {
            false
        }
    }
}

fn load_color_image(path: &Path) -> Option<egui::ColorImage> {
    let img = image::open(path).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [w as usize, h as usize],
        img.as_raw(),
    ))
}

pub fn load_sprite_icons(gui: &mut Gui) -> SpriteIcons {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("textures").join("rendered");
    let ctx = gui.context();

    let mut icons = SpriteIcons::default();

    // Missing placeholder (required)
    let missing_handle = load_color_image(&dir.join("missing.png"))
        .map(|image| ctx.load_texture("sprite_missing", image, egui::TextureOptions::NEAREST));
    if let Some(handle) = missing_handle {
        icons.missing = handle.id();
        icons.handles.push(handle);
    } else {
        let image = egui::ColorImage::from_rgba_unmultiplied([1, 1], &[255, 0, 255, 255]);
        let handle = ctx.load_texture(
            "sprite_missing_fallback",
            image,
            egui::TextureOptions::NEAREST,
        );
        icons.missing = handle.id();
        icons.handles.push(handle);
    }

    const BLOCK_FILES: &[(BlockType, &str)] = &[
        (BlockType::Stone, "block_stone.png"),
        (BlockType::Dirt, "block_dirt.png"),
        (BlockType::Grass, "block_grass.png"),
        (BlockType::Planks, "block_planks.png"),
        (BlockType::Leaves, "block_leaves.png"),
        (BlockType::Sand, "block_sand.png"),
        (BlockType::Gravel, "block_gravel.png"),
        (BlockType::Water, "block_water.png"),
        (BlockType::Glass, "block_glass.png"),
        // TintedGlass is loaded separately per tint color
        (BlockType::Log, "block_log.png"),
        (BlockType::Brick, "block_brick.png"),
        (BlockType::Snow, "block_snow.png"),
        (BlockType::Ice, "block_ice.png"),
        (BlockType::Cobblestone, "block_cobblestone.png"),
        (BlockType::Iron, "block_iron.png"),
        (BlockType::Bedrock, "block_bedrock.png"),
        // Emissive blocks
        (BlockType::Lava, "block_lava.png"),
        (BlockType::GlowStone, "block_glowstone.png"),
        (BlockType::GlowMushroom, "block_glowmushroom.png"),
        // Tree variants
        (BlockType::PineLog, "block_pinelog.png"),
        (BlockType::WillowLog, "block_willowlog.png"),
        (BlockType::BirchLog, "block_birchlog.png"),
        (BlockType::PineLeaves, "block_pineleaves.png"),
        (BlockType::WillowLeaves, "block_willowleaves.png"),
        (BlockType::BirchLeaves, "block_birchleaves.png"),
        // Terrain blocks
        (BlockType::Mud, "block_mud.png"),
        (BlockType::Sandstone, "block_sandstone.png"),
        (BlockType::Cactus, "block_cactus.png"),
        (BlockType::DecorativeStone, "block_decorativestone.png"),
        (BlockType::Concrete, "block_concrete.png"),
        // Crystal is loaded separately per tint color (like TintedGlass)
        // Cave/biome blocks
        (BlockType::Deepslate, "block_deepslate.png"),
        (BlockType::Moss, "block_moss.png"),
        (BlockType::MossyCobblestone, "block_mossycobblestone.png"),
        (BlockType::Clay, "block_clay.png"),
        (BlockType::Dripstone, "block_dripstone.png"),
        (BlockType::Calcite, "block_calcite.png"),
        (BlockType::Terracotta, "block_terracotta.png"),
        (BlockType::PackedIce, "block_packedice.png"),
        (BlockType::Podzol, "block_podzol.png"),
        (BlockType::Mycelium, "block_mycelium.png"),
        (BlockType::CoarseDirt, "block_coarsedirt.png"),
        (BlockType::RootedDirt, "block_rooteddirt.png"),
    ];

    // Tint indices used in the palette (from hud_render.rs TINTED_GLASS_COLORS)
    const TINTED_GLASS_INDICES: [u8; 7] = [0, 1, 2, 4, 6, 8, 9];
    // Crystal tint indices (from hud_render.rs CRYSTAL_COLORS)
    const CRYSTAL_INDICES: [u8; 8] = [0, 1, 2, 4, 6, 8, 9, 12];

    for (block, filename) in BLOCK_FILES {
        let path = dir.join(filename);
        if let Some(image) = load_color_image(&path) {
            let handle = ctx.load_texture(
                format!("sprite_block_{}", filename),
                image,
                egui::TextureOptions::NEAREST,
            );
            icons.block.insert(*block, handle.id());
            icons.handles.push(handle);
        }
    }

    // Load tinted glass sprites
    for tint_idx in TINTED_GLASS_INDICES {
        let filename = format!("block_tintedglass_{}.png", tint_idx);
        let path = dir.join(&filename);
        if let Some(image) = load_color_image(&path) {
            let handle = ctx.load_texture(
                format!("sprite_tintedglass_{}", tint_idx),
                image,
                egui::TextureOptions::NEAREST,
            );
            icons.tinted_glass.insert(tint_idx, handle.id());
            icons.handles.push(handle);
        }
    }

    // Load crystal sprites
    for tint_idx in CRYSTAL_INDICES {
        let filename = format!("block_crystal_{}.png", tint_idx);
        let path = dir.join(&filename);
        if let Some(image) = load_color_image(&path) {
            let handle = ctx.load_texture(
                format!("sprite_crystal_{}", tint_idx),
                image,
                egui::TextureOptions::NEAREST,
            );
            icons.crystal.insert(tint_idx, handle.id());
            icons.handles.push(handle);
        }
    }

    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with("model_") || !name.ends_with(".png") {
                continue;
            }
            if let Ok(id) = name
                .trim_start_matches("model_")
                .trim_end_matches(".png")
                .parse::<u8>()
            {
                let path = entry.path();
                if let Some(image) = load_color_image(&path) {
                    let handle = ctx.load_texture(
                        format!("sprite_model_{}", id),
                        image,
                        egui::TextureOptions::NEAREST,
                    );
                    icons.model.insert(id, handle.id());
                    icons.handles.push(handle);
                }
            }
        }
    }

    icons
}

#[derive(BufferContents, Clone, Copy)]
#[repr(C)]
pub struct PushConstants {
    pub pixel_to_ray: Matrix4<f32>,
    pub texture_size_x: u32,
    pub texture_size_y: u32,
    pub texture_size_z: u32,
    pub render_mode: u32,
    pub show_chunk_boundaries: u32,
    pub player_in_water: u32,
    pub time_of_day: f32,
    pub animation_time: f32,
    pub cloud_speed: f32,
    pub cloud_coverage: f32,
    pub cloud_color_r: f32,
    pub cloud_color_g: f32,
    pub cloud_color_b: f32,
    pub clouds_enabled: u32,
    pub break_block_x: i32,
    pub break_block_y: i32,
    pub break_block_z: i32,
    pub break_progress: f32,
    pub particle_count: u32,
    pub preview_block_x: i32,
    pub preview_block_y: i32,
    pub preview_block_z: i32,
    pub preview_block_type: u32,
    pub light_count: u32,
    pub ambient_light: f32,
    pub fog_density: f32,
    pub fog_start: f32,
    pub fog_overlay_scale: f32,
    pub target_block_x: i32,
    pub target_block_y: i32,
    pub target_block_z: i32,
    pub max_ray_steps: u32,
    pub shadow_max_steps: u32,
    pub texture_origin_x: i32,
    pub texture_origin_y: i32,
    pub texture_origin_z: i32,
    pub enable_ao: u32,
    pub enable_shadows: u32,
    pub enable_model_shadows: u32,
    pub enable_point_lights: u32,
    pub enable_tinted_shadows: u32,
    pub transparent_background: u32,
    pub pass_mode: u32,
    pub lod_ao_distance: f32,
    pub lod_shadow_distance: f32,
    pub lod_point_light_distance: f32,
    pub lod_model_distance: f32,
    pub falling_block_count: u32,
    pub show_water_sources: u32,
    pub water_source_count: u32,
    pub template_block_count: u32,
    pub template_preview_min_x: i32,
    pub template_preview_min_y: i32,
    pub template_preview_min_z: i32,
    pub template_preview_max_x: i32,
    pub template_preview_max_y: i32,
    pub template_preview_max_z: i32,
    pub _padding: [u8; 12],   // GLSL aligns vec4 to 16 bytes
    pub camera_pos: [f32; 4], // vec4 in GLSL requires 16-byte alignment
    pub selection_pos1_x: i32,
    pub selection_pos1_y: i32,
    pub selection_pos1_z: i32,
    pub selection_pos2_x: i32,
    pub selection_pos2_y: i32,
    pub selection_pos2_z: i32,
    pub hide_ground_cover: u32,
    pub cutaway_enabled: u32,
    pub cutaway_chunk_x: i32,
    pub cutaway_chunk_y: i32,
    pub cutaway_chunk_z: i32,
    pub cutaway_player_chunk_x: i32,
    pub cutaway_player_chunk_z: i32,
    // Measurement markers (up to 4 positions)
    pub measurement_marker_count: u32,
    pub measurement_marker_0_x: i32,
    pub measurement_marker_0_y: i32,
    pub measurement_marker_0_z: i32,
    pub measurement_marker_1_x: i32,
    pub measurement_marker_1_y: i32,
    pub measurement_marker_1_z: i32,
    pub measurement_marker_2_x: i32,
    pub measurement_marker_2_y: i32,
    pub measurement_marker_2_z: i32,
    pub measurement_marker_3_x: i32,
    pub measurement_marker_3_y: i32,
    pub measurement_marker_3_z: i32,
    // Stencil rendering
    pub stencil_block_count: u32,
    pub stencil_opacity: f32,
    pub stencil_render_mode: u32,
    // Measurement laser color
    pub laser_color_r: f32,
    pub laser_color_g: f32,
    pub laser_color_b: f32,
    // Sky colors (day)
    pub sky_zenith_r: f32,
    pub sky_zenith_g: f32,
    pub sky_zenith_b: f32,
    pub sky_horizon_r: f32,
    pub sky_horizon_g: f32,
    pub sky_horizon_b: f32,
    // Picture frame rendering
    pub selected_picture_id: u32,
    // Remote player rendering
    pub remote_player_count: u32,
    // Custom texture count for multiplayer
    pub custom_texture_count: u32,
}

pub fn get_swapchain_images(
    device: &Arc<Device>,
    surface: &Arc<Surface>,
    window: &Window,
) -> (Arc<Swapchain>, Vec<Arc<Image>>) {
    let caps = device
        .physical_device()
        .surface_capabilities(surface, Default::default())
        .unwrap();

    let image_format = device
        .physical_device()
        .surface_formats(surface, Default::default())
        .unwrap()[0]
        .0;

    let composite_alpha = caps.supported_composite_alpha.into_iter().next().unwrap();

    Swapchain::new(
        device.clone(),
        surface.clone(),
        SwapchainCreateInfo {
            min_image_count: caps.min_image_count.max(3),
            image_format,
            image_extent: window.inner_size().into(),
            image_usage: ImageUsage::COLOR_ATTACHMENT
                | ImageUsage::TRANSFER_DST
                | ImageUsage::TRANSFER_SRC,
            composite_alpha,
            present_mode: PresentMode::Immediate,
            ..Default::default()
        },
    )
    .unwrap()
}

pub fn get_render_image(
    memory_allocator: Arc<StandardMemoryAllocator>,
    extent: [u32; 2],
) -> (Arc<Image>, Arc<ImageView>) {
    let image = Image::new(
        memory_allocator,
        ImageCreateInfo {
            usage: ImageUsage::STORAGE | ImageUsage::TRANSFER_DST | ImageUsage::TRANSFER_SRC,
            format: Format::R8G8B8A8_UNORM,
            extent: [extent[0], extent[1], 1],
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        },
    )
    .unwrap();

    let image_view =
        ImageView::new(image.clone(), ImageViewCreateInfo::from_image(&image)).unwrap();

    (image, image_view)
}

pub fn get_resample_image(
    memory_allocator: Arc<StandardMemoryAllocator>,
    extent: [u32; 2],
) -> (Arc<Image>, Arc<ImageView>) {
    let image = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            usage: ImageUsage::STORAGE | ImageUsage::TRANSFER_SRC,
            format: Format::R8G8B8A8_UNORM,
            extent: [extent[0], extent[1], 1],
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        },
    )
    .unwrap();

    let image_view =
        ImageView::new(image.clone(), ImageViewCreateInfo::from_image(&image)).unwrap();

    (image, image_view)
}

pub fn get_images_and_sets(
    memory_allocator: Arc<StandardMemoryAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
    resample_pipeline: &ComputePipeline,
    render_extent: [u32; 2],
    window_extent: [u32; 2],
    multiplayer_texture_array: Option<(Arc<ImageView>, Arc<Sampler>, u32)>,
) -> (
    Arc<Image>,
    Arc<DescriptorSet>,
    Arc<Image>,
    Arc<DescriptorSet>,
) {
    let (render_image, render_image_view) =
        get_render_image(memory_allocator.clone(), render_extent);

    // Create render set with optional multiplayer texture array at binding 10
    let render_set = if let Some((texture_view, sampler, _count)) = multiplayer_texture_array {
        make_set(
            &descriptor_set_allocator,
            render_pipeline,
            0,
            [
                WriteDescriptorSet::image_view(0, render_image_view.clone()),
                WriteDescriptorSet::image_view_sampler(10, texture_view, sampler),
            ],
        )
    } else {
        make_set(
            &descriptor_set_allocator,
            render_pipeline,
            0,
            [WriteDescriptorSet::image_view(0, render_image_view.clone())],
        )
    };

    let (resample_image, resample_image_view) = get_resample_image(memory_allocator, window_extent);

    let resample_set = make_set(
        &descriptor_set_allocator,
        resample_pipeline,
        0,
        [
            WriteDescriptorSet::image_view(0, render_image_view.clone()),
            WriteDescriptorSet::image_view(1, resample_image_view.clone()),
        ],
    );

    (render_image, render_set, resample_image, resample_set)
}

/// Creates a distance buffer for two-pass beam optimization.
/// The distance buffer is at 1/4 of render resolution and stores hit distances.
pub fn get_distance_image_and_set(
    memory_allocator: Arc<StandardMemoryAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
    render_extent: [u32; 2],
) -> (Arc<Image>, Arc<DescriptorSet>) {
    // Distance buffer at 1/4 resolution (1/16 the pixels)
    let distance_extent = [(render_extent[0] / 4).max(1), (render_extent[1] / 4).max(1)];

    let distance_image = Image::new(
        memory_allocator,
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            usage: ImageUsage::STORAGE | ImageUsage::TRANSFER_DST,
            format: Format::R32_SFLOAT,
            extent: [distance_extent[0], distance_extent[1], 1],
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        },
    )
    .unwrap();

    let distance_image_view = ImageView::new(
        distance_image.clone(),
        ImageViewCreateInfo::from_image(&distance_image),
    )
    .unwrap();

    let distance_set = make_set(
        &descriptor_set_allocator,
        render_pipeline,
        6,
        [WriteDescriptorSet::image_view(0, distance_image_view)],
    );

    (distance_image, distance_set)
}

pub fn create_empty_voxel_texture(
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
    queue: &Arc<Queue>,
    world_extent: [u32; 3],
) -> (Arc<DescriptorSet>, Arc<Image>) {
    // Create 3D texture sized to fit entire world
    let image = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim3d,
            format: Format::R8_UINT,
            extent: world_extent,
            usage: ImageUsage::STORAGE | ImageUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo::default(),
    )
    .unwrap();

    // Clear the image to all zeros (air)
    let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
        command_buffer_allocator.clone(),
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    command_buffer_builder
        .clear_color_image(ClearColorImageInfo::image(image.clone()))
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

    let image_view =
        ImageView::new(image.clone(), ImageViewCreateInfo::from_image(&image)).unwrap();

    let descriptor_set = make_set(
        &descriptor_set_allocator,
        render_pipeline,
        1,
        [WriteDescriptorSet::image_view(0, image_view)],
    );

    (descriptor_set, image)
}

pub fn load_icon(icon: &[u8]) -> Icon {
    let (icon_rgba, icon_width, icon_height) = {
        let image = image::load_from_memory(icon).unwrap().to_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };
    Icon::from_rgba(icon_rgba, icon_width, icon_height).unwrap()
}

/// Load a texture atlas from a file and create a GPU texture with sampler.
/// Returns (descriptor_set, sampler, image_view) for binding to the shader and egui.
#[allow(dead_code)]
pub fn load_texture_atlas(
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
    queue: &Arc<Queue>,
    texture_path: &std::path::Path,
) -> (Arc<DescriptorSet>, Arc<Sampler>, Arc<ImageView>) {
    // Load the image file
    let img = image::open(texture_path)
        .expect("Failed to load texture")
        .to_rgba8();
    let (width, height) = img.dimensions();
    let image_data: Vec<u8> = img.into_raw();

    println!(
        "Loaded texture: {}x{} from {:?}",
        width, height, texture_path
    );

    // Create the GPU image
    let image = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: Format::R8G8B8A8_UNORM,
            extent: [width, height, 1],
            usage: ImageUsage::SAMPLED | ImageUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo::default(),
    )
    .unwrap();

    // Upload image data
    let src_buffer = Buffer::from_iter(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::TRANSFER_SRC,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        image_data,
    )
    .unwrap();

    let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
        command_buffer_allocator.clone(),
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    command_buffer_builder
        .copy_buffer_to_image(CopyBufferToImageInfo::buffer_image(
            src_buffer,
            image.clone(),
        ))
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

    let image_view =
        ImageView::new(image.clone(), ImageViewCreateInfo::from_image(&image)).unwrap();

    // Create sampler with nearest-neighbor filtering for pixel art
    let sampler = Sampler::new(
        memory_allocator.device().clone(),
        SamplerCreateInfo {
            mag_filter: Filter::Nearest,
            min_filter: Filter::Nearest,
            address_mode: [SamplerAddressMode::Repeat; 3],
            ..Default::default()
        },
    )
    .unwrap();

    let descriptor_set = make_set(
        &descriptor_set_allocator,
        render_pipeline,
        2,
        [WriteDescriptorSet::image_view_sampler(
            0,
            image_view.clone(),
            sampler.clone(),
        )],
    );

    (descriptor_set, sampler, image_view)
}

/// Custom texture atlas dimensions (16 slots × 64×64 pixels each)
pub const CUSTOM_TEXTURE_SLOTS: u32 = 16;
pub const CUSTOM_TEXTURE_SIZE: u32 = 64;
pub const CUSTOM_ATLAS_WIDTH: u32 = CUSTOM_TEXTURE_SLOTS * CUSTOM_TEXTURE_SIZE; // 1024
pub const CUSTOM_ATLAS_HEIGHT: u32 = CUSTOM_TEXTURE_SIZE; // 64

// Picture atlas for frame pictures
pub const PICTURE_ATLAS_SLOTS: u32 = 64;
pub const PICTURE_ATLAS_SIZE: u32 = 128; // Each picture slot is 128×128 pixels
pub const PICTURE_ATLAS_WIDTH: u32 = PICTURE_ATLAS_SLOTS * PICTURE_ATLAS_SIZE; // 8192
pub const PICTURE_ATLAS_HEIGHT: u32 = PICTURE_ATLAS_SIZE; // 128

/// Multiplayer custom texture array dimensions
/// Each texture is 64×64 RGBA, with up to 32 slots by default
pub const MULTIPLAYER_TEXTURE_SIZE: u32 = 64;
pub const MULTIPLAYER_TEXTURE_DEFAULT_SLOTS: u32 = 32;

/// Create the multiplayer custom texture array (2DArray).
/// Returns (image, image_view, sampler) for use in descriptor sets.
pub fn create_multiplayer_texture_array(
    memory_allocator: Arc<StandardMemoryAllocator>,
    max_slots: u32,
) -> (
    Arc<Image>,
    Arc<ImageView>,
    Arc<Sampler>,
) {
    let extent = [MULTIPLAYER_TEXTURE_SIZE, MULTIPLAYER_TEXTURE_SIZE, 1];
    let array_layers = max_slots;

    let image = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: Format::R8G8B8A8_UNORM,
            extent,
            array_layers,
            usage: ImageUsage::SAMPLED | ImageUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo::default(),
    )
    .unwrap();

    // Create image view for the array
    let image_view = ImageView::new(
        image.clone(),
        ImageViewCreateInfo {
            ..ImageViewCreateInfo::from_image(&image)
        },
    )
    .unwrap();

    // Create sampler with nearest-neighbor filtering for pixel art
    let sampler = Sampler::new(
        memory_allocator.device().clone(),
        SamplerCreateInfo {
            mag_filter: Filter::Nearest,
            min_filter: Filter::Nearest,
            address_mode: [SamplerAddressMode::Repeat; 3],
            ..Default::default()
        },
    )
    .unwrap();

    println!(
        "Created multiplayer texture array: {}x{}x{} slots",
        MULTIPLAYER_TEXTURE_SIZE, MULTIPLAYER_TEXTURE_SIZE, max_slots
    );

    (image, image_view, sampler)
}

/// Update a slot in the multiplayer texture array with new PNG data.
/// Decodes PNG and uploads to the GPU at the specified array layer.
pub fn update_multiplayer_texture_slot(
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    queue: &Arc<Queue>,
    texture_array: &Arc<Image>,
    slot: u32,
    png_data: &[u8],
) -> Result<(), String> {
    // Decode PNG
    let decoder = png::Decoder::new(std::io::Cursor::new(png_data));
    let mut reader = decoder.read_info().map_err(|e| format!("Invalid PNG: {}", e))?;

    if reader.info().width != MULTIPLAYER_TEXTURE_SIZE
        || reader.info().height != MULTIPLAYER_TEXTURE_SIZE
    {
        return Err(format!(
            "Texture must be {}x{}, got {}x{}",
            MULTIPLAYER_TEXTURE_SIZE,
            MULTIPLAYER_TEXTURE_SIZE,
            reader.info().width,
            reader.info().height
        ));
    }

    let output_buffer_size = reader.output_buffer_size().unwrap_or(0);
    let mut buf = vec![0u8; output_buffer_size];
    reader.next_frame(&mut buf).map_err(|e| format!("Failed to decode PNG: {}", e))?;

    // Create staging buffer
    let src_buffer = Buffer::from_iter(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::TRANSFER_SRC,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        buf,
    )
    .unwrap();

    // Copy to the specific array layer using BufferImageCopy
    let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
        command_buffer_allocator.clone(),
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    // Get subresource layers for this specific array layer
    let subresource = vulkano::image::ImageSubresourceLayers {
        aspects: vulkano::image::ImageAspects::COLOR,
        mip_level: 0,
        array_layers: slot..slot + 1,
    };

    command_buffer_builder
        .copy_buffer_to_image(CopyBufferToImageInfo {
            regions: vec![BufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: MULTIPLAYER_TEXTURE_SIZE as u32,
                buffer_image_height: MULTIPLAYER_TEXTURE_SIZE as u32,
                image_subresource: subresource,
                image_offset: [0, 0, 0],
                image_extent: [MULTIPLAYER_TEXTURE_SIZE, MULTIPLAYER_TEXTURE_SIZE, 1],
                ..Default::default()
            }]
            .into(),
            ..CopyBufferToImageInfo::buffer_image(src_buffer, texture_array.clone())
        })
        .map_err(|e| format!("Failed to copy to image: {}", e))?;

    command_buffer_builder
        .build()
        .unwrap()
        .execute(queue.clone())
        .unwrap()
        .then_signal_fence_and_flush()
        .unwrap()
        .wait(None)
        .unwrap();

    Ok(())
}

/// Load texture atlases (main, custom, and picture) and create a combined descriptor set.
/// Returns (descriptor_set, sampler, main_image_view, custom_image_view, custom_image, picture_image_view, picture_image)
/// The custom_image and picture_image are returned so they can be updated dynamically.
#[allow(clippy::type_complexity)]
pub fn load_texture_atlases(
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
    queue: &Arc<Queue>,
    texture_path: &std::path::Path,
) -> (
    Arc<DescriptorSet>,
    Arc<Sampler>,
    Arc<ImageView>,
    Arc<ImageView>,
    Arc<Image>,
    Arc<ImageView>,
    Arc<Image>,
) {
    // Load the main texture atlas
    let img = image::open(texture_path)
        .expect("Failed to load texture")
        .to_rgba8();
    let (width, height) = img.dimensions();
    let image_data: Vec<u8> = img.into_raw();

    println!(
        "Loaded main texture atlas: {}x{} from {:?}",
        width, height, texture_path
    );

    // Create the main GPU image
    let main_image = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: Format::R8G8B8A8_UNORM,
            extent: [width, height, 1],
            usage: ImageUsage::SAMPLED | ImageUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo::default(),
    )
    .unwrap();

    // Upload main atlas data
    let src_buffer = Buffer::from_iter(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::TRANSFER_SRC,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        image_data,
    )
    .unwrap();

    // Create the custom texture atlas (initially empty/gray)
    let custom_image = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: Format::R8G8B8A8_UNORM,
            extent: [CUSTOM_ATLAS_WIDTH, CUSTOM_ATLAS_HEIGHT, 1],
            usage: ImageUsage::SAMPLED | ImageUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo::default(),
    )
    .unwrap();

    // Initialize custom atlas with a default pattern (gray checkerboard)
    let custom_data: Vec<u8> = (0..CUSTOM_ATLAS_WIDTH * CUSTOM_ATLAS_HEIGHT)
        .flat_map(|i| {
            let x = i % CUSTOM_ATLAS_WIDTH;
            let y = i / CUSTOM_ATLAS_WIDTH;
            let checker = ((x / 8) + (y / 8)) % 2;
            if checker == 0 {
                [64u8, 64, 64, 255] // Dark gray
            } else {
                [96u8, 96, 96, 255] // Light gray
            }
        })
        .collect();

    let custom_src_buffer = Buffer::from_iter(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::TRANSFER_SRC,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        custom_data,
    )
    .unwrap();

    println!(
        "Created custom texture atlas: {}x{}",
        CUSTOM_ATLAS_WIDTH, CUSTOM_ATLAS_HEIGHT
    );

    // Create the picture atlas (initially white/transparent)
    let picture_image = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            format: Format::R8G8B8A8_UNORM,
            extent: [PICTURE_ATLAS_WIDTH, PICTURE_ATLAS_HEIGHT, 1],
            usage: ImageUsage::SAMPLED | ImageUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo::default(),
    )
    .unwrap();

    // Initialize picture atlas with white (empty pictures)
    let picture_data: Vec<u8> = (0..PICTURE_ATLAS_WIDTH * PICTURE_ATLAS_HEIGHT)
        .flat_map(|_| [255u8, 255, 255, 255]) // White
        .collect();

    let picture_src_buffer = Buffer::from_iter(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::TRANSFER_SRC,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        picture_data,
    )
    .unwrap();

    println!(
        "Created picture atlas: {}x{} ({} slots)",
        PICTURE_ATLAS_WIDTH, PICTURE_ATLAS_HEIGHT, PICTURE_ATLAS_SLOTS
    );

    // Upload all three atlases
    let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
        command_buffer_allocator.clone(),
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    command_buffer_builder
        .copy_buffer_to_image(CopyBufferToImageInfo::buffer_image(
            src_buffer,
            main_image.clone(),
        ))
        .unwrap()
        .copy_buffer_to_image(CopyBufferToImageInfo::buffer_image(
            custom_src_buffer,
            custom_image.clone(),
        ))
        .unwrap()
        .copy_buffer_to_image(CopyBufferToImageInfo::buffer_image(
            picture_src_buffer,
            picture_image.clone(),
        ))
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

    let main_image_view = ImageView::new(
        main_image.clone(),
        ImageViewCreateInfo::from_image(&main_image),
    )
    .unwrap();
    let custom_image_view = ImageView::new(
        custom_image.clone(),
        ImageViewCreateInfo::from_image(&custom_image),
    )
    .unwrap();
    let picture_image_view = ImageView::new(
        picture_image.clone(),
        ImageViewCreateInfo::from_image(&picture_image),
    )
    .unwrap();

    // Create sampler with nearest-neighbor filtering for pixel art
    let sampler = Sampler::new(
        memory_allocator.device().clone(),
        SamplerCreateInfo {
            mag_filter: Filter::Nearest,
            min_filter: Filter::Nearest,
            address_mode: [SamplerAddressMode::Repeat; 3],
            ..Default::default()
        },
    )
    .unwrap();

    // Create descriptor set with all three atlases
    let descriptor_set = make_set(
        &descriptor_set_allocator,
        render_pipeline,
        2,
        [
            WriteDescriptorSet::image_view_sampler(0, main_image_view.clone(), sampler.clone()),
            WriteDescriptorSet::image_view_sampler(1, custom_image_view.clone(), sampler.clone()),
            WriteDescriptorSet::image_view_sampler(2, picture_image_view.clone(), sampler.clone()),
        ],
    );

    (
        descriptor_set,
        sampler,
        main_image_view,
        custom_image_view,
        custom_image,
        picture_image_view,
        picture_image,
    )
}

/// Update a slot in the custom texture atlas with new pixel data.
/// slot: 0-15, pixels: 64×64 RGBA data (16384 bytes)
pub fn update_custom_texture_slot(
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    queue: &Arc<Queue>,
    custom_image: &Arc<Image>,
    slot: u32,
    pixels: &[u8],
) {
    assert!(slot < CUSTOM_TEXTURE_SLOTS, "Invalid custom texture slot");
    assert_eq!(
        pixels.len(),
        (CUSTOM_TEXTURE_SIZE * CUSTOM_TEXTURE_SIZE * 4) as usize,
        "Invalid pixel data size"
    );

    let src_buffer = Buffer::from_iter(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::TRANSFER_SRC,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        pixels.to_vec(),
    )
    .unwrap();

    let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
        command_buffer_allocator.clone(),
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    // Copy to the specific slot region in the atlas
    command_buffer_builder
        .copy_buffer_to_image(CopyBufferToImageInfo {
            regions: vec![BufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: CUSTOM_TEXTURE_SIZE,
                buffer_image_height: CUSTOM_TEXTURE_SIZE,
                image_subresource: custom_image.subresource_layers(),
                image_offset: [slot * CUSTOM_TEXTURE_SIZE, 0, 0],
                image_extent: [CUSTOM_TEXTURE_SIZE, CUSTOM_TEXTURE_SIZE, 1],
                ..Default::default()
            }]
            .into(),
            ..CopyBufferToImageInfo::buffer_image(src_buffer, custom_image.clone())
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

    println!("Updated custom texture slot {}", slot);
}

/// Update a slot in the picture atlas with picture data.
/// slot: 0-63, width: picture width (max 256), height: picture height (max 256)
/// pixels: RGBA data (width × height × 4 bytes)
#[allow(clippy::too_many_arguments)]
pub fn update_picture_slot(
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    queue: &Arc<Queue>,
    picture_image: &Arc<Image>,
    slot: u32,
    width: u32,
    height: u32,
    pixels: &[u8],
) {
    assert!(slot < PICTURE_ATLAS_SLOTS, "Invalid picture slot");
    assert_eq!(
        pixels.len(),
        (width * height * 4) as usize,
        "Invalid pixel data size"
    );
    assert!(width <= 128, "Picture width too large");
    assert!(height <= 128, "Picture height too large");

    let src_buffer = Buffer::from_iter(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::TRANSFER_SRC,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        pixels.to_vec(),
    )
    .unwrap();

    let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
        command_buffer_allocator.clone(),
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    // Copy to the specific slot region in the picture atlas
    // Each slot is PICTURE_ATLAS_SIZE (384) wide, but pictures can be smaller
    command_buffer_builder
        .copy_buffer_to_image(CopyBufferToImageInfo {
            regions: vec![BufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: width,
                buffer_image_height: height,
                image_subresource: picture_image.subresource_layers(),
                image_offset: [slot * PICTURE_ATLAS_SIZE, 0, 0],
                image_extent: [width, height, 1],
                ..Default::default()
            }]
            .into(),
            ..CopyBufferToImageInfo::buffer_image(src_buffer, picture_image.clone())
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

    println!("Updated picture slot {} ({}×{})", slot, width, height);
}

/// Upload a picture from the picture library to the GPU atlas.
/// Returns true if successful, false if picture_id is not found.
pub fn upload_picture_to_atlas(
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    queue: &Arc<Queue>,
    picture_image: &Arc<Image>,
    picture_library: &crate::pictures::PictureLibrary,
    picture_id: u32,
) -> bool {
    let picture = match picture_library.get(picture_id) {
        Some(p) => p,
        None => return false,
    };

    // Ensure picture is 128×128 for frames (no resize needed for correct size)
    let (width, height, pixels) = if picture.width == 128 && picture.height == 128 {
        (picture.width, picture.height, picture.pixels.clone())
    } else {
        // Picture is not 128×128 - this shouldn't happen with current export
        // Use as-is but log a warning
        println!(
            "[PictureAtlas] WARNING: Picture '{}' is {}×{} (expected 128×128), using as-is",
            picture.name, picture.width, picture.height
        );
        (picture.width, picture.height, picture.pixels.clone())
    };

    update_picture_slot(
        memory_allocator,
        command_buffer_allocator,
        queue,
        picture_image,
        picture_id % PICTURE_ATLAS_SLOTS,
        width.into(),
        height.into(),
        &pixels,
    );

    println!(
        "[PictureAtlas] Uploaded picture '{}' (ID {}, slot {}) to GPU atlas",
        picture.name,
        picture_id,
        picture_id % PICTURE_ATLAS_SLOTS
    );
    true
}

/// Maximum number of water/lava sources to show in debug mode.
pub const MAX_WATER_SOURCES: usize = 512;

/// GPU-compatible water source data for debug visualization.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuWaterSource {
    /// Position XYZ + type W (0=water, 1=lava)
    pub position: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuTemplateBlock {
    /// Position XYZ + unused W
    pub position: [f32; 4],
}

pub const MAX_TEMPLATE_BLOCKS: usize = 256;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuStencilBlock {
    /// Position XYZ + stencil_id W
    pub position: [f32; 4],
}

pub const MAX_STENCIL_BLOCKS: usize = 4096;

/// Creates storage buffers and descriptor set for particle, falling block, water source, template block, stencil block, and remote player data.
/// All share set index 3: particles at binding 0, falling blocks at binding 1, water sources at binding 2, template blocks at binding 3, stencil blocks at binding 4, remote players at binding 5.
#[allow(clippy::type_complexity)]
pub fn get_particle_and_falling_block_set(
    memory_allocator: Arc<StandardMemoryAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
) -> (
    Subbuffer<[particles::GpuParticle]>,
    Subbuffer<[GpuFallingBlock]>,
    Subbuffer<[GpuWaterSource]>,
    Subbuffer<[GpuTemplateBlock]>,
    Subbuffer<[GpuStencilBlock]>,
    Subbuffer<[GpuRemotePlayer]>,
    Arc<DescriptorSet>,
) {
    use particles::{GpuParticle, MAX_PARTICLES};

    // Create storage buffers
    let particle_buffer =
        make_storage_buffer::<GpuParticle>(&memory_allocator, MAX_PARTICLES as u64);
    let falling_block_buffer =
        make_storage_buffer::<GpuFallingBlock>(&memory_allocator, MAX_FALLING_BLOCKS as u64);
    let water_source_buffer =
        make_storage_buffer::<GpuWaterSource>(&memory_allocator, MAX_WATER_SOURCES as u64);
    let template_block_buffer =
        make_storage_buffer::<GpuTemplateBlock>(&memory_allocator, MAX_TEMPLATE_BLOCKS as u64);
    let stencil_block_buffer =
        make_storage_buffer::<GpuStencilBlock>(&memory_allocator, MAX_STENCIL_BLOCKS as u64);
    let remote_player_buffer =
        make_storage_buffer::<GpuRemotePlayer>(&memory_allocator, MAX_REMOTE_PLAYERS as u64);

    // Create descriptor set at set index 3 with all buffers
    let descriptor_set = make_set(
        &descriptor_set_allocator,
        render_pipeline,
        3,
        [
            WriteDescriptorSet::buffer(0, particle_buffer.clone()),
            WriteDescriptorSet::buffer(1, falling_block_buffer.clone()),
            WriteDescriptorSet::buffer(2, water_source_buffer.clone()),
            WriteDescriptorSet::buffer(3, template_block_buffer.clone()),
            WriteDescriptorSet::buffer(4, stencil_block_buffer.clone()),
            WriteDescriptorSet::buffer(5, remote_player_buffer.clone()),
        ],
    );

    (
        particle_buffer,
        falling_block_buffer,
        water_source_buffer,
        template_block_buffer,
        stencil_block_buffer,
        remote_player_buffer,
        descriptor_set,
    )
}

/// Maximum number of point lights (torches) that can be active at once.
pub const MAX_LIGHTS: usize = 256;

/// GPU-compatible point light data for shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuLight {
    /// Position XYZ + radius W
    pub pos_radius: [f32; 4],
    /// Color RGB + intensity A (intensity is raw value, mode is in animation.x)
    pub color_intensity: [f32; 4],
    /// Animation: x = mode (as float), y = reserved, z = reserved, w = pre-computed animation factor
    pub animation: [f32; 4],
}

/// Creates a storage buffer and descriptor set for point light data.
pub fn get_light_set(
    memory_allocator: Arc<StandardMemoryAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
) -> (Subbuffer<[GpuLight]>, Arc<DescriptorSet>) {
    // Create a storage buffer for lights (initialized to zeros)
    let light_buffer = make_storage_buffer::<GpuLight>(&memory_allocator, MAX_LIGHTS as u64);

    // Create descriptor set at set index 4
    let descriptor_set = make_set(
        &descriptor_set_allocator,
        render_pipeline,
        4,
        [WriteDescriptorSet::buffer(0, light_buffer.clone())],
    );

    (light_buffer, descriptor_set)
}

/// Number of chunks in the metadata buffer (must match shader constants)
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

/// Number of u32 words for brick masks (2 words = 64 bits per chunk).
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
#[allow(clippy::type_complexity)]
pub fn get_brick_and_model_set(
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
    queue: &Arc<Queue>,
    world_extent: [u32; 3],
    model_registry: &ModelRegistry,
) -> (
    Subbuffer<[u32]>,                // brick_mask_buffer
    Subbuffer<[u32]>,                // brick_dist_buffer
    Arc<Image>,                      // model_atlas_8 (8³ resolution tier)
    Arc<Image>,                      // model_atlas_16 (16³ resolution tier)
    Arc<Image>,                      // model_atlas_32 (32³ resolution tier)
    Arc<Image>,                      // model_palettes
    Arc<Image>,                      // model_palette_emission
    Arc<Image>,                      // model_metadata
    Arc<Image>,                      // block_custom_data
    Subbuffer<[GpuModelProperties]>, // model_properties_buffer
    Arc<DescriptorSet>,              // combined set 7
) {
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
        &model_atlas_8,
        &model_atlas_16,
        &model_atlas_32,
        &model_palettes,
        &model_palette_emission,
        &model_properties_buffer,
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

    (
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
    )
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

/// Uploads model registry data (atlas, palettes, properties) to GPU.
/// Uploads models to three separate atlases (8³, 16³, 32³) at native resolutions.
#[allow(clippy::too_many_arguments)]
pub fn upload_model_registry(
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    queue: &Arc<Queue>,
    registry: &ModelRegistry,
    atlas_8: &Arc<Image>,
    atlas_16: &Arc<Image>,
    atlas_32: &Arc<Image>,
    palettes: &Arc<Image>,
    palette_emission: &Arc<Image>,
    properties_buffer: &Subbuffer<[GpuModelProperties]>,
) {
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
    println!(
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

// Thread-local transfer ring buffer for async chunk uploads.
// Using 6 slots provides headroom for 2 frames worth of transfers before blocking.
// Each frame can have up to 3 upload calls (completed chunks, unloaded chunks, dirty chunks).
thread_local! {
    static TRANSFER_RING: RefCell<TransferRingBuffer> = RefCell::new(TransferRingBuffer::new(6));
    static STAGING_POOL: RefCell<Vec<StagingBufferPair>> = const { RefCell::new(Vec::new()) };
}

const STAGING_POOL_MAX: usize = 12; // 2x ring buffer capacity

/// Uploads chunk data to GPU textures using async DMA transfers.
///
/// On discrete GPUs with separate transfer queues, this allows PCIe transfers
/// to run in parallel with graphics workloads for better performance.
///
/// # Arguments
/// * `transfer_queue` - The queue to use for transfers (dedicated transfer or graphics fallback)
/// * `graphics_queue_family` - Graphics queue family index for ownership transfers
/// * `separate_transfer_queue` - Whether transfer and graphics queues are different families
#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
pub fn upload_chunks_batched(
    memory_allocator: &Arc<StandardMemoryAllocator>,
    command_buffer_allocator: &Arc<StandardCommandBufferAllocator>,
    transfer_queue: &Arc<Queue>,
    graphics_queue_family: u32,
    separate_transfer_queue: bool,
    voxel_image: &Arc<Image>,
    model_metadata_image: &Arc<Image>,
    block_custom_data_image: &Arc<Image>,
    texture_origin: Vector3<i32>,
    chunks: &[(Vector3<i32>, &[u8], &[u8], &[u8])],
) {
    if chunks.is_empty() {
        return;
    }

    // Filter uploads that fit into the current texture window and collect offsets.
    struct Upload<'a> {
        offset: [u32; 3],
        block_data: &'a [u8],
        model_metadata: &'a [u8],
        custom_data: &'a [u8],
    }
    let mut uploads: Vec<Upload> = Vec::with_capacity(chunks.len());
    let mut total_block_bytes = 0usize;
    let mut total_meta_bytes = 0usize;
    let mut total_custom_bytes = 0usize;

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
        });
        total_block_bytes += block_data.len();
        total_meta_bytes += model_metadata.len();
        total_custom_bytes += custom_data.len();
    }

    if uploads.is_empty() {
        return;
    }

    // Acquire a slot in the ring buffer (may block if all slots busy)
    // Also reclaims completed transfers and their staging buffers
    let (slot_index, reclaimed) = TRANSFER_RING.with(|ring| ring.borrow_mut().acquire_slot());

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
    let (block_staging, meta_staging, custom_staging) = STAGING_POOL.with(|pool| {
        let mut p = pool.borrow_mut();

        // Find a triple with sufficient sizes
        let idx_opt = p.iter().position(|(b, m, c)| {
            b.size() as usize >= total_block_bytes
                && m.size() as usize >= total_meta_bytes
                && c.size() as usize >= total_custom_bytes
        });

        if let Some(idx) = idx_opt {
            p.swap_remove(idx)
        } else {
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
            )
            .unwrap();

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
                total_meta_bytes as u64,
            )
            .unwrap();

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
                total_custom_bytes as u64,
            )
            .unwrap();

            (block_buf, meta_buf, custom_buf)
        }
    });

    // Write data to staging buffers
    {
        let mut block_write = block_staging.write().unwrap();
        let mut meta_write = meta_staging.write().unwrap();
        let mut custom_write = custom_staging.write().unwrap();
        let mut block_cursor = 0usize;
        let mut meta_cursor = 0usize;
        let mut custom_cursor = 0usize;

        for upload in &uploads {
            let blen = upload.block_data.len();
            block_write[block_cursor..block_cursor + blen].copy_from_slice(upload.block_data);
            block_cursor += blen;

            let mlen = upload.model_metadata.len();
            meta_write[meta_cursor..meta_cursor + mlen].copy_from_slice(upload.model_metadata);
            meta_cursor += mlen;

            let clen = upload.custom_data.len();
            custom_write[custom_cursor..custom_cursor + clen].copy_from_slice(upload.custom_data);
            custom_cursor += clen;
        }
    }

    // Build copy regions referencing the contiguous staging buffers.
    let mut block_regions = Vec::with_capacity(uploads.len());
    let mut metadata_regions = Vec::with_capacity(uploads.len());
    let mut custom_regions = Vec::with_capacity(uploads.len());
    let mut block_offset = 0u64;
    let mut meta_offset = 0u64;
    let mut custom_offset = 0u64;

    for upload in &uploads {
        block_regions.push(BufferImageCopy {
            buffer_offset: block_offset,
            buffer_row_length: CHUNK_SIZE as u32,
            buffer_image_height: CHUNK_SIZE as u32,
            image_subresource: voxel_image.subresource_layers(),
            image_offset: upload.offset,
            image_extent: [CHUNK_SIZE as u32, CHUNK_SIZE as u32, CHUNK_SIZE as u32],
            ..Default::default()
        });
        block_offset += upload.block_data.len() as u64;

        metadata_regions.push(BufferImageCopy {
            buffer_offset: meta_offset,
            buffer_row_length: CHUNK_SIZE as u32,
            buffer_image_height: CHUNK_SIZE as u32,
            image_subresource: model_metadata_image.subresource_layers(),
            image_offset: upload.offset,
            image_extent: [CHUNK_SIZE as u32, CHUNK_SIZE as u32, CHUNK_SIZE as u32],
            ..Default::default()
        });
        meta_offset += upload.model_metadata.len() as u64;

        custom_regions.push(BufferImageCopy {
            buffer_offset: custom_offset,
            buffer_row_length: CHUNK_SIZE as u32,
            buffer_image_height: CHUNK_SIZE as u32,
            image_subresource: block_custom_data_image.subresource_layers(),
            image_offset: upload.offset,
            image_extent: [CHUNK_SIZE as u32, CHUNK_SIZE as u32, CHUNK_SIZE as u32],
            ..Default::default()
        });
        custom_offset += upload.custom_data.len() as u64;
    }

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

    let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
        command_buffer_allocator.clone(),
        transfer_queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    command_buffer_builder
        .copy_buffer_to_image(CopyBufferToImageInfo {
            regions: block_regions.into(),
            ..CopyBufferToImageInfo::buffer_image(block_staging.clone(), voxel_image.clone())
        })
        .unwrap();

    command_buffer_builder
        .copy_buffer_to_image(CopyBufferToImageInfo {
            regions: metadata_regions.into(),
            ..CopyBufferToImageInfo::buffer_image(
                meta_staging.clone(),
                model_metadata_image.clone(),
            )
        })
        .unwrap();

    command_buffer_builder
        .copy_buffer_to_image(CopyBufferToImageInfo {
            regions: custom_regions.into(),
            ..CopyBufferToImageInfo::buffer_image(
                custom_staging.clone(),
                block_custom_data_image.clone(),
            )
        })
        .unwrap();

    let cb = command_buffer_builder.build().unwrap();

    // Submit to transfer queue and get fence (non-blocking)
    let fence = cb
        .execute(transfer_queue.clone())
        .unwrap()
        .then_signal_fence_and_flush()
        .unwrap();

    // Submit the transfer to the ring buffer (keeps staging buffers alive until GPU completes)
    TRANSFER_RING.with(|ring| {
        ring.borrow_mut().submit(
            slot_index,
            TransferSlot {
                fence,
                block_staging,
                meta_staging,
                custom_staging,
            },
        );
    });

    // Note: We do NOT wait here! The fence is polled on the next upload call.
    // Staging buffers are kept alive in the ring buffer until the transfer completes.
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
) {
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

        let mut buffer_write = chunk_metadata_buffer.write().unwrap();
        buffer_write.copy_from_slice(&metadata);
    });
}

#[allow(dead_code)]
pub fn update_brick_metadata(
    world: &crate::world::World,
    brick_mask_buffer: &Subbuffer<[u32]>,
    brick_dist_buffer: &Subbuffer<[u32]>,
    texture_origin: Vector3<i32>,
) {
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
                let mut mask_write = brick_mask_buffer.write().unwrap();
                mask_write.copy_from_slice(&brick_masks);
            }
            {
                let mut dist_write = brick_dist_buffer.write().unwrap();
                dist_write.copy_from_slice(&brick_distances);
            }
        });
    });
}

pub fn save_screenshot(
    device: &Arc<Device>,

    queue: &Arc<Queue>,

    memory_allocator: &Arc<StandardMemoryAllocator>,

    command_buffer_allocator: &Arc<StandardCommandBufferAllocator>,

    image_view: &Arc<ImageView>,

    path: &str,
) {
    let image = image_view.image();

    let extent = image.extent();

    // Create a buffer to copy the image data into

    let buffer_size = (extent[0] * extent[1] * 4) as u64; // RGBA

    let staging_buffer = Buffer::new_slice::<u8>(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::TRANSFER_DST,

            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_HOST
                | MemoryTypeFilter::HOST_RANDOM_ACCESS,

            ..Default::default()
        },
        buffer_size,
    )
    .expect("Failed to create screenshot staging buffer");

    // Build command buffer to copy image to buffer

    let mut builder = AutoCommandBufferBuilder::primary(
        command_buffer_allocator.clone(),
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    builder
        .copy_image_to_buffer(
            vulkano::command_buffer::CopyImageToBufferInfo::image_buffer(
                image.clone(),
                staging_buffer.clone(),
            ),
        )
        .unwrap();

    let command_buffer = builder.build().unwrap();

    // Execute and wait

    let future = vulkano::sync::now(device.clone())
        .then_execute(queue.clone(), command_buffer)
        .unwrap()
        .then_signal_fence_and_flush()
        .unwrap();

    future.wait(None).unwrap();

    // Read the buffer data

    let buffer_content = staging_buffer.read().unwrap();

    // Create image and save

    let img = image::RgbaImage::from_raw(extent[0], extent[1], buffer_content.to_vec())
        .expect("Failed to create image from buffer");

    img.save(path).expect("Failed to save screenshot");

    println!("[SCREENSHOT] Saved to {}", path);
}

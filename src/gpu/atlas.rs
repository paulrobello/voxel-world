//! Texture atlas and sprite icon GPU resources: the async atlas-upload ring,
//! the main/custom/picture texture atlases, the multiplayer texture array,
//! and the egui sprite-icon loader.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui_winit_vulkano::{Gui, egui};
use vulkano::{
    buffer::{Buffer, BufferCreateInfo, BufferUsage, Subbuffer},
    command_buffer::{
        AutoCommandBufferBuilder, BufferImageCopy, CommandBufferUsage, CopyBufferToImageInfo,
        PrimaryCommandBufferAbstract, allocator::StandardCommandBufferAllocator,
    },
    descriptor_set::{
        DescriptorSet, WriteDescriptorSet, allocator::StandardDescriptorSetAllocator,
    },
    device::{DeviceOwned, Queue},
    format::Format,
    image::{
        Image, ImageCreateInfo, ImageType, ImageUsage,
        sampler::{Filter, Sampler, SamplerAddressMode, SamplerCreateInfo},
        view::{ImageView, ImageViewCreateInfo},
    },
    memory::allocator::{AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator},
    pipeline::ComputePipeline,
    sync::{
        GpuFuture,
        future::{FenceSignalFuture, NowFuture},
    },
};

use crate::chunk::BlockType;

use super::make_set;

// ---------------------------------------------------------------------------
// Atlas upload ring (custom textures + pictures)
// ---------------------------------------------------------------------------

type AtlasUploadFence =
    FenceSignalFuture<vulkano::command_buffer::CommandBufferExecFuture<NowFuture>>;

struct PendingAtlasUpload {
    fence: AtlasUploadFence,
    // Keep staging buffers alive until the fence signals. Accessed only via
    // Drop when the entry is removed from the ring.
    _staging: Vec<Subbuffer<[u8]>>,
}

/// Tracks in-flight custom-texture and picture atlas uploads so we can submit
/// them asynchronously. Staging buffers are kept alive until their fence
/// signals, avoiding the `.wait(None)` stall after every slot upload.
#[derive(Default)]
pub struct AtlasUploadRing {
    pending: Vec<PendingAtlasUpload>,
}

impl AtlasUploadRing {
    fn push(&mut self, fence: AtlasUploadFence, staging: Vec<Subbuffer<[u8]>>) {
        self.pending.push(PendingAtlasUpload {
            fence,
            _staging: staging,
        });
    }

    /// Drops entries whose fences have signaled. Call once per frame.
    pub fn poll(&mut self) {
        self.pending
            .retain(|u| !u.fence.is_signaled().unwrap_or(false));
    }
}

thread_local! {
    static ATLAS_UPLOAD_RING: RefCell<AtlasUploadRing> =
        const { RefCell::new(AtlasUploadRing { pending: Vec::new() }) };
}

/// Release staging buffers for any atlas uploads whose fences have signaled.
/// Safe to call every frame; cheap no-op when the ring is empty.
pub fn poll_atlas_upload_ring() {
    ATLAS_UPLOAD_RING.with(|r| r.borrow_mut().poll());
}

/// Submit a pre-built command buffer without blocking and park its staging
/// buffers in the atlas upload ring until the GPU fence signals.
fn submit_atlas_upload(
    queue: &Arc<Queue>,
    command_buffer: Arc<vulkano::command_buffer::PrimaryAutoCommandBuffer>,
    staging: Vec<Subbuffer<[u8]>>,
) {
    let fence = command_buffer
        .execute(queue.clone())
        .unwrap()
        .then_signal_fence_and_flush()
        .unwrap();
    ATLAS_UPLOAD_RING.with(|r| r.borrow_mut().push(fence, staging));
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

    log::debug!(
        "Loaded texture: {}x{} from {:?}",
        width,
        height,
        texture_path
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

/// Create the multiplayer custom texture array (2DArray).
/// Returns (image, image_view, sampler) for use in descriptor sets.
pub fn create_multiplayer_texture_array(
    memory_allocator: Arc<StandardMemoryAllocator>,
    max_slots: u32,
) -> (Arc<Image>, Arc<ImageView>, Arc<Sampler>) {
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

    log::debug!(
        "Created multiplayer texture array: {}x{}x{} slots",
        MULTIPLAYER_TEXTURE_SIZE,
        MULTIPLAYER_TEXTURE_SIZE,
        max_slots
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
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("Invalid PNG: {}", e))?;

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
    reader
        .next_frame(&mut buf)
        .map_err(|e| format!("Failed to decode PNG: {}", e))?;

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
                buffer_row_length: MULTIPLAYER_TEXTURE_SIZE,
                buffer_image_height: MULTIPLAYER_TEXTURE_SIZE,
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

/// Resources returned by [`load_texture_atlases`].
pub struct TextureAtlasResources {
    pub descriptor_set: Arc<DescriptorSet>,
    /// Retained for callers that need to recreate descriptor sets with a different sampler.
    #[allow(dead_code)]
    pub sampler: Arc<Sampler>,
    pub main_image_view: Arc<ImageView>,
    /// Retained for callers that need to update custom atlas slots.
    #[allow(dead_code)]
    pub custom_image_view: Arc<ImageView>,
    pub custom_image: Arc<Image>,
    pub picture_image_view: Arc<ImageView>,
    pub picture_image: Arc<Image>,
}

/// Load texture atlases (main, custom, and picture) and create a combined descriptor set.
/// The `custom_image` and `picture_image` fields are retained so callers can update
/// the atlases dynamically at runtime.
pub fn load_texture_atlases(
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
    queue: &Arc<Queue>,
    texture_path: &std::path::Path,
) -> TextureAtlasResources {
    // Load the main texture atlas
    let img = image::open(texture_path)
        .expect("Failed to load texture")
        .to_rgba8();
    let (width, height) = img.dimensions();
    let image_data: Vec<u8> = img.into_raw();

    log::debug!(
        "Loaded main texture atlas: {}x{} from {:?}",
        width,
        height,
        texture_path
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

    log::debug!(
        "Created custom texture atlas: {}x{}",
        CUSTOM_ATLAS_WIDTH,
        CUSTOM_ATLAS_HEIGHT
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

    log::debug!(
        "Created picture atlas: {}x{} ({} slots)",
        PICTURE_ATLAS_WIDTH,
        PICTURE_ATLAS_HEIGHT,
        PICTURE_ATLAS_SLOTS
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

    TextureAtlasResources {
        descriptor_set,
        sampler,
        main_image_view,
        custom_image_view,
        custom_image,
        picture_image_view,
        picture_image,
    }
}

/// Upload one or more custom texture slots in a single command buffer submission.
///
/// Each slot gets its own small staging buffer, but all copies are recorded into
/// one command buffer and protected by a single fence. Callers should prefer
/// this over calling [`update_custom_texture_slot`] in a loop when more than one
/// slot is dirty.
pub fn batch_update_custom_texture_slots<'a, I>(
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    queue: &Arc<Queue>,
    custom_image: &Arc<Image>,
    slots: I,
) where
    I: IntoIterator<Item = (u32, &'a [u8])>,
{
    let mut builder = AutoCommandBufferBuilder::primary(
        command_buffer_allocator.clone(),
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    let mut staging_buffers: Vec<Subbuffer<[u8]>> = Vec::new();
    let mut recorded = 0usize;
    for (slot, pixels) in slots {
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

        builder
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
                ..CopyBufferToImageInfo::buffer_image(src_buffer.clone(), custom_image.clone())
            })
            .unwrap();

        staging_buffers.push(src_buffer);
        recorded += 1;
    }

    if recorded == 0 {
        return;
    }

    submit_atlas_upload(queue, builder.build().unwrap(), staging_buffers);
}

/// Parameters for a single picture-atlas slot upload.
pub struct PictureSlotUpload<'a> {
    /// Destination slot index (0–63).
    pub slot: u32,
    /// Width of the picture in pixels (max 128).
    pub width: u32,
    /// Height of the picture in pixels (max 128).
    pub height: u32,
    /// RGBA pixel data (`width × height × 4` bytes).
    pub pixels: &'a [u8],
}

/// Update a slot in the picture atlas with picture data.
pub fn update_picture_slot(
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    queue: &Arc<Queue>,
    picture_image: &Arc<Image>,
    upload: PictureSlotUpload<'_>,
) {
    let PictureSlotUpload {
        slot,
        width,
        height,
        pixels,
    } = upload;
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
            ..CopyBufferToImageInfo::buffer_image(src_buffer.clone(), picture_image.clone())
        })
        .unwrap();

    submit_atlas_upload(
        queue,
        command_buffer_builder.build().unwrap(),
        vec![src_buffer],
    );
    log::debug!("Queued picture slot {} ({}×{})", slot, width, height);
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
        log::debug!(
            "[PictureAtlas] WARNING: Picture '{}' is {}×{} (expected 128×128), using as-is",
            picture.name,
            picture.width,
            picture.height
        );
        (picture.width, picture.height, picture.pixels.clone())
    };

    update_picture_slot(
        memory_allocator,
        command_buffer_allocator,
        queue,
        picture_image,
        PictureSlotUpload {
            slot: picture_id % PICTURE_ATLAS_SLOTS,
            width: width.into(),
            height: height.into(),
            pixels: &pixels,
        },
    );

    log::debug!(
        "[PictureAtlas] Uploaded picture '{}' (ID {}, slot {}) to GPU atlas",
        picture.name,
        picture_id,
        picture_id % PICTURE_ATLAS_SLOTS
    );
    true
}

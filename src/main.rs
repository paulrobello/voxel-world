//! Voxel Game Engine
//!
//! A Minecraft-like voxel game with GPU ray-marching rendering.

#[cfg(target_os = "macos")]
mod macos_cursor {
    use core_graphics::display::CGAssociateMouseAndMouseCursorPosition;
    use objc2_app_kit::NSCursor;

    /// Grab cursor and hide it using native macOS APIs.
    /// This avoids winit's set_cursor_visible which crashes with SIGBUS.
    pub fn grab_and_hide() {
        unsafe {
            // Disconnect mouse movement from cursor position (0 = false)
            CGAssociateMouseAndMouseCursorPosition(0);
            // Hide the cursor
            NSCursor::hide();
        }
    }

    /// Release cursor and show it using native macOS APIs.
    pub fn release_and_show() {
        unsafe {
            // Reconnect mouse movement to cursor position (1 = true)
            CGAssociateMouseAndMouseCursorPosition(1);
            // Show the cursor
            NSCursor::unhide();
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod macos_cursor {
    pub fn grab_and_hide() {}
    pub fn release_and_show() {}
}

use egui_winit_vulkano::{Gui, GuiConfig, egui};
use nalgebra::{Matrix4, Vector3, vector};
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};
use std::path::PathBuf;
use std::{
    f64::consts::{FRAC_PI_2, TAU},
    sync::Arc,
    time::{Duration, Instant},
};
use vulkano::{
    Validated, Version, VulkanError, VulkanLibrary,
    buffer::{Buffer, BufferContents, BufferCreateInfo, BufferUsage, Subbuffer},
    command_buffer::{
        AutoCommandBufferBuilder, BlitImageInfo, ClearColorImageInfo, CommandBufferUsage,
        CopyBufferToImageInfo, PrimaryCommandBufferAbstract,
        allocator::StandardCommandBufferAllocator,
    },
    descriptor_set::{
        DescriptorSet, WriteDescriptorSet, allocator::StandardDescriptorSetAllocator,
    },
    device::{
        Device, DeviceCreateInfo, DeviceExtensions, DeviceFeatures, DeviceOwned, Queue,
        QueueCreateInfo, QueueFlags, physical::PhysicalDeviceType,
    },
    format::Format,
    image::{
        Image, ImageCreateInfo, ImageType, ImageUsage,
        sampler::{Filter, Sampler, SamplerAddressMode, SamplerCreateInfo},
        view::{ImageView, ImageViewCreateInfo},
    },
    instance::{Instance, InstanceCreateFlags, InstanceCreateInfo},
    memory::allocator::{AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator},
    pipeline::{ComputePipeline, Pipeline, PipelineBindPoint},
    swapchain::{
        PresentMode, Surface, Swapchain, SwapchainCreateInfo, SwapchainPresentInfo,
        acquire_next_image,
    },
    sync::GpuFuture,
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{DeviceEvent, DeviceId, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::KeyCode,
    window::{Icon, Window, WindowId},
};
use winit_input_helper::WinitInputHelper;

mod camera;
mod chunk;
mod hot_reload;
mod particles;
mod raycast;
mod world;

use crate::camera::Camera;
use crate::chunk::{BlockType, CHUNK_SIZE, Chunk};
use crate::hot_reload::HotReloadComputePipeline;
use crate::particles::ParticleSystem;
use crate::raycast::{MAX_RAYCAST_DISTANCE, RaycastHit, get_place_position, raycast};
use crate::world::World;

const INITIAL_WINDOW_RESOLUTION: PhysicalSize<u32> = PhysicalSize::new(960, 960);

// World size in chunks
const WORLD_CHUNKS_X: i32 = 3;
const WORLD_CHUNKS_Y: i32 = 1;
const WORLD_CHUNKS_Z: i32 = 3;

// World size in blocks
const WORLD_SIZE_X: usize = WORLD_CHUNKS_X as usize * CHUNK_SIZE;
const WORLD_SIZE_Y: usize = WORLD_CHUNKS_Y as usize * CHUNK_SIZE;
const WORLD_SIZE_Z: usize = WORLD_CHUNKS_Z as usize * CHUNK_SIZE;

// Player physics constants (in world/voxel units, where 1 unit = 1 block)
/// Gravity acceleration in blocks per second squared
const GRAVITY: f64 = 20.0;
/// Jump velocity in blocks per second
const JUMP_VELOCITY: f64 = 8.0;
/// Player movement speed in blocks per second
const MOVE_SPEED: f64 = 5.0;
/// Player hitbox half-width (X and Z)
const PLAYER_HALF_WIDTH: f64 = 0.3;
/// Player height (from feet to camera)
const PLAYER_HEIGHT: f64 = 1.7;
/// Player eye height from feet
const PLAYER_EYE_HEIGHT: f64 = 1.6;

// Swimming physics constants
/// Gravity when submerged in water (reduced buoyancy effect)
const WATER_GRAVITY: f64 = 4.0;
/// Buoyancy force when in water (pushes player up slightly)
const WATER_BUOYANCY: f64 = 2.0;
/// Movement speed in water (slower than on land)
const SWIM_SPEED: f64 = 3.0;
/// Vertical swim speed (when pressing Space to swim up)
const SWIM_UP_SPEED: f64 = 4.0;
/// Vertical sink speed (when pressing Shift to swim down)
const SWIM_DOWN_SPEED: f64 = 3.0;
/// Water drag (velocity multiplier per second, lower = more drag)
const WATER_DRAG: f64 = 0.85;

// Day/night cycle constants
/// Duration of a full day cycle in seconds (real time)
const DAY_CYCLE_DURATION: f32 = 120.0;
/// Default time of day (0.0 = 6am, 0.5 = 6pm, formula: hours = (v * 24 + 6) % 24)
/// 0.583 = 20:00 (8pm)
const DEFAULT_TIME_OF_DAY: f32 = 0.583;

// Block breaking constants
/// Time in seconds to break a block
const BLOCK_BREAK_TIME: f32 = 0.5;

/// Render modes for debugging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
enum RenderMode {
    Coord = 0,
    Steps = 1,
    #[default]
    Normal = 2,
    UV = 3,
    Depth = 4,
}

impl RenderMode {
    pub const ALL: &'static [RenderMode] = &[
        RenderMode::Coord,
        RenderMode::Steps,
        RenderMode::Normal,
        RenderMode::UV,
        RenderMode::Depth,
    ];
}

/// Terrain generator using Fractal Brownian Motion noise
struct TerrainGenerator {
    height_noise: Fbm<Perlin>,
    detail_noise: Perlin,
}

impl TerrainGenerator {
    fn new(seed: u32) -> Self {
        // Multi-octave noise for smooth terrain with detail
        let height_noise = Fbm::<Perlin>::new(seed)
            .set_octaves(5)
            .set_frequency(0.015) // Lower frequency = larger, smoother features
            .set_lacunarity(2.0) // Frequency multiplier per octave
            .set_persistence(0.4); // Lower persistence = smoother terrain

        let detail_noise = Perlin::new(seed.wrapping_add(1));

        Self {
            height_noise,
            detail_noise,
        }
    }

    /// Get terrain height at world coordinates
    fn get_height(&self, world_x: i32, world_z: i32) -> i32 {
        let x = world_x as f64;
        let z = world_z as f64;

        // Base terrain from FBM noise (returns roughly -1 to 1)
        let base_height = self.height_noise.get([x, z]);

        // Add very subtle detail
        let detail = self.detail_noise.get([x * 0.03, z * 0.03]) * 0.1;

        // Map noise to height range: base of 10, amplitude of 6 (gentler slopes)
        let height = 10.0 + (base_height + detail) * 6.0;

        height.round() as i32
    }

    /// Simple hash for tree placement randomness
    fn hash(&self, x: i32, z: i32) -> i32 {
        let mut h = (x.wrapping_mul(374761393)) ^ (z.wrapping_mul(668265263));
        h = (h ^ (h >> 13)).wrapping_mul(1274126177);
        (h ^ (h >> 16)).abs()
    }
}

/// Creates a test world with terrain across multiple chunks.
fn create_game_world() -> World {
    let mut world = World::new();
    let terrain = TerrainGenerator::new(42); // Fixed seed for reproducibility

    // Generate all chunks
    for cx in 0..WORLD_CHUNKS_X {
        for cy in 0..WORLD_CHUNKS_Y {
            for cz in 0..WORLD_CHUNKS_Z {
                let mut chunk = Chunk::new();
                let chunk_world_x = cx * CHUNK_SIZE as i32;
                let chunk_world_y = cy * CHUNK_SIZE as i32;
                let chunk_world_z = cz * CHUNK_SIZE as i32;

                // Generate terrain for this chunk
                for lx in 0..CHUNK_SIZE {
                    for lz in 0..CHUNK_SIZE {
                        let world_x = chunk_world_x + lx as i32;
                        let world_z = chunk_world_z + lz as i32;
                        let height = terrain.get_height(world_x, world_z);

                        for ly in 0..CHUNK_SIZE {
                            let world_y = chunk_world_y + ly as i32;

                            let block_type = if world_y > height {
                                BlockType::Air
                            } else if world_y == height {
                                BlockType::Grass
                            } else if world_y > height - 3 {
                                BlockType::Dirt
                            } else {
                                BlockType::Stone
                            };
                            chunk.set_block(lx, ly, lz, block_type);
                        }
                    }
                }

                world.insert_chunk(vector![cx, cy, cz], chunk);
            }
        }
    }

    // Add trees at various locations across the world
    let tree_positions = [
        (16, 16),
        (48, 16),
        (80, 16),
        (16, 48),
        (80, 48),
        (16, 80),
        (48, 80),
        (80, 80),
        (32, 32),
        (64, 64),
        (24, 72),
        (70, 20),
        (20, 70),
        (55, 55),
    ];

    for (tx, tz) in tree_positions {
        if tx < WORLD_SIZE_X as i32 && tz < WORLD_SIZE_Z as i32 {
            let height = terrain.get_height(tx, tz);
            // Taller trunks: 5-7 blocks (randomized)
            let trunk_height = 5 + (terrain.hash(tx, tz).abs() % 3);

            // Tree trunk
            for y in (height + 1)..=(height + trunk_height) {
                if y < WORLD_SIZE_Y as i32 {
                    world.set_block(vector![tx, y, tz], BlockType::Log);
                }
            }

            // Rounded tree canopy using spherical distance
            let canopy_center_y = height + trunk_height;
            let canopy_radius = 2.5 + (terrain.hash(tx + 1, tz).abs() % 2) as f32 * 0.5; // 2.5-3.0
            let canopy_height = 3 + (terrain.hash(tx, tz + 1).abs() % 2); // 3-4 blocks tall

            for dx in -3..=3i32 {
                for dy in -1..=canopy_height {
                    for dz in -3..=3i32 {
                        let lx = tx + dx;
                        let ly = canopy_center_y + dy;
                        let lz = tz + dz;

                        if lx >= 0
                            && lx < WORLD_SIZE_X as i32
                            && ly >= 0
                            && ly < WORLD_SIZE_Y as i32
                            && lz >= 0
                            && lz < WORLD_SIZE_Z as i32
                        {
                            // Calculate distance from canopy center (ellipsoid shape)
                            let dist_xz = ((dx * dx + dz * dz) as f32).sqrt();
                            let dist_y = (dy as f32 - canopy_height as f32 * 0.3).abs() / 1.5;
                            let dist = (dist_xz * dist_xz + dist_y * dist_y).sqrt();

                            // Add leaves if within rounded canopy shape
                            if dist <= canopy_radius
                                && world.get_block(vector![lx, ly, lz]) == Some(BlockType::Air)
                            {
                                world.set_block(vector![lx, ly, lz], BlockType::Leaves);
                            }
                        }
                    }
                }
            }
        }
    }

    // Add a water pool in the center - carve out terrain and fill with water
    let pool_center_x = (WORLD_SIZE_X / 2) as i32;
    let pool_center_z = (WORLD_SIZE_Z / 2) as i32;
    let pool_radius = 10i32;
    let pool_depth = 4; // Depth of water

    for wx in (pool_center_x - pool_radius - 2)..=(pool_center_x + pool_radius + 2) {
        for wz in (pool_center_z - pool_radius - 2)..=(pool_center_z + pool_radius + 2) {
            if wx >= 0 && wx < WORLD_SIZE_X as i32 && wz >= 0 && wz < WORLD_SIZE_Z as i32 {
                let dx = wx - pool_center_x;
                let dz = wz - pool_center_z;
                let dist_sq = dx * dx + dz * dz;
                let terrain_height = terrain.get_height(wx, wz);

                // Inside pool - carve out and fill with water
                if dist_sq <= pool_radius * pool_radius {
                    let pool_bottom = terrain_height - pool_depth;
                    let water_surface = terrain_height;

                    // Carve out the pool (set to air, then fill with water)
                    for y in pool_bottom..=terrain_height {
                        world.set_block(vector![wx, y, wz], BlockType::Air);
                    }

                    // Sand at bottom
                    world.set_block(vector![wx, pool_bottom, wz], BlockType::Sand);

                    // Fill with water
                    for y in (pool_bottom + 1)..=water_surface {
                        world.set_block(vector![wx, y, wz], BlockType::Water);
                    }
                }
                // Sandy beach around pool
                else if dist_sq <= (pool_radius + 2) * (pool_radius + 2) {
                    world.set_block(vector![wx, terrain_height, wz], BlockType::Sand);
                }
            }
        }
    }

    // Count non-air blocks
    let mut count = 0;
    for cx in 0..WORLD_CHUNKS_X {
        for cy in 0..WORLD_CHUNKS_Y {
            for cz in 0..WORLD_CHUNKS_Z {
                if let Some(chunk) = world.get_chunk(vector![cx, cy, cz]) {
                    for x in 0..CHUNK_SIZE {
                        for y in 0..CHUNK_SIZE {
                            for z in 0..CHUNK_SIZE {
                                if chunk.get_block(x, y, z) != BlockType::Air {
                                    count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    println!(
        "Created world: {}x{}x{} blocks ({} chunks), {} non-air blocks",
        WORLD_SIZE_X,
        WORLD_SIZE_Y,
        WORLD_SIZE_Z,
        WORLD_CHUNKS_X * WORLD_CHUNKS_Y * WORLD_CHUNKS_Z,
        count
    );

    world
}

/// Converts the world to a flat array of block data for GPU upload.
/// Layout: data[x + y * WORLD_SIZE_X + z * WORLD_SIZE_X * WORLD_SIZE_Y]
fn world_to_block_data(world: &World) -> Vec<u8> {
    let total_size = WORLD_SIZE_X * WORLD_SIZE_Y * WORLD_SIZE_Z;
    let mut data = vec![0u8; total_size];

    // Iterate over all chunks in the world
    for cx in 0..WORLD_CHUNKS_X {
        for cy in 0..WORLD_CHUNKS_Y {
            for cz in 0..WORLD_CHUNKS_Z {
                let chunk_pos = vector![cx, cy, cz];
                if let Some(chunk) = world.get_chunk(chunk_pos) {
                    let chunk_data = chunk.to_block_data();

                    // Copy chunk data to correct position in world array
                    for lx in 0..CHUNK_SIZE {
                        for ly in 0..CHUNK_SIZE {
                            for lz in 0..CHUNK_SIZE {
                                let world_x = cx as usize * CHUNK_SIZE + lx;
                                let world_y = cy as usize * CHUNK_SIZE + ly;
                                let world_z = cz as usize * CHUNK_SIZE + lz;

                                let chunk_idx = lx + ly * CHUNK_SIZE + lz * CHUNK_SIZE * CHUNK_SIZE;
                                let world_idx = world_x
                                    + world_y * WORLD_SIZE_X
                                    + world_z * WORLD_SIZE_X * WORLD_SIZE_Y;

                                data[world_idx] = chunk_data[chunk_idx];
                            }
                        }
                    }
                }
            }
        }
    }

    let non_air: usize = data.iter().filter(|&&b| b != 0).count();
    println!(
        "Block data: {} bytes, {} non-air blocks",
        data.len(),
        non_air
    );
    data
}

fn get_allocators(
    device: &Arc<Device>,
) -> (
    Arc<StandardMemoryAllocator>,
    Arc<StandardDescriptorSetAllocator>,
    Arc<StandardCommandBufferAllocator>,
) {
    let memory_allocator = Arc::new(StandardMemoryAllocator::new_default(device.clone()));
    let descriptor_set_allocator = Arc::new(StandardDescriptorSetAllocator::new(
        device.clone(),
        Default::default(),
    ));
    let command_buffer_allocator = Arc::new(StandardCommandBufferAllocator::new(
        device.clone(),
        Default::default(),
    ));
    (
        memory_allocator,
        descriptor_set_allocator,
        command_buffer_allocator,
    )
}

fn get_swapchain_images(
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
            image_usage: ImageUsage::COLOR_ATTACHMENT | ImageUsage::TRANSFER_DST,
            composite_alpha,
            present_mode: PresentMode::Immediate,
            ..Default::default()
        },
    )
    .unwrap()
}

fn get_render_image(
    memory_allocator: Arc<StandardMemoryAllocator>,
    extent: [u32; 2],
) -> (Arc<Image>, Arc<ImageView>) {
    let image = Image::new(
        memory_allocator,
        ImageCreateInfo {
            usage: ImageUsage::STORAGE | ImageUsage::TRANSFER_DST,
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

fn get_resample_image(
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

fn get_images_and_sets(
    memory_allocator: Arc<StandardMemoryAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
    resample_pipeline: &ComputePipeline,
    render_extent: [u32; 2],
    window_extent: [u32; 2],
) -> (
    Arc<Image>,
    Arc<DescriptorSet>,
    Arc<Image>,
    Arc<DescriptorSet>,
) {
    let (render_image, render_image_view) =
        get_render_image(memory_allocator.clone(), render_extent);

    let layout = render_pipeline.layout().set_layouts()[0].clone();
    let render_set = DescriptorSet::new(
        descriptor_set_allocator.clone(),
        layout,
        [WriteDescriptorSet::image_view(0, render_image_view.clone())],
        [],
    )
    .unwrap();

    let (resample_image, resample_image_view) = get_resample_image(memory_allocator, window_extent);

    let layout = resample_pipeline.layout().set_layouts()[0].clone();
    let resample_set = DescriptorSet::new(
        descriptor_set_allocator.clone(),
        layout,
        [
            WriteDescriptorSet::image_view(0, render_image_view.clone()),
            WriteDescriptorSet::image_view(1, resample_image_view.clone()),
        ],
        [],
    )
    .unwrap();

    (render_image, render_set, resample_image, resample_set)
}

fn get_voxel_set(
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
    queue: &Arc<Queue>,
    block_data: Vec<u8>,
    world_extent: [u32; 3],
) -> Arc<DescriptorSet> {
    // Each texel is one block (8-bit block type).
    // 3D texture sized to fit entire world.
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
        block_data,
    )
    .unwrap();

    let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
        command_buffer_allocator.clone(),
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    command_buffer_builder
        .clear_color_image(ClearColorImageInfo::image(image.clone()))
        .unwrap()
        .copy_buffer_to_image(CopyBufferToImageInfo::buffer_image(
            src_buffer,
            image.clone(),
        ))
        .unwrap();

    // Wait for the upload to complete before returning the descriptor set
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

    let layout = render_pipeline
        .layout()
        .set_layouts()
        .get(1)
        .unwrap()
        .clone();
    DescriptorSet::new(
        descriptor_set_allocator.clone(),
        layout.clone(),
        [WriteDescriptorSet::image_view(0, image_view)],
        [],
    )
    .unwrap()
}

fn load_icon(icon: &[u8]) -> Icon {
    let (icon_rgba, icon_width, icon_height) = {
        let image = image::load_from_memory(icon).unwrap().to_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };
    Icon::from_rgba(icon_rgba, icon_width, icon_height).unwrap()
}

/// Load a texture atlas from a file and create a GPU texture with sampler.
/// Returns (descriptor_set, sampler) for binding to the shader.
fn load_texture_atlas(
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
    queue: &Arc<Queue>,
    texture_path: &std::path::Path,
) -> (Arc<DescriptorSet>, Arc<Sampler>) {
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

    // Create descriptor set at set index 2
    let layout = render_pipeline
        .layout()
        .set_layouts()
        .get(2)
        .unwrap()
        .clone();

    let descriptor_set = DescriptorSet::new(
        descriptor_set_allocator.clone(),
        layout,
        [WriteDescriptorSet::image_view_sampler(
            0,
            image_view,
            sampler.clone(),
        )],
        [],
    )
    .unwrap();

    (descriptor_set, sampler)
}

/// Creates a storage buffer and descriptor set for particle data.
fn get_particle_set(
    memory_allocator: Arc<StandardMemoryAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
) -> (Subbuffer<[particles::GpuParticle]>, Arc<DescriptorSet>) {
    use particles::{GpuParticle, MAX_PARTICLES};

    // Create a storage buffer for particles (initialized to zeros)
    let particle_buffer = Buffer::new_slice::<GpuParticle>(
        memory_allocator,
        BufferCreateInfo {
            usage: BufferUsage::STORAGE_BUFFER,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        MAX_PARTICLES as u64,
    )
    .unwrap();

    // Create descriptor set at set index 3
    let layout = render_pipeline
        .layout()
        .set_layouts()
        .get(3)
        .unwrap()
        .clone();

    let descriptor_set = DescriptorSet::new(
        descriptor_set_allocator,
        layout,
        [WriteDescriptorSet::buffer(0, particle_buffer.clone())],
        [],
    )
    .unwrap();

    (particle_buffer, descriptor_set)
}

struct App {
    instance: Arc<Instance>,
    device: Arc<Device>,
    queue: Arc<Queue>,

    memory_allocator: Arc<StandardMemoryAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,

    render_pipeline: HotReloadComputePipeline,
    resample_pipeline: HotReloadComputePipeline,

    /// The game world containing all chunks.
    world: World,
    /// GPU descriptor set for voxel data.
    voxel_set: Arc<DescriptorSet>,
    /// GPU descriptor set for texture atlas.
    texture_set: Arc<DescriptorSet>,
    /// GPU buffer for particle data.
    particle_buffer: Subbuffer<[particles::GpuParticle]>,
    /// GPU descriptor set for particles.
    particle_set: Arc<DescriptorSet>,
    /// World dimensions in blocks [X, Y, Z].
    world_extent: [u32; 3],
    /// Flag indicating GPU voxel data needs updating.
    world_dirty: bool,

    camera: Camera,
    render_mode: RenderMode,
    render_scale: f32,

    /// Player physics state
    player_velocity: Vector3<f64>,
    on_ground: bool,
    /// True when player's head is submerged in water
    in_water: bool,
    /// Flying mode (no gravity, vertical movement with Space/Shift)
    fly_mode: bool,
    /// Show debug chunk boundary wireframes
    show_chunk_boundaries: bool,

    /// Current time of day (0.0 = midnight, 0.5 = noon, 1.0 = midnight)
    time_of_day: f32,
    /// Whether the day/night cycle is paused
    day_cycle_paused: bool,
    /// Continuous animation time in seconds (for water waves, etc.)
    animation_time: f32,

    /// Currently selected block type for placing.
    selected_block: BlockType,
    /// Current raycast hit result (for crosshair display).
    current_hit: Option<RaycastHit>,

    /// Block currently being broken (position).
    breaking_block: Option<Vector3<i32>>,
    /// Progress of breaking current block (0.0 to 1.0).
    break_progress: f32,

    /// Particle system for visual effects.
    particles: ParticleSystem,

    input: WinitInputHelper,
    focused: bool,
    /// Deferred cursor grab change (workaround for macOS crash).
    /// true = grab and hide, false = release and show
    pending_grab: Option<bool>,
    last_second: Instant,
    frames_since_last_second: u32,
    fps: u32,

    rcx: Option<RenderContext>,
}

struct RenderContext {
    window: Arc<Window>,
    swapchain: Arc<Swapchain>,
    image_views: Vec<Arc<ImageView>>,

    render_image: Arc<Image>,
    render_set: Arc<DescriptorSet>,
    resample_image: Arc<Image>,
    resample_set: Arc<DescriptorSet>,

    gui: Gui,

    recreate_swapchain: bool,
}

impl App {
    fn new(event_loop: &EventLoop<()>) -> Self {
        let library = VulkanLibrary::new().unwrap();

        let mut required_extensions = Surface::required_extensions(event_loop).unwrap();

        required_extensions.ext_debug_utils = true;

        let instance = Instance::new(
            library,
            InstanceCreateInfo {
                flags: InstanceCreateFlags::ENUMERATE_PORTABILITY,
                enabled_extensions: required_extensions,
                ..Default::default()
            },
        )
        .unwrap();

        let mut device_extensions = DeviceExtensions {
            khr_swapchain: true,
            khr_portability_subset: true,
            ..DeviceExtensions::empty()
        };

        let (physical_device, queue_family_index) = instance
            .enumerate_physical_devices()
            .unwrap()
            .filter(|p| {
                p.api_version() >= Version::V1_3 || p.supported_extensions().khr_dynamic_rendering
            })
            .filter(|p| p.supported_extensions().contains(&device_extensions))
            .filter_map(|p| {
                p.queue_family_properties()
                    .iter()
                    .enumerate()
                    .position(|(i, q)| {
                        q.queue_flags.intersects(QueueFlags::GRAPHICS)
                            && p.presentation_support(i as u32, event_loop).unwrap()
                    })
                    .map(|i| (p, i as u32))
            })
            .min_by_key(|(p, _)| match p.properties().device_type {
                PhysicalDeviceType::DiscreteGpu => 0,
                PhysicalDeviceType::IntegratedGpu => 1,
                PhysicalDeviceType::VirtualGpu => 2,
                PhysicalDeviceType::Cpu => 3,
                PhysicalDeviceType::Other => 4,
                _ => 5,
            })
            .unwrap();

        println!(
            "Using device: {} (type: {:?})",
            physical_device.properties().device_name,
            physical_device.properties().device_type,
        );

        if physical_device.api_version() < Version::V1_3 {
            device_extensions.khr_dynamic_rendering = true;
        }

        let (device, mut queues) = Device::new(
            physical_device,
            DeviceCreateInfo {
                queue_create_infos: vec![QueueCreateInfo {
                    queue_family_index,
                    ..Default::default()
                }],
                enabled_extensions: device_extensions,
                enabled_features: DeviceFeatures {
                    dynamic_rendering: true,
                    image_view_format_swizzle: true,
                    ..DeviceFeatures::empty()
                },
                ..Default::default()
            },
        )
        .unwrap();

        let queue = queues.next().unwrap();

        let (memory_allocator, descriptor_set_allocator, command_buffer_allocator) =
            get_allocators(&device);

        let shaders_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("shaders");
        let render_pipeline =
            HotReloadComputePipeline::new(device.clone(), &shaders_dir.join("traverse.comp"));
        let resample_pipeline =
            HotReloadComputePipeline::new(device.clone(), &shaders_dir.join("resample.comp"));

        // Create the game world
        let world = create_game_world();
        let world_extent = [
            WORLD_SIZE_X as u32,
            WORLD_SIZE_Y as u32,
            WORLD_SIZE_Z as u32,
        ];
        let block_data = world_to_block_data(&world);

        let voxel_set = get_voxel_set(
            memory_allocator.clone(),
            command_buffer_allocator.clone(),
            descriptor_set_allocator.clone(),
            &render_pipeline,
            &queue,
            block_data,
            world_extent,
        );

        // Load texture atlas
        let texture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("textures")
            .join("texture_atlas.png");
        let (texture_set, _sampler) = load_texture_atlas(
            memory_allocator.clone(),
            command_buffer_allocator.clone(),
            descriptor_set_allocator.clone(),
            &render_pipeline,
            &queue,
            &texture_path,
        );

        // Create particle buffer and descriptor set
        let (particle_buffer, particle_set) = get_particle_set(
            memory_allocator.clone(),
            descriptor_set_allocator.clone(),
            &render_pipeline,
        );

        let input = WinitInputHelper::new();

        // Position camera - normalized coords (0-1) get scaled by world extent
        // Start player at center of terrain, above ground
        let mut camera = Camera::new(
            Vector3::new(0.5, 0.625, 0.5), // Center of world, will fall to ground
            Vector3::zeros(),
            INITIAL_WINDOW_RESOLUTION.into(),
            70.0,
        );
        camera.look_at(Vector3::new(0.5, 0.25, 0.75)); // Look forward

        println!(
            "Voxel Game started! Click to focus, then use WASD to move, mouse to look, left/right click to edit blocks."
        );

        App {
            instance,
            device,
            queue,

            memory_allocator,
            descriptor_set_allocator,
            command_buffer_allocator,

            render_pipeline,
            resample_pipeline,

            world,
            voxel_set,
            texture_set,
            particle_buffer,
            particle_set,
            world_extent,
            world_dirty: false,

            camera,
            render_mode: RenderMode::Normal,
            render_scale: 1.0,

            player_velocity: Vector3::zeros(),
            on_ground: false,
            in_water: false,
            fly_mode: false,
            show_chunk_boundaries: false,

            time_of_day: DEFAULT_TIME_OF_DAY,
            day_cycle_paused: true,
            animation_time: 0.0,

            selected_block: BlockType::Stone,
            current_hit: None,

            breaking_block: None,
            break_progress: 0.0,

            particles: ParticleSystem::new(),

            input,
            focused: false,
            pending_grab: None,
            last_second: Instant::now(),
            frames_since_last_second: 0,
            fps: 0,

            rcx: None,
        }
    }

    /// Gets the camera's forward direction vector.
    fn camera_direction(&self) -> Vector3<f64> {
        // Y-up coordinate system: forward is -Z (negative of column 2)
        -self.camera.rotation_matrix().column(2).xyz()
    }

    /// Gets the player's feet position in world coordinates.
    fn player_feet_pos(&self) -> Vector3<f64> {
        let scale = Vector3::new(
            self.world_extent[0] as f64,
            self.world_extent[1] as f64,
            self.world_extent[2] as f64,
        );
        let eye_pos = self.camera.position.component_mul(&scale);
        Vector3::new(eye_pos.x, eye_pos.y - PLAYER_EYE_HEIGHT, eye_pos.z)
    }

    /// Sets the player position from feet position (world coordinates).
    fn set_player_feet_pos(&mut self, feet_pos: Vector3<f64>) {
        let scale = Vector3::new(
            self.world_extent[0] as f64,
            self.world_extent[1] as f64,
            self.world_extent[2] as f64,
        );
        self.camera.position = Vector3::new(
            feet_pos.x / scale.x,
            (feet_pos.y + PLAYER_EYE_HEIGHT) / scale.y,
            feet_pos.z / scale.z,
        );
    }

    /// Checks if a block position is solid (not air, water, or other non-solid blocks).
    fn is_solid(&self, x: i32, y: i32, z: i32) -> bool {
        if x < 0 || y < 0 || z < 0 {
            return false; // Out of bounds = not solid (can fall out of world)
        }
        if x >= WORLD_SIZE_X as i32 || y >= WORLD_SIZE_Y as i32 || z >= WORLD_SIZE_Z as i32 {
            return false;
        }
        self.world
            .get_block(Vector3::new(x, y, z))
            .is_some_and(|b| b.is_solid())
    }

    /// Checks if the block at given position is water.
    fn is_water(&self, x: i32, y: i32, z: i32) -> bool {
        if x < 0 || y < 0 || z < 0 {
            return false;
        }
        if x >= WORLD_SIZE_X as i32 || y >= WORLD_SIZE_Y as i32 || z >= WORLD_SIZE_Z as i32 {
            return false;
        }
        self.world.get_block(Vector3::new(x, y, z)) == Some(BlockType::Water)
    }

    /// Checks if player's head is submerged in water.
    fn check_player_in_water(&self, feet_pos: Vector3<f64>) -> bool {
        // Check block at eye level (head position)
        let head_y = feet_pos.y + PLAYER_EYE_HEIGHT;
        let head_x = feet_pos.x.floor() as i32;
        let head_y_block = head_y.floor() as i32;
        let head_z = feet_pos.z.floor() as i32;
        self.is_water(head_x, head_y_block, head_z)
    }

    /// Checks if any part of the player's body is in water.
    fn check_player_touching_water(&self, feet_pos: Vector3<f64>) -> bool {
        // Check all blocks the player's body might occupy
        let min_x = (feet_pos.x - PLAYER_HALF_WIDTH).floor() as i32;
        let max_x = (feet_pos.x + PLAYER_HALF_WIDTH).floor() as i32;
        let min_y = (feet_pos.y - 0.1).floor() as i32; // Slightly below feet
        let max_y = (feet_pos.y + PLAYER_HEIGHT).floor() as i32;
        let min_z = (feet_pos.z - PLAYER_HALF_WIDTH).floor() as i32;
        let max_z = (feet_pos.z + PLAYER_HALF_WIDTH).floor() as i32;

        for bx in min_x..=max_x {
            for by in min_y..=max_y {
                for bz in min_z..=max_z {
                    if self.is_water(bx, by, bz) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Checks if player AABB collides with any solid blocks at given feet position.
    fn check_collision(&self, feet_pos: Vector3<f64>) -> bool {
        // Player AABB: centered on X/Z, extends from feet to feet+height on Y
        let min_x = (feet_pos.x - PLAYER_HALF_WIDTH).floor() as i32;
        let max_x = (feet_pos.x + PLAYER_HALF_WIDTH).floor() as i32;
        let min_y = feet_pos.y.floor() as i32;
        let max_y = (feet_pos.y + PLAYER_HEIGHT).floor() as i32;
        let min_z = (feet_pos.z - PLAYER_HALF_WIDTH).floor() as i32;
        let max_z = (feet_pos.z + PLAYER_HALF_WIDTH).floor() as i32;

        for bx in min_x..=max_x {
            for by in min_y..=max_y {
                for bz in min_z..=max_z {
                    if self.is_solid(bx, by, bz) {
                        // Check actual AABB overlap
                        let block_min = Vector3::new(bx as f64, by as f64, bz as f64);
                        let block_max = block_min + Vector3::new(1.0, 1.0, 1.0);
                        let player_min = Vector3::new(
                            feet_pos.x - PLAYER_HALF_WIDTH,
                            feet_pos.y,
                            feet_pos.z - PLAYER_HALF_WIDTH,
                        );
                        let player_max = Vector3::new(
                            feet_pos.x + PLAYER_HALF_WIDTH,
                            feet_pos.y + PLAYER_HEIGHT,
                            feet_pos.z + PLAYER_HALF_WIDTH,
                        );
                        // AABB overlap test
                        if player_min.x < block_max.x
                            && player_max.x > block_min.x
                            && player_min.y < block_max.y
                            && player_max.y > block_min.y
                            && player_min.z < block_max.z
                            && player_max.z > block_min.z
                        {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Updates player physics: applies gravity, handles movement, checks collisions.
    fn update_physics(&mut self, delta_time: f64) {
        let mut feet = self.player_feet_pos();

        // Check if player is in water
        let head_in_water = self.check_player_in_water(feet);
        let touching_water = self.check_player_touching_water(feet);
        self.in_water = head_in_water;

        // Get movement input
        let t = |k: KeyCode| self.input.key_held(k) as u8 as f64;
        let forward = t(KeyCode::KeyW) - t(KeyCode::KeyS);
        let right = t(KeyCode::KeyD) - t(KeyCode::KeyA);

        // Calculate horizontal movement direction (ignore pitch, only yaw)
        let yaw = self.camera.rotation.y;
        let move_dir = Vector3::new(
            -forward * yaw.sin() + right * yaw.cos(),
            0.0,
            -forward * yaw.cos() - right * yaw.sin(),
        );

        // Determine movement speed based on environment
        let current_speed = if touching_water {
            SWIM_SPEED
        } else {
            MOVE_SPEED
        };

        // Normalize and apply speed
        let move_len = move_dir.magnitude();
        if move_len > 0.001 {
            let normalized = move_dir / move_len;
            self.player_velocity.x = normalized.x * current_speed;
            self.player_velocity.z = normalized.z * current_speed;
        } else {
            self.player_velocity.x = 0.0;
            self.player_velocity.z = 0.0;
        }

        if self.fly_mode {
            // Fly mode: no gravity, E=up, Q=down for vertical movement
            let up = t(KeyCode::KeyE) - t(KeyCode::KeyQ);
            self.player_velocity.y = up * MOVE_SPEED;

            // Move without collision checks in fly mode
            feet.x += self.player_velocity.x * delta_time;
            feet.y += self.player_velocity.y * delta_time;
            feet.z += self.player_velocity.z * delta_time;

            // Clamp to world bounds
            feet.x = feet.x.clamp(0.5, WORLD_SIZE_X as f64 - 0.5);
            feet.y = feet.y.clamp(0.5, WORLD_SIZE_Y as f64 - 0.5);
            feet.z = feet.z.clamp(0.5, WORLD_SIZE_Z as f64 - 0.5);
        } else if touching_water {
            // Swimming mode: reduced gravity, buoyancy, vertical swim controls

            // Apply water physics: reduced gravity + buoyancy
            self.player_velocity.y -= WATER_GRAVITY * delta_time;
            self.player_velocity.y += WATER_BUOYANCY * delta_time;

            // Apply water drag to slow down
            let drag = WATER_DRAG.powf(delta_time);
            self.player_velocity.y *= drag;

            // Swim up with E, swim down with Q (same as fly mode)
            if self.input.key_held(KeyCode::KeyE) {
                self.player_velocity.y = SWIM_UP_SPEED;
            } else if self.input.key_held(KeyCode::KeyQ) {
                self.player_velocity.y = -SWIM_DOWN_SPEED;
            }

            // Move on each axis separately and check collisions
            // X axis
            let new_x = feet.x + self.player_velocity.x * delta_time;
            let test_pos = Vector3::new(new_x, feet.y, feet.z);
            if !self.check_collision(test_pos) {
                feet.x = new_x;
            } else {
                self.player_velocity.x = 0.0;
            }

            // Z axis
            let new_z = feet.z + self.player_velocity.z * delta_time;
            let test_pos = Vector3::new(feet.x, feet.y, new_z);
            if !self.check_collision(test_pos) {
                feet.z = new_z;
            } else {
                self.player_velocity.z = 0.0;
            }

            // Y axis
            let new_y = feet.y + self.player_velocity.y * delta_time;
            let test_pos = Vector3::new(feet.x, new_y, feet.z);
            if !self.check_collision(test_pos) {
                feet.y = new_y;
            } else {
                self.player_velocity.y = 0.0;
            }

            // Not on ground while swimming
            self.on_ground = false;
        } else {
            // Normal ground mode: apply gravity
            self.player_velocity.y -= GRAVITY * delta_time;

            // Jump
            if self.on_ground && self.input.key_pressed(KeyCode::Space) {
                self.player_velocity.y = JUMP_VELOCITY;
                self.on_ground = false;
            }

            // Move on each axis separately and check collisions
            // X axis
            let new_x = feet.x + self.player_velocity.x * delta_time;
            let test_pos = Vector3::new(new_x, feet.y, feet.z);
            if !self.check_collision(test_pos) {
                feet.x = new_x;
            } else {
                self.player_velocity.x = 0.0;
            }

            // Z axis
            let new_z = feet.z + self.player_velocity.z * delta_time;
            let test_pos = Vector3::new(feet.x, feet.y, new_z);
            if !self.check_collision(test_pos) {
                feet.z = new_z;
            } else {
                self.player_velocity.z = 0.0;
            }

            // Y axis
            let new_y = feet.y + self.player_velocity.y * delta_time;
            let test_pos = Vector3::new(feet.x, new_y, feet.z);
            if !self.check_collision(test_pos) {
                feet.y = new_y;
                self.on_ground = false;
            } else {
                // Hit something
                if self.player_velocity.y < 0.0 {
                    // Hit ground - snap to top of block
                    let ground_y = (feet.y + self.player_velocity.y * delta_time).floor() + 1.0;
                    feet.y = ground_y;
                    self.on_ground = true;
                }
                self.player_velocity.y = 0.0;
            }
        }

        // Update camera position
        self.set_player_feet_pos(feet);
    }

    /// Performs a raycast from the camera and updates the current hit.
    fn update_raycast(&mut self) {
        // Camera uses normalized coords (0-1), world uses voxel coords
        let scale = Vector3::new(
            self.world_extent[0] as f32,
            self.world_extent[1] as f32,
            self.world_extent[2] as f32,
        );
        let origin = self.camera.position.cast::<f32>().component_mul(&scale);
        let direction = self.camera_direction().cast::<f32>();

        self.current_hit = raycast(&self.world, origin, direction, MAX_RAYCAST_DISTANCE);
    }

    /// Updates block breaking progress while holding left mouse button.
    /// Returns true if a block was broken this frame.
    fn update_block_breaking(&mut self, delta_time: f32, holding_break: bool) -> bool {
        // Get the block we're looking at
        let target_block = self.current_hit.as_ref().map(|hit| hit.block_pos);

        // If not holding break button or not looking at anything, reset
        if !holding_break || target_block.is_none() {
            self.breaking_block = None;
            self.break_progress = 0.0;
            return false;
        }

        let target = target_block.unwrap();

        // If we're looking at a different block, reset progress
        if self.breaking_block != Some(target) {
            self.breaking_block = Some(target);
            self.break_progress = 0.0;
        }

        // Increment break progress
        self.break_progress += delta_time / BLOCK_BREAK_TIME;

        // Check if block is fully broken
        if self.break_progress >= 1.0 {
            // Bounds check
            if target.x >= 0
                && target.x < WORLD_SIZE_X as i32
                && target.y >= 0
                && target.y < WORLD_SIZE_Y as i32
                && target.z >= 0
                && target.z < WORLD_SIZE_Z as i32
            {
                // Get block color for particles before breaking
                if let Some(block_type) = self.world.get_block(target) {
                    let color = block_type.color();
                    let particle_color = nalgebra::Vector3::new(color[0], color[1], color[2]);
                    self.particles
                        .spawn_block_break(target.cast::<f32>(), particle_color);
                }

                self.world.set_block(target, BlockType::Air);
                self.world_dirty = true;
            }

            // Reset for next block
            self.breaking_block = None;
            self.break_progress = 0.0;
            return true;
        }

        false
    }

    /// Places a block adjacent to the one the player is looking at.
    fn place_block(&mut self) {
        if let Some(hit) = &self.current_hit {
            let place_pos = get_place_position(hit);
            // Bounds check
            if place_pos.x >= 0
                && place_pos.x < WORLD_SIZE_X as i32
                && place_pos.y >= 0
                && place_pos.y < WORLD_SIZE_Y as i32
                && place_pos.z >= 0
                && place_pos.z < WORLD_SIZE_Z as i32
            {
                // Don't place if it would be inside the player (convert camera to world coords)
                let scale = Vector3::new(
                    self.world_extent[0] as f32,
                    self.world_extent[1] as f32,
                    self.world_extent[2] as f32,
                );
                let player_pos = self.camera.position.cast::<f32>().component_mul(&scale);
                let block_center = place_pos.cast::<f32>() + Vector3::new(0.5, 0.5, 0.5);
                if (player_pos - block_center).norm() > 1.5 {
                    println!("Placing {:?} at {:?}", self.selected_block, place_pos);
                    self.world.set_block(place_pos, self.selected_block);
                    self.world_dirty = true;
                }
            } else {
                println!("Place position {:?} out of bounds, ignoring", place_pos);
            }
        }
    }

    /// Uploads the world voxels to the GPU if dirty.
    fn upload_world_to_gpu(&mut self) {
        if !self.world_dirty {
            return;
        }

        println!("Uploading world to GPU...");
        let block_data = world_to_block_data(&self.world);
        self.voxel_set = get_voxel_set(
            self.memory_allocator.clone(),
            self.command_buffer_allocator.clone(),
            self.descriptor_set_allocator.clone(),
            &self.render_pipeline,
            &self.queue,
            block_data,
            self.world_extent,
        );
        self.world_dirty = false;
        println!("GPU upload complete");
    }

    fn update(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if now.duration_since(self.last_second) > Duration::from_secs(1) {
            self.fps = self.frames_since_last_second;
            self.frames_since_last_second = 0;
            self.last_second = now;
        }
        self.frames_since_last_second += 1;

        let Some(delta_time) = self.input.delta_time().as_ref().map(Duration::as_secs_f64) else {
            return;
        };

        if self.input.close_requested() {
            event_loop.exit();
            return;
        }

        // Handle escape to unfocus
        if self.input.key_pressed(KeyCode::Escape) && self.focused {
            self.focused = false;
            self.pending_grab = Some(false);
            println!("Unfocused - cursor will be released");
        }

        // Handle focus toggling - click to focus (don't process this click for gameplay)
        if !self.focused && self.input.mouse_pressed(MouseButton::Left) {
            println!("Focus click...");
            self.focused = true;
            self.pending_grab = Some(true);
            println!("Focus complete - cursor will be grabbed");
            return;
        }

        // Update day/night cycle
        if !self.day_cycle_paused {
            self.time_of_day += delta_time as f32 / DAY_CYCLE_DURATION;
            self.time_of_day = self.time_of_day.rem_euclid(1.0);
        }

        // Update animation time (always advances for water waves, etc.)
        self.animation_time += delta_time as f32;

        // Update particle system with world collision
        let world = &self.world;
        let world_extent = self.world_extent;
        self.particles.update(delta_time as f32, |x, y, z| {
            // Bounds check
            if x < 0 || y < 0 || z < 0 {
                return false;
            }
            if x >= world_extent[0] as i32
                || y >= world_extent[1] as i32
                || z >= world_extent[2] as i32
            {
                return false;
            }
            world
                .get_block(Vector3::new(x, y, z))
                .is_some_and(|b| b.is_solid())
        });

        if self.focused {
            // Update player physics (movement, gravity, collisions)
            self.update_physics(delta_time);

            // Mouse look
            let sens = 0.002 * (self.camera.fov.to_radians() * 0.5).tan();

            let (dx, dy) = self.input.mouse_diff();
            // rotation.y = yaw (horizontal), rotation.x = pitch (vertical)
            self.camera.rotation.y -= dx as f64 * sens;
            self.camera.rotation.x -= dy as f64 * sens;
            self.camera.rotation.x = self.camera.rotation.x.clamp(-FRAC_PI_2, FRAC_PI_2);
            self.camera.rotation.y = self.camera.rotation.y.rem_euclid(TAU);

            let ds = self.input.scroll_diff();
            let tanfov = (self.camera.fov.to_radians() * 0.5).tan();
            self.camera.fov = ((tanfov * (ds.1 as f64 * -0.1).exp()).atan() * 2.0).to_degrees();

            // Block number keys to select block type
            if self.input.key_pressed(KeyCode::Digit1) {
                self.selected_block = BlockType::Stone;
            }
            if self.input.key_pressed(KeyCode::Digit2) {
                self.selected_block = BlockType::Dirt;
            }
            if self.input.key_pressed(KeyCode::Digit3) {
                self.selected_block = BlockType::Grass;
            }
            if self.input.key_pressed(KeyCode::Digit4) {
                self.selected_block = BlockType::Planks;
            }
            if self.input.key_pressed(KeyCode::Digit5) {
                self.selected_block = BlockType::Log;
            }
            if self.input.key_pressed(KeyCode::Digit6) {
                self.selected_block = BlockType::Leaves;
            }
            if self.input.key_pressed(KeyCode::Digit7) {
                self.selected_block = BlockType::Sand;
            }
            if self.input.key_pressed(KeyCode::Digit8) {
                self.selected_block = BlockType::Glass;
            }

            // Toggle fly mode (F key)
            if self.input.key_pressed(KeyCode::KeyF) {
                self.fly_mode = !self.fly_mode;
                if self.fly_mode {
                    println!("Fly mode: ON");
                } else {
                    println!("Fly mode: OFF");
                }
            }

            // Toggle chunk boundary debug (B key)
            if self.input.key_pressed(KeyCode::KeyB) {
                self.show_chunk_boundaries = !self.show_chunk_boundaries;
                if self.show_chunk_boundaries {
                    println!("Chunk boundaries: ON");
                } else {
                    println!("Chunk boundaries: OFF");
                }
            }

            // Block placing (instant on click)
            if self.input.mouse_pressed(MouseButton::Right) {
                self.place_block();
            }
        }

        // Update raycast for block selection
        self.update_raycast();

        // Block breaking (hold to break) - must be after raycast update
        if self.focused {
            let holding_break = self.input.mouse_held(MouseButton::Left);
            self.update_block_breaking(delta_time as f32, holding_break);
        } else {
            // Reset breaking if unfocused
            self.breaking_block = None;
            self.break_progress = 0.0;
        }

        // Upload dirty world data to GPU
        self.upload_world_to_gpu();
    }

    fn render(&mut self, _event_loop: &ActiveEventLoop) {
        self.render_pipeline.maybe_reload();
        self.resample_pipeline.maybe_reload();

        let rcx = self.rcx.as_mut().unwrap();

        if self.input.window_resized().is_some() {
            rcx.recreate_swapchain = true;
        }

        let window_size = rcx.window.inner_size();

        if window_size.width == 0 || window_size.height == 0 {
            return;
        }

        if rcx.recreate_swapchain {
            let images;
            (rcx.swapchain, images) = rcx
                .swapchain
                .recreate(SwapchainCreateInfo {
                    image_extent: window_size.into(),
                    ..rcx.swapchain.create_info()
                })
                .unwrap();

            rcx.image_views = images
                .iter()
                .map(|i| ImageView::new(i.clone(), ImageViewCreateInfo::from_image(i)).unwrap())
                .collect();

            let window_extent: [u32; 2] = window_size.into();
            let render_extent = [
                (window_extent[0] as f32 * self.render_scale) as u32,
                (window_extent[1] as f32 * self.render_scale) as u32,
            ];
            (
                rcx.render_image,
                rcx.render_set,
                rcx.resample_image,
                rcx.resample_set,
            ) = get_images_and_sets(
                self.memory_allocator.clone(),
                self.descriptor_set_allocator.clone(),
                &self.render_pipeline,
                &self.resample_pipeline,
                render_extent,
                window_extent,
            );

            rcx.recreate_swapchain = false;
        }

        let (image_index, suboptimal, acquire_future) =
            match acquire_next_image(rcx.swapchain.clone(), None).map_err(Validated::unwrap) {
                Ok(r) => r,
                Err(VulkanError::OutOfDate) => {
                    rcx.recreate_swapchain = true;
                    return;
                }
                Err(e) => panic!("failed to acquire next image: {e}"),
            };

        if suboptimal {
            rcx.recreate_swapchain = true;
        }

        rcx.gui.immediate_ui(|gui| {
            let ctx = gui.context();

            egui::Window::new("Voxel Game")
                .default_open(false)
                .show(&ctx, |ui| {
                    ui.label("Controls:");
                    ui.label("  WASD - Move");
                    ui.label("  Space - Jump");
                    ui.label("  QE - Down/Up (fly & swim)");
                    ui.label("  Mouse - Look around");
                    ui.label("  Scroll - Zoom");
                    ui.label("  F - Toggle fly mode");
                    ui.label("  B - Toggle chunk boundaries");
                    ui.label("  Left Click - Break block");
                    ui.label("  Right Click - Place block");
                    ui.label("  1-8 - Select block type");
                    ui.label("  Escape - Release cursor");
                    ui.separator();

                    ui.label(format!("FPS: {}", self.fps));
                    ui.label(format!("Chunks: {}", self.world.chunk_count()));
                    if self.in_water {
                        ui.colored_label(egui::Color32::from_rgb(100, 150, 255), "🌊 UNDERWATER");
                    }

                    ui.separator();

                    // Block selection
                    ui.label(format!("Selected: {:?}", self.selected_block));
                    if let Some(hit) = &self.current_hit {
                        ui.label(format!(
                            "Looking at: ({}, {}, {})",
                            hit.block_pos.x, hit.block_pos.y, hit.block_pos.z
                        ));
                        ui.label(format!("Distance: {:.1}", hit.distance));
                    } else {
                        ui.label("Looking at: (nothing)");
                    }

                    ui.separator();

                    // Debug render mode
                    ui.label("Render Mode:");
                    ui.horizontal(|ui| {
                        for &mode in RenderMode::ALL {
                            ui.selectable_value(&mut self.render_mode, mode, format!("{:?}", mode));
                        }
                    });

                    ui.separator();

                    ui.add(egui::Slider::new(&mut self.camera.fov, 20.0..=120.0).text("FOV"));

                    if ui
                        .add(
                            egui::Slider::new(&mut self.render_scale, 0.25..=2.0)
                                .text("Render Scale"),
                        )
                        .changed()
                    {
                        let window_extent: [u32; 2] = rcx.window.inner_size().into();
                        let render_extent = [
                            (window_extent[0] as f32 * self.render_scale) as u32,
                            (window_extent[1] as f32 * self.render_scale) as u32,
                        ];
                        (
                            rcx.render_image,
                            rcx.render_set,
                            rcx.resample_image,
                            rcx.resample_set,
                        ) = get_images_and_sets(
                            self.memory_allocator.clone(),
                            self.descriptor_set_allocator.clone(),
                            &self.render_pipeline,
                            &self.resample_pipeline,
                            render_extent,
                            window_extent,
                        );
                    }

                    ui.separator();

                    // Day/night cycle controls
                    ui.label("Day/Night Cycle:");
                    ui.checkbox(&mut self.day_cycle_paused, "Pause cycle");
                    let time_label = match (self.time_of_day * 4.0) as u32 {
                        0 => "Night",
                        1 => "Sunrise",
                        2 => "Day",
                        3 => "Sunset",
                        _ => "Day",
                    };
                    ui.add(
                        egui::Slider::new(&mut self.time_of_day, 0.0..=1.0)
                            .text(time_label)
                            .custom_formatter(|v, _| {
                                let hours = ((v * 24.0) + 6.0) % 24.0; // 0.0 = 6am, 0.5 = 6pm
                                let h = hours as u32;
                                let m = ((hours - h as f64) * 60.0) as u32;
                                format!("{:02}:{:02}", h, m)
                            }),
                    );

                    ui.separator();

                    // Camera position debug
                    ui.label(format!(
                        "Position: ({:.1}, {:.1}, {:.1})",
                        self.camera.position.x, self.camera.position.y, self.camera.position.z
                    ));
                });

            // Draw crosshair at screen center
            let screen_rect = ctx.screen_rect();
            let center = screen_rect.center();
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("crosshair"),
            ));
            let crosshair_size = 10.0;
            let crosshair_color = egui::Color32::WHITE;
            let stroke = egui::Stroke::new(2.0, crosshair_color);
            // Horizontal line
            painter.line_segment(
                [
                    egui::pos2(center.x - crosshair_size, center.y),
                    egui::pos2(center.x + crosshair_size, center.y),
                ],
                stroke,
            );
            // Vertical line
            painter.line_segment(
                [
                    egui::pos2(center.x, center.y - crosshair_size),
                    egui::pos2(center.x, center.y + crosshair_size),
                ],
                stroke,
            );
        });

        let render_extent = rcx.render_image.extent();
        let resample_extent = rcx.resample_image.extent();
        self.camera.extent = [render_extent[0] as f64, render_extent[1] as f64];

        let pixel_to_ray = self.camera.pixel_to_ray_matrix();

        // Scale only the position (column 4), not the direction (3x3 rotation part)
        // This prevents ray distortion from non-uniform world dimensions
        let mut pixel_to_ray_scaled = pixel_to_ray;
        pixel_to_ray_scaled.m14 *= self.world_extent[0] as f64;
        pixel_to_ray_scaled.m24 *= self.world_extent[1] as f64;
        pixel_to_ray_scaled.m34 *= self.world_extent[2] as f64;
        let pixel_to_ray = pixel_to_ray_scaled;

        #[derive(BufferContents)]
        #[repr(C)]
        struct PushConstants {
            pixel_to_ray: Matrix4<f32>,
            world_size_x: u32,
            world_size_y: u32,
            world_size_z: u32,
            render_mode: u32,
            show_chunk_boundaries: u32,
            player_in_water: u32,
            time_of_day: f32,
            animation_time: f32,
            // Block breaking info (-1 = no block being broken)
            break_block_x: i32,
            break_block_y: i32,
            break_block_z: i32,
            break_progress: f32,
            // Particle count
            particle_count: u32,
            // Block placement preview (-1 = no preview)
            preview_block_x: i32,
            preview_block_y: i32,
            preview_block_z: i32,
            preview_block_type: u32,
        }
        let (break_x, break_y, break_z) = self
            .breaking_block
            .map(|b| (b.x, b.y, b.z))
            .unwrap_or((-1, -1, -1));

        // Calculate preview block position (where block would be placed)
        let (preview_x, preview_y, preview_z, preview_type) = self
            .current_hit
            .as_ref()
            .map(|hit| {
                let place_pos = get_place_position(hit);
                // Only show preview if position is in bounds and not inside player
                let scale = Vector3::new(
                    self.world_extent[0] as f32,
                    self.world_extent[1] as f32,
                    self.world_extent[2] as f32,
                );
                let player_pos = self.camera.position.cast::<f32>().component_mul(&scale);
                let block_center = place_pos.cast::<f32>() + Vector3::new(0.5, 0.5, 0.5);
                let in_bounds = place_pos.x >= 0
                    && place_pos.x < WORLD_SIZE_X as i32
                    && place_pos.y >= 0
                    && place_pos.y < WORLD_SIZE_Y as i32
                    && place_pos.z >= 0
                    && place_pos.z < WORLD_SIZE_Z as i32;
                let not_in_player = (player_pos - block_center).norm() > 1.5;
                if in_bounds && not_in_player {
                    (
                        place_pos.x,
                        place_pos.y,
                        place_pos.z,
                        self.selected_block as u32,
                    )
                } else {
                    (-1, -1, -1, 0)
                }
            })
            .unwrap_or((-1, -1, -1, 0));

        // Update particle buffer
        let gpu_particles = self.particles.gpu_data();
        let particle_count = gpu_particles.len() as u32;
        {
            let mut write = self.particle_buffer.write().unwrap();
            for (i, p) in gpu_particles.iter().enumerate() {
                write[i] = *p;
            }
        }

        let push_constants = PushConstants {
            pixel_to_ray: pixel_to_ray.cast(),
            world_size_x: self.world_extent[0],
            world_size_y: self.world_extent[1],
            world_size_z: self.world_extent[2],
            render_mode: self.render_mode as u32,
            show_chunk_boundaries: self.show_chunk_boundaries as u32,
            player_in_water: self.in_water as u32,
            time_of_day: self.time_of_day,
            animation_time: self.animation_time,
            break_block_x: break_x,
            break_block_y: break_y,
            break_block_z: break_z,
            break_progress: self.break_progress,
            particle_count,
            preview_block_x: preview_x,
            preview_block_y: preview_y,
            preview_block_z: preview_z,
            preview_block_type: preview_type,
        };

        let mut builder = AutoCommandBufferBuilder::primary(
            self.command_buffer_allocator.clone(),
            self.queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )
        .unwrap();

        builder
            .clear_color_image(ClearColorImageInfo::image(rcx.render_image.clone()))
            .unwrap();

        builder
            .bind_pipeline_compute(self.render_pipeline.clone())
            .unwrap()
            .push_constants(self.render_pipeline.layout().clone(), 0, push_constants)
            .unwrap()
            .bind_descriptor_sets(
                PipelineBindPoint::Compute,
                self.render_pipeline.layout().clone(),
                0,
                vec![
                    rcx.render_set.clone(),
                    self.voxel_set.clone(),
                    self.texture_set.clone(),
                    self.particle_set.clone(),
                ],
            )
            .unwrap();
        unsafe {
            builder
                .dispatch([
                    render_extent[0].div_ceil(8),
                    render_extent[1].div_ceil(8),
                    1,
                ])
                .unwrap();
        }

        builder
            .bind_pipeline_compute(self.resample_pipeline.clone())
            .unwrap()
            .bind_descriptor_sets(
                PipelineBindPoint::Compute,
                self.resample_pipeline.layout().clone(),
                0,
                vec![rcx.resample_set.clone()],
            )
            .unwrap();

        unsafe {
            builder
                .dispatch([
                    resample_extent[0].div_ceil(8),
                    resample_extent[1].div_ceil(8),
                    1,
                ])
                .unwrap();
        }

        let mut info = BlitImageInfo::images(
            rcx.resample_image.clone(),
            rcx.image_views[image_index as usize].image().clone(),
        );
        info.filter = Filter::Nearest;
        builder.blit_image(info).unwrap();

        let command_buffer = builder.build().unwrap();

        let render_future = acquire_future
            .then_execute(self.queue.clone(), command_buffer)
            .unwrap();

        let gui_future = rcx
            .gui
            .draw_on_image(render_future, rcx.image_views[image_index as usize].clone());

        gui_future
            .then_swapchain_present(
                self.queue.clone(),
                SwapchainPresentInfo::swapchain_image_index(rcx.swapchain.clone(), image_index),
            )
            .then_signal_fence_and_flush()
            .unwrap()
            .wait(None)
            .unwrap();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_inner_size(INITIAL_WINDOW_RESOLUTION)
                        .with_window_icon(Some(load_icon(include_bytes!("../assets/icon.png"))))
                        .with_title("Voxel Ray Traversal"),
                )
                .unwrap(),
        );
        let surface = Surface::from_window(self.instance.clone(), window.clone()).unwrap();

        let (swapchain, images) = get_swapchain_images(&self.device, &surface, &window);
        let image_views = images
            .iter()
            .map(|i| ImageView::new(i.clone(), ImageViewCreateInfo::from_image(i)).unwrap())
            .collect::<Vec<_>>();

        let window_extent: [u32; 2] = window.inner_size().into();
        let render_extent = [
            (window_extent[0] as f32 * self.render_scale) as u32,
            (window_extent[1] as f32 * self.render_scale) as u32,
        ];
        let (render_image, render_set, resample_image, resample_set) = get_images_and_sets(
            self.memory_allocator.clone(),
            self.descriptor_set_allocator.clone(),
            &self.render_pipeline,
            &self.resample_pipeline,
            render_extent,
            window_extent,
        );

        let gui = Gui::new(
            event_loop,
            surface,
            self.queue.clone(),
            swapchain.image_format(),
            GuiConfig {
                is_overlay: true,
                ..Default::default()
            },
        );

        let recreate_swapchain = false;

        self.rcx = Some(RenderContext {
            window,
            swapchain,
            image_views,

            render_image,
            render_set,
            resample_image,
            resample_set,

            gui,

            recreate_swapchain,
        });
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, _cause: winit::event::StartCause) {
        self.input.step();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if !self.rcx.as_mut().unwrap().gui.update(&event) {
            self.input.process_window_event(&event);
        }

        if event == WindowEvent::RedrawRequested {
            self.render(event_loop);
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        self.input.process_device_event(&event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.input.end_step();
        self.update(event_loop);

        // Apply deferred cursor grab/release using native macOS APIs
        // winit's set_cursor_grab and set_cursor_visible cause SIGBUS on macOS
        if let Some(grab) = self.pending_grab.take() {
            if grab {
                macos_cursor::grab_and_hide();
                println!("Cursor grabbed and hidden (native macOS API)");
            } else {
                macos_cursor::release_and_show();
                println!("Cursor released and shown (native macOS API)");
            }
        }

        let rcx = self.rcx.as_mut().unwrap();
        rcx.window.request_redraw();
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new(&event_loop);
    event_loop.run_app(&mut app).unwrap();
}

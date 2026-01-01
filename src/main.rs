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

use clap::Parser;
use egui_winit_vulkano::{Gui, GuiConfig, egui};
use nalgebra::{Matrix4, Vector3, vector};

use std::path::PathBuf;
use std::{
    f64::consts::{FRAC_PI_2, TAU},
    sync::Arc,
    time::{Duration, Instant},
};
use vulkano::{
    Validated, VulkanError,
    buffer::{Buffer, BufferContents, BufferCreateInfo, BufferUsage, Subbuffer},
    command_buffer::{
        AutoCommandBufferBuilder, BlitImageInfo, BufferImageCopy, ClearColorImageInfo,
        CommandBufferUsage, CopyBufferToImageInfo, PrimaryCommandBufferAbstract,
    },
    descriptor_set::DescriptorSet,
    device::{Device, Queue},
    image::{
        Image,
        sampler::{Filter, SamplerAddressMode, SamplerCreateInfo},
        view::{ImageView, ImageViewCreateInfo},
    },
    instance::Instance,
    memory::allocator::{AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator},
    pipeline::{Pipeline, PipelineBindPoint},
    swapchain::{
        Surface, Swapchain, SwapchainCreateInfo, SwapchainPresentInfo, acquire_next_image,
    },
    sync::GpuFuture,
};
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::KeyCode,
    window::{Window, WindowId},
};
use winit_input_helper::WinitInputHelper;

mod block_update;
mod camera;
mod chunk;
mod chunk_loader;
mod config;
mod constants;
mod falling_block;
mod gpu_resources;
mod hot_reload;
mod hud;
mod particles;
mod player;
mod raycast;
mod render_mode;
mod sub_voxel;
mod sub_voxel_builtins;
mod svt;
mod terrain_gen;
mod utils;
mod vulkan_context;
mod water;
mod world;

use crate::block_update::{BlockUpdateQueue, BlockUpdateType};
use crate::chunk::{BlockType, CHUNK_SIZE};
use crate::chunk_loader::ChunkLoader;
use crate::config::{Args, INITIAL_WINDOW_RESOLUTION};
use crate::constants::{
    CHUNKS_PER_FRAME, EMPTY_CHUNK_DATA, EMPTY_MODEL_METADATA, LOADED_CHUNKS_X, LOADED_CHUNKS_Z,
    TEXTURE_SIZE_X, TEXTURE_SIZE_Y, TEXTURE_SIZE_Z, UNLOAD_DISTANCE, VIEW_DISTANCE, WORLD_CHUNKS_Y,
};
use crate::falling_block::{FallingBlockSystem, GpuFallingBlock};
use crate::gpu_resources::{
    GpuLight, MAX_LIGHTS, create_empty_voxel_texture, get_brick_and_model_set,
    get_chunk_metadata_set, get_distance_image_and_set, get_images_and_sets, get_light_set,
    get_particle_and_falling_block_set, get_swapchain_images, load_icon, load_texture_atlas,
    save_screenshot, update_brick_metadata, update_chunk_metadata, upload_chunks_batched,
};
use crate::hot_reload::HotReloadComputePipeline;
use crate::hud::Minimap;
use crate::particles::ParticleSystem;
use crate::player::{
    HEAD_BOB_AMPLITUDE, PLAYER_EYE_HEIGHT, PLAYER_HALF_WIDTH, PLAYER_HEIGHT, Player,
};
use crate::raycast::{MAX_RAYCAST_DISTANCE, RaycastHit, get_place_position, raycast};
use crate::render_mode::RenderMode;
use crate::sub_voxel::ModelRegistry;
use crate::terrain_gen::{TerrainGenerator, generate_chunk_terrain};
use crate::utils::{ChunkStats, Profiler};
use crate::vulkan_context::VulkanContext;
use crate::water::WaterGrid;
use crate::world::World;
use vulkano::command_buffer::allocator::StandardCommandBufferAllocator;
use vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator as StdDescriptorSetAllocator;

// Constants moved to constants.rs

// Player physics constants moved to player.rs

// Day/night cycle constants
/// Duration of a full day cycle in seconds (real time)
const DAY_CYCLE_DURATION: f32 = 120.0;
/// Default time of day (0.0 = 6am, 0.5 = 6pm, formula: hours = (v * 24 + 6) % 24)
/// 0.583 = 20:00 (8pm)
const DEFAULT_TIME_OF_DAY: f32 = 0.583;

/// Default blocks available in the hotbar (9 slots, keys 1-9)
const DEFAULT_HOTBAR_BLOCKS: [BlockType; 9] = [
    BlockType::Stone,
    BlockType::Dirt,
    BlockType::Grass,
    BlockType::Sand,
    BlockType::Log,
    BlockType::Model, // Fence (model_id=4)
    BlockType::Model, // Gate (model_id=20)
    BlockType::Model, // Ladder (model_id=29)
    BlockType::Model, // Torch model (model_id=1)
];

/// Default model IDs for Model blocks in hotbar.
/// 0 means the slot is not a Model block, non-zero is the model_id.
const DEFAULT_HOTBAR_MODEL_IDS: [u8; 9] = [
    0,  // Stone
    0,  // Dirt
    0,  // Grass
    0,  // Sand
    0,  // Log
    4,  // Fence (fence base model_id, connections computed dynamically)
    20, // Gate closed (gate base model_id, connections computed dynamically)
    29, // Ladder
    1,  // Torch
];

/// Finds the ground level (highest non-air block) at the given world coordinates.
fn find_ground_level(world: &World, world_x: i32, world_z: i32) -> i32 {
    // Search from top of world downward (Y dimension is still bounded)
    for y in (0..TEXTURE_SIZE_Y as i32).rev() {
        let pos = vector![world_x, y, world_z];
        if let Some(block) = world.get_block(pos) {
            if block != BlockType::Air && block != BlockType::Water {
                return y;
            }
        }
    }
    // Fallback to base height if nothing found
    32
}

/// Creates a world with only chunks near the spawn point loaded.
/// Additional chunks are loaded dynamically as the player moves.
fn create_initial_world_with_seed(spawn_chunk: Vector3<i32>, seed: u32) -> World {
    let mut world = World::new();
    let terrain = TerrainGenerator::new(seed);

    // Load chunks within horizontal view distance, all Y levels
    // Uses circular distance to match runtime loading behavior
    for dx in -VIEW_DISTANCE..=VIEW_DISTANCE {
        for dz in -VIEW_DISTANCE..=VIEW_DISTANCE {
            // Check horizontal distance (circular)
            let dist_sq = dx * dx + dz * dz;
            if dist_sq > VIEW_DISTANCE * VIEW_DISTANCE {
                continue;
            }

            let cx = spawn_chunk.x + dx;
            let cz = spawn_chunk.z + dz;

            // No horizontal bounds check - world is infinite in X/Z
            // Load ALL Y levels within this horizontal range
            for cy in 0..WORLD_CHUNKS_Y {
                let chunk_pos = vector![cx, cy, cz];
                let chunk = generate_chunk_terrain(&terrain, chunk_pos);
                world.insert_chunk(chunk_pos, chunk);
            }
        }
    }

    println!("Initial world created with {} chunks", world.chunk_count());
    world
}

/// Legacy function - kept for reference but no longer used
#[allow(dead_code)]
fn create_game_world_full() -> World {
    let mut world = World::new();
    let terrain = TerrainGenerator::new(42); // Fixed seed for reproducibility

    // Generate chunks within the loaded area (centered at origin for legacy mode)
    for cx in 0..LOADED_CHUNKS_X {
        for cy in 0..WORLD_CHUNKS_Y {
            for cz in 0..LOADED_CHUNKS_Z {
                let chunk_pos = vector![cx, cy, cz];
                let chunk = generate_chunk_terrain(&terrain, chunk_pos);
                world.insert_chunk(chunk_pos, chunk);
            }
        }
    }

    // Count non-air blocks
    let mut count = 0;
    for cx in 0..LOADED_CHUNKS_X {
        for cy in 0..WORLD_CHUNKS_Y {
            for cz in 0..LOADED_CHUNKS_Z {
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
        TEXTURE_SIZE_X,
        TEXTURE_SIZE_Y,
        TEXTURE_SIZE_Z,
        LOADED_CHUNKS_X * WORLD_CHUNKS_Y * LOADED_CHUNKS_Z,
        count
    );

    world
}

struct App {
    instance: Arc<Instance>,
    device: Arc<Device>,
    queue: Arc<Queue>,

    memory_allocator: Arc<StandardMemoryAllocator>,
    descriptor_set_allocator: Arc<StdDescriptorSetAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,

    render_pipeline: HotReloadComputePipeline,
    resample_pipeline: HotReloadComputePipeline,

    /// The game world containing all chunks.
    world: World,
    /// GPU descriptor set for voxel data.
    voxel_set: Arc<DescriptorSet>,
    /// GPU descriptor set for texture atlas.
    texture_set: Arc<DescriptorSet>,
    /// Texture atlas image view (for egui HUD).
    texture_atlas_view: Arc<ImageView>,
    /// GPU buffer for particle data.
    particle_buffer: Subbuffer<[particles::GpuParticle]>,
    /// GPU descriptor set for particles.
    particle_set: Arc<DescriptorSet>,
    /// GPU buffer for point light data.
    light_buffer: Subbuffer<[GpuLight]>,
    /// GPU descriptor set for lights.
    light_set: Arc<DescriptorSet>,
    /// GPU buffer for chunk metadata (empty/solid flags).
    chunk_metadata_buffer: Subbuffer<[u32]>,
    /// GPU descriptor set for chunk metadata.
    chunk_metadata_set: Arc<DescriptorSet>,
    /// GPU buffer for brick masks (64 bits per chunk, which of 64 bricks are solid).
    brick_mask_buffer: Subbuffer<[u32]>,
    /// GPU buffer for brick distances (distance to nearest solid brick, per brick).
    brick_dist_buffer: Subbuffer<[u32]>,
    /// GPU descriptor set for brick metadata AND model resources (combined set 7).
    brick_and_model_set: Arc<DescriptorSet>,
    /// GPU buffer for falling block data (shares descriptor set with particles).
    falling_block_buffer: Subbuffer<[GpuFallingBlock]>,
    /// World dimensions in blocks [X, Y, Z].
    world_extent: [u32; 3],

    /// Sub-voxel model registry.
    model_registry: ModelRegistry,
    /// GPU 3D texture for model atlas (8³ voxels per model).
    #[allow(dead_code)] // Held for GPU lifetime
    model_atlas: Arc<Image>,
    /// GPU 3D texture for model metadata (model_id + rotation per block).
    model_metadata: Arc<Image>,

    player: Player,
    render_mode: RenderMode,
    render_scale: f32,
    /// Current window size for debug output
    window_size: [u32; 2],

    /// Show debug chunk boundary wireframes
    show_chunk_boundaries: bool,
    /// Show block placement preview
    show_block_preview: bool,
    /// Show target block outline (wireframe around block player is looking at)
    show_target_outline: bool,

    // Minimap settings
    /// Whether to show the minimap
    show_minimap: bool,
    /// Minimap component
    minimap: Minimap,
    /// Cached minimap image for reuse between frames
    minimap_cached_image: Option<egui::ColorImage>,
    /// Last player position for minimap update throttling
    minimap_last_pos: Vector3<i32>,
    /// Last minimap update time for rate limiting
    minimap_last_update: Instant,
    /// Last player yaw for rotation-based updates
    minimap_last_yaw: f32,

    // Compass settings
    /// Whether to show the compass
    show_compass: bool,

    // Performance profiling toggles
    /// Enable ambient occlusion
    enable_ao: bool,
    /// Enable sun shadow rays
    enable_shadows: bool,
    /// Enable model participation in sun shadows
    enable_model_shadows: bool,
    /// Enable point lights (torches)
    enable_point_lights: bool,

    // LOD distance thresholds (0 = use shader defaults)
    /// Distance for AO calculations (default 32)
    lod_ao_distance: f32,
    /// Distance for shadow rays (default 64)
    lod_shadow_distance: f32,
    /// Distance for point light calculations (default 24)
    lod_point_light_distance: f32,

    /// Current time of day (0.0 = midnight, 0.5 = noon, 1.0 = midnight)
    time_of_day: f32,
    /// Whether the day/night cycle is paused
    day_cycle_paused: bool,
    /// Base ambient light level (0.0 = pitch black, 1.0 = fully lit)
    ambient_light: f32,
    /// Fog density (0.0 = no fog, higher = thicker fog)
    fog_density: f32,
    /// Distance where fog starts (blocks)
    fog_start: f32,
    /// Whether fog affects the sky (false = clear sky regardless of fog)
    fog_affects_sky: bool,
    /// Maximum ray marching steps (higher = see farther, lower = better FPS)
    max_ray_steps: u32,
    /// Continuous animation time in seconds (for water waves, etc.)
    animation_time: f32,

    /// Chunk streaming: last chunk position the player was in.
    last_player_chunk: Vector3<i32>,
    /// The GPU 3D texture storing voxel data (for partial updates).
    voxel_image: Arc<Image>,
    /// Texture origin in world blocks - the world position that maps to texture coord (0,0,0).
    /// This moves as the player explores, enabling infinite worlds.
    texture_origin: Vector3<i32>,
    /// Current chunk statistics for HUD display.
    chunk_stats: ChunkStats,
    /// Async chunk loader for background terrain generation.
    chunk_loader: ChunkLoader,

    /// Currently selected hotbar slot (0-8).
    hotbar_index: usize,
    /// Blocks in the hotbar (can be modified by middle-click picker).
    hotbar_blocks: [BlockType; 9],
    /// Model IDs for Model blocks in hotbar (0 = not a model block).
    hotbar_model_ids: [u8; 9],
    /// Current raycast hit result (for crosshair display).
    current_hit: Option<RaycastHit>,

    /// Block currently being broken (position).
    breaking_block: Option<Vector3<i32>>,
    /// Progress of breaking current block (0.0 to 1.0).
    break_progress: f32,
    /// Whether blocks break instantly on click (no hold required).
    instant_break: bool,
    /// Cooldown timer after breaking a block in instant mode (seconds remaining).
    break_cooldown: f32,
    /// Configurable cooldown duration for breaking blocks (seconds).
    break_cooldown_duration: f32,
    /// Skip block breaking until mouse is released (used to ignore focus click).
    skip_break_until_release: bool,

    /// Last position where a block was placed (for continuous placing).
    last_place_pos: Option<Vector3<i32>>,
    /// Cooldown timer for continuous block placing.
    place_cooldown: f32,
    /// Configurable cooldown duration for placing blocks (seconds).
    place_cooldown_duration: f32,
    /// Whether we need to release right-click before placing another model.
    model_needs_reclick: bool,
    /// Whether we need to release left-click before toggling another gate.
    gate_needs_reclick: bool,
    /// Starting position for line building (first block in the line).
    line_start_pos: Option<Vector3<i32>>,
    /// Locked axis for line building: 0=X, 1=Y, 2=Z, None=not locked yet.
    line_locked_axis: Option<u8>,

    /// Particle system for visual effects.
    particles: ParticleSystem,
    /// Falling block system for gravity-affected blocks.
    falling_blocks: FallingBlockSystem,
    /// Queue for frame-distributed block physics updates.
    block_updates: BlockUpdateQueue,
    /// Water flow simulation grid.
    water_grid: WaterGrid,
    /// Whether water simulation is enabled.
    water_simulation_enabled: bool,

    input: WinitInputHelper,
    focused: bool,
    /// Deferred cursor grab change (workaround for macOS crash).
    /// true = grab and hide, false = release and show
    pending_grab: Option<bool>,
    last_second: Instant,
    frames_since_last_second: u32,
    fps: u32,

    /// Command-line arguments.
    args: Args,
    /// Start time for screenshot delay.
    start_time: Instant,
    /// Whether screenshot has been taken yet.
    screenshot_taken: bool,
    /// Total frame count since start (for debug interval).
    total_frames: u64,
    /// Performance profiler for timing operations.
    profiler: Profiler,
    /// View distance in chunks (adjustable via slider)
    view_distance: i32,
    /// Unload distance in chunks (adjustable via slider)
    unload_distance: i32,

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

    /// Distance buffer for two-pass beam optimization (1/4 resolution)
    distance_image: Arc<Image>,
    distance_set: Arc<DescriptorSet>,

    gui: Gui,
    /// Texture ID for the atlas in egui.
    atlas_texture_id: egui::TextureId,

    recreate_swapchain: bool,
}

impl App {
    /// Returns the currently selected block from the hotbar.
    fn selected_block(&self) -> BlockType {
        self.hotbar_blocks[self.hotbar_index]
    }

    fn new(event_loop: &EventLoop<()>) -> Self {
        // Parse command line arguments
        let args = Args::parse();

        if args.verbose {
            println!("CLI Args: {:?}", args);
        }

        let seed = args.seed.unwrap_or(12345);
        let view_distance = args.view_distance.unwrap_or(VIEW_DISTANCE);
        let unload_distance = UNLOAD_DISTANCE;

        let vk = VulkanContext::new(event_loop);

        let shaders_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("shaders");
        let render_pipeline =
            HotReloadComputePipeline::new(vk.device.clone(), &shaders_dir.join("traverse.comp"));
        let resample_pipeline =
            HotReloadComputePipeline::new(vk.device.clone(), &shaders_dir.join("resample.comp"));

        // Calculate spawn chunk based on CLI args (or default to origin)
        let spawn_block_x = args.spawn_x.unwrap_or(0);
        let spawn_block_z = args.spawn_z.unwrap_or(0);
        let spawn_chunk = vector![
            spawn_block_x.div_euclid(CHUNK_SIZE as i32),
            0,
            spawn_block_z.div_euclid(CHUNK_SIZE as i32)
        ];

        if args.verbose {
            println!(
                "Spawn at block ({}, {}), chunk ({}, {})",
                spawn_block_x, spawn_block_z, spawn_chunk.x, spawn_chunk.z
            );
        }

        // Texture origin: the world position that maps to texture coordinate (0,0,0)
        // For infinite worlds, center the texture on the spawn chunk
        let texture_origin = Vector3::new(
            (spawn_chunk.x - LOADED_CHUNKS_X / 2) * CHUNK_SIZE as i32,
            0, // Y always starts at 0
            (spawn_chunk.z - LOADED_CHUNKS_Z / 2) * CHUNK_SIZE as i32,
        );

        if args.verbose {
            println!(
                "Texture origin: ({}, {}, {})",
                texture_origin.x, texture_origin.y, texture_origin.z
            );
        }

        // Create world with only chunks near spawn loaded
        let world = create_initial_world_with_seed(spawn_chunk, seed);

        // Texture dimensions (not world bounds - world is infinite)
        let world_extent = [
            TEXTURE_SIZE_X as u32,
            TEXTURE_SIZE_Y as u32,
            TEXTURE_SIZE_Z as u32,
        ];

        // Create empty GPU texture (chunks will be uploaded by update_chunk_loading)
        let (voxel_set, voxel_image) = create_empty_voxel_texture(
            vk.memory_allocator.clone(),
            vk.command_buffer_allocator.clone(),
            vk.descriptor_set_allocator.clone(),
            &render_pipeline,
            &vk.queue,
            world_extent,
        );

        // Load texture atlas
        let texture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("textures")
            .join("texture_atlas.png");
        let (texture_set, _sampler, texture_atlas_view) = load_texture_atlas(
            vk.memory_allocator.clone(),
            vk.command_buffer_allocator.clone(),
            vk.descriptor_set_allocator.clone(),
            &render_pipeline,
            &vk.queue,
            &texture_path,
        );

        // Create particle and falling block buffers (share set 3)
        let (particle_buffer, falling_block_buffer, particle_set) =
            get_particle_and_falling_block_set(
                vk.memory_allocator.clone(),
                vk.descriptor_set_allocator.clone(),
                &render_pipeline,
            );

        // Create light buffer and descriptor set
        let (light_buffer, light_set) = get_light_set(
            vk.memory_allocator.clone(),
            vk.descriptor_set_allocator.clone(),
            &render_pipeline,
        );

        // Create chunk metadata buffer and descriptor set
        let (chunk_metadata_buffer, chunk_metadata_set) = get_chunk_metadata_set(
            vk.memory_allocator.clone(),
            vk.descriptor_set_allocator.clone(),
            &render_pipeline,
        );

        // Create model registry with built-in models
        let model_registry = ModelRegistry::new();

        // Create combined brick metadata and model resources (set 7)
        let (
            brick_mask_buffer,
            brick_dist_buffer,
            model_atlas,
            model_metadata,
            brick_and_model_set,
        ) = get_brick_and_model_set(
            vk.memory_allocator.clone(),
            vk.command_buffer_allocator.clone(),
            vk.descriptor_set_allocator.clone(),
            &render_pipeline,
            &vk.queue,
            world_extent,
            &model_registry,
        );

        let input = WinitInputHelper::new();

        // Spawn at world origin (0, ground_level, 0) for infinite worlds
        let spawn_x = 0;
        let spawn_z = 0;
        let spawn_y = find_ground_level(&world, spawn_x, spawn_z);
        let spawn_pos = Vector3::new(spawn_x as f64, spawn_y as f64 + 1.0, spawn_z as f64);

        let mut player = Player::new(spawn_pos, texture_origin, world_extent, args.fly_mode);
        player.auto_jump = true;

        println!(
            "Voxel Game started! Click to focus, then use WASD to move, mouse to look, left/right click to edit blocks."
        );

        App {
            instance: vk.instance,
            device: vk.device,
            queue: vk.queue,

            memory_allocator: vk.memory_allocator,
            descriptor_set_allocator: vk.descriptor_set_allocator,
            command_buffer_allocator: vk.command_buffer_allocator,

            render_pipeline,
            resample_pipeline,

            world,
            voxel_set,
            texture_set,
            texture_atlas_view,
            particle_buffer,
            particle_set,
            light_buffer,
            light_set,
            chunk_metadata_buffer,
            chunk_metadata_set,
            brick_mask_buffer,
            brick_dist_buffer,
            brick_and_model_set,
            falling_block_buffer,
            world_extent,

            model_registry,
            model_atlas,
            model_metadata,

            player,
            render_mode: match args.render_mode.as_deref() {
                Some("normal") => RenderMode::Normal,
                Some("coord") => RenderMode::Coord,
                Some("steps") => RenderMode::Steps,
                Some("uv") => RenderMode::UV,
                Some("depth") => RenderMode::Depth,
                _ => RenderMode::Textured,
            },
            render_scale: 0.75, // Balance between quality and FPS, upscaled to window
            window_size: INITIAL_WINDOW_RESOLUTION.into(),

            show_chunk_boundaries: args.show_chunk_boundaries,
            show_block_preview: false,  // Off by default
            show_target_outline: false, // Off by default (toggle in UI)

            // Minimap - disabled by default, toggle with M
            show_minimap: false,
            minimap: Minimap::new(),
            minimap_cached_image: None,
            minimap_last_pos: Vector3::new(i32::MAX, 0, i32::MAX), // Force initial update
            minimap_last_update: Instant::now(),
            minimap_last_yaw: f32::MAX, // Force initial update

            // Compass - enabled by default
            show_compass: true,

            // Performance toggles - all enabled by default
            enable_ao: true,
            enable_shadows: true,
            enable_model_shadows: true,
            enable_point_lights: true,

            // LOD distances - use more aggressive defaults for better performance
            // Set to 0 to use shader defaults (32, 64, 24)
            lod_ao_distance: 24.0,          // Reduced from 32
            lod_shadow_distance: 48.0,      // Reduced from 64
            lod_point_light_distance: 20.0, // Reduced from 24

            time_of_day: args
                .time_of_day
                .map(|t| t as f32)
                .unwrap_or(DEFAULT_TIME_OF_DAY),
            day_cycle_paused: true, // Day cycle paused by default
            ambient_light: 0.1,
            fog_density: 0.01,
            fog_start: 128.0,
            fog_affects_sky: false,
            max_ray_steps: 256,
            animation_time: 0.0,

            last_player_chunk: spawn_chunk,
            voxel_image,
            texture_origin,
            chunk_stats: ChunkStats::default(),
            chunk_loader: {
                let terrain = TerrainGenerator::new(seed);
                ChunkLoader::new(move |pos| generate_chunk_terrain(&terrain, pos))
            },

            hotbar_index: 0,
            hotbar_blocks: DEFAULT_HOTBAR_BLOCKS,
            hotbar_model_ids: DEFAULT_HOTBAR_MODEL_IDS,
            current_hit: None,

            breaking_block: None,
            break_progress: 0.0,
            instant_break: true,
            break_cooldown: 0.0,
            break_cooldown_duration: 0.1,
            skip_break_until_release: false,

            last_place_pos: None,
            place_cooldown: 0.0,
            place_cooldown_duration: 0.1,
            model_needs_reclick: false,
            gate_needs_reclick: false,
            line_start_pos: None,
            line_locked_axis: None,

            particles: ParticleSystem::new(),
            falling_blocks: FallingBlockSystem::new(),
            block_updates: BlockUpdateQueue::new(32),
            water_grid: WaterGrid::new(),
            water_simulation_enabled: true,

            input,
            focused: false,
            pending_grab: None,
            last_second: Instant::now(),
            frames_since_last_second: 0,
            fps: 0,

            args,
            start_time: Instant::now(),
            screenshot_taken: false,
            total_frames: 0,
            profiler: Profiler::default(),
            view_distance,
            unload_distance,

            rcx: None,
        }
    }

    /// Checks if texture origin needs to shift and handles re-upload if necessary.
    /// Returns true if a shift occurred.
    fn check_and_shift_texture_origin(&mut self) -> bool {
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
        // Camera position is in normalized texture-relative coords:
        //   camera.position = (world_pos - texture_origin) / scale
        // When texture_origin changes, we need to adjust camera.position:
        //   new_camera = old_camera + (old_origin - new_origin) / scale
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

    /// Clears the entire voxel texture to air.
    fn clear_voxel_texture(&self) {
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

    /// Updates chunk loading/unloading based on player position.
    /// Uses async chunk generation - queues chunks for background generation
    /// and uploads completed chunks to GPU.
    /// Returns (chunks_loaded, chunks_unloaded) counts.
    fn update_chunk_loading(&mut self) -> (usize, usize) {
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
        // We can queue more than CHUNKS_PER_FRAME since generation is async
        let max_to_queue = CHUNKS_PER_FRAME * 4; // Allow larger batches since it's non-blocking
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

    /// Performs a raycast from the camera and updates the current hit.
    fn update_raycast(&mut self) {
        // Camera uses normalized texture-relative coords (0-1), raycast needs world coords
        let scale = Vector3::new(
            self.world_extent[0] as f32,
            self.world_extent[1] as f32,
            self.world_extent[2] as f32,
        );
        // Convert camera position from normalized texture coords to texture coords
        let texture_pos = self
            .player
            .camera
            .position
            .cast::<f32>()
            .component_mul(&scale);
        // Convert texture coords to world coords by adding texture_origin
        let origin = Vector3::new(
            texture_pos.x + self.texture_origin.x as f32,
            texture_pos.y + self.texture_origin.y as f32,
            texture_pos.z + self.texture_origin.z as f32,
        );
        let direction = self.player.camera_direction().cast::<f32>();

        self.current_hit = raycast(&self.world, origin, direction, MAX_RAYCAST_DISTANCE);
    }

    /// Updates block breaking progress while holding left mouse button.
    /// Returns true if a block was broken this frame.
    fn update_block_breaking(&mut self, delta_time: f32, holding_break: bool) -> bool {
        // Decrement cooldown timer
        if self.break_cooldown > 0.0 {
            self.break_cooldown -= delta_time;
            if self.break_cooldown < 0.0 {
                self.break_cooldown = 0.0;
            }
        }

        // Get the block we're looking at
        let target_block = self.current_hit.as_ref().map(|hit| hit.block_pos);

        // If not holding break button or not looking at anything, reset
        if !holding_break || target_block.is_none() {
            self.breaking_block = None;
            self.break_progress = 0.0;
            return false;
        }

        // Don't start breaking if on cooldown (instant break mode)
        if self.instant_break && self.break_cooldown > 0.0 {
            return false;
        }

        let target = target_block.unwrap();

        // Get the block type to determine break time
        let block_type = self.world.get_block(target).unwrap_or(BlockType::Air);
        let break_time = block_type.break_time();

        // Can't break air or water
        if break_time <= 0.0 {
            self.breaking_block = None;
            self.break_progress = 0.0;
            return false;
        }

        // If we're looking at a different block, reset progress
        if self.breaking_block != Some(target) {
            self.breaking_block = Some(target);
            self.break_progress = 0.0;
        }

        // Increment break progress (instant if enabled)
        if self.instant_break {
            self.break_progress = 1.0;
        } else {
            self.break_progress += delta_time / break_time;
        }

        // Check if block is fully broken
        if self.break_progress >= 1.0 {
            // Bounds check (Y only - X/Z are infinite)
            if target.y >= 0 && target.y < TEXTURE_SIZE_Y as i32 {
                // Get block color for particles before breaking
                if let Some(block_type) = self.world.get_block(target) {
                    let color = block_type.color();
                    let particle_color = nalgebra::Vector3::new(color[0], color[1], color[2]);
                    self.particles
                        .spawn_block_break(target.cast::<f32>(), particle_color);
                }

                self.world.set_block(target, BlockType::Air);
                self.world.invalidate_minimap_cache(target.x, target.z);

                // Update neighboring fence/gate connections
                self.world.update_fence_connections(target);

                // Notify water grid that a block was removed (may trigger flow)
                self.water_grid.on_block_removed(target);

                // Check if any adjacent terrain water should start flowing
                self.water_grid
                    .activate_adjacent_terrain_water(&self.world, target);

                // Queue physics checks (frame-distributed to prevent FPS spikes)
                let player_pos = self
                    .player
                    .feet_pos(self.world_extent, self.texture_origin)
                    .cast::<f32>();

                // Queue gravity check for block above
                self.block_updates.enqueue(
                    target + Vector3::new(0, 1, 0),
                    BlockUpdateType::Gravity,
                    player_pos,
                );

                // Queue ground support check for model block above (fences, torches, gates)
                self.block_updates.enqueue(
                    target + Vector3::new(0, 1, 0),
                    BlockUpdateType::ModelGroundSupport,
                    player_pos,
                );

                // Queue tree support checks for all nearby logs
                if block_type.is_log() {
                    self.block_updates.enqueue_neighbors(
                        target,
                        BlockUpdateType::TreeSupport,
                        player_pos,
                    );
                }
                self.block_updates.enqueue_radius(
                    target,
                    3,
                    BlockUpdateType::TreeSupport,
                    player_pos,
                );

                // Queue orphaned leaves checks
                self.block_updates.enqueue_radius(
                    target,
                    4,
                    BlockUpdateType::OrphanedLeaves,
                    player_pos,
                );
            }

            // Reset for next block
            self.breaking_block = None;
            self.break_progress = 0.0;

            // Set cooldown for instant break mode
            if self.instant_break {
                self.break_cooldown = self.break_cooldown_duration;
            }

            return true;
        }

        false
    }

    /// Toggles a gate between open and closed states.
    /// Returns true if a gate was toggled.
    fn toggle_gate_at(&mut self, pos: Vector3<i32>) -> bool {
        // Check if target is a Model block
        let Some(BlockType::Model) = self.world.get_block(pos) else {
            return false;
        };

        // Get model data
        let Some(model_data) = self.world.get_model_data(pos) else {
            return false;
        };

        let model_id = model_data.model_id;
        let rotation = model_data.rotation;

        // Check if it's a gate (closed: 20-23, open: 24-27)
        if !(20..=27).contains(&model_id) {
            return false;
        }

        // Calculate new model_id (toggle between closed and open)
        let new_model_id = if model_id < 24 {
            // Closed -> Open: add 4
            model_id + 4
        } else {
            // Open -> Closed: subtract 4
            model_id - 4
        };

        // Update the gate
        self.world.set_model_block(pos, new_model_id, rotation);

        true
    }

    /// Updates block placing - allows continuous placing while holding right mouse button.
    /// Implements line-locking: once a direction is established, blocks are placed along that axis.
    fn update_block_placing(&mut self, delta_time: f32) {
        // Decrease cooldown
        if self.place_cooldown > 0.0 {
            self.place_cooldown -= delta_time;
        }

        let holding_place = self.input.mouse_held(MouseButton::Right);

        if !holding_place {
            // Reset all line building state when mouse released
            self.last_place_pos = None;
            self.line_start_pos = None;
            self.line_locked_axis = None;
            self.model_needs_reclick = false; // Allow model placement on next click
            self.gate_needs_reclick = false; // Allow gate toggle on next click
            return;
        }

        // Check if right-clicking on a gate - toggle it instead of placing
        if let Some(hit) = &self.current_hit {
            // Check if target is a gate
            if let Some(BlockType::Model) = self.world.get_block(hit.block_pos) {
                if let Some(model_data) = self.world.get_model_data(hit.block_pos) {
                    if (20..=27).contains(&model_data.model_id) {
                        // It's a gate - toggle if not already toggled this click
                        if !self.gate_needs_reclick {
                            self.toggle_gate_at(hit.block_pos);
                            self.gate_needs_reclick = true;
                        }
                        return; // Don't place a block when targeting a gate
                    }
                }
            }
        }

        // Get current target position
        let Some(raw_place_pos) = self.current_hit.as_ref().map(get_place_position) else {
            return;
        };

        // Apply line-locking constraint if axis is locked
        let constrained_pos =
            if let (Some(start), Some(axis)) = (self.line_start_pos, self.line_locked_axis) {
                // Constrain position to the locked axis
                let mut pos = raw_place_pos;
                match axis {
                    0 => {
                        // X-axis locked: Y and Z must match start
                        pos.y = start.y;
                        pos.z = start.z;
                    }
                    1 => {
                        // Y-axis locked: X and Z must match start
                        pos.x = start.x;
                        pos.z = start.z;
                    }
                    _ => {
                        // Z-axis locked: X and Y must match start
                        pos.x = start.x;
                        pos.y = start.y;
                    }
                }
                pos
            } else {
                raw_place_pos
            };

        // Check if we should place a block
        let is_model_block = self.selected_block() == BlockType::Model;
        let should_place = if self.last_place_pos.is_none() {
            // First click - no cooldown needed, but check model reclick
            !is_model_block || !self.model_needs_reclick
        } else {
            // Must wait for cooldown, and check model reclick
            self.place_cooldown <= 0.0 && (!is_model_block || !self.model_needs_reclick)
        };

        if should_place && self.place_block_at(constrained_pos) {
            // Model blocks require releasing and re-clicking
            if is_model_block {
                self.model_needs_reclick = true;
            }
            // First block: set as line start
            if self.line_start_pos.is_none() {
                self.line_start_pos = Some(constrained_pos);
            }
            // Second block: detect and lock the axis
            else if self.line_locked_axis.is_none() {
                if let Some(start) = self.line_start_pos {
                    let dx = (constrained_pos.x - start.x).abs();
                    let dy = (constrained_pos.y - start.y).abs();
                    let dz = (constrained_pos.z - start.z).abs();
                    // Lock to the axis with the largest movement
                    if dx >= dy && dx >= dz && dx > 0 {
                        self.line_locked_axis = Some(0); // X
                    } else if dy >= dx && dy >= dz && dy > 0 {
                        self.line_locked_axis = Some(1); // Y
                    } else if dz > 0 {
                        self.line_locked_axis = Some(2); // Z
                    }
                }
            }

            self.last_place_pos = Some(constrained_pos);
            self.place_cooldown = self.place_cooldown_duration;
        }
    }

    /// Places a block at a specific position (used for line-locked building).
    fn place_block_at(&mut self, place_pos: Vector3<i32>) -> bool {
        // Bounds check (Y only, X/Z are infinite)
        if place_pos.y < 0 || place_pos.y >= TEXTURE_SIZE_Y as i32 {
            return false;
        }

        // Check if block would overlap with player hitbox (AABB collision)
        let feet = self.player.feet_pos(self.world_extent, self.texture_origin);
        let player_min = Vector3::new(
            feet.x - PLAYER_HALF_WIDTH,
            feet.y,
            feet.z - PLAYER_HALF_WIDTH,
        );
        let player_max = Vector3::new(
            feet.x + PLAYER_HALF_WIDTH,
            feet.y + PLAYER_HEIGHT,
            feet.z + PLAYER_HALF_WIDTH,
        );
        let block_min = place_pos.cast::<f64>();
        let block_max = block_min + Vector3::new(1.0, 1.0, 1.0);

        // AABB overlap check
        let overlaps = player_min.x < block_max.x
            && player_max.x > block_min.x
            && player_min.y < block_max.y
            && player_max.y > block_min.y
            && player_min.z < block_max.z
            && player_max.z > block_min.z;

        if overlaps {
            return false; // Can't place block inside player
        }

        // Check if target position already has a block
        if let Some(existing) = self.world.get_block(place_pos) {
            if existing != BlockType::Air {
                return false; // Can't place on non-air
            }
        }

        println!("Placing {:?} at {:?}", self.selected_block(), place_pos);
        let block_to_place = self.selected_block();

        // Handle model blocks specially - set both block type and metadata
        if block_to_place == BlockType::Model {
            let base_model_id = self.hotbar_model_ids[self.hotbar_index];
            let rotation = 0u8;

            // Determine final model_id based on type and connections
            let model_id = if ModelRegistry::is_fence_model(base_model_id)
                || (4..20).contains(&base_model_id)
            {
                // Fence: calculate connections and get correct variant
                let connections = self.world.calculate_fence_connections(place_pos);
                ModelRegistry::fence_model_id(connections)
            } else if ModelRegistry::is_gate_model(base_model_id)
                || (20..28).contains(&base_model_id)
            {
                // Gate: auto-detect orientation based on neighboring fences
                // Check E/W neighbors vs N/S neighbors to determine orientation
                let has_west = self
                    .world
                    .is_fence_connectable(place_pos + Vector3::new(-1, 0, 0));
                let has_east = self
                    .world
                    .is_fence_connectable(place_pos + Vector3::new(1, 0, 0));
                let has_north = self
                    .world
                    .is_fence_connectable(place_pos + Vector3::new(0, 0, -1));
                let has_south = self
                    .world
                    .is_fence_connectable(place_pos + Vector3::new(0, 0, 1));

                // Calculate player position relative to gate for open direction
                let player_pos = self.player.feet_pos(self.world_extent, self.texture_origin);
                let gate_center = place_pos.cast::<f64>() + Vector3::new(0.5, 0.0, 0.5);
                let to_player = player_pos - gate_center;

                // Determine fence line orientation and rotation for gate to open toward player
                // Gate model: doors swing toward -Z in model space
                // rotation=0: swing -Z, rotation=1: swing +X, rotation=2: swing +Z, rotation=3: swing -X
                let (connections, rotation) = if (has_north || has_south) && !has_west && !has_east
                {
                    // N/S fence line - gate spans Z axis
                    // Player to the west (-X): rotation=1 (doors swing toward player)
                    // Player to the east (+X): rotation=3 (doors swing toward player)
                    let mut conn = 0u8;
                    if has_north {
                        conn |= 1;
                    }
                    if has_south {
                        conn |= 2;
                    }
                    let rot = if to_player.x < 0.0 { 1u8 } else { 3u8 };
                    (conn, rot)
                } else {
                    // E/W fence line or no clear preference - gate spans X axis
                    // Player to the north (-Z): rotation=0 (doors swing -Z toward player)
                    // Player to the south (+Z): rotation=2 (doors swing +Z toward player)
                    let connections = self.world.calculate_gate_connections(place_pos);
                    let rot = if to_player.z < 0.0 { 0u8 } else { 2u8 };
                    (connections, rot)
                };
                // Use closed gate by default (base_model_id 20-23)
                // Store rotation for the gate
                self.world.set_model_block(
                    place_pos,
                    ModelRegistry::gate_closed_model_id(connections),
                    rotation,
                );

                // Update neighboring fences/gates
                self.world.update_fence_connections(place_pos);
                // Skip the normal set_model_block below since we already did it
                return true;
            } else if ModelRegistry::is_ladder_model(base_model_id) {
                // Ladder: auto-orient to face the player
                // The ladder model has rungs at Z=7, so rotation determines which wall it faces
                // rotation=0: against +Z wall (facing -Z toward player)
                // rotation=1: against +X wall (facing -X)
                // rotation=2: against -Z wall (facing +Z)
                // rotation=3: against -X wall (facing +X)
                let player_pos = self.player.feet_pos(self.world_extent, self.texture_origin);
                let ladder_center = place_pos.cast::<f64>() + Vector3::new(0.5, 0.0, 0.5);
                let to_player = player_pos - ladder_center;

                // Determine which direction the player is from the ladder
                // Ladder should face the player (player climbs from the front)
                let rotation = if to_player.x.abs() > to_player.z.abs() {
                    // Player is more to E/W
                    if to_player.x > 0.0 { 3 } else { 1 } // Face +X or -X
                } else {
                    // Player is more to N/S
                    if to_player.z > 0.0 { 2 } else { 0 } // Face +Z or -Z
                };

                self.world
                    .set_model_block(place_pos, base_model_id, rotation);
                return true;
            } else {
                // Other model types (torch, slab, etc.) - use as-is
                base_model_id
            };

            self.world.set_model_block(place_pos, model_id, rotation);

            // Update neighboring fences/gates if we placed a fence-connectable block
            if ModelRegistry::is_fence_or_gate(model_id) {
                self.world.update_fence_connections(place_pos);
            }
        } else {
            self.world.set_block(place_pos, block_to_place);

            // Solid blocks can also connect to fences
            if block_to_place.is_solid() {
                self.world.update_fence_connections(place_pos);
            }
        }
        self.world
            .invalidate_minimap_cache(place_pos.x, place_pos.z);

        // Update water grid based on what was placed
        if block_to_place == BlockType::Water {
            // Player-placed water becomes a source (infinite water)
            self.water_grid.place_source(place_pos);
        } else {
            // Solid block placed - removes any water at this position
            self.water_grid.on_block_placed(place_pos);
        }

        true
    }

    /// Processes blocks that have landed and places them in the world.
    /// Handles multiple blocks landing at the same X,Z by stacking them.
    fn process_landed_blocks(&mut self, mut landed: Vec<crate::falling_block::LandedBlock>) {
        // Sort by Y position (lowest first) so blocks stack properly
        landed.sort_by_key(|lb| lb.position.y);

        for lb in landed {
            // Bounds check
            if lb.position.y >= 0 && lb.position.y < TEXTURE_SIZE_Y as i32 {
                // Find the actual landing position by checking what's already there
                let mut place_y = lb.position.y;
                while place_y < TEXTURE_SIZE_Y as i32 {
                    let check_pos = Vector3::new(lb.position.x, place_y, lb.position.z);
                    if let Some(existing) = self.world.get_block(check_pos) {
                        if existing == BlockType::Air {
                            // Found empty spot, place here
                            break;
                        }
                    }
                    place_y += 1;
                }

                // Place the block if within bounds
                if place_y < TEXTURE_SIZE_Y as i32 {
                    let final_pos = Vector3::new(lb.position.x, place_y, lb.position.z);
                    self.world.set_block(final_pos, lb.block_type);
                    self.world
                        .invalidate_minimap_cache(final_pos.x, final_pos.z);

                    // Queue gravity check for chain reaction (blocks above might now fall)
                    let player_pos = self
                        .player
                        .feet_pos(self.world_extent, self.texture_origin)
                        .cast::<f32>();
                    self.block_updates.enqueue(
                        final_pos + Vector3::new(0, 1, 0),
                        BlockUpdateType::Gravity,
                        player_pos,
                    );
                }
            }
        }
    }

    /// Takes a screenshot and saves it to the specified path.
    /// Uploads dirty chunks to the GPU.
    fn upload_world_to_gpu(&mut self) {
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

    /// Uploads all dirty chunks to GPU at once (used for initial world load).
    fn upload_all_dirty_chunks(&mut self) {
        let chunks_to_upload: Vec<(Vector3<i32>, Vec<u8>, Vec<u8>)> = self
            .world
            .chunks()
            .filter(|(_, chunk)| chunk.dirty)
            .map(|(pos, chunk)| (*pos, chunk.to_block_data(), chunk.to_model_metadata()))
            .collect();

        if chunks_to_upload.is_empty() {
            return;
        }

        println!(
            "Uploading {} initial chunks to GPU...",
            chunks_to_upload.len()
        );

        // Upload all at once - convert to slice references
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

        // Mark all as clean
        for (pos, _, _) in &chunks_to_upload {
            if let Some(chunk) = self.world.get_chunk_mut(*pos) {
                chunk.mark_clean();
            }
        }

        // Clear the dirty queue
        self.world.drain_dirty_chunks();

        println!("Initial chunk upload complete.");

        // Update chunk and brick metadata after initial upload
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

    /// Clears a chunk region in the GPU 3D texture (fills with air).
    /// Note: Prefer using upload_chunks_batched with empty data for better performance.
    #[allow(dead_code)]
    fn clear_chunk_in_gpu(&self, chunk_pos: Vector3<i32>) {
        // Calculate image offset for this chunk
        let offset = [
            (chunk_pos.x * CHUNK_SIZE as i32) as u32,
            (chunk_pos.y * CHUNK_SIZE as i32) as u32,
            (chunk_pos.z * CHUNK_SIZE as i32) as u32,
        ];

        // Create buffer filled with zeros (air)
        let block_data = vec![0u8; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE];
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
            block_data,
        )
        .unwrap();

        // Build command buffer for region copy
        let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
            self.command_buffer_allocator.clone(),
            self.queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )
        .unwrap();

        // Copy zeros to specific region in 3D texture
        let region = BufferImageCopy {
            buffer_offset: 0,
            buffer_row_length: CHUNK_SIZE as u32,
            buffer_image_height: CHUNK_SIZE as u32,
            image_subresource: self.voxel_image.subresource_layers(),
            image_offset: offset,
            image_extent: [CHUNK_SIZE as u32, CHUNK_SIZE as u32, CHUNK_SIZE as u32],
            ..Default::default()
        };

        command_buffer_builder
            .copy_buffer_to_image(CopyBufferToImageInfo {
                regions: [region].into(),
                ..CopyBufferToImageInfo::buffer_image(src_buffer, self.voxel_image.clone())
            })
            .unwrap();

        // Execute and wait
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

    /// Uploads dirty chunks to the GPU using batched upload.
    /// Returns the number of chunks uploaded.
    #[allow(dead_code)]
    fn upload_dirty_chunks(&mut self) -> usize {
        let mut chunks_to_upload: Vec<(Vector3<i32>, Vec<u8>, Vec<u8>)> = Vec::new();
        let mut dirty_positions: Vec<Vector3<i32>> = Vec::new();

        // Collect dirty chunks from all loaded chunks
        for (chunk_pos, chunk) in self.world.chunks() {
            if chunk.dirty {
                chunks_to_upload.push((
                    *chunk_pos,
                    chunk.to_block_data(),
                    chunk.to_model_metadata(),
                ));
                dirty_positions.push(*chunk_pos);
                if chunks_to_upload.len() >= CHUNKS_PER_FRAME {
                    break;
                }
            }
        }

        let uploaded = chunks_to_upload.len();

        // Batch upload
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

            // Mark as clean
            for pos in dirty_positions {
                if let Some(chunk) = self.world.get_chunk_mut(pos) {
                    chunk.mark_clean();
                }
            }

            // Update chunk and brick metadata since chunks may have changed empty status
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

        uploaded
    }

    /// Collects all light-emitting blocks (including model blocks like torches)
    /// and returns them as GPU light data.
    fn collect_torch_lights(&self) -> Vec<GpuLight> {
        let mut lights = Vec::new();

        // Add player light if enabled (like holding a torch)
        if self.player.light_enabled {
            let player_pos = self.player.feet_pos(self.world_extent, self.texture_origin);
            // Light is at player's hand/chest level, convert to texture coordinates for shader
            let tex_x = (player_pos.x - self.texture_origin.x as f64) as f32;
            let tex_y =
                (player_pos.y + PLAYER_EYE_HEIGHT * 0.7 - self.texture_origin.y as f64) as f32;
            let tex_z = (player_pos.z - self.texture_origin.z as f64) as f32;
            lights.push(GpuLight {
                pos_radius: [tex_x, tex_y, tex_z, 12.0], // Torch-like radius
                color_intensity: [1.0, 0.8, 0.5, 1.5],   // Warm torch color
            });
        }

        // Iterate over all loaded chunks
        for (chunk_pos, chunk) in self.world.chunks() {
            // Scan chunk for light-emitting blocks
            for lx in 0..CHUNK_SIZE {
                for ly in 0..CHUNK_SIZE {
                    for lz in 0..CHUNK_SIZE {
                        let block = chunk.get_block(lx, ly, lz);

                        // Check for Model blocks with emission
                        if block == BlockType::Model {
                            if let Some(model_data) = chunk.get_model_data(lx, ly, lz) {
                                if let Some(model) = self.model_registry.get(model_data.model_id) {
                                    if let Some(emission) = &model.emission {
                                        // Calculate world position (center of block)
                                        let world_x = chunk_pos.x * CHUNK_SIZE as i32 + lx as i32;
                                        let world_y = chunk_pos.y * CHUNK_SIZE as i32 + ly as i32;
                                        let world_z = chunk_pos.z * CHUNK_SIZE as i32 + lz as i32;

                                        // Convert to texture coordinates
                                        let tex_x = (world_x - self.texture_origin.x) as f32 + 0.5;
                                        let tex_y = (world_y - self.texture_origin.y) as f32 + 0.5;
                                        let tex_z = (world_z - self.texture_origin.z) as f32 + 0.5;

                                        let r = emission.r as f32 / 255.0;
                                        let g = emission.g as f32 / 255.0;
                                        let b = emission.b as f32 / 255.0;

                                        lights.push(GpuLight {
                                            pos_radius: [tex_x, tex_y, tex_z, 10.0], // Torch radius
                                            color_intensity: [r, g, b, 1.2],
                                        });

                                        if lights.len() >= MAX_LIGHTS {
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
                            let tex_x = (world_x - self.texture_origin.x) as f32 + 0.5;
                            let tex_y = (world_y - self.texture_origin.y) as f32 + 0.5;
                            let tex_z = (world_z - self.texture_origin.z) as f32 + 0.5;

                            lights.push(GpuLight {
                                pos_radius: [tex_x, tex_y, tex_z, radius],
                                color_intensity: [color[0], color[1], color[2], 1.2],
                            });

                            if lights.len() >= MAX_LIGHTS {
                                return lights;
                            }
                        }
                    }
                }
            }
        }

        lights
    }

    fn update(&mut self, event_loop: &ActiveEventLoop) {
        self.total_frames += 1;
        let now = Instant::now();

        // Check for screenshot delay
        if let Some(delay) = self.args.screenshot_delay {
            if !self.screenshot_taken {
                let elapsed = now.duration_since(self.start_time).as_secs_f64();
                if elapsed >= delay {
                    // Mark that we need to take a screenshot on the next render
                    // The actual screenshot will be taken in render()
                    println!(
                        "[SCREENSHOT] Taking screenshot after {:.1}s (saving to voxel_world_screen_shot.png)",
                        elapsed
                    );
                }
            }
        }

        if now.duration_since(self.last_second) > Duration::from_secs(1) {
            self.fps = self.frames_since_last_second;
            self.frames_since_last_second = 0;
            self.last_second = now;

            // Debug stats output (always show if verbose, otherwise once per second)
            let player_pos = self.player.feet_pos(self.world_extent, self.texture_origin);
            let player_chunk = self
                .player
                .get_chunk_pos(self.world_extent, self.texture_origin);
            let frame_time_ms = if self.fps > 0 {
                1000.0 / self.fps as f32
            } else {
                0.0
            };

            let render_res = [
                (self.window_size[0] as f32 * self.render_scale) as u32,
                (self.window_size[1] as f32 * self.render_scale) as u32,
            ];
            if self.args.verbose {
                println!(
                    "[STATS] FPS: {} ({:.1}ms) | Win: {}x{} Render: {}x{} | Chunks: {} | Dirty: {} | Gen: {} | Pos: ({:.1}, {:.1}, {:.1}) | Chunk: ({}, {}, {}) | TexOrigin: ({}, {})",
                    self.fps,
                    frame_time_ms,
                    self.window_size[0],
                    self.window_size[1],
                    render_res[0],
                    render_res[1],
                    self.chunk_stats.loaded_count,
                    self.chunk_stats.dirty_count,
                    self.chunk_stats.in_flight_count,
                    player_pos.x,
                    player_pos.y,
                    player_pos.z,
                    player_chunk.x,
                    player_chunk.y,
                    player_chunk.z,
                    self.texture_origin.x,
                    self.texture_origin.z,
                );
            } else {
                println!(
                    "[STATS] FPS: {} ({:.1}ms) | Win: {}x{} Render: {}x{} | Chunks: {} | Gen: {} | Pos: ({:.1}, {:.1}, {:.1})",
                    self.fps,
                    frame_time_ms,
                    self.window_size[0],
                    self.window_size[1],
                    render_res[0],
                    render_res[1],
                    self.chunk_stats.loaded_count,
                    self.chunk_stats.in_flight_count,
                    player_pos.x,
                    player_pos.y,
                    player_pos.z,
                );
            }

            // Print profiler stats and reset
            self.profiler.print_stats();
            self.profiler.reset();
        }
        self.frames_since_last_second += 1;

        // Debug interval output
        if self.args.debug_interval > 0 && self.total_frames % self.args.debug_interval as u64 == 0
        {
            let player_pos = self.player.feet_pos(self.world_extent, self.texture_origin);
            let player_chunk = self
                .player
                .get_chunk_pos(self.world_extent, self.texture_origin);
            println!(
                "[DEBUG Frame {}] Pos: ({:.2}, {:.2}, {:.2}) Chunk: ({}, {}, {}) TexOrigin: ({}, {}, {}) Velocity: ({:.2}, {:.2}, {:.2})",
                self.total_frames,
                player_pos.x,
                player_pos.y,
                player_pos.z,
                player_chunk.x,
                player_chunk.y,
                player_chunk.z,
                self.texture_origin.x,
                self.texture_origin.y,
                self.texture_origin.z,
                self.player.velocity.x,
                self.player.velocity.y,
                self.player.velocity.z
            );
        }

        // Always update chunks and upload to GPU, even before delta_time is available
        // This ensures initial chunks are uploaded on the first frame
        let t0 = Instant::now();
        self.update_chunk_loading();
        self.profiler.chunk_loading_us += t0.elapsed().as_micros() as u64;

        let t1 = Instant::now();
        self.upload_world_to_gpu();
        self.profiler.gpu_upload_us += t1.elapsed().as_micros() as u64;

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
            // Skip block breaking until mouse is released to avoid breaking on focus click
            self.skip_break_until_release = true;
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
        // Note: X and Z can be any value in an infinite world, only Y has bounds
        let world = &self.world;
        self.particles.update(delta_time as f32, |x, y, z| {
            // Y bounds check only (X and Z are infinite)
            if y < 0 || y >= TEXTURE_SIZE_Y as i32 {
                return false;
            }
            // Check if block is solid - world.get_block handles infinite X/Z
            world
                .get_block(Vector3::new(x, y, z))
                .is_some_and(|b| b.is_solid())
        });

        // Update falling blocks with world collision
        // Note: X and Z can be any value in an infinite world, only Y has bounds
        let landed = self.falling_blocks.update(delta_time as f32, |x, y, z| {
            // Y bounds check only (X and Z are infinite)
            if y < 0 || y >= TEXTURE_SIZE_Y as i32 {
                return false;
            }
            // Check if block is solid - world.get_block handles infinite X/Z
            world
                .get_block(Vector3::new(x, y, z))
                .is_some_and(|b| b.is_solid())
        });

        // Process any blocks that have landed
        if !landed.is_empty() {
            self.process_landed_blocks(landed);
        }

        // Process queued block physics updates (frame-distributed to prevent FPS spikes)
        let player_pos_f32 = self
            .player
            .feet_pos(self.world_extent, self.texture_origin)
            .cast::<f32>();
        self.block_updates.process_updates(
            &mut self.world,
            &mut self.falling_blocks,
            &mut self.particles,
            &self.model_registry,
            player_pos_f32,
        );

        // Process water flow simulation (frame-distributed)
        if self.water_simulation_enabled {
            let player_pos_f32 = self
                .player
                .feet_pos(self.world_extent, self.texture_origin)
                .cast::<f32>();
            self.water_grid
                .process_simulation(&mut self.world, player_pos_f32);
        }

        if self.focused {
            // Update player physics (movement, gravity, collisions)
            self.player.update_physics(
                delta_time,
                &self.world,
                self.world_extent,
                self.texture_origin,
                &self.input,
                &self.model_registry,
                self.args.verbose,
            );

            // Mouse look
            let sens = 0.002 * (self.player.camera.fov.to_radians() * 0.5).tan();

            let (dx, dy) = self.input.mouse_diff();
            // rotation.y = yaw (horizontal), rotation.x = pitch (vertical)
            self.player.camera.rotation.y -= dx as f64 * sens;
            self.player.camera.rotation.x -= dy as f64 * sens;
            self.player.camera.rotation.x =
                self.player.camera.rotation.x.clamp(-FRAC_PI_2, FRAC_PI_2);
            self.player.camera.rotation.y = self.player.camera.rotation.y.rem_euclid(TAU);

            // Scroll wheel to cycle through hotbar slots
            let ds = self.input.scroll_diff();
            if ds.1.abs() > 0.1 {
                self.hotbar_index = if ds.1 > 0.0 {
                    (self.hotbar_index + self.hotbar_blocks.len() - 1) % self.hotbar_blocks.len()
                } else {
                    (self.hotbar_index + 1) % self.hotbar_blocks.len()
                };
            }

            // Number keys 1-9 to select hotbar slot
            if self.input.key_pressed(KeyCode::Digit1) {
                self.hotbar_index = 0;
            }
            if self.input.key_pressed(KeyCode::Digit2) {
                self.hotbar_index = 1;
            }
            if self.input.key_pressed(KeyCode::Digit3) {
                self.hotbar_index = 2;
            }
            if self.input.key_pressed(KeyCode::Digit4) {
                self.hotbar_index = 3;
            }
            if self.input.key_pressed(KeyCode::Digit5) {
                self.hotbar_index = 4;
            }
            if self.input.key_pressed(KeyCode::Digit6) {
                self.hotbar_index = 5;
            }
            if self.input.key_pressed(KeyCode::Digit7) {
                self.hotbar_index = 6;
            }
            if self.input.key_pressed(KeyCode::Digit8) {
                self.hotbar_index = 7;
            }
            if self.input.key_pressed(KeyCode::Digit9) {
                self.hotbar_index = 8;
            }

            // Toggle fly mode (F key)
            if self.input.key_pressed(KeyCode::KeyF) {
                self.player.fly_mode = !self.player.fly_mode;
                if self.player.fly_mode {
                    println!("Fly mode: ON");
                } else {
                    println!("Fly mode: OFF");
                }
            }

            // Toggle sprint mode (Left Control)
            if self.input.key_pressed(KeyCode::ControlLeft) {
                self.player.sprint_mode = !self.player.sprint_mode;
                if self.player.sprint_mode {
                    println!("Sprint mode: ON");
                } else {
                    println!("Sprint mode: OFF");
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

            // Toggle minimap (M key)
            if self.input.key_pressed(KeyCode::KeyM) {
                self.show_minimap = !self.show_minimap;
                println!("Minimap: {}", if self.show_minimap { "ON" } else { "OFF" });
            }

            // Block placing - continuous when holding right mouse button
            self.update_block_placing(delta_time as f32);
        }

        // Update raycast for block selection
        self.update_raycast();

        // Block breaking (hold to break) - must be after raycast update
        if self.focused {
            let holding_break = self.input.mouse_held(MouseButton::Left);

            // Clear skip flag when mouse is released
            if self.skip_break_until_release && !holding_break {
                self.skip_break_until_release = false;
            }

            // Skip block breaking until mouse is released after focusing
            if !self.skip_break_until_release {
                self.update_block_breaking(delta_time as f32, holding_break);
            }
        } else {
            // Reset breaking if unfocused
            self.breaking_block = None;
            self.break_progress = 0.0;
        }

        // Middle-click block picker: pick block type under cursor
        if self.focused && self.input.mouse_pressed(MouseButton::Middle) {
            if let Some(hit) = self.current_hit {
                if let Some(block_type) = self.world.get_block(hit.block_pos) {
                    if block_type != BlockType::Air {
                        // Check if block type is already in hotbar
                        if let Some(idx) = self.hotbar_blocks.iter().position(|&b| b == block_type)
                        {
                            // Switch to that slot
                            self.hotbar_index = idx;
                            println!("Picked {:?} (slot {})", block_type, idx + 1);
                        } else {
                            // Replace current slot with the picked block
                            self.hotbar_blocks[self.hotbar_index] = block_type;
                            println!(
                                "Replaced slot {} with {:?}",
                                self.hotbar_index + 1,
                                block_type
                            );
                        }
                    }
                }
            }
        }
    }

    fn render(&mut self, _event_loop: &ActiveEventLoop) {
        let t_render_start = Instant::now();
        self.render_pipeline.maybe_reload();
        self.resample_pipeline.maybe_reload();

        // Collect data before borrowing rcx (avoids borrow checker issues)
        let gpu_lights = self.collect_torch_lights();
        let light_count = gpu_lights.len() as u32;
        let player_world_pos = self.player.feet_pos(self.world_extent, self.texture_origin);
        let selected_block = self.selected_block();
        let hotbar_index = self.hotbar_index;
        let hotbar_blocks = self.hotbar_blocks;
        let hotbar_model_ids = self.hotbar_model_ids;

        // Pre-generate minimap image if showing (before entering gui closure)
        // Throttle updates based on position change and rotation change
        let camera_yaw = self.player.camera.rotation.y as f32;
        let minimap_image: Option<egui::ColorImage> = if self.show_minimap {
            let current_pos = Vector3::new(
                player_world_pos.x.floor() as i32,
                player_world_pos.y.floor() as i32,
                player_world_pos.z.floor() as i32,
            );
            // Check if player moved at least 1 block
            let moved = (current_pos.x - self.minimap_last_pos.x).abs() >= 1
                || (current_pos.z - self.minimap_last_pos.z).abs() >= 1;
            // Check if player rotated significantly (5 degrees) - only matters when rotate mode is on
            let yaw_changed =
                self.minimap.rotate && (camera_yaw - self.minimap_last_yaw).abs() > 0.087; // ~5 degrees
            // Check if enough time has passed (0.1 seconds for rotation, 0.5 for position)
            let time_elapsed = self.minimap_last_update.elapsed().as_secs_f32();
            let time_ok = if self.minimap.rotate {
                time_elapsed >= 0.1 // Faster updates for rotation
            } else {
                time_elapsed >= 0.5
            };

            if ((moved || yaw_changed) && time_ok) || self.minimap_cached_image.is_none() {
                // Update last position/time/yaw and regenerate
                self.minimap_last_pos = current_pos;
                self.minimap_last_update = Instant::now();
                self.minimap_last_yaw = camera_yaw;
                let image =
                    self.world
                        .generate_minimap_image(player_world_pos, camera_yaw, &self.minimap);
                self.minimap_cached_image = Some(image.clone());
                Some(image)
            } else {
                // Use cached image
                self.minimap_cached_image.clone()
            }
        } else {
            None
        };
        // Get minimap settings for HUD
        let show_minimap = self.show_minimap;
        let minimap_size = self.minimap.size;
        let minimap_rotate = self.minimap.rotate;
        let show_compass = self.show_compass;

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
            self.window_size = window_extent;
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

            // Recreate distance buffer for two-pass beam optimization
            (rcx.distance_image, rcx.distance_set) = get_distance_image_and_set(
                self.memory_allocator.clone(),
                self.descriptor_set_allocator.clone(),
                &self.render_pipeline,
                render_extent,
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

        // Get atlas texture id before borrowing gui
        let atlas_texture_id = rcx.atlas_texture_id;

        rcx.gui.immediate_ui(|gui| {
            let ctx = gui.context();

            // FPS counter and chunk stats in top right corner
            egui::Area::new(egui::Id::new("fps_overlay"))
                .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 10.0))
                .show(&ctx, |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180))
                        .corner_radius(egui::CornerRadius::same(4))
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.set_min_width(100.0);
                            ui.label(
                                egui::RichText::new(format!("FPS: {}", self.fps))
                                    .color(egui::Color32::WHITE)
                                    .strong(),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "Chunks: {}",
                                    self.chunk_stats.loaded_count
                                ))
                                .color(egui::Color32::LIGHT_GRAY)
                                .small(),
                            );
                            if self.chunk_stats.dirty_count > 0 {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Dirty: {}",
                                        self.chunk_stats.dirty_count
                                    ))
                                    .color(egui::Color32::YELLOW)
                                    .small(),
                                );
                            }
                            if self.chunk_stats.in_flight_count > 0 {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Generating: {}",
                                        self.chunk_stats.in_flight_count
                                    ))
                                    .color(egui::Color32::LIGHT_GREEN)
                                    .small(),
                                );
                            }
                            ui.label(
                                egui::RichText::new(format!(
                                    "GPU: {:.1} MB",
                                    self.chunk_stats.memory_mb
                                ))
                                .color(egui::Color32::LIGHT_GRAY)
                                .small(),
                            );
                        });
                });

            // World position at top center
            egui::Area::new(egui::Id::new("position_overlay"))
                .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 10.0))
                .show(&ctx, |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180))
                        .corner_radius(egui::CornerRadius::same(4))
                        .inner_margin(egui::Margin::symmetric(12, 6))
                        .show(ui, |ui| {
                            // Format position with enough width to prevent wrapping
                            let pos_text = format!(
                                "Pos: {:.1}, {:.1}, {:.1}",
                                player_world_pos.x, player_world_pos.y, player_world_pos.z
                            );
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(pos_text)
                                        .color(egui::Color32::WHITE)
                                        .strong()
                                        .monospace(),
                                )
                                .wrap_mode(egui::TextWrapMode::Extend),
                            );
                        });
                });

            egui::Window::new("Voxel Game")
                .default_open(false)
                .default_pos(egui::pos2(10.0, 40.0))
                .show(&ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(500.0)
                        .show(ui, |ui| {
                            ui.collapsing("Controls", |ui| {
                                ui.label("  WASD - Move");
                                ui.label("  Space - Jump");
                                ui.label("  Space/Shift - Up/Down (fly, swim & climb)");
                                ui.label("  Mouse - Look around");
                                ui.label("  Scroll - Select block");
                                ui.label("  Ctrl - Toggle sprint");
                                ui.label("  F - Toggle fly mode");
                                ui.label("  B - Toggle chunk boundaries");
                                ui.label("  Left Click - Break block");
                                ui.label("  Right Click - Place block");
                                ui.label("  1-9 - Select block type (8=Ladder, 9=Torch)");
                                ui.label("  Escape - Release cursor");
                            });
                            ui.separator();

                            ui.label(format!("Chunks: {}", self.world.chunk_count()));
                            if self.player.in_water {
                                ui.colored_label(
                                    egui::Color32::from_rgb(100, 150, 255),
                                    "🌊 UNDERWATER",
                                );
                            }

                            ui.separator();

                            // Block selection
                            ui.label(format!("Selected: {:?}", selected_block));
                            if let Some(hit) = &self.current_hit {
                                let block_type = self.world.get_block(hit.block_pos);
                                let block_name = block_type
                                    .map(|b| format!("{:?}", b))
                                    .unwrap_or_else(|| "Unknown".to_string());
                                ui.label(format!(
                                    "Looking at: {} ({}, {}, {})",
                                    block_name, hit.block_pos.x, hit.block_pos.y, hit.block_pos.z
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
                                    ui.selectable_value(
                                        &mut self.render_mode,
                                        mode,
                                        format!("{:?}", mode),
                                    );
                                }
                            });

                            ui.separator();

                            ui.add(
                                egui::Slider::new(&mut self.player.camera.fov, 20.0..=120.0)
                                    .text("FOV"),
                            );

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
                                // Recreate distance buffer for two-pass beam optimization
                                (rcx.distance_image, rcx.distance_set) = get_distance_image_and_set(
                                    self.memory_allocator.clone(),
                                    self.descriptor_set_allocator.clone(),
                                    &self.render_pipeline,
                                    render_extent,
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
                            ui.add(
                                egui::Slider::new(&mut self.ambient_light, 0.0..=1.0)
                                    .text("Ambient Light"),
                            );
                            ui.add(
                                egui::Slider::new(&mut self.fog_density, 0.0..=0.1)
                                    .text("Fog Density"),
                            );
                            ui.add(
                                egui::Slider::new(&mut self.fog_start, 0.0..=128.0)
                                    .text("Fog Start"),
                            );
                            ui.checkbox(&mut self.fog_affects_sky, "Fog Affects Sky");
                            if ui
                                .add(
                                    egui::Slider::new(&mut self.max_ray_steps, 128..=1024)
                                        .text("Ray Steps"),
                                )
                                .changed()
                            {
                                println!("[SETTING] Ray Steps: {}", self.max_ray_steps);
                            }
                            if ui
                                .add(
                                    egui::Slider::new(&mut self.view_distance, 2..=10)
                                        .text("View Distance"),
                                )
                                .changed()
                            {
                                println!("[SETTING] View Distance: {} chunks", self.view_distance);
                                // Ensure unload distance is at least view distance + 1
                                if self.unload_distance <= self.view_distance {
                                    self.unload_distance = self.view_distance + 2;
                                }
                            }
                            if ui
                                .add(
                                    egui::Slider::new(&mut self.unload_distance, 3..=12)
                                        .text("Unload Distance"),
                                )
                                .changed()
                            {
                                println!(
                                    "[SETTING] Unload Distance: {} chunks",
                                    self.unload_distance
                                );
                                // Ensure unload distance is greater than view distance
                                if self.unload_distance <= self.view_distance {
                                    self.unload_distance = self.view_distance + 2;
                                }
                            }

                            ui.separator();
                            ui.label("Feature Toggles (for FPS profiling):");
                            if ui
                                .checkbox(&mut self.enable_ao, "Ambient Occlusion")
                                .changed()
                            {
                                println!(
                                    "[TOGGLE] Ambient Occlusion: {}",
                                    if self.enable_ao { "ON" } else { "OFF" }
                                );
                            }
                            if ui
                                .checkbox(&mut self.enable_shadows, "Sun Shadows")
                                .changed()
                            {
                                println!(
                                    "[TOGGLE] Sun Shadows: {}",
                                    if self.enable_shadows { "ON" } else { "OFF" }
                                );
                            }
                            if ui
                                .checkbox(&mut self.enable_model_shadows, "Model Sun Shadows")
                                .changed()
                            {
                                println!(
                                    "[TOGGLE] Model Sun Shadows: {}",
                                    if self.enable_model_shadows {
                                        "ON"
                                    } else {
                                        "OFF"
                                    }
                                );
                            }
                            if ui
                                .checkbox(&mut self.enable_point_lights, "Point Lights (torches)")
                                .changed()
                            {
                                println!(
                                    "[TOGGLE] Point Lights: {}",
                                    if self.enable_point_lights {
                                        "ON"
                                    } else {
                                        "OFF"
                                    }
                                );
                            }

                            ui.separator();
                            ui.label("LOD Distances (lower = faster):");
                            ui.horizontal(|ui| {
                                ui.label("AO:");
                                if ui
                                    .add(
                                        egui::Slider::new(&mut self.lod_ao_distance, 8.0..=64.0)
                                            .suffix(" blocks"),
                                    )
                                    .changed()
                                {
                                    println!("[LOD] AO distance: {:.0}", self.lod_ao_distance);
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("Shadows:");
                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut self.lod_shadow_distance,
                                            16.0..=128.0,
                                        )
                                        .suffix(" blocks"),
                                    )
                                    .changed()
                                {
                                    println!(
                                        "[LOD] Shadow distance: {:.0}",
                                        self.lod_shadow_distance
                                    );
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("Lights:");
                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut self.lod_point_light_distance,
                                            8.0..=48.0,
                                        )
                                        .suffix(" blocks"),
                                    )
                                    .changed()
                                {
                                    println!(
                                        "[LOD] Point light distance: {:.0}",
                                        self.lod_point_light_distance
                                    );
                                }
                            });

                            ui.separator();

                            // Gameplay options
                            ui.checkbox(&mut self.instant_break, "Instant block break");
                            ui.checkbox(&mut self.show_block_preview, "Block placement preview");
                            ui.checkbox(&mut self.show_target_outline, "Target block outline");
                            if ui
                                .checkbox(&mut self.player.light_enabled, "Player torch light")
                                .changed()
                            {
                                println!(
                                    "[TOGGLE] Player Light: {}",
                                    if self.player.light_enabled {
                                        "ON"
                                    } else {
                                        "OFF"
                                    }
                                );
                            }

                            ui.add(
                                egui::Slider::new(&mut self.break_cooldown_duration, 0.05..=0.5)
                                    .text("Break cooldown")
                                    .suffix("s"),
                            );
                            ui.add(
                                egui::Slider::new(&mut self.place_cooldown_duration, 0.05..=1.0)
                                    .text("Place cooldown")
                                    .suffix("s"),
                            );

                            // Block physics updates per frame (higher = faster cascades, more CPU)
                            let mut max_updates = self.block_updates.max_per_frame as u32;
                            if ui
                                .add(
                                    egui::Slider::new(&mut max_updates, 16..=128)
                                        .text("Physics updates/frame")
                                        .logarithmic(true),
                                )
                                .changed()
                            {
                                self.block_updates.max_per_frame = max_updates as usize;
                            }

                            ui.separator();

                            // Movement settings
                            ui.checkbox(&mut self.player.auto_jump, "Auto-jump");
                            ui.checkbox(&mut self.show_compass, "Show compass");

                            ui.separator();

                            // Minimap settings
                            ui.label("Minimap");
                            if ui
                                .checkbox(&mut self.show_minimap, "Show minimap (M)")
                                .changed()
                            {
                                println!(
                                    "Minimap: {}",
                                    if self.show_minimap { "ON" } else { "OFF" }
                                );
                            }

                            ui.horizontal(|ui| {
                                ui.label("Size:");
                                if ui
                                    .selectable_label(self.minimap.size == 128, "Small")
                                    .clicked()
                                {
                                    self.minimap.size = 128;
                                    self.minimap_cached_image = None; // Force refresh
                                }
                                if ui
                                    .selectable_label(self.minimap.size == 192, "Medium")
                                    .clicked()
                                {
                                    self.minimap.size = 192;
                                    self.minimap_cached_image = None; // Force refresh
                                }
                                if ui
                                    .selectable_label(self.minimap.size == 256, "Large")
                                    .clicked()
                                {
                                    self.minimap.size = 256;
                                    self.minimap_cached_image = None; // Force refresh
                                }
                            });

                            ui.horizontal(|ui| {
                                ui.label("Colors:");
                                if ui
                                    .selectable_label(self.minimap.color_mode == 0, "Blocks")
                                    .clicked()
                                {
                                    self.minimap.color_mode = 0;
                                    self.minimap_cached_image = None; // Force refresh
                                }
                                if ui
                                    .selectable_label(self.minimap.color_mode == 1, "Height")
                                    .clicked()
                                {
                                    self.minimap.color_mode = 1;
                                    self.minimap_cached_image = None; // Force refresh
                                }
                                if ui
                                    .selectable_label(self.minimap.color_mode == 2, "Both")
                                    .clicked()
                                {
                                    self.minimap.color_mode = 2;
                                    self.minimap_cached_image = None; // Force refresh
                                }
                            });

                            if ui
                                .add(
                                    egui::Slider::new(&mut self.minimap.zoom, 0.5..=3.0)
                                        .text("Zoom")
                                        .logarithmic(true),
                                )
                                .changed()
                            {
                                self.minimap_cached_image = None; // Force refresh
                            }

                            if ui
                                .checkbox(&mut self.minimap.rotate, "Rotate with player")
                                .changed()
                            {
                                // Force minimap refresh when rotation mode changes
                                self.minimap_cached_image = None;
                            }

                            ui.separator();

                            // Camera position debug
                            ui.label(format!(
                                "Position: ({:.1}, {:.1}, {:.1})",
                                self.player.camera.position.x,
                                self.player.camera.position.y,
                                self.player.camera.position.z
                            ));
                        }); // end ScrollArea
                });

            // Draw crosshair at screen center
            // Changes appearance when targeting a block
            let screen_rect = ctx.screen_rect();
            let center = screen_rect.center();
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("crosshair"),
            ));

            let targeting_block = self.current_hit.is_some();
            let (crosshair_size, crosshair_gap, crosshair_color) = if targeting_block {
                (12.0, 4.0, egui::Color32::from_rgb(100, 255, 100)) // Green, larger, with gap
            } else {
                (8.0, 0.0, egui::Color32::WHITE) // White, smaller, no gap
            };
            let stroke = egui::Stroke::new(2.0, crosshair_color);

            // Horizontal lines (with gap when targeting)
            painter.line_segment(
                [
                    egui::pos2(center.x - crosshair_size, center.y),
                    egui::pos2(center.x - crosshair_gap, center.y),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x + crosshair_gap, center.y),
                    egui::pos2(center.x + crosshair_size, center.y),
                ],
                stroke,
            );
            // Vertical lines (with gap when targeting)
            painter.line_segment(
                [
                    egui::pos2(center.x, center.y - crosshair_size),
                    egui::pos2(center.x, center.y - crosshair_gap),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x, center.y + crosshair_gap),
                    egui::pos2(center.x, center.y + crosshair_size),
                ],
                stroke,
            );

            // Minimap HUD (top-left)
            if show_minimap {
                if let Some(image) = minimap_image {
                    // Load the pre-generated image as texture
                    let texture = ctx.load_texture("minimap", image, egui::TextureOptions::NEAREST);

                    egui::Area::new(egui::Id::new("minimap_hud"))
                        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-10.0, -60.0))
                        .show(&ctx, |ui| {
                            egui::Frame::new()
                                .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 200))
                                .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(60, 60, 60)))
                                .corner_radius(egui::CornerRadius::same(4))
                                .inner_margin(egui::Margin::same(4))
                                .show(ui, |ui| {
                                    let size = minimap_size as f32;
                                    let image_response = ui.add(
                                        egui::Image::new(egui::load::SizedTexture::new(
                                            texture.id(),
                                            egui::vec2(size, size),
                                        ))
                                        .fit_to_exact_size(egui::vec2(size, size)),
                                    );

                                    // Draw player indicator (triangle pointing in direction)
                                    let center = image_response.rect.center();
                                    let tri_size = 6.0;

                                    // Calculate triangle rotation angle
                                    let angle = if minimap_rotate {
                                        0.0 // Always point up when map rotates
                                    } else {
                                        -camera_yaw // Point in player's direction
                                    };

                                    // Triangle vertices: tip at front, two corners at back
                                    let (sin_a, cos_a) = (angle.sin(), angle.cos());
                                    let tip = egui::pos2(
                                        center.x - sin_a * tri_size,
                                        center.y - cos_a * tri_size,
                                    );
                                    let left = egui::pos2(
                                        center.x + cos_a * tri_size * 0.6 + sin_a * tri_size * 0.5,
                                        center.y - sin_a * tri_size * 0.6 + cos_a * tri_size * 0.5,
                                    );
                                    let right = egui::pos2(
                                        center.x - cos_a * tri_size * 0.6 + sin_a * tri_size * 0.5,
                                        center.y + sin_a * tri_size * 0.6 + cos_a * tri_size * 0.5,
                                    );

                                    ui.painter().add(egui::Shape::convex_polygon(
                                        vec![tip, left, right],
                                        egui::Color32::RED,
                                        egui::Stroke::new(1.0, egui::Color32::WHITE),
                                    ));
                                });
                        });
                }
            }

            // Compass HUD (bottom-left)
            if show_compass {
                egui::Area::new(egui::Id::new("compass_hud"))
                    .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(10.0, -60.0))
                    .show(&ctx, |ui| {
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 200))
                            .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(60, 60, 60)))
                            .corner_radius(egui::CornerRadius::same(4))
                            .inner_margin(egui::Margin::same(8))
                            .show(ui, |ui| {
                                let compass_size = 60.0;
                                let (response, painter) = ui.allocate_painter(
                                    egui::vec2(compass_size, compass_size),
                                    egui::Sense::hover(),
                                );
                                let center = response.rect.center();
                                let radius = compass_size / 2.0 - 4.0;

                                // Draw compass circle
                                painter.circle_stroke(
                                    center,
                                    radius,
                                    egui::Stroke::new(1.5, egui::Color32::from_rgb(100, 100, 100)),
                                );

                                // Cardinal direction positions (N=-Z, S=+Z, E=+X, W=-X)
                                // In our coordinate system: yaw=0 looks at -Z (North)
                                let directions = [
                                    ("N", 0.0_f32, egui::Color32::RED), // North at yaw=0
                                    ("E", std::f32::consts::FRAC_PI_2, egui::Color32::WHITE), // East at yaw=90°
                                    ("S", std::f32::consts::PI, egui::Color32::WHITE), // South at yaw=180°
                                    ("W", -std::f32::consts::FRAC_PI_2, egui::Color32::WHITE), // West at yaw=-90°
                                ];

                                for (label, dir_angle, color) in directions {
                                    // Calculate angle relative to player's view
                                    // Player yaw: 0 = looking North (-Z)
                                    let relative_angle = dir_angle - camera_yaw;
                                    let (sin_a, cos_a) = relative_angle.sin_cos();

                                    // Position on compass (up = forward direction in player's view)
                                    let label_pos = egui::pos2(
                                        center.x + sin_a * (radius - 8.0),
                                        center.y - cos_a * (radius - 8.0),
                                    );

                                    painter.text(
                                        label_pos,
                                        egui::Align2::CENTER_CENTER,
                                        label,
                                        egui::FontId::proportional(12.0),
                                        color,
                                    );
                                }

                                // Draw direction indicator (line pointing up = forward)
                                painter.line_segment(
                                    [
                                        egui::pos2(center.x, center.y),
                                        egui::pos2(center.x, center.y - radius + 12.0),
                                    ],
                                    egui::Stroke::new(2.0, egui::Color32::YELLOW),
                                );
                                // Arrow head
                                painter.line_segment(
                                    [
                                        egui::pos2(center.x - 4.0, center.y - radius + 18.0),
                                        egui::pos2(center.x, center.y - radius + 12.0),
                                    ],
                                    egui::Stroke::new(2.0, egui::Color32::YELLOW),
                                );
                                painter.line_segment(
                                    [
                                        egui::pos2(center.x + 4.0, center.y - radius + 18.0),
                                        egui::pos2(center.x, center.y - radius + 12.0),
                                    ],
                                    egui::Stroke::new(2.0, egui::Color32::YELLOW),
                                );
                            });
                    });
            }

            // Hotbar HUD at bottom center - 9 slots
            const ATLAS_TILE_COUNT: f32 = 19.0;
            const SLOT_SIZE: f32 = 40.0;

            egui::Area::new(egui::Id::new("hotbar_hud"))
                .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -10.0))
                .show(&ctx, |ui| {
                    // Background frame for the whole hotbar
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180))
                        .corner_radius(egui::CornerRadius::same(4))
                        .inner_margin(egui::Margin::same(6))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);

                                for (i, block) in hotbar_blocks.iter().enumerate() {
                                    let is_selected = i == hotbar_index;

                                    // Calculate UV for this block
                                    // For Model blocks, use texture based on model type
                                    let block_idx = if *block == BlockType::Model {
                                        match hotbar_model_ids[i] {
                                            1 => 11.0,      // Torch
                                            4..=19 => 4.0,  // Fence -> use planks texture
                                            20..=27 => 4.0, // Gate -> use planks texture
                                            29 => 4.0,      // Ladder -> use planks texture
                                            _ => 11.0,      // Default to torch
                                        }
                                    } else {
                                        *block as u8 as f32
                                    };
                                    let uv_left = block_idx / ATLAS_TILE_COUNT;
                                    let uv_right = (block_idx + 1.0) / ATLAS_TILE_COUNT;
                                    let uv_rect = egui::Rect::from_min_max(
                                        egui::pos2(uv_left, 0.0),
                                        egui::pos2(uv_right, 1.0),
                                    );

                                    // Slot border color
                                    let border_color = if is_selected {
                                        egui::Color32::from_rgb(100, 255, 100)
                                    } else {
                                        egui::Color32::from_rgb(60, 60, 60)
                                    };
                                    let border_width = if is_selected { 3.0 } else { 1.0 };

                                    // Allocate space for slot
                                    let (rect, _response) = ui.allocate_exact_size(
                                        egui::vec2(SLOT_SIZE + 4.0, SLOT_SIZE + 16.0),
                                        egui::Sense::hover(),
                                    );

                                    // Draw slot background
                                    ui.painter().rect_filled(
                                        rect,
                                        egui::CornerRadius::same(2),
                                        egui::Color32::from_rgb(40, 40, 40),
                                    );

                                    // Draw texture
                                    let texture_rect = egui::Rect::from_min_size(
                                        rect.min + egui::vec2(2.0, 2.0),
                                        egui::vec2(SLOT_SIZE, SLOT_SIZE),
                                    );
                                    ui.painter().image(
                                        atlas_texture_id,
                                        texture_rect,
                                        uv_rect,
                                        egui::Color32::WHITE,
                                    );

                                    // Draw border
                                    ui.painter().rect_stroke(
                                        rect,
                                        egui::CornerRadius::same(2),
                                        egui::Stroke::new(border_width, border_color),
                                        egui::StrokeKind::Outside,
                                    );

                                    // Draw number label
                                    let text_pos = egui::pos2(rect.center().x, rect.max.y - 8.0);
                                    ui.painter().text(
                                        text_pos,
                                        egui::Align2::CENTER_CENTER,
                                        format!("{}", i + 1),
                                        egui::FontId::proportional(10.0),
                                        egui::Color32::WHITE,
                                    );
                                }
                            });

                            // Selected block name below hotbar
                            ui.vertical_centered(|ui| {
                                ui.add_space(4.0);
                                // For Model blocks, show the model type name
                                let block_name = if selected_block == BlockType::Model {
                                    match hotbar_model_ids[hotbar_index] {
                                        1 => "Torch".to_string(),
                                        4..=19 => "Fence".to_string(),
                                        20..=23 => "Gate (Closed)".to_string(),
                                        24..=27 => "Gate (Open)".to_string(),
                                        29 => "Ladder".to_string(),
                                        _ => format!("{:?}", selected_block),
                                    }
                                } else {
                                    format!("{:?}", selected_block)
                                };
                                ui.label(
                                    egui::RichText::new(block_name)
                                        .color(egui::Color32::WHITE)
                                        .strong(),
                                );
                            });
                        });
                });
        });

        let render_extent = rcx.render_image.extent();
        let resample_extent = rcx.resample_image.extent();
        self.player.camera.extent = [render_extent[0] as f64, render_extent[1] as f64];

        let pixel_to_ray = self.player.camera.pixel_to_ray_matrix();

        // Scale only the position (column 4), not the direction (3x3 rotation part)
        // This prevents ray distortion from non-uniform world dimensions
        let mut pixel_to_ray_scaled = pixel_to_ray;
        // Camera position is normalized (0-1), scale to texture size
        // Ray marching happens in texture space (0 to textureSize)
        pixel_to_ray_scaled.m14 *= self.world_extent[0] as f64;
        pixel_to_ray_scaled.m24 *= self.world_extent[1] as f64;
        pixel_to_ray_scaled.m34 *= self.world_extent[2] as f64;

        // Apply head bob offset to camera Y position for rendering
        let head_bob_offset = (self.player.head_bob_timer * std::f64::consts::TAU).sin()
            * HEAD_BOB_AMPLITUDE
            * self.player.head_bob_intensity;
        pixel_to_ray_scaled.m24 += head_bob_offset;

        let pixel_to_ray = pixel_to_ray_scaled;

        #[derive(BufferContents, Clone, Copy)]
        #[repr(C)]
        struct PushConstants {
            pixel_to_ray: Matrix4<f32>,
            // Texture dimensions (not world bounds - world is infinite)
            texture_size_x: u32,
            texture_size_y: u32,
            texture_size_z: u32,
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
            // Point light count
            light_count: u32,
            // Ambient light level
            ambient_light: f32,
            // Fog density
            fog_density: f32,
            // Fog start distance
            fog_start: f32,
            // Whether fog affects sky (0 = false, 1 = true)
            fog_affects_sky: u32,
            // Target block (block player is looking at, -1 = none)
            target_block_x: i32,
            target_block_y: i32,
            target_block_z: i32,
            // Maximum ray marching steps
            max_ray_steps: u32,
            // Texture origin in world coordinates (world pos that maps to texture 0,0,0)
            texture_origin_x: i32,
            texture_origin_y: i32,
            texture_origin_z: i32,
            // Feature toggles for performance profiling
            enable_ao: u32,
            enable_shadows: u32,
            enable_model_shadows: u32,
            enable_point_lights: u32,
            // Two-pass beam optimization: 0 = normal, 1 = distance only, 2 = use distance hints
            pass_mode: u32,
            // LOD distance thresholds (0 = use defaults)
            lod_ao_distance: f32,
            lod_shadow_distance: f32,
            lod_point_light_distance: f32,
            // Falling block count
            falling_block_count: u32,
        }
        // Convert world coordinates to texture coordinates for shader
        // Shader works in texture space, so we subtract texture_origin
        let tex_origin = self.texture_origin;
        let world_to_tex = |world_pos: Vector3<i32>| -> (i32, i32, i32) {
            (
                world_pos.x - tex_origin.x,
                world_pos.y - tex_origin.y,
                world_pos.z - tex_origin.z,
            )
        };

        let (break_x, break_y, break_z) = self
            .breaking_block
            .map(&world_to_tex)
            .unwrap_or((-1, -1, -1));

        // Calculate preview block position (where block would be placed)
        let selected_block_id = selected_block as u32;
        // Use player_world_pos computed earlier (before rcx borrow)
        let (preview_x, preview_y, preview_z, preview_type) = if self.show_block_preview {
            self.current_hit
                .as_ref()
                .map(|hit| {
                    let place_pos = get_place_position(hit);
                    // Only show preview if position is in bounds and not inside player
                    let block_center = place_pos.cast::<f64>() + Vector3::new(0.5, 0.5, 0.5);
                    // Y bounds only (X/Z are infinite)
                    let in_bounds = place_pos.y >= 0 && place_pos.y < TEXTURE_SIZE_Y as i32;
                    let not_in_player = (player_world_pos - block_center).norm() > 1.5;
                    if in_bounds && not_in_player {
                        let tex_pos = world_to_tex(place_pos);
                        (tex_pos.0, tex_pos.1, tex_pos.2, selected_block_id)
                    } else {
                        (-1, -1, -1, 0)
                    }
                })
                .unwrap_or((-1, -1, -1, 0))
        } else {
            (-1, -1, -1, 0) // Preview disabled
        };

        // Target block (block player is looking at) - convert to texture coords
        // Only send if outline is enabled
        let (target_x, target_y, target_z) = if self.show_target_outline {
            self.current_hit
                .as_ref()
                .map(|hit| world_to_tex(hit.block_pos))
                .unwrap_or((-1, -1, -1))
        } else {
            (-1, -1, -1) // Outline disabled
        };

        // Update particle buffer (convert world coords to texture coords)
        let gpu_particles = self.particles.gpu_data();
        let particle_count = gpu_particles.len() as u32;
        {
            let tex_origin = self.texture_origin;
            let mut write = self.particle_buffer.write().unwrap();
            for (i, p) in gpu_particles.iter().enumerate() {
                let mut converted = *p;
                // Convert world position to texture position
                converted.pos_size[0] -= tex_origin.x as f32;
                converted.pos_size[1] -= tex_origin.y as f32;
                converted.pos_size[2] -= tex_origin.z as f32;
                write[i] = converted;
            }
        }

        // Update falling block buffer (convert world coords to texture coords)
        let gpu_falling_blocks = self.falling_blocks.gpu_data();
        {
            let tex_origin = self.texture_origin;
            let mut write = self.falling_block_buffer.write().unwrap();
            for (i, fb) in gpu_falling_blocks.iter().enumerate() {
                let mut converted = *fb;
                // Convert world position to texture position
                converted.pos_type[0] -= tex_origin.x as f32;
                converted.pos_type[1] -= tex_origin.y as f32;
                converted.pos_type[2] -= tex_origin.z as f32;
                write[i] = converted;
            }
        }

        // Update light buffer with torch positions (collected earlier)
        {
            let mut write = self.light_buffer.write().unwrap();
            for (i, l) in gpu_lights.iter().enumerate() {
                write[i] = *l;
            }
        }

        let push_constants = PushConstants {
            pixel_to_ray: pixel_to_ray.cast(),
            texture_size_x: self.world_extent[0],
            texture_size_y: self.world_extent[1],
            texture_size_z: self.world_extent[2],
            render_mode: self.render_mode as u32,
            show_chunk_boundaries: self.show_chunk_boundaries as u32,
            player_in_water: self.player.in_water as u32,
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
            light_count,
            ambient_light: self.ambient_light,
            fog_density: self.fog_density,
            fog_start: self.fog_start,
            fog_affects_sky: self.fog_affects_sky as u32,
            target_block_x: target_x,
            target_block_y: target_y,
            target_block_z: target_z,
            max_ray_steps: self.max_ray_steps,
            texture_origin_x: self.texture_origin.x,
            texture_origin_y: self.texture_origin.y,
            texture_origin_z: self.texture_origin.z,
            enable_ao: if self.enable_ao { 1 } else { 0 },
            enable_shadows: if self.enable_shadows { 1 } else { 0 },
            enable_model_shadows: if self.enable_model_shadows { 1 } else { 0 },
            enable_point_lights: if self.enable_point_lights { 1 } else { 0 },
            pass_mode: 0, // Will be set per-pass
            lod_ao_distance: self.lod_ao_distance,
            lod_shadow_distance: self.lod_shadow_distance,
            lod_point_light_distance: self.lod_point_light_distance,
            falling_block_count: self.falling_blocks.count() as u32,
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

        // Single-pass rendering with empty chunk skip optimization
        // (Two-pass beam optimization was tested but added overhead without benefit
        // since empty chunk skip already makes rays very fast)
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
                    self.light_set.clone(),
                    self.chunk_metadata_set.clone(),
                    rcx.distance_set.clone(),
                    self.brick_and_model_set.clone(), // Combined set 7: brick + model resources
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

        // Check if we need to take a screenshot (do this before the borrow is released)
        let needs_screenshot = if let Some(delay) = self.args.screenshot_delay {
            if !self.screenshot_taken {
                let elapsed = Instant::now().duration_since(self.start_time).as_secs_f64();
                if elapsed >= delay {
                    Some(rcx.image_views[image_index as usize].clone())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // Take screenshot if delay has elapsed (outside rcx borrow scope)
        if let Some(image_view) = needs_screenshot {
            save_screenshot(
                &self.device,
                &self.queue,
                &self.memory_allocator,
                &self.command_buffer_allocator,
                &image_view,
                "voxel_world_screen_shot.png",
            );
            self.screenshot_taken = true;
        }

        // Record render time and increment sample count
        self.profiler.render_us += t_render_start.elapsed().as_micros() as u64;
        self.profiler.sample_count += 1;
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

        // Create distance buffer for two-pass beam optimization
        let (distance_image, distance_set) = get_distance_image_and_set(
            self.memory_allocator.clone(),
            self.descriptor_set_allocator.clone(),
            &self.render_pipeline,
            render_extent,
        );

        let mut gui = Gui::new(
            event_loop,
            surface,
            self.queue.clone(),
            swapchain.image_format(),
            GuiConfig {
                is_overlay: true,
                ..Default::default()
            },
        );

        // Register the texture atlas with egui for HUD display
        let atlas_texture_id = gui.register_user_image_view(
            self.texture_atlas_view.clone(),
            SamplerCreateInfo {
                mag_filter: Filter::Nearest,
                min_filter: Filter::Nearest,
                address_mode: [SamplerAddressMode::ClampToEdge; 3],
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

            distance_image,
            distance_set,

            gui,
            atlas_texture_id,

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

    // Upload all initial chunks to GPU before starting the game
    app.upload_all_dirty_chunks();

    event_loop.run_app(&mut app).unwrap();
}

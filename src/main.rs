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
use nalgebra::{Vector3, vector};

use std::path::PathBuf;
use std::{
    f64::consts::{FRAC_PI_2, TAU},
    sync::Arc,
    time::{Duration, Instant},
};
use vulkano::{
    Validated, VulkanError,
    buffer::Subbuffer,
    command_buffer::{
        AutoCommandBufferBuilder, BlitImageInfo, ClearColorImageInfo, CommandBufferUsage,
    },
    descriptor_set::DescriptorSet,
    device::{Device, Queue},
    image::{
        Image,
        sampler::{Filter, SamplerAddressMode, SamplerCreateInfo},
        view::{ImageView, ImageViewCreateInfo},
    },
    instance::Instance,
    memory::allocator::StandardMemoryAllocator,
    pipeline::{Pipeline, PipelineBindPoint},
    swapchain::{Surface, SwapchainCreateInfo, SwapchainPresentInfo, acquire_next_image},
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

mod atmosphere;
mod block_interaction;
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
mod hud_render;
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
mod world_streaming;

use crate::block_update::BlockUpdateQueue;
use crate::chunk::{BlockType, CHUNK_SIZE};
use crate::chunk_loader::ChunkLoader;
use crate::config::{Args, INITIAL_WINDOW_RESOLUTION, Settings};
use crate::constants::{
    LOADED_CHUNKS_X, LOADED_CHUNKS_Z, TEXTURE_SIZE_X, TEXTURE_SIZE_Y, TEXTURE_SIZE_Z,
    UNLOAD_DISTANCE, VIEW_DISTANCE, WORLD_CHUNKS_Y,
};
use crate::falling_block::{FallingBlockSystem, GpuFallingBlock};
use crate::gpu_resources::{
    GpuLight, PushConstants, create_empty_voxel_texture, get_brick_and_model_set,
    get_chunk_metadata_set, get_distance_image_and_set, get_images_and_sets, get_light_set,
    get_particle_and_falling_block_set, get_swapchain_images, load_icon, load_texture_atlas,
    save_screenshot,
};
use crate::hot_reload::HotReloadComputePipeline;
use crate::hud::Minimap;
use crate::hud_render::HUDRenderer;
use crate::particles::ParticleSystem;
use crate::player::{HEAD_BOB_AMPLITUDE, Player};
use crate::raycast::{RaycastHit, get_place_position};
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
    settings: Settings,
    /// Current window size for debug output
    window_size: [u32; 2],

    // Minimap state
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

    /// Current time of day (0.0 = midnight, 0.5 = noon, 1.0 = midnight)
    time_of_day: f32,
    /// Whether the day/night cycle is paused
    day_cycle_paused: bool,
    /// Atmospheric lighting/fog settings
    atmosphere: atmosphere::AtmosphereSettings,
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
    /// Cooldown timer after breaking a block in instant mode (seconds remaining).
    break_cooldown: f32,
    /// Skip block breaking until mouse is released (used to ignore focus click).
    skip_break_until_release: bool,

    /// Last position where a block was placed (for continuous placing).
    last_place_pos: Option<Vector3<i32>>,
    /// Cooldown timer for continuous block placing.
    place_cooldown: f32,
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

    rcx: Option<gpu_resources::RenderContext>,
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
            settings: Settings {
                show_chunk_boundaries: args.show_chunk_boundaries,
                render_scale: 0.75,
                ..Settings::default()
            },
            window_size: INITIAL_WINDOW_RESOLUTION.into(),

            // Minimap - disabled by default, toggle with M
            show_minimap: false,
            minimap: Minimap::new(),
            minimap_cached_image: None,
            minimap_last_pos: Vector3::new(i32::MAX, 0, i32::MAX), // Force initial update
            minimap_last_update: Instant::now(),
            minimap_last_yaw: f32::MAX, // Force initial update

            time_of_day: args
                .time_of_day
                .map(|t| t as f32)
                .unwrap_or(DEFAULT_TIME_OF_DAY),
            day_cycle_paused: true, // Day cycle paused by default
            atmosphere: atmosphere::AtmosphereSettings::default(),
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
            break_cooldown: 0.0,
            skip_break_until_release: false,

            last_place_pos: None,
            place_cooldown: 0.0,
            model_needs_reclick: false,
            gate_needs_reclick: false,
            line_start_pos: None,
            line_locked_axis: None,

            particles: ParticleSystem::new(),
            falling_blocks: FallingBlockSystem::new(),
            block_updates: BlockUpdateQueue::new(32),
            water_grid: WaterGrid::new(),

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
                (self.window_size[0] as f32 * self.settings.render_scale) as u32,
                (self.window_size[1] as f32 * self.settings.render_scale) as u32,
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
        if self.settings.water_simulation_enabled {
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
                self.settings.show_chunk_boundaries = !self.settings.show_chunk_boundaries;
                if self.settings.show_chunk_boundaries {
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
        let gpu_lights = self.world.collect_torch_lights(
            self.player.light_enabled,
            self.player.camera.position,
            self.texture_origin,
            &self.model_registry,
            self.world_extent,
        );
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
                (window_extent[0] as f32 * self.settings.render_scale) as u32,
                (window_extent[1] as f32 * self.settings.render_scale) as u32,
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
        let _atlas_texture_id = rcx.atlas_texture_id;

        if HUDRenderer.render(
            &mut rcx.gui,
            hud_render::HudInputs {
                fps: self.fps,
                chunk_stats: &self.chunk_stats,
                player: &mut self.player,
                world: &mut self.world,
                settings: &mut self.settings,
                render_mode: &mut self.render_mode,
                current_hit: &self.current_hit,
                selected_block,
                hotbar_index,
                hotbar_blocks: &hotbar_blocks,
                hotbar_model_ids: &hotbar_model_ids,
                minimap_image,
                atlas_texture_id: rcx.atlas_texture_id,
                camera_yaw,
                player_world_pos,
                time_of_day: &mut self.time_of_day,
                day_cycle_paused: &mut self.day_cycle_paused,
                atmosphere: &mut self.atmosphere,
                view_distance: &mut self.view_distance,
                unload_distance: &mut self.unload_distance,
                block_updates: &mut self.block_updates,
                show_minimap: &mut self.show_minimap,
                minimap: &mut self.minimap,
                minimap_cached_image: &mut self.minimap_cached_image,
            },
        ) {
            let window_extent: [u32; 2] = rcx.window.inner_size().into();
            let render_extent = [
                (window_extent[0] as f32 * self.settings.render_scale) as u32,
                (window_extent[1] as f32 * self.settings.render_scale) as u32,
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
        let (preview_x, preview_y, preview_z, preview_type) = if self.settings.show_block_preview {
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
        let (target_x, target_y, target_z) = if self.settings.show_target_outline {
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
            show_chunk_boundaries: self.settings.show_chunk_boundaries as u32,
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
            ambient_light: self.atmosphere.ambient_light,
            fog_density: self.atmosphere.fog_density,
            fog_start: self.atmosphere.fog_start,
            fog_affects_sky: self.atmosphere.fog_affects_sky as u32,
            fog_overlay_scale: self.atmosphere.fog_overlay_scale,
            target_block_x: target_x,
            target_block_y: target_y,
            target_block_z: target_z,
            max_ray_steps: self.settings.max_ray_steps,
            texture_origin_x: self.texture_origin.x,
            texture_origin_y: self.texture_origin.y,
            texture_origin_z: self.texture_origin.z,
            enable_ao: if self.settings.enable_ao { 1 } else { 0 },
            enable_shadows: if self.settings.enable_shadows { 1 } else { 0 },
            enable_model_shadows: if self.settings.enable_model_shadows {
                1
            } else {
                0
            },
            enable_point_lights: if self.settings.enable_point_lights {
                1
            } else {
                0
            },
            pass_mode: 0, // Will be set per-pass
            lod_ao_distance: self.settings.lod_ao_distance,
            lod_shadow_distance: self.settings.lod_shadow_distance,
            lod_point_light_distance: self.settings.lod_point_light_distance,
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
            (window_extent[0] as f32 * self.settings.render_scale) as u32,
            (window_extent[1] as f32 * self.settings.render_scale) as u32,
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

        self.rcx = Some(gpu_resources::RenderContext {
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

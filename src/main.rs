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

use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::{
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
    event::{DeviceEvent, DeviceId, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};
use winit_input_helper::WinitInputHelper;

mod app_hud;
mod app_input;
mod app_minimap;
mod app_stats;
mod atmosphere;
mod block_interaction;
mod block_update;
mod camera;
mod chunk;
mod chunk_loader;
mod config;
mod constants;
mod editor;
mod falling_block;
mod gpu_resources;
mod hot_reload;
mod hud;
mod hud_render;
mod launcher_config;
mod particles;
mod player;
mod raycast;
mod render_mode;
mod sprite_gen;
mod storage;
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
use crate::particles::ParticleSystem;
use crate::player::{HEAD_BOB_AMPLITUDE, PLAYER_EYE_HEIGHT, Player};
use crate::raycast::{RaycastHit, get_place_position};
use crate::render_mode::RenderMode;
use crate::sub_voxel::ModelRegistry;
use crate::terrain_gen::{TerrainGenerator, generate_chunk_terrain};
use crate::utils::{ChunkStats, Profiler};
use crate::vulkan_context::VulkanContext;
use crate::water::WaterGrid;
use crate::world::World;
use std::process;
use vulkano::command_buffer::allocator::StandardCommandBufferAllocator;
use vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator as StdDescriptorSetAllocator;
use world_streaming::MetadataState;

// Constants moved to constants.rs

// Player physics constants moved to player.rs

// Day/night cycle constants
/// Duration of a full day cycle in seconds (real time)
const DAY_CYCLE_DURATION: f32 = 120.0;
/// Default time of day (0.0 = 6am, 0.5 = 6pm, formula: hours = (v * 24 + 6) % 24)
/// 0.25 = 12:00 (Noon)
const DEFAULT_TIME_OF_DAY: f32 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum PaletteTab {
    #[default]
    All,
    Blocks,
    Models,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PaletteItem {
    block: BlockType,
    /// For non-Model blocks this is 0; for Model blocks this is the registry model_id.
    model_id: u8,
}

/// Default blocks available in the hotbar (9 slots, keys 1-9)
const DEFAULT_HOTBAR_BLOCKS: [BlockType; 9] = [
    BlockType::Stone,
    BlockType::Dirt,
    BlockType::Grass,
    BlockType::Sand,
    BlockType::Log,
    BlockType::Model, // Stairs
    BlockType::Model, // Slab bottom
    BlockType::Model, // Slab top
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
    28, // Stairs (base straight)
    2,  // Slab bottom
    3,  // Slab top
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
fn create_initial_world_with_seed(
    spawn_chunk: Vector3<i32>,
    seed: u32,
    storage: Option<&storage::worker::StorageSystem>,
) -> World {
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

                // Try to load from storage first
                let mut loaded = false;
                if let Some(storage) = storage {
                    if let Ok(Some(mut chunk)) = storage.load_chunk(chunk_pos) {
                        chunk.update_metadata();
                        chunk.mark_dirty();
                        chunk.persistence_dirty = false;
                        world.insert_chunk(chunk_pos, chunk);
                        loaded = true;
                    }
                }

                if !loaded {
                    let chunk = generate_chunk_terrain(&terrain, chunk_pos);
                    world.insert_chunk(chunk_pos, chunk);
                }
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

struct Graphics {
    instance: Arc<Instance>,
    device: Arc<Device>,
    queue: Arc<Queue>,

    memory_allocator: Arc<StandardMemoryAllocator>,
    descriptor_set_allocator: Arc<StdDescriptorSetAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,

    render_pipeline: HotReloadComputePipeline,
    resample_pipeline: HotReloadComputePipeline,

    voxel_set: Arc<DescriptorSet>,
    texture_set: Arc<DescriptorSet>,
    texture_atlas_view: Arc<ImageView>,

    particle_buffer: Subbuffer<[particles::GpuParticle]>,
    particle_set: Arc<DescriptorSet>,
    light_buffer: Subbuffer<[GpuLight]>,
    light_set: Arc<DescriptorSet>,
    chunk_metadata_buffer: Subbuffer<[u32]>,
    chunk_metadata_set: Arc<DescriptorSet>,
    brick_mask_buffer: Subbuffer<[u32]>,
    brick_dist_buffer: Subbuffer<[u32]>,
    brick_and_model_set: Arc<DescriptorSet>,
    falling_block_buffer: Subbuffer<[GpuFallingBlock]>,
    voxel_image: Arc<Image>,
    #[allow(dead_code)] // Held for GPU lifetime
    model_atlas: Arc<Image>,
    model_metadata: Arc<Image>,

    rcx: Option<gpu_resources::RenderContext>,
}

struct WorldSim {
    world: World,
    model_registry: ModelRegistry,
    player: Player,
    world_extent: [u32; 3],
    texture_origin: Vector3<i32>,
    last_player_chunk: Vector3<i32>,
    chunk_stats: ChunkStats,
    chunk_loader: ChunkLoader,
    storage: Arc<storage::worker::StorageSystem>,

    particles: ParticleSystem,
    falling_blocks: FallingBlockSystem,
    block_updates: BlockUpdateQueue,
    water_grid: WaterGrid,

    time_of_day: f32,
    day_cycle_paused: bool,
    atmosphere: atmosphere::AtmosphereSettings,
    animation_time: f32,

    render_mode: RenderMode,
    view_distance: i32,
    unload_distance: i32,

    profiler: Profiler,

    metadata_state: MetadataState,
    last_save: Instant,
    world_dir: PathBuf,
    seed: u32,
}

struct UiState {
    settings: Settings,
    window_size: [u32; 2],
    start_time: Instant,
    profile_log_path: Option<String>,
    profile_log_header_written: bool,

    show_minimap: bool,
    minimap: Minimap,
    minimap_cached_image: Option<egui::ColorImage>,
    minimap_last_pos: Vector3<i32>,
    minimap_last_update: Instant,
    minimap_last_yaw: f32,

    palette_open: bool,
    palette_tab: PaletteTab,
    palette_previously_focused: bool,
    dragging_item: Option<PaletteItem>,

    hotbar_index: usize,
    hotbar_blocks: [BlockType; 9],
    hotbar_model_ids: [u8; 9],
    current_hit: Option<RaycastHit>,

    breaking_block: Option<Vector3<i32>>,
    break_progress: f32,
    break_cooldown: f32,
    skip_break_until_release: bool,

    last_place_pos: Option<Vector3<i32>>,
    place_cooldown: f32,
    place_needs_reclick: bool,
    model_needs_reclick: bool,
    gate_needs_reclick: bool,
    line_start_pos: Option<Vector3<i32>>,
    line_locked_axis: Option<u8>,

    last_second: Instant,
    frames_since_last_second: u32,
    fps: u32,
    total_frames: u64,
    screenshot_taken: bool,
}

struct InputState {
    helper: WinitInputHelper,
    focused: bool,
    /// Deferred cursor grab change (workaround for macOS crash).
    /// true = grab and hide, false = release and show
    pending_grab: Option<bool>,
}

impl Deref for InputState {
    type Target = WinitInputHelper;

    fn deref(&self) -> &Self::Target {
        &self.helper
    }
}

impl DerefMut for InputState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.helper
    }
}

struct App {
    args: Args,
    start_time: Instant,
    graphics: Graphics,
    sim: WorldSim,
    ui: UiState,
    input: InputState,
}

impl WorldSim {
    pub fn auto_save(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last_save) > Duration::from_secs(30) {
            self.save_dirty(10);
            self.save_metadata();
            // Update last_save even if nothing was saved, to wait for the next interval
            self.last_save = now;
        }
    }

    pub fn save_metadata(&self) {
        let player_pos = self.player.feet_pos(self.world_extent, self.texture_origin);
        let yaw = self.player.camera.rotation.y;
        let pitch = self.player.camera.rotation.x;

        let meta = storage::metadata::WorldMetadata {
            seed: self.seed,
            spawn_pos: [player_pos.x, player_pos.y, player_pos.z], // Legacy field, keeping updated
            version: 1,
            time_of_day: self.time_of_day,
            day_cycle_paused: self.day_cycle_paused,
            player: Some(storage::metadata::PlayerData {
                position: [player_pos.x, player_pos.y, player_pos.z],
                yaw: yaw as f32,
                pitch: pitch as f32,
            }),
        };

        if let Err(e) = meta.save(self.world_dir.join("level.dat")) {
            eprintln!("[Storage] Failed to save metadata: {}", e);
        }
    }

    pub fn save_dirty(&mut self, limit: usize) {
        let mut saved_count = 0;
        for (pos, chunk) in self.world.chunks_mut() {
            if chunk.persistence_dirty {
                let serialized = storage::format::SerializedChunk::from(&*chunk);
                self.storage.save_chunk(*pos, serialized);
                chunk.persistence_dirty = false;
                saved_count += 1;
                if saved_count >= limit {
                    break;
                }
            }
        }
        if saved_count > 0 && limit < 1000 {
            println!("[Storage] Auto-saved {} chunks", saved_count);
        }
    }

    pub fn save_all(&mut self) {
        let mut saved_count = 0;
        for (pos, chunk) in self.world.chunks_mut() {
            if chunk.persistence_dirty {
                let serialized = storage::format::SerializedChunk::from(&*chunk);
                self.storage.save_chunk(*pos, serialized);
                chunk.persistence_dirty = false;
                saved_count += 1;
            }
        }
        println!("[Storage] Saved {} chunks to disk", saved_count);
        self.save_metadata();
    }
}

impl App {
    /// Returns the currently selected block from the hotbar.
    fn selected_block(&self) -> BlockType {
        self.ui.hotbar_blocks[self.ui.hotbar_index]
    }

    /// Move the player upward in small steps until no collision, to safely exit fly mode.
    fn resolve_player_overlap(&mut self) {
        let mut feet = self
            .sim
            .player
            .feet_pos(self.sim.world_extent, self.sim.texture_origin);
        for _ in 0..12 {
            if !self
                .sim
                .player
                .check_collision(feet, &self.sim.world, &self.sim.model_registry)
            {
                break;
            }
            feet.y += 0.25;
        }
        self.sim
            .player
            .set_feet_pos(feet, self.sim.world_extent, self.sim.texture_origin);
    }

    fn toggle_palette_panel(&mut self) {
        self.ui.palette_open = !self.ui.palette_open;
        if self.ui.palette_open {
            self.ui.palette_previously_focused = self.input.focused;
            self.input.focused = false;
            self.input.pending_grab = Some(false);
            macos_cursor::release_and_show();
            self.ui.dragging_item = None;
        } else if self.ui.palette_previously_focused {
            self.input.focused = true;
            self.input.pending_grab = Some(true);
            macos_cursor::grab_and_hide();
            self.ui.palette_previously_focused = false;
        }
    }

    fn new(event_loop: &EventLoop<()>) -> Self {
        // Parse command line arguments
        let args = Args::parse();

        if args.verbose {
            println!("CLI Args: {:?}", args);
        }

        if args.generate_sprites {
            match sprite_gen::run(&args, event_loop) {
                Ok(()) => process::exit(0),
                Err(e) => {
                    eprintln!("[sprites] failed: {e}");
                    process::exit(1);
                }
            }
        }

        // Determine world name
        let mut launcher_config = launcher_config::LauncherConfig::load();
        let world_name = args
            .world
            .clone()
            .or(launcher_config.last_world.clone())
            .unwrap_or_else(|| "default".to_string());

        println!("[Launcher] Loading world: '{}'", world_name);
        launcher_config.update_last_world(&world_name);

        let worlds_dir = PathBuf::from("worlds");
        let world_dir = worlds_dir.join(&world_name);

        // Migration: If 'world' exists but 'worlds/default' doesn't, migrate it
        if PathBuf::from("world").exists() && !world_dir.exists() && world_name == "default" {
            println!("[Launcher] Migrating legacy world to 'worlds/default'...");
            if !worlds_dir.exists() {
                std::fs::create_dir_all(&worlds_dir).expect("Failed to create worlds directory");
            }
            std::fs::rename("world", &world_dir).expect("Failed to migrate legacy world");
        }

        if !world_dir.exists() {
            std::fs::create_dir_all(&world_dir).expect("Failed to create world directory");
        }

        let metadata_path = world_dir.join("level.dat");
        let mut seed = args.seed.unwrap_or(12345);
        let mut initial_time_of_day = DEFAULT_TIME_OF_DAY;
        let mut initial_day_paused = true; // Default
        let mut initial_player_data = None;

        if metadata_path.exists() {
            if let Ok(meta) = storage::metadata::WorldMetadata::load(&metadata_path) {
                println!("[Storage] Loaded world metadata. Seed: {}", meta.seed);
                seed = meta.seed;
                initial_time_of_day = meta.time_of_day;
                initial_day_paused = meta.day_cycle_paused;
                initial_player_data = meta.player;
            }
        } else {
            let meta = storage::metadata::WorldMetadata {
                seed,
                spawn_pos: [0.0, 64.0, 0.0], // Initial guess, will be updated
                version: 1,
                time_of_day: DEFAULT_TIME_OF_DAY,
                day_cycle_paused: true,
                player: None,
            };
            let _ = meta.save(&metadata_path);
            println!("[Storage] Created new world metadata. Seed: {}", seed);
        }

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

        // Texture origin: the world position that maps to texture coordinate (0,0,0)
        // For infinite worlds, center the texture on the spawn chunk
        let spawn_chunk_x = spawn_block_x.div_euclid(CHUNK_SIZE as i32);
        let spawn_chunk_z = spawn_block_z.div_euclid(CHUNK_SIZE as i32);

        let texture_origin = Vector3::new(
            (spawn_chunk_x - LOADED_CHUNKS_X / 2) * CHUNK_SIZE as i32,
            0, // Y always starts at 0
            (spawn_chunk_z - LOADED_CHUNKS_Z / 2) * CHUNK_SIZE as i32,
        );

        // Initialize world
        let spawn_chunk = vector![spawn_chunk_x, 0, spawn_chunk_z];

        let storage = Arc::new(storage::worker::StorageSystem::new(world_dir.clone()));

        // Create world with only chunks near spawn loaded, checking storage first
        let world = create_initial_world_with_seed(spawn_chunk, seed, Some(&storage));

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
        let spawn_pos = if let Some(ref player_data) = initial_player_data {
            Vector3::new(
                player_data.position[0],
                player_data.position[1] + PLAYER_EYE_HEIGHT,
                player_data.position[2],
            )
        } else {
            let spawn_x = 0;
            let spawn_z = 0;
            let spawn_y = find_ground_level(&world, spawn_x, spawn_z);
            Vector3::new(spawn_x as f64, spawn_y as f64 + 1.0, spawn_z as f64)
        };

        let mut player = Player::new(spawn_pos, texture_origin, world_extent, args.fly_mode);
        player.auto_jump = true;

        // Restore rotation if available
        if let Some(p) = initial_player_data {
            player.camera.rotation.y = p.yaw as f64;
            player.camera.rotation.x = p.pitch as f64;
        }

        println!(
            "Voxel Game started! Click to focus, then use WASD to move, mouse to look, left/right click to edit blocks."
        );

        let graphics = Graphics {
            instance: vk.instance,
            device: vk.device,
            queue: vk.queue,
            memory_allocator: vk.memory_allocator,
            descriptor_set_allocator: vk.descriptor_set_allocator,
            command_buffer_allocator: vk.command_buffer_allocator,
            render_pipeline,
            resample_pipeline,
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
            voxel_image,
            model_atlas,
            model_metadata,
            rcx: None,
        };

        let sim = WorldSim {
            world,
            model_registry,
            player,
            world_extent,
            texture_origin,
            last_player_chunk: spawn_chunk,
            chunk_stats: ChunkStats::default(),
            chunk_loader: {
                let terrain = TerrainGenerator::new(seed);
                let storage_clone = Arc::clone(&storage);
                ChunkLoader::new(
                    move |pos| generate_chunk_terrain(&terrain, pos),
                    Some(storage_clone),
                )
            },
            storage,
            particles: ParticleSystem::new(),
            falling_blocks: FallingBlockSystem::new(),
            block_updates: BlockUpdateQueue::new(32),
            water_grid: WaterGrid::new(),
            time_of_day: if args.time_of_day.is_some() {
                args.time_of_day.map(|t| t as f32).unwrap()
            } else {
                initial_time_of_day
            },
            day_cycle_paused: initial_day_paused,
            atmosphere: atmosphere::AtmosphereSettings::default(),
            animation_time: 0.0,
            render_mode: match args.render_mode.as_deref() {
                Some("normal") => RenderMode::Normal,
                Some("coord") => RenderMode::Coord,
                Some("steps") => RenderMode::Steps,
                Some("uv") => RenderMode::UV,
                Some("depth") => RenderMode::Depth,
                _ => RenderMode::Textured,
            },
            view_distance,
            unload_distance,
            profiler: Profiler::default(),
            metadata_state: MetadataState::new(texture_origin),
            last_save: Instant::now(),
            world_dir: world_dir.clone(),
            seed,
        };

        let start_time = Instant::now();

        let ui = UiState {
            settings: Settings {
                show_chunk_boundaries: args.show_chunk_boundaries,
                render_scale: 0.75,
                ..Settings::default()
            },
            window_size: INITIAL_WINDOW_RESOLUTION.into(),
            start_time,
            profile_log_path: args.profile_log.clone(),
            profile_log_header_written: false,
            show_minimap: false,
            minimap: Minimap::new(),
            minimap_cached_image: None,
            minimap_last_pos: Vector3::new(i32::MAX, 0, i32::MAX),
            minimap_last_update: Instant::now(),
            minimap_last_yaw: f32::MAX,
            palette_open: false,
            palette_tab: PaletteTab::default(),
            palette_previously_focused: false,
            dragging_item: None,
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
            place_needs_reclick: false,
            model_needs_reclick: false,
            gate_needs_reclick: false,
            line_start_pos: None,
            line_locked_axis: None,
            last_second: Instant::now(),
            frames_since_last_second: 0,
            fps: 0,
            total_frames: 0,
            screenshot_taken: false,
        };

        let input = InputState {
            helper: input,
            focused: false,
            pending_grab: None,
        };

        App {
            args,
            start_time,
            graphics,
            sim,
            ui,
            input,
        }
    }

    /// Checks if texture origin needs to shift and handles re-upload if necessary.
    /// Returns true if a shift occurred.
    fn update(&mut self, event_loop: &ActiveEventLoop) {
        self.ui.total_frames += 1;
        let now = Instant::now();

        // Check for screenshot delay
        if let Some(delay) = self.args.screenshot_delay {
            if !self.ui.screenshot_taken {
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

        if now.duration_since(self.ui.last_second) > Duration::from_secs(1) {
            self.ui.fps = self.ui.frames_since_last_second;
            self.ui.frames_since_last_second = 0;
            self.ui.last_second = now;

            app_stats::print_stats(&mut self.ui, &mut self.sim, self.args.verbose);
        }
        self.ui.frames_since_last_second += 1;

        // Debug interval output
        if self.args.debug_interval > 0
            && self.ui.total_frames % self.args.debug_interval as u64 == 0
        {
            let player_pos = self
                .sim
                .player
                .feet_pos(self.sim.world_extent, self.sim.texture_origin);
            let player_chunk = self
                .sim
                .player
                .get_chunk_pos(self.sim.world_extent, self.sim.texture_origin);
            println!(
                "[DEBUG Frame {}] Pos: ({:.2}, {:.2}, {:.2}) Chunk: ({}, {}, {}) TexOrigin: ({}, {}, {}) Velocity: ({:.2}, {:.2}, {:.2})",
                self.ui.total_frames,
                player_pos.x,
                player_pos.y,
                player_pos.z,
                player_chunk.x,
                player_chunk.y,
                player_chunk.z,
                self.sim.texture_origin.x,
                self.sim.texture_origin.y,
                self.sim.texture_origin.z,
                self.sim.player.velocity.x,
                self.sim.player.velocity.y,
                self.sim.player.velocity.z
            );
        }

        // Always update chunks and upload to GPU, even before delta_time is available
        // This ensures initial chunks are uploaded on the first frame
        let t0 = Instant::now();
        self.update_chunk_loading();
        self.sim.profiler.chunk_loading_us += t0.elapsed().as_micros() as u64;

        let t1 = Instant::now();
        self.upload_world_to_gpu();
        self.sim.profiler.gpu_upload_us += t1.elapsed().as_micros() as u64;

        self.sim.auto_save();

        // Amortized metadata refresh runs once per frame.
        self.update_metadata_buffers();

        let Some(delta_time) = self.input.delta_time().as_ref().map(Duration::as_secs_f64) else {
            return;
        };

        if self.input.close_requested() {
            println!("[Storage] Saving world before exit...");
            self.sim.save_all();
            event_loop.exit();
            return;
        }

        if self.handle_focus_toggles() {
            return;
        }

        self.handle_global_shortcuts();

        if !self.ui.palette_open && self.ui.palette_previously_focused && !self.input.focused {
            self.input.focused = true;
            self.input.pending_grab = Some(true);
            self.ui.palette_previously_focused = false;
        }
        if !self.ui.palette_open {
            self.ui.dragging_item = None;
        }

        // Update day/night cycle
        if !self.sim.day_cycle_paused {
            self.sim.time_of_day += delta_time as f32 / DAY_CYCLE_DURATION;
            self.sim.time_of_day = self.sim.time_of_day.rem_euclid(1.0);
        }

        // Update animation time (always advances for water waves, etc.)
        self.sim.animation_time += delta_time as f32;

        // Update particle system with world collision
        // Note: X and Z can be any value in an infinite world, only Y has bounds
        let world = &self.sim.world;
        self.sim.particles.update(delta_time as f32, |x, y, z| {
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
        let landed = self
            .sim
            .falling_blocks
            .update(delta_time as f32, |x, y, z| {
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
            .sim
            .player
            .feet_pos(self.sim.world_extent, self.sim.texture_origin)
            .cast::<f32>();
        self.sim.block_updates.process_updates(
            &mut self.sim.world,
            &mut self.sim.falling_blocks,
            &mut self.sim.particles,
            &self.sim.model_registry,
            player_pos_f32,
        );

        // Process water flow simulation (frame-distributed)
        if self.ui.settings.water_simulation_enabled {
            let player_pos_f32 = self
                .sim
                .player
                .feet_pos(self.sim.world_extent, self.sim.texture_origin)
                .cast::<f32>();
            self.sim
                .water_grid
                .process_simulation(&mut self.sim.world, player_pos_f32);
        }

        self.handle_focused_controls(delta_time);
        self.handle_block_interactions(delta_time as f32);
    }

    fn render(&mut self, _event_loop: &ActiveEventLoop) {
        let t_render_start = Instant::now();
        self.graphics.render_pipeline.maybe_reload();
        self.graphics.resample_pipeline.maybe_reload();

        // Collect data before borrowing rcx (avoids borrow checker issues)
        let gpu_lights = self.sim.world.collect_torch_lights(
            self.sim.player.light_enabled,
            self.sim.player.camera.position,
            self.sim.texture_origin,
            &self.sim.model_registry,
            self.sim.world_extent,
        );
        let light_count = gpu_lights.len() as u32;
        let player_world_pos = self
            .sim
            .player
            .feet_pos(self.sim.world_extent, self.sim.texture_origin);
        let selected_block = self.selected_block();

        // Pre-generate minimap image if showing (before entering gui closure)
        // Throttle updates based on position change and rotation change
        let camera_yaw = self.sim.player.camera.rotation.y as f32;
        let minimap_image = app_minimap::prepare_minimap_image(
            &mut self.ui,
            &mut self.sim,
            player_world_pos,
            camera_yaw,
        );

        let rcx = self.graphics.rcx.as_mut().unwrap();

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
            self.ui.window_size = window_extent;
            let render_extent = [
                (window_extent[0] as f32 * self.ui.settings.render_scale) as u32,
                (window_extent[1] as f32 * self.ui.settings.render_scale) as u32,
            ];
            (
                rcx.render_image,
                rcx.render_set,
                rcx.resample_image,
                rcx.resample_set,
            ) = get_images_and_sets(
                self.graphics.memory_allocator.clone(),
                self.graphics.descriptor_set_allocator.clone(),
                &self.graphics.render_pipeline,
                &self.graphics.resample_pipeline,
                render_extent,
                window_extent,
            );

            // Recreate distance buffer for two-pass beam optimization
            (rcx.distance_image, rcx.distance_set) = get_distance_image_and_set(
                self.graphics.memory_allocator.clone(),
                self.graphics.descriptor_set_allocator.clone(),
                &self.graphics.render_pipeline,
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

        if app_hud::render_hud(
            rcx,
            &mut self.ui,
            &mut self.sim,
            selected_block,
            minimap_image,
            camera_yaw,
            player_world_pos,
        ) {
            let window_extent: [u32; 2] = rcx.window.inner_size().into();
            let render_extent = [
                (window_extent[0] as f32 * self.ui.settings.render_scale) as u32,
                (window_extent[1] as f32 * self.ui.settings.render_scale) as u32,
            ];
            (
                rcx.render_image,
                rcx.render_set,
                rcx.resample_image,
                rcx.resample_set,
            ) = get_images_and_sets(
                self.graphics.memory_allocator.clone(),
                self.graphics.descriptor_set_allocator.clone(),
                &self.graphics.render_pipeline,
                &self.graphics.resample_pipeline,
                render_extent,
                window_extent,
            );
            // Recreate distance buffer for two-pass beam optimization
            (rcx.distance_image, rcx.distance_set) = get_distance_image_and_set(
                self.graphics.memory_allocator.clone(),
                self.graphics.descriptor_set_allocator.clone(),
                &self.graphics.render_pipeline,
                render_extent,
            );
        }

        let render_extent = rcx.render_image.extent();
        let resample_extent = rcx.resample_image.extent();
        self.sim.player.camera.extent = [render_extent[0] as f64, render_extent[1] as f64];

        let pixel_to_ray = self.sim.player.camera.pixel_to_ray_matrix();

        // Scale only the position (column 4), not the direction (3x3 rotation part)
        // This prevents ray distortion from non-uniform world dimensions
        let mut pixel_to_ray_scaled = pixel_to_ray;
        // Camera position is normalized (0-1), scale to texture size
        // Ray marching happens in texture space (0 to textureSize)
        pixel_to_ray_scaled.m14 *= self.sim.world_extent[0] as f64;
        pixel_to_ray_scaled.m24 *= self.sim.world_extent[1] as f64;
        pixel_to_ray_scaled.m34 *= self.sim.world_extent[2] as f64;

        // Apply head bob offset to camera Y position for rendering
        let head_bob_offset = (self.sim.player.head_bob_timer * std::f64::consts::TAU).sin()
            * HEAD_BOB_AMPLITUDE
            * self.sim.player.head_bob_intensity;
        pixel_to_ray_scaled.m24 += head_bob_offset;

        let pixel_to_ray = pixel_to_ray_scaled;

        // Convert world coordinates to texture coordinates for shader
        // Shader works in texture space, so we subtract texture_origin
        let tex_origin = self.sim.texture_origin;
        let world_to_tex = |world_pos: Vector3<i32>| -> (i32, i32, i32) {
            (
                world_pos.x - tex_origin.x,
                world_pos.y - tex_origin.y,
                world_pos.z - tex_origin.z,
            )
        };

        let (break_x, break_y, break_z) = self
            .ui
            .breaking_block
            .map(&world_to_tex)
            .unwrap_or((-1, -1, -1));

        // Calculate preview block position (where block would be placed)
        let selected_block_id = selected_block as u32;
        // Use player_world_pos computed earlier (before rcx borrow)
        let (preview_x, preview_y, preview_z, preview_type) = if self.ui.settings.show_block_preview
        {
            self.ui
                .current_hit
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
        let (target_x, target_y, target_z) = if self.ui.settings.show_target_outline {
            self.ui
                .current_hit
                .as_ref()
                .map(|hit| world_to_tex(hit.block_pos))
                .unwrap_or((-1, -1, -1))
        } else {
            (-1, -1, -1) // Outline disabled
        };

        // Update particle buffer (convert world coords to texture coords)
        let gpu_particles = self.sim.particles.gpu_data();
        let particle_count = gpu_particles.len() as u32;
        {
            let tex_origin = self.sim.texture_origin;
            let mut write = self.graphics.particle_buffer.write().unwrap();
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
        let gpu_falling_blocks = self.sim.falling_blocks.gpu_data();
        {
            let tex_origin = self.sim.texture_origin;
            let mut write = self.graphics.falling_block_buffer.write().unwrap();
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
            let mut write = self.graphics.light_buffer.write().unwrap();
            for (i, l) in gpu_lights.iter().enumerate() {
                write[i] = *l;
            }
        }

        let push_constants = PushConstants {
            pixel_to_ray: pixel_to_ray.cast(),
            texture_size_x: self.sim.world_extent[0],
            texture_size_y: self.sim.world_extent[1],
            texture_size_z: self.sim.world_extent[2],
            render_mode: self.sim.render_mode as u32,
            show_chunk_boundaries: self.ui.settings.show_chunk_boundaries as u32,
            player_in_water: self.sim.player.in_water as u32,
            time_of_day: self.sim.time_of_day,
            animation_time: self.sim.animation_time,
            cloud_speed: self.sim.atmosphere.cloud_speed,
            break_block_x: break_x,
            break_block_y: break_y,
            break_block_z: break_z,
            break_progress: self.ui.break_progress,
            particle_count,
            preview_block_x: preview_x,
            preview_block_y: preview_y,
            preview_block_z: preview_z,
            preview_block_type: preview_type,
            light_count,
            ambient_light: self.sim.atmosphere.ambient_light,
            fog_density: self.sim.atmosphere.fog_density,
            fog_start: self.sim.atmosphere.fog_start,
            fog_overlay_scale: self.sim.atmosphere.fog_overlay_scale,
            target_block_x: target_x,
            target_block_y: target_y,
            target_block_z: target_z,
            max_ray_steps: self.ui.settings.max_ray_steps,
            texture_origin_x: self.sim.texture_origin.x,
            texture_origin_y: self.sim.texture_origin.y,
            texture_origin_z: self.sim.texture_origin.z,
            enable_ao: if self.ui.settings.enable_ao { 1 } else { 0 },
            enable_shadows: if self.ui.settings.enable_shadows {
                1
            } else {
                0
            },
            enable_model_shadows: if self.ui.settings.enable_model_shadows {
                1
            } else {
                0
            },
            enable_point_lights: if self.ui.settings.enable_point_lights {
                1
            } else {
                0
            },
            transparent_background: 0,
            pass_mode: 0, // Will be set per-pass
            lod_ao_distance: self.ui.settings.lod_ao_distance,
            lod_shadow_distance: self.ui.settings.lod_shadow_distance,
            lod_point_light_distance: self.ui.settings.lod_point_light_distance,
            falling_block_count: self.sim.falling_blocks.count() as u32,
            camera_pos: {
                let cam = self
                    .sim
                    .player
                    .camera_world_pos(self.sim.world_extent, self.sim.texture_origin);
                [cam.x as f32, cam.y as f32, cam.z as f32, 0.0]
            },
        };

        let mut builder = AutoCommandBufferBuilder::primary(
            self.graphics.command_buffer_allocator.clone(),
            self.graphics.queue.queue_family_index(),
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
            .bind_pipeline_compute(self.graphics.render_pipeline.clone())
            .unwrap()
            .push_constants(
                self.graphics.render_pipeline.layout().clone(),
                0,
                push_constants,
            )
            .unwrap()
            .bind_descriptor_sets(
                PipelineBindPoint::Compute,
                self.graphics.render_pipeline.layout().clone(),
                0,
                vec![
                    rcx.render_set.clone(),
                    self.graphics.voxel_set.clone(),
                    self.graphics.texture_set.clone(),
                    self.graphics.particle_set.clone(),
                    self.graphics.light_set.clone(),
                    self.graphics.chunk_metadata_set.clone(),
                    rcx.distance_set.clone(),
                    self.graphics.brick_and_model_set.clone(), // Combined set 7: brick + model resources
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
            .bind_pipeline_compute(self.graphics.resample_pipeline.clone())
            .unwrap()
            .bind_descriptor_sets(
                PipelineBindPoint::Compute,
                self.graphics.resample_pipeline.layout().clone(),
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
            .then_execute(self.graphics.queue.clone(), command_buffer)
            .unwrap();

        let gui_future = rcx
            .gui
            .draw_on_image(render_future, rcx.image_views[image_index as usize].clone());

        gui_future
            .then_swapchain_present(
                self.graphics.queue.clone(),
                SwapchainPresentInfo::swapchain_image_index(rcx.swapchain.clone(), image_index),
            )
            .then_signal_fence_and_flush()
            .unwrap()
            .wait(None)
            .unwrap();

        // Check if we need to take a screenshot (do this before the borrow is released)
        let needs_screenshot = if let Some(delay) = self.args.screenshot_delay {
            if !self.ui.screenshot_taken {
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
                &self.graphics.device,
                &self.graphics.queue,
                &self.graphics.memory_allocator,
                &self.graphics.command_buffer_allocator,
                &image_view,
                "voxel_world_screen_shot.png",
            );
            self.ui.screenshot_taken = true;
        }

        // Record render time and increment sample count
        self.sim.profiler.render_us += t_render_start.elapsed().as_micros() as u64;
        self.sim.profiler.sample_count += 1;
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
                        .with_title("Voxel World"),
                )
                .unwrap(),
        );
        let surface = Surface::from_window(self.graphics.instance.clone(), window.clone()).unwrap();

        let (swapchain, images) = get_swapchain_images(&self.graphics.device, &surface, &window);
        let image_views = images
            .iter()
            .map(|i| ImageView::new(i.clone(), ImageViewCreateInfo::from_image(i)).unwrap())
            .collect::<Vec<_>>();

        let window_extent: [u32; 2] = window.inner_size().into();
        let render_extent = [
            (window_extent[0] as f32 * self.ui.settings.render_scale) as u32,
            (window_extent[1] as f32 * self.ui.settings.render_scale) as u32,
        ];
        let (render_image, render_set, resample_image, resample_set) = get_images_and_sets(
            self.graphics.memory_allocator.clone(),
            self.graphics.descriptor_set_allocator.clone(),
            &self.graphics.render_pipeline,
            &self.graphics.resample_pipeline,
            render_extent,
            window_extent,
        );

        // Create distance buffer for two-pass beam optimization
        let (distance_image, distance_set) = get_distance_image_and_set(
            self.graphics.memory_allocator.clone(),
            self.graphics.descriptor_set_allocator.clone(),
            &self.graphics.render_pipeline,
            render_extent,
        );

        let mut gui = Gui::new(
            event_loop,
            surface,
            self.graphics.queue.clone(),
            swapchain.image_format(),
            GuiConfig {
                is_overlay: true,
                ..Default::default()
            },
        );

        // Register the texture atlas with egui for HUD display
        let atlas_texture_id = gui.register_user_image_view(
            self.graphics.texture_atlas_view.clone(),
            SamplerCreateInfo {
                mag_filter: Filter::Nearest,
                min_filter: Filter::Nearest,
                address_mode: [SamplerAddressMode::ClampToEdge; 3],
                ..Default::default()
            },
        );
        let sprite_icons = gpu_resources::load_sprite_icons(&mut gui);

        let recreate_swapchain = false;

        self.graphics.rcx = Some(gpu_resources::RenderContext {
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
            sprite_icons,

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
        if !self.graphics.rcx.as_mut().unwrap().gui.update(&event) {
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
        if let Some(grab) = self.input.pending_grab.take() {
            if grab {
                macos_cursor::grab_and_hide();
                println!("Cursor grabbed and hidden (native macOS API)");
            } else {
                macos_cursor::release_and_show();
                println!("Cursor released and shown (native macOS API)");
            }
        }

        let rcx = self.graphics.rcx.as_mut().unwrap();
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

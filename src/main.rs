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
use noise::{Fbm, MultiFractal, NoiseFn, Perlin, RidgedMulti};
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
        AutoCommandBufferBuilder, BlitImageInfo, BufferImageCopy, ClearColorImageInfo,
        CommandBufferUsage, CopyBufferToImageInfo, PrimaryCommandBufferAbstract,
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

mod block_update;
mod camera;
mod chunk;
mod chunk_loader;
mod falling_block;
mod hot_reload;
mod particles;
mod raycast;
mod sub_voxel;
mod svt;
mod water;
mod world;

use crate::block_update::{BlockUpdateQueue, BlockUpdateType};
use crate::camera::Camera;
use crate::chunk::{BlockType, CHUNK_SIZE, Chunk};
use crate::chunk_loader::ChunkLoader;
use crate::falling_block::{FallingBlockSystem, GpuFallingBlock, MAX_FALLING_BLOCKS};
use crate::hot_reload::HotReloadComputePipeline;
use crate::particles::ParticleSystem;
use crate::raycast::{MAX_RAYCAST_DISTANCE, RaycastHit, get_place_position, raycast};
use crate::sub_voxel::{MAX_MODELS, ModelRegistry, PALETTE_SIZE, SUB_VOXEL_SIZE};
use crate::svt::ChunkSVT;
use crate::water::WaterGrid;
use crate::world::World;

/// Voxel Game Engine - A Minecraft-like voxel game with GPU ray-marching rendering.
#[derive(Parser, Debug, Clone)]
#[command(name = "voxel_ray_traversal")]
#[command(version, about, long_about = None)]
struct Args {
    /// Spawn X coordinate in world blocks (default: auto-find suitable location)
    #[arg(long, short = 'x')]
    spawn_x: Option<i32>,

    /// Spawn Z coordinate in world blocks (default: auto-find suitable location)
    #[arg(long, short = 'z')]
    spawn_z: Option<i32>,

    /// Take screenshot after N seconds and save to voxel_world_screen_shot.png
    #[arg(long, short = 's')]
    screenshot_delay: Option<f64>,

    /// Print debug info every N frames (0 = off)
    #[arg(long, short = 'd', default_value_t = 0)]
    debug_interval: u32,

    /// Start in fly mode
    #[arg(long, short = 'f')]
    fly_mode: bool,

    /// Pause day/night cycle at specific time (0.0-1.0, where 0.5 = noon)
    #[arg(long, short = 't')]
    time_of_day: Option<f64>,

    /// Enable chunk boundary visualization
    #[arg(long, short = 'b')]
    show_chunk_boundaries: bool,

    /// Set view distance in chunks (default: 6)
    #[arg(long, short = 'v')]
    view_distance: Option<i32>,

    /// Seed for terrain generation (default: 12345)
    #[arg(long, short = 'S')]
    seed: Option<u32>,

    /// Start in render mode: textured, normal, coord, steps, uv, depth (default: textured)
    #[arg(long, short = 'r')]
    render_mode: Option<String>,

    /// Verbose debug output to console
    #[arg(long)]
    verbose: bool,
}

const INITIAL_WINDOW_RESOLUTION: PhysicalSize<u32> = PhysicalSize::new(1200, 1080);

// World height in chunks (fixed - Y dimension is bounded)
const WORLD_CHUNKS_Y: i32 = 4;

// Texture pool dimensions for loaded chunks (X and Z are centered on player)
// This defines how many chunks can be loaded at once, not world bounds
const LOADED_CHUNKS_X: i32 = 16; // Chunks loaded in X direction (8 each side of player)
const LOADED_CHUNKS_Z: i32 = 16; // Chunks loaded in Z direction (8 each side of player)

// GPU texture size in blocks (holds all currently loaded chunks)
const TEXTURE_SIZE_X: usize = LOADED_CHUNKS_X as usize * CHUNK_SIZE;
const TEXTURE_SIZE_Y: usize = WORLD_CHUNKS_Y as usize * CHUNK_SIZE;
const TEXTURE_SIZE_Z: usize = LOADED_CHUNKS_Z as usize * CHUNK_SIZE;

// Terrain generation constants
/// Sea level for water filling (blocks below this in valleys become water)
const SEA_LEVEL: i32 = 28;

// Chunk streaming constants
/// View distance in chunks (horizontal - all Y levels loaded within this range)
const VIEW_DISTANCE: i32 = 6;
/// Unload distance in chunks (horizontal - chunks beyond this are unloaded)
const UNLOAD_DISTANCE: i32 = 7;
/// Maximum chunks to load or unload per frame
const CHUNKS_PER_FRAME: usize = 4;

/// Cached empty chunk data for GPU clearing (avoids repeated allocations)
static EMPTY_CHUNK_DATA: std::sync::LazyLock<Vec<u8>> =
    std::sync::LazyLock::new(|| vec![0u8; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE]);

/// Cached empty model metadata for GPU clearing (2 bytes per block: model_id + rotation)
static EMPTY_MODEL_METADATA: std::sync::LazyLock<Vec<u8>> =
    std::sync::LazyLock::new(|| vec![0u8; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE * 2]);

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
const PLAYER_HEIGHT: f64 = 1.6; // Reduced from 1.7 for better cave navigation
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

// Ladder/climbing constants
/// Vertical climb speed (when pressing Space to climb up)
const CLIMB_UP_SPEED: f64 = 4.0;
/// Vertical climb speed (when pressing Shift to climb down)
const CLIMB_DOWN_SPEED: f64 = 3.0;
/// Horizontal movement speed while on ladder (slower than walking)
const CLIMB_HORIZ_SPEED: f64 = 2.0;

// Day/night cycle constants
/// Duration of a full day cycle in seconds (real time)
const DAY_CYCLE_DURATION: f32 = 120.0;
/// Default time of day (0.0 = 6am, 0.5 = 6pm, formula: hours = (v * 24 + 6) % 24)
/// 0.583 = 20:00 (8pm)
const DEFAULT_TIME_OF_DAY: f32 = 0.583;

/// Head bob amplitude (in blocks)
const HEAD_BOB_AMPLITUDE: f64 = 0.04;
/// Head bob frequency (cycles per block walked)
const HEAD_BOB_FREQUENCY: f64 = 0.8;

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

/// Render modes for debugging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
enum RenderMode {
    Coord = 0,
    Steps = 1,
    #[default]
    Textured = 2,
    Normal = 3,
    UV = 4,
    Depth = 5,
    BrickDebug = 6,
    ShadowDebug = 7,
}

impl RenderMode {
    pub const ALL: &'static [RenderMode] = &[
        RenderMode::Coord,
        RenderMode::Steps,
        RenderMode::Textured,
        RenderMode::Normal,
        RenderMode::UV,
        RenderMode::Depth,
        RenderMode::BrickDebug,
        RenderMode::ShadowDebug,
    ];
}

/// Terrain generator using multiple noise layers for varied landscapes
#[derive(Clone)]
struct TerrainGenerator {
    height_noise: Fbm<Perlin>,
    detail_noise: Perlin,
    mountain_noise: RidgedMulti<Perlin>,
    biome_noise: Perlin,
    cave_noise: Perlin,
    cave_mask_noise: Perlin,
    entrance_noise: Perlin,
}

impl TerrainGenerator {
    fn new(seed: u32) -> Self {
        // Base continental noise for large-scale terrain features
        let height_noise = Fbm::<Perlin>::new(seed)
            .set_octaves(4)
            .set_frequency(0.003) // Very low frequency for continent-scale features
            .set_lacunarity(2.0)
            .set_persistence(0.5);

        let detail_noise = Perlin::new(seed.wrapping_add(1));

        // Mountain ridges using RidgedMulti for sharp peaks
        let mountain_noise = RidgedMulti::<Perlin>::new(seed.wrapping_add(2))
            .set_octaves(5)
            .set_frequency(0.008) // Mountain-scale features
            .set_lacunarity(2.2)
            .set_persistence(0.5);

        // Biome noise - determines flat plains vs hilly vs mountainous regions
        // Very low frequency for large biome regions
        let biome_noise = Perlin::new(seed.wrapping_add(6));

        // 3D noise for cave carving
        let cave_noise = Perlin::new(seed.wrapping_add(3));

        // Regional variation in cave density
        let cave_mask_noise = Perlin::new(seed.wrapping_add(4));

        // Noise for cave entrance locations (~25% of cave areas get entrances)
        let entrance_noise = Perlin::new(seed.wrapping_add(5));

        Self {
            height_noise,
            detail_noise,
            mountain_noise,
            biome_noise,
            cave_noise,
            cave_mask_noise,
            entrance_noise,
        }
    }

    /// Get terrain height at world coordinates
    fn get_height(&self, world_x: i32, world_z: i32) -> i32 {
        let x = world_x as f64;
        let z = world_z as f64;

        // Biome type: determines flat plains (-1) vs rolling hills (0) vs mountains (+1)
        // Very low frequency for large coherent regions
        let biome_raw = self.biome_noise.get([x * 0.004, z * 0.004]);

        // Create distinct biome zones with sharper transitions
        // Values < -0.3 = flat plains, > 0.3 = mountains, between = rolling hills
        let biome_type = if biome_raw < -0.3 {
            0.0 // Flat plains
        } else if biome_raw > 0.3 {
            1.0 // Mountains
        } else {
            // Smooth transition zone (rolling hills)
            ((biome_raw + 0.3) / 0.6).clamp(0.0, 1.0)
        };

        // Base continental terrain (large smooth features)
        let base = self.height_noise.get([x, z]);

        // Mountain ridges (sharp peaks)
        let ridges = self.mountain_noise.get([x, z]);

        // Detail noise for subtle variation
        let detail = self.detail_noise.get([x * 0.02, z * 0.02]);

        // Calculate height based on biome type:
        // - Flat plains: height 32-36 with minimal variation
        // - Rolling hills: height 28-45 with moderate variation
        // - Mountains: height 32-90 with dramatic peaks
        let height = if biome_type < 0.1 {
            // Flat plains - very little variation
            32.0 + detail * 2.0
        } else if biome_type > 0.9 {
            // Mountain biome - dramatic peaks
            let mountain_height = ridges * 55.0;
            32.0 + base * 6.0 + mountain_height
        } else {
            // Transition zone - blend between plains and mountains
            let plains_height = 32.0 + detail * 2.0;
            let hills_height = 32.0 + base * 10.0 + detail * 3.0;
            let mountain_height = 32.0 + base * 6.0 + ridges * 55.0;

            // Smooth blend based on biome_type
            if biome_type < 0.5 {
                // Plains to hills transition
                let t = biome_type / 0.5;
                plains_height * (1.0 - t) + hills_height * t
            } else {
                // Hills to mountains transition
                let t = (biome_type - 0.5) / 0.5;
                hills_height * (1.0 - t) + mountain_height * t
            }
        };

        height.round() as i32
    }

    /// Check if a location is a cave entrance point (~25% of cave areas)
    fn is_entrance(&self, world_x: i32, world_z: i32) -> bool {
        let x = world_x as f64;
        let z = world_z as f64;

        // Low frequency noise for sparse, grouped entrance locations
        // Use multiple octaves for varied entrance sizes
        let entrance_value = self.entrance_noise.get([x * 0.02, z * 0.02]);

        // Threshold of 0.45 gives roughly 25-30% coverage
        // Higher threshold = fewer entrances
        entrance_value > 0.45
    }

    /// Check if a position should be carved out as a cave
    fn is_cave(&self, world_x: i32, world_y: i32, world_z: i32, surface_height: i32) -> bool {
        // Determine surface buffer based on whether this is an entrance location
        // Entrances reduce the buffer to allow caves to breach the surface
        let is_entrance = self.is_entrance(world_x, world_z);
        let surface_buffer = if is_entrance { 0 } else { 5 };

        // Don't carve near surface unless at entrance, and never below y=2
        if world_y > surface_height - surface_buffer || world_y < 2 {
            return false;
        }

        let x = world_x as f64;
        let y = world_y as f64;
        let z = world_z as f64;

        // Regional cave density (some areas have more caves)
        let cave_density = self.cave_mask_noise.get([x * 0.01, z * 0.01]) * 0.5 + 0.5;

        // 3D cave noise - "spaghetti" style caves
        // Stretched in Y for more horizontal tunnels
        let cave_value = self.cave_noise.get([x * 0.05, y * 0.08, z * 0.05]);

        // Threshold varies by depth (more caves deeper down)
        let depth_factor = ((surface_height - world_y) as f64 / 30.0).clamp(0.0, 1.0);
        let threshold = 0.55 - (depth_factor * 0.15) - (cave_density * 0.1);

        cave_value.abs() > threshold
    }

    /// Simple hash for tree placement randomness
    fn hash(&self, x: i32, z: i32) -> i32 {
        let mut h = (x.wrapping_mul(374761393)) ^ (z.wrapping_mul(668265263));
        h = (h ^ (h >> 13)).wrapping_mul(1274126177);
        (h ^ (h >> 16)).abs()
    }
}

/// Generates terrain for a single chunk at the given position.
fn generate_chunk_terrain(terrain: &TerrainGenerator, chunk_pos: Vector3<i32>) -> Chunk {
    let mut chunk = Chunk::new();
    let chunk_world_x = chunk_pos.x * CHUNK_SIZE as i32;
    let chunk_world_y = chunk_pos.y * CHUNK_SIZE as i32;
    let chunk_world_z = chunk_pos.z * CHUNK_SIZE as i32;

    // Generate terrain for this chunk
    for lx in 0..CHUNK_SIZE {
        for lz in 0..CHUNK_SIZE {
            let world_x = chunk_world_x + lx as i32;
            let world_z = chunk_world_z + lz as i32;
            let height = terrain.get_height(world_x, world_z);

            for ly in 0..CHUNK_SIZE {
                let world_y = chunk_world_y + ly as i32;

                let block_type = if world_y == 0 {
                    // Bedrock floor - unbreakable, prevents falling out of world
                    BlockType::Bedrock
                } else if world_y > height && world_y > SEA_LEVEL {
                    // Above terrain and above sea level = air
                    BlockType::Air
                } else if world_y > height && world_y <= SEA_LEVEL {
                    // Above terrain but below sea level = water (flat lake surface)
                    BlockType::Water
                } else if terrain.is_cave(world_x, world_y, world_z, height) {
                    // Carved out cave - fill with water if below sea level
                    if world_y <= SEA_LEVEL {
                        BlockType::Water
                    } else {
                        BlockType::Air
                    }
                } else if world_y == height {
                    // Surface block - varies by elevation (biome)
                    if height > 70 {
                        BlockType::Snow // Snow-capped peaks
                    } else if height > 55 {
                        BlockType::Stone // Rocky mountain surface
                    } else if height <= SEA_LEVEL + 2 {
                        BlockType::Sand // Beach/shore near water level
                    } else {
                        BlockType::Grass // Normal grassland
                    }
                } else if world_y > height - 3 {
                    // Subsurface layer
                    if height > 55 {
                        BlockType::Stone // Mountains: stone all the way
                    } else if height <= SEA_LEVEL + 2 {
                        BlockType::Sand // Sandy beach substrate
                    } else {
                        BlockType::Dirt // Normal: dirt layer
                    }
                } else {
                    BlockType::Stone // Deep underground
                };
                chunk.set_block(lx, ly, lz, block_type);
            }
        }
    }

    // Add trees deterministically based on chunk position
    // Trees are placed if hash of position within chunk meets threshold
    for lx in (2..CHUNK_SIZE - 2).step_by(8) {
        for lz in (2..CHUNK_SIZE - 2).step_by(8) {
            let world_x = chunk_world_x + lx as i32;
            let world_z = chunk_world_z + lz as i32;
            let height = terrain.get_height(world_x, world_z);

            // Only place trees in grassland areas (not on mountains)
            if height > 55 {
                continue;
            }

            // Deterministic tree placement
            if terrain.hash(world_x, world_z) % 100 < 15 {
                let local_base_y = height - chunk_world_y;

                // Only place tree if the base is in this chunk
                if local_base_y >= 0 && local_base_y < CHUNK_SIZE as i32 - 6 {
                    let trunk_height = 5 + (terrain.hash(world_x, world_z).abs() % 3);

                    // Tree trunk
                    for dy in 1..=trunk_height {
                        let ly = (local_base_y + dy) as usize;
                        if ly < CHUNK_SIZE {
                            chunk.set_block(lx, ly, lz, BlockType::Log);
                        }
                    }

                    // Simple canopy
                    let canopy_base = (local_base_y + trunk_height) as usize;
                    for dx in -2i32..=2 {
                        for dz in -2i32..=2 {
                            for dy in 0..3 {
                                let nlx = lx as i32 + dx;
                                let nly = canopy_base as i32 + dy;
                                let nlz = lz as i32 + dz;

                                if nlx >= 0
                                    && nlx < CHUNK_SIZE as i32
                                    && nly >= 0
                                    && nly < CHUNK_SIZE as i32
                                    && nlz >= 0
                                    && nlz < CHUNK_SIZE as i32
                                {
                                    let dist =
                                        ((dx * dx + dz * dz) as f32).sqrt() + (dy as f32 * 0.5);
                                    if dist <= 2.5 {
                                        let block = chunk.get_block(
                                            nlx as usize,
                                            nly as usize,
                                            nlz as usize,
                                        );
                                        if block == BlockType::Air {
                                            chunk.set_block(
                                                nlx as usize,
                                                nly as usize,
                                                nlz as usize,
                                                BlockType::Leaves,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    chunk
}

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

/// Creates a distance buffer for two-pass beam optimization.
/// The distance buffer is at 1/4 of render resolution and stores hit distances.
fn get_distance_image_and_set(
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
            usage: ImageUsage::STORAGE,
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

    let layout = render_pipeline
        .layout()
        .set_layouts()
        .get(6)
        .unwrap()
        .clone();
    let distance_set = DescriptorSet::new(
        descriptor_set_allocator,
        layout,
        [WriteDescriptorSet::image_view(0, distance_image_view)],
        [],
    )
    .unwrap();

    (distance_image, distance_set)
}

/// Creates an empty voxel texture for the world.
/// Returns (descriptor_set, image) where image is cleared to all zeros (air).
fn create_empty_voxel_texture(
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

    let layout = render_pipeline
        .layout()
        .set_layouts()
        .get(1)
        .unwrap()
        .clone();
    let descriptor_set = DescriptorSet::new(
        descriptor_set_allocator.clone(),
        layout.clone(),
        [WriteDescriptorSet::image_view(0, image_view)],
        [],
    )
    .unwrap();

    (descriptor_set, image)
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
/// Returns (descriptor_set, sampler, image_view) for binding to the shader and egui.
fn load_texture_atlas(
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
            image_view.clone(),
            sampler.clone(),
        )],
        [],
    )
    .unwrap();

    (descriptor_set, sampler, image_view)
}

/// Creates storage buffers and descriptor set for particle and falling block data.
/// Both share set index 3: particles at binding 0, falling blocks at binding 1.
fn get_particle_and_falling_block_set(
    memory_allocator: Arc<StandardMemoryAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
) -> (
    Subbuffer<[particles::GpuParticle]>,
    Subbuffer<[GpuFallingBlock]>,
    Arc<DescriptorSet>,
) {
    use particles::{GpuParticle, MAX_PARTICLES};

    // Create a storage buffer for particles (initialized to zeros)
    let particle_buffer = Buffer::new_slice::<GpuParticle>(
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
        MAX_PARTICLES as u64,
    )
    .unwrap();

    // Create a storage buffer for falling blocks (initialized to zeros)
    let falling_block_buffer = Buffer::new_slice::<GpuFallingBlock>(
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
        MAX_FALLING_BLOCKS as u64,
    )
    .unwrap();

    // Create descriptor set at set index 3 with both buffers
    let layout = render_pipeline
        .layout()
        .set_layouts()
        .get(3)
        .unwrap()
        .clone();

    let descriptor_set = DescriptorSet::new(
        descriptor_set_allocator,
        layout,
        [
            WriteDescriptorSet::buffer(0, particle_buffer.clone()),
            WriteDescriptorSet::buffer(1, falling_block_buffer.clone()),
        ],
        [],
    )
    .unwrap();

    (particle_buffer, falling_block_buffer, descriptor_set)
}

/// Maximum number of point lights (torches) that can be active at once.
const MAX_LIGHTS: usize = 256;

/// GPU-compatible point light data for shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuLight {
    /// Position XYZ + radius W
    pos_radius: [f32; 4],
    /// Color RGB + intensity A
    color_intensity: [f32; 4],
}

/// Creates a storage buffer and descriptor set for point light data.
fn get_light_set(
    memory_allocator: Arc<StandardMemoryAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
) -> (Subbuffer<[GpuLight]>, Arc<DescriptorSet>) {
    // Create a storage buffer for lights (initialized to zeros)
    let light_buffer = Buffer::new_slice::<GpuLight>(
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
        MAX_LIGHTS as u64,
    )
    .unwrap();

    // Create descriptor set at set index 4
    let layout = render_pipeline
        .layout()
        .set_layouts()
        .get(4)
        .unwrap()
        .clone();

    let descriptor_set = DescriptorSet::new(
        descriptor_set_allocator,
        layout,
        [WriteDescriptorSet::buffer(0, light_buffer.clone())],
        [],
    )
    .unwrap();

    (light_buffer, descriptor_set)
}

/// Number of chunks in the metadata buffer (must match shader constants)
const TOTAL_CHUNKS: usize =
    LOADED_CHUNKS_X as usize * WORLD_CHUNKS_Y as usize * LOADED_CHUNKS_Z as usize;
/// Number of u32 words needed to store 1 bit per chunk
const CHUNK_METADATA_WORDS: usize = TOTAL_CHUNKS.div_ceil(32);

/// Creates a storage buffer and descriptor set for chunk metadata (empty/solid flags).
fn get_chunk_metadata_set(
    memory_allocator: Arc<StandardMemoryAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
) -> (Subbuffer<[u32]>, Arc<DescriptorSet>) {
    // Create a storage buffer for chunk metadata (bit-packed flags)
    let chunk_metadata_buffer = Buffer::new_slice::<u32>(
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
        CHUNK_METADATA_WORDS as u64,
    )
    .unwrap();

    // Create descriptor set at set index 5
    let layout = render_pipeline
        .layout()
        .set_layouts()
        .get(5)
        .unwrap()
        .clone();

    let descriptor_set = DescriptorSet::new(
        descriptor_set_allocator,
        layout,
        [WriteDescriptorSet::buffer(0, chunk_metadata_buffer.clone())],
        [],
    )
    .unwrap();

    (chunk_metadata_buffer, descriptor_set)
}

/// Number of u32 words for brick masks (2 words = 64 bits per chunk).
const BRICK_MASK_WORDS: usize = TOTAL_CHUNKS * 2;
/// Number of u32 words for brick distances (16 words = 64 bytes per chunk).
const BRICK_DIST_WORDS: usize = TOTAL_CHUNKS * 16;

/// Creates combined descriptor set 7 containing brick metadata AND model resources.
/// This merges brick metadata with model resources to stay within the 8 descriptor set limit.
///
/// Layout:
/// - Binding 0: Brick masks - 64 bits per chunk (2 u32 words per chunk)
/// - Binding 1: Brick distances - 64 bytes per chunk (distance to nearest solid brick)
/// - Binding 2: Model atlas - 128×8×128 (256 models, each 8³ voxels), R8_UINT palette indices
/// - Binding 3: Model palettes - 256×16 (256 models × 16 colors), RGBA8
/// - Binding 4: Model metadata - model_id (R) + rotation (G) per block
/// - Binding 5: Model properties - collision mask, emission, flags per model
#[allow(clippy::type_complexity)]
fn get_brick_and_model_set(
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
    queue: &Arc<Queue>,
    world_extent: [u32; 3],
    model_registry: &ModelRegistry,
) -> (
    Subbuffer<[u32]>,   // brick_mask_buffer
    Subbuffer<[u32]>,   // brick_dist_buffer
    Arc<Image>,         // model_atlas
    Arc<Image>,         // model_metadata
    Arc<DescriptorSet>, // combined set 7
) {
    // === Brick metadata resources (bindings 0-1) ===

    // Create buffer for brick masks (64 bits per chunk)
    let brick_mask_buffer = Buffer::new_slice::<u32>(
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
        BRICK_MASK_WORDS as u64,
    )
    .unwrap();

    // Create buffer for brick distances (64 bytes per chunk)
    let brick_dist_buffer = Buffer::new_slice::<u32>(
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
        BRICK_DIST_WORDS as u64,
    )
    .unwrap();

    // === Model resources (bindings 2-5) ===

    // Create model atlas 3D texture (R8_UINT, 128×8×128)
    let model_atlas = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim3d,
            format: Format::R8_UINT,
            extent: [MODEL_ATLAS_WIDTH, MODEL_ATLAS_HEIGHT, MODEL_ATLAS_DEPTH],
            usage: ImageUsage::STORAGE | ImageUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo::default(),
    )
    .unwrap();

    // Create model palette 2D texture (RGBA8, 256×16)
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

    // Upload model registry data to GPU
    upload_model_registry(
        memory_allocator.clone(),
        command_buffer_allocator.clone(),
        queue,
        model_registry,
        &model_atlas,
        &model_palettes,
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
        .build()
        .unwrap()
        .execute(queue.clone())
        .unwrap()
        .then_signal_fence_and_flush()
        .unwrap()
        .wait(None)
        .unwrap();

    // Create image views
    let atlas_view = ImageView::new(
        model_atlas.clone(),
        ImageViewCreateInfo::from_image(&model_atlas),
    )
    .unwrap();

    let palette_view = ImageView::new(
        model_palettes.clone(),
        ImageViewCreateInfo::from_image(&model_palettes),
    )
    .unwrap();

    let metadata_view = ImageView::new(
        model_metadata.clone(),
        ImageViewCreateInfo::from_image(&model_metadata),
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
    let layout = render_pipeline
        .layout()
        .set_layouts()
        .get(7)
        .unwrap()
        .clone();

    let descriptor_set = DescriptorSet::new(
        descriptor_set_allocator,
        layout,
        [
            // Brick metadata (bindings 0-1)
            WriteDescriptorSet::buffer(0, brick_mask_buffer.clone()),
            WriteDescriptorSet::buffer(1, brick_dist_buffer.clone()),
            // Model resources (bindings 2-5)
            WriteDescriptorSet::image_view(2, atlas_view),
            WriteDescriptorSet::image_view_sampler(3, palette_view, palette_sampler),
            WriteDescriptorSet::image_view(4, metadata_view),
            WriteDescriptorSet::buffer(5, model_properties_buffer),
        ],
        [],
    )
    .unwrap();

    (
        brick_mask_buffer,
        brick_dist_buffer,
        model_atlas,
        model_metadata,
        descriptor_set,
    )
}

/// GPU-side model properties for sub-voxel rendering.
/// Must match the shader struct layout.
#[derive(Debug, Clone, Copy, Default, BufferContents)]
#[repr(C)]
struct GpuModelProperties {
    /// 64-bit collision mask (4×4×4 grid) stored as two u32s.
    collision_mask: [u32; 2],
    /// Packed AABB min (x, y, z bytes).
    aabb_min: u32,
    /// Packed AABB max (x, y, z bytes).
    aabb_max: u32,
    /// Light emission color (RGB) and intensity (A).
    emission: [f32; 4],
    /// Flags: bit 0 = rotatable, bit 1 = light_blocking_full, bit 2 = light_blocking_partial.
    flags: u32,
    /// Padding to align to 16 bytes (total 48 bytes).
    _pad2: [u32; 3],
}

/// Model atlas dimensions: 16 models per row, 16 rows = 256 models.
const MODEL_ATLAS_WIDTH: u32 = 16 * SUB_VOXEL_SIZE as u32; // 128
const MODEL_ATLAS_DEPTH: u32 = 16 * SUB_VOXEL_SIZE as u32; // 128
const MODEL_ATLAS_HEIGHT: u32 = SUB_VOXEL_SIZE as u32; // 8

/// Uploads model registry data (atlas, palettes, properties) to GPU.
fn upload_model_registry(
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    queue: &Arc<Queue>,
    registry: &ModelRegistry,
    atlas: &Arc<Image>,
    palettes: &Arc<Image>,
    properties_buffer: &Subbuffer<[GpuModelProperties]>,
) {
    // Pack model voxels into atlas layout
    let atlas_data = registry.pack_voxels_for_gpu();
    let palette_data = registry.pack_palettes_for_gpu();
    let properties_data = registry.pack_properties_for_gpu();

    // Create staging buffers
    let atlas_staging = Buffer::from_iter(
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
        atlas_data,
    )
    .unwrap();

    let palette_staging = Buffer::from_iter(
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
        palette_data,
    )
    .unwrap();

    // Convert properties data to GpuModelProperties
    let gpu_properties: Vec<GpuModelProperties> = properties_data
        .chunks(48)
        .map(|chunk| {
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

    // Copy atlas data
    command_buffer_builder
        .copy_buffer_to_image(CopyBufferToImageInfo {
            regions: [BufferImageCopy {
                image_subresource: atlas.subresource_layers(),
                image_extent: atlas.extent(),
                ..Default::default()
            }]
            .into(),
            ..CopyBufferToImageInfo::buffer_image(atlas_staging, atlas.clone())
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
            ..CopyBufferToImageInfo::buffer_image(palette_staging, palettes.clone())
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
}

/// Statistics about loaded chunks for HUD display.
#[derive(Debug, Clone, Copy, Default)]
struct ChunkStats {
    /// Number of chunks currently loaded in memory.
    loaded_count: usize,
    /// Number of chunks with pending GPU uploads.
    dirty_count: usize,
    /// Number of chunks being generated in background.
    in_flight_count: usize,
    /// Estimated GPU memory usage in megabytes.
    memory_mb: f32,
}

/// Performance profiler for tracking operation timings.
#[derive(Debug, Default)]
struct Profiler {
    /// Accumulated time for chunk loading/streaming (microseconds).
    chunk_loading_us: u64,
    /// Accumulated time for GPU uploads (microseconds).
    gpu_upload_us: u64,
    /// Accumulated time for metadata updates (microseconds).
    metadata_update_us: u64,
    /// Accumulated time for rendering (microseconds).
    render_us: u64,
    /// Number of samples accumulated.
    sample_count: u32,
    /// Number of chunks uploaded this period.
    chunks_uploaded: u32,
}

impl Profiler {
    fn reset(&mut self) {
        self.chunk_loading_us = 0;
        self.gpu_upload_us = 0;
        self.metadata_update_us = 0;
        self.render_us = 0;
        self.sample_count = 0;
        self.chunks_uploaded = 0;
    }

    fn print_stats(&self) {
        if self.sample_count == 0 {
            return;
        }
        let n = self.sample_count as f64;
        println!(
            "[PROFILE] ChunkLoad: {:.2}ms | Upload: {:.2}ms ({} chunks) | Metadata: {:.2}ms | Render: {:.2}ms",
            self.chunk_loading_us as f64 / 1000.0 / n,
            self.gpu_upload_us as f64 / 1000.0 / n,
            self.chunks_uploaded,
            self.metadata_update_us as f64 / 1000.0 / n,
            self.render_us as f64 / 1000.0 / n,
        );
    }
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

    camera: Camera,
    render_mode: RenderMode,
    render_scale: f32,
    /// Current window size for debug output
    window_size: [u32; 2],

    /// Player physics state
    player_velocity: Vector3<f64>,
    on_ground: bool,
    /// Head bob phase (continuously accumulates while walking)
    head_bob_timer: f64,
    /// Head bob intensity (0-1, smoothly fades in/out)
    head_bob_intensity: f64,
    /// True when player's head is submerged in water
    in_water: bool,
    /// Flying mode (no gravity, vertical movement with Space/Shift)
    fly_mode: bool,
    /// Sprint mode (toggle with Caps Lock for faster movement)
    sprint_mode: bool,
    /// Auto-jump when walking into 1-block obstacles
    auto_jump: bool,
    /// Player carries a torch-like light
    player_light: bool,
    /// Show debug chunk boundary wireframes
    show_chunk_boundaries: bool,
    /// Show block placement preview
    show_block_preview: bool,
    /// Show target block outline (wireframe around block player is looking at)
    show_target_outline: bool,

    // Minimap settings
    /// Whether to show the minimap
    show_minimap: bool,
    /// Minimap size in pixels (128, 192, or 256)
    minimap_size: u32,
    /// Minimap color mode: 0=block colors, 1=height, 2=both
    minimap_color_mode: u8,
    /// Whether to rotate minimap to match player direction
    minimap_rotate: bool,
    /// Minimap zoom level (0.5 = zoomed in 2x, 1.0 = normal, 2.0 = zoomed out 2x)
    minimap_zoom: f32,
    /// Cached minimap image for reuse between frames
    minimap_cached_image: Option<egui::ColorImage>,
    /// Last player position for minimap update throttling
    minimap_last_pos: Vector3<i32>,
    /// Last minimap update time for rate limiting
    minimap_last_update: Instant,
    /// Last player yaw for rotation-based updates
    minimap_last_yaw: f32,
    /// Height cache for minimap: (x, z) -> (block_type, height)
    minimap_height_cache: std::collections::HashMap<(i32, i32), (BlockType, i32)>,

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

    /// Timer for throttled debug logging (sub-voxel collision debug)
    debug_log_timer: f64,
    /// Last logged sub-voxel debug state
    debug_last_ladder_state: bool,

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
            memory_allocator.clone(),
            command_buffer_allocator.clone(),
            descriptor_set_allocator.clone(),
            &render_pipeline,
            &queue,
            world_extent,
        );

        // Load texture atlas
        let texture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("textures")
            .join("texture_atlas.png");
        let (texture_set, _sampler, texture_atlas_view) = load_texture_atlas(
            memory_allocator.clone(),
            command_buffer_allocator.clone(),
            descriptor_set_allocator.clone(),
            &render_pipeline,
            &queue,
            &texture_path,
        );

        // Create particle and falling block buffers (share set 3)
        let (particle_buffer, falling_block_buffer, particle_set) =
            get_particle_and_falling_block_set(
                memory_allocator.clone(),
                descriptor_set_allocator.clone(),
                &render_pipeline,
            );

        // Create light buffer and descriptor set
        let (light_buffer, light_set) = get_light_set(
            memory_allocator.clone(),
            descriptor_set_allocator.clone(),
            &render_pipeline,
        );

        // Create chunk metadata buffer and descriptor set
        let (chunk_metadata_buffer, chunk_metadata_set) = get_chunk_metadata_set(
            memory_allocator.clone(),
            descriptor_set_allocator.clone(),
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
            memory_allocator.clone(),
            command_buffer_allocator.clone(),
            descriptor_set_allocator.clone(),
            &render_pipeline,
            &queue,
            world_extent,
            &model_registry,
        );

        let input = WinitInputHelper::new();

        // Spawn at world origin (0, ground_level, 0) for infinite worlds
        let spawn_x = 0;
        let spawn_z = 0;
        let spawn_y = find_ground_level(&world, spawn_x, spawn_z);
        let spawn_pos = Vector3::new(spawn_x as f64, spawn_y as f64 + 1.0, spawn_z as f64);

        // Convert spawn position to texture-relative normalized camera coordinates
        // Camera position is relative to texture_origin, then normalized by texture size
        let texture_relative_pos = spawn_pos - texture_origin.cast::<f64>();
        let camera_pos = Vector3::new(
            texture_relative_pos.x / world_extent[0] as f64,
            (texture_relative_pos.y + PLAYER_EYE_HEIGHT) / world_extent[1] as f64,
            texture_relative_pos.z / world_extent[2] as f64,
        );

        let mut camera = Camera::new(
            camera_pos,
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

            camera,
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

            player_velocity: Vector3::zeros(),
            on_ground: false,
            head_bob_timer: 0.0,
            head_bob_intensity: 0.0,
            in_water: false,
            fly_mode: args.fly_mode,
            sprint_mode: false,
            auto_jump: true, // Enabled by default
            player_light: false,
            show_chunk_boundaries: args.show_chunk_boundaries,
            show_block_preview: false,  // Off by default
            show_target_outline: false, // Off by default (toggle in UI)

            // Minimap - disabled by default, toggle with M
            show_minimap: false,
            minimap_size: 256,     // Large
            minimap_color_mode: 2, // Both (block colors + height)
            minimap_rotate: true,  // Rotate with player by default
            minimap_zoom: 0.5,     // Zoomed in 2x
            minimap_cached_image: None,
            minimap_last_pos: Vector3::new(i32::MAX, 0, i32::MAX), // Force initial update
            minimap_last_update: Instant::now(),
            minimap_last_yaw: f32::MAX, // Force initial update
            minimap_height_cache: std::collections::HashMap::new(),

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

            debug_log_timer: 0.0,
            debug_last_ladder_state: false,

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
        // Camera position is in texture-relative normalized coords
        // Convert to texture coords, then to world coords by adding texture_origin
        let texture_pos = self.camera.position.component_mul(&scale);
        Vector3::new(
            texture_pos.x + self.texture_origin.x as f64,
            texture_pos.y - PLAYER_EYE_HEIGHT + self.texture_origin.y as f64,
            texture_pos.z + self.texture_origin.z as f64,
        )
    }

    /// Gets the chunk position the player is currently in (world chunk coordinates).
    fn get_player_chunk(&self) -> Vector3<i32> {
        let feet = self.player_feet_pos();
        // Use div_euclid for correct floor division with negative coordinates
        vector![
            (feet.x.floor() as i32).div_euclid(CHUNK_SIZE as i32),
            (feet.y.floor() as i32).div_euclid(CHUNK_SIZE as i32),
            (feet.z.floor() as i32).div_euclid(CHUNK_SIZE as i32)
        ]
    }

    /// Checks if texture origin needs to shift and handles re-upload if necessary.
    /// Returns true if a shift occurred.
    fn check_and_shift_texture_origin(&mut self) -> bool {
        let player_chunk = self.get_player_chunk();

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
        self.camera.position.x += origin_delta.x as f64 / scale.x;
        self.camera.position.y += origin_delta.y as f64 / scale.y;
        self.camera.position.z += origin_delta.z as f64 / scale.z;

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
            self.upload_chunks_batched(&upload_refs);
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

        let player_chunk = self.get_player_chunk();

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
            self.upload_chunks_batched(&upload_refs);

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
            self.upload_chunks_batched(&chunks_to_clear);
        }

        // Update chunk metadata if any chunks were loaded or unloaded
        if !chunks_to_upload.is_empty() || !positions_to_clear.is_empty() {
            self.update_chunk_metadata();
            self.update_brick_metadata();
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

    /// Sets the player position from feet position (world coordinates).
    fn set_player_feet_pos(&mut self, feet_pos: Vector3<f64>) {
        let scale = Vector3::new(
            self.world_extent[0] as f64,
            self.world_extent[1] as f64,
            self.world_extent[2] as f64,
        );
        // Convert world coords to texture coords, then to normalized camera coords
        let texture_pos = Vector3::new(
            feet_pos.x - self.texture_origin.x as f64,
            feet_pos.y - self.texture_origin.y as f64,
            feet_pos.z - self.texture_origin.z as f64,
        );
        self.camera.position = Vector3::new(
            texture_pos.x / scale.x,
            (texture_pos.y + PLAYER_EYE_HEIGHT) / scale.y,
            texture_pos.z / scale.z,
        );
    }

    /// Checks if a block position is solid (not air, water, or other non-solid blocks).
    /// Note: For Model blocks, this returns false - use check_model_collision for sub-voxel collision.
    #[allow(dead_code)]
    fn is_solid(&self, x: i32, y: i32, z: i32) -> bool {
        // Y is bounded, X and Z are infinite (handled by World returning None for unloaded chunks)
        if y < 0 || y >= TEXTURE_SIZE_Y as i32 {
            return false; // Out of Y bounds = not solid (can fall out of world)
        }
        self.world
            .get_block(Vector3::new(x, y, z))
            .is_some_and(|b| b.is_solid())
    }

    /// Checks if the block at given position is water.
    fn is_water(&self, x: i32, y: i32, z: i32) -> bool {
        // Y is bounded, X and Z are infinite
        if y < 0 || y >= TEXTURE_SIZE_Y as i32 {
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

    /// Checks if the block at given position is a ladder.
    fn is_ladder(&self, x: i32, y: i32, z: i32) -> bool {
        // Y is bounded, X and Z are infinite
        if y < 0 || y >= TEXTURE_SIZE_Y as i32 {
            return false;
        }
        let pos = Vector3::new(x, y, z);
        if let Some(BlockType::Model) = self.world.get_block(pos) {
            if let Some(model_data) = self.world.get_model_data(pos) {
                return ModelRegistry::is_ladder_model(model_data.model_id);
            }
        }
        false
    }

    /// Checks if any part of the player's body is touching a ladder.
    fn check_player_touching_ladder(&self, feet_pos: Vector3<f64>) -> bool {
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
                    if self.is_ladder(bx, by, bz) {
                        if self.args.verbose {
                            println!(
                                "[LADDER] Touching ladder at ({}, {}, {}), player feet: ({:.2}, {:.2}, {:.2})",
                                bx, by, bz, feet_pos.x, feet_pos.y, feet_pos.z
                            );
                        }
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Checks if player AABB collides with any solid blocks at given feet position.
    /// For Model blocks, uses sub-voxel collision from the 4³ collision mask.
    /// If `skip_ladders` is true, ladder models are ignored (for climbing).
    fn check_collision_ex(&self, feet_pos: Vector3<f64>, skip_ladders: bool) -> bool {
        // Player AABB: centered on X/Z, extends from feet to feet+height on Y
        let min_x = (feet_pos.x - PLAYER_HALF_WIDTH).floor() as i32;
        let max_x = (feet_pos.x + PLAYER_HALF_WIDTH).floor() as i32;
        let min_y = feet_pos.y.floor() as i32;
        let max_y = (feet_pos.y + PLAYER_HEIGHT).floor() as i32;
        let min_z = (feet_pos.z - PLAYER_HALF_WIDTH).floor() as i32;
        let max_z = (feet_pos.z + PLAYER_HALF_WIDTH).floor() as i32;

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

        for bx in min_x..=max_x {
            for by in min_y..=max_y {
                for bz in min_z..=max_z {
                    let world_pos = Vector3::new(bx, by, bz);
                    let block = self.world.get_block(world_pos);

                    if let Some(block_type) = block {
                        if block_type.is_solid() {
                            // Full block collision check
                            let block_min = Vector3::new(bx as f64, by as f64, bz as f64);
                            let block_max = block_min + Vector3::new(1.0, 1.0, 1.0);

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
                        } else if block_type == BlockType::Model {
                            // Skip ladder collision when climbing
                            if skip_ladders {
                                if let Some(model_data) = self.world.get_model_data(world_pos) {
                                    if ModelRegistry::is_ladder_model(model_data.model_id) {
                                        if self.args.verbose {
                                            println!(
                                                "[LADDER SKIP] Skipping collision for ladder at ({}, {}, {})",
                                                world_pos.x, world_pos.y, world_pos.z
                                            );
                                        }
                                        continue;
                                    }
                                }
                            }

                            // Sub-voxel collision check for Model blocks
                            let collides =
                                self.check_model_collision(world_pos, &player_min, &player_max);
                            if self.args.verbose {
                                // Get model info for debug output
                                if let Some(model_data) = self.world.get_model_data(world_pos) {
                                    let model_name = self
                                        .model_registry
                                        .get(model_data.model_id)
                                        .map(|m| m.name.as_str())
                                        .unwrap_or("unknown");
                                    println!(
                                        "[MODEL COLLISION] Block ({}, {}, {}): {} (id={}, rot={}), collides={}",
                                        world_pos.x,
                                        world_pos.y,
                                        world_pos.z,
                                        model_name,
                                        model_data.model_id,
                                        model_data.rotation,
                                        collides
                                    );
                                }
                            }
                            if collides {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// Checks if player AABB collides with any solid blocks at given feet position.
    /// For Model blocks, uses sub-voxel collision from the 4³ collision mask.
    fn check_collision(&self, feet_pos: Vector3<f64>) -> bool {
        self.check_collision_ex(feet_pos, false)
    }

    /// Checks if player AABB collides with a Model block's sub-voxel collision mask.
    fn check_model_collision(
        &self,
        block_pos: Vector3<i32>,
        player_min: &Vector3<f64>,
        player_max: &Vector3<f64>,
    ) -> bool {
        // Get the chunk and model data
        let chunk_pos = World::world_to_chunk(block_pos);
        let (lx, ly, lz) = World::world_to_local(block_pos);

        let chunk = match self.world.get_chunk(chunk_pos) {
            Some(c) => c,
            None => return false,
        };

        let model_data = match chunk.get_model_data(lx, ly, lz) {
            Some(d) => d,
            None => return false,
        };

        let model = match self.model_registry.get(model_data.model_id) {
            Some(m) => m,
            None => return false,
        };

        // Calculate player AABB overlap with this block
        let block_min = Vector3::new(block_pos.x as f64, block_pos.y as f64, block_pos.z as f64);
        let block_max = block_min + Vector3::new(1.0, 1.0, 1.0);

        // Check if player AABB overlaps with block at all
        if player_min.x >= block_max.x
            || player_max.x <= block_min.x
            || player_min.y >= block_max.y
            || player_max.y <= block_min.y
            || player_min.z >= block_max.z
            || player_max.z <= block_min.z
        {
            return false;
        }

        // Calculate the overlap region in local block coordinates (0-1)
        let overlap_min_x = (player_min.x - block_min.x).max(0.0);
        let overlap_max_x = (player_max.x - block_min.x).min(1.0);
        let overlap_min_y = (player_min.y - block_min.y).max(0.0);
        let overlap_max_y = (player_max.y - block_min.y).min(1.0);
        let overlap_min_z = (player_min.z - block_min.z).max(0.0);
        let overlap_max_z = (player_max.z - block_min.z).min(1.0);

        if self.args.verbose {
            println!(
                "[MODEL DETAIL] {} at ({},{},{}): mask=0x{:016X}, overlap=({:.2}-{:.2}, {:.2}-{:.2}, {:.2}-{:.2})",
                model.name,
                block_pos.x,
                block_pos.y,
                block_pos.z,
                model.collision_mask,
                overlap_min_x,
                overlap_max_x,
                overlap_min_y,
                overlap_max_y,
                overlap_min_z,
                overlap_max_z
            );
        }

        // Get rotation for coordinate transformation
        let rotation = model_data.rotation;

        // Helper to rotate local coordinates based on model rotation
        // Rotation is around Y axis: 0=none, 1=90°CW, 2=180°, 3=270°CW
        let rotate_point = |x: f64, z: f64| -> (f64, f64) {
            match rotation {
                0 => (x, z),
                1 => (1.0 - z, x),
                2 => (1.0 - x, 1.0 - z),
                3 => (z, 1.0 - x),
                _ => (x, z),
            }
        };

        // Sample multiple points in the overlap region to check against collision mask
        // The collision mask is 4³, so sample at collision cell resolution
        let step = 0.25; // 1/4 block = one collision cell
        let mut local_y = overlap_min_y;
        while local_y < overlap_max_y {
            let mut local_z = overlap_min_z;
            while local_z < overlap_max_z {
                let mut local_x = overlap_min_x;
                while local_x < overlap_max_x {
                    // Rotate the test point to model space
                    let (rot_x, rot_z) = rotate_point(local_x, local_z);
                    if model.point_collides(rot_x as f32, local_y as f32, rot_z as f32) {
                        return true;
                    }
                    local_x += step;
                }
                local_z += step;
            }
            local_y += step;
        }

        // Also check the max corners (rotated)
        let (rot_max_x, rot_max_z) = rotate_point(overlap_max_x - 0.001, overlap_max_z - 0.001);
        if model.point_collides(
            rot_max_x as f32,
            (overlap_max_y - 0.001) as f32,
            rot_max_z as f32,
        ) {
            return true;
        }

        false
    }

    /// Updates player physics: applies gravity, handles movement, checks collisions.
    fn update_physics(&mut self, delta_time: f64) {
        let mut feet = self.player_feet_pos();

        // Check if player is in water or on ladder
        let head_in_water = self.check_player_in_water(feet);
        let touching_water = self.check_player_touching_water(feet);
        let touching_ladder = self.check_player_touching_ladder(feet);
        self.in_water = head_in_water;

        // Debug logging for ladder state changes (throttled)
        if self.args.verbose {
            self.debug_log_timer += delta_time;
            if touching_ladder != self.debug_last_ladder_state {
                let camera_pos = self.camera.position;
                let block_x = feet.x.floor() as i32;
                let block_y = feet.y.floor() as i32;
                let block_z = feet.z.floor() as i32;
                println!(
                    "[LADDER STATE] {} -> {}, feet=({:.3}, {:.3}, {:.3}), camera=({:.3}, {:.3}, {:.3}), block=({}, {}, {})",
                    if self.debug_last_ladder_state {
                        "ON"
                    } else {
                        "OFF"
                    },
                    if touching_ladder { "ON" } else { "OFF" },
                    feet.x,
                    feet.y,
                    feet.z,
                    camera_pos.x,
                    camera_pos.y,
                    camera_pos.z,
                    block_x,
                    block_y,
                    block_z
                );
                self.debug_last_ladder_state = touching_ladder;
            }
            // Periodic position logging while on ladder (every 0.5 seconds)
            if touching_ladder && self.debug_log_timer >= 0.5 {
                let camera_pos = self.camera.position;
                // Calculate local position within the block
                let local_x = feet.x - feet.x.floor();
                let local_y = feet.y - feet.y.floor();
                let local_z = feet.z - feet.z.floor();
                println!(
                    "[LADDER POS] feet=({:.3}, {:.3}, {:.3}), local=({:.3}, {:.3}, {:.3}), camera=({:.3}, {:.3}, {:.3})",
                    feet.x,
                    feet.y,
                    feet.z,
                    local_x,
                    local_y,
                    local_z,
                    camera_pos.x,
                    camera_pos.y,
                    camera_pos.z
                );
                self.debug_log_timer = 0.0;
            }
        }

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

        // Determine movement speed based on environment, fly mode, and sprint
        // Fly mode doubles speed, sprint doubles that again
        let base_speed = if touching_water {
            SWIM_SPEED
        } else if touching_ladder {
            CLIMB_HORIZ_SPEED // Slower horizontal movement on ladder
        } else if self.fly_mode {
            MOVE_SPEED * 2.0 // Fly mode: 2x speed
        } else {
            MOVE_SPEED
        };
        let current_speed = if self.sprint_mode {
            base_speed * 2.0 // Sprint: 2x current speed
        } else {
            base_speed
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
            // Fly mode: no gravity, Space=up, Shift=down for vertical movement
            // Use same speed as horizontal (already accounts for fly mode and sprint)
            let shift_held = (self.input.key_held(KeyCode::ShiftLeft)
                || self.input.key_held(KeyCode::ShiftRight)) as i32
                as f64;
            let up = t(KeyCode::Space) - shift_held;
            self.player_velocity.y = up * current_speed;

            // Move without collision checks in fly mode
            feet.x += self.player_velocity.x * delta_time;
            feet.y += self.player_velocity.y * delta_time;
            feet.z += self.player_velocity.z * delta_time;

            // Clamp to Y bounds only (X/Z are infinite)
            feet.y = feet.y.clamp(0.5, TEXTURE_SIZE_Y as f64 - 0.5);
        } else if touching_water {
            // Swimming mode: reduced gravity, buoyancy, vertical swim controls

            // Apply water physics: reduced gravity + buoyancy
            self.player_velocity.y -= WATER_GRAVITY * delta_time;
            self.player_velocity.y += WATER_BUOYANCY * delta_time;

            // Apply water drag to slow down
            let drag = WATER_DRAG.powf(delta_time);
            self.player_velocity.y *= drag;

            // Swim up with Space, swim down with Shift (same as fly mode)
            if self.input.key_held(KeyCode::Space) {
                self.player_velocity.y = SWIM_UP_SPEED;
            } else if self.input.key_held(KeyCode::ShiftLeft)
                || self.input.key_held(KeyCode::ShiftRight)
            {
                self.player_velocity.y = -SWIM_DOWN_SPEED;
            }

            // Move on each axis separately and check collisions
            // Use a small Y offset for horizontal checks to avoid floor collision due to floating point
            let horiz_check_y = feet.y + 0.01;

            // X axis
            let new_x = feet.x + self.player_velocity.x * delta_time;
            let test_pos = Vector3::new(new_x, horiz_check_y, feet.z);
            if !self.check_collision(test_pos) {
                feet.x = new_x;
            } else {
                self.player_velocity.x = 0.0;
            }

            // Z axis
            let new_z = feet.z + self.player_velocity.z * delta_time;
            let test_pos = Vector3::new(feet.x, horiz_check_y, new_z);
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
        } else if touching_ladder
            && (self.input.key_held(KeyCode::Space)
                || self.input.key_held(KeyCode::ShiftLeft)
                || self.input.key_held(KeyCode::ShiftRight))
        {
            // Climbing mode: only when pressing climb keys (Space/Shift)
            // No gravity while actively climbing

            // Climb up with Space, climb down with Shift
            if self.input.key_held(KeyCode::Space) {
                self.player_velocity.y = CLIMB_UP_SPEED;
            } else {
                self.player_velocity.y = -CLIMB_DOWN_SPEED;
            }

            // Move on each axis separately and check collisions
            // Skip ladder collision while climbing (allows moving through ladder blocks)
            let horiz_check_y = feet.y + 0.01;

            // X axis
            let new_x = feet.x + self.player_velocity.x * delta_time;
            let test_pos = Vector3::new(new_x, horiz_check_y, feet.z);
            if !self.check_collision_ex(test_pos, true) {
                feet.x = new_x;
            } else {
                self.player_velocity.x = 0.0;
            }

            // Z axis
            let new_z = feet.z + self.player_velocity.z * delta_time;
            let test_pos = Vector3::new(feet.x, horiz_check_y, new_z);
            if !self.check_collision_ex(test_pos, true) {
                feet.z = new_z;
            } else {
                self.player_velocity.z = 0.0;
            }

            // Y axis
            let new_y = feet.y + self.player_velocity.y * delta_time;
            let test_pos = Vector3::new(feet.x, new_y, feet.z);
            if !self.check_collision_ex(test_pos, true) {
                feet.y = new_y;
            } else {
                self.player_velocity.y = 0.0;
            }

            // Not on ground while climbing
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
            // Use a small Y offset for horizontal checks to avoid floor collision due to floating point
            let horiz_check_y = feet.y + 0.01;

            // Track if we should auto-jump due to 1-block obstacle
            let mut should_auto_jump = false;

            // X axis
            let new_x = feet.x + self.player_velocity.x * delta_time;
            let test_pos = Vector3::new(new_x, horiz_check_y, feet.z);
            if !self.check_collision(test_pos) {
                feet.x = new_x;
            } else {
                // Check for auto-jump: is this a 1-block-high obstacle?
                if self.auto_jump && self.on_ground && self.player_velocity.x.abs() > 0.1 {
                    let step_up_pos = Vector3::new(new_x, feet.y + 1.01, feet.z);
                    if !self.check_collision(step_up_pos) {
                        should_auto_jump = true;
                    }
                }
                self.player_velocity.x = 0.0;
            }

            // Z axis
            let new_z = feet.z + self.player_velocity.z * delta_time;
            let test_pos = Vector3::new(feet.x, horiz_check_y, new_z);
            if !self.check_collision(test_pos) {
                feet.z = new_z;
            } else {
                // Check for auto-jump: is this a 1-block-high obstacle?
                if self.auto_jump && self.on_ground && self.player_velocity.z.abs() > 0.1 {
                    let step_up_pos = Vector3::new(feet.x, feet.y + 1.01, new_z);
                    if !self.check_collision(step_up_pos) {
                        should_auto_jump = true;
                    }
                }
                self.player_velocity.z = 0.0;
            }

            // Execute auto-jump if needed
            if should_auto_jump {
                self.player_velocity.y = JUMP_VELOCITY;
                self.on_ground = false;
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

        // Respawn if fallen out of world
        if feet.y < -10.0 {
            println!("Player fell out of world, respawning...");
            feet = self.get_spawn_position();
            self.player_velocity = Vector3::zeros();
            self.on_ground = false;
        }

        // Safety: if player is stuck inside a block, push them up
        if self.check_collision(feet) && !self.fly_mode {
            // Try to find a free position above
            for offset in 1..10 {
                let test_pos = Vector3::new(feet.x, feet.y + offset as f64, feet.z);
                if !self.check_collision(test_pos) {
                    feet = test_pos;
                    self.player_velocity.y = 0.0;
                    println!("Player was stuck, pushed up {} blocks", offset);
                    break;
                }
            }
        }

        // Update head bob based on horizontal movement
        let horizontal_speed =
            (self.player_velocity.x.powi(2) + self.player_velocity.z.powi(2)).sqrt();
        let is_walking = self.on_ground && horizontal_speed > 0.5 && !self.fly_mode;

        if is_walking {
            // Accumulate timer based on distance traveled (continuous phase)
            self.head_bob_timer += horizontal_speed * delta_time * HEAD_BOB_FREQUENCY;
            // Smoothly ramp up intensity
            self.head_bob_intensity += (1.0 - self.head_bob_intensity) * delta_time * 8.0;
        } else {
            // Smoothly fade out intensity (but keep timer for smooth resume)
            self.head_bob_intensity *= 0.9_f64.powf(delta_time * 60.0);
        }
        self.head_bob_intensity = self.head_bob_intensity.clamp(0.0, 1.0);

        // Update camera position
        self.set_player_feet_pos(feet);
    }

    /// Gets a safe spawn position on land.
    fn get_spawn_position(&self) -> Vector3<f64> {
        // Spawn at world origin (0, 0) for infinite world
        let spawn_x = 0;
        let spawn_z = 0;
        let spawn_y = find_ground_level(&self.world, spawn_x, spawn_z);
        Vector3::new(spawn_x as f64, spawn_y as f64 + 1.0, spawn_z as f64)
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
        let texture_pos = self.camera.position.cast::<f32>().component_mul(&scale);
        // Convert texture coords to world coords by adding texture_origin
        let origin = Vector3::new(
            texture_pos.x + self.texture_origin.x as f32,
            texture_pos.y + self.texture_origin.y as f32,
            texture_pos.z + self.texture_origin.z as f32,
        );
        let direction = self.camera_direction().cast::<f32>();

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
                self.invalidate_minimap_cache(target.x, target.z);

                // Update neighboring fence/gate connections
                self.update_fence_connections(target);

                // Notify water grid that a block was removed (may trigger flow)
                self.water_grid.on_block_removed(target);

                // Check if any adjacent terrain water should start flowing
                self.activate_adjacent_terrain_water(target);

                // Queue physics checks (frame-distributed to prevent FPS spikes)
                let player_pos = self.player_feet_pos().cast::<f32>();

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
        let feet = self.player_feet_pos();
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
                let connections = self.calculate_fence_connections(place_pos);
                ModelRegistry::fence_model_id(connections)
            } else if ModelRegistry::is_gate_model(base_model_id)
                || (20..28).contains(&base_model_id)
            {
                // Gate: auto-detect orientation based on neighboring fences
                // Check E/W neighbors vs N/S neighbors to determine orientation
                let has_west = self.is_fence_connectable(place_pos + Vector3::new(-1, 0, 0));
                let has_east = self.is_fence_connectable(place_pos + Vector3::new(1, 0, 0));
                let has_north = self.is_fence_connectable(place_pos + Vector3::new(0, 0, -1));
                let has_south = self.is_fence_connectable(place_pos + Vector3::new(0, 0, 1));

                // Calculate player position relative to gate for open direction
                let player_pos = self.player_feet_pos();
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
                    let connections = self.calculate_gate_connections(place_pos);
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
                self.update_fence_connections(place_pos);
                // Skip the normal set_model_block below since we already did it
                return true;
            } else if ModelRegistry::is_ladder_model(base_model_id) {
                // Ladder: auto-orient to face the player
                // The ladder model has rungs at Z=7, so rotation determines which wall it faces
                // rotation=0: against +Z wall (facing -Z toward player)
                // rotation=1: against +X wall (facing -X)
                // rotation=2: against -Z wall (facing +Z)
                // rotation=3: against -X wall (facing +X)
                let player_pos = self.player_feet_pos();
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
                self.update_fence_connections(place_pos);
            }
        } else {
            self.world.set_block(place_pos, block_to_place);

            // Solid blocks can also connect to fences
            if block_to_place.is_solid() {
                self.update_fence_connections(place_pos);
            }
        }
        self.invalidate_minimap_cache(place_pos.x, place_pos.z);

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

    /// Calculates fence connection bitmask based on neighboring fences/gates.
    /// Returns N=1, S=2, E=4, W=8 bitmask.
    /// Note: North is -Z, South is +Z (matching model definition)
    fn calculate_fence_connections(&self, pos: Vector3<i32>) -> u8 {
        let mut connections = 0u8;

        // Check north (-Z)
        if self.is_fence_connectable(pos + Vector3::new(0, 0, -1)) {
            connections |= 1;
        }
        // Check south (+Z)
        if self.is_fence_connectable(pos + Vector3::new(0, 0, 1)) {
            connections |= 2;
        }
        // Check east (+X)
        if self.is_fence_connectable(pos + Vector3::new(1, 0, 0)) {
            connections |= 4;
        }
        // Check west (-X)
        if self.is_fence_connectable(pos + Vector3::new(-1, 0, 0)) {
            connections |= 8;
        }

        connections
    }

    /// Calculates gate connection bitmask based on neighboring fences/gates.
    /// Returns W=1, E=2 bitmask (gates only connect east-west).
    fn calculate_gate_connections(&self, pos: Vector3<i32>) -> u8 {
        let mut connections = 0u8;

        // Check west (-X)
        if self.is_fence_connectable(pos + Vector3::new(-1, 0, 0)) {
            connections |= 1;
        }
        // Check east (+X)
        if self.is_fence_connectable(pos + Vector3::new(1, 0, 0)) {
            connections |= 2;
        }

        connections
    }

    /// Returns true if the block at pos can connect to fences/gates.
    fn is_fence_connectable(&self, pos: Vector3<i32>) -> bool {
        if let Some(block) = self.world.get_block(pos) {
            match block {
                BlockType::Model => {
                    // Check if it's a fence or gate model
                    if let Some(data) = self.world.get_model_data(pos) {
                        ModelRegistry::is_fence_or_gate(data.model_id)
                    } else {
                        false
                    }
                }
                // Solid blocks also connect to fences
                b if b.is_solid() => true,
                _ => false,
            }
        } else {
            false
        }
    }

    /// Updates fence/gate connections for a position and its neighbors.
    fn update_fence_connections(&mut self, center_pos: Vector3<i32>) {
        // Update neighbors in all 4 horizontal directions
        let neighbors = [
            Vector3::new(0, 0, 1),  // North
            Vector3::new(0, 0, -1), // South
            Vector3::new(1, 0, 0),  // East
            Vector3::new(-1, 0, 0), // West
        ];

        for offset in &neighbors {
            let neighbor_pos = center_pos + offset;
            if let Some(BlockType::Model) = self.world.get_block(neighbor_pos) {
                if let Some(data) = self.world.get_model_data(neighbor_pos) {
                    if ModelRegistry::is_fence_model(data.model_id) {
                        // Update fence connections
                        let connections = self.calculate_fence_connections(neighbor_pos);
                        let new_model_id = ModelRegistry::fence_model_id(connections);
                        if new_model_id != data.model_id {
                            // Force rotation 0 for fences as their orientation is in the model_id
                            self.world.set_model_block(neighbor_pos, new_model_id, 0);
                        }
                    } else if ModelRegistry::is_gate_model(data.model_id) {
                        // Update gate connections
                        let connections = self.calculate_gate_connections(neighbor_pos);
                        let is_open = ModelRegistry::is_gate_open_model(data.model_id);
                        let new_model_id = if is_open {
                            ModelRegistry::gate_open_model_id(connections)
                        } else {
                            ModelRegistry::gate_closed_model_id(connections)
                        };
                        if new_model_id != data.model_id {
                            self.world
                                .set_model_block(neighbor_pos, new_model_id, data.rotation);
                        }
                    }
                }
            }
        }
    }

    /// Finds all leaves connected to the starting leaf, and checks if any connect to a log.
    /// Returns (leaf_positions, has_log_connection).
    fn find_leaf_cluster_and_check_log(
        &self,
        start: Vector3<i32>,
    ) -> (Vec<(Vector3<i32>, BlockType)>, bool) {
        use std::collections::{HashSet, VecDeque};

        let mut leaves = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut found_log = false;

        // Verify starting block is leaves
        if let Some(block) = self.world.get_block(start) {
            if block != BlockType::Leaves {
                return (leaves, true); // Not leaves, assume connected
            }
        } else {
            return (leaves, true);
        }

        queue.push_back(start);
        visited.insert(start);

        // 26-directional for leaf-to-leaf, 6-directional for leaf-to-log check
        let mut neighbors_26 = Vec::with_capacity(26);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if dx != 0 || dy != 0 || dz != 0 {
                        neighbors_26.push(Vector3::new(dx, dy, dz));
                    }
                }
            }
        }

        while let Some(pos) = queue.pop_front() {
            if let Some(block) = self.world.get_block(pos) {
                if block == BlockType::Leaves {
                    leaves.push((pos, block));

                    for offset in &neighbors_26 {
                        let neighbor = pos + offset;
                        let is_cardinal = (offset.x != 0) as i32
                            + (offset.y != 0) as i32
                            + (offset.z != 0) as i32
                            == 1;

                        if let Some(neighbor_block) = self.world.get_block(neighbor) {
                            // Check for log connection (orthogonal only)
                            if neighbor_block.is_log() && is_cardinal {
                                found_log = true;
                            }

                            // Add unvisited leaves to queue (any direction)
                            if neighbor_block == BlockType::Leaves && !visited.contains(&neighbor) {
                                visited.insert(neighbor);
                                queue.push_back(neighbor);
                            }
                        }
                    }
                }
            }
        }

        (leaves, found_log)
    }

    /// Flood-fill to find all connected tree blocks (logs and leaves) starting from a log.
    /// Returns a vector of (position, block_type) for all connected blocks.
    fn find_connected_tree(&self, start: Vector3<i32>) -> Vec<(Vector3<i32>, BlockType)> {
        use std::collections::{HashSet, VecDeque};

        let mut tree_blocks = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        // Verify starting block is a log
        if let Some(block) = self.world.get_block(start) {
            if !block.is_log() {
                return tree_blocks;
            }
        } else {
            return tree_blocks;
        }

        queue.push_back(start);
        visited.insert(start);

        // 26-directional neighbors (including diagonals)
        let mut neighbors_26 = Vec::with_capacity(26);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if dx != 0 || dy != 0 || dz != 0 {
                        neighbors_26.push(Vector3::new(dx, dy, dz));
                    }
                }
            }
        }

        while let Some(pos) = queue.pop_front() {
            if let Some(block) = self.world.get_block(pos) {
                if block.is_tree_part() {
                    tree_blocks.push((pos, block));

                    // Connectivity rules to prevent merging separate trees:
                    // - Logs: only connect orthogonally (6-dir) to logs and leaves
                    // - Leaves: connect diagonally (26-dir) to OTHER leaves,
                    //           but only orthogonally (6-dir) to logs
                    for offset in &neighbors_26 {
                        let neighbor = pos + offset;
                        if !visited.contains(&neighbor) {
                            if let Some(neighbor_block) = self.world.get_block(neighbor) {
                                if !neighbor_block.is_tree_part() {
                                    continue;
                                }

                                // is_cardinal: exactly one axis is non-zero (orthogonal neighbor)
                                let is_cardinal = (offset.x != 0) as i32
                                    + (offset.y != 0) as i32
                                    + (offset.z != 0) as i32
                                    == 1;

                                let should_connect = if block.is_log() {
                                    // Logs only connect orthogonally (6-dir)
                                    is_cardinal
                                } else {
                                    // Leaves: connect to other leaves diagonally (26-dir),
                                    // but only connect to logs orthogonally (6-dir)
                                    if neighbor_block.is_log() {
                                        is_cardinal
                                    } else {
                                        true // leaf-to-leaf: any direction
                                    }
                                };

                                if should_connect {
                                    visited.insert(neighbor);
                                    queue.push_back(neighbor);
                                }
                            }
                        }
                    }
                }
            }
        }

        tree_blocks
    }

    /// Checks if any log in the tree has ground support.
    /// A log has ground support if the block below it is solid and NOT a log.
    fn tree_has_ground_support(&self, tree_blocks: &[(Vector3<i32>, BlockType)]) -> bool {
        for (pos, block) in tree_blocks {
            if block.is_log() {
                let below_pos = pos + Vector3::new(0, -1, 0);
                if let Some(below_block) = self.world.get_block(below_pos) {
                    // Supported if block below is solid and NOT part of the tree
                    // (leaves don't count as support either!)
                    if below_block.is_solid() && !below_block.is_tree_part() {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Converts all tree blocks to falling entities.
    fn fell_tree(&mut self, tree_blocks: Vec<(Vector3<i32>, BlockType)>) {
        for (pos, block_type) in tree_blocks {
            // Remove the block from the world
            self.world.set_block(pos, BlockType::Air);
            self.invalidate_minimap_cache(pos.x, pos.z);

            // Spawn a falling block entity
            self.falling_blocks.spawn(pos, block_type);
        }
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
                    self.invalidate_minimap_cache(final_pos.x, final_pos.z);

                    // Queue gravity check for chain reaction (blocks above might now fall)
                    let player_pos = self.player_feet_pos().cast::<f32>();
                    self.block_updates.enqueue(
                        final_pos + Vector3::new(0, 1, 0),
                        BlockUpdateType::Gravity,
                        player_pos,
                    );
                }
            }
        }
    }

    /// Processes queued block physics updates.
    ///
    /// This is called each frame and processes up to `max_per_frame` updates
    /// from the queue. Each update type triggers specific physics checks:
    /// - CheckGravity: Check if block at position should fall
    /// - CheckTreeSupport: Check if log lost ground support
    /// - CheckOrphanedLeaves: Check if leaf cluster is disconnected from logs
    fn process_block_updates(&mut self) {
        let player_pos = self.player_feet_pos().cast::<f32>();
        let batch = self.block_updates.take_batch();

        for update in batch {
            match update.update_type {
                BlockUpdateType::Gravity => {
                    self.process_gravity_update(update.position, player_pos);
                }
                BlockUpdateType::TreeSupport => {
                    self.process_tree_support_update(update.position, player_pos);
                }
                BlockUpdateType::OrphanedLeaves => {
                    self.process_orphaned_leaves_update(update.position, player_pos);
                }
                BlockUpdateType::ModelGroundSupport => {
                    self.process_model_ground_support_update(update.position, player_pos);
                }
            }
        }
    }

    /// Processes a gravity check for a single position.
    /// If the block is gravity-affected, converts it to falling and queues the block above.
    fn process_gravity_update(&mut self, pos: Vector3<i32>, player_pos: Vector3<f32>) {
        // Bounds check
        if pos.y < 0 || pos.y >= TEXTURE_SIZE_Y as i32 {
            return;
        }

        if let Some(block_type) = self.world.get_block(pos) {
            if block_type.is_affected_by_gravity() {
                // Remove the block from the world
                self.world.set_block(pos, BlockType::Air);
                self.invalidate_minimap_cache(pos.x, pos.z);

                // Spawn a falling block entity
                self.falling_blocks.spawn(pos, block_type);

                // Queue the next block up for cascade
                self.block_updates.enqueue(
                    pos + Vector3::new(0, 1, 0),
                    BlockUpdateType::Gravity,
                    player_pos,
                );
            }
        }
    }

    /// Processes a tree support check for a log position.
    /// If the log is part of an unsupported tree, the entire tree falls.
    fn process_tree_support_update(&mut self, pos: Vector3<i32>, _player_pos: Vector3<f32>) {
        // Bounds check
        if pos.y < 0 || pos.y >= TEXTURE_SIZE_Y as i32 {
            return;
        }

        if let Some(block) = self.world.get_block(pos) {
            if block.is_log() {
                // Find all connected tree blocks
                let tree_blocks = self.find_connected_tree(pos);

                if !tree_blocks.is_empty() && !self.tree_has_ground_support(&tree_blocks) {
                    // Tree has no ground support, fell it
                    self.fell_tree(tree_blocks);
                }
            }
        }
    }

    /// Processes an orphaned leaves check for a leaf position.
    /// If the leaf cluster is disconnected from all logs, the cluster falls.
    fn process_orphaned_leaves_update(&mut self, pos: Vector3<i32>, _player_pos: Vector3<f32>) {
        // Bounds check
        if pos.y < 0 || pos.y >= TEXTURE_SIZE_Y as i32 {
            return;
        }

        if let Some(block) = self.world.get_block(pos) {
            if block == BlockType::Leaves {
                // Check if this leaf cluster is connected to any log
                let (leaves, has_log) = self.find_leaf_cluster_and_check_log(pos);

                if !has_log && !leaves.is_empty() {
                    // Orphaned leaf cluster, fell it
                    self.fell_tree(leaves);
                }
            }
        }
    }

    /// Processes a model ground support check for a position.
    /// If the model requires ground support and the block below is air, the model breaks.
    fn process_model_ground_support_update(
        &mut self,
        pos: Vector3<i32>,
        _player_pos: Vector3<f32>,
    ) {
        // Bounds check
        if pos.y < 1 || pos.y >= TEXTURE_SIZE_Y as i32 {
            return;
        }

        // Check if there's a model block at this position
        if let Some(BlockType::Model) = self.world.get_block(pos) {
            if let Some(data) = self.world.get_model_data(pos) {
                // Check if this model requires ground support
                if self.model_registry.requires_ground_support(data.model_id) {
                    // Check if ground below is gone
                    let below = pos - Vector3::new(0, 1, 0);
                    let has_support = if let Some(block_below) = self.world.get_block(below) {
                        block_below.is_solid()
                            || (block_below == BlockType::Model
                                && self
                                    .world
                                    .get_model_data(below)
                                    .map(|d| {
                                        !self.model_registry.requires_ground_support(d.model_id)
                                    })
                                    .unwrap_or(false))
                    } else {
                        false
                    };

                    if !has_support {
                        // Get particle color before breaking
                        let particle_color = nalgebra::Vector3::new(0.5, 0.35, 0.2); // Wood brown
                        self.particles
                            .spawn_block_break(pos.cast::<f32>(), particle_color);

                        // Break the model
                        self.world.set_block(pos, BlockType::Air);
                        self.invalidate_minimap_cache(pos.x, pos.z);

                        // Update neighboring fence/gate connections
                        self.update_fence_connections(pos);
                    }
                }
            }
        }
    }

    /// Processes water flow simulation.
    ///
    /// This is called each frame and processes water cell updates.
    /// Water flows using the W-Shadow cellular automata algorithm:
    /// - Down (gravity) has highest priority
    /// - Horizontal flow equalizes water levels
    /// - Upward flow only occurs under pressure
    ///
    /// Boundary handling:
    /// - World bounds (y < 0): Water drains into void and is destroyed
    /// - Unloaded chunks: Water is blocked (treated as solid wall)
    fn process_water_simulation(&mut self) {
        if !self.water_simulation_enabled {
            return;
        }

        let player_pos = self.player_feet_pos().cast::<f32>();
        let texture_height = TEXTURE_SIZE_Y as i32;

        // Create a closure that checks if a block is solid
        // Also returns true for unloaded chunks (blocks water flow until chunk loads)
        let world = &self.world;
        let is_solid = |pos: Vector3<i32>| -> bool {
            // Check Y bounds first
            if pos.y < 0 || pos.y >= texture_height {
                return true; // Out of bounds = solid (blocks flow)
            }
            // For loaded chunks, check if block is solid
            // For unloaded chunks, get_block returns None, treat as solid (block flow)
            world.get_block(pos).map(|b| b.is_solid()).unwrap_or(true) // Unloaded chunk = treat as solid wall
        };

        // Check if position is truly out of world bounds (water should drain here)
        // Only Y < 0 is considered "out of bounds" for draining
        // Water at unloaded chunk boundaries should NOT drain, just be blocked
        let is_out_of_bounds = |pos: Vector3<i32>| -> bool {
            pos.y < 0 // Only drain water that falls below the world
        };

        // Run water simulation tick
        let changed_positions = self.water_grid.tick(is_solid, is_out_of_bounds, player_pos);

        // Update world blocks and GPU for changed water cells
        for pos in changed_positions {
            // Skip out-of-bounds positions
            if pos.y < 0 || pos.y >= texture_height {
                continue;
            }

            let has_water = self.water_grid.has_water(pos);
            let current_block = self.world.get_block(pos);

            match (current_block, has_water) {
                (Some(BlockType::Air), true) => {
                    // Air became water
                    self.world.set_block(pos, BlockType::Water);
                    self.invalidate_minimap_cache(pos.x, pos.z);
                }
                (Some(BlockType::Water), false) => {
                    // Water evaporated/drained
                    self.world.set_block(pos, BlockType::Air);
                    self.invalidate_minimap_cache(pos.x, pos.z);
                }
                _ => {
                    // No block type change needed, but may need GPU update for water level
                    // (This will be used when shader supports variable water heights)
                }
            }
        }
    }

    /// Checks adjacent blocks for terrain water (BlockType::Water that exists in the world
    /// but not yet in the water grid) and adds them to the water grid.
    ///
    /// This is called when a block is broken to allow nearby static terrain water
    /// to start flowing into the newly empty space.
    fn activate_adjacent_terrain_water(&mut self, pos: Vector3<i32>) {
        let directions = [
            Vector3::new(1, 0, 0),
            Vector3::new(-1, 0, 0),
            Vector3::new(0, 1, 0),
            Vector3::new(0, -1, 0),
            Vector3::new(0, 0, 1),
            Vector3::new(0, 0, -1),
        ];

        for dir in directions {
            let neighbor = pos + dir;

            // Skip if out of Y bounds
            if neighbor.y < 0 || neighbor.y >= TEXTURE_SIZE_Y as i32 {
                continue;
            }

            // Check if this neighbor is terrain water
            if let Some(BlockType::Water) = self.world.get_block(neighbor) {
                // If it's not already in the water grid, add it as a source
                // (terrain water is treated as infinite source until we convert all water)
                if !self.water_grid.has_water(neighbor) {
                    self.water_grid.place_source(neighbor);
                } else {
                    // Already in grid, just activate it for flow
                    self.water_grid.activate_neighbors(neighbor);
                }
            }
        }
    }

    /// Finds the surface block at a given X, Z world coordinate.
    /// Returns the block type and Y height of the topmost non-air block.
    /// Uses cached values when available.
    fn find_surface_block(&mut self, world_x: i32, world_z: i32) -> (BlockType, i32) {
        // Check cache first
        if let Some(&cached) = self.minimap_height_cache.get(&(world_x, world_z)) {
            return cached;
        }

        // Calculate and cache
        let result = self.calculate_surface_block(world_x, world_z);
        self.minimap_height_cache.insert((world_x, world_z), result);
        result
    }

    /// Invalidates the minimap height cache for a given (x, z) position.
    fn invalidate_minimap_cache(&mut self, world_x: i32, world_z: i32) {
        self.minimap_height_cache.remove(&(world_x, world_z));
    }

    /// Calculates the surface block without caching (for internal use).
    fn calculate_surface_block(&self, world_x: i32, world_z: i32) -> (BlockType, i32) {
        for y in (0..TEXTURE_SIZE_Y as i32).rev() {
            if let Some(block) = self.world.get_block(Vector3::new(world_x, y, world_z)) {
                if block != BlockType::Air {
                    return (block, y);
                }
            }
        }
        (BlockType::Air, 0)
    }

    /// Gets the color for a minimap pixel based on block type and height.
    fn get_minimap_color(&self, block: BlockType, height: i32) -> egui::Color32 {
        let base_color = block.color();
        let (r, g, b) = (base_color[0], base_color[1], base_color[2]);

        match self.minimap_color_mode {
            0 => {
                // Block colors only
                egui::Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
            }
            1 => {
                // Height shading only (grayscale)
                let brightness = ((height as f32 / 128.0) * 200.0 + 55.0).min(255.0) as u8;
                egui::Color32::from_rgb(brightness, brightness, brightness)
            }
            _ => {
                // Both: block colors with height brightness
                let height_factor = 0.5 + (height as f32 / 128.0) * 0.5;
                egui::Color32::from_rgb(
                    (r * 255.0 * height_factor).min(255.0) as u8,
                    (g * 255.0 * height_factor).min(255.0) as u8,
                    (b * 255.0 * height_factor).min(255.0) as u8,
                )
            }
        }
    }

    /// Generates a minimap image centered on the player's position.
    /// When rotate is enabled, samples from a larger area to fill corners after rotation.
    /// Returns a ColorImage that can be loaded as a texture.
    fn generate_minimap_image(&mut self, player_pos: Vector3<f64>, yaw: f32) -> egui::ColorImage {
        let display_size = self.minimap_size as usize;
        let center_x = player_pos.x as f32;
        let center_z = player_pos.z as f32;

        // Base sample radius adjusted by zoom (higher zoom = larger area = zoomed out)
        // When rotating, multiply by sqrt(2) ≈ 1.42 to fill corners
        let base_radius = (display_size as f32 / 2.0) * self.minimap_zoom;
        let sample_radius = if self.minimap_rotate {
            base_radius * 1.42
        } else {
            base_radius
        };

        let mut pixels = vec![egui::Color32::BLACK; display_size * display_size];

        // Precompute rotation (rotate world coords to align with player facing direction)
        let (sin_yaw, cos_yaw) = if self.minimap_rotate {
            (yaw.sin(), yaw.cos())
        } else {
            (0.0, 1.0) // No rotation
        };

        let half = display_size as f32 / 2.0;

        for dz in 0..display_size {
            for dx in 0..display_size {
                // Screen-space offset from center (-half to +half)
                let sx = dx as f32 - half;
                let sz = dz as f32 - half;

                // Scale to sample radius
                let scale = sample_radius / half;
                let scaled_x = sx * scale;
                let scaled_z = sz * scale;

                // Apply rotation to get world-space offset
                // Screen right (+sx) maps to player's right direction
                // Screen down (+sz) maps to player's backward direction
                let world_offset_x = scaled_x * cos_yaw + scaled_z * sin_yaw;
                let world_offset_z = -scaled_x * sin_yaw + scaled_z * cos_yaw;

                let world_x = (center_x + world_offset_x).floor() as i32;
                let world_z = (center_z + world_offset_z).floor() as i32;

                // Find surface block (top-down)
                let (block_type, height) = self.find_surface_block(world_x, world_z);

                // Calculate color based on mode
                let color = self.get_minimap_color(block_type, height);

                pixels[dz * display_size + dx] = color;
            }
        }

        egui::ColorImage {
            size: [display_size, display_size],
            pixels,
        }
    }

    /// Takes a screenshot and saves it to the specified path.
    fn save_screenshot(&self, image_view: &Arc<ImageView>, path: &str) {
        let image = image_view.image();
        let extent = image.extent();

        // Create a buffer to copy the image data into
        let buffer_size = (extent[0] * extent[1] * 4) as u64; // RGBA
        let staging_buffer = Buffer::new_slice::<u8>(
            self.memory_allocator.clone(),
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
            self.command_buffer_allocator.clone(),
            self.queue.queue_family_index(),
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
        let future = vulkano::sync::now(self.device.clone())
            .then_execute(self.queue.clone(), command_buffer)
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
            self.upload_chunks_batched(&upload_refs);

            // Update metadata so shader doesn't skip newly non-empty bricks
            self.update_chunk_metadata();
            self.update_brick_metadata();
        }
    }

    /// Uploads multiple chunks to the GPU in a single batched command buffer.
    /// This is much faster than uploading chunks one at a time.
    ///
    /// Each chunk tuple contains: (chunk_position, block_data, model_metadata)
    /// - block_data: R8_UINT format, one byte per block
    /// - model_metadata: RG8_UINT format, two bytes per block (model_id, rotation)
    fn upload_chunks_batched(&self, chunks: &[(Vector3<i32>, &[u8], &[u8])]) {
        if chunks.is_empty() {
            return;
        }

        // Create staging buffers for all chunks (both block data and model metadata)
        let mut block_buffers_and_regions = Vec::with_capacity(chunks.len());
        let mut metadata_buffers_and_regions = Vec::with_capacity(chunks.len());

        for (chunk_pos, block_data, model_metadata) in chunks {
            // Convert world chunk position to texture position
            // World block position = chunk_pos * CHUNK_SIZE
            // Texture block position = world_block_pos - texture_origin
            let world_block_x = chunk_pos.x * CHUNK_SIZE as i32;
            let world_block_y = chunk_pos.y * CHUNK_SIZE as i32;
            let world_block_z = chunk_pos.z * CHUNK_SIZE as i32;

            let texture_x = world_block_x - self.texture_origin.x;
            let texture_y = world_block_y - self.texture_origin.y;
            let texture_z = world_block_z - self.texture_origin.z;

            // Skip chunks outside texture bounds
            if texture_x < 0
                || texture_y < 0
                || texture_z < 0
                || texture_x + CHUNK_SIZE as i32 > TEXTURE_SIZE_X as i32
                || texture_y + CHUNK_SIZE as i32 > TEXTURE_SIZE_Y as i32
                || texture_z + CHUNK_SIZE as i32 > TEXTURE_SIZE_Z as i32
            {
                continue;
            }

            let offset = [texture_x as u32, texture_y as u32, texture_z as u32];

            // Block data buffer (R8_UINT)
            let block_buffer = Buffer::from_iter(
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
                block_data.iter().copied(),
            )
            .unwrap();

            let block_region = BufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: CHUNK_SIZE as u32,
                buffer_image_height: CHUNK_SIZE as u32,
                image_subresource: self.voxel_image.subresource_layers(),
                image_offset: offset,
                image_extent: [CHUNK_SIZE as u32, CHUNK_SIZE as u32, CHUNK_SIZE as u32],
                ..Default::default()
            };

            block_buffers_and_regions.push((block_buffer, block_region));

            // Model metadata buffer (RG8_UINT - 2 bytes per block)
            let metadata_buffer = Buffer::from_iter(
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
                model_metadata.iter().copied(),
            )
            .unwrap();

            let metadata_region = BufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: CHUNK_SIZE as u32,
                buffer_image_height: CHUNK_SIZE as u32,
                image_subresource: self.model_metadata.subresource_layers(),
                image_offset: offset,
                image_extent: [CHUNK_SIZE as u32, CHUNK_SIZE as u32, CHUNK_SIZE as u32],
                ..Default::default()
            };

            metadata_buffers_and_regions.push((metadata_buffer, metadata_region));
        }

        // Build single command buffer with all copies
        let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
            self.command_buffer_allocator.clone(),
            self.queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )
        .unwrap();

        // Copy block data to voxel_image
        for (src_buffer, region) in block_buffers_and_regions {
            command_buffer_builder
                .copy_buffer_to_image(CopyBufferToImageInfo {
                    regions: [region].into(),
                    ..CopyBufferToImageInfo::buffer_image(src_buffer, self.voxel_image.clone())
                })
                .unwrap();
        }

        // Copy model metadata to model_metadata image
        for (src_buffer, region) in metadata_buffers_and_regions {
            command_buffer_builder
                .copy_buffer_to_image(CopyBufferToImageInfo {
                    regions: [region].into(),
                    ..CopyBufferToImageInfo::buffer_image(src_buffer, self.model_metadata.clone())
                })
                .unwrap();
        }

        // Execute - batching reduces per-chunk overhead significantly
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
        self.upload_chunks_batched(&upload_refs);

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
        self.update_chunk_metadata();
        self.update_brick_metadata();
    }

    /// Updates the chunk metadata buffer on the GPU.
    ///
    /// This creates a bit-packed buffer where each bit indicates if a chunk is empty.
    /// The shader uses this to skip empty chunks during ray traversal for better performance.
    fn update_chunk_metadata(&mut self) {
        let t_start = Instant::now();
        let mut metadata = vec![0u32; CHUNK_METADATA_WORDS];

        // Iterate over texture-relative chunk positions
        for cy in 0..WORLD_CHUNKS_Y {
            for cz in 0..LOADED_CHUNKS_Z {
                for cx in 0..LOADED_CHUNKS_X {
                    // Convert texture-relative chunk position to world chunk position
                    let world_chunk_x = self.texture_origin.x / CHUNK_SIZE as i32 + cx;
                    let world_chunk_y = cy;
                    let world_chunk_z = self.texture_origin.z / CHUNK_SIZE as i32 + cz;
                    let world_chunk_pos = Vector3::new(world_chunk_x, world_chunk_y, world_chunk_z);

                    // Calculate flat chunk index
                    // Index matches shader layout: x + z * CHUNKS_X + y * CHUNKS_X * CHUNKS_Z
                    let chunk_idx = cx as usize
                        + cz as usize * LOADED_CHUNKS_X as usize
                        + cy as usize * LOADED_CHUNKS_X as usize * LOADED_CHUNKS_Z as usize;

                    // Check if chunk exists and get its data
                    if let Some(chunk) = self.world.get_chunk_mut(world_chunk_pos) {
                        // Ensure metadata is up-to-date before reading
                        chunk.update_metadata();

                        // Update empty flag
                        if chunk.is_empty() {
                            let word_idx = chunk_idx / 32;
                            let bit_idx = chunk_idx % 32;
                            metadata[word_idx] |= 1u32 << bit_idx;
                        }
                    } else {
                        // Missing chunks are treated as empty
                        let word_idx = chunk_idx / 32;
                        let bit_idx = chunk_idx % 32;
                        metadata[word_idx] |= 1u32 << bit_idx;
                    }
                }
            }
        }

        // Upload metadata to GPU buffer
        {
            let mut buffer_write = self.chunk_metadata_buffer.write().unwrap();
            buffer_write.copy_from_slice(&metadata);
        }

        self.profiler.metadata_update_us += t_start.elapsed().as_micros() as u64;
    }

    /// Updates the brick metadata buffers on the GPU.
    ///
    /// This creates:
    /// - Brick masks: 64-bit mask per chunk indicating which bricks have solid blocks
    /// - Brick distances: Per-brick distance to nearest solid brick
    ///
    /// The shader uses these for hierarchical brick-level ray skipping.
    fn update_brick_metadata(&mut self) {
        let t_start = Instant::now();

        // Brick mask buffer: 2 u32 per chunk (64 bits)
        let mut brick_masks = vec![0u32; BRICK_MASK_WORDS];
        // Brick distance buffer: 16 u32 per chunk (64 bytes)
        let mut brick_distances = vec![0u32; BRICK_DIST_WORDS];

        // Iterate over texture-relative chunk positions
        for cy in 0..WORLD_CHUNKS_Y {
            for cz in 0..LOADED_CHUNKS_Z {
                for cx in 0..LOADED_CHUNKS_X {
                    // Convert texture-relative chunk position to world chunk position
                    let world_chunk_x = self.texture_origin.x / CHUNK_SIZE as i32 + cx;
                    let world_chunk_y = cy;
                    let world_chunk_z = self.texture_origin.z / CHUNK_SIZE as i32 + cz;
                    let world_chunk_pos = Vector3::new(world_chunk_x, world_chunk_y, world_chunk_z);

                    // Calculate flat chunk index (matches shader layout)
                    let chunk_idx = cx as usize
                        + cz as usize * LOADED_CHUNKS_X as usize
                        + cy as usize * LOADED_CHUNKS_X as usize * LOADED_CHUNKS_Z as usize;

                    if let Some(chunk) = self.world.get_chunk(world_chunk_pos) {
                        // Build SVT for this chunk
                        let svt = ChunkSVT::from_chunk(chunk);

                        // Store brick mask (64 bits = 2 u32)
                        let mask_offset = chunk_idx * 2;
                        brick_masks[mask_offset] = svt.brick_mask as u32;
                        brick_masks[mask_offset + 1] = (svt.brick_mask >> 32) as u32;

                        // Store brick distances (64 bytes = 16 u32)
                        let dist_offset = chunk_idx * 16;
                        for (i, chunk_distances) in svt.brick_distances.chunks(4).enumerate() {
                            let word = (chunk_distances[0] as u32)
                                | ((chunk_distances[1] as u32) << 8)
                                | ((chunk_distances[2] as u32) << 16)
                                | ((chunk_distances[3] as u32) << 24);
                            brick_distances[dist_offset + i] = word;
                        }
                    } else {
                        // Missing chunk: mask = 0 (all empty), distances = 255
                        let dist_offset = chunk_idx * 16;
                        for i in 0..16 {
                            brick_distances[dist_offset + i] = 0xFFFFFFFF;
                        }
                    }
                }
            }
        }

        // Upload to GPU buffers
        {
            let mut mask_write = self.brick_mask_buffer.write().unwrap();
            mask_write.copy_from_slice(&brick_masks);
        }
        {
            let mut dist_write = self.brick_dist_buffer.write().unwrap();
            dist_write.copy_from_slice(&brick_distances);
        }

        self.profiler.metadata_update_us += t_start.elapsed().as_micros() as u64;
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
            self.upload_chunks_batched(&upload_refs);

            // Mark as clean
            for pos in dirty_positions {
                if let Some(chunk) = self.world.get_chunk_mut(pos) {
                    chunk.mark_clean();
                }
            }

            // Update chunk and brick metadata since chunks may have changed empty status
            self.update_chunk_metadata();
            self.update_brick_metadata();
        }

        uploaded
    }

    /// Collects all light-emitting blocks (including model blocks like torches)
    /// and returns them as GPU light data.
    fn collect_torch_lights(&self) -> Vec<GpuLight> {
        let mut lights = Vec::new();

        // Add player light if enabled (like holding a torch)
        if self.player_light {
            let player_pos = self.player_feet_pos();
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
            let player_pos = self.player_feet_pos();
            let player_chunk = self.get_player_chunk();
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
            let player_pos = self.player_feet_pos();
            let player_chunk = self.get_player_chunk();
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
                self.player_velocity.x,
                self.player_velocity.y,
                self.player_velocity.z
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
        self.process_block_updates();

        // Process water flow simulation (frame-distributed)
        self.process_water_simulation();

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
                self.fly_mode = !self.fly_mode;
                if self.fly_mode {
                    println!("Fly mode: ON");
                } else {
                    println!("Fly mode: OFF");
                }
            }

            // Toggle sprint mode (Left Control)
            if self.input.key_pressed(KeyCode::ControlLeft) {
                self.sprint_mode = !self.sprint_mode;
                if self.sprint_mode {
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
        let player_world_pos = self.player_feet_pos();
        let selected_block = self.selected_block();
        let hotbar_index = self.hotbar_index;
        let hotbar_blocks = self.hotbar_blocks;
        let hotbar_model_ids = self.hotbar_model_ids;

        // Pre-generate minimap image if showing (before entering gui closure)
        // Throttle updates based on position change and rotation change
        let camera_yaw = self.camera.rotation.y as f32;
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
                self.minimap_rotate && (camera_yaw - self.minimap_last_yaw).abs() > 0.087; // ~5 degrees
            // Check if enough time has passed (0.1 seconds for rotation, 0.5 for position)
            let time_elapsed = self.minimap_last_update.elapsed().as_secs_f32();
            let time_ok = if self.minimap_rotate {
                time_elapsed >= 0.1 // Faster updates for rotation
            } else {
                time_elapsed >= 0.5
            };

            if ((moved || yaw_changed) && time_ok) || self.minimap_cached_image.is_none() {
                // Update last position/time/yaw and regenerate
                self.minimap_last_pos = current_pos;
                self.minimap_last_update = Instant::now();
                self.minimap_last_yaw = camera_yaw;
                let image = self.generate_minimap_image(player_world_pos, camera_yaw);
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
        let minimap_size = self.minimap_size;
        let minimap_rotate = self.minimap_rotate;
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
                            if self.in_water {
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
                                egui::Slider::new(&mut self.camera.fov, 20.0..=120.0).text("FOV"),
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
                                .checkbox(&mut self.player_light, "Player torch light")
                                .changed()
                            {
                                println!(
                                    "[TOGGLE] Player Light: {}",
                                    if self.player_light { "ON" } else { "OFF" }
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
                            ui.checkbox(&mut self.auto_jump, "Auto-jump");
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
                                    .selectable_label(self.minimap_size == 128, "Small")
                                    .clicked()
                                {
                                    self.minimap_size = 128;
                                    self.minimap_cached_image = None; // Force refresh
                                }
                                if ui
                                    .selectable_label(self.minimap_size == 192, "Medium")
                                    .clicked()
                                {
                                    self.minimap_size = 192;
                                    self.minimap_cached_image = None; // Force refresh
                                }
                                if ui
                                    .selectable_label(self.minimap_size == 256, "Large")
                                    .clicked()
                                {
                                    self.minimap_size = 256;
                                    self.minimap_cached_image = None; // Force refresh
                                }
                            });

                            ui.horizontal(|ui| {
                                ui.label("Colors:");
                                if ui
                                    .selectable_label(self.minimap_color_mode == 0, "Blocks")
                                    .clicked()
                                {
                                    self.minimap_color_mode = 0;
                                    self.minimap_cached_image = None; // Force refresh
                                }
                                if ui
                                    .selectable_label(self.minimap_color_mode == 1, "Height")
                                    .clicked()
                                {
                                    self.minimap_color_mode = 1;
                                    self.minimap_cached_image = None; // Force refresh
                                }
                                if ui
                                    .selectable_label(self.minimap_color_mode == 2, "Both")
                                    .clicked()
                                {
                                    self.minimap_color_mode = 2;
                                    self.minimap_cached_image = None; // Force refresh
                                }
                            });

                            if ui
                                .add(
                                    egui::Slider::new(&mut self.minimap_zoom, 0.5..=3.0)
                                        .text("Zoom")
                                        .logarithmic(true),
                                )
                                .changed()
                            {
                                self.minimap_cached_image = None; // Force refresh
                            }

                            if ui
                                .checkbox(&mut self.minimap_rotate, "Rotate with player")
                                .changed()
                            {
                                // Force minimap refresh when rotation mode changes
                                self.minimap_cached_image = None;
                            }

                            ui.separator();

                            // Camera position debug
                            ui.label(format!(
                                "Position: ({:.1}, {:.1}, {:.1})",
                                self.camera.position.x,
                                self.camera.position.y,
                                self.camera.position.z
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
        self.camera.extent = [render_extent[0] as f64, render_extent[1] as f64];

        let pixel_to_ray = self.camera.pixel_to_ray_matrix();

        // Scale only the position (column 4), not the direction (3x3 rotation part)
        // This prevents ray distortion from non-uniform world dimensions
        let mut pixel_to_ray_scaled = pixel_to_ray;
        // Camera position is normalized (0-1), scale to texture size
        // Ray marching happens in texture space (0 to textureSize)
        pixel_to_ray_scaled.m14 *= self.world_extent[0] as f64;
        pixel_to_ray_scaled.m24 *= self.world_extent[1] as f64;
        pixel_to_ray_scaled.m34 *= self.world_extent[2] as f64;

        // Apply head bob offset to camera Y position for rendering
        let head_bob_offset = (self.head_bob_timer * std::f64::consts::TAU).sin()
            * HEAD_BOB_AMPLITUDE
            * self.head_bob_intensity;
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
            self.save_screenshot(&image_view, "voxel_world_screen_shot.png");
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

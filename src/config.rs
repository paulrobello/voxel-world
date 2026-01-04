use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use winit::dpi::PhysicalSize;

/// World generation type
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum WorldGenType {
    /// Normal terrain with biomes, caves, mountains, and trees
    #[default]
    Normal,
    /// Flat world: 2 chunks thick with grass/dirt/stone layers
    Flat,
}

/// Voxel Game Engine - A Minecraft-like voxel game with GPU ray-marching rendering.
#[derive(Parser, Debug, Clone)]
#[command(name = "voxel_world")]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Spawn X coordinate in world blocks (default: auto-find suitable location)
    #[arg(long, short = 'x')]
    pub spawn_x: Option<i32>,

    /// Spawn Z coordinate in world blocks (default: auto-find suitable location)
    #[arg(long, short = 'z')]
    pub spawn_z: Option<i32>,

    /// Take screenshot after N seconds and save to voxel_world_screen_shot.png
    #[arg(long, short = 's')]
    pub screenshot_delay: Option<f64>,

    /// Print debug info every N frames (0 = off)
    #[arg(long, short = 'd', default_value_t = 0)]
    pub debug_interval: u32,

    /// Start in fly mode
    #[arg(long, short = 'f')]
    pub fly_mode: bool,

    /// Pause day/night cycle at specific time (0.0-1.0, where 0.5 = noon)
    #[arg(long, short = 't')]
    pub time_of_day: Option<f64>,

    /// Enable chunk boundary visualization
    #[arg(long, short = 'b')]
    pub show_chunk_boundaries: bool,

    /// Set view distance in chunks (default: 6)
    #[arg(long, short = 'v')]
    pub view_distance: Option<i32>,

    /// Seed for terrain generation (default: 12345)
    #[arg(long, short = 'S')]
    pub seed: Option<u32>,

    /// World generation type: normal or flat (default: normal)
    #[arg(long, short = 'g', value_enum, default_value_t = WorldGenType::Normal)]
    pub world_gen: WorldGenType,

    /// Start in render mode: textured, normal, coord, steps, uv, depth (default: textured)
    #[arg(long, short = 'r')]
    pub render_mode: Option<String>,

    /// Verbose debug output to console
    #[arg(long)]
    pub verbose: bool,

    /// Enable profiling - writes per-second performance samples to profiles/ folder
    #[arg(long, short = 'p')]
    pub profile: bool,

    /// Generate hotbar/palette sprites and exit
    #[arg(long)]
    pub generate_sprites: bool,

    /// World name to load or create (default: last loaded or "default")
    #[arg(long, short = 'w')]
    pub world: Option<String>,

    /// Data directory for worlds, preferences, and models (default: current directory)
    #[arg(long, short = 'D')]
    pub data_dir: Option<String>,
}

pub const INITIAL_WINDOW_RESOLUTION: PhysicalSize<u32> = PhysicalSize::new(1200, 1080);

/// Gameplay and performance settings.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub show_chunk_boundaries: bool,
    pub show_block_preview: bool,
    pub show_target_outline: bool,
    pub show_compass: bool,
    pub show_position: bool,
    pub show_stats: bool,

    pub enable_ao: bool,
    pub enable_shadows: bool,
    pub enable_model_shadows: bool,
    pub enable_point_lights: bool,
    pub enable_tinted_shadows: bool,

    pub lod_ao_distance: f32,
    pub lod_shadow_distance: f32,
    pub lod_point_light_distance: f32,
    pub lod_model_distance: f32,

    pub max_ray_steps: u32,
    pub shadow_max_steps: u32,
    pub render_scale: f32,
    pub water_simulation_enabled: bool,
    pub instant_break: bool,
    pub instant_place: bool,
    pub break_cooldown_duration: f32,
    pub place_cooldown_duration: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_chunk_boundaries: false,
            show_block_preview: false,
            show_target_outline: true,
            show_compass: true,
            show_position: true,
            show_stats: true,

            enable_ao: true,
            enable_shadows: true,
            enable_model_shadows: true,
            enable_point_lights: true,
            enable_tinted_shadows: true,

            lod_ao_distance: 64.0,
            lod_shadow_distance: 48.0,
            lod_point_light_distance: 20.0,
            lod_model_distance: 32.0,

            max_ray_steps: 256,
            shadow_max_steps: 128,
            render_scale: 1.0,
            water_simulation_enabled: true,
            instant_break: true,
            instant_place: true,
            break_cooldown_duration: 0.1,
            place_cooldown_duration: 0.1,
        }
    }
}

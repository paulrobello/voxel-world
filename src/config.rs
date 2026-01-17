use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use winit::dpi::PhysicalSize;

/// World generation type
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
pub enum WorldGenType {
    /// Normal terrain with biomes, caves, mountains, and trees
    #[default]
    Normal,
    /// Flat world: 2 chunks thick with grass/dirt/stone layers
    Flat,
    /// Benchmark world: controlled terrain with point lights and glass for profiling
    Benchmark,
}

/// Benchmark terrain style
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
pub enum BenchmarkTerrain {
    /// Flat terrain at Y=100
    #[default]
    Flat,
    /// Rolling hills with sine-wave variation Y=90-110
    Hills,
}

/// Auto-fly movement pattern
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
pub enum AutoFlyPattern {
    /// Move in +X direction
    #[default]
    Straight,
    /// Outward spiral pattern
    Spiral,
    /// Zig-zag grid pattern
    Grid,
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

    /// Exit after N seconds (useful with --screenshot-delay for automated captures)
    #[arg(long, short = 'e')]
    pub exit_delay: Option<f64>,

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

    /// Seed for terrain generation (default: 314159)
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

    /// Auto-profile mode: cycles through each feature flag (5s off, 5s on) then exits.
    /// Implies --profile. Useful for automated regression testing.
    #[arg(long, short = 'P')]
    pub auto_profile: bool,

    /// Generate hotbar/palette sprites and exit
    #[arg(long)]
    pub generate_sprites: bool,

    /// World name to load or create (default: last loaded or "default")
    #[arg(long, short = 'w')]
    pub world: Option<String>,

    /// Data directory for worlds, preferences, and models (default: current directory)
    #[arg(long, short = 'D')]
    pub data_dir: Option<String>,

    /// Auto-fly mode: moves player automatically for benchmarking (implies --fly-mode)
    #[arg(long)]
    pub auto_fly: bool,

    /// Auto-fly speed in blocks per second (default: 20.0, matches manual fly speed)
    #[arg(long, default_value_t = 20.0)]
    pub auto_fly_speed: f64,

    /// Auto-fly movement pattern: straight, spiral, or grid
    #[arg(long, value_enum, default_value_t = AutoFlyPattern::Straight)]
    pub auto_fly_pattern: AutoFlyPattern,

    /// Benchmark duration in seconds before auto-exit
    #[arg(long)]
    pub benchmark_duration: Option<f64>,

    /// Benchmark terrain style: flat or hills (only used with --world-gen benchmark)
    #[arg(long, value_enum, default_value_t = BenchmarkTerrain::Flat)]
    pub benchmark_terrain: BenchmarkTerrain,
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
    pub show_water_sources: bool,
    pub show_biome_debug: bool,
    pub debug_cutaway_enabled: bool,

    pub enable_ao: bool,
    pub enable_shadows: bool,
    pub enable_model_shadows: bool,
    pub enable_point_lights: bool,
    pub enable_tinted_shadows: bool,
    pub hide_ground_cover: bool,

    pub lod_ao_distance: f32,
    pub lod_shadow_distance: f32,
    pub lod_point_light_distance: f32,
    pub lod_model_distance: f32,

    /// Maximum distance (in blocks) at which lights are collected for GPU processing.
    /// Lights beyond this distance are culled on the CPU before being sent to the shader.
    pub light_cull_radius: f32,
    /// Maximum number of lights to send to the GPU per frame.
    /// Lights are sorted by distance, so closest lights are prioritized.
    pub max_active_lights: u32,

    pub max_ray_steps: u32,
    pub shadow_max_steps: u32,
    pub render_scale: f32,
    /// Enable dynamic render scale based on FPS
    pub dynamic_render_scale: bool,
    /// Minimum render scale when dynamic is enabled
    pub dynamic_render_scale_min: f32,
    /// Maximum render scale when dynamic is enabled
    pub dynamic_render_scale_max: f32,
    /// Target FPS for dynamic render scale
    pub dynamic_render_scale_target_fps: f32,
    pub water_simulation_enabled: bool,
    pub instant_break: bool,
    pub instant_place: bool,
    pub break_cooldown_duration: f32,
    pub place_cooldown_duration: f32,
    pub collision_enabled_fly: bool,
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
            show_water_sources: false,
            show_biome_debug: false,
            debug_cutaway_enabled: false,

            enable_ao: true,
            enable_shadows: true,
            enable_model_shadows: true,
            enable_point_lights: true,
            enable_tinted_shadows: true,
            hide_ground_cover: false,

            lod_ao_distance: 128.0,
            lod_shadow_distance: 64.0,
            lod_point_light_distance: 20.0,
            lod_model_distance: 64.0,

            light_cull_radius: 64.0,
            max_active_lights: 64,

            max_ray_steps: 256,
            shadow_max_steps: 128,
            render_scale: 1.0,
            dynamic_render_scale: false,
            dynamic_render_scale_min: 0.5,
            dynamic_render_scale_max: 1.0,
            dynamic_render_scale_target_fps: 60.0,
            water_simulation_enabled: true,
            instant_break: true,
            instant_place: true,
            break_cooldown_duration: 0.05,
            place_cooldown_duration: 0.5,
            collision_enabled_fly: false,
        }
    }
}

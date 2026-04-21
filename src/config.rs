use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use winit::dpi::PhysicalSize;

/// Game mode determining multiplayer state
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub enum GameMode {
    /// Single player (no networking)
    #[default]
    SinglePlayer,
    /// Hosting an integrated server
    Host,
    /// Connected to a remote server
    Client,
}

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

/// Quality presets for rendering features
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
pub enum QualityPreset {
    /// Potato: Lowest quality, disables most lighting features, very aggressive LODs (Scale: 0.5)
    Potato,
    /// Low: Basic lighting, shadows on, aggressive LODs (Scale: 0.6)
    Low,
    /// Medium: Balanced performance/quality, current default (Scale: 0.75)
    #[default]
    Medium,
    /// High: High quality, long LODs, tinted shadows (Scale: 1.0)
    High,
    /// Ultra: Maximum quality, supersampling, extreme LODs (Scale: 1.5)
    Ultra,
}

/// Voxel Game Engine - A Minecraft-like voxel game with GPU ray-marching rendering.
#[derive(Parser, Debug, Clone)]
#[command(name = "voxel-world")]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Spawn X coordinate in world blocks (default: auto-find suitable location)
    #[arg(long, short = 'x')]
    pub spawn_x: Option<i32>,

    /// Spawn Z coordinate in world blocks (default: auto-find suitable location)
    #[arg(long, short = 'z')]
    pub spawn_z: Option<i32>,

    /// Take screenshot after N seconds and save to voxel-world_screen_shot.png
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

    /// Graphics quality preset (potato, low, medium, high, ultra)
    #[arg(long, short = 'q', value_enum)]
    pub quality: Option<QualityPreset>,

    // ========================================================================
    // Multiplayer options
    // ========================================================================
    /// Enable multiplayer mode (use with --host or --connect)
    #[arg(long)]
    pub multiplayer: bool,

    /// Host a multiplayer server (integrated server running in-game)
    #[arg(long)]
    pub host: bool,

    /// Connect to a multiplayer server at the specified address
    #[arg(long, value_name = "ADDRESS")]
    pub connect: Option<String>,

    /// Server port for hosting or connecting (default: 5000)
    #[arg(long, default_value_t = 5000)]
    pub port: u16,
}

#[allow(dead_code)]
impl Args {
    /// Returns the game mode based on CLI arguments.
    pub fn game_mode(&self) -> GameMode {
        if self.host {
            GameMode::Host
        } else if self.connect.is_some() {
            GameMode::Client
        } else {
            GameMode::SinglePlayer
        }
    }

    /// Returns true if multiplayer is enabled (host or client).
    pub fn is_multiplayer(&self) -> bool {
        self.multiplayer || self.host || self.connect.is_some()
    }
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
    /// Show name labels above remote players in multiplayer
    pub show_player_names: bool,

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
    /// Maximum custom texture slots for hosted servers (default: 32).
    pub max_custom_textures: u8,
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
            show_player_names: true,

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
            render_scale: 0.75,
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
            max_custom_textures: 32,
        }
    }
}

impl Settings {
    /// Applies a quality preset to the settings.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_preset(
        &mut self,
        preset: QualityPreset,
        atmosphere: &mut crate::atmosphere::AtmosphereSettings,
        view_distance: &mut i32,
        load_distance: &mut i32,
        unload_distance: &mut i32,
        show_minimap: &mut bool,
        player_light_enabled: &mut bool,
    ) {
        // Disable dynamic render scaling for all presets
        self.dynamic_render_scale = false;

        match preset {
            QualityPreset::Potato => {
                self.render_scale = 0.5;
                self.enable_ao = false;
                self.enable_shadows = false;
                self.enable_point_lights = false;
                self.enable_tinted_shadows = false;
                self.enable_model_shadows = false;
                self.hide_ground_cover = true;
                self.lod_ao_distance = 0.0;
                self.lod_shadow_distance = 0.0;
                self.lod_point_light_distance = 0.0;
                self.lod_model_distance = 16.0;
                self.max_ray_steps = 128;
                self.shadow_max_steps = 0;
                self.max_active_lights = 0;
                self.light_cull_radius = 0.0;
                self.water_simulation_enabled = false;
                *show_minimap = false;
                *player_light_enabled = false;
                *view_distance = 3;
                *load_distance = 3;
                *unload_distance = 5;
                atmosphere.clouds_enabled = false;
                atmosphere.fog_density = 0.05;
                atmosphere.fog_start = 32.0;
                atmosphere.fog_overlay_scale = 1.0;
            }
            QualityPreset::Low => {
                self.render_scale = 0.6;
                self.enable_ao = false;
                self.enable_shadows = true;
                self.enable_point_lights = true;
                self.enable_tinted_shadows = false;
                self.enable_model_shadows = false;
                self.hide_ground_cover = true;
                self.lod_ao_distance = 0.0;
                self.lod_shadow_distance = 32.0;
                self.lod_point_light_distance = 16.0;
                self.lod_model_distance = 32.0;
                self.max_ray_steps = 192;
                self.shadow_max_steps = 64;
                self.max_active_lights = 16;
                self.light_cull_radius = 32.0;
                self.water_simulation_enabled = false;
                *show_minimap = false;
                *player_light_enabled = true;
                *view_distance = 4;
                *load_distance = 4;
                *unload_distance = 6;
                atmosphere.clouds_enabled = false;
                atmosphere.fog_density = 0.04;
                atmosphere.fog_start = 64.0;
                atmosphere.fog_overlay_scale = 1.0;
            }
            QualityPreset::Medium => {
                self.render_scale = 0.75;
                self.enable_ao = true;
                self.enable_shadows = true;
                self.enable_point_lights = true;
                self.enable_tinted_shadows = false;
                self.enable_model_shadows = true;
                self.hide_ground_cover = false;
                self.lod_ao_distance = 128.0;
                self.lod_shadow_distance = 64.0;
                self.lod_point_light_distance = 20.0;
                self.lod_model_distance = 64.0;
                self.max_ray_steps = 256;
                self.shadow_max_steps = 128;
                self.max_active_lights = 64;
                self.light_cull_radius = 64.0;
                self.water_simulation_enabled = true;
                *show_minimap = true;
                *player_light_enabled = true;
                *view_distance = 6;
                *load_distance = 7;
                *unload_distance = 10;
                atmosphere.clouds_enabled = true;
                atmosphere.cloud_coverage = 0.45;
                atmosphere.cloud_speed = 1.0;
                atmosphere.fog_density = 0.03;
                atmosphere.fog_start = 100.0;
                atmosphere.fog_overlay_scale = 1.0;
            }
            QualityPreset::High => {
                self.render_scale = 1.0;
                self.enable_ao = true;
                self.enable_shadows = true;
                self.enable_point_lights = true;
                self.enable_tinted_shadows = true;
                self.enable_model_shadows = true;
                self.hide_ground_cover = false;
                self.lod_ao_distance = 128.0;
                self.lod_shadow_distance = 128.0;
                self.lod_point_light_distance = 64.0;
                self.lod_model_distance = 128.0;
                self.max_ray_steps = 384;
                self.shadow_max_steps = 256;
                self.max_active_lights = 128;
                self.light_cull_radius = 128.0;
                self.water_simulation_enabled = true;
                *show_minimap = true;
                *player_light_enabled = true;
                *view_distance = 8;
                *load_distance = 10;
                *unload_distance = 14;
                atmosphere.clouds_enabled = true;
                atmosphere.cloud_coverage = 0.5;
                atmosphere.cloud_speed = 1.0;
                atmosphere.fog_density = 0.02;
                atmosphere.fog_start = 128.0;
                atmosphere.fog_overlay_scale = 1.0;
            }
            QualityPreset::Ultra => {
                self.render_scale = 1.5;
                self.enable_ao = true;
                self.enable_shadows = true;
                self.enable_point_lights = true;
                self.enable_tinted_shadows = true;
                self.enable_model_shadows = true;
                self.hide_ground_cover = false;
                self.lod_ao_distance = 256.0;
                self.lod_shadow_distance = 256.0;
                self.lod_point_light_distance = 128.0;
                self.lod_model_distance = 256.0;
                self.max_ray_steps = 512;
                self.shadow_max_steps = 512;
                self.max_active_lights = 256;
                self.light_cull_radius = 256.0;
                self.water_simulation_enabled = true;
                *show_minimap = true;
                *player_light_enabled = true;
                *view_distance = 12;
                *load_distance = 14;
                *unload_distance = 18;
                atmosphere.clouds_enabled = true;
                atmosphere.cloud_coverage = 0.6;
                atmosphere.cloud_speed = 1.0;
                atmosphere.fog_density = 0.01;
                atmosphere.fog_start = 200.0;
                atmosphere.fog_overlay_scale = 1.0;
            }
        }
    }
}

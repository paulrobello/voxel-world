use clap::Parser;
use winit::dpi::PhysicalSize;

/// Voxel Game Engine - A Minecraft-like voxel game with GPU ray-marching rendering.
#[derive(Parser, Debug, Clone)]
#[command(name = "voxel_ray_traversal")]
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

    /// Start in render mode: textured, normal, coord, steps, uv, depth (default: textured)
    #[arg(long, short = 'r')]
    pub render_mode: Option<String>,

    /// Verbose debug output to console
    #[arg(long)]
    pub verbose: bool,
}

pub const INITIAL_WINDOW_RESOLUTION: PhysicalSize<u32> = PhysicalSize::new(1200, 1080);

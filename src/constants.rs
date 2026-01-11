use crate::chunk::CHUNK_SIZE;

/// Six orthogonal neighbor offsets (±X, ±Y, ±Z).
pub const ORTHO_DIRS: [(i32, i32, i32); 6] = [
    (1, 0, 0),
    (-1, 0, 0),
    (0, 1, 0),
    (0, -1, 0),
    (0, 0, 1),
    (0, 0, -1),
];

// World height in chunks (fixed - Y dimension is bounded)
pub const WORLD_CHUNKS_Y: i32 = 16;

// Texture pool dimensions for loaded chunks (X and Z are centered on player)
// This defines how many chunks can be loaded at once, not world bounds
pub const LOADED_CHUNKS_X: i32 = 16; // Chunks loaded in X direction (8 each side of player)
pub const LOADED_CHUNKS_Z: i32 = 16; // Chunks loaded in Z direction (8 each side of player)

// GPU texture size in blocks (holds all currently loaded chunks)
pub const TEXTURE_SIZE_X: usize = LOADED_CHUNKS_X as usize * CHUNK_SIZE;
pub const TEXTURE_SIZE_Y: usize = WORLD_CHUNKS_Y as usize * CHUNK_SIZE;
pub const TEXTURE_SIZE_Z: usize = LOADED_CHUNKS_Z as usize * CHUNK_SIZE;

// Chunk streaming constants
/// View distance in chunks (horizontal - chunks within this range are rendered)
pub const VIEW_DISTANCE: i32 = 6;
/// Load distance in chunks (horizontal - chunks within this range are loaded/generated)
/// Should be >= view_distance + 1 to preload chunks before they become visible
pub const LOAD_DISTANCE: i32 = 7;
/// Unload distance in chunks (horizontal - chunks beyond this are unloaded)
/// Should be > load_distance to prevent thrashing at boundaries
pub const UNLOAD_DISTANCE: i32 = 10;
/// Maximum chunks to load or unload per frame
pub const CHUNKS_PER_FRAME: usize = 4;

/// Cached empty chunk data for GPU clearing (avoids repeated allocations)
pub static EMPTY_CHUNK_DATA: std::sync::LazyLock<Vec<u8>> =
    std::sync::LazyLock::new(|| vec![0u8; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE]);

/// Cached empty model metadata for GPU clearing (2 bytes per block: model_id + rotation)
pub static EMPTY_MODEL_METADATA: std::sync::LazyLock<Vec<u8>> =
    std::sync::LazyLock::new(|| vec![0u8; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE * 2]);

// Day/night cycle constants
/// Duration of a full day cycle in seconds (real time)
pub const DAY_CYCLE_DURATION: f32 = 120.0;
/// Default time of day (0.0 = midnight, 0.5 = noon, formula: hours = v * 24)
/// 14/24 ≈ 0.5833 = 14:00 (2pm)
pub const DEFAULT_TIME_OF_DAY: f32 = 14.0 / 24.0;

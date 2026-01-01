use crate::chunk::CHUNK_SIZE;

// World height in chunks (fixed - Y dimension is bounded)
pub const WORLD_CHUNKS_Y: i32 = 4;

// Texture pool dimensions for loaded chunks (X and Z are centered on player)
// This defines how many chunks can be loaded at once, not world bounds
pub const LOADED_CHUNKS_X: i32 = 16; // Chunks loaded in X direction (8 each side of player)
pub const LOADED_CHUNKS_Z: i32 = 16; // Chunks loaded in Z direction (8 each side of player)

// GPU texture size in blocks (holds all currently loaded chunks)
pub const TEXTURE_SIZE_X: usize = LOADED_CHUNKS_X as usize * CHUNK_SIZE;
pub const TEXTURE_SIZE_Y: usize = WORLD_CHUNKS_Y as usize * CHUNK_SIZE;
pub const TEXTURE_SIZE_Z: usize = LOADED_CHUNKS_Z as usize * CHUNK_SIZE;

// Chunk streaming constants
/// View distance in chunks (horizontal - all Y levels loaded within this range)
pub const VIEW_DISTANCE: i32 = 6;
/// Unload distance in chunks (horizontal - chunks beyond this are unloaded)
pub const UNLOAD_DISTANCE: i32 = 7;
/// Maximum chunks to load or unload per frame
pub const CHUNKS_PER_FRAME: usize = 4;

/// Cached empty chunk data for GPU clearing (avoids repeated allocations)
pub static EMPTY_CHUNK_DATA: std::sync::LazyLock<Vec<u8>> =
    std::sync::LazyLock::new(|| vec![0u8; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE]);

/// Cached empty model metadata for GPU clearing (2 bytes per block: model_id + rotation)
pub static EMPTY_MODEL_METADATA: std::sync::LazyLock<Vec<u8>> =
    std::sync::LazyLock::new(|| vec![0u8; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE * 2]);

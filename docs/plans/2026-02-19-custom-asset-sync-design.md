# Custom Model & Texture Sync for Multiplayer

**Date:** 2026-02-19
**Status:** Approved
**Approach:** Minimal Extension (Approach 1)

## Overview

Sync custom sub-voxel models and server-hosted custom textures from server to client in multiplayer games. Both content types are server-authoritative - only the host can add models or textures.

## Requirements Summary

| Aspect | Decision |
|--------|----------|
| Content types | Custom models + custom textures |
| Texture model | Per-world fixed pool (configurable, default 32 slots) |
| Sync timing | Hybrid (models on connect, textures lazy) |
| Authority | Server-authoritative for both |
| Texture pool | Configurable, default 32 slots |

## Architecture

### Network Protocol Changes

#### New Server → Client Messages

```rust
/// Sent immediately after ConnectionAccepted.
/// Contains all custom model definitions (IDs ≥ 151).
pub struct ModelRegistrySync {
    /// Serialized WorldModelStore (same format as models.dat)
    pub models_data: Vec<u8>,  // LZ4 compressed
    /// Custom door pairs
    pub door_pairs_data: Vec<u8>,  // LZ4 compressed
}

/// Sent when client requests a texture they don't have.
pub struct TextureData {
    /// Slot index (0-31 by default)
    pub slot: u8,
    /// PNG data (64x64 RGBA)
    pub data: Vec<u8>,
}

/// Notification that a new texture was added to the pool.
pub struct TextureAdded {
    pub slot: u8,
    pub name: String,
}
```

#### New Client → Server Messages

```rust
/// Client requests a texture they encountered but don't have.
pub struct RequestTexture {
    pub slot: u8,
}
```

#### Modified Messages

- `ConnectionAccepted` - add `custom_texture_count: u8` field so client knows the pool size

### Custom Texture Storage

#### Server-Side Directory Structure

```
world/
├── custom_textures/
│   ├── slot_00.png    # 64x64 RGBA
│   ├── slot_01.png
│   ├── ...
│   └── metadata.json  # Slot assignments {"0": "cobblestone_variant", "1": "moss_pattern", ...}
├── models.dat         # Existing - custom models
└── door_pairs.dat     # Existing - custom doors
```

#### ServerConfig Addition

```rust
// In user_prefs.rs or similar
pub struct ServerConfig {
    // ... existing fields ...
    /// Maximum custom texture slots (default: 32)
    pub max_custom_textures: u8,
}
```

#### TextureSlotManager (Server-Side)

```rust
pub struct TextureSlotManager {
    /// Path to custom_textures directory
    base_path: PathBuf,
    /// Maximum slots (from config)
    max_slots: u8,
    /// Slot → Name mapping (loaded from metadata.json)
    slot_names: HashMap<u8, String>,
    /// Next available slot
    next_free: u8,
}

impl TextureSlotManager {
    /// Add a new texture from PNG data, returns assigned slot.
    pub fn add_texture(&mut self, name: &str, png_data: &[u8]) -> Result<u8, String>;

    /// Get texture PNG data for a slot.
    pub fn get_texture(&self, slot: u8) -> Option<Vec<u8>>;

    /// List all slots with names.
    pub fn list_slots(&self) -> Vec<(u8, String)>;
}
```

### Client-Side Handling

#### CustomTextureCache (Client-Side)

```rust
pub struct CustomTextureCache {
    /// Maximum slots (received from server on connect)
    max_slots: u8,
    /// Cached texture PNG data (slot → data)
    textures: HashMap<u8, Vec<u8>>,
    /// Slots currently being requested (avoid duplicate requests)
    pending_requests: HashSet<u8>,
}

impl CustomTextureCache {
    /// Check if we have a texture cached.
    pub fn has_texture(&self, slot: u8) -> bool;

    /// Get cached texture data.
    pub fn get_texture(&self, slot: u8) -> Option<&[u8]>;

    /// Mark a texture as needed (triggers request if not cached).
    pub fn request_if_needed(&mut self, slot: u8, client: &mut GameClient);

    /// Store received texture data.
    pub fn store_texture(&mut self, slot: u8, data: Vec<u8>);
}
```

#### Texture Index Interpretation

Update `BlockPaintData` handling to interpret texture indices:

```rust
// In materials.glsl or Rust equivalent
fn get_texture_color(texture_idx: u8, custom_textures: &[u8]) -> Color {
    if texture_idx < 128 {
        // Standard atlas texture (existing behavior)
        sample_atlas(texture_idx)
    } else {
        // Custom texture slot (128 + slot_index)
        let slot = texture_idx - 128;
        sample_custom_texture(slot, custom_textures)
    }
}
```

#### GPU Texture Array Extension

The custom texture pool gets uploaded to GPU as a separate texture array:

- **Dimensions:** 64×64 × max_slots
- **Format:** RGBA8
- **Binding:** Separate from main atlas, sampled when texture_idx ≥ 128

### Sync Flow

#### Connection Sequence

```
Client                              Server
   |                                   |
   |-------- Connect ---------------->|
   |                                   |
   |<-- ConnectionAccepted ------------|
   |    (player_id, world_seed,        |
   |     custom_texture_count)         |
   |                                   |
   |<-- ModelRegistrySync -------------|
   |    (compressed models.dat,        |
   |     compressed door_pairs.dat)    |
   |                                   |
   | [Client decompresses and          |
   |  populates ModelRegistry]         |
   |                                   |
   |<-- ChunkData / ChunkGenerateLocal-|
   |    (normal gameplay begins)       |
   |                                   |
```

#### Texture Request Flow (Lazy Loading)

```
Client                              Server
   |                                   |
   | [Client renders chunk with        |
   |  texture_idx = 130 (custom)]      |
   |                                   |
   |--- RequestTexture { slot: 2 } --->|
   |                                   |
   |<-- TextureData { slot: 2,         |
   |     data: <png bytes> } ----------|
   |                                   |
   | [Client caches texture,           |
   |  uploads to GPU]                  |
   |                                   |
```

#### Host Adds New Texture

```
Host UI                             Server
   |                                   |
   |-- Add texture "moss_variant" ---->|
   |                                   |
   | [Server assigns slot, saves PNG]  |
   |                                   |
   |<-- TextureAdded broadcast -------->|
   |    (to all clients including host)|
   |                                   |
```

### Server-Side Management

#### Console Commands for Host

```bash
# Texture management
/texture add <filepath> <name>     # Add texture from file, returns assigned slot
/texture list                      # List all custom textures with slots
/texture remove <slot>             # Remove texture (only if not in use)
/texture info <slot>               # Show texture details

# Model management (models created in-editor, these manage the pool)
/model list                        # List all custom models
/model export <name> <filepath>    # Export model to .vxm file
/model import <filepath>           # Import model from .vxm file
/model delete <name>               # Delete custom model
```

#### Validation Rules

```rust
// Texture validation
fn validate_texture(png_data: &[u8]) -> Result<(), String> {
    // Must be valid PNG
    // Must be exactly 64x64 pixels
    // Must be RGBA format (or converted)
    // File size reasonable (< 64KB recommended)
}

// Model validation (existing from WorldModelStore)
fn validate_model(model: &SubVoxelModel) -> Result<(), String> {
    // Name must be unique
    // ID must be >= FIRST_CUSTOM_MODEL_ID
    // Voxels must be valid palette indices
}
```

## Implementation Scope

### Files to Create

| File | Purpose |
|------|---------|
| `src/net/asset_sync.rs` | ModelRegistrySync, TextureRequest/TextureData handling |
| `src/net/texture_slots.rs` | TextureSlotManager (server) and CustomTextureCache (client) |

### Files to Modify

| File | Changes |
|------|---------|
| `src/net/protocol.rs` | Add ModelRegistrySync, TextureData, TextureAdded, RequestTexture messages |
| `src/net/server.rs` | Send ModelRegistrySync on connect, handle texture requests |
| `src/net/client.rs` | Handle ModelRegistrySync, TextureData messages |
| `src/app_state/multiplayer.rs` | Add CustomTextureCache, integrate with multiplayer state |
| `src/user_prefs.rs` | Add `max_custom_textures` config option |
| `src/storage/mod.rs` | Expose texture_slots module |
| `shaders/materials.glsl` | Add custom texture sampling for indices ≥128 |
| `src/app/init.rs` or equivalent | Upload custom texture array to GPU |

### Estimated Complexity

| Component | Effort |
|-----------|--------|
| Protocol messages | Small |
| TextureSlotManager | Medium |
| CustomTextureCache | Medium |
| Server integration | Medium |
| Client integration | Medium |
| GPU texture array | Medium |
| Console commands | Small |
| **Total** | ~Medium-Large |

## Design Decisions

### Why Hybrid Sync?

- **Models on connect:** Model registry is small (typically < 100KB compressed) and needed immediately for chunk rendering
- **Textures lazy:** Textures are larger (64x64 RGBA = 16KB each), only load what's actually visible

### Why Server-Authoritative?

- Prevents cheating (malicious textures/models)
- Ensures all clients see the same content
- Simplifies conflict resolution (no merge logic needed)
- Matches the existing chunk/block sync model

### Why Fixed Pool Instead of Dynamic?

- Predictable memory usage on GPU
- Simple slot-based indexing in BlockPaintData
- No fragmentation or garbage collection needed
- 32 slots sufficient for most use cases (configurable for power users)

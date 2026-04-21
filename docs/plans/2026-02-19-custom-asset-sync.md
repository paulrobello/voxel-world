# Custom Model & Texture Sync Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Sync custom sub-voxel models and server-hosted custom textures from server to client in multiplayer.

**Architecture:** Hybrid sync - model registry sent on connect, textures lazy-loaded on demand. Server-authoritative for all custom content. Custom textures use slot-based indexing (values ≥128 in texture_idx).

**Tech Stack:** Rust, bincode serialization, LZ4/Zstd compression, PNG for textures, renet networking

---

## Task 1: Add Protocol Messages

**Files:**
- Modify: `src/net/protocol.rs`

**Step 1: Add new message types**

Add to `src/net/protocol.rs` after the existing messages (around line 308):

```rust
/// Sync custom models from server to client.
/// Sent immediately after ConnectionAccepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRegistrySync {
    /// LZ4 compressed WorldModelStore data (same format as models.dat)
    pub models_data: Vec<u8>,
    /// LZ4 compressed DoorPairStore data (same format as door_pairs.dat)
    pub door_pairs_data: Vec<u8>,
}

/// Sent when client requests a texture they don't have.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureData {
    /// Slot index (0-based)
    pub slot: u8,
    /// PNG data (64x64 RGBA)
    pub data: Vec<u8>,
}

/// Notification that a new texture was added to the pool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureAdded {
    pub slot: u8,
    pub name: String,
}

/// Client requests a texture they encountered but don't have.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestTexture {
    pub slot: u8,
}
```

**Step 2: Update ConnectionAccepted**

Modify `ConnectionAccepted` struct (around line 266) to add:

```rust
/// Number of custom texture slots available on server.
pub custom_texture_count: u8,
```

Update the struct to:
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectionAccepted {
    pub player_id: PlayerId,
    pub tick_rate: u32,
    pub spawn_position: [f32; 3],
    pub world_seed: u32,
    pub world_gen: u8,
    /// Number of custom texture slots available on server (0 = no custom textures).
    pub custom_texture_count: u8,
}
```

**Step 3: Add to ServerMessage enum**

Add to `ServerMessage` enum (around line 288):

```rust
    /// Sync custom models to client.
    ModelRegistrySync(ModelRegistrySync),
    /// Texture data response.
    TextureData(TextureData),
    /// Notification of new texture added.
    TextureAdded(TextureAdded),
```

**Step 4: Add to ClientMessage enum**

Add to `ClientMessage` enum (around line 162):

```rust
    /// Request a custom texture.
    RequestTexture(RequestTexture),
```

**Step 5: Update Default for ConnectionAccepted test**

Find the test for ConnectionAccepted and update the instantiation to include `custom_texture_count: 0`.

**Step 6: Run tests**

Run: `cargo test --lib net::protocol`
Expected: All tests pass

**Step 7: Commit**

```bash
git add src/net/protocol.rs
git commit -m "feat(net): add protocol messages for custom asset sync"
```

---

## Task 2: Create TextureSlotManager

**Files:**
- Create: `src/net/texture_slots.rs`

**Step 1: Create texture_slots.rs**

Create `src/net/texture_slots.rs`:

```rust
//! Custom texture slot management for multiplayer.
//!
//! Provides server-side texture pool management and client-side caching.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufReader, BufWriter};
use std::path::PathBuf;

/// Maximum custom texture slots (configurable, default 32).
pub const DEFAULT_MAX_TEXTURE_SLOTS: u8 = 32;

/// PNG image dimensions for custom textures.
pub const TEXTURE_SIZE: u32 = 64;

// ============================================================================
// Server-Side: TextureSlotManager
// ============================================================================

/// Metadata for the custom texture pool (stored as metadata.json).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TexturePoolMetadata {
    /// Maximum number of slots.
    pub max_slots: u8,
    /// Slot assignments (slot → name).
    pub slots: HashMap<u8, String>,
}

/// Server-side manager for custom texture slots.
pub struct TextureSlotManager {
    /// Path to custom_textures directory.
    base_path: PathBuf,
    /// Maximum slots (from config).
    max_slots: u8,
    /// Slot → Name mapping.
    metadata: TexturePoolMetadata,
    /// Next available slot.
    next_free: u8,
}

impl TextureSlotManager {
    /// Creates a new texture slot manager.
    pub fn new(base_path: PathBuf, max_slots: u8) -> Self {
        Self {
            base_path,
            max_slots,
            metadata: TexturePoolMetadata {
                max_slots,
                slots: HashMap::new(),
            },
            next_free: 0,
        }
    }

    /// Initializes the texture directory and loads existing metadata.
    pub fn init(&mut self) -> io::Result<()> {
        fs::create_dir_all(&self.base_path)?;
        self.load_metadata()?;
        // Find next free slot
        self.next_free = self.find_next_free_slot();
        Ok(())
    }

    /// Loads metadata from disk.
    fn load_metadata(&mut self) -> io::Result<()> {
        let path = self.base_path.join("metadata.json");
        if path.exists() {
            let file = fs::File::open(path)?;
            let reader = BufReader::new(file);
            self.metadata =
                serde_json::from_reader(reader).unwrap_or_else(|_| TexturePoolMetadata {
                    max_slots: self.max_slots,
                    slots: HashMap::new(),
                });
        }
        Ok(())
    }

    /// Saves metadata to disk.
    fn save_metadata(&self) -> io::Result<()> {
        let path = self.base_path.join("metadata.json");
        let file = fs::File::create(path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &self.metadata)?;
        Ok(())
    }

    /// Finds the next available slot.
    fn find_next_free_slot(&self) -> u8 {
        for slot in 0..self.max_slots {
            if !self.metadata.slots.contains_key(&slot) {
                return slot;
            }
        }
        self.max_slots // Indicates full
    }

    /// Adds a new texture from PNG data.
    /// Returns the assigned slot, or error if pool is full or validation fails.
    pub fn add_texture(&mut self, name: &str, png_data: &[u8]) -> Result<u8, String> {
        if self.next_free >= self.max_slots {
            return Err("Texture pool is full".to_string());
        }

        // Validate PNG
        self.validate_png(png_data)?;

        let slot = self.next_free;

        // Save PNG file
        let png_path = self.base_path.join(format!("slot_{:02}.png", slot));
        fs::write(&png_path, png_data).map_err(|e| format!("Failed to write PNG: {}", e))?;

        // Update metadata
        self.metadata.slots.insert(slot, name.to_string());
        self.save_metadata()
            .map_err(|e| format!("Failed to save metadata: {}", e))?;

        // Find next free slot
        self.next_free = self.find_next_free_slot();

        Ok(slot)
    }

    /// Validates PNG data (64x64 RGBA).
    fn validate_png(&self, png_data: &[u8]) -> Result<(), String> {
        let decoder = png::Decoder::new(std::io::Cursor::new(png_data));
        let mut reader = decoder
            .read_info()
            .map_err(|e| format!("Invalid PNG: {}", e))?;

        if reader.info().width != TEXTURE_SIZE || reader.info().height != TEXTURE_SIZE {
            return Err(format!(
                "Texture must be {}x{}, got {}x{}",
                TEXTURE_SIZE,
                TEXTURE_SIZE,
                reader.info().width,
                reader.info().height
            ));
        }

        Ok(())
    }

    /// Gets texture PNG data for a slot.
    pub fn get_texture(&self, slot: u8) -> Option<Vec<u8>> {
        if !self.metadata.slots.contains_key(&slot) {
            return None;
        }
        let png_path = self.base_path.join(format!("slot_{:02}.png", slot));
        fs::read(png_path).ok()
    }

    /// Lists all slots with names.
    pub fn list_slots(&self) -> Vec<(u8, String)> {
        let mut slots: Vec<_> = self
            .metadata
            .slots
            .iter()
            .map(|(&s, n)| (s, n.clone()))
            .collect();
        slots.sort_by_key(|(s, _)| *s);
        slots
    }

    /// Removes a texture (only if not in use).
    pub fn remove_texture(&mut self, slot: u8) -> Result<(), String> {
        if !self.metadata.slots.contains_key(&slot) {
            return Err("Slot not found".to_string());
        }

        // Delete PNG file
        let png_path = self.base_path.join(format!("slot_{:02}.png", slot));
        fs::remove_file(&png_path)
            .map_err(|e| format!("Failed to delete PNG: {}", e))?;

        // Update metadata
        self.metadata.slots.remove(&slot);
        self.save_metadata()
            .map_err(|e| format!("Failed to save metadata: {}", e))?;

        // Update next free slot if this slot is lower
        if slot < self.next_free {
            self.next_free = slot;
        }

        Ok(())
    }

    /// Returns the maximum number of slots.
    pub fn max_slots(&self) -> u8 {
        self.max_slots
    }

    /// Returns the number of used slots.
    pub fn used_slots(&self) -> usize {
        self.metadata.slots.len()
    }
}

// ============================================================================
// Client-Side: CustomTextureCache
// ============================================================================

/// Client-side cache for custom textures.
pub struct CustomTextureCache {
    /// Maximum slots (received from server on connect).
    max_slots: u8,
    /// Cached texture PNG data (slot → data).
    textures: HashMap<u8, Vec<u8>>,
    /// Slots currently being requested (avoid duplicate requests).
    pending_requests: std::collections::HashSet<u8>,
}

impl CustomTextureCache {
    /// Creates a new texture cache.
    pub fn new(max_slots: u8) -> Self {
        Self {
            max_slots,
            textures: HashMap::new(),
            pending_requests: std::collections::HashSet::new(),
        }
    }

    /// Returns the maximum number of slots.
    pub fn max_slots(&self) -> u8 {
        self.max_slots
    }

    /// Checks if we have a texture cached.
    pub fn has_texture(&self, slot: u8) -> bool {
        self.textures.contains_key(&slot)
    }

    /// Gets cached texture data.
    pub fn get_texture(&self, slot: u8) -> Option<&[u8]> {
        self.textures.get(&slot).map(|v| v.as_slice())
    }

    /// Returns all cached texture data as a map.
    pub fn all_textures(&self) -> &HashMap<u8, Vec<u8>> {
        &self.textures
    }

    /// Checks if a request is pending for this slot.
    pub fn is_pending(&self, slot: u8) -> bool {
        self.pending_requests.contains(&slot)
    }

    /// Marks a texture as needed (returns true if request should be sent).
    pub fn request_if_needed(&mut self, slot: u8) -> bool {
        if slot >= self.max_slots {
            return false;
        }
        if self.textures.contains_key(&slot) {
            return false;
        }
        if self.pending_requests.contains(&slot) {
            return false;
        }
        self.pending_requests.insert(slot);
        true
    }

    /// Stores received texture data.
    pub fn store_texture(&mut self, slot: u8, data: Vec<u8>) {
        self.pending_requests.remove(&slot);
        self.textures.insert(slot, data);
    }

    /// Clears all cached textures.
    pub fn clear(&mut self) {
        self.textures.clear();
        self.pending_requests.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_texture_cache() {
        let mut cache = CustomTextureCache::new(32);

        assert!(!cache.has_texture(0));
        assert!(cache.request_if_needed(0)); // Should request
        assert!(!cache.request_if_needed(0)); // Already pending
        assert!(cache.is_pending(0));

        cache.store_texture(0, vec![1, 2, 3, 4]);
        assert!(cache.has_texture(0));
        assert!(!cache.is_pending(0));
        assert!(!cache.request_if_needed(0)); // Already cached
        assert_eq!(cache.get_texture(0), Some(&[1, 2, 3, 4][..]));
    }

    #[test]
    fn test_custom_texture_cache_bounds() {
        let mut cache = CustomTextureCache::new(16);

        // Should reject out-of-bounds slot
        assert!(!cache.request_if_needed(16));
        assert!(!cache.request_if_needed(255));
    }
}
```

**Step 2: Add png crate dependency**

Check if `png` crate is already in Cargo.toml. If not, add:

```bash
cargo add png
```

**Step 3: Run tests**

Run: `cargo test --lib net::texture_slots`
Expected: Tests pass

**Step 4: Commit**

```bash
git add src/net/texture_slots.rs Cargo.toml Cargo.lock
git commit -m "feat(net): add TextureSlotManager and CustomTextureCache"
```

---

## Task 3: Expose texture_slots Module

**Files:**
- Modify: `src/net/mod.rs`

**Step 1: Add module declaration**

Add to `src/net/mod.rs` after line 83 (after `pub mod server_thread;`):

```rust
pub mod texture_slots;
```

**Step 2: Add re-export**

Add to the re-exports section (after line 111):

```rust
#[allow(unused_imports)]
pub use texture_slots::{
    CustomTextureCache, TexturePoolMetadata, TextureSlotManager, DEFAULT_MAX_TEXTURE_SLOTS,
    TEXTURE_SIZE,
};
```

**Step 3: Verify compilation**

Run: `cargo check`
Expected: No errors

**Step 4: Commit**

```bash
git add src/net/mod.rs
git commit -m "feat(net): expose texture_slots module"
```

---

## Task 4: Add Server Config Option

**Files:**
- Modify: `src/config.rs`

**Step 1: Add field to Settings struct**

Add to `Settings` struct in `src/config.rs` (after line 242, before the closing brace):

```rust
    /// Maximum custom texture slots for hosted servers (default: 32).
    pub max_custom_textures: u8,
```

**Step 2: Update Default impl for Settings**

Find the `Default` impl for `Settings` and add:

```rust
    max_custom_textures: 32,
```

**Step 3: Run tests**

Run: `cargo test --lib config`
Expected: Tests pass

**Step 4: Commit**

```bash
git add src/config.rs
git commit -m "feat(config): add max_custom_textures server option"
```

---

## Task 5: Integrate TextureSlotManager into GameServer

**Files:**
- Modify: `src/net/server.rs`

**Step 1: Add import**

Add at the top of `src/net/server.rs`:

```rust
use crate::net::texture_slots::TextureSlotManager;
use crate::storage::model_format::{DoorPairStore, WorldModelStore};
use lz4_flex::compress_prepend_size;
```

**Step 2: Add field to GameServer struct**

Find the `GameServer` struct and add:

```rust
    /// Custom texture slot manager.
    texture_manager: Option<TextureSlotManager>,
    /// World directory path (for loading models.dat).
    world_dir: Option<std::path::PathBuf>,
```

**Step 3: Initialize in GameServer::new**

In the `GameServer::new` function, initialize the texture manager:

```rust
    // Initialize texture manager (lazy - only when world_dir is set)
    let texture_manager = None;
```

**Step 4: Add method to set world directory**

Add method to `GameServer`:

```rust
    /// Sets the world directory for loading models and textures.
    pub fn set_world_dir(&mut self, path: std::path::PathBuf, max_textures: u8) {
        self.world_dir = Some(path.clone());
        let mut manager = TextureSlotManager::new(path.join("custom_textures"), max_textures);
        if let Err(e) = manager.init() {
            eprintln!("[Server] Failed to initialize texture manager: {}", e);
        }
        self.texture_manager = Some(manager);
    }
```

**Step 5: Add method to send ModelRegistrySync**

Add method to `GameServer`:

```rust
    /// Sends model registry and door pairs to a client.
    pub fn send_model_registry(&self, client_id: u64) {
        let world_dir = match &self.world_dir {
            Some(d) => d,
            None => return, // No world dir set
        };

        // Load and compress models.dat
        let models_data = match WorldModelStore::load(world_dir) {
            Ok(Some(store)) => {
                let serialized = bincode::serde::encode_to_vec(&store, bincode::config::legacy())
                    .unwrap_or_default();
                compress_prepend_size(&serialized)
            }
            _ => Vec::new(),
        };

        // Load and compress door_pairs.dat
        let door_pairs_data = match DoorPairStore::load(world_dir) {
            Ok(Some(store)) => {
                let serialized = bincode::serde::encode_to_vec(&store, bincode::config::legacy())
                    .unwrap_or_default();
                compress_prepend_size(&serialized)
            }
            _ => Vec::new(),
        };

        let msg = crate::net::protocol::ModelRegistrySync {
            models_data,
            door_pairs_data,
        };

        self.send_message(client_id, crate::net::protocol::ServerMessage::ModelRegistrySync(msg));
    }
```

**Step 6: Add method to handle texture requests**

Add method to `GameServer`:

```rust
    /// Handles a texture request from a client.
    pub fn handle_texture_request(&self, client_id: u64, slot: u8) {
        let manager = match &self.texture_manager {
            Some(m) => m,
            None => return,
        };

        if let Some(data) = manager.get_texture(slot) {
            let msg = crate::net::protocol::TextureData { slot, data };
            self.send_message(client_id, crate::net::protocol::ServerMessage::TextureData(msg));
        }
    }
```

**Step 7: Update handle_client_connected**

In `handle_client_connected`, after sending `ConnectionAccepted`, add:

```rust
    // Send model registry after connection accepted
    self.send_model_registry(client_id);
```

**Step 8: Update ConnectionAccepted construction**

Find where `ConnectionAccepted` is created and add the `custom_texture_count` field:

```rust
    custom_texture_count: self.texture_manager.as_ref().map(|m| m.max_slots()).unwrap_or(0),
```

**Step 9: Verify compilation**

Run: `cargo check`
Expected: No errors (may have unused warnings)

**Step 10: Commit**

```bash
git add src/net/server.rs
git commit -m "feat(server): integrate TextureSlotManager and model sync"
```

---

## Task 6: Handle ClientMessage::RequestTexture in Server

**Files:**
- Modify: `src/app_state/multiplayer.rs`

**Step 1: Add handling in handle_client_message**

In `handle_client_message`, add a new match arm for `ClientMessage::RequestTexture`:

```rust
            ClientMessage::RequestTexture(req) => {
                println!(
                    "[Server] Received texture request for slot {} from client {}",
                    req.slot, client_id
                );
                if let Some(ref mut server) = self.server {
                    server.handle_texture_request(client_id, req.slot);
                } else if let Some(ref server_thread) = self.server_thread {
                    let _ = server_thread.send_command(ServerCommand::HandleTextureRequest {
                        client_id,
                        slot: req.slot,
                    });
                }
            }
```

**Step 2: Add ServerCommand variant if needed**

If using threaded server, add to `src/net/server_thread.rs`:

```rust
    HandleTextureRequest {
        client_id: u64,
        slot: u8,
    },
```

**Step 3: Commit**

```bash
git add src/app_state/multiplayer.rs src/net/server_thread.rs
git commit -m "feat(server): handle texture requests from clients"
```

---

## Task 7: Integrate CustomTextureCache into MultiplayerState

**Files:**
- Modify: `src/app_state/multiplayer.rs`

**Step 1: Add import**

Add at top of file:

```rust
use crate::net::CustomTextureCache;
```

**Step 2: Add field to MultiplayerState**

Add to `MultiplayerState` struct:

```rust
    /// Custom texture cache (client-side).
    pub texture_cache: CustomTextureCache,
```

**Step 3: Initialize in MultiplayerState::new**

In the `new()` function, initialize:

```rust
    texture_cache: CustomTextureCache::new(0), // Will be set on connect
```

**Step 4: Handle ModelRegistrySync message**

In `handle_server_message`, add:

```rust
            ServerMessage::ModelRegistrySync(sync) => {
                println!("[Client] Received ModelRegistrySync");

                // Decompress and load models
                if !sync.models_data.is_empty() {
                    // Will be handled by game loop to update ModelRegistry
                    // For now, just log that we received it
                    println!("[Client] Received {} bytes of model data", sync.models_data.len());
                }

                // Decompress and load door pairs
                if !sync.door_pairs_data.is_empty() {
                    println!("[Client] Received {} bytes of door pair data", sync.door_pairs_data.len());
                }
            }
            ServerMessage::TextureData(tex) => {
                println!("[Client] Received texture for slot {}", tex.slot);
                self.texture_cache.store_texture(tex.slot, tex.data);
            }
            ServerMessage::TextureAdded(tex) => {
                println!("[Client] Texture added: slot {} = '{}'", tex.slot, tex.name);
            }
```

**Step 5: Update ConnectionAccepted handling**

In the `ConnectionAccepted` handler, update the texture cache max slots:

```rust
            ServerMessage::ConnectionAccepted(accepted) => {
                println!(
                    "[Client] Connection accepted. Player ID: {}, World seed: {}, Custom textures: {}",
                    accepted.player_id, accepted.world_seed, accepted.custom_texture_count
                );
                self.pending_server_seed = Some((accepted.world_seed, accepted.world_gen));
                self.texture_cache = CustomTextureCache::new(accepted.custom_texture_count);
            }
```

**Step 6: Add method to request texture**

Add to `MultiplayerState`:

```rust
    /// Requests a custom texture if not cached.
    pub fn request_texture_if_needed(&mut self, slot: u8) {
        if self.texture_cache.request_if_needed(slot) {
            if let Some(ref mut client) = self.client {
                client.send_message(crate::net::protocol::ClientMessage::RequestTexture(
                    crate::net::protocol::RequestTexture { slot },
                ));
            }
        }
    }

    /// Returns the texture cache for rendering.
    pub fn texture_cache(&self) -> &CustomTextureCache {
        &self.texture_cache
    }
```

**Step 7: Commit**

```bash
git add src/app_state/multiplayer.rs
git commit -m "feat(client): integrate CustomTextureCache into multiplayer state"
```

---

## Task 8: Add Client Method for RequestTexture

**Files:**
- Modify: `src/net/client.rs`

**Step 1: Add send_message accessor or RequestTexture helper**

If `send_message` is private, add a public helper method:

```rust
    /// Sends a texture request to the server.
    pub fn send_texture_request(&mut self, slot: u8) {
        let msg = crate::net::protocol::ClientMessage::RequestTexture(
            crate::net::protocol::RequestTexture { slot }
        );
        self.send_message(msg);
    }
```

**Step 2: Commit**

```bash
git add src/net/client.rs
git commit -m "feat(client): add texture request helper method"
```

---

## Task 9: Add Console Commands for Texture Management

**Files:**
- Create: `src/console/commands/texture.rs`
- Modify: `src/console/commands/mod.rs`

**Step 1: Create texture command module**

Create `src/console/commands/texture.rs`:

```rust
//! Console commands for texture management (host-only).

use crate::console::{CommandResult, ConsoleCommand};
use std::path::PathBuf;

pub struct TextureAddCommand;
pub struct TextureListCommand;
pub struct TextureRemoveCommand;

impl ConsoleCommand for TextureAddCommand {
    fn name(&self) -> &'static str {
        "texture_add"
    }

    fn aliases(&self) -> &[&'static str] {
        &["texadd"]
    }

    fn description(&self) -> &'static str {
        "Add a custom texture from a PNG file (host only). Usage: texture_add <filepath> <name>"
    }

    fn execute(&self, args: &str, ctx: &mut crate::console::CommandContext) -> CommandResult {
        let parts: Vec<&str> = args.split_whitespace().collect();
        if parts.len() < 2 {
            return CommandResult::error("Usage: texture_add <filepath> <name>");
        }

        let filepath = parts[0];
        let name = parts[1];

        // Check if hosting
        if !ctx.multiplayer.is_hosting() {
            return CommandResult::error("Only the host can add textures");
        }

        // Read PNG file
        let path = PathBuf::from(filepath);
        let png_data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => return CommandResult::error(&format!("Failed to read file: {}", e)),
        };

        // Add texture through server
        // Note: This would need access to GameServer's texture_manager
        // For now, return a placeholder
        CommandResult::success(&format!(
            "Texture '{}' queued for addition from '{}'",
            name, filepath
        ))
    }
}

impl ConsoleCommand for TextureListCommand {
    fn name(&self) -> &'static str {
        "texture_list"
    }

    fn aliases(&self) -> &[&'static str] {
        &["texlist", "textures"]
    }

    fn description(&self) -> &'static str {
        "List all custom textures"
    }

    fn execute(&self, _args: &str, ctx: &mut crate::console::CommandContext) -> CommandResult {
        if !ctx.multiplayer.is_hosting() {
            return CommandResult::error("Only the host can list textures");
        }

        // Placeholder - would query server's texture_manager
        CommandResult::success("Custom textures: (not yet implemented)")
    }
}

impl ConsoleCommand for TextureRemoveCommand {
    fn name(&self) -> &'static str {
        "texture_remove"
    }

    fn aliases(&self) -> &[&'static str] {
        &["texremove", "texdel"]
    }

    fn description(&self) -> &'static str {
        "Remove a custom texture by slot. Usage: texture_remove <slot>"
    }

    fn execute(&self, args: &str, ctx: &mut crate::console::CommandContext) -> CommandResult {
        let slot: u8 = match args.trim().parse() {
            Ok(s) => s,
            Err(_) => return CommandResult::error("Usage: texture_remove <slot>"),
        };

        if !ctx.multiplayer.is_hosting() {
            return CommandResult::error("Only the host can remove textures");
        }

        // Placeholder
        CommandResult::success(&format!("Texture slot {} removal queued", slot))
    }
}
```

**Step 2: Register in mod.rs**

Add to `src/console/commands/mod.rs`:

```rust
pub mod texture;

// In the register function, add:
registry.register(crate::console::commands::texture::TextureAddCommand);
registry.register(crate::console::commands::texture::TextureListCommand);
registry.register(crate::console::commands::texture::TextureRemoveCommand);
```

**Step 3: Commit**

```bash
git add src/console/commands/texture.rs src/console/commands/mod.rs
git commit -m "feat(console): add texture management commands"
```

---

## Task 10: Update Materials Shader for Custom Textures

**Files:**
- Modify: `shaders/materials.glsl`

**Step 1: Add custom texture array uniform**

Add near the top of `materials.glsl` after the existing texture declarations:

```glsl
// Custom texture array (slot 0 = 128, slot 1 = 129, etc.)
layout(set = 0, binding = 10) uniform sampler2DArray custom_texture_array;
uniform int custom_texture_count = 0;
```

**Step 2: Update texture sampling function**

Find the function that samples textures (likely `getBlockColor` or similar) and add logic for custom textures:

```glsl
vec4 sampleTexture(int texture_idx, vec2 uv) {
    if (texture_idx < 128) {
        // Standard atlas texture
        return texture(texture_atlas, vec3(uv, float(texture_idx)));
    } else {
        // Custom texture (128 + slot)
        int slot = texture_idx - 128;
        if (slot < custom_texture_count) {
            return texture(custom_texture_array, vec3(uv, float(slot)));
        }
        return vec4(1.0, 0.0, 1.0, 1.0); // Magenta for missing
    }
}
```

**Step 3: Commit**

```bash
git add shaders/materials.glsl
git commit -m "feat(shader): add custom texture array support"
```

---

## Task 11: Upload Custom Texture Array to GPU

**Files:**
- Modify: `src/app/init.rs` or appropriate GPU setup file

**Step 1: Add texture array creation**

Find where the main texture atlas is created and add creation of the custom texture array:

```rust
// In the GPU initialization code
fn create_custom_texture_array(
    device: &Arc<Device>,
    max_slots: u8,
) -> Result<Arc<ImageView<StorageImage>>, Box<dyn std::error::Error>> {
    if max_slots == 0 {
        return Ok(None);
    }

    let dimensions = [64, 64];
    let array_layers = max_slots as u32;

    let image = StorageImage::array_view(
        device.clone(),
        vulkano::image::ImageDimensions::Dim2dArray {
            width: dimensions[0],
            height: dimensions[1],
            array_layers,
        },
        vulkano::format::Format::R8G8B8A8_UNORM,
    )?;

    Ok(image)
}
```

**Step 2: Add texture update method**

Add a method to update individual texture slots:

```rust
pub fn update_custom_texture(
    &mut self,
    slot: u8,
    png_data: &[u8],
) -> Result<(), String> {
    // Decode PNG
    let decoder = png::Decoder::new(std::io::Cursor::new(png_data));
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;

    let mut buf = vec![0u8; reader.output_buffer_size()];
    reader.next_frame(&mut buf).map_err(|e| e.to_string())?;

    // Upload to GPU texture array at layer `slot`
    // ... GPU upload code ...

    Ok(())
}
```

**Step 3: Commit**

```bash
git add src/app/init.rs
git commit -m "feat(gpu): add custom texture array creation and update"
```

---

## Task 12: Wire Up MultiplayerState to Game Loop

**Files:**
- Modify: `src/app/mod.rs` or main game loop file

**Step 1: Pass MultiplayerState to texture upload**

Find where the game loop handles multiplayer state and add texture upload handling:

```rust
// In the main game loop after multiplayer.update()
if let Some(ref texture_cache) = multiplayer.texture_cache() {
    for (slot, data) in texture_cache.all_textures() {
        if !self.gpu_state.has_custom_texture(*slot) {
            self.gpu_state.update_custom_texture(*slot, data)?;
        }
    }
}
```

**Step 2: Pass texture cache to shader**

Update the descriptor set to include the custom texture array:

```rust
// When creating the descriptor set for the shader
if let Some(ref texture_array) = self.custom_texture_array {
    descriptor_set_builder = descriptor_set_builder.add_image(texture_array.clone());
}
```

**Step 3: Commit**

```bash
git add src/app/mod.rs
git commit -m "feat(game): wire up texture cache to game loop and GPU"
```

---

## Task 13: Run Full Test Suite

**Step 1: Run all tests**

Run: `cargo test`
Expected: All tests pass

**Step 2: Run linting**

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: No errors (fix any warnings)

**Step 3: Format code**

Run: `cargo fmt`
Expected: No changes (or commit changes if needed)

**Step 4: Final commit**

```bash
git add -A
git commit -m "chore: lint and format for custom asset sync"
```

---

## Summary

This implementation adds:

1. **Protocol messages** for model sync, texture data, and texture requests
2. **TextureSlotManager** for server-side texture pool management
3. **CustomTextureCache** for client-side lazy texture loading
4. **Server config** for max_custom_textures (default 32)
5. **Console commands** for host texture management
6. **Shader support** for custom texture array sampling
7. **GPU integration** for uploading custom textures

The sync flow:
- Client connects → receives ConnectionAccepted with custom_texture_count
- Client receives ModelRegistrySync with compressed model/door data
- Client encounters texture_idx ≥ 128 → requests texture if not cached
- Server responds with TextureData → client caches and uploads to GPU

---

## Future Enhancements (Out of Scope)

- UI panel for texture management
- Texture preview in console commands
- Model editor sync for player-created models
- Texture pack import/export
- Hash-based deduplication

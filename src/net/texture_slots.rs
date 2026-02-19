//! Custom texture slot management for multiplayer.
//!
//! Provides server-side texture pool management and client-side caching.

// These types will be used by subsequent integration tasks
#![allow(dead_code)]

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
        let reader = decoder
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
        fs::remove_file(&png_path).map_err(|e| format!("Failed to delete PNG: {}", e))?;

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

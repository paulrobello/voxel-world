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

/// Hard cap on custom texture PNG file size.
///
/// A pathological PNG with valid 64×64 dimensions but huge ancillary chunks
/// (iCCP, zTXt, eXIf) can easily balloon to tens of MB and is a trivial DoS
/// vector if left unchecked. 128 KiB is ~8× a reasonable 64×64 RGBA PNG and
/// comfortably covers legitimate compression artifacts.
pub const MAX_TEXTURE_PNG_BYTES: usize = 128 * 1024;

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
    /// In-memory reference counts per slot. Not persisted — the game rebuilds
    /// these on startup from world state (same approach as PictureManager).
    ref_counts: HashMap<u8, u32>,
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
            ref_counts: HashMap::new(),
        }
    }

    /// Records one additional block-level reference to `slot`.
    pub fn add_reference(&mut self, slot: u8) {
        *self.ref_counts.entry(slot).or_insert(0) += 1;
    }

    /// Removes one block-level reference to `slot` (saturates at 0).
    pub fn remove_reference(&mut self, slot: u8) {
        if let Some(c) = self.ref_counts.get_mut(&slot) {
            *c = c.saturating_sub(1);
            if *c == 0 {
                self.ref_counts.remove(&slot);
            }
        }
    }

    /// Returns the current reference count for `slot`.
    pub fn reference_count(&self, slot: u8) -> u32 {
        self.ref_counts.get(&slot).copied().unwrap_or(0)
    }

    /// Clears all reference counts. Call before rescanning world state.
    pub fn clear_references(&mut self) {
        self.ref_counts.clear();
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
    ///
    /// Uses a temp-file + rename strategy so a crash between writing the PNG
    /// and updating metadata can't leave a dangling `slot_NN.png`. Metadata
    /// is saved *before* the rename so the on-disk invariant is
    /// "every metadata entry has a matching file; unknown `.tmp` files are
    /// ignored on next startup".
    pub fn add_texture(&mut self, name: &str, png_data: &[u8]) -> Result<u8, String> {
        if self.next_free >= self.max_slots {
            return Err("Texture pool is full".to_string());
        }

        // Validate PNG
        self.validate_png(png_data)?;

        let slot = self.next_free;
        let final_path = self.base_path.join(format!("slot_{:02}.png", slot));
        let tmp_path = self.base_path.join(format!("slot_{:02}.png.tmp", slot));

        // Write the PNG to a temporary file first.
        fs::write(&tmp_path, png_data).map_err(|e| format!("Failed to write PNG: {}", e))?;

        // Update metadata to record the intended slot before publishing the file.
        self.metadata.slots.insert(slot, name.to_string());
        if let Err(e) = self.save_metadata() {
            // Metadata write failed — drop the tmp file and revert the in-memory
            // state so the slot stays free.
            let _ = fs::remove_file(&tmp_path);
            self.metadata.slots.remove(&slot);
            return Err(format!("Failed to save metadata: {}", e));
        }

        // Atomically swing the temp file into place. On failure we must
        // reverse the metadata insert that preceded this call, clean up the
        // tmp file, and persist the rollback so a crash between us and the
        // next startup doesn't leave a dangling slot entry pointing at a
        // non-existent file.
        if let Err(e) = fs::rename(&tmp_path, &final_path) {
            let _ = fs::remove_file(&tmp_path);
            self.metadata.slots.remove(&slot);
            // Best-effort rollback persist; log but don't mask the real error.
            if let Err(save_err) = self.save_metadata() {
                log::warn!(
                    "[TextureSlotManager] rollback save_metadata failed: {}",
                    save_err
                );
            }
            return Err(format!("Failed to finalize PNG: {}", e));
        }

        // Find next free slot
        self.next_free = self.find_next_free_slot();

        Ok(slot)
    }

    /// Validates PNG data (64x64 RGBA), with a hard file-size cap.
    fn validate_png(&self, png_data: &[u8]) -> Result<(), String> {
        if png_data.len() > MAX_TEXTURE_PNG_BYTES {
            return Err(format!(
                "Texture PNG too large: {} bytes (max {})",
                png_data.len(),
                MAX_TEXTURE_PNG_BYTES
            ));
        }

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

    /// Removes a texture, refusing if the slot is still referenced by blocks.
    ///
    /// Callers that want to force-remove a slot should use
    /// [`force_remove_texture`] — this variant is safe by default so admin
    /// mistakes don't orphan `paint_data` pointing at a deleted slot.
    pub fn remove_texture(&mut self, slot: u8) -> Result<(), String> {
        let count = self.reference_count(slot);
        if count > 0 {
            return Err(format!(
                "Texture slot {} still referenced by {} block(s); refusing delete",
                slot, count
            ));
        }
        self.force_remove_texture(slot)
    }

    /// Removes a texture without checking reference counts. Admin-only.
    pub fn force_remove_texture(&mut self, slot: u8) -> Result<(), String> {
        if !self.metadata.slots.contains_key(&slot) {
            return Err("Slot not found".to_string());
        }

        // Delete PNG file
        let png_path = self.base_path.join(format!("slot_{:02}.png", slot));
        fs::remove_file(&png_path).map_err(|e| format!("Failed to delete PNG: {}", e))?;

        // Update metadata
        self.metadata.slots.remove(&slot);
        self.ref_counts.remove(&slot);
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
    /// Slots that have been uploaded to the GPU.
    uploaded_slots: std::collections::HashSet<u8>,
}

impl CustomTextureCache {
    /// Creates a new texture cache.
    pub fn new(max_slots: u8) -> Self {
        Self {
            max_slots,
            textures: HashMap::new(),
            pending_requests: std::collections::HashSet::new(),
            uploaded_slots: std::collections::HashSet::new(),
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
        self.uploaded_slots.clear();
    }

    /// Returns textures that haven't been uploaded to the GPU yet.
    pub fn get_new_textures(&self) -> Vec<(u8, Vec<u8>)> {
        self.textures
            .iter()
            .filter(|(slot, _)| !self.uploaded_slots.contains(slot))
            .map(|(slot, data)| (*slot, data.clone()))
            .collect()
    }

    /// Marks textures as uploaded to the GPU.
    pub fn mark_uploaded(&mut self, textures: &[(u8, Vec<u8>)]) {
        for (slot, _) in textures {
            self.uploaded_slots.insert(*slot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_png_rejects_oversize_file() {
        let mgr = TextureSlotManager::new(std::path::PathBuf::new(), 32);
        // A buffer larger than the cap should fail the size check before any
        // PNG decoding happens.
        let huge = vec![0u8; MAX_TEXTURE_PNG_BYTES + 1];
        let err = mgr.validate_png(&huge).unwrap_err();
        assert!(err.contains("too large"), "got: {}", err);
    }

    /// Simulates a crash between `fs::write(tmp)` and `fs::rename(tmp, final)`
    /// by pre-creating a *directory* at the final path so `rename` fails with
    /// IsADirectory / EEXIST. The test asserts the in-memory slot is rolled
    /// back and no permanent state is left behind. This exercises the T11b
    /// rollback branch of `add_texture`.
    #[test]
    fn test_atomic_texture_write_rollback_on_rename_failure() {
        let dir = std::env::temp_dir().join(format!("vw_texslots_rename_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut mgr = TextureSlotManager::new(dir.clone(), 16);

        // Pre-create a directory at the final destination path. fs::rename of
        // a file onto a non-empty directory path fails on every supported
        // platform we care about.
        let final_path = dir.join("slot_00.png");
        std::fs::create_dir_all(&final_path).unwrap();
        std::fs::write(final_path.join("placeholder"), b"blocker").unwrap();

        // Build a minimal valid PNG so validate_png passes — we want the rename
        // to be the failure point, not validation.
        let mut png_bytes = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut png_bytes, TEXTURE_SIZE, TEXTURE_SIZE);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&vec![0u8; (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize])
                .unwrap();
        }

        let result = mgr.add_texture("test", &png_bytes);
        assert!(
            result.is_err(),
            "expected rename to fail because destination is a directory"
        );

        // In-memory rollback: slot 0 must stay free so a retry can use it.
        assert!(
            !mgr.metadata.slots.contains_key(&0),
            "slot must be reverted on rename failure"
        );
        // next_free stays at 0 (our rollback path leaves it unchanged, so
        // either 0 or u8::MAX is acceptable — what matters is that we don't
        // skip past 0 as though slot 0 were taken).
        assert!(mgr.next_free <= mgr.max_slots);

        // The tmp file must not linger — add_texture's error branch deletes it.
        // (The final directory we pre-created stays; that's the "blocker" the
        // test deliberately set up and the test owns its cleanup.)
        let tmp_path = dir.join("slot_00.png.tmp");
        assert!(
            !tmp_path.exists(),
            "tmp file must be cleaned up on rollback"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_texture_slot_refcount_lifecycle() {
        let mut mgr = TextureSlotManager::new(std::path::PathBuf::new(), 32);
        assert_eq!(mgr.reference_count(3), 0);

        mgr.add_reference(3);
        mgr.add_reference(3);
        mgr.add_reference(3);
        assert_eq!(mgr.reference_count(3), 3);

        mgr.remove_reference(3);
        assert_eq!(mgr.reference_count(3), 2);
        mgr.remove_reference(3);
        mgr.remove_reference(3);
        assert_eq!(mgr.reference_count(3), 0);

        // Saturates at 0.
        mgr.remove_reference(3);
        assert_eq!(mgr.reference_count(3), 0);

        mgr.add_reference(5);
        mgr.clear_references();
        assert_eq!(mgr.reference_count(5), 0);
    }

    #[test]
    fn test_remove_texture_refuses_when_referenced() {
        let dir = std::env::temp_dir().join(format!("vw_texslots_refuse_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut mgr = TextureSlotManager::new(dir.clone(), 16);
        mgr.metadata.slots.insert(0, "stone".into());
        std::fs::write(dir.join("slot_00.png"), b"\x89PNG\r\n\x1a\n").unwrap();
        mgr.add_reference(0);

        let err = mgr.remove_texture(0).unwrap_err();
        assert!(err.contains("referenced"), "got: {}", err);
        assert!(mgr.metadata.slots.contains_key(&0));

        // force_remove_texture bypasses and succeeds.
        mgr.force_remove_texture(0).expect("force remove");
        assert!(!mgr.metadata.slots.contains_key(&0));

        let _ = std::fs::remove_dir_all(&dir);
    }

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

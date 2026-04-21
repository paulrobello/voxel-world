//! Picture storage for picture frame multiplayer sync.
//!
//! Provides server-side picture storage and client-side caching for
//! picture frames. Pictures are uploaded by clients and assigned
//! unique IDs by the server.

// These types will be used by multiplayer integration
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufReader, BufWriter};
use std::path::PathBuf;

/// Maximum picture ID (u16::MAX = 65535 pictures).
pub const MAX_PICTURE_ID: u16 = u16::MAX;

/// Default maximum picture size (in bytes, 1MB default).
pub const DEFAULT_MAX_PICTURE_SIZE: usize = 1024 * 1024;

/// Maximum pictures per server (configurable).
pub const DEFAULT_MAX_PICTURES: u16 = 1024;

/// Maximum accepted picture name length (bytes). Prevents metadata.json from
/// ballooning if a client uploads thousands of pictures with 10 KB names.
pub const MAX_PICTURE_NAME_LEN: usize = 64;

// ============================================================================
// Server-Side: PictureManager
// ============================================================================

/// Metadata for the picture store (stored as metadata.json).
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct PictureStoreMetadata {
    /// Next available picture ID.
    pub next_id: u16,
    /// ID → Name mapping.
    pub pictures: HashMap<u16, String>,
}

/// Server-side manager for picture storage.
///
/// Pictures are stored on disk and assigned sequential IDs.
/// The manager persists metadata across server restarts.
pub struct PictureManager {
    /// Path to pictures directory.
    base_path: PathBuf,
    /// Maximum pictures (from config).
    max_pictures: u16,
    /// Maximum picture size in bytes.
    max_size: usize,
    /// Picture metadata.
    metadata: PictureStoreMetadata,
    /// In-memory reference counts. Populated by the game as it discovers
    /// blocks that embed a picture_id (paint_data, picture frames, etc.).
    /// Not persisted — rebuilt on startup from world state.
    ref_counts: HashMap<u16, u32>,
    /// In-memory content-hash → picture_id index for dedup on upload.
    /// Rebuilt on startup by `init()` scanning the on-disk PNGs; a 64-bit
    /// SipHash is good enough for a few thousand pictures (birthday-bound
    /// collision probability stays well below one-in-a-billion per session).
    content_hash_index: HashMap<u64, u16>,
}

impl PictureManager {
    /// Creates a new picture manager.
    pub fn new(base_path: PathBuf, max_pictures: u16) -> Self {
        Self {
            base_path,
            max_pictures,
            max_size: DEFAULT_MAX_PICTURE_SIZE,
            metadata: PictureStoreMetadata {
                next_id: 1, // Start at 1, 0 is reserved for "no picture"
                pictures: HashMap::new(),
            },
            ref_counts: HashMap::new(),
            content_hash_index: HashMap::new(),
        }
    }

    /// Returns a 64-bit content hash of `png_data`. Used for dedup on upload.
    fn hash_png(png_data: &[u8]) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        png_data.hash(&mut h);
        h.finish()
    }

    /// Rebuilds the content-hash index by reading every stored PNG. Called
    /// from `init()` after metadata load so uploads that match an
    /// already-persisted picture dedup correctly across restarts.
    fn rebuild_content_hash_index(&mut self) {
        self.content_hash_index.clear();
        for (&id, _name) in self.metadata.pictures.iter() {
            let path = self.base_path.join(format!("picture_{:05}.png", id));
            if let Ok(bytes) = fs::read(&path) {
                let h = Self::hash_png(&bytes);
                self.content_hash_index.entry(h).or_insert(id);
            }
        }
    }

    /// Sets the maximum picture size.
    pub fn with_max_size(mut self, max_size: usize) -> Self {
        self.max_size = max_size;
        self
    }

    /// Initializes the picture directory and loads existing metadata.
    pub fn init(&mut self) -> io::Result<()> {
        fs::create_dir_all(&self.base_path)?;
        self.load_metadata()?;
        self.rebuild_content_hash_index();
        Ok(())
    }

    /// Loads metadata from disk.
    fn load_metadata(&mut self) -> io::Result<()> {
        let path = self.base_path.join("metadata.json");
        if path.exists() {
            let file = fs::File::open(path)?;
            let reader = BufReader::new(file);
            self.metadata =
                serde_json::from_reader(reader).unwrap_or_else(|_| PictureStoreMetadata {
                    next_id: 1,
                    pictures: HashMap::new(),
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

    /// Adds a new picture from PNG data.
    /// Returns the assigned picture ID, or error if storage is full or validation fails.
    pub fn add_picture(&mut self, name: &str, png_data: &[u8]) -> Result<u16, String> {
        // Name length + path-traversal check before any I/O.
        if name.len() > MAX_PICTURE_NAME_LEN {
            return Err(format!(
                "Picture name too long: {} bytes (max {})",
                name.len(),
                MAX_PICTURE_NAME_LEN
            ));
        }
        if name.contains("..") || name.contains('/') || name.contains('\\') || name.contains('\0') {
            return Err("Picture name contains invalid characters".into());
        }

        // Check size limit
        if png_data.len() > self.max_size {
            return Err(format!(
                "Picture too large: {} bytes (max {})",
                png_data.len(),
                self.max_size
            ));
        }

        // Content-hash dedup: if the same bytes have been uploaded before,
        // short-circuit and return the existing ID. Callers see this as a
        // successful upload while the disk and storage quota are spared.
        let content_hash = Self::hash_png(png_data);
        if let Some(&existing_id) = self.content_hash_index.get(&content_hash) {
            log::debug!(
                "[PictureManager] Dedup hit on '{}' → reusing picture ID {}",
                name,
                existing_id
            );
            return Ok(existing_id);
        }

        // Check count limit
        if self.metadata.pictures.len() >= self.max_pictures as usize {
            return Err("Picture storage is full".to_string());
        }

        // Validate PNG
        self.validate_png(png_data)?;

        let picture_id = self.metadata.next_id;

        // Ensure we don't overflow
        if picture_id == 0 || picture_id == MAX_PICTURE_ID {
            return Err("Picture ID overflow".to_string());
        }

        // Save PNG file
        let png_path = self
            .base_path
            .join(format!("picture_{:05}.png", picture_id));
        fs::write(&png_path, png_data).map_err(|e| format!("Failed to write PNG: {}", e))?;

        // Update metadata + content-hash index together so a subsequent
        // upload of the same bytes hits the dedup path.
        self.metadata.pictures.insert(picture_id, name.to_string());
        self.metadata.next_id += 1;
        self.content_hash_index.insert(content_hash, picture_id);
        self.save_metadata()
            .map_err(|e| format!("Failed to save metadata: {}", e))?;

        log::debug!(
            "[PictureManager] Added picture '{}' with ID {}",
            name,
            picture_id
        );

        Ok(picture_id)
    }

    /// Validates PNG data.
    ///
    /// In addition to the magic-byte check, actually decodes the IHDR via the
    /// `png` crate so a crafted file with the right prefix but bogus chunks
    /// fails early instead of being written to disk and later rejected by
    /// consumers.
    fn validate_png(&self, png_data: &[u8]) -> Result<(), String> {
        if png_data.len() < 8 {
            return Err("Invalid PNG: too short".to_string());
        }

        let magic: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        if png_data[..8] != magic {
            return Err("Invalid PNG: wrong magic bytes".to_string());
        }

        // Full header decode catches malformed IHDR / ancillary chunks.
        let decoder = png::Decoder::new(std::io::Cursor::new(png_data));
        decoder
            .read_info()
            .map_err(|e| format!("Invalid PNG: {}", e))?;

        Ok(())
    }

    /// Gets picture PNG data by ID.
    pub fn get_picture(&self, picture_id: u16) -> Option<Vec<u8>> {
        if picture_id == 0 || !self.metadata.pictures.contains_key(&picture_id) {
            return None;
        }
        let png_path = self
            .base_path
            .join(format!("picture_{:05}.png", picture_id));
        fs::read(png_path).ok()
    }

    /// Gets picture name by ID.
    pub fn get_picture_name(&self, picture_id: u16) -> Option<&str> {
        self.metadata.pictures.get(&picture_id).map(|s| s.as_str())
    }

    /// Lists all pictures with IDs and names.
    pub fn list_pictures(&self) -> Vec<(u16, String)> {
        let mut pictures: Vec<_> = self
            .metadata
            .pictures
            .iter()
            .map(|(&id, name)| (id, name.clone()))
            .collect();
        pictures.sort_by_key(|(id, _)| *id);
        pictures
    }

    /// Removes a picture by ID, refusing if blocks in the world still reference it.
    ///
    /// Callers must `add_reference` for every block that embeds this picture_id
    /// and `remove_reference` when the block is broken/overwritten. Deletion is
    /// rejected while `reference_count(picture_id) > 0` to avoid orphaned
    /// paint_data. Use [`force_remove_picture`] for admin / wipe flows that
    /// explicitly want to drop the picture regardless.
    pub fn remove_picture(&mut self, picture_id: u16) -> Result<(), String> {
        let count = self.reference_count(picture_id);
        if count > 0 {
            return Err(format!(
                "Picture {} still referenced by {} block(s); refusing delete",
                picture_id, count
            ));
        }
        self.force_remove_picture(picture_id)
    }

    /// Removes a picture by ID without checking reference counts.
    /// Use only for admin wipes / world reset. Leaves any paint_data pointing
    /// at this ID dangling.
    pub fn force_remove_picture(&mut self, picture_id: u16) -> Result<(), String> {
        if picture_id == 0 {
            return Err("Invalid picture ID 0".to_string());
        }

        if !self.metadata.pictures.contains_key(&picture_id) {
            return Err("Picture not found".to_string());
        }

        // Delete PNG file — also read it first so we can drop the matching
        // entry from the content-hash index before it disappears. If the read
        // fails we best-effort drop any hash that points at this id.
        let png_path = self
            .base_path
            .join(format!("picture_{:05}.png", picture_id));
        let hash_to_drop = fs::read(&png_path).ok().map(|b| Self::hash_png(&b));
        fs::remove_file(&png_path).map_err(|e| format!("Failed to delete PNG: {}", e))?;

        // Update metadata + indexes
        self.metadata.pictures.remove(&picture_id);
        self.ref_counts.remove(&picture_id);
        if let Some(h) = hash_to_drop {
            self.content_hash_index.remove(&h);
        } else {
            // Couldn't hash — sweep the index to clear any stale pointer.
            self.content_hash_index.retain(|_, id| *id != picture_id);
        }
        self.save_metadata()
            .map_err(|e| format!("Failed to save metadata: {}", e))?;

        log::debug!("[PictureManager] Removed picture ID {}", picture_id);

        Ok(())
    }

    /// Records one additional block-level reference to `picture_id`.
    /// Called by the game when a block that embeds a picture is placed / loaded.
    /// No-op for picture_id == 0 (the "no picture" sentinel).
    pub fn add_reference(&mut self, picture_id: u16) {
        if picture_id == 0 {
            return;
        }
        *self.ref_counts.entry(picture_id).or_insert(0) += 1;
    }

    /// Removes one block-level reference to `picture_id`.
    /// Saturates at zero and cleans up the map entry once unused.
    pub fn remove_reference(&mut self, picture_id: u16) {
        if picture_id == 0 {
            return;
        }
        if let Some(count) = self.ref_counts.get_mut(&picture_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.ref_counts.remove(&picture_id);
            }
        }
    }

    /// Returns the current reference count for `picture_id`.
    pub fn reference_count(&self, picture_id: u16) -> u32 {
        self.ref_counts.get(&picture_id).copied().unwrap_or(0)
    }

    /// Clears all reference counts. Call before rescanning world state so the
    /// game can repopulate them from scratch on load.
    pub fn clear_references(&mut self) {
        self.ref_counts.clear();
    }

    /// Returns the number of stored pictures.
    pub fn picture_count(&self) -> usize {
        self.metadata.pictures.len()
    }

    /// Returns the maximum number of pictures.
    pub fn max_pictures(&self) -> u16 {
        self.max_pictures
    }
}

// ============================================================================
// Client-Side: PictureCache
// ============================================================================

/// Client-side cache for pictures.
///
/// Stores pictures received from the server for use in picture frames.
/// Pictures are cached in memory and can be requested on demand.
pub struct PictureCache {
    /// Cached picture PNG data (id → data).
    pictures: HashMap<u16, Vec<u8>>,
    /// Picture names (id → name).
    names: HashMap<u16, String>,
    /// IDs currently being requested (avoid duplicate requests).
    pending_requests: std::collections::HashSet<u16>,
}

impl Default for PictureCache {
    fn default() -> Self {
        Self::new()
    }
}

impl PictureCache {
    /// Creates a new picture cache.
    pub fn new() -> Self {
        Self {
            pictures: HashMap::new(),
            names: HashMap::new(),
            pending_requests: std::collections::HashSet::new(),
        }
    }

    /// Checks if we have a picture cached.
    /// Returns false for picture_id 0 (which means "no picture").
    pub fn has_picture(&self, picture_id: u16) -> bool {
        picture_id != 0 && self.pictures.contains_key(&picture_id)
    }

    /// Gets cached picture data.
    pub fn get_picture(&self, picture_id: u16) -> Option<&[u8]> {
        if picture_id == 0 {
            return None; // No picture
        }
        self.pictures.get(&picture_id).map(|v| v.as_slice())
    }

    /// Gets picture name by ID.
    pub fn get_name(&self, picture_id: u16) -> Option<&str> {
        self.names.get(&picture_id).map(|s| s.as_str())
    }

    /// Checks if a request is pending for this picture.
    pub fn is_pending(&self, picture_id: u16) -> bool {
        self.pending_requests.contains(&picture_id)
    }

    /// Marks a picture as needed (returns true if request should be sent).
    pub fn request_if_needed(&mut self, picture_id: u16) -> bool {
        if picture_id == 0 {
            return false; // No picture
        }
        if self.pictures.contains_key(&picture_id) {
            return false; // Already cached
        }
        if self.pending_requests.contains(&picture_id) {
            return false; // Already pending
        }
        self.pending_requests.insert(picture_id);
        true
    }

    /// Stores received picture data.
    pub fn store_picture(&mut self, picture_id: u16, name: String, data: Vec<u8>) {
        if picture_id == 0 {
            return; // Invalid ID
        }
        self.pending_requests.remove(&picture_id);
        self.names.insert(picture_id, name);
        self.pictures.insert(picture_id, data);
        log::debug!(
            "[PictureCache] Stored picture {} (ID {})",
            self.names.get(&picture_id).unwrap_or(&"".to_string()),
            picture_id
        );
    }

    /// Registers picture metadata (without data).
    pub fn register_metadata(&mut self, picture_id: u16, name: String) {
        if picture_id == 0 {
            return;
        }
        self.names.insert(picture_id, name);
    }

    /// Clears all cached pictures.
    pub fn clear(&mut self) {
        self.pictures.clear();
        self.names.clear();
        self.pending_requests.clear();
    }

    /// Returns all cached picture IDs.
    pub fn cached_ids(&self) -> Vec<u16> {
        self.pictures.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_picture_cache() {
        let mut cache = PictureCache::new();

        assert!(!cache.has_picture(1));
        assert!(cache.request_if_needed(1)); // Should request
        assert!(!cache.request_if_needed(1)); // Already pending
        assert!(cache.is_pending(1));

        cache.store_picture(1, "test.png".to_string(), vec![1, 2, 3, 4]);
        assert!(cache.has_picture(1));
        assert!(!cache.is_pending(1));
        assert!(!cache.request_if_needed(1)); // Already cached
        assert_eq!(cache.get_picture(1), Some(&[1, 2, 3, 4][..]));
        assert_eq!(cache.get_name(1), Some("test.png"));
    }

    #[test]
    fn test_picture_cache_zero_id() {
        let mut cache = PictureCache::new();

        // ID 0 is "no picture"
        assert!(!cache.has_picture(0));
        assert!(!cache.request_if_needed(0));
        assert!(!cache.is_pending(0));
        assert_eq!(cache.get_picture(0), None);
    }

    #[test]
    fn test_reference_counting() {
        let mut mgr = PictureManager::new(std::path::PathBuf::new(), 100);

        assert_eq!(mgr.reference_count(42), 0);
        mgr.add_reference(42);
        mgr.add_reference(42);
        assert_eq!(mgr.reference_count(42), 2);

        mgr.remove_reference(42);
        assert_eq!(mgr.reference_count(42), 1);
        mgr.remove_reference(42);
        assert_eq!(mgr.reference_count(42), 0);

        // Below-zero saturates; does not underflow.
        mgr.remove_reference(42);
        assert_eq!(mgr.reference_count(42), 0);

        // picture_id 0 is the "no picture" sentinel — refs are no-ops.
        mgr.add_reference(0);
        assert_eq!(mgr.reference_count(0), 0);
    }

    #[test]
    fn test_force_remove_picture_bypasses_refcount() {
        // Use a temp dir so we actually get a PNG file to delete.
        let dir = std::env::temp_dir().join(format!("vw_picstore_force_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut mgr = PictureManager::new(dir.clone(), 10);
        mgr.metadata.pictures.insert(11, "referenced".into());
        std::fs::write(dir.join("picture_00011.png"), b"\x89PNG\r\n\x1a\n").unwrap();
        mgr.add_reference(11);
        mgr.add_reference(11);
        assert_eq!(mgr.reference_count(11), 2);

        mgr.force_remove_picture(11).expect("force-remove");
        assert_eq!(mgr.reference_count(11), 0, "refs cleared on force remove");
        assert!(!mgr.metadata.pictures.contains_key(&11));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_remove_picture_allows_after_clear_references() {
        let dir = std::env::temp_dir().join(format!("vw_picstore_clear_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut mgr = PictureManager::new(dir.clone(), 10);
        mgr.metadata.pictures.insert(5, "x".into());
        std::fs::write(dir.join("picture_00005.png"), b"\x89PNG\r\n\x1a\n").unwrap();
        mgr.add_reference(5);

        assert!(mgr.remove_picture(5).is_err(), "refuses while ref > 0");
        mgr.clear_references();
        mgr.remove_picture(5).expect("removes after clear");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_remove_picture_refuses_when_referenced() {
        let mut mgr = PictureManager::new(std::path::PathBuf::new(), 100);
        // Pre-seed metadata as if the picture existed on disk so we hit the
        // refcount guard before any fs I/O would be attempted.
        mgr.metadata.pictures.insert(7, "frame".into());
        mgr.add_reference(7);

        let err = mgr.remove_picture(7).unwrap_err();
        assert!(err.contains("referenced"), "got: {}", err);
        // Metadata is untouched because deletion was refused.
        assert!(mgr.metadata.pictures.contains_key(&7));
    }

    #[test]
    fn test_validate_png() {
        let manager = PictureManager::new(std::path::PathBuf::new(), 100);

        // Valid PNG (full encode so the IHDR / IDAT / IEND chunks are real).
        let mut png_bytes = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut png_bytes, 2, 2);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&[0u8; 2 * 2 * 4]).unwrap();
        }
        assert!(manager.validate_png(&png_bytes).is_ok());

        // Magic-bytes-only (no IHDR) now rejected by full decode.
        let magic_only: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert!(manager.validate_png(&magic_only[..]).is_err());

        // Invalid PNG
        let invalid_png = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert!(manager.validate_png(&invalid_png[..]).is_err());

        // Too short
        let short_png = [0x89, 0x50, 0x4E];
        assert!(manager.validate_png(&short_png[..]).is_err());
    }

    /// Helper: build a minimal valid PNG with a given pixel pattern so tests
    /// can produce identical-bytes and differing-bytes PNGs on demand.
    fn make_png(w: u32, h: u32, fill: u8) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc.write_header().unwrap();
            let pixels: Vec<u8> = (0..(w * h * 4)).map(|_| fill).collect();
            writer.write_image_data(&pixels).unwrap();
        }
        out
    }

    #[test]
    fn test_add_picture_dedups_identical_content() {
        let dir = std::env::temp_dir().join(format!("vw_picstore_dedup_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut mgr = PictureManager::new(dir.clone(), 10);

        let png = make_png(4, 4, 0xaa);
        let id1 = mgr.add_picture("first", &png).unwrap();
        let id2 = mgr.add_picture("second", &png).unwrap();
        assert_eq!(id1, id2, "identical content must return the same ID");
        // Only one metadata entry + one file on disk.
        assert_eq!(mgr.metadata.pictures.len(), 1);
        assert!(dir.join(format!("picture_{:05}.png", id1)).exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_add_picture_different_content_gets_different_ids() {
        let dir = std::env::temp_dir().join(format!("vw_picstore_diff_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut mgr = PictureManager::new(dir.clone(), 10);

        let a = make_png(4, 4, 0x00);
        let b = make_png(4, 4, 0xff);
        let id_a = mgr.add_picture("a", &a).unwrap();
        let id_b = mgr.add_picture("b", &b).unwrap();
        assert_ne!(id_a, id_b);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_content_hash_index_survives_restart() {
        let dir = std::env::temp_dir().join(format!("vw_picstore_restart_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut mgr = PictureManager::new(dir.clone(), 10);
        mgr.init().unwrap();
        let png = make_png(2, 2, 0x7f);
        let id = mgr.add_picture("persistent", &png).unwrap();

        // Simulate a restart: drop and reconstruct + init. The content-hash
        // index must be rebuilt from disk so a subsequent dup-upload still
        // short-circuits.
        drop(mgr);
        let mut fresh = PictureManager::new(dir.clone(), 10);
        fresh.init().unwrap();
        let id_again = fresh.add_picture("different-name", &png).unwrap();
        assert_eq!(
            id, id_again,
            "content-hash index must persist across restart"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_add_picture_rejects_bad_names() {
        let mut mgr = PictureManager::new(std::path::PathBuf::new(), 10);
        let placeholder = vec![0u8; 8]; // validate_png will fail, but name check runs first

        let err = mgr
            .add_picture(&"a".repeat(MAX_PICTURE_NAME_LEN + 1), &placeholder)
            .unwrap_err();
        assert!(err.contains("too long"));

        let err = mgr.add_picture("../etc/passwd", &placeholder).unwrap_err();
        assert!(err.contains("invalid characters"));

        let err = mgr.add_picture("foo/bar", &placeholder).unwrap_err();
        assert!(err.contains("invalid characters"));
    }
}

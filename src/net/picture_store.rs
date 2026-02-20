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
        // Check size limit
        if png_data.len() > self.max_size {
            return Err(format!(
                "Picture too large: {} bytes (max {})",
                png_data.len(),
                self.max_size
            ));
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

        // Update metadata
        self.metadata.pictures.insert(picture_id, name.to_string());
        self.metadata.next_id += 1;
        self.save_metadata()
            .map_err(|e| format!("Failed to save metadata: {}", e))?;

        println!(
            "[PictureManager] Added picture '{}' with ID {}",
            name, picture_id
        );

        Ok(picture_id)
    }

    /// Validates PNG data.
    fn validate_png(&self, png_data: &[u8]) -> Result<(), String> {
        // Check PNG magic bytes
        if png_data.len() < 8 {
            return Err("Invalid PNG: too short".to_string());
        }

        // PNG magic bytes: 89 50 4E 47 0D 0A 1A 0A
        let magic: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        if png_data[..8] != magic {
            return Err("Invalid PNG: wrong magic bytes".to_string());
        }

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

    /// Removes a picture by ID.
    pub fn remove_picture(&mut self, picture_id: u16) -> Result<(), String> {
        if picture_id == 0 {
            return Err("Invalid picture ID 0".to_string());
        }

        if !self.metadata.pictures.contains_key(&picture_id) {
            return Err("Picture not found".to_string());
        }

        // Delete PNG file
        let png_path = self
            .base_path
            .join(format!("picture_{:05}.png", picture_id));
        fs::remove_file(&png_path).map_err(|e| format!("Failed to delete PNG: {}", e))?;

        // Update metadata
        self.metadata.pictures.remove(&picture_id);
        self.save_metadata()
            .map_err(|e| format!("Failed to save metadata: {}", e))?;

        println!("[PictureManager] Removed picture ID {}", picture_id);

        Ok(())
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
        println!(
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
    fn test_validate_png() {
        let manager = PictureManager::new(std::path::PathBuf::new(), 100);

        // Valid PNG magic bytes
        let valid_png: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert!(manager.validate_png(&valid_png[..]).is_ok());

        // Invalid PNG
        let invalid_png = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert!(manager.validate_png(&invalid_png[..]).is_err());

        // Too short
        let short_png = [0x89, 0x50, 0x4E];
        assert!(manager.validate_png(&short_png[..]).is_err());
    }
}

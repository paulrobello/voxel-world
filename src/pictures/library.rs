//! Picture library for storing and managing user-created pictures.
//!
//! Pictures are stored globally in `~/.voxel-world/pictures.bin` and shared
//! across all worlds. The library uses a simple binary format with zstd compression.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Maximum picture dimension (128×128 pixels).
/// Each frame displays 128×128 pixels of the picture.
/// For multi-frame clusters, use larger pictures and the shader will
/// sample the appropriate region.
pub const MAX_PICTURE_SIZE: u16 = 128;

/// Maximum number of pictures that can be loaded on GPU at once.
pub const MAX_GPU_PICTURES: usize = 64;

/// A single picture stored in the library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Picture {
    /// Unique identifier for this picture.
    pub id: u32,
    /// Human-readable name for the picture.
    pub name: String,
    /// Width in pixels (1-384).
    pub width: u16,
    /// Height in pixels (1-384).
    pub height: u16,
    /// RGBA pixel data (width × height × 4 bytes).
    pub pixels: Vec<u8>,
    /// Unix timestamp when picture was created.
    pub created: u64,
    /// Unix timestamp when picture was last modified.
    pub modified: u64,
}

impl Picture {
    /// Creates a new blank picture with the specified dimensions.
    ///
    /// The picture is initialized with transparent (RGBA 0,0,0,0) pixels.
    pub fn new(name: impl Into<String>, width: u16, height: u16) -> Self {
        let width = width.clamp(1, MAX_PICTURE_SIZE);
        let height = height.clamp(1, MAX_PICTURE_SIZE);
        let pixel_count = width as usize * height as usize;
        let now = current_timestamp();

        Self {
            id: 0, // Will be assigned by library
            name: name.into(),
            width,
            height,
            pixels: vec![0; pixel_count * 4],
            created: now,
            modified: now,
        }
    }

    /// Creates a new picture filled with a solid color.
    pub fn filled(name: impl Into<String>, width: u16, height: u16, rgba: [u8; 4]) -> Self {
        let mut picture = Self::new(name, width, height);
        for chunk in picture.pixels.chunks_exact_mut(4) {
            chunk.copy_from_slice(&rgba);
        }
        picture
    }

    /// Returns the number of pixels in this picture.
    #[inline]
    pub fn pixel_count(&self) -> usize {
        self.width as usize * self.height as usize
    }

    /// Gets the RGBA color at the specified pixel coordinates.
    ///
    /// Returns `None` if coordinates are out of bounds.
    #[inline]
    pub fn get_pixel(&self, x: u16, y: u16) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = (y as usize * self.width as usize + x as usize) * 4;
        Some([
            self.pixels[idx],
            self.pixels[idx + 1],
            self.pixels[idx + 2],
            self.pixels[idx + 3],
        ])
    }

    /// Sets the RGBA color at the specified pixel coordinates.
    ///
    /// Does nothing if coordinates are out of bounds.
    #[inline]
    pub fn set_pixel(&mut self, x: u16, y: u16, rgba: [u8; 4]) {
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = (y as usize * self.width as usize + x as usize) * 4;
        self.pixels[idx..idx + 4].copy_from_slice(&rgba);
        self.modified = current_timestamp();
    }

    /// Fills a rectangular region with a color.
    pub fn fill_rect(&mut self, x: u16, y: u16, w: u16, h: u16, rgba: [u8; 4]) {
        let x1 = x.min(self.width);
        let y1 = y.min(self.height);
        let x2 = (x + w).min(self.width);
        let y2 = (y + h).min(self.height);

        for py in y1..y2 {
            for px in x1..x2 {
                let idx = (py as usize * self.width as usize + px as usize) * 4;
                self.pixels[idx..idx + 4].copy_from_slice(&rgba);
            }
        }
        self.modified = current_timestamp();
    }

    /// Clears the entire picture to transparent.
    pub fn clear(&mut self) {
        self.pixels.fill(0);
        self.modified = current_timestamp();
    }

    /// Returns a copy of the pixel data suitable for GPU upload.
    pub fn gpu_data(&self) -> &[u8] {
        &self.pixels
    }

    /// Draws a line using Bresenham's algorithm.
    pub fn draw_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, rgba: [u8; 4]) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        let mut x = x0;
        let mut y = y0;

        loop {
            if x >= 0 && x < self.width as i32 && y >= 0 && y < self.height as i32 {
                self.set_pixel(x as u16, y as u16, rgba);
            }

            if x == x1 && y == y1 {
                break;
            }

            let e2 = 2 * err;
            if e2 >= dy {
                if x == x1 {
                    break;
                }
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                if y == y1 {
                    break;
                }
                err += dx;
                y += sy;
            }
        }
        self.modified = current_timestamp();
    }

    /// Draws a rectangle outline.
    pub fn draw_rect(&mut self, x: i32, y: i32, w: i32, h: i32, rgba: [u8; 4]) {
        self.draw_line(x, y, x + w - 1, y, rgba);
        self.draw_line(x + w - 1, y, x + w - 1, y + h - 1, rgba);
        self.draw_line(x + w - 1, y + h - 1, x, y + h - 1, rgba);
        self.draw_line(x, y + h - 1, x, y, rgba);
    }

    /// Draws a circle outline using midpoint algorithm.
    pub fn draw_circle(&mut self, cx: i32, cy: i32, radius: i32, rgba: [u8; 4]) {
        let mut x = radius;
        let mut y = 0;
        let mut err = 0;

        while x >= y {
            self.set_pixel_safe(cx + x, cy + y, rgba);
            self.set_pixel_safe(cx + y, cy + x, rgba);
            self.set_pixel_safe(cx - y, cy + x, rgba);
            self.set_pixel_safe(cx - x, cy + y, rgba);
            self.set_pixel_safe(cx - x, cy - y, rgba);
            self.set_pixel_safe(cx - y, cy - x, rgba);
            self.set_pixel_safe(cx + y, cy - x, rgba);
            self.set_pixel_safe(cx + x, cy - y, rgba);

            y += 1;
            if err <= 0 {
                err += 2 * y + 1;
            }
            if err > 0 {
                x -= 1;
                err -= 2 * x + 1;
            }
        }
        self.modified = current_timestamp();
    }

    /// Draws a filled circle.
    pub fn fill_circle(&mut self, cx: i32, cy: i32, radius: i32, rgba: [u8; 4]) {
        for y in -radius..=radius {
            for x in -radius..=radius {
                if x * x + y * y <= radius * radius {
                    self.set_pixel_safe(cx + x, cy + y, rgba);
                }
            }
        }
        self.modified = current_timestamp();
    }

    /// Flood fills connected pixels of the same color starting from (x, y).
    pub fn flood_fill(&mut self, start_x: u16, start_y: u16, new_color: [u8; 4]) {
        let Some(target_color) = self.get_pixel(start_x, start_y) else {
            return;
        };

        // Don't fill if already the target color
        if target_color == new_color {
            return;
        }

        let mut stack = vec![(start_x, start_y)];
        let mut visited = std::collections::HashSet::new();

        while let Some((x, y)) = stack.pop() {
            if visited.contains(&(x, y)) {
                continue;
            }

            let Some(pixel_color) = self.get_pixel(x, y) else {
                continue;
            };

            if pixel_color != target_color {
                continue;
            }

            visited.insert((x, y));
            self.set_pixel(x, y, new_color);

            // Add neighbors
            if x > 0 {
                stack.push((x - 1, y));
            }
            if x < self.width - 1 {
                stack.push((x + 1, y));
            }
            if y > 0 {
                stack.push((x, y - 1));
            }
            if y < self.height - 1 {
                stack.push((x, y + 1));
            }
        }
        self.modified = current_timestamp();
    }

    /// Sets a pixel with i32 coordinates (clips to bounds).
    fn set_pixel_safe(&mut self, x: i32, y: i32, rgba: [u8; 4]) {
        if x >= 0 && x < self.width as i32 && y >= 0 && y < self.height as i32 {
            self.set_pixel(x as u16, y as u16, rgba);
        }
    }

    /// Creates a thumbnail of this picture for UI display.
    ///
    /// Returns a downscaled RGBA buffer at the specified max dimension.
    pub fn thumbnail(&self, max_size: u16) -> (u16, u16, Vec<u8>) {
        let scale = if self.width >= self.height {
            max_size as f32 / self.width as f32
        } else {
            max_size as f32 / self.height as f32
        };

        let new_width = ((self.width as f32 * scale).round() as u16).max(1);
        let new_height = ((self.height as f32 * scale).round() as u16).max(1);

        let mut result = vec![0u8; new_width as usize * new_height as usize * 4];

        for y in 0..new_height {
            for x in 0..new_width {
                let src_x = ((x as f32 / scale) as u16).min(self.width - 1);
                let src_y = ((y as f32 / scale) as u16).min(self.height - 1);

                if let Some(pixel) = self.get_pixel(src_x, src_y) {
                    let dst_idx = (y as usize * new_width as usize + x as usize) * 4;
                    result[dst_idx..dst_idx + 4].copy_from_slice(&pixel);
                }
            }
        }

        (new_width, new_height, result)
    }
}

/// Library of pictures stored globally.
///
/// Pictures are stored in `~/.voxel-world/pictures.bin` and persist across
/// game sessions and worlds.
#[derive(Debug, Serialize, Deserialize)]
pub struct PictureLibrary {
    /// All pictures indexed by ID.
    pictures: HashMap<u32, Picture>,
    /// Next available picture ID.
    next_id: u32,
}

impl Default for PictureLibrary {
    fn default() -> Self {
        Self::new()
    }
}

impl PictureLibrary {
    /// Creates a new empty picture library.
    pub fn new() -> Self {
        Self {
            pictures: HashMap::new(),
            next_id: 1,
        }
    }

    /// Returns the path to the library file.
    pub fn library_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".voxel-world").join("pictures.bin")
    }

    /// Loads the library from disk, or creates a new one if it doesn't exist.
    pub fn load() -> Self {
        let path = Self::library_path();
        if !path.exists() {
            return Self::new();
        }

        match Self::load_from_file(&path) {
            Ok(library) => library,
            Err(e) => {
                eprintln!("[PictureLibrary] Failed to load: {e}");
                Self::new()
            }
        }
    }

    /// Loads the library from a specific file.
    fn load_from_file(path: &PathBuf) -> Result<Self, String> {
        let mut file = File::open(path).map_err(|e| format!("Failed to open: {e}"))?;

        let mut compressed = Vec::new();
        file.read_to_end(&mut compressed)
            .map_err(|e| format!("Failed to read: {e}"))?;

        let decompressed =
            zstd::decode_all(&compressed[..]).map_err(|e| format!("Decompression failed: {e}"))?;

        let library: Self = bincode::deserialize(&decompressed)
            .map_err(|e| format!("Deserialization failed: {e}"))?;

        Ok(library)
    }

    /// Saves the library to disk.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::library_path();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {e}"))?;
        }

        let binary = bincode::serialize(self).map_err(|e| format!("Serialization failed: {e}"))?;

        let compressed =
            zstd::encode_all(&binary[..], 3).map_err(|e| format!("Compression failed: {e}"))?;

        let mut file = File::create(&path).map_err(|e| format!("Failed to create file: {e}"))?;

        file.write_all(&compressed)
            .map_err(|e| format!("Failed to write: {e}"))?;

        Ok(())
    }

    /// Adds a picture to the library.
    ///
    /// Returns the assigned picture ID.
    pub fn add(&mut self, mut picture: Picture) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        picture.id = id;
        self.pictures.insert(id, picture);
        id
    }

    /// Removes a picture from the library.
    ///
    /// Returns the removed picture, or `None` if not found.
    pub fn remove(&mut self, id: u32) -> Option<Picture> {
        self.pictures.remove(&id)
    }

    /// Gets a picture by ID.
    pub fn get(&self, id: u32) -> Option<&Picture> {
        self.pictures.get(&id)
    }

    /// Gets a mutable reference to a picture by ID.
    pub fn get_mut(&mut self, id: u32) -> Option<&mut Picture> {
        self.pictures.get_mut(&id)
    }

    /// Returns an iterator over all pictures.
    pub fn iter(&self) -> impl Iterator<Item = &Picture> {
        self.pictures.values()
    }

    /// Returns an iterator over all picture IDs.
    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.pictures.keys().copied()
    }

    /// Returns the number of pictures in the library.
    pub fn len(&self) -> usize {
        self.pictures.len()
    }

    /// Returns true if the library is empty.
    pub fn is_empty(&self) -> bool {
        self.pictures.is_empty()
    }

    /// Finds pictures by name (case-insensitive substring match).
    pub fn find_by_name(&self, query: &str) -> Vec<&Picture> {
        let query_lower = query.to_lowercase();
        self.pictures
            .values()
            .filter(|p| p.name.to_lowercase().contains(&query_lower))
            .collect()
    }

    /// Returns pictures sorted by modification time (most recent first).
    pub fn recent(&self) -> Vec<&Picture> {
        let mut pics: Vec<_> = self.pictures.values().collect();
        pics.sort_by(|a, b| b.modified.cmp(&a.modified));
        pics
    }

    /// Creates a new picture with the given dimensions and adds it to the library.
    ///
    /// Returns the picture ID.
    pub fn create(&mut self, name: impl Into<String>, width: u16, height: u16) -> u32 {
        let picture = Picture::new(name, width, height);
        self.add(picture)
    }

    /// Duplicates an existing picture.
    ///
    /// Returns the new picture ID, or `None` if the source doesn't exist.
    pub fn duplicate(&mut self, id: u32) -> Option<u32> {
        let source = self.pictures.get(&id)?;
        let mut copy = source.clone();
        copy.name = format!("{} (copy)", source.name);
        copy.created = current_timestamp();
        copy.modified = copy.created;
        Some(self.add(copy))
    }

    /// Imports an image from raw RGBA data.
    ///
    /// Resizes to fit within MAX_PICTURE_SIZE if necessary.
    pub fn import_rgba(
        &mut self,
        name: impl Into<String>,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> Option<u32> {
        if data.len() != (width * height * 4) as usize {
            return None;
        }

        // Check if resize is needed
        let (final_width, final_height, final_data) = if width > MAX_PICTURE_SIZE as u32
            || height > MAX_PICTURE_SIZE as u32
        {
            // Need to downscale
            let scale = (MAX_PICTURE_SIZE as f32 / width.max(height) as f32).min(1.0);
            let new_w = ((width as f32 * scale).round() as u16).max(1);
            let new_h = ((height as f32 * scale).round() as u16).max(1);

            let mut resized = vec![0u8; new_w as usize * new_h as usize * 4];

            // Simple nearest-neighbor downscale
            for y in 0..new_h {
                for x in 0..new_w {
                    let src_x = (x as f32 / scale) as u32;
                    let src_y = (y as f32 / scale) as u32;
                    let src_idx = (src_y * width + src_x) as usize * 4;
                    let dst_idx = (y as usize * new_w as usize + x as usize) * 4;

                    if src_idx + 4 <= data.len() {
                        resized[dst_idx..dst_idx + 4].copy_from_slice(&data[src_idx..src_idx + 4]);
                    }
                }
            }

            (new_w, new_h, resized)
        } else {
            (width as u16, height as u16, data.to_vec())
        };

        let mut picture = Picture::new(name, final_width, final_height);
        picture.pixels = final_data;

        Some(self.add(picture))
    }
}

/// Returns the current Unix timestamp.
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_picture_new() {
        let pic = Picture::new("test", 64, 64);
        assert_eq!(pic.width, 64);
        assert_eq!(pic.height, 64);
        assert_eq!(pic.pixels.len(), 64 * 64 * 4);
    }

    #[test]
    fn test_picture_clamp_size() {
        let pic = Picture::new("test", 1000, 500);
        assert_eq!(pic.width, 128);
        assert_eq!(pic.height, 128);
    }

    #[test]
    fn test_picture_get_set_pixel() {
        let mut pic = Picture::new("test", 16, 16);
        pic.set_pixel(5, 5, [255, 0, 0, 255]);
        assert_eq!(pic.get_pixel(5, 5), Some([255, 0, 0, 255]));
        assert_eq!(pic.get_pixel(0, 0), Some([0, 0, 0, 0]));
    }

    #[test]
    fn test_picture_out_of_bounds() {
        let mut pic = Picture::new("test", 16, 16);
        pic.set_pixel(100, 100, [255, 0, 0, 255]); // Should not panic
        assert_eq!(pic.get_pixel(100, 100), None);
    }

    #[test]
    fn test_library_add_get() {
        let mut lib = PictureLibrary::new();
        let pic = Picture::new("test", 32, 32);
        let id = lib.add(pic);

        assert!(lib.get(id).is_some());
        assert_eq!(lib.get(id).unwrap().name, "test");
    }

    #[test]
    fn test_library_remove() {
        let mut lib = PictureLibrary::new();
        let pic = Picture::new("test", 32, 32);
        let id = lib.add(pic);

        let removed = lib.remove(id);
        assert!(removed.is_some());
        assert!(lib.get(id).is_none());
    }

    #[test]
    fn test_library_duplicate() {
        let mut lib = PictureLibrary::new();
        let pic = Picture::filled("original", 8, 8, [255, 0, 0, 255]);
        let id = lib.add(pic);

        let dup_id = lib.duplicate(id).unwrap();
        assert_ne!(id, dup_id);
        assert!(lib.get(dup_id).unwrap().name.contains("copy"));
    }

    #[test]
    fn test_flood_fill() {
        let mut pic = Picture::filled("test", 8, 8, [255, 255, 255, 255]);
        pic.flood_fill(0, 0, [255, 0, 0, 255]);
        assert_eq!(pic.get_pixel(0, 0), Some([255, 0, 0, 255]));
        assert_eq!(pic.get_pixel(7, 7), Some([255, 0, 0, 255]));
    }
}

//! Custom texture generation and storage.
#![allow(dead_code)] // Many helpers will be used once texture generator is fully integrated

use super::patterns::TexturePattern;
use crate::user_prefs::get_data_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Maximum number of custom texture slots.
pub const MAX_CUSTOM_TEXTURES: usize = 16;

/// Texture size in pixels.
pub const TEXTURE_SIZE: u32 = 64;

/// Flag bit for custom texture indices (128+).
/// When this bit is set in a texture index, the shader samples from the custom atlas.
pub const CUSTOM_TEXTURE_FLAG: u8 = 128;

/// Converts a custom texture slot ID (0-15) to a paint texture index (128-143).
pub fn slot_to_texture_index(slot: u8) -> u8 {
    CUSTOM_TEXTURE_FLAG | (slot & 0x0F)
}

/// Checks if a texture index refers to a custom texture.
pub fn is_custom_texture(texture_idx: u8) -> bool {
    texture_idx >= CUSTOM_TEXTURE_FLAG
}

/// Extracts the custom slot ID from a texture index (128-143 -> 0-15).
pub fn texture_index_to_slot(texture_idx: u8) -> u8 {
    texture_idx & 0x0F
}

/// Color for texture generation (RGB).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TextureColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Default for TextureColor {
    fn default() -> Self {
        Self::WHITE
    }
}

impl TextureColor {
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
    };
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0 };
    pub const RED: Self = Self { r: 255, g: 0, b: 0 };
    pub const GREEN: Self = Self { r: 0, g: 255, b: 0 };
    pub const BLUE: Self = Self { r: 0, g: 0, b: 255 };
    pub const YELLOW: Self = Self {
        r: 255,
        g: 255,
        b: 0,
    };
    pub const CYAN: Self = Self {
        r: 0,
        g: 255,
        b: 255,
    };
    pub const MAGENTA: Self = Self {
        r: 255,
        g: 0,
        b: 255,
    };
    pub const GRAY: Self = Self {
        r: 128,
        g: 128,
        b: 128,
    };
    pub const STONE: Self = Self {
        r: 136,
        g: 136,
        b: 136,
    };
    pub const DIRT: Self = Self {
        r: 134,
        g: 96,
        b: 67,
    };
    pub const WOOD: Self = Self {
        r: 156,
        g: 127,
        b: 90,
    };

    /// Creates a new color from RGB values.
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Creates a color from a hex value (0xRRGGBB).
    pub const fn from_hex(hex: u32) -> Self {
        Self {
            r: ((hex >> 16) & 0xFF) as u8,
            g: ((hex >> 8) & 0xFF) as u8,
            b: (hex & 0xFF) as u8,
        }
    }

    /// Converts to [r, g, b] array.
    pub const fn to_array(self) -> [u8; 3] {
        [self.r, self.g, self.b]
    }

    /// Converts to [r, g, b, a] array with full opacity.
    pub const fn to_rgba(self) -> [u8; 4] {
        [self.r, self.g, self.b, 255]
    }

    /// Linearly interpolates between two colors.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let inv_t = 1.0 - t;
        Self {
            r: (self.r as f32 * inv_t + other.r as f32 * t) as u8,
            g: (self.g as f32 * inv_t + other.g as f32 * t) as u8,
            b: (self.b as f32 * inv_t + other.b as f32 * t) as u8,
        }
    }
}

/// A custom texture (either procedurally generated or raw pixel data).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomTexture {
    /// Slot ID (0-15).
    pub id: u8,
    /// Display name for the texture.
    pub name: String,
    /// Pattern type (only used if not raw).
    pub pattern: TexturePattern,
    /// Primary color (only used if not raw).
    pub color1: TextureColor,
    /// Secondary color (only used if not raw).
    pub color2: TextureColor,
    /// Pattern scale (0.5 = larger, 2.0 = smaller).
    pub scale: f32,
    /// Rotation in 90° increments (0-3).
    pub rotation: u8,
    /// Random seed for noise patterns.
    pub seed: u32,
    /// Whether this is a raw pixel texture (painted/imported) vs procedural.
    /// Raw textures save/load pixel data to/from PNG files.
    #[serde(default)]
    pub is_raw: bool,
    /// Cached pixel data (64×64 RGBA, not serialized directly).
    #[serde(skip)]
    pub pixels: Vec<u8>,
}

impl Default for CustomTexture {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            pattern: TexturePattern::Solid,
            color1: TextureColor::WHITE,
            color2: TextureColor::BLACK,
            scale: 1.0,
            rotation: 0,
            seed: 0,
            is_raw: false,
            pixels: Vec::new(),
        }
    }
}

impl CustomTexture {
    /// Creates a new custom texture with the given parameters.
    pub fn new(
        name: impl Into<String>,
        pattern: TexturePattern,
        color1: TextureColor,
        color2: TextureColor,
    ) -> Self {
        let mut tex = Self {
            id: 0,
            name: name.into(),
            pattern,
            color1,
            color2,
            scale: 1.0,
            rotation: 0,
            seed: 0,
            is_raw: false,
            pixels: Vec::new(),
        };
        tex.regenerate();
        tex
    }

    /// Creates a new raw texture from pixel data (for painted/imported textures).
    pub fn from_pixels(name: impl Into<String>, pixels: Vec<u8>) -> Self {
        Self {
            id: 0,
            name: name.into(),
            pattern: TexturePattern::Solid, // Unused for raw textures
            color1: TextureColor::WHITE,
            color2: TextureColor::BLACK,
            scale: 1.0,
            rotation: 0,
            seed: 0,
            is_raw: true,
            pixels,
        }
    }

    /// Regenerates the pixel data from current settings.
    pub fn regenerate(&mut self) {
        self.pixels = vec![0u8; (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize];

        for y in 0..TEXTURE_SIZE {
            for x in 0..TEXTURE_SIZE {
                // Apply rotation
                let (rx, ry) = self.rotate_coords(x, y);

                // Sample pattern
                let t = self.pattern.sample(rx, ry, self.scale, self.seed);

                // Blend colors
                let color = self.color1.lerp(self.color2, t);

                // Write RGBA
                let idx = ((y * TEXTURE_SIZE + x) * 4) as usize;
                self.pixels[idx] = color.r;
                self.pixels[idx + 1] = color.g;
                self.pixels[idx + 2] = color.b;
                self.pixels[idx + 3] = 255; // Full opacity
            }
        }
    }

    /// Rotates coordinates based on rotation setting.
    fn rotate_coords(&self, x: u32, y: u32) -> (u32, u32) {
        let max = TEXTURE_SIZE - 1;
        match self.rotation % 4 {
            0 => (x, y),
            1 => (max - y, x),       // 90° CW
            2 => (max - x, max - y), // 180°
            3 => (y, max - x),       // 270° CW
            _ => unreachable!(),
        }
    }

    /// Returns the pixel data as a slice.
    pub fn pixel_data(&self) -> &[u8] {
        &self.pixels
    }

    /// Returns true if pixel data needs regeneration (procedural) or loading (raw).
    pub fn needs_regeneration(&self) -> bool {
        self.pixels.is_empty()
    }

    /// Gets the path for raw pixel data file.
    fn raw_pixel_path(data_dir: &std::path::Path, slot: u8) -> PathBuf {
        data_dir.join(format!("custom_texture_{}.png", slot))
    }

    /// Saves raw pixel data to a PNG file.
    pub fn save_raw_pixels(&self, data_dir: &std::path::Path) -> Result<(), String> {
        if !self.is_raw || self.pixels.is_empty() {
            return Ok(());
        }

        let path = Self::raw_pixel_path(data_dir, self.id);
        let img = image::RgbaImage::from_raw(TEXTURE_SIZE, TEXTURE_SIZE, self.pixels.clone())
            .ok_or_else(|| "Failed to create image from pixels".to_string())?;

        img.save(&path)
            .map_err(|e| format!("Failed to save raw texture: {}", e))?;

        Ok(())
    }

    /// Loads raw pixel data from a PNG file.
    pub fn load_raw_pixels(&mut self, data_dir: &std::path::Path) -> Result<(), String> {
        if !self.is_raw {
            return Ok(());
        }

        let path = Self::raw_pixel_path(data_dir, self.id);
        if !path.exists() {
            // Create empty pixels if file doesn't exist
            self.pixels = vec![255u8; (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize];
            return Ok(());
        }

        let img = image::open(&path)
            .map_err(|e| format!("Failed to load raw texture: {}", e))?
            .to_rgba8();

        if img.width() != TEXTURE_SIZE || img.height() != TEXTURE_SIZE {
            return Err(format!(
                "Raw texture wrong size: {}x{}, expected {}x{}",
                img.width(),
                img.height(),
                TEXTURE_SIZE,
                TEXTURE_SIZE
            ));
        }

        self.pixels = img.into_raw();
        Ok(())
    }

    /// Deletes the raw pixel data file if it exists.
    pub fn delete_raw_pixels(data_dir: &std::path::Path, slot: u8) {
        let path = Self::raw_pixel_path(data_dir, slot);
        let _ = fs::remove_file(path);
    }
}

/// Texture generation utility functions.
pub struct TextureGenerator;

impl TextureGenerator {
    /// Creates a solid color texture.
    pub fn solid(name: &str, color: TextureColor) -> CustomTexture {
        CustomTexture::new(name, TexturePattern::Solid, color, color)
    }

    /// Creates a horizontal stripe texture.
    pub fn h_stripes(
        name: &str,
        color1: TextureColor,
        color2: TextureColor,
        scale: f32,
    ) -> CustomTexture {
        let mut tex = CustomTexture::new(name, TexturePattern::HorizontalStripes, color1, color2);
        tex.scale = scale;
        tex.regenerate();
        tex
    }

    /// Creates a vertical stripe texture.
    pub fn v_stripes(
        name: &str,
        color1: TextureColor,
        color2: TextureColor,
        scale: f32,
    ) -> CustomTexture {
        let mut tex = CustomTexture::new(name, TexturePattern::VerticalStripes, color1, color2);
        tex.scale = scale;
        tex.regenerate();
        tex
    }

    /// Creates a checkerboard texture.
    pub fn checkerboard(
        name: &str,
        color1: TextureColor,
        color2: TextureColor,
        scale: f32,
    ) -> CustomTexture {
        let mut tex = CustomTexture::new(name, TexturePattern::Checkerboard, color1, color2);
        tex.scale = scale;
        tex.regenerate();
        tex
    }

    /// Creates a brick pattern texture.
    pub fn brick(
        name: &str,
        brick_color: TextureColor,
        mortar_color: TextureColor,
        scale: f32,
    ) -> CustomTexture {
        let mut tex = CustomTexture::new(name, TexturePattern::Brick, brick_color, mortar_color);
        tex.scale = scale;
        tex.regenerate();
        tex
    }

    /// Creates a noise texture.
    pub fn noise(
        name: &str,
        color1: TextureColor,
        color2: TextureColor,
        scale: f32,
        seed: u32,
    ) -> CustomTexture {
        let mut tex = CustomTexture::new(name, TexturePattern::Noise, color1, color2);
        tex.scale = scale;
        tex.seed = seed;
        tex.regenerate();
        tex
    }
}

/// Library for managing custom textures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextureLibrary {
    /// Custom texture slots (up to 16).
    textures: Vec<CustomTexture>,
    /// Auto-increment counter for naming.
    next_id: u32,
}

impl Default for TextureLibrary {
    fn default() -> Self {
        Self::new()
    }
}

impl TextureLibrary {
    /// Creates a new empty texture library.
    pub fn new() -> Self {
        Self {
            textures: Vec::new(),
            next_id: 1,
        }
    }

    /// Returns the storage path for the texture library.
    fn storage_path() -> PathBuf {
        get_data_dir().join("custom_textures.json")
    }

    /// Loads the texture library from disk, creating defaults if not found.
    pub fn load() -> Self {
        let path = Self::storage_path();
        if path.exists() {
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(mut lib) = serde_json::from_str::<Self>(&data) {
                    let data_dir = get_data_dir();
                    // Load or regenerate pixel data since it's not serialized
                    for tex in &mut lib.textures {
                        if tex.is_raw {
                            // Raw textures load from PNG file
                            if let Err(e) = tex.load_raw_pixels(&data_dir) {
                                eprintln!(
                                    "Warning: Failed to load raw texture '{}': {}",
                                    tex.name, e
                                );
                                // Create empty white pixels as fallback
                                tex.pixels =
                                    vec![255u8; (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize];
                            }
                        } else {
                            // Procedural textures regenerate from pattern settings
                            tex.regenerate();
                        }
                    }
                    return lib;
                }
            }
        }
        Self::new()
    }

    /// Saves the texture library to disk.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::storage_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        // Save raw pixel data to PNG files
        let data_dir = get_data_dir();
        for tex in &self.textures {
            if tex.is_raw {
                tex.save_raw_pixels(&data_dir)?;
            }
        }

        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&path, json).map_err(|e| e.to_string())
    }

    /// Returns the number of custom textures.
    pub fn count(&self) -> usize {
        self.textures.len()
    }

    /// Returns true if the library is full.
    pub fn is_full(&self) -> bool {
        self.textures.len() >= MAX_CUSTOM_TEXTURES
    }

    /// Returns the next available slot ID, or None if full.
    pub fn next_slot(&self) -> Option<u8> {
        if self.is_full() {
            return None;
        }
        // Find first unused slot
        (0..MAX_CUSTOM_TEXTURES as u8).find(|&id| !self.textures.iter().any(|t| t.id == id))
    }

    /// Adds a new custom texture. Returns the assigned ID or error if full.
    pub fn add(&mut self, mut texture: CustomTexture) -> Result<u8, String> {
        if self.is_full() {
            return Err("Texture library is full (max 16)".to_string());
        }

        // Assign slot ID
        let slot = self.next_slot().ok_or("No available slots")?;
        texture.id = slot;

        // Generate unique name if empty
        if texture.name.is_empty() {
            texture.name = format!("Custom {}", self.next_id);
            self.next_id += 1;
        }

        // Ensure pixels are generated
        if texture.needs_regeneration() {
            texture.regenerate();
        }

        self.textures.push(texture);
        Ok(slot)
    }

    /// Updates an existing texture by slot ID.
    pub fn update(&mut self, slot: u8, texture: CustomTexture) -> Result<(), String> {
        if let Some(existing) = self.textures.iter_mut().find(|t| t.id == slot) {
            *existing = texture;
            existing.id = slot;
            if existing.needs_regeneration() {
                existing.regenerate();
            }
            Ok(())
        } else {
            Err(format!("No texture in slot {}", slot))
        }
    }

    /// Removes a texture by slot ID.
    pub fn remove(&mut self, slot: u8) -> Result<(), String> {
        if let Some(idx) = self.textures.iter().position(|t| t.id == slot) {
            let tex = &self.textures[idx];
            // Clean up raw pixel file if it exists
            if tex.is_raw {
                CustomTexture::delete_raw_pixels(&get_data_dir(), slot);
            }
            self.textures.remove(idx);
            Ok(())
        } else {
            Err(format!("No texture in slot {}", slot))
        }
    }

    /// Gets a texture by slot ID.
    pub fn get(&self, slot: u8) -> Option<&CustomTexture> {
        self.textures.iter().find(|t| t.id == slot)
    }

    /// Gets a mutable texture by slot ID.
    pub fn get_mut(&mut self, slot: u8) -> Option<&mut CustomTexture> {
        self.textures.iter_mut().find(|t| t.id == slot)
    }

    /// Iterates over all textures.
    pub fn iter(&self) -> impl Iterator<Item = &CustomTexture> {
        self.textures.iter()
    }

    /// Iterates mutably over all textures.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut CustomTexture> {
        self.textures.iter_mut()
    }

    /// Returns combined pixel data for GPU upload (16 textures × 64×64 RGBA).
    /// Empty slots are filled with transparent pixels.
    pub fn combined_pixel_data(&self) -> Vec<u8> {
        let slot_size = (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize;
        let total_size = MAX_CUSTOM_TEXTURES * slot_size;
        let mut data = vec![0u8; total_size];

        for tex in &self.textures {
            let offset = tex.id as usize * slot_size;
            if offset + slot_size <= total_size && tex.pixels.len() == slot_size {
                data[offset..offset + slot_size].copy_from_slice(&tex.pixels);
            }
        }

        data
    }

    /// Returns all texture names for UI display.
    pub fn names(&self) -> Vec<(u8, String)> {
        self.textures
            .iter()
            .map(|t| (t.id, t.name.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_texture_color_lerp() {
        let black = TextureColor::BLACK;
        let white = TextureColor::WHITE;

        let mid = black.lerp(white, 0.5);
        assert!(mid.r >= 127 && mid.r <= 128);
        assert!(mid.g >= 127 && mid.g <= 128);
        assert!(mid.b >= 127 && mid.b <= 128);
    }

    #[test]
    fn test_custom_texture_regenerate() {
        let tex = CustomTexture::new(
            "Test",
            TexturePattern::Solid,
            TextureColor::RED,
            TextureColor::RED,
        );
        assert!(!tex.needs_regeneration());
        assert_eq!(tex.pixels.len(), 64 * 64 * 4);

        // Check it's all red
        for y in 0..64 {
            for x in 0..64 {
                let idx = (y * 64 + x) * 4;
                assert_eq!(tex.pixels[idx], 255); // R
                assert_eq!(tex.pixels[idx + 1], 0); // G
                assert_eq!(tex.pixels[idx + 2], 0); // B
                assert_eq!(tex.pixels[idx + 3], 255); // A
            }
        }
    }

    #[test]
    fn test_library_add_remove() {
        let mut lib = TextureLibrary::new();
        assert_eq!(lib.count(), 0);

        let tex = CustomTexture::new(
            "Test",
            TexturePattern::Solid,
            TextureColor::RED,
            TextureColor::RED,
        );
        let slot = lib.add(tex).unwrap();
        assert_eq!(lib.count(), 1);
        assert!(lib.get(slot).is_some());

        lib.remove(slot).unwrap();
        assert_eq!(lib.count(), 0);
        assert!(lib.get(slot).is_none());
    }

    #[test]
    fn test_library_full() {
        let mut lib = TextureLibrary::new();
        for i in 0..MAX_CUSTOM_TEXTURES {
            let tex = CustomTexture::new(
                format!("Tex {}", i),
                TexturePattern::Solid,
                TextureColor::WHITE,
                TextureColor::WHITE,
            );
            lib.add(tex).unwrap();
        }
        assert!(lib.is_full());

        let extra = CustomTexture::new(
            "Extra",
            TexturePattern::Solid,
            TextureColor::WHITE,
            TextureColor::WHITE,
        );
        assert!(lib.add(extra).is_err());
    }

    #[test]
    fn test_combined_pixel_data() {
        let mut lib = TextureLibrary::new();
        let tex = CustomTexture::new(
            "Test",
            TexturePattern::Solid,
            TextureColor::RED,
            TextureColor::RED,
        );
        let slot = lib.add(tex).unwrap();

        let data = lib.combined_pixel_data();
        assert_eq!(data.len(), MAX_CUSTOM_TEXTURES * 64 * 64 * 4);

        // Check slot 0 has red pixels
        let slot_offset = slot as usize * 64 * 64 * 4;
        assert_eq!(data[slot_offset], 255); // R
        assert_eq!(data[slot_offset + 1], 0); // G
    }

    #[test]
    fn test_generator_helpers() {
        let tex = TextureGenerator::solid("Red", TextureColor::RED);
        assert_eq!(tex.pattern, TexturePattern::Solid);
        assert_eq!(tex.color1.r, 255);

        let checker =
            TextureGenerator::checkerboard("Check", TextureColor::WHITE, TextureColor::BLACK, 1.0);
        assert_eq!(checker.pattern, TexturePattern::Checkerboard);
    }
}

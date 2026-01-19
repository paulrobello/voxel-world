//! Picture atlas for GPU rendering.
//!
//! Manages a texture atlas containing loaded pictures for frame rendering.
//! Pictures are loaded on-demand and use LRU eviction when the atlas is full.

use super::library::{MAX_GPU_PICTURES, MAX_PICTURE_SIZE, Picture, PictureLibrary};
use std::collections::HashMap;

/// A slot in the picture atlas.
#[derive(Debug, Clone, Default)]
struct AtlasSlot {
    /// Picture ID loaded in this slot (0 = empty).
    picture_id: u32,
    /// Last frame this slot was accessed (for LRU eviction).
    last_access_frame: u64,
}

/// Picture atlas for GPU rendering.
///
/// The atlas stores up to MAX_GPU_PICTURES pictures in a texture array.
/// Each slot can hold a picture up to MAX_PICTURE_SIZE × MAX_PICTURE_SIZE.
/// Pictures are loaded on-demand and evicted using LRU when full.
pub struct PictureAtlas {
    /// Atlas slots (picture data).
    slots: Vec<AtlasSlot>,

    /// Map from picture_id to slot index.
    picture_to_slot: HashMap<u32, usize>,

    /// Current frame number for LRU tracking.
    current_frame: u64,

    /// Whether the atlas needs GPU update.
    dirty: bool,

    /// Slots that need to be uploaded to GPU.
    dirty_slots: Vec<usize>,

    /// Packed RGBA data for all slots.
    /// Each slot is MAX_PICTURE_SIZE × MAX_PICTURE_SIZE × 4 bytes.
    /// Total: 64 × 256 × 256 × 4 = 16 MB
    data: Vec<u8>,
}

impl Default for PictureAtlas {
    fn default() -> Self {
        Self::new()
    }
}

impl PictureAtlas {
    /// Creates a new empty picture atlas.
    pub fn new() -> Self {
        let slot_size = MAX_PICTURE_SIZE as usize * MAX_PICTURE_SIZE as usize * 4;
        let total_size = MAX_GPU_PICTURES * slot_size;

        Self {
            slots: vec![AtlasSlot::default(); MAX_GPU_PICTURES],
            picture_to_slot: HashMap::new(),
            current_frame: 0,
            dirty: false,
            dirty_slots: Vec::new(),
            data: vec![0; total_size],
        }
    }

    /// Advances the frame counter for LRU tracking.
    pub fn new_frame(&mut self) {
        self.current_frame += 1;
    }

    /// Clears the dirty flags after GPU upload.
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
        self.dirty_slots.clear();
    }

    /// Returns true if the atlas needs GPU update.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Returns the slots that need GPU upload.
    pub fn dirty_slots(&self) -> &[usize] {
        &self.dirty_slots
    }

    /// Gets the slot index for a picture, loading it if necessary.
    ///
    /// Returns None if the picture doesn't exist in the library.
    pub fn get_or_load(&mut self, picture_id: u32, library: &PictureLibrary) -> Option<usize> {
        // Check if already loaded
        if let Some(&slot) = self.picture_to_slot.get(&picture_id) {
            self.slots[slot].last_access_frame = self.current_frame;
            return Some(slot);
        }

        // Get the picture from library
        let picture = library.get(picture_id)?;

        // Find a slot (empty or LRU)
        let slot = self.find_slot();

        // Evict old picture if necessary
        if self.slots[slot].picture_id != 0 {
            self.picture_to_slot.remove(&self.slots[slot].picture_id);
        }

        // Load the picture into the slot
        self.load_picture(slot, picture);
        self.slots[slot].picture_id = picture_id;
        self.slots[slot].last_access_frame = self.current_frame;
        self.picture_to_slot.insert(picture_id, slot);

        self.dirty = true;
        self.dirty_slots.push(slot);

        Some(slot)
    }

    /// Finds an empty slot or the least recently used slot.
    fn find_slot(&self) -> usize {
        // First try to find an empty slot
        for (i, slot) in self.slots.iter().enumerate() {
            if slot.picture_id == 0 {
                return i;
            }
        }

        // Find LRU slot
        let mut lru_slot = 0;
        let mut lru_frame = u64::MAX;

        for (i, slot) in self.slots.iter().enumerate() {
            if slot.last_access_frame < lru_frame {
                lru_frame = slot.last_access_frame;
                lru_slot = i;
            }
        }

        lru_slot
    }

    /// Loads a picture into an atlas slot.
    fn load_picture(&mut self, slot: usize, picture: &Picture) {
        let slot_size = MAX_PICTURE_SIZE as usize * MAX_PICTURE_SIZE as usize * 4;
        let slot_offset = slot * slot_size;

        // Clear the slot first (transparent)
        for i in 0..slot_size {
            self.data[slot_offset + i] = 0;
        }

        // Copy picture data (centered in slot)
        let offset_x = (MAX_PICTURE_SIZE - picture.width) / 2;
        let offset_y = (MAX_PICTURE_SIZE - picture.height) / 2;

        for y in 0..picture.height {
            for x in 0..picture.width {
                let src_idx = (y as usize * picture.width as usize + x as usize) * 4;
                let dst_x = offset_x as usize + x as usize;
                let dst_y = offset_y as usize + y as usize;
                let dst_idx = slot_offset + (dst_y * MAX_PICTURE_SIZE as usize + dst_x) * 4;

                if src_idx + 4 <= picture.pixels.len() && dst_idx + 4 <= self.data.len() {
                    self.data[dst_idx..dst_idx + 4]
                        .copy_from_slice(&picture.pixels[src_idx..src_idx + 4]);
                }
            }
        }
    }

    /// Evicts a picture from the atlas (when deleted from library).
    pub fn evict(&mut self, picture_id: u32) {
        if let Some(slot) = self.picture_to_slot.remove(&picture_id) {
            self.slots[slot].picture_id = 0;
            self.slots[slot].last_access_frame = 0;

            // Clear the slot data
            let slot_size = MAX_PICTURE_SIZE as usize * MAX_PICTURE_SIZE as usize * 4;
            let slot_offset = slot * slot_size;
            for i in 0..slot_size {
                self.data[slot_offset + i] = 0;
            }

            self.dirty = true;
            self.dirty_slots.push(slot);
        }
    }

    /// Reloads a picture if it's currently in the atlas (after edits).
    pub fn reload(&mut self, picture_id: u32, library: &PictureLibrary) {
        if let Some(&slot) = self.picture_to_slot.get(&picture_id) {
            if let Some(picture) = library.get(picture_id) {
                self.load_picture(slot, picture);
                self.dirty = true;
                self.dirty_slots.push(slot);
            }
        }
    }

    /// Returns the raw RGBA data for the entire atlas.
    ///
    /// Format: 64 slots arranged vertically, each 256×256 pixels.
    /// Total dimensions: 256 × (256 × 64) = 256 × 16384 pixels.
    pub fn get_data(&self) -> &[u8] {
        &self.data
    }

    /// Returns the data for a single slot.
    pub fn get_slot_data(&self, slot: usize) -> Option<&[u8]> {
        if slot >= MAX_GPU_PICTURES {
            return None;
        }
        let slot_size = MAX_PICTURE_SIZE as usize * MAX_PICTURE_SIZE as usize * 4;
        let start = slot * slot_size;
        Some(&self.data[start..start + slot_size])
    }

    /// Returns the atlas dimensions for GPU texture creation.
    pub fn dimensions() -> (u32, u32) {
        // Atlas is 256 pixels wide, 256 × 64 = 16384 pixels tall
        (
            MAX_PICTURE_SIZE as u32,
            MAX_PICTURE_SIZE as u32 * MAX_GPU_PICTURES as u32,
        )
    }

    /// Returns the UV coordinates for a picture slot.
    /// Returns (u_min, v_min, u_max, v_max) in 0.0-1.0 range.
    pub fn slot_uv(slot: usize) -> Option<(f32, f32, f32, f32)> {
        if slot >= MAX_GPU_PICTURES {
            return None;
        }
        let slot_height = 1.0 / MAX_GPU_PICTURES as f32;
        let v_min = slot as f32 * slot_height;
        let v_max = (slot + 1) as f32 * slot_height;
        Some((0.0, v_min, 1.0, v_max))
    }

    /// Returns the number of pictures currently loaded in the atlas.
    pub fn loaded_count(&self) -> usize {
        self.picture_to_slot.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atlas_new() {
        let atlas = PictureAtlas::new();
        assert_eq!(atlas.loaded_count(), 0);
        assert!(!atlas.is_dirty());
    }

    #[test]
    fn test_atlas_dimensions() {
        let (width, height) = PictureAtlas::dimensions();
        assert_eq!(width, 256);
        assert_eq!(height, 256 * 64);
    }

    #[test]
    fn test_slot_uv() {
        let (u_min, v_min, u_max, v_max) = PictureAtlas::slot_uv(0).unwrap();
        assert_eq!(u_min, 0.0);
        assert_eq!(v_min, 0.0);
        assert_eq!(u_max, 1.0);
        assert!((v_max - (1.0 / 64.0)).abs() < 0.0001);

        assert!(PictureAtlas::slot_uv(64).is_none());
    }

    #[test]
    fn test_get_or_load() {
        let mut atlas = PictureAtlas::new();
        let mut library = PictureLibrary::new();

        let pic = Picture::filled("test", 64, 64, [255, 0, 0, 255]);
        let id = library.add(pic);

        let slot = atlas.get_or_load(id, &library);
        assert!(slot.is_some());
        assert_eq!(atlas.loaded_count(), 1);
        assert!(atlas.is_dirty());

        // Loading again should return same slot
        atlas.clear_dirty();
        let slot2 = atlas.get_or_load(id, &library);
        assert_eq!(slot, slot2);
        assert!(!atlas.is_dirty());
    }

    #[test]
    fn test_evict() {
        let mut atlas = PictureAtlas::new();
        let mut library = PictureLibrary::new();

        let pic = Picture::filled("test", 64, 64, [255, 0, 0, 255]);
        let id = library.add(pic);

        atlas.get_or_load(id, &library);
        assert_eq!(atlas.loaded_count(), 1);

        atlas.evict(id);
        assert_eq!(atlas.loaded_count(), 0);
    }
}

//! Query methods for world data including minimap and height cache.

use super::World;
use crate::chunk::BlockType;
use std::collections::HashMap;

impl World {
    /// Invalidates the minimap height cache for a given (x, z) position.
    pub fn invalidate_minimap_cache(&mut self, world_x: i32, world_z: i32) {
        self.minimap_height_cache.remove(&(world_x, world_z));
    }

    /// Clears the entire minimap height cache.
    pub fn clear_minimap_cache(&mut self) {
        self.minimap_height_cache.clear();
    }

    /// Gets the minimap height cache.
    #[allow(dead_code)] // reason: world API — kept for future use
    pub fn minimap_height_cache(&self) -> &HashMap<(i32, i32), (BlockType, i32)> {
        &self.minimap_height_cache
    }

    /// Gets a mutable reference to the minimap height cache.
    pub fn minimap_height_cache_mut(&mut self) -> &mut HashMap<(i32, i32), (BlockType, i32)> {
        &mut self.minimap_height_cache
    }
}

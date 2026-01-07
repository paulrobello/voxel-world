//! Query methods for world data including minimap and height cache.

use super::World;
use crate::chunk::BlockType;
use nalgebra::Vector3;
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
    pub fn minimap_height_cache(&self) -> &HashMap<(i32, i32), (BlockType, i32)> {
        &self.minimap_height_cache
    }

    /// Gets a mutable reference to the minimap height cache.
    pub fn minimap_height_cache_mut(&mut self) -> &mut HashMap<(i32, i32), (BlockType, i32)> {
        &mut self.minimap_height_cache
    }

    pub fn generate_minimap_image(
        &mut self,
        player_pos: Vector3<f64>,
        yaw: f32,
        minimap: &crate::hud::Minimap,
        terrain: &crate::terrain_gen::TerrainGenerator,
    ) -> egui_winit_vulkano::egui::ColorImage {
        use egui_winit_vulkano::egui;
        let display_size = minimap.size as usize;
        let center_x = player_pos.x as f32;
        let center_z = player_pos.z as f32;

        // Base sample radius adjusted by zoom (higher zoom = larger area = zoomed out)
        // When rotating, multiply by sqrt(2) ≈ 1.42 to fill corners
        let base_radius = (display_size as f32 / 2.0) * minimap.zoom;
        let sample_radius = if minimap.rotate {
            base_radius * 1.42
        } else {
            base_radius
        };

        let mut pixels = vec![egui::Color32::BLACK; display_size * display_size];

        // Precompute rotation (rotate world coords to align with player facing direction)
        let (sin_yaw, cos_yaw) = if minimap.rotate {
            (yaw.sin(), yaw.cos())
        } else {
            (0.0, 1.0) // No rotation
        };

        let half = display_size as f32 / 2.0;

        for dz in 0..display_size {
            for dx in 0..display_size {
                // Screen-space offset from center (-half to +half)
                let sx = dx as f32 - half;
                let sz = dz as f32 - half;

                // Scale to sample radius
                let scale = sample_radius / half;
                let scaled_x = sx * scale;
                let scaled_z = sz * scale;

                // Apply rotation to get world-space offset
                let world_offset_x = scaled_x * cos_yaw + scaled_z * sin_yaw;
                let world_offset_z = -scaled_x * sin_yaw + scaled_z * cos_yaw;

                let world_x = (center_x + world_offset_x).floor() as i32;
                let world_z = (center_z + world_offset_z).floor() as i32;

                // Find surface block (top-down) with caching
                let (block_type, height) =
                    if let Some(&cached) = self.minimap_height_cache.get(&(world_x, world_z)) {
                        cached
                    } else {
                        let mut res = (BlockType::Air, 0);
                        for y in (0..crate::constants::TEXTURE_SIZE_Y as i32).rev() {
                            if let Some(block) = self.get_block(Vector3::new(world_x, y, world_z)) {
                                if block != BlockType::Air {
                                    // Skip decorative blocks if enabled
                                    if minimap.skip_decorative
                                        && matches!(
                                            block,
                                            BlockType::Model
                                                | BlockType::Leaves
                                                | BlockType::PineLeaves
                                                | BlockType::WillowLeaves
                                        )
                                    {
                                        continue; // Keep scanning down
                                    }
                                    res = (block, y);
                                    break;
                                }
                            }
                        }
                        self.minimap_height_cache.insert((world_x, world_z), res);
                        res
                    };

                // Get biome info for noise maps
                let biome_info = Some(terrain.get_biome_info(world_x, world_z));

                // Calculate color based on mode
                let color = minimap.get_color(block_type, height, biome_info);

                pixels[dz * display_size + dx] = color;
            }
        }

        egui::ColorImage {
            size: [display_size, display_size],
            pixels,
        }
    }
}

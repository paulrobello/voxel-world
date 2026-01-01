use crate::chunk::BlockType;
use crate::constants::TEXTURE_SIZE_Y;
use crate::world::World;
use egui_winit_vulkano::egui;
use nalgebra::Vector3;

pub struct Minimap {
    pub size: u32,
    pub color_mode: u8,
    pub rotate: bool,
    pub zoom: f32,
}

impl Minimap {
    pub fn new() -> Self {
        Self {
            size: 256,
            color_mode: 2,
            rotate: true,
            zoom: 0.5,
        }
    }

    pub fn get_color(&self, block: BlockType, height: i32) -> egui::Color32 {
        let base_color = block.color();
        let (r, g, b) = (base_color[0], base_color[1], base_color[2]);

        match self.color_mode {
            0 => {
                // Block colors only
                egui::Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
            }
            1 => {
                // Height shading only (grayscale)
                let brightness = ((height as f32 / 128.0) * 200.0 + 55.0).min(255.0) as u8;
                egui::Color32::from_rgb(brightness, brightness, brightness)
            }
            _ => {
                // Both: block colors with height brightness
                let height_factor = 0.5 + (height as f32 / 128.0) * 0.5;
                egui::Color32::from_rgb(
                    (r * 255.0 * height_factor).min(255.0) as u8,
                    (g * 255.0 * height_factor).min(255.0) as u8,
                    (b * 255.0 * height_factor).min(255.0) as u8,
                )
            }
        }
    }

    pub fn generate_image(
        &self,
        world: &World,
        player_pos: Vector3<f64>,
        yaw: f32,
        height_cache: &mut std::collections::HashMap<(i32, i32), (BlockType, i32)>,
    ) -> egui::ColorImage {
        let display_size = self.size as usize;
        let center_x = player_pos.x as f32;
        let center_z = player_pos.z as f32;

        // Base sample radius adjusted by zoom (higher zoom = larger area = zoomed out)
        // When rotating, multiply by sqrt(2) ≈ 1.42 to fill corners
        let base_radius = (display_size as f32 / 2.0) * self.zoom;
        let sample_radius = if self.rotate {
            base_radius * 1.42
        } else {
            base_radius
        };

        let mut pixels = vec![egui::Color32::BLACK; display_size * display_size];

        // Precompute rotation (rotate world coords to align with player facing direction)
        let (sin_yaw, cos_yaw) = if self.rotate {
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
                    if let Some(&cached) = height_cache.get(&(world_x, world_z)) {
                        cached
                    } else {
                        let result = Self::calculate_surface_block(world, world_x, world_z);
                        height_cache.insert((world_x, world_z), result);
                        result
                    };

                // Calculate color based on mode
                let color = self.get_color(block_type, height);

                pixels[dz * display_size + dx] = color;
            }
        }

        egui::ColorImage {
            size: [display_size, display_size],
            pixels,
        }
    }

    fn calculate_surface_block(world: &World, world_x: i32, world_z: i32) -> (BlockType, i32) {
        for y in (0..TEXTURE_SIZE_Y as i32).rev() {
            if let Some(block) = world.get_block(Vector3::new(world_x, y, world_z)) {
                if block != BlockType::Air {
                    return (block, y);
                }
            }
        }
        (BlockType::Air, 0)
    }
}

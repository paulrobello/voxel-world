//! Light collection and emission logic.

use super::World;
use crate::chunk::{BlockType, CHUNK_SIZE};
use crate::gpu_resources::GpuLight;
use crate::player::PLAYER_EYE_HEIGHT;
use crate::sub_voxel::ModelRegistry;
use nalgebra::Vector3;

impl World {
    /// Encodes light mode and intensity for the shader.
    /// Mode: 0 = steady, 1 = slow pulse, 2 = torch flicker
    /// Encoded as: mode + (intensity / 2.0) where intensity is clamped to 0-2 range
    #[inline]
    fn encode_light_intensity(mode: u8, intensity: f32) -> f32 {
        mode as f32 + (intensity.clamp(0.0, 2.0) / 2.0)
    }

    /// Collects all light-emitting blocks (including model blocks like torches)
    /// and returns them as GPU light data.
    pub fn collect_torch_lights(
        &self,
        player_light_enabled: bool,
        player_pos: Vector3<f64>,
        texture_origin: Vector3<i32>,
        model_registry: &ModelRegistry,
        _world_extent: [u32; 3],
    ) -> Vec<GpuLight> {
        let mut lights = Vec::new();

        // Add player light if enabled (like holding a torch)
        if player_light_enabled {
            // Light is at player's hand/chest level, convert to texture coordinates for shader
            let tex_x = (player_pos.x - texture_origin.x as f64) as f32;
            let tex_y = (player_pos.y + PLAYER_EYE_HEIGHT * 0.7 - texture_origin.y as f64) as f32;
            let tex_z = (player_pos.z - texture_origin.z as f64) as f32;

            lights.push(GpuLight {
                pos_radius: [tex_x, tex_y, tex_z, 12.0],
                color_intensity: [1.0, 0.8, 0.5, Self::encode_light_intensity(2, 1.5)], // Flicker mode
            });
        }

        // Iterate over all loaded chunks
        for (chunk_pos, chunk) in self.chunks() {
            // Skip chunks that cannot contribute any light.
            if chunk.is_empty() && chunk.model_count() == 0 && chunk.light_block_count() == 0 {
                continue;
            }

            // Fast path: iterate only model blocks that have metadata.
            if chunk.model_count() > 0 {
                for (idx, model_data) in chunk.model_entries() {
                    if let Some(model) = model_registry.get(model_data.model_id) {
                        if let Some(emission) = &model.emission {
                            let (lx, ly, lz) = crate::chunk::Chunk::index_to_coords(*idx);
                            let world_x = chunk_pos.x * CHUNK_SIZE as i32 + lx as i32;
                            let world_y = chunk_pos.y * CHUNK_SIZE as i32 + ly as i32;
                            let world_z = chunk_pos.z * CHUNK_SIZE as i32 + lz as i32;

                            let tex_x = (world_x - texture_origin.x) as f32 + 0.5;
                            let tex_y = (world_y - texture_origin.y) as f32 + 0.5;
                            let tex_z = (world_z - texture_origin.z) as f32 + 0.5;

                            let r = emission.r as f32 / 255.0;
                            let g = emission.g as f32 / 255.0;
                            let b = emission.b as f32 / 255.0;

                            lights.push(GpuLight {
                                pos_radius: [tex_x, tex_y, tex_z, 10.0],
                                color_intensity: [r, g, b, Self::encode_light_intensity(2, 1.2)], // Flicker mode for torches
                            });

                            if lights.len() >= crate::gpu_resources::MAX_LIGHTS {
                                return lights;
                            }
                        }
                    }
                }
            }

            // Optional scan for non-model light sources (if any).
            if chunk.light_block_count() > 0 {
                for (idx, block) in chunk.iter_blocks() {
                    if !block.is_light_source() {
                        continue;
                    }
                    // light_properties returns (color, intensity), light_radius returns actual radius
                    if let Some((mut color, intensity)) = block.light_properties() {
                        // For Crystal blocks, use the tint color instead of default
                        if block == BlockType::Crystal {
                            let (lx, ly, lz) = crate::chunk::Chunk::index_to_coords(idx);
                            if let Some(tint_index) = chunk.get_tint_index(lx, ly, lz) {
                                color = crate::chunk::tint_color(tint_index);
                            }
                        }

                        let radius = block.light_radius();
                        let mode = block.light_mode();
                        let (lx, ly, lz) = crate::chunk::Chunk::index_to_coords(idx);
                        let world_x = chunk_pos.x * CHUNK_SIZE as i32 + lx as i32;
                        let world_y = chunk_pos.y * CHUNK_SIZE as i32 + ly as i32;
                        let world_z = chunk_pos.z * CHUNK_SIZE as i32 + lz as i32;

                        let tex_x = (world_x - texture_origin.x) as f32 + 0.5;
                        let tex_y = (world_y - texture_origin.y) as f32 + 0.5;
                        let tex_z = (world_z - texture_origin.z) as f32 + 0.5;

                        lights.push(GpuLight {
                            pos_radius: [tex_x, tex_y, tex_z, radius],
                            color_intensity: [
                                color[0],
                                color[1],
                                color[2],
                                Self::encode_light_intensity(mode, intensity),
                            ],
                        });

                        if lights.len() >= crate::gpu_resources::MAX_LIGHTS {
                            return lights;
                        }
                    }
                }
            }
        }

        lights
    }
}

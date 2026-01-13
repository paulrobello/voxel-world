//! Light collection and emission logic.

use super::World;
use crate::chunk::{BlockType, CHUNK_SIZE};
use crate::gpu_resources::GpuLight;
use crate::player::PLAYER_EYE_HEIGHT;
use crate::sub_voxel::ModelRegistry;
use nalgebra::Vector3;

/// Light animation modes (must match shader LIGHT_MODE_* constants)
const LIGHT_MODE_STEADY: u8 = 0;
const LIGHT_MODE_PULSE: u8 = 1;
const LIGHT_MODE_FLICKER: u8 = 2;
const LIGHT_MODE_CANDLE: u8 = 3;
const LIGHT_MODE_STROBE: u8 = 4;
const LIGHT_MODE_BREATHE: u8 = 5;
const LIGHT_MODE_SPARKLE: u8 = 6;
const LIGHT_MODE_WAVE: u8 = 7;
const LIGHT_MODE_WARMUP: u8 = 8;
const LIGHT_MODE_ARC: u8 = 9;

impl World {
    /// Simple hash function for pseudo-random effects based on time and index.
    /// Returns a value in [0, 1].
    #[inline]
    fn hash_noise(seed: f32) -> f32 {
        let x = (seed * 12.9898).sin() * 43_758.547;
        x - x.floor()
    }

    /// Computes the animation factor for a light based on mode, time, and light index.
    /// This is pre-computed on CPU to avoid expensive sin() calls per-pixel in shader.
    #[inline]
    fn compute_animation_factor(mode: u8, animation_time: f32, light_index: usize) -> f32 {
        let i = light_index as f32;
        match mode {
            LIGHT_MODE_STEADY => 1.0,

            LIGHT_MODE_PULSE => {
                // Smooth sine wave pulsing (range 0.5-1.0, speed 2.0)
                0.75 + 0.25 * (animation_time * 2.0 + i * 2.1).sin()
            }

            LIGHT_MODE_FLICKER => {
                // Fire/torch-like random flickering (range 0.3-1.0, speed 10.0)
                let flicker1 = 0.65 + 0.35 * (animation_time * 10.0 + i * 7.3).sin();
                let flicker2 = 0.85 + 0.15 * (animation_time * 17.0 + i * 11.1).sin();
                let flicker3 = 0.90 + 0.10 * (animation_time * 23.0 + i * 3.7).sin();
                (flicker1 * flicker2 * flicker3).clamp(0.3, 1.0)
            }

            LIGHT_MODE_CANDLE => {
                // Subtle candle-like flickering (range 0.6-1.0, speed 4.0)
                // Gentler than torch, with occasional dips
                let base = 0.85 + 0.10 * (animation_time * 4.0 + i * 5.1).sin();
                let wobble = 0.95 + 0.05 * (animation_time * 7.0 + i * 13.3).sin();
                (base * wobble).clamp(0.6, 1.0)
            }

            LIGHT_MODE_STROBE => {
                // Fast on/off blinking (range 0.0-1.0, speed 15.0)
                // Sharp square wave effect
                let phase = (animation_time * 15.0 + i * std::f32::consts::PI).sin();
                if phase > 0.0 { 1.0 } else { 0.05 }
            }

            LIGHT_MODE_BREATHE => {
                // Very slow, gentle pulsing (range 0.5-1.0, ~8 second cycle)
                // Uses cosine curve for smooth easing at peaks and troughs
                // Like a sleeping creature breathing - inhale, hold, exhale, hold
                let phase = animation_time * 0.8 + i * 0.3; // ~8 sec cycle, slight per-light offset
                let breath = (1.0 - phase.cos()) * 0.5; // Smooth 0-1-0 curve
                0.5 + 0.5 * breath // Range 0.5 to 1.0
            }

            LIGHT_MODE_SPARKLE => {
                // Occasional random bright flashes (range 0.7-1.5, speed 8.0)
                // Mostly steady with random bright spikes
                let t = animation_time * 8.0 + i * 17.3;
                let noise = Self::hash_noise(t.floor());
                // 15% chance of sparkle each "frame"
                if noise > 0.85 {
                    // Bright flash that fades within the frame
                    let flash_phase = t - t.floor();
                    1.0 + 0.5 * (1.0 - flash_phase * 2.0).max(0.0)
                } else {
                    // Base shimmer
                    0.75 + 0.05 * (t * 3.0).sin()
                }
            }

            LIGHT_MODE_WAVE => {
                // Synchronized wave pattern (range 0.3-1.0, speed 1.0)
                // All lights pulse together (no per-light phase offset)
                0.65 + 0.35 * (animation_time * 1.0).sin()
            }

            LIGHT_MODE_WARMUP => {
                // Gradual warm-up then steady (range 0.0-1.0, speed 0.3)
                // Ramps up over ~10 seconds then stays at full
                let warmup_duration = 10.0;
                let progress = (animation_time * 0.3).min(warmup_duration) / warmup_duration;
                // Smooth ease-out curve
                let eased = 1.0 - (1.0 - progress).powi(3);
                // Add slight flicker once warmed up
                if progress > 0.9 {
                    eased * (0.97 + 0.03 * (animation_time * 5.0 + i * 2.1).sin())
                } else {
                    eased
                }
            }

            LIGHT_MODE_ARC => {
                // Electrical arc/welding effect (range 0.2-2.0, speed 20.0)
                // Intense, erratic bursts with very bright peaks
                let t = animation_time * 20.0 + i * 7.7;
                let noise1 = Self::hash_noise(t.floor());
                let noise2 = Self::hash_noise(t.floor() + 0.5);

                // 25% chance of bright arc
                if noise1 > 0.75 {
                    // Bright arc burst
                    1.5 + 0.5 * noise2
                } else if noise1 > 0.5 {
                    // Medium intensity crackle
                    0.8 + 0.4 * (t * 3.0).sin().abs()
                } else {
                    // Low idle with occasional flickers
                    0.3 + 0.2 * noise2
                }
            }

            _ => 1.0, // Unknown modes default to steady
        }
    }

    /// Collects all light-emitting blocks (including model blocks like torches)
    /// and returns them as GPU light data with pre-computed animation factors.
    pub fn collect_torch_lights(
        &self,
        player_light_enabled: bool,
        player_pos: Vector3<f64>,
        texture_origin: Vector3<i32>,
        model_registry: &ModelRegistry,
        _world_extent: [u32; 3],
        animation_time: f32,
    ) -> Vec<GpuLight> {
        let mut lights = Vec::new();

        // Add player light if enabled (like holding a torch)
        if player_light_enabled {
            // Light is at player's hand/chest level, convert to texture coordinates for shader
            let tex_x = (player_pos.x - texture_origin.x as f64) as f32;
            let tex_y = (player_pos.y + PLAYER_EYE_HEIGHT * 0.7 - texture_origin.y as f64) as f32;
            let tex_z = (player_pos.z - texture_origin.z as f64) as f32;

            let mode = LIGHT_MODE_FLICKER;
            let intensity = 1.5_f32;
            let anim_factor = Self::compute_animation_factor(mode, animation_time, lights.len());

            lights.push(GpuLight {
                pos_radius: [tex_x, tex_y, tex_z, 12.0],
                color_intensity: [1.0, 0.8, 0.5, intensity],
                animation: [mode as f32, 0.0, 0.0, anim_factor],
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

                            let mode = LIGHT_MODE_FLICKER; // Torches use flicker mode
                            let intensity = 1.2_f32;
                            let anim_factor =
                                Self::compute_animation_factor(mode, animation_time, lights.len());

                            lights.push(GpuLight {
                                pos_radius: [tex_x, tex_y, tex_z, 10.0],
                                color_intensity: [r, g, b, intensity],
                                animation: [mode as f32, 0.0, 0.0, anim_factor],
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

                        let anim_factor =
                            Self::compute_animation_factor(mode, animation_time, lights.len());

                        lights.push(GpuLight {
                            pos_radius: [tex_x, tex_y, tex_z, radius],
                            color_intensity: [color[0], color[1], color[2], intensity],
                            animation: [mode as f32, 0.0, 0.0, anim_factor],
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

//! Lighting and simulation GPU buffers: water sources, template/stencil
//! debug blocks, the per-frame simulation descriptor set (particles, falling
//! blocks, remote players), the point-light buffer + descriptor set, and the
//! per-frame gathering of emissive blocks into animated, frustum-sorted
//! `GpuLight`s ready for upload (see [`collect_torch_lights`]).
//!
//! This gathers lights for the GPU; it does not simulate light propagation.

use super::*;
use crate::chunk::{BlockType, CHUNK_SIZE};
use crate::falling_block::{GpuFallingBlock, MAX_FALLING_BLOCKS};
use crate::particles;
use crate::player::PLAYER_EYE_HEIGHT;
use crate::remote_player::{GpuRemotePlayer, MAX_REMOTE_PLAYERS};
use crate::sub_voxel::{LightMode, ModelRegistry};
use crate::world::World;
use nalgebra::Vector3;
use std::sync::Arc;
use vulkano::{
    buffer::Subbuffer,
    descriptor_set::{
        DescriptorSet, WriteDescriptorSet, allocator::StandardDescriptorSetAllocator,
    },
    memory::allocator::StandardMemoryAllocator,
    pipeline::ComputePipeline,
};

pub const MAX_WATER_SOURCES: usize = 512;

/// GPU-compatible water source data for debug visualization.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuWaterSource {
    /// Position XYZ + type W (0=water, 1=lava)
    pub position: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuTemplateBlock {
    /// Position XYZ + unused W
    pub position: [f32; 4],
}

pub const MAX_TEMPLATE_BLOCKS: usize = 256;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuStencilBlock {
    /// Position XYZ + stencil_id W
    pub position: [f32; 4],
}

pub const MAX_STENCIL_BLOCKS: usize = 4096;

/// GPU storage buffers for dynamic simulation objects (set index 3).
pub struct SimulationBuffers {
    pub particle_buffer: Subbuffer<[particles::GpuParticle]>,
    pub falling_block_buffer: Subbuffer<[GpuFallingBlock]>,
    pub water_source_buffer: Subbuffer<[GpuWaterSource]>,
    pub template_block_buffer: Subbuffer<[GpuTemplateBlock]>,
    pub stencil_block_buffer: Subbuffer<[GpuStencilBlock]>,
    pub remote_player_buffer: Subbuffer<[GpuRemotePlayer]>,
    pub descriptor_set: Arc<DescriptorSet>,
}

/// Creates storage buffers and descriptor set for simulation objects (set index 3).
pub fn get_particle_and_falling_block_set(
    memory_allocator: Arc<StandardMemoryAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
) -> SimulationBuffers {
    use particles::{GpuParticle, MAX_PARTICLES};

    // Create storage buffers
    let particle_buffer =
        make_storage_buffer::<GpuParticle>(&memory_allocator, MAX_PARTICLES as u64);
    let falling_block_buffer =
        make_storage_buffer::<GpuFallingBlock>(&memory_allocator, MAX_FALLING_BLOCKS as u64);
    let water_source_buffer =
        make_storage_buffer::<GpuWaterSource>(&memory_allocator, MAX_WATER_SOURCES as u64);
    let template_block_buffer =
        make_storage_buffer::<GpuTemplateBlock>(&memory_allocator, MAX_TEMPLATE_BLOCKS as u64);
    let stencil_block_buffer =
        make_storage_buffer::<GpuStencilBlock>(&memory_allocator, MAX_STENCIL_BLOCKS as u64);
    let remote_player_buffer =
        make_storage_buffer::<GpuRemotePlayer>(&memory_allocator, MAX_REMOTE_PLAYERS as u64);

    // Create descriptor set at set index 3 with all buffers
    let descriptor_set = make_set(
        &descriptor_set_allocator,
        render_pipeline,
        3,
        [
            WriteDescriptorSet::buffer(0, particle_buffer.clone()),
            WriteDescriptorSet::buffer(1, falling_block_buffer.clone()),
            WriteDescriptorSet::buffer(2, water_source_buffer.clone()),
            WriteDescriptorSet::buffer(3, template_block_buffer.clone()),
            WriteDescriptorSet::buffer(4, stencil_block_buffer.clone()),
            WriteDescriptorSet::buffer(5, remote_player_buffer.clone()),
        ],
    );

    SimulationBuffers {
        particle_buffer,
        falling_block_buffer,
        water_source_buffer,
        template_block_buffer,
        stencil_block_buffer,
        remote_player_buffer,
        descriptor_set,
    }
}

/// Maximum number of point lights (torches) that can be active at once.
pub const MAX_LIGHTS: usize = 256;

/// GPU-compatible point light data for shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuLight {
    /// Position XYZ + radius W
    pub pos_radius: [f32; 4],
    /// Color RGB + intensity A (intensity is raw value, mode is in animation.x)
    pub color_intensity: [f32; 4],
    /// Animation: x = mode (as float), y = reserved, z = reserved, w = pre-computed animation factor
    pub animation: [f32; 4],
}

/// Creates a storage buffer and descriptor set for point light data.
pub fn get_light_set(
    memory_allocator: Arc<StandardMemoryAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
) -> (Subbuffer<[GpuLight]>, Arc<DescriptorSet>) {
    // Create a storage buffer for lights (initialized to zeros)
    let light_buffer = make_storage_buffer::<GpuLight>(&memory_allocator, MAX_LIGHTS as u64);

    // Create descriptor set at set index 4
    let descriptor_set = make_set(
        &descriptor_set_allocator,
        render_pipeline,
        4,
        [WriteDescriptorSet::buffer(0, light_buffer.clone())],
    );

    (light_buffer, descriptor_set)
}

// ===========================================================================
// Per-frame point-light gathering (moved here from `world::lighting` — these
// are render concerns: build `GpuLight`s, frustum-sort, pre-compute animation
// factors; they are not light propagation).
// ===========================================================================

/// Candidate light data for sorting/culling before GPU upload.
struct LightCandidate {
    /// World position of the light
    world_pos: Vector3<f32>,
    /// Light radius (for rendering)
    radius: f32,
    /// Light color RGB
    color: [f32; 3],
    /// Light intensity
    intensity: f32,
    /// Animation mode
    mode: u8,
    /// Squared distance to player (for sorting)
    distance_sq: f32,
}

// Light animation mode numbers. The `LightMode` enum in `sub_voxel::types` is
// the single source of truth: `build.rs` feeds its discriminants to the shader
// (`generated_constants.glsl`), and these consts derive from the same enum so
// Rust and GLSL cannot drift apart.
const LIGHT_MODE_STEADY: u8 = LightMode::Steady as u8;
const LIGHT_MODE_PULSE: u8 = LightMode::Pulse as u8;
const LIGHT_MODE_FLICKER: u8 = LightMode::Flicker as u8;
const LIGHT_MODE_CANDLE: u8 = LightMode::Candle as u8;
const LIGHT_MODE_STROBE: u8 = LightMode::Strobe as u8;
const LIGHT_MODE_BREATHE: u8 = LightMode::Breathe as u8;
const LIGHT_MODE_SPARKLE: u8 = LightMode::Sparkle as u8;
const LIGHT_MODE_WAVE: u8 = LightMode::Wave as u8;
const LIGHT_MODE_WARMUP: u8 = LightMode::WarmUp as u8;
const LIGHT_MODE_ARC: u8 = LightMode::Arc as u8;

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
            let noise = hash_noise(t.floor());
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
            let noise1 = hash_noise(t.floor());
            let noise2 = hash_noise(t.floor() + 0.5);

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
///
/// Uses distance-based culling and sorting for optimal performance:
/// - Lights beyond `cull_radius` are not sent to the GPU
/// - Lights are sorted by distance (closest first)
/// - Lights behind the player are deprioritized via frustum factor
/// - Only up to `max_lights` are sent to the GPU
#[allow(clippy::too_many_arguments)]
pub fn collect_torch_lights(
    world: &World,
    player_light_enabled: bool,
    player_pos: Vector3<f64>,
    camera_dir: Vector3<f32>,
    texture_origin: Vector3<i32>,
    model_registry: &ModelRegistry,
    _world_extent: [u32; 3],
    animation_time: f32,
    cull_radius: f32,
    max_lights: usize,
) -> Vec<GpuLight> {
    let player_pos_f32 = Vector3::new(
        player_pos.x as f32,
        player_pos.y as f32,
        player_pos.z as f32,
    );
    let cull_radius_sq = cull_radius * cull_radius;

    // Collect all candidate lights with their world positions
    let mut candidates: Vec<LightCandidate> = Vec::with_capacity(256);

    // Iterate over all loaded chunks
    for (chunk_pos, chunk) in world.chunks() {
        // Skip chunks that cannot contribute any light.
        if chunk.is_empty() && chunk.model_count() == 0 && chunk.light_block_count() == 0 {
            continue;
        }

        // Early chunk-level distance check: skip entire chunk if too far
        let chunk_center = Vector3::new(
            chunk_pos.x as f32 * CHUNK_SIZE as f32 + CHUNK_SIZE as f32 * 0.5,
            chunk_pos.y as f32 * CHUNK_SIZE as f32 + CHUNK_SIZE as f32 * 0.5,
            chunk_pos.z as f32 * CHUNK_SIZE as f32 + CHUNK_SIZE as f32 * 0.5,
        );
        let chunk_dist_sq = (chunk_center - player_pos_f32).magnitude_squared();
        // Add chunk diagonal to cull radius for conservative check
        let chunk_max_dist = cull_radius + CHUNK_SIZE as f32 * 1.732; // sqrt(3) ≈ 1.732
        if chunk_dist_sq > chunk_max_dist * chunk_max_dist {
            continue;
        }

        // Fast path: iterate only model blocks that have metadata.
        if chunk.model_count() > 0 {
            for (idx, model_data) in chunk.model_entries() {
                if let Some(model) = model_registry.get(model_data.model_id)
                    && let Some(emission) = &model.emission
                {
                    let (lx, ly, lz) = crate::chunk::Chunk::index_to_coords(*idx);
                    let world_x = chunk_pos.x * CHUNK_SIZE as i32 + lx as i32;
                    let world_y = chunk_pos.y * CHUNK_SIZE as i32 + ly as i32;
                    let world_z = chunk_pos.z * CHUNK_SIZE as i32 + lz as i32;

                    let world_pos = Vector3::new(
                        world_x as f32 + 0.5,
                        world_y as f32 + 0.5,
                        world_z as f32 + 0.5,
                    );

                    // Distance-based culling
                    let distance_sq = (world_pos - player_pos_f32).magnitude_squared();
                    if distance_sq > cull_radius_sq {
                        continue;
                    }

                    let r = emission.r as f32 / 255.0;
                    let g = emission.g as f32 / 255.0;
                    let b = emission.b as f32 / 255.0;

                    candidates.push(LightCandidate {
                        world_pos,
                        radius: 10.0,
                        color: [r, g, b],
                        intensity: 1.2,
                        mode: LIGHT_MODE_FLICKER,
                        distance_sq,
                    });
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

                    let world_pos = Vector3::new(
                        world_x as f32 + 0.5,
                        world_y as f32 + 0.5,
                        world_z as f32 + 0.5,
                    );

                    // Distance-based culling
                    let distance_sq = (world_pos - player_pos_f32).magnitude_squared();
                    if distance_sq > cull_radius_sq {
                        continue;
                    }

                    candidates.push(LightCandidate {
                        world_pos,
                        radius,
                        color,
                        intensity,
                        mode,
                        distance_sq,
                    });
                }
            }
        }
    }

    // Apply frustum-aware sorting: lights behind player get deprioritized
    // by multiplying their distance by a factor (lights in front = lower effective distance)
    let camera_dir_normalized = if camera_dir.magnitude_squared() > 0.001 {
        camera_dir.normalize()
    } else {
        Vector3::new(0.0, 0.0, -1.0)
    };

    candidates.sort_by(|a, b| {
        // Calculate dot product with camera direction
        let dir_a = (a.world_pos - player_pos_f32).normalize();
        let dir_b = (b.world_pos - player_pos_f32).normalize();
        let dot_a = dir_a.dot(&camera_dir_normalized);
        let dot_b = dir_b.dot(&camera_dir_normalized);

        // Frustum factor: 1.0 for lights directly in front, 2.0 for lights directly behind
        // This deprioritizes lights behind the player without completely removing them
        let frustum_factor_a = 1.5 - dot_a * 0.5; // Range: 1.0 (in front) to 2.0 (behind)
        let frustum_factor_b = 1.5 - dot_b * 0.5;

        let effective_dist_a = a.distance_sq * frustum_factor_a;
        let effective_dist_b = b.distance_sq * frustum_factor_b;

        effective_dist_a
            .partial_cmp(&effective_dist_b)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Convert sorted candidates to GPU lights
    let mut lights = Vec::with_capacity(max_lights.min(candidates.len()) + 1);

    // Add player light first if enabled (always highest priority)
    if player_light_enabled {
        let tex_x = (player_pos.x - texture_origin.x as f64) as f32;
        let tex_y = (player_pos.y + PLAYER_EYE_HEIGHT * 0.7 - texture_origin.y as f64) as f32;
        let tex_z = (player_pos.z - texture_origin.z as f64) as f32;

        let mode = LIGHT_MODE_FLICKER;
        let intensity = 1.5_f32;
        let anim_factor = compute_animation_factor(mode, animation_time, 0);

        lights.push(GpuLight {
            pos_radius: [tex_x, tex_y, tex_z, 12.0],
            color_intensity: [1.0, 0.8, 0.5, intensity],
            animation: [mode as f32, 0.0, 0.0, anim_factor],
        });
    }

    // Add sorted world lights up to max_lights
    let remaining_capacity = max_lights.saturating_sub(lights.len());
    for candidate in candidates.into_iter().take(remaining_capacity) {
        let tex_x = candidate.world_pos.x - texture_origin.x as f32;
        let tex_y = candidate.world_pos.y - texture_origin.y as f32;
        let tex_z = candidate.world_pos.z - texture_origin.z as f32;

        let anim_factor = compute_animation_factor(candidate.mode, animation_time, lights.len());

        lights.push(GpuLight {
            pos_radius: [tex_x, tex_y, tex_z, candidate.radius],
            color_intensity: [
                candidate.color[0],
                candidate.color[1],
                candidate.color[2],
                candidate.intensity,
            ],
            animation: [candidate.mode as f32, 0.0, 0.0, anim_factor],
        });
    }

    lights
}

//! GPU resource management: Vulkan buffers, textures, and per-frame upload batching.
//!
//! Manages the GPU-side representation of the voxel world, including the 3D block
//! texture array, SVT (sparse voxel tree) brick masks, chunk metadata buffers, and
//! sub-voxel model atlases. Provides batched chunk upload logic to minimize GPU
//! synchronization overhead during streaming.

// Shared GPU imports. The per-bucket submodules (atlas / chunk_upload / lighting
// / model_upload / swapchain / staging) reach these via `use super::*`, so this
// block is intentionally broader than `mod.rs` core uses on its own.
use egui_winit_vulkano::{Gui, egui};
use nalgebra::Matrix4;
use std::sync::Arc;
use vulkano::{
    buffer::{Buffer, BufferContents, BufferCreateInfo, BufferUsage, Subbuffer},
    command_buffer::{
        AutoCommandBufferBuilder, BufferImageCopy, ClearColorImageInfo, CommandBufferUsage,
        CopyBufferToImageInfo, PrimaryCommandBufferAbstract,
        allocator::StandardCommandBufferAllocator,
    },
    descriptor_set::{
        DescriptorSet, WriteDescriptorSet, allocator::StandardDescriptorSetAllocator,
    },
    device::{DeviceOwned, Queue},
    format::Format,
    image::{
        Image, ImageCreateInfo, ImageType, ImageUsage,
        sampler::{Filter, Sampler, SamplerAddressMode, SamplerCreateInfo},
        view::{ImageView, ImageViewCreateInfo},
    },
    memory::allocator::{AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator},
    pipeline::{ComputePipeline, Pipeline},
    swapchain::Swapchain,
    sync::GpuFuture,
};
use winit::window::Window;

mod staging;
pub use staging::*;

mod atlas;
pub use atlas::*;
mod chunk_upload;
pub use chunk_upload::*;
mod lighting;
pub use lighting::*;
mod model_upload;
pub use model_upload::*;
mod swapchain;
pub use swapchain::*;

/// Helper to allocate a storage buffer with the common flags used across GPU resources.
pub(crate) fn make_storage_buffer<T: BufferContents>(
    memory_allocator: &Arc<StandardMemoryAllocator>,
    len: u64,
) -> Subbuffer<[T]> {
    Buffer::new_slice::<T>(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::STORAGE_BUFFER,
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
            ..Default::default()
        },
        len,
    )
    .expect("Failed to allocate GPU storage buffer")
}

/// Helper to allocate a host-local storage buffer for metadata that is updated frequently.
/// Uses PREFER_HOST | HOST_RANDOM_ACCESS for optimal CPU write performance.
/// On unified memory systems (like M4 Max), this eliminates sync stalls when
/// the GPU reads metadata that was just written by the CPU.
pub(crate) fn make_coherent_storage_buffer<T: BufferContents>(
    memory_allocator: &Arc<StandardMemoryAllocator>,
    len: u64,
) -> Subbuffer<[T]> {
    Buffer::new_slice::<T>(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::STORAGE_BUFFER,
            ..Default::default()
        },
        AllocationCreateInfo {
            // PREFER_HOST places the buffer in host-local (system) memory, which is:
            // 1. Optimal for frequent CPU writes (no PCIe transfer overhead)
            // 2. On unified memory systems, still directly accessible by GPU
            // HOST_RANDOM_ACCESS allows efficient per-element updates via HOST_CACHED.
            memory_type_filter: MemoryTypeFilter::PREFER_HOST
                | MemoryTypeFilter::HOST_RANDOM_ACCESS,
            ..Default::default()
        },
        len,
    )
    .expect("Failed to allocate coherent GPU storage buffer")
}

/// Helper to create a descriptor set for a given pipeline set index.
pub(crate) fn make_set(
    descriptor_set_allocator: &Arc<StandardDescriptorSetAllocator>,
    pipeline: &ComputePipeline,
    set_idx: usize,
    writes: impl IntoIterator<Item = WriteDescriptorSet>,
) -> Arc<DescriptorSet> {
    let layout = pipeline
        .layout()
        .set_layouts()
        .get(set_idx)
        .expect("Pipeline set layout index out of bounds")
        .clone();
    DescriptorSet::new(descriptor_set_allocator.clone(), layout, writes, [])
        .expect("Failed to create descriptor set")
}

pub struct RenderContext {
    pub window: Arc<Window>,
    pub swapchain: Arc<Swapchain>,
    pub image_views: Vec<Arc<ImageView>>,

    pub render_image: Arc<Image>,
    pub render_set: Arc<DescriptorSet>,
    pub resample_image: Arc<Image>,
    pub resample_set: Arc<DescriptorSet>,

    /// Distance buffer for two-pass beam optimization (1/4 resolution)
    pub distance_image: Arc<Image>,
    pub distance_set: Arc<DescriptorSet>,

    pub gui: Gui,
    /// Texture ID for the atlas in egui.
    pub atlas_texture_id: egui::TextureId,
    /// Optional per-block/model sprite textures loaded from disk.
    pub sprite_icons: SpriteIcons,

    /// Picture atlas for frame pictures.
    #[allow(dead_code)]
    pub picture_atlas: Arc<Image>,
    /// Picture atlas image view for shader access.
    #[allow(dead_code)]
    pub picture_atlas_view: Arc<ImageView>,

    pub recreate_swapchain: bool,
}

#[derive(BufferContents, Clone, Copy)]
#[repr(C)]
pub struct PushConstants {
    pub pixel_to_ray: Matrix4<f32>,
    pub texture_size_x: u32,
    pub texture_size_y: u32,
    pub texture_size_z: u32,
    pub render_mode: u32,
    pub show_chunk_boundaries: u32,
    pub player_in_water: u32,
    pub time_of_day: f32,
    pub animation_time: f32,
    pub cloud_speed: f32,
    pub cloud_coverage: f32,
    pub cloud_color_r: f32,
    pub cloud_color_g: f32,
    pub cloud_color_b: f32,
    pub clouds_enabled: u32,
    pub break_block_x: i32,
    pub break_block_y: i32,
    pub break_block_z: i32,
    pub break_progress: f32,
    pub particle_count: u32,
    pub preview_block_x: i32,
    pub preview_block_y: i32,
    pub preview_block_z: i32,
    pub preview_block_type: u32,
    pub light_count: u32,
    pub ambient_light: f32,
    pub fog_density: f32,
    pub fog_start: f32,
    pub fog_overlay_scale: f32,
    pub target_block_x: i32,
    pub target_block_y: i32,
    pub target_block_z: i32,
    pub max_ray_steps: u32,
    pub shadow_max_steps: u32,
    pub texture_origin_x: i32,
    pub texture_origin_y: i32,
    pub texture_origin_z: i32,
    pub enable_ao: u32,
    pub enable_shadows: u32,
    pub enable_model_shadows: u32,
    pub enable_point_lights: u32,
    pub enable_tinted_shadows: u32,
    pub transparent_background: u32,
    pub pass_mode: u32,
    pub lod_ao_distance: f32,
    pub lod_shadow_distance: f32,
    pub lod_point_light_distance: f32,
    pub lod_model_distance: f32,
    pub falling_block_count: u32,
    pub show_water_sources: u32,
    pub water_source_count: u32,
    pub template_block_count: u32,
    pub template_preview_min_x: i32,
    pub template_preview_min_y: i32,
    pub template_preview_min_z: i32,
    pub template_preview_max_x: i32,
    pub template_preview_max_y: i32,
    pub template_preview_max_z: i32,
    pub _padding: [u8; 12],   // GLSL aligns vec4 to 16 bytes
    pub camera_pos: [f32; 4], // vec4 in GLSL requires 16-byte alignment
    pub selection_pos1_x: i32,
    pub selection_pos1_y: i32,
    pub selection_pos1_z: i32,
    pub selection_pos2_x: i32,
    pub selection_pos2_y: i32,
    pub selection_pos2_z: i32,
    pub hide_ground_cover: u32,
    pub cutaway_enabled: u32,
    pub cutaway_chunk_x: i32,
    pub cutaway_chunk_y: i32,
    pub cutaway_chunk_z: i32,
    pub cutaway_player_chunk_x: i32,
    pub cutaway_player_chunk_z: i32,
    // Measurement markers (up to 4 positions)
    pub measurement_marker_count: u32,
    pub measurement_marker_0_x: i32,
    pub measurement_marker_0_y: i32,
    pub measurement_marker_0_z: i32,
    pub measurement_marker_1_x: i32,
    pub measurement_marker_1_y: i32,
    pub measurement_marker_1_z: i32,
    pub measurement_marker_2_x: i32,
    pub measurement_marker_2_y: i32,
    pub measurement_marker_2_z: i32,
    pub measurement_marker_3_x: i32,
    pub measurement_marker_3_y: i32,
    pub measurement_marker_3_z: i32,
    // Stencil rendering
    pub stencil_block_count: u32,
    pub stencil_opacity: f32,
    pub stencil_render_mode: u32,
    // Measurement laser color
    pub laser_color_r: f32,
    pub laser_color_g: f32,
    pub laser_color_b: f32,
    // Sky colors (day)
    pub sky_zenith_r: f32,
    pub sky_zenith_g: f32,
    pub sky_zenith_b: f32,
    pub sky_horizon_r: f32,
    pub sky_horizon_g: f32,
    pub sky_horizon_b: f32,
    // Picture frame rendering
    pub selected_picture_id: u32,
    // Remote player rendering
    pub remote_player_count: u32,
    // Custom texture count for multiplayer
    pub custom_texture_count: u32,
    // Pre-computed animated pulses (host-side to avoid per-fragment sin)
    /// Fully-baked GlowMushroom emission multiplier: `0.95 + 0.05 * sin(animation_time * 1.5)`.
    pub mushroom_pulse: f32,
    /// Time phase for the lava pulse: `animation_time * 2.0`. The per-fragment sin still
    /// runs (pulse depends on hit.x/hit.z) but the time mul is done once per frame.
    pub lava_time_phase: f32,
}

/// "All features off" baseline for `PushConstants`.
///
/// Coordinate fields use the shader's inactive sentinels (`-1` for block coords,
/// `-1000` for cutaway chunks, `-10000` for measurement markers); counts and
/// enable flags are `0`. Secondary construction sites (sprite generation, etc.)
/// override only the fields they care about via `PushConstants { ..Default::default() }`,
/// so adding a field to the struct no longer forces every site to be edited.
impl Default for PushConstants {
    fn default() -> Self {
        Self {
            pixel_to_ray: Matrix4::identity(),
            texture_size_x: 0,
            texture_size_y: 0,
            texture_size_z: 0,
            render_mode: 0,
            show_chunk_boundaries: 0,
            player_in_water: 0,
            time_of_day: 0.0,
            animation_time: 0.0,
            cloud_speed: 0.0,
            cloud_coverage: 0.0,
            cloud_color_r: 0.0,
            cloud_color_g: 0.0,
            cloud_color_b: 0.0,
            clouds_enabled: 0,
            break_block_x: -1,
            break_block_y: -1,
            break_block_z: -1,
            break_progress: 0.0,
            particle_count: 0,
            preview_block_x: -1,
            preview_block_y: -1,
            preview_block_z: -1,
            preview_block_type: 0,
            light_count: 0,
            ambient_light: 0.0,
            fog_density: 0.0,
            fog_start: 0.0,
            fog_overlay_scale: 0.0,
            target_block_x: -1,
            target_block_y: -1,
            target_block_z: -1,
            max_ray_steps: 0,
            shadow_max_steps: 0,
            texture_origin_x: 0,
            texture_origin_y: 0,
            texture_origin_z: 0,
            enable_ao: 0,
            enable_shadows: 0,
            enable_model_shadows: 0,
            enable_point_lights: 0,
            enable_tinted_shadows: 0,
            transparent_background: 0,
            pass_mode: 0,
            lod_ao_distance: 0.0,
            lod_shadow_distance: 0.0,
            lod_point_light_distance: 0.0,
            lod_model_distance: 0.0,
            falling_block_count: 0,
            show_water_sources: 0,
            water_source_count: 0,
            template_block_count: 0,
            template_preview_min_x: -1,
            template_preview_min_y: -1,
            template_preview_min_z: -1,
            template_preview_max_x: -1,
            template_preview_max_y: -1,
            template_preview_max_z: -1,
            _padding: [0; 12],
            camera_pos: [0.0; 4],
            selection_pos1_x: -1,
            selection_pos1_y: -1,
            selection_pos1_z: -1,
            selection_pos2_x: -1,
            selection_pos2_y: -1,
            selection_pos2_z: -1,
            hide_ground_cover: 0,
            cutaway_enabled: 0,
            cutaway_chunk_x: -1000,
            cutaway_chunk_y: -1000,
            cutaway_chunk_z: -1000,
            cutaway_player_chunk_x: -1000,
            cutaway_player_chunk_z: -1000,
            measurement_marker_count: 0,
            measurement_marker_0_x: -10000,
            measurement_marker_0_y: -10000,
            measurement_marker_0_z: -10000,
            measurement_marker_1_x: -10000,
            measurement_marker_1_y: -10000,
            measurement_marker_1_z: -10000,
            measurement_marker_2_x: -10000,
            measurement_marker_2_y: -10000,
            measurement_marker_2_z: -10000,
            measurement_marker_3_x: -10000,
            measurement_marker_3_y: -10000,
            measurement_marker_3_z: -10000,
            stencil_block_count: 0,
            stencil_opacity: 0.0,
            stencil_render_mode: 0,
            laser_color_r: 0.0,
            laser_color_g: 0.0,
            laser_color_b: 0.0,
            sky_zenith_r: 0.0,
            sky_zenith_g: 0.0,
            sky_zenith_b: 0.0,
            sky_horizon_r: 0.0,
            sky_horizon_g: 0.0,
            sky_horizon_b: 0.0,
            selected_picture_id: 0,
            remote_player_count: 0,
            custom_texture_count: 0,
            mushroom_pulse: 0.0,
            lava_time_phase: 0.0,
        }
    }
}

/// Maximum number of water/lava sources to show in debug mode.

/// Number of chunks in the metadata buffer (must match shader constants)
/// Number of u32 words for brick masks (2 words = 64 bits per chunk).

/// Queue and image targets used by [`upload_chunks_batched`].
///
/// Groups the transfer-queue configuration and destination images so that
/// callers only need to pass allocators + this config instead of 8+ separate
/// arguments.

#[cfg(test)]
mod tests {
    use super::PushConstants;

    // REN-M06: sprite_gen builds its push constants as `PushConstants { ..Default::default() }`,
    // so the Default impl is the contract that "feature off" sentinels must keep working without
    // sprite_gen having to name every field. Pin the sentinels the shader treats as "inactive".
    #[test]
    fn default_push_constants_uses_inactive_sentinels() {
        let d = PushConstants::default();
        // Block coordinates default to -1 ("no block").
        assert_eq!(d.break_block_x, -1);
        assert_eq!(d.preview_block_x, -1);
        assert_eq!(d.target_block_x, -1);
        assert_eq!(d.template_preview_min_x, -1);
        assert_eq!(d.selection_pos1_x, -1);
        assert_eq!(d.selection_pos2_z, -1);
        // Cutaway chunks default to -1000 ("no cutaway").
        assert_eq!(d.cutaway_chunk_x, -1000);
        assert_eq!(d.cutaway_player_chunk_x, -1000);
        // Measurement markers default to -10000 ("no marker").
        assert_eq!(d.measurement_marker_0_x, -10000);
        assert_eq!(d.measurement_marker_3_z, -10000);
        assert_eq!(d.measurement_marker_count, 0);
        // Counts / enable flags default to 0.
        assert_eq!(d.clouds_enabled, 0);
        assert_eq!(d.enable_ao, 0);
        assert_eq!(d.light_count, 0);
        assert_eq!(d.stencil_block_count, 0);
        assert_eq!(d.falling_block_count, 0);
        // Padding and camera_pos default to zero.
        assert_eq!(d._padding, [0u8; 12]);
        assert_eq!(d.camera_pos, [0.0f32; 4]);
    }
}

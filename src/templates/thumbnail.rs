//! Template thumbnail generation using GPU rendering.
//!
//! This module generates 64x64 PNG thumbnails for templates by rendering them
//! with the same GPU raymarching system used for sprite generation.

use crate::chunk::{CHUNK_SIZE, CHUNK_VOLUME};
use crate::gpu_resources::{
    PushConstants, create_empty_voxel_texture, get_brick_and_model_set, get_chunk_metadata_set,
    get_distance_image_and_set, get_images_and_sets, get_light_set,
    get_particle_and_falling_block_set, load_texture_atlas, save_screenshot, upload_chunks_batched,
};
use crate::hot_reload::HotReloadComputePipeline;
use crate::render_mode::RenderMode;
use crate::sub_voxel::ModelRegistry;
use crate::vulkan_context::VulkanContext;
use nalgebra::{Matrix4, Vector3};
use std::error::Error;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use vulkano::command_buffer::allocator::StandardCommandBufferAllocator;
use vulkano::command_buffer::{
    AutoCommandBufferBuilder, ClearColorImageInfo, CommandBufferUsage, PrimaryCommandBufferAbstract,
};
use vulkano::device::Queue;
use vulkano::image::Image;
use vulkano::image::view::{ImageView, ImageViewCreateInfo};
use vulkano::memory::allocator::StandardMemoryAllocator;
use vulkano::pipeline::Pipeline;
use vulkano::sync::GpuFuture;
use winit::event_loop::EventLoop;

use super::format::VxtFile;

const THUMBNAIL_SIZE: u32 = 64;
const THUMBNAIL_WORLD_EXTENT: [u32; 3] = [CHUNK_SIZE as u32, CHUNK_SIZE as u32, CHUNK_SIZE as u32];

/// Generate a thumbnail for a template and save it as a PNG.
///
/// The thumbnail is saved alongside the template with the same name but .png extension.
/// Returns the path to the generated thumbnail.
pub fn generate_template_thumbnail(
    template: &VxtFile,
    template_path: &Path,
    event_loop: &EventLoop<()>,
) -> Result<PathBuf, Box<dyn Error>> {
    let thumbnail_path = template_path.with_extension("png");

    // Initialize Vulkan context
    let vk = VulkanContext::new(event_loop);
    let memory_allocator = vk.memory_allocator.clone();
    let descriptor_set_allocator = vk.descriptor_set_allocator.clone();
    let command_buffer_allocator = vk.command_buffer_allocator.clone();

    // Load shaders
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let shaders_dir = root.join("shaders");
    let render_pipeline =
        HotReloadComputePipeline::new(vk.device.clone(), &shaders_dir.join("traverse.comp"));
    let resample_pipeline =
        HotReloadComputePipeline::new(vk.device.clone(), &shaders_dir.join("resample.comp"));

    // Set up rendering resources
    let render_extent = [THUMBNAIL_SIZE, THUMBNAIL_SIZE];
    let window_extent = render_extent;

    let (render_image, render_set, _resample_image, _resample_set) = get_images_and_sets(
        memory_allocator.clone(),
        descriptor_set_allocator.clone(),
        &render_pipeline,
        &resample_pipeline,
        render_extent,
        window_extent,
    );

    let (distance_image, distance_set) = get_distance_image_and_set(
        memory_allocator.clone(),
        descriptor_set_allocator.clone(),
        &render_pipeline,
        render_extent,
    );

    let (voxel_set, voxel_image) = create_empty_voxel_texture(
        memory_allocator.clone(),
        command_buffer_allocator.clone(),
        descriptor_set_allocator.clone(),
        &render_pipeline,
        &vk.queue,
        THUMBNAIL_WORLD_EXTENT,
    );

    let texture_path = root.join("textures").join("texture_atlas.png");
    let (texture_set, _sampler, _atlas_view) = load_texture_atlas(
        memory_allocator.clone(),
        command_buffer_allocator.clone(),
        descriptor_set_allocator.clone(),
        &render_pipeline,
        &vk.queue,
        &texture_path,
    );

    let (
        _particle_buffer,
        _falling_block_buffer,
        _water_source_buffer,
        _template_block_buffer,
        particle_set,
    ) = get_particle_and_falling_block_set(
        memory_allocator.clone(),
        descriptor_set_allocator.clone(),
        &render_pipeline,
    );

    let (_light_buffer, light_set) = get_light_set(
        memory_allocator.clone(),
        descriptor_set_allocator.clone(),
        &render_pipeline,
    );

    let (chunk_meta_buffer, chunk_metadata_set) = get_chunk_metadata_set(
        memory_allocator.clone(),
        descriptor_set_allocator.clone(),
        &render_pipeline,
    );
    {
        let mut flags = chunk_meta_buffer.write().unwrap();
        for f in flags.iter_mut() {
            *f = 0;
        }
    }

    // Load model registry (for template models)
    let mut model_registry = ModelRegistry::new();
    let library_path = std::path::Path::new("user_models");
    let _ = model_registry.load_library_models(library_path);

    let (
        brick_mask_buffer,
        _brick_dist_buffer,
        _model_atlas_8,
        _model_atlas_16,
        _model_atlas_32,
        _model_palettes,
        _model_palette_emission,
        model_metadata_image,
        _model_properties_buffer,
        brick_and_model_set,
    ) = get_brick_and_model_set(
        memory_allocator.clone(),
        command_buffer_allocator.clone(),
        descriptor_set_allocator.clone(),
        &render_pipeline,
        &vk.queue,
        THUMBNAIL_WORLD_EXTENT,
        &model_registry,
    );
    {
        let mut masks = brick_mask_buffer.write().unwrap();
        masks.fill(u32::MAX);
    }

    let render_image_view = ImageView::new(
        render_image.clone(),
        ImageViewCreateInfo::from_image(&render_image),
    )
    .unwrap();

    // Clear images
    {
        let mut builder = AutoCommandBufferBuilder::primary(
            command_buffer_allocator.clone(),
            vk.queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )?;

        builder.clear_color_image(ClearColorImageInfo::image(render_image.clone()))?;
        builder.clear_color_image(ClearColorImageInfo::image(voxel_image.clone()))?;
        builder.clear_color_image(ClearColorImageInfo::image(model_metadata_image.clone()))?;
        builder.clear_color_image(ClearColorImageInfo::image(distance_image.clone()))?;

        builder
            .build()?
            .execute(vk.queue.clone())?
            .then_signal_fence_and_flush()?
            .wait(None)?;
    }

    // Fill voxel texture with template blocks
    fill_template_voxels(
        template,
        &vk.queue,
        &memory_allocator,
        &command_buffer_allocator,
        &voxel_image,
        &model_metadata_image,
    )?;

    // Calculate camera position based on template size
    let (cam_world, look_at) = calculate_camera_position(template);
    let pixel_to_ray = build_pixel_to_ray(cam_world, look_at, render_extent, 35.0);

    // Set up push constants
    let push_constants = PushConstants {
        pixel_to_ray,
        texture_size_x: THUMBNAIL_WORLD_EXTENT[0],
        texture_size_y: THUMBNAIL_WORLD_EXTENT[1],
        texture_size_z: THUMBNAIL_WORLD_EXTENT[2],
        render_mode: RenderMode::Textured as u32,
        show_chunk_boundaries: 0,
        player_in_water: 0,
        time_of_day: 14.0 / 24.0, // 14:00 for good lighting
        animation_time: 0.0,
        cloud_speed: 0.0,
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
        ambient_light: 0.1,
        fog_density: 0.0,
        fog_start: 99999.0,
        fog_overlay_scale: 0.0,
        target_block_x: -1,
        target_block_y: -1,
        target_block_z: -1,
        max_ray_steps: 256,
        shadow_max_steps: 128,
        texture_origin_x: 0,
        texture_origin_y: 0,
        texture_origin_z: 0,
        enable_ao: 1,
        enable_shadows: 1,
        enable_model_shadows: 1,
        enable_point_lights: 0,
        enable_tinted_shadows: 0,
        transparent_background: 1,
        pass_mode: 0,
        lod_ao_distance: 64.0,
        lod_shadow_distance: 48.0,
        lod_point_light_distance: 20.0,
        lod_model_distance: 32.0,
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
        camera_pos: [
            cam_world.x as f32,
            cam_world.y as f32,
            cam_world.z as f32,
            0.0,
        ],
        selection_pos1_x: -1,
        selection_pos1_y: -1,
        selection_pos1_z: -1,
        selection_pos2_x: -1,
        selection_pos2_y: -1,
        selection_pos2_z: -1,
    };

    // Render
    {
        let mut builder = AutoCommandBufferBuilder::primary(
            command_buffer_allocator.clone(),
            vk.queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )?;

        let pipeline = Arc::clone(&*render_pipeline);
        builder
            .bind_pipeline_compute(pipeline.clone())?
            .bind_descriptor_sets(
                vulkano::pipeline::PipelineBindPoint::Compute,
                pipeline.layout().clone(),
                0,
                vec![
                    render_set.clone(),
                    distance_set.clone(),
                    voxel_set.clone(),
                    texture_set.clone(),
                    particle_set.clone(),
                    light_set.clone(),
                    chunk_metadata_set.clone(),
                    brick_and_model_set.clone(),
                ],
            )?
            .push_constants(pipeline.layout().clone(), 0, push_constants)?;

        unsafe {
            builder.dispatch([
                render_extent[0].div_ceil(8),
                render_extent[1].div_ceil(8),
                1,
            ])?;
        }

        builder
            .build()?
            .execute(vk.queue.clone())?
            .then_signal_fence_and_flush()?
            .wait(None)?;
    }

    // Save thumbnail
    let thumbnail_path_str = thumbnail_path
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid thumbnail path"))?;
    save_screenshot(
        &vk.device,
        &vk.queue,
        &memory_allocator,
        &command_buffer_allocator,
        &render_image_view,
        thumbnail_path_str,
    );

    println!(
        "[thumbnail] Generated thumbnail for '{}' at {}",
        template.name,
        thumbnail_path.display()
    );

    Ok(thumbnail_path)
}

/// Fill the voxel texture with template blocks.
fn fill_template_voxels(
    template: &VxtFile,
    queue: &Arc<Queue>,
    memory_allocator: &Arc<StandardMemoryAllocator>,
    command_buffer_allocator: &Arc<StandardCommandBufferAllocator>,
    voxel_image: &Arc<Image>,
    model_metadata_image: &Arc<Image>,
) -> Result<(), Box<dyn Error>> {
    let chunk_pos = Vector3::new(0, 0, 0);
    let mut block_buf = vec![0u8; CHUNK_VOLUME];
    let mut meta_buf = vec![0u8; CHUNK_VOLUME * 2];

    // Center the template in the chunk
    let offset_x = (CHUNK_SIZE - template.width as usize) / 2;
    let offset_y = (CHUNK_SIZE - template.height as usize) / 2;
    let offset_z = (CHUNK_SIZE - template.depth as usize) / 2;

    // Fill blocks
    for block in &template.blocks {
        let x = block.x as usize + offset_x;
        let y = block.y as usize + offset_y;
        let z = block.z as usize + offset_z;

        if x < CHUNK_SIZE && y < CHUNK_SIZE && z < CHUNK_SIZE {
            let idx = x + y * CHUNK_SIZE + z * CHUNK_SIZE * CHUNK_SIZE;
            block_buf[idx] = block.block_type;
        }
    }

    // Fill model metadata
    for model_data in &template.model_data {
        let x = model_data.x as usize + offset_x;
        let y = model_data.y as usize + offset_y;
        let z = model_data.z as usize + offset_z;

        if x < CHUNK_SIZE && y < CHUNK_SIZE && z < CHUNK_SIZE {
            let idx = x + y * CHUNK_SIZE + z * CHUNK_SIZE * CHUNK_SIZE;
            meta_buf[idx * 2] = model_data.model_id;
            meta_buf[idx * 2 + 1] = model_data.rotation;
        }
    }

    // Fill tint metadata (stored in rotation byte for tinted glass/crystal)
    for tint_data in &template.tint_data {
        let x = tint_data.x as usize + offset_x;
        let y = tint_data.y as usize + offset_y;
        let z = tint_data.z as usize + offset_z;

        if x < CHUNK_SIZE && y < CHUNK_SIZE && z < CHUNK_SIZE {
            let idx = x + y * CHUNK_SIZE + z * CHUNK_SIZE * CHUNK_SIZE;
            meta_buf[idx * 2 + 1] = tint_data.tint_index;
        }
    }

    // Fill paint metadata
    for paint_data in &template.paint_data {
        let x = paint_data.x as usize + offset_x;
        let y = paint_data.y as usize + offset_y;
        let z = paint_data.z as usize + offset_z;

        if x < CHUNK_SIZE && y < CHUNK_SIZE && z < CHUNK_SIZE {
            let idx = x + y * CHUNK_SIZE + z * CHUNK_SIZE * CHUNK_SIZE;
            meta_buf[idx * 2] = paint_data.texture_idx;
            meta_buf[idx * 2 + 1] = paint_data.tint_idx;
        }
    }

    upload_chunks_batched(
        memory_allocator,
        command_buffer_allocator,
        queue,
        voxel_image,
        model_metadata_image,
        Vector3::zeros(),
        &[(chunk_pos, &block_buf, &meta_buf)],
    );

    Ok(())
}

/// Calculate appropriate camera position for template based on its dimensions.
fn calculate_camera_position(template: &VxtFile) -> (Vector3<f64>, Vector3<f64>) {
    // Template is centered in the chunk
    let offset_x = (CHUNK_SIZE - template.width as usize) / 2;
    let offset_y = (CHUNK_SIZE - template.height as usize) / 2;
    let offset_z = (CHUNK_SIZE - template.depth as usize) / 2;

    // Center of the template
    let center_x = offset_x as f64 + template.width as f64 / 2.0;
    let center_y = offset_y as f64 + template.height as f64 / 2.0;
    let center_z = offset_z as f64 + template.depth as f64 / 2.0;

    let look_at = Vector3::new(center_x, center_y, center_z);

    // Calculate camera distance based on template size
    let max_dim = template.width.max(template.height).max(template.depth) as f64;
    let distance = (max_dim * 1.5).max(3.0); // At least 3 blocks away

    // Isometric 3/4 view
    let cam_world = look_at + Vector3::new(distance, distance, distance);

    (cam_world, look_at)
}

/// Build a pixel-to-ray matrix for the camera.
fn build_pixel_to_ray(
    cam_world: Vector3<f64>,
    target_world: Vector3<f64>,
    render_extent: [u32; 2],
    fov_deg: f64,
) -> Matrix4<f32> {
    let forward = {
        let dir = target_world - cam_world;
        let norm = dir.norm();
        if norm < 1e-6 {
            Vector3::new(0.0, 0.0, -1.0)
        } else {
            dir / norm
        }
    };
    let world_up = Vector3::new(0.0, 1.0, 0.0);
    let mut right = forward.cross(&world_up);
    if right.norm() < 1e-6 {
        right = Vector3::new(1.0, 0.0, 0.0);
    }
    right = right.normalize();
    let up = right.cross(&forward).normalize();

    let aspect = render_extent[0] as f64 / render_extent[1] as f64;
    let tan_fov = (fov_deg.to_radians() * 0.5).tan();

    let kx = (2.0 / render_extent[0] as f64) * aspect * tan_fov;
    let ky = (2.0 / render_extent[1] as f64) * -tan_fov;
    let x_bias = ((0.5 * (2.0 / render_extent[0] as f64)) - 1.0) * aspect * tan_fov;
    let y_bias = ((0.5 * (2.0 / render_extent[1] as f64)) - 1.0) * -tan_fov;

    let col0 = right * kx;
    let col1 = up * ky;
    let col2 = forward + right * x_bias + up * y_bias;

    let mut m = Matrix4::<f64>::identity();
    m.m11 = col0.x;
    m.m21 = col0.y;
    m.m31 = col0.z;

    m.m12 = col1.x;
    m.m22 = col1.y;
    m.m32 = col1.z;

    m.m13 = col2.x;
    m.m23 = col2.y;
    m.m33 = col2.z;

    m.m14 = cam_world.x;
    m.m24 = cam_world.y;
    m.m34 = cam_world.z;

    m.cast()
}

use crate::chunk::{BlockType, CHUNK_SIZE, CHUNK_VOLUME};
use crate::config::Args;
use crate::gpu_resources::{
    PushConstants, create_empty_voxel_texture, get_brick_and_model_set, get_chunk_metadata_set,
    get_distance_image_and_set, get_images_and_sets, get_light_set,
    get_particle_and_falling_block_set, load_texture_atlas, save_screenshot, upload_chunks_batched,
};
use crate::hot_reload::HotReloadComputePipeline;
use crate::render_mode::RenderMode;
use crate::sub_voxel::ModelRegistry;
use crate::vulkan_context::VulkanContext;
use image::{ImageBuffer, Rgba};
use nalgebra::{Matrix4, Vector3};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use vulkano::command_buffer::PrimaryCommandBufferAbstract;
use vulkano::command_buffer::allocator::StandardCommandBufferAllocator;
use vulkano::command_buffer::{AutoCommandBufferBuilder, ClearColorImageInfo, CommandBufferUsage};
use vulkano::descriptor_set::DescriptorSet;
use vulkano::device::Queue;
use vulkano::image::Image;
use vulkano::image::view::{ImageView, ImageViewCreateInfo};
use vulkano::memory::allocator::StandardMemoryAllocator;
use vulkano::pipeline::Pipeline;
use vulkano::sync::GpuFuture;
use winit::event_loop::EventLoop;

const ICON_SIZE: u32 = 64;
const ICON_WORLD_EXTENT: [u32; 3] = [CHUNK_SIZE as u32, CHUNK_SIZE as u32, CHUNK_SIZE as u32];

pub fn run(_args: &Args, event_loop: &EventLoop<()>) -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out_dir = root.join("textures").join("rendered");
    std::fs::create_dir_all(&out_dir)?;
    let missing_path = ensure_missing_texture(&out_dir)?;

    println!("[sprites] output: {}", out_dir.display());
    println!("[sprites] placeholder: {}", missing_path.display());

    let vk = VulkanContext::new(event_loop);
    let memory_allocator = vk.memory_allocator.clone();
    let descriptor_set_allocator = vk.descriptor_set_allocator.clone();
    let command_buffer_allocator = vk.command_buffer_allocator.clone();

    let shaders_dir = root.join("shaders");
    let render_pipeline =
        HotReloadComputePipeline::new(vk.device.clone(), &shaders_dir.join("traverse.comp"));
    let resample_pipeline =
        HotReloadComputePipeline::new(vk.device.clone(), &shaders_dir.join("resample.comp"));

    let world_extent = ICON_WORLD_EXTENT;
    let render_extent = [ICON_SIZE, ICON_SIZE];
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
        world_extent,
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

    let (_particle_buffer, _falling_block_buffer, particle_set) =
        get_particle_and_falling_block_set(
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
        // Mark all chunks as non-empty by clearing the bitset.
        let mut flags = chunk_meta_buffer.write().unwrap();
        for f in flags.iter_mut() {
            *f = 0;
        }
    }

    // Create model registry and load custom models from library
    let mut model_registry = ModelRegistry::new();
    let library_path = std::path::Path::new("user_models");
    match model_registry.load_library_models(library_path) {
        Ok(count) if count > 0 => {
            println!("[sprites] Loaded {} custom models from library", count);
        }
        Err(e) => {
            eprintln!("[sprites] Warning: Failed to load library models: {}", e);
        }
        _ => {}
    }

    let (
        brick_mask_buffer,
        _brick_dist_buffer,
        _model_atlas,
        _model_palettes,
        model_metadata_image,
        _model_properties_buffer,
        brick_and_model_set,
    ) = get_brick_and_model_set(
        memory_allocator.clone(),
        command_buffer_allocator.clone(),
        descriptor_set_allocator.clone(),
        &render_pipeline,
        &vk.queue,
        world_extent,
        &model_registry,
    );
    // Ensure brick masks mark bricks as non-empty to avoid skipping geometry during icon renders.
    {
        let mut masks = brick_mask_buffer.write().unwrap();
        masks.fill(u32::MAX);
    }

    // Image view needed for saving
    let render_image_view = ImageView::new(
        render_image.clone(),
        ImageViewCreateInfo::from_image(&render_image),
    )
    .unwrap();

    let blocks: [BlockType; 16] = [
        BlockType::Stone,
        BlockType::Dirt,
        BlockType::Grass,
        BlockType::Planks,
        BlockType::Leaves,
        BlockType::Sand,
        BlockType::Gravel,
        BlockType::Water,
        BlockType::Glass,
        BlockType::TintedGlass,
        BlockType::Log,
        BlockType::Brick,
        BlockType::Snow,
        BlockType::Cobblestone,
        BlockType::Iron,
        BlockType::Bedrock,
    ];

    for block in blocks {
        render_icon(
            IconTarget::Block(block),
            &vk.queue,
            &render_pipeline,
            &render_image,
            &render_set,
            &distance_image,
            &distance_set,
            &voxel_image,
            &voxel_set,
            &model_metadata_image,
            &texture_set,
            &particle_set,
            &light_set,
            &chunk_metadata_set,
            &brick_and_model_set,
            &model_registry,
            world_extent,
            render_extent,
            &render_image_view,
            &out_dir,
            &memory_allocator,
            &command_buffer_allocator,
        )?;
    }

    for model_id in 1u8..(model_registry.len() as u8) {
        render_icon(
            IconTarget::Model(model_id),
            &vk.queue,
            &render_pipeline,
            &render_image,
            &render_set,
            &distance_image,
            &distance_set,
            &voxel_image,
            &voxel_set,
            &model_metadata_image,
            &texture_set,
            &particle_set,
            &light_set,
            &chunk_metadata_set,
            &brick_and_model_set,
            &model_registry,
            world_extent,
            render_extent,
            &render_image_view,
            &out_dir,
            &memory_allocator,
            &command_buffer_allocator,
        )?;
    }

    println!("[sprites] done");
    Ok(())
}

fn ensure_missing_texture(out_dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let path = out_dir.join("missing.png");
    if path.exists() {
        return Ok(path);
    }

    let size = ICON_SIZE;
    let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(size, size);
    for y in 0..size {
        for x in 0..size {
            let check = ((x / 8) + (y / 8)) % 2 == 0;
            let color = if check {
                Rgba([255, 0, 255, 255])
            } else {
                Rgba([0, 0, 0, 255])
            };
            img.put_pixel(x, y, color);
        }
    }
    img.save(&path)?;
    Ok(path)
}

#[derive(Clone, Copy, Debug)]
enum IconTarget {
    Block(BlockType),
    Model(u8),
}

fn build_pixel_to_ray(
    cam_world: Vector3<f64>,
    target_world: Vector3<f64>,
    render_extent: [u32; 2],
    fov_deg: f64,
) -> Matrix4<f32> {
    // Build a simple pinhole camera that looks directly at the target.
    // Forward points towards the target, right is derived from world-up, and
    // up is orthogonalised. The resulting matrix matches the shader
    // expectation: origin = column3.xyz, direction = mat3(M) * vec3(x, y, 1).
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
        // Degenerate (looking straight up/down); pick arbitrary right.
        right = Vector3::new(1.0, 0.0, 0.0);
    }
    right = right.normalize();
    let up = right.cross(&forward).normalize();

    let aspect = render_extent[0] as f64 / render_extent[1] as f64;
    let tan_fov = (fov_deg.to_radians() * 0.5).tan();

    // Screen-space to view-space scale/bias so that pixelCoord maps to
    // NDC in [-1,1], with a half-pixel offset.
    let kx = (2.0 / render_extent[0] as f64) * aspect * tan_fov;
    let ky = (2.0 / render_extent[1] as f64) * -tan_fov; // y grows downwards in image space
    let x_bias = ((0.5 * (2.0 / render_extent[0] as f64)) - 1.0) * aspect * tan_fov;
    let y_bias = ((0.5 * (2.0 / render_extent[1] as f64)) - 1.0) * -tan_fov;

    let col0 = right * kx;
    let col1 = up * ky;
    let col2 = forward + right * x_bias + up * y_bias;

    let mut m = Matrix4::<f64>::identity();
    // Columns 0-2: ray direction construction
    m.m11 = col0.x;
    m.m21 = col0.y;
    m.m31 = col0.z;

    m.m12 = col1.x;
    m.m22 = col1.y;
    m.m32 = col1.z;

    m.m13 = col2.x;
    m.m23 = col2.y;
    m.m33 = col2.z;

    // Column 3: ray origin
    m.m14 = cam_world.x;
    m.m24 = cam_world.y;
    m.m34 = cam_world.z;

    m.cast()
}

#[allow(clippy::too_many_arguments)]
fn render_icon(
    target: IconTarget,
    queue: &Arc<Queue>,
    render_pipeline: &HotReloadComputePipeline,
    render_image: &Arc<Image>,
    render_set: &Arc<DescriptorSet>,
    distance_image: &Arc<Image>,
    distance_set: &Arc<DescriptorSet>,
    voxel_image: &Arc<Image>,
    voxel_set: &Arc<DescriptorSet>,
    model_metadata_image: &Arc<Image>,
    texture_set: &Arc<DescriptorSet>,
    particle_set: &Arc<DescriptorSet>,
    light_set: &Arc<DescriptorSet>,
    chunk_metadata_set: &Arc<DescriptorSet>,
    brick_and_model_set: &Arc<DescriptorSet>,
    _model_registry: &ModelRegistry,
    world_extent: [u32; 3],
    render_extent: [u32; 2],
    render_image_view: &Arc<vulkano::image::view::ImageView>,
    out_dir: &Path,
    memory_allocator: &Arc<StandardMemoryAllocator>,
    command_buffer_allocator: &Arc<StandardCommandBufferAllocator>,
) -> Result<(), Box<dyn Error>> {
    let block_type = match target {
        IconTarget::Block(b) => b,
        IconTarget::Model(_) => BlockType::Model,
    };
    let model_id = match target {
        IconTarget::Model(id) => Some(id),
        _ => None,
    };

    // Clear voxel + metadata + render targets
    {
        let mut builder = AutoCommandBufferBuilder::primary(
            command_buffer_allocator.clone(),
            queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )?;

        builder
            .clear_color_image(ClearColorImageInfo::image(render_image.clone()))
            .unwrap();
        builder
            .clear_color_image(ClearColorImageInfo::image(voxel_image.clone()))
            .unwrap();
        builder
            .clear_color_image(ClearColorImageInfo::image(model_metadata_image.clone()))
            .unwrap();
        builder
            .clear_color_image(ClearColorImageInfo::image(distance_image.clone()))
            .unwrap();

        builder
            .build()?
            .execute(queue.clone())?
            .then_signal_fence_and_flush()?
            .wait(None)?;
    }

    // Upload a single-block chunk at origin (small icon world = one chunk)
    let chunk_pos = Vector3::new(0, 0, 0);
    let local_pos = (CHUNK_SIZE / 2, CHUNK_SIZE / 2, CHUNK_SIZE / 2);
    let mut block_buf = vec![0u8; CHUNK_VOLUME];
    let idx = local_pos.0 + local_pos.1 * CHUNK_SIZE + local_pos.2 * CHUNK_SIZE * CHUNK_SIZE;
    block_buf[idx] = block_type as u8;

    // For glass blocks, add a backing block behind to make transparency visible
    // Camera is at block_center + (2, 2, 2), so backing is at (-1, -1, -1) offset
    if matches!(block_type, BlockType::Glass | BlockType::TintedGlass) {
        let back_pos = (local_pos.0 - 1, local_pos.1 - 1, local_pos.2 - 1);
        let back_idx = back_pos.0 + back_pos.1 * CHUNK_SIZE + back_pos.2 * CHUNK_SIZE * CHUNK_SIZE;
        block_buf[back_idx] = BlockType::Stone as u8;
    }

    let mut meta_buf = vec![0u8; CHUNK_VOLUME * 2];
    if let Some(id) = model_id {
        meta_buf[idx * 2] = id;
        meta_buf[idx * 2 + 1] = 0; // rotation 0
    }
    // For tinted glass, set a visible tint index (e.g., red = 0)
    if block_type == BlockType::TintedGlass {
        meta_buf[idx * 2 + 1] = 0; // tint index 0 = red
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

    let block_center = Vector3::new(
        (chunk_pos.x * CHUNK_SIZE as i32 + local_pos.0 as i32) as f64 + 0.5,
        (chunk_pos.y * CHUNK_SIZE as i32 + local_pos.1 as i32) as f64 + 0.5,
        (chunk_pos.z * CHUNK_SIZE as i32 + local_pos.2 as i32) as f64 + 0.5,
    );
    // 3/4 view: camera offset diagonally and above the block.
    let cam_world = block_center + Vector3::new(2.0, 2.0, 2.0);
    let pixel_to_ray = build_pixel_to_ray(cam_world, block_center, render_extent, 35.0);

    let push_constants = PushConstants {
        pixel_to_ray,
        texture_size_x: world_extent[0],
        texture_size_y: world_extent[1],
        texture_size_z: world_extent[2],
        render_mode: RenderMode::Textured as u32,
        show_chunk_boundaries: 0,
        player_in_water: 0,
        time_of_day: 14.0 / 24.0, // 14:00 requested lighting
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
        _padding: 0,
        camera_pos: [
            cam_world.x as f32,
            cam_world.y as f32,
            cam_world.z as f32,
            0.0,
        ],
    };

    // Render
    {
        let pipeline = Arc::clone(render_pipeline);
        let mut builder = AutoCommandBufferBuilder::primary(
            command_buffer_allocator.clone(),
            queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )?;

        builder
            .bind_pipeline_compute(pipeline.clone())
            .unwrap()
            .push_constants(pipeline.layout().clone(), 0, push_constants)
            .unwrap()
            .bind_descriptor_sets(
                vulkano::pipeline::PipelineBindPoint::Compute,
                pipeline.layout().clone(),
                0,
                vec![
                    render_set.clone(),
                    voxel_set.clone(),
                    texture_set.clone(),
                    particle_set.clone(),
                    light_set.clone(),
                    chunk_metadata_set.clone(),
                    distance_set.clone(),
                    brick_and_model_set.clone(),
                ],
            )
            .unwrap();
        unsafe {
            builder.dispatch([
                render_extent[0].div_ceil(8),
                render_extent[1].div_ceil(8),
                1,
            ])?;
        }

        builder
            .build()?
            .execute(queue.clone())?
            .then_signal_fence_and_flush()?
            .wait(None)?;
    }

    // Save
    let filename = match target {
        IconTarget::Block(b) => format!("block_{}.png", format!("{:?}", b).to_ascii_lowercase()),
        IconTarget::Model(id) => format!("model_{}.png", id),
    };
    let path = out_dir.join(filename);
    save_screenshot(
        queue.device(),
        queue,
        memory_allocator,
        command_buffer_allocator,
        render_image_view,
        path.to_str().unwrap(),
    );

    Ok(())
}

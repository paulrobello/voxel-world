//! Swapchain and render-target construction, plus window icon loading and
//! screenshot capture. Covers `get_swapchain_images`, the render / resample /
//! distance image + descriptor-set factories, `create_empty_voxel_texture`,
//! `load_icon`, and `save_screenshot`.

use super::*;

use std::sync::Arc;

use vulkano::{
    buffer::{Buffer, BufferCreateInfo, BufferUsage},
    command_buffer::{
        AutoCommandBufferBuilder, ClearColorImageInfo, CommandBufferUsage,
        allocator::StandardCommandBufferAllocator,
    },
    descriptor_set::{
        DescriptorSet, WriteDescriptorSet, allocator::StandardDescriptorSetAllocator,
    },
    device::{Device, Queue},
    format::Format,
    image::{
        Image, ImageCreateInfo, ImageType, ImageUsage,
        sampler::Sampler,
        view::{ImageView, ImageViewCreateInfo},
    },
    memory::allocator::{AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator},
    pipeline::ComputePipeline,
    swapchain::{PresentMode, Surface, Swapchain, SwapchainCreateInfo},
};
use winit::window::{Icon, Window};

pub fn get_swapchain_images(
    device: &Arc<Device>,
    surface: &Arc<Surface>,
    window: &Window,
) -> (Arc<Swapchain>, Vec<Arc<Image>>) {
    let caps = device
        .physical_device()
        .surface_capabilities(surface, Default::default())
        .expect("Failed to query surface capabilities");

    let image_format = device
        .physical_device()
        .surface_formats(surface, Default::default())
        .expect("Failed to query surface formats")[0]
        .0;

    let composite_alpha = caps
        .supported_composite_alpha
        .into_iter()
        .next()
        .expect("No composite alpha mode supported");

    Swapchain::new(
        device.clone(),
        surface.clone(),
        SwapchainCreateInfo {
            min_image_count: caps.min_image_count.max(3),
            image_format,
            image_extent: window.inner_size().into(),
            image_usage: ImageUsage::COLOR_ATTACHMENT
                | ImageUsage::TRANSFER_DST
                | ImageUsage::TRANSFER_SRC,
            composite_alpha,
            present_mode: PresentMode::Immediate,
            ..Default::default()
        },
    )
    .expect("Failed to create swapchain")
}

pub fn get_render_image(
    memory_allocator: Arc<StandardMemoryAllocator>,
    extent: [u32; 2],
) -> (Arc<Image>, Arc<ImageView>) {
    let image = Image::new(
        memory_allocator,
        ImageCreateInfo {
            usage: ImageUsage::STORAGE | ImageUsage::TRANSFER_DST | ImageUsage::TRANSFER_SRC,
            format: Format::R8G8B8A8_UNORM,
            extent: [extent[0], extent[1], 1],
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        },
    )
    .unwrap();

    let image_view =
        ImageView::new(image.clone(), ImageViewCreateInfo::from_image(&image)).unwrap();

    (image, image_view)
}

pub fn get_resample_image(
    memory_allocator: Arc<StandardMemoryAllocator>,
    extent: [u32; 2],
) -> (Arc<Image>, Arc<ImageView>) {
    let image = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            usage: ImageUsage::STORAGE | ImageUsage::TRANSFER_SRC,
            format: Format::R8G8B8A8_UNORM,
            extent: [extent[0], extent[1], 1],
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        },
    )
    .unwrap();

    let image_view =
        ImageView::new(image.clone(), ImageViewCreateInfo::from_image(&image)).unwrap();

    (image, image_view)
}

pub fn get_images_and_sets(
    memory_allocator: Arc<StandardMemoryAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
    resample_pipeline: &ComputePipeline,
    render_extent: [u32; 2],
    window_extent: [u32; 2],
    multiplayer_texture_array: Option<(Arc<ImageView>, Arc<Sampler>, u32)>,
) -> (
    Arc<Image>,
    Arc<DescriptorSet>,
    Arc<Image>,
    Arc<DescriptorSet>,
) {
    let (render_image, render_image_view) =
        get_render_image(memory_allocator.clone(), render_extent);

    // Create render set with optional multiplayer texture array at binding 10
    let render_set = if let Some((texture_view, sampler, _count)) = multiplayer_texture_array {
        make_set(
            &descriptor_set_allocator,
            render_pipeline,
            0,
            [
                WriteDescriptorSet::image_view(0, render_image_view.clone()),
                WriteDescriptorSet::image_view_sampler(10, texture_view, sampler),
            ],
        )
    } else {
        make_set(
            &descriptor_set_allocator,
            render_pipeline,
            0,
            [WriteDescriptorSet::image_view(0, render_image_view.clone())],
        )
    };

    let (resample_image, resample_image_view) = get_resample_image(memory_allocator, window_extent);

    let resample_set = make_set(
        &descriptor_set_allocator,
        resample_pipeline,
        0,
        [
            WriteDescriptorSet::image_view(0, render_image_view.clone()),
            WriteDescriptorSet::image_view(1, resample_image_view.clone()),
        ],
    );

    (render_image, render_set, resample_image, resample_set)
}

/// Creates a distance buffer for two-pass beam optimization.
/// The distance buffer is at 1/4 of render resolution and stores hit distances.
pub fn get_distance_image_and_set(
    memory_allocator: Arc<StandardMemoryAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
    render_extent: [u32; 2],
) -> (Arc<Image>, Arc<DescriptorSet>) {
    // Distance buffer at 1/4 resolution (1/16 the pixels)
    let distance_extent = [(render_extent[0] / 4).max(1), (render_extent[1] / 4).max(1)];

    let distance_image = Image::new(
        memory_allocator,
        ImageCreateInfo {
            image_type: ImageType::Dim2d,
            usage: ImageUsage::STORAGE | ImageUsage::TRANSFER_DST,
            format: Format::R32_SFLOAT,
            extent: [distance_extent[0], distance_extent[1], 1],
            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_DEVICE,
            ..Default::default()
        },
    )
    .unwrap();

    let distance_image_view = ImageView::new(
        distance_image.clone(),
        ImageViewCreateInfo::from_image(&distance_image),
    )
    .unwrap();

    let distance_set = make_set(
        &descriptor_set_allocator,
        render_pipeline,
        6,
        [WriteDescriptorSet::image_view(0, distance_image_view)],
    );

    (distance_image, distance_set)
}

pub fn create_empty_voxel_texture(
    memory_allocator: Arc<StandardMemoryAllocator>,
    command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
    descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
    render_pipeline: &ComputePipeline,
    queue: &Arc<Queue>,
    world_extent: [u32; 3],
) -> (Arc<DescriptorSet>, Arc<Image>) {
    // Create 3D texture sized to fit entire world
    let image = Image::new(
        memory_allocator.clone(),
        ImageCreateInfo {
            image_type: ImageType::Dim3d,
            format: Format::R8_UINT,
            extent: world_extent,
            usage: ImageUsage::STORAGE | ImageUsage::TRANSFER_DST,
            ..Default::default()
        },
        AllocationCreateInfo::default(),
    )
    .unwrap();

    // Clear the image to all zeros (air)
    let mut command_buffer_builder = AutoCommandBufferBuilder::primary(
        command_buffer_allocator.clone(),
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    command_buffer_builder
        .clear_color_image(ClearColorImageInfo::image(image.clone()))
        .unwrap();

    command_buffer_builder
        .build()
        .unwrap()
        .execute(queue.clone())
        .unwrap()
        .then_signal_fence_and_flush()
        .unwrap()
        .wait(None)
        .unwrap();

    let image_view =
        ImageView::new(image.clone(), ImageViewCreateInfo::from_image(&image)).unwrap();

    let descriptor_set = make_set(
        &descriptor_set_allocator,
        render_pipeline,
        1,
        [WriteDescriptorSet::image_view(0, image_view)],
    );

    (descriptor_set, image)
}

pub fn load_icon(icon: &[u8]) -> Icon {
    let (icon_rgba, icon_width, icon_height) = {
        let image = image::load_from_memory(icon)
            .expect("Failed to decode icon image")
            .to_rgba8();
        let (width, height) = image.dimensions();
        let rgba = image.into_raw();
        (rgba, width, height)
    };
    Icon::from_rgba(icon_rgba, icon_width, icon_height).expect("Failed to create window icon")
}

pub fn save_screenshot(
    device: &Arc<Device>,

    queue: &Arc<Queue>,

    memory_allocator: &Arc<StandardMemoryAllocator>,

    command_buffer_allocator: &Arc<StandardCommandBufferAllocator>,

    image_view: &Arc<ImageView>,

    path: &str,
) {
    let image = image_view.image();

    let extent = image.extent();

    // Create a buffer to copy the image data into

    let buffer_size = (extent[0] * extent[1] * 4) as u64; // RGBA

    let staging_buffer = Buffer::new_slice::<u8>(
        memory_allocator.clone(),
        BufferCreateInfo {
            usage: BufferUsage::TRANSFER_DST,

            ..Default::default()
        },
        AllocationCreateInfo {
            memory_type_filter: MemoryTypeFilter::PREFER_HOST
                | MemoryTypeFilter::HOST_RANDOM_ACCESS,

            ..Default::default()
        },
        buffer_size,
    )
    .expect("Failed to create screenshot staging buffer");

    // Build command buffer to copy image to buffer

    let mut builder = AutoCommandBufferBuilder::primary(
        command_buffer_allocator.clone(),
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    builder
        .copy_image_to_buffer(
            vulkano::command_buffer::CopyImageToBufferInfo::image_buffer(
                image.clone(),
                staging_buffer.clone(),
            ),
        )
        .unwrap();

    let command_buffer = builder.build().unwrap();

    // Execute and wait

    let future = vulkano::sync::now(device.clone())
        .then_execute(queue.clone(), command_buffer)
        .unwrap()
        .then_signal_fence_and_flush()
        .unwrap();

    future.wait(None).unwrap();

    // Read the buffer data

    let buffer_content = staging_buffer.read().unwrap();

    // Create image and save

    let img = image::RgbaImage::from_raw(extent[0], extent[1], buffer_content.to_vec())
        .expect("Failed to create image from buffer");

    img.save(path).expect("Failed to save screenshot");

    log::debug!("[SCREENSHOT] Saved to {}", path);
}

use crate::utils::get_allocators;
use std::sync::Arc;
use vulkano::{
    Version, VulkanLibrary,
    device::{
        Device, DeviceCreateInfo, DeviceExtensions, DeviceFeatures, Queue, QueueCreateInfo,
        QueueFlags, physical::PhysicalDeviceType,
    },
    instance::{Instance, InstanceCreateFlags, InstanceCreateInfo},
    swapchain::Surface,
};
use winit::event_loop::EventLoop;

pub struct VulkanContext {
    pub instance: Arc<Instance>,
    pub device: Arc<Device>,
    /// Primary graphics queue (also used for compute and presentation).
    pub queue: Arc<Queue>,
    /// Dedicated transfer queue for async DMA uploads (beneficial on discrete GPUs).
    /// Falls back to graphics queue on unified memory architectures.
    pub transfer_queue: Arc<Queue>,
    /// Queue family index of the transfer queue (for ownership transfers).
    pub transfer_queue_family: u32,
    /// Queue family index of the graphics queue.
    pub graphics_queue_family: u32,
    /// Whether transfer and graphics queues are from different families
    /// (requires ownership transfers on discrete GPUs).
    pub separate_transfer_queue: bool,
    pub memory_allocator: Arc<vulkano::memory::allocator::StandardMemoryAllocator>,
    pub descriptor_set_allocator:
        Arc<vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator>,
    pub command_buffer_allocator:
        Arc<vulkano::command_buffer::allocator::StandardCommandBufferAllocator>,
}

impl VulkanContext {
    pub fn new(event_loop: &EventLoop<()>) -> Self {
        let library = VulkanLibrary::new().unwrap();
        let mut required_extensions = Surface::required_extensions(event_loop).unwrap();
        required_extensions.ext_debug_utils = true;

        let instance = Instance::new(
            library,
            InstanceCreateInfo {
                flags: InstanceCreateFlags::ENUMERATE_PORTABILITY,
                enabled_extensions: required_extensions,
                ..Default::default()
            },
        )
        .unwrap();

        let mut device_extensions = DeviceExtensions {
            khr_swapchain: true,
            khr_portability_subset: true,
            ..DeviceExtensions::empty()
        };

        // Find device with graphics queue, prefer discrete GPUs
        let (physical_device, graphics_queue_family) = instance
            .enumerate_physical_devices()
            .unwrap()
            .filter(|p| {
                p.api_version() >= Version::V1_3 || p.supported_extensions().khr_dynamic_rendering
            })
            .filter(|p| p.supported_extensions().contains(&device_extensions))
            .filter_map(|p| {
                p.queue_family_properties()
                    .iter()
                    .enumerate()
                    .position(|(i, q)| {
                        q.queue_flags.intersects(QueueFlags::GRAPHICS)
                            && p.presentation_support(i as u32, event_loop).unwrap()
                    })
                    .map(|i| (p, i as u32))
            })
            .min_by_key(|(p, _)| match p.properties().device_type {
                PhysicalDeviceType::DiscreteGpu => 0,
                PhysicalDeviceType::IntegratedGpu => 1,
                PhysicalDeviceType::VirtualGpu => 2,
                PhysicalDeviceType::Cpu => 3,
                PhysicalDeviceType::Other => 4,
                _ => 5,
            })
            .unwrap();

        // Find a dedicated transfer queue family (TRANSFER but not GRAPHICS/COMPUTE).
        // This is beneficial on discrete GPUs where DMA engine can run in parallel.
        // Falls back to graphics queue on unified memory architectures.
        let transfer_queue_family = physical_device
            .queue_family_properties()
            .iter()
            .enumerate()
            .find(|(i, q)| {
                // Look for transfer-only queue (dedicated DMA engine)
                q.queue_flags.intersects(QueueFlags::TRANSFER)
                    && !q.queue_flags.intersects(QueueFlags::GRAPHICS)
                    && !q.queue_flags.intersects(QueueFlags::COMPUTE)
                    && *i as u32 != graphics_queue_family
            })
            .or_else(|| {
                // Fallback: any transfer-capable queue that's not the graphics queue
                physical_device
                    .queue_family_properties()
                    .iter()
                    .enumerate()
                    .find(|(i, q)| {
                        q.queue_flags.intersects(QueueFlags::TRANSFER)
                            && *i as u32 != graphics_queue_family
                    })
            })
            .map(|(i, _)| i as u32)
            .unwrap_or(graphics_queue_family);

        let separate_transfer_queue = transfer_queue_family != graphics_queue_family;

        // Log queue configuration
        let device_name = physical_device.properties().device_name.clone();
        let device_type = physical_device.properties().device_type;
        eprintln!("Vulkan device: {} ({:?})", device_name, device_type);
        eprintln!(
            "Queue families: graphics={}, transfer={} (separate={})",
            graphics_queue_family, transfer_queue_family, separate_transfer_queue
        );

        if physical_device.api_version() < Version::V1_3 {
            device_extensions.khr_dynamic_rendering = true;
        }

        // Create queue infos for both graphics and transfer queues
        let mut queue_create_infos = vec![QueueCreateInfo {
            queue_family_index: graphics_queue_family,
            ..Default::default()
        }];

        // Only add separate transfer queue info if it's a different family
        if separate_transfer_queue {
            queue_create_infos.push(QueueCreateInfo {
                queue_family_index: transfer_queue_family,
                ..Default::default()
            });
        }

        let (device, mut queues) = Device::new(
            physical_device,
            DeviceCreateInfo {
                queue_create_infos,
                enabled_extensions: device_extensions,
                enabled_features: DeviceFeatures {
                    dynamic_rendering: true,
                    image_view_format_swizzle: true,
                    ..DeviceFeatures::empty()
                },
                ..Default::default()
            },
        )
        .unwrap();

        let graphics_queue = queues.next().unwrap();
        // Use separate transfer queue if available, otherwise share graphics queue
        let transfer_queue = if separate_transfer_queue {
            queues.next().unwrap()
        } else {
            graphics_queue.clone()
        };
        let (memory_allocator, descriptor_set_allocator, command_buffer_allocator) =
            get_allocators(&device);

        Self {
            instance,
            device,
            queue: graphics_queue,
            transfer_queue,
            transfer_queue_family,
            graphics_queue_family,
            separate_transfer_queue,
            memory_allocator,
            descriptor_set_allocator,
            command_buffer_allocator,
        }
    }
}

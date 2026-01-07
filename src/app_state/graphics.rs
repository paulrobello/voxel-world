use std::sync::Arc;

use vulkano::command_buffer::allocator::StandardCommandBufferAllocator;
use vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator as StdDescriptorSetAllocator;
use vulkano::{
    buffer::Subbuffer,
    descriptor_set::DescriptorSet,
    device::{Device, Queue},
    image::{Image, view::ImageView},
    instance::Instance,
    memory::allocator::StandardMemoryAllocator,
};

use crate::falling_block::GpuFallingBlock;
use crate::gpu_resources::{self, GpuLight};
use crate::hot_reload::HotReloadComputePipeline;
use crate::particles;

pub struct Graphics {
    pub instance: Arc<Instance>,
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,

    pub memory_allocator: Arc<StandardMemoryAllocator>,
    pub descriptor_set_allocator: Arc<StdDescriptorSetAllocator>,
    pub command_buffer_allocator: Arc<StandardCommandBufferAllocator>,

    pub render_pipeline: HotReloadComputePipeline,
    pub resample_pipeline: HotReloadComputePipeline,

    pub voxel_set: Arc<DescriptorSet>,
    pub texture_set: Arc<DescriptorSet>,
    pub texture_atlas_view: Arc<ImageView>,

    pub particle_buffer: Subbuffer<[particles::GpuParticle]>,
    pub particle_set: Arc<DescriptorSet>,
    pub light_buffer: Subbuffer<[GpuLight]>,
    pub light_set: Arc<DescriptorSet>,
    pub chunk_metadata_buffer: Subbuffer<[u32]>,
    pub chunk_metadata_set: Arc<DescriptorSet>,
    pub brick_mask_buffer: Subbuffer<[u32]>,
    pub brick_dist_buffer: Subbuffer<[u32]>,
    pub brick_and_model_set: Arc<DescriptorSet>,
    pub falling_block_buffer: Subbuffer<[GpuFallingBlock]>,
    pub water_source_buffer: Subbuffer<[gpu_resources::GpuWaterSource]>,
    pub voxel_image: Arc<Image>,
    pub model_atlas_8: Arc<Image>,
    pub model_atlas_16: Arc<Image>,
    pub model_atlas_32: Arc<Image>,
    pub model_palettes: Arc<Image>,
    pub model_palette_emission: Arc<Image>,
    pub model_metadata: Arc<Image>,
    pub model_properties_buffer: Subbuffer<[gpu_resources::GpuModelProperties]>,

    pub rcx: Option<gpu_resources::RenderContext>,
}

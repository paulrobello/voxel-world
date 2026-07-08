//! Lighting and simulation GPU buffers: water sources, template/stencil
//! debug blocks, the per-frame simulation descriptor set (particles, falling
//! blocks, remote players), and the point-light buffer + descriptor set.

use super::*;
use crate::falling_block::{GpuFallingBlock, MAX_FALLING_BLOCKS};
use crate::particles;
use crate::remote_player::{GpuRemotePlayer, MAX_REMOTE_PLAYERS};
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

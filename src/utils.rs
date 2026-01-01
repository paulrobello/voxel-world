/// Statistics about loaded chunks for HUD display.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChunkStats {
    /// Number of chunks currently loaded in memory.
    pub loaded_count: usize,
    /// Number of chunks with pending GPU uploads.
    pub dirty_count: usize,
    /// Number of chunks being generated in background.
    pub in_flight_count: usize,
    /// Estimated GPU memory usage in megabytes.
    pub memory_mb: f32,
}

/// Performance profiler for tracking operation timings.
#[derive(Debug, Default)]
pub struct Profiler {
    /// Accumulated time for chunk loading/streaming (microseconds).
    pub chunk_loading_us: u64,
    /// Accumulated time for GPU uploads (microseconds).
    pub gpu_upload_us: u64,
    /// Accumulated time for metadata updates (microseconds).
    pub metadata_update_us: u64,
    /// Accumulated time for rendering (microseconds).
    pub render_us: u64,
    /// Number of samples accumulated.
    pub sample_count: u32,
    /// Number of chunks uploaded this period.
    pub chunks_uploaded: u32,
}

impl Profiler {
    pub fn reset(&mut self) {
        self.chunk_loading_us = 0;
        self.gpu_upload_us = 0;
        self.metadata_update_us = 0;
        self.render_us = 0;
        self.sample_count = 0;
        self.chunks_uploaded = 0;
    }

    pub fn print_stats(&self) {
        if self.sample_count == 0 {
            return;
        }
        let n = self.sample_count as f64;
        println!(
            "[PROFILE] ChunkLoad: {:.2}ms | Upload: {:.2}ms ({} chunks) | Metadata: {:.2}ms | Render: {:.2}ms",
            self.chunk_loading_us as f64 / 1000.0 / n,
            self.gpu_upload_us as f64 / 1000.0 / n,
            self.chunks_uploaded,
            self.metadata_update_us as f64 / 1000.0 / n,
            self.render_us as f64 / 1000.0 / n,
        );
    }
}

pub fn get_allocators(
    device: &std::sync::Arc<vulkano::device::Device>,
) -> (
    std::sync::Arc<vulkano::memory::allocator::StandardMemoryAllocator>,
    std::sync::Arc<vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator>,
    std::sync::Arc<vulkano::command_buffer::allocator::StandardCommandBufferAllocator>,
) {
    let memory_allocator = std::sync::Arc::new(
        vulkano::memory::allocator::StandardMemoryAllocator::new_default(device.clone()),
    );
    let descriptor_set_allocator = std::sync::Arc::new(
        vulkano::descriptor_set::allocator::StandardDescriptorSetAllocator::new(
            device.clone(),
            Default::default(),
        ),
    );
    let command_buffer_allocator = std::sync::Arc::new(
        vulkano::command_buffer::allocator::StandardCommandBufferAllocator::new(
            device.clone(),
            Default::default(),
        ),
    );
    (
        memory_allocator,
        descriptor_set_allocator,
        command_buffer_allocator,
    )
}

/// Returns true if a Y coordinate is within world bounds.
#[inline]
pub fn y_in_bounds(y: i32) -> bool {
    y >= 0 && y < crate::constants::TEXTURE_SIZE_Y as i32
}

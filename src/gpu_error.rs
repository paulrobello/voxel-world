//! GPU error types for runtime recovery from device loss and other Vulkan errors.
//!
//! Initialization failures (startup panics) are intentionally NOT covered here —
//! those use `.expect()` and should terminate the process. This module handles
//! errors that can occur during the active render loop where world data should
//! be saved before exiting.

use vulkano::{Validated, VulkanError, command_buffer::CommandBufferExecError};

/// Errors that can occur during the render loop at runtime.
#[derive(Debug)]
pub enum GpuError {
    /// The Vulkan device was lost (GPU crash, driver reset, disconnection).
    DeviceLost(String),
    /// The swapchain is out of date and must be recreated.
    SwapchainOutOfDate,
    /// Any other Vulkan runtime error that is not device loss.
    Other(String),
}

impl std::fmt::Display for GpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GpuError::DeviceLost(msg) => write!(f, "GPU device lost: {msg}"),
            GpuError::SwapchainOutOfDate => write!(f, "Swapchain out of date"),
            GpuError::Other(msg) => write!(f, "GPU error: {msg}"),
        }
    }
}

impl std::error::Error for GpuError {}

impl From<Validated<VulkanError>> for GpuError {
    fn from(e: Validated<VulkanError>) -> Self {
        GpuError::from(e.unwrap())
    }
}

impl From<VulkanError> for GpuError {
    fn from(e: VulkanError) -> Self {
        match e {
            VulkanError::DeviceLost => GpuError::DeviceLost(e.to_string()),
            VulkanError::OutOfDate => GpuError::SwapchainOutOfDate,
            other => GpuError::Other(other.to_string()),
        }
    }
}

impl From<Box<vulkano::ValidationError>> for GpuError {
    fn from(e: Box<vulkano::ValidationError>) -> Self {
        GpuError::Other(e.to_string())
    }
}

impl From<CommandBufferExecError> for GpuError {
    fn from(e: CommandBufferExecError) -> Self {
        GpuError::Other(e.to_string())
    }
}

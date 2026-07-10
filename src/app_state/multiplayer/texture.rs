//! Client custom-texture state, extracted from `MultiplayerState`
//! (ARC-002 phase 5).
//!
//! Owns the client-side custom-texture cache + the one-shot "GPU textures need
//! initializing" flag. On `ConnectionAccepted` the host tells the client how
//! many custom-texture slots exist; the cache is rebuilt and the GPU-init flag
//! is armed. Received textures are stored here for the render loop to upload.

use crate::net::CustomTextureCache;

/// Client custom-texture cache + the pending GPU-init flag.
///
/// Extracted from `MultiplayerState` (ARC-002). The host holds this as
/// `textures: TextureState` and forwards the public accessors.
pub struct TextureState {
    cache: CustomTextureCache,
    pending_gpu_init: Option<u8>,
}

impl TextureState {
    /// Creates texture state sized for `slot_count` slots (0 at construction;
    /// resized on `ConnectionAccepted`).
    pub fn new(slot_count: u8) -> Self {
        Self {
            cache: CustomTextureCache::new(slot_count),
            pending_gpu_init: None,
        }
    }

    /// Called on `ConnectionAccepted`: rebuilds the cache for the host's slot
    /// count and (if any) arms the GPU-init flag.
    pub fn on_connect(&mut self, slot_count: u8) {
        self.cache = CustomTextureCache::new(slot_count);
        if slot_count > 0 {
            self.pending_gpu_init = Some(slot_count);
        }
    }

    /// Returns the texture cache for rendering.
    pub fn cache(&self) -> &CustomTextureCache {
        &self.cache
    }

    /// Returns a mutable reference to the texture cache (for storing received
    /// textures + GPU uploads).
    pub fn cache_mut(&mut self) -> &mut CustomTextureCache {
        &mut self.cache
    }

    /// Takes the pending GPU-init flag, if armed.
    pub fn take_pending_gpu_init(&mut self) -> Option<u8> {
        self.pending_gpu_init.take()
    }
}

//! Chunk-upload staging machinery: the async transfer ring buffer and the
//! thread-local staging-buffer pool consumed by the per-frame upload path
//! (`upload_chunks_batched` and friends).

use std::cell::RefCell;
use std::sync::Arc;

use nalgebra::Vector3;
use vulkano::{
    buffer::{Buffer, BufferCreateInfo, BufferUsage, Subbuffer},
    memory::allocator::{AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator},
    sync::future::{FenceSignalFuture, NowFuture},
};

/// Type alias for the fence future returned by chunk upload commands.
/// CommandBuffer execute -> then_signal_fence_and_flush produces this type.
pub(crate) type ChunkTransferFence =
    FenceSignalFuture<vulkano::command_buffer::CommandBufferExecFuture<NowFuture>>;

/// Type alias for a triple of staging buffers (block data + model metadata + custom data).
pub(crate) type StagingBufferPair = (Subbuffer<[u8]>, Subbuffer<[u8]>, Subbuffer<[u8]>);

/// One chunk's worth of upload data: position + block bytes + model-metadata bytes + custom-data bytes.
pub type ChunkDataSlice<'a> = (Vector3<i32>, &'a [u8], &'a [u8], &'a [u8]);

/// A slot in the transfer ring buffer holding an in-flight transfer's fence and staging buffers.
pub(crate) struct TransferSlot {
    fence: ChunkTransferFence,
    block_staging: Subbuffer<[u8]>,
    meta_staging: Subbuffer<[u8]>,
    custom_staging: Subbuffer<[u8]>,
}

impl TransferSlot {
    /// Assembles a slot from the fence of a submitted transfer and its three
    /// staging buffers. Fields stay private so only the staging module
    /// constructs slots.
    pub(crate) fn new(
        fence: ChunkTransferFence,
        block_staging: Subbuffer<[u8]>,
        meta_staging: Subbuffer<[u8]>,
        custom_staging: Subbuffer<[u8]>,
    ) -> Self {
        Self {
            fence,
            block_staging,
            meta_staging,
            custom_staging,
        }
    }
}

/// Ring buffer for async GPU chunk uploads.
/// Tracks N in-flight transfers, allowing CPU and GPU to work in parallel.
/// Only blocks when all slots are busy (rare with 3+ slots).
pub struct TransferRingBuffer {
    slots: Vec<Option<TransferSlot>>,
    capacity: usize,
}

impl TransferRingBuffer {
    /// Creates a new ring buffer with the specified number of slots.
    /// More slots = less blocking, but more staging memory usage.
    pub fn new(capacity: usize) -> Self {
        let mut slots = Vec::with_capacity(capacity);
        slots.resize_with(capacity, || None);
        Self { slots, capacity }
    }

    /// Polls all slots and reclaims completed transfers.
    /// Returns staging buffers from completed slots (block, meta) for reuse.
    pub fn poll_completed(&mut self) -> Vec<StagingBufferPair> {
        let mut reclaimed = Vec::new();

        for slot in &mut self.slots {
            if let Some(transfer) = slot.as_ref() {
                // Poll the fence without blocking (is_signaled returns immediately)
                if transfer.fence.is_signaled().unwrap_or(false) {
                    // Transfer completed, reclaim staging buffers
                    let transfer = slot
                        .take()
                        .expect("Transfer slot was Some but take() returned None");
                    reclaimed.push((
                        transfer.block_staging,
                        transfer.meta_staging,
                        transfer.custom_staging,
                    ));
                }
            }
        }

        reclaimed
    }

    /// Finds an empty slot or waits for the oldest transfer to complete.
    /// Returns the slot index and any reclaimed staging buffers.
    pub fn acquire_slot(&mut self) -> (usize, Vec<StagingBufferPair>) {
        // First, poll all slots to reclaim completed transfers
        let mut reclaimed = self.poll_completed();

        // Find first empty slot
        for (i, slot) in self.slots.iter().enumerate() {
            if slot.is_none() {
                return (i, reclaimed);
            }
        }

        // All slots busy - must wait for the oldest (slot 0)
        // Rotate the ring: wait on slot 0, then shift all slots left
        if let Some(transfer) = self.slots[0].take() {
            transfer
                .fence
                .wait(None)
                .expect("GPU fence wait failed in acquire_slot");
            reclaimed.push((
                transfer.block_staging,
                transfer.meta_staging,
                transfer.custom_staging,
            ));
        }

        // Shift all slots left
        for i in 1..self.capacity {
            self.slots[i - 1] = self.slots[i].take();
        }

        // Return the last slot (now empty)
        (self.capacity - 1, reclaimed)
    }

    /// Submits a transfer to the specified slot.
    pub fn submit(&mut self, slot_index: usize, transfer: TransferSlot) {
        self.slots[slot_index] = Some(transfer);
    }

    /// Wait for all in-flight transfers to complete.
    /// Call this before destroying the ring buffer.
    #[allow(dead_code)]
    pub fn flush(&mut self) {
        for slot in &mut self.slots {
            if let Some(transfer) = slot.take() {
                transfer
                    .fence
                    .wait(None)
                    .expect("GPU fence wait failed during flush");
            }
        }
    }
}

impl Default for TransferRingBuffer {
    fn default() -> Self {
        // 6 slots provides good CPU/GPU overlap with reasonable memory usage.
        // Each frame can have up to 3 upload calls (completed chunks, unloaded chunks, dirty chunks).
        // With only 3 slots, if any previous transfer is still in-flight, we block.
        // 6 slots gives 2 frames of headroom before blocking occurs.
        Self::new(6)
    }
}

// Thread-local transfer ring buffer for async chunk uploads.
// Using 6 slots provides headroom for 2 frames worth of transfers before blocking.
// Each frame can have up to 3 upload calls (completed chunks, unloaded chunks, dirty chunks).
thread_local! {
    pub(crate) static TRANSFER_RING: RefCell<TransferRingBuffer> = RefCell::new(TransferRingBuffer::new(6));
    pub(crate) static STAGING_POOL: RefCell<Vec<StagingBufferPair>> = const { RefCell::new(Vec::new()) };
}

pub(crate) const STAGING_POOL_MAX: usize = 12; // 2x ring buffer capacity

/// Pre-warms the chunk-upload staging pool with `count` triples sized for a
/// typical origin-shift batch so the first shift doesn't pay the 5+ ms cost of
/// allocating ~30 MB of HOST-visible buffers inline. The pool's existing
/// "reuse if size ≥ request" policy means these stay hot for every subsequent
/// shift; short-idle/cold-start is the path this is intended to cover.
///
/// Sizes:
/// - block:  32 MiB (covers ~1024 chunks × 32 KiB = typical near-shift batch)
/// - meta:   16 MiB
/// - custom: 16 MiB
///
/// Safe to call once at GPU-resources init. No-op if called after the pool
/// already has entries — the thread_local staging pool keeps existing buffers.
pub fn prewarm_staging_pool(memory_allocator: &Arc<StandardMemoryAllocator>, count: usize) {
    const BLOCK_BYTES: u64 = 32 * 1024 * 1024;
    const META_BYTES: u64 = 16 * 1024 * 1024;
    const CUSTOM_BYTES: u64 = 16 * 1024 * 1024;

    STAGING_POOL.with(|pool| {
        let mut p = pool.borrow_mut();
        while p.len() < count.min(STAGING_POOL_MAX) {
            let mk = |size: u64| {
                Buffer::new_slice::<u8>(
                    memory_allocator.clone(),
                    BufferCreateInfo {
                        usage: BufferUsage::TRANSFER_SRC,
                        ..Default::default()
                    },
                    AllocationCreateInfo {
                        memory_type_filter: MemoryTypeFilter::PREFER_HOST
                            | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                        ..Default::default()
                    },
                    size,
                )
                .expect("prewarm staging buffer alloc")
            };
            p.push((mk(BLOCK_BYTES), mk(META_BYTES), mk(CUSTOM_BYTES)));
        }
    });
}

//! Async chunk loading system.
//!
//! This module provides background chunk generation using a thread pool
//! to avoid blocking the main thread during terrain generation.

use crossbeam_channel::{Receiver, Sender, TryRecvError, bounded, unbounded};
use nalgebra::Vector3;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Instant;

/// Global timing stats for chunk generation (for debugging/profiling).
static CHUNKS_GENERATED: AtomicUsize = AtomicUsize::new(0);
static TOTAL_GEN_TIME_US: AtomicU64 = AtomicU64::new(0);
static MAX_GEN_TIME_US: AtomicU64 = AtomicU64::new(0);

/// Get chunk generation timing statistics.
/// Returns (count, avg_ms, max_ms).
#[allow(dead_code)]
pub fn get_gen_timing_stats() -> (usize, f64, f64) {
    let count = CHUNKS_GENERATED.load(Ordering::Relaxed);
    let total_us = TOTAL_GEN_TIME_US.load(Ordering::Relaxed);
    let max_us = MAX_GEN_TIME_US.load(Ordering::Relaxed);
    let avg_ms = if count > 0 {
        (total_us as f64 / count as f64) / 1000.0
    } else {
        0.0
    };
    (count, avg_ms, max_us as f64 / 1000.0)
}

/// Reset timing statistics.
#[allow(dead_code)]
pub fn reset_gen_timing_stats() {
    CHUNKS_GENERATED.store(0, Ordering::Relaxed);
    TOTAL_GEN_TIME_US.store(0, Ordering::Relaxed);
    MAX_GEN_TIME_US.store(0, Ordering::Relaxed);
}

use crate::chunk::Chunk;
use crate::storage::ParallelStorageReader;
use crate::svt::{BRICKS_PER_CHUNK, ChunkSVT};
use crate::terrain_gen::{ChunkGenerationResult, OverflowBlock};

fn worker_threads() -> usize {
    static WORKERS: OnceLock<usize> = OnceLock::new();
    *WORKERS.get_or_init(|| {
        // Allow override via env; otherwise use physical cores, min 2 to keep parallelism.
        if let Ok(v) = std::env::var("CHUNK_WORKER_THREADS") {
            if let Ok(n) = v.parse::<usize>() {
                return n.clamp(1, num_cpus::get());
            }
        }
        // default: leave 1 core for main/render, but at least 2 workers
        let cores = num_cpus::get().max(2);
        cores.saturating_sub(1).max(2)
    })
}

/// Returns the default worker thread count (honors env override).
#[allow(dead_code)]
pub fn default_worker_threads() -> usize {
    worker_threads()
}

/// Maximum chunks to queue for generation.
/// Prevents memory buildup if generation is slower than requests.
/// Increased from 256 to reduce queue saturation during complex terrain.
const MAX_QUEUE_SIZE: usize = 384;
/// Soft cap for batches (defensive: avoids accidental huge fan-out).
const MAX_BATCH_REQUEST: usize = 256;

/// Request to generate a chunk at a specific position.
#[derive(Debug, Clone)]
pub struct ChunkRequest {
    pub position: Vector3<i32>,
    pub epoch: u32,
}

/// Pre-computed SVT metadata to avoid redundant computation on main thread.
///
/// SVT computation is expensive (~130,000 operations per chunk: 32,768 block reads
/// plus 4 BFS distance passes). Computing it once in the worker thread and passing
/// it to the main thread avoids duplicate work.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SvtMetadata {
    /// 64-bit mask indicating which bricks are non-empty.
    pub brick_mask: u64,
    /// Per-brick minimum distance to nearest solid brick.
    pub brick_distances: [u8; BRICKS_PER_CHUNK],
}

impl SvtMetadata {
    /// Create from a ChunkSVT.
    pub fn from_svt(svt: &ChunkSVT) -> Self {
        Self {
            brick_mask: svt.brick_mask,
            brick_distances: svt.brick_distances,
        }
    }

    /// Create empty metadata (all bricks empty, max distances).
    #[allow(dead_code)]
    pub fn empty() -> Self {
        Self {
            brick_mask: 0,
            brick_distances: [255; BRICKS_PER_CHUNK],
        }
    }
}

/// Result of chunk generation.
pub struct ChunkResult {
    pub position: Vector3<i32>,
    pub chunk: Chunk,
    /// Blocks that should be placed in neighboring chunks.
    pub overflow_blocks: Vec<OverflowBlock>,
    pub epoch: u32,
    /// Pre-computed SVT metadata for GPU buffer updates.
    /// This avoids computing SVT twice (once in worker, once on main thread).
    #[allow(dead_code)]
    pub svt_metadata: SvtMetadata,
}

/// Stats snapshot for loader/backpressure observability.
#[derive(Debug, Default, Clone, Copy)]
pub struct LoaderStats {
    pub in_flight: usize,
    pub queue_full_events: u32,
    pub dropped_stale_results: u32,
    pub queue_len: usize,
}

/// Batch request results.
#[derive(Debug, Default, Clone, Copy)]
pub struct RequestStats {
    pub queued: usize,
    pub failed_full: usize,
}

/// Async chunk loader that generates chunks in background threads.
pub struct ChunkLoader {
    /// Sender to queue chunk generation requests (crossbeam MPMC).
    /// Wrapped in Option to allow explicit drop before joining workers.
    request_tx: Option<Sender<ChunkRequest>>,
    /// Receiver handle for draining pending requests when resetting.
    request_rx: Receiver<ChunkRequest>,
    /// Receiver for completed chunks (crossbeam MPMC).
    result_rx: Receiver<ChunkResult>,
    /// Worker thread handles (for cleanup).
    workers: Vec<JoinHandle<()>>,
    /// Set of chunks currently being generated (to avoid duplicates).
    in_flight: HashSet<Vector3<i32>>,
    /// Current epoch/generation for requests (bumped on origin shift).
    current_epoch: u32,
    /// Count of try_send failures (queue full).
    queue_full_events: u32,
    /// Count of stale results dropped due to epoch mismatch.
    dropped_stale_results: u32,
    /// Number of worker threads currently active.
    worker_count: usize,
}

impl ChunkLoader {
    /// Creates a new chunk loader with the given terrain generator function.
    ///
    /// # Arguments
    /// * `generator` - A function that generates a ChunkGenerationResult from a chunk position.
    ///   This is typically a closure that captures a TerrainGenerator.
    /// * `world_dir` - Optional world directory path for loading chunks from disk.
    ///   Each worker creates its own `ParallelStorageReader` for true parallel I/O.
    pub fn new<F>(generator: F, world_dir: Option<PathBuf>) -> Self
    where
        F: Fn(Vector3<i32>) -> ChunkGenerationResult + Send + Sync + 'static,
    {
        Self::new_with_threads(generator, world_dir, worker_threads())
    }

    /// Creates a new chunk loader with an explicit worker thread count.
    pub fn new_with_threads<F>(
        generator: F,
        world_dir: Option<PathBuf>,
        worker_count: usize,
    ) -> Self
    where
        F: Fn(Vector3<i32>) -> ChunkGenerationResult + Send + Sync + 'static,
    {
        let generator = Arc::new(generator);
        let world_dir = world_dir.map(Arc::new);

        // Crossbeam channels - true MPMC without mutex overhead.
        // Bounded request channel prevents unbounded memory growth.
        let (request_tx, request_rx) = bounded::<ChunkRequest>(MAX_QUEUE_SIZE);
        // Unbounded result channel - workers should never block on sending results.
        let (result_tx, result_rx) = unbounded::<ChunkResult>();

        // Spawn worker threads
        let mut workers = Vec::with_capacity(worker_count);
        for i in 0..worker_count {
            let request_rx = request_rx.clone();
            let result_tx = result_tx.clone();
            let generator = Arc::clone(&generator);
            let world_dir = world_dir.clone();

            let handle = thread::Builder::new()
                .name(format!("chunk-worker-{}", i))
                .spawn(move || {
                    // Each worker creates its own storage reader for parallel disk I/O.
                    // This avoids the single-threaded StorageSystem bottleneck.
                    let mut storage_reader = world_dir
                        .as_ref()
                        .map(|dir| ParallelStorageReader::new(dir.as_ref().clone()));

                    // Use recv_timeout to allow periodic shutdown checks while
                    // still blocking efficiently when no work is available.
                    // crossbeam channels don't need a mutex - multiple workers
                    // can call recv concurrently without serialization.
                    loop {
                        match request_rx.recv_timeout(std::time::Duration::from_millis(20)) {
                            Ok(req) => {
                                // Try to load from disk first (parallel - each worker has own reader)
                                let (chunk, overflow_blocks) = if let Some(ref mut reader) =
                                    storage_reader
                                {
                                    match reader.load_chunk(req.position) {
                                        Ok(Some(mut chunk)) => {
                                            chunk.update_metadata();
                                            // Mark dirty for GPU upload
                                            chunk.mark_dirty();
                                            // Loaded from disk, so it's clean for persistence
                                            chunk.persistence_dirty = false;
                                            // Loaded chunks have no overflow blocks
                                            (chunk, Vec::new())
                                        }
                                        Ok(None) => {
                                            let gen_start = Instant::now();
                                            let result = generator(req.position);
                                            let gen_time = gen_start.elapsed();

                                            // Update timing stats
                                            let gen_us = gen_time.as_micros() as u64;
                                            CHUNKS_GENERATED.fetch_add(1, Ordering::Relaxed);
                                            TOTAL_GEN_TIME_US.fetch_add(gen_us, Ordering::Relaxed);
                                            MAX_GEN_TIME_US.fetch_max(gen_us, Ordering::Relaxed);

                                            let mut chunk = result.chunk;
                                            // New procedural chunk, clean for persistence until modified
                                            chunk.persistence_dirty = false;
                                            (chunk, result.overflow_blocks)
                                        }
                                        Err(e) => {
                                            eprintln!(
                                                "[Storage] Load error for {:?}: {}",
                                                req.position, e
                                            );
                                            let result = generator(req.position);
                                            let mut chunk = result.chunk;
                                            chunk.persistence_dirty = false;
                                            (chunk, result.overflow_blocks)
                                        }
                                    }
                                } else {
                                    let result = generator(req.position);
                                    let mut chunk = result.chunk;
                                    chunk.persistence_dirty = false;
                                    (chunk, result.overflow_blocks)
                                };

                                // Compute SVT metadata once here in worker thread.
                                // This avoids computing it twice: once on load, once on metadata update.
                                let svt = ChunkSVT::from_chunk(&chunk);
                                let svt_metadata = SvtMetadata::from_svt(&svt);

                                // Send result back (ignore error if receiver dropped)
                                let _ = result_tx.send(ChunkResult {
                                    position: req.position,
                                    chunk,
                                    overflow_blocks,
                                    epoch: req.epoch,
                                    svt_metadata,
                                });
                            }
                            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                                // No work available, loop and check again
                                continue;
                            }
                            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                                // Channel closed, exit worker
                                break;
                            }
                        }
                    }
                })
                .expect("Failed to spawn chunk worker thread");

            workers.push(handle);
        }

        Self {
            request_tx: Some(request_tx),
            request_rx,
            result_rx,
            workers,
            in_flight: HashSet::new(),
            current_epoch: 0,
            queue_full_events: 0,
            dropped_stale_results: 0,
            worker_count,
        }
    }

    /// Queues a chunk for generation if not already queued or in flight.
    ///
    /// Returns true if the chunk was queued, false if it was already in flight
    /// or the queue is full.
    pub fn request_chunk(&mut self, position: Vector3<i32>) -> bool {
        // Don't queue if already in flight
        if self.in_flight.contains(&position) {
            return false;
        }

        // Check queue size limit
        if self.in_flight.len() >= MAX_QUEUE_SIZE {
            return false;
        }

        // Get sender (may be None if shutting down)
        let Some(ref request_tx) = self.request_tx else {
            return false;
        };

        // Send request to workers (non-blocking try_send to avoid stalling main thread)
        match request_tx.try_send(ChunkRequest {
            position,
            epoch: self.current_epoch,
        }) {
            Ok(()) => {
                self.in_flight.insert(position);
                true
            }
            Err(_) => {
                self.queue_full_events = self.queue_full_events.wrapping_add(1);
                false
            } // Channel full or disconnected
        }
    }

    /// Queues multiple chunks for generation.
    ///
    /// Returns the number of chunks successfully queued.
    pub fn request_chunks(&mut self, positions: &[Vector3<i32>]) -> RequestStats {
        let mut queued = 0;
        let mut failed_full = 0;
        // Deduplicate within the batch to avoid spamming the channel with repeats.
        let mut batch_seen = HashSet::with_capacity(positions.len().min(MAX_BATCH_REQUEST));
        for &pos in positions.iter().take(MAX_BATCH_REQUEST) {
            if !batch_seen.insert(pos) {
                continue;
            }
            if self.request_chunk(pos) {
                queued += 1;
            } else {
                failed_full += 1;
            }
        }
        RequestStats {
            queued,
            failed_full,
        }
    }

    /// Receives completed chunks (non-blocking).
    ///
    /// Returns a vector of all currently available completed chunks.
    pub fn receive_chunks(&mut self) -> Vec<ChunkResult> {
        let mut results = Vec::with_capacity(self.in_flight.len().min(64));

        loop {
            match self.result_rx.try_recv() {
                Ok(result) => {
                    if result.epoch == self.current_epoch {
                        self.in_flight.remove(&result.position);
                        results.push(result);
                    } else {
                        // Stale result from previous epoch; drop it.
                        self.dropped_stale_results = self.dropped_stale_results.wrapping_add(1);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }

        results
    }

    /// Returns the number of chunks currently being generated.
    #[allow(dead_code)]
    pub fn in_flight_count(&self) -> usize {
        self.in_flight.len()
    }

    /// Returns true if a position is already queued or in-flight.
    #[allow(dead_code)]
    pub fn is_in_flight(&self, position: Vector3<i32>) -> bool {
        self.in_flight.contains(&position)
    }

    /// Bumps epoch and clears all pending and in-flight work.
    ///
    /// Used when the texture origin shifts to drop stale work immediately.
    pub fn reset_epoch_and_clear(&mut self) {
        self.current_epoch = self.current_epoch.wrapping_add(1);
        self.in_flight.clear();

        // Drain request queue
        while self.request_rx.try_recv().is_ok() {}
        // Drain results queue
        while self.result_rx.try_recv().is_ok() {}
    }

    /// Cancels a pending chunk request if it hasn't started yet.
    ///
    /// Note: This only removes it from tracking, the worker may still
    /// process it but the result will be ignored.
    pub fn cancel_chunk(&mut self, position: Vector3<i32>) {
        self.in_flight.remove(&position);
    }

    /// Clears all pending requests and drains any completed results.
    ///
    /// This is called during texture origin shifts to ensure no stale chunks
    /// are processed with the wrong coordinate system.
    #[allow(dead_code)]
    pub fn clear_pending(&mut self) {
        self.reset_epoch_and_clear();
    }

    /// Returns loader stats for HUD/telemetry.
    pub fn stats(&self) -> LoaderStats {
        LoaderStats {
            in_flight: self.in_flight.len(),
            queue_full_events: self.queue_full_events,
            dropped_stale_results: self.dropped_stale_results,
            queue_len: self.request_rx.len(),
        }
    }

    pub fn worker_count(&self) -> usize {
        self.worker_count
    }

    pub fn queue_capacity(&self) -> usize {
        MAX_QUEUE_SIZE
    }
}

impl Drop for ChunkLoader {
    fn drop(&mut self) {
        // Explicitly drop the sender FIRST to close the channel.
        // Workers will see Disconnected error on recv and exit.
        // We must do this before joining, otherwise workers block forever.
        self.request_tx.take();

        // Wait for workers to finish (they will exit after seeing Disconnected)
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::BlockType;
    use std::time::Duration;

    fn test_generator(pos: Vector3<i32>) -> ChunkGenerationResult {
        let mut chunk = Chunk::new();
        // Set a block based on position for testing
        chunk.set_block(0, 0, 0, BlockType::Stone);
        chunk.set_block(
            (pos.x.abs() % 32) as usize,
            (pos.y.abs() % 32) as usize,
            (pos.z.abs() % 32) as usize,
            BlockType::Dirt,
        );
        ChunkGenerationResult {
            chunk,
            overflow_blocks: Vec::new(), // No overflow in tests
        }
    }

    #[test]
    fn test_chunk_loader_basic() {
        let mut loader = ChunkLoader::new(test_generator, None);

        // Request a chunk
        assert!(loader.request_chunk(Vector3::new(0, 0, 0)));
        assert_eq!(loader.in_flight_count(), 1);

        // Duplicate request should fail
        assert!(!loader.request_chunk(Vector3::new(0, 0, 0)));
        assert_eq!(loader.in_flight_count(), 1);

        // Wait for completion
        thread::sleep(Duration::from_millis(200));

        let results = loader.receive_chunks();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].position, Vector3::new(0, 0, 0));
        assert_eq!(loader.in_flight_count(), 0);
    }

    #[test]
    fn test_chunk_loader_multiple() {
        let mut loader = ChunkLoader::new(test_generator, None);

        // Request multiple chunks
        let positions: Vec<_> = (0..8).map(|i| Vector3::new(i, 0, 0)).collect();
        let stats = loader.request_chunks(&positions);
        assert_eq!(stats.queued, 8);

        // Wait for completion
        thread::sleep(Duration::from_millis(500));

        let results = loader.receive_chunks();
        assert_eq!(results.len(), 8);
    }

    #[test]
    fn test_chunk_loader_batch_dedupe_and_cap() {
        let mut loader = ChunkLoader::new(test_generator, None);

        // Create a batch with duplicates and over the soft cap
        let mut positions: Vec<_> = (0..300).map(|i| Vector3::new(i / 2, 0, 0)).collect();
        // Add some explicit duplicates
        positions.push(Vector3::new(0, 0, 0));
        positions.push(Vector3::new(1, 0, 0));

        let stats = loader.request_chunks(&positions);

        // Expect at most MAX_BATCH_REQUEST unique positions to be queued
        assert!(stats.queued <= MAX_BATCH_REQUEST);

        // Duplicates of the first few positions should be ignored
        assert!(loader.is_in_flight(Vector3::new(0, 0, 0)));
        assert!(loader.is_in_flight(Vector3::new(1, 0, 0)));
        // And the count should match the in-flight tracking
        assert_eq!(loader.in_flight_count(), stats.queued);
    }
}

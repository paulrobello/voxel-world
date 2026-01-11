//! Async chunk loading system.
//!
//! This module provides background chunk generation using a thread pool
//! to avoid blocking the main thread during terrain generation.

use crossbeam_channel::{Receiver, Sender, TryRecvError, bounded, unbounded};
use nalgebra::Vector3;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crate::chunk::Chunk;
use crate::storage::ParallelStorageReader;
use crate::terrain_gen::{ChunkGenerationResult, OverflowBlock};

/// Number of worker threads for chunk generation.
/// Using more threads provides better parallelism for CPU-bound terrain generation.
/// This should roughly match available CPU cores minus 1-2 for main thread/GPU.
const WORKER_THREADS: usize = 8;

/// Maximum chunks to queue for generation.
/// Prevents memory buildup if generation is slower than requests.
const MAX_QUEUE_SIZE: usize = 256;
/// Soft cap for batches (defensive: avoids accidental huge fan-out).
const MAX_BATCH_REQUEST: usize = 256;

/// Request to generate a chunk at a specific position.
#[derive(Debug, Clone)]
pub struct ChunkRequest {
    pub position: Vector3<i32>,
}

/// Result of chunk generation.
pub struct ChunkResult {
    pub position: Vector3<i32>,
    pub chunk: Chunk,
    /// Blocks that should be placed in neighboring chunks.
    pub overflow_blocks: Vec<OverflowBlock>,
}

/// Async chunk loader that generates chunks in background threads.
pub struct ChunkLoader {
    /// Sender to queue chunk generation requests (crossbeam MPMC).
    /// Wrapped in Option to allow explicit drop before joining workers.
    request_tx: Option<Sender<ChunkRequest>>,
    /// Receiver for completed chunks (crossbeam MPMC).
    result_rx: Receiver<ChunkResult>,
    /// Worker thread handles (for cleanup).
    workers: Vec<JoinHandle<()>>,
    /// Set of chunks currently being generated (to avoid duplicates).
    in_flight: HashSet<Vector3<i32>>,
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
        let generator = Arc::new(generator);
        let world_dir = world_dir.map(Arc::new);

        // Crossbeam channels - true MPMC without mutex overhead.
        // Bounded request channel prevents unbounded memory growth.
        let (request_tx, request_rx) = bounded::<ChunkRequest>(MAX_QUEUE_SIZE);
        // Unbounded result channel - workers should never block on sending results.
        let (result_tx, result_rx) = unbounded::<ChunkResult>();

        // Spawn worker threads
        let mut workers = Vec::with_capacity(WORKER_THREADS);
        for i in 0..WORKER_THREADS {
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
                        match request_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                            Ok(req) => {
                                // Try to load from disk first (parallel - each worker has own reader)
                                let (chunk, overflow_blocks) =
                                    if let Some(ref mut reader) = storage_reader {
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
                                                let result = generator(req.position);
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

                                // Send result back (ignore error if receiver dropped)
                                let _ = result_tx.send(ChunkResult {
                                    position: req.position,
                                    chunk,
                                    overflow_blocks,
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
            result_rx,
            workers,
            in_flight: HashSet::new(),
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
        match request_tx.try_send(ChunkRequest { position }) {
            Ok(()) => {
                self.in_flight.insert(position);
                true
            }
            Err(_) => false, // Channel full or disconnected
        }
    }

    /// Queues multiple chunks for generation.
    ///
    /// Returns the number of chunks successfully queued.
    pub fn request_chunks(&mut self, positions: &[Vector3<i32>]) -> usize {
        let mut queued = 0;
        // Deduplicate within the batch to avoid spamming the channel with repeats.
        let mut batch_seen = HashSet::with_capacity(positions.len().min(MAX_BATCH_REQUEST));
        for &pos in positions.iter().take(MAX_BATCH_REQUEST) {
            if !batch_seen.insert(pos) {
                continue;
            }
            if self.request_chunk(pos) {
                queued += 1;
            }
        }
        queued
    }

    /// Receives completed chunks (non-blocking).
    ///
    /// Returns a vector of all currently available completed chunks.
    pub fn receive_chunks(&mut self) -> Vec<ChunkResult> {
        let mut results = Vec::with_capacity(self.in_flight.len().min(64));

        loop {
            match self.result_rx.try_recv() {
                Ok(result) => {
                    self.in_flight.remove(&result.position);
                    results.push(result);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }

        results
    }

    /// Returns the number of chunks currently being generated.
    pub fn in_flight_count(&self) -> usize {
        self.in_flight.len()
    }

    /// Returns true if a position is already queued or in-flight.
    #[allow(dead_code)]
    pub fn is_in_flight(&self, position: Vector3<i32>) -> bool {
        self.in_flight.contains(&position)
    }

    /// Cancels a pending chunk request if it hasn't started yet.
    ///
    /// Note: This only removes it from tracking, the worker may still
    /// process it but the result will be ignored.
    pub fn cancel_chunk(&mut self, position: Vector3<i32>) {
        self.in_flight.remove(&position);
    }

    /// Clears all pending requests.
    ///
    /// Note: Workers may still process some requests, but results will be ignored.
    #[allow(dead_code)]
    pub fn clear_pending(&mut self) {
        self.in_flight.clear();
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
        let queued = loader.request_chunks(&positions);
        assert_eq!(queued, 8);

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

        let queued = loader.request_chunks(&positions);

        // Expect at most MAX_BATCH_REQUEST unique positions to be queued
        assert!(queued <= MAX_BATCH_REQUEST);

        // Duplicates of the first few positions should be ignored
        assert!(loader.is_in_flight(Vector3::new(0, 0, 0)));
        assert!(loader.is_in_flight(Vector3::new(1, 0, 0)));
        // And the count should match the in-flight tracking
        assert_eq!(loader.in_flight_count(), queued);
    }
}

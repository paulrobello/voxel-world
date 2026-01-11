//! Async chunk loading system.
//!
//! This module provides background chunk generation using a thread pool
//! to avoid blocking the main thread during terrain generation.
//!
//! Uses a priority-based work queue that gets re-sorted each frame to ensure
//! chunks in the player's viewing direction are loaded first.

use nalgebra::Vector3;
use std::collections::{HashSet, VecDeque};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread::{self, JoinHandle};

use crate::chunk::Chunk;
use crate::storage::worker::StorageSystem;
use crate::terrain_gen::{ChunkGenerationResult, OverflowBlock};

/// Number of worker threads for chunk generation.
/// Using 4 threads provides good parallelism without overwhelming the CPU.
const WORKER_THREADS: usize = 4;

/// Maximum chunks to queue for generation.
/// Prevents memory buildup if generation is slower than requests.
const MAX_QUEUE_SIZE: usize = 128;
/// Soft cap for batches (defensive: avoids accidental huge fan-out).
const MAX_BATCH_REQUEST: usize = 256;

/// Request to generate a chunk at a specific position.
#[derive(Debug, Clone)]
#[allow(dead_code)]
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

/// Priority-based work queue for chunk generation.
/// Allows the main thread to update priorities each frame.
struct WorkQueue {
    /// Queue of chunk positions to generate, in priority order (front = highest)
    queue: VecDeque<Vector3<i32>>,
    /// Set of positions in the queue for fast lookup
    queued_set: HashSet<Vector3<i32>>,
    /// Shutdown flag
    shutdown: bool,
}

impl WorkQueue {
    fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            queued_set: HashSet::new(),
            shutdown: false,
        }
    }

    /// Replace the queue with a new priority-sorted list of chunks.
    /// Only adds chunks that are in the in_flight set (not cancelled).
    fn update_queue(&mut self, positions: &[Vector3<i32>], in_flight: &HashSet<Vector3<i32>>) {
        self.queue.clear();
        self.queued_set.clear();
        for &pos in positions {
            if in_flight.contains(&pos) && self.queued_set.insert(pos) {
                self.queue.push_back(pos);
            }
        }
    }

    /// Pop the highest priority chunk (front of queue).
    fn pop(&mut self) -> Option<Vector3<i32>> {
        if let Some(pos) = self.queue.pop_front() {
            self.queued_set.remove(&pos);
            Some(pos)
        } else {
            None
        }
    }
}

/// Async chunk loader that generates chunks in background threads.
pub struct ChunkLoader {
    /// Shared work queue protected by mutex
    work_queue: Arc<(Mutex<WorkQueue>, Condvar)>,
    /// Receiver for completed chunks.
    result_rx: Receiver<ChunkResult>,
    /// Worker thread handles (for cleanup).
    workers: Vec<JoinHandle<()>>,
    /// Set of chunks currently being generated (to avoid duplicates).
    in_flight: Arc<RwLock<HashSet<Vector3<i32>>>>,
}

impl ChunkLoader {
    /// Creates a new chunk loader with the given terrain generator function.
    ///
    /// # Arguments
    /// * `generator` - A function that generates a ChunkGenerationResult from a chunk position.
    ///   This is typically a closure that captures a TerrainGenerator.
    /// * `storage` - Optional storage system to load chunks from disk.
    pub fn new<F>(generator: F, storage: Option<Arc<StorageSystem>>) -> Self
    where
        F: Fn(Vector3<i32>) -> ChunkGenerationResult + Send + Sync + 'static,
    {
        let generator = Arc::new(generator);

        // Shared work queue with condition variable for notification
        let work_queue = Arc::new((Mutex::new(WorkQueue::new()), Condvar::new()));

        // Channel for receiving results from workers
        let (result_tx, result_rx) = mpsc::channel::<ChunkResult>();

        // Shared in-flight set so workers can check for cancellation
        let in_flight = Arc::new(RwLock::new(HashSet::new()));

        // Spawn worker threads
        let mut workers = Vec::with_capacity(WORKER_THREADS);
        for i in 0..WORKER_THREADS {
            let work_queue = Arc::clone(&work_queue);
            let result_tx = result_tx.clone();
            let generator = Arc::clone(&generator);
            let storage = storage.as_ref().map(Arc::clone);
            let in_flight_worker = Arc::clone(&in_flight);

            let handle = thread::Builder::new()
                .name(format!("chunk-worker-{}", i))
                .spawn(move || {
                    loop {
                        // Wait for work or shutdown
                        let position = {
                            let (lock, cvar) = &*work_queue;
                            let mut queue = lock.lock().unwrap();

                            // Wait until there's work or shutdown
                            while queue.queue.is_empty() && !queue.shutdown {
                                queue = cvar.wait(queue).unwrap();
                            }

                            if queue.shutdown {
                                break;
                            }

                            queue.pop()
                        };

                        let Some(position) = position else {
                            continue;
                        };

                        // Check if this chunk was cancelled
                        {
                            let in_flight_guard = in_flight_worker.read().unwrap();
                            if !in_flight_guard.contains(&position) {
                                // Chunk was cancelled, skip processing
                                continue;
                            }
                        }

                        // Try to load from disk first
                        let (chunk, overflow_blocks) = if let Some(ref storage) = storage {
                            match storage.load_chunk(position) {
                                Ok(Some(mut chunk)) => {
                                    chunk.update_metadata();
                                    chunk.mark_dirty();
                                    chunk.persistence_dirty = false;
                                    (chunk, Vec::new())
                                }
                                Ok(None) => {
                                    let result = generator(position);
                                    let mut chunk = result.chunk;
                                    chunk.persistence_dirty = false;
                                    (chunk, result.overflow_blocks)
                                }
                                Err(e) => {
                                    eprintln!("[Storage] Load error for {:?}: {}", position, e);
                                    let result = generator(position);
                                    let mut chunk = result.chunk;
                                    chunk.persistence_dirty = false;
                                    (chunk, result.overflow_blocks)
                                }
                            }
                        } else {
                            let result = generator(position);
                            let mut chunk = result.chunk;
                            chunk.persistence_dirty = false;
                            (chunk, result.overflow_blocks)
                        };

                        // Send result back
                        let _ = result_tx.send(ChunkResult {
                            position,
                            chunk,
                            overflow_blocks,
                        });
                    }
                })
                .expect("Failed to spawn chunk worker thread");

            workers.push(handle);
        }

        Self {
            work_queue,
            result_rx,
            workers,
            in_flight,
        }
    }

    /// Queues a chunk for generation if not already queued or in flight.
    ///
    /// Returns true if the chunk was queued, false if it was already in flight
    /// or the queue is full.
    pub fn request_chunk(&mut self, position: Vector3<i32>) -> bool {
        let mut in_flight = self.in_flight.write().unwrap();

        // Don't queue if already in flight
        if in_flight.contains(&position) {
            return false;
        }

        // Check queue size limit
        if in_flight.len() >= MAX_QUEUE_SIZE {
            return false;
        }

        // Add to in_flight (will be added to work queue in update_priorities)
        in_flight.insert(position);
        true
    }

    /// Queues multiple chunks for generation.
    ///
    /// Returns the number of chunks successfully queued.
    pub fn request_chunks(&mut self, positions: &[Vector3<i32>]) -> usize {
        let mut queued = 0;
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

    /// Update the work queue with a new priority-sorted list of chunks.
    /// This should be called each frame with chunks sorted by priority (highest first).
    /// Only chunks that are in in_flight will be added to the work queue.
    pub fn update_priorities(&mut self, priority_sorted_chunks: &[Vector3<i32>]) {
        let in_flight = self.in_flight.read().unwrap();
        let (lock, cvar) = &*self.work_queue;
        let mut queue = lock.lock().unwrap();
        queue.update_queue(priority_sorted_chunks, &in_flight);
        // Notify all waiting workers
        cvar.notify_all();
    }

    /// Receives completed chunks (non-blocking).
    ///
    /// Returns a vector of all currently available completed chunks.
    pub fn receive_chunks(&mut self) -> Vec<ChunkResult> {
        let in_flight_len = self.in_flight.read().unwrap().len();
        let mut results = Vec::with_capacity(in_flight_len.min(32));

        loop {
            match self.result_rx.try_recv() {
                Ok(result) => {
                    self.in_flight.write().unwrap().remove(&result.position);
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
        self.in_flight.read().unwrap().len()
    }

    /// Returns true if a position is already queued or in-flight.
    #[allow(dead_code)]
    pub fn is_in_flight(&self, position: Vector3<i32>) -> bool {
        self.in_flight.read().unwrap().contains(&position)
    }

    /// Returns a copy of all currently in-flight chunk positions.
    #[allow(dead_code)]
    pub fn in_flight_positions(&self) -> Vec<Vector3<i32>> {
        self.in_flight.read().unwrap().iter().copied().collect()
    }

    /// Cancels a pending chunk request if it hasn't started yet.
    ///
    /// Workers check in_flight before processing, so cancelled chunks
    /// will be skipped efficiently.
    pub fn cancel_chunk(&mut self, position: Vector3<i32>) {
        self.in_flight.write().unwrap().remove(&position);
    }

    /// Clears all pending requests.
    ///
    /// Workers check in_flight before processing, so all pending chunks
    /// will be skipped.
    #[allow(dead_code)]
    pub fn clear_pending(&mut self) {
        self.in_flight.write().unwrap().clear();
    }
}

impl Drop for ChunkLoader {
    fn drop(&mut self) {
        // Signal shutdown
        {
            let (lock, cvar) = &*self.work_queue;
            let mut queue = lock.lock().unwrap();
            queue.shutdown = true;
            cvar.notify_all();
        }

        // Wait for workers to finish
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

        // Update priorities to trigger work
        loader.update_priorities(&[Vector3::new(0, 0, 0)]);

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

        // Update priorities to trigger work
        loader.update_priorities(&positions);

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

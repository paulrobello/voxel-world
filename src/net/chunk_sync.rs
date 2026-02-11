//! Chunk streaming and synchronization for multiplayer.
//!
//! Handles chunk requests, prioritization, and cancellation.
//! Prioritizes chunks closest to the player in their look direction.

// Allow unused code until networking is integrated into the game
#![allow(dead_code)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use crate::chunk::{BlockModelData, BlockPaintData, CHUNK_SIZE, CHUNK_VOLUME, WaterType};
use crate::net::protocol::RequestChunks;
use lz4_flex::compress_prepend_size;

/// Priority level for chunk requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChunkPriority {
    /// Lowest priority - background loading.
    Background = 0,
    /// Medium priority - adjacent to loaded chunks.
    Adjacent = 1,
    /// High priority - in player's view direction.
    ViewDirection = 2,
    /// Critical priority - player's current position.
    PlayerPosition = 3,
}

/// A chunk request with priority and timestamp.
#[derive(Debug, Clone)]
pub struct ChunkRequest {
    /// Chunk position (chunk coordinates).
    pub position: [i32; 3],
    /// Priority level.
    pub priority: ChunkPriority,
    /// When this request was made.
    pub requested_at: Instant,
    /// Whether this request has been sent to the server.
    pub sent: bool,
}

/// Manages chunk streaming with prioritization and cancellation.
pub struct ChunkSyncManager {
    /// Maximum chunks to request per batch.
    max_batch_size: usize,
    /// Maximum chunks to have in-flight at once.
    max_in_flight: usize,
    /// Currently requested chunks (position -> request info).
    pending_requests: HashMap<[i32; 3], ChunkRequest>,
    /// Priority queue ordered by priority (highest first).
    priority_queue: VecDeque<[i32; 3]>,
    /// Chunks that have been received.
    received_chunks: HashSet<[i32; 3]>,
    /// Last player position (for distance calculations).
    last_player_pos: [f32; 3],
    /// Last player look direction (for view priority).
    last_look_dir: [f32; 3],
}

impl Default for ChunkSyncManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ChunkSyncManager {
    /// Creates a new chunk sync manager.
    pub fn new() -> Self {
        Self {
            max_batch_size: 16,
            max_in_flight: 64,
            pending_requests: HashMap::new(),
            priority_queue: VecDeque::new(),
            received_chunks: HashSet::new(),
            last_player_pos: [0.0, 0.0, 0.0],
            last_look_dir: [0.0, 0.0, -1.0],
        }
    }

    /// Updates player position and look direction.
    /// Returns chunks that should be cancelled due to direction change.
    pub fn update_player_state(&mut self, position: [f32; 3], look_dir: [f32; 3]) -> Vec<[i32; 3]> {
        let direction_changed = self.direction_changed(look_dir);
        let position_changed = self.position_changed(position);

        self.last_player_pos = position;
        self.last_look_dir = look_dir;

        // Recalculate priorities for all pending requests
        let mut to_cancel = Vec::new();

        if direction_changed {
            // Cancel low-priority chunks that are now behind the player
            for (pos, request) in &self.pending_requests {
                if !request.sent {
                    continue;
                }
                let dot = dot_product(chunk_center(*pos, position), look_dir);
                // If chunk is behind the player (dot < 0) and not critical priority
                if dot < -0.3 && request.priority < ChunkPriority::Adjacent {
                    to_cancel.push(*pos);
                }
            }
        }

        // Update priorities based on new position
        if position_changed || direction_changed {
            self.recalculate_priorities();
        }

        to_cancel
    }

    /// Requests chunks around the player position.
    /// Returns a RequestChunks message to send to the server.
    pub fn request_chunks_around(
        &mut self,
        player_chunk: [i32; 3],
        view_distance: i32,
    ) -> Option<RequestChunks> {
        let mut needed = Vec::new();

        // Collect all chunks within view distance
        for dz in -view_distance..=view_distance {
            for dy in -view_distance..=view_distance {
                for dx in -view_distance..=view_distance {
                    let pos = [
                        player_chunk[0] + dx,
                        player_chunk[1] + dy,
                        player_chunk[2] + dz,
                    ];

                    // Skip if already received or already requested
                    if self.received_chunks.contains(&pos)
                        || self.pending_requests.contains_key(&pos)
                    {
                        continue;
                    }

                    let priority = self.calculate_priority(pos);
                    needed.push((pos, priority));
                }
            }
        }

        if needed.is_empty() {
            return None;
        }

        // Sort by priority (highest first)
        needed.sort_by(|a, b| b.1.cmp(&a.1));

        // Take up to batch size
        let batch: Vec<[i32; 3]> = needed
            .into_iter()
            .take(self.max_batch_size)
            .map(|(pos, priority)| {
                // Add to pending requests
                self.pending_requests.insert(
                    pos,
                    ChunkRequest {
                        position: pos,
                        priority,
                        requested_at: Instant::now(),
                        sent: true,
                    },
                );
                pos
            })
            .collect();

        if batch.is_empty() {
            None
        } else {
            Some(RequestChunks { positions: batch })
        }
    }

    /// Cancels chunk requests.
    pub fn cancel_requests(&mut self, positions: &[[i32; 3]]) {
        for pos in positions {
            self.pending_requests.remove(pos);
            self.priority_queue.retain(|p| p != pos);
        }
    }

    /// Marks a chunk as received.
    pub fn mark_received(&mut self, position: [i32; 3]) {
        self.pending_requests.remove(&position);
        self.received_chunks.insert(position);
    }

    /// Clears received chunks (e.g., on teleport or world change).
    pub fn clear_received(&mut self) {
        self.received_chunks.clear();
        self.pending_requests.clear();
        self.priority_queue.clear();
    }

    /// Returns the number of pending requests.
    pub fn pending_count(&self) -> usize {
        self.pending_requests.len()
    }

    /// Returns the number of received chunks.
    pub fn received_count(&self) -> usize {
        self.received_chunks.len()
    }

    /// Calculates priority for a chunk position.
    fn calculate_priority(&self, chunk_pos: [i32; 3]) -> ChunkPriority {
        let player_chunk = world_to_chunk(self.last_player_pos);
        let dx = (chunk_pos[0] - player_chunk[0]).abs();
        let dy = (chunk_pos[1] - player_chunk[1]).abs();
        let dz = (chunk_pos[2] - player_chunk[2]).abs();
        let distance = dx + dy + dz; // Manhattan distance

        // Player's current chunk
        if distance == 0 {
            return ChunkPriority::PlayerPosition;
        }

        // Check if in view direction
        let to_chunk = chunk_center(chunk_pos, self.last_player_pos);
        let dot = dot_product(to_chunk, self.last_look_dir);

        if dot > 0.5 && distance <= 3 {
            return ChunkPriority::ViewDirection;
        }

        // Adjacent chunks
        if distance <= 2 {
            return ChunkPriority::Adjacent;
        }

        ChunkPriority::Background
    }

    /// Recalculates priorities for all pending requests.
    fn recalculate_priorities(&mut self) {
        // Collect positions first to avoid borrow conflict
        let positions: Vec<[i32; 3]> = self.pending_requests.keys().copied().collect();

        // Calculate new priorities
        let updates: Vec<([i32; 3], ChunkPriority)> = positions
            .into_iter()
            .map(|pos| (pos, self.calculate_priority(pos)))
            .collect();

        // Apply updates
        for (pos, priority) in updates {
            if let Some(request) = self.pending_requests.get_mut(&pos) {
                request.priority = priority;
            }
        }
    }

    /// Checks if look direction has changed significantly.
    fn direction_changed(&self, new_dir: [f32; 3]) -> bool {
        let dot = dot_product(self.last_look_dir, new_dir);
        dot < 0.9 // ~25 degree threshold
    }

    /// Checks if position has changed significantly.
    fn position_changed(&self, new_pos: [f32; 3]) -> bool {
        let dx = new_pos[0] - self.last_player_pos[0];
        let dy = new_pos[1] - self.last_player_pos[1];
        let dz = new_pos[2] - self.last_player_pos[2];
        let dist_sq = dx * dx + dy * dy + dz * dz;
        dist_sq > (CHUNK_SIZE as f32 / 2.0).powi(2) // Half chunk threshold
    }
}

/// Converts world position to chunk coordinates.
fn world_to_chunk(world_pos: [f32; 3]) -> [i32; 3] {
    [
        (world_pos[0] / CHUNK_SIZE as f32).floor() as i32,
        (world_pos[1] / CHUNK_SIZE as f32).floor() as i32,
        (world_pos[2] / CHUNK_SIZE as f32).floor() as i32,
    ]
}

/// Gets vector from player to chunk center.
fn chunk_center(chunk_pos: [i32; 3], player_pos: [f32; 3]) -> [f32; 3] {
    let chunk_world = [
        (chunk_pos[0] * CHUNK_SIZE as i32 + CHUNK_SIZE as i32 / 2) as f32,
        (chunk_pos[1] * CHUNK_SIZE as i32 + CHUNK_SIZE as i32 / 2) as f32,
        (chunk_pos[2] * CHUNK_SIZE as i32 + CHUNK_SIZE as i32 / 2) as f32,
    ];
    normalize([
        chunk_world[0] - player_pos[0],
        chunk_world[1] - player_pos[1],
        chunk_world[2] - player_pos[2],
    ])
}

/// Computes dot product of two vectors.
fn dot_product(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Normalizes a vector.
fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len > 0.0 {
        [v[0] / len, v[1] / len, v[2] / len]
    } else {
        v
    }
}

/// Serialized chunk data for network transmission.
#[derive(Debug, Clone)]
pub struct SerializedChunk {
    /// Chunk position.
    pub position: [i32; 3],
    /// Version number for delta compression.
    pub version: u32,
    /// Raw block data (CHUNK_VOLUME bytes).
    pub blocks: Vec<u8>,
    /// Model metadata (2 bytes per model block).
    pub model_data: Vec<(usize, BlockModelData)>,
    /// Paint metadata.
    pub paint_data: Vec<(usize, BlockPaintData)>,
    /// Tint data (for TintedGlass/Crystal).
    pub tint_data: Vec<(usize, u8)>,
    /// Water type data.
    pub water_data: Vec<(usize, WaterType)>,
}

impl SerializedChunk {
    /// Compresses the chunk data for network transmission.
    pub fn compress(&self) -> Result<Vec<u8>, &'static str> {
        let mut raw = Vec::with_capacity(CHUNK_VOLUME + 1024);

        // Write block data
        raw.extend_from_slice(&self.blocks);

        // Write model data count and entries
        let model_count = self.model_data.len() as u16;
        raw.extend_from_slice(&model_count.to_le_bytes());
        for (idx, data) in &self.model_data {
            raw.extend_from_slice(&(*idx as u32).to_le_bytes());
            raw.push(data.model_id);
            raw.push(data.rotation);
            raw.push(data.waterlogged as u8);
            raw.extend_from_slice(&data.custom_data.to_le_bytes());
        }

        // Write paint data
        let paint_count = self.paint_data.len() as u16;
        raw.extend_from_slice(&paint_count.to_le_bytes());
        for (idx, data) in &self.paint_data {
            raw.extend_from_slice(&(*idx as u32).to_le_bytes());
            raw.push(data.texture_idx);
            raw.push(data.tint_idx);
            raw.push(data.blend_mode);
        }

        // Write tint data
        let tint_count = self.tint_data.len() as u16;
        raw.extend_from_slice(&tint_count.to_le_bytes());
        for (idx, tint) in &self.tint_data {
            raw.extend_from_slice(&(*idx as u32).to_le_bytes());
            raw.push(*tint);
        }

        // Write water data
        let water_count = self.water_data.len() as u16;
        raw.extend_from_slice(&water_count.to_le_bytes());
        for (idx, water_type) in &self.water_data {
            raw.extend_from_slice(&(*idx as u32).to_le_bytes());
            raw.push(*water_type as u8);
        }

        // Compress with LZ4
        Ok(compress_prepend_size(&raw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_priority_player_position() {
        let mut manager = ChunkSyncManager::new();
        manager.last_player_pos = [16.0, 16.0, 16.0];

        let priority = manager.calculate_priority([0, 0, 0]); // Chunk containing (16,16,16)
        assert_eq!(priority, ChunkPriority::PlayerPosition);
    }

    #[test]
    fn test_chunk_sync_request() {
        let mut manager = ChunkSyncManager::new();
        manager.last_player_pos = [0.0, 0.0, 0.0];
        manager.last_look_dir = [0.0, 0.0, -1.0]; // Looking down -Z

        let request = manager.request_chunks_around([0, 0, 0], 2);
        assert!(request.is_some());

        let request = request.unwrap();
        assert!(!request.positions.is_empty());
        assert!(request.positions.len() <= 16);
    }

    #[test]
    fn test_chunk_cancellation() {
        let mut manager = ChunkSyncManager::new();
        manager.last_player_pos = [0.0, 0.0, 0.0];
        manager.last_look_dir = [0.0, 0.0, -1.0];

        // Request some chunks
        let _ = manager.request_chunks_around([0, 0, 0], 2);
        assert!(manager.pending_count() > 0);

        // Cancel them
        let positions: Vec<[i32; 3]> = manager.pending_requests.keys().copied().collect();
        manager.cancel_requests(&positions);
        assert_eq!(manager.pending_count(), 0);
    }
}

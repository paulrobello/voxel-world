//! Chunk streaming and synchronization for multiplayer.
//!
//! Handles chunk requests, prioritization, and cancellation.
//! Prioritizes chunks closest to the player in their look direction.

// Allow unused code until networking is integrated into the game
#![allow(dead_code)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use crate::chunk::{
    BlockModelData, BlockPaintData, BlockType, CHUNK_SIZE, CHUNK_VOLUME, Chunk, WaterType,
};
use crate::net::protocol::RequestChunks;
use lz4_flex::{compress_prepend_size, decompress_size_prepended};

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
    /// Chunks that have been received and applied to the world.
    received_chunks: HashSet<[i32; 3]>,
    /// Chunks queued for local generation (ChunkGenerateLocal received but not yet generated).
    /// These should NOT be re-requested, but also not considered "received" until generated.
    pending_local_generation: HashSet<[i32; 3]>,
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
            pending_local_generation: HashSet::new(),
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

                    // Skip if already received, already requested, or pending local generation
                    if self.received_chunks.contains(&pos)
                        || self.pending_requests.contains_key(&pos)
                        || self.pending_local_generation.contains(&pos)
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

    /// Marks a chunk as pending local generation (ChunkGenerateLocal received).
    /// The chunk is NOT considered received until `mark_local_generation_complete` is called.
    pub fn mark_pending_local_generation(&mut self, position: [i32; 3]) {
        self.pending_requests.remove(&position);
        self.pending_local_generation.insert(position);
    }

    /// Marks a locally-generated chunk as complete and received.
    /// Call this after the chunk_loader has finished generating the chunk.
    pub fn mark_local_generation_complete(&mut self, position: [i32; 3]) {
        self.pending_local_generation.remove(&position);
        self.received_chunks.insert(position);
    }

    /// Clears received chunks (e.g., on teleport or world change).
    pub fn clear_received(&mut self) {
        self.received_chunks.clear();
        self.pending_requests.clear();
        self.priority_queue.clear();
        self.pending_local_generation.clear();
    }

    /// Marks a chunk as received if it was pending local generation.
    /// Returns true if the chunk was pending local generation (and is now marked received).
    /// This should be called when a chunk is successfully applied to the world.
    pub fn try_complete_local_generation(&mut self, position: [i32; 3]) -> bool {
        if self.pending_local_generation.remove(&position) {
            self.received_chunks.insert(position);
            true
        } else {
            false
        }
    }

    /// Returns the number of pending local generation chunks.
    pub fn pending_local_count(&self) -> usize {
        self.pending_local_generation.len()
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
    /// Creates a SerializedChunk from a Chunk reference.
    /// This is used on the server side to serialize chunks for network transmission.
    pub fn from_chunk(position: [i32; 3], chunk: &Chunk) -> Self {
        // Collect block data as bytes
        let mut blocks = Vec::with_capacity(CHUNK_VOLUME);
        let mut model_data = Vec::new();
        let mut paint_data = Vec::new();
        let mut tint_data = Vec::new();
        let mut water_data = Vec::new();

        for idx in 0..CHUNK_VOLUME {
            let (x, y, z) = Chunk::index_to_coords(idx);
            let block_type = chunk.get_block(x, y, z);
            blocks.push(block_type as u8);

            // Collect sparse metadata
            if block_type == BlockType::Model {
                if let Some(data) = chunk.get_model_data(x, y, z) {
                    model_data.push((idx, data));
                }
            }

            if block_type == BlockType::Painted {
                if let Some(data) = chunk.get_paint_data(x, y, z) {
                    paint_data.push((idx, data));
                }
            }

            if block_type == BlockType::TintedGlass || block_type == BlockType::Crystal {
                if let Some(tint) = chunk.get_tint_index(x, y, z) {
                    tint_data.push((idx, tint));
                }
            }

            if block_type == BlockType::Water {
                if let Some(water_type) = chunk.get_water_type(x, y, z) {
                    water_data.push((idx, water_type));
                }
            }
        }

        Self {
            position,
            version: 1, // Version 1 for initial implementation
            blocks,
            model_data,
            paint_data,
            tint_data,
            water_data,
        }
    }

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

    /// Decompresses chunk data from network transmission.
    /// Returns a SerializedChunk ready for conversion to a Chunk.
    pub fn decompress(compressed_data: &[u8]) -> Result<Self, &'static str> {
        // Decompress with LZ4
        let raw = decompress_size_prepended(compressed_data)
            .map_err(|_| "Failed to decompress chunk data")?;

        if raw.len() < CHUNK_VOLUME {
            return Err("Compressed chunk data too small");
        }

        let mut cursor = 0;

        // Read block data
        let blocks: Vec<u8> = raw[cursor..cursor + CHUNK_VOLUME].to_vec();
        cursor += CHUNK_VOLUME;

        // Helper to read u16
        let read_u16 = |data: &[u8], pos: &mut usize| -> Result<u16, &'static str> {
            if *pos + 2 > data.len() {
                return Err("Unexpected end of chunk data");
            }
            let val = u16::from_le_bytes([data[*pos], data[*pos + 1]]);
            *pos += 2;
            Ok(val)
        };

        // Helper to read u32
        let read_u32 = |data: &[u8], pos: &mut usize| -> Result<u32, &'static str> {
            if *pos + 4 > data.len() {
                return Err("Unexpected end of chunk data");
            }
            let val =
                u32::from_le_bytes([data[*pos], data[*pos + 1], data[*pos + 2], data[*pos + 3]]);
            *pos += 4;
            Ok(val)
        };

        // Helper to read u8
        let read_u8 = |data: &[u8], pos: &mut usize| -> Result<u8, &'static str> {
            if *pos >= data.len() {
                return Err("Unexpected end of chunk data");
            }
            let val = data[*pos];
            *pos += 1;
            Ok(val)
        };

        // Read model data
        let model_count = read_u16(&raw, &mut cursor)? as usize;
        let mut model_data = Vec::with_capacity(model_count);
        for _ in 0..model_count {
            let idx = read_u32(&raw, &mut cursor)? as usize;
            let model_id = read_u8(&raw, &mut cursor)?;
            let rotation = read_u8(&raw, &mut cursor)?;
            let waterlogged_byte = read_u8(&raw, &mut cursor)?;
            let custom_data = read_u32(&raw, &mut cursor)?;
            model_data.push((
                idx,
                BlockModelData {
                    model_id,
                    rotation,
                    waterlogged: waterlogged_byte != 0,
                    custom_data,
                },
            ));
        }

        // Read paint data
        let paint_count = read_u16(&raw, &mut cursor)? as usize;
        let mut paint_data = Vec::with_capacity(paint_count);
        for _ in 0..paint_count {
            let idx = read_u32(&raw, &mut cursor)? as usize;
            let texture_idx = read_u8(&raw, &mut cursor)?;
            let tint_idx = read_u8(&raw, &mut cursor)?;
            let blend_mode = read_u8(&raw, &mut cursor)?;
            paint_data.push((
                idx,
                BlockPaintData {
                    texture_idx,
                    tint_idx,
                    blend_mode,
                },
            ));
        }

        // Read tint data
        let tint_count = read_u16(&raw, &mut cursor)? as usize;
        let mut tint_data = Vec::with_capacity(tint_count);
        for _ in 0..tint_count {
            let idx = read_u32(&raw, &mut cursor)? as usize;
            let tint = read_u8(&raw, &mut cursor)?;
            tint_data.push((idx, tint));
        }

        // Read water data
        let water_count = read_u16(&raw, &mut cursor)? as usize;
        let mut water_data = Vec::with_capacity(water_count);
        for _ in 0..water_count {
            let idx = read_u32(&raw, &mut cursor)? as usize;
            let water_type_byte = read_u8(&raw, &mut cursor)?;
            water_data.push((idx, WaterType::from_u8(water_type_byte)));
        }

        Ok(Self {
            position: [0, 0, 0], // Position is set by caller from ChunkData message
            version: 0,          // Version is set by caller from ChunkData message
            blocks,
            model_data,
            paint_data,
            tint_data,
            water_data,
        })
    }

    /// Converts serialized chunk data into a Chunk struct.
    /// This creates a fully populated Chunk ready for insertion into the World.
    pub fn to_chunk(&self) -> Result<Chunk, &'static str> {
        if self.blocks.len() != CHUNK_VOLUME {
            return Err("Invalid block data size");
        }

        // Convert raw block bytes to BlockType array
        let mut blocks: Box<[BlockType; CHUNK_VOLUME]> = Box::new([BlockType::Air; CHUNK_VOLUME]);
        for (i, &byte) in self.blocks.iter().enumerate() {
            blocks[i] = BlockType::from(byte);
        }

        // Convert sparse metadata to HashMaps
        let model_data: HashMap<usize, BlockModelData> = self.model_data.iter().cloned().collect();
        let tint_data: HashMap<usize, u8> = self.tint_data.iter().cloned().collect();
        let painted_data: HashMap<usize, BlockPaintData> =
            self.paint_data.iter().cloned().collect();
        let water_data: HashMap<usize, WaterType> = self.water_data.iter().cloned().collect();

        // Count light blocks for optimization
        let mut light_block_count = 0;
        for block in blocks.iter() {
            if block.is_light_source() {
                light_block_count += 1;
            }
        }

        // Create the chunk with populated data
        Ok(Chunk::from_network_data(
            blocks,
            model_data,
            tint_data,
            painted_data,
            water_data,
            light_block_count,
        ))
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

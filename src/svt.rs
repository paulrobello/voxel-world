//! Sparse Voxel Tree (SVT-64) implementation.
//!
//! This module implements a hierarchical sparse voxel structure for efficient
//! ray traversal. Each chunk is represented as a 2-level tree:
//!
//! - Level 0: Root node with 64-bit mask (4×4×4 = 64 bricks)
//! - Level 1: Brick data (8×8×8 = 512 voxels per brick)
//!
//! Only non-empty bricks store voxel data, dramatically reducing memory
//! for sparse worlds and enabling efficient ray skipping.

use crate::chunk::{BlockType, CHUNK_SIZE, Chunk};
use nalgebra::Vector3;

/// Size of a brick in each dimension (8³ = 512 voxels per brick).
pub const BRICK_SIZE: usize = 8;

/// Number of bricks per axis in a chunk (32/8 = 4).
pub const BRICKS_PER_AXIS: usize = CHUNK_SIZE / BRICK_SIZE;

/// Total bricks per chunk (4×4×4 = 64).
pub const BRICKS_PER_CHUNK: usize = BRICKS_PER_AXIS * BRICKS_PER_AXIS * BRICKS_PER_AXIS;

/// Voxels per brick (8×8×8 = 512).
pub const VOXELS_PER_BRICK: usize = BRICK_SIZE * BRICK_SIZE * BRICK_SIZE;

/// A sparse voxel tree for a single chunk.
///
/// The tree uses a 64-bit mask to indicate which of the 64 bricks contain
/// solid blocks. Only non-empty bricks store voxel data.
#[derive(Debug, Clone)]
pub struct ChunkSVT {
    /// 64-bit mask indicating which bricks are non-empty.
    /// Bit i is set if brick i contains at least one solid block.
    pub brick_mask: u64,

    /// Voxel data for non-empty bricks.
    /// Bricks are stored in order of their set bits in brick_mask.
    /// Each brick is 512 bytes (8³ voxels, 1 byte per voxel).
    pub brick_data: Vec<[u8; VOXELS_PER_BRICK]>,

    /// Per-brick minimum distance to nearest solid voxel.
    /// Used for sphere-tracing optimization within the tree traversal.
    /// Distance is in brick units (0 = brick is solid, 1 = adjacent brick has solid).
    pub brick_distances: [u8; BRICKS_PER_CHUNK],
}

impl ChunkSVT {
    /// Creates an empty sparse voxel tree.
    #[allow(dead_code)]
    pub fn empty() -> Self {
        Self {
            brick_mask: 0,
            brick_data: Vec::new(),
            brick_distances: [255; BRICKS_PER_CHUNK],
        }
    }

    /// Builds a sparse voxel tree from chunk data.
    pub fn from_chunk(chunk: &Chunk) -> Self {
        let mut brick_mask = 0u64;
        let mut brick_data = Vec::new();
        let mut brick_has_solid = [false; BRICKS_PER_CHUNK];

        // First pass: check which bricks have solid blocks and collect data
        for bz in 0..BRICKS_PER_AXIS {
            for by in 0..BRICKS_PER_AXIS {
                for bx in 0..BRICKS_PER_AXIS {
                    let brick_idx =
                        bx + by * BRICKS_PER_AXIS + bz * BRICKS_PER_AXIS * BRICKS_PER_AXIS;
                    let mut brick_voxels = [0u8; VOXELS_PER_BRICK];
                    let mut has_solid = false;

                    // Extract voxel data for this brick
                    for vz in 0..BRICK_SIZE {
                        for vy in 0..BRICK_SIZE {
                            for vx in 0..BRICK_SIZE {
                                let world_x = bx * BRICK_SIZE + vx;
                                let world_y = by * BRICK_SIZE + vy;
                                let world_z = bz * BRICK_SIZE + vz;

                                let block = chunk.get_block(world_x, world_y, world_z);
                                let block_u8 = block as u8;
                                let voxel_idx = vx + vy * BRICK_SIZE + vz * BRICK_SIZE * BRICK_SIZE;
                                brick_voxels[voxel_idx] = block_u8;

                                if block != BlockType::Air {
                                    has_solid = true;
                                }
                            }
                        }
                    }

                    if has_solid {
                        brick_mask |= 1u64 << brick_idx;
                        brick_data.push(brick_voxels);
                        brick_has_solid[brick_idx] = true;
                    }
                }
            }
        }

        // Calculate per-brick minimum distances using Manhattan propagation
        let brick_distances = Self::calculate_brick_distances(&brick_has_solid);

        Self {
            brick_mask,
            brick_data,
            brick_distances,
        }
    }

    /// Calculates minimum Manhattan distance from each brick to nearest solid brick.
    fn calculate_brick_distances(has_solid: &[bool; BRICKS_PER_CHUNK]) -> [u8; BRICKS_PER_CHUNK] {
        let mut distances = [255u8; BRICKS_PER_CHUNK];

        // Initialize solid bricks with distance 0
        for (idx, &solid) in has_solid.iter().enumerate() {
            if solid {
                distances[idx] = 0;
            }
        }

        // Propagate distances (simple 3D BFS-like propagation)
        // We do multiple passes until convergence
        for _pass in 0..BRICKS_PER_AXIS {
            let mut changed = false;
            for bz in 0..BRICKS_PER_AXIS {
                for by in 0..BRICKS_PER_AXIS {
                    for bx in 0..BRICKS_PER_AXIS {
                        let idx =
                            bx + by * BRICKS_PER_AXIS + bz * BRICKS_PER_AXIS * BRICKS_PER_AXIS;
                        if distances[idx] == 0 {
                            continue;
                        }

                        let mut min_neighbor = 255u8;

                        // Check 6-connected neighbors
                        if bx > 0 {
                            let n = (bx - 1)
                                + by * BRICKS_PER_AXIS
                                + bz * BRICKS_PER_AXIS * BRICKS_PER_AXIS;
                            min_neighbor = min_neighbor.min(distances[n]);
                        }
                        if bx < BRICKS_PER_AXIS - 1 {
                            let n = (bx + 1)
                                + by * BRICKS_PER_AXIS
                                + bz * BRICKS_PER_AXIS * BRICKS_PER_AXIS;
                            min_neighbor = min_neighbor.min(distances[n]);
                        }
                        if by > 0 {
                            let n = bx
                                + (by - 1) * BRICKS_PER_AXIS
                                + bz * BRICKS_PER_AXIS * BRICKS_PER_AXIS;
                            min_neighbor = min_neighbor.min(distances[n]);
                        }
                        if by < BRICKS_PER_AXIS - 1 {
                            let n = bx
                                + (by + 1) * BRICKS_PER_AXIS
                                + bz * BRICKS_PER_AXIS * BRICKS_PER_AXIS;
                            min_neighbor = min_neighbor.min(distances[n]);
                        }
                        if bz > 0 {
                            let n = bx
                                + by * BRICKS_PER_AXIS
                                + (bz - 1) * BRICKS_PER_AXIS * BRICKS_PER_AXIS;
                            min_neighbor = min_neighbor.min(distances[n]);
                        }
                        if bz < BRICKS_PER_AXIS - 1 {
                            let n = bx
                                + by * BRICKS_PER_AXIS
                                + (bz + 1) * BRICKS_PER_AXIS * BRICKS_PER_AXIS;
                            min_neighbor = min_neighbor.min(distances[n]);
                        }

                        let new_dist = min_neighbor.saturating_add(1);
                        if new_dist < distances[idx] {
                            distances[idx] = new_dist;
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }

        distances
    }

    /// Returns true if the tree is completely empty (all air).
    #[inline]
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.brick_mask == 0
    }

    /// Returns true if a specific brick is non-empty.
    #[inline]
    #[allow(dead_code)]
    pub fn is_brick_solid(&self, brick_x: usize, brick_y: usize, brick_z: usize) -> bool {
        let idx = brick_x + brick_y * BRICKS_PER_AXIS + brick_z * BRICKS_PER_AXIS * BRICKS_PER_AXIS;
        (self.brick_mask & (1u64 << idx)) != 0
    }

    /// Returns the number of non-empty bricks.
    #[inline]
    #[allow(dead_code)]
    pub fn brick_count(&self) -> usize {
        self.brick_mask.count_ones() as usize
    }

    /// Gets the voxel data index for a brick (None if brick is empty).
    #[inline]
    #[allow(dead_code)]
    pub fn get_brick_data_index(
        &self,
        brick_x: usize,
        brick_y: usize,
        brick_z: usize,
    ) -> Option<usize> {
        let brick_idx =
            brick_x + brick_y * BRICKS_PER_AXIS + brick_z * BRICKS_PER_AXIS * BRICKS_PER_AXIS;
        if (self.brick_mask & (1u64 << brick_idx)) == 0 {
            return None;
        }
        // Count how many bricks come before this one
        let mask_before = self.brick_mask & ((1u64 << brick_idx) - 1);
        Some(mask_before.count_ones() as usize)
    }

    /// Gets a block from the tree by local coordinates.
    #[allow(dead_code)]
    pub fn get_block(&self, x: usize, y: usize, z: usize) -> BlockType {
        let brick_x = x / BRICK_SIZE;
        let brick_y = y / BRICK_SIZE;
        let brick_z = z / BRICK_SIZE;

        if let Some(data_idx) = self.get_brick_data_index(brick_x, brick_y, brick_z) {
            let local_x = x % BRICK_SIZE;
            let local_y = y % BRICK_SIZE;
            let local_z = z % BRICK_SIZE;
            let voxel_idx = local_x + local_y * BRICK_SIZE + local_z * BRICK_SIZE * BRICK_SIZE;
            BlockType::from(self.brick_data[data_idx][voxel_idx])
        } else {
            BlockType::Air
        }
    }

    /// Calculates the total memory size in bytes.
    #[allow(dead_code)]
    pub fn memory_size(&self) -> usize {
        std::mem::size_of::<u64>()  // brick_mask
            + self.brick_data.len() * VOXELS_PER_BRICK  // brick data
            + BRICKS_PER_CHUNK // brick_distances
    }
}

/// GPU-ready format for the sparse voxel tree.
///
/// The entire world's SVT data is packed into a linear buffer:
/// - Chunk headers (one per chunk)
/// - Brick data (variable length per chunk)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WorldSVT {
    /// SVT for each loaded chunk, keyed by chunk position.
    pub chunks: std::collections::HashMap<Vector3<i32>, ChunkSVT>,
}

#[allow(dead_code)]
impl WorldSVT {
    /// Creates an empty world SVT.
    pub fn new() -> Self {
        Self {
            chunks: std::collections::HashMap::new(),
        }
    }

    /// Inserts or updates a chunk's SVT.
    pub fn insert(&mut self, pos: Vector3<i32>, chunk: &Chunk) {
        let svt = ChunkSVT::from_chunk(chunk);
        self.chunks.insert(pos, svt);
    }

    /// Removes a chunk's SVT.
    pub fn remove(&mut self, pos: Vector3<i32>) {
        self.chunks.remove(&pos);
    }

    /// Returns the total memory size in bytes.
    pub fn total_memory(&self) -> usize {
        self.chunks.values().map(|svt| svt.memory_size()).sum()
    }

    /// Packs the world SVT into a GPU-ready buffer format.
    ///
    /// Layout for GPU:
    /// - Per-chunk header: 8 bytes brick_mask + 64 bytes brick_distances + 4 bytes data_offset
    /// - Brick data: 512 bytes per non-empty brick
    ///
    /// Returns (header_buffer, data_buffer).
    pub fn pack_for_gpu(&self, chunk_positions: &[Vector3<i32>]) -> (Vec<u8>, Vec<u8>) {
        // Header per chunk: brick_mask (8) + brick_distances (64) + data_offset (4) = 76 bytes
        const HEADER_SIZE: usize = 8 + 64 + 4;

        let mut headers = vec![0u8; chunk_positions.len() * HEADER_SIZE];
        let mut brick_data = Vec::new();

        for (chunk_idx, &pos) in chunk_positions.iter().enumerate() {
            let header_offset = chunk_idx * HEADER_SIZE;

            if let Some(svt) = self.chunks.get(&pos) {
                // Write brick mask (8 bytes)
                headers[header_offset..header_offset + 8]
                    .copy_from_slice(&svt.brick_mask.to_le_bytes());

                // Write brick distances (64 bytes)
                headers[header_offset + 8..header_offset + 8 + 64]
                    .copy_from_slice(&svt.brick_distances);

                // Write data offset (4 bytes)
                let data_offset = brick_data.len() as u32;
                headers[header_offset + 72..header_offset + 76]
                    .copy_from_slice(&data_offset.to_le_bytes());

                // Append brick data
                for brick in &svt.brick_data {
                    brick_data.extend_from_slice(brick);
                }
            } else {
                // Empty chunk: all zeros (mask = 0, distances = 255, offset = 0)
                headers[header_offset + 8..header_offset + 72].fill(255);
            }
        }

        (headers, brick_data)
    }
}

impl Default for WorldSVT {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_chunk_svt() {
        let chunk = Chunk::new();
        let svt = ChunkSVT::from_chunk(&chunk);

        assert!(svt.is_empty());
        assert_eq!(svt.brick_count(), 0);
        assert_eq!(svt.brick_data.len(), 0);
    }

    #[test]
    fn test_single_block_svt() {
        let mut chunk = Chunk::new();
        chunk.set_block(4, 4, 4, BlockType::Stone);
        let svt = ChunkSVT::from_chunk(&chunk);

        assert!(!svt.is_empty());
        assert_eq!(svt.brick_count(), 1);
        assert!(svt.is_brick_solid(0, 0, 0));
        assert!(!svt.is_brick_solid(1, 0, 0));

        // Verify we can read the block back
        assert_eq!(svt.get_block(4, 4, 4), BlockType::Stone);
        assert_eq!(svt.get_block(0, 0, 0), BlockType::Air);
    }

    #[test]
    fn test_multiple_bricks_svt() {
        let mut chunk = Chunk::new();
        // Place blocks in different bricks
        chunk.set_block(0, 0, 0, BlockType::Stone); // Brick (0,0,0)
        chunk.set_block(8, 0, 0, BlockType::Dirt); // Brick (1,0,0)
        chunk.set_block(0, 8, 0, BlockType::Grass); // Brick (0,1,0)
        chunk.set_block(24, 24, 24, BlockType::Planks); // Brick (3,3,3)

        let svt = ChunkSVT::from_chunk(&chunk);

        assert_eq!(svt.brick_count(), 4);
        assert!(svt.is_brick_solid(0, 0, 0));
        assert!(svt.is_brick_solid(1, 0, 0));
        assert!(svt.is_brick_solid(0, 1, 0));
        assert!(svt.is_brick_solid(3, 3, 3));
        assert!(!svt.is_brick_solid(2, 2, 2));

        // Verify blocks
        assert_eq!(svt.get_block(0, 0, 0), BlockType::Stone);
        assert_eq!(svt.get_block(8, 0, 0), BlockType::Dirt);
        assert_eq!(svt.get_block(0, 8, 0), BlockType::Grass);
        assert_eq!(svt.get_block(24, 24, 24), BlockType::Planks);
    }

    #[test]
    fn test_brick_distances() {
        let mut chunk = Chunk::new();
        // Place a solid brick at (0,0,0)
        chunk.set_block(0, 0, 0, BlockType::Stone);

        let svt = ChunkSVT::from_chunk(&chunk);

        // Brick (0,0,0) should have distance 0
        assert_eq!(svt.brick_distances[0], 0);

        // Brick (1,0,0) should have distance 1
        let idx_100 = 1 + 0 * BRICKS_PER_AXIS + 0 * BRICKS_PER_AXIS * BRICKS_PER_AXIS;
        assert_eq!(svt.brick_distances[idx_100], 1);

        // Brick (2,0,0) should have distance 2
        let idx_200 = 2 + 0 * BRICKS_PER_AXIS + 0 * BRICKS_PER_AXIS * BRICKS_PER_AXIS;
        assert_eq!(svt.brick_distances[idx_200], 2);
    }

    #[test]
    fn test_memory_savings() {
        // Empty chunk should use minimal memory
        let empty_chunk = Chunk::new();
        let empty_svt = ChunkSVT::from_chunk(&empty_chunk);
        let empty_mem = empty_svt.memory_size();

        // Chunk with one brick should use ~512 bytes for data
        let mut one_block = Chunk::new();
        one_block.set_block(0, 0, 0, BlockType::Stone);
        let one_svt = ChunkSVT::from_chunk(&one_block);
        let one_mem = one_svt.memory_size();

        // Full chunk would use all 64 bricks
        let full_chunk = Chunk::filled(BlockType::Stone);
        let full_svt = ChunkSVT::from_chunk(&full_chunk);
        let full_mem = full_svt.memory_size();

        // Flat chunk data is 32KB
        let flat_size = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

        // SVT overhead: 8 bytes mask + 64 bytes distances = 72 bytes
        let svt_overhead = 8 + BRICKS_PER_CHUNK;

        assert!(empty_mem < 100); // ~72 bytes for mask + distances
        assert!(one_mem < 700); // ~72 + 512 bytes
        // Full chunk has slight overhead from mask + distances
        assert!(full_mem <= flat_size + svt_overhead);

        // SVT should save significant memory for sparse chunks
        assert!(one_mem < flat_size / 10);
    }
}

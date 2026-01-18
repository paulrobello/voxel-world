//! Column data caching for efficient chunk generation.
//!
//! Pre-computes column data (height, biome, river info) for a chunk plus 1-block overlap,
//! reducing noise evaluations from ~40,000 per chunk to ~1,156.

use crate::chunk::CHUNK_SIZE;
use crate::world_gen::terrain::{ColumnData, TerrainGenerator};

/// Size of the cache including 1-block overlap on each side.
/// 32 + 2 = 34 columns per axis.
const CACHE_SIZE: usize = CHUNK_SIZE + 2;

/// Cached column data for a single chunk plus 1-block neighbors.
///
/// This cache pre-computes all column data (height, biome, river water level, hash)
/// for a 34×34 grid centered on the chunk. This avoids redundant noise evaluations
/// when generating terrain, trees, and vegetation.
///
/// # Performance Impact
/// - **Before**: ~40,000 noise evaluations per chunk (1024 columns × ~40 noise calls each)
/// - **After**: ~1,156 noise evaluations (34×34 = 1,156 columns, computed once)
/// - **Speedup**: ~35x reduction in column data lookups
pub struct ColumnDataCache {
    /// Pre-computed column data for 34×34 grid.
    /// Flattened for cache-friendly access: index = (lx + 1) + (lz + 1) * CACHE_SIZE
    data: Box<[ColumnData; CACHE_SIZE * CACHE_SIZE]>,
    /// World X coordinate of chunk origin (lx=0 maps to this)
    #[allow(dead_code)]
    chunk_world_x: i32,
    /// World Z coordinate of chunk origin (lz=0 maps to this)
    #[allow(dead_code)]
    chunk_world_z: i32,
}

impl ColumnDataCache {
    /// Creates a new column data cache for the given chunk position.
    ///
    /// Pre-computes all 34×34 = 1,156 column data entries in one pass.
    ///
    /// # Arguments
    /// * `terrain` - The terrain generator to use for column data lookup
    /// * `chunk_world_x` - World X coordinate of chunk origin (chunk_pos.x * CHUNK_SIZE)
    /// * `chunk_world_z` - World Z coordinate of chunk origin (chunk_pos.z * CHUNK_SIZE)
    pub fn for_chunk(terrain: &TerrainGenerator, chunk_world_x: i32, chunk_world_z: i32) -> Self {
        // Pre-allocate with zeroed data, then fill
        let mut data = Box::new(
            [ColumnData {
                height: 0,
                biome: crate::world_gen::biome::BiomeType::Plains,
                river_water_level: None,
                hash: 0,
            }; CACHE_SIZE * CACHE_SIZE],
        );

        // Fill cache: local coords from -1 to 32 (inclusive)
        // Index 0 = local -1, index 33 = local 32
        for lz_offset in 0..CACHE_SIZE {
            for lx_offset in 0..CACHE_SIZE {
                let world_x = chunk_world_x + lx_offset as i32 - 1;
                let world_z = chunk_world_z + lz_offset as i32 - 1;
                let idx = lx_offset + lz_offset * CACHE_SIZE;
                data[idx] = terrain.get_column_data(world_x, world_z);
            }
        }

        Self {
            data,
            chunk_world_x,
            chunk_world_z,
        }
    }

    /// Gets column data for a local chunk coordinate.
    ///
    /// # Arguments
    /// * `lx` - Local X coordinate within chunk (0..31)
    /// * `lz` - Local Z coordinate within chunk (0..31)
    ///
    /// # Panics
    /// Panics if coordinates are outside valid range.
    #[inline]
    pub fn get_local(&self, lx: usize, lz: usize) -> &ColumnData {
        debug_assert!(lx < CHUNK_SIZE, "lx out of bounds: {}", lx);
        debug_assert!(lz < CHUNK_SIZE, "lz out of bounds: {}", lz);
        // lx=0 maps to index 1 (offset 0 is for lx=-1)
        let idx = (lx + 1) + (lz + 1) * CACHE_SIZE;
        &self.data[idx]
    }

    /// Gets column data for a world coordinate within or adjacent to the chunk.
    ///
    /// Returns `Some(&ColumnData)` if the coordinate is in the cache,
    /// `None` if it's outside the 34×34 cached region.
    ///
    /// # Arguments
    /// * `world_x` - World X coordinate
    /// * `world_z` - World Z coordinate
    #[inline]
    #[allow(dead_code)]
    pub fn get_world(&self, world_x: i32, world_z: i32) -> Option<&ColumnData> {
        let rel_x = world_x - self.chunk_world_x + 1;
        let rel_z = world_z - self.chunk_world_z + 1;

        if rel_x < 0 || rel_x >= CACHE_SIZE as i32 || rel_z < 0 || rel_z >= CACHE_SIZE as i32 {
            return None;
        }

        let idx = rel_x as usize + rel_z as usize * CACHE_SIZE;
        Some(&self.data[idx])
    }

    /// Gets column data for a world coordinate, falling back to terrain generator if not cached.
    ///
    /// This is useful for tree generation where trees might sample heights
    /// outside the immediate chunk boundary.
    #[inline]
    #[allow(dead_code)]
    pub fn get_world_or_compute(
        &self,
        terrain: &TerrainGenerator,
        world_x: i32,
        world_z: i32,
    ) -> ColumnData {
        if let Some(col) = self.get_world(world_x, world_z) {
            *col
        } else {
            terrain.get_column_data(world_x, world_z)
        }
    }

    /// Returns the height at a local coordinate.
    #[inline]
    #[allow(dead_code)]
    pub fn height(&self, lx: usize, lz: usize) -> i32 {
        self.get_local(lx, lz).height
    }

    /// Returns the biome at a local coordinate.
    #[inline]
    #[allow(dead_code)]
    pub fn biome(&self, lx: usize, lz: usize) -> crate::world_gen::biome::BiomeType {
        self.get_local(lx, lz).biome
    }

    /// Returns the hash at a local coordinate (for placement randomness).
    #[inline]
    #[allow(dead_code)]
    pub fn hash(&self, lx: usize, lz: usize) -> i32 {
        self.get_local(lx, lz).hash
    }

    /// Returns the chunk world X coordinate.
    #[inline]
    #[allow(dead_code)]
    pub fn chunk_world_x(&self) -> i32 {
        self.chunk_world_x
    }

    /// Returns the chunk world Z coordinate.
    #[inline]
    #[allow(dead_code)]
    pub fn chunk_world_z(&self) -> i32 {
        self.chunk_world_z
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_local_access() {
        let terrain = TerrainGenerator::new(12345);
        let cache = ColumnDataCache::for_chunk(&terrain, 0, 0);

        // Check that local coordinates work
        for lz in 0..CHUNK_SIZE {
            for lx in 0..CHUNK_SIZE {
                let col = cache.get_local(lx, lz);
                // Height should be reasonable
                assert!(col.height > -100 && col.height < 512);
            }
        }
    }

    #[test]
    fn test_cache_world_access() {
        let terrain = TerrainGenerator::new(12345);
        let cache = ColumnDataCache::for_chunk(&terrain, 64, 64);

        // Check world coordinate access
        // Valid range: 63..97 (64-1 to 64+32)
        assert!(cache.get_world(63, 64).is_some()); // -1 offset
        assert!(cache.get_world(64, 64).is_some()); // chunk origin
        assert!(cache.get_world(95, 95).is_some()); // chunk end (64+31)
        assert!(cache.get_world(96, 96).is_some()); // +1 offset
        assert!(cache.get_world(62, 64).is_none()); // outside cache
        assert!(cache.get_world(97, 64).is_none()); // outside cache
    }

    #[test]
    fn test_cache_matches_direct() {
        let terrain = TerrainGenerator::new(12345);
        let cache = ColumnDataCache::for_chunk(&terrain, 128, 256);

        // Verify cached data matches direct lookup
        for lz in 0..CHUNK_SIZE {
            for lx in 0..CHUNK_SIZE {
                let world_x = 128 + lx as i32;
                let world_z = 256 + lz as i32;

                let cached = cache.get_local(lx, lz);
                let direct = terrain.get_column_data(world_x, world_z);

                assert_eq!(cached.height, direct.height);
                assert_eq!(cached.biome, direct.biome);
                assert_eq!(cached.hash, direct.hash);
            }
        }
    }
}

//! River generation system.
//!
//! Uses Minecraft-style region boundary detection. Rivers form at the
//! EXACT boundaries between quantized noise regions, creating naturally
//! connected linear features.
//!
//! ## Algorithm
//! 1. Quantize noise into discrete "regions" (e.g., 4 region types)
//! 2. Rivers form where a point's region differs from its neighbors
//! 3. This creates continuous boundary lines, not scattered points
//!
//! ## Key Design
//! - Rivers have a FIXED water level (not per-column)
//! - Terrain is carved DOWN to below the water level
//! - Rivers only form where terrain is high enough above sea level

use crate::terrain_gen::BiomeType;
use crate::world_gen::SEA_LEVEL;
use noise::{NoiseFn, Perlin};

/// Base water level for all rivers (above sea level).
/// Rivers carve terrain down to below this level.
pub const RIVER_WATER_LEVEL: i32 = SEA_LEVEL + 8; // 83

/// Minimum terrain height for rivers to form.
/// Terrain must be at least this high for a river to carve through it.
const MIN_RIVER_TERRAIN: i32 = RIVER_WATER_LEVEL + 4; // 87

/// River generator using quantized region boundaries.
#[derive(Clone)]
pub struct RiverGenerator {
    /// Region noise - quantized to create distinct regions
    region_noise: Perlin,
    /// Secondary noise for tributaries
    tributary_noise: Perlin,
    /// Noise for width/depth variation
    variation_noise: Perlin,
}

impl RiverGenerator {
    /// Creates a new river generator with the given seed.
    pub fn new(seed: u32) -> Self {
        Self {
            region_noise: Perlin::new(seed + 500),
            tributary_noise: Perlin::new(seed + 501),
            variation_noise: Perlin::new(seed + 502),
        }
    }

    /// Check if a position is within a river channel.
    ///
    /// Rivers form at boundaries between quantized noise regions.
    /// This creates naturally connected, linear river paths.
    pub fn get_river_at(
        &self,
        world_x: i32,
        world_z: i32,
        terrain_height: i32,
        biome: BiomeType,
    ) -> Option<RiverInfo> {
        // Check if rivers are enabled for this biome
        if !self.is_river_enabled(biome) {
            return None;
        }

        // Rivers need terrain high enough to carve into
        if terrain_height < MIN_RIVER_TERRAIN {
            return None;
        }

        let x = world_x as f64;
        let z = world_z as f64;

        // Check for main river (large-scale region boundaries)
        if let Some(info) = self.check_main_river(x, z, terrain_height, biome) {
            return Some(info);
        }

        // Check for tributary (smaller-scale boundaries)
        if let Some(info) = self.check_tributary(x, z, terrain_height, biome) {
            return Some(info);
        }

        None
    }

    /// Check for main river at region boundary.
    fn check_main_river(
        &self,
        x: f64,
        z: f64,
        terrain_height: i32,
        biome: BiomeType,
    ) -> Option<RiverInfo> {
        // Scale for large regions (~250 blocks across)
        let scale = 0.004;

        // Number of distinct regions - more regions = more boundaries = more rivers
        let num_regions = 5.0;

        // Quantize center point into a region
        let center_val = self.region_noise.get([x * scale, z * scale]);
        let center_region = (center_val * num_regions).floor() as i32;

        // Check distance for boundary detection - wider = wider rivers
        let river_half_width = self.get_river_half_width(x, z, biome, true);

        // Check if any neighbor is in a different region
        let at_boundary = self.is_at_region_boundary(
            x,
            z,
            scale,
            center_region,
            num_regions,
            river_half_width,
            &self.region_noise,
        );

        if !at_boundary {
            return None;
        }

        // Calculate carving depth - terrain carved down to below water level
        let base_depth = (terrain_height - RIVER_WATER_LEVEL + 3).max(3);
        let variation = self.variation_noise.get([x * 0.02, z * 0.02]) * 0.3 + 0.85;
        // Minimum depth ensures carved terrain is at least 1 block below water level
        let min_depth = terrain_height - RIVER_WATER_LEVEL + 1;
        let depth = ((base_depth as f64) * variation)
            .round()
            .max(min_depth as f64) as i32;

        Some(RiverInfo {
            width: river_half_width * 2.0,
            depth,
            river_type: RiverType::MainRiver,
            water_level: RIVER_WATER_LEVEL,
        })
    }

    /// Check for tributary at smaller region boundary.
    fn check_tributary(
        &self,
        x: f64,
        z: f64,
        terrain_height: i32,
        biome: BiomeType,
    ) -> Option<RiverInfo> {
        // Higher threshold for tributaries - only in wetter biomes
        if !self.has_tributaries(biome) {
            return None;
        }

        // Scale for smaller regions (~120 blocks across)
        let scale = 0.008;
        let num_regions = 4.0;

        let center_val = self.tributary_noise.get([x * scale, z * scale]);
        let center_region = (center_val * num_regions).floor() as i32;

        let river_half_width = self.get_river_half_width(x, z, biome, false);

        let at_boundary = self.is_at_region_boundary(
            x,
            z,
            scale,
            center_region,
            num_regions,
            river_half_width,
            &self.tributary_noise,
        );

        if !at_boundary {
            return None;
        }

        let base_depth = (terrain_height - RIVER_WATER_LEVEL + 2).max(2);
        let variation = self.variation_noise.get([x * 0.03, z * 0.03]) * 0.3 + 0.85;
        // Minimum depth ensures carved terrain is at least 1 block below water level
        let min_depth = terrain_height - RIVER_WATER_LEVEL + 1;
        let depth = ((base_depth as f64) * variation)
            .round()
            .max(min_depth as f64) as i32;

        Some(RiverInfo {
            width: river_half_width * 2.0,
            depth,
            river_type: if terrain_height > 120 {
                RiverType::MountainStream
            } else {
                RiverType::Tributary
            },
            water_level: RIVER_WATER_LEVEL,
        })
    }

    /// Check if position is at a region boundary by comparing with neighbors.
    #[allow(clippy::too_many_arguments)]
    fn is_at_region_boundary(
        &self,
        x: f64,
        z: f64,
        scale: f64,
        center_region: i32,
        num_regions: f64,
        check_dist: f64,
        noise: &Perlin,
    ) -> bool {
        // Check 4 cardinal directions
        for (dx, dz) in &[(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            let nx = x + dx * check_dist;
            let nz = z + dz * check_dist;
            let neighbor_val = noise.get([nx * scale, nz * scale]);
            let neighbor_region = (neighbor_val * num_regions).floor() as i32;

            if neighbor_region != center_region {
                return true;
            }
        }
        false
    }

    /// Get river half-width with variation.
    fn get_river_half_width(&self, x: f64, z: f64, biome: BiomeType, is_main: bool) -> f64 {
        let base = if is_main { 5.0 } else { 3.0 };

        // Biome modifier
        let biome_mult = match biome {
            BiomeType::Jungle => 1.3,
            BiomeType::Swamp => 1.2,
            BiomeType::Desert => 0.7,
            BiomeType::Mountains => 0.6,
            _ => 1.0,
        };

        // Add variation
        let variation = self.variation_noise.get([x * 0.008, z * 0.008]) * 0.3 + 0.85;

        base * biome_mult * variation
    }

    /// Check if rivers are enabled for this biome.
    fn is_river_enabled(&self, biome: BiomeType) -> bool {
        #[allow(deprecated)]
        match biome {
            BiomeType::Ocean => false,
            BiomeType::Beach => false, // Rivers end at beaches, not cross them
            BiomeType::LushCaves | BiomeType::DripstoneCaves | BiomeType::DeepDark => false,
            _ => true,
        }
    }

    /// Check if tributaries are enabled for this biome.
    #[allow(deprecated)]
    fn has_tributaries(&self, biome: BiomeType) -> bool {
        matches!(
            biome,
            BiomeType::Jungle
                | BiomeType::Swamp
                | BiomeType::Forest
                | BiomeType::DarkForest
                | BiomeType::Taiga
        )
    }

    /// Get terrain height modification for river carving.
    pub fn get_height_modification(
        &self,
        _world_x: i32,
        _world_z: i32,
        river_info: &RiverInfo,
    ) -> i32 {
        river_info.depth
    }

    /// Check if position should have river banks (sand/gravel).
    pub fn is_river_bank(&self, world_x: i32, world_z: i32, terrain_height: i32) -> bool {
        if !(MIN_RIVER_TERRAIN - 2..=MIN_RIVER_TERRAIN + 10).contains(&terrain_height) {
            return false;
        }

        let x = world_x as f64;
        let z = world_z as f64;
        let scale = 0.004;
        let num_regions = 5.0;

        let center_val = self.region_noise.get([x * scale, z * scale]);
        let center_region = (center_val * num_regions).floor() as i32;

        // Bank is slightly wider than river - check at wider distance
        let bank_dist = 8.0;
        let river_dist = 5.0;

        // Must be near boundary (bank) but not AT boundary (river)
        let at_bank = self.is_at_region_boundary(
            x,
            z,
            scale,
            center_region,
            num_regions,
            bank_dist,
            &self.region_noise,
        );
        let at_river = self.is_at_region_boundary(
            x,
            z,
            scale,
            center_region,
            num_regions,
            river_dist,
            &self.region_noise,
        );

        at_bank && !at_river
    }

    /// Get water type for rivers in a biome.
    #[allow(dead_code)]
    pub fn get_water_type(&self, biome: BiomeType) -> crate::chunk::WaterType {
        biome.water_type()
    }
}

/// Information about a river at a specific location.
#[derive(Debug, Clone, Copy)]
pub struct RiverInfo {
    /// Width of the river in blocks
    #[allow(dead_code)]
    pub width: f64,
    /// Depth to carve terrain
    pub depth: i32,
    /// Type of river
    pub river_type: RiverType,
    /// Water surface level (FIXED for all river positions)
    pub water_level: i32,
}

/// Types of rivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiverType {
    /// Large main river
    MainRiver,
    /// Smaller tributary stream
    Tributary,
    /// Narrow mountain stream
    MountainStream,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_river_generator_consistency() {
        let rivers = RiverGenerator::new(12345);

        // Same coordinates should give same result
        let result1 = rivers.get_river_at(100, 100, 95, BiomeType::Plains);
        let result2 = rivers.get_river_at(100, 100, 95, BiomeType::Plains);

        match (result1, result2) {
            (Some(r1), Some(r2)) => {
                assert!((r1.width - r2.width).abs() < 0.001);
                assert_eq!(r1.depth, r2.depth);
                assert_eq!(r1.water_level, r2.water_level);
            }
            (None, None) => {}
            _ => panic!("Inconsistent river generation"),
        }
    }

    #[test]
    fn test_fixed_water_level() {
        let rivers = RiverGenerator::new(12345);

        // All rivers should have the same water level
        for x in 0..200 {
            for z in 0..200 {
                if let Some(info) = rivers.get_river_at(x, z, 95, BiomeType::Plains) {
                    assert_eq!(
                        info.water_level, RIVER_WATER_LEVEL,
                        "All rivers must have fixed water level"
                    );
                }
            }
        }
    }

    #[test]
    fn test_no_rivers_in_low_terrain() {
        let rivers = RiverGenerator::new(12345);

        // Rivers shouldn't form in terrain below minimum
        for x in 0..100 {
            for z in 0..100 {
                let result = rivers.get_river_at(x, z, MIN_RIVER_TERRAIN - 1, BiomeType::Plains);
                assert!(
                    result.is_none(),
                    "No rivers should form below minimum terrain height"
                );
            }
        }
    }

    #[test]
    fn test_ocean_has_no_rivers() {
        let rivers = RiverGenerator::new(12345);

        for x in 0..50 {
            for z in 0..50 {
                assert!(
                    rivers.get_river_at(x, z, 95, BiomeType::Ocean).is_none(),
                    "Ocean should not have rivers"
                );
            }
        }
    }

    #[test]
    fn test_river_carves_below_water() {
        let rivers = RiverGenerator::new(12345);

        for x in 0..200 {
            for z in 0..200 {
                let terrain_height = 100;
                if let Some(info) = rivers.get_river_at(x, z, terrain_height, BiomeType::Plains) {
                    let carved_height = terrain_height - info.depth;
                    assert!(
                        carved_height < info.water_level,
                        "Carved terrain ({}) must be below water level ({})",
                        carved_height,
                        info.water_level
                    );
                }
            }
        }
    }
}

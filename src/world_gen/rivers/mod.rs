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

/// Maximum water level for rivers (caps how high river water can be).
/// On high terrain, water is at this level. On lower terrain, water
/// tracks the terrain height to avoid floating water.
pub const MAX_RIVER_WATER_LEVEL: i32 = SEA_LEVEL + 8; // 83

/// Minimum terrain height for rivers to form.
/// Just above sea level - rivers can form on most land.
const MIN_RIVER_TERRAIN: i32 = SEA_LEVEL + 1; // 76

/// River generator using quantized region boundaries.
#[derive(Clone)]
pub struct RiverGenerator {
    /// Region noise - quantized to create distinct regions
    region_noise: Perlin,
    /// Secondary noise for tributaries
    tributary_noise: Perlin,
    /// Noise for width/depth variation
    variation_noise: Perlin,
    /// Domain warping noise X - breaks up radial patterns
    warp_noise_x: Perlin,
    /// Domain warping noise Z - breaks up radial patterns
    warp_noise_z: Perlin,
}

/// Primary warp strength (how much coordinates are offset)
const WARP_STRENGTH_1: f64 = 400.0;
/// Secondary warp strength (finer detail)
const WARP_STRENGTH_2: f64 = 150.0;
/// Tertiary warp strength (finest detail)
const WARP_STRENGTH_3: f64 = 50.0;
/// Primary warp scale (very large-scale curves)
const WARP_SCALE_1: f64 = 0.0004;
/// Secondary warp scale (medium curves)
const WARP_SCALE_2: f64 = 0.0015;
/// Tertiary warp scale (fine curves)
const WARP_SCALE_3: f64 = 0.005;

impl RiverGenerator {
    /// Creates a new river generator with the given seed.
    pub fn new(seed: u32) -> Self {
        Self {
            region_noise: Perlin::new(seed + 500),
            tributary_noise: Perlin::new(seed + 501),
            variation_noise: Perlin::new(seed + 502),
            warp_noise_x: Perlin::new(seed + 503),
            warp_noise_z: Perlin::new(seed + 504),
        }
    }

    /// Apply multi-octave domain warping to break up radial patterns.
    /// Uses three octaves of noise at different scales plus asymmetric offsets
    /// to prevent Perlin noise gradient convergence artifacts.
    fn warp_coordinates(&self, x: f64, z: f64) -> (f64, f64) {
        // Octave 1: Large-scale warping (biggest curves)
        let warp1_x = self.warp_noise_x.get([x * WARP_SCALE_1, z * WARP_SCALE_1]) * WARP_STRENGTH_1;
        let warp1_z = self.warp_noise_z.get([x * WARP_SCALE_1, z * WARP_SCALE_1]) * WARP_STRENGTH_1;

        // Octave 2: Medium-scale warping (with offset to break symmetry)
        let warp2_x = self
            .warp_noise_x
            .get([x * WARP_SCALE_2 + 1000.0, z * WARP_SCALE_2 + 500.0])
            * WARP_STRENGTH_2;
        let warp2_z = self
            .warp_noise_z
            .get([x * WARP_SCALE_2 + 500.0, z * WARP_SCALE_2 + 1000.0])
            * WARP_STRENGTH_2;

        // Octave 3: Fine-scale warping (with different offset)
        let warp3_x = self
            .warp_noise_x
            .get([x * WARP_SCALE_3 + 2500.0, z * WARP_SCALE_3 + 3500.0])
            * WARP_STRENGTH_3;
        let warp3_z = self
            .warp_noise_z
            .get([x * WARP_SCALE_3 + 3500.0, z * WARP_SCALE_3 + 2500.0])
            * WARP_STRENGTH_3;

        // Add asymmetric rotation component to further break radial patterns
        // This rotates coordinates slightly based on position
        let angle = (x * 0.0001 + z * 0.00007) * std::f64::consts::PI * 0.1;
        let cos_a = angle.cos();
        let sin_a = angle.sin();

        let total_warp_x = warp1_x + warp2_x + warp3_x;
        let total_warp_z = warp1_z + warp2_z + warp3_z;

        // Apply rotation to the warp offset
        let rotated_x = total_warp_x * cos_a - total_warp_z * sin_a;
        let rotated_z = total_warp_x * sin_a + total_warp_z * cos_a;

        (x + rotated_x, z + rotated_z)
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
        // Apply domain warping to break up radial patterns
        let (wx, wz) = self.warp_coordinates(x, z);

        // Scale for very large regions (~800-1000 blocks across)
        // Smaller scale = larger regions = fewer river boundaries
        let scale = 0.00125;

        // Only 2 regions - creates sparse river network with single boundary lines
        let num_regions = 2.0;

        // Quantize center point into a region (using warped coordinates)
        let center_val = self.region_noise.get([wx * scale, wz * scale]);
        let center_region = (center_val * num_regions).floor() as i32;

        // Check distance for boundary detection - wider = wider rivers
        let river_half_width = self.get_river_half_width(x, z, biome, true);

        // Check if any neighbor is in a different region (using warped coordinates)
        let at_boundary = self.is_at_region_boundary_warped(
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

        // Calculate dynamic water level based on biome and terrain
        // Coastal biomes (beach) use sea level so rivers flow into the ocean
        // Inland biomes track terrain height, capped at MAX_RIVER_WATER_LEVEL
        let water_level = if self.is_coastal_biome(biome) {
            SEA_LEVEL
        } else {
            (terrain_height - 1).min(MAX_RIVER_WATER_LEVEL)
        };

        // Calculate carving depth - must carve below water level
        // min_depth ensures carved terrain is at least 2 blocks below water surface
        let min_depth = (terrain_height - water_level + 2).max(3);
        let variation = self.variation_noise.get([x * 0.02, z * 0.02]) * 0.3 + 0.85;
        let depth = ((min_depth as f64) * variation)
            .round()
            .max(min_depth as f64) as i32;

        Some(RiverInfo {
            width: river_half_width * 2.0,
            depth,
            river_type: RiverType::MainRiver,
            water_level,
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

        // Apply domain warping to break up radial patterns
        let (wx, wz) = self.warp_coordinates(x, z);

        // Scale for medium regions (~400-500 blocks across)
        // Still larger than main rivers to avoid overlapping too much
        let scale = 0.002;
        let num_regions = 2.0;

        let center_val = self.tributary_noise.get([wx * scale, wz * scale]);
        let center_region = (center_val * num_regions).floor() as i32;

        let river_half_width = self.get_river_half_width(x, z, biome, false);

        let at_boundary = self.is_at_region_boundary_warped(
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

        // Calculate dynamic water level for tributaries
        // Coastal biomes use sea level, inland tracks terrain
        let water_level = if self.is_coastal_biome(biome) {
            SEA_LEVEL
        } else {
            (terrain_height - 1).min(MAX_RIVER_WATER_LEVEL)
        };

        // Calculate carving depth - must carve below water level
        // Tributaries slightly shallower (+1 instead of +2)
        let min_depth = (terrain_height - water_level + 1).max(2);
        let variation = self.variation_noise.get([x * 0.03, z * 0.03]) * 0.3 + 0.85;
        let depth = ((min_depth as f64) * variation)
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
            water_level,
        })
    }

    /// Check if position is at a region boundary by comparing with neighbors.
    /// Uses domain warping to break up radial patterns in the noise.
    #[allow(clippy::too_many_arguments)]
    fn is_at_region_boundary_warped(
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
            // Apply warping to neighbor coordinates too
            let (wnx, wnz) = self.warp_coordinates(nx, nz);
            let neighbor_val = noise.get([wnx * scale, wnz * scale]);
            let neighbor_region = (neighbor_val * num_regions).floor() as i32;

            if neighbor_region != center_region {
                return true;
            }
        }
        false
    }

    /// Check if position is at a region boundary (non-warped version for banks).
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
            // Apply warping here too for consistency
            let (wnx, wnz) = self.warp_coordinates(nx, nz);
            let neighbor_val = noise.get([wnx * scale, wnz * scale]);
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
            // Beach: rivers flow through to meet the ocean
            BiomeType::Beach => true,
            BiomeType::LushCaves | BiomeType::DripstoneCaves | BiomeType::DeepDark => false,
            _ => true,
        }
    }

    /// Check if this biome is coastal (uses sea level for water).
    fn is_coastal_biome(&self, biome: BiomeType) -> bool {
        matches!(biome, BiomeType::Beach)
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
        // Apply domain warping for consistent river bank positions
        let (wx, wz) = self.warp_coordinates(x, z);
        let scale = 0.004;
        let num_regions = 5.0;

        let center_val = self.region_noise.get([wx * scale, wz * scale]);
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

        // On high terrain, water level should be capped at MAX_RIVER_WATER_LEVEL
        for x in 0..200 {
            for z in 0..200 {
                if let Some(info) = rivers.get_river_at(x, z, 95, BiomeType::Plains) {
                    assert_eq!(
                        info.water_level, MAX_RIVER_WATER_LEVEL,
                        "High terrain rivers should have max water level"
                    );
                }
            }
        }

        // On low terrain, water level should track terrain height
        for x in 0..200 {
            for z in 0..200 {
                let terrain_height = 80; // Below MAX_RIVER_WATER_LEVEL (83)
                if let Some(info) = rivers.get_river_at(x, z, terrain_height, BiomeType::Plains) {
                    assert_eq!(
                        info.water_level,
                        terrain_height - 1,
                        "Low terrain rivers should track terrain height"
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

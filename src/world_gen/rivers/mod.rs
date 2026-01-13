//! River generation system.
//!
//! Uses elevation-based valley detection. Rivers form in natural valleys
//! where terrain height is locally lower than surrounding areas, and flow
//! toward lower elevations (sea level).
//!
//! ## Algorithm
//! 1. Sample terrain height in multiple directions around each point
//! 2. Rivers form where local height is lower than neighbors (valley)
//! 3. Valley depth determines river width
//! 4. Rivers only form above sea level and flow downhill
//!
//! ## Key Design
//! - Rivers follow natural terrain contours
//! - No artificial convergence points from noise artifacts
//! - Valley detection creates realistic drainage patterns

use crate::terrain_gen::BiomeType;
use crate::world_gen::SEA_LEVEL;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

/// Maximum water level for rivers (caps how high river water can be).
pub const MAX_RIVER_WATER_LEVEL: i32 = SEA_LEVEL + 8; // 83

/// Minimum terrain height for rivers to form.
const MIN_RIVER_TERRAIN: i32 = SEA_LEVEL + 1; // 76

/// River generator using elevation-based valley detection.
#[derive(Clone)]
pub struct RiverGenerator {
    /// Height noise - matches terrain generator for valley detection
    height_noise: Fbm<Perlin>,
    /// Detail noise for width/depth variation
    detail_noise: Perlin,
    /// Meander noise - adds curves to rivers
    meander_noise: Perlin,
    /// River density noise - controls where rivers can form
    density_noise: Perlin,
}

impl RiverGenerator {
    /// Creates a new river generator with the given seed.
    pub fn new(seed: u32) -> Self {
        // Match terrain generator's height noise exactly
        let height_noise = Fbm::<Perlin>::new(seed)
            .set_octaves(4)
            .set_frequency(0.003)
            .set_lacunarity(2.0)
            .set_persistence(0.5);

        let detail_noise = Perlin::new(seed + 502);
        let meander_noise = Perlin::new(seed + 600);
        let density_noise = Perlin::new(seed + 601);

        Self {
            height_noise,
            detail_noise,
            meander_noise,
            density_noise,
        }
    }

    /// Get raw terrain height at a position (simplified, no biome blending).
    fn get_terrain_height(&self, x: f64, z: f64) -> f64 {
        // Base continental height from noise
        let base = self.height_noise.get([x, z]);

        // Simple height calculation - we just need relative heights for valley detection
        // Base height around 90 with ±20 variation
        90.0 + base * 25.0
    }

    /// Check if a position is within a river channel.
    ///
    /// Rivers form in valleys - locations where terrain is locally lower
    /// than surrounding areas.
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

        // Check river density - not everywhere should have rivers
        let density = self.density_noise.get([x * 0.0003, z * 0.0003]);
        if density < -0.2 {
            return None; // No rivers in this region
        }

        // Check for valley (main river or tributary)
        if let Some(info) = self.check_valley_river(x, z, terrain_height, biome, true) {
            return Some(info);
        }

        // Check for smaller tributary
        if self.has_tributaries(biome) {
            if let Some(info) = self.check_valley_river(x, z, terrain_height, biome, false) {
                return Some(info);
            }
        }

        None
    }

    /// Check if position is in a valley suitable for a river.
    fn check_valley_river(
        &self,
        x: f64,
        z: f64,
        terrain_height: i32,
        biome: BiomeType,
        is_main: bool,
    ) -> Option<RiverInfo> {
        // Sample distances for valley detection
        let sample_dist = if is_main { 12.0 } else { 6.0 };

        // Get terrain heights at sample points
        let center_h = self.get_terrain_height(x, z);

        // Add meandering offset to sample positions
        let meander_scale = if is_main { 0.002 } else { 0.004 };
        let meander_strength = if is_main { 8.0 } else { 4.0 };
        let mx = self
            .meander_noise
            .get([x * meander_scale, z * meander_scale])
            * meander_strength;
        let mz = self
            .meander_noise
            .get([x * meander_scale + 100.0, z * meander_scale])
            * meander_strength;

        // Sample in 8 directions with meander offset
        let samples = [
            self.get_terrain_height(x + sample_dist + mx, z + mz),
            self.get_terrain_height(x - sample_dist + mx, z + mz),
            self.get_terrain_height(x + mx, z + sample_dist + mz),
            self.get_terrain_height(x + mx, z - sample_dist + mz),
            self.get_terrain_height(x + sample_dist * 0.7 + mx, z + sample_dist * 0.7 + mz),
            self.get_terrain_height(x - sample_dist * 0.7 + mx, z + sample_dist * 0.7 + mz),
            self.get_terrain_height(x + sample_dist * 0.7 + mx, z - sample_dist * 0.7 + mz),
            self.get_terrain_height(x - sample_dist * 0.7 + mx, z - sample_dist * 0.7 + mz),
        ];

        // Calculate how much lower we are than neighbors
        let avg_neighbor = samples.iter().sum::<f64>() / samples.len() as f64;
        let min_neighbor = samples.iter().cloned().fold(f64::INFINITY, f64::min);

        // Valley depth = how much lower center is than average neighbor
        let valley_depth = avg_neighbor - center_h;

        // Threshold for valley detection
        let depth_threshold = if is_main { 1.5 } else { 0.8 };

        // Must be in a valley (lower than neighbors)
        if valley_depth < depth_threshold {
            return None;
        }

        // Additional check: shouldn't be lower than ALL neighbors by too much
        // (that would be a pit, not a river valley)
        let depth_from_min = min_neighbor - center_h;
        if depth_from_min > 8.0 {
            return None; // Too deep, probably a pit not a river
        }

        // Calculate river width based on valley depth
        let base_width = if is_main { 4.0 } else { 2.5 };
        let width_variation = self.detail_noise.get([x * 0.01, z * 0.01]) * 0.3 + 0.85;
        let biome_mult = self.get_biome_width_mult(biome);
        let width =
            base_width * biome_mult * width_variation * (valley_depth / depth_threshold).min(2.0);

        // Calculate water level
        let water_level = if self.is_coastal_biome(biome) {
            SEA_LEVEL
        } else {
            (terrain_height - 1).min(MAX_RIVER_WATER_LEVEL)
        };

        // Calculate carving depth
        let min_depth = (terrain_height - water_level + 2).max(3);
        let depth_variation = self.detail_noise.get([x * 0.02, z * 0.02]) * 0.3 + 0.85;
        let depth = ((min_depth as f64) * depth_variation)
            .round()
            .max(min_depth as f64) as i32;

        Some(RiverInfo {
            width,
            depth,
            river_type: if is_main {
                RiverType::MainRiver
            } else if terrain_height > 120 {
                RiverType::MountainStream
            } else {
                RiverType::Tributary
            },
            water_level,
        })
    }

    /// Get width multiplier for biome.
    fn get_biome_width_mult(&self, biome: BiomeType) -> f64 {
        match biome {
            BiomeType::Jungle => 1.3,
            BiomeType::Swamp => 1.2,
            BiomeType::Desert => 0.7,
            BiomeType::Mountains => 0.6,
            _ => 1.0,
        }
    }

    /// Check if rivers are enabled for this biome.
    fn is_river_enabled(&self, biome: BiomeType) -> bool {
        #[allow(deprecated)]
        match biome {
            BiomeType::Ocean => false,
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

        // Check if we're near a valley but not in it
        let center_h = self.get_terrain_height(x, z);

        // Sample nearby for valley detection
        let samples = [
            self.get_terrain_height(x + 6.0, z),
            self.get_terrain_height(x - 6.0, z),
            self.get_terrain_height(x, z + 6.0),
            self.get_terrain_height(x, z - 6.0),
        ];

        let avg_neighbor = samples.iter().sum::<f64>() / samples.len() as f64;
        let valley_depth = avg_neighbor - center_h;

        // Bank is at edge of valley (slight depression but not deep enough for river)
        valley_depth > 0.5 && valley_depth < 1.5
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

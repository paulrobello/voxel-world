//! River generation system.
//!
//! Uses noise-based flow lines to create natural river paths.
//! Rivers follow contour lines of a noise field, creating connected
//! meandering paths without artificial convergence points.
//!
//! ## Algorithm
//! 1. Use 2D noise to define a "flow field"
//! 2. Rivers form along specific contour values of the noise
//! 3. Contour-following naturally creates connected, meandering paths
//! 4. Multiple noise octaves prevent straight lines
//!
//! ## Key Design
//! - Rivers follow noise contours (like elevation contours on a map)
//! - No convergence artifacts since contours don't converge
//! - Density noise controls river frequency per region

use crate::terrain_gen::BiomeType;
use crate::world_gen::SEA_LEVEL;
use noise::{NoiseFn, Perlin, Simplex};

/// Maximum water level for rivers (caps how high river water can be).
pub const MAX_RIVER_WATER_LEVEL: i32 = SEA_LEVEL + 8; // 83

/// Minimum terrain height for rivers to form.
const MIN_RIVER_TERRAIN: i32 = SEA_LEVEL + 1; // 76

/// River generator using noise contour flow lines.
#[derive(Clone)]
pub struct RiverGenerator {
    /// Primary flow noise - rivers follow contours of this
    flow_noise: Simplex,
    /// Secondary flow noise for tributaries
    tributary_noise: Simplex,
    /// Meander noise - adds curves
    meander_noise: Perlin,
    /// Detail noise for width/depth variation
    detail_noise: Perlin,
    /// Density noise - controls where rivers can form
    density_noise: Perlin,
}

/// Width of river channel detection (how close to contour = river)
const MAIN_RIVER_WIDTH: f64 = 0.03;
const TRIBUTARY_WIDTH: f64 = 0.02;

/// Contour values where rivers form (multiple for river network)
const MAIN_CONTOURS: [f64; 2] = [0.0, 0.5];
const TRIBUTARY_CONTOURS: [f64; 3] = [0.25, -0.25, 0.75];

impl RiverGenerator {
    /// Creates a new river generator with the given seed.
    pub fn new(seed: u32) -> Self {
        // Use Simplex noise - smoother gradients, no grid artifacts
        let flow_noise = Simplex::new(seed + 700);
        let tributary_noise = Simplex::new(seed + 701);
        let meander_noise = Perlin::new(seed + 702);
        let detail_noise = Perlin::new(seed + 703);
        let density_noise = Perlin::new(seed + 704);

        Self {
            flow_noise,
            tributary_noise,
            meander_noise,
            detail_noise,
            density_noise,
        }
    }

    /// Check if a position is within a river channel.
    ///
    /// Rivers form along contour lines of a noise field, creating
    /// naturally connected meandering paths.
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
        let density = self.density_noise.get([x * 0.0002, z * 0.0002]);
        if density < -0.3 {
            return None; // No rivers in this region
        }

        // Check for main river (follows primary flow contours)
        if let Some(info) = self.check_contour_river(x, z, terrain_height, biome, true) {
            return Some(info);
        }

        // Check for tributary (follows secondary contours)
        if self.has_tributaries(biome)
            && let Some(info) = self.check_contour_river(x, z, terrain_height, biome, false)
        {
            return Some(info);
        }

        None
    }

    /// Check if position is on a river contour.
    fn check_contour_river(
        &self,
        x: f64,
        z: f64,
        terrain_height: i32,
        biome: BiomeType,
        is_main: bool,
    ) -> Option<RiverInfo> {
        // Scale for noise sampling - larger = wider river spacing
        let scale = if is_main { 0.0008 } else { 0.0015 };
        let contour_width = if is_main {
            MAIN_RIVER_WIDTH
        } else {
            TRIBUTARY_WIDTH
        };
        let contours = if is_main {
            &MAIN_CONTOURS[..]
        } else {
            &TRIBUTARY_CONTOURS[..]
        };

        // Add meandering offset
        let meander_scale = if is_main { 0.003 } else { 0.005 };
        let meander_strength = if is_main { 30.0 } else { 15.0 };
        let mx = self
            .meander_noise
            .get([x * meander_scale, z * meander_scale])
            * meander_strength;
        let mz = self
            .meander_noise
            .get([x * meander_scale + 500.0, z * meander_scale])
            * meander_strength;

        // Sample flow noise with meander offset
        let noise = if is_main {
            &self.flow_noise
        } else {
            &self.tributary_noise
        };
        let flow_value = noise.get([(x + mx) * scale, (z + mz) * scale]);

        // Check if we're close to any contour line
        let mut best_distance = f64::MAX;
        for &contour in contours {
            let distance = (flow_value - contour).abs();
            if distance < best_distance {
                best_distance = distance;
            }
        }

        // Not close enough to a contour = no river
        if best_distance > contour_width {
            return None;
        }

        // Calculate river width based on how close to contour center
        let width_factor = 1.0 - (best_distance / contour_width);
        let base_width = if is_main { 5.0 } else { 3.0 };
        let biome_mult = self.get_biome_width_mult(biome);
        let width_variation = self.detail_noise.get([x * 0.01, z * 0.01]) * 0.3 + 0.85;
        let width = base_width * biome_mult * width_variation * width_factor;

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

        // Check if we're near a river contour but not on it
        let scale = 0.0008;
        let flow_value = self.flow_noise.get([x * scale, z * scale]);

        // Bank is slightly wider than river
        let bank_width = MAIN_RIVER_WIDTH * 2.0;
        let river_width = MAIN_RIVER_WIDTH;

        for &contour in &MAIN_CONTOURS {
            let distance = (flow_value - contour).abs();
            // On bank but not in river
            if distance > river_width && distance < bank_width {
                return true;
            }
        }

        false
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

    #[test]
    fn test_rivers_exist() {
        let rivers = RiverGenerator::new(314159);

        // Sample a large area - should find at least some rivers
        let mut river_count = 0;
        for x in (-1000..1000).step_by(10) {
            for z in (-1000..1000).step_by(10) {
                if rivers.get_river_at(x, z, 95, BiomeType::Plains).is_some() {
                    river_count += 1;
                }
            }
        }

        assert!(
            river_count > 100,
            "Expected to find rivers in the world, found only {}",
            river_count
        );
    }
}

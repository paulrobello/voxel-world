//! River generation system.
//!
//! Uses Minecraft-style edge detection on a patchy noise map.
//! Rivers form along the BOUNDARIES between noise regions, not at zero-crossings.
//! This creates naturally connected, meandering river paths.
//!
//! ## Algorithm (based on Minecraft's River Layer)
//! 1. Generate a "region" noise map that creates distinct patches
//! 2. Calculate the gradient (rate of change) at each point
//! 3. Rivers form where the gradient is HIGH (boundaries between patches)
//! 4. This naturally creates connected linear features
//!
//! ## River Types
//! - **Main rivers**: Wide rivers at major region boundaries
//! - **Tributaries**: Smaller streams at minor boundaries

use crate::terrain_gen::BiomeType;
use noise::{NoiseFn, Perlin};

/// River generator using edge detection on patchy noise.
#[derive(Clone)]
pub struct RiverGenerator {
    /// Region noise - rivers form at edges between regions
    region_noise: Perlin,
    /// Secondary region noise for tributaries (different frequency)
    tributary_region_noise: Perlin,
    /// Width variation noise
    width_noise: Perlin,
}

impl RiverGenerator {
    /// Creates a new river generator with the given seed.
    pub fn new(seed: u32) -> Self {
        Self {
            region_noise: Perlin::new(seed + 500),
            tributary_region_noise: Perlin::new(seed + 501),
            width_noise: Perlin::new(seed + 502),
        }
    }

    /// Check if a position is within a river channel.
    ///
    /// Rivers form at edges between noise regions (high gradient areas).
    /// This creates naturally connected, meandering river paths like Minecraft.
    ///
    /// # Arguments
    /// * `world_x`, `world_z` - World coordinates
    /// * `terrain_height` - Height of terrain at this position
    /// * `biome` - Biome type (rivers are less common in deserts)
    ///
    /// # Returns
    /// `Some(river_info)` if this is a river, `None` otherwise
    pub fn get_river_at(
        &self,
        world_x: i32,
        world_z: i32,
        terrain_height: i32,
        biome: BiomeType,
    ) -> Option<RiverInfo> {
        // Get river parameters for this biome
        let (gradient_threshold, enabled) = self.get_river_params(biome);
        if !enabled {
            return None;
        }

        let x = world_x as f64;
        let z = world_z as f64;

        // Main rivers: detect edges in low-frequency region noise
        // Scale creates large patches (~200 blocks across)
        let main_scale = 0.005;
        let main_gradient = self.calculate_gradient(x, z, main_scale, &self.region_noise);

        // Rivers form where gradient is HIGH (at region boundaries)
        // gradient_threshold controls how "sharp" the boundary needs to be
        if main_gradient > gradient_threshold {
            // Stronger gradient = more centered in river
            let river_strength = ((main_gradient - gradient_threshold) / 0.02).clamp(0.0, 1.0);
            let base_width = self.calculate_river_width(world_x, world_z, terrain_height, true);
            let depth = (base_width * 0.6).clamp(3.0, 6.0) as i32;

            return Some(RiverInfo {
                width: base_width,
                depth,
                river_type: RiverType::MainRiver,
                strength: river_strength,
            });
        }

        // Tributary rivers: higher frequency noise creates smaller stream networks
        let tributary_scale = 0.012;
        let tributary_gradient =
            self.calculate_gradient(x, z, tributary_scale, &self.tributary_region_noise);
        let tributary_threshold = gradient_threshold * 1.2; // Slightly higher threshold

        if tributary_gradient > tributary_threshold {
            let river_strength =
                ((tributary_gradient - tributary_threshold) / 0.02).clamp(0.0, 1.0);
            let base_width = self.calculate_river_width(world_x, world_z, terrain_height, false);
            let depth = (base_width * 0.5).clamp(2.0, 4.0) as i32;

            return Some(RiverInfo {
                width: base_width,
                depth,
                river_type: if terrain_height > 120 {
                    RiverType::MountainStream
                } else {
                    RiverType::Tributary
                },
                strength: river_strength,
            });
        }

        None
    }

    /// Calculate the gradient magnitude at a position.
    /// High gradient = boundary between noise regions = river location.
    fn calculate_gradient(&self, x: f64, z: f64, scale: f64, noise: &Perlin) -> f64 {
        // Sample noise at 4 neighbors to compute gradient
        let sample_dist = 2.0; // Distance for gradient calculation
        let north = noise.get([x * scale, (z - sample_dist) * scale]);
        let south = noise.get([x * scale, (z + sample_dist) * scale]);
        let west = noise.get([(x - sample_dist) * scale, z * scale]);
        let east = noise.get([(x + sample_dist) * scale, z * scale]);

        // Calculate gradient components (rate of change in x and z)
        let grad_x = (east - west) / (2.0 * sample_dist);
        let grad_z = (south - north) / (2.0 * sample_dist);

        // Return gradient magnitude
        (grad_x * grad_x + grad_z * grad_z).sqrt()
    }

    /// Get river parameters for a biome.
    /// Returns (gradient_threshold, enabled).
    /// Lower threshold = more rivers (easier to trigger at boundaries).
    fn get_river_params(&self, biome: BiomeType) -> (f64, bool) {
        #[allow(deprecated)]
        match biome {
            // Desert has very rare rivers (high threshold = hard to trigger)
            BiomeType::Desert => (0.025, true),
            // Ocean doesn't have surface rivers
            BiomeType::Ocean => (0.0, false),
            // Beach has minimal rivers
            BiomeType::Beach => (0.022, true),
            // Mountains have streams (moderate threshold)
            BiomeType::Mountains => (0.015, true),
            // Swamp has rivers
            BiomeType::Swamp => (0.012, true),
            // Jungle has many rivers (low threshold = easy to trigger)
            BiomeType::Jungle => (0.008, true),
            // Snowy biomes have rivers
            BiomeType::SnowyPlains | BiomeType::SnowyTaiga | BiomeType::Snow => (0.015, true),
            // Underground biomes don't have surface rivers
            BiomeType::LushCaves | BiomeType::DripstoneCaves | BiomeType::DeepDark => (0.0, false),
            // Default: moderate rivers
            _ => (0.012, true),
        }
    }

    /// Calculate river width at a position.
    fn calculate_river_width(
        &self,
        world_x: i32,
        world_z: i32,
        terrain_height: i32,
        is_main: bool,
    ) -> f64 {
        let x = world_x as f64;
        let z = world_z as f64;

        // Base width: main rivers are wider
        let base = if is_main { 6.0 } else { 3.0 };

        // Width varies with terrain height
        // Higher terrain = narrower rivers (mountain streams)
        // Lower terrain = wider rivers (approaching ocean)
        let height_factor = 1.0 - ((terrain_height as f64 - 75.0) / 100.0).clamp(0.0, 0.5);
        let height_adjusted = base * (0.7 + height_factor * 0.6);

        // Add width variation
        let variation = self.width_noise.get([x * 0.01, z * 0.01]) * 0.3 + 0.85;

        height_adjusted * variation
    }

    /// Get the terrain height modification for a river.
    ///
    /// This returns how much to lower the terrain at this position
    /// to create the river channel.
    #[allow(dead_code)]
    pub fn get_height_modification(
        &self,
        world_x: i32,
        world_z: i32,
        river_info: &RiverInfo,
    ) -> i32 {
        let x = world_x as f64;
        let z = world_z as f64;

        // Add some variation to the carving depth using width_noise
        let variation = self.width_noise.get([x * 0.05, z * 0.05]) * 0.3 + 0.85;

        // Depth varies based on position within river channel
        let depth_factor = river_info.strength.clamp(0.3, 1.0);

        (river_info.depth as f64 * variation * depth_factor).round() as i32
    }

    /// Check if a position should have river banks.
    ///
    /// Returns true if near a river but not in the river itself.
    /// Used for placing sand/gravel banks along river edges.
    pub fn is_river_bank(&self, world_x: i32, world_z: i32, terrain_height: i32) -> bool {
        let x = world_x as f64;
        let z = world_z as f64;

        // Use same gradient approach as river detection
        let main_scale = 0.005;
        let gradient = self.calculate_gradient(x, z, main_scale, &self.region_noise);

        // Bank is where gradient is moderate (near river but not in it)
        // River threshold is ~0.012, bank is slightly below that
        gradient > 0.008 && gradient < 0.012 && terrain_height < 120
    }

    /// Get the water type for rivers in a biome.
    #[allow(dead_code)]
    pub fn get_water_type(&self, biome: BiomeType) -> crate::chunk::WaterType {
        biome.water_type()
    }
}

/// Information about a river at a specific location.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct RiverInfo {
    /// Width of the river in blocks
    pub width: f64,
    /// Depth of the river carving
    pub depth: i32,
    /// Type of river
    pub river_type: RiverType,
    /// Strength of river presence (0.0-1.0)
    pub strength: f64,
}

/// Types of rivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiverType {
    /// Large main river flowing to ocean
    MainRiver,
    /// Smaller stream feeding into main river
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
        let result1 = rivers.get_river_at(100, 100, 80, BiomeType::Plains);
        let result2 = rivers.get_river_at(100, 100, 80, BiomeType::Plains);

        match (result1, result2) {
            (Some(r1), Some(r2)) => {
                assert!((r1.width - r2.width).abs() < 0.001);
                assert_eq!(r1.depth, r2.depth);
            }
            (None, None) => {}
            _ => panic!("Inconsistent river generation"),
        }
    }

    #[test]
    fn test_desert_has_few_rivers() {
        let rivers = RiverGenerator::new(12345);

        // Count rivers in desert vs plains
        let mut desert_count = 0;
        let mut plains_count = 0;

        for x in 0..100 {
            for z in 0..100 {
                if rivers.get_river_at(x, z, 80, BiomeType::Desert).is_some() {
                    desert_count += 1;
                }
                if rivers.get_river_at(x, z, 80, BiomeType::Plains).is_some() {
                    plains_count += 1;
                }
            }
        }

        // Desert should have significantly fewer rivers
        assert!(
            desert_count < plains_count,
            "Desert should have fewer rivers: desert={desert_count}, plains={plains_count}"
        );
    }

    #[test]
    fn test_ocean_has_no_rivers() {
        let rivers = RiverGenerator::new(12345);

        for x in 0..50 {
            for z in 0..50 {
                assert!(
                    rivers.get_river_at(x, z, 70, BiomeType::Ocean).is_none(),
                    "Ocean should not have rivers"
                );
            }
        }
    }

    #[test]
    fn test_river_depth_reasonable() {
        let rivers = RiverGenerator::new(12345);

        for x in 0..100 {
            for z in 0..100 {
                if let Some(info) = rivers.get_river_at(x, z, 80, BiomeType::Plains) {
                    assert!(info.depth >= 2, "River depth should be at least 2");
                    assert!(info.depth <= 4, "River depth should be at most 4");
                }
            }
        }
    }
}

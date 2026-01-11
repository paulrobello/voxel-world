//! River generation system.
//!
//! Uses noise-based river detection that works with chunk-based terrain generation.
//! Rivers are procedurally determined at each point using 2D noise, allowing
//! them to be generated independently per chunk while maintaining continuity.
//!
//! ## Algorithm
//! Rivers are created by looking for where noise values cross near zero.
//! This creates LINEAR features (contour lines) rather than circular patches.
//! Domain warping adds organic curves to the rivers.
//!
//! ## River Types
//! - **Main rivers**: Wide rivers flowing from mountains to ocean
//! - **Tributaries**: Smaller streams that feed into main rivers
//! - **Mountain streams**: Narrow channels in mountainous terrain

use crate::terrain_gen::BiomeType;
use noise::{NoiseFn, Perlin};

/// River generator using noise-based detection.
#[derive(Clone)]
pub struct RiverGenerator {
    /// Primary river noise - rivers form at zero-crossings
    river_noise: Perlin,
    /// Domain warping noise for organic curves
    warp_noise_x: Perlin,
    warp_noise_z: Perlin,
    /// Secondary river layer for tributaries
    tributary_noise: Perlin,
    /// Width variation noise
    width_noise: Perlin,
}

impl RiverGenerator {
    /// Creates a new river generator with the given seed.
    pub fn new(seed: u32) -> Self {
        Self {
            river_noise: Perlin::new(seed + 500),
            warp_noise_x: Perlin::new(seed + 501),
            warp_noise_z: Perlin::new(seed + 502),
            tributary_noise: Perlin::new(seed + 503),
            width_noise: Perlin::new(seed + 504),
        }
    }

    /// Check if a position is within a river channel.
    ///
    /// Rivers form where noise crosses near zero, creating linear features.
    /// Domain warping adds organic curves to prevent straight lines.
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
        // Get river width threshold for this biome
        let (river_half_width, enabled) = self.get_river_params(biome);
        if !enabled {
            return None;
        }

        let x = world_x as f64;
        let z = world_z as f64;

        // Apply domain warping for organic river curves
        // Warp amount increases at lower frequencies for smoother bends
        let warp_scale = 0.002;
        let warp_amount = 50.0; // How much to warp coordinates
        let warp_x = self.warp_noise_x.get([x * warp_scale, z * warp_scale]) * warp_amount;
        let warp_z = self.warp_noise_z.get([x * warp_scale, z * warp_scale]) * warp_amount;

        let warped_x = x + warp_x;
        let warped_z = z + warp_z;

        // Main river: low frequency noise, rivers at zero-crossing
        let river_scale = 0.0015; // Large scale for main rivers
        let main_river_value = self
            .river_noise
            .get([warped_x * river_scale, warped_z * river_scale]);

        // Distance from zero-crossing (closer = more in river)
        let main_distance = main_river_value.abs();

        // Check if we're within the river channel
        if main_distance < river_half_width {
            let river_strength = 1.0 - (main_distance / river_half_width);
            let base_width = self.calculate_river_width(world_x, world_z, terrain_height, true);
            let depth = (base_width * 0.5).clamp(2.0, 5.0) as i32;

            return Some(RiverInfo {
                width: base_width,
                depth,
                river_type: RiverType::MainRiver,
                strength: river_strength,
            });
        }

        // Tributary rivers: higher frequency, narrower
        let tributary_scale = 0.004;
        let tributary_value = self
            .tributary_noise
            .get([warped_x * tributary_scale, warped_z * tributary_scale]);
        let tributary_distance = tributary_value.abs();
        let tributary_half_width = river_half_width * 0.5; // Narrower than main rivers

        if tributary_distance < tributary_half_width {
            let river_strength = 1.0 - (tributary_distance / tributary_half_width);
            let base_width = self.calculate_river_width(world_x, world_z, terrain_height, false);
            let depth = (base_width * 0.4).clamp(2.0, 3.0) as i32;

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

    /// Get river parameters for a biome.
    /// Returns (river_half_width, enabled).
    /// Larger half_width = wider rivers, more likely to find rivers.
    fn get_river_params(&self, biome: BiomeType) -> (f64, bool) {
        #[allow(deprecated)]
        match biome {
            // Desert has very rare thin rivers (oases)
            BiomeType::Desert => (0.02, true),
            // Ocean doesn't have surface rivers
            BiomeType::Ocean => (0.0, false),
            // Beach has minimal rivers
            BiomeType::Beach => (0.03, true),
            // Mountains have streams
            BiomeType::Mountains => (0.06, true),
            // Swamp has rivers
            BiomeType::Swamp => (0.07, true),
            // Jungle has many wide rivers
            BiomeType::Jungle => (0.10, true),
            // Snowy biomes have rivers
            BiomeType::SnowyPlains | BiomeType::SnowyTaiga | BiomeType::Snow => (0.06, true),
            // Underground biomes don't have surface rivers
            BiomeType::LushCaves | BiomeType::DripstoneCaves | BiomeType::DeepDark => (0.0, false),
            // Default: moderate rivers
            _ => (0.07, true),
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

        // Apply same domain warping as river detection
        let warp_scale = 0.002;
        let warp_amount = 50.0;
        let warp_x = self.warp_noise_x.get([x * warp_scale, z * warp_scale]) * warp_amount;
        let warp_z = self.warp_noise_z.get([x * warp_scale, z * warp_scale]) * warp_amount;

        let warped_x = x + warp_x;
        let warped_z = z + warp_z;

        let river_scale = 0.0015;
        let river_value = self
            .river_noise
            .get([warped_x * river_scale, warped_z * river_scale]);
        let distance = river_value.abs();

        // Bank is slightly outside the river channel (0.07-0.12 range)
        // but only at reasonable terrain heights
        distance > 0.07 && distance < 0.12 && terrain_height < 120
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

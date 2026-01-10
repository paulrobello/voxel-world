//! River generation system.
//!
//! Uses noise-based river detection that works with chunk-based terrain generation.
//! Rivers are procedurally determined at each point using 2D noise, allowing
//! them to be generated independently per chunk while maintaining continuity.
//!
//! ## River Types
//! - **Main rivers**: Wide rivers flowing from mountains to ocean
//! - **Tributaries**: Smaller streams that feed into main rivers
//! - **Mountain streams**: Narrow channels in mountainous terrain

use crate::terrain_gen::BiomeType;
use noise::{NoiseFn, Perlin, RidgedMulti};

/// River generator using noise-based detection.
#[derive(Clone)]
pub struct RiverGenerator {
    /// Primary river noise (ridged for river-like patterns)
    river_noise: RidgedMulti<Perlin>,
    /// Secondary noise for river variation
    variation_noise: Perlin,
    /// Width variation noise
    width_noise: Perlin,
}

impl RiverGenerator {
    /// Creates a new river generator with the given seed.
    pub fn new(seed: u32) -> Self {
        // RidgedMulti creates ridge-like structures perfect for rivers
        let mut river_noise = RidgedMulti::new(seed + 500);
        river_noise.octaves = 4;
        river_noise.frequency = 0.003; // Large-scale river patterns

        Self {
            river_noise,
            variation_noise: Perlin::new(seed + 501),
            width_noise: Perlin::new(seed + 502),
        }
    }

    /// Check if a position is within a river channel.
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
        let x = world_x as f64;
        let z = world_z as f64;

        // Get river presence from ridged noise
        let river_value = self.river_noise.get([x, z]);

        // Rivers form where ridged noise creates "valleys"
        // The ridged noise naturally creates river-like branching patterns
        let river_threshold = self.get_river_threshold(biome);

        // Check if we're in a river (very high ridge values = river channel)
        if river_value < river_threshold {
            return None;
        }

        // Calculate how "centered" we are in the river
        let river_strength = (river_value - river_threshold) / (1.0 - river_threshold);

        // Calculate river width based on terrain height and noise
        // Rivers are wider at lower elevations (near ocean)
        let base_width = self.calculate_river_width(world_x, world_z, terrain_height);

        // Calculate depth based on width
        let depth = (base_width * 0.4).clamp(2.0, 4.0) as i32;

        // Determine river type
        let river_type = self.get_river_type(river_strength, terrain_height);

        Some(RiverInfo {
            width: base_width,
            depth,
            river_type,
            strength: river_strength,
        })
    }

    /// Get the river detection threshold for a biome.
    fn get_river_threshold(&self, biome: BiomeType) -> f64 {
        #[allow(deprecated)]
        match biome {
            // Desert has very few rivers (oases only)
            BiomeType::Desert => 0.95,
            // Ocean doesn't have surface rivers
            BiomeType::Ocean => 2.0, // Never generate (ridged noise max is ~1.0)
            // Beach has minimal rivers
            BiomeType::Beach => 0.92,
            // Mountains have mountain streams
            BiomeType::Mountains => 0.80,
            // Swamp is already wet, fewer distinct rivers
            BiomeType::Swamp => 0.88,
            // Jungle has many rivers
            BiomeType::Jungle => 0.78,
            // Snowy biomes have frozen rivers
            BiomeType::SnowyPlains | BiomeType::SnowyTaiga | BiomeType::Snow => 0.85,
            // Underground biomes don't have surface rivers
            BiomeType::LushCaves | BiomeType::DripstoneCaves | BiomeType::DeepDark => 2.0,
            // Default threshold for most biomes
            _ => 0.82,
        }
    }

    /// Calculate river width at a position.
    fn calculate_river_width(&self, world_x: i32, world_z: i32, terrain_height: i32) -> f64 {
        let x = world_x as f64;
        let z = world_z as f64;

        // Base width varies with terrain height
        // Higher terrain = narrower rivers (mountain streams)
        // Lower terrain = wider rivers (approaching ocean)
        let height_factor = 1.0 - ((terrain_height as f64 - 75.0) / 80.0).clamp(0.0, 0.7);
        let base_width = 3.0 + height_factor * 5.0;

        // Add width variation
        let variation = self.width_noise.get([x * 0.02, z * 0.02]) * 0.3 + 0.85;

        base_width * variation
    }

    /// Determine river type based on characteristics.
    fn get_river_type(&self, strength: f64, terrain_height: i32) -> RiverType {
        if terrain_height > 140 {
            RiverType::MountainStream
        } else if strength > 0.7 {
            RiverType::MainRiver
        } else {
            RiverType::Tributary
        }
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

        // Add some variation to the carving depth
        let variation = self.variation_noise.get([x * 0.05, z * 0.05]) * 0.3 + 0.85;

        // Depth varies based on position within river channel
        let depth_factor = river_info.strength.clamp(0.3, 1.0);

        (river_info.depth as f64 * variation * depth_factor).round() as i32
    }

    /// Check if a position should have river banks (Beach biome).
    ///
    /// Returns true if within a few blocks of a river.
    pub fn is_river_bank(&self, world_x: i32, world_z: i32, terrain_height: i32) -> bool {
        let x = world_x as f64;
        let z = world_z as f64;

        let river_value = self.river_noise.get([x, z]);

        // Bank threshold is slightly lower than river threshold
        // This creates a band around rivers
        river_value > 0.75 && river_value < 0.82 && terrain_height < 100
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

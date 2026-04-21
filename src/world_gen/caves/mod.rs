//! Cave generation system with multiple cave types.
//!
//! Implements a multi-layered cave system similar to Minecraft 1.18+:
//! - **Cheese caves**: Large irregular caverns with natural pillars
//! - **Spaghetti caves**: Long winding tunnel networks
//! - **Noodle caves**: Fine web of narrow passages
//! - **Carved caves**: Traditional carved tunnels and ravines
//!
//! Each cave type uses different noise algorithms and thresholds to
//! create distinct underground features that combine into a rich
//! underground environment.

pub mod carved;
pub mod cheese;
pub mod noodle;
pub mod spaghetti;

use crate::chunk::WaterType;
use crate::terrain_gen::BiomeType;
use carved::CarvedCaves;
use cheese::CheeseCaves;
use noise::{NoiseFn, Perlin};
use noodle::NoodleCaves;
use spaghetti::SpaghettiCaves;

/// Unified cave generator coordinating all cave types.
#[derive(Clone)]
pub struct CaveCoordinator {
    /// Cheese caves - large caverns
    cheese: CheeseCaves,
    /// Spaghetti caves - long tunnels
    spaghetti: SpaghettiCaves,
    /// Noodle caves - fine network
    noodle: NoodleCaves,
    /// Carved caves - traditional tunnels and ravines
    carved: CarvedCaves,
    /// Decoration noise for stalactites/stalagmites
    decoration_noise: Perlin,
    /// Entrance detection noise
    entrance_noise: Perlin,
}

impl CaveCoordinator {
    /// Creates a new cave coordinator with the given seed.
    pub fn new(seed: u32) -> Self {
        Self {
            cheese: CheeseCaves::new(seed),
            spaghetti: SpaghettiCaves::new(seed),
            noodle: NoodleCaves::new(seed),
            carved: CarvedCaves::new(seed),
            decoration_noise: Perlin::new(seed + 8),
            entrance_noise: Perlin::new(seed + 5),
        }
    }

    /// Fast 2D column-based check for cave potential.
    ///
    /// This is a cheap pre-filter that returns `false` if caves are unlikely
    /// in this column, allowing expensive 3D cave checks to be skipped entirely.
    /// Returns `true` if caves are possible (3D checks still needed to confirm).
    ///
    /// # Performance
    /// This single 2D noise lookup replaces ~32 expensive 3D cave checks per column,
    /// reducing noise evaluations by 60-80% in most biomes.
    pub fn column_has_caves(&self, world_x: i32, world_z: i32, biome: BiomeType) -> bool {
        // Get biome density - low density biomes have fewer caves
        let density = self.get_biome_density(biome);

        // Very low density biomes (ocean, beach, desert) rarely have caves
        if density < 0.3 {
            // Use sparse regional check - only 10% of these columns can have caves
            let hash =
                ((world_x.wrapping_mul(374761393)) ^ (world_z.wrapping_mul(668265263))) as u32;
            if !hash.is_multiple_of(10) {
                return false;
            }
        }

        // Use spaghetti cave density noise as regional cave probability
        // This is the same noise used by spaghetti caves at scale 0.01
        let x = world_x as f64;
        let z = world_z as f64;
        let regional = self.spaghetti.density_noise.get([x * 0.01, z * 0.01]);

        // Threshold varies by biome density:
        // - High density (underground biomes): almost always have caves
        // - Normal density: moderate chance
        // - Low density: rare caves
        let threshold = match density {
            d if d >= 2.0 => -0.9, // Underground biomes - almost always
            d if d >= 1.5 => -0.7, // Mountains - very common
            d if d >= 1.0 => -0.5, // Normal biomes
            d if d >= 0.5 => -0.3, // Low density
            _ => 0.0,              // Very low - need high regional value
        };

        regional > threshold
    }

    /// Check if this is a cave entrance location.
    pub fn is_entrance(&self, world_x: i32, world_z: i32) -> bool {
        let x = world_x as f64;
        let z = world_z as f64;
        let entrance_value = self.entrance_noise.get([x * 0.02, z * 0.02]);
        entrance_value > 0.45
    }

    /// Check if a position should be carved as a cave.
    ///
    /// Combines all four cave types (cheese, spaghetti, noodle, carved)
    /// with biome-specific density adjustments.
    ///
    /// # Arguments
    /// * `world_x`, `world_y`, `world_z` - World coordinates
    /// * `surface_height` - Terrain height at this XZ position
    /// * `biome` - Biome type for density adjustments
    ///
    /// # Returns
    /// `true` if this block should be carved as cave space
    pub fn is_cave(
        &self,
        world_x: i32,
        world_y: i32,
        world_z: i32,
        surface_height: i32,
        biome: BiomeType,
    ) -> bool {
        // Get biome density multiplier
        let density = self.get_biome_density(biome);

        // Apply entrance logic for spaghetti caves
        let is_entrance = self.is_entrance(world_x, world_z);
        let surface_buffer = if is_entrance { 0 } else { 12 };

        // Don't carve near surface unless at entrance
        if world_y > surface_height - surface_buffer {
            // Check for ravines which can get closer to surface
            if !self
                .carved
                .is_cave(world_x, world_y, world_z, surface_height)
            {
                return false;
            }
        }

        // Don't carve at bedrock level
        if world_y <= 1 {
            return false;
        }

        // Low density biomes skip some cave types
        if density < 0.5 {
            // Only check spaghetti caves in low density biomes
            return self
                .spaghetti
                .is_cave(world_x, world_y, world_z, surface_height);
        }

        // Check all cave types (any match = cave)
        // Ordered by computational cost (cheapest first for early exit)

        // 1. Spaghetti caves - most common tunnels
        if self
            .spaghetti
            .is_cave_with_entrance(world_x, world_y, world_z, surface_height)
        {
            return true;
        }

        // 2. Cheese caves - large caverns (only in medium-high density)
        if density >= 0.8
            && self
                .cheese
                .is_cave(world_x, world_y, world_z, surface_height)
        {
            return true;
        }

        // 3. Noodle caves - fine network (only in high density)
        if density >= 1.0
            && self
                .noodle
                .is_cave(world_x, world_y, world_z, surface_height)
        {
            return true;
        }

        // 4. Carved caves - ravines and tunnels
        if self
            .carved
            .is_cave(world_x, world_y, world_z, surface_height)
        {
            return true;
        }

        false
    }

    /// Get biome-specific cave density multiplier.
    fn get_biome_density(&self, biome: BiomeType) -> f64 {
        #[allow(deprecated)]
        match biome {
            // Underground biomes have many more caves
            BiomeType::LushCaves | BiomeType::DripstoneCaves => 2.0,
            BiomeType::DeepDark => 2.5,
            // Mountains have extensive cave systems
            BiomeType::Mountains => 1.8,
            // Desert has fewer caves (dry, less erosion)
            BiomeType::Desert => 0.6,
            // Swamp has slightly fewer caves
            BiomeType::Swamp => 0.8,
            // Snowy biomes have ice caves
            BiomeType::SnowyPlains | BiomeType::SnowyTaiga | BiomeType::Snow => 0.9,
            // Taiga has slightly more caves
            BiomeType::Taiga => 1.2,
            // Jungle has wet caves
            BiomeType::Jungle => 1.1,
            // Ocean/Beach - fewer caves
            BiomeType::Ocean | BiomeType::Beach => 0.5,
            // Forest biomes - normal density
            BiomeType::Forest | BiomeType::DarkForest | BiomeType::BirchForest => 1.0,
            // Default
            BiomeType::Plains | BiomeType::Grassland | BiomeType::Meadow | BiomeType::Savanna => {
                1.0
            }
        }
    }

    /// Determine what should fill a cave block based on biome and depth.
    pub fn get_cave_fill(&self, biome: BiomeType, world_y: i32, sea_level: i32) -> CaveFillType {
        // All biomes: lava lakes at Y: 2-10
        if (2..=10).contains(&world_y) {
            let depth_factor = (10 - world_y) as f64 / 8.0;
            let lava_threshold = 0.7 - (depth_factor * 0.4);

            let noise_value = self.decoration_noise.get([
                (world_y as f64) * 0.3,
                (world_y as f64) * 0.2,
                (world_y as f64) * 0.25,
            ]);

            if noise_value.abs() > lava_threshold {
                return CaveFillType::Lava;
            }
        }

        #[allow(deprecated)]
        match biome {
            // Desert caves are always dry
            BiomeType::Desert | BiomeType::Savanna => CaveFillType::Air,

            // Swamp caves are heavily flooded
            BiomeType::Swamp => {
                if world_y <= sea_level + 5 {
                    CaveFillType::Water(biome.water_type())
                } else {
                    CaveFillType::Air
                }
            }

            // Ice caves are dry
            BiomeType::SnowyPlains | BiomeType::SnowyTaiga | BiomeType::Snow => CaveFillType::Air,

            // Mountain caves have lava lakes to sea level
            BiomeType::Mountains => {
                if world_y <= sea_level {
                    let depth_factor = (sea_level - world_y) as f64 / sea_level as f64;
                    let lava_threshold = 0.7 - (depth_factor * 0.4);

                    let noise_value = self.decoration_noise.get([
                        (world_y as f64) * 0.05,
                        (world_y as f64) * 0.05,
                        (world_y as f64) * 0.05,
                    ]);

                    if noise_value.abs() > lava_threshold {
                        CaveFillType::Lava
                    } else {
                        CaveFillType::Water(biome.water_type())
                    }
                } else {
                    CaveFillType::Air
                }
            }

            // Jungle caves partially flooded
            BiomeType::Jungle => {
                if world_y <= sea_level - 10 {
                    CaveFillType::Water(biome.water_type())
                } else {
                    CaveFillType::Air
                }
            }

            // Underground biomes
            BiomeType::LushCaves => {
                if world_y <= sea_level - 20 {
                    CaveFillType::Water(biome.water_type())
                } else {
                    CaveFillType::Air
                }
            }
            BiomeType::DripstoneCaves | BiomeType::DeepDark => CaveFillType::Air,

            // Ocean/Beach caves flood to sea level
            BiomeType::Ocean | BiomeType::Beach => {
                if world_y <= sea_level {
                    CaveFillType::Water(biome.water_type())
                } else {
                    CaveFillType::Air
                }
            }

            // All other biomes - dry caves
            _ => CaveFillType::Air,
        }
    }

    /// Get stalactite model ID based on biome.
    pub fn get_stalactite_model(&self, biome: BiomeType) -> u8 {
        #[allow(deprecated)]
        match biome {
            BiomeType::SnowyPlains | BiomeType::SnowyTaiga | BiomeType::Snow => 108, // Ice
            _ => 106,                                                                // Stone
        }
    }

    /// Get stalagmite model ID based on biome.
    pub fn get_stalagmite_model(&self, biome: BiomeType) -> u8 {
        #[allow(deprecated)]
        match biome {
            BiomeType::SnowyPlains | BiomeType::SnowyTaiga | BiomeType::Snow => 109, // Ice
            _ => 107,                                                                // Stone
        }
    }

    /// Check if a stalactite should be placed at this ceiling position.
    pub fn should_place_stalactite(
        &self,
        world_x: i32,
        world_y: i32,
        world_z: i32,
        biome: BiomeType,
    ) -> Option<u8> {
        let x = world_x as f64;
        let y = world_y as f64;
        let z = world_z as f64;

        let noise_value = self.decoration_noise.get([x * 0.1, y * 0.1, z * 0.1]);

        // ~15% spawn rate in normal caves
        // Higher in dripstone caves
        #[allow(deprecated)]
        let threshold = match biome {
            BiomeType::DripstoneCaves => 0.5,
            _ => 0.7,
        };

        if noise_value > threshold {
            Some(self.get_stalactite_model(biome))
        } else {
            None
        }
    }

    /// Check if a stalagmite should be placed at this floor position.
    pub fn should_place_stalagmite(
        &self,
        world_x: i32,
        world_y: i32,
        world_z: i32,
        biome: BiomeType,
    ) -> Option<u8> {
        let x = world_x as f64;
        let y = world_y as f64;
        let z = world_z as f64;

        let noise_value = self
            .decoration_noise
            .get([x * 0.1 + 100.0, y * 0.1, z * 0.1 + 100.0]);

        // ~15% spawn rate in normal caves
        // Higher in dripstone caves
        #[allow(deprecated)]
        let threshold = match biome {
            BiomeType::DripstoneCaves => 0.5,
            _ => 0.7,
        };

        if noise_value > threshold {
            Some(self.get_stalagmite_model(biome))
        } else {
            None
        }
    }
}

/// Type of content to fill a cave with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaveFillType {
    /// Empty cave space (air)
    Air,
    /// Filled with water of a specific type
    Water(WaterType),
    /// Filled with lava
    Lava,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cave_coordinator_basic() {
        let caves = CaveCoordinator::new(12345);

        // Should not carve at bedrock
        assert!(!caves.is_cave(0, 0, 0, 75, BiomeType::Plains));
        assert!(!caves.is_cave(0, 1, 0, 75, BiomeType::Plains));
    }

    #[test]
    fn test_cave_fill_types() {
        let caves = CaveCoordinator::new(12345);

        // Desert should be air
        assert_eq!(
            caves.get_cave_fill(BiomeType::Desert, 50, 75),
            CaveFillType::Air
        );

        // Ocean at sea level should be water
        assert!(matches!(
            caves.get_cave_fill(BiomeType::Ocean, 70, 75),
            CaveFillType::Water(_)
        ));
    }

    #[test]
    fn test_biome_density() {
        let caves = CaveCoordinator::new(12345);

        // Underground biomes should have high density
        assert!(caves.get_biome_density(BiomeType::DeepDark) > 2.0);
        assert!(caves.get_biome_density(BiomeType::LushCaves) > 1.5);

        // Desert should have low density
        assert!(caves.get_biome_density(BiomeType::Desert) < 1.0);
    }
}

//! Cheese caves - large caverns with Swiss-cheese-like structure.
//!
//! Uses 3D Perlin noise with low frequency and high threshold to create
//! large irregular caverns with natural pillar formations preserved.

use noise::{NoiseFn, Perlin};

/// Cheese cave generator for large caverns.
#[derive(Clone)]
pub struct CheeseCaves {
    /// Primary noise for cavern shape
    primary_noise: Perlin,
    /// Secondary noise for pillar preservation
    pillar_noise: Perlin,
}

impl CheeseCaves {
    /// Creates a new cheese cave generator with the given seed.
    pub fn new(seed: u32) -> Self {
        Self {
            primary_noise: Perlin::new(seed + 100),
            pillar_noise: Perlin::new(seed + 101),
        }
    }

    /// Check if a position is within a cheese cave.
    ///
    /// Cheese caves are large caverns created by carving where
    /// noise values exceed a threshold. The result is Swiss-cheese-like
    /// irregular cavern shapes.
    ///
    /// # Arguments
    /// * `world_x`, `world_y`, `world_z` - World coordinates
    /// * `surface_height` - Height of terrain at this XZ position
    ///
    /// # Returns
    /// `true` if this position should be carved as a cheese cave
    pub fn is_cave(&self, world_x: i32, world_y: i32, world_z: i32, surface_height: i32) -> bool {
        // Don't carve near surface (minimum 16 blocks below)
        if world_y > surface_height - 16 {
            return false;
        }

        // Don't carve near bedrock
        if world_y <= 2 {
            return false;
        }

        let x = world_x as f64;
        let y = world_y as f64;
        let z = world_z as f64;

        // Low frequency for large features
        // Stretched vertically for taller caverns
        let noise_value = self.primary_noise.get([x * 0.02, y * 0.015, z * 0.02]);

        // High threshold means only strong peaks become caves
        // This creates isolated large caverns
        if noise_value > 0.62 {
            // Check pillar preservation - prevent carving in pillar regions
            // This creates natural stone pillars inside large caverns
            let pillar_value = self.pillar_noise.get([x * 0.08, y * 0.04, z * 0.08]);

            // If pillar noise is high, preserve as a pillar (don't carve)
            if pillar_value > 0.7 {
                return false;
            }

            return true;
        }

        false
    }

    /// Get the density multiplier for cheese caves in this biome.
    ///
    /// Some biomes have larger or smaller cheese caves.
    #[allow(dead_code)]
    pub fn get_biome_multiplier(&self, biome: crate::terrain_gen::BiomeType) -> f64 {
        #[allow(deprecated)]
        match biome {
            // Underground biomes have more caverns
            crate::terrain_gen::BiomeType::LushCaves => 1.5,
            crate::terrain_gen::BiomeType::DripstoneCaves => 1.3,
            crate::terrain_gen::BiomeType::DeepDark => 2.0,
            // Mountains have large internal caverns
            crate::terrain_gen::BiomeType::Mountains => 1.4,
            // Desert has fewer caverns (less erosion)
            crate::terrain_gen::BiomeType::Desert => 0.6,
            // Ocean/Beach rarely have cheese caves
            crate::terrain_gen::BiomeType::Ocean | crate::terrain_gen::BiomeType::Beach => 0.3,
            // Default for other biomes
            _ => 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cheese_caves_surface_protection() {
        let caves = CheeseCaves::new(12345);

        // Should not carve near surface
        assert!(!caves.is_cave(0, 70, 0, 75));
        assert!(!caves.is_cave(0, 65, 0, 75));

        // Should not carve near bedrock
        assert!(!caves.is_cave(0, 1, 0, 75));
        assert!(!caves.is_cave(0, 2, 0, 75));
    }

    #[test]
    fn test_cheese_caves_consistency() {
        let caves = CheeseCaves::new(12345);

        // Same coordinates should give same result
        let result1 = caves.is_cave(100, 30, 100, 75);
        let result2 = caves.is_cave(100, 30, 100, 75);
        assert_eq!(result1, result2);
    }
}

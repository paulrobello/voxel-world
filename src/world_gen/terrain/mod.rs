//! Core terrain generation.
//!
//! Contains the TerrainGenerator struct and height calculation methods.

use crate::cave_gen::CaveGenerator;
use crate::world_gen::biome::{BiomeInfo, BiomeType};
use crate::world_gen::climate::{ClimateGenerator, ClimatePoint};
use noise::{Fbm, MultiFractal, NoiseFn, Perlin, RidgedMulti};
use std::collections::HashMap;

/// Terrain generator using multiple noise layers for varied landscapes
#[derive(Clone)]
pub struct TerrainGenerator {
    height_noise: Fbm<Perlin>,
    detail_noise: Perlin,
    mountain_noise: RidgedMulti<Perlin>,
    climate_generator: ClimateGenerator,
    cave_generator: CaveGenerator,
}

impl TerrainGenerator {
    pub fn new(seed: u32) -> Self {
        // Base continental noise for large-scale terrain features
        let height_noise = Fbm::<Perlin>::new(seed)
            .set_octaves(4)
            .set_frequency(0.003)
            .set_lacunarity(2.0)
            .set_persistence(0.5);

        let detail_noise = Perlin::new(seed.wrapping_add(1));

        // Mountain ridges using RidgedMulti for sharp peaks
        let mountain_noise = RidgedMulti::<Perlin>::new(seed.wrapping_add(2))
            .set_octaves(5)
            .set_frequency(0.008)
            .set_lacunarity(2.2)
            .set_persistence(0.5);

        let climate_generator = ClimateGenerator::new(seed);
        let cave_generator = CaveGenerator::new(seed);

        Self {
            height_noise,
            detail_noise,
            mountain_noise,
            climate_generator,
            cave_generator,
        }
    }

    /// Get biome info (elevation, temp, rain) at world coordinates
    pub fn get_biome_info(&self, world_x: i32, world_z: i32) -> BiomeInfo {
        let x = world_x as f64;
        let z = world_z as f64;

        // Get full climate point from multinoise system
        let climate = self.climate_generator.get_climate_2d(world_x, world_z);

        // Normalize temperature and humidity to 0.0-1.0 for backward compatibility
        let temp = climate.temperature * 0.5 + 0.5;
        let humidity = climate.humidity * 0.5 + 0.5;

        // Get base height for elevation-based adjustments
        let base_height = self.height_noise.get([x, z]);

        // Adjust temperature by elevation (higher = colder)
        let elevation_cooling = base_height.max(0.0) * 0.4;
        let adjusted_temp = (temp - elevation_cooling).clamp(0.0, 1.0);

        // Use climate parameters to select biome
        // Continentalness: -1.0 = ocean, 1.0 = inland
        // Erosion: -1.0 = peaks, 1.0 = flat
        let biome = self.select_biome_from_climate(&climate, adjusted_temp, base_height);

        BiomeInfo {
            elevation: base_height,
            temperature: adjusted_temp,
            rainfall: humidity,
            biome,
            climate,
        }
    }

    /// Select biome based on climate parameters.
    /// Uses multinoise system for more varied biome distribution.
    fn select_biome_from_climate(
        &self,
        climate: &ClimatePoint,
        adjusted_temp: f64,
        base_height: f64,
    ) -> BiomeType {
        // Temperature thresholds (using adjusted_temp which is 0.0-1.0)
        let is_cold = adjusted_temp < 0.3;
        let is_hot = adjusted_temp > 0.65;

        // Humidity thresholds (climate.humidity is -1.0 to 1.0)
        let is_dry = climate.humidity < -0.3;
        let is_wet = climate.humidity > 0.3;

        // Erosion thresholds (-1.0 = peaks, 1.0 = flat)
        let is_mountainous = climate.erosion < -0.2;

        // Cold biomes
        if is_cold {
            return BiomeType::Snow;
        }

        // Hot dry = Desert
        if is_hot && is_dry {
            return BiomeType::Desert;
        }

        // Warm wet = Swamp
        if adjusted_temp > 0.55 && is_wet {
            return BiomeType::Swamp;
        }

        // Mountainous terrain
        if is_mountainous && base_height > 0.2 {
            return BiomeType::Mountains;
        }

        // Default to Grassland
        BiomeType::Grassland
    }

    /// Get climate point at 2D coordinates.
    #[allow(dead_code)]
    pub fn get_climate(&self, world_x: i32, world_z: i32) -> ClimatePoint {
        self.climate_generator.get_climate_2d(world_x, world_z)
    }

    /// Get climate point at 3D coordinates (for underground biomes).
    #[allow(dead_code)]
    pub fn get_climate_3d(&self, world_x: i32, world_y: i32, world_z: i32) -> ClimatePoint {
        self.climate_generator
            .get_climate_3d(world_x, world_y, world_z)
    }

    /// Get biome type at world coordinates
    pub fn get_biome(&self, world_x: i32, world_z: i32) -> BiomeType {
        self.get_biome_info(world_x, world_z).biome
    }

    /// Get reference to the cave generator
    pub fn cave_generator(&self) -> &CaveGenerator {
        &self.cave_generator
    }

    /// Calculate height for a specific biome
    fn calculate_biome_height(&self, biome: BiomeType, base: f64, ridges: f64, detail: f64) -> f64 {
        match biome {
            BiomeType::Grassland => 128.0 + detail * 2.0 + base * 4.0,
            BiomeType::Mountains => 128.0 + base * 10.0 + ridges * 55.0,
            BiomeType::Desert => 128.0 + detail * 1.0 + base * 2.0,
            BiomeType::Swamp => 128.0 + detail * 2.0,
            BiomeType::Snow => {
                if base > 0.5 {
                    128.0 + base * 8.0 + ridges * 40.0
                } else {
                    128.0 + detail * 2.0
                }
            }
        }
    }

    /// Get terrain height at world coordinates with smooth transitions at biome boundaries
    pub fn get_height(&self, world_x: i32, world_z: i32) -> i32 {
        let x = world_x as f64;
        let z = world_z as f64;

        let base = self.height_noise.get([x, z]);
        let ridges = self.mountain_noise.get([x, z]);
        let detail = self.detail_noise.get([x * 0.02, z * 0.02]);

        let center_biome = self.get_biome(world_x, world_z);

        // Sample neighboring biomes to detect boundaries
        const SAMPLE_OFFSET: i32 = 4;
        let neighbors = [
            self.get_biome(world_x + SAMPLE_OFFSET, world_z),
            self.get_biome(world_x - SAMPLE_OFFSET, world_z),
            self.get_biome(world_x, world_z + SAMPLE_OFFSET),
            self.get_biome(world_x, world_z - SAMPLE_OFFSET),
        ];

        let at_boundary = neighbors.iter().any(|&b| b != center_biome);

        if !at_boundary {
            let height = self.calculate_biome_height(center_biome, base, ridges, detail);
            return height.round() as i32;
        }

        // At a boundary - calculate weighted blend
        const BLEND_SAMPLES: i32 = 3;
        let mut biome_heights: HashMap<BiomeType, (f64, f64)> = HashMap::new();

        for dx in -BLEND_SAMPLES..=BLEND_SAMPLES {
            for dz in -BLEND_SAMPLES..=BLEND_SAMPLES {
                let sample_biome = self.get_biome(world_x + dx, world_z + dz);
                let dist = ((dx * dx + dz * dz) as f64).sqrt();
                let weight = if dist > 0.0 { 1.0 / dist } else { 4.0 };

                let entry = biome_heights.entry(sample_biome).or_insert((0.0, 0.0));
                entry.0 += weight;
                entry.1 = self.calculate_biome_height(sample_biome, base, ridges, detail);
            }
        }

        let total_weight: f64 = biome_heights.values().map(|(w, _)| w).sum();
        let blended_height: f64 = biome_heights
            .values()
            .map(|(weight, height)| weight * height)
            .sum::<f64>()
            / total_weight;

        blended_height.round() as i32
    }

    /// Simple hash for placement randomness
    pub fn hash(&self, x: i32, z: i32) -> i32 {
        let mut h = (x.wrapping_mul(374761393)) ^ (z.wrapping_mul(668265263));
        h = (h ^ (h >> 13)).wrapping_mul(1274126177);
        (h ^ (h >> 16)).abs()
    }
}

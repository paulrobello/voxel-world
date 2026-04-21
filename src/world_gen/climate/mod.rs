//! Multinoise climate system for biome selection.
//!
//! Implements a 5-parameter climate system similar to Minecraft 1.18+:
//! - Temperature: Hot/Cold biome selection
//! - Humidity: Wet/Dry biome selection
//! - Continentalness: Ocean to inland, controls base terrain height
//! - Erosion: Flat to mountainous, controls height variation amplitude
//! - Weirdness: Enables rare biome variants

use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

/// Climate parameters at a specific world location.
/// All values range from -1.0 to 1.0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClimatePoint {
    /// Temperature: -1.0 (cold) to 1.0 (hot)
    pub temperature: f64,
    /// Humidity: -1.0 (dry) to 1.0 (wet)
    pub humidity: f64,
    /// Continentalness: -1.0 (ocean) to 1.0 (inland/continental)
    pub continentalness: f64,
    /// Erosion: -1.0 (peaks/mountains) to 1.0 (flat/eroded)
    pub erosion: f64,
    /// Weirdness: -1.0 to 1.0 (variant selector for rare biomes)
    pub weirdness: f64,
}

impl ClimatePoint {
    /// Create a new climate point with all parameters.
    #[allow(dead_code)]
    pub fn new(
        temperature: f64,
        humidity: f64,
        continentalness: f64,
        erosion: f64,
        weirdness: f64,
    ) -> Self {
        Self {
            temperature,
            humidity,
            continentalness,
            erosion,
            weirdness,
        }
    }

    /// Calculate climate distance between two points (for biome matching).
    /// Uses weighted Euclidean distance in climate space.
    #[allow(dead_code)]
    pub fn distance(&self, other: &ClimatePoint) -> f64 {
        // Weight factors for each parameter (can be tuned)
        const TEMP_WEIGHT: f64 = 1.0;
        const HUMIDITY_WEIGHT: f64 = 1.0;
        const CONT_WEIGHT: f64 = 0.8;
        const EROSION_WEIGHT: f64 = 0.6;
        const WEIRD_WEIGHT: f64 = 0.4;

        let dt = (self.temperature - other.temperature) * TEMP_WEIGHT;
        let dh = (self.humidity - other.humidity) * HUMIDITY_WEIGHT;
        let dc = (self.continentalness - other.continentalness) * CONT_WEIGHT;
        let de = (self.erosion - other.erosion) * EROSION_WEIGHT;
        let dw = (self.weirdness - other.weirdness) * WEIRD_WEIGHT;

        (dt * dt + dh * dh + dc * dc + de * de + dw * dw).sqrt()
    }

    /// Check if this climate point falls within given ranges.
    #[allow(dead_code)]
    pub fn in_range(
        &self,
        temp_range: (f64, f64),
        humidity_range: (f64, f64),
        cont_range: (f64, f64),
        erosion_range: (f64, f64),
    ) -> bool {
        self.temperature >= temp_range.0
            && self.temperature <= temp_range.1
            && self.humidity >= humidity_range.0
            && self.humidity <= humidity_range.1
            && self.continentalness >= cont_range.0
            && self.continentalness <= cont_range.1
            && self.erosion >= erosion_range.0
            && self.erosion <= erosion_range.1
    }
}

/// Generates climate values using multiple noise layers.
#[derive(Clone)]
pub struct ClimateGenerator {
    temperature_noise: Fbm<Perlin>,
    humidity_noise: Fbm<Perlin>,
    continentalness_noise: Fbm<Perlin>,
    erosion_noise: Fbm<Perlin>,
    weirdness_noise: Perlin,
}

impl ClimateGenerator {
    /// Create a new climate generator with the given seed.
    pub fn new(seed: u32) -> Self {
        // Temperature: Large-scale bands (like latitude)
        let temperature_noise = Fbm::<Perlin>::new(seed.wrapping_add(100))
            .set_octaves(3)
            .set_frequency(0.0008) // Very large scale
            .set_lacunarity(2.0)
            .set_persistence(0.5);

        // Humidity: Medium-scale variation
        let humidity_noise = Fbm::<Perlin>::new(seed.wrapping_add(200))
            .set_octaves(4)
            .set_frequency(0.0015)
            .set_lacunarity(2.0)
            .set_persistence(0.5);

        // Continentalness: Very large-scale (ocean vs land masses)
        let continentalness_noise = Fbm::<Perlin>::new(seed.wrapping_add(300))
            .set_octaves(4)
            .set_frequency(0.0004) // Extremely large scale
            .set_lacunarity(2.0)
            .set_persistence(0.55);

        // Erosion: Medium scale (valleys vs ridges)
        let erosion_noise = Fbm::<Perlin>::new(seed.wrapping_add(400))
            .set_octaves(4)
            .set_frequency(0.001)
            .set_lacunarity(2.2)
            .set_persistence(0.5);

        // Weirdness: Smaller scale for variant selection
        let weirdness_noise = Perlin::new(seed.wrapping_add(500));

        Self {
            temperature_noise,
            humidity_noise,
            continentalness_noise,
            erosion_noise,
            weirdness_noise,
        }
    }

    /// Get climate point at 2D world coordinates.
    /// This is the primary method for surface biome selection.
    pub fn get_climate_2d(&self, world_x: i32, world_z: i32) -> ClimatePoint {
        let x = world_x as f64;
        let z = world_z as f64;

        let temperature = self.temperature_noise.get([x, z]);
        let humidity = self.humidity_noise.get([x, z]);
        let continentalness = self.continentalness_noise.get([x, z]);
        let erosion = self.erosion_noise.get([x, z]);
        let weirdness = self.weirdness_noise.get([x * 0.003, z * 0.003]);

        ClimatePoint::new(temperature, humidity, continentalness, erosion, weirdness)
    }

    /// Get climate point at 3D world coordinates.
    /// Used for underground biome selection where humidity varies with depth.
    #[allow(dead_code)]
    pub fn get_climate_3d(&self, world_x: i32, world_y: i32, world_z: i32) -> ClimatePoint {
        let x = world_x as f64;
        let y = world_y as f64;
        let z = world_z as f64;

        // Surface values (2D)
        let temperature = self.temperature_noise.get([x, z]);
        let continentalness = self.continentalness_noise.get([x, z]);
        let erosion = self.erosion_noise.get([x, z]);
        let weirdness = self.weirdness_noise.get([x * 0.003, z * 0.003]);

        // 3D humidity varies with depth (used for lush caves, etc.)
        let humidity_2d = self.humidity_noise.get([x, z]);
        let humidity_depth = self.humidity_noise.get([x * 0.8, y * 0.5, z * 0.8]);
        let humidity = (humidity_2d + humidity_depth) * 0.5;

        ClimatePoint::new(temperature, humidity, continentalness, erosion, weirdness)
    }

    /// Get raw continentalness value for terrain height calculation.
    #[allow(dead_code)]
    pub fn get_continentalness(&self, world_x: i32, world_z: i32) -> f64 {
        self.continentalness_noise
            .get([world_x as f64, world_z as f64])
    }

    /// Get raw erosion value for terrain height calculation.
    #[allow(dead_code)]
    pub fn get_erosion(&self, world_x: i32, world_z: i32) -> f64 {
        self.erosion_noise.get([world_x as f64, world_z as f64])
    }

    /// Get raw temperature value.
    #[allow(dead_code)]
    pub fn get_temperature(&self, world_x: i32, world_z: i32) -> f64 {
        self.temperature_noise.get([world_x as f64, world_z as f64])
    }

    /// Get raw humidity value.
    #[allow(dead_code)]
    pub fn get_humidity(&self, world_x: i32, world_z: i32) -> f64 {
        self.humidity_noise.get([world_x as f64, world_z as f64])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_climate_generator_consistency() {
        let climate_gen = ClimateGenerator::new(12345);

        // Same coordinates should give same results
        let c1 = climate_gen.get_climate_2d(100, 200);
        let c2 = climate_gen.get_climate_2d(100, 200);
        assert_eq!(c1.temperature, c2.temperature);
        assert_eq!(c1.humidity, c2.humidity);
    }

    #[test]
    fn test_climate_point_distance() {
        let p1 = ClimatePoint::new(0.0, 0.0, 0.0, 0.0, 0.0);
        let p2 = ClimatePoint::new(0.0, 0.0, 0.0, 0.0, 0.0);
        assert!((p1.distance(&p2) - 0.0).abs() < 0.001);

        let p3 = ClimatePoint::new(1.0, 0.0, 0.0, 0.0, 0.0);
        assert!(p1.distance(&p3) > 0.0);
    }

    #[test]
    fn test_climate_in_range() {
        let p = ClimatePoint::new(0.5, -0.2, 0.3, 0.0, 0.0);
        assert!(p.in_range((0.0, 1.0), (-0.5, 0.5), (0.0, 0.5), (-0.5, 0.5)));
        assert!(!p.in_range((0.6, 1.0), (-0.5, 0.5), (0.0, 0.5), (-0.5, 0.5)));
    }
}

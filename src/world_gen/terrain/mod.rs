//! Core terrain generation.
//!
//! Contains the TerrainGenerator struct and height calculation methods.

use crate::cave_gen::CaveGenerator;
use crate::world_gen::biome::{BiomeInfo, BiomeType};
use crate::world_gen::climate::{ClimateGenerator, ClimatePoint};
use crate::world_gen::rivers::{RiverGenerator, RiverInfo};
use noise::{Fbm, MultiFractal, NoiseFn, Perlin, RidgedMulti};

/// Terrain generator using multiple noise layers for varied landscapes
#[derive(Clone)]
pub struct TerrainGenerator {
    height_noise: Fbm<Perlin>,
    detail_noise: Perlin,
    mountain_noise: RidgedMulti<Perlin>,
    climate_generator: ClimateGenerator,
    cave_generator: CaveGenerator,
    river_generator: RiverGenerator,
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
        let river_generator = RiverGenerator::new(seed);

        Self {
            height_noise,
            detail_noise,
            mountain_noise,
            climate_generator,
            cave_generator,
            river_generator,
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
        // Reduced from 0.4 to 0.25 to prevent excessive snow at moderate elevations
        let elevation_cooling = base_height.max(0.0) * 0.25;
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
        // Continentalness thresholds (-1.0 = ocean, 1.0 = inland)
        let is_ocean = climate.continentalness < -0.4;
        let is_coastal = climate.continentalness >= -0.4 && climate.continentalness < -0.1;

        // Temperature thresholds (using adjusted_temp which is 0.0-1.0)
        let is_freezing = adjusted_temp < 0.2;
        let is_cold = adjusted_temp < 0.35;
        let is_hot = adjusted_temp > 0.65;
        let is_warm = adjusted_temp > 0.55;

        // Humidity thresholds (climate.humidity is -1.0 to 1.0)
        let is_very_dry = climate.humidity < -0.5;
        let is_dry = climate.humidity < -0.1;
        let is_wet = climate.humidity > 0.2;
        let is_very_wet = climate.humidity > 0.5;

        // Erosion thresholds (-1.0 = peaks, 1.0 = flat)
        let is_mountainous = climate.erosion < -0.3;
        let is_hilly = climate.erosion < 0.1;
        let is_flat = climate.erosion > 0.3;

        // Weirdness for variant selection
        let is_weird = climate.weirdness.abs() > 0.5;

        // Ocean biome
        if is_ocean {
            return BiomeType::Ocean;
        }

        // Beach (coastal + flat + not too cold)
        if is_coastal && is_flat && !is_cold {
            return BiomeType::Beach;
        }

        // Mountains (low erosion = peaks)
        if is_mountainous && base_height > 0.15 {
            return BiomeType::Mountains;
        }

        // Cold biomes
        if is_freezing {
            if is_wet {
                return BiomeType::SnowyTaiga;
            }
            return BiomeType::SnowyPlains;
        }

        if is_cold {
            if is_wet {
                return BiomeType::Taiga;
            }
            // Cold but not freezing - transitional biomes, NOT snowy
            if is_weird && is_hilly {
                return BiomeType::Meadow;
            }
            // Cold dry areas become plains (not snowy) - snow requires freezing temps
            return BiomeType::Plains;
        }

        // Hot biomes
        if is_hot {
            if is_very_dry {
                return BiomeType::Desert;
            }
            if is_very_wet {
                return BiomeType::Jungle;
            }
            return BiomeType::Savanna;
        }

        // Swamp - warm wet areas with flat terrain
        // Relaxed criteria: warm + wet + flat, or warm + very wet
        if is_warm && is_wet && is_flat {
            return BiomeType::Swamp;
        }
        // Very wet areas at warm temps become swamps even if not perfectly flat
        if is_warm && is_very_wet && !is_mountainous {
            return BiomeType::Swamp;
        }

        // Temperate biomes (mid temperature)
        if is_wet {
            // Forest variants based on weirdness
            if is_very_wet {
                return BiomeType::DarkForest;
            }
            if is_weird {
                return BiomeType::BirchForest;
            }
            return BiomeType::Forest;
        }

        // Meadow variant for hilly, moderate areas
        if is_hilly && !is_dry && is_weird {
            return BiomeType::Meadow;
        }

        // Default to Plains
        BiomeType::Plains
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

    /// Get biome type at world coordinates (2D surface biome)
    pub fn get_biome(&self, world_x: i32, world_z: i32) -> BiomeType {
        self.get_biome_info(world_x, world_z).biome
    }

    /// Get biome type at 3D coordinates (including underground biomes).
    ///
    /// Underground biomes are selected based on depth and 3D noise:
    /// - Near surface (< 16 blocks deep): Use surface biome
    /// - Deep underground (Y < 32): DeepDark regions
    /// - High 3D humidity: LushCaves
    /// - Medium depths with specific conditions: DripstoneCaves
    #[allow(dead_code)]
    pub fn get_biome_3d(&self, world_x: i32, world_y: i32, world_z: i32) -> BiomeType {
        let surface_height = self.get_height(world_x, world_z);
        let depth_below_surface = surface_height - world_y;

        // Near surface: use surface biome
        if depth_below_surface < 16 {
            return self.get_biome(world_x, world_z);
        }

        // Get 3D climate for underground biome selection
        let climate_3d = self
            .climate_generator
            .get_climate_3d(world_x, world_y, world_z);

        // Deep Dark: Y < 32 and in deep dark regions
        if world_y < 32 {
            // Use weirdness noise to create patches of deep dark
            if climate_3d.weirdness < -0.3 && climate_3d.humidity < 0.0 {
                return BiomeType::DeepDark;
            }
        }

        // Lush Caves: High 3D humidity and moderate depth
        if climate_3d.humidity > 0.4 && world_y > 20 && world_y < surface_height - 30 {
            // Additional check: lush caves are more common in wet surface biomes
            let surface_biome = self.get_biome(world_x, world_z);
            if matches!(
                surface_biome,
                BiomeType::Forest | BiomeType::DarkForest | BiomeType::Jungle | BiomeType::Swamp
            ) || climate_3d.humidity > 0.6
            {
                return BiomeType::LushCaves;
            }
        }

        // Dripstone Caves: Medium depth with moderate humidity
        if world_y > 10
            && world_y < surface_height - 20
            && climate_3d.humidity > -0.2
            && climate_3d.humidity < 0.3
            && climate_3d.weirdness.abs() > 0.3
        {
            return BiomeType::DripstoneCaves;
        }

        // Default: inherit surface biome characteristics
        self.get_biome(world_x, world_z)
    }

    /// Check if a position is in an underground biome.
    #[allow(dead_code)]
    pub fn is_underground_biome(&self, world_x: i32, world_y: i32, world_z: i32) -> bool {
        let biome = self.get_biome_3d(world_x, world_y, world_z);
        biome.is_underground()
    }

    /// Get reference to the cave generator
    pub fn cave_generator(&self) -> &CaveGenerator {
        &self.cave_generator
    }

    /// Get reference to the river generator
    #[allow(dead_code)]
    pub fn river_generator(&self) -> &RiverGenerator {
        &self.river_generator
    }

    /// Get river information at a position if a river exists there.
    ///
    /// This must be called with the *unmodified* terrain height (before river carving)
    /// to correctly determine if a river should exist at this position.
    #[allow(dead_code)]
    pub fn get_river_at(&self, world_x: i32, world_z: i32) -> Option<RiverInfo> {
        // Get base terrain height (without river modification)
        let base_height = self.get_base_height(world_x, world_z);
        let biome = self.get_biome(world_x, world_z);

        self.river_generator
            .get_river_at(world_x, world_z, base_height, biome)
    }

    /// Check if a position is along a river bank.
    #[allow(dead_code)]
    pub fn is_river_bank(&self, world_x: i32, world_z: i32) -> bool {
        let base_height = self.get_base_height(world_x, world_z);
        self.river_generator
            .is_river_bank(world_x, world_z, base_height)
    }

    /// Get base terrain height without river modifications.
    ///
    /// Used internally for river detection (rivers need to know base height
    /// before deciding whether to carve).
    fn get_base_height(&self, world_x: i32, world_z: i32) -> i32 {
        let x = world_x as f64;
        let z = world_z as f64;

        let base = self.height_noise.get([x, z]);
        let ridges = self.mountain_noise.get([x, z]);
        let detail = self.detail_noise.get([x * 0.02, z * 0.02]);
        let climate = self.climate_generator.get_climate_2d(world_x, world_z);
        let center_biome = self.get_biome(world_x, world_z);

        // Simplified height calculation without blending for performance
        let height = self.calculate_biome_height(center_biome, &climate, base, ridges, detail);
        height.round() as i32
    }

    /// Calculate height for a specific biome using climate parameters.
    ///
    /// Height is influenced by:
    /// - `continentalness`: Controls base height (-1.0 = ocean depth, 1.0 = inland plateau)
    /// - `erosion`: Controls height amplitude (-1.0 = peaks, 1.0 = flat eroded)
    /// - `base`: Base noise value for terrain features
    /// - `ridges`: Mountain ridge noise for dramatic peaks
    /// - `detail`: Fine detail noise for surface variation
    #[allow(deprecated)]
    fn calculate_biome_height(
        &self,
        biome: BiomeType,
        climate: &ClimatePoint,
        base: f64,
        ridges: f64,
        detail: f64,
    ) -> f64 {
        // Use climate parameters for more realistic height variation
        // Continentalness: -1.0 (deep ocean) to 1.0 (inland continental)
        // Erosion: -1.0 (peaks/mountains) to 1.0 (flat/eroded)

        // Base continental height factor (ocean to inland gradient)
        // Maps -1.0..1.0 to roughly 40..140 base height
        let continental_base = 90.0 + climate.continentalness * 40.0;

        // Erosion controls amplitude of terrain features
        // Low erosion (-1.0) = dramatic peaks, high erosion (1.0) = flat
        let erosion_amplitude = (1.0 - climate.erosion) * 0.5 + 0.5; // 0.5 to 1.0

        match biome {
            // Ocean - depth varies with continentalness
            BiomeType::Ocean => {
                let depth = (-climate.continentalness - 0.4).max(0.0) * 30.0;
                55.0 - depth + detail * 3.0 + base * 5.0
            }

            // Beach - just above sea level
            BiomeType::Beach => 73.0 + detail * 1.5 + base.abs() * 2.0,

            // Flat biomes - use continental base with low variation
            BiomeType::Plains | BiomeType::Grassland => {
                continental_base + detail * 2.0 + base * 4.0 * erosion_amplitude
            }
            BiomeType::Meadow => {
                continental_base + 5.0 + detail * 3.0 + base * 6.0 * erosion_amplitude
            }
            BiomeType::Swamp => 72.0 + detail * 2.0 + base.abs() * 3.0, // Near sea level
            BiomeType::Desert => {
                continental_base - 5.0 + detail * 1.5 + base * 3.0 * erosion_amplitude
            }
            BiomeType::Savanna => {
                continental_base - 3.0 + detail * 2.5 + base * 4.0 * erosion_amplitude
            }

            // Forested biomes - moderate terrain variation
            BiomeType::Forest | BiomeType::BirchForest => {
                continental_base + detail * 3.0 + base * 6.0 * erosion_amplitude
            }
            BiomeType::DarkForest => {
                continental_base - 3.0 + detail * 2.5 + base * 5.0 * erosion_amplitude
            }
            BiomeType::Jungle => continental_base + detail * 4.0 + base * 7.0 * erosion_amplitude,

            // Cold biomes
            BiomeType::Taiga => continental_base + detail * 3.0 + base * 6.0 * erosion_amplitude,
            BiomeType::SnowyPlains | BiomeType::Snow => {
                // Snow biomes can have glacier-like elevated areas
                if base > 0.4 && erosion_amplitude > 0.7 {
                    continental_base + base * 12.0 + ridges * 25.0 * erosion_amplitude
                } else {
                    continental_base + detail * 2.0 + base * 4.0
                }
            }
            BiomeType::SnowyTaiga => {
                continental_base
                    + 5.0
                    + detail * 3.0
                    + base * 7.0 * erosion_amplitude
                    + ridges * 12.0 * erosion_amplitude
            }

            // Mountains - dramatic elevation based on erosion
            BiomeType::Mountains => {
                // Low erosion = sharp peaks, high erosion = worn mountains
                let peak_height = ridges * 60.0 * erosion_amplitude;
                let base_elevation = base * 15.0 * erosion_amplitude;
                continental_base + 10.0 + base_elevation + peak_height
            }

            // Underground biomes inherit surface height (continental base)
            BiomeType::LushCaves | BiomeType::DripstoneCaves | BiomeType::DeepDark => {
                continental_base + detail * 2.0 + base * 4.0
            }
        }
    }

    /// Get terrain height at world coordinates with smooth transitions at biome boundaries.
    ///
    /// Uses climate parameters (continentalness, erosion) to influence height calculation.
    #[allow(clippy::let_and_return)] // River code disabled, will be re-enabled
    pub fn get_height(&self, world_x: i32, world_z: i32) -> i32 {
        let x = world_x as f64;
        let z = world_z as f64;

        let base = self.height_noise.get([x, z]);
        let ridges = self.mountain_noise.get([x, z]);
        let detail = self.detail_noise.get([x * 0.02, z * 0.02]);

        // Get climate for this position (used in height calculation)
        let climate = self.climate_generator.get_climate_2d(world_x, world_z);
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

        let base_terrain_height = if !at_boundary {
            let height = self.calculate_biome_height(center_biome, &climate, base, ridges, detail);
            height.round() as i32
        } else {
            // At a boundary - calculate weighted blend
            // Use the center climate for consistency (climate changes more gradually than biomes)
            const BLEND_SAMPLES: i32 = 3;

            // Accumulate weighted heights for smooth blending
            let mut total_weight = 0.0f64;
            let mut weighted_height_sum = 0.0f64;

            for dx in -BLEND_SAMPLES..=BLEND_SAMPLES {
                for dz in -BLEND_SAMPLES..=BLEND_SAMPLES {
                    let sample_biome = self.get_biome(world_x + dx, world_z + dz);
                    let dist = ((dx * dx + dz * dz) as f64).sqrt();
                    let weight = if dist > 0.0 { 1.0 / dist } else { 4.0 };

                    // Use the climate at the sample point for accurate blending
                    let sample_climate = self
                        .climate_generator
                        .get_climate_2d(world_x + dx, world_z + dz);
                    let height = self.calculate_biome_height(
                        sample_biome,
                        &sample_climate,
                        base,
                        ridges,
                        detail,
                    );

                    // Properly accumulate weighted heights
                    total_weight += weight;
                    weighted_height_sum += weight * height;
                }
            }

            let blended_height = weighted_height_sum / total_weight;

            blended_height.round() as i32
        };

        // DISABLED: River carving causing chunk artifacts
        // TODO: Debug why rivers cause chunk-sized stone cubes in some locations
        // Apply river carving if a river exists at this position
        // Only carve on very flat terrain to prevent rivers on slopes/cliffs
        // let slope = self.calculate_slope(world_x, world_z);
        // if slope <= 0.25 {
        //     if let Some(river_info) = self.river_generator.get_river_at(
        //         world_x,
        //         world_z,
        //         base_terrain_height,
        //         center_biome,
        //     ) {
        //         let carve_depth =
        //             self.river_generator
        //                 .get_height_modification(world_x, world_z, &river_info);
        //         return base_terrain_height - carve_depth;
        //     }
        // }

        base_terrain_height
    }

    /// Get the river water level at a position, if this position is in a river.
    ///
    /// Returns Some(water_level) if a river exists here (water fills from carved
    /// terrain up to water_level), or None if no river.
    pub fn get_river_water_level(&self, _world_x: i32, _world_z: i32) -> Option<i32> {
        // DISABLED: Rivers causing chunk artifacts and water on cliffs
        // TODO: Debug and re-enable river system
        None

        // let base_height = self.get_base_height(world_x, world_z);
        // let biome = self.get_biome(world_x, world_z);
        //
        // // Check slope - rivers shouldn't form on steep terrain
        // // Use a strict threshold to prevent water appearing on hillsides/cliffs
        // let slope = self.calculate_slope(world_x, world_z);
        // if slope > 0.25 {
        //     // Too steep for a river (more than 0.25 blocks height change per block)
        //     // With 8-block sampling, this means max 2 blocks height diff over 8 blocks
        //     return None;
        // }
        //
        // if let Some(river_info) =
        //     self.river_generator
        //         .get_river_at(world_x, world_z, base_height, biome)
        // {
        //     let carve_depth =
        //         self.river_generator
        //             .get_height_modification(world_x, world_z, &river_info);
        //
        //     // Water level is 1 block below the original surface
        //     // (river carves out the channel, water fills it but not to the very top)
        //     Some(base_height - 1.max(carve_depth.saturating_sub(1)))
        // } else {
        //     None
        // }
    }

    /// Calculate the local terrain slope at a position.
    /// Checks a wide area to detect if we're on or near a cliff/slope.
    /// Returns the maximum height difference per block found in the area.
    #[allow(dead_code)]
    fn calculate_slope(&self, world_x: i32, world_z: i32) -> f64 {
        let center_height = self.get_base_height(world_x, world_z) as f64;

        // Sample heights in a wider area (8 blocks) to catch larger terrain features
        // This prevents rivers from appearing on cliffs where local slope is deceptively low
        let mut max_slope = 0.0f64;

        // Check at multiple distances: 2, 4, and 8 blocks
        for &dist in &[2, 4, 8] {
            let heights = [
                self.get_base_height(world_x + dist, world_z) as f64,
                self.get_base_height(world_x - dist, world_z) as f64,
                self.get_base_height(world_x, world_z + dist) as f64,
                self.get_base_height(world_x, world_z - dist) as f64,
                // Also check diagonals for better cliff detection
                self.get_base_height(world_x + dist, world_z + dist) as f64,
                self.get_base_height(world_x - dist, world_z - dist) as f64,
                self.get_base_height(world_x + dist, world_z - dist) as f64,
                self.get_base_height(world_x - dist, world_z + dist) as f64,
            ];

            for h in heights {
                let slope = (h - center_height).abs() / dist as f64;
                max_slope = max_slope.max(slope);
            }
        }

        max_slope
    }

    /// Simple hash for placement randomness
    pub fn hash(&self, x: i32, z: i32) -> i32 {
        let mut h = (x.wrapping_mul(374761393)) ^ (z.wrapping_mul(668265263));
        h = (h ^ (h >> 13)).wrapping_mul(1274126177);
        (h ^ (h >> 16)).abs()
    }
}

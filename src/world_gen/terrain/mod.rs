//! Core terrain generation.
//!
//! Contains the TerrainGenerator struct and height calculation methods.

use crate::cave_gen::CaveGenerator;
use crate::world_gen::biome::{BiomeInfo, BiomeType};
use crate::world_gen::climate::{ClimateGenerator, ClimatePoint};
use crate::world_gen::rivers::{RiverGenerator, RiverInfo};
use noise::{Fbm, MultiFractal, NoiseFn, Perlin, RidgedMulti};

/// Pre-computed data for a single terrain column.
///
/// Used to avoid redundant noise evaluations when generating chunks.
/// All column-level data is computed once and cached for efficient block iteration.
#[derive(Clone, Copy, Debug)]
pub struct ColumnData {
    /// Terrain height at this column
    pub height: i32,
    /// Surface biome type
    pub biome: BiomeType,
    /// River water level if this column is in a river
    pub river_water_level: Option<i32>,
    /// Hash value for placement randomness
    pub hash: i32,
}

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

    /// Calculate terrain height using climate-driven approach (Minecraft-style).
    ///
    /// Height is PRIMARILY driven by climate parameters, not biome type.
    /// This ensures smooth transitions at biome boundaries since the same
    /// climate values produce similar heights regardless of biome.
    ///
    /// - `continentalness`: Controls base height (-1.0 = ocean, 1.0 = inland)
    /// - `erosion`: Controls height amplitude (-1.0 = peaks, 1.0 = flat)
    /// - Biome type adds only SMALL adjustments (not dramatic differences)
    #[allow(deprecated)]
    fn calculate_biome_height(
        &self,
        biome: BiomeType,
        climate: &ClimatePoint,
        base: f64,
        ridges: f64,
        detail: f64,
    ) -> f64 {
        // ========================================
        // STEP 1: Climate-driven base height
        // This is the PRIMARY height determinant
        // ========================================

        // Continental base with proper land/ocean separation
        // Ocean (continentalness < -0.4): below sea level (40-70)
        // Land (continentalness >= -0.4): above sea level (80-115)
        // This creates a "continental shelf" step at the coast
        let continental_base = if climate.continentalness < -0.4 {
            // Ocean: deeper as continentalness decreases
            // -0.4 -> 70, -1.0 -> 40
            70.0 + (climate.continentalness + 0.4) * 50.0
        } else {
            // Land: maps -0.4 (coast) to 1.0 (deep inland) -> 80 to 115
            // This ensures ALL land is above sea level (75)
            let land_factor = (climate.continentalness + 0.4) / 1.4; // 0.0 at coast, 1.0 at inland
            80.0 + land_factor * 35.0
        };

        // Erosion controls how much terrain varies from the base
        // Low erosion (-1.0) = dramatic peaks/valleys
        // High erosion (1.0) = flat, eroded terrain
        // Map -1..1 to 1.0..0.2 (inverted - low erosion = high amplitude)
        let erosion_factor = (1.0 - climate.erosion) * 0.4 + 0.2;

        // Base terrain variation (applies to ALL biomes)
        // Reduced from 15.0 to 12.0 to prevent dipping below sea level
        let terrain_variation = base * 12.0 * erosion_factor;

        // Mountain/peak contribution (driven by erosion, not biome)
        // Only significant when erosion is very low (< -0.3)
        let peak_contribution = if climate.erosion < -0.3 {
            let peak_strength = (-climate.erosion - 0.3) / 0.7; // 0..1 as erosion goes -0.3..-1.0
            ridges * 45.0 * peak_strength * erosion_factor
        } else {
            0.0
        };

        // Fine detail (small variations, same for all biomes)
        let surface_detail = detail * 3.0;

        // Climate-driven height (same formula for ALL biomes)
        let climate_height =
            continental_base + terrain_variation + peak_contribution + surface_detail;

        // ========================================
        // STEP 2: Small biome-specific adjustments
        // These are MINOR tweaks, not dramatic changes
        // ========================================

        let biome_adjustment = match biome {
            // Ocean uses the climate-driven height (already below sea level)
            BiomeType::Ocean => 0.0,

            // Beach - just above sea level with gentle slopes
            BiomeType::Beach => {
                // Beach is at continentalness -0.4 to -0.1, which gives base ~80
                // Return a fixed height near sea level for smooth sandy beaches
                return 76.0 + detail * 1.5 + base.abs() * 2.0;
            }

            // Swamp - near sea level (wetlands)
            BiomeType::Swamp => {
                return 76.0 + detail * 2.0 + base.abs() * 2.0;
            }

            // Mountains get a small boost (most height comes from erosion)
            BiomeType::Mountains => 8.0,

            // Snow biomes slightly elevated
            BiomeType::SnowyPlains | BiomeType::Snow | BiomeType::SnowyTaiga => 3.0,

            // Taiga/forest biomes neutral
            BiomeType::Taiga | BiomeType::Forest | BiomeType::BirchForest => 0.0,

            // Meadow slightly elevated
            BiomeType::Meadow => 4.0,

            // Dark forest in valleys
            BiomeType::DarkForest => -2.0,

            // Jungle neutral
            BiomeType::Jungle => 0.0,

            // Plains/grassland neutral
            BiomeType::Plains | BiomeType::Grassland => 0.0,

            // Desert slightly lower
            BiomeType::Desert => -3.0,

            // Savanna neutral
            BiomeType::Savanna => 0.0,

            // Underground biomes use surface height
            BiomeType::LushCaves | BiomeType::DripstoneCaves | BiomeType::DeepDark => 0.0,
        };

        climate_height + biome_adjustment
    }

    /// Get terrain height at world coordinates with smooth transitions at biome boundaries.
    ///
    /// Height is primarily driven by climate parameters (continentalness, erosion),
    /// ensuring smooth transitions since adjacent areas have similar climate values.
    /// Biome type only adds small adjustments that are blended at boundaries.
    pub fn get_height(&self, world_x: i32, world_z: i32) -> i32 {
        let x = world_x as f64;
        let z = world_z as f64;

        let base = self.height_noise.get([x, z]);
        let ridges = self.mountain_noise.get([x, z]);
        let detail = self.detail_noise.get([x * 0.02, z * 0.02]);

        // Get climate for this position - this drives MOST of the height calculation
        let climate = self.climate_generator.get_climate_2d(world_x, world_z);
        let center_biome = self.get_biome(world_x, world_z);

        // Sample neighboring biomes to detect boundaries
        const SAMPLE_OFFSET: i32 = 8;
        let neighbors = [
            self.get_biome(world_x + SAMPLE_OFFSET, world_z),
            self.get_biome(world_x - SAMPLE_OFFSET, world_z),
            self.get_biome(world_x, world_z + SAMPLE_OFFSET),
            self.get_biome(world_x, world_z - SAMPLE_OFFSET),
        ];

        let at_boundary = neighbors.iter().any(|&b| b != center_biome);

        let base_terrain_height = if !at_boundary {
            // Not at boundary - use center biome height directly
            let height = self.calculate_biome_height(center_biome, &climate, base, ridges, detail);
            height.round() as i32
        } else {
            // At a boundary - use distance-weighted blend of surrounding biomes
            // Since height is climate-driven, we only need to blend the small
            // biome-specific adjustments (max ±8 blocks)
            const BLEND_RADIUS: i32 = 16;

            // Collect biome heights weighted by inverse distance
            let mut total_weight = 0.0;
            let mut weighted_height = 0.0;

            // Sample in a grid pattern for efficiency
            for dx in (-BLEND_RADIUS..=BLEND_RADIUS).step_by(4) {
                for dz in (-BLEND_RADIUS..=BLEND_RADIUS).step_by(4) {
                    let dist_sq = (dx * dx + dz * dz) as f64;
                    if dist_sq > (BLEND_RADIUS * BLEND_RADIUS) as f64 {
                        continue;
                    }

                    let sample_x = world_x + dx;
                    let sample_z = world_z + dz;
                    let sample_biome = self.get_biome(sample_x, sample_z);

                    // Calculate height at sample position using LOCAL climate and noise
                    let sample_climate = self.climate_generator.get_climate_2d(sample_x, sample_z);
                    let sample_base = self.height_noise.get([sample_x as f64, sample_z as f64]);
                    let sample_ridges = self.mountain_noise.get([sample_x as f64, sample_z as f64]);
                    let sample_detail = self
                        .detail_noise
                        .get([sample_x as f64 * 0.02, sample_z as f64 * 0.02]);

                    let height = self.calculate_biome_height(
                        sample_biome,
                        &sample_climate,
                        sample_base,
                        sample_ridges,
                        sample_detail,
                    );

                    // Weight by inverse distance (closer = more influence)
                    // Add 1.0 to avoid division by zero at center
                    let weight = 1.0 / (dist_sq.sqrt() + 1.0);
                    weighted_height += height * weight;
                    total_weight += weight;
                }
            }

            (weighted_height / total_weight).round() as i32
        };

        // Apply river carving if a river exists at this position
        // Only carve on very flat terrain to prevent rivers on slopes/cliffs
        let slope = self.calculate_slope(world_x, world_z);
        if slope <= 0.25 {
            if let Some(river_info) = self.river_generator.get_river_at(
                world_x,
                world_z,
                base_terrain_height,
                center_biome,
            ) {
                let carve_depth =
                    self.river_generator
                        .get_height_modification(world_x, world_z, &river_info);
                return base_terrain_height - carve_depth;
            }
        }

        base_terrain_height
    }

    /// Get the river water level at a position, if this position is in a river.
    ///
    /// Returns Some(water_level) if a river exists here (water fills from carved
    /// terrain up to water_level), or None if no river.
    #[allow(dead_code)]
    pub fn get_river_water_level(&self, world_x: i32, world_z: i32) -> Option<i32> {
        let base_height = self.get_base_height(world_x, world_z);
        let biome = self.get_biome(world_x, world_z);

        // Check slope - rivers shouldn't form on steep terrain
        // Use a strict threshold to prevent water appearing on hillsides/cliffs
        let slope = self.calculate_slope(world_x, world_z);
        if slope > 0.25 {
            // Too steep for a river (more than 0.25 blocks height change per block)
            // With 8-block sampling, this means max 2 blocks height diff over 8 blocks
            return None;
        }

        if let Some(river_info) =
            self.river_generator
                .get_river_at(world_x, world_z, base_height, biome)
        {
            let carve_depth =
                self.river_generator
                    .get_height_modification(world_x, world_z, &river_info);

            // Water level is 1 block below the original surface
            // (river carves out the channel, water fills it but not to the very top)
            Some(base_height - 1.max(carve_depth.saturating_sub(1)))
        } else {
            None
        }
    }

    /// Calculate the local terrain slope at a position.
    /// Checks a wide area to detect if we're on or near a cliff/slope.
    /// Returns the maximum height difference per block found in the area.
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

    /// Get all pre-computed column data at once.
    ///
    /// More efficient than calling get_height(), get_biome(), etc. separately
    /// because it avoids redundant noise evaluations.
    pub fn get_column_data(&self, world_x: i32, world_z: i32) -> ColumnData {
        let x = world_x as f64;
        let z = world_z as f64;

        // Compute all noise values once
        let base = self.height_noise.get([x, z]);
        let ridges = self.mountain_noise.get([x, z]);
        let detail = self.detail_noise.get([x * 0.02, z * 0.02]);

        // Get climate once (this is the expensive part)
        let climate = self.climate_generator.get_climate_2d(world_x, world_z);

        // Compute biome from climate (cheap)
        let adjusted_temp = {
            let temp = climate.temperature * 0.5 + 0.5;
            let elevation_cooling = base.max(0.0) * 0.25;
            (temp - elevation_cooling).clamp(0.0, 1.0)
        };
        let biome = self.select_biome_from_climate(&climate, adjusted_temp, base);

        // Compute hash (very cheap)
        let hash = self.hash(world_x, world_z);

        // Compute height using the already-computed values
        let height =
            self.get_height_internal(world_x, world_z, base, ridges, detail, &climate, biome);

        // Check river water level using cached values
        let river_water_level = self.get_river_water_level_fast(world_x, world_z, height, biome);

        ColumnData {
            height,
            biome,
            river_water_level,
            hash,
        }
    }

    /// Internal height calculation that reuses pre-computed noise values.
    #[allow(clippy::too_many_arguments)]
    fn get_height_internal(
        &self,
        world_x: i32,
        world_z: i32,
        base: f64,
        ridges: f64,
        detail: f64,
        climate: &ClimatePoint,
        center_biome: BiomeType,
    ) -> i32 {
        // Sample neighboring biomes to detect boundaries
        // Use smaller offset for faster boundary detection
        const SAMPLE_OFFSET: i32 = 8;
        let neighbors = [
            self.get_biome(world_x + SAMPLE_OFFSET, world_z),
            self.get_biome(world_x - SAMPLE_OFFSET, world_z),
            self.get_biome(world_x, world_z + SAMPLE_OFFSET),
            self.get_biome(world_x, world_z - SAMPLE_OFFSET),
        ];

        let at_boundary = neighbors.iter().any(|&b| b != center_biome);

        let base_terrain_height = if !at_boundary {
            // Not at boundary - use center biome height directly
            let height = self.calculate_biome_height(center_biome, climate, base, ridges, detail);
            height.round() as i32
        } else {
            // At a boundary - use reduced sampling for performance
            // Optimized: 12-block radius with step 6 = 5x5 = 25 samples (was 81)
            const BLEND_RADIUS: i32 = 12;
            const BLEND_STEP: usize = 6;

            let mut total_weight = 0.0;
            let mut weighted_height = 0.0;

            for dx in (-BLEND_RADIUS..=BLEND_RADIUS).step_by(BLEND_STEP) {
                for dz in (-BLEND_RADIUS..=BLEND_RADIUS).step_by(BLEND_STEP) {
                    let dist_sq = (dx * dx + dz * dz) as f64;
                    if dist_sq > (BLEND_RADIUS * BLEND_RADIUS) as f64 {
                        continue;
                    }

                    let sample_x = world_x + dx;
                    let sample_z = world_z + dz;
                    let sample_biome = self.get_biome(sample_x, sample_z);

                    // Calculate height at sample position
                    let sample_climate = self.climate_generator.get_climate_2d(sample_x, sample_z);
                    let sample_base = self.height_noise.get([sample_x as f64, sample_z as f64]);
                    let sample_ridges = self.mountain_noise.get([sample_x as f64, sample_z as f64]);
                    let sample_detail = self
                        .detail_noise
                        .get([sample_x as f64 * 0.02, sample_z as f64 * 0.02]);

                    let height = self.calculate_biome_height(
                        sample_biome,
                        &sample_climate,
                        sample_base,
                        sample_ridges,
                        sample_detail,
                    );

                    // Weight by inverse distance
                    let weight = 1.0 / (dist_sq.sqrt() + 1.0);
                    weighted_height += height * weight;
                    total_weight += weight;
                }
            }

            (weighted_height / total_weight).round() as i32
        };

        // Apply river carving if terrain is flat enough
        let slope = self.calculate_slope_fast(world_x, world_z, base_terrain_height);
        if slope <= 0.25 {
            if let Some(river_info) = self.river_generator.get_river_at(
                world_x,
                world_z,
                base_terrain_height,
                center_biome,
            ) {
                let carve_depth =
                    self.river_generator
                        .get_height_modification(world_x, world_z, &river_info);
                return base_terrain_height - carve_depth;
            }
        }

        base_terrain_height
    }

    /// Fast slope calculation using fewer samples.
    fn calculate_slope_fast(&self, world_x: i32, world_z: i32, center_height: i32) -> f64 {
        let center = center_height as f64;

        // Sample only at distance 4 and 8 (was 2, 4, 8 with diagonals)
        // This reduces samples from 24 to 8
        let mut max_slope = 0.0f64;

        for &dist in &[4, 8] {
            let heights = [
                self.get_base_height(world_x + dist, world_z) as f64,
                self.get_base_height(world_x - dist, world_z) as f64,
                self.get_base_height(world_x, world_z + dist) as f64,
                self.get_base_height(world_x, world_z - dist) as f64,
            ];

            for h in heights {
                let slope = (h - center).abs() / dist as f64;
                max_slope = max_slope.max(slope);
            }
        }

        max_slope
    }

    /// Fast river water level check using pre-computed values.
    fn get_river_water_level_fast(
        &self,
        world_x: i32,
        world_z: i32,
        height: i32,
        biome: BiomeType,
    ) -> Option<i32> {
        // Use a simplified slope check - just cardinal directions at distance 4
        let mut max_slope = 0.0f64;
        let center = height as f64;
        for &(dx, dz) in &[(4, 0), (-4, 0), (0, 4), (0, -4)] {
            let h = self.get_base_height(world_x + dx, world_z + dz) as f64;
            let slope = (h - center).abs() / 4.0;
            max_slope = max_slope.max(slope);
        }

        if max_slope > 0.25 {
            return None;
        }

        if let Some(river_info) = self
            .river_generator
            .get_river_at(world_x, world_z, height, biome)
        {
            let carve_depth =
                self.river_generator
                    .get_height_modification(world_x, world_z, &river_info);
            Some(height - 1.max(carve_depth.saturating_sub(1)))
        } else {
            None
        }
    }
}

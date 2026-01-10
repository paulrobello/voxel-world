//! Biome system for terrain generation.
//!
//! Handles biome type definitions, climate-based selection, and biome properties.
//!
//! The biome system uses a multinoise approach similar to Minecraft 1.18+,
//! with 5 climate parameters: temperature, humidity, continentalness, erosion, and weirdness.

use crate::chunk::WaterType;
use crate::world_gen::climate::ClimatePoint;

/// Climate and elevation data for a location
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BiomeInfo {
    pub elevation: f64,
    pub temperature: f64,
    pub rainfall: f64,
    pub biome: BiomeType,
    /// Full climate data for advanced biome selection
    pub climate: ClimatePoint,
}

/// Available biome types in the world.
///
/// Surface biomes (0-14):
/// - Temperature-based: Hot (Desert, Savanna, Jungle), Cold (Snow variants, Taiga)
/// - Humidity-based: Dry (Desert, Savanna), Wet (Swamp, Jungle, Forest)
/// - Terrain-based: Mountains, Ocean, Beach
///
/// Underground biomes (15-17):
/// - LushCaves: High humidity underground
/// - DripstoneCaves: Medium humidity underground
/// - DeepDark: Deep underground (Y < 32)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BiomeType {
    // Surface biomes
    Ocean,
    Beach,
    Plains,
    Forest,
    DarkForest,
    BirchForest,
    Taiga,
    SnowyPlains,
    SnowyTaiga,
    Desert,
    Savanna,
    Swamp,
    Mountains,
    Meadow,
    Jungle,

    // Underground biomes
    LushCaves,
    DripstoneCaves,
    DeepDark,

    // Legacy aliases (mapped to new biomes internally)
    #[deprecated(note = "Use Plains instead")]
    #[allow(dead_code)]
    Grassland,
    #[deprecated(note = "Use SnowyPlains instead")]
    #[allow(dead_code)]
    Snow,
}

impl BiomeType {
    /// Get the water type for this biome's bodies of water
    pub fn water_type(&self) -> WaterType {
        #[allow(deprecated)]
        match self {
            // Ocean and coastal
            BiomeType::Ocean => WaterType::Ocean,
            BiomeType::Beach => WaterType::Ocean,

            // Cold biomes - icy water
            BiomeType::SnowyPlains | BiomeType::SnowyTaiga | BiomeType::Taiga => WaterType::River,
            BiomeType::Snow => WaterType::River, // Legacy

            // Wet biomes
            BiomeType::Swamp => WaterType::Swamp,
            BiomeType::Jungle => WaterType::River,
            BiomeType::LushCaves => WaterType::Spring,

            // Dry biomes
            BiomeType::Desert | BiomeType::Savanna => WaterType::River, // Sparse oases

            // Mountain and underground
            BiomeType::Mountains => WaterType::Spring,
            BiomeType::DripstoneCaves | BiomeType::DeepDark => WaterType::Spring,

            // Forest and plains biomes
            BiomeType::Plains | BiomeType::Grassland => WaterType::Lake,
            BiomeType::Forest | BiomeType::DarkForest | BiomeType::BirchForest => WaterType::Lake,
            BiomeType::Meadow => WaterType::Lake,
        }
    }

    /// Check if this is a cold biome (snow/ice generation)
    #[allow(dead_code, deprecated)]
    pub fn is_cold(&self) -> bool {
        matches!(
            self,
            BiomeType::SnowyPlains
                | BiomeType::SnowyTaiga
                | BiomeType::Taiga
                | BiomeType::Snow
                | BiomeType::Mountains
        )
    }

    /// Check if this is a hot/dry biome
    #[allow(dead_code)]
    pub fn is_hot(&self) -> bool {
        matches!(
            self,
            BiomeType::Desert | BiomeType::Savanna | BiomeType::Jungle
        )
    }

    /// Check if this is a wet biome (dense vegetation)
    #[allow(dead_code)]
    pub fn is_wet(&self) -> bool {
        matches!(
            self,
            BiomeType::Swamp | BiomeType::Jungle | BiomeType::DarkForest | BiomeType::LushCaves
        )
    }

    /// Check if this is an underground biome
    #[allow(dead_code)]
    pub fn is_underground(&self) -> bool {
        matches!(
            self,
            BiomeType::LushCaves | BiomeType::DripstoneCaves | BiomeType::DeepDark
        )
    }

    /// Check if this is a forested biome
    #[allow(dead_code)]
    pub fn is_forested(&self) -> bool {
        matches!(
            self,
            BiomeType::Forest
                | BiomeType::DarkForest
                | BiomeType::BirchForest
                | BiomeType::Taiga
                | BiomeType::SnowyTaiga
                | BiomeType::Jungle
        )
    }

    /// Get tree density for this biome (0.0 = none, 1.0 = dense)
    #[allow(dead_code)]
    pub fn tree_density(&self) -> f64 {
        #[allow(deprecated)]
        match self {
            // Dense forests
            BiomeType::DarkForest | BiomeType::Jungle => 0.8,
            BiomeType::Forest | BiomeType::BirchForest => 0.5,
            BiomeType::Taiga | BiomeType::SnowyTaiga => 0.4,
            BiomeType::Swamp => 0.3,

            // Sparse trees
            BiomeType::Plains | BiomeType::Grassland => 0.05,
            BiomeType::Meadow => 0.03,
            BiomeType::Savanna => 0.08,
            BiomeType::SnowyPlains | BiomeType::Snow => 0.02,

            // No trees
            BiomeType::Desert => 0.01, // Cacti only
            BiomeType::Ocean | BiomeType::Beach | BiomeType::Mountains => 0.0,

            // Underground
            BiomeType::LushCaves | BiomeType::DripstoneCaves | BiomeType::DeepDark => 0.0,
        }
    }
}

/// Climate ranges for biome selection.
/// All ranges are in the -1.0 to 1.0 scale.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub struct BiomeClimate {
    pub temp_range: (f64, f64),
    pub humidity_range: (f64, f64),
    pub continentalness_range: (f64, f64),
    pub erosion_range: (f64, f64),
    pub weirdness_threshold: Option<f64>,
}

impl BiomeClimate {
    /// Create a new biome climate specification.
    #[allow(dead_code)]
    pub const fn new(
        temp_range: (f64, f64),
        humidity_range: (f64, f64),
        continentalness_range: (f64, f64),
        erosion_range: (f64, f64),
        weirdness_threshold: Option<f64>,
    ) -> Self {
        Self {
            temp_range,
            humidity_range,
            continentalness_range,
            erosion_range,
            weirdness_threshold,
        }
    }

    /// Check if a climate point fits this biome's criteria.
    #[allow(dead_code)]
    pub fn matches(&self, climate: &ClimatePoint) -> bool {
        climate.temperature >= self.temp_range.0
            && climate.temperature <= self.temp_range.1
            && climate.humidity >= self.humidity_range.0
            && climate.humidity <= self.humidity_range.1
            && climate.continentalness >= self.continentalness_range.0
            && climate.continentalness <= self.continentalness_range.1
            && climate.erosion >= self.erosion_range.0
            && climate.erosion <= self.erosion_range.1
    }
}

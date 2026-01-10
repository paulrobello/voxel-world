//! Biome system for terrain generation.
//!
//! Handles biome type definitions, climate-based selection, and biome properties.

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

/// Available biome types in the world
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BiomeType {
    Grassland,
    Mountains,
    Desert,
    Swamp,
    Snow,
}

impl BiomeType {
    /// Get the water type for this biome's bodies of water
    pub fn water_type(&self) -> WaterType {
        match self {
            BiomeType::Grassland => WaterType::Lake,
            BiomeType::Mountains => WaterType::Spring,
            BiomeType::Desert => WaterType::River, // Sparse rivers in desert
            BiomeType::Swamp => WaterType::Swamp,
            BiomeType::Snow => WaterType::River, // Icy rivers
        }
    }
}

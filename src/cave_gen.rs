//! Cave generation system facade.
//!
//! This module provides backward compatibility by wrapping the new modular
//! cave system in world_gen/caves/.
//!
//! The actual implementation has been split into:
//! - `world_gen/caves/cheese.rs` - Large caverns
//! - `world_gen/caves/spaghetti.rs` - Long tunnel networks
//! - `world_gen/caves/noodle.rs` - Fine passage networks
//! - `world_gen/caves/carved.rs` - Traditional carved tunnels and ravines
//! - `world_gen/caves/mod.rs` - CaveCoordinator combining all types

use crate::terrain_gen::BiomeType;
use crate::world_gen::caves::CaveCoordinator;

// Re-export CaveFillType for backward compatibility
pub use crate::world_gen::caves::CaveFillType;

/// Cave generation system with biome-specific characteristics.
///
/// This is a facade over the new modular cave system that provides
/// backward compatibility with existing code.
#[derive(Clone)]
pub struct CaveGenerator {
    /// The underlying cave coordinator
    coordinator: CaveCoordinator,
}

impl CaveGenerator {
    /// Creates a new cave generator with the given seed.
    pub fn new(seed: u32) -> Self {
        Self {
            coordinator: CaveCoordinator::new(seed),
        }
    }

    /// Check if a position is at a cave entrance (allows caves to breach surface).
    #[allow(dead_code)]
    pub fn is_entrance(&self, world_x: i32, world_z: i32) -> bool {
        self.coordinator.is_entrance(world_x, world_z)
    }

    /// Check if a position should be carved out as a cave.
    ///
    /// # Arguments
    /// * `world_x`, `world_y`, `world_z` - World coordinates of the block
    /// * `surface_height` - Terrain height at this XZ position
    /// * `biome` - Biome type for biome-specific cave characteristics
    ///
    /// # Returns
    /// `true` if this block should be carved out as cave space
    pub fn is_cave(
        &self,
        world_x: i32,
        world_y: i32,
        world_z: i32,
        surface_height: i32,
        biome: BiomeType,
    ) -> bool {
        self.coordinator
            .is_cave(world_x, world_y, world_z, surface_height, biome)
    }

    /// Determine what should fill a cave block based on biome and depth.
    ///
    /// # Returns
    /// * `CaveFillType::Air` - Empty cave space
    /// * `CaveFillType::Water(WaterType)` - Water-filled cave
    /// * `CaveFillType::Lava` - Lava-filled cave (mountain caves at low depths)
    pub fn get_cave_fill(&self, biome: BiomeType, world_y: i32, sea_level: i32) -> CaveFillType {
        self.coordinator.get_cave_fill(biome, world_y, sea_level)
    }

    /// Check if lava lakes should spawn at this cave position.
    ///
    /// All biomes have lava lakes at Y: 2-10.
    /// Mountains have additional lava lakes up to sea level (Y: 75).
    #[allow(dead_code)]
    pub fn should_spawn_lava(
        &self,
        _world_x: i32,
        biome: BiomeType,
        world_y: i32,
        _world_z: i32,
    ) -> bool {
        // The new system handles lava via get_cave_fill, so this is a convenience check
        matches!(
            self.coordinator.get_cave_fill(biome, world_y, 75),
            CaveFillType::Lava
        )
    }

    /// Get the model ID for a stalactite (hanging from ceiling) based on biome.
    ///
    /// Model IDs:
    /// - 106: Stone stalactite
    /// - 108: Ice stalactite (for snow biomes)
    #[allow(dead_code)]
    pub fn get_stalactite_model(&self, biome: BiomeType) -> u8 {
        self.coordinator.get_stalactite_model(biome)
    }

    /// Get the model ID for a stalagmite (growing from floor) based on biome.
    ///
    /// Model IDs:
    /// - 107: Stone stalagmite
    /// - 109: Ice stalagmite (for snow biomes)
    #[allow(dead_code)]
    pub fn get_stalagmite_model(&self, biome: BiomeType) -> u8 {
        self.coordinator.get_stalagmite_model(biome)
    }

    /// Check if a stalactite should be placed at this ceiling position.
    ///
    /// Returns `Some(model_id)` if a stalactite should be placed, `None` otherwise.
    ///
    /// Stalactites spawn on ~15% of cave ceiling blocks (more in dripstone caves).
    pub fn should_place_stalactite(
        &self,
        world_x: i32,
        world_y: i32,
        world_z: i32,
        biome: BiomeType,
    ) -> Option<u8> {
        self.coordinator
            .should_place_stalactite(world_x, world_y, world_z, biome)
    }

    /// Check if a stalagmite should be placed at this floor position.
    ///
    /// Returns `Some(model_id)` if a stalagmite should be placed, `None` otherwise.
    ///
    /// Stalagmites spawn on ~15% of cave floor blocks (more in dripstone caves).
    pub fn should_place_stalagmite(
        &self,
        world_x: i32,
        world_y: i32,
        world_z: i32,
        biome: BiomeType,
    ) -> Option<u8> {
        self.coordinator
            .should_place_stalagmite(world_x, world_y, world_z, biome)
    }
}

//! World generation module - handles terrain, caves, rivers, trees, and vegetation.
//!
//! This module provides a modular approach to world generation similar to Minecraft 1.18+,
//! with multinoise biome selection and multiple cave types.
//!
//! ## Submodules
//!
//! - [`biome`]: Biome types, climate ranges, and biome properties
//! - [`caves`]: Multi-type cave system (cheese, spaghetti, noodle, carved)
//! - [`climate`]: Multinoise climate generator (temp, humidity, continentalness, erosion, weirdness)
//! - [`rivers`]: Noise-based river detection and carving
//! - [`terrain`]: Height calculation and biome selection
//! - [`trees`]: Tree generation for all biome types
//! - [`vegetation`]: Ground cover and cave decorations
//! - [`utils`]: Helper types for overflow blocks and chunk results
//!
//! ## Biome System
//!
//! The biome system uses 5 climate parameters for smooth, varied biome distribution:
//!
//! | Parameter | Range | Effect |
//! |-----------|-------|--------|
//! | Temperature | -1.0 to 1.0 | Hot/cold biome selection |
//! | Humidity | -1.0 to 1.0 | Wet/dry biome selection |
//! | Continentalness | -1.0 to 1.0 | Ocean to inland, base height |
//! | Erosion | -1.0 to 1.0 | Flat to mountainous |
//! | Weirdness | -1.0 to 1.0 | Variant biome selector |
//!
//! ## Cave Types
//!
//! Four cave types combine to create varied underground:
//!
//! - **Cheese caves**: Large irregular caverns with natural pillars
//! - **Spaghetti caves**: Long winding tunnel networks
//! - **Noodle caves**: Fine web of narrow passages
//! - **Carved caves**: Traditional carved tunnels and ravines
//!
//! ## Underground Biomes
//!
//! Three underground biome types with depth-based 3D selection:
//!
//! - **Lush Caves**: High humidity underground, moss, glow berries
//! - **Dripstone Caves**: Stalactite/stalagmite formations
//! - **Deep Dark**: Y < 32, sparse glow mushrooms

pub mod biome;
pub mod caves;
pub mod climate;
pub mod rivers;
pub mod terrain;
pub mod trees;
pub mod utils;
pub mod vegetation;

// Re-export functions at module root (types are accessed directly via submodules)
pub use trees::generate_trees;
pub use vegetation::{generate_cave_decorations, generate_ground_cover};

/// Sea level for water filling (blocks below this in valleys become water)
pub const SEA_LEVEL: i32 = 75;

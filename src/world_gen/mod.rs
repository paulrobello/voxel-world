//! World generation module - handles terrain, caves, rivers, trees, and vegetation.
//!
//! This module provides a modular approach to world generation, splitting
//! functionality into focused submodules for maintainability.

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

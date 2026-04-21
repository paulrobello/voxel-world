//! Placement module for shape tools and building operations.
//!
//! This module provides shared block placement logic used by all shape tools.
//! Each tool generates positions, and this module handles the actual block placement
//! with proper metadata handling for different block types.

mod helpers;

pub use helpers::{BlockPlacementParams, place_blocks_at_positions};

// Re-export per-tool placement modules (will be added as tools are refactored)
// pub mod sphere;
// pub mod cube;
// pub mod bridge;
// etc.

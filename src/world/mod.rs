//! World management for the voxel game.
//!
//! The World struct manages a collection of chunks and provides
//! methods for accessing and modifying blocks at world coordinates.

#![allow(dead_code)]

use nalgebra::Vector3;

/// A position in chunk coordinates (each unit = one chunk).
pub type ChunkPos = Vector3<i32>;

/// A position in world/block coordinates.
pub type WorldPos = Vector3<i32>;

mod connections;
mod lighting;
mod query;
mod stair_logic;
mod storage;
mod tree_logic;
mod world_gen;

pub use storage::World;

#[cfg(test)]
mod tests;

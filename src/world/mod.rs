//! World management for the voxel game.
//!
//! The World struct manages a collection of chunks and provides
//! methods for accessing and modifying blocks at world coordinates.

use std::borrow::Borrow;
use std::ops::{Deref, DerefMut};

use nalgebra::Vector3;

/// A position in chunk-grid coordinates (each unit = one 32³ chunk).
///
/// Deliberately a distinct newtype from [`WorldPos`] so the compiler rejects
/// the classic off-by-`CHUNK_SIZE` mistake of using one where the other is
/// expected. There is intentionally **no** `From<ChunkPos> for WorldPos` (or
/// vice versa): the only way to cross the boundary is through
/// [`World::world_to_chunk`] / [`World::chunk_to_world`].
///
/// `#[repr(transparent)]` over `Vector3<i32>` keeps the layout identical to the
/// old type alias, so the ABI and zero-cost-ness of the previous alias form are
/// preserved. [`Deref`] / [`From<Vector3<i32>>`] / [`AsRef`] keep legacy
/// `Vector3<i32>` callers compiling via deref coercion and `.into()`.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkPos(pub Vector3<i32>);

/// A position in world/block coordinates (1 unit = 1 block).
///
/// Distinct newtype counterpart to [`ChunkPos`]; see its docs for the rationale.
/// No cross-`From` impl exists between the two.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorldPos(pub Vector3<i32>);

impl ChunkPos {
    /// Constructs a chunk-grid position from `(x, y, z)` chunk coordinates.
    #[allow(clippy::new_without_default)] // a "default" position would be a footgun
    #[inline]
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self(Vector3::new(x, y, z))
    }
}

impl WorldPos {
    /// Constructs a world/block position from `(x, y, z)` block coordinates.
    #[allow(clippy::new_without_default)] // a "default" position would be a footgun
    #[inline]
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self(Vector3::new(x, y, z))
    }
}

// --- Shared trait impls (identical for both newtypes) -----------------------
//
// `Deref`/`DerefMut` let ChunkPos/WorldPos flow into any `Vector3<i32>`-taking
// API by deref coercion, so existing call sites need no conversion. `From`/
// `AsRef`/`Borrow` do the same for by-value and lookup boundaries. The cross-
// type `PartialEq<Vector3<i32>>` impls exist solely so `assert_eq!(typed,
// vector![...])` keeps working in tests; they do NOT enable assignment.

impl Deref for ChunkPos {
    type Target = Vector3<i32>;
    #[inline]
    fn deref(&self) -> &Vector3<i32> {
        &self.0
    }
}
impl DerefMut for ChunkPos {
    #[inline]
    fn deref_mut(&mut self) -> &mut Vector3<i32> {
        &mut self.0
    }
}
impl From<Vector3<i32>> for ChunkPos {
    #[inline]
    fn from(v: Vector3<i32>) -> Self {
        Self(v)
    }
}
impl AsRef<Vector3<i32>> for ChunkPos {
    #[inline]
    fn as_ref(&self) -> &Vector3<i32> {
        &self.0
    }
}
impl Borrow<Vector3<i32>> for ChunkPos {
    #[inline]
    fn borrow(&self) -> &Vector3<i32> {
        &self.0
    }
}
impl PartialEq<Vector3<i32>> for ChunkPos {
    #[inline]
    fn eq(&self, other: &Vector3<i32>) -> bool {
        self.0 == *other
    }
}
impl PartialEq<ChunkPos> for Vector3<i32> {
    #[inline]
    fn eq(&self, other: &ChunkPos) -> bool {
        *self == other.0
    }
}

impl Deref for WorldPos {
    type Target = Vector3<i32>;
    #[inline]
    fn deref(&self) -> &Vector3<i32> {
        &self.0
    }
}
impl DerefMut for WorldPos {
    #[inline]
    fn deref_mut(&mut self) -> &mut Vector3<i32> {
        &mut self.0
    }
}
impl From<Vector3<i32>> for WorldPos {
    #[inline]
    fn from(v: Vector3<i32>) -> Self {
        Self(v)
    }
}
impl AsRef<Vector3<i32>> for WorldPos {
    #[inline]
    fn as_ref(&self) -> &Vector3<i32> {
        &self.0
    }
}
impl Borrow<Vector3<i32>> for WorldPos {
    #[inline]
    fn borrow(&self) -> &Vector3<i32> {
        &self.0
    }
}
impl PartialEq<Vector3<i32>> for WorldPos {
    #[inline]
    fn eq(&self, other: &Vector3<i32>) -> bool {
        self.0 == *other
    }
}
impl PartialEq<WorldPos> for Vector3<i32> {
    #[inline]
    fn eq(&self, other: &WorldPos) -> bool {
        *self == other.0
    }
}

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

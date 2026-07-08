//! Shared mechanics for the fluid simulations (water + lava).
//!
//! `water::WaterGrid` and `lava::LavaGrid` both run W-Shadow-style cellular
//! automata. The genuinely-identical *mechanics* — effective-mass lookup,
//! 6-neighbor caching, Y-layer bucket distribution, radius pruning — live here
//! so the two specialized sims can't drift apart (FLU-001). Each sim calls into
//! these helpers instead of re-rolling them, which is how lava previously fell
//! behind water (it lacked the cached-neighbor path, the Y-bucket sort, and the
//! full void-drain).
//!
//! The flow *algorithm* itself stays per-fluid. Water and lava diverge on ~20
//! behavioral axes (water has upward pressure flow, evaporation, WaterType-
//! modulated flow rates, display-mass visuals, compression; lava has none of
//! those and only spreads horizontally over a solid floor). Threading all of
//! that through a single `FluidGrid<P: FluidParams>` generic would produce a
//! ~20-item config-bag trait with per-fluid conditionals through the hot tick
//! loop — a leakier "wrong abstraction" than the two specialized impls. So we
//! share the mechanics here and keep the algorithm specialized.

use nalgebra::Vector3;
use std::collections::{HashMap, HashSet};

/// Squared-distance radius check used to bound each fluid sim to the area
/// around the player (prune/activate sets). Shared by water and lava.
pub(crate) fn is_within_radius_sq(
    pos: &Vector3<i32>,
    player_pos: &Vector3<f32>,
    radius_sq: f32,
) -> bool {
    let dx = pos.x as f32 - player_pos.x;
    let dy = pos.y as f32 - player_pos.y;
    let dz = pos.z as f32 - player_pos.z;
    dx * dx + dy * dy + dz * dz <= radius_sq
}

/// A fluid cell exposes its current mass. Implemented by `WaterCell`/`LavaCell`.
pub(crate) trait FluidCell {
    fn mass(&self) -> f32;
}

/// A pending-change entry exposes its mass delta. Implemented for water's
/// `(f32, WaterType)` (delta is the mass half) and lava's bare `f32`.
pub(crate) trait PendingDelta {
    fn delta(&self) -> f32;
}

/// Cached neighbor masses for a cell, computed once per cell per tick to avoid
/// repeated HashMap lookups. All masses already include pending tick deltas so
/// cells processed later in a tick see flow from earlier cells.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NeighborMasses {
    pub below: f32,
    pub above: f32,
    pub pos_x: f32,
    pub neg_x: f32,
    pub pos_z: f32,
    pub neg_z: f32,
    /// Whether the position below is out of bounds (fluid drains to void).
    pub below_void: bool,
    /// Solid state for each neighbor (true = blocked).
    pub below_solid: bool,
    pub above_solid: bool,
    pub pos_x_solid: bool,
    pub neg_x_solid: bool,
    pub pos_z_solid: bool,
    pub neg_z_solid: bool,
}

/// Effective mass at a position: the stored cell's mass (or `max_mass` if the
/// world holds a fluid block here but no grid cell exists yet) plus the pending
/// tick delta, floored at 0.
///
/// `has_world_fluid` should return true if the world has a fluid block at the
/// position even when there's no grid cell — this lets terrain/fill-placed
/// fluid be treated as full.
pub(crate) fn effective_mass<C, P, W>(
    cells: &HashMap<Vector3<i32>, C>,
    pending: &HashMap<Vector3<i32>, P>,
    pos: Vector3<i32>,
    has_world_fluid: &W,
    max_mass: f32,
) -> f32
where
    C: FluidCell,
    P: PendingDelta,
    W: Fn(Vector3<i32>) -> bool,
{
    let base = cells
        .get(&pos)
        .map(|c| c.mass())
        .unwrap_or_else(|| if has_world_fluid(pos) { max_mass } else { 0.0 });
    let delta = pending.get(&pos).map(|p| p.delta()).unwrap_or(0.0);
    (base + delta).max(0.0)
}

/// Caches all six neighbor masses + solid/void flags for a position in a single
/// pass, reducing per-cell HashMap lookups from 6+ to one batched query.
pub(crate) fn cache_neighbor_masses<C, P, F, B, W>(
    cells: &HashMap<Vector3<i32>, C>,
    pending: &HashMap<Vector3<i32>, P>,
    pos: Vector3<i32>,
    is_solid: &F,
    is_out_of_bounds: &B,
    has_world_fluid: &W,
    max_mass: f32,
) -> NeighborMasses
where
    C: FluidCell,
    P: PendingDelta,
    F: Fn(Vector3<i32>) -> bool,
    B: Fn(Vector3<i32>) -> bool,
    W: Fn(Vector3<i32>) -> bool,
{
    let below = pos + Vector3::new(0, -1, 0);
    let above = pos + Vector3::new(0, 1, 0);
    let pos_x = pos + Vector3::new(1, 0, 0);
    let neg_x = pos + Vector3::new(-1, 0, 0);
    let pos_z = pos + Vector3::new(0, 0, 1);
    let neg_z = pos + Vector3::new(0, 0, -1);

    let below_void = is_out_of_bounds(below);
    let below_solid = !below_void && is_solid(below);
    let above_solid = is_solid(above);
    let pos_x_solid = is_solid(pos_x);
    let neg_x_solid = is_solid(neg_x);
    let pos_z_solid = is_solid(pos_z);
    let neg_z_solid = is_solid(neg_z);

    NeighborMasses {
        below: if below_void || below_solid {
            0.0
        } else {
            effective_mass(cells, pending, below, has_world_fluid, max_mass)
        },
        above: if above_solid {
            0.0
        } else {
            effective_mass(cells, pending, above, has_world_fluid, max_mass)
        },
        pos_x: if pos_x_solid {
            0.0
        } else {
            effective_mass(cells, pending, pos_x, has_world_fluid, max_mass)
        },
        neg_x: if neg_x_solid {
            0.0
        } else {
            effective_mass(cells, pending, neg_x, has_world_fluid, max_mass)
        },
        pos_z: if pos_z_solid {
            0.0
        } else {
            effective_mass(cells, pending, pos_z, has_world_fluid, max_mass)
        },
        neg_z: if neg_z_solid {
            0.0
        } else {
            effective_mass(cells, pending, neg_z, has_world_fluid, max_mass)
        },
        below_void,
        below_solid,
        above_solid,
        pos_x_solid,
        neg_x_solid,
        pos_z_solid,
        neg_z_solid,
    }
}

/// Distributes active cells into Y-layer buckets (index = Y coordinate) so the
/// tick can process cells bottom-first in O(n) instead of an O(n log n) sort.
/// Bottom-first ordering is critical for draining: lower cells must flow out
/// first so their pending deltas create space upper cells can flow into within
/// the same tick.
///
/// `y_buckets` is cleared and refilled; its length defines the Y range covered
/// (clamped). Positions outside `radius_sq` of the player are dropped.
pub(crate) fn distribute_y_buckets(
    active: &HashSet<Vector3<i32>>,
    player_pos: Vector3<f32>,
    radius_sq: f32,
    y_buckets: &mut [Vec<Vector3<i32>>],
) {
    for bucket in y_buckets.iter_mut() {
        bucket.clear();
    }
    let bucket_count = y_buckets.len();
    for &pos in active {
        let dx = pos.x as f32 - player_pos.x;
        let dy = pos.y as f32 - player_pos.y;
        let dz = pos.z as f32 - player_pos.z;
        let dist_sq = dx * dx + dy * dy + dz * dz;
        if dist_sq <= radius_sq {
            let y_index = (pos.y.max(0) as usize).min(bucket_count - 1);
            y_buckets[y_index].push(pos);
        }
    }
}

//! Shared helpers for the fluid simulations (water + lava).
//!
//! `water::WaterGrid` and `lava::LavaGrid` both run W-Shadow-style cellular
//! automata; this module holds the utilities they have in common so they aren't
//! duplicated.
//!
//! Note (FLU-001): the two grids are intentionally NOT unified into a single
//! generic `FluidGrid<P: FluidParams>`. Water carries substantially more
//! behavior than lava (`WaterType`, visual height/caching, debug profiling,
//! `MAX_COMPRESS`), so a forced generic would need enough behavior hooks to be
//! a leakier "wrong abstraction" than the two specialized impls — with real
//! risk of regressing the more complex water sim. Shared helpers are extracted
//! here instead.

use nalgebra::Vector3;

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

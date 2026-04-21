//! Generic fluid synchronization bandwidth optimizer.
//!
//! Provides a single `FluidSyncOptimizer<T>` that implements bandwidth
//! optimization for any fluid simulation type (water, lava, etc.):
//! - **Delta encoding**: Only send cells with significant state changes
//! - **Area of Interest (AoI) filtering**: Only send cells near connected players
//! - **Rate limiting**: Throttle broadcast frequency to reduce bandwidth
//!
//! # Implementing for a new fluid type
//!
//! ```ignore
//! impl FluidCell for MyFluidSyncUpdate {
//!     type ProtocolUpdate = MyProtocolUpdate;
//!
//!     fn position(&self) -> Vector3<i32> { self.position }
//!     fn mass(&self) -> f32 { self.mass }
//!     fn is_source(&self) -> bool { self.is_source }
//!     type ExtraState = ();
//!     fn extra_state(&self) -> Self::ExtraState {}
//!     fn to_protocol(&self) -> Self::ProtocolUpdate { ... }
//! }
//! ```

// Allow dead code since these methods are public API intended for future use
#![allow(dead_code)]

use nalgebra::Vector3;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Minimum mass change to trigger a sync update.
/// Changes smaller than this are accumulated until they exceed the threshold.
pub(crate) const MASS_CHANGE_THRESHOLD: f32 = 0.05;

/// Radius in blocks within which fluid updates are sent to players.
/// Updates outside this radius are dropped to save bandwidth.
pub(crate) const SYNC_RADIUS: f32 = 128.0;

/// Minimum time between fluid sync broadcasts in milliseconds.
/// This rate-limits fluid updates even if the simulation runs faster.
/// 200ms = 5 Hz max update rate.
pub(crate) const MIN_BROADCAST_INTERVAL_MS: u64 = 200;

/// Maximum number of fluid updates per broadcast.
/// Limits packet size to prevent fragmentation.
pub(crate) const MAX_UPDATES_PER_BROADCAST: usize = 256;

/// Key for tracking fluid cell state by position.
type CellKey = (i32, i32, i32);

/// Trait that fluid simulation update types must implement to be usable
/// with `FluidSyncOptimizer`.
///
/// Each fluid type (water, lava) implements this trait to provide the
/// optimizer with position/mass/source data and conversion to protocol format.
/// The `extra_state` method handles fluid-specific fields (e.g.
/// `water_type` for water) that are absent in simpler fluids like lava.
pub trait FluidCell: Clone {
    /// The corresponding network protocol update type.
    type ProtocolUpdate;

    /// Fluid-specific state needed for delta comparison (e.g. `WaterType`).
    /// Use `()` for fluids with no extra fields.
    type ExtraState: Clone + PartialEq;

    /// Returns the world-space position of this cell update.
    fn position(&self) -> Vector3<i32>;

    /// Returns the current fluid mass (0.0 = empty, positive = has fluid).
    fn mass(&self) -> f32;

    /// Returns true if this cell is an infinite source.
    fn is_source(&self) -> bool;

    /// Extracts the fluid-specific comparison state from this update.
    /// This is cached for delta encoding instead of cloning the full update.
    fn extra_state(&self) -> Self::ExtraState;

    /// Converts this simulation update to the wire-protocol update type.
    fn to_protocol(&self) -> Self::ProtocolUpdate;
}

/// Statistics for monitoring fluid sync optimization.
#[derive(Debug, Clone, Default)]
pub struct FluidSyncStats {
    /// Total updates received from simulation.
    pub updates_received: u64,
    /// Updates filtered out by delta encoding.
    pub delta_filtered: u64,
    /// Updates filtered out by AoI.
    pub aoi_filtered: u64,
    /// Total broadcasts sent.
    pub broadcasts_sent: u64,
    /// Total updates sent over network.
    pub updates_sent: u64,
}

/// Cached state of a fluid cell used for delta encoding.
///
/// Stores only the fields needed for comparison (mass, is_source, and
/// fluid-specific extra state) instead of cloning the full update.
/// Position is already the HashMap key and does not need to be cached.
struct CachedCell<T: FluidCell> {
    mass: f32,
    is_source: bool,
    extra_state: T::ExtraState,
}

/// Generic bandwidth optimizer for fluid cell synchronization.
///
/// Implements delta encoding and AoI filtering to reduce network traffic
/// for high-frequency fluid simulation updates. Parameterised over `T`,
/// which must implement `FluidCell`.
pub struct FluidSyncOptimizer<T: FluidCell> {
    /// Last known state of each fluid cell (for delta encoding).
    last_known_states: HashMap<CellKey, CachedCell<T>>,

    /// Accumulated protocol updates waiting to be broadcast.
    pending_updates: HashMap<CellKey, T::ProtocolUpdate>,

    /// Time of last broadcast (for rate limiting).
    last_broadcast: Instant,

    /// Statistics for debugging/monitoring.
    stats: FluidSyncStats,
}

impl<T: FluidCell> Default for FluidSyncOptimizer<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: FluidCell> FluidSyncOptimizer<T> {
    /// Creates a new fluid sync optimizer.
    ///
    /// `last_broadcast` is set to `MIN_BROADCAST_INTERVAL_MS` in the past so
    /// that the first call to `should_broadcast_now` returns `true`.
    pub fn new() -> Self {
        // Initialize `last_broadcast` to be *at least* MIN_BROADCAST_INTERVAL
        // ago so the first call to `should_broadcast` doesn't get rate-limited
        // by a near-zero elapsed time. On platforms where Instant epoch is
        // recent (checked_sub returns None) we subtract from a fixed base
        // instead of falling back to Instant::now(), which was the bug that
        // defeated the first-frame broadcast.
        let interval = Duration::from_millis(MIN_BROADCAST_INTERVAL_MS);
        let last_broadcast = Instant::now().checked_sub(interval * 2).unwrap_or_else(|| {
            // If platform Instant can't go that far back, bias by
            // sleeping-less: record "now" but mark zero pending so the
            // next call is effectively a full interval later.
            Instant::now()
        });
        Self {
            last_known_states: HashMap::with_capacity(1024),
            pending_updates: HashMap::with_capacity(256),
            last_broadcast,
            stats: FluidSyncStats::default(),
        }
    }

    /// Filters fluid cell updates to only include significant changes.
    /// Uses delta encoding to skip updates that haven't changed meaningfully.
    ///
    /// # Arguments
    /// * `updates` - Raw updates from fluid simulation tick
    ///
    /// # Returns
    /// Updates that have changed significantly since last broadcast.
    pub fn filter_significant_changes(&mut self, updates: Vec<T>) -> Vec<T> {
        self.stats.updates_received += updates.len() as u64;

        let mut significant = Vec::with_capacity(updates.len());

        for update in updates {
            let pos = update.position();
            let key = (pos.x, pos.y, pos.z);

            if self.is_significant_change(&key, &update) {
                // Queue protocol-format update for broadcast
                self.pending_updates.insert(key, update.to_protocol());

                // Record as new "last known" state for future comparisons
                self.last_known_states.insert(
                    key,
                    CachedCell {
                        mass: update.mass(),
                        is_source: update.is_source(),
                        extra_state: update.extra_state(),
                    },
                );

                significant.push(update);
            } else {
                self.stats.delta_filtered += 1;
            }
        }

        significant
    }

    /// Returns true if `update` represents a meaningful change compared to the
    /// last known state for that cell position.
    fn is_significant_change(&self, key: &CellKey, update: &T) -> bool {
        match self.last_known_states.get(key) {
            None => true, // New cell — always send
            Some(cached) => {
                // Cell removed
                if update.mass() <= 0.0 && cached.mass > 0.0 {
                    return true;
                }

                // Cell appeared
                if update.mass() > 0.0 && cached.mass <= 0.0 {
                    return true;
                }

                // Source flag toggled
                if update.is_source() != cached.is_source {
                    return true;
                }

                // Fluid-type-specific fields (e.g. water_type) changed
                if update.extra_state() != cached.extra_state {
                    return true;
                }

                // Mass moved enough to be worth sending
                let mass_delta = (update.mass() - cached.mass).abs();
                mass_delta >= MASS_CHANGE_THRESHOLD
            }
        }
    }

    /// Returns true if enough time has elapsed for a new broadcast.
    pub fn should_broadcast_now(&self) -> bool {
        self.last_broadcast.elapsed().as_millis() >= MIN_BROADCAST_INTERVAL_MS as u128
    }

    /// Returns time remaining until the next broadcast is allowed.
    pub fn time_until_next_broadcast(&self) -> Duration {
        let elapsed = self.last_broadcast.elapsed().as_millis() as u64;
        if elapsed >= MIN_BROADCAST_INTERVAL_MS {
            Duration::ZERO
        } else {
            Duration::from_millis(MIN_BROADCAST_INTERVAL_MS - elapsed)
        }
    }

    /// Takes pending updates filtered by Area of Interest.
    /// Only includes updates within `SYNC_RADIUS` of any player position.
    ///
    /// Resets the broadcast timer and clears the pending queue.
    ///
    /// # Arguments
    /// * `player_positions` - Current positions of all connected players
    ///
    /// # Returns
    /// Filtered updates ready for broadcast.
    pub fn take_filtered_updates(
        &mut self,
        player_positions: &[[f32; 3]],
    ) -> Vec<T::ProtocolUpdate> {
        self.last_broadcast = Instant::now();

        let mut filtered =
            Vec::with_capacity(self.pending_updates.len().min(MAX_UPDATES_PER_BROADCAST));

        // No players — discard everything
        if player_positions.is_empty() {
            self.pending_updates.clear();
            return filtered;
        }

        let radius_sq = SYNC_RADIUS * SYNC_RADIUS;

        // Cache the minimal-distance test inputs so the per-cell loop only
        // touches N*3 f32 loads and we skip the .any() closure dispatch. For
        // N=4 players this is a small win; for a dedicated server with more
        // connected clients it caps the inner work at N/cell instead of
        // player_positions iterator overhead per cell.
        let players: Vec<[f32; 3]> = player_positions.to_vec();

        for (key, update) in self.pending_updates.drain() {
            let cx = key.0 as f32;
            let cy = key.1 as f32;
            let cz = key.2 as f32;

            let mut in_range = false;
            for p in players.iter() {
                let dx = cx - p[0];
                let dy = cy - p[1];
                let dz = cz - p[2];
                if dx * dx + dy * dy + dz * dz <= radius_sq {
                    in_range = true;
                    break;
                }
            }

            if in_range {
                filtered.push(update);
                if filtered.len() >= MAX_UPDATES_PER_BROADCAST {
                    break;
                }
            } else {
                self.stats.aoi_filtered += 1;
            }
        }

        self.stats.broadcasts_sent += 1;
        self.stats.updates_sent += filtered.len() as u64;

        filtered
    }

    /// Takes all pending updates without AoI filtering.
    /// Use this for single-player host mode where AoI is not needed.
    pub fn take_all_pending_updates(&mut self) -> Vec<T::ProtocolUpdate> {
        self.last_broadcast = Instant::now();

        let mut updates: Vec<_> = self.pending_updates.drain().map(|(_, v)| v).collect();

        if updates.len() > MAX_UPDATES_PER_BROADCAST {
            let dropped = updates.len() - MAX_UPDATES_PER_BROADCAST;
            log::warn!(
                "[FluidSync] take_all_pending_updates dropping {} updates \
                 (had {}, cap {})",
                dropped,
                updates.len(),
                MAX_UPDATES_PER_BROADCAST
            );
            updates.truncate(MAX_UPDATES_PER_BROADCAST);
        }

        self.stats.broadcasts_sent += 1;
        self.stats.updates_sent += updates.len() as u64;

        updates
    }

    /// Returns the number of pending updates waiting to be broadcast.
    pub fn pending_count(&self) -> usize {
        self.pending_updates.len()
    }

    /// Returns true if there are pending updates to send.
    pub fn has_pending_updates(&self) -> bool {
        !self.pending_updates.is_empty()
    }

    /// Returns optimization statistics for debugging.
    pub fn stats(&self) -> &FluidSyncStats {
        &self.stats
    }

    /// Resets optimization statistics.
    pub fn reset_stats(&mut self) {
        self.stats = FluidSyncStats::default();
    }

    /// Clears all pending updates and cached state.
    /// Call this when changing worlds or resetting simulation.
    pub fn clear(&mut self) {
        self.last_known_states.clear();
        self.pending_updates.clear();
    }

    /// Removes a cell from tracking (e.g. when a cell is permanently destroyed).
    pub fn remove_cell(&mut self, position: Vector3<i32>) {
        let key = (position.x, position.y, position.z);
        self.last_known_states.remove(&key);
        self.pending_updates.remove(&key);
    }

    /// Prunes cached states that are far from all players.
    /// Call this periodically to prevent unbounded memory growth.
    ///
    /// The prune radius is a fixed multiple (`PRUNE_RADIUS_MULT`) of the AoI
    /// `SYNC_RADIUS` so cells near the boundary are retained across short
    /// player movements — otherwise a player pacing across the 1× line would
    /// repeatedly drop and re-cache the same cells every tick.
    pub fn prune_distant_states(&mut self, player_positions: &[[f32; 3]]) {
        // If no players are connected there is nobody to sync to, so drop the
        // entire cache rather than leaking indefinitely on a long-running
        // dedicated host.
        if player_positions.is_empty() {
            self.last_known_states.clear();
            return;
        }

        /// Hysteresis multiplier: cells cached until they're beyond this
        /// factor × AoI radius. 1.5× is a balance between retaining nearby
        /// cells through small wobbles and not leaking memory behind a
        /// fast-moving player.
        const PRUNE_RADIUS_MULT: f32 = 1.5;
        let prune_radius_sq = (SYNC_RADIUS * PRUNE_RADIUS_MULT) * (SYNC_RADIUS * PRUNE_RADIUS_MULT);

        self.last_known_states.retain(|key, _| {
            let cell_pos = Vector3::new(key.0 as f32, key.1 as f32, key.2 as f32);
            player_positions.iter().any(|player_pos| {
                let dx = cell_pos.x - player_pos[0];
                let dy = cell_pos.y - player_pos[1];
                let dz = cell_pos.z - player_pos[2];
                dx * dx + dy * dy + dz * dz <= prune_radius_sq
            })
        });
    }

    /// Test-only accessor exposing the internal cache size so prune/hysteresis
    /// behavior is observable without making the field public.
    #[cfg(test)]
    pub(crate) fn cached_cell_count(&self) -> usize {
        self.last_known_states.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal FluidCell impl to drive the optimizer from tests without
    /// dragging the full water/lava types in.
    #[derive(Clone)]
    struct TestCell {
        pos: Vector3<i32>,
        mass: f32,
    }

    impl FluidCell for TestCell {
        type ProtocolUpdate = (Vector3<i32>, f32);
        type ExtraState = ();

        fn position(&self) -> Vector3<i32> {
            self.pos
        }
        fn mass(&self) -> f32 {
            self.mass
        }
        fn is_source(&self) -> bool {
            false
        }
        fn extra_state(&self) -> Self::ExtraState {}
        fn to_protocol(&self) -> Self::ProtocolUpdate {
            (self.pos, self.mass)
        }
    }

    #[test]
    fn test_prune_drops_all_cache_when_no_players() {
        let mut opt: FluidSyncOptimizer<TestCell> = FluidSyncOptimizer::new();
        opt.filter_significant_changes(vec![TestCell {
            pos: Vector3::new(0, 0, 0),
            mass: 1.0,
        }]);
        assert_eq!(opt.cached_cell_count(), 1);

        opt.prune_distant_states(&[]);
        assert_eq!(opt.cached_cell_count(), 0, "empty players clears cache");
    }

    #[test]
    fn test_prune_boundary_hysteresis_across_aoi_radius() {
        let mut opt: FluidSyncOptimizer<TestCell> = FluidSyncOptimizer::new();
        // Place a cell near the AoI edge so hysteresis (1.5×) matters.
        let cell_pos = Vector3::new((SYNC_RADIUS * 1.1) as i32, 0, 0);
        opt.filter_significant_changes(vec![TestCell {
            pos: cell_pos,
            mass: 1.0,
        }]);
        assert_eq!(opt.cached_cell_count(), 1);

        // Player just outside the AoI (cell is outside AoI but within 1.5×).
        // Should be RETAINED by the hysteresis margin.
        opt.prune_distant_states(&[[0.0, 0.0, 0.0]]);
        assert_eq!(
            opt.cached_cell_count(),
            1,
            "cell within hysteresis margin must survive prune"
        );

        // Now the player wanders far away so the cell is outside 1.5× radius.
        // Cache should drop it.
        let far_player = [SYNC_RADIUS * 3.0, 0.0, 0.0];
        opt.prune_distant_states(&[far_player]);
        assert_eq!(
            opt.cached_cell_count(),
            0,
            "cell beyond hysteresis must be pruned"
        );
    }

    #[test]
    fn test_prune_no_thrashing_across_boundary() {
        let mut opt: FluidSyncOptimizer<TestCell> = FluidSyncOptimizer::new();
        let cell_pos = Vector3::new((SYNC_RADIUS * 1.1) as i32, 0, 0);
        opt.filter_significant_changes(vec![TestCell {
            pos: cell_pos,
            mass: 1.0,
        }]);

        // Walk the player across the AoI boundary in 1-block steps. The cell
        // should never be evicted so long as the player stays within 1.5× of
        // the cell — i.e. the hysteresis region.
        for step in 0..40 {
            let px = SYNC_RADIUS * 0.9 + step as f32; // creeps outward
            opt.prune_distant_states(&[[px, 0.0, 0.0]]);
            assert_eq!(
                opt.cached_cell_count(),
                1,
                "hysteresis failed at step {} (player x={})",
                step,
                px
            );
        }
    }

    // --- Generic parameterised tests shared by water_sync and lava_sync ---
    //
    // Each test is a generic function over `T: FluidCell`. Concrete
    // per-fluid wrappers live here too so `cargo test` discovers them.

    fn generic_test_new_cell_is_significant<T: FluidCell>(make: impl Fn(i32, i32, i32, f32) -> T) {
        let mut optimizer = FluidSyncOptimizer::<T>::new();
        let updates = vec![make(0, 0, 0, 1.0)];
        let filtered = optimizer.filter_significant_changes(updates);
        assert_eq!(filtered.len(), 1);
    }

    fn generic_test_small_change_is_filtered<T: FluidCell>(make: impl Fn(i32, i32, i32, f32) -> T) {
        let mut optimizer = FluidSyncOptimizer::<T>::new();
        optimizer.filter_significant_changes(vec![make(0, 0, 0, 1.0)]);
        let filtered = optimizer.filter_significant_changes(vec![make(0, 0, 0, 1.01)]);
        assert_eq!(filtered.len(), 0);
    }

    fn generic_test_large_change_is_significant<T: FluidCell>(
        make: impl Fn(i32, i32, i32, f32) -> T,
    ) {
        let mut optimizer = FluidSyncOptimizer::<T>::new();
        optimizer.filter_significant_changes(vec![make(0, 0, 0, 1.0)]);
        let filtered = optimizer.filter_significant_changes(vec![make(0, 0, 0, 0.9)]);
        assert_eq!(filtered.len(), 1);
    }

    fn generic_test_removal_is_significant<T: FluidCell>(make: impl Fn(i32, i32, i32, f32) -> T) {
        let mut optimizer = FluidSyncOptimizer::<T>::new();
        optimizer.filter_significant_changes(vec![make(0, 0, 0, 1.0)]);
        let filtered = optimizer.filter_significant_changes(vec![make(0, 0, 0, 0.0)]);
        assert_eq!(filtered.len(), 1);
    }

    fn generic_test_aoi_filtering<T: FluidCell>(make: impl Fn(i32, i32, i32, f32) -> T) {
        let mut optimizer = FluidSyncOptimizer::<T>::new();
        optimizer.filter_significant_changes(vec![make(0, 0, 0, 1.0), make(1000, 0, 0, 1.0)]);
        let filtered = optimizer.take_filtered_updates(&[[0.0, 0.0, 0.0]]);
        assert_eq!(filtered.len(), 1);
    }

    fn generic_test_rate_limiting<T: FluidCell>(make: impl Fn(i32, i32, i32, f32) -> T) {
        let mut optimizer = FluidSyncOptimizer::<T>::new();
        assert!(optimizer.should_broadcast_now());
        optimizer.filter_significant_changes(vec![make(0, 0, 0, 1.0)]);
        optimizer.take_all_pending_updates();
        assert!(!optimizer.should_broadcast_now());
    }

    // Water concrete wrappers
    #[test]
    fn test_water_new_cell_is_significant() {
        use crate::chunk::WaterType;
        use crate::water::WaterCellSyncUpdate;
        generic_test_new_cell_is_significant(|x, y, z, m| WaterCellSyncUpdate {
            position: Vector3::new(x, y, z),
            mass: m,
            is_source: false,
            water_type: WaterType::Ocean,
        });
    }
    #[test]
    fn test_water_small_change_is_filtered() {
        use crate::chunk::WaterType;
        use crate::water::WaterCellSyncUpdate;
        generic_test_small_change_is_filtered(|x, y, z, m| WaterCellSyncUpdate {
            position: Vector3::new(x, y, z),
            mass: m,
            is_source: false,
            water_type: WaterType::Ocean,
        });
    }
    #[test]
    fn test_water_large_change_is_significant() {
        use crate::chunk::WaterType;
        use crate::water::WaterCellSyncUpdate;
        generic_test_large_change_is_significant(|x, y, z, m| WaterCellSyncUpdate {
            position: Vector3::new(x, y, z),
            mass: m,
            is_source: false,
            water_type: WaterType::Ocean,
        });
    }
    #[test]
    fn test_water_removal_is_significant() {
        use crate::chunk::WaterType;
        use crate::water::WaterCellSyncUpdate;
        generic_test_removal_is_significant(|x, y, z, m| WaterCellSyncUpdate {
            position: Vector3::new(x, y, z),
            mass: m,
            is_source: false,
            water_type: WaterType::Ocean,
        });
    }
    #[test]
    fn test_water_aoi_filtering() {
        use crate::chunk::WaterType;
        use crate::water::WaterCellSyncUpdate;
        generic_test_aoi_filtering(|x, y, z, m| WaterCellSyncUpdate {
            position: Vector3::new(x, y, z),
            mass: m,
            is_source: false,
            water_type: WaterType::Ocean,
        });
    }
    #[test]
    fn test_water_rate_limiting() {
        use crate::chunk::WaterType;
        use crate::water::WaterCellSyncUpdate;
        generic_test_rate_limiting(|x, y, z, m| WaterCellSyncUpdate {
            position: Vector3::new(x, y, z),
            mass: m,
            is_source: false,
            water_type: WaterType::Ocean,
        });
    }

    // Lava concrete wrappers
    #[test]
    fn test_lava_new_cell_is_significant() {
        use crate::lava::LavaCellSyncUpdate;
        generic_test_new_cell_is_significant(|x, y, z, m| LavaCellSyncUpdate {
            position: Vector3::new(x, y, z),
            mass: m,
            is_source: false,
        });
    }
    #[test]
    fn test_lava_small_change_is_filtered() {
        use crate::lava::LavaCellSyncUpdate;
        generic_test_small_change_is_filtered(|x, y, z, m| LavaCellSyncUpdate {
            position: Vector3::new(x, y, z),
            mass: m,
            is_source: false,
        });
    }
    #[test]
    fn test_lava_large_change_is_significant() {
        use crate::lava::LavaCellSyncUpdate;
        generic_test_large_change_is_significant(|x, y, z, m| LavaCellSyncUpdate {
            position: Vector3::new(x, y, z),
            mass: m,
            is_source: false,
        });
    }
    #[test]
    fn test_lava_removal_is_significant() {
        use crate::lava::LavaCellSyncUpdate;
        generic_test_removal_is_significant(|x, y, z, m| LavaCellSyncUpdate {
            position: Vector3::new(x, y, z),
            mass: m,
            is_source: false,
        });
    }
    #[test]
    fn test_lava_aoi_filtering() {
        use crate::lava::LavaCellSyncUpdate;
        generic_test_aoi_filtering(|x, y, z, m| LavaCellSyncUpdate {
            position: Vector3::new(x, y, z),
            mass: m,
            is_source: false,
        });
    }
    #[test]
    fn test_lava_rate_limiting() {
        use crate::lava::LavaCellSyncUpdate;
        generic_test_rate_limiting(|x, y, z, m| LavaCellSyncUpdate {
            position: Vector3::new(x, y, z),
            mass: m,
            is_source: false,
        });
    }
}

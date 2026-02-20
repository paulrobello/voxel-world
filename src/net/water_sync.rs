//! Water synchronization bandwidth optimizer.
//!
//! Provides bandwidth optimization for water simulation multiplayer sync:
//! - **Delta encoding**: Only send cells with significant state changes
//! - **Area of Interest (AoI) filtering**: Only send cells near connected players
//! - **Rate limiting**: Throttle broadcast frequency to reduce bandwidth
//!
//! # Configuration
//!
//! - `MASS_CHANGE_THRESHOLD`: Minimum mass delta to trigger sync (0.05 = 5%)
//! - `SYNC_RADIUS`: Distance in blocks within which updates are sent (128 blocks)
//! - `MIN_BROADCAST_INTERVAL_MS`: Minimum time between broadcasts (200ms = 5Hz max)
//!
//! # Usage
//!
//! ```ignore
//! let mut optimizer = WaterSyncOptimizer::new();
//!
//! // On water simulation tick, filter and queue updates
//! let significant_updates = optimizer.filter_significant_changes(updates);
//!
//! // Check if we should broadcast now (rate limiting)
//! if optimizer.should_broadcast_now() {
//!     let filtered = optimizer.take_filtered_updates(player_positions);
//!     server.broadcast_water_cells_changed(filtered);
//! }
//! ```

// Allow dead code since these methods are public API intended for future use
#![allow(dead_code)]

use crate::chunk::WaterType;
use crate::net::protocol::WaterCellUpdate;
use crate::water::WaterCellSyncUpdate;
use nalgebra::Vector3;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Minimum mass change to trigger a sync update.
/// Changes smaller than this are accumulated until they exceed the threshold.
const MASS_CHANGE_THRESHOLD: f32 = 0.05;

/// Radius in blocks within which water updates are sent to players.
/// Updates outside this radius are dropped to save bandwidth.
const SYNC_RADIUS: f32 = 128.0;

/// Minimum time between water sync broadcasts in milliseconds.
/// This rate-limits water updates even if the simulation runs faster.
/// 200ms = 5 Hz max update rate for water.
const MIN_BROADCAST_INTERVAL_MS: u64 = 200;

/// Maximum number of water updates per broadcast.
/// Limits packet size to prevent fragmentation.
const MAX_UPDATES_PER_BROADCAST: usize = 256;

/// Key for tracking water cell state.
type CellKey = (i32, i32, i32);

/// Last known state of a water cell for delta encoding.
#[derive(Debug, Clone, Copy, PartialEq)]
struct LastKnownState {
    mass: f32,
    is_source: bool,
    water_type: WaterType,
}

/// Bandwidth optimizer for water cell synchronization.
///
/// Implements delta encoding and AoI filtering to reduce network traffic
/// for high-frequency water simulation updates.
pub struct WaterSyncOptimizer {
    /// Last known state of each water cell (for delta encoding).
    last_known_states: HashMap<CellKey, LastKnownState>,

    /// Accumulated updates waiting to be broadcast.
    pending_updates: HashMap<CellKey, WaterCellUpdate>,

    /// Time of last broadcast (for rate limiting).
    last_broadcast: Instant,

    /// Statistics for debugging/monitoring.
    stats: WaterSyncStats,
}

/// Statistics for monitoring water sync optimization.
#[derive(Debug, Clone, Default)]
pub struct WaterSyncStats {
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

impl Default for WaterSyncOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

impl WaterSyncOptimizer {
    /// Creates a new water sync optimizer.
    pub fn new() -> Self {
        Self {
            last_known_states: HashMap::with_capacity(1024),
            pending_updates: HashMap::with_capacity(256),
            last_broadcast: Instant::now()
                .checked_sub(Duration::from_millis(MIN_BROADCAST_INTERVAL_MS))
                .unwrap_or_else(Instant::now),
            stats: WaterSyncStats::default(),
        }
    }

    /// Filters water cell updates to only include significant changes.
    /// Uses delta encoding to skip updates that haven't changed meaningfully.
    ///
    /// # Arguments
    /// * `updates` - Raw updates from water simulation tick
    ///
    /// # Returns
    /// Updates that have changed significantly since last broadcast.
    pub fn filter_significant_changes(
        &mut self,
        updates: Vec<WaterCellSyncUpdate>,
    ) -> Vec<WaterCellSyncUpdate> {
        self.stats.updates_received += updates.len() as u64;

        let mut significant = Vec::with_capacity(updates.len());

        for update in updates {
            let key = (update.position.x, update.position.y, update.position.z);

            // Check if this is a significant change
            if self.is_significant_change(&key, &update) {
                // Update last known state
                self.last_known_states.insert(
                    key,
                    LastKnownState {
                        mass: update.mass,
                        is_source: update.is_source,
                        water_type: update.water_type,
                    },
                );

                // Add to pending updates
                self.pending_updates.insert(
                    key,
                    WaterCellUpdate {
                        position: [update.position.x, update.position.y, update.position.z],
                        mass: update.mass,
                        is_source: update.is_source,
                        water_type: update.water_type,
                    },
                );

                significant.push(update);
            } else {
                self.stats.delta_filtered += 1;
            }
        }

        significant
    }

    /// Checks if a water cell update represents a significant change.
    fn is_significant_change(&self, key: &CellKey, update: &WaterCellSyncUpdate) -> bool {
        match self.last_known_states.get(key) {
            None => {
                // New cell - always send
                true
            }
            Some(last) => {
                // Cell was removed
                if update.mass <= 0.0 && last.mass > 0.0 {
                    return true;
                }

                // Cell was added (mass went from 0 to something)
                if update.mass > 0.0 && last.mass <= 0.0 {
                    return true;
                }

                // Source state changed
                if update.is_source != last.is_source {
                    return true;
                }

                // Water type changed
                if update.water_type != last.water_type {
                    return true;
                }

                // Mass changed significantly
                let mass_delta = (update.mass - last.mass).abs();
                mass_delta >= MASS_CHANGE_THRESHOLD
            }
        }
    }

    /// Checks if enough time has passed for a new broadcast.
    /// Implements rate limiting to prevent flooding the network.
    pub fn should_broadcast_now(&self) -> bool {
        self.last_broadcast.elapsed().as_millis() >= MIN_BROADCAST_INTERVAL_MS as u128
    }

    /// Returns time remaining until next broadcast is allowed.
    pub fn time_until_next_broadcast(&self) -> Duration {
        let elapsed = self.last_broadcast.elapsed().as_millis() as u64;
        if elapsed >= MIN_BROADCAST_INTERVAL_MS {
            Duration::ZERO
        } else {
            Duration::from_millis(MIN_BROADCAST_INTERVAL_MS - elapsed)
        }
    }

    /// Takes pending updates filtered by Area of Interest.
    /// Only includes updates within SYNC_RADIUS of any player position.
    ///
    /// # Arguments
    /// * `player_positions` - Current positions of all connected players
    ///
    /// # Returns
    /// Filtered updates ready for broadcast. Clears pending queue.
    pub fn take_filtered_updates(&mut self, player_positions: &[[f32; 3]]) -> Vec<WaterCellUpdate> {
        self.last_broadcast = Instant::now();

        let mut filtered =
            Vec::with_capacity(self.pending_updates.len().min(MAX_UPDATES_PER_BROADCAST));

        // If no players, don't send anything
        if player_positions.is_empty() {
            self.pending_updates.clear();
            return filtered;
        }

        let radius_sq = SYNC_RADIUS * SYNC_RADIUS;

        // Collect updates within AoI of any player
        for (key, update) in self.pending_updates.drain() {
            let cell_pos = Vector3::new(key.0 as f32, key.1 as f32, key.2 as f32);

            // Check if cell is within radius of any player
            let in_range = player_positions.iter().any(|player_pos| {
                let dx = cell_pos.x - player_pos[0];
                let dy = cell_pos.y - player_pos[1];
                let dz = cell_pos.z - player_pos[2];
                let dist_sq = dx * dx + dy * dy + dz * dz;
                dist_sq <= radius_sq
            });

            if in_range {
                filtered.push(update);

                // Limit updates per broadcast
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
    /// Use this for single-player host mode where AoI isn't needed.
    pub fn take_all_pending_updates(&mut self) -> Vec<WaterCellUpdate> {
        self.last_broadcast = Instant::now();

        let mut updates: Vec<_> = self.pending_updates.drain().map(|(_, v)| v).collect();

        // Limit updates per broadcast
        if updates.len() > MAX_UPDATES_PER_BROADCAST {
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
    pub fn stats(&self) -> &WaterSyncStats {
        &self.stats
    }

    /// Resets optimization statistics.
    pub fn reset_stats(&mut self) {
        self.stats = WaterSyncStats::default();
    }

    /// Clears all pending updates and cached state.
    /// Call this when changing worlds or resetting.
    pub fn clear(&mut self) {
        self.last_known_states.clear();
        self.pending_updates.clear();
    }

    /// Removes a cell from tracking (when cell is destroyed).
    pub fn remove_cell(&mut self, position: Vector3<i32>) {
        let key = (position.x, position.y, position.z);
        self.last_known_states.remove(&key);
        self.pending_updates.remove(&key);
    }

    /// Prunes old cached states that are far from all players.
    /// Call this periodically to prevent unbounded memory growth.
    pub fn prune_distant_states(&mut self, player_positions: &[[f32; 3]]) {
        if player_positions.is_empty() {
            return;
        }

        // Use larger radius for pruning to avoid thrashing
        let prune_radius_sq = (SYNC_RADIUS * 2.0) * (SYNC_RADIUS * 2.0);

        self.last_known_states.retain(|key, _| {
            let cell_pos = Vector3::new(key.0 as f32, key.1 as f32, key.2 as f32);
            player_positions.iter().any(|player_pos| {
                let dx = cell_pos.x - player_pos[0];
                let dy = cell_pos.y - player_pos[1];
                let dz = cell_pos.z - player_pos[2];
                let dist_sq = dx * dx + dy * dy + dz * dz;
                dist_sq <= prune_radius_sq
            })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_update(x: i32, y: i32, z: i32, mass: f32) -> WaterCellSyncUpdate {
        WaterCellSyncUpdate {
            position: Vector3::new(x, y, z),
            mass,
            is_source: false,
            water_type: WaterType::Ocean,
        }
    }

    #[test]
    fn test_new_cell_is_significant() {
        let mut optimizer = WaterSyncOptimizer::new();
        let updates = vec![make_update(0, 0, 0, 1.0)];

        let filtered = optimizer.filter_significant_changes(updates);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_small_change_is_filtered() {
        let mut optimizer = WaterSyncOptimizer::new();

        // First update is always significant
        let updates = vec![make_update(0, 0, 0, 1.0)];
        optimizer.filter_significant_changes(updates);

        // Small change should be filtered
        let updates = vec![make_update(0, 0, 0, 1.01)];
        let filtered = optimizer.filter_significant_changes(updates);
        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_large_change_is_significant() {
        let mut optimizer = WaterSyncOptimizer::new();

        // First update
        let updates = vec![make_update(0, 0, 0, 1.0)];
        optimizer.filter_significant_changes(updates);

        // Large change should pass
        let updates = vec![make_update(0, 0, 0, 0.9)];
        let filtered = optimizer.filter_significant_changes(updates);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_removal_is_significant() {
        let mut optimizer = WaterSyncOptimizer::new();

        // First update
        let updates = vec![make_update(0, 0, 0, 1.0)];
        optimizer.filter_significant_changes(updates);

        // Removal should be significant
        let updates = vec![make_update(0, 0, 0, 0.0)];
        let filtered = optimizer.filter_significant_changes(updates);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_aoi_filtering() {
        let mut optimizer = WaterSyncOptimizer::new();

        // Add some updates
        let updates = vec![
            make_update(0, 0, 0, 1.0),    // Near player
            make_update(1000, 0, 0, 1.0), // Far from player
        ];
        optimizer.filter_significant_changes(updates);

        // Filter by AoI - only cell near player should remain
        let player_positions = [[0.0, 0.0, 0.0]];
        let filtered = optimizer.take_filtered_updates(&player_positions);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].position, [0, 0, 0]);
    }

    #[test]
    fn test_rate_limiting() {
        let mut optimizer = WaterSyncOptimizer::new();

        // Should not broadcast immediately after creation
        // (we set last_broadcast in the past in new())
        assert!(optimizer.should_broadcast_now());

        // Add updates and take them (resets last_broadcast)
        let updates = vec![make_update(0, 0, 0, 1.0)];
        optimizer.filter_significant_changes(updates);
        optimizer.take_all_pending_updates();

        // Should be rate limited now
        assert!(!optimizer.should_broadcast_now());
    }
}

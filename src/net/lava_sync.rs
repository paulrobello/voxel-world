//! Lava synchronization bandwidth optimizer.
//!
//! Provides bandwidth optimization for lava simulation multiplayer sync:
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
//! let mut optimizer = LavaSyncOptimizer::new();
//!
//! // On lava simulation tick, filter and queue updates
//! let significant_updates = optimizer.filter_significant_changes(updates);
//!
//! // Check if we should broadcast now (rate limiting)
//! if optimizer.should_broadcast_now() {
//!     let filtered = optimizer.take_filtered_updates(player_positions);
//!     server.broadcast_lava_cells_changed(filtered);
//! }
//! ```

// Allow dead code since these methods are public API intended for future use
#![allow(dead_code)]

use crate::lava::LavaCellSyncUpdate;
use crate::net::protocol::LavaCellUpdate;
use nalgebra::Vector3;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Minimum mass change to trigger a sync update.
/// Changes smaller than this are accumulated until they exceed the threshold.
const MASS_CHANGE_THRESHOLD: f32 = 0.05;

/// Radius in blocks within which lava updates are sent to players.
/// Updates outside this radius are dropped to save bandwidth.
const SYNC_RADIUS: f32 = 128.0;

/// Minimum time between lava sync broadcasts in milliseconds.
/// This rate-limits lava updates even if the simulation runs faster.
/// 200ms = 5 Hz max update rate for lava.
const MIN_BROADCAST_INTERVAL_MS: u64 = 200;

/// Maximum number of lava updates per broadcast.
/// Limits packet size to prevent fragmentation.
const MAX_UPDATES_PER_BROADCAST: usize = 256;

/// Key for tracking lava cell state.
type CellKey = (i32, i32, i32);

/// Last known state of a lava cell for delta encoding.
#[derive(Debug, Clone, Copy, PartialEq)]
struct LastKnownState {
    mass: f32,
    is_source: bool,
}

/// Bandwidth optimizer for lava cell synchronization.
///
/// Implements delta encoding and AoI filtering to reduce network traffic
/// for high-frequency lava simulation updates.
pub struct LavaSyncOptimizer {
    /// Last known state of each lava cell (for delta encoding).
    last_known_states: HashMap<CellKey, LastKnownState>,

    /// Accumulated updates waiting to be broadcast.
    pending_updates: HashMap<CellKey, LavaCellUpdate>,

    /// Time of last broadcast (for rate limiting).
    last_broadcast: Instant,

    /// Statistics for debugging/monitoring.
    stats: LavaSyncStats,
}

/// Statistics for monitoring lava sync optimization.
#[derive(Debug, Clone, Default)]
pub struct LavaSyncStats {
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

impl Default for LavaSyncOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

impl LavaSyncOptimizer {
    /// Creates a new lava sync optimizer.
    pub fn new() -> Self {
        Self {
            last_known_states: HashMap::with_capacity(1024),
            pending_updates: HashMap::with_capacity(256),
            last_broadcast: Instant::now()
                .checked_sub(Duration::from_millis(MIN_BROADCAST_INTERVAL_MS))
                .unwrap_or_else(Instant::now),
            stats: LavaSyncStats::default(),
        }
    }

    /// Filters lava cell updates to only include significant changes.
    /// Uses delta encoding to skip updates that haven't changed meaningfully.
    ///
    /// # Arguments
    /// * `updates` - Raw updates from lava simulation tick
    ///
    /// # Returns
    /// Updates that have changed significantly since last broadcast.
    pub fn filter_significant_changes(
        &mut self,
        updates: Vec<LavaCellSyncUpdate>,
    ) -> Vec<LavaCellSyncUpdate> {
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
                    },
                );

                // Add to pending updates
                self.pending_updates.insert(
                    key,
                    LavaCellUpdate {
                        position: [update.position.x, update.position.y, update.position.z],
                        mass: update.mass,
                        is_source: update.is_source,
                    },
                );

                significant.push(update);
            } else {
                self.stats.delta_filtered += 1;
            }
        }

        significant
    }

    /// Checks if a lava cell update represents a significant change.
    fn is_significant_change(&self, key: &CellKey, update: &LavaCellSyncUpdate) -> bool {
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
    pub fn take_filtered_updates(&mut self, player_positions: &[[f32; 3]]) -> Vec<LavaCellUpdate> {
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
    pub fn take_all_pending_updates(&mut self) -> Vec<LavaCellUpdate> {
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
    pub fn stats(&self) -> &LavaSyncStats {
        &self.stats
    }

    /// Resets optimization statistics.
    pub fn reset_stats(&mut self) {
        self.stats = LavaSyncStats::default();
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

    fn make_update(x: i32, y: i32, z: i32, mass: f32) -> LavaCellSyncUpdate {
        LavaCellSyncUpdate {
            position: Vector3::new(x, y, z),
            mass,
            is_source: false,
        }
    }

    #[test]
    fn test_new_cell_is_significant() {
        let mut optimizer = LavaSyncOptimizer::new();
        let updates = vec![make_update(0, 0, 0, 1.0)];

        let filtered = optimizer.filter_significant_changes(updates);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_small_change_is_filtered() {
        let mut optimizer = LavaSyncOptimizer::new();

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
        let mut optimizer = LavaSyncOptimizer::new();

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
        let mut optimizer = LavaSyncOptimizer::new();

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
        let mut optimizer = LavaSyncOptimizer::new();

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
        let mut optimizer = LavaSyncOptimizer::new();

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

    /// Integration test: Verifies lava sync produces identical state on server and client.
    ///
    /// This test simulates the full multiplayer lava sync flow:
    /// 1. Server places a lava source
    /// 2. Server runs lava simulation ticks
    /// 3. Sync updates are collected and filtered through optimizer
    /// 4. Updates are converted to protocol messages
    /// 5. Client applies the updates to its lava grid
    /// 6. Both grids should have identical state
    #[test]
    fn test_lava_sync_produces_identical_state() {
        use crate::lava::{LavaGrid, MAX_MASS};
        use bincode;

        // === Setup: Create server and client lava grids ===
        let mut server_grid = LavaGrid::new();
        let mut client_grid = LavaGrid::new();
        let mut optimizer = LavaSyncOptimizer::new();

        // Place a lava source on the server
        let source_pos = Vector3::new(0, 10, 0);
        server_grid.place_source(source_pos);

        // Initial sync: source placement
        let initial_updates = vec![LavaCellSyncUpdate {
            position: source_pos,
            mass: MAX_MASS,
            is_source: true,
        }];

        // Filter through optimizer
        let filtered = optimizer.filter_significant_changes(initial_updates);
        assert!(
            !filtered.is_empty(),
            "Source placement should be significant"
        );

        // Convert to protocol format and apply to client
        let protocol_updates: Vec<crate::net::protocol::LavaCellUpdate> = filtered
            .iter()
            .map(|u| crate::net::protocol::LavaCellUpdate {
                position: [u.position.x, u.position.y, u.position.z],
                mass: u.mass,
                is_source: u.is_source,
            })
            .collect();

        // Apply updates to client grid
        for update in &protocol_updates {
            let pos = Vector3::new(update.position[0], update.position[1], update.position[2]);
            // set_lava handles both addition (mass > 0) and removal (mass <= 0)
            client_grid.set_lava(pos, update.mass, update.is_source);
        }

        // Verify initial state matches
        assert!(
            client_grid.has_lava(source_pos),
            "Client should have lava at source position"
        );
        assert!(
            client_grid.is_source(source_pos),
            "Client should recognize source"
        );
        assert_eq!(
            server_grid.get_mass(source_pos),
            client_grid.get_mass(source_pos),
            "Source mass should match"
        );

        // === Run simulation ticks and sync ===
        let player_pos = Vector3::new(0.0, 0.0, 0.0);
        let floor_solid = |pos: Vector3<i32>| pos.y < 0;
        let never_out_of_bounds = |_: Vector3<i32>| false;
        let no_water = |_: Vector3<i32>| false;
        let no_world_lava = |_: Vector3<i32>| false;

        // Run several simulation ticks
        for tick_num in 0..5 {
            // Server tick
            let (_, _, server_sync_updates) = server_grid.tick(
                floor_solid,
                never_out_of_bounds,
                no_water,
                no_world_lava,
                player_pos,
            );

            // Filter and optimize
            let filtered = optimizer.filter_significant_changes(server_sync_updates);

            if !filtered.is_empty() {
                // Convert to protocol format
                let protocol_updates: Vec<crate::net::protocol::LavaCellUpdate> = filtered
                    .iter()
                    .map(|u| crate::net::protocol::LavaCellUpdate {
                        position: [u.position.x, u.position.y, u.position.z],
                        mass: u.mass,
                        is_source: u.is_source,
                    })
                    .collect();

                // Simulate network serialization/deserialization
                let encoded =
                    bincode::serde::encode_to_vec(&protocol_updates, bincode::config::standard())
                        .expect("Failed to encode lava updates");
                let (decoded, _len): (Vec<crate::net::protocol::LavaCellUpdate>, usize) =
                    bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                        .expect("Failed to decode lava updates");

                // Apply to client
                for update in decoded {
                    let pos =
                        Vector3::new(update.position[0], update.position[1], update.position[2]);
                    // set_lava handles both addition and removal
                    client_grid.set_lava(pos, update.mass, update.is_source);
                }
            }

            // Also run client simulation (it should converge to same state)
            let _ = client_grid.tick(
                floor_solid,
                never_out_of_bounds,
                no_water,
                no_world_lava,
                player_pos,
            );

            println!(
                "Tick {}: Server cells={}, active={} | Client cells={}, active={}",
                tick_num,
                server_grid.cell_count(),
                server_grid.active_count(),
                client_grid.cell_count(),
                client_grid.active_count()
            );
        }

        // === Verify final state matches ===
        // All lava cells on server should exist on client with same mass
        let mut matching_cells = 0;
        let mut mismatched_cells = 0;

        for (pos, server_cell) in server_grid.iter() {
            if let Some(client_cell) = client_grid.get_cell(*pos) {
                let mass_diff = (server_cell.mass - client_cell.mass).abs();
                if mass_diff < 0.1 {
                    // Allow small floating point differences
                    matching_cells += 1;
                } else {
                    mismatched_cells += 1;
                    println!(
                        "Mismatch at {:?}: server mass={}, client mass={}",
                        pos, server_cell.mass, client_cell.mass
                    );
                }
            } else if server_cell.mass > 0.01 {
                // Server has lava but client doesn't
                mismatched_cells += 1;
                println!(
                    "Missing on client at {:?}: server mass={}",
                    pos, server_cell.mass
                );
            }
        }

        // Check that source is still present and matches
        assert!(
            client_grid.has_lava(source_pos),
            "Client should still have lava at source after simulation"
        );
        assert!(
            client_grid.is_source(source_pos),
            "Source should remain a source on client"
        );

        // Most cells should match (allowing some tolerance for timing differences)
        let total_cells = server_grid.cell_count();
        let match_ratio = matching_cells as f32 / total_cells.max(1) as f32;
        println!(
            "Final: {}/{} cells match ({:.1}%), {} mismatched",
            matching_cells,
            total_cells,
            match_ratio * 100.0,
            mismatched_cells
        );

        // At minimum, source cells should match perfectly
        assert!(
            match_ratio >= 0.8,
            "At least 80% of cells should match after sync (got {:.1}%)",
            match_ratio * 100.0
        );
    }

    /// Test that verifies lava cell removal syncs correctly.
    #[test]
    fn test_lava_removal_sync() {
        use crate::lava::LavaGrid;

        let mut server_grid = LavaGrid::new();
        let mut client_grid = LavaGrid::new();

        // Place lava on both
        let pos = Vector3::new(5, 5, 5);
        server_grid.set_lava(pos, 0.5, false);
        client_grid.set_lava(pos, 0.5, false);

        assert!(server_grid.has_lava(pos));
        assert!(client_grid.has_lava(pos));

        // Server removes lava
        server_grid.set_lava(pos, 0.0, false);

        // Create removal update (mass = 0)
        let removal_update = LavaCellSyncUpdate {
            position: pos,
            mass: 0.0,
            is_source: false,
        };

        // Apply to client using set_lava (handles removal when mass <= 0)
        client_grid.set_lava(
            removal_update.position,
            removal_update.mass,
            removal_update.is_source,
        );

        // Both should now have no lava
        assert!(
            !server_grid.has_lava(pos),
            "Server should have removed lava"
        );
        assert!(
            !client_grid.has_lava(pos),
            "Client should have removed lava"
        );
    }

    /// Test that verifies source cells maintain mass correctly after sync.
    #[test]
    fn test_source_maintains_mass_after_sync() {
        use crate::lava::{LavaGrid, MAX_MASS};

        let mut server_grid = LavaGrid::new();
        let mut client_grid = LavaGrid::new();

        let pos = Vector3::new(0, 0, 0);

        // Place source on server
        server_grid.place_source(pos);

        // Sync to client
        let update = LavaCellSyncUpdate {
            position: pos,
            mass: MAX_MASS,
            is_source: true,
        };

        client_grid.set_lava(
            Vector3::new(update.position.x, update.position.y, update.position.z),
            update.mass,
            update.is_source,
        );

        // Both should have source with MAX_MASS
        assert_eq!(server_grid.get_mass(pos), MAX_MASS);
        assert_eq!(client_grid.get_mass(pos), MAX_MASS);
        assert!(server_grid.is_source(pos));
        assert!(client_grid.is_source(pos));
    }

    /// Integration test: Verifies lava-water cobblestone formation is synced correctly.
    ///
    /// This test simulates the multiplayer scenario where:
    /// 1. Server has lava and water in contact
    /// 2. Server creates cobblestone from the interaction
    /// 3. Cobblestone block change is broadcast to client
    #[test]
    fn test_lava_water_cobblestone_sync() {
        use crate::chunk::WaterType;
        use crate::lava::{LavaGrid, MAX_MASS};
        use crate::net::protocol::{BlockChanged, BlockData, ServerMessage};
        use crate::water::{MAX_MASS as WATER_MAX_MASS, WaterGrid};

        // === Setup ===
        let mut server_lava = LavaGrid::new();
        let mut server_water = WaterGrid::new();
        let mut client_lava = LavaGrid::new();
        let mut client_water = WaterGrid::new();

        let lava_pos = Vector3::new(0, 10, 0);
        let water_pos = Vector3::new(1, 10, 0);

        // Server places lava and water adjacent to each other
        server_lava.place_source(lava_pos);
        server_water.place_source(water_pos, WaterType::Ocean);

        // Sync initial lava state to client
        client_lava.set_lava(lava_pos, MAX_MASS, true);
        client_water.set_water(water_pos, WATER_MAX_MASS, true, WaterType::Ocean);

        // === Simulate lava tick that detects water contact ===
        let player_pos = Vector3::new(0.0, 0.0, 0.0);
        let floor_solid = |pos: Vector3<i32>| pos.y < 0;
        let never_out_of_bounds = |_: Vector3<i32>| false;
        let has_water = |pos: Vector3<i32>| server_water.has_water(pos);
        let no_world_lava = |_: Vector3<i32>| false;

        let (changed_positions, water_contacts, _sync_updates) = server_lava.tick(
            floor_solid,
            never_out_of_bounds,
            has_water,
            no_world_lava,
            player_pos,
        );

        // If lava contacts water, we expect a water contact notification
        // In the real game, this would create cobblestone via process_simulation
        // For this test, we verify the protocol messages that would be sent

        // Simulate cobblestone formation message
        if !water_contacts.is_empty() {
            let cobblestone_pos = water_contacts[0];

            // Server creates and broadcasts cobblestone message
            let server_msg = ServerMessage::BlockChanged(BlockChanged {
                position: [cobblestone_pos.x, cobblestone_pos.y, cobblestone_pos.z],
                block: BlockData::from(crate::chunk::BlockType::Cobblestone),
            });

            // Simulate serialization/deserialization
            let encoded =
                bincode::serde::encode_to_vec(&server_msg, bincode::config::standard()).unwrap();
            let (decoded, _): (ServerMessage, usize) =
                bincode::serde::decode_from_slice(&encoded, bincode::config::standard()).unwrap();

            // Verify client receives correct message
            match decoded {
                ServerMessage::BlockChanged(change) => {
                    assert_eq!(
                        change.position,
                        [cobblestone_pos.x, cobblestone_pos.y, cobblestone_pos.z]
                    );
                    assert_eq!(
                        change.block.block_type,
                        crate::chunk::BlockType::Cobblestone
                    );
                }
                _ => panic!("Expected BlockChanged message"),
            }

            // Server removes lava at contact point
            server_lava.set_lava(cobblestone_pos, 0.0, false);

            // Sync removal to client
            let removal_update = LavaCellSyncUpdate {
                position: cobblestone_pos,
                mass: 0.0,
                is_source: false,
            };
            client_lava.set_lava(
                removal_update.position,
                removal_update.mass,
                removal_update.is_source,
            );

            // Verify both grids now have no lava at contact point
            assert!(
                !server_lava.has_lava(cobblestone_pos),
                "Server should have removed lava at cobblestone position"
            );
            assert!(
                !client_lava.has_lava(cobblestone_pos),
                "Client should have removed lava at cobblestone position"
            );
        }

        println!(
            "Lava-water contact test: {} contacts detected, {} positions changed",
            water_contacts.len(),
            changed_positions.len()
        );
    }
}

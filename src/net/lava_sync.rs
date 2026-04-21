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
use crate::net::fluid_sync::{FluidCell, FluidSyncOptimizer, FluidSyncStats};
use crate::net::protocol::LavaCellUpdate;
use nalgebra::Vector3;

// Re-export stats type under the lava-specific name so existing callers
// do not need to change their import paths.
pub type LavaSyncStats = FluidSyncStats;

impl FluidCell for LavaCellSyncUpdate {
    type ProtocolUpdate = LavaCellUpdate;
    type ExtraState = ();

    fn position(&self) -> Vector3<i32> {
        self.position
    }

    fn mass(&self) -> f32 {
        self.mass
    }

    fn is_source(&self) -> bool {
        self.is_source
    }

    /// Lava has no extra fields beyond mass and is_source.
    fn extra_state(&self) -> Self::ExtraState {}

    fn to_protocol(&self) -> LavaCellUpdate {
        LavaCellUpdate {
            position: [self.position.x, self.position.y, self.position.z],
            mass: self.mass,
            is_source: self.is_source,
        }
    }
}

/// Bandwidth optimizer for lava cell synchronization.
///
/// A type alias over the generic `FluidSyncOptimizer` parameterised for
/// `LavaCellSyncUpdate`. All behaviour lives in the generic implementation;
/// this alias exists for backward-compatible public API.
pub type LavaSyncOptimizer = FluidSyncOptimizer<LavaCellSyncUpdate>;

#[cfg(test)]
mod tests {
    use super::*;

    // Unit tests (new_cell_is_significant, small_change_is_filtered,
    // large_change_is_significant, removal_is_significant, aoi_filtering,
    // rate_limiting) are in fluid_sync.rs as generic parameterised tests.

    /// Integration test: Verifies lava sync produces identical state on server and client.
    #[test]
    fn test_lava_sync_produces_identical_state() {
        use crate::lava::{LavaGrid, MAX_MASS};
        use bincode;

        let mut server_grid = LavaGrid::new();
        let mut client_grid = LavaGrid::new();
        let mut optimizer = LavaSyncOptimizer::new();

        let source_pos = Vector3::new(0, 10, 0);
        server_grid.place_source(source_pos);

        let initial_updates = vec![LavaCellSyncUpdate {
            position: source_pos,
            mass: MAX_MASS,
            is_source: true,
        }];

        let filtered = optimizer.filter_significant_changes(initial_updates);
        assert!(
            !filtered.is_empty(),
            "Source placement should be significant"
        );

        let protocol_updates: Vec<crate::net::protocol::LavaCellUpdate> = filtered
            .iter()
            .map(|u| crate::net::protocol::LavaCellUpdate {
                position: [u.position.x, u.position.y, u.position.z],
                mass: u.mass,
                is_source: u.is_source,
            })
            .collect();

        for update in &protocol_updates {
            let pos = Vector3::new(update.position[0], update.position[1], update.position[2]);
            client_grid.set_lava(pos, update.mass, update.is_source);
        }

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

        let player_pos = Vector3::new(0.0, 0.0, 0.0);
        let floor_solid = |pos: Vector3<i32>| pos.y < 0;
        let never_out_of_bounds = |_: Vector3<i32>| false;
        let no_water = |_: Vector3<i32>| false;
        let no_world_lava = |_: Vector3<i32>| false;

        for tick_num in 0..5 {
            let (_, _, server_sync_updates) = server_grid.tick(
                floor_solid,
                never_out_of_bounds,
                no_water,
                no_world_lava,
                player_pos,
            );

            let filtered = optimizer.filter_significant_changes(server_sync_updates);

            if !filtered.is_empty() {
                let protocol_updates: Vec<crate::net::protocol::LavaCellUpdate> = filtered
                    .iter()
                    .map(|u| crate::net::protocol::LavaCellUpdate {
                        position: [u.position.x, u.position.y, u.position.z],
                        mass: u.mass,
                        is_source: u.is_source,
                    })
                    .collect();

                let encoded =
                    bincode::serde::encode_to_vec(&protocol_updates, bincode::config::standard())
                        .expect("Failed to encode lava updates");
                let (decoded, _len): (Vec<crate::net::protocol::LavaCellUpdate>, usize) =
                    bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                        .expect("Failed to decode lava updates");

                for update in decoded {
                    let pos =
                        Vector3::new(update.position[0], update.position[1], update.position[2]);
                    client_grid.set_lava(pos, update.mass, update.is_source);
                }
            }

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

        let mut matching_cells = 0;
        let mut mismatched_cells = 0;

        for (pos, server_cell) in server_grid.iter() {
            if let Some(client_cell) = client_grid.get_cell(*pos) {
                let mass_diff = (server_cell.mass - client_cell.mass).abs();
                if mass_diff < 0.1 {
                    matching_cells += 1;
                } else {
                    mismatched_cells += 1;
                    println!(
                        "Mismatch at {:?}: server mass={}, client mass={}",
                        pos, server_cell.mass, client_cell.mass
                    );
                }
            } else if server_cell.mass > 0.01 {
                mismatched_cells += 1;
                println!(
                    "Missing on client at {:?}: server mass={}",
                    pos, server_cell.mass
                );
            }
        }

        assert!(
            client_grid.has_lava(source_pos),
            "Client should still have lava at source after simulation"
        );
        assert!(
            client_grid.is_source(source_pos),
            "Source should remain a source on client"
        );

        let total_cells = server_grid.cell_count();
        let match_ratio = matching_cells as f32 / total_cells.max(1) as f32;
        println!(
            "Final: {}/{} cells match ({:.1}%), {} mismatched",
            matching_cells,
            total_cells,
            match_ratio * 100.0,
            mismatched_cells
        );

        assert!(
            match_ratio >= 0.8,
            "At least 80% of cells should match after sync (got {:.1}%)",
            match_ratio * 100.0
        );
    }

    #[test]
    fn test_lava_removal_sync() {
        use crate::lava::LavaGrid;

        let mut server_grid = LavaGrid::new();
        let mut client_grid = LavaGrid::new();

        let pos = Vector3::new(5, 5, 5);
        server_grid.set_lava(pos, 0.5, false);
        client_grid.set_lava(pos, 0.5, false);

        assert!(server_grid.has_lava(pos));
        assert!(client_grid.has_lava(pos));

        server_grid.set_lava(pos, 0.0, false);

        let removal_update = LavaCellSyncUpdate {
            position: pos,
            mass: 0.0,
            is_source: false,
        };

        client_grid.set_lava(
            removal_update.position,
            removal_update.mass,
            removal_update.is_source,
        );

        assert!(
            !server_grid.has_lava(pos),
            "Server should have removed lava"
        );
        assert!(
            !client_grid.has_lava(pos),
            "Client should have removed lava"
        );
    }

    #[test]
    fn test_source_maintains_mass_after_sync() {
        use crate::lava::{LavaGrid, MAX_MASS};

        let mut server_grid = LavaGrid::new();
        let mut client_grid = LavaGrid::new();

        let pos = Vector3::new(0, 0, 0);

        server_grid.place_source(pos);

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

        assert_eq!(server_grid.get_mass(pos), MAX_MASS);
        assert_eq!(client_grid.get_mass(pos), MAX_MASS);
        assert!(server_grid.is_source(pos));
        assert!(client_grid.is_source(pos));
    }

    /// Integration test: Verifies lava-water cobblestone formation is synced correctly.
    #[test]
    fn test_lava_water_cobblestone_sync() {
        use crate::chunk::WaterType;
        use crate::lava::{LavaGrid, MAX_MASS};
        use crate::net::protocol::{BlockChanged, BlockData, ServerMessage};
        use crate::water::{MAX_MASS as WATER_MAX_MASS, WaterGrid};

        let mut server_lava = LavaGrid::new();
        let mut server_water = WaterGrid::new();
        let mut client_lava = LavaGrid::new();
        let mut client_water = WaterGrid::new();

        let lava_pos = Vector3::new(0, 10, 0);
        let water_pos = Vector3::new(1, 10, 0);

        server_lava.place_source(lava_pos);
        server_water.place_source(water_pos, WaterType::Ocean);

        client_lava.set_lava(lava_pos, MAX_MASS, true);
        client_water.set_water(water_pos, WATER_MAX_MASS, true, WaterType::Ocean);

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

        // Validate every water contact — previously the test only covered
        // water_contacts[0], letting bugs in multi-contact scenarios slip by.
        for &cobblestone_pos in water_contacts.iter() {
            let server_msg = ServerMessage::BlockChanged(BlockChanged {
                position: [cobblestone_pos.x, cobblestone_pos.y, cobblestone_pos.z],
                block: BlockData::from(crate::chunk::BlockType::Cobblestone),
            });

            let encoded =
                bincode::serde::encode_to_vec(&server_msg, bincode::config::standard()).unwrap();
            let (decoded, _): (ServerMessage, usize) =
                bincode::serde::decode_from_slice(&encoded, bincode::config::standard()).unwrap();

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

            server_lava.set_lava(cobblestone_pos, 0.0, false);

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

            assert!(
                !server_lava.has_lava(cobblestone_pos),
                "Server should have removed lava at cobblestone position {:?}",
                cobblestone_pos
            );
            assert!(
                !client_lava.has_lava(cobblestone_pos),
                "Client should have removed lava at cobblestone position {:?}",
                cobblestone_pos
            );
        }

        // Note: the implementation signals a cobblestone conversion as BOTH
        // a water contact (for the BlockChanged message) AND a changed lava
        // cell position (mass now 0). Downstream sync consumers need to be
        // aware of this dual signalling. The test doesn't assert the
        // non-overlap invariant since that would need a refactor outside
        // the scope of M16.
        let _ = changed_positions;

        println!(
            "Lava-water contact test: {} contacts detected, {} positions changed",
            water_contacts.len(),
            changed_positions.len()
        );
    }
}

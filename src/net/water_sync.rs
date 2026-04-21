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
use crate::net::fluid_sync::{FluidCell, FluidSyncOptimizer, FluidSyncStats};
use crate::net::protocol::WaterCellUpdate;
use crate::water::WaterCellSyncUpdate;
use nalgebra::Vector3;

// Re-export stats type under the water-specific name so existing callers
// do not need to change their import paths.
pub type WaterSyncStats = FluidSyncStats;

impl FluidCell for WaterCellSyncUpdate {
    type ProtocolUpdate = WaterCellUpdate;
    type ExtraState = WaterType;

    fn position(&self) -> Vector3<i32> {
        self.position
    }

    fn mass(&self) -> f32 {
        self.mass
    }

    fn is_source(&self) -> bool {
        self.is_source
    }

    fn extra_state(&self) -> Self::ExtraState {
        self.water_type
    }

    fn to_protocol(&self) -> WaterCellUpdate {
        WaterCellUpdate {
            position: [self.position.x, self.position.y, self.position.z],
            mass: self.mass,
            is_source: self.is_source,
            water_type: self.water_type,
        }
    }
}

/// Bandwidth optimizer for water cell synchronization.
///
/// A type alias over the generic `FluidSyncOptimizer` parameterised for
/// `WaterCellSyncUpdate`. All behaviour lives in the generic implementation;
/// this alias exists for backward-compatible public API.
pub type WaterSyncOptimizer = FluidSyncOptimizer<WaterCellSyncUpdate>;

#[cfg(test)]
mod tests {
    use super::*;

    // Unit tests (new_cell_is_significant, small_change_is_filtered,
    // large_change_is_significant, removal_is_significant, aoi_filtering,
    // rate_limiting) are in fluid_sync.rs as generic parameterised tests.

    /// Integration test: Verifies water sync produces identical state on server and client.
    #[test]
    fn test_water_sync_produces_identical_state() {
        use crate::water::{MAX_MASS, WaterGrid};
        use bincode;

        let mut server_grid = WaterGrid::new();
        let mut client_grid = WaterGrid::new();
        let mut optimizer = WaterSyncOptimizer::new();

        let source_pos = Vector3::new(0, 10, 0);
        server_grid.place_source(source_pos, WaterType::Ocean);

        let initial_updates = vec![WaterCellSyncUpdate {
            position: source_pos,
            mass: MAX_MASS,
            is_source: true,
            water_type: WaterType::Ocean,
        }];

        let filtered = optimizer.filter_significant_changes(initial_updates);
        assert!(
            !filtered.is_empty(),
            "Source placement should be significant"
        );

        let protocol_updates: Vec<crate::net::protocol::WaterCellUpdate> = filtered
            .iter()
            .map(|u| crate::net::protocol::WaterCellUpdate {
                position: [u.position.x, u.position.y, u.position.z],
                mass: u.mass,
                is_source: u.is_source,
                water_type: u.water_type,
            })
            .collect();

        for update in &protocol_updates {
            let pos = Vector3::new(update.position[0], update.position[1], update.position[2]);
            client_grid.set_water(pos, update.mass, update.is_source, update.water_type);
        }

        assert!(
            client_grid.has_water(source_pos),
            "Client should have water at source position"
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
        let no_world_water = |_: Vector3<i32>| false;

        for tick_num in 0..5 {
            let (_, server_sync_updates) =
                server_grid.tick(floor_solid, never_out_of_bounds, no_world_water, player_pos);

            let filtered = optimizer.filter_significant_changes(server_sync_updates);

            if !filtered.is_empty() {
                let protocol_updates: Vec<crate::net::protocol::WaterCellUpdate> = filtered
                    .iter()
                    .map(|u| crate::net::protocol::WaterCellUpdate {
                        position: [u.position.x, u.position.y, u.position.z],
                        mass: u.mass,
                        is_source: u.is_source,
                        water_type: u.water_type,
                    })
                    .collect();

                let encoded =
                    bincode::serde::encode_to_vec(&protocol_updates, bincode::config::standard())
                        .expect("Failed to encode water updates");
                let (decoded, _len): (Vec<crate::net::protocol::WaterCellUpdate>, usize) =
                    bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                        .expect("Failed to decode water updates");

                for update in decoded {
                    let pos =
                        Vector3::new(update.position[0], update.position[1], update.position[2]);
                    client_grid.set_water(pos, update.mass, update.is_source, update.water_type);
                }
            }

            let _ = client_grid.tick(floor_solid, never_out_of_bounds, no_world_water, player_pos);

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
            client_grid.has_water(source_pos),
            "Client should still have water at source after simulation"
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

        // Tightened from 0.8 → 0.95 so the test actually fails if divergence
        // creeps in. 80 % tolerance let persistent-mass-mismatch bugs slip
        // past silently.
        assert!(
            match_ratio >= 0.95,
            "At least 95% of cells should match after sync (got {:.1}%)",
            match_ratio * 100.0
        );
    }

    #[test]
    fn test_water_removal_sync() {
        use crate::water::WaterGrid;

        let mut server_grid = WaterGrid::new();
        let mut client_grid = WaterGrid::new();

        let pos = Vector3::new(5, 5, 5);
        server_grid.set_water(pos, 0.5, false, WaterType::Ocean);
        client_grid.set_water(pos, 0.5, false, WaterType::Ocean);

        assert!(server_grid.has_water(pos));
        assert!(client_grid.has_water(pos));

        server_grid.set_water(pos, 0.0, false, WaterType::Ocean);

        let removal_update = WaterCellSyncUpdate {
            position: pos,
            mass: 0.0,
            is_source: false,
            water_type: WaterType::Ocean,
        };

        client_grid.set_water(
            removal_update.position,
            removal_update.mass,
            removal_update.is_source,
            removal_update.water_type,
        );

        assert!(
            !server_grid.has_water(pos),
            "Server should have removed water"
        );
        assert!(
            !client_grid.has_water(pos),
            "Client should have removed water"
        );
    }

    #[test]
    fn test_source_maintains_mass_after_sync() {
        use crate::water::{MAX_MASS, WaterGrid};

        let mut server_grid = WaterGrid::new();
        let mut client_grid = WaterGrid::new();

        let pos = Vector3::new(0, 0, 0);

        server_grid.place_source(pos, WaterType::Spring);

        let update = WaterCellSyncUpdate {
            position: pos,
            mass: MAX_MASS,
            is_source: true,
            water_type: WaterType::Spring,
        };

        client_grid.set_water(
            Vector3::new(update.position.x, update.position.y, update.position.z),
            update.mass,
            update.is_source,
            update.water_type,
        );

        assert_eq!(server_grid.get_mass(pos), MAX_MASS);
        assert_eq!(client_grid.get_mass(pos), MAX_MASS);
        assert!(server_grid.is_source(pos));
        assert!(client_grid.is_source(pos));

        assert_eq!(
            server_grid.get_cell(pos).unwrap().water_type,
            client_grid.get_cell(pos).unwrap().water_type
        );
    }
}

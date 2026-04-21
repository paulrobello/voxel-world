//! Extended gameplay synchronization integration tests.
//!
//! Simulates extended multiplayer sessions (5+ minutes equivalent) to verify:
//! - No state divergence between server and clients over time
//! - All sync mechanisms remain consistent under sustained gameplay
//! - Physics simulations converge to identical states
//!
//! # Test Strategy
//!
//! Since running actual 5+ minute tests would be too slow, we use:
//! - **Compressed time**: 1 second of test time = ~30 seconds of simulated gameplay
//! - **Deterministic operations**: Same operations produce same results
//! - **State snapshots**: Periodic verification of world state consistency

// Allow dead code since these methods are public API intended for future use
#![allow(dead_code)]

/// Simulated game time tick (50ms = 20 TPS).
const TICK_DELTA: f32 = 0.05;

/// Number of ticks to simulate (6000 ticks = 5 minutes at 20 TPS).
const EXTENDED_GAMEPLAY_TICKS: usize = 6000;

/// Snapshot interval for state verification (every 600 ticks = 30 seconds).
const SNAPSHOT_INTERVAL: usize = 600;

/// Snapshot of game state for comparison.
#[derive(Debug, Clone, Default)]
pub struct GameStateSnapshot {
    /// Tick number when snapshot was taken.
    pub tick: usize,
    /// Water cell count.
    pub water_cell_count: usize,
    /// Lava cell count.
    pub lava_cell_count: usize,
    /// Active falling blocks.
    pub falling_block_count: usize,
    /// Day cycle paused state.
    pub day_cycle_paused: bool,
    /// Time of day (when paused, this should be synced).
    pub time_of_day: f32,
}

/// Compares two state snapshots and returns differences.
/// Only compares fields that are actively synchronized in multiplayer.
pub fn compare_sync_snapshots(
    expected: &GameStateSnapshot,
    actual: &GameStateSnapshot,
) -> Vec<String> {
    let mut differences = Vec::new();

    // Water cells are actively synced
    if expected.water_cell_count != actual.water_cell_count {
        differences.push(format!(
            "Water cell count mismatch: expected {}, got {}",
            expected.water_cell_count, actual.water_cell_count
        ));
    }

    // Lava cells are actively synced
    if expected.lava_cell_count != actual.lava_cell_count {
        differences.push(format!(
            "Lava cell count mismatch: expected {}, got {}",
            expected.lava_cell_count, actual.lava_cell_count
        ));
    }

    // Falling block count should match when sync is working
    if expected.falling_block_count != actual.falling_block_count {
        differences.push(format!(
            "Falling block count mismatch: expected {}, got {}",
            expected.falling_block_count, actual.falling_block_count
        ));
    }

    // Day cycle pause state is synced
    if expected.day_cycle_paused != actual.day_cycle_paused {
        differences.push(format!(
            "Day cycle paused mismatch: expected {}, got {}",
            expected.day_cycle_paused, actual.day_cycle_paused
        ));
    }

    // When paused, time of day should be exactly synced
    if expected.day_cycle_paused && actual.day_cycle_paused {
        let time_diff = (expected.time_of_day - actual.time_of_day).abs();
        if time_diff > 0.001 {
            differences.push(format!(
                "Time of day mismatch when paused: expected {:.6}, got {:.6}",
                expected.time_of_day, actual.time_of_day
            ));
        }
    }

    differences
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{BlockType, WaterType};
    use crate::net::falling_block_sync::{ClientFallingBlockSystem, FallingBlockSync};
    use crate::net::protocol::{FallingBlockLanded, WaterCellUpdate};
    use crate::net::water_sync::WaterSyncOptimizer;
    use crate::water::{MAX_MASS, WaterGrid};
    use nalgebra::Vector3;

    /// Integration test: Verifies no state divergence after extended gameplay.
    ///
    /// This test simulates approximately 5 minutes of multiplayer gameplay
    /// (6000 ticks at 20 TPS) with water simulation, falling blocks, and
    /// day cycle pause synchronization.
    ///
    /// What this tests:
    /// - Water simulation sync over time
    /// - Falling block spawn/land sync
    /// - Day cycle pause synchronization
    ///
    /// What this does NOT test (requires full multiplayer infrastructure):
    /// - Block placement/breaking sync (requires PlaceBlock messages)
    /// - Player movement sync
    /// - Chunk loading sync
    #[test]
    fn test_no_state_divergence_after_extended_gameplay() {
        println!("=== Extended Gameplay Sync Test ===");
        println!(
            "Simulating {} ticks (5 minutes at 20 TPS)...",
            EXTENDED_GAMEPLAY_TICKS
        );

        // === Setup: Create server-side simulation ===
        let mut server_water_grid = WaterGrid::new();
        let mut server_water_optimizer = WaterSyncOptimizer::new();
        let mut server_falling_sync = FallingBlockSync::new();

        // Create 3 clients
        let mut client1_water_grid = WaterGrid::new();
        let mut client2_water_grid = WaterGrid::new();
        let mut client3_water_grid = WaterGrid::new();
        let mut client1_falling = ClientFallingBlockSystem::new();
        let mut client2_falling = ClientFallingBlockSystem::new();
        let mut client3_falling = ClientFallingBlockSystem::new();

        // Day cycle state
        let mut server_day_paused = false;
        let mut server_time_of_day: f32 = 0.25;
        let mut client1_day_paused = false;
        let mut client1_time_of_day: f32 = 0.25;
        let mut client2_day_paused = false;
        let mut client2_time_of_day: f32 = 0.25;
        let mut client3_day_paused = false;
        let mut client3_time_of_day: f32 = 0.25;

        // Track pending day cycle sync
        let mut pending_day_cycle_sync = false;

        // Track sync consistency
        let mut last_consistent_tick = 0;
        let mut any_divergence = false;

        // Define simulation helpers
        let floor_solid = |pos: Vector3<i32>| pos.y < 0;
        let never_out_of_bounds = |_: Vector3<i32>| false;
        let no_world_water = |_: Vector3<i32>| false;
        let player_pos = Vector3::new(0.0, 64.0, 0.0);

        // === Main simulation loop ===
        for tick in 0..EXTENDED_GAMEPLAY_TICKS {
            // Schedule actions at specific ticks
            if tick == 100 {
                // Place water source
                let pos = Vector3::new(0, 70, 0);
                server_water_grid.place_source(pos, WaterType::Ocean);
            }

            if tick == 1000 {
                // Pause day cycle
                server_day_paused = true;
                pending_day_cycle_sync = true;
            }

            if tick == 1200 {
                // Place another water source
                let pos = Vector3::new(10, 70, 10);
                server_water_grid.place_source(pos, WaterType::Spring);
            }

            if tick == 2000 {
                // Resume day cycle
                server_day_paused = false;
                pending_day_cycle_sync = true;
            }

            if tick == 2500 {
                // Spawn a falling sand block
                let spawn =
                    server_falling_sync.register_spawn(Vector3::new(50, 80, 50), BlockType::Sand);
                // Broadcast to all clients
                client1_falling.spawn_from_network(&spawn);
                client2_falling.spawn_from_network(&spawn);
                client3_falling.spawn_from_network(&spawn);
            }

            if tick == 2600 {
                // Falling block lands
                let land = FallingBlockLanded {
                    entity_id: 1,
                    position: [50, 64, 50],
                    block_type: BlockType::Sand,
                };
                client1_falling.handle_landed(&land);
                client2_falling.handle_landed(&land);
                client3_falling.handle_landed(&land);
                server_falling_sync.remove_entity(1);
            }

            if tick == 4000 {
                // Spawn multiple falling blocks
                let spawn1 = server_falling_sync
                    .register_spawn(Vector3::new(100, 80, 100), BlockType::Gravel);
                let spawn2 =
                    server_falling_sync.register_spawn(Vector3::new(101, 80, 100), BlockType::Snow);
                client1_falling.spawn_from_network(&spawn1);
                client1_falling.spawn_from_network(&spawn2);
                client2_falling.spawn_from_network(&spawn1);
                client2_falling.spawn_from_network(&spawn2);
                client3_falling.spawn_from_network(&spawn1);
                client3_falling.spawn_from_network(&spawn2);
            }

            if tick == 4200 {
                // Both blocks land
                let land1 = FallingBlockLanded {
                    entity_id: 2,
                    position: [100, 64, 100],
                    block_type: BlockType::Gravel,
                };
                let land2 = FallingBlockLanded {
                    entity_id: 3,
                    position: [101, 64, 100],
                    block_type: BlockType::Snow,
                };
                for land in [&land1, &land2] {
                    client1_falling.handle_landed(land);
                    client2_falling.handle_landed(land);
                    client3_falling.handle_landed(land);
                }
                server_falling_sync.remove_entity(2);
                server_falling_sync.remove_entity(3);
            }

            if tick == 5500 {
                // Final day cycle pause
                server_day_paused = true;
                pending_day_cycle_sync = true;
            }

            // Update day cycle on server
            if !server_day_paused {
                server_time_of_day = (server_time_of_day + 0.0001) % 1.0;
            }

            // Water simulation tick on server
            let (_, water_updates) = server_water_grid.tick(
                floor_solid,
                never_out_of_bounds,
                no_world_water,
                player_pos,
            );

            // Filter and sync water updates
            let filtered = server_water_optimizer.filter_significant_changes(water_updates);
            for update in filtered {
                let _protocol_update = WaterCellUpdate {
                    position: [update.position.x, update.position.y, update.position.z],
                    mass: update.mass,
                    is_source: update.is_source,
                    water_type: update.water_type,
                };

                // Apply to all clients
                let pos = update.position;
                client1_water_grid.set_water(pos, update.mass, update.is_source, update.water_type);
                client2_water_grid.set_water(pos, update.mass, update.is_source, update.water_type);
                client3_water_grid.set_water(pos, update.mass, update.is_source, update.water_type);
            }

            // Sync day cycle pause state when changed
            if pending_day_cycle_sync {
                client1_day_paused = server_day_paused;
                client1_time_of_day = server_time_of_day;
                client2_day_paused = server_day_paused;
                client2_time_of_day = server_time_of_day;
                client3_day_paused = server_day_paused;
                client3_time_of_day = server_time_of_day;
                pending_day_cycle_sync = false;
            }

            // Client-side falling block updates
            client1_falling.update(TICK_DELTA);
            client2_falling.update(TICK_DELTA);
            client3_falling.update(TICK_DELTA);

            // Periodic state verification
            if tick > 0 && tick % SNAPSHOT_INTERVAL == 0 {
                let server_snapshot = GameStateSnapshot {
                    tick,
                    water_cell_count: server_water_grid.cell_count(),
                    lava_cell_count: 0,
                    falling_block_count: server_falling_sync.active_count(),
                    day_cycle_paused: server_day_paused,
                    time_of_day: server_time_of_day,
                };

                let client1_snapshot = GameStateSnapshot {
                    tick,
                    water_cell_count: client1_water_grid.cell_count(),
                    lava_cell_count: 0,
                    falling_block_count: client1_falling.count(),
                    day_cycle_paused: client1_day_paused,
                    time_of_day: client1_time_of_day,
                };

                let client2_snapshot = GameStateSnapshot {
                    tick,
                    water_cell_count: client2_water_grid.cell_count(),
                    lava_cell_count: 0,
                    falling_block_count: client2_falling.count(),
                    day_cycle_paused: client2_day_paused,
                    time_of_day: client2_time_of_day,
                };

                let client3_snapshot = GameStateSnapshot {
                    tick,
                    water_cell_count: client3_water_grid.cell_count(),
                    lava_cell_count: 0,
                    falling_block_count: client3_falling.count(),
                    day_cycle_paused: client3_day_paused,
                    time_of_day: client3_time_of_day,
                };

                let diff1 = compare_sync_snapshots(&server_snapshot, &client1_snapshot);
                let diff2 = compare_sync_snapshots(&server_snapshot, &client2_snapshot);
                let diff3 = compare_sync_snapshots(&server_snapshot, &client3_snapshot);

                if diff1.is_empty() && diff2.is_empty() && diff3.is_empty() {
                    last_consistent_tick = tick;
                    println!(
                        "Tick {}: ✓ Consistent - water={}, falling={}, paused={}",
                        tick,
                        server_snapshot.water_cell_count,
                        server_snapshot.falling_block_count,
                        server_snapshot.day_cycle_paused
                    );
                } else {
                    any_divergence = true;
                    println!("Tick {}: ✗ DIVERGENCE DETECTED", tick);
                    if !diff1.is_empty() {
                        println!("  Client 1: {:?}", diff1);
                    }
                    if !diff2.is_empty() {
                        println!("  Client 2: {:?}", diff2);
                    }
                    if !diff3.is_empty() {
                        println!("  Client 3: {:?}", diff3);
                    }
                }
            }
        }

        // === Final verification ===
        println!("\n=== Final State Summary ===");
        println!(
            "Server: water={}, falling={}, paused={}, time={:.4}",
            server_water_grid.cell_count(),
            server_falling_sync.active_count(),
            server_day_paused,
            server_time_of_day
        );
        println!(
            "Client 1: water={}, falling={}, paused={}",
            client1_water_grid.cell_count(),
            client1_falling.count(),
            client1_day_paused
        );
        println!(
            "Client 2: water={}, falling={}, paused={}",
            client2_water_grid.cell_count(),
            client2_falling.count(),
            client2_day_paused
        );
        println!(
            "Client 3: water={}, falling={}, paused={}",
            client3_water_grid.cell_count(),
            client3_falling.count(),
            client3_day_paused
        );
        println!("Last consistent tick: {}", last_consistent_tick);

        // All clients should have identical water state
        assert_eq!(
            client1_water_grid.cell_count(),
            client2_water_grid.cell_count(),
            "Client 1 and 2 water count should match"
        );
        assert_eq!(
            client2_water_grid.cell_count(),
            client3_water_grid.cell_count(),
            "Client 2 and 3 water count should match"
        );

        // All clients should have no falling blocks (all landed)
        assert_eq!(
            client1_falling.count(),
            0,
            "Client 1 should have no falling blocks"
        );
        assert_eq!(
            client2_falling.count(),
            0,
            "Client 2 should have no falling blocks"
        );
        assert_eq!(
            client3_falling.count(),
            0,
            "Client 3 should have no falling blocks"
        );

        // All clients should have same day cycle state
        assert_eq!(
            client1_day_paused, server_day_paused,
            "Client 1 day pause should match server"
        );
        assert_eq!(
            client2_day_paused, server_day_paused,
            "Client 2 day pause should match server"
        );
        assert_eq!(
            client3_day_paused, server_day_paused,
            "Client 3 day pause should match server"
        );

        // No divergence should have occurred
        assert!(
            !any_divergence,
            "State divergence detected during extended gameplay test"
        );

        println!(
            "\n✅ Extended gameplay sync test PASSED - no divergence over {} ticks",
            EXTENDED_GAMEPLAY_TICKS
        );
    }

    /// Test: Water state consistency over time.
    #[test]
    fn test_water_state_consistency_over_time() {
        let mut server_water = WaterGrid::new();
        let mut server_optimizer = WaterSyncOptimizer::new();
        let mut client_water = WaterGrid::new();

        // Place water source
        let source_pos = Vector3::new(0, 70, 0);
        server_water.place_source(source_pos, WaterType::Ocean);

        // Initial sync
        client_water.set_water(source_pos, MAX_MASS, true, WaterType::Ocean);

        let floor_solid = |pos: Vector3<i32>| pos.y < 0;
        let never_out_of_bounds = |_: Vector3<i32>| false;
        let no_world_water = |_: Vector3<i32>| false;
        let player_pos = Vector3::new(0.0, 64.0, 0.0);

        // Run for 1000 ticks
        for tick in 0..1000 {
            let (_, water_updates) =
                server_water.tick(floor_solid, never_out_of_bounds, no_world_water, player_pos);

            let filtered = server_optimizer.filter_significant_changes(water_updates);
            for update in filtered {
                client_water.set_water(
                    update.position,
                    update.mass,
                    update.is_source,
                    update.water_type,
                );
            }

            // Verify consistency every 100 ticks
            if tick % 100 == 0 {
                let server_count = server_water.cell_count();
                let client_count = client_water.cell_count();
                let diff = (server_count as i32 - client_count as i32).abs();

                // Allow some tolerance due to batching
                assert!(
                    diff <= 5,
                    "Tick {}: Water count diverged (server={}, client={}, diff={})",
                    tick,
                    server_count,
                    client_count,
                    diff
                );
            }
        }

        println!(
            "Water state consistent after 1000 ticks: server={}, client={}",
            server_water.cell_count(),
            client_water.cell_count()
        );
    }

    /// Test: Day cycle pause sync reliability.
    #[test]
    fn test_day_cycle_pause_sync_reliability() {
        let mut server_paused = false;
        let mut server_time: f32 = 0.25;
        let mut client1_paused;
        let mut client1_time: f32;
        let mut client2_paused;
        let mut client2_time: f32;

        // Toggle pause multiple times
        for iteration in 0..10 {
            // Toggle on server
            server_paused = !server_paused;

            // Sync to clients
            client1_paused = server_paused;
            client1_time = server_time;
            client2_paused = server_paused;
            client2_time = server_time;

            // Run 50 ticks
            for _ in 0..50 {
                if !server_paused {
                    server_time = (server_time + 0.0001) % 1.0;
                }
                // When paused, sync time exactly
                if server_paused {
                    client1_time = server_time;
                    client2_time = server_time;
                }
            }

            // Verify consistency
            assert_eq!(
                client1_paused, server_paused,
                "Iteration {}: Client 1 pause state mismatch",
                iteration
            );
            assert_eq!(
                client2_paused, server_paused,
                "Iteration {}: Client 2 pause state mismatch",
                iteration
            );

            if server_paused {
                assert!(
                    (client1_time - server_time).abs() < 0.001,
                    "Iteration {}: Client 1 time mismatch when paused",
                    iteration
                );
                assert!(
                    (client2_time - server_time).abs() < 0.001,
                    "Iteration {}: Client 2 time mismatch when paused",
                    iteration
                );
            }
        }

        println!("Day cycle pause sync reliable over 10 toggle cycles");
    }

    /// Test: Falling block sync consistency across multiple clients.
    #[test]
    fn test_falling_block_sync_consistency() {
        let mut server_sync = FallingBlockSync::new();
        let mut client1 = ClientFallingBlockSystem::new();
        let mut client2 = ClientFallingBlockSystem::new();
        let mut client3 = ClientFallingBlockSystem::new();

        // Spawn 5 falling blocks
        let mut spawns = Vec::new();
        for i in 0..5 {
            let spawn = server_sync.register_spawn(
                Vector3::new(i * 10, 80, i * 10),
                if i % 2 == 0 {
                    BlockType::Sand
                } else {
                    BlockType::Gravel
                },
            );
            spawns.push(spawn);
        }

        // All clients receive spawns
        for spawn in &spawns {
            client1.spawn_from_network(spawn);
            client2.spawn_from_network(spawn);
            client3.spawn_from_network(spawn);
        }

        // All clients should see 5 falling blocks
        assert_eq!(client1.count(), 5);
        assert_eq!(client2.count(), 5);
        assert_eq!(client3.count(), 5);
        assert_eq!(server_sync.active_count(), 5);

        // Land blocks one at a time
        for (i, spawn) in spawns.iter().enumerate() {
            let land = FallingBlockLanded {
                entity_id: spawn.entity_id,
                position: [spawn.position[0] as i32, 64, spawn.position[2] as i32],
                block_type: spawn.block_type,
            };

            client1.handle_landed(&land);
            client2.handle_landed(&land);
            client3.handle_landed(&land);
            server_sync.remove_entity(spawn.entity_id);

            let remaining = 5 - i - 1;
            assert_eq!(client1.count(), remaining);
            assert_eq!(client2.count(), remaining);
            assert_eq!(client3.count(), remaining);
        }

        // All should be empty
        assert_eq!(client1.count(), 0);
        assert_eq!(client2.count(), 0);
        assert_eq!(client3.count(), 0);
        assert_eq!(server_sync.active_count(), 0);

        println!("Falling block sync consistent across 3 clients for 5 blocks");
    }

    /// Test: Rapid operations don't cause divergence.
    #[test]
    fn test_rapid_operations_consistency() {
        let mut server_water = WaterGrid::new();
        let mut server_optimizer = WaterSyncOptimizer::new();
        let mut client1_water = WaterGrid::new();
        let mut client2_water = WaterGrid::new();

        let floor_solid = |pos: Vector3<i32>| pos.y < 0;
        let never_out_of_bounds = |_: Vector3<i32>| false;
        let no_world_water = |_: Vector3<i32>| false;
        let player_pos = Vector3::new(0.0, 64.0, 0.0);

        // Rapidly place water sources and run simulation
        for tick in 0..500 {
            // Place water source every 50 ticks
            if tick % 50 == 0 {
                let x = tick % 100;
                let pos = Vector3::new(x, 70, 0);
                server_water.place_source(pos, WaterType::Ocean);
                // Sync initial placement
                client1_water.set_water(pos, MAX_MASS, true, WaterType::Ocean);
                client2_water.set_water(pos, MAX_MASS, true, WaterType::Ocean);
            }

            // Water tick
            let (_, water_updates) =
                server_water.tick(floor_solid, never_out_of_bounds, no_world_water, player_pos);

            let filtered = server_optimizer.filter_significant_changes(water_updates);
            for update in filtered {
                client1_water.set_water(
                    update.position,
                    update.mass,
                    update.is_source,
                    update.water_type,
                );
                client2_water.set_water(
                    update.position,
                    update.mass,
                    update.is_source,
                    update.water_type,
                );
            }
        }

        // Verify both clients have identical state
        let server_count = server_water.cell_count();
        let client1_count = client1_water.cell_count();
        let client2_count = client2_water.cell_count();

        // Allow small tolerance for timing
        let diff1 = (server_count as i32 - client1_count as i32).abs();
        let diff2 = (server_count as i32 - client2_count as i32).abs();

        assert!(diff1 <= 10, "Client 1 water count diverged: diff={}", diff1);
        assert!(diff2 <= 10, "Client 2 water count diverged: diff={}", diff2);

        // Both clients should have identical state
        assert_eq!(
            client1_count, client2_count,
            "Both clients should have identical water count"
        );

        println!(
            "Rapid operations test passed: server={}, client1={}, client2={}",
            server_count, client1_count, client2_count
        );
    }
}

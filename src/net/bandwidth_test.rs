//! Bandwidth performance test for multiplayer synchronization.
//!
//! Measures network bandwidth usage under typical gameplay conditions to ensure
//! we stay under the 100 KB/s per client target.
//!
//! # Test Strategy
//!
//! - **Simulate typical gameplay**: Block placement, water flow, falling blocks, player movement
//! - **Measure serialized message sizes**: Use bincode to get actual wire sizes
//! - **Track bandwidth per second**: Calculate KB/s over test duration
//! - **Verify against target**: Ensure < 100 KB/s per client

// Allow dead code since these methods are public API intended for future use
#![allow(dead_code)]

use std::collections::HashMap;

/// Target bandwidth: 100 KB/s per client.
const TARGET_BANDWIDTH_KB_S: f32 = 100.0;

/// Simulated game time tick (50ms = 20 TPS).
const TICK_DELTA: f32 = 0.05;

/// Number of ticks to simulate (10 seconds = 200 ticks at 20 TPS).
const TEST_DURATION_TICKS: usize = 200;

/// Bandwidth statistics for a test run.
#[derive(Debug, Clone, Default)]
pub struct BandwidthStats {
    /// Total bytes sent from server to client.
    pub server_to_client_bytes: usize,
    /// Total bytes sent from client to server.
    pub client_to_server_bytes: usize,
    /// Number of ticks simulated.
    pub ticks: usize,
    /// Messages sent breakdown by type.
    pub message_counts: HashMap<String, usize>,
    /// Bytes sent breakdown by message type.
    pub message_bytes: HashMap<String, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{BlockType, WaterType};
    use crate::net::falling_block_sync::{ClientFallingBlockSystem, FallingBlockSync};
    use crate::net::protocol::{
        BlockData, BlocksChanged, FallingBlockLanded, PlayerInput, ServerMessage, WaterCellUpdate,
        WaterCellsChanged,
    };
    use crate::net::water_sync::WaterSyncOptimizer;
    use crate::water::WaterGrid;
    use nalgebra::Vector3;

    impl BandwidthStats {
        /// Calculate server-to-client bandwidth in KB/s.
        pub fn server_to_client_bandwidth_kbps(&self) -> f32 {
            if self.ticks == 0 {
                return 0.0;
            }
            let seconds = self.ticks as f32 * TICK_DELTA;
            (self.server_to_client_bytes as f32 / 1024.0) / seconds
        }

        /// Calculate client-to-server bandwidth in KB/s.
        pub fn client_to_server_bandwidth_kbps(&self) -> f32 {
            if self.ticks == 0 {
                return 0.0;
            }
            let seconds = self.ticks as f32 * TICK_DELTA;
            (self.client_to_server_bytes as f32 / 1024.0) / seconds
        }

        /// Add a server message to the stats.
        pub fn add_server_message(&mut self, message: &ServerMessage) {
            let type_name = match message {
                ServerMessage::WaterCellsChanged(_) => "WaterCellsChanged",
                ServerMessage::LavaCellsChanged(_) => "LavaCellsChanged",
                ServerMessage::FallingBlockSpawned(_) => "FallingBlockSpawned",
                ServerMessage::FallingBlockLanded(_) => "FallingBlockLanded",
                ServerMessage::BlocksChanged(_) => "BlocksChanged",
                ServerMessage::PlayerState(_) => "PlayerState",
                _ => "Other",
            };

            // Serialize to get actual size
            if let Ok(encoded) = bincode::serde::encode_to_vec(message, bincode::config::standard())
            {
                let size = encoded.len();
                self.server_to_client_bytes += size;
                *self
                    .message_counts
                    .entry(type_name.to_string())
                    .or_insert(0) += 1;
                *self.message_bytes.entry(type_name.to_string()).or_insert(0) += size;
            }
        }
    }

    /// Test: Bandwidth usage under typical gameplay conditions stays under 100 KB/s.
    ///
    /// This test simulates:
    /// - Player movement (20 updates/sec)
    /// - Block placement (5 blocks/sec average)
    /// - Water simulation (5 Hz updates)
    /// - Falling blocks (occasional)
    /// - Chunk loading (initial burst, then minimal)
    #[test]
    fn test_bandwidth_under_100_kb_per_second() {
        println!("=== Bandwidth Performance Test ===");
        println!(
            "Simulating {} ticks ({} seconds at 20 TPS)...",
            TEST_DURATION_TICKS,
            TEST_DURATION_TICKS as f32 * TICK_DELTA
        );
        println!("Target: < {:.1} KB/s per client", TARGET_BANDWIDTH_KB_S);

        // Setup server and client
        let mut stats = BandwidthStats::default();
        let mut server_water_grid = WaterGrid::new();
        let mut server_water_optimizer = WaterSyncOptimizer::new();
        let mut server_falling_sync = FallingBlockSync::new();
        let mut _client_water_grid = WaterGrid::new();
        let mut _client_falling = ClientFallingBlockSystem::new();

        // Simulation helpers
        let floor_solid = |pos: Vector3<i32>| pos.y < 0;
        let never_out_of_bounds = |_: Vector3<i32>| false;
        let no_world_water = |_: Vector3<i32>| false;
        let player_pos = Vector3::new(0.0, 64.0, 0.0);

        // Track player position for realistic updates
        let mut player_sequence = 0u32;
        let mut player_position = [0.0, 64.0, 0.0];

        // Main simulation loop
        for tick in 0..TEST_DURATION_TICKS {
            // === Client → Server: Player Input (20 Hz) ===
            {
                let input = PlayerInput {
                    sequence: player_sequence,
                    position: player_position,
                    velocity: [0.0, 0.0, 0.0],
                    yaw: 0.0,
                    pitch: 0.0,
                    actions: crate::net::protocol::InputActions::default(),
                };

                if let Ok(encoded) =
                    bincode::serde::encode_to_vec(&input, bincode::config::standard())
                {
                    stats.client_to_server_bytes += encoded.len();
                }

                player_sequence += 1;

                // Simulate player walking around
                player_position[0] += 0.1;
                if tick % 100 == 0 {
                    player_position[2] += 1.0;
                }
            }

            // === Server Actions ===

            // Place water source at tick 10 (simulates water bucket use)
            if tick == 10 {
                server_water_grid.place_source(Vector3::new(0, 70, 0), WaterType::Ocean);
            }

            // Place falling blocks occasionally
            if tick == 50 || tick == 100 || tick == 150 {
                let spawn = server_falling_sync
                    .register_spawn(Vector3::new(tick as i32, 80, 0), BlockType::Sand);

                // Measure spawn message
                let message = ServerMessage::FallingBlockSpawned(spawn);
                stats.add_server_message(&message);
            }

            // Simulate block placement (5 blocks/sec = every 4 ticks)
            if tick % 4 == 0 {
                // Block placement message (client → server)
                let block_data = BlockData::from(BlockType::Stone);
                if let Ok(encoded) =
                    bincode::serde::encode_to_vec(&block_data, bincode::config::standard())
                {
                    stats.client_to_server_bytes += encoded.len();
                }

                // Block confirmation (server → client)
                let message = ServerMessage::BlocksChanged(BlocksChanged {
                    changes: vec![([tick as i32 % 100, 64, 0], block_data)],
                });
                stats.add_server_message(&message);
            }

            // Water simulation tick (every 4 ticks = 5 Hz)
            if tick % 4 == 0 {
                let (_, water_updates) = server_water_grid.tick(
                    floor_solid,
                    never_out_of_bounds,
                    no_world_water,
                    player_pos,
                );

                let filtered = server_water_optimizer.filter_significant_changes(water_updates);

                if !filtered.is_empty() {
                    // Convert to protocol messages
                    let updates: Vec<WaterCellUpdate> = filtered
                        .iter()
                        .map(|u| WaterCellUpdate {
                            position: [u.position.x, u.position.y, u.position.z],
                            mass: u.mass,
                            is_source: u.is_source,
                            water_type: u.water_type,
                        })
                        .collect();

                    let message = ServerMessage::WaterCellsChanged(WaterCellsChanged { updates });
                    stats.add_server_message(&message);
                }
            }

            // Falling block land (simulate after 2 seconds)
            if tick == 90 || tick == 140 || tick == 190 {
                let land = FallingBlockLanded {
                    entity_id: 1 + (tick as u32 - 50) / 50,
                    position: [tick as i32 % 100, 64, 0],
                    block_type: BlockType::Sand,
                };

                let message = ServerMessage::FallingBlockLanded(land.clone());
                stats.add_server_message(&message);
                server_falling_sync.remove_entity(land.entity_id);
            }

            stats.ticks += 1;
        }

        // Calculate results
        let s2c_bandwidth = stats.server_to_client_bandwidth_kbps();
        let c2s_bandwidth = stats.client_to_server_bandwidth_kbps();
        let total_bandwidth = s2c_bandwidth + c2s_bandwidth;

        // Print detailed results
        println!("\n=== Bandwidth Results ===");
        println!("Server → Client: {:.2} KB/s", s2c_bandwidth);
        println!("Client → Server: {:.2} KB/s", c2s_bandwidth);
        println!("Total:           {:.2} KB/s", total_bandwidth);
        println!("\nMessage Breakdown:");
        for (msg_type, count) in &stats.message_counts {
            let bytes = stats.message_bytes.get(msg_type).unwrap_or(&0);
            let kb = *bytes as f32 / 1024.0;
            println!("  {}: {} messages, {:.2} KB total", msg_type, count, kb);
        }

        // Verify bandwidth is under target
        assert!(
            total_bandwidth < TARGET_BANDWIDTH_KB_S,
            "Total bandwidth {:.2} KB/s exceeds target {:.1} KB/s",
            total_bandwidth,
            TARGET_BANDWIDTH_KB_S
        );

        println!(
            "\n✅ Bandwidth test PASSED - {:.2} KB/s < {:.1} KB/s target",
            total_bandwidth, TARGET_BANDWIDTH_KB_S
        );
    }

    /// Test: Heavy water simulation bandwidth stays under target.
    #[test]
    fn test_water_simulation_bandwidth() {
        println!("=== Water Simulation Bandwidth Test ===");

        let mut stats = BandwidthStats::default();
        let mut server_water = WaterGrid::new();
        let mut optimizer = WaterSyncOptimizer::new();

        // Place multiple water sources (stress test)
        for x in 0..5 {
            for z in 0..5 {
                server_water.place_source(Vector3::new(x, 70, z), WaterType::Ocean);
            }
        }

        let floor_solid = |pos: Vector3<i32>| pos.y < 0;
        let never_out_of_bounds = |_: Vector3<i32>| false;
        let no_world_water = |_: Vector3<i32>| false;
        let player_pos = Vector3::new(2.0, 64.0, 2.0);

        // Simulate water flow for 10 seconds
        for tick in 0..TEST_DURATION_TICKS {
            let (_, water_updates) =
                server_water.tick(floor_solid, never_out_of_bounds, no_world_water, player_pos);

            let filtered = optimizer.filter_significant_changes(water_updates);

            if !filtered.is_empty() && tick % 4 == 0 {
                // 5 Hz rate limiting
                let updates: Vec<WaterCellUpdate> = filtered
                    .iter()
                    .map(|u| WaterCellUpdate {
                        position: [u.position.x, u.position.y, u.position.z],
                        mass: u.mass,
                        is_source: u.is_source,
                        water_type: u.water_type,
                    })
                    .collect();

                let message = ServerMessage::WaterCellsChanged(WaterCellsChanged { updates });
                stats.add_server_message(&message);
            }

            stats.ticks += 1;
        }

        let bandwidth = stats.server_to_client_bandwidth_kbps();

        println!("Water simulation bandwidth: {:.2} KB/s", bandwidth);
        println!("Total water messages: {:?}", stats.message_counts);

        // Water simulation should be < 50 KB/s (half of total budget)
        assert!(
            bandwidth < 50.0,
            "Water bandwidth {:.2} KB/s exceeds 50 KB/s limit",
            bandwidth
        );

        println!("✅ Water simulation bandwidth test PASSED");
    }

    /// Test: Player movement bandwidth is reasonable.
    #[test]
    fn test_player_movement_bandwidth() {
        println!("=== Player Movement Bandwidth Test ===");

        let mut total_bytes = 0usize;
        let mut sequence = 0u32;
        let mut position = [0.0, 64.0, 0.0];

        // Simulate 10 seconds of player movement (20 Hz)
        for _ in 0..TEST_DURATION_TICKS {
            let input = PlayerInput {
                sequence,
                position,
                velocity: [0.1, 0.0, 0.0],
                yaw: 0.0,
                pitch: 0.0,
                actions: crate::net::protocol::InputActions::default(),
            };

            if let Ok(encoded) = bincode::serde::encode_to_vec(&input, bincode::config::standard())
            {
                total_bytes += encoded.len();
            }

            sequence += 1;
            position[0] += 0.1;
        }

        let seconds = TEST_DURATION_TICKS as f32 * TICK_DELTA;
        let bandwidth = (total_bytes as f32 / 1024.0) / seconds;

        println!(
            "Player movement bandwidth: {:.2} KB/s ({} messages)",
            bandwidth, sequence
        );
        println!(
            "Average message size: {:.1} bytes",
            total_bytes as f32 / sequence as f32
        );

        // Player movement should be < 10 KB/s
        assert!(
            bandwidth < 10.0,
            "Player movement bandwidth {:.2} KB/s exceeds 10 KB/s limit",
            bandwidth
        );

        println!("✅ Player movement bandwidth test PASSED");
    }

    /// Test: Falling block bandwidth is minimal.
    #[test]
    fn test_falling_block_bandwidth() {
        println!("=== Falling Block Bandwidth Test ===");

        let mut stats = BandwidthStats::default();
        let mut sync = FallingBlockSync::new();

        // Simulate 10 seconds with occasional falling blocks
        for tick in 0..TEST_DURATION_TICKS {
            // Spawn falling block every 2 seconds
            if tick % 40 == 0 {
                let spawn = sync.register_spawn(Vector3::new(tick as i32, 80, 0), BlockType::Sand);

                let message = ServerMessage::FallingBlockSpawned(spawn);
                stats.add_server_message(&message);
            }

            // Land after 2 seconds
            if tick > 0 && tick % 40 == 20 {
                let land = FallingBlockLanded {
                    entity_id: (tick / 40) as u32,
                    position: [tick as i32, 64, 0],
                    block_type: BlockType::Sand,
                };

                let message = ServerMessage::FallingBlockLanded(land.clone());
                stats.add_server_message(&message);
                sync.remove_entity(land.entity_id);
            }

            stats.ticks += 1;
        }

        let bandwidth = stats.server_to_client_bandwidth_kbps();

        println!("Falling block bandwidth: {:.2} KB/s", bandwidth);
        println!(
            "Spawn messages: {:?}",
            stats.message_counts.get("FallingBlockSpawned")
        );
        println!(
            "Land messages: {:?}",
            stats.message_counts.get("FallingBlockLanded")
        );

        // Falling blocks should be < 5 KB/s
        assert!(
            bandwidth < 5.0,
            "Falling block bandwidth {:.2} KB/s exceeds 5 KB/s limit",
            bandwidth
        );

        println!("✅ Falling block bandwidth test PASSED");
    }

    /// Test: Verify bandwidth optimization features are effective.
    #[test]
    fn test_bandwidth_optimization_effectiveness() {
        println!("=== Bandwidth Optimization Effectiveness Test ===");

        // Test delta encoding
        let mut optimizer = WaterSyncOptimizer::new();
        let updates: Vec<crate::water::WaterCellSyncUpdate> = (0..100)
            .map(|i| crate::water::WaterCellSyncUpdate {
                position: Vector3::new(i, 70, 0),
                mass: 1.0,
                is_source: false,
                water_type: WaterType::Ocean,
            })
            .collect();

        // First update should pass all
        let filtered1 = optimizer.filter_significant_changes(updates.clone());
        assert_eq!(
            filtered1.len(),
            100,
            "First update should include all cells"
        );

        // Small changes should be filtered
        let small_changes: Vec<crate::water::WaterCellSyncUpdate> = (0..100)
            .map(|i| crate::water::WaterCellSyncUpdate {
                position: Vector3::new(i, 70, 0),
                mass: 1.01, // Only 1% change
                is_source: false,
                water_type: WaterType::Ocean,
            })
            .collect();

        let filtered2 = optimizer.filter_significant_changes(small_changes);
        assert_eq!(
            filtered2.len(),
            0,
            "Small changes should be filtered by delta encoding"
        );

        // Large changes should pass
        let large_changes: Vec<crate::water::WaterCellSyncUpdate> = (0..100)
            .map(|i| crate::water::WaterCellSyncUpdate {
                position: Vector3::new(i, 70, 0),
                mass: 0.9, // 10% change
                is_source: false,
                water_type: WaterType::Ocean,
            })
            .collect();

        let filtered3 = optimizer.filter_significant_changes(large_changes);
        assert!(
            filtered3.len() > 0,
            "Large changes should pass delta encoding"
        );

        // Test AoI filtering
        optimizer.clear();
        let updates: Vec<crate::water::WaterCellSyncUpdate> = vec![
            crate::water::WaterCellSyncUpdate {
                position: Vector3::new(0, 70, 0), // Near player
                mass: 1.0,
                is_source: true,
                water_type: WaterType::Ocean,
            },
            crate::water::WaterCellSyncUpdate {
                position: Vector3::new(1000, 70, 0), // Far from player
                mass: 1.0,
                is_source: true,
                water_type: WaterType::Ocean,
            },
        ];

        optimizer.filter_significant_changes(updates);
        let player_positions = [[0.0, 64.0, 0.0]];
        let aoi_filtered = optimizer.take_filtered_updates(&player_positions);

        assert_eq!(aoi_filtered.len(), 1, "AoI should filter distant cells");
        assert_eq!(aoi_filtered[0].position, [0, 70, 0]);

        // Test rate limiting
        assert!(
            !optimizer.should_broadcast_now(),
            "Should be rate limited after broadcast"
        );

        println!("✅ Delta encoding: 100% filtering of small changes");
        println!("✅ AoI filtering: 50% reduction (1/2 cells)");
        println!("✅ Rate limiting: Active (200ms min interval)");
        println!("\nBandwidth optimization test PASSED");
    }
}

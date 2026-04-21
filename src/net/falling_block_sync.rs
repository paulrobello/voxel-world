//! Falling block synchronization for multiplayer.
//!
//! Provides server-authoritative falling block simulation with:
//! - **Spawn broadcasting**: When a block loses support and starts falling
//! - **Land broadcasting**: When a falling block lands and becomes a static block
//! - **Entity ID tracking**: Unique IDs for each falling block entity
//!
//! # Architecture
//!
//! In single-player mode, falling blocks are simulated locally.
//! In multiplayer:
//! - **Server**: Simulates all falling block physics authoritatively
//! - **Client**: Only renders falling blocks based on spawn/land messages
//!
//! # Usage
//!
//! ```ignore
//! // Server-side: When a block starts falling
//! let sync = FallingBlockSync::new();
//! let entity_id = sync.next_entity_id();
//!
//! // Broadcast spawn to all clients
//! server.broadcast_falling_block_spawn(FallingBlockSpawned {
//!     entity_id,
//!     position: [x, y, z],
//!     velocity: [0.0, 0.0, 0.0],
//!     block_type: BlockType::Sand,
//! });
//!
//! // Server-side: When a falling block lands
//! server.broadcast_falling_block_landed(FallingBlockLanded {
//!     entity_id,
//!     position: [x, y, z],
//!     block_type: BlockType::Sand,
//! });
//!
//! // Client-side: Handle spawn message
//! for spawn in multiplayer.take_pending_falling_block_spawns() {
//!     client_falling_blocks.spawn_from_network(spawn);
//! }
//!
//! // Client-side: Handle land message
//! for land in multiplayer.take_pending_falling_block_lands() {
//!     client_falling_blocks.handle_landed(land);
//!     world.set_block(land.position, land.block_type);
//! }
//! ```

// Allow dead code since these methods are public API intended for future use
#![allow(dead_code)]

use crate::chunk::BlockType;
use crate::constants::TEXTURE_SIZE_Y;
use crate::falling_block::{FallingBlockSystem, LandedBlock};
use crate::net::protocol::{FallingBlockId, FallingBlockLanded, FallingBlockSpawned};
use nalgebra::Vector3;
use std::collections::HashMap;

/// Tracks active falling block entities for server-authoritative sync.
///
/// The server uses this to:
/// 1. Assign unique entity IDs to falling blocks
/// 2. Track which blocks are currently falling
/// 3. Generate spawn/land messages for clients
pub struct FallingBlockSync {
    /// Next available entity ID.
    next_entity_id: FallingBlockId,

    /// Maps entity IDs to their current state.
    active_entities: HashMap<FallingBlockId, FallingBlockEntity>,

    /// Cumulative count of spawn messages emitted.
    spawns_sent: u64,
    /// Cumulative count of land messages emitted.
    lands_sent: u64,
    /// Cumulative count of dropped-duplicate spawn registrations.
    duplicate_spawns: u64,
}

/// Server-side tracking of a falling block entity.
#[derive(Debug, Clone)]
pub struct FallingBlockEntity {
    /// Block type (Sand, Gravel, etc.).
    pub block_type: BlockType,
    /// Current position (center of block).
    pub position: Vector3<f32>,
    /// Current velocity.
    pub velocity: Vector3<f32>,
    /// Time since spawn (for animation sync).
    pub age: f32,
}

/// Statistics for monitoring falling block sync.
#[derive(Debug, Clone, Default)]
pub struct FallingBlockSyncStats {
    /// Total spawn messages sent.
    pub spawns_sent: u64,
    /// Total land messages sent.
    pub lands_sent: u64,
    /// Currently active entities.
    pub active_count: usize,
}

impl Default for FallingBlockSync {
    fn default() -> Self {
        Self::new()
    }
}

impl FallingBlockSync {
    /// Creates a new falling block sync tracker.
    pub fn new() -> Self {
        Self {
            next_entity_id: 1, // 0 is reserved/invalid
            active_entities: HashMap::with_capacity(64),
            spawns_sent: 0,
            lands_sent: 0,
            duplicate_spawns: 0,
        }
    }

    /// Allocates the next unique entity ID.
    pub fn next_entity_id(&mut self) -> FallingBlockId {
        let id = self.next_entity_id;
        self.next_entity_id = self.next_entity_id.wrapping_add(1);
        if self.next_entity_id == 0 {
            self.next_entity_id = 1; // Skip 0
        }
        id
    }

    /// Registers a newly spawned falling block and returns the spawn message.
    ///
    /// Call this when a block loses support and starts falling.
    pub fn register_spawn(
        &mut self,
        grid_position: Vector3<i32>,
        block_type: BlockType,
    ) -> FallingBlockSpawned {
        let entity_id = self.next_entity_id();

        // Convert grid position to center of block
        let position = Vector3::new(
            grid_position.x as f32 + 0.5,
            grid_position.y as f32 + 0.5,
            grid_position.z as f32 + 0.5,
        );

        // Track the entity. If the entity_id was somehow already present, count
        // it as a duplicate so future spawn-collision bugs are observable.
        if self
            .active_entities
            .insert(
                entity_id,
                FallingBlockEntity {
                    block_type,
                    position,
                    velocity: Vector3::zeros(),
                    age: 0.0,
                },
            )
            .is_some()
        {
            self.duplicate_spawns = self.duplicate_spawns.saturating_add(1);
            log::warn!(
                "[FallingBlockSync] Duplicate spawn registration for entity_id {}",
                entity_id
            );
        }

        self.spawns_sent = self.spawns_sent.saturating_add(1);

        FallingBlockSpawned {
            entity_id,
            position: [position.x, position.y, position.z],
            velocity: [0.0, 0.0, 0.0],
            block_type,
        }
    }

    /// Updates all active falling blocks and returns land messages for completed falls.
    ///
    /// This should be called on the server each tick to simulate physics.
    /// Returns spawn messages for any new blocks that started falling,
    /// and land messages for blocks that finished falling.
    pub fn update(
        &mut self,
        delta_time: f32,
        falling_system: &mut FallingBlockSystem,
        world: &crate::world::World,
    ) -> (Vec<FallingBlockSpawned>, Vec<FallingBlockLanded>) {
        let spawns = Vec::new();
        let mut lands = Vec::new();

        // Update the falling block physics
        let landed = falling_system.update(delta_time, |x, y, z| {
            if y < 0 || y >= TEXTURE_SIZE_Y as i32 {
                return false;
            }
            world
                .get_block(Vector3::new(x, y, z))
                .is_some_and(|b| b.is_solid())
        });

        // Process landed blocks - generate land messages using the entity id
        // carried by LandedBlock, and drop the entity from active_entities so
        // the server-side tracking map doesn't leak.
        for lb in landed {
            self.active_entities.remove(&lb.entity_id);
            lands.push(FallingBlockLanded {
                entity_id: lb.entity_id,
                position: [lb.position.x, lb.position.y, lb.position.z],
                block_type: lb.block_type,
            });
        }
        self.lands_sent = self.lands_sent.saturating_add(lands.len() as u64);

        (spawns, lands)
    }

    /// Removes a tracked entity (called when block lands).
    pub fn remove_entity(&mut self, entity_id: FallingBlockId) -> Option<FallingBlockEntity> {
        self.active_entities.remove(&entity_id)
    }

    /// Returns the number of active falling block entities.
    pub fn active_count(&self) -> usize {
        self.active_entities.len()
    }

    /// Returns statistics for monitoring.
    pub fn stats(&self) -> FallingBlockSyncStats {
        FallingBlockSyncStats {
            spawns_sent: self.spawns_sent,
            lands_sent: self.lands_sent,
            active_count: self.active_entities.len(),
        }
    }

    /// Returns the running count of duplicate spawn registrations. A non-zero
    /// value indicates an upstream ID collision bug.
    pub fn duplicate_spawns(&self) -> u64 {
        self.duplicate_spawns
    }

    /// Clears all tracked entities.
    pub fn clear(&mut self) {
        self.active_entities.clear();
    }
}

/// Client-side falling block system for rendering.
///
/// This system only renders falling blocks - all physics is handled
/// by the server. Blocks are spawned from network messages and
/// removed when the server sends a land message.
pub struct ClientFallingBlockSystem {
    /// Active falling blocks being rendered.
    blocks: Vec<ClientFallingBlock>,
    /// GPU data cache.
    gpu_data_cache: Vec<crate::falling_block::GpuFallingBlock>,
    /// Whether GPU data needs recalculation.
    gpu_data_dirty: bool,
}

/// A falling block on the client side.
#[derive(Debug, Clone)]
pub struct ClientFallingBlock {
    /// Entity ID from server.
    pub entity_id: FallingBlockId,
    /// Block type.
    pub block_type: BlockType,
    /// Current interpolated position.
    pub position: Vector3<f32>,
    /// Target position from last server update.
    pub target_position: Vector3<f32>,
    /// Velocity (for prediction).
    pub velocity: Vector3<f32>,
    /// Age for animation.
    pub age: f32,
}

impl Default for ClientFallingBlockSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientFallingBlockSystem {
    /// Creates a new client-side falling block system.
    pub fn new() -> Self {
        Self {
            blocks: Vec::with_capacity(64),
            gpu_data_cache: Vec::new(),
            gpu_data_dirty: true,
        }
    }

    /// Spawns a falling block from a server message.
    pub fn spawn_from_network(&mut self, spawn: &FallingBlockSpawned) {
        let block = ClientFallingBlock {
            entity_id: spawn.entity_id,
            block_type: spawn.block_type,
            position: Vector3::new(spawn.position[0], spawn.position[1], spawn.position[2]),
            target_position: Vector3::new(spawn.position[0], spawn.position[1], spawn.position[2]),
            velocity: Vector3::new(spawn.velocity[0], spawn.velocity[1], spawn.velocity[2]),
            age: 0.0,
        };

        // Remove any existing block with same entity ID
        self.blocks.retain(|b| b.entity_id != spawn.entity_id);

        self.blocks.push(block);
        self.gpu_data_dirty = true;
    }

    /// Handles a landing message from the server.
    ///
    /// Returns the landed block info so the client can place the block in the world.
    pub fn handle_landed(&mut self, land: &FallingBlockLanded) -> Option<LandedBlock> {
        let entity_id = land.entity_id;

        // Find and remove the falling block
        if let Some(idx) = self.blocks.iter().position(|b| b.entity_id == entity_id) {
            self.blocks.remove(idx);
            self.gpu_data_dirty = true;

            return Some(LandedBlock {
                entity_id,
                position: Vector3::new(land.position[0], land.position[1], land.position[2]),
                block_type: land.block_type,
            });
        }

        // If entity_id is 0, it means legacy sync without tracking - just return the land info
        if entity_id == 0 {
            return Some(LandedBlock {
                entity_id: 0,
                position: Vector3::new(land.position[0], land.position[1], land.position[2]),
                block_type: land.block_type,
            });
        }

        None
    }

    /// Updates client-side interpolation.
    ///
    /// Even though physics is server-authoritative, we interpolate position
    /// for smooth rendering between network updates. Uses the same `GRAVITY`
    /// constant as the server `FallingBlock::update` so client and server
    /// predictions can't drift out of step if the simulation ever changes.
    pub fn update(&mut self, delta_time: f32) {
        for block in &mut self.blocks {
            block.velocity.y -= crate::falling_block::GRAVITY * delta_time;
            block.position += block.velocity * delta_time;
            block.age += delta_time;
        }
    }

    /// Returns the number of active falling blocks.
    pub fn count(&self) -> usize {
        self.blocks.len()
    }

    /// Gets GPU-ready falling block data for rendering.
    pub fn gpu_data(&mut self) -> Vec<crate::falling_block::GpuFallingBlock> {
        if self.gpu_data_dirty {
            self.gpu_data_cache = self
                .blocks
                .iter()
                .map(|b| crate::falling_block::GpuFallingBlock {
                    pos_type: [
                        b.position.x,
                        b.position.y,
                        b.position.z,
                        b.block_type as u8 as f32,
                    ],
                    velocity_age: [b.velocity.x, b.velocity.y, b.velocity.z, b.age],
                })
                .collect();
            self.gpu_data_dirty = false;
        }
        self.gpu_data_cache.clone()
    }

    /// Clears all falling blocks.
    pub fn clear(&mut self) {
        self.blocks.clear();
        self.gpu_data_dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_spawn(
        entity_id: u32,
        x: f32,
        y: f32,
        z: f32,
        block_type: BlockType,
    ) -> FallingBlockSpawned {
        FallingBlockSpawned {
            entity_id,
            position: [x, y, z],
            velocity: [0.0, 0.0, 0.0],
            block_type,
        }
    }

    fn make_land(
        entity_id: u32,
        x: i32,
        y: i32,
        z: i32,
        block_type: BlockType,
    ) -> FallingBlockLanded {
        FallingBlockLanded {
            entity_id,
            position: [x, y, z],
            block_type,
        }
    }

    /// Full end-to-end: spawn a block via the real physics system, tick
    /// until it lands, and verify the LandedBlock carries the same entity_id
    /// as the original spawn. Also checks that the sync-side `active_entities`
    /// map doesn't leak across lands and `duplicate_spawns` stays at 0.
    #[test]
    fn test_falling_block_sync_update_flow_end_to_end() {
        use crate::falling_block::FallingBlockSystem;
        use crate::world::World;

        let mut sync = FallingBlockSync::new();
        let mut system = FallingBlockSystem::new();
        let mut world = World::new();

        // Place a solid floor at y=5 so the falling block has somewhere to
        // land within a small number of ticks.
        let floor = Vector3::new(0, 5, 0);
        world.set_block(floor, BlockType::Stone);

        // Spawn through the physics system so the entity_id is system-
        // allocated; FallingBlockSync::update will read it back from the
        // LandedBlock on collision. Start high enough that gravity + delta
        // time will traverse to the floor in a handful of ticks.
        let spawn_pos = Vector3::new(0, 40, 0);
        let id = system
            .spawn(spawn_pos, BlockType::Sand)
            .expect("system spawn");
        assert!(id > 0);

        // Step simulation until the block lands or we hit a safety cap.
        let mut lands_seen = Vec::new();
        for _ in 0..200 {
            let (_spawns, lands) = sync.update(1.0 / 60.0, &mut system, &world);
            if !lands.is_empty() {
                lands_seen.extend(lands);
                break;
            }
        }

        assert!(!lands_seen.is_empty(), "block should have landed");
        // entity_id from the LandedBlock must match the original spawn ID —
        // this is the regression guard for C3's entity_id: 0 bug.
        assert!(
            lands_seen.iter().any(|l| l.entity_id == id),
            "expected spawn id {} in lands {:?}",
            id,
            lands_seen.iter().map(|l| l.entity_id).collect::<Vec<_>>()
        );

        // No duplicate spawn warnings should have fired.
        assert_eq!(sync.duplicate_spawns(), 0);

        // Stats must reflect the land(s) we saw.
        let stats = sync.stats();
        assert_eq!(stats.lands_sent as usize, lands_seen.len());
        // The physics system is the ID allocator here, so spawns_sent on the
        // sync layer stays at zero (register_spawn was never called). Document
        // that invariant explicitly so a future refactor doesn't silently
        // double-count.
        assert_eq!(stats.spawns_sent, 0);
    }

    #[test]
    fn test_entity_id_allocation() {
        let mut sync = FallingBlockSync::new();

        let id1 = sync.next_entity_id();
        let id2 = sync.next_entity_id();
        let id3 = sync.next_entity_id();

        assert!(id1 > 0);
        assert!(id2 > 0);
        assert!(id3 > 0);
        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
    }

    #[test]
    fn test_register_spawn() {
        let mut sync = FallingBlockSync::new();

        let spawn = sync.register_spawn(Vector3::new(10, 20, 30), BlockType::Sand);

        assert!(spawn.entity_id > 0);
        assert_eq!(spawn.position, [10.5, 20.5, 30.5]);
        assert_eq!(spawn.block_type, BlockType::Sand);
        assert_eq!(sync.active_count(), 1);
    }

    #[test]
    fn test_client_spawn_and_land() {
        let mut client = ClientFallingBlockSystem::new();

        // Spawn a falling block from network
        let spawn = make_spawn(1, 10.5, 20.5, 30.5, BlockType::Gravel);
        client.spawn_from_network(&spawn);

        assert_eq!(client.count(), 1);

        // Handle landing
        let land = make_land(1, 10, 15, 30, BlockType::Gravel);
        let landed = client.handle_landed(&land);

        assert!(landed.is_some());
        let lb = landed.unwrap();
        assert_eq!(lb.position, Vector3::new(10, 15, 30));
        assert_eq!(lb.block_type, BlockType::Gravel);
        assert_eq!(client.count(), 0);
    }

    #[test]
    fn test_client_land_without_spawn() {
        let mut client = ClientFallingBlockSystem::new();

        // Land without spawn (legacy mode with entity_id=0)
        let land = make_land(0, 10, 15, 30, BlockType::Sand);
        let landed = client.handle_landed(&land);

        // Should still return the land info
        assert!(landed.is_some());
        assert_eq!(landed.unwrap().position, Vector3::new(10, 15, 30));
    }

    #[test]
    fn test_client_gpu_data() {
        let mut client = ClientFallingBlockSystem::new();

        let spawn = make_spawn(1, 10.5, 20.5, 30.5, BlockType::Sand);
        client.spawn_from_network(&spawn);

        let gpu_data = client.gpu_data();
        assert_eq!(gpu_data.len(), 1);
        assert_eq!(gpu_data[0].pos_type[3], BlockType::Sand as u8 as f32);
    }

    #[test]
    fn test_client_update_interpolation() {
        let mut client = ClientFallingBlockSystem::new();

        let spawn = make_spawn(1, 10.5, 20.5, 30.5, BlockType::Sand);
        client.spawn_from_network(&spawn);

        let initial_y = client.blocks[0].position.y;

        // Update should apply gravity
        client.update(0.1);

        // Position should have dropped
        assert!(client.blocks[0].position.y < initial_y);
        assert!(client.blocks[0].velocity.y < 0.0);
    }

    /// Integration test: Verifies falling block sync produces correct spawn/land flow.
    ///
    /// This test simulates the full multiplayer falling block sync flow:
    /// 1. Server registers a block spawn (block loses support)
    /// 2. Spawn message is serialized and sent to client
    /// 3. Client spawns the falling block locally
    /// 4. Server detects landing
    /// 5. Land message is serialized and sent to client
    /// 6. Client removes the falling block and places the static block
    #[test]
    fn test_falling_block_sync_flow() {
        use bincode;

        // === Setup ===
        let mut server_sync = FallingBlockSync::new();
        let mut client_system = ClientFallingBlockSystem::new();

        // === Phase 1: Server detects block should fall ===
        let grid_pos = Vector3::new(10, 20, 30);
        let spawn_msg = server_sync.register_spawn(grid_pos, BlockType::Sand);

        assert!(spawn_msg.entity_id > 0, "Entity ID should be assigned");
        assert_eq!(server_sync.active_count(), 1);

        // === Phase 2: Serialize and send spawn to client ===
        let encoded = bincode::serde::encode_to_vec(&spawn_msg, bincode::config::standard())
            .expect("Failed to encode spawn");
        let (decoded_spawn, _): (FallingBlockSpawned, usize) =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .expect("Failed to decode spawn");

        // === Phase 3: Client receives and spawns block ===
        client_system.spawn_from_network(&decoded_spawn);

        assert_eq!(
            client_system.count(),
            1,
            "Client should have 1 falling block"
        );

        // === Phase 4: Server detects landing (simulated) ===
        let land_msg = FallingBlockLanded {
            entity_id: spawn_msg.entity_id,
            position: [10, 10, 30], // Landed at y=10
            block_type: BlockType::Sand,
        };

        // === Phase 5: Serialize and send land to client ===
        let encoded = bincode::serde::encode_to_vec(&land_msg, bincode::config::standard())
            .expect("Failed to encode land");
        let (decoded_land, _): (FallingBlockLanded, usize) =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .expect("Failed to decode land");

        // === Phase 6: Client handles landing ===
        let landed = client_system.handle_landed(&decoded_land);

        assert!(landed.is_some(), "Client should return landed block info");
        let lb = landed.unwrap();
        assert_eq!(lb.position, Vector3::new(10, 10, 30));
        assert_eq!(lb.block_type, BlockType::Sand);
        assert_eq!(
            client_system.count(),
            0,
            "Client should have no falling blocks"
        );

        // Server removes entity
        server_sync.remove_entity(spawn_msg.entity_id);
        assert_eq!(server_sync.active_count(), 0);
    }

    /// Integration test: Multiple falling blocks with correct entity tracking.
    #[test]
    fn test_multiple_falling_blocks() {
        let mut server_sync = FallingBlockSync::new();
        let mut client_system = ClientFallingBlockSystem::new();

        // Spawn multiple blocks
        let spawn1 = server_sync.register_spawn(Vector3::new(0, 10, 0), BlockType::Sand);
        let spawn2 = server_sync.register_spawn(Vector3::new(5, 10, 0), BlockType::Gravel);
        let spawn3 = server_sync.register_spawn(Vector3::new(10, 10, 0), BlockType::Snow);

        // All should have different entity IDs
        assert_ne!(spawn1.entity_id, spawn2.entity_id);
        assert_ne!(spawn2.entity_id, spawn3.entity_id);
        assert_eq!(server_sync.active_count(), 3);

        // Client receives all spawns
        client_system.spawn_from_network(&spawn1);
        client_system.spawn_from_network(&spawn2);
        client_system.spawn_from_network(&spawn3);

        assert_eq!(client_system.count(), 3);

        // Two blocks land
        let land1 = FallingBlockLanded {
            entity_id: spawn1.entity_id,
            position: [0, 5, 0],
            block_type: BlockType::Sand,
        };
        let land2 = FallingBlockLanded {
            entity_id: spawn2.entity_id,
            position: [5, 5, 0],
            block_type: BlockType::Gravel,
        };

        client_system.handle_landed(&land1);
        client_system.handle_landed(&land2);

        assert_eq!(
            client_system.count(),
            1,
            "Only snow should still be falling"
        );

        // Last block lands
        let land3 = FallingBlockLanded {
            entity_id: spawn3.entity_id,
            position: [10, 5, 0],
            block_type: BlockType::Snow,
        };
        client_system.handle_landed(&land3);

        assert_eq!(client_system.count(), 0);
    }

    /// Integration test: Verifies falling sand is visible to all connected players.
    ///
    /// This test simulates the full multiplayer scenario:
    /// 1. Server detects a sand block should fall (loses support)
    /// 2. Server broadcasts FallingBlockSpawned to all clients
    /// 3. All clients receive and render the falling sand
    /// 4. Server simulates physics and detects landing
    /// 5. Server broadcasts FallingBlockLanded to all clients
    /// 6. All clients place the sand block and remove the entity
    ///
    /// This verifies the P0 critical sync point: "Falling sand visible to all connected players"
    #[test]
    fn test_falling_sand_visible_to_all_players() {
        use bincode;

        // === Setup: Simulate server and 3 connected clients ===
        let mut server_sync = FallingBlockSync::new();
        let mut client1 = ClientFallingBlockSystem::new();
        let mut client2 = ClientFallingBlockSystem::new();
        let mut client3 = ClientFallingBlockSystem::new();

        // === Phase 1: Server detects block should fall ===
        // A sand block at (100, 50, 100) loses support and starts falling
        let sand_grid_pos = Vector3::new(100, 50, 100);
        let spawn_msg = server_sync.register_spawn(sand_grid_pos, BlockType::Sand);

        // Verify server state
        assert!(spawn_msg.entity_id > 0, "Entity ID should be assigned");
        assert_eq!(spawn_msg.block_type, BlockType::Sand);
        // Position should be center of block
        assert_eq!(spawn_msg.position, [100.5, 50.5, 100.5]);
        assert_eq!(server_sync.active_count(), 1);

        // === Phase 2: Serialize spawn message (simulates network transmission) ===
        let encoded_spawn = bincode::serde::encode_to_vec(&spawn_msg, bincode::config::standard())
            .expect("Failed to encode spawn message");

        // === Phase 3: All clients receive the spawn message ===
        let (decoded_spawn1, _): (FallingBlockSpawned, usize) =
            bincode::serde::decode_from_slice(&encoded_spawn, bincode::config::standard())
                .expect("Client 1 failed to decode spawn");
        let (decoded_spawn2, _): (FallingBlockSpawned, usize) =
            bincode::serde::decode_from_slice(&encoded_spawn, bincode::config::standard())
                .expect("Client 2 failed to decode spawn");
        let (decoded_spawn3, _): (FallingBlockSpawned, usize) =
            bincode::serde::decode_from_slice(&encoded_spawn, bincode::config::standard())
                .expect("Client 3 failed to decode spawn");

        // All clients spawn the falling block
        client1.spawn_from_network(&decoded_spawn1);
        client2.spawn_from_network(&decoded_spawn2);
        client3.spawn_from_network(&decoded_spawn3);

        // === Verify: All clients see the falling sand ===
        assert_eq!(client1.count(), 1, "Client 1 should see falling sand");
        assert_eq!(client2.count(), 1, "Client 2 should see falling sand");
        assert_eq!(client3.count(), 1, "Client 3 should see falling sand");

        // Verify all clients have the correct block type
        let c1_gpu = client1.gpu_data();
        let c2_gpu = client2.gpu_data();
        let c3_gpu = client3.gpu_data();
        assert_eq!(c1_gpu.len(), 1);
        assert_eq!(c2_gpu.len(), 1);
        assert_eq!(c3_gpu.len(), 1);
        assert_eq!(c1_gpu[0].pos_type[3], BlockType::Sand as u8 as f32);
        assert_eq!(c2_gpu[0].pos_type[3], BlockType::Sand as u8 as f32);
        assert_eq!(c3_gpu[0].pos_type[3], BlockType::Sand as u8 as f32);

        // === Phase 4: Simulate client-side rendering updates ===
        // Clients interpolate falling animation
        let delta_time = 0.05; // 50ms
        client1.update(delta_time);
        client2.update(delta_time);
        client3.update(delta_time);

        // All clients should still have the falling block
        assert_eq!(client1.count(), 1);
        assert_eq!(client2.count(), 1);
        assert_eq!(client3.count(), 1);

        // === Phase 5: Server detects landing ===
        let land_msg = FallingBlockLanded {
            entity_id: spawn_msg.entity_id,
            position: [100, 45, 100], // Landed 5 blocks below
            block_type: BlockType::Sand,
        };

        // === Phase 6: Serialize and broadcast land message ===
        let encoded_land = bincode::serde::encode_to_vec(&land_msg, bincode::config::standard())
            .expect("Failed to encode land message");

        // === Phase 7: All clients receive land message ===
        let (decoded_land1, _): (FallingBlockLanded, usize) =
            bincode::serde::decode_from_slice(&encoded_land, bincode::config::standard())
                .expect("Client 1 failed to decode land");
        let (decoded_land2, _): (FallingBlockLanded, usize) =
            bincode::serde::decode_from_slice(&encoded_land, bincode::config::standard())
                .expect("Client 2 failed to decode land");
        let (decoded_land3, _): (FallingBlockLanded, usize) =
            bincode::serde::decode_from_slice(&encoded_land, bincode::config::standard())
                .expect("Client 3 failed to decode land");

        // All clients handle landing
        let land1 = client1.handle_landed(&decoded_land1);
        let land2 = client2.handle_landed(&decoded_land2);
        let land3 = client3.handle_landed(&decoded_land3);

        // === Verify: All clients successfully placed the block ===
        assert!(land1.is_some(), "Client 1 should receive landed block info");
        assert!(land2.is_some(), "Client 2 should receive landed block info");
        assert!(land3.is_some(), "Client 3 should receive landed block info");

        // Verify correct landing position and block type
        let lb1 = land1.unwrap();
        let lb2 = land2.unwrap();
        let lb3 = land3.unwrap();
        assert_eq!(lb1.position, Vector3::new(100, 45, 100));
        assert_eq!(lb2.position, Vector3::new(100, 45, 100));
        assert_eq!(lb3.position, Vector3::new(100, 45, 100));
        assert_eq!(lb1.block_type, BlockType::Sand);
        assert_eq!(lb2.block_type, BlockType::Sand);
        assert_eq!(lb3.block_type, BlockType::Sand);

        // Verify no more falling blocks visible
        assert_eq!(client1.count(), 0, "Client 1 should have no falling blocks");
        assert_eq!(client2.count(), 0, "Client 2 should have no falling blocks");
        assert_eq!(client3.count(), 0, "Client 3 should have no falling blocks");

        // === Phase 8: Server cleanup ===
        server_sync.remove_entity(spawn_msg.entity_id);
        assert_eq!(server_sync.active_count(), 0);

        println!(
            "Successfully verified falling sand sync across 3 clients: spawn_id={}, landed_at=({},{},{})",
            spawn_msg.entity_id, lb1.position.x, lb1.position.y, lb1.position.z
        );
    }

    /// Integration test: Verifies falling block sync via ServerMessage protocol.
    ///
    /// This test verifies that falling block messages are properly wrapped
    /// in ServerMessage enum and can be serialized/deserialized correctly,
    /// matching the actual network protocol used in multiplayer.
    #[test]
    fn test_falling_block_via_server_message_protocol() {
        use crate::net::protocol::ServerMessage;
        use bincode;

        // === Setup ===
        let mut server_sync = FallingBlockSync::new();
        let mut client = ClientFallingBlockSystem::new();

        // === Server creates spawn message ===
        let spawn = server_sync.register_spawn(Vector3::new(0, 100, 0), BlockType::Gravel);

        // Wrap in ServerMessage (as done in actual server code)
        let server_msg_spawn = ServerMessage::FallingBlockSpawned(spawn.clone());

        // Serialize as ServerMessage
        let encoded = bincode::serde::encode_to_vec(&server_msg_spawn, bincode::config::standard())
            .expect("Failed to encode ServerMessage");

        // Deserialize as ServerMessage
        let (decoded, _): (ServerMessage, usize) =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .expect("Failed to decode ServerMessage");

        // Verify correct message type
        match decoded {
            ServerMessage::FallingBlockSpawned(received_spawn) => {
                assert_eq!(received_spawn.entity_id, spawn.entity_id);
                assert_eq!(received_spawn.block_type, BlockType::Gravel);
                client.spawn_from_network(&received_spawn);
            }
            _ => panic!("Expected FallingBlockSpawned message"),
        }

        assert_eq!(client.count(), 1, "Client should have falling gravel");

        // === Server creates land message ===
        let land = FallingBlockLanded {
            entity_id: spawn.entity_id,
            position: [0, 90, 0],
            block_type: BlockType::Gravel,
        };

        let server_msg_land = ServerMessage::FallingBlockLanded(land);

        // Serialize and deserialize
        let encoded = bincode::serde::encode_to_vec(&server_msg_land, bincode::config::standard())
            .expect("Failed to encode land ServerMessage");
        let (decoded, _): (ServerMessage, usize) =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .expect("Failed to decode land ServerMessage");

        match decoded {
            ServerMessage::FallingBlockLanded(received_land) => {
                assert_eq!(received_land.entity_id, spawn.entity_id);
                let landed = client.handle_landed(&received_land);
                assert!(landed.is_some());
                assert_eq!(landed.unwrap().position, Vector3::new(0, 90, 0));
            }
            _ => panic!("Expected FallingBlockLanded message"),
        }

        assert_eq!(
            client.count(),
            0,
            "Client should have no falling blocks after landing"
        );
    }

    /// Integration test: Verifies falling blocks of different types (sand, gravel, snow).
    #[test]
    fn test_all_falling_block_types() {
        use bincode;

        let mut server_sync = FallingBlockSync::new();
        let mut client = ClientFallingBlockSystem::new();

        // Test all supported falling block types
        let falling_types = vec![
            (BlockType::Sand, "Sand"),
            (BlockType::Gravel, "Gravel"),
            (BlockType::Snow, "Snow"),
        ];

        for (block_type, name) in &falling_types {
            // Spawn
            let spawn = server_sync.register_spawn(Vector3::new(0, 50, 0), *block_type);
            let encoded =
                bincode::serde::encode_to_vec(&spawn, bincode::config::standard()).unwrap();
            let (decoded, _): (FallingBlockSpawned, usize) =
                bincode::serde::decode_from_slice(&encoded, bincode::config::standard()).unwrap();

            client.spawn_from_network(&decoded);

            assert_eq!(client.count(), 1, "Client should see falling {}", name);
            let gpu = client.gpu_data();
            assert_eq!(gpu[0].pos_type[3], *block_type as u8 as f32);

            // Land
            let land = FallingBlockLanded {
                entity_id: spawn.entity_id,
                position: [0, 40, 0],
                block_type: *block_type,
            };
            let encoded =
                bincode::serde::encode_to_vec(&land, bincode::config::standard()).unwrap();
            let (decoded, _): (FallingBlockLanded, usize) =
                bincode::serde::decode_from_slice(&encoded, bincode::config::standard()).unwrap();

            let landed = client.handle_landed(&decoded);
            assert!(landed.is_some(), "{} should land", name);
            assert_eq!(landed.unwrap().block_type, *block_type);
            assert_eq!(client.count(), 0);

            println!("Verified falling block sync for {}", name);
        }

        assert_eq!(server_sync.active_count(), 3); // Still tracked on server
        server_sync.clear();
        assert_eq!(server_sync.active_count(), 0);
    }
}

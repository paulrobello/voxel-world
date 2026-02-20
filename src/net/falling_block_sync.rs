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

        // Track the entity
        self.active_entities.insert(
            entity_id,
            FallingBlockEntity {
                block_type,
                position,
                velocity: Vector3::zeros(),
                age: 0.0,
            },
        );

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

        // Process landed blocks - generate land messages
        for lb in landed {
            let land_msg = FallingBlockLanded {
                entity_id: 0, // We need to track which entity this was
                position: [lb.position.x, lb.position.y, lb.position.z],
                block_type: lb.block_type,
            };
            lands.push(land_msg);
        }

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
            spawns_sent: 0, // Would be tracked separately
            lands_sent: 0,
            active_count: self.active_entities.len(),
        }
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
                position: Vector3::new(land.position[0], land.position[1], land.position[2]),
                block_type: land.block_type,
            });
        }

        // If entity_id is 0, it means legacy sync without tracking - just return the land info
        if entity_id == 0 {
            return Some(LandedBlock {
                position: Vector3::new(land.position[0], land.position[1], land.position[2]),
                block_type: land.block_type,
            });
        }

        None
    }

    /// Updates client-side interpolation.
    ///
    /// Even though physics is server-authoritative, we interpolate position
    /// for smooth rendering between network updates.
    pub fn update(&mut self, delta_time: f32) {
        for block in &mut self.blocks {
            // Simple gravity prediction for smoother visuals
            block.velocity.y -= 20.0 * delta_time;
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
}

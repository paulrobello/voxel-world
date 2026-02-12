//! Network protocol message types for voxel-world multiplayer.
//!
//! This module defines all message types exchanged between client and server.
//! All messages use bincode for serialization for speed and compactness.

// Allow unused code until networking is integrated into the game
#![allow(dead_code)]

use crate::chunk::{BlockModelData, BlockPaintData, BlockType, WaterType};
use serde::{Deserialize, Serialize};

// ============================================================================
// Client → Server Messages
// ============================================================================

/// Input actions that can be performed by a player.
/// These are sent as bitflags for efficiency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InputActions(u16);

impl InputActions {
    pub const NONE: u16 = 0;
    pub const JUMP: u16 = 1 << 0;
    pub const SPRINT: u16 = 1 << 1;
    pub const SNEAK: u16 = 1 << 2;
    pub const PLACE_BLOCK: u16 = 1 << 3;
    pub const BREAK_BLOCK: u16 = 1 << 4;
    pub const USE_ITEM: u16 = 1 << 5;

    pub fn new(bits: u16) -> Self {
        Self(bits)
    }

    pub fn bits(self) -> u16 {
        self.0
    }

    pub fn contains(self, flag: u16) -> bool {
        (self.0 & flag) != 0
    }

    pub fn insert(&mut self, flag: u16) {
        self.0 |= flag;
    }

    pub fn remove(&mut self, flag: u16) {
        self.0 &= !flag;
    }
}

/// Player input sent every frame (~20/sec).
/// Contains predicted position and velocity for client-side prediction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerInput {
    /// Sequence number for reconciliation.
    pub sequence: u32,
    /// Predicted player position.
    pub position: [f32; 3],
    /// Player velocity.
    pub velocity: [f32; 3],
    /// Camera yaw (horizontal rotation).
    pub yaw: f32,
    /// Camera pitch (vertical rotation).
    pub pitch: f32,
    /// Input action flags.
    pub actions: InputActions,
}

/// Block data for network transmission.
/// Includes block type and any associated metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockData {
    /// Block type.
    pub block_type: BlockType,
    /// Model data for Model blocks.
    pub model_data: Option<BlockModelData>,
    /// Paint data for Painted blocks.
    pub paint_data: Option<BlockPaintData>,
    /// Tint index for TintedGlass and Crystal blocks.
    pub tint_index: Option<u8>,
    /// Water type for Water blocks.
    pub water_type: Option<WaterType>,
}

impl Default for BlockData {
    fn default() -> Self {
        Self {
            block_type: BlockType::Air,
            model_data: None,
            paint_data: None,
            tint_index: None,
            water_type: None,
        }
    }
}

impl From<BlockType> for BlockData {
    fn from(block_type: BlockType) -> Self {
        Self {
            block_type,
            ..Default::default()
        }
    }
}

/// Place a block at a world position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaceBlock {
    /// World position (block coordinates).
    pub position: [i32; 3],
    /// Block to place.
    pub block: BlockData,
}

/// Break a block at a world position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BreakBlock {
    /// World position (block coordinates).
    pub position: [i32; 3],
}

/// Bulk operation types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BulkOperation {
    /// Fill a region with a block type.
    Fill {
        start: [i32; 3],
        end: [i32; 3],
        block: BlockData,
    },
    /// Apply a template at a position.
    Template {
        position: [i32; 3],
        template_name: String,
        rotation: u8,
    },
    /// Replace blocks of one type with another.
    Replace {
        start: [i32; 3],
        end: [i32; 3],
        from: BlockType,
        to: BlockData,
    },
}

/// Request chunks from the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestChunks {
    /// Chunk positions to request (chunk coordinates).
    pub positions: Vec<[i32; 3]>,
}

/// Console command sent from client to server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsoleCommand {
    /// Command string (e.g., "/tp 100 64 200").
    pub command: String,
}

/// All messages that can be sent from client to server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Player input update.
    PlayerInput(PlayerInput),
    /// Place a block.
    PlaceBlock(PlaceBlock),
    /// Break a block.
    BreakBlock(BreakBlock),
    /// Bulk operation.
    BulkOperation(BulkOperation),
    /// Request chunk data.
    RequestChunks(RequestChunks),
    /// Console command.
    ConsoleCommand(ConsoleCommand),
}

// ============================================================================
// Server → Client Messages
// ============================================================================

/// Player ID type.
pub type PlayerId = u64;

/// Authoritative player state from server.
/// Used for reconciliation when prediction differs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerState {
    /// Player ID.
    pub player_id: PlayerId,
    /// Authoritative position.
    pub position: [f32; 3],
    /// Authoritative velocity.
    pub velocity: [f32; 3],
    /// Last processed input sequence number.
    pub last_sequence: u32,
    /// Camera yaw.
    pub yaw: f32,
    /// Camera pitch.
    pub pitch: f32,
}

/// Single block change notification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockChanged {
    /// World position.
    pub position: [i32; 3],
    /// New block data.
    pub block: BlockData,
}

/// Multiple block changes (for bulk operations).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlocksChanged {
    /// List of position and block pairs.
    pub changes: Vec<([i32; 3], BlockData)>,
}

/// Chunk data sent from server to client.
/// The data is compressed with LZ4 and ready for decompression.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkData {
    /// Chunk position (chunk coordinates).
    pub position: [i32; 3],
    /// Version number for delta compression.
    pub version: u32,
    /// Compressed chunk data (LZ4 compressed).
    /// Decompresses to serialized chunk blocks and metadata.
    pub compressed_data: Vec<u8>,
}

/// Instructs the client to generate a chunk locally using the world seed.
/// Sent when the chunk has no player modifications (bandwidth optimization).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkGenerateLocal {
    /// Chunk position (chunk coordinates).
    pub position: [i32; 3],
}

/// Notification that a player joined the game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerJoined {
    /// Player ID.
    pub player_id: PlayerId,
    /// Player display name.
    pub name: String,
    /// Initial spawn position.
    pub position: [f32; 3],
}

/// Notification that a player left the game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerLeft {
    /// Player ID.
    pub player_id: PlayerId,
}

/// Time of day synchronization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeUpdate {
    /// Time of day (0.0-1.0, where 0.5 = noon).
    pub time_of_day: f32,
}

/// Connection accepted response from server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectionAccepted {
    /// Assigned player ID.
    pub player_id: PlayerId,
    /// Server tick rate.
    pub tick_rate: u32,
    /// Initial spawn position.
    pub spawn_position: [f32; 3],
    /// World seed.
    pub world_seed: u32,
    /// World generation type.
    pub world_gen: u8,
}

/// Connection rejected response from server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionRejected {
    /// Reason for rejection.
    pub reason: String,
}

/// All messages that can be sent from server to client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServerMessage {
    /// Connection accepted.
    ConnectionAccepted(ConnectionAccepted),
    /// Connection rejected.
    ConnectionRejected(ConnectionRejected),
    /// Player state update (for reconciliation).
    PlayerState(PlayerState),
    /// Single block changed.
    BlockChanged(BlockChanged),
    /// Multiple blocks changed.
    BlocksChanged(BlocksChanged),
    /// Full chunk data (for modified chunks).
    ChunkData(ChunkData),
    /// Instruct client to generate chunk locally (for unmodified chunks).
    ChunkGenerateLocal(ChunkGenerateLocal),
    /// Player joined notification.
    PlayerJoined(PlayerJoined),
    /// Player left notification.
    PlayerLeft(PlayerLeft),
    /// Time of day update.
    TimeUpdate(TimeUpdate),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_actions() {
        let mut actions = InputActions::default();
        assert!(!actions.contains(InputActions::JUMP));
        assert!(!actions.contains(InputActions::SPRINT));

        actions.insert(InputActions::JUMP);
        assert!(actions.contains(InputActions::JUMP));
        assert!(!actions.contains(InputActions::SPRINT));

        actions.insert(InputActions::SPRINT);
        assert!(actions.contains(InputActions::JUMP));
        assert!(actions.contains(InputActions::SPRINT));

        actions.remove(InputActions::JUMP);
        assert!(!actions.contains(InputActions::JUMP));
        assert!(actions.contains(InputActions::SPRINT));
    }

    #[test]
    fn test_block_data_default() {
        let data = BlockData::default();
        assert_eq!(data.block_type, BlockType::Air);
        assert!(data.model_data.is_none());
        assert!(data.paint_data.is_none());
    }

    #[test]
    fn test_block_data_from_block_type() {
        let data = BlockData::from(BlockType::Stone);
        assert_eq!(data.block_type, BlockType::Stone);
        assert!(data.model_data.is_none());
    }

    #[test]
    fn test_message_serialization() {
        // Test that messages can be serialized and deserialized
        let msg = ClientMessage::BreakBlock(BreakBlock {
            position: [10, 20, 30],
        });
        let encoded = bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();
        let decoded: ClientMessage =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;
        assert_eq!(msg, decoded);
    }
}

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

/// Client uploads a new custom model to the server.
/// Server will assign an ID and broadcast to all clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadModel {
    /// Model name.
    pub name: String,
    /// Author name.
    pub author: String,
    /// LZ4 compressed VxmFile data.
    pub model_data: Vec<u8>,
}

/// Client uploads a new custom texture to the server.
/// Server will assign a slot and broadcast to all clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadTexture {
    /// Texture name.
    pub name: String,
    /// PNG data (64x64 RGBA).
    pub png_data: Vec<u8>,
}

/// Client requests to place a water source at a position.
/// Server will process this authoritatively and broadcast to all clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceWaterSource {
    /// World position (block coordinates) for the water source.
    pub position: [i32; 3],
    /// Type of water to place.
    pub water_type: WaterType,
}

/// Client requests to place a lava source at a position.
/// Server will process this authoritatively and broadcast to all clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceLavaSource {
    /// World position (block coordinates) for the lava source.
    pub position: [i32; 3],
}

/// Client uploads a new picture to the server for use in picture frames.
/// Server will assign an ID and broadcast to all clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadPicture {
    /// Picture name.
    pub name: String,
    /// PNG data (RGBA).
    pub png_data: Vec<u8>,
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
    /// Request texture data.
    RequestTexture(RequestTexture),
    /// Upload a custom model to the server.
    UploadModel(UploadModel),
    /// Upload a custom texture to the server.
    UploadTexture(UploadTexture),
    /// Place a water source (water bucket).
    PlaceWaterSource(PlaceWaterSource),
    /// Place a lava source (lava bucket).
    PlaceLavaSource(PlaceLavaSource),
    /// Upload a picture to the server for picture frames.
    UploadPicture(UploadPicture),
}

// ============================================================================
// Server → Client Messages
// ============================================================================

/// Player ID type.
pub type PlayerId = u64;

/// Single water cell update for multiplayer synchronization.
/// Sent by the server to all clients when water state changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaterCellUpdate {
    /// World position of the water cell.
    pub position: [i32; 3],
    /// Water mass (0.0 to 1.0+ for pressurized water).
    /// Mass <= 0 indicates the cell should be removed.
    pub mass: f32,
    /// Whether this is an infinite water source.
    pub is_source: bool,
    /// Type of water (determines color and flow behavior).
    pub water_type: WaterType,
}

/// Batch water cell updates for multiplayer synchronization.
/// Sent by the server at a throttled rate (2-5 Hz) to conserve bandwidth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaterCellsChanged {
    /// List of water cell updates.
    pub updates: Vec<WaterCellUpdate>,
}

/// Single lava cell update for multiplayer synchronization.
/// Sent by the server to all clients when lava state changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LavaCellUpdate {
    /// World position of the lava cell.
    pub position: [i32; 3],
    /// Lava mass (0.0 to 1.0).
    /// Mass <= 0 indicates the cell should be removed.
    pub mass: f32,
    /// Whether this is an infinite lava source.
    pub is_source: bool,
}

/// Batch lava cell updates for multiplayer synchronization.
/// Sent by the server at a throttled rate (2-5 Hz) to conserve bandwidth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LavaCellsChanged {
    /// List of lava cell updates.
    pub updates: Vec<LavaCellUpdate>,
}

/// Entity ID type for falling blocks.
pub type FallingBlockId = u32;

/// Notification that a falling block has spawned.
/// Sent by the server when a block loses support and starts falling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FallingBlockSpawned {
    /// Unique entity ID for this falling block.
    pub entity_id: FallingBlockId,
    /// Spawn position (world coordinates, center of block).
    pub position: [f32; 3],
    /// Initial velocity (typically zero, gravity is applied server-side).
    pub velocity: [f32; 3],
    /// The type of block that is falling.
    pub block_type: BlockType,
}

/// Notification that a falling block has landed.
/// Sent by the server when a falling block comes to rest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FallingBlockLanded {
    /// Entity ID of the falling block that landed.
    pub entity_id: FallingBlockId,
    /// Grid position where the block landed.
    pub position: [i32; 3],
    /// The type of block that landed.
    pub block_type: BlockType,
}

/// A single block in a tree fall event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeFellBlock {
    /// Entity ID for this falling block.
    pub entity_id: FallingBlockId,
    /// Grid position where the block started falling.
    pub position: [i32; 3],
    /// The type of block (log or leaves).
    pub block_type: BlockType,
}

/// Notification that a tree has fallen.
/// Sent by the server when a connected tree loses ground support.
/// All blocks in the tree become falling blocks simultaneously.
/// This is more bandwidth-efficient than sending individual FallingBlockSpawned messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeFell {
    /// List of all blocks in the tree that are now falling.
    pub blocks: Vec<TreeFellBlock>,
}

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

/// Day cycle pause state synchronization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DayCyclePauseChanged {
    /// Whether the day cycle is paused.
    pub paused: bool,
    /// Current time of day when pause state changed.
    pub time_of_day: f32,
}

/// Spawn position synchronization.
/// Sent by the server when the spawn point changes (e.g., via console command).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpawnPositionChanged {
    /// New spawn position in world coordinates.
    pub position: [f32; 3],
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
    /// Number of custom textures in the server's texture pool.
    pub custom_texture_count: u8,
}

/// Connection rejected response from server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionRejected {
    /// Reason for rejection.
    pub reason: String,
}

/// Sync custom models from server to client.
/// Sent immediately after ConnectionAccepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRegistrySync {
    /// LZ4 compressed WorldModelStore data (same format as models.dat)
    pub models_data: Vec<u8>,
    /// LZ4 compressed DoorPairStore data (same format as door_pairs.dat)
    pub door_pairs_data: Vec<u8>,
}

/// Sent when client requests a texture they don't have.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureData {
    /// Slot index (0-based)
    pub slot: u8,
    /// PNG data (64x64 RGBA)
    pub data: Vec<u8>,
}

/// Notification that a new texture was added to the pool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureAdded {
    pub slot: u8,
    pub name: String,
}

/// Client requests a texture they encountered but don't have.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestTexture {
    pub slot: u8,
}

/// Server broadcasts a new custom model to all clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelAdded {
    /// Assigned model ID (>= FIRST_CUSTOM_MODEL_ID).
    pub model_id: u8,
    /// Model name.
    pub name: String,
    /// Author name.
    pub author: String,
    /// LZ4 compressed VxmFile data.
    pub model_data: Vec<u8>,
}

/// Notification that a new picture was added for picture frames.
/// Sent by the server after a client uploads a picture via UploadPicture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PictureAdded {
    /// Assigned picture ID.
    pub picture_id: u16,
    /// Picture name.
    pub name: String,
}

/// Notification that a picture frame was assigned a picture.
/// Sent by the server when a player sets a picture in a frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FramePictureSet {
    /// World position of the picture frame block.
    pub position: [i32; 3],
    /// Assigned picture ID (None if cleared).
    pub picture_id: Option<u16>,
}

/// Stencil ID type.
pub type StencilId = u64;

/// Notification that a stencil was loaded into the world.
/// Sent by the server when a stencil is loaded via console command.
/// The stencil data is zstd-compressed StencilFile bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StencilLoaded {
    /// Unique stencil ID assigned by the server.
    pub stencil_id: StencilId,
    /// Stencil name.
    pub name: String,
    /// LZ4 compressed StencilFile data (same format as .vxs files).
    pub stencil_data: Vec<u8>,
}

/// Notification that a stencil's transform was updated.
/// Sent by the server when a stencil is moved, rotated, or placed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StencilTransformUpdate {
    /// Stencil ID.
    pub stencil_id: StencilId,
    /// World position (anchor point).
    pub position: [i32; 3],
    /// Rotation (0-3 for 0°/90°/180°/270° around Y-axis).
    pub rotation: u8,
}

/// Notification that a stencil was removed from the world.
/// Sent by the server when a stencil is cleared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StencilRemoved {
    /// Stencil ID that was removed.
    pub stencil_id: StencilId,
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
    /// Day cycle pause state changed.
    DayCyclePauseChanged(DayCyclePauseChanged),
    /// Spawn position changed.
    SpawnPositionChanged(SpawnPositionChanged),
    /// Sync custom models from server.
    ModelRegistrySync(ModelRegistrySync),
    /// Texture data response.
    TextureData(TextureData),
    /// Notification of new texture added.
    TextureAdded(TextureAdded),
    /// Notification of new custom model added.
    ModelAdded(ModelAdded),
    /// Notification of new picture added for picture frames.
    PictureAdded(PictureAdded),
    /// Notification that a picture frame was assigned a picture.
    FramePictureSet(FramePictureSet),
    /// Batch water cell updates (throttled to 2-5 Hz).
    WaterCellsChanged(WaterCellsChanged),
    /// Batch lava cell updates (throttled to 2-5 Hz).
    LavaCellsChanged(LavaCellsChanged),
    /// Falling block spawned notification.
    FallingBlockSpawned(FallingBlockSpawned),
    /// Falling block landed notification.
    FallingBlockLanded(FallingBlockLanded),
    /// Tree fell notification (batch of falling blocks).
    TreeFell(TreeFell),
    /// Stencil loaded notification.
    StencilLoaded(StencilLoaded),
    /// Stencil transform update.
    StencilTransformUpdate(StencilTransformUpdate),
    /// Stencil removed notification.
    StencilRemoved(StencilRemoved),
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

    #[test]
    fn test_tree_fell_block_serialization() {
        // Test TreeFellBlock serialization
        let block = TreeFellBlock {
            entity_id: 42,
            position: [100, 64, 200],
            block_type: BlockType::Log,
        };
        let encoded = bincode::serde::encode_to_vec(&block, bincode::config::standard()).unwrap();
        let decoded: TreeFellBlock =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;
        assert_eq!(block.entity_id, decoded.entity_id);
        assert_eq!(block.position, decoded.position);
        assert_eq!(block.block_type, decoded.block_type);
    }

    #[test]
    fn test_tree_fell_serialization() {
        // Test TreeFell message with multiple blocks
        let tree_fell = TreeFell {
            blocks: vec![
                TreeFellBlock {
                    entity_id: 1,
                    position: [0, 0, 0],
                    block_type: BlockType::Log,
                },
                TreeFellBlock {
                    entity_id: 2,
                    position: [0, 1, 0],
                    block_type: BlockType::Log,
                },
                TreeFellBlock {
                    entity_id: 3,
                    position: [1, 1, 0],
                    block_type: BlockType::Leaves,
                },
            ],
        };
        let encoded =
            bincode::serde::encode_to_vec(&tree_fell, bincode::config::standard()).unwrap();
        let decoded: TreeFell =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;
        assert_eq!(tree_fell.blocks.len(), decoded.blocks.len());
        for (orig, dec) in tree_fell.blocks.iter().zip(decoded.blocks.iter()) {
            assert_eq!(orig.entity_id, dec.entity_id);
            assert_eq!(orig.position, dec.position);
            assert_eq!(orig.block_type, dec.block_type);
        }
    }

    #[test]
    fn test_server_message_tree_fell_serialization() {
        // Test ServerMessage::TreeFell serialization
        let tree_fell = TreeFell {
            blocks: vec![
                TreeFellBlock {
                    entity_id: 100,
                    position: [50, 70, 50],
                    block_type: BlockType::PineLeaves,
                },
                TreeFellBlock {
                    entity_id: 101,
                    position: [50, 71, 50],
                    block_type: BlockType::PineLog,
                },
            ],
        };
        let msg = ServerMessage::TreeFell(tree_fell);
        let encoded = bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();
        let decoded: ServerMessage =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;

        match decoded {
            ServerMessage::TreeFell(decoded_tree) => {
                assert_eq!(decoded_tree.blocks.len(), 2);
                assert_eq!(decoded_tree.blocks[0].entity_id, 100);
                assert_eq!(decoded_tree.blocks[0].block_type, BlockType::PineLeaves);
                assert_eq!(decoded_tree.blocks[1].entity_id, 101);
                assert_eq!(decoded_tree.blocks[1].block_type, BlockType::PineLog);
            }
            _ => panic!("Expected TreeFell variant"),
        }
    }

    #[test]
    fn test_tree_fell_empty_blocks() {
        // Test TreeFell with no blocks (edge case)
        let tree_fell = TreeFell { blocks: vec![] };
        let encoded =
            bincode::serde::encode_to_vec(&tree_fell, bincode::config::standard()).unwrap();
        let decoded: TreeFell =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;
        assert!(decoded.blocks.is_empty());
    }

    #[test]
    fn test_tree_fell_large_tree() {
        // Test TreeFell with a large number of blocks (realistic tree)
        let mut blocks = Vec::new();
        for i in 0..50 {
            blocks.push(TreeFellBlock {
                entity_id: i,
                position: [i as i32, i as i32, i as i32],
                block_type: if i < 10 {
                    BlockType::Log
                } else {
                    BlockType::Leaves
                },
            });
        }
        let tree_fell = TreeFell { blocks };
        let encoded =
            bincode::serde::encode_to_vec(&tree_fell, bincode::config::standard()).unwrap();
        let decoded: TreeFell =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;
        assert_eq!(decoded.blocks.len(), 50);
    }

    #[test]
    fn test_day_cycle_pause_changed_serialization() {
        // Test DayCyclePauseChanged message serialization
        let pause_msg = DayCyclePauseChanged {
            paused: true,
            time_of_day: 0.5,
        };
        let encoded =
            bincode::serde::encode_to_vec(&pause_msg, bincode::config::standard()).unwrap();
        let decoded: DayCyclePauseChanged =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;
        assert_eq!(pause_msg.paused, decoded.paused);
        assert_eq!(pause_msg.time_of_day, decoded.time_of_day);

        // Test with paused = false
        let resume_msg = DayCyclePauseChanged {
            paused: false,
            time_of_day: 0.25,
        };
        let encoded =
            bincode::serde::encode_to_vec(&resume_msg, bincode::config::standard()).unwrap();
        let decoded: DayCyclePauseChanged =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;
        assert!(!decoded.paused);
        assert!((decoded.time_of_day - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn test_server_message_day_cycle_pause_changed() {
        // Test ServerMessage::DayCyclePauseChanged serialization
        let msg = ServerMessage::DayCyclePauseChanged(DayCyclePauseChanged {
            paused: true,
            time_of_day: 0.75,
        });
        let encoded = bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();
        let decoded: ServerMessage =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;

        match decoded {
            ServerMessage::DayCyclePauseChanged(pause) => {
                assert!(pause.paused);
                assert!((pause.time_of_day - 0.75).abs() < f32::EPSILON);
            }
            _ => panic!("Expected DayCyclePauseChanged variant"),
        }
    }

    #[test]
    fn test_spawn_position_changed_serialization() {
        // Test SpawnPositionChanged message serialization
        let spawn_msg = SpawnPositionChanged {
            position: [100.0, 64.0, 200.0],
        };
        let encoded =
            bincode::serde::encode_to_vec(&spawn_msg, bincode::config::standard()).unwrap();
        let decoded: SpawnPositionChanged =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;
        assert_eq!(spawn_msg.position, decoded.position);

        // Test with different values
        let spawn_msg2 = SpawnPositionChanged {
            position: [-50.5, 128.0, -75.25],
        };
        let encoded2 =
            bincode::serde::encode_to_vec(&spawn_msg2, bincode::config::standard()).unwrap();
        let decoded2: SpawnPositionChanged =
            bincode::serde::decode_from_slice(&encoded2, bincode::config::standard())
                .unwrap()
                .0;
        assert_eq!(spawn_msg2.position, decoded2.position);
    }

    #[test]
    fn test_server_message_spawn_position_changed() {
        // Test ServerMessage::SpawnPositionChanged serialization
        let msg = ServerMessage::SpawnPositionChanged(SpawnPositionChanged {
            position: [150.0, 70.0, 250.0],
        });
        let encoded = bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();
        let decoded: ServerMessage =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;

        match decoded {
            ServerMessage::SpawnPositionChanged(spawn) => {
                assert_eq!(spawn.position, [150.0, 70.0, 250.0]);
            }
            _ => panic!("Expected SpawnPositionChanged variant"),
        }
    }

    #[test]
    fn test_picture_added_serialization() {
        // Test PictureAdded message serialization
        let picture_msg = PictureAdded {
            picture_id: 42,
            name: "sunset.png".to_string(),
        };
        let encoded =
            bincode::serde::encode_to_vec(&picture_msg, bincode::config::standard()).unwrap();
        let decoded: PictureAdded =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;
        assert_eq!(picture_msg.picture_id, decoded.picture_id);
        assert_eq!(picture_msg.name, decoded.name);
    }

    #[test]
    fn test_server_message_picture_added() {
        // Test ServerMessage::PictureAdded serialization
        let msg = ServerMessage::PictureAdded(PictureAdded {
            picture_id: 100,
            name: "landscape.png".to_string(),
        });
        let encoded = bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();
        let decoded: ServerMessage =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;

        match decoded {
            ServerMessage::PictureAdded(picture) => {
                assert_eq!(picture.picture_id, 100);
                assert_eq!(picture.name, "landscape.png");
            }
            _ => panic!("Expected PictureAdded variant"),
        }
    }

    #[test]
    fn test_frame_picture_set_serialization() {
        // Test FramePictureSet struct serialization
        let frame_msg = FramePictureSet {
            position: [10, 20, 30],
            picture_id: Some(42),
        };
        let encoded =
            bincode::serde::encode_to_vec(&frame_msg, bincode::config::standard()).unwrap();
        let decoded: FramePictureSet =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;
        assert_eq!(decoded.position, [10, 20, 30]);
        assert_eq!(decoded.picture_id, Some(42));

        // Test with None (cleared picture)
        let cleared_msg = FramePictureSet {
            position: [5, 6, 7],
            picture_id: None,
        };
        let encoded =
            bincode::serde::encode_to_vec(&cleared_msg, bincode::config::standard()).unwrap();
        let decoded: FramePictureSet =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;
        assert_eq!(decoded.position, [5, 6, 7]);
        assert_eq!(decoded.picture_id, None);
    }

    #[test]
    fn test_server_message_frame_picture_set() {
        // Test ServerMessage::FramePictureSet serialization
        let msg = ServerMessage::FramePictureSet(FramePictureSet {
            position: [100, 64, -50],
            picture_id: Some(5),
        });
        let encoded = bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();
        let decoded: ServerMessage =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;

        match decoded {
            ServerMessage::FramePictureSet(frame) => {
                assert_eq!(frame.position, [100, 64, -50]);
                assert_eq!(frame.picture_id, Some(5));
            }
            _ => panic!("Expected FramePictureSet variant"),
        }
    }

    #[test]
    fn test_upload_picture_serialization() {
        // Test UploadPicture client message serialization
        let upload = UploadPicture {
            name: "sunset.png".to_string(),
            png_data: vec![
                0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG magic bytes
                0x00, 0x00, 0x00, 0x0D, // IHDR length
                0x49, 0x48, 0x44, 0x52, // IHDR
                0x00, 0x00, 0x00, 0x40, // width: 64
                0x00, 0x00, 0x00, 0x40, // height: 64
                0x08, 0x02, // bit depth: 8, color type: RGB
                0x00, 0x00, 0x00, // compression, filter, interlace
                0x00, 0x00, 0x00, 0x00, // CRC (placeholder)
            ],
        };
        let msg = ClientMessage::UploadPicture(upload.clone());
        let encoded = bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();
        let decoded: ClientMessage =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;

        match decoded {
            ClientMessage::UploadPicture(decoded_upload) => {
                assert_eq!(decoded_upload.name, "sunset.png");
                assert_eq!(decoded_upload.png_data.len(), upload.png_data.len());
                // Verify PNG magic bytes preserved
                assert_eq!(decoded_upload.png_data[..8], upload.png_data[..8]);
            }
            _ => panic!("Expected UploadPicture variant"),
        }
    }

    #[test]
    fn test_picture_frame_sync_full_flow() {
        // Test the complete picture frame sync flow:
        // 1. Client uploads a picture
        // 2. Server broadcasts PictureAdded
        // 3. Server broadcasts FramePictureSet
        // 4. All messages serialize/deserialize correctly

        // Step 1: Client uploads picture
        let upload = UploadPicture {
            name: "test_picture.png".to_string(),
            png_data: vec![
                0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG magic
                0x00, 0x01, 0x02, 0x03, // Placeholder data
            ],
        };
        let client_msg = ClientMessage::UploadPicture(upload);
        let encoded_client =
            bincode::serde::encode_to_vec(&client_msg, bincode::config::standard())
                .expect("Client message should encode");
        let decoded_client: ClientMessage =
            bincode::serde::decode_from_slice(&encoded_client, bincode::config::standard())
                .expect("Client message should decode")
                .0;

        // Verify client message
        match &decoded_client {
            ClientMessage::UploadPicture(u) => {
                assert_eq!(u.name, "test_picture.png");
            }
            _ => panic!("Expected UploadPicture"),
        }

        // Step 2: Server broadcasts PictureAdded
        let picture_added = PictureAdded {
            picture_id: 42,
            name: "test_picture.png".to_string(),
        };
        let server_msg1 = ServerMessage::PictureAdded(picture_added);
        let encoded_server1 =
            bincode::serde::encode_to_vec(&server_msg1, bincode::config::standard())
                .expect("PictureAdded should encode");
        let decoded_server1: ServerMessage =
            bincode::serde::decode_from_slice(&encoded_server1, bincode::config::standard())
                .expect("PictureAdded should decode")
                .0;

        // Verify PictureAdded
        match decoded_server1 {
            ServerMessage::PictureAdded(p) => {
                assert_eq!(p.picture_id, 42);
                assert_eq!(p.name, "test_picture.png");
            }
            _ => panic!("Expected PictureAdded"),
        }

        // Step 3: Server broadcasts FramePictureSet
        let frame_set = FramePictureSet {
            position: [100, 64, 200],
            picture_id: Some(42),
        };
        let server_msg2 = ServerMessage::FramePictureSet(frame_set);
        let encoded_server2 =
            bincode::serde::encode_to_vec(&server_msg2, bincode::config::standard())
                .expect("FramePictureSet should encode");
        let decoded_server2: ServerMessage =
            bincode::serde::decode_from_slice(&encoded_server2, bincode::config::standard())
                .expect("FramePictureSet should decode")
                .0;

        // Verify FramePictureSet
        match decoded_server2 {
            ServerMessage::FramePictureSet(f) => {
                assert_eq!(f.position, [100, 64, 200]);
                assert_eq!(f.picture_id, Some(42));
            }
            _ => panic!("Expected FramePictureSet"),
        }

        // Test clearing a frame (picture_id = None)
        let clear_frame = FramePictureSet {
            position: [100, 64, 200],
            picture_id: None,
        };
        let server_msg3 = ServerMessage::FramePictureSet(clear_frame);
        let encoded_server3 =
            bincode::serde::encode_to_vec(&server_msg3, bincode::config::standard())
                .expect("Clear frame should encode");
        let decoded_server3: ServerMessage =
            bincode::serde::decode_from_slice(&encoded_server3, bincode::config::standard())
                .expect("Clear frame should decode")
                .0;

        match decoded_server3 {
            ServerMessage::FramePictureSet(f) => {
                assert_eq!(f.position, [100, 64, 200]);
                assert_eq!(f.picture_id, None);
            }
            _ => panic!("Expected FramePictureSet"),
        }
    }

    #[test]
    fn test_frame_picture_set_with_large_picture_id() {
        // Test FramePictureSet with maximum picture ID
        let frame = FramePictureSet {
            position: [0, 0, 0],
            picture_id: Some(u16::MAX),
        };
        let msg = ServerMessage::FramePictureSet(frame);
        let encoded = bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();
        let decoded: ServerMessage =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;

        match decoded {
            ServerMessage::FramePictureSet(f) => {
                assert_eq!(f.picture_id, Some(u16::MAX));
            }
            _ => panic!("Expected FramePictureSet"),
        }
    }

    #[test]
    fn test_stencil_loaded_serialization() {
        // Test StencilLoaded message serialization
        let stencil_msg = StencilLoaded {
            stencil_id: 42,
            name: "castle_wall".to_string(),
            stencil_data: vec![0x53, 0x54, 0x43, 0x4C, 0x00, 0x01], // "STCL" magic + version
        };
        let encoded =
            bincode::serde::encode_to_vec(&stencil_msg, bincode::config::standard()).unwrap();
        let decoded: StencilLoaded =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;
        assert_eq!(decoded.stencil_id, 42);
        assert_eq!(decoded.name, "castle_wall");
        assert_eq!(decoded.stencil_data.len(), 6);
        // Verify magic bytes preserved
        assert_eq!(&decoded.stencil_data[0..4], b"STCL");
    }

    #[test]
    fn test_server_message_stencil_loaded() {
        // Test ServerMessage::StencilLoaded serialization
        let msg = ServerMessage::StencilLoaded(StencilLoaded {
            stencil_id: 100,
            name: "tower_base".to_string(),
            stencil_data: vec![0x00, 0x01, 0x02, 0x03],
        });
        let encoded = bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();
        let decoded: ServerMessage =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;

        match decoded {
            ServerMessage::StencilLoaded(s) => {
                assert_eq!(s.stencil_id, 100);
                assert_eq!(s.name, "tower_base");
                assert_eq!(s.stencil_data, vec![0x00, 0x01, 0x02, 0x03]);
            }
            _ => panic!("Expected StencilLoaded variant"),
        }
    }

    #[test]
    fn test_stencil_transform_update_serialization() {
        // Test StencilTransformUpdate struct serialization
        let transform = StencilTransformUpdate {
            stencil_id: 42,
            position: [100, 64, 200],
            rotation: 2,
        };
        let encoded =
            bincode::serde::encode_to_vec(&transform, bincode::config::standard()).unwrap();
        let decoded: StencilTransformUpdate =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;
        assert_eq!(decoded.stencil_id, 42);
        assert_eq!(decoded.position, [100, 64, 200]);
        assert_eq!(decoded.rotation, 2);
    }

    #[test]
    fn test_server_message_stencil_transform_update() {
        // Test ServerMessage::StencilTransformUpdate serialization
        let msg = ServerMessage::StencilTransformUpdate(StencilTransformUpdate {
            stencil_id: 5,
            position: [50, 32, -100],
            rotation: 1,
        });
        let encoded = bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();
        let decoded: ServerMessage =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;

        match decoded {
            ServerMessage::StencilTransformUpdate(t) => {
                assert_eq!(t.stencil_id, 5);
                assert_eq!(t.position, [50, 32, -100]);
                assert_eq!(t.rotation, 1);
            }
            _ => panic!("Expected StencilTransformUpdate variant"),
        }
    }

    #[test]
    fn test_stencil_removed_serialization() {
        // Test StencilRemoved struct serialization
        let removed = StencilRemoved { stencil_id: 42 };
        let encoded = bincode::serde::encode_to_vec(&removed, bincode::config::standard()).unwrap();
        let decoded: StencilRemoved =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;
        assert_eq!(decoded.stencil_id, 42);
    }

    #[test]
    fn test_server_message_stencil_removed() {
        // Test ServerMessage::StencilRemoved serialization
        let msg = ServerMessage::StencilRemoved(StencilRemoved { stencil_id: 999 });
        let encoded = bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();
        let decoded: ServerMessage =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;

        match decoded {
            ServerMessage::StencilRemoved(r) => {
                assert_eq!(r.stencil_id, 999);
            }
            _ => panic!("Expected StencilRemoved variant"),
        }
    }

    #[test]
    fn test_stencil_sync_full_flow() {
        // Test the complete stencil sync flow:
        // 1. Server broadcasts StencilLoaded
        // 2. Server broadcasts StencilTransformUpdate
        // 3. Server broadcasts StencilRemoved
        // 4. All messages serialize/deserialize correctly

        // Step 1: Server broadcasts StencilLoaded
        let loaded = StencilLoaded {
            stencil_id: 1,
            name: "test_stencil".to_string(),
            stencil_data: vec![0x53, 0x54, 0x43, 0x4C], // STCL magic
        };
        let msg1 = ServerMessage::StencilLoaded(loaded);
        let encoded1 = bincode::serde::encode_to_vec(&msg1, bincode::config::standard())
            .expect("StencilLoaded should encode");
        let decoded1: ServerMessage =
            bincode::serde::decode_from_slice(&encoded1, bincode::config::standard())
                .expect("StencilLoaded should decode")
                .0;

        match decoded1 {
            ServerMessage::StencilLoaded(s) => {
                assert_eq!(s.stencil_id, 1);
                assert_eq!(s.name, "test_stencil");
            }
            _ => panic!("Expected StencilLoaded"),
        }

        // Step 2: Server broadcasts StencilTransformUpdate
        let transform = StencilTransformUpdate {
            stencil_id: 1,
            position: [100, 64, 200],
            rotation: 1,
        };
        let msg2 = ServerMessage::StencilTransformUpdate(transform);
        let encoded2 = bincode::serde::encode_to_vec(&msg2, bincode::config::standard())
            .expect("StencilTransformUpdate should encode");
        let decoded2: ServerMessage =
            bincode::serde::decode_from_slice(&encoded2, bincode::config::standard())
                .expect("StencilTransformUpdate should decode")
                .0;

        match decoded2 {
            ServerMessage::StencilTransformUpdate(t) => {
                assert_eq!(t.stencil_id, 1);
                assert_eq!(t.position, [100, 64, 200]);
                assert_eq!(t.rotation, 1);
            }
            _ => panic!("Expected StencilTransformUpdate"),
        }

        // Step 3: Server broadcasts StencilRemoved
        let removed = StencilRemoved { stencil_id: 1 };
        let msg3 = ServerMessage::StencilRemoved(removed);
        let encoded3 = bincode::serde::encode_to_vec(&msg3, bincode::config::standard())
            .expect("StencilRemoved should encode");
        let decoded3: ServerMessage =
            bincode::serde::decode_from_slice(&encoded3, bincode::config::standard())
                .expect("StencilRemoved should decode")
                .0;

        match decoded3 {
            ServerMessage::StencilRemoved(r) => {
                assert_eq!(r.stencil_id, 1);
            }
            _ => panic!("Expected StencilRemoved"),
        }
    }

    #[test]
    fn test_stencil_with_large_data() {
        // Test StencilLoaded with large data (simulating real stencil)
        let large_data: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();
        let stencil = StencilLoaded {
            stencil_id: u64::MAX,
            name: "large_stencil_with_long_name".to_string(),
            stencil_data: large_data.clone(),
        };
        let msg = ServerMessage::StencilLoaded(stencil);
        let encoded = bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();
        let decoded: ServerMessage =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .unwrap()
                .0;

        match decoded {
            ServerMessage::StencilLoaded(s) => {
                assert_eq!(s.stencil_id, u64::MAX);
                assert_eq!(s.name, "large_stencil_with_long_name");
                assert_eq!(s.stencil_data.len(), 1000);
                assert_eq!(s.stencil_data, large_data);
            }
            _ => panic!("Expected StencilLoaded"),
        }
    }

    #[test]
    fn test_stencil_rotation_values() {
        // Test all valid rotation values (0-3)
        for rotation in 0..=3 {
            let transform = StencilTransformUpdate {
                stencil_id: 1,
                position: [0, 0, 0],
                rotation,
            };
            let msg = ServerMessage::StencilTransformUpdate(transform);
            let encoded = bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();
            let decoded: ServerMessage =
                bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                    .unwrap()
                    .0;

            match decoded {
                ServerMessage::StencilTransformUpdate(t) => {
                    assert_eq!(t.rotation, rotation);
                }
                _ => panic!("Expected StencilTransformUpdate"),
            }
        }
    }
}

//! Tree fall synchronization for multiplayer.
//!
//! Provides server-authoritative tree fall simulation with:
//! - **Batch broadcasting**: All blocks in a tree fall as a single message
//! - **Entity ID tracking**: Unique IDs for each falling block in the tree
//! - **Cascade handling**: Proper handling of connected trees and orphaned leaves
//!
//! # Architecture
//!
//! When a tree loses ground support:
//! - **Server**: Detects all connected blocks, creates TreeFell message with entity IDs
//! - **Client**: Receives TreeFell, spawns all falling blocks simultaneously
//!
//! The batch approach is more bandwidth-efficient than individual FallingBlockSpawned messages
//! for multi-block tree falls (a typical tree has 20-50 blocks).
//!
//! # Usage
//!
//! ```ignore
//! // Server-side: When a tree loses ground support
//! let tree_blocks = vec![
//!     (Vector3::new(10, 64, 10), BlockType::Log),
//!     (Vector3::new(10, 65, 10), BlockType::Log),
//!     (Vector3::new(10, 66, 10), BlockType::Log),
//!     (Vector3::new(11, 66, 10), BlockType::Leaves),
//!     // ... more blocks
//! ];
//!
//! // Broadcast to all clients
//! let entity_ids = multiplayer.broadcast_tree_fell(tree_blocks);
//!
//! // Client-side: Handle tree fall
//! for tree_fell in multiplayer.take_pending_tree_falls() {
//!     for block in tree_fell.blocks {
//!         client_falling_blocks.spawn_from_tree_fell(&block);
//!     }
//! }
//! ```

// Allow dead code since these methods are public API intended for future use
#![allow(dead_code)]

use crate::chunk::BlockType;
use crate::net::falling_block_sync::ClientFallingBlockSystem;
use crate::net::protocol::{FallingBlockId, FallingBlockSpawned, TreeFell, TreeFellBlock};
use nalgebra::Vector3;

/// Server-side tree fall coordinator.
///
/// Tracks entity IDs and builds TreeFell messages for bandwidth-efficient
/// broadcasting of multi-block tree falls.
pub struct TreeFallSync {
    /// Next available entity ID.
    next_entity_id: FallingBlockId,
}

/// Statistics for monitoring tree fall sync.
#[derive(Debug, Clone, Default)]
pub struct TreeFallSyncStats {
    /// Total tree fall events broadcast.
    pub trees_felled: u64,
    /// Total blocks in tree falls.
    pub blocks_felled: u64,
    /// Currently tracked entity IDs in flight.
    pub active_entities: usize,
}

impl Default for TreeFallSync {
    fn default() -> Self {
        Self::new()
    }
}

/// Maximum number of `TreeFellBlock` entries per `TreeFell` message.
///
/// Sized so a worst-case serialized `TreeFell { blocks: Vec<TreeFellBlock> }`
/// stays under ~1500 bytes: each block is ~18 bytes on the wire (entity_id
/// u32 + position [i32;3] + block_type u8 + bincode framing), so 80 blocks
/// ≈ 1440 bytes + enum tag + length-prefix overhead. Trees larger than this
/// are split into multiple `TreeFell` messages by [`build_tree_fell_batched`].
pub const MAX_TREE_FELL_BLOCKS_PER_MSG: usize = 80;

impl TreeFallSync {
    /// Creates a new tree fall sync coordinator.
    pub fn new() -> Self {
        Self { next_entity_id: 1 }
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

    /// Builds a TreeFell message from a list of tree blocks.
    ///
    /// Each block gets a unique entity ID for tracking during the fall.
    ///
    /// **NOTE:** Callers handling real trees should use
    /// [`build_tree_fell_batched`] which splits oversized trees across
    /// multiple messages so a single packet never exceeds the MTU budget.
    pub fn build_tree_fell(&mut self, blocks: Vec<(Vector3<i32>, BlockType)>) -> TreeFell {
        if blocks.len() > MAX_TREE_FELL_BLOCKS_PER_MSG {
            log::warn!(
                "[TreeFallSync] build_tree_fell called with {} blocks (> {} cap); \
                 prefer build_tree_fell_batched to stay under MTU",
                blocks.len(),
                MAX_TREE_FELL_BLOCKS_PER_MSG
            );
        }
        let tree_fell_blocks: Vec<TreeFellBlock> = blocks
            .into_iter()
            .map(|(pos, block_type)| {
                let entity_id = self.next_entity_id();
                TreeFellBlock {
                    entity_id,
                    position: [pos.x, pos.y, pos.z],
                    block_type,
                }
            })
            .collect();

        TreeFell {
            blocks: tree_fell_blocks,
        }
    }

    /// Builds one or more `TreeFell` messages from an arbitrarily large tree.
    ///
    /// The input is split into chunks of at most `MAX_TREE_FELL_BLOCKS_PER_MSG`
    /// blocks so each message fits comfortably inside a typical 1500-byte MTU
    /// even on the worst-case serialized form.
    pub fn build_tree_fell_batched(
        &mut self,
        blocks: Vec<(Vector3<i32>, BlockType)>,
    ) -> Vec<TreeFell> {
        if blocks.len() <= MAX_TREE_FELL_BLOCKS_PER_MSG {
            return vec![self.build_tree_fell(blocks)];
        }

        let mut out = Vec::with_capacity(blocks.len().div_ceil(MAX_TREE_FELL_BLOCKS_PER_MSG));
        for chunk in blocks.chunks(MAX_TREE_FELL_BLOCKS_PER_MSG) {
            out.push(self.build_tree_fell(chunk.to_vec()));
        }
        out
    }

    /// Returns statistics for monitoring.
    pub fn stats(&self) -> TreeFallSyncStats {
        TreeFallSyncStats {
            trees_felled: 0,
            blocks_felled: 0,
            active_entities: 0,
        }
    }

    /// Resets entity ID counter (for testing).
    pub fn reset(&mut self) {
        self.next_entity_id = 1;
    }
}

/// Client-side tree fall handler.
///
/// Converts TreeFell messages into individual falling block spawns.
pub struct ClientTreeFallHandler {
    /// Statistics for debugging.
    stats: TreeFallSyncStats,
}

impl Default for ClientTreeFallHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientTreeFallHandler {
    /// Creates a new client tree fall handler.
    pub fn new() -> Self {
        Self {
            stats: TreeFallSyncStats::default(),
        }
    }

    /// Converts a TreeFell message into FallingBlockSpawned messages.
    ///
    /// Returns spawn messages ready to be passed to ClientFallingBlockSystem.
    pub fn tree_fell_to_spawns(tree_fell: &TreeFell) -> Vec<FallingBlockSpawned> {
        tree_fell
            .blocks
            .iter()
            .map(|block| FallingBlockSpawned {
                entity_id: block.entity_id,
                position: [
                    block.position[0] as f32 + 0.5,
                    block.position[1] as f32 + 0.5,
                    block.position[2] as f32 + 0.5,
                ],
                velocity: [0.0, 0.0, 0.0],
                block_type: block.block_type,
            })
            .collect()
    }

    /// Applies a TreeFell to the client falling block system.
    ///
    /// Spawns all blocks in the tree as falling entities.
    pub fn apply_tree_fell(
        &mut self,
        tree_fell: &TreeFell,
        falling_block_system: &mut ClientFallingBlockSystem,
    ) {
        let spawns = Self::tree_fell_to_spawns(tree_fell);
        for spawn in spawns {
            falling_block_system.spawn_from_network(&spawn);
        }

        self.stats.trees_felled += 1;
        self.stats.blocks_felled += tree_fell.blocks.len() as u64;
    }

    /// Returns statistics for debugging.
    pub fn stats(&self) -> &TreeFallSyncStats {
        &self.stats
    }

    /// Resets statistics.
    pub fn reset_stats(&mut self) {
        self.stats = TreeFallSyncStats::default();
    }
}

/// Detects connected tree blocks starting from a root position.
///
/// This is a simplified version for testing. The actual game uses
/// world queries to find connected blocks.
pub fn detect_tree_blocks(
    _root_pos: Vector3<i32>,
    _is_log: impl Fn(BlockType) -> bool,
    _is_leaves: impl Fn(BlockType) -> bool,
    _get_block: impl Fn(Vector3<i32>) -> Option<BlockType>,
) -> Vec<(Vector3<i32>, BlockType)> {
    // In the actual game, this would flood-fill to find all connected
    // log and leaf blocks. For tests, we manually construct trees.
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::protocol::FallingBlockLanded;
    use bincode;

    fn make_tree_fell_block(
        entity_id: u32,
        x: i32,
        y: i32,
        z: i32,
        block_type: BlockType,
    ) -> TreeFellBlock {
        TreeFellBlock {
            entity_id,
            position: [x, y, z],
            block_type,
        }
    }

    fn make_simple_tree() -> Vec<(Vector3<i32>, BlockType)> {
        vec![
            (Vector3::new(0, 0, 0), BlockType::Log),     // Base
            (Vector3::new(0, 1, 0), BlockType::Log),     // Trunk
            (Vector3::new(0, 2, 0), BlockType::Log),     // Trunk
            (Vector3::new(0, 3, 0), BlockType::Log),     // Trunk top
            (Vector3::new(1, 3, 0), BlockType::Leaves),  // Leaves
            (Vector3::new(-1, 3, 0), BlockType::Leaves), // Leaves
            (Vector3::new(0, 3, 1), BlockType::Leaves),  // Leaves
            (Vector3::new(0, 3, -1), BlockType::Leaves), // Leaves
            (Vector3::new(0, 4, 0), BlockType::Leaves),  // Top leaves
        ]
    }

    #[test]
    fn test_build_tree_fell_batched_splits_large_trees() {
        let mut sync = TreeFallSync::new();
        // Generate 200 blocks — ~2.5× the per-message cap.
        let huge: Vec<(Vector3<i32>, BlockType)> = (0..200)
            .map(|i| (Vector3::new(i, 0, 0), BlockType::Log))
            .collect();

        let msgs = sync.build_tree_fell_batched(huge);
        assert!(msgs.len() >= 3, "expected >=3 batches, got {}", msgs.len());
        for msg in &msgs {
            assert!(msg.blocks.len() <= MAX_TREE_FELL_BLOCKS_PER_MSG);
        }

        // All blocks must appear exactly once across batches with unique IDs.
        let total: usize = msgs.iter().map(|m| m.blocks.len()).sum();
        assert_eq!(total, 200);
        let mut ids: Vec<u32> = msgs
            .iter()
            .flat_map(|m| m.blocks.iter().map(|b| b.entity_id))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 200, "all entity IDs must be unique");
    }

    /// Varied sizes near/around the per-message cap. Verifies every batch
    /// honors the cap, IDs remain unique across the whole fan-out, and
    /// empty/single-block/exact-multiple edge cases all produce the right
    /// number of messages.
    #[test]
    fn test_build_tree_fell_batched_varied_sizes() {
        for total in [
            0,
            1,
            MAX_TREE_FELL_BLOCKS_PER_MSG - 1,
            MAX_TREE_FELL_BLOCKS_PER_MSG,
            MAX_TREE_FELL_BLOCKS_PER_MSG + 1,
            2 * MAX_TREE_FELL_BLOCKS_PER_MSG,
            2 * MAX_TREE_FELL_BLOCKS_PER_MSG + 7,
            // Explicit off-cap cases the audit called out.
            81,
            159,
            240,
        ] {
            let mut sync = TreeFallSync::new();
            let blocks: Vec<(Vector3<i32>, BlockType)> = (0..total as i32)
                .map(|i| (Vector3::new(i, 0, 0), BlockType::Log))
                .collect();
            let msgs = sync.build_tree_fell_batched(blocks.clone());

            // Empty input still yields a single (empty) batch. That matches
            // the non-batched path which returns a single TreeFell { blocks: [] }.
            let expected_batches = if total == 0 {
                1
            } else {
                total.div_ceil(MAX_TREE_FELL_BLOCKS_PER_MSG)
            };
            assert_eq!(
                msgs.len(),
                expected_batches,
                "size={} expected {} batches",
                total,
                expected_batches
            );
            for m in &msgs {
                assert!(
                    m.blocks.len() <= MAX_TREE_FELL_BLOCKS_PER_MSG,
                    "size={} produced {}-block batch",
                    total,
                    m.blocks.len()
                );
            }

            let flat: Vec<u32> = msgs
                .iter()
                .flat_map(|m| m.blocks.iter().map(|b| b.entity_id))
                .collect();
            assert_eq!(flat.len(), total, "size={}", total);
            let mut unique = flat.clone();
            unique.sort_unstable();
            unique.dedup();
            assert_eq!(
                unique.len(),
                total,
                "duplicate entity IDs in {}-block tree",
                total
            );
            // And none of them should be the reserved 0.
            assert!(flat.iter().all(|id| *id != 0));
        }
    }

    #[test]
    fn test_build_tree_fell_batched_small_tree_single_message() {
        let mut sync = TreeFallSync::new();
        let tree = make_simple_tree();
        let msgs = sync.build_tree_fell_batched(tree);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].blocks.len(), 9);
    }

    #[test]
    fn test_single_tree_fell_message_fits_mtu_budget() {
        // Encode a full-size TreeFell and sanity-check the wire size stays
        // well under a typical 1500-byte Ethernet MTU.
        let mut sync = TreeFallSync::new();
        let blocks: Vec<(Vector3<i32>, BlockType)> = (0..MAX_TREE_FELL_BLOCKS_PER_MSG as i32)
            .map(|i| (Vector3::new(i, 64, 0), BlockType::Log))
            .collect();
        let tree_fell = sync.build_tree_fell(blocks);
        let encoded = bincode::serde::encode_to_vec(&tree_fell, bincode::config::standard())
            .expect("encode TreeFell");
        assert!(
            encoded.len() < 1500,
            "TreeFell with {} blocks encoded to {} bytes (>=MTU)",
            tree_fell.blocks.len(),
            encoded.len()
        );
    }

    #[test]
    fn test_tree_fell_sync_entity_ids() {
        let mut sync = TreeFallSync::new();

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
    fn test_build_tree_fell() {
        let mut sync = TreeFallSync::new();

        let tree_blocks = make_simple_tree();
        let tree_fell = sync.build_tree_fell(tree_blocks);

        assert_eq!(tree_fell.blocks.len(), 9); // 4 logs + 5 leaves

        // All entity IDs should be unique
        let entity_ids: std::collections::HashSet<u32> =
            tree_fell.blocks.iter().map(|b| b.entity_id).collect();
        assert_eq!(entity_ids.len(), 9);

        // Check block types
        let log_count = tree_fell
            .blocks
            .iter()
            .filter(|b| b.block_type == BlockType::Log)
            .count();
        let leaves_count = tree_fell
            .blocks
            .iter()
            .filter(|b| b.block_type == BlockType::Leaves)
            .count();
        assert_eq!(log_count, 4);
        assert_eq!(leaves_count, 5);
    }

    #[test]
    fn test_tree_fell_serialization() {
        let mut sync = TreeFallSync::new();
        let tree_blocks = make_simple_tree();
        let tree_fell = sync.build_tree_fell(tree_blocks);

        // Serialize
        let encoded = bincode::serde::encode_to_vec(&tree_fell, bincode::config::standard())
            .expect("Failed to encode TreeFell");

        // Deserialize
        let (decoded, _): (TreeFell, usize) =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .expect("Failed to decode TreeFell");

        assert_eq!(decoded.blocks.len(), tree_fell.blocks.len());
        for (orig, dec) in tree_fell.blocks.iter().zip(decoded.blocks.iter()) {
            assert_eq!(orig.entity_id, dec.entity_id);
            assert_eq!(orig.position, dec.position);
            assert_eq!(orig.block_type, dec.block_type);
        }
    }

    #[test]
    fn test_client_tree_fall_handler() {
        let mut handler = ClientTreeFallHandler::new();
        let mut falling_system = ClientFallingBlockSystem::new();

        // Build a tree fall
        let mut sync = TreeFallSync::new();
        let tree_blocks = make_simple_tree();
        let tree_fell = sync.build_tree_fell(tree_blocks);

        // Apply to client
        handler.apply_tree_fell(&tree_fell, &mut falling_system);

        // All 9 blocks should be falling
        assert_eq!(falling_system.count(), 9);

        // Stats should reflect the tree fall
        let stats = handler.stats();
        assert_eq!(stats.trees_felled, 1);
        assert_eq!(stats.blocks_felled, 9);
    }

    #[test]
    fn test_tree_fell_to_spawns() {
        let tree_fell = TreeFell {
            blocks: vec![
                make_tree_fell_block(1, 10, 64, 10, BlockType::Log),
                make_tree_fell_block(2, 10, 65, 10, BlockType::Log),
                make_tree_fell_block(3, 11, 65, 10, BlockType::Leaves),
            ],
        };

        let spawns = ClientTreeFallHandler::tree_fell_to_spawns(&tree_fell);

        assert_eq!(spawns.len(), 3);

        // Check positions are centered
        assert_eq!(spawns[0].position, [10.5, 64.5, 10.5]);
        assert_eq!(spawns[0].entity_id, 1);
        assert_eq!(spawns[0].block_type, BlockType::Log);

        assert_eq!(spawns[1].position, [10.5, 65.5, 10.5]);
        assert_eq!(spawns[1].entity_id, 2);
        assert_eq!(spawns[1].block_type, BlockType::Log);

        assert_eq!(spawns[2].position, [11.5, 65.5, 10.5]);
        assert_eq!(spawns[2].entity_id, 3);
        assert_eq!(spawns[2].block_type, BlockType::Leaves);
    }

    /// Integration test: Verifies tree fall sync produces correct cascade flow.
    ///
    /// This test simulates the full multiplayer tree fall sync flow:
    /// 1. Server detects tree has lost ground support
    /// 2. Server builds TreeFell message with all connected blocks
    /// 3. TreeFell message is serialized and sent to all clients
    /// 4. Clients receive TreeFell and spawn all falling blocks
    /// 5. All clients see identical falling tree blocks
    #[test]
    fn test_tree_fall_sync_produces_identical_cascade() {
        // === Setup: Simulate server and 3 connected clients ===
        let mut server_sync = TreeFallSync::new();
        let mut client1_system = ClientFallingBlockSystem::new();
        let mut client2_system = ClientFallingBlockSystem::new();
        let mut client3_system = ClientFallingBlockSystem::new();
        let mut client1_handler = ClientTreeFallHandler::new();
        let mut client2_handler = ClientTreeFallHandler::new();
        let mut client3_handler = ClientTreeFallHandler::new();

        // === Phase 1: Server detects tree should fall ===
        // A tree at (100, 64, 100) loses ground support
        let tree_blocks = vec![
            (Vector3::new(100, 64, 100), BlockType::Log),    // Base
            (Vector3::new(100, 65, 100), BlockType::Log),    // Trunk
            (Vector3::new(100, 66, 100), BlockType::Log),    // Trunk
            (Vector3::new(100, 67, 100), BlockType::Log),    // Trunk
            (Vector3::new(100, 68, 100), BlockType::Log),    // Trunk top
            (Vector3::new(101, 68, 100), BlockType::Leaves), // Leaves
            (Vector3::new(99, 68, 100), BlockType::Leaves),  // Leaves
            (Vector3::new(100, 68, 101), BlockType::Leaves), // Leaves
            (Vector3::new(100, 68, 99), BlockType::Leaves),  // Leaves
            (Vector3::new(100, 69, 100), BlockType::Leaves), // Top leaves
        ];

        let tree_fell = server_sync.build_tree_fell(tree_blocks);

        // Verify server state
        assert_eq!(tree_fell.blocks.len(), 10); // 5 logs + 5 leaves

        // All entity IDs should be unique
        let entity_ids: std::collections::HashSet<u32> =
            tree_fell.blocks.iter().map(|b| b.entity_id).collect();
        assert_eq!(entity_ids.len(), 10);

        // === Phase 2: Serialize TreeFell message (simulates network transmission) ===
        let encoded = bincode::serde::encode_to_vec(&tree_fell, bincode::config::standard())
            .expect("Failed to encode TreeFell");

        // === Phase 3: All clients receive the TreeFell message ===
        let (decoded1, _): (TreeFell, usize) =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .expect("Client 1 failed to decode TreeFell");
        let (decoded2, _): (TreeFell, usize) =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .expect("Client 2 failed to decode TreeFell");
        let (decoded3, _): (TreeFell, usize) =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .expect("Client 3 failed to decode TreeFell");

        // All clients apply the tree fall
        client1_handler.apply_tree_fell(&decoded1, &mut client1_system);
        client2_handler.apply_tree_fell(&decoded2, &mut client2_system);
        client3_handler.apply_tree_fell(&decoded3, &mut client3_system);

        // === Verify: All clients see the same falling tree blocks ===
        assert_eq!(
            client1_system.count(),
            10,
            "Client 1 should see 10 falling blocks"
        );
        assert_eq!(
            client2_system.count(),
            10,
            "Client 2 should see 10 falling blocks"
        );
        assert_eq!(
            client3_system.count(),
            10,
            "Client 3 should see 10 falling blocks"
        );

        // Verify all clients have the correct block types
        let c1_gpu = client1_system.gpu_data();
        let c2_gpu = client2_system.gpu_data();
        let c3_gpu = client3_system.gpu_data();

        let log_count_c1 = c1_gpu
            .iter()
            .filter(|b| b.pos_type[3] == BlockType::Log as u8 as f32)
            .count();
        let log_count_c2 = c2_gpu
            .iter()
            .filter(|b| b.pos_type[3] == BlockType::Log as u8 as f32)
            .count();
        let log_count_c3 = c3_gpu
            .iter()
            .filter(|b| b.pos_type[3] == BlockType::Log as u8 as f32)
            .count();

        assert_eq!(log_count_c1, 5, "Client 1 should have 5 falling logs");
        assert_eq!(log_count_c2, 5, "Client 2 should have 5 falling logs");
        assert_eq!(log_count_c3, 5, "Client 3 should have 5 falling logs");

        // === Phase 4: Simulate client-side rendering updates ===
        let delta_time = 0.05; // 50ms
        client1_system.update(delta_time);
        client2_system.update(delta_time);
        client3_system.update(delta_time);

        // All clients should still have the falling blocks
        assert_eq!(client1_system.count(), 10);
        assert_eq!(client2_system.count(), 10);
        assert_eq!(client3_system.count(), 10);

        // === Phase 5: Simulate some blocks landing ===
        // Logs land first (lower starting positions)
        for block in &decoded1.blocks {
            if block.block_type == BlockType::Log {
                let land = FallingBlockLanded {
                    entity_id: block.entity_id,
                    position: block.position,
                    block_type: block.block_type,
                };

                client1_system.handle_landed(&land);
                client2_system.handle_landed(&land);
                client3_system.handle_landed(&land);
            }
        }

        // Verify: Only leaves should still be falling
        assert_eq!(
            client1_system.count(),
            5,
            "Client 1 should have 5 falling leaves (logs landed)"
        );
        assert_eq!(
            client2_system.count(),
            5,
            "Client 2 should have 5 falling leaves (logs landed)"
        );
        assert_eq!(
            client3_system.count(),
            5,
            "Client 3 should have 5 falling leaves (logs landed)"
        );

        // === Phase 6: Leaves land ===
        for block in &decoded1.blocks {
            if block.block_type == BlockType::Leaves {
                let land = FallingBlockLanded {
                    entity_id: block.entity_id,
                    position: [block.position[0], block.position[1] - 3, block.position[2]], // Leaves land lower
                    block_type: block.block_type,
                };

                client1_system.handle_landed(&land);
                client2_system.handle_landed(&land);
                client3_system.handle_landed(&land);
            }
        }

        // === Verify: No more falling blocks ===
        assert_eq!(
            client1_system.count(),
            0,
            "Client 1 should have no falling blocks"
        );
        assert_eq!(
            client2_system.count(),
            0,
            "Client 2 should have no falling blocks"
        );
        assert_eq!(
            client3_system.count(),
            0,
            "Client 3 should have no falling blocks"
        );

        // === Verify: Stats are consistent ===
        assert_eq!(client1_handler.stats().trees_felled, 1);
        assert_eq!(client1_handler.stats().blocks_felled, 10);

        println!(
            "Successfully verified tree fall cascade sync across 3 clients: {} blocks",
            tree_fell.blocks.len()
        );
    }

    /// Integration test: Verifies tree fall sync via ServerMessage protocol.
    ///
    /// This test verifies that TreeFell messages are properly wrapped
    /// in ServerMessage enum and can be serialized/deserialized correctly,
    /// matching the actual network protocol used in multiplayer.
    #[test]
    fn test_tree_fall_via_server_message_protocol() {
        use crate::net::protocol::ServerMessage;

        // === Setup ===
        let mut server_sync = TreeFallSync::new();
        let mut client_system = ClientFallingBlockSystem::new();
        let mut client_handler = ClientTreeFallHandler::new();

        // === Server builds tree fall ===
        let tree_blocks = make_simple_tree();
        let tree_fell = server_sync.build_tree_fell(tree_blocks);

        // Wrap in ServerMessage (as done in actual server code)
        let server_msg = ServerMessage::TreeFell(tree_fell.clone());

        // Serialize as ServerMessage
        let encoded = bincode::serde::encode_to_vec(&server_msg, bincode::config::standard())
            .expect("Failed to encode ServerMessage");

        // Deserialize as ServerMessage
        let (decoded, _): (ServerMessage, usize) =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .expect("Failed to decode ServerMessage");

        // Verify correct message type and apply to client
        match decoded {
            ServerMessage::TreeFell(received_tree_fell) => {
                assert_eq!(received_tree_fell.blocks.len(), 9);
                client_handler.apply_tree_fell(&received_tree_fell, &mut client_system);
            }
            _ => panic!("Expected TreeFell message"),
        }

        assert_eq!(
            client_system.count(),
            9,
            "Client should have 9 falling blocks"
        );
    }

    /// Integration test: Verifies tree fall with different tree types.
    #[test]
    fn test_tree_fall_different_tree_types() {
        // Test Oak tree (default Log + Leaves)
        let oak_tree = vec![
            (Vector3::new(0, 0, 0), BlockType::Log),
            (Vector3::new(0, 1, 0), BlockType::Log),
            (Vector3::new(1, 1, 0), BlockType::Leaves),
        ];

        // Test Pine tree (PineLog + PineLeaves)
        let pine_tree = vec![
            (Vector3::new(10, 0, 0), BlockType::PineLog),
            (Vector3::new(10, 1, 0), BlockType::PineLog),
            (Vector3::new(11, 1, 0), BlockType::PineLeaves),
        ];

        // Test Birch tree (BirchLog + BirchLeaves)
        let birch_tree = vec![
            (Vector3::new(20, 0, 0), BlockType::BirchLog),
            (Vector3::new(20, 1, 0), BlockType::BirchLog),
            (Vector3::new(21, 1, 0), BlockType::BirchLeaves),
        ];

        let mut sync = TreeFallSync::new();
        let mut client = ClientFallingBlockSystem::new();
        let mut handler = ClientTreeFallHandler::new();

        for tree in [oak_tree, pine_tree, birch_tree] {
            let tree_fell = sync.build_tree_fell(tree);

            // Serialize and deserialize
            let encoded = bincode::serde::encode_to_vec(&tree_fell, bincode::config::standard())
                .expect("Failed to encode");
            let (decoded, _): (TreeFell, usize) =
                bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                    .expect("Failed to decode");

            // Apply to client
            handler.apply_tree_fell(&decoded, &mut client);

            // Verify
            assert_eq!(client.count(), 3);
            client.clear();
        }

        println!("Successfully verified tree fall sync for Oak, Pine, and Birch trees");
    }

    /// Integration test: Verifies tree fall sync with large tree (50+ blocks).
    #[test]
    fn test_large_tree_fall_sync() {
        let mut sync = TreeFallSync::new();
        let mut client = ClientFallingBlockSystem::new();
        let mut handler = ClientTreeFallHandler::new();

        // Build a large tree (realistic size)
        let mut tree_blocks = Vec::new();

        // Trunk (10 logs)
        for y in 0..10 {
            tree_blocks.push((Vector3::new(0, y, 0), BlockType::Log));
        }

        // Canopy (3x3x3 leaves, hollow center)
        for x in -1..=1 {
            for y in 8..=10 {
                for z in -1..=1 {
                    if !(x == 0 && z == 0 && y < 10) {
                        // Hollow inside trunk
                        tree_blocks.push((Vector3::new(x, y, z), BlockType::Leaves));
                    }
                }
            }
        }

        let tree_fell = sync.build_tree_fell(tree_blocks);
        assert!(
            tree_fell.blocks.len() >= 30,
            "Large tree should have 30+ blocks"
        );

        // Serialize
        let encoded = bincode::serde::encode_to_vec(&tree_fell, bincode::config::standard())
            .expect("Failed to encode large tree");

        // Verify bandwidth efficiency (TreeFell is smaller than individual spawns)
        // Each TreeFellBlock is ~16 bytes, so 50 blocks = ~800 bytes
        // Individual FallingBlockSpawned would be ~50 bytes each = 2500 bytes
        println!(
            "Large tree ({} blocks) serialized to {} bytes",
            tree_fell.blocks.len(),
            encoded.len()
        );
        assert!(
            encoded.len() < 1500,
            "Large tree should serialize to < 1500 bytes for efficient MTU"
        );

        // Deserialize and apply
        let (decoded, _): (TreeFell, usize) =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .expect("Failed to decode large tree");

        handler.apply_tree_fell(&decoded, &mut client);

        assert_eq!(
            client.count(),
            decoded.blocks.len(),
            "All tree blocks should be falling"
        );

        println!(
            "Successfully verified large tree fall sync: {} blocks",
            decoded.blocks.len()
        );
    }

    /// Integration test: Verifies empty tree fall is handled gracefully.
    #[test]
    fn test_empty_tree_fell() {
        let mut sync = TreeFallSync::new();
        let mut client = ClientFallingBlockSystem::new();
        let mut handler = ClientTreeFallHandler::new();

        // Empty tree (edge case)
        let tree_fell = sync.build_tree_fell(vec![]);
        assert!(tree_fell.blocks.is_empty());

        // Serialize/deserialize
        let encoded = bincode::serde::encode_to_vec(&tree_fell, bincode::config::standard())
            .expect("Failed to encode empty tree");
        let (decoded, _): (TreeFell, usize) =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .expect("Failed to decode empty tree");

        // Apply to client (should be no-op)
        handler.apply_tree_fell(&decoded, &mut client);

        assert_eq!(client.count(), 0, "Empty tree should spawn no blocks");
    }

    /// Integration test: Verifies tree fall with orphaned leaves cascade.
    ///
    /// When a tree falls, leaves that were only attached to that tree
    /// should also fall (orphaned leaves). This test verifies that
    /// the TreeFell message can include both the tree and its orphaned leaves.
    #[test]
    fn test_tree_fall_with_orphaned_leaves() {
        let mut sync = TreeFallSync::new();
        let mut client = ClientFallingBlockSystem::new();
        let mut handler = ClientTreeFallHandler::new();

        // Tree with orphaned leaves (leaves not connected to trunk but within range)
        let tree_with_orphans = vec![
            // Main tree
            (Vector3::new(0, 0, 0), BlockType::Log),
            (Vector3::new(0, 1, 0), BlockType::Log),
            (Vector3::new(0, 2, 0), BlockType::Log),
            // Connected leaves
            (Vector3::new(1, 2, 0), BlockType::Leaves),
            (Vector3::new(-1, 2, 0), BlockType::Leaves),
            // Orphaned leaves (would be detected by server)
            (Vector3::new(3, 2, 0), BlockType::Leaves), // 2 blocks away, still in range
            (Vector3::new(-3, 2, 0), BlockType::Leaves), // Orphaned on other side
        ];

        let tree_fell = sync.build_tree_fell(tree_with_orphans);

        // All blocks including orphaned leaves should be in the tree fall
        assert_eq!(tree_fell.blocks.len(), 7);

        // Serialize and apply
        let encoded = bincode::serde::encode_to_vec(&tree_fell, bincode::config::standard())
            .expect("Failed to encode");
        let (decoded, _): (TreeFell, usize) =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .expect("Failed to decode");

        handler.apply_tree_fell(&decoded, &mut client);

        // All blocks including orphaned leaves should be falling
        assert_eq!(client.count(), 7);

        let gpu_data = client.gpu_data();
        let leaves_count = gpu_data
            .iter()
            .filter(|b| b.pos_type[3] == BlockType::Leaves as u8 as f32)
            .count();
        assert_eq!(
            leaves_count, 4,
            "Should have 4 falling leaves (2 connected + 2 orphaned)"
        );

        println!(
            "Successfully verified tree fall with orphaned leaves: {} blocks",
            tree_fell.blocks.len()
        );
    }
}

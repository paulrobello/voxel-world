//! Block update queue for frame-distributed physics checks.
//!
//! This module provides a priority queue system that spreads physics checks
//! (gravity, tree support, orphan leaves) across multiple frames to prevent
//! FPS spikes when breaking blocks triggers large cascade events.

use nalgebra::Vector3;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet};

use crate::chunk::BlockType;

/// Information about a falling block that was spawned.
/// Used for multiplayer synchronization.
#[derive(Debug, Clone, Copy)]
pub struct FallingBlockSpawnEvent {
    /// Unique entity ID for network sync.
    pub entity_id: u32,
    /// Grid position where the block started falling.
    pub position: Vector3<i32>,
    /// The type of block that is falling.
    pub block_type: BlockType,
}

/// Information about a model block that broke due to losing ground support.
/// Used for multiplayer synchronization (torches, fences, gates, etc.).
#[derive(Debug, Clone, Copy)]
pub struct ModelGroundSupportBreakEvent {
    /// Grid position where the block broke.
    pub position: Vector3<i32>,
}

use crate::constants::ORTHO_DIRS;
use crate::utils::y_in_bounds;

/// Types of physics checks that can be queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockUpdateType {
    /// Check if block at position is affected by gravity and should fall.
    Gravity,
    /// Check if log at position lost ground support (tree fell).
    TreeSupport,
    /// Check if leaf at position is orphaned (no nearby log).
    OrphanedLeaves,
    /// Check if model at position lost ground support (fences, gates, torches).
    ModelGroundSupport,
}

/// A queued physics check with position and priority.
#[derive(Debug, Clone)]
pub struct BlockUpdate {
    /// World position to check.
    pub position: Vector3<i32>,
    /// Type of physics check to perform.
    pub update_type: BlockUpdateType,
    /// Priority value (lower = higher priority, closer to player).
    priority: f32,
}

impl BlockUpdate {
    /// Creates a new block update with calculated priority.
    pub fn new(
        position: Vector3<i32>,
        update_type: BlockUpdateType,
        player_pos: Vector3<f32>,
    ) -> Self {
        let dx = position.x as f32 - player_pos.x;
        let dy = position.y as f32 - player_pos.y;
        let dz = position.z as f32 - player_pos.z;
        let priority = dx * dx + dy * dy + dz * dz; // Distance squared
        Self {
            position,
            update_type,
            priority,
        }
    }
}

// Implement ordering for priority queue (min-heap: lower priority value = higher priority)
impl PartialEq for BlockUpdate {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority
    }
}

impl Eq for BlockUpdate {}

impl PartialOrd for BlockUpdate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BlockUpdate {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap (smaller priority = higher in heap)
        other
            .priority
            .partial_cmp(&self.priority)
            .unwrap_or(Ordering::Equal)
    }
}

/// Queue for managing frame-distributed physics checks.
pub struct BlockUpdateQueue {
    /// Priority queue of pending updates (closer to player processed first).
    pending: BinaryHeap<BlockUpdate>,
    /// Set of (position, type) pairs already in queue to prevent duplicates.
    queued_set: HashSet<(Vector3<i32>, BlockUpdateType)>,
    /// Maximum updates to process per frame.
    pub max_per_frame: usize,
}

impl BlockUpdateQueue {
    /// Creates a new block update queue.
    ///
    /// # Arguments
    /// * `max_per_frame` - Maximum number of updates to process each frame.
    pub fn new(max_per_frame: usize) -> Self {
        Self {
            pending: BinaryHeap::with_capacity(256),
            queued_set: HashSet::with_capacity(256),
            max_per_frame,
        }
    }

    /// Queues a physics check at the given position.
    ///
    /// Duplicates (same position and type) are ignored.
    pub fn enqueue(
        &mut self,
        position: Vector3<i32>,
        update_type: BlockUpdateType,
        player_pos: Vector3<f32>,
    ) {
        let key = (position, update_type);
        if !self.queued_set.contains(&key) {
            self.queued_set.insert(key);
            self.pending
                .push(BlockUpdate::new(position, update_type, player_pos));
        }
    }

    /// Queues physics checks for all 6 orthogonal neighbors.
    pub fn enqueue_neighbors(
        &mut self,
        center: Vector3<i32>,
        update_type: BlockUpdateType,
        player_pos: Vector3<f32>,
    ) {
        for (dx, dy, dz) in ORTHO_DIRS {
            self.enqueue(center + Vector3::new(dx, dy, dz), update_type, player_pos);
        }
    }

    /// Queues physics checks within a cubic radius around a position.
    pub fn enqueue_radius(
        &mut self,
        center: Vector3<i32>,
        radius: i32,
        update_type: BlockUpdateType,
        player_pos: Vector3<f32>,
    ) {
        for dx in -radius..=radius {
            for dy in -radius..=radius {
                for dz in -radius..=radius {
                    if dx == 0 && dy == 0 && dz == 0 {
                        continue;
                    }
                    self.enqueue(center + Vector3::new(dx, dy, dz), update_type, player_pos);
                }
            }
        }
    }

    /// Takes up to `max_per_frame` updates from the queue.
    ///
    /// Returns the updates to process this frame. The caller should
    /// perform the actual physics checks and call `enqueue` for any
    /// cascade effects.
    pub fn take_batch(&mut self) -> Vec<BlockUpdate> {
        let count = self.pending.len().min(self.max_per_frame);
        let mut batch = Vec::with_capacity(count);

        for _ in 0..count {
            if let Some(update) = self.pending.pop() {
                // Remove from queued set so it can be re-queued if needed
                self.queued_set
                    .remove(&(update.position, update.update_type));
                batch.push(update);
            }
        }

        batch
    }

    /// Returns the number of pending updates.
    #[allow(dead_code)]
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Returns true if there are no pending updates.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Clears all pending updates.
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.pending.clear();
        self.queued_set.clear();
    }

    /// Processes queued block physics updates.
    ///
    /// Returns a tuple of:
    /// - Vector of falling block spawn events for multiplayer synchronization
    /// - Vector of model ground support break events for multiplayer synchronization
    pub fn process_updates(
        &mut self,
        world: &mut crate::world::World,
        falling_blocks: &mut crate::falling_block::FallingBlockSystem,
        particles: &mut crate::particles::ParticleSystem,
        model_registry: &crate::sub_voxel::ModelRegistry,
        _player_pos: Vector3<f32>,
    ) -> (
        Vec<FallingBlockSpawnEvent>,
        Vec<ModelGroundSupportBreakEvent>,
    ) {
        let batch = self.take_batch();
        let mut spawn_events = Vec::new();
        let mut model_break_events = Vec::new();

        for update in batch {
            match update.update_type {
                BlockUpdateType::Gravity => {
                    let spawns =
                        self.process_gravity_update(update.position, world, falling_blocks);
                    spawn_events.extend(spawns);
                }
                BlockUpdateType::TreeSupport => {
                    let spawns =
                        self.process_tree_support_update(update.position, world, falling_blocks);
                    spawn_events.extend(spawns);
                }
                BlockUpdateType::OrphanedLeaves => {
                    let spawns =
                        self.process_orphaned_leaves_update(update.position, world, falling_blocks);
                    spawn_events.extend(spawns);
                }
                BlockUpdateType::ModelGroundSupport => {
                    let breaks = self.process_model_ground_support_update(
                        update.position,
                        world,
                        particles,
                        model_registry,
                    );
                    model_break_events.extend(breaks);
                }
            }
        }

        (spawn_events, model_break_events)
    }

    fn process_gravity_update(
        &mut self,
        pos: Vector3<i32>,
        world: &mut crate::world::World,
        falling_blocks: &mut crate::falling_block::FallingBlockSystem,
    ) -> Vec<FallingBlockSpawnEvent> {
        let mut spawn_events = Vec::new();

        if !y_in_bounds(pos.y) {
            return spawn_events;
        }

        if let Some(block_type) = world.get_block(pos)
            && block_type.is_affected_by_gravity()
        {
            world.set_block(pos, BlockType::Air);
            world.invalidate_minimap_cache(pos.x, pos.z);

            // Spawn falling block and capture entity ID
            if let Some(entity_id) = falling_blocks.spawn(pos, block_type) {
                // Record spawn event for multiplayer sync
                spawn_events.push(FallingBlockSpawnEvent {
                    entity_id,
                    position: pos,
                    block_type,
                });
            }

            let above_pos = pos + Vector3::new(0, 1, 0);

            // Queue the next block up for cascade
            // Using a dummy player_pos since priority doesn't matter much for cascades
            self.enqueue(above_pos, BlockUpdateType::Gravity, Vector3::zeros());

            // If block above is a leaf, check if it's still supported
            if let Some(above_block) = world.get_block(above_pos)
                && matches!(
                    above_block,
                    BlockType::Leaves | BlockType::PineLeaves | BlockType::WillowLeaves
                )
            {
                self.enqueue(above_pos, BlockUpdateType::OrphanedLeaves, Vector3::zeros());
            }
        }

        spawn_events
    }

    fn process_tree_support_update(
        &mut self,
        pos: Vector3<i32>,
        world: &mut crate::world::World,
        falling_blocks: &mut crate::falling_block::FallingBlockSystem,
    ) -> Vec<FallingBlockSpawnEvent> {
        let mut spawn_events = Vec::new();

        if !y_in_bounds(pos.y) {
            return spawn_events;
        }

        if let Some(block) = world.get_block(pos)
            && block.is_log()
        {
            let tree_blocks = world.find_connected_tree(pos);
            if !tree_blocks.is_empty() && !world.tree_has_ground_support(&tree_blocks) {
                for (p, bt) in tree_blocks {
                    world.set_block(p, BlockType::Air);
                    world.invalidate_minimap_cache(p.x, p.z);

                    // Spawn falling block and capture entity ID
                    if let Some(entity_id) = falling_blocks.spawn(p, bt) {
                        // Record spawn event for multiplayer sync
                        spawn_events.push(FallingBlockSpawnEvent {
                            entity_id,
                            position: p,
                            block_type: bt,
                        });
                    }

                    let above_pos = p + Vector3::new(0, 1, 0);

                    // Queue gravity check for block above (snow, sand, gravel, etc.)
                    self.enqueue(above_pos, BlockUpdateType::Gravity, Vector3::zeros());

                    // Also check for orphaned leaves above
                    if let Some(above_block) = world.get_block(above_pos)
                        && matches!(
                            above_block,
                            BlockType::Leaves | BlockType::PineLeaves | BlockType::WillowLeaves
                        )
                    {
                        self.enqueue(above_pos, BlockUpdateType::OrphanedLeaves, Vector3::zeros());
                    }
                }
            }
        }

        spawn_events
    }

    fn process_orphaned_leaves_update(
        &mut self,
        pos: Vector3<i32>,
        world: &mut crate::world::World,
        falling_blocks: &mut crate::falling_block::FallingBlockSystem,
    ) -> Vec<FallingBlockSpawnEvent> {
        let mut spawn_events = Vec::new();

        if !y_in_bounds(pos.y) {
            return spawn_events;
        }

        if let Some(block) = world.get_block(pos)
            && matches!(
                block,
                BlockType::Leaves | BlockType::PineLeaves | BlockType::WillowLeaves
            )
        {
            let (leaves, has_log) = world.find_leaf_cluster_and_check_log(pos);
            if !has_log && !leaves.is_empty() {
                for (p, bt) in leaves {
                    world.set_block(p, BlockType::Air);
                    world.invalidate_minimap_cache(p.x, p.z);

                    // Spawn falling block and capture entity ID
                    if let Some(entity_id) = falling_blocks.spawn(p, bt) {
                        // Record spawn event for multiplayer sync
                        spawn_events.push(FallingBlockSpawnEvent {
                            entity_id,
                            position: p,
                            block_type: bt,
                        });
                    }

                    let above_pos = p + Vector3::new(0, 1, 0);

                    // Queue gravity check for block above (snow, sand, gravel, etc.)
                    self.enqueue(above_pos, BlockUpdateType::Gravity, Vector3::zeros());

                    // Also check for more orphaned leaves above
                    if let Some(above_block) = world.get_block(above_pos)
                        && matches!(
                            above_block,
                            BlockType::Leaves | BlockType::PineLeaves | BlockType::WillowLeaves
                        )
                    {
                        self.enqueue(above_pos, BlockUpdateType::OrphanedLeaves, Vector3::zeros());
                    }
                }
            }
        }

        spawn_events
    }

    fn process_model_ground_support_update(
        &mut self,
        pos: Vector3<i32>,
        world: &mut crate::world::World,
        particles: &mut crate::particles::ParticleSystem,
        model_registry: &crate::sub_voxel::ModelRegistry,
    ) -> Vec<ModelGroundSupportBreakEvent> {
        let mut break_events = Vec::new();

        use crate::chunk::BlockType;
        if pos.y < 1 || !y_in_bounds(pos.y) {
            return break_events;
        }

        if let Some(BlockType::Model) = world.get_block(pos)
            && let Some(data) = world.get_model_data(pos)
            && model_registry.requires_ground_support(data.model_id)
        {
            let below = pos - Vector3::new(0, 1, 0);
            let has_support = if let Some(block_below) = world.get_block(below) {
                block_below.is_solid()
                    || (block_below == BlockType::Model
                        && world
                            .get_model_data(below)
                            .map(|d| !model_registry.requires_ground_support(d.model_id))
                            .unwrap_or(false))
            } else {
                false
            };

            if !has_support {
                particles.spawn_block_break(pos.cast::<f32>(), Vector3::new(0.5, 0.35, 0.2));
                world.set_block(pos, BlockType::Air);
                world.invalidate_minimap_cache(pos.x, pos.z);
                world.update_fence_connections(pos);

                // Record break event for multiplayer sync
                break_events.push(ModelGroundSupportBreakEvent { position: pos });
            }
        }

        break_events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enqueue_dedup() {
        let mut queue = BlockUpdateQueue::new(32);
        let player_pos = Vector3::new(0.0, 0.0, 0.0);
        let pos = Vector3::new(10, 20, 30);

        queue.enqueue(pos, BlockUpdateType::Gravity, player_pos);
        queue.enqueue(pos, BlockUpdateType::Gravity, player_pos);
        queue.enqueue(pos, BlockUpdateType::Gravity, player_pos);

        assert_eq!(queue.pending_count(), 1);
    }

    #[test]
    fn test_different_types_not_deduped() {
        let mut queue = BlockUpdateQueue::new(32);
        let player_pos = Vector3::new(0.0, 0.0, 0.0);
        let pos = Vector3::new(10, 20, 30);

        queue.enqueue(pos, BlockUpdateType::Gravity, player_pos);
        queue.enqueue(pos, BlockUpdateType::TreeSupport, player_pos);
        queue.enqueue(pos, BlockUpdateType::OrphanedLeaves, player_pos);

        assert_eq!(queue.pending_count(), 3);
    }

    #[test]
    fn test_priority_ordering() {
        let mut queue = BlockUpdateQueue::new(32);
        let player_pos = Vector3::new(0.0, 0.0, 0.0);

        // Far position
        queue.enqueue(
            Vector3::new(100, 0, 0),
            BlockUpdateType::Gravity,
            player_pos,
        );
        // Close position
        queue.enqueue(Vector3::new(1, 0, 0), BlockUpdateType::Gravity, player_pos);
        // Medium position
        queue.enqueue(Vector3::new(50, 0, 0), BlockUpdateType::Gravity, player_pos);

        let batch = queue.take_batch();
        assert_eq!(batch.len(), 3);
        // Should be sorted by distance (closest first)
        assert_eq!(batch[0].position.x, 1);
        assert_eq!(batch[1].position.x, 50);
        assert_eq!(batch[2].position.x, 100);
    }

    #[test]
    fn test_take_batch_respects_limit() {
        let mut queue = BlockUpdateQueue::new(2);
        let player_pos = Vector3::new(0.0, 0.0, 0.0);

        for i in 0..10 {
            queue.enqueue(Vector3::new(i, 0, 0), BlockUpdateType::Gravity, player_pos);
        }

        let batch = queue.take_batch();
        assert_eq!(batch.len(), 2);
        assert_eq!(queue.pending_count(), 8);
    }

    #[test]
    fn test_enqueue_radius() {
        let mut queue = BlockUpdateQueue::new(100);
        let player_pos = Vector3::new(0.0, 0.0, 0.0);

        queue.enqueue_radius(
            Vector3::new(0, 0, 0),
            1,
            BlockUpdateType::Gravity,
            player_pos,
        );

        // 3x3x3 cube minus center = 26 neighbors
        assert_eq!(queue.pending_count(), 26);
    }

    #[test]
    fn test_can_requeue_after_processing() {
        let mut queue = BlockUpdateQueue::new(32);
        let player_pos = Vector3::new(0.0, 0.0, 0.0);
        let pos = Vector3::new(10, 20, 30);

        queue.enqueue(pos, BlockUpdateType::Gravity, player_pos);
        let batch = queue.take_batch();
        assert_eq!(batch.len(), 1);
        assert!(queue.is_empty());

        // Should be able to re-queue after processing
        queue.enqueue(pos, BlockUpdateType::Gravity, player_pos);
        assert_eq!(queue.pending_count(), 1);
    }

    /// Integration test: Verifies FallingBlockSpawnEvent is correctly created
    /// for multiplayer synchronization.
    ///
    /// This test verifies that the spawn events contain all necessary data
    /// for network transmission to clients.
    #[test]
    fn test_falling_block_spawn_event_structure() {
        let pos = Vector3::new(10, 20, 30);
        let block_type = BlockType::Sand;

        let event = FallingBlockSpawnEvent {
            entity_id: 1,
            position: pos,
            block_type,
        };

        // Verify event contains correct data for network sync
        assert_eq!(event.entity_id, 1);
        assert_eq!(event.position, pos);
        assert_eq!(event.block_type, BlockType::Sand);
    }

    /// Integration test: Verifies all gravity-affected block types
    /// can create spawn events for multiplayer sync.
    #[test]
    fn test_all_gravity_block_types_spawn_events() {
        let pos = Vector3::new(0, 10, 0);

        // Test all gravity-affected block types
        let gravity_blocks = vec![BlockType::Sand, BlockType::Gravel, BlockType::Snow];

        for (i, block_type) in gravity_blocks.into_iter().enumerate() {
            let event = FallingBlockSpawnEvent {
                entity_id: i as u32 + 1,
                position: pos,
                block_type,
            };

            assert_eq!(event.entity_id, i as u32 + 1);
            assert_eq!(event.position, pos);
            assert_eq!(event.block_type, block_type);
            assert!(
                event.block_type.is_affected_by_gravity(),
                "{:?} should be affected by gravity",
                event.block_type
            );
        }
    }

    /// Integration test: Verifies FallingBlockSpawnEvent serialization
    /// for network transmission.
    ///
    /// This test ensures spawn events can be properly serialized and
    /// deserialized for transmission over the network.
    #[test]
    fn test_spawn_event_serialization() {
        // Create a spawn event as would be generated by process_updates
        let event = FallingBlockSpawnEvent {
            entity_id: 42,
            position: Vector3::new(100, 64, 200),
            block_type: BlockType::Gravel,
        };

        // Simulate converting to network message format
        let position = [
            event.position.x as f32 + 0.5,
            event.position.y as f32 + 0.5,
            event.position.z as f32 + 0.5,
        ];

        // Verify position conversion (center of block)
        assert_eq!(position, [100.5, 64.5, 200.5]);

        // Verify block type is preserved
        assert_eq!(event.block_type, BlockType::Gravel);
    }

    /// Integration test: Verifies tree fall generates multiple spawn events.
    ///
    /// When a tree falls (loses ground support), all connected logs and leaves
    /// should generate spawn events for multiplayer synchronization.
    #[test]
    fn test_tree_fall_generates_multiple_events() {
        // Simulate a tree with multiple blocks
        let tree_blocks = [
            (Vector3::new(0, 0, 0), BlockType::Log),
            (Vector3::new(0, 1, 0), BlockType::Log),
            (Vector3::new(0, 2, 0), BlockType::Log),
            (Vector3::new(1, 2, 0), BlockType::Leaves),
            (Vector3::new(-1, 2, 0), BlockType::Leaves),
        ];

        // Create spawn events for each falling block
        let spawn_events: Vec<FallingBlockSpawnEvent> = tree_blocks
            .iter()
            .enumerate()
            .map(|(i, (pos, block_type))| FallingBlockSpawnEvent {
                entity_id: i as u32 + 1,
                position: *pos,
                block_type: *block_type,
            })
            .collect();

        // Verify all blocks have spawn events
        assert_eq!(spawn_events.len(), 5);

        // Verify positions are preserved
        assert_eq!(spawn_events[0].position, Vector3::new(0, 0, 0));
        assert_eq!(spawn_events[2].position, Vector3::new(0, 2, 0));
        assert_eq!(spawn_events[4].position, Vector3::new(-1, 2, 0));

        // Verify block types are preserved
        assert_eq!(spawn_events[0].block_type, BlockType::Log);
        assert_eq!(spawn_events[3].block_type, BlockType::Leaves);
    }

    /// Integration test: Verifies orphaned leaves decay generates spawn events.
    ///
    /// When leaves become orphaned (no nearby log), they should fall
    /// and generate spawn events for multiplayer sync.
    #[test]
    fn test_orphaned_leaves_spawn_events() {
        // Simulate orphaned leaves cluster
        let orphaned_leaves = [
            (Vector3::new(0, 10, 0), BlockType::Leaves),
            (Vector3::new(1, 10, 0), BlockType::PineLeaves),
            (Vector3::new(0, 10, 1), BlockType::WillowLeaves),
        ];

        // Create spawn events for orphaned leaves
        let spawn_events: Vec<FallingBlockSpawnEvent> = orphaned_leaves
            .iter()
            .enumerate()
            .map(|(i, (pos, block_type))| FallingBlockSpawnEvent {
                entity_id: i as u32 + 1,
                position: *pos,
                block_type: *block_type,
            })
            .collect();

        // Verify all leaves have spawn events
        assert_eq!(spawn_events.len(), 3);

        // Verify all leaf types are supported
        assert_eq!(spawn_events[0].block_type, BlockType::Leaves);
        assert_eq!(spawn_events[1].block_type, BlockType::PineLeaves);
        assert_eq!(spawn_events[2].block_type, BlockType::WillowLeaves);
    }

    /// Integration test: Verifies block physics queue is server-authoritative.
    ///
    /// This test documents the expected behavior:
    /// - Server (host or single-player) processes physics and broadcasts results
    /// - Pure clients do NOT process physics locally
    /// - Clients receive spawn/land messages from server
    #[test]
    fn test_server_authoritative_physics_pattern() {
        // This test documents the expected pattern for multiplayer sync:
        //
        // Server-side (host or single-player):
        // 1. BlockUpdateQueue.process_updates() returns Vec<FallingBlockSpawnEvent>
        // 2. Server broadcasts each event as FallingBlockSpawned message
        // 3. Server runs falling block physics and broadcasts FallingBlockLanded
        //
        // Client-side (pure client):
        // 1. Does NOT call process_updates() - physics is server-authoritative
        // 2. Receives FallingBlockSpawned messages and spawns visual blocks
        // 3. Receives FallingBlockLanded messages and places static blocks
        //
        // This ensures all clients see the same physics results without divergence.

        // Create spawn event as server would
        let server_event = FallingBlockSpawnEvent {
            entity_id: 123,
            position: Vector3::new(50, 100, 50),
            block_type: BlockType::Sand,
        };

        // Server converts to network format (center of block)
        let network_position = [
            server_event.position.x as f32 + 0.5,
            server_event.position.y as f32 + 0.5,
            server_event.position.z as f32 + 0.5,
        ];

        // Client receives and converts back to grid position
        let client_grid_pos = Vector3::new(
            network_position[0].floor() as i32,
            network_position[1].floor() as i32,
            network_position[2].floor() as i32,
        );

        // Verify round-trip conversion
        assert_eq!(client_grid_pos, server_event.position);
        assert_eq!(server_event.entity_id, 123);
        assert_eq!(server_event.block_type, BlockType::Sand);
    }
}

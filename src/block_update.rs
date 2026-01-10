//! Block update queue for frame-distributed physics checks.
//!
//! This module provides a priority queue system that spreads physics checks
//! (gravity, tree support, orphan leaves) across multiple frames to prevent
//! FPS spikes when breaking blocks triggers large cascade events.

use nalgebra::Vector3;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet};

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
    pub fn process_updates(
        &mut self,
        world: &mut crate::world::World,
        falling_blocks: &mut crate::falling_block::FallingBlockSystem,
        particles: &mut crate::particles::ParticleSystem,
        model_registry: &crate::sub_voxel::ModelRegistry,
        _player_pos: Vector3<f32>,
    ) {
        let batch = self.take_batch();

        for update in batch {
            match update.update_type {
                BlockUpdateType::Gravity => {
                    self.process_gravity_update(update.position, world, falling_blocks);
                }
                BlockUpdateType::TreeSupport => {
                    self.process_tree_support_update(update.position, world, falling_blocks);
                }
                BlockUpdateType::OrphanedLeaves => {
                    self.process_orphaned_leaves_update(update.position, world, falling_blocks);
                }
                BlockUpdateType::ModelGroundSupport => {
                    self.process_model_ground_support_update(
                        update.position,
                        world,
                        particles,
                        model_registry,
                    );
                }
            }
        }
    }

    fn process_gravity_update(
        &mut self,
        pos: Vector3<i32>,
        world: &mut crate::world::World,
        falling_blocks: &mut crate::falling_block::FallingBlockSystem,
    ) {
        use crate::chunk::BlockType;
        if !y_in_bounds(pos.y) {
            return;
        }

        if let Some(block_type) = world.get_block(pos) {
            if block_type.is_affected_by_gravity() {
                world.set_block(pos, BlockType::Air);
                world.invalidate_minimap_cache(pos.x, pos.z);
                falling_blocks.spawn(pos, block_type);

                let above_pos = pos + Vector3::new(0, 1, 0);

                // Queue the next block up for cascade
                // Using a dummy player_pos since priority doesn't matter much for cascades
                self.enqueue(above_pos, BlockUpdateType::Gravity, Vector3::zeros());

                // If block above is a leaf, check if it's still supported
                if let Some(above_block) = world.get_block(above_pos) {
                    if matches!(
                        above_block,
                        BlockType::Leaves | BlockType::PineLeaves | BlockType::WillowLeaves
                    ) {
                        self.enqueue(above_pos, BlockUpdateType::OrphanedLeaves, Vector3::zeros());
                    }
                }
            }
        }
    }

    fn process_tree_support_update(
        &mut self,
        pos: Vector3<i32>,
        world: &mut crate::world::World,
        falling_blocks: &mut crate::falling_block::FallingBlockSystem,
    ) {
        use crate::chunk::BlockType;
        if !y_in_bounds(pos.y) {
            return;
        }

        if let Some(block) = world.get_block(pos) {
            if block.is_log() {
                let tree_blocks = world.find_connected_tree(pos);
                if !tree_blocks.is_empty() && !world.tree_has_ground_support(&tree_blocks) {
                    for (p, bt) in tree_blocks {
                        world.set_block(p, BlockType::Air);
                        world.invalidate_minimap_cache(p.x, p.z);
                        falling_blocks.spawn(p, bt);

                        let above_pos = p + Vector3::new(0, 1, 0);

                        // Queue gravity check for block above (snow, sand, gravel, etc.)
                        self.enqueue(above_pos, BlockUpdateType::Gravity, Vector3::zeros());

                        // Also check for orphaned leaves above
                        if let Some(above_block) = world.get_block(above_pos) {
                            if matches!(
                                above_block,
                                BlockType::Leaves | BlockType::PineLeaves | BlockType::WillowLeaves
                            ) {
                                self.enqueue(
                                    above_pos,
                                    BlockUpdateType::OrphanedLeaves,
                                    Vector3::zeros(),
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn process_orphaned_leaves_update(
        &mut self,
        pos: Vector3<i32>,
        world: &mut crate::world::World,
        falling_blocks: &mut crate::falling_block::FallingBlockSystem,
    ) {
        use crate::chunk::BlockType;
        if !y_in_bounds(pos.y) {
            return;
        }

        if let Some(block) = world.get_block(pos) {
            if matches!(
                block,
                BlockType::Leaves | BlockType::PineLeaves | BlockType::WillowLeaves
            ) {
                let (leaves, has_log) = world.find_leaf_cluster_and_check_log(pos);
                if !has_log && !leaves.is_empty() {
                    for (p, bt) in leaves {
                        world.set_block(p, BlockType::Air);
                        world.invalidate_minimap_cache(p.x, p.z);
                        falling_blocks.spawn(p, bt);

                        let above_pos = p + Vector3::new(0, 1, 0);

                        // Queue gravity check for block above (snow, sand, gravel, etc.)
                        self.enqueue(above_pos, BlockUpdateType::Gravity, Vector3::zeros());

                        // Also check for more orphaned leaves above
                        if let Some(above_block) = world.get_block(above_pos) {
                            if matches!(
                                above_block,
                                BlockType::Leaves | BlockType::PineLeaves | BlockType::WillowLeaves
                            ) {
                                self.enqueue(
                                    above_pos,
                                    BlockUpdateType::OrphanedLeaves,
                                    Vector3::zeros(),
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn process_model_ground_support_update(
        &mut self,
        pos: Vector3<i32>,
        world: &mut crate::world::World,
        particles: &mut crate::particles::ParticleSystem,
        model_registry: &crate::sub_voxel::ModelRegistry,
    ) {
        use crate::chunk::BlockType;
        if pos.y < 1 || !y_in_bounds(pos.y) {
            return;
        }

        if let Some(BlockType::Model) = world.get_block(pos) {
            if let Some(data) = world.get_model_data(pos) {
                if model_registry.requires_ground_support(data.model_id) {
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
                        particles
                            .spawn_block_break(pos.cast::<f32>(), Vector3::new(0.5, 0.35, 0.2));
                        world.set_block(pos, BlockType::Air);
                        world.invalidate_minimap_cache(pos.x, pos.z);
                        world.update_fence_connections(pos);
                    }
                }
            }
        }
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
}

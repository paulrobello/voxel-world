//! Block update queue for frame-distributed physics checks.
//!
//! This module provides a priority queue system that spreads physics checks
//! (gravity, tree support, orphan leaves) across multiple frames to prevent
//! FPS spikes when breaking blocks triggers large cascade events.

use nalgebra::Vector3;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet};

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
        let offsets = [
            Vector3::new(1, 0, 0),
            Vector3::new(-1, 0, 0),
            Vector3::new(0, 1, 0),
            Vector3::new(0, -1, 0),
            Vector3::new(0, 0, 1),
            Vector3::new(0, 0, -1),
        ];
        for offset in offsets {
            self.enqueue(center + offset, update_type, player_pos);
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

//! Block change synchronization for multiplayer.
//!
//! Handles broadcasting block changes to connected clients
//! and applying remote changes from the server.

// Allow unused code until networking is integrated into the game
#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};

use crate::net::protocol::{
    BlockChanged, BlockData, BreakBlock, BulkOperation, PlaceBlock, PlayerId,
};

/// Maximum number of pending block changes to buffer.
const MAX_PENDING_CHANGES: usize = 1024;

/// Area of Interest radius for block updates (in blocks).
const AOI_RADIUS: i32 = 64;

/// Manages block synchronization between client and server.
pub struct BlockSyncManager {
    /// Pending block changes to send to server (client-side).
    pending_changes: VecDeque<BlockChange>,
    /// Recent block changes for replay on new clients (server-side).
    recent_changes: VecDeque<([i32; 3], BlockData, u64)>, // position, block, timestamp
    /// Player positions for AoI calculations (server-side).
    player_positions: HashMap<PlayerId, [f32; 3]>,
    /// Whether we're running as server.
    is_server: bool,
}

/// A block change to be synchronized.
#[derive(Debug, Clone)]
pub enum BlockChange {
    /// Place a block.
    Place(PlaceBlock),
    /// Break a block.
    Break(BreakBlock),
    /// Bulk operation.
    Bulk(BulkOperation),
}

impl BlockSyncManager {
    /// Creates a new block sync manager.
    pub fn new(is_server: bool) -> Self {
        Self {
            pending_changes: VecDeque::with_capacity(MAX_PENDING_CHANGES),
            recent_changes: VecDeque::with_capacity(1000),
            player_positions: HashMap::new(),
            is_server,
        }
    }

    // ========================================================================
    // Client-side methods
    // ========================================================================

    /// Queues a block placement for synchronization (client-side).
    pub fn queue_place(&mut self, position: [i32; 3], block: BlockData) {
        if self.is_server {
            return;
        }

        self.pending_changes
            .push_back(BlockChange::Place(PlaceBlock { position, block }));

        // Trim queue if too large
        while self.pending_changes.len() > MAX_PENDING_CHANGES {
            self.pending_changes.pop_front();
        }
    }

    /// Queues a block break for synchronization (client-side).
    pub fn queue_break(&mut self, position: [i32; 3]) {
        if self.is_server {
            return;
        }

        self.pending_changes
            .push_back(BlockChange::Break(BreakBlock { position }));

        while self.pending_changes.len() > MAX_PENDING_CHANGES {
            self.pending_changes.pop_front();
        }
    }

    /// Queues a bulk operation for synchronization (client-side).
    pub fn queue_bulk(&mut self, operation: BulkOperation) {
        if self.is_server {
            return;
        }

        self.pending_changes.push_back(BlockChange::Bulk(operation));

        while self.pending_changes.len() > MAX_PENDING_CHANGES {
            self.pending_changes.pop_front();
        }
    }

    /// Returns pending changes and clears the queue.
    pub fn take_pending_changes(&mut self) -> VecDeque<BlockChange> {
        std::mem::take(&mut self.pending_changes)
    }

    /// Returns true if there are pending changes.
    pub fn has_pending_changes(&self) -> bool {
        !self.pending_changes.is_empty()
    }

    // ========================================================================
    // Server-side methods
    // ========================================================================

    /// Updates a player's position for AoI calculations (server-side).
    pub fn update_player_position(&mut self, player_id: PlayerId, position: [f32; 3]) {
        self.player_positions.insert(player_id, position);
    }

    /// Removes a player from AoI tracking (server-side).
    pub fn remove_player(&mut self, player_id: PlayerId) {
        self.player_positions.remove(&player_id);
    }

    /// Records a block change for recent history (server-side).
    pub fn record_change(&mut self, position: [i32; 3], block: BlockData, timestamp: u64) {
        if !self.is_server {
            return;
        }

        self.recent_changes.push_back((position, block, timestamp));

        // Keep last 1000 changes
        while self.recent_changes.len() > 1000 {
            self.recent_changes.pop_front();
        }
    }

    /// Returns players that should receive a block update based on AoI.
    pub fn get_players_in_range(&self, position: [i32; 3]) -> Vec<PlayerId> {
        let mut result = Vec::new();

        for (&player_id, &player_pos) in &self.player_positions {
            let dx = (position[0] as f32 - player_pos[0]).abs() as i32;
            let dy = (position[1] as f32 - player_pos[1]).abs() as i32;
            let dz = (position[2] as f32 - player_pos[2]).abs() as i32;
            let distance = dx + dy + dz; // Manhattan distance

            if distance <= AOI_RADIUS {
                result.push(player_id);
            }
        }

        result
    }

    /// Returns recent changes for a newly connected player.
    /// Filters by distance from player's spawn position.
    pub fn get_recent_changes_for_player(&self, spawn_position: [f32; 3]) -> Vec<BlockChanged> {
        let mut result = Vec::new();

        for (position, block, _timestamp) in &self.recent_changes {
            let dx = (position[0] as f32 - spawn_position[0]).abs() as i32;
            let dy = (position[1] as f32 - spawn_position[1]).abs() as i32;
            let dz = (position[2] as f32 - spawn_position[2]).abs() as i32;
            let distance = dx + dy + dz;

            if distance <= AOI_RADIUS * 2 {
                result.push(BlockChanged {
                    position: *position,
                    block: block.clone(),
                });
            }
        }

        result
    }

    /// Clears recent change history (e.g., on world change).
    pub fn clear_history(&mut self) {
        self.recent_changes.clear();
    }
}

/// Validates block placements server-side.
pub struct BlockValidator {
    /// Maximum placement distance from player.
    max_placement_distance: f32,
    /// Maximum blocks per second per player.
    rate_limit: u32,
    /// Player action timestamps for rate limiting.
    player_actions: HashMap<PlayerId, VecDeque<u64>>,
}

impl Default for BlockValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockValidator {
    /// Creates a new block validator.
    pub fn new() -> Self {
        Self {
            max_placement_distance: 6.0,
            rate_limit: 20, // 20 blocks per second
            player_actions: HashMap::new(),
        }
    }

    /// Validates a block placement.
    /// Returns Ok(()) if valid, Err with reason if invalid.
    pub fn validate_placement(
        &mut self,
        player_id: PlayerId,
        player_pos: [f32; 3],
        placement: &PlaceBlock,
        current_time: u64,
    ) -> Result<(), String> {
        // Check distance
        let dx = placement.position[0] as f32 - player_pos[0];
        let dy = placement.position[1] as f32 - player_pos[1];
        let dz = placement.position[2] as f32 - player_pos[2];
        let distance = (dx * dx + dy * dy + dz * dz).sqrt();

        if distance > self.max_placement_distance {
            return Err(format!(
                "Block too far away: {:.1} > {:.1}",
                distance, self.max_placement_distance
            ));
        }

        // Check rate limit
        self.check_rate_limit(player_id, current_time)?;

        Ok(())
    }

    /// Validates a block break.
    pub fn validate_break(
        &mut self,
        player_id: PlayerId,
        player_pos: [f32; 3],
        break_block: &BreakBlock,
        current_time: u64,
    ) -> Result<(), String> {
        // Check distance
        let dx = break_block.position[0] as f32 - player_pos[0];
        let dy = break_block.position[1] as f32 - player_pos[1];
        let dz = break_block.position[2] as f32 - player_pos[2];
        let distance = (dx * dx + dy * dy + dz * dz).sqrt();

        if distance > self.max_placement_distance {
            return Err(format!(
                "Block too far away: {:.1} > {:.1}",
                distance, self.max_placement_distance
            ));
        }

        // Check rate limit
        self.check_rate_limit(player_id, current_time)?;

        Ok(())
    }

    /// Checks rate limit for a player.
    fn check_rate_limit(&mut self, player_id: PlayerId, current_time: u64) -> Result<(), String> {
        let actions = self.player_actions.entry(player_id).or_default();

        // Remove actions older than 1 second
        let cutoff = current_time.saturating_sub(1_000_000); // microseconds
        actions.retain(|&t| t > cutoff);

        if actions.len() >= self.rate_limit as usize {
            return Err(format!(
                "Rate limit exceeded: {} actions/second",
                actions.len()
            ));
        }

        actions.push_back(current_time);
        Ok(())
    }

    /// Clears rate limit tracking for a player.
    pub fn clear_player(&mut self, player_id: PlayerId) {
        self.player_actions.remove(&player_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::BlockType;

    #[test]
    fn test_block_sync_queue_place() {
        let mut sync = BlockSyncManager::new(false);

        sync.queue_place([0, 0, 0], BlockData::from(BlockType::Stone));
        assert!(sync.has_pending_changes());

        let changes = sync.take_pending_changes();
        assert_eq!(changes.len(), 1);
        assert!(!sync.has_pending_changes());
    }

    #[test]
    fn test_block_sync_queue_break() {
        let mut sync = BlockSyncManager::new(false);

        sync.queue_break([0, 0, 0]);
        assert!(sync.has_pending_changes());

        let changes = sync.take_pending_changes();
        assert_eq!(changes.len(), 1);
        matches!(changes[0], BlockChange::Break(_));
    }

    #[test]
    fn test_block_sync_server_aoi() {
        let mut sync = BlockSyncManager::new(true);

        // Add players at different positions
        sync.update_player_position(1, [0.0, 0.0, 0.0]);
        sync.update_player_position(2, [100.0, 0.0, 0.0]); // Far away

        // Check who should receive update at [10, 0, 0]
        let players = sync.get_players_in_range([10, 0, 0]);
        assert!(players.contains(&1));
        assert!(!players.contains(&2));

        // Check who should receive update at [100, 0, 0]
        let players = sync.get_players_in_range([100, 0, 0]);
        assert!(players.contains(&2));
        assert!(!players.contains(&1));
    }

    #[test]
    fn test_block_validator_distance() {
        let mut validator = BlockValidator::new();

        // Valid placement (close enough)
        let result = validator.validate_placement(
            1,
            [0.0, 0.0, 0.0],
            &PlaceBlock {
                position: [3, 0, 0],
                block: BlockData::from(BlockType::Stone),
            },
            0,
        );
        assert!(result.is_ok());

        // Invalid placement (too far)
        let result = validator.validate_placement(
            1,
            [0.0, 0.0, 0.0],
            &PlaceBlock {
                position: [10, 0, 0],
                block: BlockData::from(BlockType::Stone),
            },
            0,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_block_validator_rate_limit() {
        let mut validator = BlockValidator::new();
        validator.rate_limit = 5;

        // Start at 1 second to avoid edge case with saturating_sub
        let mut time: u64 = 1_000_000;

        // Should allow up to rate_limit actions within 1 second
        // Time is in microseconds, so 100_000 = 100ms apart
        for _ in 0..5 {
            let result = validator.validate_placement(
                1,
                [0.0, 0.0, 0.0],
                &PlaceBlock {
                    position: [1, 0, 0],
                    block: BlockData::from(BlockType::Stone),
                },
                time,
            );
            assert!(result.is_ok());
            time += 100_000; // 100ms apart in microseconds
        }

        // Should reject on 6th action (still within 1 second window: 1.0s to 1.5s)
        let result = validator.validate_placement(
            1,
            [0.0, 0.0, 0.0],
            &PlaceBlock {
                position: [1, 0, 0],
                block: BlockData::from(BlockType::Stone),
            },
            time,
        );
        assert!(result.is_err());
    }
}

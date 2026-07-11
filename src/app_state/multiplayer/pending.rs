//! Non-queued pending network state, extracted from `MultiplayerState`
//! (ARC-002 phase 7).
//!
//! Holds the "latest value wins" pending slots (server seed, day-cycle pause,
//! time-of-day, spawn position) that can't live in the ordered `events` queue
//! because only the most recent value matters, plus the rate-limited bulk-block
//! placement queue drained at a capped rate each tick by `take_bulk_block_batch`
//! so a 32³ Fill / Replace doesn't stall the host for a full frame.

use std::collections::VecDeque;

use crate::net::protocol::{BlockData, DayCyclePauseChanged, SpawnPositionChanged};

/// Bulk-block queue entry: `(world_position, block, from_filter)`.
///
/// `from_filter` is `None` for Fill (apply unconditionally) or `Some(from_type)`
/// for Replace (skip if the live world block doesn't match).
pub(crate) type BulkBlockEntry = ([i32; 3], BlockData, Option<crate::chunk::BlockType>);

/// "Latest-value-wins" pending slots + the rate-limited bulk-block queue.
///
/// Extracted from `MultiplayerState` (ARC-002). The host/client holds this as
/// `pending: PendingState`; the facade forwards the public take/has accessors.
pub struct PendingState {
    server_seed: Option<(u32, u8)>,
    day_cycle_pause: Option<DayCyclePauseChanged>,
    time_update: Option<f32>,
    spawn_position: Option<SpawnPositionChanged>,
    bulk_blocks: VecDeque<BulkBlockEntry>,
}

impl PendingState {
    /// Creates empty pending state.
    pub fn new() -> Self {
        Self {
            server_seed: None,
            day_cycle_pause: None,
            time_update: None,
            spawn_position: None,
            bulk_blocks: VecDeque::new(),
        }
    }

    /// Records the server world seed (latest value wins).
    pub fn set_server_seed(&mut self, seed: (u32, u8)) {
        self.server_seed = Some(seed);
    }

    /// Takes the pending server world seed, if any.
    pub fn take_server_seed(&mut self) -> Option<(u32, u8)> {
        self.server_seed.take()
    }

    /// Returns true if a server seed is pending.
    pub fn has_server_seed(&self) -> bool {
        self.server_seed.is_some()
    }

    /// Records a day-cycle pause change from the server (latest wins).
    pub fn set_day_cycle_pause(&mut self, pause: DayCyclePauseChanged) {
        self.day_cycle_pause = Some(pause);
    }

    /// Takes the pending day-cycle pause change, if any.
    pub fn take_day_cycle_pause(&mut self) -> Option<DayCyclePauseChanged> {
        self.day_cycle_pause.take()
    }

    /// Returns true if a day-cycle pause change is pending.
    pub fn has_day_cycle_pause(&self) -> bool {
        self.day_cycle_pause.is_some()
    }

    /// Records a time-of-day update from the server (latest wins).
    pub fn set_time_update(&mut self, time_of_day: f32) {
        self.time_update = Some(time_of_day);
    }

    /// Takes the pending time-of-day update, if any.
    pub fn take_time_update(&mut self) -> Option<f32> {
        self.time_update.take()
    }

    /// Returns true if a time-of-day update is pending.
    pub fn has_time_update(&self) -> bool {
        self.time_update.is_some()
    }

    /// Records a spawn-position update from the server (latest wins).
    pub fn set_spawn_position(&mut self, spawn: SpawnPositionChanged) {
        self.spawn_position = Some(spawn);
    }

    /// Takes the pending spawn-position update, if any.
    pub fn take_spawn_position(&mut self) -> Option<SpawnPositionChanged> {
        self.spawn_position.take()
    }

    /// Returns true if a spawn-position update is pending.
    pub fn has_spawn_position(&self) -> bool {
        self.spawn_position.is_some()
    }

    /// Returns a mutable handle to the bulk-block queue, for
    /// `MultiplayerState::materialize_bulk_op` to push into.
    pub fn bulk_blocks_mut(&mut self) -> &mut VecDeque<BulkBlockEntry> {
        &mut self.bulk_blocks
    }

    /// Drains up to `budget` placements from the bulk queue. The caller applies
    /// matching entries to the world and broadcasts the result.
    pub fn take_bulk_batch(&mut self, budget: usize) -> Vec<BulkBlockEntry> {
        let n = budget.min(self.bulk_blocks.len());
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            if let Some(triple) = self.bulk_blocks.pop_front() {
                out.push(triple);
            }
        }
        out
    }

    /// Returns the current bulk-queue depth (debug HUD).
    pub fn bulk_depth(&self) -> usize {
        self.bulk_blocks.len()
    }
}

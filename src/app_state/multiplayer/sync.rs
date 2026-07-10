//! Server-side sync bandwidth state, extracted from `MultiplayerState`
//! (ARC-002 phase 6).
//!
//! Holds the water/lava bandwidth optimizers (delta-encoding + area-of-interest
//! filtering + rate limiting), the persistent tree-fall entity-ID allocator,
//! and the per-position compressed-chunk memoization cache. Consumed only by
//! the host-side broadcast / send-chunk paths, which borrow the server
//! separately (disjoint from this sub-state).

use std::collections::HashMap;

use crate::net::tree_fall_sync::TreeFallSync;
use crate::net::{LavaSyncOptimizer, WaterSyncOptimizer};

/// Server-side sync bandwidth optimizers + the chunk-compression memo cache.
///
/// Extracted from `MultiplayerState` (ARC-002). The host holds this as
/// `sync: SyncState`; the broadcast methods reach the optimizers through the
/// accessors (the `&self` stats getters use the immutable accessors).
pub struct SyncState {
    water_optimizer: WaterSyncOptimizer,
    lava_optimizer: LavaSyncOptimizer,
    tree_fall_sync: TreeFallSync,
    chunk_compression_cache: HashMap<[i32; 3], (u64, Vec<u8>)>,
}

impl SyncState {
    /// Creates fresh sync state (empty optimizers + caches).
    pub fn new() -> Self {
        Self {
            water_optimizer: WaterSyncOptimizer::new(),
            lava_optimizer: LavaSyncOptimizer::new(),
            tree_fall_sync: TreeFallSync::new(),
            chunk_compression_cache: HashMap::new(),
        }
    }

    /// Returns the water bandwidth optimizer (read-only, for stats).
    pub fn water_optimizer(&self) -> &WaterSyncOptimizer {
        &self.water_optimizer
    }

    /// Returns the water bandwidth optimizer (mutable, for broadcast/prune).
    pub fn water_optimizer_mut(&mut self) -> &mut WaterSyncOptimizer {
        &mut self.water_optimizer
    }

    /// Returns the lava bandwidth optimizer (read-only, for stats).
    pub fn lava_optimizer(&self) -> &LavaSyncOptimizer {
        &self.lava_optimizer
    }

    /// Returns the lava bandwidth optimizer (mutable, for broadcast/prune).
    pub fn lava_optimizer_mut(&mut self) -> &mut LavaSyncOptimizer {
        &mut self.lava_optimizer
    }

    /// Returns the persistent tree-fall entity-ID allocator (mutable; builds
    /// batched TreeFell messages and advances the monotonic ID counter).
    pub fn tree_fall_sync_mut(&mut self) -> &mut TreeFallSync {
        &mut self.tree_fall_sync
    }

    /// Returns the per-position compressed-chunk memo cache (read-only lookup).
    pub fn chunk_compression_cache(&self) -> &HashMap<[i32; 3], (u64, Vec<u8>)> {
        &self.chunk_compression_cache
    }

    /// Returns the per-position compressed-chunk memo cache (mutable, to insert).
    pub fn chunk_compression_cache_mut(&mut self) -> &mut HashMap<[i32; 3], (u64, Vec<u8>)> {
        &mut self.chunk_compression_cache
    }
}

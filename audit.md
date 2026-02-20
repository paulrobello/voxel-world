# Multiplayer Sync Points Audit

**Date:** 2026-02-19
**Scope:** Identify all game state that requires multiplayer synchronization and find any missing sync points

## Executive Summary

The multiplayer system has a solid foundation with block placements/breaks, player movement, chunks, custom assets, and time-of-day all being synchronized. However, there are **12 critical missing sync points** and several partial implementations that will cause state divergence between clients and server.

---

## Currently Synchronized State ✅

| State | Protocol Message | Direction | Status |
|-------|------------------|-----------|--------|
| Player position/velocity | `PlayerInput` / `PlayerState` | C→S / S→C | ✅ Complete |
| Block placements | `PlaceBlock` / `BlockChanged` | C→S / S→C | ✅ Complete |
| Block breaks | `BreakBlock` / `BlockChanged` | C→S / S→C | ✅ Complete |
| Bulk operations | `BulkOperation` / `BlocksChanged` | C→S / S→C | ✅ Complete |
| Chunk data | `ChunkData` | S→C | ✅ Complete |
| Chunk local gen | `ChunkGenerateLocal` | S→C | ✅ Complete |
| Time of day | `TimeUpdate` | S→C | ✅ Complete |
| Player join/leave | `PlayerJoined` / `PlayerLeft` | S→C | ✅ Complete |
| Custom models | `ModelRegistrySync` / `ModelAdded` | S→C | ✅ Complete |
| Custom textures | `TextureData` / `TextureAdded` | S→C | ✅ Complete |
| Console commands | `ConsoleCommand` | C→S | ✅ Complete |

---

## Missing Sync Points ❌

### 1. **Water Simulation State** - CRITICAL

**Location:** `src/water.rs`

**Problem:** Water flow is simulated locally on each client via `WaterGrid`. When a player breaks a block next to water, the water grid activates and flows. Other clients will not see this flow because:
- No message type exists for water state changes
- No sync of water cell positions, masses, or source blocks
- Water/lava interactions (cobblestone creation) happen locally only

**Impact:**
- Players see different water levels
- Cobblestone may appear for one player but not others
- Water sources created by one player don't exist for others

**Required Sync:**
```rust
// New protocol messages needed
pub struct WaterCellUpdate {
    pub position: [i32; 3],
    pub mass: f32,
    pub is_source: bool,
    pub water_type: WaterType,
}

pub struct WaterSourcePlaced {
    pub position: [i32; 3],
    pub water_type: WaterType,
}
```

**Files to modify:**
- `src/net/protocol.rs` - Add message types
- `src/app_state/multiplayer.rs` - Handle messages
- `src/water.rs` - Hook into `place_source()`, `set_water()`

---

### 2. **Lava Simulation State** - CRITICAL

**Location:** `src/lava.rs`

**Problem:** Same as water - lava flow is entirely local. Lava sources and flow states are not synchronized.

**Impact:**
- Players see different lava levels
- Lava/water interactions (cobblestone) diverge
- Lava placed by one player doesn't flow for others

**Required Sync:** Same approach as water above.

---

### 3. **Falling Block Entities** - CRITICAL

**Location:** `src/falling_block.rs`

**Problem:** When sand, gravel, or orphaned leaves fall, they become `FallingBlock` entities with position/velocity. These are rendered client-side only. The block landing is applied locally.

**Current flow:**
1. Block loses support → spawns `FallingBlock` locally
2. Physics simulates falling locally
3. Block lands at position locally
4. `world.set_block()` called locally

**Impact:**
- Other clients never see the falling block
- Block may land at different positions due to timing differences
- Cascading gravity (sand column) will diverge completely

**Required Sync:**
```rust
pub struct FallingBlockSpawned {
    pub position: [i32; 3],
    pub block_type: BlockType,
    pub velocity: [f32; 3],
}

pub struct FallingBlockLanded {
    pub position: [i32; 3],
    pub block_type: BlockType,
}
```

**Alternative approach:** Server-authority only - server simulates falling and broadcasts final position.

---

### 4. **Block Update Physics Queue** - HIGH

**Location:** `src/block_update.rs`

**Problem:** The `BlockUpdateQueue` processes gravity checks, tree support, orphaned leaves, and model ground support checks locally. These are frame-distributed and player-distance-prioritized.

**Current triggers:**
- `BlockUpdateType::Gravity` - Sand/gravel/snow falling
- `BlockUpdateType::TreeSupport` - Trees falling when logs broken
- `BlockUpdateType::OrphanedLeaves` - Leaves decaying
- `BlockUpdateType::ModelGroundSupport` - Torches/fences breaking

**Impact:**
- Physics cascades diverge between clients
- Trees may fall for one player but not others
- Leaves decay timing different per client

**Required approach:**
- Server processes ALL physics updates authoritatively
- Broadcast results as block changes
- Clients only predict visual effects, not world state

---

### 5. **Day Cycle Pause State** - HIGH

**Location:** `src/app_state/simulation.rs:49`

**Problem:** `day_cycle_paused` is local state. One player pausing time doesn't affect others.

**Impact:**
- Players experience different times of day
- Lighting differs between clients
- Mob spawning (if added) would be inconsistent

**Required Sync:**
```rust
pub struct DayCyclePauseUpdate {
    pub paused: bool,
    pub time_of_day: f32,
}
```

---

### 6. **Spawn Position** - MEDIUM

**Location:** Server sends `spawn_position` in `ConnectionAccepted`, but:
- Respawn after death uses local spawn
- `/spawn` command has no multiplayer sync
- Beds (if added) would need spawn sync

**Required:** Add `SpawnPositionUpdate` message when spawn changes.

---

### 7. **Measurement Markers** - LOW

**Location:** `src/console/mod.rs` - `ClearMeasurementMarkers`

**Problem:** Markers are local UI state only. Not synced between players.

**Impact:** Minor - markers are user-specific building aids.

**Recommendation:** Intentionally keep local-only (per-player preference).

---

### 8. **Picture Library / Frames** - MEDIUM

**Location:** `src/pictures/library.rs`

**Problem:** Pictures placed in item frames are local state. New pictures uploaded by one player aren't synced.

**Required Sync:**
- `PictureUploaded` message to broadcast new pictures
- `FramePictureSet` to sync frame contents

---

### 9. **Stencil State** - MEDIUM

**Location:** `src/stencils/mod.rs`

**Problem:** Stencils loaded via console command are local. Multiple players can't collaborate on stencil placement.

**Required Sync:**
```rust
pub struct StencilLoaded {
    pub stencil_id: u64,
    pub stencil_data: Vec<u8>, // compressed
}

pub struct StencilTransformUpdate {
    pub stencil_id: u64,
    pub position: [i32; 3],
    pub rotation: u8,
}
```

---

### 10. **Template State** - MEDIUM

**Location:** `src/templates/mod.rs`

**Problem:** Templates loaded for placement are local only.

**Required Sync:** Similar to stencils above.

---

### 11. **Door State Changes** - MEDIUM

**Location:** `src/block_interaction.rs` - doors are models with state

**Problem:** When a player opens/closes a door, the model state changes locally. The block metadata change needs to be synced.

**Current status:** Block placement sync should catch this, but need to verify model data (rotation, waterlogged) is included in `BlockData`.

**Verification needed:** Check `BlockData.model_data` serialization.

---

### 12. **Model Ground Support Breaks** - HIGH

**Location:** `src/block_update.rs:355-392` - `process_model_ground_support_update`

**Problem:** When a torch, fence, or other ground-supported model loses its support block, it breaks and spawns particles locally. This break is not synced.

**Impact:**
- Torches may exist for one player but have broken for others
- Fences float in air for some players

**Required:** Server must process ground support checks and broadcast breaks.

---

## Partial Implementation Issues

### Console Commands

**Status:** Commands are sent to server (`ConsoleCommand`), but server handling is incomplete.

**Issue in `src/app_state/multiplayer.rs:591`:**
```rust
ClientMessage::ConsoleCommand(_) => {
    // Other message types not yet implemented
}
```

**Required:** Server must:
1. Parse and validate commands
2. Execute commands authoritatively
3. Broadcast results to all clients

### Block Validation

**Status:** `BlockValidator` in `src/net/block_sync.rs` exists but is not used.

**Issue:** Server accepts all block placements without:
- Distance validation
- Rate limiting
- Permission checks

---

## Recommended Implementation Priority

| Priority | Sync Point | Effort | Impact |
|----------|------------|--------|--------|
| P0 | Falling blocks | Medium | World divergence |
| P0 | Block physics (gravity/trees) | High | World divergence |
| P1 | Water simulation | High | Visual/gameplay |
| P1 | Lava simulation | Medium | Visual/gameplay |
| P2 | Day cycle pause | Low | Visual consistency |
| P2 | Model ground support breaks | Medium | World divergence |
| P3 | Templates/Stencils | Medium | Collaboration |
| P3 | Picture frames | Low | Decoration |
| P4 | Measurement markers | N/A | Keep local-only |

---

## Architecture Recommendations

### 1. Server-Authoritative Physics

All physics (water, lava, falling blocks, tree support, gravity) should be **server-authoritative**:

```
Client → Request block break
Server → Process break, queue physics checks
Server → Run physics simulation
Server → Broadcast all resulting changes
Client → Apply server changes (rollback prediction if needed)
```

### 2. Sync Protocol Extensions

Add new message categories:
- `FluidState` - Batch water/lava cell updates
- `PhysicsEvent` - Falling blocks, tree falls, etc.
- `WorldState` - Time, spawn, global flags

### 3. Prediction vs Authority

Current system has client prediction for player movement. Extend this pattern:
- **Predict visual effects locally** (particles, falling block rendering)
- **Never predict world state changes** (block placements, fluid levels)
- **Reconcile on server update** (already done for player position)

---

## Code Locations Reference

| Component | File | Lines |
|-----------|------|-------|
| Protocol messages | `src/net/protocol.rs` | 1-445 |
| Client handler | `src/net/client.rs` | 1-300 |
| Server handler | `src/net/server.rs` | 1-300 |
| Block sync | `src/net/block_sync.rs` | 1-405 |
| Multiplayer state | `src/app_state/multiplayer.rs` | 1-1093 |
| Water grid | `src/water.rs` | 1-1803 |
| Lava grid | `src/lava.rs` | 1-832 |
| Falling blocks | `src/falling_block.rs` | 1-295 |
| Block physics queue | `src/block_update.rs` | 1-495 |
| World simulation | `src/app_state/simulation.rs` | 1-241 |

---

## Conclusion

The multiplayer system has a solid foundation but is missing synchronization for all physics-based game state. The most critical gaps are:

1. **Falling blocks** - Currently invisible to other players
2. **Water/lava simulation** - Entirely local, causes world divergence
3. **Block physics queue** - Gravity, tree falls, orphan leaves all local

Without these sync points, multiplayer will result in significantly different world states between clients within minutes of gameplay.

**Estimated effort to complete:** 40-60 hours of development work to implement all critical sync points.

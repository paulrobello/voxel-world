# Multiplayer Synchronization Points

**Document Version:** 1.0
**Last Updated:** 2026-02-19

This document catalogs all game state that requires synchronization in multiplayer mode, identifies what is currently synced, and highlights missing sync points that need implementation.

---

## Table of Contents

1. [Currently Synchronized](#currently-synchronized)
2. [Missing Sync Points](#missing-sync-points)
3. [Protocol Message Reference](#protocol-message-reference)
4. [Implementation Guidelines](#implementation-guidelines)
5. [Code Locations](#code-locations)

---

## Currently Synchronized

### Player State

| State | Message | Direction | Notes |
|-------|---------|-----------|-------|
| Position | `PlayerInput` / `PlayerState` | C→S / S→C | 20Hz, with client prediction and server reconciliation |
| Velocity | `PlayerInput` / `PlayerState` | C→S / S→C | Used for prediction |
| Camera yaw/pitch | `PlayerInput` / `PlayerState` | C→S / S→C | For remote player rendering |
| Input actions | `PlayerInput` | C→S | Jump, sprint, sneak flags |

**Implementation:**
- `src/net/player_sync.rs` - `PredictionState` for client prediction
- `src/app_state/multiplayer.rs` - `send_input()`, reconciliation in `handle_server_message()`

### Block Modifications

| State | Message | Direction | Notes |
|-------|---------|-----------|-------|
| Single block place | `PlaceBlock` / `BlockChanged` | C→S / S→C | Full block data including metadata |
| Single block break | `BreakBlock` / `BlockChanged` | C→S / S→C | Position only, result is Air |
| Bulk operations | `BulkOperation` / `BlocksChanged` | C→S / S→C | Fill, template, replace |

**Block metadata synced:**
- `block_type` - The block type enum
- `model_data` - Model ID, rotation, waterlogged state
- `paint_data` - Texture/tint for painted blocks
- `tint_index` - Color for tinted glass/crystal
- `water_type` - Ocean/Fresh for water blocks

**Implementation:**
- `src/net/protocol.rs` - `BlockData` struct
- `src/app_state/multiplayer.rs` - `sync_block_place()`, `sync_block_break()`
- `src/app/hud.rs` - `sync_block_changes_from_commands()` for console commands

### World Data

| State | Message | Direction | Notes |
|-------|---------|-----------|-------|
| Chunk data | `ChunkData` | S→C | LZ4 compressed, for modified chunks |
| Chunk local gen | `ChunkGenerateLocal` | S→C | Position only, client generates from seed |
| Time of day | `TimeUpdate` | S→C | 0.0-1.0 cycle, periodic broadcast |
| World seed | `ConnectionAccepted` | S→C | Sent once on connect |
| World gen type | `ConnectionAccepted` | S→C | Flat/Normal/Benchmark |

**Chunk sync optimization:**
- Unmodified chunks: Server sends `ChunkGenerateLocal`, client generates locally (bandwidth savings)
- Modified chunks: Server sends full compressed data via `ChunkData`

**Implementation:**
- `src/net/chunk_sync.rs` - Priority-based chunk requests
- `src/app_state/multiplayer.rs` - `send_chunk_to_client()`

### Player Presence

| State | Message | Direction | Notes |
|-------|---------|-----------|-------|
| Player join | `PlayerJoined` | S→C | ID, name, spawn position |
| Player leave | `PlayerLeft` | S→C | ID only |
| Player list | Tracked locally | - | Updated on join/leave |

**Remote player rendering:**
- `remote_players: Vec<RemotePlayer>` in `MultiplayerState`
- Interpolation for smooth movement
- Minimap markers and 3D name labels

### Custom Assets

| State | Message | Direction | Notes |
|-------|---------|-----------|-------|
| Model registry | `ModelRegistrySync` | S→C | All models on connect |
| Model added | `ModelAdded` | S→C | New custom model broadcast |
| Texture data | `TextureData` | S→C | On-demand texture fetch |
| Texture added | `TextureAdded` | S→C | Notification of new texture |

**Implementation:**
- `src/net/texture_slots.rs` - `TextureSlotManager`, `CustomTextureCache`
- `src/app_state/multiplayer.rs` - Model/texture upload and sync

### Console Commands

| State | Message | Direction | Notes |
|-------|---------|-----------|-------|
| Command string | `ConsoleCommand` | C→S | Raw command text |

**Status:** Message type exists, but server processing is incomplete.

---

## Missing Sync Points

### 1. Water Simulation

**Priority:** CRITICAL
**Location:** `src/water.rs`
**Impact:** World divergence, inconsistent water levels between players

**Current Behavior:**
- `WaterGrid` stores cell positions, masses, source states locally
- `process_simulation()` runs cellular automata tick locally
- Water/lava interactions (cobblestone) happen locally only

**Missing Sync:**
```rust
// Proposed protocol messages
ServerMessage::WaterCellsChanged(WaterCellsChanged {
    changes: Vec<WaterCellUpdate>,
})

struct WaterCellUpdate {
    position: [i32; 3],
    mass: f32,
    is_source: bool,
    water_type: u8,
}

ClientMessage::PlaceWaterSource(PlaceWaterSource {
    position: [i32; 3],
    water_type: u8,
})
```

**Implementation Points:**
- `WaterGrid::place_source()` - Hook to sync source placement
- `WaterGrid::tick()` - Server broadcasts changed cells
- `World::set_water_block()` - Trigger water grid activation

---

### 2. Lava Simulation

**Priority:** CRITICAL
**Location:** `src/lava.rs`
**Impact:** Same as water

**Current Behavior:**
- `LavaGrid` is independent per client
- Lava/water interactions create cobblestone locally

**Required Sync:** Same approach as water above.

---

### 3. Falling Block Entities

**Priority:** CRITICAL
**Location:** `src/falling_block.rs`
**Impact:** Falling sand/gravel/leaves invisible to other players

**Current Behavior:**
```
1. Block loses support → FallingBlockSystem::spawn()
2. Local physics simulation with gravity
3. Block lands → world.set_block()
```

**Missing Sync:**
```rust
ServerMessage::FallingBlockSpawned(FallingBlockSpawned {
    entity_id: u32,
    position: [f32; 3],
    velocity: [f32; 3],
    block_type: BlockType,
})

ServerMessage::FallingBlockLanded(FallingBlockLanded {
    entity_id: u32,
    position: [i32; 3],
    block_type: BlockType,
})
```

**Alternative (Simpler):**
- Server simulates all falling blocks
- Broadcast only the final landing position as `BlockChanged`
- Clients render visual falling effect locally (prediction)

---

### 4. Block Physics Queue

**Priority:** CRITICAL
**Location:** `src/block_update.rs`
**Impact:** Physics cascades diverge between clients

**Current Behavior:**
- `BlockUpdateQueue` processes gravity, tree support, orphan leaves locally
- Frame-distributed with player-distance prioritization
- Results directly modify world state

**Update Types That Need Sync:**
| Type | Trigger | Current Behavior |
|------|---------|------------------|
| `Gravity` | Sand/gravel/snow loses support | Spawns falling block locally |
| `TreeSupport` | Log broken, check tree root | Falls entire tree locally |
| `OrphanedLeaves` | No log within 6 blocks | Leaves decay locally |
| `ModelGroundSupport` | Torch/fence loses ground | Breaks with particles locally |

**Required Approach:**
- Server processes ALL physics updates authoritatively
- Broadcast results as `BlockChanged` messages
- Clients only render visual effects (particles)

---

### 5. Day Cycle Pause

**Priority:** HIGH
**Location:** `src/app_state/simulation.rs:49`
**Impact:** Time of day differs between players

**Current State:**
```rust
pub time_of_day: f32,
pub day_cycle_paused: bool,
```

**Missing Sync:**
```rust
ServerMessage::DayCyclePauseChanged(DayCyclePauseChanged {
    paused: bool,
    time_of_day: f32,
})
```

---

### 6. Model Ground Support Breaks

**Priority:** HIGH
**Location:** `src/block_update.rs:355-392`
**Impact:** Torches/fences may exist for one player but not others

**Current Behavior:**
```rust
fn process_model_ground_support_update(...) {
    if !has_support {
        particles.spawn_block_break(...);
        world.set_block(pos, BlockType::Air);  // Local only!
    }
}
```

**Required:** Server must process ground support checks and broadcast breaks.

---

### 7. Spawn Position Updates

**Priority:** MEDIUM
**Location:** Initial spawn in `ConnectionAccepted`

**Current Behavior:**
- `spawn_position` sent once on connect
- No updates when spawn changes (e.g., via command)
- Respawn uses local spawn point

**Missing Sync:**
```rust
ServerMessage::SpawnPositionChanged(SpawnPositionChanged {
    position: [f32; 3],
})
```

---

### 8. Picture Frames

**Priority:** MEDIUM
**Location:** `src/pictures/library.rs`

**Current Behavior:**
- Pictures uploaded locally
- Picture selection for frames is local state

**Missing Sync:**
```rust
ClientMessage::UploadPicture(UploadPicture {
    name: String,
    png_data: Vec<u8>,
})

ServerMessage::PictureAdded(PictureAdded {
    picture_id: u32,
    name: String,
})

ServerMessage::FramePictureSet(FramePictureSet {
    position: [i32; 3],
    picture_id: Option<u32>,
})
```

---

### 9. Stencils

**Priority:** MEDIUM
**Location:** `src/stencils/mod.rs`

**Current Behavior:**
- Stencils loaded via console command locally
- Transform (position, rotation) is local

**Missing Sync:**
```rust
ServerMessage::StencilLoaded(StencilLoaded {
    stencil_id: u64,
    data: Vec<u8>,  // Compressed StencilFile
})

ServerMessage::StencilTransformUpdate(StencilTransformUpdate {
    stencil_id: u64,
    position: [i32; 3],
    rotation: u8,
})

ServerMessage::StencilRemoved(StencilRemoved {
    stencil_id: u64,
})
```

---

### 10. Templates

**Priority:** MEDIUM
**Location:** `src/templates/mod.rs`

**Current Behavior:** Templates loaded locally for placement.

**Missing Sync:** Similar to stencils.

---

### 11. Door State

**Priority:** LOW (may already work via block metadata)
**Location:** Model block data

**Verification Needed:**
- Check if door open/close updates `BlockModelData`
- Check if `BlockData.model_data` is properly serialized in `PlaceBlock`

---

### 12. Measurement Markers

**Priority:** INTENTIONALLY LOCAL
**Location:** `src/console/mod.rs`

**Recommendation:** Keep local-only. These are per-player building aids.

---

## Protocol Message Reference

### Client → Server Messages

```rust
enum ClientMessage {
    PlayerInput(PlayerInput),       // 20Hz position/velocity update
    PlaceBlock(PlaceBlock),         // Single block placement
    BreakBlock(BreakBlock),         // Single block break
    BulkOperation(BulkOperation),   // Fill, template, replace
    RequestChunks(RequestChunks),   // Request chunk data
    ConsoleCommand(ConsoleCommand), // Chat/command string
    RequestTexture(RequestTexture), // Request texture by slot
    UploadModel(UploadModel),       // Upload custom model
    UploadTexture(UploadTexture),   // Upload custom texture
}
```

### Server → Client Messages

```rust
enum ServerMessage {
    ConnectionAccepted(ConnectionAccepted),  // Initial handshake
    ConnectionRejected(ConnectionRejected),  // Failed connection
    PlayerState(PlayerState),                // Authoritative position
    BlockChanged(BlockChanged),              // Single block update
    BlocksChanged(BlocksChanged),            // Multiple block updates
    ChunkData(ChunkData),                    // Compressed chunk data
    ChunkGenerateLocal(ChunkGenerateLocal),  // Generate from seed
    PlayerJoined(PlayerJoined),              // New player notification
    PlayerLeft(PlayerLeft),                  // Player disconnect
    TimeUpdate(TimeUpdate),                  // Day/night cycle
    ModelRegistrySync(ModelRegistrySync),    // All models on connect
    TextureData(TextureData),                // Texture PNG data
    TextureAdded(TextureAdded),              // New texture notification
    ModelAdded(ModelAdded),                  // New model notification
}
```

### Proposed New Messages

```rust
// Fluids
ServerMessage::WaterCellsChanged(WaterCellsChanged),
ServerMessage::LavaCellsChanged(LavaCellsChanged),
ClientMessage::PlaceWaterSource(PlaceWaterSource),
ClientMessage::PlaceLavaSource(PlaceLavaSource),

// Physics
ServerMessage::FallingBlockSpawned(FallingBlockSpawned),
ServerMessage::FallingBlockLanded(FallingBlockLanded),
ServerMessage::TreeFell(TreeFell),  // Multiple blocks at once

// World State
ServerMessage::DayCyclePauseChanged(DayCyclePauseChanged),
ServerMessage::SpawnPositionChanged(SpawnPositionChanged),

// Assets
ClientMessage::UploadPicture(UploadPicture),
ServerMessage::PictureAdded(PictureAdded),
ServerMessage::FramePictureSet(FramePictureSet),

ServerMessage::StencilLoaded(StencilLoaded),
ServerMessage::StencilTransformUpdate(StencilTransformUpdate),
ServerMessage::StencilRemoved(StencilRemoved),
```

---

## Implementation Guidelines

### Server-Authoritative Pattern

All physics and world state changes should follow this pattern:

```
┌─────────────┐                    ┌─────────────┐
│   Client    │                    │   Server    │
└──────┬──────┘                    └──────┬──────┘
       │                                  │
       │ Request (break block at X)       │
       │ ──────────────────────────────► │
       │                                  │
       │                    Validate request
       │                    Process physics
       │                    Update world state
       │                                  │
       │ BlockChanged(Air at X)           │
       │ ◄────────────────────────────── │
       │                                  │
       │ FallingBlockSpawned(sand above)  │
       │ ◄────────────────────────────── │
       │                                  │
       │                    Simulate fall
       │                                  │
       │ FallingBlockLanded(Y)            │
       │ BlockChanged(Sand at Y)          │
       │ ◄────────────────────────────── │
       │                                  │
  Render visual                 Broadcast to
  effects locally               all clients
```

### Client Prediction Rules

**CAN predict locally:**
- Player movement (already implemented)
- Particle effects
- Sound effects
- Visual falling block position (until server confirms landing)

**MUST NOT predict:**
- Block placements (wait for server confirmation)
- Block breaks (wait for server confirmation)
- Water/lava level changes
- Tree falls
- Gravity cascades

### Bandwidth Optimization

For high-frequency state like water:

1. **Batch updates:** Collect changes per tick, send as one message
2. **Delta encoding:** Only send changed cells, not full grid
3. **AoI filtering:** Only send updates near each player
4. **Throttle rate:** Water doesn't need 20Hz, 2-5Hz is fine

---

## Code Locations

### Network Stack

| Component | File | Key Structures |
|-----------|------|----------------|
| Protocol | `src/net/protocol.rs` | `ClientMessage`, `ServerMessage`, all message structs |
| Client | `src/net/client.rs` | `GameClient`, send/receive methods |
| Server | `src/net/server.rs` | `GameServer`, broadcast methods |
| Block sync | `src/net/block_sync.rs` | `BlockSyncManager`, `BlockValidator` |
| Chunk sync | `src/net/chunk_sync.rs` | `ChunkSyncManager`, `SerializedChunk` |
| Player sync | `src/net/player_sync.rs` | `PredictionState`, `RemotePlayer` |
| Texture sync | `src/net/texture_slots.rs` | `TextureSlotManager`, `CustomTextureCache` |
| Multiplayer state | `src/app_state/multiplayer.rs` | `MultiplayerState`, message handling |

### Physics Systems

| System | File | Key Structures |
|--------|------|----------------|
| Water | `src/water.rs` | `WaterGrid`, `WaterCell` |
| Lava | `src/lava.rs` | `LavaGrid`, `LavaCell` |
| Falling blocks | `src/falling_block.rs` | `FallingBlockSystem`, `FallingBlock` |
| Block physics | `src/block_update.rs` | `BlockUpdateQueue`, `BlockUpdateType` |

### Game State

| State | File | Key Fields |
|-------|------|------------|
| World sim | `src/app_state/simulation.rs` | `WorldSim` (world, player, physics, time) |
| World | `src/world/mod.rs` | `World` (chunks, blocks, metadata) |
| UI state | `src/app_state/ui_state.rs` | Hotbar, breaking progress, panels |
| Console | `src/console/mod.rs` | Commands, pending actions |

---

## Implementation Priority

| Priority | Sync Point | Effort | Dependencies |
|----------|------------|--------|--------------|
| **P0** | Falling blocks | 2-3 days | Protocol changes |
| **P0** | Block physics queue | 3-5 days | Server authority model |
| **P1** | Water simulation | 5-7 days | Bandwidth optimization |
| **P1** | Lava simulation | 2-3 days | Same as water |
| **P2** | Day cycle pause | 0.5 days | Simple message |
| **P2** | Model ground support | 1-2 days | Server physics |
| **P3** | Pictures/frames | 2-3 days | Asset sync |
| **P3** | Stencils/templates | 2-3 days | Asset sync |
| **P4** | Spawn position | 0.5 days | Simple message |

**Total estimated effort:** 20-30 days for all critical (P0-P2) sync points.

---

## Testing Checklist

When implementing sync points, verify:

- [ ] Both clients see the same world state
- [ ] Actions by one player appear for others within 100ms
- [ ] No state divergence after 5+ minutes of gameplay
- [ ] Graceful handling of packet loss
- [ ] Graceful handling of client reconnection
- [ ] Server validates all client requests
- [ ] Rate limiting prevents abuse
- [ ] Bandwidth usage is acceptable (< 100 KB/s per client)

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-02-19 | Initial audit and documentation |

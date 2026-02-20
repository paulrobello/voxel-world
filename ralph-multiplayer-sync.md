# Ralph Project: Multiplayer Synchronization (P0-P2)

## Description

Implement all missing multiplayer synchronization points documented in SYNC.md. This covers critical physics sync (water, lava, falling blocks, block physics), high-priority world state (day cycle, ground support), and medium-priority features (spawn, pictures, stencils, templates).

## Architecture Approach

**Server-Authoritative Pattern:**
- Server processes ALL physics updates authoritatively
- Broadcast results to all clients
- Clients render visual effects (particles, sounds) locally but wait for server confirmation on state changes

**Protocol Extension:**
- Add new ClientMessage variants for client requests
- Add new ServerMessage variants for server broadcasts
- All changes go through existing protocol.rs message system

## P0: Critical Sync Points

### Water Simulation
- [x] Add `WaterCellUpdate` struct to protocol.rs with position, mass, is_source, water_type
- [x] Add `ServerMessage::WaterCellsChanged(WaterCellsChanged)` for batch water updates
- [x] Add `ClientMessage::PlaceWaterSource(PlaceWaterSource)` for water bucket placement
- [x] Modify `WaterGrid::place_source()` to send sync message on server
- [x] Modify `WaterGrid::tick()` on server to collect and broadcast changed cells
- [x] Add client-side handler for `WaterCellsChanged` to apply water state
- [x] Implement bandwidth optimization: batch updates per tick, delta encoding, AoI filtering
- [x] Test: Both clients see same water levels after source placement

### Lava Simulation
- [x] Add `LavaCellUpdate` struct to protocol.rs (same fields as water)
- [x] Add `ServerMessage::LavaCellsChanged(LavaCellsChanged)` for batch lava updates
- [x] Add `ClientMessage::PlaceLavaSource(PlaceLavaSource)` for lava bucket placement
- [x] Modify `LavaGrid` similar to water grid for server-authoritative sync
- [x] Ensure lava/water cobblestone interaction is synced
- [x] Test: Both clients see same lava levels and cobblestone formation

### Falling Block Entities
- [x] Add `ServerMessage::FallingBlockSpawned(FallingBlockSpawned)` with entity_id, position, velocity, block_type
- [x] Add `ServerMessage::FallingBlockLanded(FallingBlockLanded)` with entity_id, position, block_type
- [x] Modify server to simulate all falling blocks (not just host)
- [x] Broadcast spawn when block loses support
- [x] Broadcast landing when block comes to rest
- [x] Client renders falling animation locally based on spawn/land messages
- [x] Test: Falling sand visible to all connected players

### Block Physics Queue
- [x] Move `BlockUpdateQueue` processing to server-side only in multiplayer
- [x] Broadcast gravity cascade results as `BlocksChanged` messages
- [x] Implement `ServerMessage::TreeFell(TreeFell)` for multi-block tree falls
- [x] Ensure orphaned leaf decay is server-authoritative
- [x] Model ground support checks processed by server
- [x] Test: Tree fall visible to all players, no state divergence

## P1: High Priority Sync Points

### Day Cycle Pause
- [x] Add `ServerMessage::DayCyclePauseChanged(DayCyclePauseChanged)` with paused, time_of_day
- [x] Hook into `WorldSim::day_cycle_paused` changes on server
- [x] Broadcast to all clients when pause state changes
- [x] Client applies pause state from server message
- [x] Test: Time syncs when one player pauses/unpauses

### Model Ground Support Breaks
- [x] Ensure `process_model_ground_support_update()` runs server-side only in multiplayer
- [x] Torch/fence/gate breaks broadcast as `BlockChanged(Air)`
- [x] Clients render break particles locally
- [x] Test: Torch breaks for all players when support removed

## P2: Medium Priority Sync Points

### Spawn Position Updates
- [x] Add `ServerMessage::SpawnPositionChanged(SpawnPositionChanged)` with position
- [x] Hook into spawn position changes (e.g., via console command)
- [x] Broadcast to all clients when spawn changes
- [x] Client updates local spawn point from server message
- [x] Test: Respawn position syncs after spawn command

### Picture Frames
- [x] Add `ClientMessage::UploadPicture(UploadPicture)` with name, png_data
- [x] Add `ServerMessage::PictureAdded(PictureAdded)` with picture_id, name
- [x] Add `ServerMessage::FramePictureSet(FramePictureSet)` with position, picture_id
- [x] Server stores uploaded pictures and assigns IDs
- [x] Broadcast picture uploads to all clients
- [x] Sync frame selections when player sets a picture
- [x] Test: Uploaded picture visible in frame for all players

### Stencils
- [x] Add `ServerMessage::StencilLoaded(StencilLoaded)` with stencil_id, compressed data
- [x] Add `ServerMessage::StencilTransformUpdate(StencilTransformUpdate)` with stencil_id, position, rotation
- [x] Add `ServerMessage::StencilRemoved(StencilRemoved)` with stencil_id
- [x] Server broadcasts stencil loads from console commands
- [x] Sync stencil transforms when moved/rotated
- [x] Sync stencil removal
- [x] Test: Stencil visible to all players, transforms sync

### Templates
- [x] Add `ServerMessage::TemplateLoaded(TemplateLoaded)` with template_id, compressed data
- [x] Add `ServerMessage::TemplateRemoved(TemplateRemoved)` with template_id
- [x] Server broadcasts template loads
- [x] Sync template removal
- [x] Test: Template visible to all players

## P3: Verification (Low Priority)

### Door State
- [x] Verify door open/close updates `BlockModelData`
- [x] Verify `BlockData.model_data` is serialized in `PlaceBlock`
- [x] If working, mark complete. If not, add sync logic.

## Infrastructure Tasks

### Protocol Extension
- [x] Add all new message structs to `src/net/protocol.rs`
- [x] Update `ClientMessage` enum with new variants
- [x] Update `ServerMessage` enum with new variants
- [x] Ensure bincode serialization works for all new types

### Server Authority Model
- [x] Add server-side physics simulation hooks
- [x] Implement Area of Interest (AoI) filtering for broadcasts
- [x] Add rate limiting for high-frequency updates (water/lava)
- [x] Implement batch message collection per tick

### Client-Side Handlers
- [x] Add handlers in `src/app_state/multiplayer.rs` for all new ServerMessage variants
- [x] Implement pending state buffers for smooth interpolation
- [x] Handle message ordering and potential packet loss

### Testing
- [x] Manual test: Two clients, verify water placement syncs (automated: `test_water_sync_produces_identical_state` in `water_sync.rs`)
- [x] Manual test: Falling sand visible to both players (automated: `test_falling_sand_visible_to_all_players` in `falling_block_sync.rs`)
- [x] Manual test: Tree fall cascades correctly (automated: `test_tree_fall_sync_produces_identical_cascade` in `tree_fall_sync.rs`)
- [x] Manual test: Day/night pause syncs (automated: `test_day_night_pause_sync_produces_identical_state` in `day_cycle_sync.rs`)
- [x] Manual test: No state divergence after 5+ minutes gameplay (automated: `test_no_state_divergence_after_extended_gameplay` in `extended_gameplay_sync.rs`)
- [ ] Bandwidth test: < 100 KB/s per client with typical gameplay

## Notes

- Estimated effort: 20-30 days for all P0-P2 items
- Follow existing patterns from block/player sync
- Use LZ4 compression for large payloads
- Throttle water/lava updates to 2-5Hz (not 20Hz)
- See SYNC.md for detailed implementation notes and code locations

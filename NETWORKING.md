# Voxel World Multiplayer Networking

This document describes the multiplayer networking architecture for voxel-world.

## Overview

Voxel-world supports both single-player and multiplayer modes through an integrated server architecture. The same binary can run as:
- **Single-player**: Local world without networking
- **Integrated server**: Host a game that other players can join
- **Dedicated server**: Headless server (future phase)

## Scope

- **Max Players**: 4 (small co-op sessions)
- **Network Scope**: LAN only (no NAT traversal initially)
- **Implementation Priority**: Player sync → Chunk streaming → Dedicated server

## Technology Stack

| Layer | Crate | Version | Purpose |
|-------|-------|---------|---------|
| Networking | `renet` | 2.0 | UDP-based game networking |
| Authentication | `renet_netcode` | 2.0 | Secure handshake, encryption |
| Serialization (Messages) | `bincode` | 2.0 | Fast message serialization |
| Compression | `lz4_flex` | 0.11 | Chunk data compression |
| Async Runtime | `tokio` | 1.x | Async networking support |

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Game Layer                              │
│  ┌─────────────────┐    ┌─────────────────────────────────┐ │
│  │   ClientApp     │    │       ServerApp                 │ │
│  │  - LocalPlayer  │    │  - World (authoritative)        │ │
│  │  - Input        │    │  - PlayerSessions               │ │
│  │  - Prediction   │    │  - Physics Simulation           │ │
│  └────────┬────────┘    └───────────────┬─────────────────┘ │
│           │                             │                    │
└───────────┼─────────────────────────────┼────────────────────┘
            │                             │
┌───────────┼─────────────────────────────┼────────────────────┐
│           │      Networking Layer       │                    │
│  ┌────────▼────────┐    ┌──────────────▼─────────────────┐  │
│  │   RenetClient   │◄───┤       RenetServer              │  │
│  │  - Channels     │    │  - Channels                    │  │
│  │  - Reliability  │    │  - Broadcast/Unicast           │  │
│  └─────────────────┘    └────────────────────────────────┘  │
│           │                             │                    │
└───────────┼─────────────────────────────┼────────────────────┘
            │                             │
┌───────────┼─────────────────────────────┼────────────────────┐
│           │       Transport Layer       │                    │
│           │         (UDP via            │                    │
│           │       renet_netcode)        │                    │
└───────────┴─────────────────────────────┴────────────────────┘
```

## Channel Configuration

Renet channels with different delivery guarantees:

| Channel | Mode | Use Case | Frequency |
|---------|------|----------|-----------|
| `PlayerMovement` | Unreliable | Position, velocity, rotation | ~20/sec |
| `BlockUpdates` | Reliable Unordered | Block changes | On action |
| `GameState` | Reliable Ordered | Join/leave, chat, time | As needed |
| `ChunkStream` | Unreliable | Chunk data | On request |

## Protocol Messages

### Client → Server

```rust
// Player input (sent every frame, ~20/sec)
PlayerInput {
    sequence: u32,
    position: [f32; 3],
    velocity: [f32; 3],
    yaw: f32,
    pitch: f32,
    actions: InputActions, // bitflags: jump, sprint, place, break
}

// Block operations
PlaceBlock { position: [i32; 3], block: BlockData }
BreakBlock { position: [i32; 3] }

// Chunk requests
RequestChunks { positions: Vec<[i32; 3]> }

// Console commands
ConsoleCommand { command: String }
```

### Server → Client

```rust
// World state (position corrections)
PlayerState {
    player_id: u64,
    position: [f32; 3],
    velocity: [f32; 3],
    last_sequence: u32,
}

// Block updates
BlockChanged { position: [i32; 3], block: BlockData }
BlocksChanged { changes: Vec<([i32; 3], BlockData)> }

// Chunk data
ChunkData {
    position: [i32; 3],
    version: u32,
    compressed_data: Vec<u8>, // LZ4 compressed
}

// Player events
PlayerJoined { id: u64, name: String, position: [f32; 3] }
PlayerLeft { id: u64 }

// Time sync
TimeUpdate { time_of_day: f32 }
```

## State Synchronization

### Server-Authoritative Model

| State | Authority | Sync Strategy |
|-------|-----------|---------------|
| World blocks | Server | Server validates, broadcasts changes |
| Chunk data | Server | On-demand streaming with versioning |
| Player position | Server | Client predicts, server corrects |
| Water/Lava sim | Server | Full server authority |
| Falling blocks | Server | Server simulates, broadcasts |
| Time of day | Server | Periodic sync |
| Templates | Client | Stored locally, optional sharing |

### Client-Side Prediction

1. Client sends input + predicted position to server
2. Client renders predicted state immediately
3. Server validates and returns authoritative state
4. Client reconciles if prediction differs

```rust
struct PredictionState {
    input_buffer: VecDeque<(u32, PlayerInput)>, // last 64 inputs
    predicted_positions: VecDeque<[f32; 3]>,
    last_server_sequence: u32,
}
```

## Chunk Streaming Strategy

### Priority Queue

Chunks are requested based on priority:

1. **PlayerPosition** (Critical): Chunk containing the player
2. **ViewDirection** (High): Chunks in player's look direction
3. **Adjacent** (Medium): Chunks adjacent to loaded chunks
4. **Background** (Low): Remaining chunks within view distance

### Cancellation

When the player changes look direction:
1. Calculate dot product of look direction to pending chunks
2. Cancel requests for chunks now behind the player (dot < -0.3)
3. Re-prioritize remaining requests
4. Request new chunks in the new look direction

### Compression

Chunk data is compressed with LZ4 before transmission:
- Block data: 32KB (32³ bytes)
- Model metadata: Variable (sparse)
- Typical compression ratio: 5-10x

### Chunk Data Format

The `SerializedChunk` format is used for network transmission:

```
┌─────────────────────────────────────────────────────────────┐
│  Block Data (CHUNK_VOLUME bytes)                            │
│  - One byte per block, representing BlockType               │
├─────────────────────────────────────────────────────────────┤
│  Model Metadata (sparse)                                    │
│  - count: u16                                               │
│  - entries: [index: u32, model_id: u8, rotation: u8,        │
│              waterlogged: u8, custom_data: u32]             │
├─────────────────────────────────────────────────────────────┤
│  Paint Metadata (sparse)                                    │
│  - count: u16                                               │
│  - entries: [index: u32, texture_idx: u8, tint_idx: u8,     │
│              blend_mode: u8]                                │
├─────────────────────────────────────────────────────────────┤
│  Tint Metadata (sparse)                                     │
│  - count: u16                                               │
│  - entries: [index: u32, tint: u8]                          │
├─────────────────────────────────────────────────────────────┤
│  Water Metadata (sparse)                                    │
│  - count: u16                                               │
│  - entries: [index: u32, water_type: u8]                    │
└─────────────────────────────────────────────────────────────┘
```

### Client-Side Chunk Processing

1. **Receive**: `ChunkData` message arrives from server
2. **Decompress**: LZ4 decompression via `decompress_size_prepended()`
3. **Deserialize**: Parse block data and sparse metadata
4. **Convert**: Create `Chunk` struct via `from_network_data()`
5. **Queue**: Store in `MultiplayerState.pending_chunks`
6. **Apply**: Insert into world and upload to GPU in `update_chunk_loading()`

### Server-Side Chunk Fulfillment

When hosting a game, the server fulfills chunk requests from clients:

1. **Receive Request**: `ClientMessage::RequestChunks` arrives from client
2. **Parse**: `GameServer::receive_client_messages()` deserializes typed messages
3. **Queue**: `MultiplayerState::handle_client_message()` stores in `pending_chunk_requests`
4. **Process**: `App::fulfill_chunk_requests()` retrieves chunks from world
5. **Serialize**: `SerializedChunk::from_chunk()` extracts block data and metadata
6. **Compress**: LZ4 compression via `compress_prepend_size()`
7. **Send**: `GameServer::send_chunk()` transmits `ChunkData` to client

```rust
// In App::fulfill_chunk_requests():
let requests = self.multiplayer.take_pending_chunk_requests();
for (client_id, positions) in requests {
    for chunk_pos in positions {
        if let Some(chunk) = self.sim.world.get_chunk(pos) {
            self.multiplayer.send_chunk_to_client(client_id, chunk_pos, chunk);
        }
    }
}
```

## LAN Discovery Protocol

UDP broadcast-based server discovery allows clients to find servers on the local network without manual IP entry.

### Protocol Details

- **Discovery Port**: 5001 (separate from game port 5000)
- **Magic Bytes**: `VXLD` (identifies voxel-world discovery packets)
- **Timeout**: 5 seconds (stale server entries removed)

### Packet Types

| Type | Code | Direction | Description |
|------|------|-----------|-------------|
| DiscoveryRequest | 0x01 | Client → Broadcast | Client scanning for servers |
| ServerAnnouncement | 0x02 | Server → Client | Server response with info |

### Server Announcement Format

```rust
struct ServerAnnouncement {
    game_port: u16,      // Actual game server port (5000)
    server_name: String, // Human-readable server name
    player_count: u8,    // Current players
    max_players: u8,     // Maximum capacity
}
```

### Discovery Flow

1. **Client broadcasts** discovery request to `255.255.255.255:5001`
2. **Servers respond** with `ServerAnnouncement` (unicast to client)
3. **Client tracks** discovered servers with timestamp
4. **Stale entries** (5s timeout) are automatically removed

## Multiplayer UI

In-game multiplayer panels provide a graphical interface for hosting and joining games.

### UI Components

| Panel | Trigger | Description |
|-------|---------|-------------|
| Multiplayer Panel | O key | Tabbed Host/Join interface |
| Connection Status | Auto | Top-right overlay when connected |
| Player List | Tab key | Shows connected players |

### Host Tab Features

- Server name configuration (displayed in LAN discovery)
- Port selection (default: 5000)
- Start/Stop hosting controls
- Status display with player count

### Join Tab Features

- **Direct Connect**: Manual IP:port entry
- **LAN Discovery**: Automatic server scanning
- Server list with name, players, and address
- Double-click or "Join Selected" to connect

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| O | Toggle multiplayer panel |
| Tab | Toggle player list (when connected) |
| Escape | Close multiplayer panel |

## File Structure

```
src/
├── net/
│   ├── mod.rs              # Module exports
│   ├── channel.rs          # Renet channel configuration
│   ├── protocol.rs         # Message types (bincode serialization)
│   ├── server.rs           # GameServer wrapper (RenetServer)
│   │                       # - receive_client_messages(): parse typed messages
│   │                       # - send_chunk(): send ChunkData to client
│   │                       # - broadcast_block_change(): broadcast to all clients
│   ├── server_thread.rs    # Dedicated server thread (experimental)
│   │                       # - ServerThread: wrapper that spawns dedicated thread
│   │                       # - ServerCommand: commands from main to server thread
│   │                       # - ServerThreadEvent: events from server to main thread
│   ├── client.rs           # GameClient wrapper (RenetClient)
│   ├── chunk_sync.rs       # Chunk streaming with priority queue
│   │                       # - ChunkSyncManager: tracks requests/received
│   │                       # - SerializedChunk: compression/decompression
│   │                       #   - from_chunk(): serialize for transmission
│   │                       #   - to_chunk(): deserialize to Chunk
│   │                       # - Priority calculation based on position/view
│   ├── player_sync.rs      # Player position sync + prediction
│   ├── block_sync.rs       # Block change broadcasting + AoI
│   ├── discovery.rs        # LAN server discovery (UDP broadcast)
│   │                       # - LanDiscovery: client-side listener
│   │                       # - DiscoveryResponder: server-side broadcaster
│   │                       # - ServerAnnouncement: protocol message
│   │                       # - DiscoveredServer: tracked server entry
│   └── auth.rs             # Connection handshake (renet_netcode)
├── ui/
│   ├── mod.rs              # HUD rendering with multiplayer support
│   └── multiplayer.rs      # Multiplayer UI panels
│                           # - MultiplayerPanelState: panel state
│                           # - HostPanelState: host configuration
│                           # - JoinPanelState: join/discovery state
│                           # - MultiplayerAction: UI action results
│                           # - MultiplayerUI::draw(): panel renderer
├── app_state/
│   ├── ui_state.rs         # UiState with multiplayer_panel field
│   └── multiplayer.rs      # MultiplayerState (server/client management)
│                           # - pending_chunks: received but not yet applied
│                           # - pending_block_changes: remote block updates
│                           # - pending_chunk_requests: server-side queue
│                           # - discovery: Option<LanDiscovery>
│                           # - discovery_responder: Option<DiscoveryResponder>
│                           # - handle_client_message(): route client messages
│                           # - send_chunk_to_client(): send chunk to client
│                           # - take_pending_chunk_requests(): get requests
├── config.rs               # CLI args (--host, --connect, --port)
├── block_interaction.rs    # Block place/break (multiplayer sync hooks)
├── chunk.rs                # Chunk struct with from_network_data()
├── world_streaming.rs      # update_chunk_loading() with network chunk support
└── app/
    ├── core.rs             # App struct with multiplayer helpers
    │                       # - request_network_chunks(): client requests
    │                       # - apply_network_chunks(): apply received chunks
    │                       # - apply_remote_block_changes(): apply block changes
    │                       # - fulfill_chunk_requests(): server sends chunks
    ├── init.rs             # Multiplayer initialization from CLI
    ├── input.rs            # Keyboard shortcuts (O, Tab, Escape)
    ├── hud.rs              # HUD rendering with multiplayer actions
    └── update.rs           # Game loop with multiplayer.update()
```

## Integration Points

### Game Loop Integration

The multiplayer system is integrated into the main game loop in `src/app/update.rs`:

```rust
// Update multiplayer networking (process server/client updates)
if self.multiplayer.mode != GameMode::SinglePlayer {
    self.multiplayer.update(Duration::from_secs_f64(delta_time));

    // Apply any remote block changes received from server
    self.apply_remote_block_changes();

    // Request chunks from server when in client mode
    if self.multiplayer.mode == GameMode::Client {
        self.request_network_chunks();
    }

    // Fulfill chunk requests from clients when hosting
    if self.multiplayer.is_hosting() {
        self.fulfill_chunk_requests();
    }
}
```

### Chunk Streaming Integration

Chunk requests are sent from the client in `src/app/core.rs`:

```rust
pub fn request_network_chunks(&mut self) {
    // Get player position and look direction for prioritization
    let player_world_pos = self.sim.player.feet_pos(...);
    let look_dir = [yaw.sin(), 0.0, -yaw.cos()];

    // Update chunk sync manager and get chunks to request
    let request = self.multiplayer.chunk_sync.request_chunks_around(
        [player_chunk.x, player_chunk.y, player_chunk.z],
        self.sim.view_distance,
    );

    // Send chunk request to server
    if let Some(ref mut client) = self.multiplayer.client {
        client.send_chunk_request(request.positions);
    }
}
```

Network chunks are applied in `src/world_streaming.rs`:

```rust
// In update_chunk_loading():
let network_chunks = self.apply_network_chunks();

for (pos, chunk) in network_chunks {
    self.sim.world.insert_chunk(pos, chunk);
    // Upload to GPU with metadata updates
}
```

### Block Change Sync

Block changes are synchronized in `src/block_interaction.rs`:

```rust
// After breaking a block:
self.sync_block_break([target.x, target.y, target.z]);

// After placing a block:
let block_data = BlockData { block_type, model_data, ... };
self.sync_block_placement([place_pos.x, place_pos.y, place_pos.z], block_data);
```

### Remote Block Changes

Remote block changes from the server are applied via `apply_remote_block_changes()` in `src/app/core.rs`:

- Handles all block types (Model, TintedGlass, Crystal, Painted, Water, etc.)
- Applies metadata (model_data, paint_data, tint_index, water_type)
- Invalidates minimap cache for affected chunks

## CLI Arguments

```bash
# Single-player (default)
make run

# Host a multiplayer game (integrated server)
make run ARGS="--host"

# Host on a specific port (default: 5000)
make run ARGS="--host --port 5001"

# Join a multiplayer game
make run ARGS="--connect 192.168.1.100:5000"

# Join localhost server
make run ARGS="--connect 127.0.0.1:5000"
```

## Security

### renet_netcode Authentication

- Secure handshake with encryption
- Private/public key authentication
- Token-based connection approval

### Server Validation

- All block changes validated server-side
- Rate limiting on chunk requests
- Anti-cheat: position validation, speed checks

## Performance Considerations

### Bandwidth

- Player movement: ~1 KB/sec per player
- Block changes: ~50 bytes per change
- Chunk streaming: ~50-100 KB per chunk (compressed)
- Total for 4 players: ~2-5 MB/sec

### Latency Handling

- Client-side prediction for responsive movement
- Server reconciliation at 20 Hz
- Interpolation for remote players (100ms buffer)

## Implementation Phases

### Phase 1: Foundation ✅
- [x] Add networking dependencies (renet 2.0, renet_netcode 2.0, lz4_flex, tokio)
- [x] Create `src/net/` module structure
- [x] Define protocol message types with bincode
- [x] Configure renet channels (PlayerMovement, BlockUpdates, GameState, ChunkStream)
- [x] Basic server/client connection handshake

### Phase 2: Player Synchronization ✅
- [x] Player join/leave messages
- [x] Position broadcasting (unreliable channel)
- [x] Client-side prediction implementation
- [x] Server reconciliation
- [x] Remote player interpolation

### Phase 3: Block Synchronization ✅
- [x] Block place/break broadcast
- [x] Bulk operation messages
- [x] Area of Interest (AoI)
- [x] Block metadata sync

### Phase 4: Chunk Streaming ✅
- [x] Chunk request/response system
- [x] LZ4 compression
- [x] Priority queue with cancellation
- [x] Chunk deserialization and world integration
- [x] Server-side chunk serialization and fulfillment
- [ ] Delta compression (future)

### Phase 5: Integrated Server ✅
- [x] GameMode enum (SinglePlayer, Host, Client)
- [x] CLI arguments (--host, --connect, --port)
- [x] MultiplayerState in app_state
- [x] Game loop integration (multiplayer.update())
- [x] Block sync integration (send/receive block changes)
- [x] Chunk request fulfillment (server processes client requests)
- [x] UI for host/join (O key opens multiplayer panel)
- [x] LAN server discovery (UDP broadcast on port 5001)
- [ ] Server thread management

### Phase 6: Dedicated Server
- [ ] Separate binary target
- [ ] Headless mode
- [ ] Configuration file
- [ ] Admin commands

## Testing

### Local Testing (CLI)

```bash
# Terminal 1: Host game
make run ARGS="--host"

# Terminal 2: Join game (in a separate terminal)
make run ARGS="--connect 127.0.0.1:5000"
```

### Local Testing (UI)

```bash
# Terminal 1: Start game and host via UI
make run
# Press O to open multiplayer panel
# Click "Host" tab, configure server name/port, click "Start Hosting"

# Terminal 2: Start another game and join
make run
# Press O to open multiplayer panel
# Click "Join" tab, click "Scan for Servers" or enter address directly
# Click "Connect"
```

### Verification Checklist

- [ ] Both players see each other's positions
- [ ] Block changes sync in real-time
- [ ] Chunks load correctly for joining player
- [ ] Large templates sync without lag
- [ ] Player movement is smooth (prediction working)
- [ ] No desync after extended play

## Current Status

**Completed:**
- ✅ Phase 1: Foundation - Networking module, protocol, channels, authentication
- ✅ Phase 2: Player Synchronization - Prediction, reconciliation, interpolation, remote player rendering
- ✅ Phase 3: Block Synchronization - Block change broadcast, metadata sync, AoI filtering, validation
- ✅ Phase 4: Chunk Streaming - LZ4 compression, priority queue, chunk request/response, world integration, server-side serialization
- ✅ Phase 5: Integrated Server - CLI arguments, MultiplayerState, game loop integration, block sync hooks, chunk request fulfillment, multiplayer UI, LAN discovery
- ✅ Server thread management (experimental) - ServerThread wrapper with crossbeam channels

**Future:**
- Phase 6: Dedicated Server

### What's Working Now

1. **Server/Client Startup**: `--host` starts integrated server, `--connect <addr>` joins remote server
2. **Player Synchronization**: Position broadcasting, client-side prediction, server reconciliation
3. **Block Synchronization**: Block place/break events sync between server and all clients
4. **Metadata Sync**: Model data, paint data, tint indices, and water types all sync correctly
5. **Remote Change Application**: Server-authoritative block changes applied to local world
6. **Chunk Request System**: Client requests chunks from server based on player position and view direction
7. **Chunk Deserialization**: Network chunks decompressed and applied to local world
8. **Server-Side Chunk Streaming**: Server processes chunk requests and sends compressed chunk data to clients
9. **Multiplayer UI**: Press O to open multiplayer panel with Host/Join tabs
10. **LAN Discovery**: Automatic server scanning finds games on local network
11. **Connection Status Overlay**: Top-right display shows connection info when connected
12. **Player List**: Press Tab to see connected players
13. **Threaded Server Mode**: Optional dedicated thread for server network processing (experimental)

### Known Limitations

- Threaded server mode is experimental (disabled by default, enable via `USE_THREADED_SERVER` constant)
- No dedicated server binary yet

## Next Steps

The multiplayer system is feature-complete for LAN play. Future improvements include:

1. **Threaded server mode testing**:
   - Enable by default after stability testing
   - Add configuration option for thread mode selection

2. **Delta compression** (optimization):
   - Send only changed portions of chunks
   - Reduce bandwidth for partially modified chunks

3. **Dedicated server**:
   - Separate headless binary for server-only operation
   - Configuration file support
   - Admin commands

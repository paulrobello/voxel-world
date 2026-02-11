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

## File Structure

```
src/
├── net/
│   ├── mod.rs              # Module exports
│   ├── channel.rs          # Renet channel configuration
│   ├── protocol.rs         # Message types (bincode serialization)
│   ├── server.rs           # GameServer wrapper (RenetServer)
│   ├── client.rs           # GameClient wrapper (RenetClient)
│   ├── chunk_sync.rs       # Chunk streaming with priority queue
│   ├── player_sync.rs      # Player position sync + prediction
│   ├── block_sync.rs       # Block change broadcasting + AoI
│   └── auth.rs             # Connection handshake (renet_netcode)
├── app_state/
│   └── multiplayer.rs      # MultiplayerState (server/client management)
├── config.rs               # CLI args (--host, --connect, --port)
└── app/
    ├── core.rs             # App struct with multiplayer field
    └── init.rs             # Multiplayer initialization
```

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

### Phase 4: Chunk Streaming
- [ ] Chunk request/response system
- [x] LZ4 compression
- [x] Priority queue with cancellation
- [ ] Delta compression (future)

### Phase 5: Integrated Server ✅
- [x] GameMode enum (SinglePlayer, Host, Client)
- [x] CLI arguments (--host, --connect, --port)
- [x] MultiplayerState in app_state
- [x] Game loop integration (multiplayer.update())
- [x] Block sync integration (send/receive block changes)
- [ ] Server thread management
- [ ] UI for host/join

### Phase 6: Dedicated Server
- [ ] Separate binary target
- [ ] Headless mode
- [ ] Configuration file
- [ ] Admin commands

## Testing

### Local Testing

```bash
# Terminal 1: Host game
make run ARGS="--host"

# Terminal 2: Join game (in a separate terminal)
make run ARGS="--connect 127.0.0.1:5000"
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
- ✅ Phase 1: Foundation - Networking module, protocol, channels
- ✅ Phase 2: Player Synchronization - Prediction, reconciliation, interpolation
- ✅ Phase 3: Block Synchronization - Block change broadcast, metadata sync, AoI
- ✅ Phase 5: Integrated Server - CLI arguments, MultiplayerState, game loop integration

**In Progress:**
- Phase 4: Chunk Streaming (LZ4 compression and priority queue implemented, needs world integration)

**Future:**
- Phase 6: Dedicated Server

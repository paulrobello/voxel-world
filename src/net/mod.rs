//! Multiplayer networking module for voxel-world.
//!
//! This module provides networking capabilities using renet and renet_netcode:
//!
//! - **Channel configuration**: Different delivery guarantees for different message types
//! - **Protocol messages**: Client↔Server message types with bincode serialization
//! - **Chunk streaming**: Priority-based chunk loading with cancellation
//! - **Player sync**: Client-side prediction and server reconciliation
//! - **Block sync**: Block change broadcasting with AoI filtering
//! - **Authentication**: Secure handshake via renet_netcode
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐     ┌─────────────────┐
//! │   GameClient    │◄───►│   GameServer    │
//! │  - Prediction   │     │  - Authority    │
//! │  - Interpolation│     │  - Validation   │
//! └────────┬────────┘     └────────┬────────┘
//!          │                       │
//!          └───────────┬───────────┘
//!                      │
//!              ┌───────▼───────┐
//!              │ renet + netcode│
//!              │  (UDP secure) │
//!              └───────────────┘
//! ```
//!
//! # Usage
//!
//! ## Server
//!
//! ```ignore
//! use voxel_world::net::{GameServer, ServerMessage};
//!
//! let mut server = GameServer::new("0.0.0.0:5000".parse().unwrap(), 12345, 0)?;
//!
//! loop {
//!     let events = server.update(std::time::Duration::from_millis(16));
//!     for event in events {
//!         match event {
//!             ServerEvent::ClientConnected { client_id } => {
//!                 server.handle_client_connected(client_id, [0.0, 64.0, 0.0]);
//!             }
//!             // ...
//!         }
//!     }
//! }
//! ```
//!
//! ## Client
//!
//! ```ignore
//! use voxel_world::net::{GameClient, ClientMessage};
//!
//! let mut client = GameClient::localhost()?;
//! client.connect();
//!
//! loop {
//!     client.update(std::time::Duration::from_millis(16))?;
//!
//!     let messages = client.receive_messages();
//!     for msg in messages {
//!         match msg {
//!             ServerMessage::ConnectionAccepted(accepted) => {
//!                 println!("Connected as player {}", accepted.player_id);
//!             }
//!             // ...
//!         }
//!     }
//! }
//! ```

pub mod auth;
pub mod block_sync;
pub mod channel;
pub mod chunk_sync;
pub mod client;
pub mod discovery;
pub mod player_sync;
pub mod protocol;
pub mod server;
pub mod server_thread;
pub mod texture_slots;
pub mod water_sync;

// Re-export main types for convenience
// These are intentionally unused until multiplayer is integrated into the game
#[allow(unused_imports)]
pub use auth::{ClientAuth, ConnectionState, ConnectionTracker, ServerAuth};
#[allow(unused_imports)]
pub use block_sync::{BlockChange, BlockSyncManager, BlockValidator};
#[allow(unused_imports)]
pub use channel::Channel;
#[allow(unused_imports)]
pub use chunk_sync::{ChunkPriority, ChunkRequest, ChunkSyncManager, SerializedChunk};
#[allow(unused_imports)]
pub use client::{GameClient, RemotePlayerInfo};
#[allow(unused_imports)]
pub use discovery::{DiscoveredServer, DiscoveryResponder, LanDiscovery, ServerAnnouncement};
#[allow(unused_imports)]
pub use player_sync::{PredictionState, RemotePlayer};
#[allow(unused_imports)]
pub use protocol::{
    BlockChanged, BlockData, BlocksChanged, BreakBlock, BulkOperation, ChunkData,
    ChunkGenerateLocal, ClientMessage, ConnectionAccepted, ConnectionRejected, ConsoleCommand,
    InputActions, PlaceBlock, PlayerId, PlayerInput, PlayerJoined, PlayerLeft, PlayerState,
    RequestChunks, ServerMessage, TimeUpdate,
};
#[allow(unused_imports)]
pub use server::{GameServer, PlayerInfo};
#[allow(unused_imports)]
pub use server_thread::{ServerCommand, ServerThread, ServerThreadEvent};
#[allow(unused_imports)]
pub use texture_slots::{
    CustomTextureCache, DEFAULT_MAX_TEXTURE_SLOTS, TEXTURE_SIZE, TexturePoolMetadata,
    TextureSlotManager,
};
#[allow(unused_imports)]
pub use water_sync::{WaterSyncOptimizer, WaterSyncStats};

/// Default server port.
#[allow(dead_code)]
pub const DEFAULT_PORT: u16 = 5000;

/// Maximum players per server.
#[allow(dead_code)]
pub const MAX_PLAYERS: usize = 4;

/// Server tick rate (updates per second).
#[allow(dead_code)]
pub const TICK_RATE: u64 = 20;

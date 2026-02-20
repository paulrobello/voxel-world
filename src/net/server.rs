//! Server-side networking using renet.
//!
//! Provides a RenetServer wrapper with voxel-world specific functionality.

// Allow unused code until networking is integrated into the game
#![allow(dead_code)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use lz4_flex::compress_prepend_size;
use renet::{RenetServer, ServerEvent};
use renet_netcode::NetcodeServerTransport;

use crate::net::auth::ServerAuth;
use crate::net::channel::create_connection_config;
use crate::net::protocol::{
    BlockChanged, BlocksChanged, ChunkData, ChunkGenerateLocal, ClientMessage, ConnectionAccepted,
    PlayerId, PlayerJoined, PlayerLeft, PlayerState, ServerMessage, TimeUpdate,
};
use crate::net::texture_slots::TextureSlotManager;
use crate::storage::model_format::{DoorPairStore, WorldModelStore};

/// Server tick rate (updates per second).
const TICK_RATE: u64 = 20;

/// Voxel-world game server.
pub struct GameServer {
    /// Renet server instance.
    server: RenetServer,
    /// Netcode transport layer.
    transport: NetcodeServerTransport,
    /// Connected players (client_id -> player info).
    players: HashMap<u64, PlayerInfo>,
    /// Host player info (the server's own player).
    host_player: Option<PlayerInfo>,
    /// Host's client ID (the loopback connection from host to itself).
    /// This client should not be broadcast to other clients.
    host_client_id: Option<u64>,
    /// Server start time.
    start_time: Instant,
    /// Last tick time.
    last_tick: Instant,
    /// World seed for new clients.
    world_seed: u32,
    /// World generation type.
    world_gen: u8,
    /// Custom texture slot manager.
    texture_manager: Option<TextureSlotManager>,
    /// World directory path (for loading models.dat).
    world_dir: Option<std::path::PathBuf>,
}

/// Information about a connected player.
#[derive(Debug, Clone)]
pub struct PlayerInfo {
    /// Assigned player ID.
    pub player_id: PlayerId,
    /// Client ID in renet.
    pub client_id: u64,
    /// Player name.
    pub name: String,
    /// Current position.
    pub position: [f32; 3],
    /// Current velocity.
    pub velocity: [f32; 3],
    /// Camera yaw.
    pub yaw: f32,
    /// Camera pitch.
    pub pitch: f32,
    /// Last processed input sequence.
    pub last_sequence: u32,
    /// Connection time.
    pub connected_at: Instant,
}

impl GameServer {
    /// Creates a new game server.
    pub fn new(address: SocketAddr, world_seed: u32, world_gen: u8) -> Result<Self, String> {
        let auth = ServerAuth::new(address);
        let transport = auth.create_transport()?;

        let connection_config = create_connection_config();
        let server = RenetServer::new(connection_config);

        Ok(Self {
            server,
            transport,
            players: HashMap::new(),
            host_player: None,
            host_client_id: None,
            start_time: Instant::now(),
            last_tick: Instant::now(),
            world_seed,
            world_gen,
            texture_manager: None,
            world_dir: None,
        })
    }

    /// Sets the host player info.
    pub fn set_host_player(&mut self, player_id: PlayerId, name: String, position: [f32; 3]) {
        self.host_player = Some(PlayerInfo {
            player_id,
            client_id: 0, // Host doesn't have a client_id
            name,
            position,
            velocity: [0.0, 0.0, 0.0],
            last_sequence: 0,
            yaw: 0.0,
            pitch: 0.0,
            connected_at: Instant::now(),
        });
    }

    /// Sets the world directory for loading models and textures.
    pub fn set_world_dir(&mut self, path: std::path::PathBuf, max_textures: u8) {
        self.world_dir = Some(path.clone());
        let mut manager = TextureSlotManager::new(path.join("custom_textures"), max_textures);
        if let Err(e) = manager.init() {
            eprintln!("[Server] Failed to initialize texture manager: {}", e);
        }
        self.texture_manager = Some(manager);
    }

    /// Sets the host's client ID (the loopback connection from host to itself).
    /// This is used to exclude the host's own client from broadcasts to other clients.
    pub fn set_host_client_id(&mut self, client_id: u64) {
        self.host_client_id = Some(client_id);
        println!("[GameServer] Set host_client_id={}", client_id);
    }

    /// Returns the host's client ID if set.
    pub fn host_client_id(&self) -> Option<u64> {
        self.host_client_id
    }

    /// Updates the host player's state.
    pub fn update_host_player(
        &mut self,
        position: [f32; 3],
        velocity: [f32; 3],
        yaw: f32,
        pitch: f32,
    ) {
        if let Some(ref mut host) = self.host_player {
            host.position = position;
            host.velocity = velocity;
            host.yaw = yaw;
            host.pitch = pitch;
        }
    }

    /// Updates the server (should be called every frame).
    /// Returns server events that need processing.
    pub fn update(&mut self, duration: Duration) -> Vec<ServerEvent> {
        // Update the server logic
        self.server.update(duration);

        // Update the transport layer - receives packets and handles connections
        let _ = self.transport.update(duration, &mut self.server);

        let mut events = Vec::new();
        while let Some(event) = self.server.get_event() {
            events.push(event);
        }

        self.last_tick = Instant::now();
        events
    }

    /// Sends queued packets to all connected clients.
    /// Call this AFTER processing events and sending messages.
    pub fn flush_packets(&mut self) {
        self.transport.send_packets(&mut self.server);
    }

    /// Handles a new client connection.
    pub fn handle_client_connected(
        &mut self,
        client_id: u64,
        spawn_position: [f32; 3],
    ) -> Option<PlayerInfo> {
        // Generate unique player ID
        let player_id = generate_player_id(client_id);

        let info = PlayerInfo {
            player_id,
            client_id,
            name: format!("Player_{}", player_id),
            position: spawn_position,
            velocity: [0.0, 0.0, 0.0],
            yaw: 0.0,
            pitch: 0.0,
            last_sequence: 0,
            connected_at: Instant::now(),
        };

        println!(
            "[GameServer] Client {} connected as player_id={}",
            client_id, player_id
        );

        // Get custom texture count from texture manager
        let custom_texture_count = self
            .texture_manager
            .as_ref()
            .map(|m| m.max_slots())
            .unwrap_or(0);

        // Send connection accepted message
        let msg = ServerMessage::ConnectionAccepted(ConnectionAccepted {
            player_id,
            tick_rate: TICK_RATE as u32,
            spawn_position,
            world_seed: self.world_seed,
            world_gen: self.world_gen,
            custom_texture_count,
        });

        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.server
                .send_message(client_id, 2, renet::Bytes::from(encoded)); // Channel 2 = GameState
        } else {
            eprintln!("[GameServer] Failed to encode ConnectionAccepted message!");
        }

        // Send model registry sync after connection accepted
        self.send_model_registry(client_id);

        // Broadcast player joined to other clients
        let join_msg = ServerMessage::PlayerJoined(PlayerJoined {
            player_id,
            name: info.name.clone(),
            position: spawn_position,
        });

        if let Ok(encoded) = bincode::serde::encode_to_vec(&join_msg, bincode::config::standard()) {
            let bytes = renet::Bytes::from(encoded);
            // Send to all other clients
            for &other_client_id in self.players.keys() {
                if other_client_id != client_id {
                    self.server.send_message(other_client_id, 2, bytes.clone());
                }
            }
        }

        self.players.insert(client_id, info.clone());
        Some(info)
    }

    /// Handles a client disconnection.
    pub fn handle_client_disconnected(&mut self, client_id: u64) -> Option<PlayerId> {
        if let Some(info) = self.players.remove(&client_id) {
            // Broadcast player left
            let msg = ServerMessage::PlayerLeft(PlayerLeft {
                player_id: info.player_id,
            });

            if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
                self.server
                    .broadcast_message(2, renet::Bytes::from(encoded)); // Channel 2 = GameState
            }

            Some(info.player_id)
        } else {
            None
        }
    }

    /// Updates a player's state from input.
    pub fn update_player_state(
        &mut self,
        client_id: u64,
        position: [f32; 3],
        velocity: [f32; 3],
        yaw: f32,
        pitch: f32,
        sequence: u32,
    ) {
        if let Some(info) = self.players.get_mut(&client_id) {
            info.position = position;
            info.velocity = velocity;
            info.yaw = yaw;
            info.pitch = pitch;
            info.last_sequence = sequence;
        }
    }

    /// Broadcasts a single block change to all clients.
    pub fn broadcast_block_change(&mut self, change: BlockChanged) {
        let msg = ServerMessage::BlockChanged(change);
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.server
                .broadcast_message(1, renet::Bytes::from(encoded)); // Channel 1 = BlockUpdates
        }
    }

    /// Broadcasts multiple block changes to all clients.
    pub fn broadcast_block_changes(&mut self, changes: BlocksChanged) {
        let msg = ServerMessage::BlocksChanged(changes);
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.server
                .broadcast_message(1, renet::Bytes::from(encoded));
        }
    }

    /// Broadcasts water cell changes to all clients.
    /// Used for server-authoritative water simulation sync.
    pub fn broadcast_water_cells_changed(
        &mut self,
        updates: Vec<crate::net::protocol::WaterCellUpdate>,
    ) {
        let msg =
            ServerMessage::WaterCellsChanged(crate::net::protocol::WaterCellsChanged { updates });
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.server
                .broadcast_message(1, renet::Bytes::from(encoded)); // Channel 1 = BlockUpdates
        }
    }

    /// Broadcasts lava cell changes to all clients.
    /// Used for server-authoritative lava simulation sync.
    pub fn broadcast_lava_cells_changed(
        &mut self,
        updates: Vec<crate::net::protocol::LavaCellUpdate>,
    ) {
        let msg =
            ServerMessage::LavaCellsChanged(crate::net::protocol::LavaCellsChanged { updates });
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.server
                .broadcast_message(1, renet::Bytes::from(encoded)); // Channel 1 = BlockUpdates
        }
    }

    /// Sends chunk data to a specific client.
    pub fn send_chunk(&mut self, client_id: u64, chunk: ChunkData) {
        let msg = ServerMessage::ChunkData(chunk);
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            let len = encoded.len();
            self.server
                .send_message(client_id, 3, renet::Bytes::from(encoded)); // Channel 3 = ChunkStream
            println!(
                "[GameServer] Sent ChunkData to client {} ({} bytes)",
                client_id, len
            );
        }
    }

    /// Instructs a client to generate a chunk locally (for unmodified chunks).
    /// This saves bandwidth by not sending the full chunk data.
    pub fn send_chunk_generate_local(&mut self, client_id: u64, position: [i32; 3]) {
        let msg = ServerMessage::ChunkGenerateLocal(ChunkGenerateLocal { position });
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            let len = encoded.len();
            self.server
                .send_message(client_id, 3, renet::Bytes::from(encoded)); // Channel 3 = ChunkStream
            println!(
                "[GameServer] Sent ChunkGenerateLocal for {:?} to client {} ({} bytes)",
                position, client_id, len
            );
        }
    }

    /// Broadcasts player states to all clients.
    /// Includes both connected players and the host player.
    pub fn broadcast_player_states(&mut self) {
        // First, broadcast the host player's state to all connected clients
        if let Some(ref host) = self.host_player {
            let state = PlayerState {
                player_id: host.player_id,
                position: host.position,
                velocity: host.velocity,
                last_sequence: host.last_sequence,
                yaw: host.yaw,
                pitch: host.pitch,
            };

            let msg = ServerMessage::PlayerState(state);
            if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
                let bytes = renet::Bytes::from(encoded);
                // Send to all connected clients
                for &client_id in self.players.keys() {
                    self.server.send_message(client_id, 0, bytes.clone());
                }
            }
        }

        // Then, broadcast each connected player's state to all other clients (and potentially the host)
        // Skip the host's own client connection (it's just a loopback, external clients shouldn't see it)
        for (&client_id, info) in &self.players {
            // Skip the host's loopback client - external clients should only see player_id=0 for the host
            if self.host_client_id == Some(client_id) {
                continue;
            }

            let state = PlayerState {
                player_id: info.player_id,
                position: info.position,
                velocity: info.velocity,
                last_sequence: info.last_sequence,
                yaw: info.yaw,
                pitch: info.pitch,
            };

            let msg = ServerMessage::PlayerState(state);
            if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
                let bytes = renet::Bytes::from(encoded);
                // Send to all other clients (excluding only the player themselves)
                // Include host's loopback client so the host can see other players
                for &other_client_id in self.players.keys() {
                    if other_client_id != client_id {
                        self.server.send_message(other_client_id, 0, bytes.clone());
                    }
                }
            }
        }
    }

    /// Broadcasts time of day to all clients.
    pub fn broadcast_time(&mut self, time_of_day: f32) {
        let msg = ServerMessage::TimeUpdate(TimeUpdate { time_of_day });
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.server
                .broadcast_message(2, renet::Bytes::from(encoded));
        }
    }

    /// Broadcasts a new custom model to all clients.
    pub fn broadcast_model_added(
        &mut self,
        model_id: u8,
        name: String,
        author: String,
        model_data: Vec<u8>,
    ) {
        let msg = ServerMessage::ModelAdded(crate::net::protocol::ModelAdded {
            model_id,
            name,
            author,
            model_data,
        });
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.server
                .broadcast_message(2, renet::Bytes::from(encoded)); // Channel 2 = GameState
            println!("[Server] Broadcast ModelAdded to all clients");
        }
    }

    /// Broadcasts a new custom texture to all clients.
    pub fn broadcast_texture_added(&mut self, slot: u8, name: String, png_data: Vec<u8>) {
        let msg = ServerMessage::TextureData(crate::net::protocol::TextureData {
            slot,
            data: png_data,
        });
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.server
                .broadcast_message(2, renet::Bytes::from(encoded)); // Channel 2 = GameState
            println!(
                "[Server] Broadcast TextureData (slot {}, '{}') to all clients",
                slot, name
            );
        }
    }

    /// Sends model registry and door pairs to a client.
    pub fn send_model_registry(&mut self, client_id: u64) {
        let world_dir = match &self.world_dir {
            Some(d) => d,
            None => return,
        };

        // Load and compress models.dat
        let models_data = match WorldModelStore::load(world_dir) {
            Ok(Some(store)) => {
                let serialized = bincode::serde::encode_to_vec(&store, bincode::config::legacy())
                    .unwrap_or_default();
                compress_prepend_size(&serialized)
            }
            _ => Vec::new(),
        };

        // Load and compress door_pairs.dat
        let door_pairs_data = match DoorPairStore::load(world_dir) {
            Ok(Some(store)) => {
                let serialized = bincode::serde::encode_to_vec(&store, bincode::config::legacy())
                    .unwrap_or_default();
                compress_prepend_size(&serialized)
            }
            _ => Vec::new(),
        };

        let msg = crate::net::protocol::ModelRegistrySync {
            models_data,
            door_pairs_data,
        };

        if let Ok(encoded) = bincode::serde::encode_to_vec(
            ServerMessage::ModelRegistrySync(msg),
            bincode::config::standard(),
        ) {
            self.server
                .send_message(client_id, 2, renet::Bytes::from(encoded)); // Channel 2 = GameState
        }
    }

    /// Handles a texture request from a client.
    pub fn handle_texture_request(&mut self, client_id: u64, slot: u8) {
        let manager = match &self.texture_manager {
            Some(m) => m,
            None => return,
        };

        if let Some(data) = manager.get_texture(slot) {
            let msg = crate::net::protocol::TextureData { slot, data };
            if let Ok(encoded) = bincode::serde::encode_to_vec(
                ServerMessage::TextureData(msg),
                bincode::config::standard(),
            ) {
                self.server
                    .send_message(client_id, 2, renet::Bytes::from(encoded)); // Channel 2 = GameState
            }
        }
    }

    /// Adds a new texture to the pool.
    /// Returns the assigned slot, or error if pool is full or validation fails.
    pub fn add_texture(&mut self, name: &str, png_data: &[u8]) -> Result<u8, String> {
        let manager = match &mut self.texture_manager {
            Some(m) => m,
            None => return Err("Texture manager not initialized".to_string()),
        };

        manager.add_texture(name, png_data)
    }

    /// Receives messages from clients.
    /// Returns an iterator of (client_id, channel_id, message_data).
    pub fn receive_messages(&mut self) -> impl Iterator<Item = (u64, u8, Vec<u8>)> + '_ {
        let mut messages = Vec::new();

        for client_id in self.server.clients_id() {
            for channel in crate::net::channel::Channel::all() {
                while let Some(message) = self.server.receive_message(client_id, channel.id()) {
                    messages.push((client_id, channel.id(), message.to_vec()));
                }
            }
        }

        messages.into_iter()
    }

    /// Receives and parses client messages into typed ClientMessage enums.
    /// Returns a vector of (client_id, parsed_message).
    pub fn receive_client_messages(&mut self) -> Vec<(u64, ClientMessage)> {
        let mut parsed_messages = Vec::new();

        for (client_id, _channel_id, data) in self.receive_messages() {
            if let Ok((msg, _)) = bincode::serde::decode_from_slice::<ClientMessage, _>(
                &data,
                bincode::config::standard(),
            ) {
                parsed_messages.push((client_id, msg));
            }
        }

        parsed_messages
    }

    /// Returns connected player count.
    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    /// Returns player info by client ID.
    pub fn get_player(&self, client_id: u64) -> Option<&PlayerInfo> {
        self.players.get(&client_id)
    }

    /// Returns all connected players.
    pub fn players(&self) -> impl Iterator<Item = &PlayerInfo> {
        self.players.values()
    }

    /// Returns server uptime.
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Returns the number of messages waiting to be sent.
    pub fn has_pending_messages(&self) -> bool {
        // Check if there are any connected clients
        !self.server.clients_id().is_empty()
    }
}

/// Generates a unique player ID from client ID.
fn generate_player_id(client_id: u64) -> PlayerId {
    // Mix client ID with some entropy
    client_id.wrapping_mul(0x5851F42E4C957F2D) ^ 0x123456789ABCDEF0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_id_generation() {
        let id1 = generate_player_id(1);
        let id2 = generate_player_id(2);
        let id1_again = generate_player_id(1);

        assert_ne!(id1, id2);
        assert_eq!(id1, id1_again);
    }
}

//! Server-side networking using renet.
//!
//! Provides a RenetServer wrapper with voxel-world specific functionality.

// Allow unused code until networking is integrated into the game
#![allow(dead_code)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use renet::{RenetServer, ServerEvent};
use renet_netcode::NetcodeServerTransport;

use crate::net::auth::ServerAuth;
use crate::net::channel::create_connection_config;
use crate::net::protocol::{
    BlockChanged, BlocksChanged, ChunkData, ChunkGenerateLocal, ClientMessage, ConnectionAccepted,
    PlayerId, PlayerJoined, PlayerLeft, PlayerState, ServerMessage, TimeUpdate,
};

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
    /// Server start time.
    start_time: Instant,
    /// Last tick time.
    last_tick: Instant,
    /// World seed for new clients.
    world_seed: u32,
    /// World generation type.
    world_gen: u8,
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
            start_time: Instant::now(),
            last_tick: Instant::now(),
            world_seed,
            world_gen,
        })
    }

    /// Updates the server (should be called every frame).
    /// Returns server events that need processing.
    pub fn update(&mut self, duration: Duration) -> Vec<ServerEvent> {
        // Log update start periodically
        static UPDATE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let count = UPDATE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if count % 60 == 0 {
            println!(
                "[GameServer] Update #{}, player_count: {}",
                count,
                self.players.len()
            );
        }

        // Log clients before update
        let clients_before = self.server.clients_id();
        if !clients_before.is_empty() && count % 60 == 0 {
            println!("[GameServer] Clients before update: {:?}", clients_before);
        }

        // Update the server logic
        self.server.update(duration);

        // Update the transport layer - receives packets and handles connections
        let transport_result = self.transport.update(duration, &mut self.server);
        if count % 60 == 0 {
            println!(
                "[GameServer] Transport update result: {:?}",
                transport_result
            );
        }

        let mut events = Vec::new();
        while let Some(event) = self.server.get_event() {
            println!("[GameServer] Received server event: {:?}", event);
            events.push(event);
        }

        // Log connected clients count
        let client_count = self.server.clients_id().len();
        if client_count > 0 && count % 60 == 0 {
            println!("[GameServer] Connected clients: {}", client_count);
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
        println!(
            "[GameServer] handle_client_connected called for client {}",
            client_id
        );

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

        // Send connection accepted message
        let msg = ServerMessage::ConnectionAccepted(ConnectionAccepted {
            player_id,
            tick_rate: TICK_RATE as u32,
            spawn_position,
            world_seed: self.world_seed,
            world_gen: self.world_gen,
        });

        println!(
            "[GameServer] Sending ConnectionAccepted to client {}: player_id={}, seed={}, gen={}",
            client_id, player_id, self.world_seed, self.world_gen
        );

        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            let len = encoded.len();
            println!(
                "[GameServer] ConnectionAccepted encoded successfully, {} bytes",
                len
            );
            // Log first few bytes for debugging
            if len >= 8 {
                println!("[GameServer] First 8 bytes: {:02x?}", &encoded[..8]);
            }
            self.server
                .send_message(client_id, 2, renet::Bytes::from(encoded)); // Channel 2 = GameState
            println!(
                "[GameServer] ConnectionAccepted message QUEUED for client {}",
                client_id
            );
        } else {
            eprintln!("[GameServer] Failed to encode ConnectionAccepted message!");
        }

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
    pub fn broadcast_player_states(&mut self) {
        for (&client_id, info) in &self.players {
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
                // Send to all other clients
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

        for (client_id, channel_id, data) in self.receive_messages() {
            println!(
                "[GameServer] Received {} bytes from client {} on channel {}",
                data.len(),
                client_id,
                channel_id
            );
            if let Ok((msg, _)) = bincode::serde::decode_from_slice::<ClientMessage, _>(
                &data,
                bincode::config::standard(),
            ) {
                println!(
                    "[GameServer] Decoded message from client {}: {:?}",
                    client_id,
                    std::mem::discriminant(&msg)
                );
                parsed_messages.push((client_id, msg));
            } else {
                println!(
                    "[GameServer] Failed to decode message from client {}!",
                    client_id
                );
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

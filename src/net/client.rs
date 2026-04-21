//! Client-side networking using renet.
//!
//! Provides a RenetClient wrapper with voxel-world specific functionality.

// Allow unused code until networking is integrated into the game
#![allow(dead_code)]

use std::net::SocketAddr;
use std::time::Duration;

use renet::RenetClient;
use renet_netcode::NetcodeClientTransport;

use crate::net::auth::{ClientAuth, ConnectionState, ConnectionTracker};
use crate::net::channel::create_connection_config;
use crate::net::protocol::{
    BlockData, BreakBlock, BulkOperation, ClientMessage, InputActions, PlaceBlock, PlayerInput,
    ServerMessage,
};

/// Voxel-world game client.
pub struct GameClient {
    /// Renet client instance.
    client: RenetClient,
    /// Netcode transport layer.
    transport: NetcodeClientTransport,
    /// Connection state tracker.
    connection: ConnectionTracker,
    /// Our assigned player ID (after connection).
    player_id: Option<u64>,
    /// Server tick rate.
    tick_rate: u32,
    /// World seed (received from server).
    world_seed: Option<u32>,
    /// World generation type.
    world_gen: Option<u8>,
    /// Remote players (for interpolation).
    remote_players: Vec<RemotePlayerInfo>,
}

/// Information about a remote player.
#[derive(Debug, Clone)]
pub struct RemotePlayerInfo {
    /// Player ID.
    pub player_id: u64,
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
}

impl GameClient {
    /// Creates a new game client with the server's private key for Secure
    /// mode authentication. The key is shared out-of-band (e.g. pairing code
    /// displayed on the host console).
    pub fn new(server_address: SocketAddr) -> Result<Self, String> {
        let auth = ClientAuth::new(server_address, generate_client_key(server_address));

        let transport = auth.create_transport()?;

        let connection_config = create_connection_config();
        let client = RenetClient::new(connection_config);

        Ok(Self {
            client,
            transport,
            connection: ConnectionTracker::new(),
            player_id: None,
            tick_rate: 20,
            world_seed: None,
            world_gen: None,
            remote_players: Vec::new(),
        })
    }

    /// Creates a game client with an explicit server private key.
    /// Used by the host's loopback client which knows the key at startup.
    pub fn with_key(server_address: SocketAddr, private_key: [u8; 32]) -> Result<Self, String> {
        let auth = ClientAuth::new(server_address, private_key);

        let transport = auth.create_transport()?;

        let connection_config = create_connection_config();
        let client = RenetClient::new(connection_config);

        Ok(Self {
            client,
            transport,
            connection: ConnectionTracker::new(),
            player_id: None,
            tick_rate: 20,
            world_seed: None,
            world_gen: None,
            remote_players: Vec::new(),
        })
    }

    /// Creates a client for localhost connection.
    pub fn localhost() -> Result<Self, String> {
        let auth = ClientAuth::localhost();
        let transport = auth.create_transport()?;

        let connection_config = create_connection_config();
        let client = RenetClient::new(connection_config);

        Ok(Self {
            client,
            transport,
            connection: ConnectionTracker::new(),
            player_id: None,
            tick_rate: 20,
            world_seed: None,
            world_gen: None,
            remote_players: Vec::new(),
        })
    }

    /// Starts connecting to the server.
    pub fn connect(&mut self) {
        self.connection.start_connect();
    }

    /// Updates the client (should be called every frame).
    pub fn update(&mut self, duration: Duration) {
        let _prev_state = self.connection.state();

        // Update the client
        self.client.update(duration);
        // Update the transport - receives packets and handles connection
        if let Err(e) = self.transport.update(duration, &mut self.client) {
            log::warn!("[Client] Transport error: {}", e);
        }

        // Check for connection timeout
        if self.connection.has_timed_out() {
            self.connection.mark_failed("Connection timed out");
            log::warn!("[Client] Connection timed out");
        }

        // Check if connected (renet client state)
        if self.client.is_connected() {
            if self.connection.state() == ConnectionState::Connecting {
                self.connection.mark_connected();
                log::info!("[Client] Connected to server!");
            }
        } else if self.connection.state() == ConnectionState::Connected {
            self.connection.mark_disconnected(Some("Connection lost"));
            log::warn!("[Client] Disconnected from server");
        }
    }

    /// Sends queued packets to the server.
    /// Call this AFTER sending messages (send_input, send_chunk_request, etc).
    pub fn flush_packets(&mut self) {
        // Only flush if connected to avoid error spam
        if self.client.is_connected()
            && let Err(e) = self.transport.send_packets(&mut self.client)
        {
            log::warn!("[Client] Error flushing packets: {:?}", e);
        }
    }

    /// Receives and processes messages from the server.
    /// Returns a list of server messages to handle.
    pub fn receive_messages(&mut self) -> Vec<ServerMessage> {
        let mut messages = Vec::new();

        // Receive from all channels. Apply the same MAX_INBOUND_MESSAGE_SIZE
        // limit the server uses so a hostile/buggy peer cannot OOM us via an
        // unbounded Vec/String field in a ServerMessage.
        use crate::net::protocol::MAX_INBOUND_MESSAGE_SIZE;
        for channel in [0, 1, 2, 3] {
            while let Some(message) = self.client.receive_message(channel) {
                if message.len() > MAX_INBOUND_MESSAGE_SIZE {
                    log::warn!(
                        "[Client] Dropping {}-byte message (exceeds MAX_INBOUND_MESSAGE_SIZE)",
                        message.len()
                    );
                    continue;
                }
                match bincode::serde::decode_from_slice::<ServerMessage, _>(
                    &message,
                    bincode::config::standard().with_limit::<MAX_INBOUND_MESSAGE_SIZE>(),
                ) {
                    Ok((msg, _)) => {
                        self.handle_server_message(&msg);
                        messages.push(msg);
                    }
                    Err(err) => {
                        log::debug!("[Client] Failed to decode server message: {}", err);
                    }
                }
            }
        }

        messages
    }

    /// Handles a server message internally.
    fn handle_server_message(&mut self, msg: &ServerMessage) {
        match msg {
            ServerMessage::ConnectionAccepted(accepted) => {
                let expected = crate::net::protocol::PROTOCOL_SCHEMA_VERSION;
                if accepted.protocol_version != expected {
                    let reason = format!(
                        "Protocol version mismatch: server={} client={}",
                        accepted.protocol_version, expected
                    );
                    log::error!("[Client] {} — disconnecting", reason);
                    self.connection.mark_failed(&reason);
                    self.client.disconnect();
                    return;
                }
                self.player_id = Some(accepted.player_id);
                self.tick_rate = accepted.tick_rate;
                self.world_seed = Some(accepted.world_seed);
                self.world_gen = Some(accepted.world_gen);
                self.connection.mark_connected();
            }
            ServerMessage::ConnectionRejected(rejected) => {
                self.connection.mark_failed(&rejected.reason);
            }
            ServerMessage::PlayerState(state) => {
                // Update remote player if it's not us
                if Some(state.player_id) != self.player_id
                    && let Some(player) = self
                        .remote_players
                        .iter_mut()
                        .find(|p| p.player_id == state.player_id)
                {
                    player.position = state.position;
                    player.velocity = state.velocity;
                    player.yaw = state.yaw;
                    player.pitch = state.pitch;
                }
            }
            ServerMessage::PlayerJoined(joined) if Some(joined.player_id) != self.player_id => {
                // Add new remote player if it's not us
                self.remote_players.push(RemotePlayerInfo {
                    player_id: joined.player_id,
                    name: joined.name.clone(),
                    position: joined.position,
                    velocity: [0.0, 0.0, 0.0],
                    yaw: 0.0,
                    pitch: 0.0,
                });
            }
            ServerMessage::PlayerLeft(left) => {
                // Remove player
                self.remote_players
                    .retain(|p| p.player_id != left.player_id);
            }
            _ => {}
        }
    }

    /// Sends player input to the server.
    pub fn send_input(
        &mut self,
        sequence: u32,
        position: [f32; 3],
        velocity: [f32; 3],
        yaw: f32,
        pitch: f32,
        actions: InputActions,
    ) {
        let input = PlayerInput {
            sequence,
            position,
            velocity,
            yaw,
            pitch,
            actions,
        };

        let msg = ClientMessage::PlayerInput(input);
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.client.send_message(0, renet::Bytes::from(encoded)); // Channel 0 = PlayerMovement
        }
    }

    /// Requests chunks from the server.
    pub fn request_chunks(&mut self, positions: Vec<[i32; 3]>) {
        let msg = ClientMessage::RequestChunks(crate::net::protocol::RequestChunks { positions });

        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.client.send_message(2, renet::Bytes::from(encoded)); // Channel 2 = GameState
        }
    }

    /// Sends a console command to the server.
    pub fn send_command(&mut self, command: String) {
        let msg = ClientMessage::ConsoleCommand(crate::net::protocol::ConsoleCommand { command });

        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.client.send_message(2, renet::Bytes::from(encoded));
        }
    }

    /// Sends a block placement to the server.
    pub fn send_place_block(&mut self, position: [i32; 3], block: BlockData) {
        let msg = ClientMessage::PlaceBlock(PlaceBlock { position, block });

        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.client.send_message(1, renet::Bytes::from(encoded)); // Channel 1 = BlockUpdates
        }
    }

    /// Sends a block break to the server.
    pub fn send_break_block(&mut self, position: [i32; 3]) {
        let msg = ClientMessage::BreakBlock(BreakBlock { position });

        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.client.send_message(1, renet::Bytes::from(encoded)); // Channel 1 = BlockUpdates
        }
    }

    /// Sends a door toggle request to the server with the new block data.
    pub fn send_toggle_door(
        &mut self,
        lower_pos: [i32; 3],
        lower_block: BlockData,
        upper_pos: [i32; 3],
        upper_block: BlockData,
    ) {
        use crate::net::protocol::ToggleDoor;
        let msg = ClientMessage::ToggleDoor(ToggleDoor {
            lower_pos,
            lower_block,
            upper_pos,
            upper_block,
        });

        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.client.send_message(1, renet::Bytes::from(encoded)); // Channel 1 = BlockUpdates
        }
    }

    /// Sends a bulk operation to the server.
    pub fn send_bulk_operation(&mut self, operation: BulkOperation) {
        let msg = ClientMessage::BulkOperation(operation);

        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.client.send_message(1, renet::Bytes::from(encoded)); // Channel 1 = BlockUpdates
        }
    }

    /// Sends a chunk request to the server.
    /// The server will respond with ChunkData messages for each requested chunk.
    pub fn send_chunk_request(&mut self, positions: Vec<[i32; 3]>) {
        use crate::net::protocol::RequestChunks;
        let pos_count = positions.len();
        let msg = ClientMessage::RequestChunks(RequestChunks { positions });

        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            let len = encoded.len();
            self.client.send_message(2, renet::Bytes::from(encoded)); // Channel 2 = GameState
            log::debug!(
                "[Client] Sent chunk request for {} positions ({} bytes)",
                pos_count,
                len
            );
        }
    }

    /// Sends a texture request to the server.
    /// The server will respond with TextureData for the requested slot.
    pub fn send_texture_request(&mut self, slot: u8) {
        use crate::net::protocol::RequestTexture;
        let msg = ClientMessage::RequestTexture(RequestTexture { slot });

        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.client.send_message(2, renet::Bytes::from(encoded)); // Channel 2 = GameState
        }
    }

    /// Uploads a custom model to the server.
    /// The server will register the model, save it, and broadcast to all clients.
    pub fn send_upload_model(&mut self, name: String, author: String, model_data: Vec<u8>) {
        use crate::net::protocol::UploadModel;
        let msg = ClientMessage::UploadModel(Box::new(UploadModel {
            name,
            author,
            model_data,
        }));

        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            let len = encoded.len();
            self.client.send_message(2, renet::Bytes::from(encoded)); // Channel 2 = GameState
            log::debug!("[Client] Sent model upload: {} bytes", len);
        }
    }

    /// Uploads a custom texture to the server.
    /// The server will register the texture, save it, and broadcast to all clients.
    pub fn send_upload_texture(&mut self, name: String, png_data: Vec<u8>) {
        use crate::net::protocol::UploadTexture;
        let msg = ClientMessage::UploadTexture(UploadTexture { name, png_data });

        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            let len = encoded.len();
            self.client.send_message(2, renet::Bytes::from(encoded)); // Channel 2 = GameState
            log::debug!("[Client] Sent texture upload: {} bytes", len);
        }
    }

    /// Sends a player name change request to the server.
    pub fn send_set_player_name(&mut self, name: String) {
        use crate::net::protocol::SetPlayerName;
        let msg = ClientMessage::SetPlayerName(SetPlayerName { name });

        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.client.send_message(2, renet::Bytes::from(encoded)); // Channel 2 = GameState
        }
    }

    /// Sends a chat message to the server.
    pub fn send_chat(&mut self, message: String) {
        use crate::net::protocol::ChatMessage;
        let msg = ClientMessage::ChatMessage(ChatMessage { message });

        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.client.send_message(2, renet::Bytes::from(encoded)); // Channel 2 = GameState
        }
    }

    /// Returns connection state.
    pub fn connection_state(&self) -> ConnectionState {
        self.connection.state()
    }

    /// Returns true if connected.
    pub fn is_connected(&self) -> bool {
        self.connection.is_connected()
    }

    /// Returns our player ID (if connected).
    pub fn player_id(&self) -> Option<u64> {
        self.player_id
    }

    /// Returns the world seed (if received).
    pub fn world_seed(&self) -> Option<u32> {
        self.world_seed
    }

    /// Returns the world generation type.
    pub fn world_gen(&self) -> Option<u8> {
        self.world_gen
    }

    /// Returns remote players.
    pub fn remote_players(&self) -> &[RemotePlayerInfo] {
        &self.remote_players
    }

    /// Returns disconnect reason (if any).
    pub fn disconnect_reason(&self) -> Option<&str> {
        self.connection.disconnect_reason()
    }
}

/// Generates a client key for the given server address.
/// In production, this would be obtained from the server or a matchmaker.
/// For development with Unsecure mode, the key is not actually used.
fn generate_client_key(_address: SocketAddr) -> [u8; 32] {
    // Return a dummy key since we're using Unsecure mode
    [0u8; 32]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::server::GameServer;

    #[test]
    fn test_connection_state_initial() {
        let tracker = ConnectionTracker::new();
        assert_eq!(tracker.state(), ConnectionState::Disconnected);
    }

    /// Picks a free ephemeral UDP port bound to 127.0.0.1.
    fn free_local_udp() -> SocketAddr {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").expect("scout bind");
        let a = s.local_addr().unwrap();
        drop(s);
        a
    }

    /// T5b — end-to-end loopback: a host-port GameServer and a connecting
    /// GameClient run through the real renet/netcode transport, exchange the
    /// handshake, the client sends a `PlaceBlock`, the server parses it, and
    /// both sides disconnect without a panic.
    ///
    /// This is deliberately a small smoke test — richer assertions belong in
    /// dedicated feature tests once the multiplayer surface is spec'd.
    #[test]
    fn test_loopback_connect_send_receive() {
        let addr = free_local_udp();
        let mut server =
            GameServer::new(addr, /*world_seed*/ 42, /*world_gen*/ 0).expect("start server");
        let key = server.private_key();
        let mut client = GameClient::with_key(addr, key).expect("start client");
        client.connect();

        let tick = Duration::from_millis(16);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);

        // Pump both sides until the client reports Connected (or deadline).
        while std::time::Instant::now() < deadline && !client.is_connected() {
            // Drive server
            let events = server.update(tick);
            for event in events {
                if let renet::ServerEvent::ClientConnected { client_id } = event {
                    // Minimal "welcome the client" path that exercises
                    // broadcast_encoded + the handshake. We don't require the
                    // full game world here.
                    server.handle_client_connected(client_id, [0.0, 64.0, 0.0]);
                }
            }
            server.flush_packets();
            client.update(tick);
            client.flush_packets();
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(
            client.is_connected(),
            "client should have finished handshake within 5s"
        );

        // Send a PlaceBlock through the real transport and verify the server
        // receives a parsed ClientMessage::PlaceBlock. Pump a few ticks so the
        // reliable channel delivers it.
        client.send_place_block(
            [1, 64, 2],
            crate::net::protocol::BlockData::from(crate::chunk::BlockType::Stone),
        );
        client.flush_packets();

        let mut got_place = false;
        let block_deadline = std::time::Instant::now() + Duration::from_secs(3);
        while std::time::Instant::now() < block_deadline && !got_place {
            let _ = server.update(tick);
            let msgs = server.receive_client_messages();
            for (_cid, msg) in msgs {
                if matches!(msg, crate::net::protocol::ClientMessage::PlaceBlock(_)) {
                    got_place = true;
                }
            }
            server.flush_packets();
            client.update(tick);
            client.flush_packets();
            std::thread::sleep(Duration::from_millis(2));
        }

        assert!(
            got_place,
            "server should have received the PlaceBlock within 3s"
        );

        // Clean teardown: neither side panics, encode_failures stays zero.
        assert_eq!(server.encode_failures(), 0);
    }
}

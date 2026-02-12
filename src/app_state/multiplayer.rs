//! Multiplayer state management.
//!
//! Handles the game mode (single-player, host, client), server/client instances,
//! and player synchronization.

// Allow unused code until multiplayer is fully integrated into the game
#![allow(dead_code)]

use std::net::SocketAddr;
use std::time::Duration;

use crate::chunk::Chunk;
use crate::config::GameMode;
use crate::net::{
    BlockSyncManager, ChunkSyncManager, DiscoveredServer, DiscoveryResponder, GameClient,
    GameServer, LanDiscovery, PredictionState, RemotePlayer, SerializedChunk, ServerCommand,
    ServerThread, ServerThreadEvent,
};
use nalgebra::Vector3;

/// Whether to use threaded server mode (experimental).
/// When enabled, server network processing runs in a dedicated thread.
const USE_THREADED_SERVER: bool = false;

/// Multiplayer state for the game.
pub struct MultiplayerState {
    /// Current game mode.
    pub mode: GameMode,
    /// Server instance (only when hosting, non-threaded mode).
    pub server: Option<GameServer>,
    /// Server thread (only when hosting, threaded mode).
    server_thread: Option<ServerThread>,
    /// Whether threaded server mode is enabled.
    use_threaded_server: bool,
    /// Client instance (when hosting or connecting).
    pub client: Option<GameClient>,
    /// Prediction state for client-side prediction.
    pub prediction: PredictionState,
    /// Remote players for rendering.
    pub remote_players: Vec<RemotePlayer>,
    /// Chunk sync manager.
    pub chunk_sync: ChunkSyncManager,
    /// Block sync manager for block changes.
    pub block_sync: BlockSyncManager,
    /// Input sequence number.
    pub input_sequence: u32,
    /// Pending block changes to apply to the world (received from server).
    pub pending_block_changes: Vec<crate::net::protocol::BlockChanged>,
    /// Pending chunks received from server (position, chunk data).
    pending_chunks: Vec<(Vector3<i32>, Chunk)>,
    /// Pending chunk requests from clients (server-side, when hosting).
    pending_chunk_requests: Vec<(u64, Vec<[i32; 3]>)>,

    // LAN Discovery
    /// Client-side LAN discovery (for finding servers).
    discovery: Option<LanDiscovery>,
    /// Server-side discovery responder (for advertising presence).
    discovery_responder: Option<DiscoveryResponder>,
    /// Server name for discovery announcements.
    server_name: String,
    /// Maximum players for this server.
    max_players: u8,
    /// Current player count (updated by host).
    player_count: u8,
    /// Connected player names (updated by host).
    player_names: Vec<String>,
    /// Server address (set when hosting or connected).
    pub server_address: Option<SocketAddr>,
    /// Last known ping in milliseconds.
    pub ping_ms: Option<u32>,
}

impl Default for MultiplayerState {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiplayerState {
    /// Creates a new multiplayer state in single-player mode.
    pub fn new() -> Self {
        Self {
            mode: GameMode::SinglePlayer,
            server: None,
            server_thread: None,
            use_threaded_server: USE_THREADED_SERVER,
            client: None,
            prediction: PredictionState::new(),
            remote_players: Vec::new(),
            chunk_sync: ChunkSyncManager::new(),
            block_sync: BlockSyncManager::new(false),
            input_sequence: 0,
            pending_block_changes: Vec::new(),
            pending_chunks: Vec::new(),
            pending_chunk_requests: Vec::new(),
            discovery: None,
            discovery_responder: None,
            server_name: String::new(),
            max_players: 4,
            player_count: 1, // Host counts as player
            player_names: vec!["Host".to_string()],
            server_address: None,
            ping_ms: None,
        }
    }

    /// Starts hosting a server with the given configuration.
    pub fn start_host(
        &mut self,
        server_name: String,
        port: u16,
        world_seed: u32,
        world_gen: u8,
    ) -> Result<(), String> {
        let addr: SocketAddr = ([0, 0, 0, 0], port).into();

        if self.use_threaded_server {
            // Spawn server in dedicated thread
            self.server_thread = Some(ServerThread::spawn(addr, world_seed, world_gen)?);
        } else {
            // Direct server mode (legacy)
            self.server = Some(GameServer::new(addr, world_seed, world_gen)?);
        }

        self.mode = GameMode::Host;
        self.server_name = server_name.clone();
        self.server_address = Some(addr);

        // Start discovery responder for LAN advertising
        match DiscoveryResponder::new(server_name, port, self.max_players) {
            Ok(responder) => {
                self.discovery_responder = Some(responder);
            }
            Err(e) => {
                eprintln!("[Multiplayer] Failed to start discovery responder: {}", e);
            }
        }

        // Create local client that connects to localhost
        let localhost: SocketAddr = ([127, 0, 0, 1], port).into();
        self.client = Some(GameClient::new(localhost)?);
        self.client.as_mut().unwrap().connect();

        Ok(())
    }

    /// Stops hosting the server.
    pub fn stop_host(&mut self) {
        self.server = None;
        self.server_thread = None; // Drops and joins thread
        self.discovery_responder = None;
        self.server_address = None;
        self.server_name.clear();
        self.player_count = 1;
        self.player_names = vec!["Host".to_string()];

        if self.mode == GameMode::Host {
            self.mode = GameMode::SinglePlayer;
        }
    }

    /// Connects to a remote server.
    pub fn connect(&mut self, address: &str) -> Result<(), String> {
        let addr: SocketAddr = address
            .parse()
            .map_err(|e| format!("Invalid address '{}': {}", address, e))?;

        self.client = Some(GameClient::new(addr)?);
        self.client.as_mut().unwrap().connect();
        self.mode = GameMode::Client;
        self.server_address = Some(addr);

        Ok(())
    }

    /// Disconnects from the current server.
    pub fn disconnect(&mut self) {
        self.client = None;
        self.server_address = None;
        self.ping_ms = None;

        if self.mode == GameMode::Client {
            self.mode = GameMode::SinglePlayer;
        }
    }

    /// Starts LAN discovery to find servers.
    pub fn start_discovery(&mut self) -> Result<(), String> {
        if self.discovery.is_none() {
            self.discovery =
                Some(LanDiscovery::new().map_err(|e| format!("Failed to start discovery: {}", e))?);
        }
        Ok(())
    }

    /// Stops LAN discovery.
    pub fn stop_discovery(&mut self) {
        self.discovery = None;
    }

    /// Returns discovered servers from LAN discovery.
    pub fn get_discovered_servers(&self) -> Vec<DiscoveredServer> {
        self.discovery
            .as_ref()
            .map(|d| d.get_servers())
            .unwrap_or_default()
    }

    /// Returns the current player count.
    pub fn get_player_count(&self) -> u8 {
        self.player_count
    }

    /// Returns the maximum player count.
    pub fn get_max_players(&self) -> u8 {
        self.max_players
    }

    /// Returns the list of player names.
    pub fn get_player_names(&self) -> &[String] {
        &self.player_names
    }

    /// Returns the server name (if hosting).
    pub fn get_server_name(&self) -> &str {
        &self.server_name
    }

    /// Returns the server address (if hosting or connected).
    pub fn get_server_address(&self) -> Option<SocketAddr> {
        self.server_address
    }

    /// Returns the last known ping.
    pub fn get_ping_ms(&self) -> Option<u32> {
        self.ping_ms
    }

    /// Updates the multiplayer state (call every frame).
    pub fn update(&mut self, duration: Duration) {
        // Handle threaded server events
        if let Some(ref server_thread) = self.server_thread {
            for event in server_thread.recv_events() {
                self.handle_thread_event(event);
            }
        }

        // Collect events and messages from direct server first (non-threaded mode)
        let (server_events, client_messages) = if let Some(ref mut server) = self.server {
            let events = server.update(duration);
            let messages = server.receive_client_messages();
            (events, messages)
        } else {
            (Vec::new(), Vec::new())
        };

        // Process direct server events (now that server borrow is released)
        for event in server_events {
            self.handle_server_event(event);
        }

        // Process client messages from direct server
        for (client_id, msg) in client_messages {
            self.handle_client_message_direct(client_id, msg);
        }

        // Update client if connected
        if let Some(ref mut client) = self.client {
            client.update(duration);

            // Process received messages
            let messages = client.receive_messages();
            for msg in messages {
                self.handle_server_message(&msg);
            }
        }

        // Update discovery responder (server-side)
        if let Some(ref responder) = self.discovery_responder {
            responder.update(self.player_count);
        }

        // Update discovery client (client-side)
        if let Some(ref mut discovery) = self.discovery {
            discovery.update();
        }

        // Update player count based on remote players + host
        if self.mode == GameMode::Host {
            self.player_count = (self.remote_players.len() + 1) as u8;
        }
    }

    /// Handles an event from the server thread.
    fn handle_thread_event(&mut self, event: ServerThreadEvent) {
        match event {
            ServerThreadEvent::ClientConnected { client_id } => {
                // Send connection acceptance with spawn position
                // TODO: Get actual spawn position from world
                let spawn_position = [0.0, 64.0, 0.0];
                if let Some(ref server_thread) = self.server_thread {
                    let _ = server_thread.send_command(ServerCommand::HandleClientConnected {
                        client_id,
                        spawn_position,
                    });
                }
            }
            ServerThreadEvent::ClientDisconnected { client_id, reason } => {
                if let Some(ref server_thread) = self.server_thread {
                    let _ = server_thread
                        .send_command(ServerCommand::HandleClientDisconnected { client_id });
                }
                let _ = reason; // Log in production
            }
            ServerThreadEvent::ClientMessage { client_id, message } => {
                self.handle_client_message(client_id, message);
            }
            ServerThreadEvent::Error { error } => {
                eprintln!("[Multiplayer] Server thread error: {}", error);
            }
        }
    }

    /// Handles a message received from a client (direct server mode).
    fn handle_client_message_direct(
        &mut self,
        client_id: u64,
        msg: crate::net::protocol::ClientMessage,
    ) {
        self.handle_client_message(client_id, msg);
    }

    /// Handles a message received from a client (server-side, when hosting).
    fn handle_client_message(&mut self, client_id: u64, msg: crate::net::protocol::ClientMessage) {
        use crate::net::protocol::ClientMessage;

        match msg {
            ClientMessage::RequestChunks(request) => {
                // Queue chunk request for processing by game loop
                self.pending_chunk_requests
                    .push((client_id, request.positions));
            }
            ClientMessage::PlayerInput(input) => {
                // Update player state on server
                if let Some(ref mut server) = self.server {
                    server.update_player_state(
                        client_id,
                        input.position,
                        input.velocity,
                        input.yaw,
                        input.pitch,
                        input.sequence,
                    );
                } else if let Some(ref server_thread) = self.server_thread {
                    let _ = server_thread.send_command(ServerCommand::UpdatePlayerState {
                        client_id,
                        position: input.position,
                        velocity: input.velocity,
                        yaw: input.yaw,
                        pitch: input.pitch,
                        sequence: input.sequence,
                    });
                }
            }
            ClientMessage::PlaceBlock(place) => {
                // TODO: Validate and apply block placement server-side
                // For now, broadcast to all clients including the sender
                let change = crate::net::protocol::BlockChanged {
                    position: place.position,
                    block: place.block,
                };
                if let Some(ref mut server) = self.server {
                    server.broadcast_block_change(change);
                } else if let Some(ref server_thread) = self.server_thread {
                    let _ = server_thread.send_command(ServerCommand::BroadcastBlockChange(change));
                }
            }
            ClientMessage::BreakBlock(break_msg) => {
                // TODO: Validate and apply block break server-side
                // For now, broadcast to all clients
                let change = crate::net::protocol::BlockChanged {
                    position: break_msg.position,
                    block: crate::net::protocol::BlockData::default(), // Air
                };
                if let Some(ref mut server) = self.server {
                    server.broadcast_block_change(change);
                } else if let Some(ref server_thread) = self.server_thread {
                    let _ = server_thread.send_command(ServerCommand::BroadcastBlockChange(change));
                }
            }
            _ => {
                // Other message types not yet implemented
            }
        }
    }

    /// Handles a server event (for the host).
    fn handle_server_event(&mut self, event: renet::ServerEvent) {
        match event {
            renet::ServerEvent::ClientConnected { client_id } => {
                // When hosting, spawn new players
                if let Some(ref mut server) = self.server {
                    // TODO: Get actual spawn position from world
                    server.handle_client_connected(client_id, [0.0, 64.0, 0.0]);
                }
            }
            renet::ServerEvent::ClientDisconnected { client_id, reason } => {
                if let Some(ref mut server) = self.server {
                    server.handle_client_disconnected(client_id);
                }
                let _ = reason; // Log reason in production
            }
        }
    }

    /// Handles a message received from the server.
    fn handle_server_message(&mut self, msg: &crate::net::protocol::ServerMessage) {
        use crate::net::protocol::ServerMessage;

        match msg {
            ServerMessage::ConnectionAccepted(_accepted) => {
                // Connection established, prediction is enabled by default
            }
            ServerMessage::PlayerState(state) => {
                // Reconcile with server
                self.prediction.reconcile(state);

                // Update remote player rendering
                if let Some(ref client) = self.client {
                    // Check if this is a remote player
                    if Some(state.player_id) != client.player_id() {
                        if let Some(remote) = self
                            .remote_players
                            .iter_mut()
                            .find(|p| p.player_id == state.player_id)
                        {
                            let timestamp = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs_f64();
                            remote.update_state(state, timestamp);
                        }
                    }
                }
            }
            ServerMessage::PlayerJoined(joined) => {
                // Add new remote player
                let remote =
                    RemotePlayer::new(joined.player_id, joined.name.clone(), joined.position);
                self.remote_players.push(remote);
            }
            ServerMessage::PlayerLeft(left) => {
                self.remote_players
                    .retain(|p| p.player_id != left.player_id);
            }
            ServerMessage::ChunkData(chunk) => {
                // Mark chunk as received
                self.chunk_sync.mark_received(chunk.position);

                // Decompress and deserialize chunk data
                match SerializedChunk::decompress(&chunk.compressed_data) {
                    Ok(serialized) => {
                        // Convert to Chunk struct
                        match serialized.to_chunk() {
                            Ok(chunk_data) => {
                                // Store for later application to world
                                self.receive_chunk(chunk.position, chunk_data);
                            }
                            Err(e) => {
                                eprintln!(
                                    "[Multiplayer] Failed to convert chunk at {:?}: {}",
                                    chunk.position, e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "[Multiplayer] Failed to decompress chunk at {:?}: {}",
                            chunk.position, e
                        );
                    }
                }
            }
            ServerMessage::BlockChanged(change) => {
                // Queue block change for application to world
                self.pending_block_changes.push(change.clone());
            }
            ServerMessage::BlocksChanged(changes) => {
                // Queue multiple block changes
                self.pending_block_changes
                    .extend(changes.changes.iter().map(|(pos, block)| {
                        crate::net::protocol::BlockChanged {
                            position: *pos,
                            block: block.clone(),
                        }
                    }));
            }
            _ => {}
        }
    }

    /// Sends player input to the server.
    pub fn send_input(
        &mut self,
        position: [f32; 3],
        velocity: [f32; 3],
        yaw: f32,
        pitch: f32,
        actions: crate::net::protocol::InputActions,
    ) {
        if let Some(ref mut client) = self.client {
            // Record input for prediction
            self.prediction
                .record_input(position, velocity, yaw, pitch, actions);

            // Send to server
            client.send_input(self.input_sequence, position, velocity, yaw, pitch, actions);
            self.input_sequence = self.input_sequence.wrapping_add(1);
        }
    }

    /// Sends a block placement to the server.
    pub fn send_place_block(&mut self, position: [i32; 3], block: crate::net::protocol::BlockData) {
        if let Some(ref mut client) = self.client {
            client.send_place_block(position, block);
        }
    }

    /// Sends a block break to the server.
    pub fn send_break_block(&mut self, position: [i32; 3]) {
        if let Some(ref mut client) = self.client {
            client.send_break_block(position);
        }
    }

    /// Takes pending block changes and clears the queue.
    /// Call this from the game loop to apply changes to the world.
    pub fn take_pending_block_changes(&mut self) -> Vec<crate::net::protocol::BlockChanged> {
        std::mem::take(&mut self.pending_block_changes)
    }

    /// Returns true if there are pending block changes to apply.
    pub fn has_pending_block_changes(&self) -> bool {
        !self.pending_block_changes.is_empty()
    }

    /// Returns true if connected to a server.
    pub fn is_connected(&self) -> bool {
        self.client
            .as_ref()
            .map(|c| c.is_connected())
            .unwrap_or(false)
    }

    /// Returns true if hosting a server.
    pub fn is_hosting(&self) -> bool {
        self.server.is_some()
    }

    /// Returns the local player ID (if connected).
    pub fn local_player_id(&self) -> Option<u64> {
        self.client.as_ref().and_then(|c| c.player_id())
    }

    /// Returns the world seed (if received from server).
    pub fn world_seed(&self) -> Option<u32> {
        self.client.as_ref().and_then(|c| c.world_seed())
    }

    /// Updates remote player interpolation.
    pub fn update_remote_players(&mut self) {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        for remote in &mut self.remote_players {
            remote.interpolate(current_time);
        }
    }

    /// Receives a chunk from the server and stores it for later application.
    pub fn receive_chunk(&mut self, position: [i32; 3], chunk: Chunk) {
        let pos = Vector3::new(position[0], position[1], position[2]);
        self.pending_chunks.push((pos, chunk));
    }

    /// Takes all pending chunks and clears the queue.
    /// Call this from the game loop to apply chunks to the world.
    pub fn take_pending_chunks(&mut self) -> Vec<(Vector3<i32>, Chunk)> {
        std::mem::take(&mut self.pending_chunks)
    }

    /// Returns true if there are pending chunks to apply.
    pub fn has_pending_chunks(&self) -> bool {
        !self.pending_chunks.is_empty()
    }

    /// Returns the number of pending chunks.
    pub fn pending_chunk_count(&self) -> usize {
        self.pending_chunks.len()
    }

    /// Takes all pending chunk requests from clients and clears the queue.
    /// Call this from the game loop when hosting to fulfill chunk requests.
    /// Returns (client_id, requested_chunk_positions) pairs.
    pub fn take_pending_chunk_requests(&mut self) -> Vec<(u64, Vec<[i32; 3]>)> {
        std::mem::take(&mut self.pending_chunk_requests)
    }

    /// Returns true if there are pending chunk requests from clients.
    pub fn has_pending_chunk_requests(&self) -> bool {
        !self.pending_chunk_requests.is_empty()
    }

    /// Sends chunk data to a specific client (server-side, when hosting).
    /// The game loop calls this after retrieving chunk data from the world.
    pub fn send_chunk_to_client(&mut self, client_id: u64, position: [i32; 3], chunk: &Chunk) {
        // Serialize the chunk (happens on main thread regardless of mode)
        let serialized = SerializedChunk::from_chunk(position, chunk);

        // Compress for network transmission
        match serialized.compress() {
            Ok(compressed) => {
                let chunk_data = crate::net::protocol::ChunkData {
                    position,
                    version: serialized.version,
                    compressed_data: compressed,
                };

                if let Some(ref mut server) = self.server {
                    server.send_chunk(client_id, chunk_data);
                } else if let Some(ref server_thread) = self.server_thread {
                    let _ = server_thread.send_command(ServerCommand::SendChunk {
                        client_id,
                        chunk: chunk_data,
                    });
                }
            }
            Err(e) => {
                eprintln!(
                    "[Multiplayer] Failed to compress chunk at {:?}: {}",
                    position, e
                );
            }
        }
    }
}

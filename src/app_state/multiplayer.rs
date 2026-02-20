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
    BlockSyncManager, ChunkSyncManager, CustomTextureCache, DiscoveredServer, DiscoveryResponder,
    GameClient, GameServer, LanDiscovery, PredictionState, RemotePlayer, SerializedChunk,
    ServerCommand, ServerThread, ServerThreadEvent,
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
    /// Positions of chunks that should be generated locally from seed (bandwidth optimization).
    pending_local_chunks: Vec<[i32; 3]>,
    /// Pending chunk requests from clients (server-side, when hosting).
    pending_chunk_requests: Vec<(u64, Vec<[i32; 3]>)>,
    /// Pending server world seed (received on ConnectionAccepted, needs to be applied).
    pending_server_seed: Option<(u32, u8)>,
    /// Custom texture cache (client-side).
    pub texture_cache: CustomTextureCache,
    /// Flag indicating GPU textures need initialization.
    pending_gpu_texture_init: Option<u8>,
    /// Pending custom models received from server (to be registered).
    pub pending_models: Vec<crate::net::protocol::ModelAdded>,
    /// Pending model uploads from clients (server-side, when hosting).
    pub pending_model_uploads: Vec<(u64, crate::net::protocol::UploadModel)>,
    /// Pending texture uploads from clients (server-side, when hosting).
    pub pending_texture_uploads: Vec<(u64, crate::net::protocol::UploadTexture)>,

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
            pending_local_chunks: Vec::new(),
            pending_chunk_requests: Vec::new(),
            pending_server_seed: None,
            texture_cache: CustomTextureCache::new(0), // Will be set on connect
            pending_gpu_texture_init: None,
            pending_models: Vec::new(),
            pending_model_uploads: Vec::new(),
            pending_texture_uploads: Vec::new(),
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
        println!(
            "[Multiplayer] Starting host on {} with seed {}",
            addr, world_seed
        );

        if self.use_threaded_server {
            // Spawn server in dedicated thread
            self.server_thread = Some(ServerThread::spawn(addr, world_seed, world_gen)?);
            println!("[Multiplayer] Server thread spawned");
        } else {
            // Direct server mode (legacy)
            self.server = Some(GameServer::new(addr, world_seed, world_gen)?);
            println!("[Multiplayer] Direct server created");
        }

        self.mode = GameMode::Host;
        self.server_name = server_name.clone();
        self.server_address = Some(addr);

        // Start discovery responder for LAN advertising
        match DiscoveryResponder::new(server_name, port, self.max_players) {
            Ok(responder) => {
                self.discovery_responder = Some(responder);
                println!("[Multiplayer] Discovery responder started");
            }
            Err(e) => {
                eprintln!("[Multiplayer] Failed to start discovery responder: {}", e);
            }
        }

        // Initialize host player on the server
        // Host gets player_id 0, first connected client gets 1, etc.
        if let Some(ref mut server) = self.server {
            server.set_host_player(0, "Host".to_string(), [0.0, 64.0, 0.0]);
        } else if let Some(ref server_thread) = self.server_thread {
            let _ = server_thread.send_command(ServerCommand::SetHostPlayer {
                player_id: 0,
                name: "Host".to_string(),
                position: [0.0, 64.0, 0.0],
            });
        }

        // Create local client that connects to localhost
        let localhost: SocketAddr = ([127, 0, 0, 1], port).into();
        println!(
            "[Multiplayer] Creating local client connecting to {}",
            localhost
        );
        self.client = Some(GameClient::new(localhost)?);
        self.client.as_mut().unwrap().connect();
        println!("[Multiplayer] Local client created and connection started");

        Ok(())
    }

    /// Updates the host player's position on the server.
    /// This should be called every frame with the local player's position.
    pub fn update_host_position(
        &mut self,
        position: [f32; 3],
        velocity: [f32; 3],
        yaw: f32,
        pitch: f32,
    ) {
        if self.mode != GameMode::Host {
            return;
        }

        if let Some(ref mut server) = self.server {
            server.update_host_player(position, velocity, yaw, pitch);
        } else if let Some(ref server_thread) = self.server_thread {
            let _ = server_thread.send_command(ServerCommand::UpdateHostPlayer {
                position,
                velocity,
                yaw,
                pitch,
            });
        }
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

        println!("[Multiplayer] Connecting to {}...", addr);
        self.client = Some(GameClient::new(addr)?);
        self.client.as_mut().unwrap().connect();
        self.mode = GameMode::Client;
        self.server_address = Some(addr);
        println!(
            "[Multiplayer] Client created and connection started to {}",
            addr
        );

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

    /// Returns remote player markers for minimap display.
    /// Each marker includes position (x, z) and player_id for color assignment.
    /// The local player is NOT included in this list.
    pub fn get_minimap_markers(&self) -> Vec<crate::ui::minimap::RemotePlayerMarker> {
        self.remote_players
            .iter()
            .map(|player| crate::ui::minimap::RemotePlayerMarker {
                name: player.name.clone(),
                position: (player.position[0], player.position[2]),
                player_id: player.player_id,
            })
            .collect()
    }

    /// Returns remote player positions for 3D rendering.
    /// Each tuple contains (position [x, y, z], player_id for color).
    pub fn get_remote_player_positions(&self) -> Vec<([f32; 3], u64)> {
        self.remote_players
            .iter()
            .map(|player| (player.position, player.player_id))
            .collect()
    }

    /// Returns remote player data for 3D name label rendering.
    /// Each tuple contains (name, position [x, y, z], color_index).
    pub fn get_remote_players_for_labels(&self) -> Vec<(String, [f32; 3], usize)> {
        self.remote_players
            .iter()
            .enumerate()
            .map(|(idx, player)| (player.name.clone(), player.position, idx))
            .collect()
    }

    /// Returns remote player labels for HUD rendering.
    pub fn get_remote_player_labels(&self) -> Vec<crate::ui::minimap::RemotePlayerLabel> {
        self.remote_players
            .iter()
            .enumerate()
            .map(|(idx, player)| crate::ui::minimap::RemotePlayerLabel {
                name: player.name.clone(),
                position: player.position,
                color_index: idx,
            })
            .collect()
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
        // Frame counter for periodic operations (no logging)
        static UPDATE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let count = UPDATE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

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

        // Broadcast player states periodically (every 3 frames = ~20 times per second at 60fps)
        if count % 3 == 0 {
            if let Some(ref mut server) = self.server {
                server.broadcast_player_states();
            } else if let Some(ref server_thread) = self.server_thread {
                let _ = server_thread.send_command(ServerCommand::BroadcastPlayerStates);
            }
        }

        // CRITICAL: Flush packets after processing events (which may queue messages)
        if let Some(ref mut server) = self.server {
            server.flush_packets();
        }

        // Process client messages from direct server
        for (client_id, msg) in client_messages {
            self.handle_client_message_direct(client_id, msg);
        }

        // Update client if connected
        let client_messages: Vec<crate::net::protocol::ServerMessage> =
            if let Some(ref mut client) = self.client {
                client.update(duration);

                // Process received messages
                let messages = client.receive_messages();
                if !messages.is_empty() {
                    println!(
                        "[Multiplayer] Client received {} message(s)",
                        messages.len()
                    );
                }

                // Flush packets (send any queued outgoing messages)
                client.flush_packets();

                messages
            } else {
                Vec::new()
            };

        // Process messages after client borrow ends
        for msg in client_messages {
            self.handle_server_message(&msg);
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
                println!(
                    "[Server] Received chunk request from client {} for {} chunks",
                    client_id,
                    request.positions.len()
                );
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
                println!(
                    "[Server] Received PlaceBlock at {:?} from client {}",
                    place.position, client_id
                );
                let change = crate::net::protocol::BlockChanged {
                    position: place.position,
                    block: place.block,
                };
                if let Some(ref mut server) = self.server {
                    server.broadcast_block_change(change);
                    println!("[Server] Broadcasted block change to all clients");
                } else if let Some(ref server_thread) = self.server_thread {
                    let _ = server_thread.send_command(ServerCommand::BroadcastBlockChange(change));
                }
            }
            ClientMessage::BreakBlock(break_msg) => {
                // TODO: Validate and apply block break server-side
                // For now, broadcast to all clients
                println!(
                    "[Server] Received BreakBlock at {:?} from client {}",
                    break_msg.position, client_id
                );
                let change = crate::net::protocol::BlockChanged {
                    position: break_msg.position,
                    block: crate::net::protocol::BlockData::default(), // Air
                };
                if let Some(ref mut server) = self.server {
                    server.broadcast_block_change(change);
                    println!("[Server] Broadcasted block break to all clients");
                } else if let Some(ref server_thread) = self.server_thread {
                    let _ = server_thread.send_command(ServerCommand::BroadcastBlockChange(change));
                }
            }
            ClientMessage::RequestTexture(req) => {
                println!(
                    "[Server] Received texture request for slot {} from client {}",
                    req.slot, client_id
                );
                if let Some(ref mut server) = self.server {
                    server.handle_texture_request(client_id, req.slot);
                } else if let Some(ref server_thread) = self.server_thread {
                    let _ = server_thread.send_command(ServerCommand::HandleTextureRequest {
                        client_id,
                        slot: req.slot,
                    });
                }
            }
            ClientMessage::UploadModel(upload) => {
                println!(
                    "[Server] Received model upload '{}' from client {}",
                    upload.name, client_id
                );
                // Queue for processing by game loop (needs access to model registry)
                self.pending_model_uploads.push((client_id, upload));
            }
            ClientMessage::UploadTexture(upload) => {
                println!(
                    "[Server] Received texture upload '{}' from client {}",
                    upload.name, client_id
                );
                // Queue for processing by game loop (needs access to texture manager)
                self.pending_texture_uploads.push((client_id, upload));
            }
            _ => {
                // Other message types not yet implemented
            }
        }
    }

    /// Handles a server event (for the host).
    fn handle_server_event(&mut self, event: renet::ServerEvent) {
        println!("[Multiplayer] Processing server event: {:?}", event);
        match event {
            renet::ServerEvent::ClientConnected { client_id } => {
                println!(
                    "[Server] Client {} connected - calling handle_client_connected",
                    client_id
                );
                // When hosting, spawn new players
                if let Some(ref mut server) = self.server {
                    // Check if this is the host's own client connection (first client in Host mode)
                    // The host connects to itself as a client - this is the loopback connection
                    if self.mode == GameMode::Host && server.host_client_id().is_none() {
                        println!(
                            "[Server] First client in Host mode - marking as host's loopback client"
                        );
                        server.set_host_client_id(client_id);
                    }

                    // TODO: Get actual spawn position from world
                    server.handle_client_connected(client_id, [0.0, 64.0, 0.0]);
                    println!(
                        "[Server] handle_client_connected returned for client {}",
                        client_id
                    );
                } else {
                    println!("[Server] ERROR: No server instance available!");
                }
            }
            renet::ServerEvent::ClientDisconnected { client_id, reason } => {
                println!("[Server] Client {} disconnected: {:?}", client_id, reason);
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
            ServerMessage::ConnectionAccepted(accepted) => {
                println!(
                    "[Client] Connection accepted. Player ID: {}, World seed: {}, Custom textures: {}",
                    accepted.player_id, accepted.world_seed, accepted.custom_texture_count
                );
                self.pending_server_seed = Some((accepted.world_seed, accepted.world_gen));
                self.texture_cache = CustomTextureCache::new(accepted.custom_texture_count);
                // Flag GPU texture initialization if we have custom textures
                if accepted.custom_texture_count > 0 {
                    self.pending_gpu_texture_init = Some(accepted.custom_texture_count);
                }
            }
            ServerMessage::PlayerState(state) => {
                // Reconcile with server
                self.prediction.reconcile(state);

                // Update remote player rendering
                if let Some(ref client) = self.client {
                    // Check if this is a remote player (not ourselves)
                    // Host has player_id 0, clients have their own assigned IDs
                    let is_local_player = client.player_id() == Some(state.player_id)
                        || (self.mode == GameMode::Host && state.player_id == 0);

                    if !is_local_player {
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs_f64();

                        // Try to find existing remote player
                        if let Some(remote) = self
                            .remote_players
                            .iter_mut()
                            .find(|p| p.player_id == state.player_id)
                        {
                            remote.update_state(state, timestamp);
                        } else {
                            // Player not found - this might be the host or a new player
                            // Add them to remote_players with a placeholder name
                            println!(
                                "[Client] Adding new remote player {} at ({:.1}, {:.1}, {:.1})",
                                state.player_id,
                                state.position[0],
                                state.position[1],
                                state.position[2]
                            );
                            let mut remote = RemotePlayer::new(
                                state.player_id,
                                if state.player_id == 0 {
                                    "Host".to_string()
                                } else {
                                    format!("Player {}", state.player_id)
                                },
                                state.position,
                            );
                            remote.velocity = state.velocity;
                            remote.yaw = state.yaw;
                            remote.update_state(state, timestamp);
                            self.remote_players.push(remote);
                        }
                    }
                }
            }
            ServerMessage::PlayerJoined(joined) => {
                // Add new remote player (check for duplicates)
                if !self
                    .remote_players
                    .iter()
                    .any(|p| p.player_id == joined.player_id)
                {
                    let remote =
                        RemotePlayer::new(joined.player_id, joined.name.clone(), joined.position);
                    self.remote_players.push(remote);
                }
            }
            ServerMessage::PlayerLeft(left) => {
                self.remote_players
                    .retain(|p| p.player_id != left.player_id);
            }
            ServerMessage::ChunkData(chunk) => {
                // Mark chunk as received
                self.chunk_sync.mark_received(chunk.position);
                println!("[Client] Received ChunkData for {:?}", chunk.position);

                // Decompress and deserialize chunk data
                match SerializedChunk::decompress(&chunk.compressed_data) {
                    Ok(serialized) => {
                        // Convert to Chunk struct
                        match serialized.to_chunk() {
                            Ok(chunk_data) => {
                                // Store for later application to world
                                self.receive_chunk(chunk.position, chunk_data);
                                println!(
                                    "[Client] Chunk {:?} ready for application",
                                    chunk.position
                                );
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
            ServerMessage::ChunkGenerateLocal(msg) => {
                // Server says this chunk has no modifications - generate it locally
                // Mark as pending local generation (NOT received) until chunk_loader finishes
                self.chunk_sync.mark_pending_local_generation(msg.position);
                self.pending_local_chunks.push(msg.position);
                println!(
                    "[Client] Received ChunkGenerateLocal for {:?}",
                    msg.position
                );
            }
            ServerMessage::BlockChanged(change) => {
                // Queue block change for application to world
                println!(
                    "[Client] Received BlockChanged at {:?}: {:?}",
                    change.position, change.block.block_type
                );
                self.pending_block_changes.push(change.clone());
            }
            ServerMessage::BlocksChanged(changes) => {
                // Queue multiple block changes
                println!(
                    "[Client] Received BlocksChanged with {} changes",
                    changes.changes.len()
                );
                self.pending_block_changes
                    .extend(changes.changes.iter().map(|(pos, block)| {
                        crate::net::protocol::BlockChanged {
                            position: *pos,
                            block: block.clone(),
                        }
                    }));
            }
            ServerMessage::ModelRegistrySync(sync) => {
                println!("[Client] Received ModelRegistrySync");
                if !sync.models_data.is_empty() {
                    println!(
                        "[Client] Received {} bytes of model data",
                        sync.models_data.len()
                    );
                }
                if !sync.door_pairs_data.is_empty() {
                    println!(
                        "[Client] Received {} bytes of door pair data",
                        sync.door_pairs_data.len()
                    );
                }
            }
            ServerMessage::TextureData(tex) => {
                println!("[Client] Received texture for slot {}", tex.slot);
                self.texture_cache.store_texture(tex.slot, tex.data.clone());
            }
            ServerMessage::TextureAdded(tex) => {
                println!("[Client] Texture added: slot {} = '{}'", tex.slot, tex.name);
            }
            ServerMessage::ModelAdded(model) => {
                println!(
                    "[Client] Model added: ID {} = '{}' by '{}'",
                    model.model_id, model.name, model.author
                );
                self.pending_models.push(model.clone());
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
            // Flush immediately for responsive block sync
            client.flush_packets();
        }
    }

    /// Sends a block break to the server.
    pub fn send_break_block(&mut self, position: [i32; 3]) {
        if let Some(ref mut client) = self.client {
            client.send_break_block(position);
            // Flush immediately for responsive block sync
            client.flush_packets();
        }
    }

    /// Uploads a custom model to the server.
    pub fn send_upload_model(&mut self, name: String, author: String, model_data: Vec<u8>) {
        if let Some(ref mut client) = self.client {
            client.send_upload_model(name, author, model_data);
        }
    }

    /// Uploads a custom texture to the server.
    pub fn send_upload_texture(&mut self, name: String, png_data: Vec<u8>) {
        if let Some(ref mut client) = self.client {
            client.send_upload_texture(name, png_data);
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

    /// Returns true if we are the host (server + local client).
    pub fn is_host(&self) -> bool {
        self.mode == GameMode::Host
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

    /// Takes all pending local chunk positions and clears the queue.
    /// These chunks should be generated locally using the world seed.
    pub fn take_pending_local_chunks(&mut self) -> Vec<[i32; 3]> {
        std::mem::take(&mut self.pending_local_chunks)
    }

    /// Returns true if there are pending local chunks to generate.
    pub fn has_pending_local_chunks(&self) -> bool {
        !self.pending_local_chunks.is_empty()
    }

    /// Marks a locally-generated chunk as complete (received and applied to world).
    /// This should be called when a chunk that was requested via ChunkGenerateLocal
    /// is successfully generated and inserted into the world.
    pub fn mark_local_chunk_complete(&mut self, position: [i32; 3]) {
        self.chunk_sync.try_complete_local_generation(position);
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

    /// Takes all pending models received from server and clears the queue.
    /// Call this from the game loop to register models in the registry.
    pub fn take_pending_models(&mut self) -> Vec<crate::net::protocol::ModelAdded> {
        std::mem::take(&mut self.pending_models)
    }

    /// Returns true if there are pending models to register.
    pub fn has_pending_models(&self) -> bool {
        !self.pending_models.is_empty()
    }

    /// Takes all pending model uploads from clients and clears the queue.
    /// Call this from the game loop when hosting to process model uploads.
    pub fn take_pending_model_uploads(&mut self) -> Vec<(u64, crate::net::protocol::UploadModel)> {
        std::mem::take(&mut self.pending_model_uploads)
    }

    /// Returns true if there are pending model uploads to process.
    pub fn has_pending_model_uploads(&self) -> bool {
        !self.pending_model_uploads.is_empty()
    }

    /// Takes all pending texture uploads from clients and clears the queue.
    /// Call this from the game loop when hosting to process texture uploads.
    pub fn take_pending_texture_uploads(
        &mut self,
    ) -> Vec<(u64, crate::net::protocol::UploadTexture)> {
        std::mem::take(&mut self.pending_texture_uploads)
    }

    /// Returns true if there are pending texture uploads to process.
    pub fn has_pending_texture_uploads(&self) -> bool {
        !self.pending_texture_uploads.is_empty()
    }

    /// Returns the pending server world seed if one was received.
    /// Call this from the game loop to apply the server's seed to the world generator.
    pub fn take_pending_server_seed(&mut self) -> Option<(u32, u8)> {
        self.pending_server_seed.take()
    }

    /// Returns true if there's a pending server seed to apply.
    pub fn has_pending_server_seed(&self) -> bool {
        self.pending_server_seed.is_some()
    }

    /// Sends chunk data to a specific client (server-side, when hosting).
    /// The game loop calls this after retrieving chunk data from the world.
    /// If the chunk hasn't been modified by players, sends a "generate locally"
    /// message instead of full chunk data (bandwidth optimization).
    pub fn send_chunk_to_client(&mut self, client_id: u64, position: [i32; 3], chunk: &Chunk) {
        // Check if chunk has been modified by players
        if !chunk.persistence_dirty {
            // Chunk is unmodified - tell client to generate it locally from seed
            if let Some(ref mut server) = self.server {
                server.send_chunk_generate_local(client_id, position);
            } else if let Some(ref server_thread) = self.server_thread {
                let _ = server_thread.send_command(ServerCommand::SendChunkGenerateLocal {
                    client_id,
                    position,
                });
            }
            return;
        }

        // Chunk has modifications - serialize and send full data
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

    /// Requests a custom texture if not cached.
    pub fn request_texture_if_needed(&mut self, slot: u8) {
        if self.texture_cache.request_if_needed(slot) {
            if let Some(ref mut client) = self.client {
                client.send_texture_request(slot);
            }
        }
    }

    /// Returns the texture cache for rendering.
    pub fn texture_cache(&self) -> &CustomTextureCache {
        &self.texture_cache
    }

    /// Returns a mutable reference to the texture cache for GPU uploads.
    pub fn texture_cache_mut(&mut self) -> &mut CustomTextureCache {
        &mut self.texture_cache
    }

    /// Checks if GPU textures need initialization and returns the max slot count.
    pub fn take_pending_gpu_texture_init(&mut self) -> Option<u8> {
        self.pending_gpu_texture_init.take()
    }
}

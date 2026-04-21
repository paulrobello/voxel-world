//! Multiplayer state management.
//!
//! Handles the game mode (single-player, host, client), server/client instances,
//! and player synchronization.

// Networking integration is incomplete — some fields/methods are prepared for future use.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use crate::chunk::Chunk;
use crate::config::GameMode;
use crate::net::{
    BlockSyncManager, ChunkSyncManager, CustomTextureCache, DiscoveredServer, DiscoveryResponder,
    GameClient, GameServer, LanDiscovery, LavaSyncOptimizer, PredictionState, RemotePlayer,
    SerializedChunk, WaterSyncOptimizer,
};
#[cfg(feature = "threaded-server")]
use crate::net::{ServerCommand, ServerThread, ServerThreadEvent};
use nalgebra::Vector3;

/// Whether to use threaded server mode (experimental).
/// When enabled, server network processing runs in a dedicated thread.
/// Gate behind the `threaded-server` feature; disabled by default.
#[cfg(feature = "threaded-server")]
const USE_THREADED_SERVER: bool = true;
#[cfg(not(feature = "threaded-server"))]
const USE_THREADED_SERVER: bool = false;

/// Maximum chat messages to keep in history.
const MAX_CHAT_HISTORY: usize = 50;

/// Chat message entry for display.
#[derive(Debug, Clone)]
pub struct ChatEntry {
    /// Player name who sent the message.
    pub player_name: String,
    /// Message content.
    pub message: String,
    /// Timestamp when message was received.
    pub timestamp: Instant,
}

/// Typed network event — replaces all individual `pending_*` `Vec<T>` fields.
///
/// Events are pushed into a single `VecDeque<NetworkEvent>` on arrival and
/// consumed by the game-loop subsystem that owns each variant.  Using a
/// single queue eliminates the copy-paste burden of adding a new sync type:
/// add one variant, one push site, and one drain helper.
pub enum NetworkEvent {
    /// A single block was placed or broken (received from server, client-side).
    BlockChanged(crate::net::protocol::BlockChanged),
    /// A full chunk received from the server (client-side).
    ChunkReceived(Vector3<i32>, Box<Chunk>),
    /// Server instructed the client to generate this chunk locally from seed.
    LocalChunkPending([i32; 3]),
    /// A client requested one or more chunks (server-side).
    ChunkRequested(u64, Vec<[i32; 3]>),
    /// A new custom model was announced by the server (client-side).
    ModelAdded(crate::net::protocol::ModelAdded),
    /// A client uploaded a new model for registration (server-side).
    ModelUploaded(u64, crate::net::protocol::UploadModel),
    /// A client uploaded a new texture (server-side).
    TextureUploaded(u64, crate::net::protocol::UploadTexture),
    /// A client uploaded a picture (server-side).
    PictureUploaded(u64, crate::net::protocol::UploadPicture),
    /// Fluid water cell updates received from the server (client-side).
    WaterCellUpdated(crate::net::protocol::WaterCellUpdate),
    /// Fluid lava cell updates received from the server (client-side).
    LavaCellUpdated(crate::net::protocol::LavaCellUpdate),
    /// A falling block was spawned (client-side).
    FallingBlockSpawned(crate::net::protocol::FallingBlockSpawned),
    /// A falling block landed (client-side).
    FallingBlockLanded(crate::net::protocol::FallingBlockLanded),
    /// A whole tree fell (client-side).
    TreeFell(crate::net::protocol::TreeFell),
    /// A picture frame block had its picture changed (client-side).
    FramePictureSet(crate::net::protocol::FramePictureSet),
    /// A stencil was loaded on the server and is available for use (client-side).
    StencilLoaded(crate::net::protocol::StencilLoaded),
    /// A stencil had its transform updated (client-side).
    StencilTransformUpdated(crate::net::protocol::StencilTransformUpdate),
    /// A stencil was removed (client-side).
    StencilRemoved(crate::net::protocol::StencilRemoved),
    /// A template was loaded (client-side).
    TemplateLoaded(crate::net::protocol::TemplateLoaded),
    /// A template was removed (client-side).
    TemplateRemoved(crate::net::protocol::TemplateRemoved),
    /// A player changed their display name (client-side).
    PlayerNameChanged(crate::net::protocol::PlayerNameChanged),
    /// A chat message was received from the server (client-side).
    ChatReceived(crate::net::protocol::ChatReceived),
}

/// Snapshot of the last PlayerInput actually sent to the server.
///
/// Used by `MultiplayerState::send_input` to skip near-idle frames below the movement
/// thresholds. `skips_remaining` is a countdown that forces a keep-alive send every
/// `FORCE_SEND_EVERY` calls so the server never stops hearing from an idle client.
#[derive(Debug, Clone, Copy)]
struct LastSentInput {
    position: [f32; 3],
    velocity: [f32; 3],
    yaw: f32,
    pitch: f32,
    actions: crate::net::protocol::InputActions,
    skips_remaining: u32,
}

/// Returns the largest component-wise absolute delta between two 3-vectors.
#[inline]
fn max_abs_delta(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = (a[0] - b[0]).abs();
    let dy = (a[1] - b[1]).abs();
    let dz = (a[2] - b[2]).abs();
    dx.max(dy).max(dz)
}

/// Multiplayer state for the game.
pub struct MultiplayerState {
    /// Current game mode.
    pub mode: GameMode,
    /// Server instance (only when hosting, non-threaded mode).
    pub server: Option<GameServer>,
    /// Server thread (only when hosting, threaded mode).
    #[cfg(feature = "threaded-server")]
    server_thread: Option<ServerThread>,
    /// Whether threaded server mode is enabled.
    #[cfg(feature = "threaded-server")]
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
    /// Block validator for server-side validation (anti-cheat).
    block_validator: crate::net::block_sync::BlockValidator,
    /// Input sequence number.
    pub input_sequence: u32,
    /// Last sent input state for delta-skipping idle frames.
    last_sent_input: Option<LastSentInput>,
    /// Unified incoming event queue — replaces all individual `pending_*` `Vec<T>` fields.
    ///
    /// Events are pushed here when received from the network and consumed by the
    /// game-loop subsystem that owns each variant.  Typed helper methods
    /// (`take_pending_*`, `has_pending_*`) extract only the variants they care
    /// about, preserving the ordering within each logical category.
    events: VecDeque<NetworkEvent>,
    /// Pending server world seed (received on ConnectionAccepted, needs to be applied).
    /// Kept as `Option` because "latest value wins" — a queue is not needed.
    pending_server_seed: Option<(u32, u8)>,
    /// Custom texture cache (client-side).
    pub texture_cache: CustomTextureCache,
    /// Flag indicating GPU textures need initialization.
    /// Kept as `Option` because only one initialization is ever pending at a time.
    pending_gpu_texture_init: Option<u8>,
    /// Pending day cycle pause state change from server (client-side).
    /// Kept as `Option` because the latest update supersedes any previous one.
    pub pending_day_cycle_pause: Option<crate::net::protocol::DayCyclePauseChanged>,
    /// Pending time of day update from server (client-side).
    /// Kept as `Option` because only the most recent time value matters.
    pub pending_time_update: Option<f32>,
    /// Pending spawn position update from server (client-side).
    /// Kept as `Option` because only the most recent position matters.
    pending_spawn_position: Option<crate::net::protocol::SpawnPositionChanged>,
    /// Water sync bandwidth optimizer (server-side, when hosting).
    water_sync_optimizer: WaterSyncOptimizer,
    /// Lava sync bandwidth optimizer (server-side, when hosting).
    lava_sync_optimizer: LavaSyncOptimizer,
    /// Persistent entity-ID allocator for tree fall events (server-side).
    ///
    /// Kept across broadcasts so a retransmitted `TreeFell` never reuses IDs
    /// — a previous implementation created a fresh `FallingBlockSync` every
    /// call, which meant two resends of the same tree produced overlapping
    /// IDs and broke client-side spawn/land correlation.
    tree_fall_sync: crate::net::tree_fall_sync::TreeFallSync,
    /// Per-chunk memoization of compressed chunk bytes. Keyed on position;
    /// each entry stores the `mutation_epoch` it was computed against so a
    /// block change since the snapshot invalidates on next send. Skips the
    /// LZ4 work when the same chunk is streamed to several joiners at once.
    chunk_compression_cache: std::collections::HashMap<[i32; 3], (u64, Vec<u8>)>,
    /// Materialized `(world_position, block, from_filter)` tuples queued by
    /// server-side BulkOperation handling. `from_filter` is `None` for Fill
    /// (apply unconditionally) or `Some(from_type)` for Replace (skip if the
    /// live world block doesn't match). Drained at a capped rate each tick by
    /// `take_bulk_block_batch` so a 32³ Fill / Replace doesn't stall the
    /// host for a full frame. Capacity scales with MAX_BULK_FILL_VOLUME.
    pending_bulk_blocks: VecDeque<(
        [i32; 3],
        crate::net::protocol::BlockData,
        Option<crate::chunk::BlockType>,
    )>,

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
    /// Local player's display name.
    pub local_player_name: String,
    /// Chat message history (for display).
    pub chat_history: Vec<ChatEntry>,
    /// Time remaining to show chat overlay.
    pub chat_display_timer: Option<f32>,
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
            #[cfg(feature = "threaded-server")]
            server_thread: None,
            #[cfg(feature = "threaded-server")]
            use_threaded_server: USE_THREADED_SERVER,
            client: None,
            prediction: PredictionState::new(),
            remote_players: Vec::new(),
            chunk_sync: ChunkSyncManager::new(),
            block_sync: BlockSyncManager::new(false),
            block_validator: crate::net::block_sync::BlockValidator::new(),
            input_sequence: 0,
            last_sent_input: None,
            events: VecDeque::new(),
            pending_server_seed: None,
            texture_cache: CustomTextureCache::new(0), // Will be set on connect
            pending_gpu_texture_init: None,
            pending_day_cycle_pause: None,
            pending_time_update: None,
            pending_spawn_position: None,
            water_sync_optimizer: WaterSyncOptimizer::new(),
            lava_sync_optimizer: LavaSyncOptimizer::new(),
            tree_fall_sync: crate::net::tree_fall_sync::TreeFallSync::new(),
            chunk_compression_cache: std::collections::HashMap::new(),
            pending_bulk_blocks: VecDeque::new(),
            discovery: None,
            discovery_responder: None,
            server_name: String::new(),
            max_players: 4,
            player_count: 1, // Host counts as player
            player_names: vec!["Host".to_string()],
            server_address: None,
            ping_ms: None,
            local_player_name: "Player".to_string(),
            chat_history: Vec::new(),
            chat_display_timer: None,
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
        log::debug!(
            "[Multiplayer] Starting host on {} with seed {}",
            addr,
            world_seed
        );

        #[cfg(feature = "threaded-server")]
        if self.use_threaded_server {
            // Spawn server in dedicated thread
            self.server_thread = Some(ServerThread::spawn(addr, world_seed, world_gen)?);
            log::debug!("[Multiplayer] Server thread spawned");
        } else {
            // Direct server mode
            self.server = Some(GameServer::new(addr, world_seed, world_gen)?);
            log::debug!("[Multiplayer] Direct server created");
        }
        #[cfg(not(feature = "threaded-server"))]
        {
            // Direct server mode (threaded-server feature not enabled)
            self.server = Some(GameServer::new(addr, world_seed, world_gen)?);
            log::debug!("[Multiplayer] Direct server created");
        }

        self.mode = GameMode::Host;
        self.server_name = server_name.clone();
        self.server_address = Some(addr);

        // Start discovery responder for LAN advertising
        match DiscoveryResponder::new(server_name, port, self.max_players) {
            Ok(responder) => {
                self.discovery_responder = Some(responder);
                log::debug!("[Multiplayer] Discovery responder started");
            }
            Err(e) => {
                log::error!("[Multiplayer] Failed to start discovery responder: {}", e);
            }
        }

        // Initialize host player on the server
        // Host gets player_id 0, first connected client gets 1, etc.
        if let Some(ref mut server) = self.server {
            server.set_host_player(0, "Host".to_string(), [0.0, 64.0, 0.0]);
        }
        #[cfg(feature = "threaded-server")]
        if let Some(ref server_thread) = self.server_thread {
            let _ = server_thread.send_command(ServerCommand::SetHostPlayer {
                player_id: 0,
                name: "Host".to_string(),
                position: [0.0, 64.0, 0.0],
            });
        }

        // Create local client that connects to localhost using the server's
        // per-session private key for Secure mode authentication.
        let localhost: SocketAddr = ([127, 0, 0, 1], port).into();
        let server_key = self.server.as_ref().map(|s| s.private_key());
        log::debug!(
            "[Multiplayer] Creating local client connecting to {}",
            localhost
        );
        if let Some(key) = server_key {
            self.client = Some(GameClient::with_key(localhost, key)?);
        } else {
            self.client = Some(GameClient::new(localhost)?);
        }
        self.client.as_mut().unwrap().connect();
        log::debug!("[Multiplayer] Local client created and connection started");

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
        }
        #[cfg(feature = "threaded-server")]
        if let Some(ref server_thread) = self.server_thread {
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
        // Drops and joins thread (only exists with threaded-server feature)
        #[cfg(feature = "threaded-server")]
        {
            self.server_thread = None;
        }
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

        log::debug!("[Multiplayer] Connecting to {}...", addr);
        self.client = Some(GameClient::new(addr)?);
        self.client.as_mut().unwrap().connect();
        self.mode = GameMode::Client;
        self.server_address = Some(addr);
        log::debug!(
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

    /// Sets the local player's display name.
    pub fn set_local_player_name(&mut self, name: String) {
        self.local_player_name = name;
    }

    /// Returns the local player's display name.
    pub fn get_local_player_name(&self) -> &str {
        &self.local_player_name
    }

    /// Adds a chat message to history.
    pub fn add_chat_message(&mut self, player_name: String, message: String) {
        self.chat_history.push(ChatEntry {
            player_name,
            message,
            timestamp: Instant::now(),
        });
        // Keep only last MAX_CHAT_HISTORY messages
        if self.chat_history.len() > MAX_CHAT_HISTORY {
            self.chat_history.remove(0);
        }
        // Show chat for 10 seconds
        self.chat_display_timer = Some(10.0);
    }

    /// Updates the chat display timer (call every frame with delta_time).
    pub fn update_chat_timer(&mut self, delta_time: f32) {
        if let Some(ref mut timer) = self.chat_display_timer {
            *timer -= delta_time;
            if *timer <= 0.0 {
                self.chat_display_timer = None;
            }
        }
    }

    /// Returns whether the chat overlay should be visible.
    pub fn is_chat_visible(&self) -> bool {
        self.chat_display_timer.is_some()
    }

    /// Returns the chat history for display.
    pub fn get_chat_history(&self) -> &[ChatEntry] {
        &self.chat_history
    }

    /// Returns the chat display timer remaining (if any).
    pub fn get_chat_display_timer(&self) -> Option<f32> {
        self.chat_display_timer
    }

    /// Updates the multiplayer state (call every frame).
    pub fn update(&mut self, duration: Duration) {
        // Frame counter for periodic operations (no logging)
        static UPDATE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let count = UPDATE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Handle threaded server events
        #[cfg(feature = "threaded-server")]
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
        if count.is_multiple_of(3)
            && let Some(ref mut server) = self.server
        {
            server.broadcast_player_states();
        }
        #[cfg(feature = "threaded-server")]
        #[cfg(feature = "threaded-server")]
        if let Some(ref server_thread) = self.server_thread {
            let _ = server_thread.send_command(ServerCommand::BroadcastPlayerStates);
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
                    log::debug!(
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
    #[cfg(feature = "threaded-server")]
    fn handle_thread_event(&mut self, event: ServerThreadEvent) {
        match event {
            ServerThreadEvent::ClientConnected { client_id } => {
                // Send connection acceptance with spawn position
                // TODO: Get actual spawn position from world
                let spawn_position = [0.0, 64.0, 0.0];
                #[cfg(feature = "threaded-server")]
                if let Some(ref server_thread) = self.server_thread {
                    let _ = server_thread.send_command(ServerCommand::HandleClientConnected {
                        client_id,
                        spawn_position,
                    });
                }
            }
            ServerThreadEvent::ClientDisconnected { client_id, reason } => {
                #[cfg(feature = "threaded-server")]
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
                log::error!("[Multiplayer] Server thread error: {}", error);
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
            ClientMessage::RequestChunks(mut request) => {
                // Cap chunk requests to prevent bandwidth exhaustion from a single client.
                const MAX_CHUNK_REQUEST: usize = 64;
                if request.positions.len() > MAX_CHUNK_REQUEST {
                    log::warn!(
                        "[Server] Truncating chunk request from client {}: {} > {} cap",
                        client_id,
                        request.positions.len(),
                        MAX_CHUNK_REQUEST
                    );
                    request.positions.truncate(MAX_CHUNK_REQUEST);
                }

                // Per-client dedup: drop chunks we already sent this client within
                // the recent-resend window so a noisy/buggy requester can't keep
                // costing us bandwidth.
                let before = request.positions.len();
                if let Some(ref mut server) = self.server {
                    request
                        .positions
                        .retain(|pos| server.should_send_chunk(client_id, *pos));
                }
                let deduped = before - request.positions.len();
                if deduped > 0 {
                    log::debug!(
                        "[Server] Deduped {} chunk request(s) from client {} (already recently sent)",
                        deduped,
                        client_id
                    );
                }

                if request.positions.is_empty() {
                    return;
                }

                log::debug!(
                    "[Server] Received chunk request from client {} for {} chunks",
                    client_id,
                    request.positions.len()
                );
                // Queue chunk request for processing by game loop
                self.events
                    .push_back(NetworkEvent::ChunkRequested(client_id, request.positions));
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
                }
                #[cfg(feature = "threaded-server")]
                #[cfg(feature = "threaded-server")]
                if let Some(ref server_thread) = self.server_thread {
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
                log::debug!(
                    "[Server] Received PlaceBlock at {:?} from client {}",
                    place.position,
                    client_id
                );

                // Validate placement (server-side anti-cheat)
                let validation_result = if let Some(ref server) = self.server {
                    if let Some(player_info) = server.get_player(client_id) {
                        let current_time = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_micros() as u64;
                        self.block_validator.validate_placement(
                            player_info.player_id,
                            player_info.position,
                            &place,
                            current_time,
                        )
                    } else {
                        Err("Player not found".to_string())
                    }
                } else {
                    Ok(()) // Skip validation for threaded server mode (TODO: implement)
                };

                match validation_result {
                    Ok(()) => {
                        let change = crate::net::protocol::BlockChanged {
                            position: place.position,
                            block: place.block,
                        };
                        if let Some(ref mut server) = self.server {
                            server.broadcast_block_change_except(change.clone(), client_id);
                            log::debug!(
                                "[Server] Broadcasted block change to all clients except originator"
                            );
                        }
                        #[cfg(feature = "threaded-server")]
                        #[cfg(feature = "threaded-server")]
                        if let Some(ref server_thread) = self.server_thread {
                            let _ = server_thread
                                .send_command(ServerCommand::BroadcastBlockChange(change));
                        }
                    }
                    Err(e) => {
                        log::error!(
                            "[Server] Block placement rejected for client {}: {}",
                            client_id,
                            e
                        );
                    }
                }
            }
            ClientMessage::BreakBlock(break_msg) => {
                log::debug!(
                    "[Server] Received BreakBlock at {:?} from client {}",
                    break_msg.position,
                    client_id
                );

                // Validate break (server-side anti-cheat)
                let validation_result = if let Some(ref server) = self.server {
                    if let Some(player_info) = server.get_player(client_id) {
                        let current_time = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_micros() as u64;
                        self.block_validator.validate_break(
                            player_info.player_id,
                            player_info.position,
                            &break_msg,
                            current_time,
                        )
                    } else {
                        Err("Player not found".to_string())
                    }
                } else {
                    Ok(()) // Skip validation for threaded server mode (TODO: implement)
                };

                match validation_result {
                    Ok(()) => {
                        let change = crate::net::protocol::BlockChanged {
                            position: break_msg.position,
                            block: crate::net::protocol::BlockData::default(), // Air
                        };
                        if let Some(ref mut server) = self.server {
                            server.broadcast_block_change_except(change.clone(), client_id);
                            log::debug!(
                                "[Server] Broadcasted block break to all clients except originator"
                            );
                        }
                        #[cfg(feature = "threaded-server")]
                        #[cfg(feature = "threaded-server")]
                        if let Some(ref server_thread) = self.server_thread {
                            let _ = server_thread
                                .send_command(ServerCommand::BroadcastBlockChange(change));
                        }
                    }
                    Err(e) => {
                        log::error!(
                            "[Server] Block break rejected for client {}: {}",
                            client_id,
                            e
                        );
                    }
                }
            }
            ClientMessage::ToggleDoor(toggle) => {
                // The client has already toggled the door locally and sent us the new state.
                // Broadcast the new door state to all clients.
                log::debug!(
                    "[Server] Received ToggleDoor at {:?} from client {}",
                    toggle.lower_pos,
                    client_id
                );
                let door_msg = crate::net::protocol::DoorToggled {
                    lower_pos: toggle.lower_pos,
                    lower_block: toggle.lower_block,
                    upper_pos: toggle.upper_pos,
                    upper_block: toggle.upper_block,
                };
                if let Some(ref mut server) = self.server {
                    server.broadcast_door_toggled_except(door_msg.clone(), client_id);
                    log::debug!(
                        "[Server] Broadcasted door toggle to all clients except originator"
                    );
                }
                #[cfg(feature = "threaded-server")]
                #[cfg(feature = "threaded-server")]
                if let Some(ref server_thread) = self.server_thread {
                    let _ =
                        server_thread.send_command(ServerCommand::BroadcastDoorToggled(door_msg));
                }
            }
            ClientMessage::RequestTexture(req) => {
                log::debug!(
                    "[Server] Received texture request for slot {} from client {}",
                    req.slot,
                    client_id
                );
                if let Some(ref mut server) = self.server {
                    server.handle_texture_request(client_id, req.slot);
                }
                #[cfg(feature = "threaded-server")]
                #[cfg(feature = "threaded-server")]
                if let Some(ref server_thread) = self.server_thread {
                    let _ = server_thread.send_command(ServerCommand::HandleTextureRequest {
                        client_id,
                        slot: req.slot,
                    });
                }
            }
            ClientMessage::UploadModel(upload) => {
                // Reject oversized uploads at the network boundary to prevent decompression
                // bombs; the decompressor in process_model_uploads adds a second layer of
                // validation on the declared decompressed size.
                const MAX_UPLOAD_BYTES: usize = 5 * 1024 * 1024;
                if upload.model_data.len() > MAX_UPLOAD_BYTES {
                    log::warn!(
                        "[Server] Rejected model upload '{}' from client {}: \
                         payload {} > {} byte cap",
                        upload.name,
                        client_id,
                        upload.model_data.len(),
                        MAX_UPLOAD_BYTES
                    );
                    return;
                }
                log::debug!(
                    "[Server] Received model upload '{}' from client {}",
                    upload.name,
                    client_id
                );
                // Queue for processing by game loop (needs access to model registry).
                // Unbox now that we've validated size; NetworkEvent holds the
                // UploadModel directly.
                self.events
                    .push_back(NetworkEvent::ModelUploaded(client_id, *upload));
            }
            ClientMessage::UploadTexture(upload) => {
                log::debug!(
                    "[Server] Received texture upload '{}' from client {}",
                    upload.name,
                    client_id
                );
                // Queue for processing by game loop (needs access to texture manager)
                self.events
                    .push_back(NetworkEvent::TextureUploaded(client_id, upload));
            }
            ClientMessage::UploadPicture(upload) => {
                log::debug!(
                    "[Server] Received picture upload '{}' ({} bytes) from client {}",
                    upload.name,
                    upload.png_data.len(),
                    client_id
                );
                // Queue for processing by game loop (needs access to picture manager)
                self.events
                    .push_back(NetworkEvent::PictureUploaded(client_id, upload));
            }
            ClientMessage::SetPlayerName(set_name) => {
                // Validate name
                let name = set_name.name.trim();
                if name.is_empty() || name.len() > 32 {
                    log::debug!(
                        "[Server] Rejected name change for client {}: invalid length",
                        client_id
                    );
                    return;
                }
                if !name
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == ' ')
                {
                    log::debug!(
                        "[Server] Rejected name change for client {}: invalid characters",
                        client_id
                    );
                    return;
                }
                let name = name.to_string();

                // Update player name on server
                if let Some(ref mut server) = self.server
                    && let Some((player_id, old_name, new_name)) =
                        server.set_player_name(client_id, name.clone())
                {
                    // Update remote player name
                    if let Some(remote) = self
                        .remote_players
                        .iter_mut()
                        .find(|p| p.player_id == player_id)
                    {
                        remote.name = new_name.clone();
                    }

                    // Broadcast name change to all clients
                    server.broadcast_player_name_changed(player_id, old_name, new_name);
                    log::debug!("[Server] Player {} changed name to '{}'", client_id, name);
                }
            }
            ClientMessage::ChatMessage(chat) => {
                // Reject messages that exceed the maximum allowed length.
                const MAX_CHAT_LEN: usize = 256;
                if chat.message.len() > MAX_CHAT_LEN {
                    log::warn!(
                        "[Server] Rejected chat from client {}: message too long ({} > {})",
                        client_id,
                        chat.message.len(),
                        MAX_CHAT_LEN
                    );
                    return;
                }

                // Per-client rate limit (M2): drop chat floods.
                if let Some(ref mut server) = self.server
                    && !server.check_message_rate(client_id, "chat")
                {
                    log::warn!(
                        "[Server] Rate-limited chat from client {} (5 msg / 5 s cap)",
                        client_id
                    );
                    return;
                }

                // Get player info
                let (player_id, player_name) = if let Some(ref server) = self.server {
                    if let Some(info) = server.get_player(client_id) {
                        (info.player_id, info.name.clone())
                    } else {
                        log::warn!(
                            "[Server] Rejected chat from client {}: player not found",
                            client_id
                        );
                        return;
                    }
                } else {
                    return;
                };

                // Broadcast chat to all clients
                self.broadcast_chat(player_id, player_name, chat.message);
            }
            ClientMessage::ConsoleCommand(cmd) => {
                if let Some(ref mut server) = self.server
                    && !server.check_message_rate(client_id, "console")
                {
                    log::warn!(
                        "[Server] Rate-limited console command from client {} (10 cmd / 5 s cap)",
                        client_id
                    );
                    return;
                }
                // Full server-side execution path is TODO; acknowledge the
                // message here so the rate-limit + validation boundary
                // runs regardless.
                log::debug!(
                    "[Server] Received console command from client {}: {:?}",
                    client_id,
                    cmd.command
                );
            }
            ClientMessage::BulkOperation(op) => {
                // Validate volume / template name before executing.
                if let Err(reason) = op.validate() {
                    log::warn!(
                        "[Server] Rejected BulkOperation from client {}: {}",
                        client_id,
                        reason
                    );
                    return;
                }
                // Materialize the operation into concrete (pos, block) pairs.
                // Fill / Replace produce ≤ MAX_BULK_FILL_VOLUME entries;
                // Template is not yet implemented server-side because the
                // host doesn't have a template registry wired up here.
                let queued = Self::materialize_bulk_op(&op, &mut self.pending_bulk_blocks);
                log::debug!(
                    "[Server] Queued BulkOperation from client {}: {} blocks pending",
                    client_id,
                    queued
                );
            }
            _ => {
                // Other message types not yet implemented
            }
        }
    }

    /// Handles a server event (for the host).
    fn handle_server_event(&mut self, event: renet::ServerEvent) {
        log::debug!("[Multiplayer] Processing server event: {:?}", event);
        match event {
            renet::ServerEvent::ClientConnected { client_id } => {
                log::debug!(
                    "[Server] Client {} connected - calling handle_client_connected",
                    client_id
                );
                // When hosting, spawn new players
                if let Some(ref mut server) = self.server {
                    // Check if this is the host's own client connection (first client in Host mode)
                    // The host connects to itself as a client - this is the loopback connection
                    if self.mode == GameMode::Host && server.host_client_id().is_none() {
                        log::debug!(
                            "[Server] First client in Host mode - marking as host's loopback client"
                        );
                        server.set_host_client_id(client_id);
                    }

                    // TODO: Get actual spawn position from world
                    server.handle_client_connected(client_id, [0.0, 64.0, 0.0]);
                    log::debug!(
                        "[Server] handle_client_connected returned for client {}",
                        client_id
                    );
                } else {
                    log::debug!("[Server] ERROR: No server instance available!");
                }
            }
            renet::ServerEvent::ClientDisconnected { client_id, reason } => {
                log::debug!("[Server] Client {} disconnected: {:?}", client_id, reason);

                // Get player_id before removing from server (for cleanup)
                let player_id = if let Some(ref server) = self.server {
                    server.get_player(client_id).map(|info| info.player_id)
                } else {
                    None
                };

                if let Some(ref mut server) = self.server {
                    server.handle_client_disconnected(client_id);
                }

                // Clear rate limit tracking for this player
                if let Some(pid) = player_id {
                    self.block_validator.clear_player(pid);
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
                log::debug!(
                    "[Client] Connection accepted. Player ID: {}, World seed: {}, Custom textures: {}",
                    accepted.player_id,
                    accepted.world_seed,
                    accepted.custom_texture_count
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
                            log::debug!(
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
            ServerMessage::PlayerJoined(joined)
                if !self
                    .remote_players
                    .iter()
                    .any(|p| p.player_id == joined.player_id)
                    && !{
                        let local_id = self.client.as_ref().and_then(|c| c.player_id());
                        local_id == Some(joined.player_id)
                            || (self.mode == GameMode::Host && joined.player_id == 0)
                    } =>
            {
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
                log::debug!("[Client] Received ChunkData for {:?}", chunk.position);

                // Decompress and deserialize chunk data
                match SerializedChunk::decompress(&chunk.compressed_data) {
                    Ok(serialized) => {
                        // Convert to Chunk struct
                        match serialized.to_chunk() {
                            Ok(chunk_data) => {
                                // Store for later application to world
                                self.receive_chunk(chunk.position, chunk_data);
                                log::debug!(
                                    "[Client] Chunk {:?} ready for application",
                                    chunk.position
                                );
                            }
                            Err(e) => {
                                log::error!(
                                    "[Multiplayer] Failed to convert chunk at {:?}: {}",
                                    chunk.position,
                                    e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        log::error!(
                            "[Multiplayer] Failed to decompress chunk at {:?}: {}",
                            chunk.position,
                            e
                        );
                    }
                }
            }
            ServerMessage::ChunkGenerateLocal(msg) => {
                // Server says this chunk has no modifications - generate it locally
                // Mark as pending local generation (NOT received) until chunk_loader finishes
                self.chunk_sync.mark_pending_local_generation(msg.position);
                self.events
                    .push_back(NetworkEvent::LocalChunkPending(msg.position));
                log::debug!(
                    "[Client] Received ChunkGenerateLocal for {:?}",
                    msg.position
                );
            }
            ServerMessage::BlockChanged(change) => {
                // Queue block change for application to world
                log::debug!(
                    "[Client] Received BlockChanged at {:?}: {:?}",
                    change.position,
                    change.block.block_type
                );
                self.events
                    .push_back(NetworkEvent::BlockChanged(change.clone()));
            }
            ServerMessage::BlocksChanged(changes) => {
                // Queue multiple block changes
                log::debug!(
                    "[Client] Received BlocksChanged with {} changes",
                    changes.changes.len()
                );
                for (pos, block) in &changes.changes {
                    self.events.push_back(NetworkEvent::BlockChanged(
                        crate::net::protocol::BlockChanged {
                            position: *pos,
                            block: block.clone(),
                        },
                    ));
                }
            }
            ServerMessage::ModelRegistrySync(sync) => {
                log::debug!("[Client] Received ModelRegistrySync");
                if !sync.models_data.is_empty() {
                    log::debug!(
                        "[Client] Received {} bytes of model data",
                        sync.models_data.len()
                    );
                }
                if !sync.door_pairs_data.is_empty() {
                    log::debug!(
                        "[Client] Received {} bytes of door pair data",
                        sync.door_pairs_data.len()
                    );
                }
            }
            ServerMessage::TextureData(tex) => {
                log::debug!("[Client] Received texture for slot {}", tex.slot);
                self.texture_cache.store_texture(tex.slot, tex.data.clone());
            }
            ServerMessage::TextureAdded(tex) => {
                log::debug!("[Client] Texture added: slot {} = '{}'", tex.slot, tex.name);
            }
            ServerMessage::ModelAdded(model) => {
                log::debug!(
                    "[Client] Model added: ID {} = '{}' by '{}'",
                    model.model_id,
                    model.name,
                    model.author
                );
                self.events
                    .push_back(NetworkEvent::ModelAdded(model.clone()));
            }
            ServerMessage::WaterCellsChanged(water) => {
                log::debug!(
                    "[Client] Received WaterCellsChanged with {} updates",
                    water.updates.len()
                );
                for update in &water.updates {
                    self.events
                        .push_back(NetworkEvent::WaterCellUpdated(update.clone()));
                }
            }
            ServerMessage::LavaCellsChanged(lava) => {
                log::debug!(
                    "[Client] Received LavaCellsChanged with {} updates",
                    lava.updates.len()
                );
                for update in &lava.updates {
                    self.events
                        .push_back(NetworkEvent::LavaCellUpdated(update.clone()));
                }
            }
            ServerMessage::FallingBlockSpawned(spawn) => {
                log::debug!(
                    "[Client] Received FallingBlockSpawned: entity {} at {:?}",
                    spawn.entity_id,
                    spawn.position
                );
                self.events
                    .push_back(NetworkEvent::FallingBlockSpawned(spawn.clone()));
            }
            ServerMessage::FallingBlockLanded(land) => {
                log::debug!(
                    "[Client] Received FallingBlockLanded: entity {} at {:?}",
                    land.entity_id,
                    land.position
                );
                self.events
                    .push_back(NetworkEvent::FallingBlockLanded(land.clone()));
            }
            ServerMessage::TreeFell(tree_fell) => {
                log::debug!(
                    "[Client] Received TreeFell with {} blocks",
                    tree_fell.blocks.len()
                );
                self.events
                    .push_back(NetworkEvent::TreeFell(tree_fell.clone()));
            }
            ServerMessage::DayCyclePauseChanged(pause) => {
                log::debug!(
                    "[Client] Received DayCyclePauseChanged: {} at time {:.3}",
                    if pause.paused { "PAUSED" } else { "RUNNING" },
                    pause.time_of_day
                );
                self.pending_day_cycle_pause = Some(pause.clone());
            }
            ServerMessage::TimeUpdate(time) => {
                log::debug!("[Client] Received TimeUpdate: {:.3}", time.time_of_day);
                self.pending_time_update = Some(time.time_of_day);
            }
            ServerMessage::SpawnPositionChanged(spawn) => {
                log::debug!(
                    "[Client] Received SpawnPositionChanged: ({:.1}, {:.1}, {:.1})",
                    spawn.position[0],
                    spawn.position[1],
                    spawn.position[2]
                );
                self.pending_spawn_position = Some(spawn.clone());
            }
            ServerMessage::FramePictureSet(frame) => {
                log::debug!(
                    "[Client] Received FramePictureSet at {:?}: picture_id={:?}",
                    frame.position,
                    frame.picture_id
                );
                self.events
                    .push_back(NetworkEvent::FramePictureSet(frame.clone()));
            }
            ServerMessage::PictureAdded(picture) => {
                log::debug!(
                    "[Client] Received PictureAdded: id={} name='{}'",
                    picture.picture_id,
                    picture.name
                );
                // Picture metadata is received; actual PNG data would be requested separately if needed
            }
            ServerMessage::StencilLoaded(stencil) => {
                log::debug!(
                    "[Client] Received StencilLoaded: id={} name='{}' ({} bytes)",
                    stencil.stencil_id,
                    stencil.name,
                    stencil.stencil_data.len()
                );
                self.events
                    .push_back(NetworkEvent::StencilLoaded(stencil.clone()));
            }
            ServerMessage::StencilTransformUpdate(transform) => {
                log::debug!(
                    "[Client] Received StencilTransformUpdate: id={} pos={:?} rot={}",
                    transform.stencil_id,
                    transform.position,
                    transform.rotation
                );
                self.events
                    .push_back(NetworkEvent::StencilTransformUpdated(transform.clone()));
            }
            ServerMessage::StencilRemoved(removed) => {
                log::debug!(
                    "[Client] Received StencilRemoved: id={}",
                    removed.stencil_id
                );
                self.events
                    .push_back(NetworkEvent::StencilRemoved(removed.clone()));
            }
            ServerMessage::TemplateLoaded(template) => {
                log::debug!(
                    "[Client] Received TemplateLoaded: id={} name='{}' ({} bytes)",
                    template.template_id,
                    template.name,
                    template.template_data.len()
                );
                self.events
                    .push_back(NetworkEvent::TemplateLoaded(template.clone()));
            }
            ServerMessage::TemplateRemoved(removed) => {
                log::debug!(
                    "[Client] Received TemplateRemoved: id={}",
                    removed.template_id
                );
                self.events
                    .push_back(NetworkEvent::TemplateRemoved(removed.clone()));
            }
            ServerMessage::DoorToggled(door) => {
                log::debug!(
                    "[Client] Received DoorToggled: lower={:?}, upper={:?}",
                    door.lower_pos,
                    door.upper_pos
                );
                // Queue the door changes as block changes to be applied
                self.events.push_back(NetworkEvent::BlockChanged(
                    crate::net::protocol::BlockChanged {
                        position: door.lower_pos,
                        block: door.lower_block.clone(),
                    },
                ));
                self.events.push_back(NetworkEvent::BlockChanged(
                    crate::net::protocol::BlockChanged {
                        position: door.upper_pos,
                        block: door.upper_block.clone(),
                    },
                ));
            }
            ServerMessage::PlayerNameChanged(change) => {
                log::debug!(
                    "[Client] Received PlayerNameChanged: {} -> {} (player {})",
                    change.old_name,
                    change.new_name,
                    change.player_id
                );
                // Update remote player name
                if let Some(player) = self
                    .remote_players
                    .iter_mut()
                    .find(|p| p.player_id == change.player_id)
                {
                    player.name = change.new_name.clone();
                }
                // Queue for UI notification
                self.events
                    .push_back(NetworkEvent::PlayerNameChanged(change.clone()));
            }
            ServerMessage::ChatReceived(chat) => {
                log::debug!(
                    "[Client] Received ChatReceived from {}: {}",
                    chat.player_name,
                    chat.message
                );
                // Queue for processing in update loop
                self.events
                    .push_back(NetworkEvent::ChatReceived(chat.clone()));
            }
            _ => {}
        }
    }

    /// Sends player input to the server, skipping frames where movement is below thresholds.
    ///
    /// Prediction recording still happens every call so local reconciliation stays accurate.
    /// Network sends are suppressed when all of the following are true compared to the last
    /// sent input: `|Δposition| < POSITION_THRESHOLD`, `|Δvelocity| < VELOCITY_THRESHOLD`,
    /// `|Δyaw/pitch| < ROTATION_THRESHOLD`, actions unchanged, and fewer than
    /// `FORCE_SEND_EVERY` calls have elapsed since the last send (keep-alive).
    pub fn send_input(
        &mut self,
        position: [f32; 3],
        velocity: [f32; 3],
        yaw: f32,
        pitch: f32,
        actions: crate::net::protocol::InputActions,
    ) {
        const POSITION_THRESHOLD: f32 = 0.01; // 1 cm
        const VELOCITY_THRESHOLD: f32 = 0.1; // 10 cm/s
        const ROTATION_THRESHOLD: f32 = 0.0087; // ~0.5°
        const FORCE_SEND_EVERY: u32 = 20; // ~1 Hz keep-alive at 20 Hz send rate

        if let Some(ref mut client) = self.client {
            // Record input for prediction every call (local state must stay in sync).
            self.prediction
                .record_input(position, velocity, yaw, pitch, actions);

            let should_skip = match self.last_sent_input.as_mut() {
                Some(last) if last.skips_remaining > 0 => {
                    let pos_delta = max_abs_delta(position, last.position);
                    let vel_delta = max_abs_delta(velocity, last.velocity);
                    let yaw_delta = (yaw - last.yaw).abs();
                    let pitch_delta = (pitch - last.pitch).abs();
                    let actions_changed = actions != last.actions;

                    if !actions_changed
                        && pos_delta < POSITION_THRESHOLD
                        && vel_delta < VELOCITY_THRESHOLD
                        && yaw_delta < ROTATION_THRESHOLD
                        && pitch_delta < ROTATION_THRESHOLD
                    {
                        last.skips_remaining -= 1;
                        true
                    } else {
                        false
                    }
                }
                _ => false,
            };

            if !should_skip {
                client.send_input(self.input_sequence, position, velocity, yaw, pitch, actions);
                self.input_sequence = self.input_sequence.wrapping_add(1);
                self.last_sent_input = Some(LastSentInput {
                    position,
                    velocity,
                    yaw,
                    pitch,
                    actions,
                    skips_remaining: FORCE_SEND_EVERY,
                });
            }
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

    /// Sends a door toggle request to the server with the new block data.
    pub fn send_toggle_door(
        &mut self,
        lower_pos: [i32; 3],
        lower_block: crate::net::protocol::BlockData,
        upper_pos: [i32; 3],
        upper_block: crate::net::protocol::BlockData,
    ) {
        if let Some(ref mut client) = self.client {
            client.send_toggle_door(lower_pos, lower_block, upper_pos, upper_block);
            // Flush immediately for responsive door sync
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

    /// Drains events matching `extract`, preserving the order of unmatched events.
    ///
    /// This is the canonical helper used by all `take_pending_*` methods to
    /// extract typed variants from the unified `events` queue.
    fn drain_variant<T, F>(events: &mut VecDeque<NetworkEvent>, mut extract: F) -> Vec<T>
    where
        F: FnMut(&NetworkEvent) -> Option<T>,
    {
        let mut out = Vec::new();
        let mut remaining = VecDeque::with_capacity(events.len());
        for event in events.drain(..) {
            match extract(&event) {
                Some(value) => out.push(value),
                None => remaining.push_back(event),
            }
        }
        *events = remaining;
        out
    }

    /// Like `drain_variant` but stops after extracting `budget` items.
    /// Remaining matched events stay in the queue for subsequent frames.
    fn drain_variant_budgeted<T, F>(
        events: &mut VecDeque<NetworkEvent>,
        budget: usize,
        mut extract: F,
    ) -> Vec<T>
    where
        F: FnMut(&NetworkEvent) -> Option<T>,
    {
        let mut out = Vec::with_capacity(budget);
        let mut remaining = VecDeque::with_capacity(events.len());
        let mut taken = 0;
        for event in events.drain(..) {
            if taken < budget {
                match extract(&event) {
                    Some(value) => {
                        out.push(value);
                        taken += 1;
                    }
                    None => remaining.push_back(event),
                }
            } else {
                remaining.push_back(event);
            }
        }
        *events = remaining;
        out
    }

    /// Takes pending block changes and clears the queue.
    /// Call this from the game loop to apply changes to the world.
    pub fn take_pending_block_changes(&mut self) -> Vec<crate::net::protocol::BlockChanged> {
        Self::drain_variant(&mut self.events, |e| match e {
            NetworkEvent::BlockChanged(change) => Some(change.clone()),
            _ => None,
        })
    }

    /// Takes up to `budget` pending block changes, leaving the rest in the
    /// queue for subsequent frames. Used by the client-side frame-distributed
    /// bulk-block application path (H13).
    pub fn take_pending_block_changes_budgeted(
        &mut self,
        budget: usize,
    ) -> Vec<crate::net::protocol::BlockChanged> {
        Self::drain_variant_budgeted(&mut self.events, budget, |e| match e {
            NetworkEvent::BlockChanged(change) => Some(change.clone()),
            _ => None,
        })
    }

    /// Returns true if there are pending block changes to apply.
    pub fn has_pending_block_changes(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, NetworkEvent::BlockChanged(_)))
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

    /// Returns true if we are a pure client (connected to remote server).
    /// Pure clients should NOT process physics locally - the server is authoritative.
    pub fn is_client(&self) -> bool {
        self.mode == GameMode::Client
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
        self.events
            .push_back(NetworkEvent::ChunkReceived(pos, Box::new(chunk)));
    }

    /// Takes all pending chunks and clears the queue.
    /// Call this from the game loop to apply chunks to the world.
    pub fn take_pending_chunks(&mut self) -> Vec<(Vector3<i32>, Chunk)> {
        let mut out = Vec::new();
        let mut remaining = VecDeque::with_capacity(self.events.len());
        for event in self.events.drain(..) {
            match event {
                NetworkEvent::ChunkReceived(pos, chunk) => out.push((pos, *chunk)),
                other => remaining.push_back(other),
            }
        }
        self.events = remaining;
        out
    }

    /// Returns true if there are pending chunks to apply.
    pub fn has_pending_chunks(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, NetworkEvent::ChunkReceived(_, _)))
    }

    /// Returns the number of pending chunks.
    pub fn pending_chunk_count(&self) -> usize {
        self.events
            .iter()
            .filter(|e| matches!(e, NetworkEvent::ChunkReceived(_, _)))
            .count()
    }

    /// Takes all pending local chunk positions and clears the queue.
    /// These chunks should be generated locally using the world seed.
    pub fn take_pending_local_chunks(&mut self) -> Vec<[i32; 3]> {
        Self::drain_variant(&mut self.events, |e| match e {
            NetworkEvent::LocalChunkPending(pos) => Some(*pos),
            _ => None,
        })
    }

    /// Returns true if there are pending local chunks to generate.
    pub fn has_pending_local_chunks(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, NetworkEvent::LocalChunkPending(_)))
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
        Self::drain_variant(&mut self.events, |e| match e {
            NetworkEvent::ChunkRequested(client_id, positions) => {
                Some((*client_id, positions.clone()))
            }
            _ => None,
        })
    }

    /// Returns true if there are pending chunk requests from clients.
    pub fn has_pending_chunk_requests(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, NetworkEvent::ChunkRequested(_, _)))
    }

    /// Expands a validated `BulkOperation` into `(world_pos, block)` pairs and
    /// pushes them into `queue`. Returns the number of entries enqueued.
    ///
    /// Replace entries carry `Some(from_type)` as a filter — the applier checks
    /// the live world and skips positions where the current block doesn't match.
    /// Fill entries carry `None` (apply unconditionally).
    /// Template ops are not yet materialized server-side (no template registry
    /// hooked into the host path).
    fn materialize_bulk_op(
        op: &crate::net::protocol::BulkOperation,
        queue: &mut VecDeque<(
            [i32; 3],
            crate::net::protocol::BlockData,
            Option<crate::chunk::BlockType>,
        )>,
    ) -> usize {
        use crate::net::protocol::BulkOperation;
        match op {
            BulkOperation::Fill { start, end, block } => {
                let (sx, ex) = (start[0].min(end[0]), start[0].max(end[0]));
                let (sy, ey) = (start[1].min(end[1]), start[1].max(end[1]));
                let (sz, ez) = (start[2].min(end[2]), start[2].max(end[2]));
                let before = queue.len();
                for y in sy..=ey {
                    for z in sz..=ez {
                        for x in sx..=ex {
                            queue.push_back(([x, y, z], block.clone(), None));
                        }
                    }
                }
                queue.len() - before
            }
            BulkOperation::Replace {
                start,
                end,
                from,
                to,
            } => {
                let (sx, ex) = (start[0].min(end[0]), start[0].max(end[0]));
                let (sy, ey) = (start[1].min(end[1]), start[1].max(end[1]));
                let (sz, ez) = (start[2].min(end[2]), start[2].max(end[2]));
                let before = queue.len();
                for y in sy..=ey {
                    for z in sz..=ez {
                        for x in sx..=ex {
                            queue.push_back(([x, y, z], to.clone(), Some(*from)));
                        }
                    }
                }
                queue.len() - before
            }
            BulkOperation::Template { .. } => {
                log::warn!(
                    "[Server] BulkOperation::Template is not yet materializable \
                     server-side; skipping (no host-side template registry)"
                );
                0
            }
        }
    }

    /// Drains up to `budget` block placements from the pending bulk queue.
    /// Each entry carries `(position, block, from_filter)` where `from_filter`
    /// is `None` for Fill or `Some(from_type)` for Replace. The caller applies
    /// matching entries to the world and broadcasts the result.
    pub fn take_bulk_block_batch(
        &mut self,
        budget: usize,
    ) -> Vec<(
        [i32; 3],
        crate::net::protocol::BlockData,
        Option<crate::chunk::BlockType>,
    )> {
        let n = budget.min(self.pending_bulk_blocks.len());
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            if let Some(triple) = self.pending_bulk_blocks.pop_front() {
                out.push(triple);
            }
        }
        out
    }

    /// Returns the current pending-bulk queue depth. Useful for the debug HUD.
    pub fn pending_bulk_depth(&self) -> usize {
        self.pending_bulk_blocks.len()
    }

    /// Takes all pending models received from server and clears the queue.
    /// Call this from the game loop to register models in the registry.
    pub fn take_pending_models(&mut self) -> Vec<crate::net::protocol::ModelAdded> {
        Self::drain_variant(&mut self.events, |e| match e {
            NetworkEvent::ModelAdded(model) => Some(model.clone()),
            _ => None,
        })
    }

    /// Returns true if there are pending models to register.
    pub fn has_pending_models(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, NetworkEvent::ModelAdded(_)))
    }

    /// Takes all pending model uploads from clients and clears the queue.
    /// Call this from the game loop when hosting to process model uploads.
    pub fn take_pending_model_uploads(&mut self) -> Vec<(u64, crate::net::protocol::UploadModel)> {
        Self::drain_variant(&mut self.events, |e| match e {
            NetworkEvent::ModelUploaded(client_id, upload) => Some((*client_id, upload.clone())),
            _ => None,
        })
    }

    /// Returns true if there are pending model uploads to process.
    pub fn has_pending_model_uploads(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, NetworkEvent::ModelUploaded(_, _)))
    }

    /// Takes all pending texture uploads from clients and clears the queue.
    /// Call this from the game loop when hosting to process texture uploads.
    pub fn take_pending_texture_uploads(
        &mut self,
    ) -> Vec<(u64, crate::net::protocol::UploadTexture)> {
        Self::drain_variant(&mut self.events, |e| match e {
            NetworkEvent::TextureUploaded(client_id, upload) => Some((*client_id, upload.clone())),
            _ => None,
        })
    }

    /// Returns true if there are pending texture uploads to process.
    pub fn has_pending_texture_uploads(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, NetworkEvent::TextureUploaded(_, _)))
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
        // Epoch-aware dedup: skip sending if this exact epoch was already sent
        // to this client within the dedup window.
        let epoch = chunk.mutation_epoch();
        if let Some(ref server) = self.server
            && !server.should_send_chunk_with_epoch(client_id, position, epoch)
        {
            return;
        }

        // Check if chunk has been modified by players
        if !chunk.persistence_dirty {
            // Chunk is unmodified - tell client to generate it locally from seed
            if let Some(ref mut server) = self.server {
                server.send_chunk_generate_local(client_id, position);
            }
            #[cfg(feature = "threaded-server")]
            #[cfg(feature = "threaded-server")]
            if let Some(ref server_thread) = self.server_thread {
                let _ = server_thread.send_command(ServerCommand::SendChunkGenerateLocal {
                    client_id,
                    position,
                });
            }
            return;
        }

        // Chunk has modifications - serialize and send full data. Consult
        // the per-position compression cache first: if its mutation_epoch
        // matches the chunk's current epoch, reuse the bytes instead of
        // re-running LZ4.
        let cur_epoch = chunk.mutation_epoch();
        let compressed_opt: Option<Vec<u8>> = match self.chunk_compression_cache.get(&position) {
            Some((cached_epoch, bytes)) if *cached_epoch == cur_epoch => Some(bytes.clone()),
            _ => None,
        };

        let (compressed, version) = if let Some(bytes) = compressed_opt {
            // Cache hit — skip serialize + compress entirely.
            (Ok(bytes), 1u32)
        } else {
            let serialized = SerializedChunk::from_chunk(position, chunk);
            let compressed = serialized.compress();
            if let Ok(ref bytes) = compressed {
                self.chunk_compression_cache
                    .insert(position, (cur_epoch, bytes.clone()));
            }
            (compressed, serialized.version)
        };

        // Compress for network transmission
        match compressed {
            Ok(compressed) => {
                let chunk_data = crate::net::protocol::ChunkData {
                    position,
                    version,
                    compressed_data: compressed,
                };

                if let Some(ref mut server) = self.server {
                    server.send_chunk_with_epoch(client_id, chunk_data.clone(), epoch);
                }
                #[cfg(feature = "threaded-server")]
                #[cfg(feature = "threaded-server")]
                if let Some(ref server_thread) = self.server_thread {
                    let _ = server_thread.send_command(ServerCommand::SendChunk {
                        client_id,
                        chunk: chunk_data,
                    });
                }
            }
            Err(e) => {
                log::error!(
                    "[Multiplayer] Failed to compress chunk at {:?}: {}",
                    position,
                    e
                );
            }
        }
    }

    /// Requests a custom texture if not cached.
    pub fn request_texture_if_needed(&mut self, slot: u8) {
        if self.texture_cache.request_if_needed(slot)
            && let Some(ref mut client) = self.client
        {
            client.send_texture_request(slot);
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

    /// Broadcasts water source placement to all clients (server-side, when hosting).
    pub fn broadcast_water_source(
        &mut self,
        position: [i32; 3],
        water_type: crate::chunk::WaterType,
    ) {
        let update = crate::net::protocol::WaterCellUpdate {
            position,
            mass: 1.0, // Source is always full
            is_source: true,
            water_type,
        };
        if let Some(ref mut server) = self.server {
            server.broadcast_water_cells_changed(vec![update]);
        }
        // Note: Threaded server mode would need ServerCommand variant added
    }

    /// Broadcasts batch water cell updates to all clients (server-side, when hosting).
    ///
    /// Uses bandwidth optimization:
    /// - **Delta encoding**: Only sends cells with significant mass changes (> 5%)
    /// - **AoI filtering**: Only sends cells within 128 blocks of any player
    /// - **Rate limiting**: Max 5 Hz update rate regardless of simulation speed
    ///
    /// Call this after each water simulation tick. The optimizer will accumulate
    /// changes and broadcast them when appropriate.
    pub fn broadcast_water_cell_updates(
        &mut self,
        updates: Vec<crate::water::WaterCellSyncUpdate>,
    ) {
        if updates.is_empty() {
            return;
        }

        // Apply delta encoding - filter to only significant changes
        let _significant = self
            .water_sync_optimizer
            .filter_significant_changes(updates);

        // Check rate limiting - only broadcast at appropriate intervals
        if !self.water_sync_optimizer.should_broadcast_now() {
            // Accumulate changes for next broadcast window
            return;
        }

        // Collect player positions for AoI filtering
        let player_positions = self.get_all_player_positions();

        // Get filtered updates (AoI + rate limiting)
        let filtered_updates = if player_positions.is_empty() {
            // No players - use all pending (shouldn't happen in practice)
            self.water_sync_optimizer.take_all_pending_updates()
        } else {
            self.water_sync_optimizer
                .take_filtered_updates(&player_positions)
        };

        if filtered_updates.is_empty() {
            return;
        }

        if let Some(ref mut server) = self.server {
            server.broadcast_water_cells_changed(filtered_updates);
        }
        // Note: Threaded server mode would need ServerCommand variant added
    }

    /// Collects positions of all players (host + connected) for AoI filtering.
    fn get_all_player_positions(&self) -> Vec<[f32; 3]> {
        let mut positions = Vec::new();

        // Get positions from the server (includes both host and connected players)
        if let Some(ref server) = self.server {
            for player in server.players() {
                positions.push(player.position);
            }
        }

        positions
    }

    /// Broadcasts lava cell updates to all connected clients.
    ///
    /// Call this after each lava simulation tick when hosting.
    /// Uses the same optimizer pipeline as water (delta encoding, AoI, rate limiting).
    pub fn broadcast_lava_cell_updates(&mut self, updates: Vec<crate::lava::LavaCellSyncUpdate>) {
        if updates.is_empty() {
            return;
        }

        // Apply delta encoding - filter to only significant changes
        let _significant = self.lava_sync_optimizer.filter_significant_changes(updates);

        // Check rate limiting - only broadcast at appropriate intervals
        if !self.lava_sync_optimizer.should_broadcast_now() {
            return;
        }

        // Collect player positions for AoI filtering
        let player_positions = self.get_all_player_positions();

        // Get filtered updates (AoI + rate limiting)
        let filtered_updates = if player_positions.is_empty() {
            self.lava_sync_optimizer.take_all_pending_updates()
        } else {
            self.lava_sync_optimizer
                .take_filtered_updates(&player_positions)
        };

        if filtered_updates.is_empty() {
            return;
        }

        if let Some(ref mut server) = self.server {
            server.broadcast_lava_cells_changed(filtered_updates);
        }
    }

    /// Returns water sync optimizer statistics for debugging.
    pub fn water_sync_stats(&self) -> &crate::net::water_sync::WaterSyncStats {
        self.water_sync_optimizer.stats()
    }

    /// Prunes distant cached water states to prevent memory growth.
    /// Call this periodically (e.g., every 30 seconds).
    pub fn prune_water_sync_cache(&mut self) {
        let player_positions = self.get_all_player_positions();
        self.water_sync_optimizer
            .prune_distant_states(&player_positions);
    }

    /// Returns lava sync optimizer statistics for debugging.
    pub fn lava_sync_stats(&self) -> &crate::net::lava_sync::LavaSyncStats {
        self.lava_sync_optimizer.stats()
    }

    /// Prunes distant cached lava states to prevent memory growth.
    /// Call this periodically (e.g., every 30 seconds).
    pub fn prune_lava_sync_cache(&mut self) {
        let player_positions = self.get_all_player_positions();
        self.lava_sync_optimizer
            .prune_distant_states(&player_positions);
    }

    /// Takes all pending water updates and clears the queue.
    /// Call this from the game loop to apply water changes to the local simulation.
    pub fn take_pending_water_updates(&mut self) -> Vec<crate::net::protocol::WaterCellUpdate> {
        Self::drain_variant(&mut self.events, |e| match e {
            NetworkEvent::WaterCellUpdated(update) => Some(update.clone()),
            _ => None,
        })
    }

    /// Returns true if there are pending water updates to apply.
    pub fn has_pending_water_updates(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, NetworkEvent::WaterCellUpdated(_)))
    }

    /// Takes all pending lava updates and clears the queue.
    /// Call this from the game loop to apply lava changes to the local simulation.
    pub fn take_pending_lava_updates(&mut self) -> Vec<crate::net::protocol::LavaCellUpdate> {
        Self::drain_variant(&mut self.events, |e| match e {
            NetworkEvent::LavaCellUpdated(update) => Some(update.clone()),
            _ => None,
        })
    }

    /// Returns true if there are pending lava updates to apply.
    pub fn has_pending_lava_updates(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, NetworkEvent::LavaCellUpdated(_)))
    }

    /// Takes all pending falling block spawns and clears the queue.
    /// Call this from the game loop to spawn falling blocks in the client simulation.
    pub fn take_pending_falling_block_spawns(
        &mut self,
    ) -> Vec<crate::net::protocol::FallingBlockSpawned> {
        Self::drain_variant(&mut self.events, |e| match e {
            NetworkEvent::FallingBlockSpawned(spawn) => Some(spawn.clone()),
            _ => None,
        })
    }

    /// Returns true if there are pending falling block spawns to apply.
    pub fn has_pending_falling_block_spawns(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, NetworkEvent::FallingBlockSpawned(_)))
    }

    /// Takes all pending falling block lands and clears the queue.
    /// Call this from the game loop to handle landed blocks in the client simulation.
    pub fn take_pending_falling_block_lands(
        &mut self,
    ) -> Vec<crate::net::protocol::FallingBlockLanded> {
        Self::drain_variant(&mut self.events, |e| match e {
            NetworkEvent::FallingBlockLanded(land) => Some(land.clone()),
            _ => None,
        })
    }

    /// Returns true if there are pending falling block lands to apply.
    pub fn has_pending_falling_block_lands(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, NetworkEvent::FallingBlockLanded(_)))
    }

    /// Broadcasts a falling block spawn to all clients (server-side, when hosting).
    ///
    /// # Arguments
    /// * `entity_id` - Unique entity ID assigned by FallingBlockSystem
    /// * `position` - World position of the falling block (center)
    /// * `block_type` - Type of block that is falling
    pub fn broadcast_falling_block_spawn(
        &mut self,
        entity_id: u32,
        position: [f32; 3],
        block_type: crate::chunk::BlockType,
    ) {
        let spawn = crate::net::protocol::FallingBlockSpawned {
            entity_id,
            position,
            velocity: [0.0, 0.0, 0.0],
            block_type,
        };

        if let Some(ref mut server) = self.server {
            server.broadcast_falling_block_spawned(spawn);
        }
        // Note: Threaded server mode would need ServerCommand variant added
    }

    /// Broadcasts a falling block landing to all clients (server-side, when hosting).
    pub fn broadcast_falling_block_land(
        &mut self,
        entity_id: u32,
        position: [i32; 3],
        block_type: crate::chunk::BlockType,
    ) {
        let land = crate::net::protocol::FallingBlockLanded {
            entity_id,
            position,
            block_type,
        };

        if let Some(ref mut server) = self.server {
            server.broadcast_falling_block_landed(land);
        }
        // Note: Threaded server mode would need ServerCommand variant added
    }

    /// Broadcasts a tree fall event to all clients (server-side, when hosting).
    /// This is more bandwidth-efficient than sending individual FallingBlockSpawned messages
    /// when a whole tree (multiple connected logs and leaves) loses ground support.
    ///
    /// # Arguments
    /// * `blocks` - List of (position, block_type) pairs for all blocks in the tree
    ///
    /// # Returns
    /// A vector of entity IDs assigned to each falling block, in the same order as input.
    pub fn broadcast_tree_fell(
        &mut self,
        blocks: Vec<(nalgebra::Vector3<i32>, crate::chunk::BlockType)>,
    ) -> Vec<u32> {
        // Use the persistent tree_fall_sync so entity IDs monotonically advance
        // across the whole session; retransmitted or back-to-back trees never
        // collide with each other's IDs. Large trees are split into multiple
        // TreeFell messages to stay under MTU.
        let msgs = self.tree_fall_sync.build_tree_fell_batched(blocks);

        let entity_ids: Vec<u32> = msgs
            .iter()
            .flat_map(|m| m.blocks.iter().map(|b| b.entity_id))
            .collect();

        if let Some(ref mut server) = self.server {
            for msg in msgs {
                server.broadcast_tree_fell(msg);
            }
        }
        // Note: Threaded server mode would need ServerCommand variant added

        entity_ids
    }

    /// Takes all pending tree fall events and clears the queue.
    /// Call this from the game loop to spawn falling blocks in the client simulation.
    pub fn take_pending_tree_falls(&mut self) -> Vec<crate::net::protocol::TreeFell> {
        Self::drain_variant(&mut self.events, |e| match e {
            NetworkEvent::TreeFell(tree_fell) => Some(tree_fell.clone()),
            _ => None,
        })
    }

    /// Returns true if there are pending tree fall events to apply.
    pub fn has_pending_tree_falls(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, NetworkEvent::TreeFell(_)))
    }

    /// Broadcasts a batch of falling block landings to all clients (server-side, when hosting).
    /// This is useful when multiple blocks from a tree fall land at similar times.
    ///
    /// # Arguments
    /// * `lands` - List of (entity_id, position, block_type) tuples
    pub fn broadcast_falling_block_lands_batch(
        &mut self,
        lands: Vec<(u32, [i32; 3], crate::chunk::BlockType)>,
    ) {
        for (entity_id, position, block_type) in lands {
            self.broadcast_falling_block_land(entity_id, position, block_type);
        }
    }

    /// Broadcasts day cycle pause state change to all clients (server-side, when hosting).
    ///
    /// # Arguments
    /// * `paused` - Whether the day cycle is now paused
    /// * `time_of_day` - Current time of day (0.0-1.0, where 0.5 = noon)
    pub fn broadcast_day_cycle_pause(&mut self, paused: bool, time_of_day: f32) {
        if let Some(ref mut server) = self.server {
            server.broadcast_day_cycle_pause(paused, time_of_day);
        }
    }

    /// Takes pending day cycle pause state change (client-side).
    /// Returns None if no pending change.
    pub fn take_pending_day_cycle_pause(
        &mut self,
    ) -> Option<crate::net::protocol::DayCyclePauseChanged> {
        self.pending_day_cycle_pause.take()
    }

    /// Returns true if there's a pending day cycle pause change.
    pub fn has_pending_day_cycle_pause(&self) -> bool {
        self.pending_day_cycle_pause.is_some()
    }

    /// Takes pending time of day update (client-side).
    /// Returns None if no pending update.
    pub fn take_pending_time_update(&mut self) -> Option<f32> {
        self.pending_time_update.take()
    }

    /// Returns true if there's a pending time update.
    pub fn has_pending_time_update(&self) -> bool {
        self.pending_time_update.is_some()
    }

    /// Broadcasts spawn position change to all clients (server-side, when hosting).
    ///
    /// # Arguments
    /// * `position` - New spawn position in world coordinates
    pub fn broadcast_spawn_position(&mut self, position: [f32; 3]) {
        if let Some(ref mut server) = self.server {
            server.broadcast_spawn_position(position);
        }
    }

    /// Takes pending spawn position update (client-side).
    /// Returns None if no pending update.
    pub fn take_pending_spawn_position(
        &mut self,
    ) -> Option<crate::net::protocol::SpawnPositionChanged> {
        self.pending_spawn_position.take()
    }

    /// Returns true if there's a pending spawn position update.
    pub fn has_pending_spawn_position(&self) -> bool {
        self.pending_spawn_position.is_some()
    }

    // ========================================================================
    // Picture Sync Methods
    // ========================================================================

    /// Takes all pending picture uploads from clients and clears the queue.
    /// Call this from the game loop when hosting to process picture uploads.
    pub fn take_pending_picture_uploads(
        &mut self,
    ) -> Vec<(u64, crate::net::protocol::UploadPicture)> {
        Self::drain_variant(&mut self.events, |e| match e {
            NetworkEvent::PictureUploaded(client_id, upload) => Some((*client_id, upload.clone())),
            _ => None,
        })
    }

    /// Returns true if there are pending picture uploads to process.
    pub fn has_pending_picture_uploads(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, NetworkEvent::PictureUploaded(_, _)))
    }

    /// Adds a picture to the server's picture store and broadcasts to all clients.
    /// Call this from the game loop when hosting after taking pending uploads.
    /// Returns the assigned picture ID, or None on failure.
    pub fn add_picture_and_broadcast(&mut self, name: &str, png_data: &[u8]) -> Option<u16> {
        // Add picture to server's picture manager
        let picture_id = if let Some(ref mut server) = self.server {
            match server.add_picture(name, png_data) {
                Ok(id) => id,
                Err(e) => {
                    log::error!("[Server] Failed to add picture '{}': {}", name, e);
                    return None;
                }
            }
        } else {
            log::error!("[Server] Cannot add picture: server not running");
            return None;
        };

        // Broadcast to all clients
        if let Some(ref mut server) = self.server {
            server.broadcast_picture_added(picture_id, name.to_string());
        }

        Some(picture_id)
    }

    /// Broadcasts a picture frame assignment to all clients (server-side, when hosting).
    ///
    /// # Arguments
    /// * `position` - World position of the picture frame block
    /// * `picture_id` - ID of the picture to display, or None to clear the frame
    pub fn broadcast_frame_picture_set(&mut self, position: [i32; 3], picture_id: Option<u16>) {
        if let Some(ref mut server) = self.server {
            server.broadcast_frame_picture_set(position, picture_id);
        }
    }

    /// Takes all pending frame picture set updates and clears the queue.
    /// Call this from the game loop to apply frame picture changes to the local world.
    pub fn take_pending_frame_picture_sets(
        &mut self,
    ) -> Vec<crate::net::protocol::FramePictureSet> {
        Self::drain_variant(&mut self.events, |e| match e {
            NetworkEvent::FramePictureSet(frame) => Some(frame.clone()),
            _ => None,
        })
    }

    /// Returns true if there are pending frame picture set updates to apply.
    pub fn has_pending_frame_picture_sets(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, NetworkEvent::FramePictureSet(_)))
    }

    // ========================================================================
    // Stencil Sync Methods
    // ========================================================================

    /// Broadcasts a stencil load to all clients (server-side, when hosting).
    ///
    /// # Arguments
    /// * `stencil_id` - Unique ID for the stencil
    /// * `name` - Stencil name
    /// * `stencil_data` - Compressed StencilFile bytes
    pub fn broadcast_stencil_loaded(
        &mut self,
        stencil_id: u64,
        name: String,
        stencil_data: Vec<u8>,
    ) {
        if let Some(ref mut server) = self.server {
            server.broadcast_stencil_loaded(stencil_id, name, stencil_data);
        }
    }

    /// Broadcasts a stencil transform update to all clients (server-side, when hosting).
    pub fn broadcast_stencil_transform(
        &mut self,
        stencil_id: u64,
        position: [i32; 3],
        rotation: u8,
    ) {
        if let Some(ref mut server) = self.server {
            server.broadcast_stencil_transform(stencil_id, position, rotation);
        }
    }

    /// Broadcasts a stencil removal to all clients (server-side, when hosting).
    pub fn broadcast_stencil_removed(&mut self, stencil_id: u64) {
        if let Some(ref mut server) = self.server {
            server.broadcast_stencil_removed(stencil_id);
        }
    }

    // ========================================================================
    // Template Sync Methods
    // ========================================================================

    /// Broadcasts a template load to all clients (server-side, when hosting).
    ///
    /// # Arguments
    /// * `template_id` - Unique ID for the template
    /// * `name` - Template name
    /// * `template_data` - Compressed VxtFile bytes
    pub fn broadcast_template_loaded(
        &mut self,
        template_id: u64,
        name: String,
        template_data: Vec<u8>,
    ) {
        if let Some(ref mut server) = self.server {
            server.broadcast_template_loaded(template_id, name, template_data);
        }
    }

    /// Broadcasts a template removal to all clients (server-side, when hosting).
    pub fn broadcast_template_removed(&mut self, template_id: u64) {
        if let Some(ref mut server) = self.server {
            server.broadcast_template_removed(template_id);
        }
    }

    /// Takes all pending stencil loads and clears the queue.
    /// Call this from the game loop to apply stencil loads to the local stencil manager.
    pub fn take_pending_stencil_loads(&mut self) -> Vec<crate::net::protocol::StencilLoaded> {
        Self::drain_variant(&mut self.events, |e| match e {
            NetworkEvent::StencilLoaded(stencil) => Some(stencil.clone()),
            _ => None,
        })
    }

    /// Returns true if there are pending stencil loads to apply.
    pub fn has_pending_stencil_loads(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, NetworkEvent::StencilLoaded(_)))
    }

    /// Takes all pending stencil transform updates and clears the queue.
    pub fn take_pending_stencil_transforms(
        &mut self,
    ) -> Vec<crate::net::protocol::StencilTransformUpdate> {
        Self::drain_variant(&mut self.events, |e| match e {
            NetworkEvent::StencilTransformUpdated(t) => Some(t.clone()),
            _ => None,
        })
    }

    /// Returns true if there are pending stencil transform updates to apply.
    pub fn has_pending_stencil_transforms(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, NetworkEvent::StencilTransformUpdated(_)))
    }

    /// Takes all pending stencil removals and clears the queue.
    pub fn take_pending_stencil_removals(&mut self) -> Vec<crate::net::protocol::StencilRemoved> {
        Self::drain_variant(&mut self.events, |e| match e {
            NetworkEvent::StencilRemoved(removed) => Some(removed.clone()),
            _ => None,
        })
    }

    /// Returns true if there are pending stencil removals to apply.
    pub fn has_pending_stencil_removals(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, NetworkEvent::StencilRemoved(_)))
    }

    /// Takes all pending template loads and clears the queue.
    pub fn take_pending_template_loads(&mut self) -> Vec<crate::net::protocol::TemplateLoaded> {
        Self::drain_variant(&mut self.events, |e| match e {
            NetworkEvent::TemplateLoaded(template) => Some(template.clone()),
            _ => None,
        })
    }

    /// Returns true if there are pending template loads to apply.
    pub fn has_pending_template_loads(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, NetworkEvent::TemplateLoaded(_)))
    }

    /// Takes all pending template removals and clears the queue.
    pub fn take_pending_template_removals(&mut self) -> Vec<crate::net::protocol::TemplateRemoved> {
        Self::drain_variant(&mut self.events, |e| match e {
            NetworkEvent::TemplateRemoved(removed) => Some(removed.clone()),
            _ => None,
        })
    }

    /// Returns true if there are pending template removals to apply.
    pub fn has_pending_template_removals(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, NetworkEvent::TemplateRemoved(_)))
    }

    // ========================================================================
    // Chat and Name Sync Methods
    // ========================================================================

    /// Takes all pending player name changes and clears the queue.
    pub fn take_pending_player_name_changes(
        &mut self,
    ) -> Vec<crate::net::protocol::PlayerNameChanged> {
        Self::drain_variant(&mut self.events, |e| match e {
            NetworkEvent::PlayerNameChanged(change) => Some(change.clone()),
            _ => None,
        })
    }

    /// Returns true if there are pending player name changes.
    pub fn has_pending_player_name_changes(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, NetworkEvent::PlayerNameChanged(_)))
    }

    /// Takes all pending chat messages and clears the queue.
    pub fn take_pending_chat_messages(&mut self) -> Vec<crate::net::protocol::ChatReceived> {
        Self::drain_variant(&mut self.events, |e| match e {
            NetworkEvent::ChatReceived(chat) => Some(chat.clone()),
            _ => None,
        })
    }

    /// Returns true if there are pending chat messages.
    pub fn has_pending_chat_messages(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, NetworkEvent::ChatReceived(_)))
    }

    /// Broadcasts a player name change to all clients (server-side, when hosting).
    pub fn broadcast_player_name_changed(
        &mut self,
        player_id: u64,
        old_name: String,
        new_name: String,
    ) {
        if let Some(ref mut server) = self.server {
            server.broadcast_player_name_changed(player_id, old_name, new_name);
        }
    }

    /// Broadcasts a chat message to all clients (server-side, when hosting).
    pub fn broadcast_chat(&mut self, player_id: u64, player_name: String, message: String) {
        if let Some(ref mut server) = self.server {
            server.broadcast_chat(player_id, player_name, message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::BlockType;
    use crate::net::protocol::{BlockData, BulkOperation};

    #[test]
    fn test_materialize_fill_enumerates_inclusive_range() {
        let mut q = VecDeque::new();
        let op = BulkOperation::Fill {
            start: [0, 0, 0],
            end: [1, 1, 1],
            block: BlockData::from(BlockType::Stone),
        };
        let n = MultiplayerState::materialize_bulk_op(&op, &mut q);
        assert_eq!(n, 8, "2×2×2 fill must enqueue 8 positions");
        assert_eq!(q.len(), 8);
    }

    #[test]
    fn test_materialize_fill_handles_reversed_endpoints() {
        let mut q = VecDeque::new();
        let op = BulkOperation::Fill {
            start: [3, 3, 3],
            end: [0, 0, 0],
            block: BlockData::from(BlockType::Dirt),
        };
        let n = MultiplayerState::materialize_bulk_op(&op, &mut q);
        assert_eq!(n, 64, "4×4×4 fill regardless of endpoint order");
    }

    #[test]
    fn test_take_bulk_block_batch_respects_budget() {
        let mut mp = MultiplayerState::new();
        let op = BulkOperation::Fill {
            start: [0, 0, 0],
            end: [4, 4, 4],
            block: BlockData::from(BlockType::Stone),
        };
        let queued = MultiplayerState::materialize_bulk_op(&op, &mut mp.pending_bulk_blocks);
        assert_eq!(queued, 125);
        assert_eq!(mp.pending_bulk_depth(), 125);

        let first = mp.take_bulk_block_batch(50);
        assert_eq!(first.len(), 50);
        assert_eq!(mp.pending_bulk_depth(), 75);

        let rest = mp.take_bulk_block_batch(1000);
        assert_eq!(rest.len(), 75);
        assert_eq!(mp.pending_bulk_depth(), 0);
    }

    #[test]
    fn test_materialize_template_not_supported_yet() {
        let mut q = VecDeque::new();
        let op = BulkOperation::Template {
            position: [0, 0, 0],
            template_name: "test".into(),
            rotation: 0,
        };
        // Not yet materialized server-side — must enqueue nothing.
        let n = MultiplayerState::materialize_bulk_op(&op, &mut q);
        assert_eq!(n, 0);
        assert!(q.is_empty());
    }
}

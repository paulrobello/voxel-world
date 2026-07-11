//! Multiplayer state management.
//!
//! Handles the game mode (single-player, host, client), server/client instances,
//! and player synchronization.

// Networking integration is incomplete — some fields/methods are prepared for future use.

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use crate::chunk::Chunk;
use crate::config::GameMode;
use crate::net::{
    BlockSyncManager, ChunkSyncManager, CustomTextureCache, DiscoveredServer, DiscoveryResponder,
    GameClient, GameServer, RemotePlayer, SerializedChunk,
};
#[cfg(feature = "threaded-server")]
use crate::net::{ServerCommand, ServerThread, ServerThreadEvent};
use nalgebra::Vector3;

mod chat;
pub use chat::{ChatEntry, ChatState};
mod roster;
pub use roster::PlayerRoster;
mod discovery;
pub use discovery::DiscoveryState;
mod input;
pub use input::InputState;
mod texture;
pub use texture::TextureState;
mod sync;
pub use sync::SyncState;
mod pending;
pub use pending::PendingState;
mod session;
pub use session::SessionMetadata;

/// Whether to use threaded server mode (experimental).
/// When enabled, server network processing runs in a dedicated thread.
/// Gate behind the `threaded-server` feature; disabled by default.
#[cfg(feature = "threaded-server")]
const USE_THREADED_SERVER: bool = true;
#[cfg(not(feature = "threaded-server"))]
#[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
const USE_THREADED_SERVER: bool = false;

/// Target send rate for periodic network updates (client input, server
/// player-state broadcasts). Was originally "every 3 frames at 60 FPS";
/// expressed here as a wall-clock interval so the send rate no longer
/// couples to render FPS (PHY-M05). 50 ms ≈ 20 Hz, matching the cadence
/// assumed by `send_input`'s `FORCE_SEND_EVERY` keep-alive math.
const NETWORK_TICK_INTERVAL: Duration = Duration::from_millis(50);

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
    /// Full model-registry sync from the server (client-side, joining clients).
    /// Boxed — the compressed payloads are large. Server-authoritative IDs are
    /// applied via `ModelRegistry::register_at` so the client matches the host.
    ModelRegistrySync(Box<crate::net::protocol::ModelRegistrySync>),
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
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
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
    /// Client input-send + prediction state (ARC-002: extracted to `InputState`).
    pub input: InputState,
    /// Remote players + host-side roster (ARC-002: extracted to `PlayerRoster`).
    pub roster: PlayerRoster,
    /// Chunk sync manager.
    pub chunk_sync: ChunkSyncManager,
    /// Block sync manager for block changes.
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
    pub block_sync: BlockSyncManager,
    /// Block validator for server-side validation (anti-cheat).
    block_validator: crate::net::block_sync::BlockValidator,
    /// Wall-clock time of the last player-state broadcast, used to
    /// throttle `broadcast_player_states` independent of render FPS (PHY-M05).
    last_player_state_broadcast: Option<Instant>,
    /// Unified incoming event queue — replaces all individual `pending_*` `Vec<T>` fields.
    ///
    /// Events are pushed here when received from the network and consumed by the
    /// game-loop subsystem that owns each variant.  Typed helper methods
    /// (`take_pending_*`, `has_pending_*`) extract only the variants they care
    /// about, preserving the ordering within each logical category.
    events: VecDeque<NetworkEvent>,
    /// Non-queued pending network state (ARC-002: extracted to `PendingState`).
    pub pending: PendingState,
    /// Client custom-texture cache + GPU-init flag (ARC-002: extracted to `TextureState`).
    pub textures: TextureState,
    /// Server-side sync bandwidth state (ARC-002: extracted to `SyncState`).
    pub sync: SyncState,

    // LAN Discovery
    /// LAN discovery (client scan + server responder) (ARC-002: extracted to `DiscoveryState`).
    pub discovery: DiscoveryState,
    /// Session/server metadata (ARC-002: extracted to `SessionMetadata`).
    pub metadata: SessionMetadata,
    /// Server address (set when hosting or connected).
    pub server_address: Option<SocketAddr>,
    /// Pairing code (64-hex of the server's per-session private key) shown on
    /// the host so a remote client can authenticate via Secure mode. `Some`
    /// only while hosting; cleared on `stop_host`.
    pub host_pairing_code: Option<String>,
    /// Chat history + display-overlay state (ARC-002: extracted to `ChatState`).
    pub chat: ChatState,
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
            input: InputState::new(),
            roster: PlayerRoster::new(),
            chunk_sync: ChunkSyncManager::new(),
            block_sync: BlockSyncManager::new(false),
            block_validator: crate::net::block_sync::BlockValidator::new(),
            last_player_state_broadcast: None,
            events: VecDeque::new(),
            textures: TextureState::new(0),
            sync: SyncState::new(),
            discovery: DiscoveryState::new(),
            pending: PendingState::new(),
            server_address: None,
            host_pairing_code: None,
            metadata: SessionMetadata::new(),
            chat: ChatState::new(),
        }
    }

    /// Starts hosting a server with the given configuration.
    pub fn start_host(
        &mut self,
        server_name: String,
        port: u16,
        world_seed: u32,
        world_gen: u8,
        pairing_code: Option<&str>,
    ) -> Result<(), String> {
        let addr: SocketAddr = ([0, 0, 0, 0], port).into();
        log::debug!(
            "[Multiplayer] Starting host on {} with seed {}",
            addr,
            world_seed
        );

        // Resolve an optional pinned pairing code (--pairing-code) to a key.
        // When provided, the server uses this exact key so clients passing the
        // same code authenticate (e.g. the shared `make run-host`/`run-client`
        // fixture). Omitted → fresh random per-session key (secure default).
        // Invalid → log + fall back to random rather than abort host startup.
        let pinned_key: Option<[u8; 32]> = match pairing_code {
            Some(code) => match crate::net::auth::pairing_code_to_key(code) {
                Ok(key) => Some(key),
                Err(e) => {
                    log::warn!(
                        "[Multiplayer] Ignoring invalid --pairing-code ({}); \
                         falling back to a random per-session key",
                        e
                    );
                    None
                }
            },
            None => None,
        };

        #[cfg(feature = "threaded-server")]
        if self.use_threaded_server {
            // NOTE: the experimental threaded-server path (QA-006, off by
            // default) still uses a random key — the pinned --pairing-code is
            // ignored here. The default non-threaded path below honors it.
            self.server_thread = Some(ServerThread::spawn(addr, world_seed, world_gen)?);
            log::debug!("[Multiplayer] Server thread spawned");
        } else {
            self.server = Some(match pinned_key {
                Some(key) => GameServer::new_with_key(addr, key, world_seed, world_gen),
                None => GameServer::new(addr, world_seed, world_gen),
            }?);
            log::debug!("[Multiplayer] Direct server created");
        }
        #[cfg(not(feature = "threaded-server"))]
        {
            self.server = Some(match pinned_key {
                Some(key) => GameServer::new_with_key(addr, key, world_seed, world_gen),
                None => GameServer::new(addr, world_seed, world_gen),
            }?);
            log::debug!("[Multiplayer] Direct server created");
        }

        self.mode = GameMode::Host;
        *self.metadata.server_name_mut() = server_name.clone();
        self.server_address = Some(addr);

        // Start discovery responder for LAN advertising
        match DiscoveryResponder::new(server_name, port, self.metadata.max_players()) {
            Ok(responder) => {
                self.discovery.set_responder(responder);
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
            // Surface the pairing code so the host can share it out-of-band
            // with a remote client. The key is the same one the loopback
            // client uses; the hex form is what a remote player will type in.
            let code = crate::net::auth::key_to_pairing_code(&key);
            log::info!(
                "[Multiplayer] Hosting on port {} — pairing code: {}",
                port,
                code
            );
            self.host_pairing_code = Some(code);
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
        self.discovery.clear_responder();
        self.server_address = None;
        self.host_pairing_code = None;
        self.metadata.server_name_mut().clear();
        self.roster.reset_to_host();

        if self.mode == GameMode::Host {
            self.mode = GameMode::SinglePlayer;
        }
    }

    /// Connects to a remote server using a 64-hex pairing code that matches
    /// the host's per-session private key. The code is decoded locally and
    /// fed into `GameClient::with_key` — the same Secure-mode path the host's
    /// loopback client uses. An empty or malformed code returns an error and
    /// does NOT fall back to unsecured transport.
    pub fn connect(&mut self, address: &str, pairing_code: &str) -> Result<(), String> {
        let addr: SocketAddr = address
            .parse()
            .map_err(|e| format!("Invalid address '{}': {}", address, e))?;

        let trimmed = pairing_code.trim();
        if trimmed.is_empty() {
            return Err(
                "Pairing code is required — ask the host for the 64-hex code shown on their screen."
                    .to_string(),
            );
        }

        let key = crate::net::auth::pairing_code_to_key(trimmed)
            .map_err(|e| format!("Invalid pairing code: {}", e))?;

        log::debug!("[Multiplayer] Connecting to {} with pairing code...", addr);
        self.client = Some(GameClient::with_key(addr, key)?);
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
        self.metadata.set_ping_ms(None);

        if self.mode == GameMode::Client {
            self.mode = GameMode::SinglePlayer;
        }
    }

    /// Starts LAN discovery to find servers.
    pub fn start_discovery(&mut self) -> Result<(), String> {
        self.discovery.start_client()
    }

    /// Stops LAN discovery.
    pub fn stop_discovery(&mut self) {
        self.discovery.stop_client();
    }

    /// Returns discovered servers from LAN discovery.
    pub fn get_discovered_servers(&self) -> Vec<DiscoveredServer> {
        self.discovery.discovered_servers()
    }

    /// Returns the current player count.
    pub fn get_player_count(&self) -> u8 {
        self.roster.player_count()
    }

    /// Returns the maximum player count.
    pub fn get_max_players(&self) -> u8 {
        self.metadata.max_players()
    }

    /// Returns the list of player names.
    pub fn get_player_names(&self) -> &[String] {
        self.roster.player_names()
    }

    /// Returns remote player markers for minimap display.
    /// Each marker includes position (x, z) and player_id for color assignment.
    /// The local player is NOT included in this list.
    pub fn get_minimap_markers(&self) -> Vec<crate::ui::minimap::RemotePlayerMarker> {
        self.roster
            .remote_players()
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
        // Exclude our own player so the client never renders its own body
        // (previously the local client's roster entry — stuck at its spawn
        // position — was rendered as a "ghost self").
        let local = self.local_player_id();
        self.roster
            .remote_players()
            .iter()
            .filter(|player| Some(player.player_id) != local)
            .map(|player| (player.position, player.player_id))
            .collect()
    }

    /// Returns remote player data for 3D name label rendering.
    /// Each tuple contains (name, position [x, y, z], color_index).
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
    pub fn get_remote_players_for_labels(&self) -> Vec<(String, [f32; 3], usize)> {
        self.roster
            .remote_players()
            .iter()
            .enumerate()
            .map(|(idx, player)| (player.name.clone(), player.position, idx))
            .collect()
    }

    /// Returns remote player labels for HUD rendering.
    pub fn get_remote_player_labels(&self) -> Vec<crate::ui::minimap::RemotePlayerLabel> {
        self.roster
            .remote_players()
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
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
    pub fn get_server_name(&self) -> &str {
        self.metadata.server_name()
    }

    /// Returns the server address (if hosting or connected).
    pub fn get_server_address(&self) -> Option<SocketAddr> {
        self.server_address
    }

    /// Returns the last known ping.
    pub fn get_ping_ms(&self) -> Option<u32> {
        self.metadata.ping_ms()
    }

    /// Sets the local player's display name.
    pub fn set_local_player_name(&mut self, name: String) {
        self.metadata.set_local_player_name(name);
    }

    /// Returns the local player's display name.
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
    pub fn get_local_player_name(&self) -> &str {
        self.metadata.local_player_name()
    }

    /// Adds a chat message to history.
    pub fn add_chat_message(&mut self, player_name: String, message: String) {
        self.chat.add_message(player_name, message);
    }

    /// Updates the chat display timer (call every frame with delta_time).
    pub fn update_chat_timer(&mut self, delta_time: f32) {
        self.chat.update_timer(delta_time);
    }

    /// Returns whether the chat overlay should be visible.
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
    pub fn is_chat_visible(&self) -> bool {
        self.chat.is_visible()
    }

    /// Returns the chat history for display.
    pub fn get_chat_history(&self) -> &[ChatEntry] {
        self.chat.history()
    }

    /// Returns the chat display timer remaining (if any).
    pub fn get_chat_display_timer(&self) -> Option<f32> {
        self.chat.display_timer()
    }

    /// Updates the multiplayer state (call every frame).
    pub fn update(&mut self, duration: Duration) {
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

        // Broadcast player states periodically (20 Hz wall-clock; was every 3
        // frames at 60 FPS — PHY-M05). The time gate covers both the direct
        // server and the threaded-server path so the send rate is FPS-independent
        // in either configuration. First call after construction always fires.
        let now = Instant::now();
        let broadcast_elapsed = self
            .last_player_state_broadcast
            .map(|t| now.duration_since(t))
            .unwrap_or(Duration::MAX);
        if broadcast_elapsed >= NETWORK_TICK_INTERVAL {
            self.last_player_state_broadcast = Some(now);
            if let Some(ref mut server) = self.server {
                server.broadcast_player_states();
            }
            #[cfg(feature = "threaded-server")]
            if let Some(ref server_thread) = self.server_thread {
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

        // Update discovery (server responder + client scan)
        self.discovery.update_responder(self.roster.player_count());
        self.discovery.update_client();

        // Update player count based on remote players + host
        if self.mode == GameMode::Host {
            self.roster.sync_count();
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
            ClientMessage::BlocksChanged(msg) => {
                // Bulk shape-edit sync (client-authoritative, mirroring
                // PlaceBlock). The originator has already applied the writes
                // locally; the server validates + relays to other clients.
                log::debug!(
                    "[Server] Received BlocksChanged with {} entries from client {}",
                    msg.changes.len(),
                    client_id
                );

                // Anti-cheat: bulk edits legitimately exceed the per-block
                // rate limit, so use the reach-only check (any block within
                // expanded build reach of the sender). Reject only batches
                // that target entirely far-away coords.
                let reach_ok = if let Some(ref server) = self.server {
                    if let Some(player_info) = server.get_player(client_id) {
                        let positions: Vec<[i32; 3]> =
                            msg.changes.iter().map(|(p, _)| *p).collect();
                        self.block_validator
                            .validate_bulk_reach(player_info.position, &positions, 3.0)
                            .is_ok()
                    } else {
                        false
                    }
                } else {
                    // Threaded-server mode: skip (mirrors the PlaceBlock TODO).
                    true
                };

                if reach_ok {
                    // Threaded-server path borrows `msg.changes` (immutably) and must
                    // run before the inline-server path moves `msg` into
                    // `broadcast_block_changes_except`. The two branches are mutually
                    // exclusive (`self.server` vs `self.server_thread`), so reordering
                    // is invisible at runtime — it only satisfies the borrow checker
                    // under `--features threaded-server`.
                    #[cfg(feature = "threaded-server")]
                    if let Some(ref server_thread) = self.server_thread {
                        // No bulk-broadcast command exists for the threaded
                        // server path; fan out per-entry so the edit still
                        // reaches remote clients. Host edits are the common
                        // case and tolerate the per-message cost.
                        for (pos, block) in &msg.changes {
                            let _ =
                                server_thread.send_command(ServerCommand::BroadcastBlockChange(
                                    crate::net::protocol::BlockChanged {
                                        position: *pos,
                                        block: block.clone(),
                                    },
                                ));
                        }
                    }
                    if let Some(ref mut server) = self.server {
                        server.broadcast_block_changes_except(msg, client_id);
                        log::debug!(
                            "[Server] Broadcasted BlocksChanged to all clients except originator"
                        );
                    }
                } else {
                    log::warn!(
                        "[Server] BlocksChanged from client {} rejected: out of reach",
                        client_id
                    );
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
                        .roster
                        .remote_players_mut()
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
                // SEC-M01: reach check. Reject bulk ops whose entire region
                // is far from the sender. Mirrors NET-001's bulk-edit reach
                // policy: 3× single-block reach, accepted if any one corner
                // of the region is in range.
                let reach_ok = if let Some(ref server) = self.server {
                    if let Some(player_info) = server.get_player(client_id) {
                        let reach = self.block_validator.max_placement_distance() * 3.0;
                        op.validate_reach(player_info.position, reach).is_ok()
                    } else {
                        false
                    }
                } else {
                    // Threaded-server mode: skip (mirrors the PlaceBlock TODO).
                    true
                };
                if !reach_ok {
                    log::warn!(
                        "[Server] Rejected BulkOperation from client {}: \
                         region entirely outside build reach",
                        client_id
                    );
                    return;
                }
                // Materialize the operation into concrete (pos, block) pairs.
                // Fill / Replace produce ≤ MAX_BULK_FILL_VOLUME entries;
                // Template is not yet implemented server-side because the
                // host doesn't have a template registry wired up here.
                let queued = Self::materialize_bulk_op(&op, self.pending.bulk_blocks_mut());
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
                self.pending
                    .set_server_seed((accepted.world_seed, accepted.world_gen));
                self.textures.on_connect(accepted.custom_texture_count);
            }
            ServerMessage::PlayerState(state) => {
                // Reconcile with server
                self.input.prediction_mut().reconcile(state);

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
                            .roster
                            .remote_players_mut()
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
                            self.roster.add(remote);
                        }
                    }
                }
            }
            ServerMessage::PlayerJoined(joined)
                if !self
                    .roster
                    .remote_players()
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
                self.roster.add(remote);
            }
            ServerMessage::PlayerLeft(left) => {
                self.roster.remove(left.player_id);
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
                log::debug!(
                    "[Client] Received ModelRegistrySync ({} model bytes, {} door-pair bytes)",
                    sync.models_data.len(),
                    sync.door_pairs_data.len()
                );
                self.events
                    .push_back(NetworkEvent::ModelRegistrySync(sync.clone()));
            }
            ServerMessage::TextureData(tex) => {
                log::debug!("[Client] Received texture for slot {}", tex.slot);
                self.textures
                    .cache_mut()
                    .store_texture(tex.slot, tex.data.clone());
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
                self.pending.set_day_cycle_pause(pause.clone());
            }
            ServerMessage::TimeUpdate(time) => {
                log::debug!("[Client] Received TimeUpdate: {:.3}", time.time_of_day);
                self.pending.set_time_update(time.time_of_day);
            }
            ServerMessage::SpawnPositionChanged(spawn) => {
                log::debug!(
                    "[Client] Received SpawnPositionChanged: ({:.1}, {:.1}, {:.1})",
                    spawn.position[0],
                    spawn.position[1],
                    spawn.position[2]
                );
                self.pending.set_spawn_position(spawn.clone());
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
                    .roster
                    .remote_players_mut()
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

    /// Returns true when the network tick interval has elapsed since the
    /// last client→server input send (and records the time). Gates
    /// `send_input` so the send rate is independent of render FPS (PHY-M05).
    /// The first call always returns true. Does not panic on clock issues.
    pub fn should_send_input_tick(&mut self) -> bool {
        self.input.should_send_tick()
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
        if let Some(ref mut client) = self.client {
            self.input
                .send_input(client, position, velocity, yaw, pitch, actions);
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

    /// Sends a batch of resolved block writes (e.g. from a shape tool) to the
    /// server. Mirrors `send_place_block`; the server relays to other clients
    /// originator-excluded so the local world is not double-applied.
    pub fn send_blocks_changed(&mut self, changes: crate::net::protocol::BlocksChanged) {
        if let Some(ref mut client) = self.client {
            client.send_blocks_changed(changes);
            // Flush immediately so the batch lands in the next server tick.
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
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
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
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
    pub fn world_seed(&self) -> Option<u32> {
        self.client.as_ref().and_then(|c| c.world_seed())
    }

    /// Updates remote player interpolation.
    pub fn update_remote_players(&mut self) {
        self.roster.interpolate();
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
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
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
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
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
        self.pending.take_bulk_batch(budget)
    }

    /// Returns the current pending-bulk queue depth. Useful for the debug HUD.
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
    pub fn pending_bulk_depth(&self) -> usize {
        self.pending.bulk_depth()
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

    /// Takes all pending full model-registry syncs received from the server.
    /// Call this from the game loop on joining clients before
    /// `take_pending_models` so the bulk registry is seeded first.
    pub fn take_pending_model_registry_sync(
        &mut self,
    ) -> Vec<crate::net::protocol::ModelRegistrySync> {
        Self::drain_variant(&mut self.events, |e| match e {
            NetworkEvent::ModelRegistrySync(sync) => Some((**sync).clone()),
            _ => None,
        })
    }

    /// Returns true if there is a pending model-registry sync to apply.
    pub fn has_pending_model_registry_sync(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, NetworkEvent::ModelRegistrySync(_)))
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
        self.pending.take_server_seed()
    }

    /// Returns true if there's a pending server seed to apply.
    pub fn has_pending_server_seed(&self) -> bool {
        self.pending.has_server_seed()
    }

    /// Sends chunk data to a specific client (server-side, when hosting).
    /// The game loop calls this after retrieving chunk data from the world.
    ///
    /// Always sends authoritative full chunk data. The "generate locally from
    /// seed" optimization for unmodified chunks is disabled — the client's
    /// on-demand streaming gen cropped cross-boundary tree canopies relative
    /// to the host's startup bulk gen (trees appeared sliced at 32-block chunk
    /// edges on the client only). See the note in the body.
    pub fn send_chunk_to_client(&mut self, client_id: u64, position: [i32; 3], chunk: &Chunk) {
        // Epoch-aware dedup: skip sending if this exact epoch was already sent
        // to this client within the dedup window.
        let epoch = chunk.mutation_epoch();
        if let Some(ref server) = self.server
            && !server.should_send_chunk_with_epoch(client_id, position, epoch)
        {
            return;
        }

        // NOTE: the "generate locally from seed" optimization for unmodified
        // chunks is intentionally disabled (see the doc comment). Always
        // serialize + send authoritative full chunk data so the client's world
        // is byte-identical to the host's (full cross-boundary trees). The
        // per-position compression cache below bounds the CPU/bandwidth cost.

        // Serialize and send full data. Consult
        // the per-position compression cache first: if its mutation_epoch
        // matches the chunk's current epoch, reuse the bytes instead of
        // re-running LZ4.
        let cur_epoch = chunk.mutation_epoch();
        let compressed_opt: Option<Vec<u8>> =
            match self.sync.chunk_compression_cache().get(&position) {
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
                self.sync
                    .chunk_compression_cache_mut()
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
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
    pub fn request_texture_if_needed(&mut self, slot: u8) {
        if self.textures.cache_mut().request_if_needed(slot)
            && let Some(ref mut client) = self.client
        {
            client.send_texture_request(slot);
        }
    }

    /// Returns the texture cache for rendering.
    pub fn texture_cache(&self) -> &CustomTextureCache {
        self.textures.cache()
    }

    /// Returns a mutable reference to the texture cache for GPU uploads.
    pub fn texture_cache_mut(&mut self) -> &mut CustomTextureCache {
        self.textures.cache_mut()
    }

    /// Checks if GPU textures need initialization and returns the max slot count.
    pub fn take_pending_gpu_texture_init(&mut self) -> Option<u8> {
        self.textures.take_pending_gpu_init()
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
            .sync
            .water_optimizer_mut()
            .filter_significant_changes(updates);

        // Check rate limiting - only broadcast at appropriate intervals
        if !self.sync.water_optimizer_mut().should_broadcast_now() {
            // Accumulate changes for next broadcast window
            return;
        }

        // Collect player positions for AoI filtering
        let player_positions = self.get_all_player_positions();

        // Get filtered updates (AoI + rate limiting)
        let filtered_updates = if player_positions.is_empty() {
            // No players - use all pending (shouldn't happen in practice)
            self.sync.water_optimizer_mut().take_all_pending_updates()
        } else {
            self.sync
                .water_optimizer_mut()
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
        let _significant = self
            .sync
            .lava_optimizer_mut()
            .filter_significant_changes(updates);

        // Check rate limiting - only broadcast at appropriate intervals
        if !self.sync.lava_optimizer_mut().should_broadcast_now() {
            return;
        }

        // Collect player positions for AoI filtering
        let player_positions = self.get_all_player_positions();

        // Get filtered updates (AoI + rate limiting)
        let filtered_updates = if player_positions.is_empty() {
            self.sync.lava_optimizer_mut().take_all_pending_updates()
        } else {
            self.sync
                .lava_optimizer_mut()
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
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
    pub fn water_sync_stats(&self) -> &crate::net::water_sync::WaterSyncStats {
        self.sync.water_optimizer().stats()
    }

    /// Prunes distant cached water states to prevent memory growth.
    /// Call this periodically (e.g., every 30 seconds).
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
    pub fn prune_water_sync_cache(&mut self) {
        let player_positions = self.get_all_player_positions();
        self.sync
            .water_optimizer_mut()
            .prune_distant_states(&player_positions);
    }

    /// Returns lava sync optimizer statistics for debugging.
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
    pub fn lava_sync_stats(&self) -> &crate::net::lava_sync::LavaSyncStats {
        self.sync.lava_optimizer().stats()
    }

    /// Prunes distant cached lava states to prevent memory growth.
    /// Call this periodically (e.g., every 30 seconds).
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
    pub fn prune_lava_sync_cache(&mut self) {
        let player_positions = self.get_all_player_positions();
        self.sync
            .lava_optimizer_mut()
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
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
    pub fn broadcast_tree_fell(
        &mut self,
        blocks: Vec<(nalgebra::Vector3<i32>, crate::chunk::BlockType)>,
    ) -> Vec<u32> {
        // Use the persistent tree_fall_sync so entity IDs monotonically advance
        // across the whole session; retransmitted or back-to-back trees never
        // collide with each other's IDs. Large trees are split into multiple
        // TreeFell messages to stay under MTU.
        let msgs = self
            .sync
            .tree_fall_sync_mut()
            .build_tree_fell_batched(blocks);

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
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
    pub fn take_pending_tree_falls(&mut self) -> Vec<crate::net::protocol::TreeFell> {
        Self::drain_variant(&mut self.events, |e| match e {
            NetworkEvent::TreeFell(tree_fell) => Some(tree_fell.clone()),
            _ => None,
        })
    }

    /// Returns true if there are pending tree fall events to apply.
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
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
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
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
        self.pending.take_day_cycle_pause()
    }

    /// Returns true if there's a pending day cycle pause change.
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
    pub fn has_pending_day_cycle_pause(&self) -> bool {
        self.pending.has_day_cycle_pause()
    }

    /// Takes pending time of day update (client-side).
    /// Returns None if no pending update.
    pub fn take_pending_time_update(&mut self) -> Option<f32> {
        self.pending.take_time_update()
    }

    /// Returns true if there's a pending time update.
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
    pub fn has_pending_time_update(&self) -> bool {
        self.pending.has_time_update()
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
        self.pending.take_spawn_position()
    }

    /// Returns true if there's a pending spawn position update.
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
    pub fn has_pending_spawn_position(&self) -> bool {
        self.pending.has_spawn_position()
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
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
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
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
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
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
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
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
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
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
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
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
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
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
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
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
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
        let queued = MultiplayerState::materialize_bulk_op(&op, mp.pending.bulk_blocks_mut());
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

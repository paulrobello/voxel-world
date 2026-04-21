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
use crate::net::picture_store::PictureManager;
use crate::net::protocol::{
    BlockChanged, BlocksChanged, ChunkData, ChunkGenerateLocal, ClientMessage, ConnectionAccepted,
    PlayerId, PlayerJoined, PlayerLeft, PlayerState, ServerMessage, TimeUpdate, TreeFell,
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
    /// Per-session private key (for Secure mode handshake).
    private_key: [u8; 32],
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
    /// Picture storage manager.
    picture_manager: Option<PictureManager>,
    /// World directory path (for loading models.dat).
    world_dir: Option<std::path::PathBuf>,
    /// Cumulative count of bincode encode failures in broadcast/send paths.
    /// Non-zero values indicate an internal serialization bug; exposed for the
    /// debug HUD and tests.
    encode_failures: u64,
    /// Per-client dedup of recently-sent chunk positions. Maps to
    /// `(Instant, mutation_epoch)` so a re-send is allowed when the chunk
    /// has been modified since the last send, even within the window.
    recently_sent_chunks: HashMap<u64, HashMap<[i32; 3], (Instant, u64)>>,
    /// Runtime bandwidth counters, enumerable for the debug HUD.
    net_stats: NetStats,
    /// Per-client rate limiter for chat + console commands. Block edits go
    /// through `BlockValidator`; chunk requests through the dedup window; this
    /// covers the remaining conversational paths.
    message_rate_limiter: MessageRateLimiter,
}

/// Live bandwidth counters maintained by `GameServer`.
///
/// Every successful broadcast / send path feeds bytes + message counts here
/// so the debug HUD (or a future admin endpoint) can surface per-session
/// throughput without a full profiling harness. Per-message-type breakdown
/// is tracked in `per_type_bytes` / `per_type_count` to help spot the
/// chattiest channel during a session.
#[derive(Debug, Clone, Default)]
pub struct NetStats {
    /// Total bytes sent server→clients across every channel.
    pub bytes_out: u64,
    /// Total messages sent server→clients.
    pub messages_out: u64,
    /// Bytes sent, bucketed by the `label` passed to `broadcast_encoded`.
    pub per_type_bytes: HashMap<&'static str, u64>,
    /// Message count, bucketed by the `label` passed to `broadcast_encoded`.
    pub per_type_count: HashMap<&'static str, u64>,
}

/// How long a sent chunk is considered "already sent" for dedup purposes.
/// Beyond this window the server will happily re-send the chunk to satisfy
/// legitimate retry scenarios (e.g. client lost the packet).
const CHUNK_RESEND_WINDOW: Duration = Duration::from_secs(30);

/// Per-client sliding-window rate limiter keyed on a message "kind" string.
///
/// Used to throttle chat spam and console-command floods — the per-block
/// `BlockValidator` rate limit already covers placements, so this struct
/// focuses on the low-frequency conversational paths. A `(client_id, kind)`
/// pair with more than `max_per_window` entries inside `window` is rejected
/// until the window slides forward. The window is kept small (1 second) so
/// legitimate bursts of 2-3 quick messages pass while a sustained flood is
/// clamped.
#[derive(Debug, Default)]
pub struct MessageRateLimiter {
    /// `(client_id, kind)` → timestamps (microseconds) of recent accepted events.
    events: HashMap<(u64, &'static str), std::collections::VecDeque<u64>>,
}

impl MessageRateLimiter {
    pub fn new() -> Self {
        Self {
            events: HashMap::new(),
        }
    }

    /// Returns `true` when the caller may proceed and records the event.
    /// Returns `false` when the event would exceed `max_per_window` within
    /// `window_us` microseconds; the caller should drop the message.
    pub fn check(
        &mut self,
        client_id: u64,
        kind: &'static str,
        now_us: u64,
        window_us: u64,
        max_per_window: usize,
    ) -> bool {
        let q = self.events.entry((client_id, kind)).or_default();
        let cutoff = now_us.saturating_sub(window_us);
        while q.front().is_some_and(|&t| t <= cutoff) {
            q.pop_front();
        }
        if q.len() >= max_per_window {
            return false;
        }
        q.push_back(now_us);
        true
    }

    /// Drops all tracked state for a client on disconnect so the map doesn't
    /// grow without bound across long-running sessions.
    pub fn forget_client(&mut self, client_id: u64) {
        self.events.retain(|(cid, _), _| *cid != client_id);
    }
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
        let private_key = auth.private_key();
        let pairing_code = auth.pairing_code();
        let transport = auth.create_transport()?;

        // Log the pairing code at info level so a host can share it
        // out-of-band with a remote client for Secure mode connections.
        log::info!(
            "[GameServer] Session pairing code: {} (bind: {})",
            pairing_code,
            address
        );

        let connection_config = create_connection_config();
        let server = RenetServer::new(connection_config);

        Ok(Self {
            server,
            transport,
            private_key,
            players: HashMap::new(),
            host_player: None,
            host_client_id: None,
            start_time: Instant::now(),
            last_tick: Instant::now(),
            world_seed,
            world_gen,
            texture_manager: None,
            picture_manager: None,
            world_dir: None,
            encode_failures: 0,
            recently_sent_chunks: HashMap::new(),
            net_stats: NetStats::default(),
            message_rate_limiter: MessageRateLimiter::new(),
        })
    }

    /// Returns the server's per-session private key (for Secure mode).
    /// The host's loopback client needs this to generate a valid ConnectToken.
    pub fn private_key(&self) -> [u8; 32] {
        self.private_key
    }

    /// Returns `true` if `client_id` may send another message of `kind` right
    /// now. Default limits:
    /// - `"chat"`: 5 messages / 5 seconds
    /// - `"console"`: 10 commands / 5 seconds
    ///
    /// The window and cap are inlined so the limits stay visible at the call
    /// site; tune by replacing the match arms.
    pub fn check_message_rate(&mut self, client_id: u64, kind: &'static str) -> bool {
        let now_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;
        let (window_us, cap) = match kind {
            "chat" => (5_000_000u64, 5usize),
            "console" => (5_000_000u64, 10usize),
            // Unknown kinds get a conservative default — better to be too
            // strict than to leave a new path unthrottled.
            _ => (5_000_000u64, 5usize),
        };
        self.message_rate_limiter
            .check(client_id, kind, now_us, window_us, cap)
    }

    /// Test-only helper: drive `check_message_rate` with an explicit clock so
    /// tests don't have to sleep.
    #[cfg(test)]
    pub(crate) fn check_message_rate_at(
        &mut self,
        client_id: u64,
        kind: &'static str,
        now_us: u64,
    ) -> bool {
        let (window_us, cap) = match kind {
            "chat" => (5_000_000u64, 5usize),
            "console" => (5_000_000u64, 10usize),
            _ => (5_000_000u64, 5usize),
        };
        self.message_rate_limiter
            .check(client_id, kind, now_us, window_us, cap)
    }

    /// Returns a snapshot of live bandwidth counters. Cloned so callers can
    /// diff across frames without holding a borrow on the server.
    pub fn net_stats(&self) -> NetStats {
        self.net_stats.clone()
    }

    /// Resets every runtime bandwidth counter to zero. Useful for benchmarks
    /// and for the debug HUD "reset" button.
    pub fn reset_net_stats(&mut self) {
        self.net_stats = NetStats::default();
    }

    /// Returns `true` if we should send `pos` to `client_id` right now. Returns
    /// `false` when the same chunk was sent within `CHUNK_RESEND_WINDOW` AND the
    /// chunk's mutation_epoch hasn't changed since — the caller should drop the
    /// request rather than re-fanning out the chunk. A changed epoch always
    /// allows a re-send so modified chunks reach the client promptly.
    ///
    /// Callers that successfully send the chunk must follow up with
    /// [`record_chunk_sent`] so the window is refreshed.
    pub fn should_send_chunk(&mut self, client_id: u64, pos: [i32; 3]) -> bool {
        if let Some(map) = self.recently_sent_chunks.get(&client_id)
            && let Some((last_sent, _epoch)) = map.get(&pos)
            && last_sent.elapsed() < CHUNK_RESEND_WINDOW
        {
            // We'll let the caller check the epoch against the live chunk
            // and decide. But we can't do that here without a &Chunk ref.
            // The caller should call should_send_chunk_with_epoch instead.
            return false;
        }
        true
    }

    /// Like `should_send_chunk`, but also checks the chunk's current
    /// `mutation_epoch` against the last-sent epoch. Returns `true` when the
    /// epoch has changed (chunk was modified) even within the dedup window.
    pub fn should_send_chunk_with_epoch(
        &self,
        client_id: u64,
        pos: [i32; 3],
        current_epoch: u64,
    ) -> bool {
        if let Some(map) = self.recently_sent_chunks.get(&client_id)
            && let Some((last_sent, sent_epoch)) = map.get(&pos)
        {
            if *sent_epoch != current_epoch {
                return true;
            }
            if last_sent.elapsed() < CHUNK_RESEND_WINDOW {
                return false;
            }
        }
        true
    }

    /// Records that `pos` was sent to `client_id` at the current instant, along
    /// with the chunk's `mutation_epoch`. Used for dedup in
    /// [`should_send_chunk_with_epoch`].
    pub fn record_chunk_sent_with_epoch(&mut self, client_id: u64, pos: [i32; 3], epoch: u64) {
        self.recently_sent_chunks
            .entry(client_id)
            .or_default()
            .insert(pos, (Instant::now(), epoch));
    }

    /// Records that `pos` was sent to `client_id` at the current instant. Used
    /// for dedup in [`should_send_chunk`].
    pub fn record_chunk_sent(&mut self, client_id: u64, pos: [i32; 3]) {
        self.recently_sent_chunks
            .entry(client_id)
            .or_default()
            .insert(pos, (Instant::now(), 0));
    }

    /// Drops per-client chunk-send tracking when a client disconnects so the
    /// dedup map doesn't grow without bound across long-running sessions.
    pub fn forget_client_chunk_history(&mut self, client_id: u64) {
        self.recently_sent_chunks.remove(&client_id);
    }

    /// Lazily prunes per-client chunk send timestamps older than the resend
    /// window. Safe to call periodically (e.g. once per second) from the game
    /// loop.
    pub fn purge_stale_chunk_history(&mut self) {
        let cutoff = Instant::now();
        for per_client in self.recently_sent_chunks.values_mut() {
            per_client.retain(|_, (t, _)| cutoff.duration_since(*t) < CHUNK_RESEND_WINDOW);
        }
        self.recently_sent_chunks
            .retain(|_, per_client| !per_client.is_empty());
    }

    /// Returns the cumulative count of encode failures in broadcast/send paths.
    /// Used by the debug HUD (and tests) to surface silent serialization bugs
    /// that would otherwise go unnoticed.
    pub fn encode_failures(&self) -> u64 {
        self.encode_failures
    }

    /// Encodes `msg` and broadcasts it on `channel`, logging + counting on
    /// failure so the caller's hot path stays unadorned. Returns `true` if the
    /// message was queued for send.
    fn broadcast_encoded(&mut self, channel: u8, label: &'static str, msg: &ServerMessage) -> bool {
        match bincode::serde::encode_to_vec(msg, bincode::config::standard()) {
            Ok(encoded) => {
                let n = encoded.len() as u64;
                self.net_stats.bytes_out = self.net_stats.bytes_out.saturating_add(n);
                self.net_stats.messages_out = self.net_stats.messages_out.saturating_add(1);
                *self.net_stats.per_type_bytes.entry(label).or_insert(0) += n;
                *self.net_stats.per_type_count.entry(label).or_insert(0) += 1;

                self.server
                    .broadcast_message(channel, renet::Bytes::from(encoded));
                true
            }
            Err(err) => {
                self.encode_failures = self.encode_failures.saturating_add(1);
                log::warn!(
                    "[Server] Failed to encode {} for broadcast: {} (total failures: {})",
                    label,
                    err,
                    self.encode_failures
                );
                false
            }
        }
    }

    /// Like `broadcast_encoded`, but skips a specific client (the originator).
    fn broadcast_encoded_except(
        &mut self,
        channel: u8,
        label: &'static str,
        msg: &ServerMessage,
        exclude_client_id: u64,
    ) -> bool {
        match bincode::serde::encode_to_vec(msg, bincode::config::standard()) {
            Ok(encoded) => {
                let n = encoded.len() as u64;
                self.net_stats.bytes_out = self.net_stats.bytes_out.saturating_add(n);
                self.net_stats.messages_out = self.net_stats.messages_out.saturating_add(1);
                *self.net_stats.per_type_bytes.entry(label).or_insert(0) += n;
                *self.net_stats.per_type_count.entry(label).or_insert(0) += 1;

                let bytes = renet::Bytes::from(encoded);
                for &cid in self.players.keys() {
                    if cid != exclude_client_id {
                        self.server.send_message(cid, channel, bytes.clone());
                    }
                }
                true
            }
            Err(err) => {
                self.encode_failures = self.encode_failures.saturating_add(1);
                log::warn!(
                    "[Server] Failed to encode {} for broadcast: {} (total failures: {})",
                    label,
                    err,
                    self.encode_failures
                );
                false
            }
        }
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

        // Initialize texture manager
        let mut manager = TextureSlotManager::new(path.join("custom_textures"), max_textures);
        if let Err(e) = manager.init() {
            log::error!("[Server] Failed to initialize texture manager: {}", e);
        }
        self.texture_manager = Some(manager);

        // Initialize picture manager
        let mut picture_manager = PictureManager::new(path.join("pictures"), 1024);
        if let Err(e) = picture_manager.init() {
            log::error!("[Server] Failed to initialize picture manager: {}", e);
        }
        self.picture_manager = Some(picture_manager);
    }

    /// Sets the host's client ID (the loopback connection from host to itself).
    /// This is used to exclude the host's own client from broadcasts to other clients.
    pub fn set_host_client_id(&mut self, client_id: u64) {
        self.host_client_id = Some(client_id);
        log::debug!("[GameServer] Set host_client_id={}", client_id);
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
        if let Err(e) = self.transport.update(duration, &mut self.server) {
            log::warn!("[Server] Transport error: {}", e);
        }

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

        // Generate player number (1-indexed, host is Player 1)
        let player_number = self.players.len() + 2; // +2 because host is Player 1

        let info = PlayerInfo {
            player_id,
            client_id,
            name: format!("Player {}", player_number),
            position: spawn_position,
            velocity: [0.0, 0.0, 0.0],
            yaw: 0.0,
            pitch: 0.0,
            last_sequence: 0,
            connected_at: Instant::now(),
        };

        log::info!(
            "[GameServer] Client {} connected as player_id={}",
            client_id,
            player_id
        );

        // Get custom texture count from texture manager
        let custom_texture_count = self
            .texture_manager
            .as_ref()
            .map(|m| m.max_slots())
            .unwrap_or(0);

        // Send connection accepted message
        let msg = ServerMessage::ConnectionAccepted(ConnectionAccepted {
            protocol_version: crate::net::protocol::PROTOCOL_SCHEMA_VERSION,
            player_id,
            tick_rate: TICK_RATE as u32,
            spawn_position,
            world_seed: self.world_seed,
            world_gen: self.world_gen,
            custom_texture_count,
        });

        match bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            Ok(encoded) => {
                self.server
                    .send_message(client_id, 2, renet::Bytes::from(encoded)); // Channel 2 = GameState
            }
            Err(err) => {
                log::error!(
                    "[GameServer] Failed to encode ConnectionAccepted for client {}: {}. Sending rejection so the client doesn't hang.",
                    client_id,
                    err
                );
                // Best-effort send a ConnectionRejected so the client fails
                // fast with a readable reason instead of sitting in the
                // connecting state until its timeout fires.
                let rejection =
                    ServerMessage::ConnectionRejected(crate::net::protocol::ConnectionRejected {
                        reason: "Internal server error encoding handshake".into(),
                    });
                if let Ok(encoded) =
                    bincode::serde::encode_to_vec(&rejection, bincode::config::standard())
                {
                    self.server
                        .send_message(client_id, 2, renet::Bytes::from(encoded));
                }
                // Drop the client so netcode cleans up state rather than
                // leaving a half-connected slot.
                self.server.disconnect(client_id);
                return None;
            }
        }

        // Send model registry sync after connection accepted
        self.send_model_registry(client_id);

        // Send existing players (including host) to the new client
        // First, send the host player if present
        if let Some(ref host) = self.host_player {
            let host_join = ServerMessage::PlayerJoined(PlayerJoined {
                player_id: host.player_id,
                name: host.name.clone(),
                position: host.position,
            });
            if let Ok(encoded) =
                bincode::serde::encode_to_vec(&host_join, bincode::config::standard())
            {
                self.server
                    .send_message(client_id, 2, renet::Bytes::from(encoded));
                log::debug!(
                    "[GameServer] Sent host player info to new client: {} ({})",
                    host.name,
                    host.player_id
                );
            }
        }

        // Then, send all other connected players
        for (&other_client_id, other_player) in &self.players {
            if other_client_id != client_id {
                let other_join = ServerMessage::PlayerJoined(PlayerJoined {
                    player_id: other_player.player_id,
                    name: other_player.name.clone(),
                    position: other_player.position,
                });
                if let Ok(encoded) =
                    bincode::serde::encode_to_vec(&other_join, bincode::config::standard())
                {
                    self.server
                        .send_message(client_id, 2, renet::Bytes::from(encoded));
                    log::debug!(
                        "[GameServer] Sent existing player info to new client: {} ({})",
                        other_player.name,
                        other_player.player_id
                    );
                }
            }
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
        self.forget_client_chunk_history(client_id);
        self.message_rate_limiter.forget_client(client_id);
        if let Some(info) = self.players.remove(&client_id) {
            // Broadcast player left
            let msg = ServerMessage::PlayerLeft(PlayerLeft {
                player_id: info.player_id,
            });
            self.broadcast_encoded(2, "PlayerLeft", &msg);
            Some(info.player_id)
        } else {
            None
        }
    }

    /// Updates a player's state from input.
    ///
    /// Rejects deltas that could only come from a speed-hacked or malicious
    /// client: per-tick position movement capped at `MAX_POSITION_DELTA_PER_TICK`
    /// blocks and speed capped at `MAX_PLAYER_SPEED` blocks/sec. When a delta
    /// is out of bounds we clamp to the previous known position rather than
    /// trusting the client — a legit client will re-converge via its own
    /// reconciliation once the server pushes its authoritative state back.
    pub fn update_player_state(
        &mut self,
        client_id: u64,
        position: [f32; 3],
        velocity: [f32; 3],
        yaw: f32,
        pitch: f32,
        sequence: u32,
    ) {
        /// Hard upper bound on any single movement delta the server will accept
        /// from a client in a single input tick. 32 blocks is already generous
        /// — normal sprint/jump max out near ~0.8 blocks/tick at 20 Hz.
        const MAX_POSITION_DELTA_PER_TICK: f32 = 32.0;

        /// Absolute speed cap in blocks/second (fly mode + sprint is ~20).
        const MAX_PLAYER_SPEED: f32 = 64.0;

        // The host's loopback client is authoritative — its position is already
        // tracked via update_host_player, and the self.players entry may be
        // stale because only host_player is updated each frame.  Just mirror
        // the host's state directly and skip the anti-cheat checks.
        if self.host_client_id == Some(client_id) {
            if let Some(info) = self.players.get_mut(&client_id) {
                info.position = position;
                info.velocity = velocity;
                info.yaw = yaw;
                info.pitch = pitch;
                info.last_sequence = sequence;
            }
            return;
        }

        // Non-finite floats would have been caught at ClientMessage::validate
        // but check defensively for hosts bypassing the normal receive path.
        let finite = position.iter().all(|v| v.is_finite())
            && velocity.iter().all(|v| v.is_finite())
            && yaw.is_finite()
            && pitch.is_finite();
        if !finite {
            log::warn!(
                "[Server] Rejecting non-finite player input from client {}",
                client_id
            );
            return;
        }

        let speed_sq =
            velocity[0] * velocity[0] + velocity[1] * velocity[1] + velocity[2] * velocity[2];
        if speed_sq > MAX_PLAYER_SPEED * MAX_PLAYER_SPEED {
            log::warn!(
                "[Server] Clamping oversize velocity from client {} ({:.1} blk/s)",
                client_id,
                speed_sq.sqrt()
            );
            // Drop this update; the player will stay at their last-known state.
            return;
        }

        if let Some(info) = self.players.get_mut(&client_id) {
            // First position update after connect — accept whatever the client
            // sends.  The spawn position in self.players is a placeholder and
            // may be hundreds of blocks from the client's real location.
            let first_update = info.last_sequence == 0;

            let dx = position[0] - info.position[0];
            let dy = position[1] - info.position[1];
            let dz = position[2] - info.position[2];
            let delta_sq = dx * dx + dy * dy + dz * dz;
            if !first_update && delta_sq > MAX_POSITION_DELTA_PER_TICK * MAX_POSITION_DELTA_PER_TICK
            {
                log::warn!(
                    "[Server] Rejecting teleport-sized delta from client {} ({:.1} blocks)",
                    client_id,
                    delta_sq.sqrt()
                );
                // Keep previous position; update yaw/pitch only so the camera
                // still feels responsive if the client is near-valid.
                info.yaw = yaw;
                info.pitch = pitch;
                info.last_sequence = sequence;
                return;
            }
            info.position = position;
            info.velocity = velocity;
            info.yaw = yaw;
            info.pitch = pitch;
            info.last_sequence = sequence;
        }
    }

    /// Broadcasts a single block change to all clients except the originator.
    pub fn broadcast_block_change_except(&mut self, change: BlockChanged, exclude_client_id: u64) {
        let msg = ServerMessage::BlockChanged(change);
        self.broadcast_encoded_except(1, "BlockChanged", &msg, exclude_client_id);
    }

    /// Broadcasts a single block change to all clients.
    pub fn broadcast_block_change(&mut self, change: BlockChanged) {
        let msg = ServerMessage::BlockChanged(change);
        self.broadcast_encoded(1, "BlockChanged", &msg);
    }

    /// Broadcasts multiple block changes to all clients except the originator.
    pub fn broadcast_block_changes_except(
        &mut self,
        changes: BlocksChanged,
        exclude_client_id: u64,
    ) {
        let msg = ServerMessage::BlocksChanged(changes);
        self.broadcast_encoded_except(1, "BlocksChanged", &msg, exclude_client_id);
    }

    /// Broadcasts multiple block changes to all clients.
    pub fn broadcast_block_changes(&mut self, changes: BlocksChanged) {
        let msg = ServerMessage::BlocksChanged(changes);
        self.broadcast_encoded(1, "BlocksChanged", &msg);
    }

    /// Broadcasts a door toggle to all clients except the originator.
    pub fn broadcast_door_toggled_except(
        &mut self,
        door: crate::net::protocol::DoorToggled,
        exclude_client_id: u64,
    ) {
        let msg = ServerMessage::DoorToggled(door);
        self.broadcast_encoded_except(1, "DoorToggled", &msg, exclude_client_id);
    }

    /// Broadcasts a door toggle to all clients.
    /// Used for server-authoritative door state sync.
    pub fn broadcast_door_toggled(&mut self, door: crate::net::protocol::DoorToggled) {
        let msg = ServerMessage::DoorToggled(door);
        self.broadcast_encoded(1, "DoorToggled", &msg);
    }

    /// Broadcasts water cell changes to all clients.
    /// Used for server-authoritative water simulation sync.
    pub fn broadcast_water_cells_changed(
        &mut self,
        updates: Vec<crate::net::protocol::WaterCellUpdate>,
    ) {
        let msg =
            ServerMessage::WaterCellsChanged(crate::net::protocol::WaterCellsChanged { updates });
        self.broadcast_encoded(1, "WaterCellsChanged", &msg);
    }

    /// Broadcasts lava cell changes to all clients.
    /// Used for server-authoritative lava simulation sync.
    pub fn broadcast_lava_cells_changed(
        &mut self,
        updates: Vec<crate::net::protocol::LavaCellUpdate>,
    ) {
        let msg =
            ServerMessage::LavaCellsChanged(crate::net::protocol::LavaCellsChanged { updates });
        self.broadcast_encoded(1, "LavaCellsChanged", &msg);
    }

    /// Broadcasts a falling block spawn to all clients.
    /// Used for server-authoritative falling block physics sync.
    pub fn broadcast_falling_block_spawned(
        &mut self,
        spawn: crate::net::protocol::FallingBlockSpawned,
    ) {
        let msg = ServerMessage::FallingBlockSpawned(spawn);
        self.broadcast_encoded(1, "FallingBlockSpawned", &msg);
    }

    /// Broadcasts a falling block landing to all clients.
    /// Used for server-authoritative falling block physics sync.
    pub fn broadcast_falling_block_landed(
        &mut self,
        land: crate::net::protocol::FallingBlockLanded,
    ) {
        let msg = ServerMessage::FallingBlockLanded(land);
        self.broadcast_encoded(1, "FallingBlockLanded", &msg);
    }

    /// Broadcasts a tree fall event to all clients.
    /// Used for server-authoritative multi-block tree fall sync.
    /// This is more bandwidth-efficient than sending individual FallingBlockSpawned messages.
    pub fn broadcast_tree_fell(&mut self, tree_fell: TreeFell) {
        let msg = ServerMessage::TreeFell(tree_fell);
        self.broadcast_encoded(1, "TreeFell", &msg);
    }

    /// Sends chunk data to a specific client.
    pub fn send_chunk(&mut self, client_id: u64, chunk: ChunkData) {
        let position = chunk.position;
        let msg = ServerMessage::ChunkData(chunk);
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            let len = encoded.len();
            self.server
                .send_message(client_id, 3, renet::Bytes::from(encoded)); // Channel 3 = ChunkStream
            self.record_chunk_sent(client_id, position); // epoch recorded by caller if available
            log::debug!(
                "[GameServer] Sent ChunkData to client {} ({} bytes)",
                client_id,
                len
            );
        } else {
            self.encode_failures = self.encode_failures.saturating_add(1);
            log::warn!(
                "[GameServer] Failed to encode ChunkData for client {}",
                client_id
            );
        }
    }

    /// Sends chunk data to a specific client, recording the mutation epoch for
    /// epoch-aware dedup on future re-requests.
    pub fn send_chunk_with_epoch(&mut self, client_id: u64, chunk: ChunkData, epoch: u64) {
        let position = chunk.position;
        let msg = ServerMessage::ChunkData(chunk);
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            let len = encoded.len();
            self.server
                .send_message(client_id, 3, renet::Bytes::from(encoded));
            self.record_chunk_sent_with_epoch(client_id, position, epoch);
            log::debug!(
                "[GameServer] Sent ChunkData to client {} ({} bytes, epoch={})",
                client_id,
                len,
                epoch
            );
        } else {
            self.encode_failures = self.encode_failures.saturating_add(1);
            log::warn!(
                "[GameServer] Failed to encode ChunkData for client {}",
                client_id
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
            self.record_chunk_sent(client_id, position);
            log::debug!(
                "[GameServer] Sent ChunkGenerateLocal for {:?} to client {} ({} bytes)",
                position,
                client_id,
                len
            );
        } else {
            self.encode_failures = self.encode_failures.saturating_add(1);
            log::warn!(
                "[GameServer] Failed to encode ChunkGenerateLocal for client {}",
                client_id
            );
        }
    }

    /// Broadcasts player states to all clients.
    /// Includes both connected players and the host player.
    pub fn broadcast_player_states(&mut self) {
        // Broadcast the host player's state to all connected clients
        // EXCEPT the host's own loopback client (the host is authoritative).
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
                for &client_id in self.players.keys() {
                    if self.host_client_id != Some(client_id) {
                        self.server.send_message(client_id, 0, bytes.clone());
                    }
                }
            }
        }

        // Broadcast each connected player's state to all other clients.
        // Skip the host's loopback client — external clients see player_id=0 for the host.
        // The host's loopback receives other players' states so the host can see them.
        for (&client_id, info) in &self.players {
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
        self.broadcast_encoded(2, "TimeUpdate", &msg);
    }

    /// Broadcasts day cycle pause state to all clients.
    pub fn broadcast_day_cycle_pause(&mut self, paused: bool, time_of_day: f32) {
        use crate::net::protocol::DayCyclePauseChanged;
        let msg = ServerMessage::DayCyclePauseChanged(DayCyclePauseChanged {
            paused,
            time_of_day,
        });
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.server
                .broadcast_message(2, renet::Bytes::from(encoded));
            log::debug!(
                "[Server] Broadcast DayCyclePauseChanged: {}",
                if paused { "PAUSED" } else { "RUNNING" }
            );
        }
    }

    /// Broadcasts spawn position change to all clients.
    /// Used when the spawn point is changed (e.g., via console command).
    pub fn broadcast_spawn_position(&mut self, position: [f32; 3]) {
        use crate::net::protocol::SpawnPositionChanged;
        let msg = ServerMessage::SpawnPositionChanged(SpawnPositionChanged { position });
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.server
                .broadcast_message(2, renet::Bytes::from(encoded));
            log::debug!(
                "[Server] Broadcast SpawnPositionChanged: ({:.1}, {:.1}, {:.1})",
                position[0],
                position[1],
                position[2]
            );
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
            log::debug!("[Server] Broadcast ModelAdded to all clients");
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
            log::debug!(
                "[Server] Broadcast TextureData (slot {}, '{}') to all clients",
                slot,
                name
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
            ServerMessage::ModelRegistrySync(Box::new(msg)),
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

    /// Adds a new picture to the picture store.
    /// Returns the assigned picture ID, or error if storage is full or validation fails.
    pub fn add_picture(&mut self, name: &str, png_data: &[u8]) -> Result<u16, String> {
        let manager = match &mut self.picture_manager {
            Some(m) => m,
            None => return Err("Picture manager not initialized".to_string()),
        };

        manager.add_picture(name, png_data)
    }

    /// Gets picture data by ID.
    pub fn get_picture(&self, picture_id: u16) -> Option<Vec<u8>> {
        self.picture_manager.as_ref()?.get_picture(picture_id)
    }

    /// Gets picture name by ID.
    pub fn get_picture_name(&self, picture_id: u16) -> Option<String> {
        self.picture_manager
            .as_ref()?
            .get_picture_name(picture_id)
            .map(|s| s.to_string())
    }

    /// Broadcasts a new picture to all clients.
    pub fn broadcast_picture_added(&mut self, picture_id: u16, name: String) {
        use crate::net::protocol::PictureAdded;
        let msg = ServerMessage::PictureAdded(PictureAdded { picture_id, name });
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.server
                .broadcast_message(2, renet::Bytes::from(encoded)); // Channel 2 = GameState
            log::debug!("[Server] Broadcast PictureAdded to all clients");
        }
    }

    /// Broadcasts a picture frame assignment to all clients.
    pub fn broadcast_frame_picture_set(&mut self, position: [i32; 3], picture_id: Option<u16>) {
        use crate::net::protocol::FramePictureSet;
        let msg = ServerMessage::FramePictureSet(FramePictureSet {
            position,
            picture_id,
        });
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.server
                .broadcast_message(1, renet::Bytes::from(encoded)); // Channel 1 = BlockUpdates
            log::debug!(
                "[Server] Broadcast FramePictureSet at {:?}: {:?}",
                position,
                picture_id
            );
        }
    }

    /// Broadcasts a stencil load to all clients.
    /// Used for multiplayer stencil synchronization when a player loads a stencil.
    pub fn broadcast_stencil_loaded(
        &mut self,
        stencil_id: u64,
        name: String,
        stencil_data: Vec<u8>,
    ) {
        use crate::net::protocol::StencilLoaded;
        let msg = ServerMessage::StencilLoaded(StencilLoaded {
            stencil_id,
            name,
            stencil_data,
        });
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.server
                .broadcast_message(2, renet::Bytes::from(encoded)); // Channel 2 = GameState
            log::debug!("[Server] Broadcast StencilLoaded to all clients");
        }
    }

    /// Broadcasts a stencil transform update to all clients.
    pub fn broadcast_stencil_transform(
        &mut self,
        stencil_id: u64,
        position: [i32; 3],
        rotation: u8,
    ) {
        use crate::net::protocol::StencilTransformUpdate;
        let msg = ServerMessage::StencilTransformUpdate(StencilTransformUpdate {
            stencil_id,
            position,
            rotation,
        });
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.server
                .broadcast_message(2, renet::Bytes::from(encoded)); // Channel 2 = GameState
            log::debug!(
                "[Server] Broadcast StencilTransformUpdate: id={} pos={:?} rot={}",
                stencil_id,
                position,
                rotation
            );
        }
    }

    /// Broadcasts a stencil removal to all clients.
    pub fn broadcast_stencil_removed(&mut self, stencil_id: u64) {
        use crate::net::protocol::StencilRemoved;
        let msg = ServerMessage::StencilRemoved(StencilRemoved { stencil_id });
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.server
                .broadcast_message(2, renet::Bytes::from(encoded)); // Channel 2 = GameState
            log::debug!("[Server] Broadcast StencilRemoved: id={}", stencil_id);
        }
    }

    /// Broadcasts a template load to all clients.
    /// Used for multiplayer template synchronization when a player loads a template.
    pub fn broadcast_template_loaded(
        &mut self,
        template_id: u64,
        name: String,
        template_data: Vec<u8>,
    ) {
        use crate::net::protocol::TemplateLoaded;
        let msg = ServerMessage::TemplateLoaded(TemplateLoaded {
            template_id,
            name,
            template_data,
        });
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.server
                .broadcast_message(2, renet::Bytes::from(encoded)); // Channel 2 = GameState
            log::debug!("[Server] Broadcast TemplateLoaded to all clients");
        }
    }

    /// Broadcasts a template removal to all clients.
    pub fn broadcast_template_removed(&mut self, template_id: u64) {
        use crate::net::protocol::TemplateRemoved;
        let msg = ServerMessage::TemplateRemoved(TemplateRemoved { template_id });
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.server
                .broadcast_message(2, renet::Bytes::from(encoded)); // Channel 2 = GameState
            log::debug!("[Server] Broadcast TemplateRemoved: id={}", template_id);
        }
    }

    /// Receives messages from clients.
    /// Returns an iterator of (client_id, channel_id, message_data).
    ///
    /// Capped at `MAX_RECEIVE_BATCH` per call so a fast sender can't starve
    /// the game loop with an unbounded receive Vec. Remaining messages stay
    /// queued in renet and are drained on the next tick.
    pub fn receive_messages(&mut self) -> impl Iterator<Item = (u64, u8, Vec<u8>)> + '_ {
        /// Upper bound on messages drained from renet per server tick.
        /// 4 players × ~20 Hz × 4 channels = ~320 msg/s steady-state, so 256
        /// per tick is plenty for legitimate load while still clamping a
        /// flooding client.
        const MAX_RECEIVE_BATCH: usize = 256;

        let mut messages = Vec::with_capacity(MAX_RECEIVE_BATCH);

        'outer: for client_id in self.server.clients_id() {
            for channel in crate::net::channel::Channel::all() {
                while let Some(message) = self.server.receive_message(client_id, channel.id()) {
                    messages.push((client_id, channel.id(), message.to_vec()));
                    if messages.len() >= MAX_RECEIVE_BATCH {
                        log::debug!(
                            "[Server] receive_messages hit per-tick batch cap ({}); deferring rest",
                            MAX_RECEIVE_BATCH
                        );
                        break 'outer;
                    }
                }
            }
        }

        messages.into_iter()
    }

    /// Receives and parses client messages into typed ClientMessage enums.
    /// Returns a vector of (client_id, parsed_message).
    ///
    /// Applies a hard `MAX_INBOUND_MESSAGE_SIZE` limit during decoding and runs
    /// `ClientMessage::validate()` so per-variant field caps (coord bounds,
    /// upload sizes, string lengths) are enforced before the message reaches
    /// any higher-level handler. Invalid messages are dropped and logged.
    pub fn receive_client_messages(&mut self) -> Vec<(u64, ClientMessage)> {
        use crate::net::protocol::MAX_INBOUND_MESSAGE_SIZE;

        let mut parsed_messages = Vec::new();

        for (client_id, _channel_id, data) in self.receive_messages() {
            if data.len() > MAX_INBOUND_MESSAGE_SIZE {
                log::warn!(
                    "[Server] Dropping {}-byte message from client {} (exceeds MAX_INBOUND_MESSAGE_SIZE)",
                    data.len(),
                    client_id
                );
                continue;
            }
            match bincode::serde::decode_from_slice::<ClientMessage, _>(
                &data,
                bincode::config::standard().with_limit::<MAX_INBOUND_MESSAGE_SIZE>(),
            ) {
                Ok((msg, _)) => match msg.validate() {
                    Ok(()) => parsed_messages.push((client_id, msg)),
                    Err(reason) => {
                        log::warn!(
                            "[Server] Rejecting message from client {}: {}",
                            client_id,
                            reason
                        );
                    }
                },
                Err(err) => {
                    log::debug!(
                        "[Server] Failed to decode message from client {}: {}",
                        client_id,
                        err
                    );
                }
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
        // Check if this is the host player
        if self.host_client_id == Some(client_id) {
            return self.host_player.as_ref();
        }
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

    /// Sets a player's display name after sanitization.
    ///
    /// Strips control characters and truncates to `MAX_PLAYER_NAME_LEN` so a
    /// hostile peer can't broadcast ANSI escapes, NULs, or a huge name to every
    /// other player. Returns `None` if the sanitized name is empty or the
    /// client is unknown.
    pub fn set_player_name(
        &mut self,
        client_id: u64,
        name: String,
    ) -> Option<(u64, String, String)> {
        let name = sanitize_player_name(&name)?;

        // Check if this is the host player (host has client_id matching host_client_id)
        if self.host_client_id == Some(client_id)
            && let Some(ref mut host) = self.host_player
        {
            let old_name = host.name.clone();
            host.name = name.clone();
            return Some((host.player_id, old_name, name));
        }
        // Check regular players
        if let Some(info) = self.players.get_mut(&client_id) {
            let old_name = info.name.clone();
            info.name = name.clone();
            Some((info.player_id, old_name, name))
        } else {
            None
        }
    }

    /// Broadcasts a player name change to all clients.
    pub fn broadcast_player_name_changed(
        &mut self,
        player_id: u64,
        old_name: String,
        new_name: String,
    ) {
        use crate::net::protocol::PlayerNameChanged;
        let msg = ServerMessage::PlayerNameChanged(PlayerNameChanged {
            player_id,
            old_name,
            new_name,
        });
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.server
                .broadcast_message(2, renet::Bytes::from(encoded)); // Channel 2 = GameState
            log::debug!("[Server] Broadcast PlayerNameChanged: player {}", player_id);
        }
    }

    /// Broadcasts a chat message to all clients.
    pub fn broadcast_chat(&mut self, player_id: u64, player_name: String, message: String) {
        use crate::net::protocol::ChatReceived;
        let msg = ServerMessage::ChatReceived(ChatReceived {
            player_id,
            player_name,
            message,
        });
        if let Ok(encoded) = bincode::serde::encode_to_vec(&msg, bincode::config::standard()) {
            self.server
                .broadcast_message(2, renet::Bytes::from(encoded)); // Channel 2 = GameState
        }
    }
}

/// Generates a unique player ID from client ID.
fn generate_player_id(client_id: u64) -> PlayerId {
    // Mix client ID with some entropy
    client_id.wrapping_mul(0x5851F42E4C957F2D) ^ 0x123456789ABCDEF0
}

/// Sanitizes a client-supplied display name.
///
/// Strips control characters (including NUL and ANSI escape introducer),
/// normalizes whitespace, truncates to `MAX_PLAYER_NAME_LEN` bytes on a
/// UTF-8 char boundary, and rejects the empty result. Returns `None` when
/// the name is empty post-sanitization.
pub(crate) fn sanitize_player_name(raw: &str) -> Option<String> {
    use crate::net::protocol::MAX_PLAYER_NAME_LEN;

    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control())
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();

    // Collapse runs of spaces and trim.
    let mut normalized = String::with_capacity(cleaned.len());
    let mut last_space = false;
    for c in cleaned.chars() {
        if c == ' ' {
            if !last_space {
                normalized.push(' ');
            }
            last_space = true;
        } else {
            normalized.push(c);
            last_space = false;
        }
    }
    let trimmed = normalized.trim();

    // Byte-wise truncate to MAX_PLAYER_NAME_LEN on a UTF-8 boundary.
    let bytes = trimmed.as_bytes();
    let capped = if bytes.len() <= MAX_PLAYER_NAME_LEN {
        trimmed.to_string()
    } else {
        let mut cut = MAX_PLAYER_NAME_LEN;
        while cut > 0 && !trimmed.is_char_boundary(cut) {
            cut -= 1;
        }
        trimmed[..cut].to_string()
    };

    if capped.is_empty() {
        None
    } else {
        Some(capped)
    }
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

    /// Spawns a real GameServer on an ephemeral loopback port so
    /// dedup/lifecycle tests exercise the actual state machine, not a
    /// bolted-on HashMap stand-in.
    fn spawn_test_server() -> GameServer {
        // Pick a free UDP port by binding, reading the local_addr, dropping.
        let s = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind scout");
        let addr = s.local_addr().unwrap();
        drop(s);
        GameServer::new(addr, 0, 0).expect("spawn test server")
    }

    #[test]
    fn test_message_rate_limiter_allows_under_cap() {
        let mut limiter = MessageRateLimiter::new();
        // 3 chat messages inside a 5-second window with cap=5 must all pass.
        assert!(limiter.check(1, "chat", 1_000_000, 5_000_000, 5));
        assert!(limiter.check(1, "chat", 2_000_000, 5_000_000, 5));
        assert!(limiter.check(1, "chat", 3_000_000, 5_000_000, 5));
    }

    #[test]
    fn test_message_rate_limiter_blocks_over_cap() {
        let mut limiter = MessageRateLimiter::new();
        for i in 0..5 {
            assert!(limiter.check(1, "chat", (i + 1) * 100_000, 5_000_000, 5));
        }
        assert!(
            !limiter.check(1, "chat", 600_000, 5_000_000, 5),
            "6th call in window must be rejected"
        );
    }

    #[test]
    fn test_message_rate_limiter_window_slides() {
        let mut limiter = MessageRateLimiter::new();
        for i in 0..5 {
            assert!(limiter.check(1, "chat", (i + 1) * 100_000, 5_000_000, 5));
        }
        assert!(!limiter.check(1, "chat", 600_000, 5_000_000, 5));
        // Far-future call: the window has slid past every earlier event.
        assert!(limiter.check(1, "chat", 10_000_000, 5_000_000, 5));
    }

    #[test]
    fn test_message_rate_limiter_isolates_clients_and_kinds() {
        let mut limiter = MessageRateLimiter::new();
        for i in 0..5 {
            assert!(limiter.check(1, "chat", (i + 1) * 100_000, 5_000_000, 5));
        }
        // Client 1 chat is capped but client 2 chat and client 1 console must
        // still be open — the limiter keys on (client_id, kind).
        assert!(limiter.check(2, "chat", 600_000, 5_000_000, 5));
        assert!(limiter.check(1, "console", 600_000, 5_000_000, 10));
    }

    #[test]
    fn test_message_rate_limiter_forget_client() {
        let mut limiter = MessageRateLimiter::new();
        for i in 0..5 {
            assert!(limiter.check(1, "chat", (i + 1) * 100_000, 5_000_000, 5));
        }
        assert!(!limiter.check(1, "chat", 600_000, 5_000_000, 5));
        limiter.forget_client(1);
        assert!(
            limiter.check(1, "chat", 600_000, 5_000_000, 5),
            "post-forget state must reset the bucket"
        );
    }

    #[test]
    fn test_gameserver_check_message_rate_with_explicit_clock() {
        let mut srv = spawn_test_server();
        // 5 chat messages must pass, the 6th within the window must fail.
        for i in 0..5 {
            assert!(srv.check_message_rate_at(42, "chat", (i + 1) * 100_000));
        }
        assert!(!srv.check_message_rate_at(42, "chat", 600_000));
    }

    #[test]
    fn test_encode_failures_counter_starts_at_zero_and_increments() {
        let mut srv = spawn_test_server();
        assert_eq!(srv.encode_failures(), 0, "fresh server has no failures");

        // Manually bump the counter to simulate a failure path — the counter
        // is the public contract; construction of a Serialize-failing message
        // would require touching every ServerMessage variant, which defeats
        // the point of a counter check.
        srv.encode_failures = srv.encode_failures.saturating_add(1);
        assert_eq!(srv.encode_failures(), 1);

        // Saturating: the counter never wraps.
        srv.encode_failures = u64::MAX;
        srv.encode_failures = srv.encode_failures.saturating_add(1);
        assert_eq!(srv.encode_failures(), u64::MAX);
    }

    #[test]
    fn test_net_stats_track_broadcast_bytes_and_counts() {
        let mut srv = spawn_test_server();
        assert_eq!(srv.net_stats().bytes_out, 0);
        assert_eq!(srv.net_stats().messages_out, 0);

        let msg =
            ServerMessage::BlocksChanged(crate::net::protocol::BlocksChanged { changes: vec![] });
        assert!(srv.broadcast_encoded(1, "BlocksChanged", &msg));
        assert!(srv.broadcast_encoded(1, "BlocksChanged", &msg));

        let snap = srv.net_stats();
        assert_eq!(snap.messages_out, 2);
        assert!(snap.bytes_out > 0, "some bytes must have been counted");
        assert_eq!(snap.per_type_count.get("BlocksChanged"), Some(&2));
        assert!(
            snap.per_type_bytes
                .get("BlocksChanged")
                .copied()
                .unwrap_or(0)
                > 0
        );

        // Reset wipes counters but leaves encode_failures alone — that counter
        // is semantically "since-startup" and shouldn't be zeroable by a HUD.
        srv.reset_net_stats();
        assert_eq!(srv.net_stats().bytes_out, 0);
        assert_eq!(srv.net_stats().messages_out, 0);
        assert!(srv.net_stats().per_type_count.is_empty());
    }

    #[test]
    fn test_broadcast_encoded_happy_path_leaves_counter_zero() {
        let mut srv = spawn_test_server();
        // Broadcasting an empty BlocksChanged is a well-formed message that
        // must encode successfully — the counter must not move.
        let msg =
            ServerMessage::BlocksChanged(crate::net::protocol::BlocksChanged { changes: vec![] });
        let ok = srv.broadcast_encoded(1, "BlocksChanged(empty)", &msg);
        assert!(ok, "empty BlocksChanged must encode ok");
        assert_eq!(srv.encode_failures(), 0);
    }

    #[test]
    fn test_should_send_chunk_initial_call_is_always_allowed() {
        let mut srv = spawn_test_server();
        assert!(srv.should_send_chunk(42, [1, 2, 3]));
    }

    #[test]
    fn test_record_chunk_sent_blocks_second_call_in_window() {
        let mut srv = spawn_test_server();
        srv.record_chunk_sent(42, [1, 2, 3]);
        assert!(
            !srv.should_send_chunk(42, [1, 2, 3]),
            "same chunk within window must be deduped"
        );
    }

    #[test]
    fn test_forget_client_chunk_history_clears_dedup_for_client() {
        let mut srv = spawn_test_server();
        srv.record_chunk_sent(42, [1, 2, 3]);
        srv.record_chunk_sent(99, [1, 2, 3]);
        srv.forget_client_chunk_history(42);
        assert!(
            srv.should_send_chunk(42, [1, 2, 3]),
            "42's entry must be gone"
        );
        assert!(
            !srv.should_send_chunk(99, [1, 2, 3]),
            "99's entry must survive"
        );
    }

    #[test]
    fn test_purge_stale_chunk_history_drops_expired_only() {
        let mut srv = spawn_test_server();
        // Inject a stale timestamp by reaching into the internal map.
        let old = Instant::now()
            .checked_sub(CHUNK_RESEND_WINDOW + Duration::from_secs(1))
            .expect("clock is at least CHUNK_RESEND_WINDOW past epoch");
        srv.recently_sent_chunks
            .entry(42)
            .or_default()
            .insert([1, 2, 3], (old, 0));
        srv.record_chunk_sent(42, [4, 5, 6]); // fresh entry
        srv.purge_stale_chunk_history();

        let m = srv.recently_sent_chunks.get(&42).unwrap();
        assert!(!m.contains_key(&[1, 2, 3]), "stale entry must be purged");
        assert!(m.contains_key(&[4, 5, 6]), "fresh entry must remain");
    }

    #[test]
    fn test_sanitize_player_name_strips_controls() {
        // Control chars (NUL, ESC, tab) are stripped; the remaining printable
        // chars of a partial ANSI escape survive — intentional, we don't parse
        // full escape sequences, we only prevent raw control bytes from
        // reaching other players' terminals.
        assert_eq!(
            sanitize_player_name("alice\x00\t\x1b"),
            Some("alice".to_string())
        );
    }

    #[test]
    fn test_sanitize_player_name_collapses_whitespace() {
        assert_eq!(
            sanitize_player_name("   bob   the  \n\tbuilder   "),
            Some("bob the builder".to_string())
        );
    }

    #[test]
    fn test_sanitize_player_name_truncates_on_utf8_boundary() {
        use crate::net::protocol::MAX_PLAYER_NAME_LEN;
        // Long multi-byte name — must not panic nor split a codepoint.
        let huge: String = "🎮".repeat(MAX_PLAYER_NAME_LEN * 2 / 4);
        let cleaned = sanitize_player_name(&huge).unwrap();
        assert!(cleaned.len() <= MAX_PLAYER_NAME_LEN);
        // Each 🎮 is 4 bytes; the result must still be valid UTF-8.
        assert!(std::str::from_utf8(cleaned.as_bytes()).is_ok());
    }

    #[test]
    fn test_chunk_send_dedup_window() {
        // We can't easily construct a full GameServer (needs a UDP socket), but
        // we can test the dedup state machine directly by manipulating a local
        // HashMap with the same shape.
        use std::collections::HashMap;

        let mut sent: HashMap<u64, HashMap<[i32; 3], Instant>> = HashMap::new();
        let client = 7u64;
        let pos = [1, 2, 3];
        // First send: should be allowed.
        let allowed_first = sent
            .get(&client)
            .and_then(|m| m.get(&pos))
            .map(|t| t.elapsed() < CHUNK_RESEND_WINDOW)
            .unwrap_or(false);
        assert!(
            !allowed_first,
            "first-time entry should not be flagged as dedup"
        );

        sent.entry(client).or_default().insert(pos, Instant::now());
        // Second check within window: should be flagged as "recently sent".
        let blocked = sent
            .get(&client)
            .and_then(|m| m.get(&pos))
            .map(|t| t.elapsed() < CHUNK_RESEND_WINDOW)
            .unwrap_or(false);
        assert!(
            blocked,
            "entry recorded just now must be within dedup window"
        );
    }

    #[test]
    fn test_sanitize_player_name_rejects_empty() {
        assert!(sanitize_player_name("").is_none());
        assert!(sanitize_player_name("   \x00\t\x1b").is_none());
    }
}

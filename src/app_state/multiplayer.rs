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
    BlockSyncManager, ChunkSyncManager, GameClient, GameServer, PredictionState, RemotePlayer,
    SerializedChunk,
};
use nalgebra::Vector3;

/// Multiplayer state for the game.
pub struct MultiplayerState {
    /// Current game mode.
    pub mode: GameMode,
    /// Server instance (only when hosting).
    pub server: Option<GameServer>,
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
            client: None,
            prediction: PredictionState::new(),
            remote_players: Vec::new(),
            chunk_sync: ChunkSyncManager::new(),
            block_sync: BlockSyncManager::new(false),
            input_sequence: 0,
            pending_block_changes: Vec::new(),
            pending_chunks: Vec::new(),
        }
    }

    /// Starts hosting a server.
    pub fn start_host(&mut self, port: u16, world_seed: u32, world_gen: u8) -> Result<(), String> {
        let addr: SocketAddr = ([0, 0, 0, 0], port).into();
        self.server = Some(GameServer::new(addr, world_seed, world_gen)?);
        self.mode = GameMode::Host;

        // Create local client that connects to localhost
        let localhost: SocketAddr = ([127, 0, 0, 1], port).into();
        self.client = Some(GameClient::new(localhost)?);
        self.client.as_mut().unwrap().connect();

        Ok(())
    }

    /// Connects to a remote server.
    pub fn connect(&mut self, address: &str) -> Result<(), String> {
        let addr: SocketAddr = address
            .parse()
            .map_err(|e| format!("Invalid address '{}': {}", address, e))?;

        self.client = Some(GameClient::new(addr)?);
        self.client.as_mut().unwrap().connect();
        self.mode = GameMode::Client;

        Ok(())
    }

    /// Updates the multiplayer state (call every frame).
    pub fn update(&mut self, duration: Duration) {
        // Update server if hosting
        if let Some(ref mut server) = self.server {
            let events = server.update(duration);
            for event in events {
                self.handle_server_event(event);
            }
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
}

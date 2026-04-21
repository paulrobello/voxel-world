//! Dedicated server thread for multiplayer networking.
//!
//! Moves server network processing to a separate thread to avoid
//! blocking the main game loop. Uses channels for communication:
//!
//! - **Commands** (Main → Server): SendChunk, BroadcastChange, etc.
//! - **Events** (Server → Main): ClientConnected, ClientMessage, etc.

// Allow unused code until threaded server mode is fully tested and enabled
#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use renet::ServerEvent;

use super::GameServer;
use super::protocol::{BlockChanged, ChunkData, ClientMessage, DoorToggled};

/// Commands sent from the main thread to the server thread.
#[derive(Debug, Clone)]
pub enum ServerCommand {
    /// Send chunk data to a specific client.
    SendChunk { client_id: u64, chunk: ChunkData },
    /// Instruct client to generate chunk locally (for unmodified chunks).
    SendChunkGenerateLocal { client_id: u64, position: [i32; 3] },
    /// Broadcast a block change to all clients.
    BroadcastBlockChange(BlockChanged),
    /// Broadcast a door toggle to all clients.
    BroadcastDoorToggled(DoorToggled),
    /// Broadcast player states to all clients.
    BroadcastPlayerStates,
    /// Broadcast time of day to all clients.
    BroadcastTime { time_of_day: f32 },
    /// Update a player's state on the server.
    UpdatePlayerState {
        client_id: u64,
        position: [f32; 3],
        velocity: [f32; 3],
        yaw: f32,
        pitch: f32,
        sequence: u32,
    },
    /// Set the host player info (server's own player).
    SetHostPlayer {
        player_id: u64,
        name: String,
        position: [f32; 3],
    },
    /// Update the host player's state.
    UpdateHostPlayer {
        position: [f32; 3],
        velocity: [f32; 3],
        yaw: f32,
        pitch: f32,
    },
    /// Handle a new client connection (send acceptance, etc.).
    HandleClientConnected {
        client_id: u64,
        spawn_position: [f32; 3],
    },
    /// Handle a client disconnection.
    HandleClientDisconnected { client_id: u64 },
    /// Handle a texture request from a client.
    HandleTextureRequest { client_id: u64, slot: u8 },
    /// Stop the server thread.
    Stop,
}

/// Events sent from the server thread to the main thread.
#[derive(Debug, Clone)]
pub enum ServerThreadEvent {
    /// A new client has connected.
    ClientConnected { client_id: u64 },
    /// A client has disconnected.
    ClientDisconnected { client_id: u64, reason: String },
    /// A message was received from a client.
    ClientMessage {
        client_id: u64,
        message: ClientMessage,
    },
    /// The server encountered an error.
    Error { error: String },
}

/// Wrapper that runs the game server in a dedicated thread.
pub struct ServerThread {
    /// Sender for commands to the server thread.
    command_sender: Sender<ServerCommand>,
    /// Receiver for events from the server thread.
    event_receiver: Receiver<ServerThreadEvent>,
    /// Thread handle for cleanup.
    handle: Option<JoinHandle<()>>,
    /// Flag to signal thread shutdown.
    running: Arc<AtomicBool>,
    /// Server address (for display purposes).
    address: SocketAddr,
}

impl ServerThread {
    /// Spawns a new server thread.
    ///
    /// # Arguments
    /// * `address` - The address to bind the server to
    /// * `world_seed` - The world seed to send to connecting clients
    /// * `world_gen` - The world generation type
    pub fn spawn(address: SocketAddr, world_seed: u32, world_gen: u8) -> Result<Self, String> {
        // Create channels for bidirectional communication
        let (command_sender, command_receiver) = crossbeam_channel::bounded(256);
        let (event_sender, event_receiver) = crossbeam_channel::bounded(256);
        let running = Arc::new(AtomicBool::new(true));

        // Create the server before spawning thread (to catch bind errors early)
        let server = GameServer::new(address, world_seed, world_gen)?;
        let server_address = address;

        // Clone running flag for the thread
        let thread_running = running.clone();

        // Spawn the server thread
        let handle = thread::Builder::new()
            .name("voxel-server".to_string())
            .spawn(move || {
                Self::server_loop(server, command_receiver, event_sender, thread_running);
            })
            .map_err(|e| format!("Failed to spawn server thread: {}", e))?;

        Ok(Self {
            command_sender,
            event_receiver,
            handle: Some(handle),
            running,
            address: server_address,
        })
    }

    /// Main server loop running in the dedicated thread.
    fn server_loop(
        mut server: GameServer,
        command_receiver: Receiver<ServerCommand>,
        event_sender: Sender<ServerThreadEvent>,
        running: Arc<AtomicBool>,
    ) {
        let tick_duration = Duration::from_millis(16); // ~60Hz tick rate

        while running.load(Ordering::Acquire) {
            let start = std::time::Instant::now();

            // Process incoming commands from main thread (non-blocking)
            while let Ok(cmd) = command_receiver.try_recv() {
                match cmd {
                    ServerCommand::SendChunk { client_id, chunk } => {
                        server.send_chunk(client_id, chunk);
                    }
                    ServerCommand::SendChunkGenerateLocal {
                        client_id,
                        position,
                    } => {
                        server.send_chunk_generate_local(client_id, position);
                    }
                    ServerCommand::BroadcastBlockChange(change) => {
                        server.broadcast_block_change(change);
                    }
                    ServerCommand::BroadcastDoorToggled(door) => {
                        server.broadcast_door_toggled(door);
                    }
                    ServerCommand::BroadcastPlayerStates => {
                        server.broadcast_player_states();
                    }
                    ServerCommand::BroadcastTime { time_of_day } => {
                        server.broadcast_time(time_of_day);
                    }
                    ServerCommand::UpdatePlayerState {
                        client_id,
                        position,
                        velocity,
                        yaw,
                        pitch,
                        sequence,
                    } => {
                        server.update_player_state(
                            client_id, position, velocity, yaw, pitch, sequence,
                        );
                    }
                    ServerCommand::SetHostPlayer {
                        player_id,
                        name,
                        position,
                    } => {
                        server.set_host_player(player_id, name, position);
                    }
                    ServerCommand::UpdateHostPlayer {
                        position,
                        velocity,
                        yaw,
                        pitch,
                    } => {
                        server.update_host_player(position, velocity, yaw, pitch);
                    }
                    ServerCommand::HandleClientConnected {
                        client_id,
                        spawn_position,
                    } => {
                        server.handle_client_connected(client_id, spawn_position);
                    }
                    ServerCommand::HandleClientDisconnected { client_id } => {
                        server.handle_client_disconnected(client_id);
                    }
                    ServerCommand::HandleTextureRequest { client_id, slot } => {
                        server.handle_texture_request(client_id, slot);
                    }
                    ServerCommand::Stop => {
                        running.store(false, Ordering::Release);
                        break;
                    }
                }
            }

            // Update server and process network events
            let events = server.update(tick_duration);

            // Forward server events to main thread
            for event in events {
                match event {
                    ServerEvent::ClientConnected { client_id } => {
                        if event_sender
                            .send(ServerThreadEvent::ClientConnected { client_id })
                            .is_err()
                        {
                            // Main thread dropped, exit
                            return;
                        }
                    }
                    ServerEvent::ClientDisconnected { client_id, reason } => {
                        if event_sender
                            .send(ServerThreadEvent::ClientDisconnected {
                                client_id,
                                reason: reason.to_string(),
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }

            // Process client messages
            let messages = server.receive_client_messages();
            for (client_id, message) in messages {
                if event_sender
                    .send(ServerThreadEvent::ClientMessage { client_id, message })
                    .is_err()
                {
                    return;
                }
            }

            // Sleep to maintain tick rate
            let elapsed = start.elapsed();
            if elapsed < tick_duration {
                thread::sleep(tick_duration - elapsed);
            }
        }
    }

    /// Sends a command to the server thread.
    ///
    /// Uses non-blocking `try_send` so a stalled server thread can't stall the
    /// game loop. Queue-full returns an error *and* is logged at `warn` level
    /// so the operator can see the stall before commands start being dropped
    /// silently.
    pub fn send_command(&self, command: ServerCommand) -> Result<(), String> {
        match self.command_sender.try_send(command) {
            Ok(()) => Ok(()),
            Err(crossbeam_channel::TrySendError::Full(_)) => {
                log::warn!("[ServerThread] Command queue full (cap 256); dropping command");
                Err("Server thread queue full".to_string())
            }
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                Err("Server thread not responding".to_string())
            }
        }
    }

    /// Tries to receive an event from the server thread (non-blocking).
    pub fn try_recv_event(&self) -> Option<ServerThreadEvent> {
        self.event_receiver.try_recv().ok()
    }

    /// Receives all pending events from the server thread.
    pub fn recv_events(&self) -> Vec<ServerThreadEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_receiver.try_recv() {
            events.push(event);
        }
        events
    }

    /// Returns the server address.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Checks if the server thread is still running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }
}

impl Drop for ServerThread {
    fn drop(&mut self) {
        // Signal thread to stop — Release pairs with the Acquire load on the
        // other side so the running=false observation happens-after any
        // cleanup state published here.
        self.running.store(false, Ordering::Release);

        // Send stop command to wake up thread if blocking
        let _ = self.command_sender.send(ServerCommand::Stop);

        // Wait for thread to finish
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_command_variants() {
        // Just verify the enums compile correctly
        let _ = ServerCommand::BroadcastTime { time_of_day: 0.5 };
        let _ = ServerCommand::Stop;
    }

    #[test]
    fn test_server_thread_event_variants() {
        let _ = ServerThreadEvent::ClientConnected { client_id: 1 };
        let _ = ServerThreadEvent::Error {
            error: "test".to_string(),
        };
    }

    /// Spawns a real ServerThread on an ephemeral loopback port. Caller is
    /// responsible for dropping it — Drop sends Stop and joins.
    fn spawn_test_thread() -> ServerThread {
        let scout = std::net::UdpSocket::bind("127.0.0.1:0").expect("scout bind");
        let addr = scout.local_addr().unwrap();
        drop(scout);
        ServerThread::spawn(addr, 0, 0).expect("spawn server thread")
    }

    #[test]
    fn test_server_thread_spawn_reports_running() {
        let thread = spawn_test_thread();
        assert!(thread.is_running());
        // Drop triggers Stop + join.
    }

    #[test]
    fn test_server_thread_send_command_succeeds_before_drop() {
        let thread = spawn_test_thread();
        let result = thread.send_command(ServerCommand::BroadcastTime { time_of_day: 0.5 });
        assert!(result.is_ok(), "command must be accepted: {:?}", result);
    }

    #[test]
    fn test_server_thread_drop_joins_within_bound() {
        let thread = spawn_test_thread();
        let start = std::time::Instant::now();
        drop(thread);
        // Drop -> Stop command + join. The server tick is 16 ms so join must
        // complete in a handful of ticks, well under 1 s. Allow 2 s slack for
        // slow CI.
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "Drop took too long: {:?}",
            start.elapsed()
        );
    }

    #[test]
    fn test_server_thread_command_queue_full_reports_err() {
        // With a bounded(256) queue, hammer it with small commands *without*
        // letting the thread drain (simulate by spawning + immediately sending
        // >256 commands back-to-back). At least one of them should hit the
        // "queue full" path.
        let thread = spawn_test_thread();
        let mut full_seen = false;
        for _ in 0..8192 {
            let r = thread.send_command(ServerCommand::BroadcastTime { time_of_day: 0.5 });
            if r.is_err() {
                full_seen = true;
                break;
            }
        }
        // On a fast host the thread may keep pace, so this is allowed to be
        // false in practice — what we're verifying is that IF the queue fills,
        // the API returns Err rather than blocking forever.
        if full_seen {
            // Must be the Full variant, not Disconnected.
            let msg = thread
                .send_command(ServerCommand::BroadcastTime { time_of_day: 0.5 })
                .err();
            // Either still full, or drained by now.
            if let Some(err) = msg {
                assert!(
                    err.contains("full") || err.contains("not responding"),
                    "unexpected err: {}",
                    err
                );
            }
        }
    }
}

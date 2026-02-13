//! Authentication and connection handling using renet_netcode.
//!
//! Provides secure handshake and connection management for both
//! server and client.

// Allow unused code until networking is integrated into the game
#![allow(dead_code)]

use std::net::SocketAddr;
use std::time::Duration;

use renet_netcode::{
    ClientAuthentication, NetcodeClientTransport, NetcodeServerTransport, ServerAuthentication,
    ServerConfig,
};

/// Default server port.
#[allow(dead_code)]
pub const DEFAULT_PORT: u16 = 5000;

/// Protocol version for compatibility checking.
#[allow(dead_code)]
pub const PROTOCOL_VERSION: &str = "voxel-world-1";

/// Connection timeout in milliseconds.
pub const CONNECTION_TIMEOUT_MS: u64 = 5000;

/// Maximum connections (max players).
pub const MAX_CONNECTIONS: usize = 4;

/// Server authentication tokens.
pub struct ServerAuth {
    /// Private key for encryption.
    private_key: [u8; 32],
    /// Server address.
    address: SocketAddr,
}

impl ServerAuth {
    /// Creates new server authentication.
    /// Uses unsecure mode for development/LAN play.
    pub fn new(address: SocketAddr) -> Self {
        // For development, use a fixed key that clients can also use
        let private_key = Self::generate_dev_key();

        Self {
            private_key,
            address,
        }
    }

    /// Creates server auth with a specific private key.
    pub fn with_key(address: SocketAddr, private_key: [u8; 32]) -> Self {
        Self {
            private_key,
            address,
        }
    }

    /// Generates the development key used by both server and client.
    pub fn generate_dev_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        let seed = b"voxel-world-dev-key-v1-32-byte!!";
        key.copy_from_slice(seed);
        key
    }

    /// Creates a NetcodeServerTransport for the server.
    /// Uses Unsecure mode for development/LAN play.
    pub fn create_transport(&self) -> Result<NetcodeServerTransport, String> {
        let server_config = ServerConfig {
            current_time: Duration::ZERO,
            max_clients: MAX_CONNECTIONS,
            protocol_id: PROTOCOL_VERSION.len() as u64,
            public_addresses: vec![self.address],
            authentication: ServerAuthentication::Unsecure,
        };

        let socket = std::net::UdpSocket::bind(self.address)
            .map_err(|e| format!("Failed to bind UDP socket: {}", e))?;

        socket
            .set_nonblocking(true)
            .map_err(|e| format!("Failed to set nonblocking: {}", e))?;

        NetcodeServerTransport::new(server_config, socket)
            .map_err(|e| format!("Failed to create server transport: {}", e))
    }

    /// Returns the private key (for sharing with clients).
    pub fn private_key(&self) -> [u8; 32] {
        self.private_key
    }

    /// Returns the server address.
    pub fn address(&self) -> SocketAddr {
        self.address
    }
}

/// Client authentication tokens.
pub struct ClientAuth {
    /// Server address.
    server_address: SocketAddr,
    /// Private key (must match server).
    private_key: [u8; 32],
}

impl ClientAuth {
    /// Creates new client authentication.
    pub fn new(server_address: SocketAddr, private_key: [u8; 32]) -> Self {
        Self {
            server_address,
            private_key,
        }
    }

    /// Creates authentication for localhost server.
    pub fn localhost() -> Self {
        Self::new(
            SocketAddr::from(([127, 0, 0, 1], DEFAULT_PORT)),
            generate_local_key(),
        )
    }

    /// Creates a NetcodeClientTransport for the client.
    pub fn create_transport(&self) -> Result<NetcodeClientTransport, String> {
        let socket = std::net::UdpSocket::bind("0.0.0.0:0")
            .map_err(|e| format!("Failed to bind client socket: {}", e))?;

        socket
            .set_nonblocking(true)
            .map_err(|e| format!("Failed to set nonblocking: {}", e))?;

        let client_id = generate_client_id();

        // Use unsecure authentication for development/LAN play
        let authentication = ClientAuthentication::Unsecure {
            protocol_id: PROTOCOL_VERSION.len() as u64,
            client_id,
            server_addr: self.server_address,
            user_data: None,
        };

        NetcodeClientTransport::new(Duration::ZERO, authentication, socket)
            .map_err(|e| format!("Failed to create client transport: {}", e))
    }

    /// Returns the server address.
    pub fn server_address(&self) -> SocketAddr {
        self.server_address
    }
}

/// Generates a client ID based on timestamp and random data.
fn generate_client_id() -> u64 {
    // Simple client ID generation
    // In production, use proper UUID or random generation
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;

    // Mix in some additional entropy
    now.wrapping_mul(0x5851F42E4C957F2D)
}

/// Generates a private key for localhost connections.
fn generate_local_key() -> [u8; 32] {
    // Use the same development key as the server
    ServerAuth::generate_dev_key()
}

/// Connection state tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionState {
    /// Not connected.
    #[default]
    Disconnected,
    /// Attempting to connect.
    Connecting,
    /// Connected and authenticated.
    Connected,
    /// Connection failed.
    Failed,
}

/// Tracks connection state and timing.
pub struct ConnectionTracker {
    /// Current state.
    state: ConnectionState,
    /// Time when connection attempt started.
    connect_start: Option<std::time::Instant>,
    /// Time when connection was established.
    connected_at: Option<std::time::Instant>,
    /// Disconnect reason (if any).
    disconnect_reason: Option<String>,
}

impl Default for ConnectionTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionTracker {
    /// Creates a new connection tracker.
    pub fn new() -> Self {
        Self {
            state: ConnectionState::Disconnected,
            connect_start: None,
            connected_at: None,
            disconnect_reason: None,
        }
    }

    /// Starts a connection attempt.
    pub fn start_connect(&mut self) {
        self.state = ConnectionState::Connecting;
        self.connect_start = Some(std::time::Instant::now());
        self.connected_at = None;
        self.disconnect_reason = None;
    }

    /// Marks connection as successful.
    pub fn mark_connected(&mut self) {
        self.state = ConnectionState::Connected;
        self.connected_at = Some(std::time::Instant::now());
    }

    /// Marks connection as failed.
    pub fn mark_failed(&mut self, reason: &str) {
        self.state = ConnectionState::Failed;
        self.disconnect_reason = Some(reason.to_string());
    }

    /// Marks as disconnected.
    pub fn mark_disconnected(&mut self, reason: Option<&str>) {
        self.state = ConnectionState::Disconnected;
        self.disconnect_reason = reason.map(|s| s.to_string());
    }

    /// Returns current connection state.
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    /// Returns whether connected.
    pub fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }

    /// Returns time connected (if connected).
    pub fn connected_duration(&self) -> Option<std::time::Duration> {
        self.connected_at.map(|t| t.elapsed())
    }

    /// Returns connection attempt duration.
    pub fn connecting_duration(&self) -> Option<std::time::Duration> {
        self.connect_start.map(|t| t.elapsed())
    }

    /// Returns true if connection attempt has timed out.
    pub fn has_timed_out(&self) -> bool {
        if self.state != ConnectionState::Connecting {
            return false;
        }

        self.connect_start
            .map(|t| t.elapsed() > std::time::Duration::from_millis(CONNECTION_TIMEOUT_MS))
            .unwrap_or(false)
    }

    /// Returns disconnect reason (if any).
    pub fn disconnect_reason(&self) -> Option<&str> {
        self.disconnect_reason.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_auth_creation() {
        let addr: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let auth = ServerAuth::new(addr);

        assert_eq!(auth.address(), addr);
        assert_ne!(auth.private_key(), [0u8; 32]);
    }

    #[test]
    fn test_client_auth_localhost() {
        let auth = ClientAuth::localhost();

        assert_eq!(
            auth.server_address(),
            SocketAddr::from(([127, 0, 0, 1], DEFAULT_PORT))
        );
    }

    #[test]
    fn test_connection_tracker() {
        let mut tracker = ConnectionTracker::new();

        assert_eq!(tracker.state(), ConnectionState::Disconnected);
        assert!(!tracker.is_connected());

        tracker.start_connect();
        assert_eq!(tracker.state(), ConnectionState::Connecting);
        assert!(!tracker.has_timed_out());

        tracker.mark_connected();
        assert_eq!(tracker.state(), ConnectionState::Connected);
        assert!(tracker.is_connected());

        tracker.mark_disconnected(Some("Test disconnect"));
        assert_eq!(tracker.state(), ConnectionState::Disconnected);
        assert_eq!(tracker.disconnect_reason(), Some("Test disconnect"));
    }
}

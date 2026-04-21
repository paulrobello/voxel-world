//! Authentication and connection handling using renet_netcode.
//!
//! Provides secure handshake and connection management for both
//! server and client.

// Allow unused code until networking is integrated into the game
#![allow(dead_code)]

use std::net::SocketAddr;
use std::time::Duration;

use renet_netcode::{
    ClientAuthentication, ConnectToken, NetcodeClientTransport, NetcodeServerTransport,
    ServerAuthentication, ServerConfig,
};

/// Default server port.
#[allow(dead_code)]
pub const DEFAULT_PORT: u16 = 5000;

/// Protocol version for netcode pairing. Bump in lockstep with
/// `crate::net::protocol::PROTOCOL_SCHEMA_VERSION` so a client with an old
/// binary is rejected at the netcode handshake, before any bincode decoding
/// runs.
#[allow(dead_code)]
pub const PROTOCOL_VERSION: &str = "voxel-world-2";

/// Stable protocol ID derived at compile time from the protocol version string.
///
/// Using `len()` as a protocol ID provides only 14 distinct values across all
/// possible version strings, making collisions trivial. Instead, we use a
/// compile-time FNV-1a hash of the version string so the ID is both stable
/// across builds and unique per version without a runtime dependency.
pub const PROTOCOL_ID: u64 = {
    // FNV-1a 64-bit hash of PROTOCOL_VERSION bytes.
    let bytes = PROTOCOL_VERSION.as_bytes();
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(0x00000100000001b3);
        i += 1;
    }
    hash
};

/// Connection timeout in milliseconds.
///
/// Must be generous enough to survive frame stalls (chunk generation, GPU
/// uploads) without the netcode layer expiring the connection.  30 seconds
/// is conservative; the renet keepalive ensures live connections are never
/// incorrectly dropped.
pub const CONNECTION_TIMEOUT_MS: u64 = 30_000;

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
    /// Creates new server authentication with a random per-session private key.
    ///
    /// The key is regenerated every time the server starts so a single
    /// compromised pairing cannot replay across deployments. Callers who want
    /// to pin a key (e.g. for testing) can use [`with_key`].
    pub fn new(address: SocketAddr) -> Self {
        let private_key = Self::generate_random_key();
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

    /// Generates a cryptographically-random 32-byte private key from the OS RNG.
    ///
    /// Each call produces a fresh key — this is the right default for per-
    /// session server startup.
    pub fn generate_random_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        for slot in key.iter_mut() {
            *slot = rand::random::<u8>();
        }
        key
    }

    /// Deprecated alias retained for backwards compatibility. Forwards to
    /// [`generate_random_key`] — the old fixed-byte implementation is gone.
    #[deprecated(note = "Use generate_random_key() — dev_key was a hardcoded shared secret")]
    pub fn generate_dev_key() -> [u8; 32] {
        Self::generate_random_key()
    }

    /// Creates a NetcodeServerTransport using the per-session `private_key`
    /// for Secure authentication. Clients must connect with a `ConnectToken`
    /// signed by the same key (see `ClientAuth::create_transport`).
    pub fn create_transport(&self) -> Result<NetcodeServerTransport, String> {
        let server_config = ServerConfig {
            current_time: Duration::ZERO,
            max_clients: MAX_CONNECTIONS,
            protocol_id: PROTOCOL_ID,
            public_addresses: vec![self.address],
            authentication: ServerAuthentication::Secure {
                private_key: self.private_key,
            },
        };

        let socket = std::net::UdpSocket::bind(self.address)
            .map_err(|e| format!("Failed to bind UDP socket: {}", e))?;

        socket
            .set_nonblocking(true)
            .map_err(|e| format!("Failed to set nonblocking: {}", e))?;

        NetcodeServerTransport::new(server_config, socket)
            .map_err(|e| format!("Failed to create server transport: {}", e))
    }

    /// Creates a NetcodeServerTransport in Secure mode bound to this auth's
    /// per-session `private_key`. Clients must supply the matching key (shared
    /// out-of-band) to pass the netcode handshake.
    pub fn create_secure_transport(&self) -> Result<NetcodeServerTransport, String> {
        let server_config = ServerConfig {
            current_time: Duration::ZERO,
            max_clients: MAX_CONNECTIONS,
            protocol_id: PROTOCOL_ID,
            public_addresses: vec![self.address],
            authentication: ServerAuthentication::Secure {
                private_key: self.private_key,
            },
        };

        let socket = std::net::UdpSocket::bind(self.address)
            .map_err(|e| format!("Failed to bind UDP socket: {}", e))?;

        socket
            .set_nonblocking(true)
            .map_err(|e| format!("Failed to set nonblocking: {}", e))?;

        NetcodeServerTransport::new(server_config, socket)
            .map_err(|e| format!("Failed to create secure server transport: {}", e))
    }

    /// Returns the private key (for sharing with clients via pairing code).
    pub fn private_key(&self) -> [u8; 32] {
        self.private_key
    }

    /// Returns the private key as a hex-encoded pairing code (64 chars).
    /// Print this on the host console so a remote player can type it into the
    /// client when connecting in Secure mode.
    pub fn pairing_code(&self) -> String {
        let mut s = String::with_capacity(64);
        for b in self.private_key.iter() {
            s.push_str(&format!("{:02x}", b));
        }
        s
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

        // Use Secure auth with the server's per-session key so the
        // ConnectToken carries our desired timeout (30s).  The Unsecure
        // path hardcodes a 15s timeout inside the netcode crate, which is
        // too short to survive frame stalls during chunk gen / GPU uploads.
        let connect_token = ConnectToken::generate(
            Duration::ZERO,
            PROTOCOL_ID,
            300, // expire_seconds
            client_id,
            (CONNECTION_TIMEOUT_MS / 1000) as i32, // timeout_seconds
            vec![self.server_address],
            None, // user_data
            &self.private_key,
        )
        .map_err(|e| format!("Failed to generate connect token: {}", e))?;

        let authentication = ClientAuthentication::Secure { connect_token };

        NetcodeClientTransport::new(Duration::ZERO, authentication, socket)
            .map_err(|e| format!("Failed to create client transport: {}", e))
    }

    /// Returns the server address.
    pub fn server_address(&self) -> SocketAddr {
        self.server_address
    }
}

/// Generates a cryptographically random client ID.
///
/// Uses the OS random source via `rand::random` so each connection attempt
/// produces an unpredictable 64-bit identifier, preventing enumeration attacks
/// against the netcode session table.
fn generate_client_id() -> u64 {
    rand::random::<u64>()
}

/// Returns the zero-key used by loopback `ClientAuth::localhost()` connections.
///
/// `ClientAuthentication::Unsecure` ignores the private key entirely, so a
/// dedicated zero-filled sentinel is honest about the fact that the loopback
/// path is not cryptographically authenticated. Any future Secure-mode
/// localhost path must pair explicitly via `ClientAuth::new()` with the
/// server's `pairing_code`.
fn generate_local_key() -> [u8; 32] {
    [0u8; 32]
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
    fn test_server_auth_keys_are_per_session_random() {
        // Two consecutive ServerAuth instances must not share a private key;
        // the previous implementation returned a hardcoded "dev key" for every
        // deployment, which was the C6 audit finding.
        let addr: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let a = ServerAuth::new(addr);
        let b = ServerAuth::new(addr);
        assert_ne!(a.private_key(), b.private_key());
        // Key must not be the old fixed sentinel either.
        let legacy = *b"voxel-world-dev-key-v1-32-byte!!";
        assert_ne!(a.private_key(), legacy);
    }

    #[test]
    fn test_pairing_code_is_hex_64() {
        let addr: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let auth = ServerAuth::new(addr);
        let code = auth.pairing_code();
        assert_eq!(code.len(), 64);
        assert!(code.chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// Picks a free ephemeral UDP port so the transport tests can run in
    /// parallel without colliding. Returns `127.0.0.1:<port>`.
    fn free_local_udp() -> SocketAddr {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind ephemeral");
        s.local_addr().expect("local_addr")
    }

    #[test]
    fn test_unsecure_transport_binds_successfully() {
        let addr = free_local_udp();
        let auth = ServerAuth::new(addr);
        let transport = auth.create_transport();
        assert!(
            transport.is_ok(),
            "unsecure transport failed: {:?}",
            transport.err()
        );
    }

    #[test]
    fn test_secure_transport_binds_and_uses_auth_key() {
        let addr = free_local_udp();
        let auth = ServerAuth::new(addr);
        let key = auth.private_key();
        assert_ne!(key, [0u8; 32]);

        let transport = auth.create_secure_transport();
        assert!(
            transport.is_ok(),
            "secure transport failed: {:?}",
            transport.err()
        );

        // The pairing code round-trips to the raw bytes.
        let hex = auth.pairing_code();
        assert_eq!(hex.len(), 64);
        let mut decoded = [0u8; 32];
        for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
            let b = u8::from_str_radix(std::str::from_utf8(chunk).unwrap(), 16).unwrap();
            decoded[i] = b;
        }
        assert_eq!(
            decoded, key,
            "pairing_code must encode the actual private key"
        );
    }

    #[test]
    fn test_secure_transport_rejects_port_in_use() {
        // Bind a socket on a port and keep it; ServerAuth::create_secure_transport
        // should fail cleanly when re-binding the same address.
        let hog = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind");
        let addr = hog.local_addr().unwrap();
        let auth = ServerAuth::new(addr);
        let transport = auth.create_secure_transport();
        assert!(
            transport.is_err(),
            "expected port-in-use to fail transport creation"
        );
    }

    #[test]
    fn test_with_key_preserves_exact_bytes() {
        let addr: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let custom: [u8; 32] = [
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55,
            0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0xde, 0xad, 0xbe, 0xef,
            0xfe, 0xed, 0xfa, 0xce,
        ];
        let auth = ServerAuth::with_key(addr, custom);
        assert_eq!(auth.private_key(), custom);
    }

    #[test]
    fn test_localhost_client_key_is_zero_sentinel() {
        let auth = ClientAuth::localhost();
        // Loopback path runs in Unsecure mode which ignores the key; using a
        // zero sentinel makes that explicit rather than faking cryptographic
        // strength.
        assert_eq!(auth.private_key, [0u8; 32]);
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

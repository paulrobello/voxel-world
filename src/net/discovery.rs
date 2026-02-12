//! LAN server discovery protocol.
//!
//! UDP broadcast-based server discovery on port 5001:
//! - Clients broadcast discovery requests to find servers
//! - Servers respond with announcements containing server info
//! - Stale entries (5s timeout) are removed from discovery list

use std::collections::HashMap;
use std::io::{self, ErrorKind};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Default port for LAN discovery broadcasts.
pub const DISCOVERY_PORT: u16 = 5001;

/// How long to wait before removing a server from the list (in seconds).
pub const SERVER_TIMEOUT_SECS: u64 = 5;

/// Maximum size for discovery packets.
pub const MAX_PACKET_SIZE: usize = 1024;

/// Magic bytes to identify discovery packets.
pub const DISCOVERY_MAGIC: &[u8; 4] = b"VXLD";

/// Packet type identifiers.
#[repr(u8)]
enum PacketType {
    /// Client discovery request (broadcast).
    DiscoveryRequest = 0x01,
    /// Server announcement response.
    ServerAnnouncement = 0x02,
}

/// Server announcement sent in response to discovery requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerAnnouncement {
    /// Game server port (the actual game server, not discovery port).
    pub game_port: u16,
    /// Human-readable server name.
    pub server_name: String,
    /// Current player count.
    pub player_count: u8,
    /// Maximum player capacity.
    pub max_players: u8,
}

/// A discovered server entry.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DiscoveredServer {
    /// Server address (IP + game port).
    pub address: SocketAddr,
    /// Human-readable server name.
    pub server_name: String,
    /// Game server port.
    pub game_port: u16,
    /// Current player count.
    pub player_count: u8,
    /// Maximum player capacity.
    pub max_players: u8,
    /// When this server was last seen.
    pub last_seen: Instant,
}

/// LAN discovery client that listens for server announcements.
pub struct LanDiscovery {
    socket: UdpSocket,
    /// Discovered servers indexed by address string.
    servers: HashMap<String, DiscoveredServer>,
    /// When we last sent a discovery request.
    last_request: Option<Instant>,
    /// Interval between discovery requests.
    request_interval: Duration,
}

impl LanDiscovery {
    /// Creates a new LAN discovery client.
    pub fn new() -> io::Result<Self> {
        let socket = UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT))?;
        socket.set_nonblocking(true)?;
        socket.set_broadcast(true)?;

        Ok(Self {
            socket,
            servers: HashMap::new(),
            last_request: None,
            request_interval: Duration::from_secs(2),
        })
    }

    /// Sends a discovery request broadcast.
    pub fn send_discovery_request(&mut self) -> io::Result<()> {
        let broadcast_addr = SocketAddrV4::new(Ipv4Addr::BROADCAST, DISCOVERY_PORT);

        // Build packet: magic + packet type
        let mut packet = Vec::with_capacity(5);
        packet.extend_from_slice(DISCOVERY_MAGIC);
        packet.push(PacketType::DiscoveryRequest as u8);

        self.socket.send_to(&packet, broadcast_addr)?;
        self.last_request = Some(Instant::now());

        Ok(())
    }

    /// Processes incoming packets and updates server list.
    /// Should be called every frame.
    pub fn update(&mut self) {
        let now = Instant::now();

        // Send discovery request if interval elapsed
        let should_request = self
            .last_request
            .map(|last| now.duration_since(last) >= self.request_interval)
            .unwrap_or(true);

        if should_request {
            let _ = self.send_discovery_request();
        }

        // Receive pending packets
        let mut buf = [0u8; MAX_PACKET_SIZE];
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((len, addr)) => {
                    if let Some(server) = Self::parse_announcement(&buf[..len], addr) {
                        let key = format!("{}:{}", server.address.ip(), server.game_port);
                        self.servers.insert(key, server);
                    }
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    // No more packets
                    break;
                }
                Err(e) => {
                    eprintln!("[Discovery] Receive error: {}", e);
                    break;
                }
            }
        }

        // Remove stale servers
        let timeout = Duration::from_secs(SERVER_TIMEOUT_SECS);
        self.servers
            .retain(|_, server| now.duration_since(server.last_seen) < timeout);
    }

    /// Parses a server announcement packet.
    fn parse_announcement(data: &[u8], addr: SocketAddr) -> Option<DiscoveredServer> {
        // Check minimum length
        if data.len() < 5 {
            return None;
        }

        // Check magic
        if &data[0..4] != DISCOVERY_MAGIC {
            return None;
        }

        // Check packet type
        if data[4] != PacketType::ServerAnnouncement as u8 {
            return None;
        }

        // Deserialize announcement using bincode 2.0 serde API
        let (announcement, _): (ServerAnnouncement, usize) =
            bincode::serde::decode_from_slice(&data[5..], bincode::config::standard()).ok()?;

        // Build full address with game port
        let game_addr = SocketAddr::new(addr.ip(), announcement.game_port);

        Some(DiscoveredServer {
            address: game_addr,
            server_name: announcement.server_name,
            game_port: announcement.game_port,
            player_count: announcement.player_count,
            max_players: announcement.max_players,
            last_seen: Instant::now(),
        })
    }

    /// Returns the list of discovered servers.
    pub fn get_servers(&self) -> Vec<DiscoveredServer> {
        self.servers.values().cloned().collect()
    }

    /// Clears the server list.
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.servers.clear();
    }
}

/// Server-side discovery responder that broadcasts server presence.
pub struct DiscoveryResponder {
    socket: UdpSocket,
    server_name: String,
    game_port: u16,
    max_players: u8,
}

impl DiscoveryResponder {
    /// Creates a new discovery responder for a server.
    pub fn new(server_name: String, game_port: u16, max_players: u8) -> io::Result<Self> {
        let socket = UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT))?;
        socket.set_nonblocking(true)?;

        Ok(Self {
            socket,
            server_name,
            game_port,
            max_players,
        })
    }

    /// Updates the player count displayed in announcements.
    #[allow(dead_code)]
    pub fn set_player_count(&mut self, _count: u8) {
        // Player count is set in respond_to_requests
    }

    /// Processes incoming discovery requests and responds.
    /// Should be called every frame.
    pub fn update(&self, player_count: u8) {
        let mut buf = [0u8; MAX_PACKET_SIZE];

        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((len, addr)) => {
                    if self.is_discovery_request(&buf[..len]) {
                        if let Err(e) = self.send_announcement(addr, player_count) {
                            eprintln!("[Discovery] Failed to send announcement: {}", e);
                        }
                    }
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    // No more packets
                    break;
                }
                Err(e) => {
                    eprintln!("[Discovery] Receive error: {}", e);
                    break;
                }
            }
        }
    }

    /// Checks if a packet is a valid discovery request.
    fn is_discovery_request(&self, data: &[u8]) -> bool {
        if data.len() < 5 {
            return false;
        }

        &data[0..4] == DISCOVERY_MAGIC && data[4] == PacketType::DiscoveryRequest as u8
    }

    /// Sends a server announcement to a client.
    fn send_announcement(&self, addr: SocketAddr, player_count: u8) -> io::Result<()> {
        let announcement = ServerAnnouncement {
            game_port: self.game_port,
            server_name: self.server_name.clone(),
            player_count,
            max_players: self.max_players,
        };

        // Serialize using bincode 2.0 serde API
        let serialized = bincode::serde::encode_to_vec(&announcement, bincode::config::standard())
            .map_err(|_| {
                io::Error::new(ErrorKind::InvalidData, "Failed to serialize announcement")
            })?;

        // Build packet: magic + packet type + payload
        let mut packet = Vec::with_capacity(5 + serialized.len());
        packet.extend_from_slice(DISCOVERY_MAGIC);
        packet.push(PacketType::ServerAnnouncement as u8);
        packet.extend_from_slice(&serialized);

        self.socket.send_to(&packet, addr)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_structure() {
        // Verify magic bytes
        assert_eq!(DISCOVERY_MAGIC.len(), 4);
        assert_eq!(DISCOVERY_MAGIC, b"VXLD");
    }

    #[test]
    fn test_server_announcement_serialization() {
        let announcement = ServerAnnouncement {
            game_port: 5000,
            server_name: "Test Server".to_string(),
            player_count: 2,
            max_players: 4,
        };

        let serialized =
            bincode::serde::encode_to_vec(&announcement, bincode::config::standard()).unwrap();
        let (deserialized, _): (ServerAnnouncement, usize) =
            bincode::serde::decode_from_slice(&serialized, bincode::config::standard()).unwrap();

        assert_eq!(deserialized.game_port, 5000);
        assert_eq!(deserialized.server_name, "Test Server");
        assert_eq!(deserialized.player_count, 2);
        assert_eq!(deserialized.max_players, 4);
    }
}

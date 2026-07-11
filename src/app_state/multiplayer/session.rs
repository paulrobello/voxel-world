//! Session/server metadata, extracted from `MultiplayerState`
//! (ARC-002 phase 8).
//!
//! Holds the descriptive state that isn't part of the connection lifecycle
//! proper (`mode`/`server`/`client` stay on the facade): the host's display
//! name + player cap, the last-measured ping, and the local player's name.

/// Host/server descriptive metadata + the local player's display name.
///
/// Extracted from `MultiplayerState` (ARC-002). The facade holds this as
/// `metadata: SessionMetadata` and forwards the accessors.
pub struct SessionMetadata {
    server_name: String,
    max_players: u8,
    ping_ms: Option<u32>,
    local_player_name: String,
}

impl SessionMetadata {
    /// Creates default metadata (empty server name, 4-player cap, no ping,
    /// "Player" as the local name).
    pub fn new() -> Self {
        Self {
            server_name: String::new(),
            max_players: 4,
            ping_ms: None,
            local_player_name: "Player".to_string(),
        }
    }

    /// Returns the host's display name (for LAN discovery announcements).
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// Returns a mutable handle to the server name (set on `start_host`,
    /// cleared on `stop_host`).
    pub fn server_name_mut(&mut self) -> &mut String {
        &mut self.server_name
    }

    /// Returns the maximum player cap.
    pub fn max_players(&self) -> u8 {
        self.max_players
    }

    /// Returns the last-measured ping, if any.
    pub fn ping_ms(&self) -> Option<u32> {
        self.ping_ms
    }

    /// Sets the last-measured ping (or `None` to clear on disconnect).
    pub fn set_ping_ms(&mut self, ping: Option<u32>) {
        self.ping_ms = ping;
    }

    /// Returns the local player's display name.
    pub fn local_player_name(&self) -> &str {
        &self.local_player_name
    }

    /// Sets the local player's display name.
    pub fn set_local_player_name(&mut self, name: String) {
        self.local_player_name = name;
    }
}

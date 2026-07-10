//! LAN discovery state, extracted from `MultiplayerState` (ARC-002 phase 3).
//!
//! Two halves: client-side discovery (finding servers on the LAN) and a
//! server-side responder (advertising this host's presence). The connection
//! lifecycle creates/clears the responder (`start_host` / `stop_host`) and
//! `update()` ticks both halves each frame.

use crate::net::{DiscoveredServer, DiscoveryResponder, LanDiscovery};

/// Client-side LAN discovery + server-side discovery responder.
///
/// Extracted from `MultiplayerState` (ARC-002). The host holds this as
/// `discovery: DiscoveryState` and forwards the public accessors; the responder
/// is wired from the connection-lifecycle methods.
pub struct DiscoveryState {
    /// Client-side LAN discovery (for finding servers).
    discovery: Option<LanDiscovery>,
    /// Server-side discovery responder (for advertising presence).
    responder: Option<DiscoveryResponder>,
}

impl DiscoveryState {
    /// Creates an idle discovery state (no client scan, no responder).
    pub fn new() -> Self {
        Self {
            discovery: None,
            responder: None,
        }
    }

    /// Starts the client-side LAN scan.
    pub fn start_client(&mut self) -> Result<(), String> {
        if self.discovery.is_none() {
            self.discovery =
                Some(LanDiscovery::new().map_err(|e| format!("Failed to start discovery: {}", e))?);
        }
        Ok(())
    }

    /// Stops the client-side LAN scan.
    pub fn stop_client(&mut self) {
        self.discovery = None;
    }

    /// Returns servers discovered by the client-side scan.
    pub fn discovered_servers(&self) -> Vec<DiscoveredServer> {
        self.discovery
            .as_ref()
            .map(|d| d.get_servers())
            .unwrap_or_default()
    }

    /// Installs the server-side responder (called from `start_host`).
    pub fn set_responder(&mut self, responder: DiscoveryResponder) {
        self.responder = Some(responder);
    }

    /// Clears the server-side responder (called from `stop_host`).
    pub fn clear_responder(&mut self) {
        self.responder = None;
    }

    /// Ticks the responder with the current player count (server-side, each frame).
    pub fn update_responder(&mut self, player_count: u8) {
        if let Some(ref responder) = self.responder {
            responder.update(player_count);
        }
    }

    /// Ticks the client-side discovery scanner (each frame).
    pub fn update_client(&mut self) {
        if let Some(ref mut discovery) = self.discovery {
            discovery.update();
        }
    }
}

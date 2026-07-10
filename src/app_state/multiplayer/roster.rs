//! Remote-player roster + host-side player-count/name tracking, extracted from
//! `MultiplayerState` (ARC-002 phase 2).
//!
//! Holds the list of remote players (for rendering / interpolation) and the
//! host-maintained count + name list (drives the discovery responder and the UI).
//! The server-message dispatcher mutates this on `PlayerJoined` / `PlayerLeft`
//! via [`PlayerRoster::add`] / [`PlayerRoster::remove`], and `update()` re-syncs
//! the count via [`PlayerRoster::sync_count`].
//!
//! The view-layer projections (minimap markers, 3D labels) intentionally stay on
//! [`super::MultiplayerState`] so this state module does not depend on
//! `crate::ui::minimap`.

use crate::net::RemotePlayer;

/// Remote players + the host-side roster (player count + names).
///
/// Extracted from `MultiplayerState` (ARC-002). The host holds this as
/// `roster: PlayerRoster` and forwards the public accessors.
pub struct PlayerRoster {
    remote_players: Vec<RemotePlayer>,
    player_count: u8,
    player_names: Vec<String>,
}

impl PlayerRoster {
    /// Creates a fresh roster in the host-only state (count 1, name "Host"),
    /// matching the original `MultiplayerState::new` / `stop_host` reset.
    pub fn new() -> Self {
        Self {
            remote_players: Vec::new(),
            player_count: 1,
            player_names: vec!["Host".to_string()],
        }
    }

    /// Returns the remote players (for view-layer projections + interpolation).
    pub fn remote_players(&self) -> &[RemotePlayer] {
        &self.remote_players
    }

    /// Returns the remote players mutably (for in-place updates found by id).
    pub fn remote_players_mut(&mut self) -> &mut [RemotePlayer] {
        &mut self.remote_players
    }

    /// Adds a remote player (on `PlayerJoined` / a fresh `PlayerState`).
    pub fn add(&mut self, player: RemotePlayer) {
        self.remote_players.push(player);
    }

    /// Removes a remote player by id (on `PlayerLeft`).
    pub fn remove(&mut self, player_id: u64) {
        self.remote_players.retain(|p| p.player_id != player_id);
    }

    /// Advances interpolation for every remote player (call every frame).
    pub fn interpolate(&mut self) {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        for remote in &mut self.remote_players {
            remote.interpolate(current_time);
        }
    }

    /// Returns the current player count.
    pub fn player_count(&self) -> u8 {
        self.player_count
    }

    /// Re-derives the player count from the remote-player list + host.
    /// Called by the host each tick after dispatching server messages.
    pub fn sync_count(&mut self) {
        self.player_count = (self.remote_players.len() + 1) as u8;
    }

    /// Returns the connected player names.
    pub fn player_names(&self) -> &[String] {
        &self.player_names
    }

    /// Resets to the host-only state (used by `stop_host`).
    pub fn reset_to_host(&mut self) {
        self.player_count = 1;
        self.player_names = vec!["Host".to_string()];
    }
}

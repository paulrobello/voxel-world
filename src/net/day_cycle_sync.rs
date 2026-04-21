//! Day cycle pause synchronization for multiplayer.
//!
//! Provides server-authoritative day/night cycle synchronization with:
//! - **Pause state broadcasting**: When the day cycle is paused/resumed
//! - **Time synchronization**: Current time of day is sent with pause state
//!
//! # Architecture
//!
//! In single-player mode, day cycle is managed locally.
//! In multiplayer:
//! - **Server**: Controls day cycle pause state authoritatively
//! - **Client**: Receives and applies pause state from server
//!
//! # Usage
//!
//! ```ignore
//! // Server-side: When player toggles day cycle pause (e.g., H key)
//! if multiplayer.is_hosting() {
//!     multiplayer.broadcast_day_cycle_pause(paused, time_of_day);
//! }
//!
//! // Client-side: Apply pending pause state from server
//! if let Some(pause) = multiplayer.take_pending_day_cycle_pause() {
//!     sim.day_cycle_paused = pause.paused;
//!     sim.time_of_day = pause.time_of_day;
//! }
//! ```

// Allow dead code since these methods are public API intended for future use
#![allow(dead_code)]

/// Statistics for monitoring day cycle sync.
#[derive(Debug, Clone, Default)]
pub struct DayCycleSyncStats {
    /// Total pause state changes broadcast.
    pub pause_broadcasts: u64,
    /// Total pause state changes received.
    pub pause_received: u64,
}

/// Tracks day cycle state for server-authoritative sync.
///
/// The server uses this to track and broadcast day cycle changes.
pub struct DayCycleSync {
    /// Current pause state.
    pub paused: bool,
    /// Current time of day (0.0-1.0).
    pub time_of_day: f32,
    /// Statistics for monitoring.
    stats: DayCycleSyncStats,
}

impl Default for DayCycleSync {
    fn default() -> Self {
        Self::new()
    }
}

impl DayCycleSync {
    /// Creates a new day cycle sync tracker with a morning default. Prefer
    /// [`with_initial_state`] when you have the actual world-save state in
    /// hand — the bare `new()` only makes sense before metadata is loaded.
    pub fn new() -> Self {
        Self {
            paused: false,
            time_of_day: 0.25, // Default to morning; overwritten by persisted state.
            stats: DayCycleSyncStats::default(),
        }
    }

    /// Creates a tracker seeded from persisted world metadata so a host
    /// restart doesn't snap remote clients back to the 0.25 default. Use this
    /// at server startup with the values loaded from `level.dat`.
    pub fn with_initial_state(paused: bool, time_of_day: f32) -> Self {
        Self {
            paused,
            time_of_day,
            stats: DayCycleSyncStats::default(),
        }
    }

    /// Updates the pause state and returns `true` if the pause boolean
    /// actually toggled — the server should only broadcast
    /// `DayCyclePauseChanged` in that case.
    ///
    /// Previously this also returned `true` on any micro-drift of
    /// `time_of_day`, which made every per-tick call to `set_paused` emit a
    /// broadcast. The time value is still captured and carried with the next
    /// real pause change, but it no longer drives the "changed" signal.
    pub fn set_paused(&mut self, paused: bool, time_of_day: f32) -> bool {
        self.time_of_day = time_of_day;
        if self.paused != paused {
            self.paused = paused;
            self.stats.pause_broadcasts += 1;
            true
        } else {
            false
        }
    }

    /// Applies a received pause state from the server (client-side).
    pub fn apply_from_server(&mut self, paused: bool, time_of_day: f32) {
        self.paused = paused;
        self.time_of_day = time_of_day;
        self.stats.pause_received += 1;
    }

    /// Returns statistics for monitoring.
    pub fn stats(&self) -> &DayCycleSyncStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::protocol::{DayCyclePauseChanged, ServerMessage};
    use bincode;

    fn make_pause_msg(paused: bool, time_of_day: f32) -> DayCyclePauseChanged {
        DayCyclePauseChanged {
            paused,
            time_of_day,
        }
    }

    #[test]
    fn test_day_cycle_sync_new() {
        let sync = DayCycleSync::new();
        assert!(!sync.paused);
        assert!((sync.time_of_day - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn test_day_cycle_sync_with_initial_state() {
        let sync = DayCycleSync::with_initial_state(true, 0.83);
        assert!(sync.paused);
        assert!((sync.time_of_day - 0.83).abs() < f32::EPSILON);
    }

    /// M14 round-trip: `WorldMetadata` must preserve `time_of_day` and
    /// `day_cycle_paused` across a save/load so the host doesn't reset
    /// every player to morning on restart.
    #[test]
    fn test_world_metadata_round_trips_day_cycle_fields() {
        use crate::storage::metadata::WorldMetadata;

        let dir = std::env::temp_dir().join(format!("vw_daycycle_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("level.dat");

        let saved = WorldMetadata {
            seed: 42,
            spawn_pos: [0.0, 64.0, 0.0],
            version: 1,
            time_of_day: 0.83, // late-ish sunset
            day_cycle_paused: true,
            world_gen: crate::config::WorldGenType::Normal,
            measurement_markers: vec![],
        };
        saved.save(&path).expect("save");

        let loaded = WorldMetadata::load(&path).expect("load");
        assert!((loaded.time_of_day - 0.83).abs() < 1e-6);
        assert!(loaded.day_cycle_paused);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_set_paused_changed() {
        let mut sync = DayCycleSync::new();

        // Change pause state
        let changed = sync.set_paused(true, 0.5);
        assert!(changed);
        assert!(sync.paused);
        assert!((sync.time_of_day - 0.5).abs() < f32::EPSILON);

        // Same state - no change
        let changed = sync.set_paused(true, 0.5);
        assert!(!changed);
    }

    #[test]
    fn test_apply_from_server() {
        let mut sync = DayCycleSync::new();

        // Apply pause from server
        sync.apply_from_server(true, 0.75);
        assert!(sync.paused);
        assert!((sync.time_of_day - 0.75).abs() < f32::EPSILON);

        // Apply resume from server
        sync.apply_from_server(false, 0.8);
        assert!(!sync.paused);
        assert!((sync.time_of_day - 0.8).abs() < f32::EPSILON);
    }

    /// Integration test: Verifies day cycle pause sync via ServerMessage protocol.
    ///
    /// This test simulates the full multiplayer day cycle pause sync flow:
    /// 1. Server broadcasts pause state change (e.g., player pressed H)
    /// 2. Message is serialized as ServerMessage
    /// 3. Client deserializes and receives the pause state
    /// 4. Client applies the pause state to its simulation
    #[test]
    fn test_day_cycle_pause_via_server_message_protocol() {
        // === Setup: Create server and client state ===
        let mut server_sync = DayCycleSync::new();
        let mut client_sync = DayCycleSync::new();

        // Initial state: both running
        assert!(!server_sync.paused);
        assert!(!client_sync.paused);

        // === Phase 1: Server pauses day cycle (e.g., player pressed H) ===
        let changed = server_sync.set_paused(true, 0.5);
        assert!(changed, "Server should detect state change");
        assert!(server_sync.paused);

        // === Phase 2: Create and serialize ServerMessage ===
        let pause_msg = make_pause_msg(true, 0.5);
        let server_msg = ServerMessage::DayCyclePauseChanged(pause_msg);

        let encoded = bincode::serde::encode_to_vec(&server_msg, bincode::config::standard())
            .expect("Failed to encode ServerMessage");

        // === Phase 3: Client receives and decodes message ===
        let (decoded, _): (ServerMessage, usize) =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .expect("Failed to decode ServerMessage");

        // === Phase 4: Client applies pause state ===
        match decoded {
            ServerMessage::DayCyclePauseChanged(pause) => {
                assert!(pause.paused, "Client should receive paused=true");
                assert!(
                    (pause.time_of_day - 0.5).abs() < f32::EPSILON,
                    "Client should receive time_of_day=0.5"
                );

                // Apply to client simulation
                client_sync.apply_from_server(pause.paused, pause.time_of_day);
            }
            _ => panic!("Expected DayCyclePauseChanged message"),
        }

        // === Verify: Client state matches server ===
        assert_eq!(
            server_sync.paused, client_sync.paused,
            "Client pause state should match server"
        );
        assert!(
            (server_sync.time_of_day - client_sync.time_of_day).abs() < f32::EPSILON,
            "Client time_of_day should match server"
        );
    }

    /// Integration test: Verifies day cycle pause sync with resume.
    ///
    /// Tests the complete pause -> resume cycle.
    #[test]
    fn test_day_cycle_pause_and_resume() {
        let mut server_sync = DayCycleSync::new();
        let mut client_sync = DayCycleSync::new();

        // === Phase 1: Pause ===
        server_sync.set_paused(true, 0.3);
        let pause_msg = ServerMessage::DayCyclePauseChanged(make_pause_msg(true, 0.3));
        let encoded = bincode::serde::encode_to_vec(&pause_msg, bincode::config::standard())
            .expect("Failed to encode");

        let (decoded, _): (ServerMessage, usize) =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .expect("Failed to decode");

        if let ServerMessage::DayCyclePauseChanged(pause) = decoded {
            client_sync.apply_from_server(pause.paused, pause.time_of_day);
        }

        assert!(server_sync.paused);
        assert!(client_sync.paused);
        assert!((server_sync.time_of_day - client_sync.time_of_day).abs() < f32::EPSILON);

        // === Phase 2: Resume ===
        server_sync.set_paused(false, 0.35);
        let resume_msg = ServerMessage::DayCyclePauseChanged(make_pause_msg(false, 0.35));
        let encoded = bincode::serde::encode_to_vec(&resume_msg, bincode::config::standard())
            .expect("Failed to encode");

        let (decoded, _): (ServerMessage, usize) =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .expect("Failed to decode");

        if let ServerMessage::DayCyclePauseChanged(pause) = decoded {
            client_sync.apply_from_server(pause.paused, pause.time_of_day);
        }

        assert!(!server_sync.paused);
        assert!(!client_sync.paused);
        assert!((server_sync.time_of_day - 0.35).abs() < f32::EPSILON);
        assert!((client_sync.time_of_day - 0.35).abs() < f32::EPSILON);
    }

    /// Integration test: Verifies day cycle sync is visible to all connected players.
    ///
    /// This test simulates the scenario where one player pauses time
    /// and all other players see the same pause state.
    #[test]
    fn test_day_cycle_sync_visible_to_all_players() {
        // === Setup: Simulate server and 3 connected clients ===
        let mut server_sync = DayCycleSync::new();
        let mut client1_sync = DayCycleSync::new();
        let mut client2_sync = DayCycleSync::new();
        let mut client3_sync = DayCycleSync::new();

        // === Phase 1: Server pauses day cycle ===
        let changed = server_sync.set_paused(true, 0.6);
        assert!(changed);

        // Create message
        let pause_msg = ServerMessage::DayCyclePauseChanged(make_pause_msg(true, 0.6));

        // Serialize once (simulates broadcast)
        let encoded = bincode::serde::encode_to_vec(&pause_msg, bincode::config::standard())
            .expect("Failed to encode");

        // === Phase 2: All clients receive the message ===
        for client_sync in &mut [&mut client1_sync, &mut client2_sync, &mut client3_sync] {
            let (decoded, _): (ServerMessage, usize) =
                bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                    .expect("Failed to decode");

            if let ServerMessage::DayCyclePauseChanged(pause) = decoded {
                client_sync.apply_from_server(pause.paused, pause.time_of_day);
            }
        }

        // === Verify: All clients have same state as server ===
        assert!(client1_sync.paused, "Client 1 should be paused");
        assert!(client2_sync.paused, "Client 2 should be paused");
        assert!(client3_sync.paused, "Client 3 should be paused");

        assert!((client1_sync.time_of_day - 0.6).abs() < f32::EPSILON);
        assert!((client2_sync.time_of_day - 0.6).abs() < f32::EPSILON);
        assert!((client3_sync.time_of_day - 0.6).abs() < f32::EPSILON);

        // === Phase 3: Server resumes day cycle ===
        let changed = server_sync.set_paused(false, 0.65);
        assert!(changed);

        let resume_msg = ServerMessage::DayCyclePauseChanged(make_pause_msg(false, 0.65));
        let encoded = bincode::serde::encode_to_vec(&resume_msg, bincode::config::standard())
            .expect("Failed to encode");

        // All clients receive resume
        for client_sync in &mut [&mut client1_sync, &mut client2_sync, &mut client3_sync] {
            let (decoded, _): (ServerMessage, usize) =
                bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                    .expect("Failed to decode");

            if let ServerMessage::DayCyclePauseChanged(pause) = decoded {
                client_sync.apply_from_server(pause.paused, pause.time_of_day);
            }
        }

        // === Verify: All clients resumed ===
        assert!(!client1_sync.paused, "Client 1 should be running");
        assert!(!client2_sync.paused, "Client 2 should be running");
        assert!(!client3_sync.paused, "Client 3 should be running");

        assert!((client1_sync.time_of_day - 0.65).abs() < f32::EPSILON);
        assert!((client2_sync.time_of_day - 0.65).abs() < f32::EPSILON);
        assert!((client3_sync.time_of_day - 0.65).abs() < f32::EPSILON);

        println!("Successfully verified day cycle sync across 3 clients: pause -> resume");
    }

    /// Integration test: Verifies time of day syncs with pause state.
    ///
    /// This ensures that when the day cycle is paused, the exact time
    /// is also synchronized so all clients see the same sky state.
    #[test]
    fn test_time_of_day_syncs_with_pause() {
        let mut server_sync = DayCycleSync::new();
        let mut client_sync = DayCycleSync::new();

        // Set specific time for testing
        let test_time = 0.12345;

        server_sync.set_paused(true, test_time);
        let pause_msg = ServerMessage::DayCyclePauseChanged(make_pause_msg(true, test_time));
        let encoded = bincode::serde::encode_to_vec(&pause_msg, bincode::config::standard())
            .expect("Failed to encode");

        let (decoded, _): (ServerMessage, usize) =
            bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                .expect("Failed to decode");

        if let ServerMessage::DayCyclePauseChanged(pause) = decoded {
            // Verify exact time is preserved
            assert!(
                (pause.time_of_day - test_time).abs() < f32::EPSILON,
                "Time should be exactly preserved"
            );
            client_sync.apply_from_server(pause.paused, pause.time_of_day);
        }

        // Client should have exact same time
        assert!(
            (client_sync.time_of_day - test_time).abs() < f32::EPSILON,
            "Client time should match server exactly"
        );
    }

    /// Integration test: Verifies multiple pause/resume cycles.
    ///
    /// Tests that the sync works correctly over multiple state changes.
    #[test]
    fn test_multiple_pause_resume_cycles() {
        let mut server_sync = DayCycleSync::new();
        let mut client_sync = DayCycleSync::new();

        // Simulate multiple pause/resume cycles (player pressing H multiple times)
        let cycles = vec![
            (true, 0.1),
            (false, 0.15),
            (true, 0.2),
            (false, 0.25),
            (true, 0.3),
            (false, 0.35),
        ];

        for (expected_paused, expected_time) in &cycles {
            // Server changes state
            server_sync.set_paused(*expected_paused, *expected_time);

            // Broadcast to client
            let msg = ServerMessage::DayCyclePauseChanged(make_pause_msg(
                *expected_paused,
                *expected_time,
            ));
            let encoded = bincode::serde::encode_to_vec(&msg, bincode::config::standard())
                .expect("Failed to encode");

            let (decoded, _): (ServerMessage, usize) =
                bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                    .expect("Failed to decode");

            if let ServerMessage::DayCyclePauseChanged(pause) = decoded {
                client_sync.apply_from_server(pause.paused, pause.time_of_day);
            }

            // Verify sync
            assert_eq!(
                server_sync.paused, client_sync.paused,
                "Pause state should match at cycle ({}, {})",
                expected_paused, expected_time
            );
            assert!(
                (server_sync.time_of_day - client_sync.time_of_day).abs() < f32::EPSILON,
                "Time should match at cycle ({}, {})",
                expected_paused,
                expected_time
            );
        }

        println!("Successfully verified {} pause/resume cycles", cycles.len());
    }

    /// Integration test: Verifies day/night pause sync produces identical state.
    ///
    /// This test verifies the complete P1 sync point:
    /// "Time syncs when one player pauses/unpauses"
    #[test]
    fn test_day_night_pause_sync_produces_identical_state() {
        // === Setup ===
        let mut server_sync = DayCycleSync::new();
        let mut client1_sync = DayCycleSync::new();
        let mut client2_sync = DayCycleSync::new();

        // Helper to sync server state to all clients
        fn broadcast_pause(server: &DayCycleSync, clients: &mut [&mut DayCycleSync]) {
            let msg = ServerMessage::DayCyclePauseChanged(DayCyclePauseChanged {
                paused: server.paused,
                time_of_day: server.time_of_day,
            });
            let encoded = bincode::serde::encode_to_vec(&msg, bincode::config::standard())
                .expect("Failed to encode");

            for client in clients {
                let (decoded, _): (ServerMessage, usize) =
                    bincode::serde::decode_from_slice(&encoded, bincode::config::standard())
                        .expect("Failed to decode");

                if let ServerMessage::DayCyclePauseChanged(pause) = decoded {
                    client.apply_from_server(pause.paused, pause.time_of_day);
                }
            }
        }

        // === Test Case 1: Pause at dawn ===
        server_sync.set_paused(true, 0.25); // Dawn
        broadcast_pause(&server_sync, &mut [&mut client1_sync, &mut client2_sync]);

        assert!(server_sync.paused);
        assert!(client1_sync.paused);
        assert!(client2_sync.paused);
        assert!((server_sync.time_of_day - 0.25).abs() < f32::EPSILON);
        assert!((client1_sync.time_of_day - 0.25).abs() < f32::EPSILON);
        assert!((client2_sync.time_of_day - 0.25).abs() < f32::EPSILON);

        // === Test Case 2: Resume at noon ===
        server_sync.set_paused(false, 0.5); // Noon
        broadcast_pause(&server_sync, &mut [&mut client1_sync, &mut client2_sync]);

        assert!(!server_sync.paused);
        assert!(!client1_sync.paused);
        assert!(!client2_sync.paused);
        assert!((server_sync.time_of_day - 0.5).abs() < f32::EPSILON);
        assert!((client1_sync.time_of_day - 0.5).abs() < f32::EPSILON);
        assert!((client2_sync.time_of_day - 0.5).abs() < f32::EPSILON);

        // === Test Case 3: Pause at dusk ===
        server_sync.set_paused(true, 0.75); // Dusk
        broadcast_pause(&server_sync, &mut [&mut client1_sync, &mut client2_sync]);

        assert!(server_sync.paused);
        assert!(client1_sync.paused);
        assert!(client2_sync.paused);
        assert!((server_sync.time_of_day - 0.75).abs() < f32::EPSILON);
        assert!((client1_sync.time_of_day - 0.75).abs() < f32::EPSILON);
        assert!((client2_sync.time_of_day - 0.75).abs() < f32::EPSILON);

        // === Test Case 4: Resume at midnight ===
        server_sync.set_paused(false, 0.0); // Midnight
        broadcast_pause(&server_sync, &mut [&mut client1_sync, &mut client2_sync]);

        assert!(!server_sync.paused);
        assert!(!client1_sync.paused);
        assert!(!client2_sync.paused);
        assert!((server_sync.time_of_day - 0.0).abs() < f32::EPSILON);
        assert!((client1_sync.time_of_day - 0.0).abs() < f32::EPSILON);
        assert!((client2_sync.time_of_day - 0.0).abs() < f32::EPSILON);

        println!("Successfully verified day/night pause sync across all time periods");
    }
}

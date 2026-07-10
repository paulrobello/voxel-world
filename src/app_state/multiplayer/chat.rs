//! Chat history + display-overlay state, extracted from `MultiplayerState`
//! (ARC-002 phase 1).
//!
//! Chat is a self-contained leaf domain: messages arrive as
//! `NetworkEvent::ChatReceived` (the server-message dispatcher routes them into the
//! unified event queue) and are applied here by the game loop via
//! [`ChatState::add_message`]. No coordinator mutates these fields directly, which is
//! why this was the first domain safe to peel off the `MultiplayerState` god object.

use std::time::Instant;

/// Maximum chat messages to keep in history.
const MAX_CHAT_HISTORY: usize = 50;

/// Chat message entry for display.
#[derive(Debug, Clone)]
pub struct ChatEntry {
    /// Player name who sent the message.
    pub player_name: String,
    /// Message content.
    pub message: String,
    /// Timestamp when message was received.
    #[allow(dead_code)] // reason: multiplayer state — kept for future wire-up
    pub timestamp: Instant,
}

/// Chat history and the display-overlay timer that fades recent messages.
///
/// The host [`super::MultiplayerState`] holds this as `chat: ChatState` and forwards
/// its public methods, so external callers (`update.rs`, `hud.rs`) are unchanged.
#[derive(Debug, Clone, Default)]
pub struct ChatState {
    history: Vec<ChatEntry>,
    display_timer: Option<f32>,
}

impl ChatState {
    /// Creates an empty chat state.
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            display_timer: None,
        }
    }

    /// Adds a chat message to history and reveals the overlay for 10 seconds.
    pub fn add_message(&mut self, player_name: String, message: String) {
        self.history.push(ChatEntry {
            player_name,
            message,
            timestamp: Instant::now(),
        });
        // Keep only last MAX_CHAT_HISTORY messages.
        if self.history.len() > MAX_CHAT_HISTORY {
            self.history.remove(0);
        }
        // Show chat for 10 seconds.
        self.display_timer = Some(10.0);
    }

    /// Advances the display timer (call every frame with `delta_time`).
    pub fn update_timer(&mut self, delta_time: f32) {
        if let Some(ref mut timer) = self.display_timer {
            *timer -= delta_time;
            if *timer <= 0.0 {
                self.display_timer = None;
            }
        }
    }

    /// Returns whether the chat overlay should be visible.
    pub fn is_visible(&self) -> bool {
        self.display_timer.is_some()
    }

    /// Returns the chat history for display.
    pub fn history(&self) -> &[ChatEntry] {
        &self.history
    }

    /// Returns the chat display timer remaining (if any).
    pub fn display_timer(&self) -> Option<f32> {
        self.display_timer
    }
}

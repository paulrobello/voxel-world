//! Client input-send + prediction state, extracted from `MultiplayerState`
//! (ARC-002 phase 4).
//!
//! Owns the input-sequence counter, the last-sent-input snapshot used to
//! delta-skip near-idle frames, the input-send wall-clock throttle (PHY-M05),
//! and the prediction buffer. [`InputState::send_input`] borrows the client so
//! the host facade still owns connection lifecycle.

use std::time::{Duration, Instant};

use crate::net::{GameClient, PredictionState};

/// Snapshot of the last PlayerInput actually sent to the server.
///
/// Used by `send_input` to skip near-idle frames below the movement thresholds.
/// `skips_remaining` is a countdown that forces a keep-alive send every
/// `FORCE_SEND_EVERY` calls so the server never stops hearing from an idle client.
#[derive(Debug, Clone, Copy)]
struct LastSentInput {
    position: [f32; 3],
    velocity: [f32; 3],
    yaw: f32,
    pitch: f32,
    actions: crate::net::protocol::InputActions,
    skips_remaining: u32,
}

/// Largest component-wise absolute delta between two 3-vectors.
#[inline]
fn max_abs_delta(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = (a[0] - b[0]).abs();
    let dy = (a[1] - b[1]).abs();
    let dz = (a[2] - b[2]).abs();
    dx.max(dy).max(dz)
}

/// Client input/prediction state.
///
/// Extracted from `MultiplayerState` (ARC-002). The host holds this as
/// `input: InputState` and forwards `send_input` / `should_send_input_tick`.
pub struct InputState {
    prediction: PredictionState,
    input_sequence: u32,
    last_sent_input: Option<LastSentInput>,
    last_input_send: Option<Instant>,
}

impl InputState {
    /// Creates fresh input state (sequence 0, nothing sent yet).
    pub fn new() -> Self {
        Self {
            prediction: PredictionState::new(),
            input_sequence: 0,
            last_sent_input: None,
            last_input_send: None,
        }
    }

    /// Exposes the prediction buffer (for server reconciliation in the dispatcher).
    pub fn prediction_mut(&mut self) -> &mut PredictionState {
        &mut self.prediction
    }

    /// Returns true when the network tick interval has elapsed since the last
    /// client→server input send (and records the time). Gates `send_input` so
    /// the send rate is independent of render FPS (PHY-M05). The first call is
    /// always true. Does not panic on clock issues.
    pub fn should_send_tick(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = self
            .last_input_send
            .map(|t| now.duration_since(t))
            .unwrap_or(Duration::MAX);
        if elapsed >= super::NETWORK_TICK_INTERVAL {
            self.last_input_send = Some(now);
            true
        } else {
            false
        }
    }

    /// Sends player input to the server, skipping frames where movement is below
    /// thresholds.
    ///
    /// Prediction recording still happens every call so local reconciliation
    /// stays accurate. Network sends are suppressed when all of the following
    /// hold vs. the last sent input: `|Δposition| < POSITION_THRESHOLD`,
    /// `|Δvelocity| < VELOCITY_THRESHOLD`, `|Δyaw/pitch| < ROTATION_THRESHOLD`,
    /// actions unchanged, and fewer than `FORCE_SEND_EVERY` calls since the last
    /// send (keep-alive).
    pub fn send_input(
        &mut self,
        client: &mut GameClient,
        position: [f32; 3],
        velocity: [f32; 3],
        yaw: f32,
        pitch: f32,
        actions: crate::net::protocol::InputActions,
    ) {
        const POSITION_THRESHOLD: f32 = 0.01; // 1 cm
        const VELOCITY_THRESHOLD: f32 = 0.1; // 10 cm/s
        const ROTATION_THRESHOLD: f32 = 0.0087; // ~0.5°
        const FORCE_SEND_EVERY: u32 = 20; // ~1 Hz keep-alive at 20 Hz send rate

        // Record input for prediction every call (local state must stay in sync).
        self.prediction
            .record_input(position, velocity, yaw, pitch, actions);

        let should_skip = match self.last_sent_input.as_mut() {
            Some(last) if last.skips_remaining > 0 => {
                let pos_delta = max_abs_delta(position, last.position);
                let vel_delta = max_abs_delta(velocity, last.velocity);
                let yaw_delta = (yaw - last.yaw).abs();
                let pitch_delta = (pitch - last.pitch).abs();
                let actions_changed = actions != last.actions;

                if !actions_changed
                    && pos_delta < POSITION_THRESHOLD
                    && vel_delta < VELOCITY_THRESHOLD
                    && yaw_delta < ROTATION_THRESHOLD
                    && pitch_delta < ROTATION_THRESHOLD
                {
                    last.skips_remaining -= 1;
                    true
                } else {
                    false
                }
            }
            _ => false,
        };

        if !should_skip {
            client.send_input(self.input_sequence, position, velocity, yaw, pitch, actions);
            self.input_sequence = self.input_sequence.wrapping_add(1);
            self.last_sent_input = Some(LastSentInput {
                position,
                velocity,
                yaw,
                pitch,
                actions,
                skips_remaining: FORCE_SEND_EVERY,
            });
        }
    }
}

//! Player synchronization with client-side prediction and server reconciliation.

// Allow unused code until networking is integrated into the game
#![allow(dead_code)]

use std::collections::VecDeque;

use crate::net::protocol::{InputActions, PlayerInput, PlayerState};

/// Maximum number of inputs to keep for reconciliation.
const INPUT_BUFFER_SIZE: usize = 64;

/// Server reconciliation threshold in blocks.
/// If prediction error exceeds this, snap to server position.
const RECONCILIATION_THRESHOLD: f32 = 5.0;

/// Prediction state for client-side prediction and reconciliation.
pub struct PredictionState {
    /// Buffer of recent inputs for reconciliation.
    input_buffer: VecDeque<(u32, PlayerInput)>,
    /// Buffer of predicted positions for comparison.
    predicted_positions: VecDeque<[f32; 3]>,
    /// Last sequence number acknowledged by server.
    last_server_sequence: u32,
    /// Current local sequence number.
    current_sequence: u32,
    /// Whether prediction is currently enabled.
    prediction_enabled: bool,
}

impl Default for PredictionState {
    fn default() -> Self {
        Self::new()
    }
}

impl PredictionState {
    /// Creates a new prediction state.
    pub fn new() -> Self {
        Self {
            input_buffer: VecDeque::with_capacity(INPUT_BUFFER_SIZE),
            predicted_positions: VecDeque::with_capacity(INPUT_BUFFER_SIZE),
            last_server_sequence: 0,
            current_sequence: 0,
            prediction_enabled: true,
        }
    }

    /// Records a new input and predicted position.
    /// Returns the sequence number assigned to this input.
    pub fn record_input(
        &mut self,
        position: [f32; 3],
        velocity: [f32; 3],
        yaw: f32,
        pitch: f32,
        actions: InputActions,
    ) -> u32 {
        let sequence = self.current_sequence;
        self.current_sequence = self.current_sequence.wrapping_add(1);

        let input = PlayerInput {
            sequence,
            position,
            velocity,
            yaw,
            pitch,
            actions,
        };

        self.input_buffer.push_back((sequence, input));
        self.predicted_positions.push_back(position);

        // Trim buffers if too large
        while self.input_buffer.len() > INPUT_BUFFER_SIZE {
            self.input_buffer.pop_front();
        }
        while self.predicted_positions.len() > INPUT_BUFFER_SIZE {
            self.predicted_positions.pop_front();
        }

        sequence
    }

    /// Reconciles with server state.
    /// Returns the corrected position if reconciliation is needed.
    pub fn reconcile(&mut self, server_state: &PlayerState) -> Option<[f32; 3]> {
        // Update last server sequence
        if server_state.last_sequence <= self.last_server_sequence && self.last_server_sequence != 0
        {
            // Old packet, ignore
            return None;
        }
        self.last_server_sequence = server_state.last_sequence;

        // Find the predicted position at this sequence
        let predicted_pos = self.find_predicted_position(server_state.last_sequence);

        // Calculate error
        let error = if let Some(predicted) = predicted_pos {
            let dx = server_state.position[0] - predicted[0];
            let dy = server_state.position[1] - predicted[1];
            let dz = server_state.position[2] - predicted[2];
            (dx * dx + dy * dy + dz * dz).sqrt()
        } else {
            // No prediction found, use server position
            RECONCILIATION_THRESHOLD + 1.0
        };

        // Remove old inputs that have been acknowledged
        self.input_buffer
            .retain(|(seq, _)| *seq > server_state.last_sequence);
        self.predicted_positions.clear();

        if error > RECONCILIATION_THRESHOLD {
            // Significant error, return server position
            Some(server_state.position)
        } else if error > 0.1 {
            // Small error, interpolate towards server position
            let lerp_factor = 0.3;
            Some([
                server_state.position[0] * lerp_factor
                    + predicted_pos.unwrap_or(server_state.position)[0] * (1.0 - lerp_factor),
                server_state.position[1] * lerp_factor
                    + predicted_pos.unwrap_or(server_state.position)[1] * (1.0 - lerp_factor),
                server_state.position[2] * lerp_factor
                    + predicted_pos.unwrap_or(server_state.position)[2] * (1.0 - lerp_factor),
            ])
        } else {
            // Error within acceptable range
            None
        }
    }

    /// Finds the predicted position at a given sequence number.
    fn find_predicted_position(&self, sequence: u32) -> Option<[f32; 3]> {
        for (seq, input) in self.input_buffer.iter() {
            if *seq == sequence {
                // Found the input at this sequence
                // The predicted position is in the input itself
                return Some(input.position);
            }
        }
        None
    }

    /// Returns the current sequence number.
    pub fn current_sequence(&self) -> u32 {
        self.current_sequence
    }

    /// Returns the last server-acknowledged sequence.
    pub fn last_server_sequence(&self) -> u32 {
        self.last_server_sequence
    }

    /// Enables or disables prediction.
    pub fn set_prediction_enabled(&mut self, enabled: bool) {
        self.prediction_enabled = enabled;
    }

    /// Returns whether prediction is enabled.
    pub fn is_prediction_enabled(&self) -> bool {
        self.prediction_enabled
    }

    /// Clears all prediction state (e.g., on teleport).
    pub fn clear(&mut self) {
        self.input_buffer.clear();
        self.predicted_positions.clear();
        self.last_server_sequence = 0;
        self.current_sequence = 0;
    }
}

/// Remote player state with interpolation support.
pub struct RemotePlayer {
    /// Player ID.
    pub player_id: u64,
    /// Player name.
    pub name: String,
    /// Current interpolated position.
    pub position: [f32; 3],
    /// Current interpolated velocity.
    pub velocity: [f32; 3],
    /// Current yaw.
    pub yaw: f32,
    /// Current pitch.
    pub pitch: f32,
    /// Buffer of recent positions for interpolation.
    position_buffer: VecDeque<([f32; 3], f64)>,
    /// Buffer of recent velocities.
    velocity_buffer: VecDeque<([f32; 3], f64)>,
    /// Interpolation delay in seconds.
    interpolation_delay: f64,
}

impl RemotePlayer {
    /// Creates a new remote player.
    pub fn new(player_id: u64, name: String, position: [f32; 3]) -> Self {
        Self {
            player_id,
            name,
            position,
            velocity: [0.0, 0.0, 0.0],
            yaw: 0.0,
            pitch: 0.0,
            position_buffer: VecDeque::with_capacity(16),
            velocity_buffer: VecDeque::with_capacity(16),
            interpolation_delay: 0.1, // 100ms interpolation delay
        }
    }

    /// Updates the player state from server data.
    pub fn update_state(&mut self, state: &PlayerState, timestamp: f64) {
        self.yaw = state.yaw;
        self.pitch = state.pitch;

        // Add to interpolation buffers
        self.position_buffer.push_back((state.position, timestamp));
        self.velocity_buffer.push_back((state.velocity, timestamp));

        // Trim buffers
        while self.position_buffer.len() > 16 {
            self.position_buffer.pop_front();
        }
        while self.velocity_buffer.len() > 16 {
            self.velocity_buffer.pop_front();
        }
    }

    /// Interpolates position based on current time.
    pub fn interpolate(&mut self, current_time: f64) {
        let render_time = current_time - self.interpolation_delay;

        // Find two positions to interpolate between
        let mut before: Option<([f32; 3], f64)> = None;
        let mut after: Option<([f32; 3], f64)> = None;

        for (pos, time) in &self.position_buffer {
            if *time <= render_time {
                before = Some((*pos, *time));
            } else {
                after = Some((*pos, *time));
                break;
            }
        }

        match (before, after) {
            (Some((pos_before, _)), Some((pos_after, time_after))) => {
                // Interpolate between the two positions
                let time_before = self
                    .position_buffer
                    .iter()
                    .find(|(_, t)| *t <= render_time)
                    .map(|(_, t)| *t)
                    .unwrap_or(render_time);

                let t = if time_after > time_before {
                    ((render_time - time_before) / (time_after - time_before)).clamp(0.0, 1.0)
                } else {
                    0.0
                };

                self.position = [
                    pos_before[0] + (pos_after[0] - pos_before[0]) * t as f32,
                    pos_before[1] + (pos_after[1] - pos_before[1]) * t as f32,
                    pos_before[2] + (pos_after[2] - pos_before[2]) * t as f32,
                ];
            }
            (Some((pos, _)), None) => {
                // Only have one position, use it
                self.position = pos;
            }
            (None, Some((pos, _))) => {
                // Future position, use it
                self.position = pos;
            }
            (None, None) => {
                // No data, keep current position
            }
        }

        // Similar for velocity
        if let Some((vel, _)) = self.velocity_buffer.back() {
            self.velocity = *vel;
        }
    }

    /// Sets the interpolation delay.
    pub fn set_interpolation_delay(&mut self, delay: f64) {
        self.interpolation_delay = delay;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prediction_state_record_input() {
        let mut state = PredictionState::new();

        let seq = state.record_input(
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            0.0,
            0.0,
            InputActions::default(),
        );
        assert_eq!(seq, 0);

        let seq = state.record_input(
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            0.0,
            0.0,
            InputActions::default(),
        );
        assert_eq!(seq, 1);

        assert_eq!(state.current_sequence(), 2);
    }

    #[test]
    fn test_prediction_state_reconcile_no_error() {
        let mut state = PredictionState::new();

        state.record_input([0.0, 0.0, 0.0], [0.0; 3], 0.0, 0.0, InputActions::default());

        let server_state = PlayerState {
            player_id: 1,
            position: [0.0, 0.0, 0.0],
            velocity: [0.0; 3],
            last_sequence: 0,
            yaw: 0.0,
            pitch: 0.0,
        };

        let result = state.reconcile(&server_state);
        // Position matches, no correction needed
        assert!(result.is_none() || result.unwrap() == [0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_prediction_state_reconcile_large_error() {
        let mut state = PredictionState::new();

        state.record_input([0.0, 0.0, 0.0], [0.0; 3], 0.0, 0.0, InputActions::default());

        let server_state = PlayerState {
            player_id: 1,
            position: [10.0, 0.0, 0.0], // 10 blocks away
            velocity: [0.0; 3],
            last_sequence: 0,
            yaw: 0.0,
            pitch: 0.0,
        };

        let result = state.reconcile(&server_state);
        // Large error, should return server position
        assert!(result.is_some());
        let corrected = result.unwrap();
        assert!((corrected[0] - 10.0).abs() < 5.0); // Lerp towards server
    }

    #[test]
    fn test_remote_player_interpolation() {
        let mut player = RemotePlayer::new(1, "Test".to_string(), [0.0, 0.0, 0.0]);

        // Add two position samples
        player.update_state(
            &PlayerState {
                player_id: 1,
                position: [0.0, 0.0, 0.0],
                velocity: [0.0; 3],
                last_sequence: 0,
                yaw: 0.0,
                pitch: 0.0,
            },
            0.0,
        );
        player.update_state(
            &PlayerState {
                player_id: 1,
                position: [10.0, 0.0, 0.0],
                velocity: [0.0; 3],
                last_sequence: 1,
                yaw: 0.0,
                pitch: 0.0,
            },
            0.2,
        );

        // Interpolate at t=0.1 (halfway between samples accounting for delay)
        player.interpolate(0.2); // render_time = 0.2 - 0.1 = 0.1

        // Position should be interpolated between 0 and 10
        assert!(player.position[0] > 0.0 && player.position[0] < 10.0);
    }
}

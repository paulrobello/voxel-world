//! Remote player rendering for multiplayer.
//!
//! Handles rendering of other players in the world as 2-block tall placeholders.

use bytemuck::{Pod, Zeroable};

/// Maximum number of remote players that can be rendered at once.
pub const MAX_REMOTE_PLAYERS: usize = 32;

/// Player colors for rendering (8 distinct colors, matching minimap).
#[allow(dead_code)]
pub const PLAYER_COLORS: [[f32; 3]; 8] = [
    [0.0, 0.78, 1.0],  // Cyan
    [1.0, 0.39, 0.39], // Light red
    [0.39, 1.0, 0.39], // Light green
    [1.0, 0.78, 0.2],  // Gold
    [0.78, 0.39, 1.0], // Purple
    [1.0, 0.59, 0.78], // Pink
    [0.39, 0.59, 1.0], // Light blue
    [1.0, 0.71, 0.39], // Orange
];

/// GPU-compatible remote player data for shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GpuRemotePlayer {
    /// Position XYZ (feet position) + color index (0-7)
    pub pos_color: [f32; 4],
    /// Height (typically 1.8 for 2-block tall) + padding
    pub height_padding: [f32; 4],
}

impl GpuRemotePlayer {
    /// Creates a new GPU remote player from position, color index, and height.
    pub fn new(position: [f32; 3], color_index: u32, height: f32) -> Self {
        Self {
            pos_color: [position[0], position[1], position[2], color_index as f32],
            height_padding: [height, 0.0, 0.0, 0.0],
        }
    }
}

/// Get the color RGB for a player based on their index.
#[allow(dead_code)]
pub fn get_player_color(index: usize) -> [f32; 3] {
    PLAYER_COLORS[index % PLAYER_COLORS.len()]
}

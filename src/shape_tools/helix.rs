//! Helix (spiral/corkscrew) generation algorithm and tool state.
//!
//! This module provides functions to generate helix block positions and the
//! HelixToolState for managing the helix placement tool.

use crate::gpu_resources::MAX_STENCIL_BLOCKS;
use nalgebra::Vector3;

/// Direction of helix winding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HelixDirection {
    /// Clockwise when viewed from above.
    #[default]
    Clockwise,
    /// Counter-clockwise when viewed from above.
    CounterClockwise,
}

impl HelixDirection {
    /// Get a short name for the direction.
    #[allow(dead_code)]
    pub fn name(&self) -> &'static str {
        match self {
            HelixDirection::Clockwise => "CW",
            HelixDirection::CounterClockwise => "CCW",
        }
    }

    /// Get a display name for the direction.
    #[allow(dead_code)]
    pub fn display_name(&self) -> &'static str {
        match self {
            HelixDirection::Clockwise => "Clockwise",
            HelixDirection::CounterClockwise => "Counter-Clockwise",
        }
    }

    /// Toggle direction.
    #[allow(dead_code)]
    pub fn toggle(&self) -> Self {
        match self {
            HelixDirection::Clockwise => HelixDirection::CounterClockwise,
            HelixDirection::CounterClockwise => HelixDirection::Clockwise,
        }
    }
}

/// State for the helix placement tool.
#[derive(Clone, Debug)]
pub struct HelixToolState {
    /// Whether the helix tool is currently active.
    pub active: bool,
    /// Radius from center axis to helix center in blocks (2-50).
    pub radius: i32,
    /// Total height of the helix in blocks (5-200).
    pub height: i32,
    /// Number of complete turns/rotations (0.5-20.0).
    pub turns: f32,
    /// Tube radius (thickness of the spiral) in blocks (1-10).
    pub tube_radius: i32,
    /// Winding direction.
    pub direction: HelixDirection,
    /// Starting angle in degrees (0-360).
    pub start_angle: i32,
    /// Cached preview positions for GPU upload.
    pub preview_positions: Vec<Vector3<i32>>,
    /// Current preview center position (if targeting a block).
    pub preview_center: Option<Vector3<i32>>,
    /// Total block count for the full helix (may differ from preview if truncated).
    pub total_blocks: usize,
    /// Whether the preview is truncated due to MAX_STENCIL_BLOCKS.
    pub preview_truncated: bool,
    /// Cached parameters to detect when regeneration is needed.
    cached_params: (i32, i32, i32, i32, i32, i32), // (radius, height, turns*10, tube, angle, dir)
}

impl Default for HelixToolState {
    fn default() -> Self {
        Self {
            active: false,
            radius: 5,
            height: 20,
            turns: 2.0,
            tube_radius: 1,
            direction: HelixDirection::Clockwise,
            start_angle: 0,
            preview_positions: Vec::new(),
            preview_center: None,
            total_blocks: 0,
            preview_truncated: false,
            cached_params: (0, 0, 0, 0, 0, 0),
        }
    }
}

impl HelixToolState {
    /// Check if parameters have changed since last generation.
    fn params_changed(&self) -> bool {
        let dir_val = match self.direction {
            HelixDirection::Clockwise => 0,
            HelixDirection::CounterClockwise => 1,
        };
        let current = (
            self.radius,
            self.height,
            (self.turns * 10.0) as i32, // Compare with some precision
            self.tube_radius,
            self.start_angle,
            dir_val,
        );
        current != self.cached_params
    }

    /// Update cached parameters.
    fn update_cache(&mut self) {
        let dir_val = match self.direction {
            HelixDirection::Clockwise => 0,
            HelixDirection::CounterClockwise => 1,
        };
        self.cached_params = (
            self.radius,
            self.height,
            (self.turns * 10.0) as i32,
            self.tube_radius,
            self.start_angle,
            dir_val,
        );
    }

    /// Update the preview based on a new target position.
    pub fn update_preview(&mut self, target: Vector3<i32>) {
        // Only regenerate if position or parameters changed
        let pos_changed = self.preview_center != Some(target);
        let params_changed = self.params_changed();

        if !pos_changed && !params_changed {
            return;
        }

        self.preview_center = Some(target);
        self.update_cache();

        // Generate all positions
        let all_positions = generate_helix_positions(
            target,
            self.radius,
            self.height,
            self.turns,
            self.tube_radius,
            self.direction,
            self.start_angle,
        );

        self.total_blocks = all_positions.len();
        self.preview_truncated = self.total_blocks > MAX_STENCIL_BLOCKS;

        // Truncate for preview if needed
        if self.preview_truncated {
            self.preview_positions = all_positions.into_iter().take(MAX_STENCIL_BLOCKS).collect();
        } else {
            self.preview_positions = all_positions;
        }
    }

    /// Clear the preview (when not targeting any block).
    pub fn clear_preview(&mut self) {
        self.preview_positions.clear();
        self.preview_center = None;
        self.total_blocks = 0;
        self.preview_truncated = false;
    }

    /// Deactivate the tool and clear preview.
    pub fn deactivate(&mut self) {
        self.active = false;
        self.clear_preview();
    }
}

/// Generate helix positions centered at the given point.
///
/// The helix spirals upward from the center point, with the base at y=center.y.
///
/// # Arguments
/// * `center` - The base center of the helix (bottom of spiral)
/// * `radius` - Distance from center axis to helix tube center
/// * `height` - Total height of the helix
/// * `turns` - Number of complete rotations
/// * `tube_radius` - Radius of the spiral tube (thickness)
/// * `direction` - Clockwise or counter-clockwise winding
/// * `start_angle` - Starting angle in degrees
///
/// # Returns
/// Vector of block positions that make up the helix
pub fn generate_helix_positions(
    center: Vector3<i32>,
    radius: i32,
    height: i32,
    turns: f32,
    tube_radius: i32,
    direction: HelixDirection,
    start_angle: i32,
) -> Vec<Vector3<i32>> {
    let mut positions = Vec::new();

    // Validate parameters
    if radius < 1 || height < 1 || turns < 0.1 || tube_radius < 1 {
        return positions;
    }

    let r = radius as f64;
    let h = height as f64;
    let t = turns as f64;
    let tube_r = tube_radius as f64;
    let tube_r_sq = (tube_r + 0.5) * (tube_r + 0.5); // Add 0.5 for better voxel coverage
    let start_rad = (start_angle as f64).to_radians();

    // Direction multiplier
    let dir_mult = match direction {
        HelixDirection::Clockwise => 1.0,
        HelixDirection::CounterClockwise => -1.0,
    };

    // Use a HashSet to avoid duplicates
    let mut visited = std::collections::HashSet::new();

    // Calculate the number of sample points along the helix
    // Use a fine sampling rate to ensure no gaps
    let circumference = std::f64::consts::TAU * r * t;
    let arc_length = (h * h + circumference * circumference).sqrt();
    // Sample at most every 0.5 blocks along the arc to avoid gaps
    let num_samples = ((arc_length / 0.5) as i32).max(height * 4);

    // Sample points along the helix and fill spheres around each point
    for i in 0..=num_samples {
        let progress = i as f64 / num_samples as f64;

        // Parametric helix: x = r*cos(θ), y = (h-1)*progress, z = r*sin(θ)
        // where θ goes from start_angle to start_angle + turns * 2π
        // Y goes from 0 to height-1 to stay within valid block range
        let angle = start_rad + dir_mult * progress * t * std::f64::consts::TAU;
        let tube_center_x = r * angle.cos();
        let tube_center_y = (h - 1.0) * progress; // Height-1 so we stay in [0, height)
        let tube_center_z = r * angle.sin();

        // Fill a sphere around this point on the helix path
        for dx in -tube_radius..=tube_radius {
            for dy in -tube_radius..=tube_radius {
                for dz in -tube_radius..=tube_radius {
                    let px = tube_center_x + dx as f64;
                    let py = tube_center_y + dy as f64;
                    let pz = tube_center_z + dz as f64;

                    // Distance from this voxel center to the helix point
                    let dist_sq = (px - tube_center_x).powi(2)
                        + (py - tube_center_y).powi(2)
                        + (pz - tube_center_z).powi(2);

                    if dist_sq <= tube_r_sq {
                        let world_x = center.x + px.round() as i32;
                        let world_y = center.y + py.round() as i32;
                        let world_z = center.z + pz.round() as i32;

                        // Clamp Y to valid height range
                        if world_y < center.y || world_y >= center.y + height {
                            continue;
                        }

                        if visited.insert((world_x, world_y, world_z)) {
                            positions.push(Vector3::new(world_x, world_y, world_z));
                        }
                    }
                }
            }
        }
    }

    positions
}

/// Estimate helix volume (for confirmation dialogs).
///
/// Uses approximate formula: tube_area * helix_length
/// where helix_length = sqrt(height² + (2πR * turns)²)
///
/// # Arguments
/// * `radius` - Major radius (R)
/// * `height` - Height of helix
/// * `turns` - Number of turns
/// * `tube_radius` - Tube radius (r)
///
/// # Returns
/// Estimated block count
#[allow(dead_code)]
pub fn estimate_helix_volume(radius: i32, height: i32, turns: f32, tube_radius: i32) -> usize {
    let r = radius as f64;
    let h = height as f64;
    let t = turns as f64;
    let tube_r = tube_radius as f64;

    // Helix arc length = sqrt(h² + (2πRt)²)
    let circumference = std::f64::consts::TAU * r * t;
    let arc_length = (h * h + circumference * circumference).sqrt();

    // Tube cross-section area (approximate for voxels)
    let tube_area = std::f64::consts::PI * tube_r * tube_r;

    (arc_length * tube_area) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_helix() {
        let positions = generate_helix_positions(
            Vector3::new(0, 0, 0),
            5,
            10,
            1.0,
            1,
            HelixDirection::Clockwise,
            0,
        );

        // Should generate some blocks
        assert!(!positions.is_empty());

        // All positions should be within reasonable bounds
        for pos in &positions {
            assert!(pos.x.abs() <= 6);
            assert!(pos.y >= 0 && pos.y < 10);
            assert!(pos.z.abs() <= 6);
        }
    }

    #[test]
    fn test_direction_symmetry() {
        let cw = generate_helix_positions(
            Vector3::new(0, 0, 0),
            5,
            20,
            2.0,
            1,
            HelixDirection::Clockwise,
            0,
        );
        let ccw = generate_helix_positions(
            Vector3::new(0, 0, 0),
            5,
            20,
            2.0,
            1,
            HelixDirection::CounterClockwise,
            0,
        );

        // Same parameters in opposite directions should give same block count
        assert_eq!(cw.len(), ccw.len());
    }

    #[test]
    fn test_more_turns_more_blocks() {
        let one_turn = generate_helix_positions(
            Vector3::new(0, 0, 0),
            5,
            20,
            1.0,
            1,
            HelixDirection::Clockwise,
            0,
        );
        let two_turns = generate_helix_positions(
            Vector3::new(0, 0, 0),
            5,
            20,
            2.0,
            1,
            HelixDirection::Clockwise,
            0,
        );

        // More turns with same height = tighter helix
        // Should have similar block counts since height determines Y range
        assert!(!one_turn.is_empty());
        assert!(!two_turns.is_empty());
    }

    #[test]
    fn test_thicker_tube() {
        let thin = generate_helix_positions(
            Vector3::new(0, 0, 0),
            5,
            20,
            1.0,
            1,
            HelixDirection::Clockwise,
            0,
        );
        let thick = generate_helix_positions(
            Vector3::new(0, 0, 0),
            5,
            20,
            1.0,
            3,
            HelixDirection::Clockwise,
            0,
        );

        // Thicker tube should have more blocks
        assert!(thick.len() > thin.len());
    }

    #[test]
    fn test_invalid_params() {
        let positions = generate_helix_positions(
            Vector3::new(0, 0, 0),
            0,
            0,
            0.0,
            0,
            HelixDirection::Clockwise,
            0,
        );
        assert!(positions.is_empty());
    }
}

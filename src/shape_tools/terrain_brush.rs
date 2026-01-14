//! Terrain Brush tool for paint-style terrain modification.
//!
//! Provides modes for raising, lowering, smoothing, and flattening terrain
//! using a brush-based painting interface.

use nalgebra::Vector3;

/// Brush mode for terrain modification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerrainBrushMode {
    /// Add blocks upward (raise terrain).
    Raise,
    /// Remove blocks downward (lower terrain).
    Lower,
    /// Average heights with neighbors.
    Smooth,
    /// Set to specific Y level.
    Flatten,
}

impl TerrainBrushMode {
    /// Get display name for this mode.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Raise => "Raise",
            Self::Lower => "Lower",
            Self::Smooth => "Smooth",
            Self::Flatten => "Flatten",
        }
    }

    /// Get description for this mode.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Raise => "Add blocks upward to raise terrain",
            Self::Lower => "Remove blocks downward to lower terrain",
            Self::Smooth => "Average heights with neighbors",
            Self::Flatten => "Set terrain to specific Y level",
        }
    }

    /// Cycle to the next mode.
    pub fn next(&self) -> Self {
        match self {
            Self::Raise => Self::Lower,
            Self::Lower => Self::Smooth,
            Self::Smooth => Self::Flatten,
            Self::Flatten => Self::Raise,
        }
    }
}

/// Brush shape for terrain modification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrushShape {
    /// Circular brush.
    Circle,
    /// Square brush.
    Square,
}

impl BrushShape {
    /// Get display name for this shape.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Circle => "Circle",
            Self::Square => "Square",
        }
    }

    /// Toggle to the other shape.
    pub fn toggle(&self) -> Self {
        match self {
            Self::Circle => Self::Square,
            Self::Square => Self::Circle,
        }
    }
}

/// State for the terrain brush tool.
#[derive(Debug, Clone)]
pub struct TerrainBrushState {
    /// Whether the tool is active.
    pub active: bool,
    /// Current brush mode.
    pub mode: TerrainBrushMode,
    /// Brush radius (1-20 blocks).
    pub radius: i32,
    /// Brush strength (1-10).
    pub strength: i32,
    /// Target Y level for flatten mode.
    pub target_y: i32,
    /// Brush shape.
    pub shape: BrushShape,
    /// Whether currently painting.
    pub painting: bool,
    /// Preview positions for brush footprint.
    pub preview_positions: Vec<Vector3<i32>>,
    /// Last paint position to avoid repainting same spot.
    last_paint_pos: Option<Vector3<i32>>,
    /// Cooldown between paint applications in seconds.
    pub cooldown: f32,
    /// Time of last paint application (for cooldown).
    last_paint_time: f64,
}

impl Default for TerrainBrushState {
    fn default() -> Self {
        Self {
            active: false,
            mode: TerrainBrushMode::Raise,
            radius: 3,
            strength: 1,
            target_y: 64,
            shape: BrushShape::Circle,
            painting: false,
            preview_positions: Vec::new(),
            last_paint_pos: None,
            cooldown: 0.5,
            last_paint_time: 0.0,
        }
    }
}

impl TerrainBrushState {
    /// Toggle the brush tool on/off.
    pub fn toggle(&mut self) {
        self.active = !self.active;
        if !self.active {
            self.painting = false;
            self.preview_positions.clear();
            self.last_paint_pos = None;
        }
    }

    /// Update preview for current cursor position.
    pub fn update_preview(&mut self, center: Vector3<i32>) {
        self.preview_positions = generate_brush_footprint(center, self.radius, self.shape);
    }

    /// Clear the preview.
    pub fn clear_preview(&mut self) {
        self.preview_positions.clear();
    }

    /// Start painting.
    pub fn start_paint(&mut self) {
        self.painting = true;
    }

    /// Stop painting.
    pub fn stop_paint(&mut self) {
        self.painting = false;
        self.last_paint_pos = None;
    }

    /// Check if we should paint at this position (avoid repeat and cooldown).
    ///
    /// # Arguments
    /// * `pos` - Current paint position
    /// * `current_time` - Current time in seconds (from game start)
    ///
    /// # Returns
    /// True if painting should proceed (position changed and cooldown elapsed).
    pub fn should_paint_at(&mut self, pos: Vector3<i32>, current_time: f64) -> bool {
        // Check cooldown
        if current_time - self.last_paint_time < self.cooldown as f64 {
            return false;
        }

        // Check position change
        if self.last_paint_pos == Some(pos) {
            return false;
        }

        self.last_paint_pos = Some(pos);
        self.last_paint_time = current_time;
        true
    }

    /// Reset cooldown timer (called when starting to paint).
    pub fn reset_cooldown(&mut self) {
        self.last_paint_time = 0.0;
    }
}

/// Generate the footprint positions for a brush.
pub fn generate_brush_footprint(
    center: Vector3<i32>,
    radius: i32,
    shape: BrushShape,
) -> Vec<Vector3<i32>> {
    let mut positions = Vec::new();
    let r2 = (radius * radius) as f32;

    for dx in -radius..=radius {
        for dz in -radius..=radius {
            let include = match shape {
                BrushShape::Circle => {
                    let dist2 = (dx * dx + dz * dz) as f32;
                    dist2 <= r2
                }
                BrushShape::Square => true,
            };

            if include {
                positions.push(Vector3::new(center.x + dx, center.y, center.z + dz));
            }
        }
    }

    positions
}

/// Find the terrain height at an XZ position by scanning down from max.
#[allow(dead_code)]
pub fn find_terrain_height<F>(x: i32, z: i32, max_y: i32, is_solid: F) -> Option<i32>
where
    F: Fn(Vector3<i32>) -> bool,
{
    (0..=max_y).rev().find(|&y| is_solid(Vector3::new(x, y, z)))
}

/// Calculate positions to raise terrain at center by strength blocks.
pub fn calculate_raise_positions(
    center: Vector3<i32>,
    radius: i32,
    strength: i32,
    shape: BrushShape,
    heights: &[(i32, i32, i32)], // (x, z, height)
) -> Vec<Vector3<i32>> {
    let mut positions = Vec::new();
    let r2 = (radius * radius) as f32;

    for &(x, z, height) in heights {
        let dx = x - center.x;
        let dz = z - center.z;

        let include = match shape {
            BrushShape::Circle => {
                let dist2 = (dx * dx + dz * dz) as f32;
                dist2 <= r2
            }
            BrushShape::Square => dx.abs() <= radius && dz.abs() <= radius,
        };

        if include {
            // Calculate falloff based on distance from center
            let falloff = match shape {
                BrushShape::Circle => {
                    let dist = ((dx * dx + dz * dz) as f32).sqrt();
                    let falloff_factor = 1.0 - (dist / radius as f32).clamp(0.0, 1.0);
                    (strength as f32 * falloff_factor).round() as i32
                }
                BrushShape::Square => strength,
            };

            // Add blocks from current height up to height + falloff
            for y_off in 1..=falloff.max(1) {
                let new_y = height + y_off;
                if new_y <= 511 {
                    positions.push(Vector3::new(x, new_y, z));
                }
            }
        }
    }

    positions
}

/// Calculate positions to lower terrain at center by strength blocks.
pub fn calculate_lower_positions(
    center: Vector3<i32>,
    radius: i32,
    strength: i32,
    shape: BrushShape,
    heights: &[(i32, i32, i32)], // (x, z, height)
) -> Vec<Vector3<i32>> {
    let mut positions = Vec::new();
    let r2 = (radius * radius) as f32;

    for &(x, z, height) in heights {
        let dx = x - center.x;
        let dz = z - center.z;

        let include = match shape {
            BrushShape::Circle => {
                let dist2 = (dx * dx + dz * dz) as f32;
                dist2 <= r2
            }
            BrushShape::Square => dx.abs() <= radius && dz.abs() <= radius,
        };

        if include {
            // Calculate falloff based on distance from center
            let falloff = match shape {
                BrushShape::Circle => {
                    let dist = ((dx * dx + dz * dz) as f32).sqrt();
                    let falloff_factor = 1.0 - (dist / radius as f32).clamp(0.0, 1.0);
                    (strength as f32 * falloff_factor).round() as i32
                }
                BrushShape::Square => strength,
            };

            // Mark blocks from current height down to height - falloff for removal
            for y_off in 0..falloff.max(1) {
                let remove_y = height - y_off;
                if remove_y >= 1 {
                    // Don't remove below y=1
                    positions.push(Vector3::new(x, remove_y, z));
                }
            }
        }
    }

    positions
}

/// Calculate positions for smoothing terrain (average heights with neighbors).
pub fn calculate_smooth_positions(
    center: Vector3<i32>,
    radius: i32,
    shape: BrushShape,
    heights: &[(i32, i32, i32)], // (x, z, height)
) -> (Vec<Vector3<i32>>, Vec<Vector3<i32>>) {
    // Returns (positions_to_add, positions_to_remove)
    let mut to_add = Vec::new();
    let mut to_remove = Vec::new();

    if heights.is_empty() {
        return (to_add, to_remove);
    }

    // Calculate average height
    let sum: i64 = heights.iter().map(|&(_, _, h)| h as i64).sum();
    let avg = (sum / heights.len() as i64) as i32;
    let r2 = (radius * radius) as f32;

    for &(x, z, height) in heights {
        let dx = x - center.x;
        let dz = z - center.z;

        let include = match shape {
            BrushShape::Circle => {
                let dist2 = (dx * dx + dz * dz) as f32;
                dist2 <= r2
            }
            BrushShape::Square => dx.abs() <= radius && dz.abs() <= radius,
        };

        if include {
            // Calculate blend factor based on distance (stronger effect at center)
            let blend = match shape {
                BrushShape::Circle => {
                    let dist = ((dx * dx + dz * dz) as f32).sqrt();
                    1.0 - (dist / radius as f32).clamp(0.0, 1.0)
                }
                BrushShape::Square => 1.0,
            };

            // Target height is blend between current and average
            let target = height + ((avg - height) as f32 * blend * 0.5).round() as i32;

            if target > height {
                // Need to add blocks
                for y in (height + 1)..=target {
                    if y <= 511 {
                        to_add.push(Vector3::new(x, y, z));
                    }
                }
            } else if target < height {
                // Need to remove blocks
                for y in (target + 1)..=height {
                    if y >= 1 {
                        to_remove.push(Vector3::new(x, y, z));
                    }
                }
            }
        }
    }

    (to_add, to_remove)
}

/// Calculate positions for flattening terrain to target Y level.
pub fn calculate_flatten_positions(
    center: Vector3<i32>,
    radius: i32,
    target_y: i32,
    shape: BrushShape,
    heights: &[(i32, i32, i32)], // (x, z, height)
) -> (Vec<Vector3<i32>>, Vec<Vector3<i32>>) {
    // Returns (positions_to_add, positions_to_remove)
    let mut to_add = Vec::new();
    let mut to_remove = Vec::new();
    let r2 = (radius * radius) as f32;

    for &(x, z, height) in heights {
        let dx = x - center.x;
        let dz = z - center.z;

        let include = match shape {
            BrushShape::Circle => {
                let dist2 = (dx * dx + dz * dz) as f32;
                dist2 <= r2
            }
            BrushShape::Square => dx.abs() <= radius && dz.abs() <= radius,
        };

        if include {
            if target_y > height {
                // Need to add blocks up to target_y
                for y in (height + 1)..=target_y {
                    if y <= 511 {
                        to_add.push(Vector3::new(x, y, z));
                    }
                }
            } else if target_y < height {
                // Need to remove blocks down to target_y
                for y in (target_y + 1)..=height {
                    if y >= 1 {
                        to_remove.push(Vector3::new(x, y, z));
                    }
                }
            }
        }
    }

    (to_add, to_remove)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brush_footprint_circle() {
        let center = Vector3::new(0, 64, 0);
        let positions = generate_brush_footprint(center, 1, BrushShape::Circle);

        // Radius 1 circle should be a cross pattern (5 blocks)
        assert_eq!(positions.len(), 5);
        assert!(positions.contains(&Vector3::new(0, 64, 0)));
        assert!(positions.contains(&Vector3::new(-1, 64, 0)));
        assert!(positions.contains(&Vector3::new(1, 64, 0)));
        assert!(positions.contains(&Vector3::new(0, 64, -1)));
        assert!(positions.contains(&Vector3::new(0, 64, 1)));
    }

    #[test]
    fn test_brush_footprint_square() {
        let center = Vector3::new(0, 64, 0);
        let positions = generate_brush_footprint(center, 1, BrushShape::Square);

        // Radius 1 square = 3x3 = 9 blocks
        assert_eq!(positions.len(), 9);
    }

    #[test]
    fn test_raise_positions() {
        let center = Vector3::new(5, 64, 5);
        let heights = vec![(5, 5, 64)];

        let positions = calculate_raise_positions(center, 1, 2, BrushShape::Circle, &heights);

        // At center, strength 2 should add 2 blocks above height 64
        assert!(positions.contains(&Vector3::new(5, 65, 5)));
        assert!(positions.contains(&Vector3::new(5, 66, 5)));
    }

    #[test]
    fn test_lower_positions() {
        let center = Vector3::new(5, 64, 5);
        let heights = vec![(5, 5, 64)];

        let positions = calculate_lower_positions(center, 1, 2, BrushShape::Circle, &heights);

        // At center, strength 2 should remove 2 blocks from height 64 down
        assert!(positions.contains(&Vector3::new(5, 64, 5)));
        assert!(positions.contains(&Vector3::new(5, 63, 5)));
    }

    #[test]
    fn test_flatten_positions() {
        let center = Vector3::new(5, 64, 5);
        let heights = vec![
            (5, 5, 62), // Below target - add
            (6, 5, 66), // Above target - remove
        ];

        let (to_add, to_remove) =
            calculate_flatten_positions(center, 2, 64, BrushShape::Circle, &heights);

        // Should add blocks from 63-64 at (5,5)
        assert!(to_add.contains(&Vector3::new(5, 63, 5)));
        assert!(to_add.contains(&Vector3::new(5, 64, 5)));

        // Should remove blocks 65-66 at (6,5)
        assert!(to_remove.contains(&Vector3::new(6, 65, 5)));
        assert!(to_remove.contains(&Vector3::new(6, 66, 5)));
    }

    #[test]
    fn test_mode_cycle() {
        let mode = TerrainBrushMode::Raise;
        assert_eq!(mode.next(), TerrainBrushMode::Lower);
        assert_eq!(mode.next().next(), TerrainBrushMode::Smooth);
        assert_eq!(mode.next().next().next(), TerrainBrushMode::Flatten);
        assert_eq!(mode.next().next().next().next(), TerrainBrushMode::Raise);
    }

    #[test]
    fn test_shape_toggle() {
        let shape = BrushShape::Circle;
        assert_eq!(shape.toggle(), BrushShape::Square);
        assert_eq!(shape.toggle().toggle(), BrushShape::Circle);
    }

    #[test]
    fn test_smooth_positions() {
        let center = Vector3::new(5, 64, 5);
        let heights = vec![
            (5, 5, 60), // Low
            (5, 6, 68), // High
        ];

        let (to_add, to_remove) =
            calculate_smooth_positions(center, 2, BrushShape::Circle, &heights);

        // Should move heights toward average (64)
        // Low height should go up, high height should go down
        // Actual values depend on blend factor
        assert!(!to_add.is_empty() || !to_remove.is_empty());
    }
}

//! Bezier curve generation algorithm and tool state.
//!
//! This module provides functions to generate cubic Bezier curve block positions
//! and the BezierToolState for managing the curve placement tool.

use crate::gpu_resources::MAX_STENCIL_BLOCKS;
use nalgebra::Vector3;

/// State for the bezier curve placement tool.
#[derive(Clone, Debug)]
pub struct BezierToolState {
    /// Whether the bezier tool is currently active.
    pub active: bool,
    /// Control points for the curve (up to 4 points).
    pub control_points: Vec<Vector3<i32>>,
    /// Tube radius (thickness of the curve) in blocks (1-10).
    pub tube_radius: i32,
    /// Resolution multiplier (segments per unit distance) (1-5).
    pub resolution: i32,
    /// Cached preview positions for GPU upload.
    pub preview_positions: Vec<Vector3<i32>>,
    /// Total block count for the full curve (may differ from preview if truncated).
    pub total_blocks: usize,
    /// Whether the preview is truncated due to MAX_STENCIL_BLOCKS.
    pub preview_truncated: bool,
    /// Cached parameters to detect when regeneration is needed.
    cached_params: (i32, i32, Vec<Vector3<i32>>),
}

impl Default for BezierToolState {
    fn default() -> Self {
        Self {
            active: false,
            control_points: Vec::new(),
            tube_radius: 1,
            resolution: 2,
            preview_positions: Vec::new(),
            total_blocks: 0,
            preview_truncated: false,
            cached_params: (0, 0, Vec::new()),
        }
    }
}

impl BezierToolState {
    /// Check if parameters have changed since last generation.
    fn params_changed(&self) -> bool {
        let current = (
            self.tube_radius,
            self.resolution,
            self.control_points.clone(),
        );
        current != self.cached_params
    }

    /// Update cached parameters.
    fn update_cache(&mut self) {
        self.cached_params = (
            self.tube_radius,
            self.resolution,
            self.control_points.clone(),
        );
    }

    /// Add a control point at the given position.
    pub fn add_control_point(&mut self, pos: Vector3<i32>) {
        if self.control_points.len() < 4 {
            self.control_points.push(pos);
            self.regenerate_preview();
        }
    }

    /// Remove the last control point.
    pub fn remove_last_point(&mut self) {
        if !self.control_points.is_empty() {
            self.control_points.pop();
            self.regenerate_preview();
        }
    }

    /// Check if we have enough points for a curve.
    pub fn has_curve(&self) -> bool {
        self.control_points.len() >= 3
    }

    /// Check if we can add more points.
    pub fn can_add_point(&self) -> bool {
        self.control_points.len() < 4
    }

    /// Get the number of control points.
    pub fn point_count(&self) -> usize {
        self.control_points.len()
    }

    /// Regenerate the preview based on current control points.
    pub fn regenerate_preview(&mut self) {
        if !self.params_changed() && !self.control_points.is_empty() {
            return;
        }

        self.update_cache();

        if self.control_points.len() < 3 {
            self.preview_positions.clear();
            self.total_blocks = 0;
            self.preview_truncated = false;
            return;
        }

        // Generate all positions
        let all_positions =
            generate_bezier_positions(&self.control_points, self.tube_radius, self.resolution);

        self.total_blocks = all_positions.len();
        self.preview_truncated = self.total_blocks > MAX_STENCIL_BLOCKS;

        // Truncate for preview if needed
        if self.preview_truncated {
            self.preview_positions = all_positions.into_iter().take(MAX_STENCIL_BLOCKS).collect();
        } else {
            self.preview_positions = all_positions;
        }
    }

    /// Update preview with live cursor position as next point.
    pub fn update_preview_with_cursor(&mut self, cursor_pos: Vector3<i32>) {
        // Create temporary control points with cursor position
        let mut temp_points = self.control_points.clone();
        if temp_points.len() < 4 {
            temp_points.push(cursor_pos);
        }

        if temp_points.len() < 3 {
            self.preview_positions.clear();
            self.total_blocks = 0;
            self.preview_truncated = false;
            return;
        }

        // Generate all positions
        let all_positions =
            generate_bezier_positions(&temp_points, self.tube_radius, self.resolution);

        self.total_blocks = all_positions.len();
        self.preview_truncated = self.total_blocks > MAX_STENCIL_BLOCKS;

        // Truncate for preview if needed
        if self.preview_truncated {
            self.preview_positions = all_positions.into_iter().take(MAX_STENCIL_BLOCKS).collect();
        } else {
            self.preview_positions = all_positions;
        }
    }

    /// Clear all control points and preview.
    pub fn clear(&mut self) {
        self.control_points.clear();
        self.preview_positions.clear();
        self.total_blocks = 0;
        self.preview_truncated = false;
    }

    /// Deactivate the tool and clear state.
    pub fn deactivate(&mut self) {
        self.active = false;
        self.clear();
    }

    /// Get the current status message.
    pub fn status_message(&self) -> &'static str {
        match self.control_points.len() {
            0 => "Right-click to place point 1",
            1 => "Right-click to place point 2",
            2 => "Right-click to place point 3 (curve start)",
            3 => "Right-click to place point 4 or Enter to confirm",
            4 => "Enter to confirm curve",
            _ => "Curve complete",
        }
    }
}

/// Generate bezier curve positions from control points.
///
/// Creates a cubic (4 points) or quadratic (3 points) Bezier curve
/// with the given tube radius.
///
/// # Arguments
/// * `control_points` - 3 or 4 control points defining the curve
/// * `tube_radius` - Radius of the tube around the curve
/// * `resolution` - Segments per unit distance (higher = smoother)
///
/// # Returns
/// Vector of block positions that make up the bezier curve
pub fn generate_bezier_positions(
    control_points: &[Vector3<i32>],
    tube_radius: i32,
    resolution: i32,
) -> Vec<Vector3<i32>> {
    let mut positions = Vec::new();

    // Validate parameters
    if control_points.len() < 3 || tube_radius < 1 || resolution < 1 {
        return positions;
    }

    // Convert control points to f64
    let points: Vec<Vector3<f64>> = control_points
        .iter()
        .map(|p| Vector3::new(p.x as f64, p.y as f64, p.z as f64))
        .collect();

    // Estimate curve length to determine number of samples
    let mut approx_length = 0.0;
    for i in 0..points.len() - 1 {
        approx_length += (points[i + 1] - points[i]).magnitude();
    }

    // Number of samples based on length and resolution
    let num_samples = ((approx_length * resolution as f64) as usize).max(10);
    let tube_r = tube_radius as f64;
    let tube_r_sq = tube_r * tube_r;

    // Use a HashSet to avoid duplicates
    let mut visited = std::collections::HashSet::new();

    // Sample points along the curve
    for i in 0..=num_samples {
        let t = i as f64 / num_samples as f64;

        // Calculate point on curve
        let curve_point = if points.len() == 3 {
            quadratic_bezier(&points[0], &points[1], &points[2], t)
        } else {
            cubic_bezier(&points[0], &points[1], &points[2], &points[3], t)
        };

        // Fill a sphere around this point
        let cx = curve_point.x.round() as i32;
        let cy = curve_point.y.round() as i32;
        let cz = curve_point.z.round() as i32;

        for dx in -tube_radius..=tube_radius {
            for dy in -tube_radius..=tube_radius {
                for dz in -tube_radius..=tube_radius {
                    let dist_sq = (dx * dx + dy * dy + dz * dz) as f64;
                    if dist_sq <= tube_r_sq {
                        let pos = (cx + dx, cy + dy, cz + dz);
                        if visited.insert(pos) {
                            positions.push(Vector3::new(pos.0, pos.1, pos.2));
                        }
                    }
                }
            }
        }
    }

    positions
}

/// Quadratic Bezier curve (3 control points).
fn quadratic_bezier(
    p0: &Vector3<f64>,
    p1: &Vector3<f64>,
    p2: &Vector3<f64>,
    t: f64,
) -> Vector3<f64> {
    let t2 = t * t;
    let mt = 1.0 - t;
    let mt2 = mt * mt;

    // B(t) = (1-t)²P₀ + 2(1-t)tP₁ + t²P₂
    p0 * mt2 + p1 * (2.0 * mt * t) + p2 * t2
}

/// Cubic Bezier curve (4 control points).
fn cubic_bezier(
    p0: &Vector3<f64>,
    p1: &Vector3<f64>,
    p2: &Vector3<f64>,
    p3: &Vector3<f64>,
    t: f64,
) -> Vector3<f64> {
    let t2 = t * t;
    let t3 = t2 * t;
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    let mt3 = mt2 * mt;

    // B(t) = (1-t)³P₀ + 3(1-t)²tP₁ + 3(1-t)t²P₂ + t³P₃
    p0 * mt3 + p1 * (3.0 * mt2 * t) + p2 * (3.0 * mt * t2) + p3 * t3
}

/// Estimate bezier curve volume (for confirmation dialogs).
///
/// # Arguments
/// * `control_points` - The control points
/// * `tube_radius` - Tube radius
///
/// # Returns
/// Estimated block count
#[allow(dead_code)]
pub fn estimate_bezier_volume(control_points: &[Vector3<i32>], tube_radius: i32) -> usize {
    if control_points.len() < 3 {
        return 0;
    }

    // Approximate curve length
    let mut approx_length = 0.0f64;
    for i in 0..control_points.len() - 1 {
        let p1 = &control_points[i];
        let p2 = &control_points[i + 1];
        let dx = (p2.x - p1.x) as f64;
        let dy = (p2.y - p1.y) as f64;
        let dz = (p2.z - p1.z) as f64;
        approx_length += (dx * dx + dy * dy + dz * dz).sqrt();
    }

    // Volume = π * r² * length (approximate)
    let r = tube_radius as f64;
    (std::f64::consts::PI * r * r * approx_length) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quadratic_bezier() {
        let p0 = Vector3::new(0.0, 0.0, 0.0);
        let p1 = Vector3::new(5.0, 10.0, 0.0);
        let p2 = Vector3::new(10.0, 0.0, 0.0);

        // t=0 should be at p0
        let start = quadratic_bezier(&p0, &p1, &p2, 0.0);
        assert!((start - p0).magnitude() < 0.001);

        // t=1 should be at p2
        let end = quadratic_bezier(&p0, &p1, &p2, 1.0);
        assert!((end - p2).magnitude() < 0.001);

        // t=0.5 should be between points
        let mid = quadratic_bezier(&p0, &p1, &p2, 0.5);
        assert!(mid.x > 0.0 && mid.x < 10.0);
    }

    #[test]
    fn test_cubic_bezier() {
        let p0 = Vector3::new(0.0, 0.0, 0.0);
        let p1 = Vector3::new(3.0, 10.0, 0.0);
        let p2 = Vector3::new(7.0, 10.0, 0.0);
        let p3 = Vector3::new(10.0, 0.0, 0.0);

        // t=0 should be at p0
        let start = cubic_bezier(&p0, &p1, &p2, &p3, 0.0);
        assert!((start - p0).magnitude() < 0.001);

        // t=1 should be at p3
        let end = cubic_bezier(&p0, &p1, &p2, &p3, 1.0);
        assert!((end - p3).magnitude() < 0.001);
    }

    #[test]
    fn test_generate_bezier_positions() {
        let points = vec![
            Vector3::new(0, 0, 0),
            Vector3::new(5, 5, 0),
            Vector3::new(10, 0, 0),
        ];

        let positions = generate_bezier_positions(&points, 1, 2);

        // Should generate some blocks
        assert!(!positions.is_empty());
    }

    #[test]
    fn test_thicker_tube() {
        let points = vec![
            Vector3::new(0, 0, 0),
            Vector3::new(10, 5, 0),
            Vector3::new(20, 0, 0),
        ];

        let thin = generate_bezier_positions(&points, 1, 2);
        let thick = generate_bezier_positions(&points, 3, 2);

        // Thicker tube should have more blocks
        assert!(thick.len() > thin.len());
    }

    #[test]
    fn test_invalid_params() {
        // Too few points
        let positions = generate_bezier_positions(&[Vector3::new(0, 0, 0)], 1, 2);
        assert!(positions.is_empty());

        // Zero tube radius
        let points = vec![
            Vector3::new(0, 0, 0),
            Vector3::new(5, 5, 0),
            Vector3::new(10, 0, 0),
        ];
        let positions = generate_bezier_positions(&points, 0, 2);
        assert!(positions.is_empty());
    }

    #[test]
    fn test_cubic_vs_quadratic() {
        // Quadratic with 3 points
        let quad_points = vec![
            Vector3::new(0, 0, 0),
            Vector3::new(10, 10, 0),
            Vector3::new(20, 0, 0),
        ];

        // Cubic with 4 points forming similar curve
        let cubic_points = vec![
            Vector3::new(0, 0, 0),
            Vector3::new(7, 10, 0),
            Vector3::new(13, 10, 0),
            Vector3::new(20, 0, 0),
        ];

        let quad_pos = generate_bezier_positions(&quad_points, 1, 2);
        let cubic_pos = generate_bezier_positions(&cubic_points, 1, 2);

        // Both should generate blocks
        assert!(!quad_pos.is_empty());
        assert!(!cubic_pos.is_empty());
    }
}

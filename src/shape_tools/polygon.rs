//! Polygon (n-gon) generation algorithm and tool state.
//!
//! This module provides functions to generate regular polygon block positions
//! (triangles, squares, pentagons, hexagons, etc.) and the PolygonToolState
//! for managing the polygon placement tool.

use crate::gpu_resources::MAX_STENCIL_BLOCKS;
use nalgebra::Vector3;

/// State for the polygon placement tool.
#[derive(Clone, Debug)]
pub struct PolygonToolState {
    /// Whether the polygon tool is currently active.
    pub active: bool,
    /// Number of sides (3-12).
    pub sides: i32,
    /// Radius from center to vertices in blocks (2-50).
    pub radius: i32,
    /// Height of the prism (1 for flat polygon, >1 for prism) (1-100).
    pub height: i32,
    /// Whether to create a hollow polygon (outline only for flat, shell for prism).
    pub hollow: bool,
    /// Rotation angle in degrees (0-360).
    pub rotation: i32,
    /// Cached preview positions for GPU upload.
    pub preview_positions: Vec<Vector3<i32>>,
    /// Current preview center position (if targeting a block).
    pub preview_center: Option<Vector3<i32>>,
    /// Total block count for the full polygon (may differ from preview if truncated).
    pub total_blocks: usize,
    /// Whether the preview is truncated due to MAX_STENCIL_BLOCKS.
    pub preview_truncated: bool,
    /// Cached parameters to detect when regeneration is needed.
    cached_params: (i32, i32, i32, bool, i32),
}

impl Default for PolygonToolState {
    fn default() -> Self {
        Self {
            active: false,
            sides: 6, // Hexagon by default
            radius: 5,
            height: 1,
            hollow: false,
            rotation: 0,
            preview_positions: Vec::new(),
            preview_center: None,
            total_blocks: 0,
            preview_truncated: false,
            cached_params: (0, 0, 0, false, 0),
        }
    }
}

impl PolygonToolState {
    /// Check if parameters have changed since last generation.
    fn params_changed(&self) -> bool {
        let current = (
            self.sides,
            self.radius,
            self.height,
            self.hollow,
            self.rotation,
        );
        current != self.cached_params
    }

    /// Update cached parameters.
    fn update_cache(&mut self) {
        self.cached_params = (
            self.sides,
            self.radius,
            self.height,
            self.hollow,
            self.rotation,
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
        let all_positions = generate_polygon_positions(
            target,
            self.sides,
            self.radius,
            self.height,
            self.hollow,
            self.rotation,
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

    /// Get the name for the current polygon type.
    pub fn polygon_name(&self) -> &'static str {
        match self.sides {
            3 => "Triangle",
            4 => "Square",
            5 => "Pentagon",
            6 => "Hexagon",
            7 => "Heptagon",
            8 => "Octagon",
            9 => "Nonagon",
            10 => "Decagon",
            11 => "Hendecagon",
            12 => "Dodecagon",
            _ => "Polygon",
        }
    }
}

/// Generate polygon positions centered at the given point.
///
/// Creates a regular n-gon (or prism if height > 1) centered at the target.
///
/// # Arguments
/// * `center` - The center of the polygon base
/// * `sides` - Number of sides (3-12)
/// * `radius` - Distance from center to vertices
/// * `height` - Height of the prism (1 = flat polygon)
/// * `hollow` - If true, only outline (flat) or shell (prism)
/// * `rotation` - Rotation angle in degrees
///
/// # Returns
/// Vector of block positions that make up the polygon/prism
pub fn generate_polygon_positions(
    center: Vector3<i32>,
    sides: i32,
    radius: i32,
    height: i32,
    hollow: bool,
    rotation: i32,
) -> Vec<Vector3<i32>> {
    let mut positions = Vec::new();

    // Validate parameters
    if sides < 3 || radius < 1 || height < 1 {
        return positions;
    }

    let n = sides as usize;
    let r = radius as f64;
    let rot_rad = (rotation as f64).to_radians();

    // Use a HashSet to avoid duplicates
    let mut visited = std::collections::HashSet::new();

    // For each height level
    for dy in 0..height {
        // Check each point in the bounding box
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                let px = dx as f64;
                let pz = dz as f64;

                // Check if point is inside (or on edge of) the polygon
                let inside = point_in_polygon(px, pz, n, r, rot_rad);
                let on_edge = point_on_polygon_edge(px, pz, n, r, rot_rad);

                // For hollow mode:
                // - Flat (height=1): only include edges
                // - Prism (height>1): include edges at all levels + top/bottom fill
                let should_include = if hollow {
                    if height == 1 {
                        on_edge
                    } else {
                        // Hollow prism: walls + top/bottom faces
                        (dy == 0 || dy == height - 1) && inside || on_edge
                    }
                } else {
                    inside
                };

                if should_include {
                    let pos = Vector3::new(center.x + dx, center.y + dy, center.z + dz);
                    if visited.insert((pos.x, pos.y, pos.z)) {
                        positions.push(pos);
                    }
                }
            }
        }
    }

    positions
}

/// Check if a point is inside a regular polygon.
///
/// Uses the ray casting algorithm adapted for regular polygons.
fn point_in_polygon(px: f64, pz: f64, n: usize, radius: f64, rotation: f64) -> bool {
    // Generate vertices
    let vertices: Vec<(f64, f64)> = (0..n)
        .map(|i| {
            let angle = rotation + (i as f64) * std::f64::consts::TAU / (n as f64);
            (radius * angle.cos(), radius * angle.sin())
        })
        .collect();

    // Point-in-polygon test using ray casting
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, zi) = vertices[i];
        let (xj, zj) = vertices[j];

        if ((zi > pz) != (zj > pz)) && (px < (xj - xi) * (pz - zi) / (zj - zi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Check if a point is on (or very close to) a polygon edge.
fn point_on_polygon_edge(px: f64, pz: f64, n: usize, radius: f64, rotation: f64) -> bool {
    // Generate vertices
    let vertices: Vec<(f64, f64)> = (0..n)
        .map(|i| {
            let angle = rotation + (i as f64) * std::f64::consts::TAU / (n as f64);
            (radius * angle.cos(), radius * angle.sin())
        })
        .collect();

    // Check distance to each edge
    let threshold = 0.75; // Half a block tolerance for voxel edges
    for i in 0..n {
        let j = (i + 1) % n;
        let (x1, z1) = vertices[i];
        let (x2, z2) = vertices[j];

        // Distance from point to line segment
        let dist = point_to_line_distance(px, pz, x1, z1, x2, z2);
        if dist <= threshold {
            return true;
        }
    }
    false
}

/// Calculate distance from a point to a line segment.
fn point_to_line_distance(px: f64, pz: f64, x1: f64, z1: f64, x2: f64, z2: f64) -> f64 {
    let dx = x2 - x1;
    let dz = z2 - z1;
    let length_sq = dx * dx + dz * dz;

    if length_sq < 0.0001 {
        // Degenerate segment
        return ((px - x1).powi(2) + (pz - z1).powi(2)).sqrt();
    }

    // Project point onto line segment
    let t = ((px - x1) * dx + (pz - z1) * dz) / length_sq;
    let t = t.clamp(0.0, 1.0);

    let closest_x = x1 + t * dx;
    let closest_z = z1 + t * dz;

    ((px - closest_x).powi(2) + (pz - closest_z).powi(2)).sqrt()
}

/// Estimate polygon prism volume (for confirmation dialogs).
///
/// Uses approximate formula: area * height
/// where area = 0.5 * n * r² * sin(2π/n)
///
/// # Arguments
/// * `sides` - Number of sides
/// * `radius` - Radius to vertices
/// * `height` - Prism height
/// * `hollow` - If hollow, estimate shell volume
///
/// # Returns
/// Estimated block count
#[allow(dead_code)]
pub fn estimate_polygon_volume(sides: i32, radius: i32, height: i32, hollow: bool) -> usize {
    let n = sides as f64;
    let r = radius as f64;
    let h = height as f64;

    // Regular polygon area = 0.5 * n * r² * sin(2π/n)
    let area = 0.5 * n * r * r * (std::f64::consts::TAU / n).sin();

    if hollow && height > 1 {
        // Approximate shell: perimeter * height + 2 * area
        let perimeter = n * 2.0 * r * (std::f64::consts::PI / n).sin();
        ((perimeter * h + 2.0 * area) as usize).max(1)
    } else {
        (area * h) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_hexagon() {
        let positions = generate_polygon_positions(Vector3::new(0, 0, 0), 6, 5, 1, false, 0);

        // Should generate some blocks
        assert!(!positions.is_empty());

        // All positions should be on y=0 (flat)
        for pos in &positions {
            assert_eq!(pos.y, 0);
            assert!(pos.x.abs() <= 5);
            assert!(pos.z.abs() <= 5);
        }
    }

    #[test]
    fn test_polygon_prism() {
        let flat = generate_polygon_positions(Vector3::new(0, 0, 0), 6, 5, 1, false, 0);
        let prism = generate_polygon_positions(Vector3::new(0, 0, 0), 6, 5, 5, false, 0);

        // Prism should have 5x the blocks of a flat polygon
        assert!(prism.len() > flat.len() * 4);
        assert!(prism.len() <= flat.len() * 5);
    }

    #[test]
    fn test_more_sides_rounder() {
        let triangle = generate_polygon_positions(Vector3::new(0, 0, 0), 3, 5, 1, false, 0);
        let dodecagon = generate_polygon_positions(Vector3::new(0, 0, 0), 12, 5, 1, false, 0);

        // 12-sided polygon should have more blocks than triangle (approaching a circle)
        assert!(dodecagon.len() > triangle.len());
    }

    #[test]
    fn test_hollow_polygon() {
        let solid = generate_polygon_positions(Vector3::new(0, 0, 0), 6, 5, 1, false, 0);
        let hollow = generate_polygon_positions(Vector3::new(0, 0, 0), 6, 5, 1, true, 0);

        // Hollow should have fewer blocks
        assert!(hollow.len() < solid.len());
    }

    #[test]
    fn test_rotation() {
        let rot0 = generate_polygon_positions(Vector3::new(0, 0, 0), 4, 5, 1, false, 0);
        let rot45 = generate_polygon_positions(Vector3::new(0, 0, 0), 4, 5, 1, false, 45);

        // Same number of blocks regardless of rotation
        // (may differ slightly due to voxelization, so allow small difference)
        let diff = (rot0.len() as i32 - rot45.len() as i32).abs();
        assert!(diff < 5);
    }

    #[test]
    fn test_invalid_params() {
        let positions = generate_polygon_positions(Vector3::new(0, 0, 0), 2, 0, 0, false, 0);
        assert!(positions.is_empty());
    }
}

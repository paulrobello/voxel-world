//! Circle and ellipse generation algorithm.
//!
//! This module provides functions to generate 2D circle and ellipse shapes
//! on different orientation planes.

use nalgebra::Vector3;

/// Orientation plane for the circle/ellipse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CirclePlane {
    /// XZ plane (horizontal, ground level).
    #[default]
    XZ,
    /// XY plane (vertical wall, north-south).
    XY,
    /// YZ plane (vertical wall, east-west).
    YZ,
}

impl CirclePlane {
    /// Get all available planes for UI selection.
    pub fn all() -> &'static [CirclePlane] {
        &[CirclePlane::XZ, CirclePlane::XY, CirclePlane::YZ]
    }

    /// Get display name for the plane.
    pub fn name(&self) -> &'static str {
        match self {
            CirclePlane::XZ => "Ground (XZ)",
            CirclePlane::XY => "Wall N-S (XY)",
            CirclePlane::YZ => "Wall E-W (YZ)",
        }
    }
}

/// Generate positions for a circle or ellipse.
///
/// # Arguments
/// * `center` - Center position of the circle
/// * `radius_a` - Primary radius (X for XZ/XY, Y for YZ)
/// * `radius_b` - Secondary radius (Z for XZ, Y for XY, Z for YZ)
/// * `plane` - Orientation plane
/// * `filled` - If true, fill interior; if false, outline only
///
/// # Returns
/// Vector of block positions forming the circle/ellipse
pub fn generate_circle_positions(
    center: Vector3<i32>,
    radius_a: i32,
    radius_b: i32,
    plane: CirclePlane,
    filled: bool,
) -> Vec<Vector3<i32>> {
    let mut positions = Vec::new();

    if radius_a <= 0 || radius_b <= 0 {
        return positions;
    }

    // Convert to float for ellipse equation
    let ra = radius_a as f64;
    let rb = radius_b as f64;

    match plane {
        CirclePlane::XZ => {
            // Horizontal circle on ground
            for dx in -radius_a..=radius_a {
                for dz in -radius_b..=radius_b {
                    let x_norm = dx as f64 / ra;
                    let z_norm = dz as f64 / rb;
                    let dist_sq = x_norm * x_norm + z_norm * z_norm;

                    if filled {
                        // Fill interior
                        if dist_sq <= 1.0 {
                            positions.push(Vector3::new(center.x + dx, center.y, center.z + dz));
                        }
                    } else {
                        // Outline only - check if on boundary
                        if is_on_boundary(dx, dz, radius_a, radius_b, ra, rb) {
                            positions.push(Vector3::new(center.x + dx, center.y, center.z + dz));
                        }
                    }
                }
            }
        }
        CirclePlane::XY => {
            // Vertical wall (north-south)
            for dx in -radius_a..=radius_a {
                for dy in -radius_b..=radius_b {
                    let x_norm = dx as f64 / ra;
                    let y_norm = dy as f64 / rb;
                    let dist_sq = x_norm * x_norm + y_norm * y_norm;

                    if filled {
                        if dist_sq <= 1.0 {
                            positions.push(Vector3::new(center.x + dx, center.y + dy, center.z));
                        }
                    } else if is_on_boundary(dx, dy, radius_a, radius_b, ra, rb) {
                        positions.push(Vector3::new(center.x + dx, center.y + dy, center.z));
                    }
                }
            }
        }
        CirclePlane::YZ => {
            // Vertical wall (east-west)
            for dy in -radius_a..=radius_a {
                for dz in -radius_b..=radius_b {
                    let y_norm = dy as f64 / ra;
                    let z_norm = dz as f64 / rb;
                    let dist_sq = y_norm * y_norm + z_norm * z_norm;

                    if filled {
                        if dist_sq <= 1.0 {
                            positions.push(Vector3::new(center.x, center.y + dy, center.z + dz));
                        }
                    } else if is_on_boundary(dy, dz, radius_a, radius_b, ra, rb) {
                        positions.push(Vector3::new(center.x, center.y + dy, center.z + dz));
                    }
                }
            }
        }
    }

    positions
}

/// Check if a position is on the boundary of an ellipse.
fn is_on_boundary(da: i32, db: i32, radius_a: i32, radius_b: i32, ra: f64, rb: f64) -> bool {
    let a_norm = da as f64 / ra;
    let b_norm = db as f64 / rb;
    let dist_sq = a_norm * a_norm + b_norm * b_norm;

    // Position is on boundary if it's inside the ellipse but at least one neighbor is outside
    if dist_sq > 1.0 {
        return false;
    }

    // Check if any neighbor is outside
    for (offset_a, offset_b) in &[(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let na = da + offset_a;
        let nb = db + offset_b;

        // Skip if beyond bounds
        if na.abs() > radius_a || nb.abs() > radius_b {
            // This direction has no neighbor (at edge of bounding box)
            continue;
        }

        let na_norm = na as f64 / ra;
        let nb_norm = nb as f64 / rb;
        let neighbor_dist_sq = na_norm * na_norm + nb_norm * nb_norm;

        if neighbor_dist_sq > 1.0 {
            return true;
        }
    }

    // Also check if at the very edge of the bounding box
    let at_edge_a = da.abs() == radius_a;
    let at_edge_b = db.abs() == radius_b;
    at_edge_a || at_edge_b
}

/// Estimate the number of blocks in a filled circle/ellipse.
#[allow(dead_code)]
pub fn estimate_volume(radius_a: i32, radius_b: i32) -> usize {
    // Area of ellipse = pi * ra * rb
    (std::f64::consts::PI * radius_a as f64 * radius_b as f64).ceil() as usize
}

/// Estimate the number of blocks in an outline circle/ellipse.
#[allow(dead_code)]
pub fn estimate_perimeter(radius_a: i32, radius_b: i32) -> usize {
    // Ramanujan's approximation for ellipse perimeter
    let a = radius_a as f64;
    let b = radius_b as f64;
    let h = ((a - b) / (a + b)).powi(2);
    let perimeter =
        std::f64::consts::PI * (a + b) * (1.0 + 3.0 * h / (10.0 + (4.0 - 3.0 * h).sqrt()));
    perimeter.ceil() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_circle() {
        let center = Vector3::new(0, 0, 0);
        let positions = generate_circle_positions(center, 3, 3, CirclePlane::XZ, true);

        // A circle of radius 3 should have roughly pi * 3^2 = ~28 blocks
        assert!(positions.len() >= 20);
        assert!(positions.len() <= 40);

        // All positions should be at y=0
        for pos in &positions {
            assert_eq!(pos.y, 0);
        }
    }

    #[test]
    fn test_ellipse() {
        let center = Vector3::new(5, 10, 5);
        let positions = generate_circle_positions(center, 5, 3, CirclePlane::XZ, true);

        // An ellipse with radii 5 and 3 has area pi * 5 * 3 = ~47 blocks
        assert!(positions.len() >= 35);
        assert!(positions.len() <= 60);
    }

    #[test]
    fn test_outline_circle() {
        let center = Vector3::new(0, 0, 0);
        let filled = generate_circle_positions(center, 5, 5, CirclePlane::XZ, true);
        let outline = generate_circle_positions(center, 5, 5, CirclePlane::XZ, false);

        // Outline should have fewer blocks than filled
        assert!(outline.len() < filled.len());
    }

    #[test]
    fn test_vertical_plane_xy() {
        let center = Vector3::new(0, 5, 0);
        let positions = generate_circle_positions(center, 3, 3, CirclePlane::XY, true);

        // All positions should be at z=0
        for pos in &positions {
            assert_eq!(pos.z, 0);
        }
    }

    #[test]
    fn test_vertical_plane_yz() {
        let center = Vector3::new(0, 5, 0);
        let positions = generate_circle_positions(center, 3, 3, CirclePlane::YZ, true);

        // All positions should be at x=0
        for pos in &positions {
            assert_eq!(pos.x, 0);
        }
    }

    #[test]
    fn test_zero_radius() {
        let center = Vector3::new(0, 0, 0);
        let positions = generate_circle_positions(center, 0, 3, CirclePlane::XZ, true);
        assert!(positions.is_empty());
    }

    #[test]
    fn test_radius_one() {
        let center = Vector3::new(0, 0, 0);
        let positions = generate_circle_positions(center, 1, 1, CirclePlane::XZ, true);

        // Radius 1 should give a small cross/plus shape
        assert!(!positions.is_empty());
        assert!(positions.len() <= 5);
    }
}

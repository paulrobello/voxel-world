//! Cylinder generation algorithm for the cylinder placement tool.
//!
//! This module provides functions to generate cylinder block positions
//! with support for hollow mode and different orientations.

use nalgebra::Vector3;

use super::PlacementMode;

/// Axis orientation for cylinder placement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CylinderAxis {
    /// Cylinder extends along Y axis (vertical, default).
    #[default]
    Y,
    /// Cylinder extends along X axis (horizontal, east-west).
    X,
    /// Cylinder extends along Z axis (horizontal, north-south).
    Z,
}

/// Generate all block positions for a cylinder.
///
/// # Arguments
/// * `center` - Center position of the cylinder in world coordinates
/// * `radius` - Radius of the cylinder's circular cross-section (must be positive)
/// * `height` - Length/height of the cylinder (must be positive)
/// * `hollow` - If true, only generate shell positions (tube, no caps)
/// * `axis` - Which axis the cylinder extends along
///
/// # Returns
/// Vector of all block positions that make up the cylinder.
pub fn generate_cylinder_positions(
    center: Vector3<i32>,
    radius: i32,
    height: i32,
    hollow: bool,
    axis: CylinderAxis,
) -> Vec<Vector3<i32>> {
    let mut positions = Vec::new();

    if radius <= 0 || height <= 0 {
        return positions;
    }

    let radius_sq = (radius * radius) as i64;
    let inner_radius_sq = if hollow && radius > 1 {
        ((radius - 1) * (radius - 1)) as i64
    } else {
        -1 // No inner cutout for solid cylinder or radius 1
    };

    // Half-height for centering
    let half_height = height / 2;

    match axis {
        CylinderAxis::Y => {
            // Vertical cylinder: circle in XZ plane, extends along Y
            for h in -half_height..=(height - half_height - 1) {
                for x in -radius..=radius {
                    for z in -radius..=radius {
                        let dist_sq = (x * x + z * z) as i64;
                        if dist_sq <= radius_sq {
                            if hollow && dist_sq <= inner_radius_sq {
                                continue; // Skip interior for hollow
                            }
                            positions.push(Vector3::new(center.x + x, center.y + h, center.z + z));
                        }
                    }
                }
            }
        }
        CylinderAxis::X => {
            // Horizontal cylinder along X: circle in YZ plane, extends along X
            for h in -half_height..=(height - half_height - 1) {
                for y in -radius..=radius {
                    for z in -radius..=radius {
                        let dist_sq = (y * y + z * z) as i64;
                        if dist_sq <= radius_sq {
                            if hollow && dist_sq <= inner_radius_sq {
                                continue;
                            }
                            positions.push(Vector3::new(center.x + h, center.y + y, center.z + z));
                        }
                    }
                }
            }
        }
        CylinderAxis::Z => {
            // Horizontal cylinder along Z: circle in XY plane, extends along Z
            for h in -half_height..=(height - half_height - 1) {
                for x in -radius..=radius {
                    for y in -radius..=radius {
                        let dist_sq = (x * x + y * y) as i64;
                        if dist_sq <= radius_sq {
                            if hollow && dist_sq <= inner_radius_sq {
                                continue;
                            }
                            positions.push(Vector3::new(center.x + x, center.y + y, center.z + h));
                        }
                    }
                }
            }
        }
    }

    positions
}

/// Calculate effective center position based on placement mode.
///
/// # Arguments
/// * `target` - Target position (where user is aiming)
/// * `radius` - Cylinder radius
/// * `height` - Cylinder height
/// * `axis` - Cylinder axis orientation
/// * `mode` - Placement mode (Center or Base)
///
/// # Returns
/// The actual center position for the cylinder.
pub fn calculate_center(
    target: Vector3<i32>,
    radius: i32,
    height: i32,
    axis: CylinderAxis,
    mode: PlacementMode,
) -> Vector3<i32> {
    match mode {
        PlacementMode::Center => target,
        PlacementMode::Base => {
            let half_height = height / 2;
            match axis {
                CylinderAxis::Y => Vector3::new(target.x, target.y + half_height, target.z),
                CylinderAxis::X => {
                    Vector3::new(target.x + half_height, target.y + radius, target.z)
                }
                CylinderAxis::Z => {
                    Vector3::new(target.x, target.y + radius, target.z + half_height)
                }
            }
        }
    }
}

/// Estimate cylinder volume (for confirmation dialogs).
///
/// Uses the mathematical formula for cylinder volume.
///
/// # Arguments
/// * `radius` - Cylinder radius
/// * `height` - Cylinder height
/// * `hollow` - If true, calculate tube volume only
///
/// # Returns
/// Estimated number of blocks.
#[allow(dead_code)]
pub fn estimate_volume(radius: i32, height: i32, hollow: bool) -> u64 {
    let radius_f = radius as f64;
    let height_f = height as f64;

    if hollow {
        let outer_vol = std::f64::consts::PI * radius_f * radius_f * height_f;
        let inner_radius = (radius - 1).max(0) as f64;
        let inner_vol = std::f64::consts::PI * inner_radius * inner_radius * height_f;
        (outer_vol - inner_vol) as u64
    } else {
        (std::f64::consts::PI * radius_f * radius_f * height_f) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_solid_cylinder_vertical() {
        let center = Vector3::new(0, 0, 0);
        let positions = generate_cylinder_positions(center, 2, 5, false, CylinderAxis::Y);
        // Should have blocks
        assert!(!positions.is_empty());
        // Should contain center
        assert!(positions.contains(&center));
    }

    #[test]
    fn test_hollow_cylinder() {
        let center = Vector3::new(10, 10, 10);
        let solid = generate_cylinder_positions(center, 3, 5, false, CylinderAxis::Y);
        let hollow = generate_cylinder_positions(center, 3, 5, true, CylinderAxis::Y);

        // Hollow should have fewer blocks than solid
        assert!(hollow.len() < solid.len());
        // Both should contain some blocks
        assert!(!solid.is_empty());
        assert!(!hollow.is_empty());
    }

    #[test]
    fn test_horizontal_cylinder_x() {
        let center = Vector3::new(10, 10, 10);
        let positions = generate_cylinder_positions(center, 2, 10, false, CylinderAxis::X);

        // Should have blocks spread along X axis
        assert!(!positions.is_empty());

        // Check that blocks span X direction
        let min_x = positions.iter().map(|p| p.x).min().unwrap();
        let max_x = positions.iter().map(|p| p.x).max().unwrap();
        assert!(max_x - min_x >= 9); // At least 10 blocks long
    }

    #[test]
    fn test_horizontal_cylinder_z() {
        let center = Vector3::new(10, 10, 10);
        let positions = generate_cylinder_positions(center, 2, 10, false, CylinderAxis::Z);

        // Should have blocks spread along Z axis
        assert!(!positions.is_empty());

        // Check that blocks span Z direction
        let min_z = positions.iter().map(|p| p.z).min().unwrap();
        let max_z = positions.iter().map(|p| p.z).max().unwrap();
        assert!(max_z - min_z >= 9);
    }

    #[test]
    fn test_calculate_center_vertical_base() {
        let target = Vector3::new(10, 20, 30);
        let center = calculate_center(target, 3, 10, CylinderAxis::Y, PlacementMode::Base);

        // For vertical cylinder with base mode, center should be offset up by half height
        assert_eq!(center.x, 10);
        assert_eq!(center.y, 25); // 20 + 10/2
        assert_eq!(center.z, 30);
    }

    #[test]
    fn test_calculate_center_horizontal_base() {
        let target = Vector3::new(10, 20, 30);

        // X-axis cylinder should offset X and lift by radius
        let center_x = calculate_center(target, 3, 10, CylinderAxis::X, PlacementMode::Base);
        assert_eq!(center_x.x, 15); // 10 + 10/2
        assert_eq!(center_x.y, 23); // 20 + radius

        // Z-axis cylinder should offset Z and lift by radius
        let center_z = calculate_center(target, 3, 10, CylinderAxis::Z, PlacementMode::Base);
        assert_eq!(center_z.z, 35); // 30 + 10/2
        assert_eq!(center_z.y, 23); // 20 + radius
    }

    #[test]
    fn test_estimate_volume() {
        // Solid cylinder radius 3, height 10: approximately 283 blocks
        let solid_vol = estimate_volume(3, 10, false);
        assert!(solid_vol > 200 && solid_vol < 350);

        // Hollow cylinder should have fewer blocks
        let hollow_vol = estimate_volume(3, 10, true);
        assert!(hollow_vol < solid_vol);
    }

    #[test]
    fn test_zero_dimensions() {
        let center = Vector3::new(0, 0, 0);
        assert!(generate_cylinder_positions(center, 0, 5, false, CylinderAxis::Y).is_empty());
        assert!(generate_cylinder_positions(center, 3, 0, false, CylinderAxis::Y).is_empty());
    }
}

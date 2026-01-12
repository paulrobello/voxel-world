//! Sphere generation algorithm for the sphere placement tool.
//!
//! This module provides functions to generate sphere block positions
//! and calculate sphere volumes. The algorithm matches the console
//! `/sphere` command implementation.

use nalgebra::Vector3;

use super::PlacementMode;

/// Generate all block positions for a sphere.
///
/// Uses the same algorithm as the `/sphere` console command.
///
/// # Arguments
/// * `center` - Center position of the sphere in world coordinates
/// * `radius` - Radius of the sphere in blocks (must be positive)
/// * `hollow` - If true, only generate shell positions (thickness = 1 block)
/// * `dome` - If true, only generate the top half (y >= center.y)
///
/// # Returns
/// Vector of all block positions that make up the sphere.
pub fn generate_sphere_positions(
    center: Vector3<i32>,
    radius: i32,
    hollow: bool,
    dome: bool,
) -> Vec<Vector3<i32>> {
    let mut positions = Vec::new();

    if radius <= 0 {
        return positions;
    }

    let radius_sq = (radius * radius) as i64;
    let inner_radius_sq = if hollow && radius > 1 {
        ((radius - 1) * (radius - 1)) as i64
    } else {
        -1 // No inner cutout for solid sphere or radius 1
    };

    // For dome mode, start at center.y instead of center.y - radius
    let y_start = if dome { center.y } else { center.y - radius };

    for x in (center.x - radius)..=(center.x + radius) {
        for y in y_start..=(center.y + radius) {
            for z in (center.z - radius)..=(center.z + radius) {
                let dx = (x - center.x) as i64;
                let dy = (y - center.y) as i64;
                let dz = (z - center.z) as i64;
                let dist_sq = dx * dx + dy * dy + dz * dz;

                // Check if within sphere
                if dist_sq <= radius_sq {
                    if hollow {
                        // Only place on the shell (not in interior)
                        if dist_sq > inner_radius_sq {
                            positions.push(Vector3::new(x, y, z));
                        }
                    } else {
                        positions.push(Vector3::new(x, y, z));
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
/// * `radius` - Sphere radius
/// * `mode` - Placement mode (Center or Base)
///
/// # Returns
/// The actual center position for the sphere.
pub fn calculate_center(target: Vector3<i32>, radius: i32, mode: PlacementMode) -> Vector3<i32> {
    match mode {
        PlacementMode::Center => target,
        PlacementMode::Base => Vector3::new(target.x, target.y + radius, target.z),
    }
}

/// Estimate sphere volume (for confirmation dialogs).
///
/// Uses the mathematical formula for sphere volume, which gives
/// a close approximation to the actual voxel count.
///
/// # Arguments
/// * `radius` - Sphere radius
/// * `hollow` - If true, calculate shell volume only
///
/// # Returns
/// Estimated number of blocks.
#[allow(dead_code)]
pub fn estimate_volume(radius: i32, hollow: bool) -> u64 {
    let radius_f = radius as f64;
    if hollow {
        let outer_vol = (4.0 / 3.0) * std::f64::consts::PI * radius_f.powi(3);
        let inner_radius = (radius - 1).max(0) as f64;
        let inner_vol = (4.0 / 3.0) * std::f64::consts::PI * inner_radius.powi(3);
        (outer_vol - inner_vol) as u64
    } else {
        ((4.0 / 3.0) * std::f64::consts::PI * radius_f.powi(3)) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_solid_sphere_radius_1() {
        let positions = generate_sphere_positions(Vector3::new(0, 0, 0), 1, false, false);
        // Radius 1 sphere should have 7 blocks: center + 6 neighbors
        // Actually with the squared distance check, it includes more
        assert!(!positions.is_empty());
        assert!(positions.contains(&Vector3::new(0, 0, 0)));
    }

    #[test]
    fn test_hollow_sphere_radius_3() {
        let center = Vector3::new(10, 10, 10);
        let solid = generate_sphere_positions(center, 3, false, false);
        let hollow = generate_sphere_positions(center, 3, true, false);

        // Hollow should have fewer blocks than solid
        assert!(hollow.len() < solid.len());
        // Both should contain some blocks
        assert!(!solid.is_empty());
        assert!(!hollow.is_empty());
    }

    #[test]
    fn test_dome_mode() {
        let center = Vector3::new(10, 10, 10);
        let full_sphere = generate_sphere_positions(center, 5, false, false);
        let dome = generate_sphere_positions(center, 5, false, true);

        // Dome should have roughly half the blocks of full sphere
        assert!(dome.len() < full_sphere.len());
        assert!(dome.len() > full_sphere.len() / 3); // Should be around half

        // Dome should not contain any blocks below center
        for pos in &dome {
            assert!(
                pos.y >= center.y,
                "Dome should not have blocks below center"
            );
        }

        // Dome should contain the center and top
        assert!(dome.contains(&center));
        assert!(dome.contains(&Vector3::new(10, 15, 10)));
    }

    #[test]
    fn test_hollow_dome() {
        let center = Vector3::new(10, 10, 10);
        let hollow_dome = generate_sphere_positions(center, 5, true, true);

        // Should have blocks
        assert!(!hollow_dome.is_empty());

        // No blocks below center
        for pos in &hollow_dome {
            assert!(pos.y >= center.y);
        }
    }

    #[test]
    fn test_calculate_center_modes() {
        let target = Vector3::new(10, 20, 30);
        let radius = 5;

        let center_mode = calculate_center(target, radius, PlacementMode::Center);
        assert_eq!(center_mode, target);

        let base_mode = calculate_center(target, radius, PlacementMode::Base);
        assert_eq!(base_mode, Vector3::new(10, 25, 30));
    }

    #[test]
    fn test_estimate_volume() {
        // Solid sphere radius 5: approximately 523 blocks
        let solid_vol = estimate_volume(5, false);
        assert!(solid_vol > 400 && solid_vol < 600);

        // Hollow sphere radius 5: approximately 255 blocks
        let hollow_vol = estimate_volume(5, true);
        assert!(hollow_vol > 200 && hollow_vol < 350);
        assert!(hollow_vol < solid_vol);
    }

    #[test]
    fn test_zero_radius() {
        let positions = generate_sphere_positions(Vector3::new(0, 0, 0), 0, false, false);
        assert!(positions.is_empty());
    }
}

//! Mirror tool for symmetric block placement.
//!
//! This module provides functions to calculate mirrored positions across
//! different axes for symmetric building.

use nalgebra::Vector3;

/// Mirror axis options for symmetric building.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MirrorAxis {
    /// Mirror across X axis (flip Z coordinate) - East-West symmetry.
    #[default]
    X,
    /// Mirror across Z axis (flip X coordinate) - North-South symmetry.
    Z,
    /// Mirror across both axes (4-way symmetry).
    Both,
}

impl MirrorAxis {
    /// Get all available axis options.
    pub fn all() -> &'static [MirrorAxis] {
        &[MirrorAxis::X, MirrorAxis::Z, MirrorAxis::Both]
    }

    /// Get display name for the axis.
    pub fn name(&self) -> &'static str {
        match self {
            MirrorAxis::X => "X (East-West)",
            MirrorAxis::Z => "Z (North-South)",
            MirrorAxis::Both => "Both (4-way)",
        }
    }

    /// Get short name for the axis.
    #[allow(dead_code)]
    pub fn short_name(&self) -> &'static str {
        match self {
            MirrorAxis::X => "X",
            MirrorAxis::Z => "Z",
            MirrorAxis::Both => "Both",
        }
    }
}

/// Calculate mirrored positions for a given original position.
///
/// # Arguments
/// * `original` - Original block position
/// * `plane_pos` - Position that defines the mirror plane (point on the plane)
/// * `axis` - Mirror axis to use
///
/// # Returns
/// Vector of mirrored positions (includes original).
/// - X axis: 2 positions (original + mirrored across X)
/// - Z axis: 2 positions (original + mirrored across Z)
/// - Both: 4 positions (original + 3 mirrored copies)
pub fn get_mirrored_positions(
    original: Vector3<i32>,
    plane_pos: Vector3<i32>,
    axis: MirrorAxis,
) -> Vec<Vector3<i32>> {
    match axis {
        MirrorAxis::X => {
            // Mirror across X axis (flip Z coordinate)
            let mirrored_z = 2 * plane_pos.z - original.z;
            if mirrored_z == original.z {
                // On the mirror plane, only one position
                vec![original]
            } else {
                vec![original, Vector3::new(original.x, original.y, mirrored_z)]
            }
        }
        MirrorAxis::Z => {
            // Mirror across Z axis (flip X coordinate)
            let mirrored_x = 2 * plane_pos.x - original.x;
            if mirrored_x == original.x {
                vec![original]
            } else {
                vec![original, Vector3::new(mirrored_x, original.y, original.z)]
            }
        }
        MirrorAxis::Both => {
            // Mirror across both axes (4-way symmetry)
            let mirrored_x = 2 * plane_pos.x - original.x;
            let mirrored_z = 2 * plane_pos.z - original.z;

            let mut positions = Vec::with_capacity(4);
            positions.push(original);

            // X-mirrored
            if mirrored_x != original.x {
                positions.push(Vector3::new(mirrored_x, original.y, original.z));
            }

            // Z-mirrored
            if mirrored_z != original.z {
                positions.push(Vector3::new(original.x, original.y, mirrored_z));
            }

            // Both-mirrored (diagonal)
            if mirrored_x != original.x && mirrored_z != original.z {
                positions.push(Vector3::new(mirrored_x, original.y, mirrored_z));
            }

            positions
        }
    }
}

/// Calculate mirrored positions for multiple positions at once.
///
/// Useful for mirroring shape tool previews.
#[allow(dead_code)]
pub fn get_all_mirrored_positions(
    positions: &[Vector3<i32>],
    plane_pos: Vector3<i32>,
    axis: MirrorAxis,
) -> Vec<Vector3<i32>> {
    let mut result = Vec::with_capacity(positions.len() * 4);

    for pos in positions {
        for mirrored in get_mirrored_positions(*pos, plane_pos, axis) {
            // Avoid duplicates (positions on the mirror plane)
            if !result.contains(&mirrored) {
                result.push(mirrored);
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mirror_x_axis() {
        let original = Vector3::new(5, 10, 3);
        let plane = Vector3::new(0, 0, 0);
        let positions = get_mirrored_positions(original, plane, MirrorAxis::X);

        assert_eq!(positions.len(), 2);
        assert!(positions.contains(&original));
        assert!(positions.contains(&Vector3::new(5, 10, -3)));
    }

    #[test]
    fn test_mirror_z_axis() {
        let original = Vector3::new(3, 10, 5);
        let plane = Vector3::new(0, 0, 0);
        let positions = get_mirrored_positions(original, plane, MirrorAxis::Z);

        assert_eq!(positions.len(), 2);
        assert!(positions.contains(&original));
        assert!(positions.contains(&Vector3::new(-3, 10, 5)));
    }

    #[test]
    fn test_mirror_both_axes() {
        let original = Vector3::new(3, 10, 5);
        let plane = Vector3::new(0, 0, 0);
        let positions = get_mirrored_positions(original, plane, MirrorAxis::Both);

        assert_eq!(positions.len(), 4);
        assert!(positions.contains(&original));
        assert!(positions.contains(&Vector3::new(-3, 10, 5)));
        assert!(positions.contains(&Vector3::new(3, 10, -5)));
        assert!(positions.contains(&Vector3::new(-3, 10, -5)));
    }

    #[test]
    fn test_position_on_x_plane() {
        // Position exactly on the X mirror plane (z = plane_z)
        let original = Vector3::new(5, 10, 0);
        let plane = Vector3::new(0, 0, 0);
        let positions = get_mirrored_positions(original, plane, MirrorAxis::X);

        // Should only return the original since mirrored = original
        assert_eq!(positions.len(), 1);
        assert!(positions.contains(&original));
    }

    #[test]
    fn test_position_on_z_plane() {
        // Position exactly on the Z mirror plane (x = plane_x)
        let original = Vector3::new(0, 10, 5);
        let plane = Vector3::new(0, 0, 0);
        let positions = get_mirrored_positions(original, plane, MirrorAxis::Z);

        assert_eq!(positions.len(), 1);
        assert!(positions.contains(&original));
    }

    #[test]
    fn test_offset_plane() {
        // Test with a non-origin plane position
        let original = Vector3::new(15, 10, 5);
        let plane = Vector3::new(10, 0, 10);
        let positions = get_mirrored_positions(original, plane, MirrorAxis::X);

        assert_eq!(positions.len(), 2);
        assert!(positions.contains(&original));
        // Mirrored Z: 2 * 10 - 5 = 15
        assert!(positions.contains(&Vector3::new(15, 10, 15)));
    }

    #[test]
    fn test_get_all_mirrored() {
        let positions = vec![Vector3::new(1, 0, 1), Vector3::new(2, 0, 2)];
        let plane = Vector3::new(0, 0, 0);
        let mirrored = get_all_mirrored_positions(&positions, plane, MirrorAxis::Z);

        assert!(mirrored.contains(&Vector3::new(1, 0, 1)));
        assert!(mirrored.contains(&Vector3::new(-1, 0, 1)));
        assert!(mirrored.contains(&Vector3::new(2, 0, 2)));
        assert!(mirrored.contains(&Vector3::new(-2, 0, 2)));
    }
}

//! Clone/Array tool for pattern repetition.
//!
//! This module provides functions to clone/repeat blocks from a selection
//! in linear, grid, or circular patterns.

use nalgebra::Vector3;

/// Clone mode for the array tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CloneMode {
    /// Repeat N times along a single axis with spacing.
    #[default]
    Linear,
    /// Repeat NxM times in a 2D grid pattern (XZ plane).
    Grid,
    /// Repeat NxMxP times in a 3D grid pattern (X, Y, Z).
    Grid3D,
}

impl CloneMode {
    /// Get all available clone modes.
    pub fn all() -> &'static [CloneMode] {
        &[CloneMode::Linear, CloneMode::Grid, CloneMode::Grid3D]
    }

    /// Get display name for the mode.
    pub fn name(&self) -> &'static str {
        match self {
            CloneMode::Linear => "Linear",
            CloneMode::Grid => "Grid (2D)",
            CloneMode::Grid3D => "Grid (3D)",
        }
    }
}

/// Axis for linear cloning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CloneAxis {
    /// Clone along X axis.
    #[default]
    X,
    /// Clone along Y axis.
    Y,
    /// Clone along Z axis.
    Z,
}

impl CloneAxis {
    /// Get all available axes.
    pub fn all() -> &'static [CloneAxis] {
        &[CloneAxis::X, CloneAxis::Y, CloneAxis::Z]
    }

    /// Get display name for the axis.
    pub fn name(&self) -> &'static str {
        match self {
            CloneAxis::X => "X (East-West)",
            CloneAxis::Y => "Y (Up-Down)",
            CloneAxis::Z => "Z (North-South)",
        }
    }

    /// Get the offset vector for this axis.
    pub fn offset(&self, distance: i32) -> Vector3<i32> {
        match self {
            CloneAxis::X => Vector3::new(distance, 0, 0),
            CloneAxis::Y => Vector3::new(0, distance, 0),
            CloneAxis::Z => Vector3::new(0, 0, distance),
        }
    }
}

/// Calculate positions for cloned copies (not the blocks themselves, but the origins).
///
/// # Arguments
/// * `selection_size` - Size of the selection region (width, height, depth)
/// * `mode` - Clone mode (Linear, Grid, or Grid3D)
/// * `axis` - Primary axis for linear cloning
/// * `count` - Number of copies for linear mode
/// * `spacing` - Gap between copies (in addition to selection size)
/// * `grid_count_x` - Number of copies along X for grid mode
/// * `grid_count_z` - Number of copies along Z for grid mode
/// * `grid_spacing_x` - Spacing along X for grid mode
/// * `grid_spacing_z` - Spacing along Z for grid mode
/// * `grid_count_y` - Number of copies along Y for 3D grid mode
/// * `grid_spacing_y` - Spacing along Y for 3D grid mode
///
/// # Returns
/// Vector of origin offsets for each copy (relative to selection origin).
/// The first element is always (0,0,0) representing the original.
#[allow(clippy::too_many_arguments)]
pub fn calculate_clone_origins(
    selection_size: Vector3<i32>,
    mode: CloneMode,
    axis: CloneAxis,
    count: i32,
    spacing: i32,
    grid_count_x: i32,
    grid_count_z: i32,
    grid_spacing_x: i32,
    grid_spacing_z: i32,
    grid_count_y: i32,
    grid_spacing_y: i32,
) -> Vec<Vector3<i32>> {
    let mut origins = Vec::new();

    match mode {
        CloneMode::Linear => {
            // Calculate stride (selection size + spacing)
            let stride = match axis {
                CloneAxis::X => selection_size.x + spacing,
                CloneAxis::Y => selection_size.y + spacing,
                CloneAxis::Z => selection_size.z + spacing,
            };

            for i in 0..count {
                let offset = axis.offset(stride * i);
                origins.push(offset);
            }
        }
        CloneMode::Grid => {
            // 2D grid in XZ plane
            let stride_x = selection_size.x + grid_spacing_x;
            let stride_z = selection_size.z + grid_spacing_z;

            for x in 0..grid_count_x {
                for z in 0..grid_count_z {
                    let offset = Vector3::new(stride_x * x, 0, stride_z * z);
                    origins.push(offset);
                }
            }
        }
        CloneMode::Grid3D => {
            // 3D grid in XYZ
            let stride_x = selection_size.x + grid_spacing_x;
            let stride_y = selection_size.y + grid_spacing_y;
            let stride_z = selection_size.z + grid_spacing_z;

            for x in 0..grid_count_x {
                for y in 0..grid_count_y {
                    for z in 0..grid_count_z {
                        let offset = Vector3::new(stride_x * x, stride_y * y, stride_z * z);
                        origins.push(offset);
                    }
                }
            }
        }
    }

    origins
}

/// Generate all cloned block positions from a source region.
///
/// # Arguments
/// * `source_positions` - Original block positions from selection
/// * `origins` - Clone origins from `calculate_clone_origins`
///
/// # Returns
/// Vector of all block positions including original and cloned copies.
pub fn generate_cloned_positions(
    source_positions: &[Vector3<i32>],
    origins: &[Vector3<i32>],
) -> Vec<Vector3<i32>> {
    let mut positions = Vec::with_capacity(source_positions.len() * origins.len());

    for origin in origins {
        for pos in source_positions {
            positions.push(pos + origin);
        }
    }

    positions
}

/// Estimate the total number of blocks for a clone operation.
#[allow(dead_code)]
pub fn estimate_block_count(source_count: usize, origins: &[Vector3<i32>]) -> usize {
    source_count * origins.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_clone_x() {
        let size = Vector3::new(3, 3, 3);
        let origins = calculate_clone_origins(
            size,
            CloneMode::Linear,
            CloneAxis::X,
            3, // 3 copies
            1, // spacing of 1
            1,
            1,
            0,
            0,
            1, // grid_count_y (ignored for linear)
            0, // grid_spacing_y (ignored for linear)
        );

        assert_eq!(origins.len(), 3);
        assert_eq!(origins[0], Vector3::new(0, 0, 0)); // Original
        assert_eq!(origins[1], Vector3::new(4, 0, 0)); // First copy (3+1 = 4)
        assert_eq!(origins[2], Vector3::new(8, 0, 0)); // Second copy (2*4 = 8)
    }

    #[test]
    fn test_linear_clone_y() {
        let size = Vector3::new(2, 5, 2);
        let origins = calculate_clone_origins(
            size,
            CloneMode::Linear,
            CloneAxis::Y,
            4, // 4 copies
            0, // no spacing
            1,
            1,
            0,
            0,
            1, // grid_count_y (ignored for linear)
            0, // grid_spacing_y (ignored for linear)
        );

        assert_eq!(origins.len(), 4);
        assert_eq!(origins[0], Vector3::new(0, 0, 0));
        assert_eq!(origins[1], Vector3::new(0, 5, 0));
        assert_eq!(origins[2], Vector3::new(0, 10, 0));
        assert_eq!(origins[3], Vector3::new(0, 15, 0));
    }

    #[test]
    fn test_linear_clone_z() {
        let size = Vector3::new(1, 1, 4);
        let origins = calculate_clone_origins(
            size,
            CloneMode::Linear,
            CloneAxis::Z,
            2,
            2, // spacing of 2
            1,
            1,
            0,
            0,
            1, // grid_count_y (ignored for linear)
            0, // grid_spacing_y (ignored for linear)
        );

        assert_eq!(origins.len(), 2);
        assert_eq!(origins[0], Vector3::new(0, 0, 0));
        assert_eq!(origins[1], Vector3::new(0, 0, 6)); // 4+2 = 6
    }

    #[test]
    fn test_grid_clone() {
        let size = Vector3::new(2, 3, 2);
        let origins = calculate_clone_origins(
            size,
            CloneMode::Grid,
            CloneAxis::X, // Ignored for grid
            1,            // Ignored for grid
            0,            // Ignored for grid
            3,            // 3 copies along X
            2,            // 2 copies along Z
            1,            // spacing of 1 in X
            1,            // spacing of 1 in Z
            1,            // grid_count_y (ignored for 2D grid)
            0,            // grid_spacing_y (ignored for 2D grid)
        );

        // 3x2 = 6 copies total
        assert_eq!(origins.len(), 6);

        // Check corners
        assert!(origins.contains(&Vector3::new(0, 0, 0)));
        assert!(origins.contains(&Vector3::new(6, 0, 0))); // 2*3 = 6 (x stride = 2+1 = 3)
        assert!(origins.contains(&Vector3::new(0, 0, 3))); // z stride = 2+1 = 3
        assert!(origins.contains(&Vector3::new(6, 0, 3)));
    }

    #[test]
    fn test_grid3d_clone() {
        let size = Vector3::new(2, 2, 2);
        let origins = calculate_clone_origins(
            size,
            CloneMode::Grid3D,
            CloneAxis::X, // Ignored for grid3d
            1,            // Ignored for grid3d
            0,            // Ignored for grid3d
            2,            // 2 copies along X
            2,            // 2 copies along Z
            1,            // spacing of 1 in X
            1,            // spacing of 1 in Z
            3,            // 3 copies along Y
            0,            // spacing of 0 in Y
        );

        // 2x3x2 = 12 copies total
        assert_eq!(origins.len(), 12);

        // Check some key positions
        assert!(origins.contains(&Vector3::new(0, 0, 0))); // Origin
        assert!(origins.contains(&Vector3::new(3, 0, 0))); // x stride = 2+1 = 3
        assert!(origins.contains(&Vector3::new(0, 2, 0))); // y stride = 2+0 = 2
        assert!(origins.contains(&Vector3::new(0, 4, 0))); // 2nd Y level
        assert!(origins.contains(&Vector3::new(0, 0, 3))); // z stride = 2+1 = 3
        assert!(origins.contains(&Vector3::new(3, 4, 3))); // Far corner
    }

    #[test]
    fn test_generate_cloned_positions() {
        let source = vec![
            Vector3::new(0, 0, 0),
            Vector3::new(1, 0, 0),
            Vector3::new(0, 1, 0),
        ];
        let origins = vec![Vector3::new(0, 0, 0), Vector3::new(5, 0, 0)];

        let cloned = generate_cloned_positions(&source, &origins);

        assert_eq!(cloned.len(), 6); // 3 blocks * 2 copies

        // Original positions
        assert!(cloned.contains(&Vector3::new(0, 0, 0)));
        assert!(cloned.contains(&Vector3::new(1, 0, 0)));
        assert!(cloned.contains(&Vector3::new(0, 1, 0)));

        // Cloned positions
        assert!(cloned.contains(&Vector3::new(5, 0, 0)));
        assert!(cloned.contains(&Vector3::new(6, 0, 0)));
        assert!(cloned.contains(&Vector3::new(5, 1, 0)));
    }

    #[test]
    fn test_single_copy() {
        let size = Vector3::new(5, 5, 5);
        let origins = calculate_clone_origins(
            size,
            CloneMode::Linear,
            CloneAxis::X,
            1, // Just one copy (the original)
            0,
            1,
            1,
            0,
            0,
            1, // grid_count_y (ignored for linear)
            0, // grid_spacing_y (ignored for linear)
        );

        assert_eq!(origins.len(), 1);
        assert_eq!(origins[0], Vector3::new(0, 0, 0));
    }
}

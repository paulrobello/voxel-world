//! Cone and pyramid generation algorithm.
//!
//! This module provides functions to generate cone (circular base) and
//! pyramid (square base) shapes with optional hollow mode.

use nalgebra::Vector3;

/// Shape type for the cone/pyramid tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConeShape {
    /// Circular base, tapering to a point.
    #[default]
    Cone,
    /// Square base, tapering to a point.
    Pyramid,
}

impl ConeShape {
    /// Get all available shapes.
    pub fn all() -> &'static [ConeShape] {
        &[ConeShape::Cone, ConeShape::Pyramid]
    }

    /// Get display name for the shape.
    pub fn name(&self) -> &'static str {
        match self {
            ConeShape::Cone => "Cone (circular)",
            ConeShape::Pyramid => "Pyramid (square)",
        }
    }
}

/// Generate positions for a cone or pyramid.
///
/// # Arguments
/// * `base_center` - Center of the base (bottom center for normal, top center for inverted)
/// * `base_size` - Radius for cone, half-side-length for pyramid
/// * `height` - Total height from base to apex
/// * `shape` - Cone (circular) or Pyramid (square)
/// * `hollow` - If true, only outer shell; if false, solid shape
/// * `inverted` - If true, point is at bottom; if false, point is at top
///
/// # Returns
/// Vector of block positions forming the shape
pub fn generate_cone_positions(
    base_center: Vector3<i32>,
    base_size: i32,
    height: i32,
    shape: ConeShape,
    hollow: bool,
    inverted: bool,
) -> Vec<Vector3<i32>> {
    let mut positions = Vec::new();

    if base_size < 1 || height < 1 {
        return positions;
    }

    for y_offset in 0..height {
        // Calculate the size at this height level
        // At y_offset=0 (base): size = base_size
        // At y_offset=height-1 (apex): size = 0
        let progress = if height > 1 {
            y_offset as f64 / (height - 1) as f64
        } else {
            0.0
        };

        // Size at this level (linear taper from base_size to 0)
        // progress=0 → level_size=base_size, progress=1 → level_size=0
        let level_size = (base_size as f64 * (1.0 - progress)).round() as i32;

        // Y coordinate: inverted builds downward from base_center
        let y = if inverted {
            base_center.y - y_offset
        } else {
            base_center.y + y_offset
        };

        // Generate positions for this level
        match shape {
            ConeShape::Cone => {
                // Circular cross-section
                for x_offset in -level_size..=level_size {
                    for z_offset in -level_size..=level_size {
                        let dist_sq = x_offset * x_offset + z_offset * z_offset;
                        let level_size_sq = level_size * level_size;

                        let in_circle = dist_sq <= level_size_sq;

                        // For hollow, check if inside inner circle
                        let in_inner = if hollow && level_size > 0 {
                            let inner_size = (level_size - 1).max(0);
                            let inner_size_sq = inner_size * inner_size;
                            dist_sq <= inner_size_sq
                        } else {
                            false
                        };

                        if in_circle && !in_inner {
                            let pos =
                                Vector3::new(base_center.x + x_offset, y, base_center.z + z_offset);
                            if !positions.contains(&pos) {
                                positions.push(pos);
                            }
                        }
                    }
                }
            }
            ConeShape::Pyramid => {
                // Square cross-section
                for x_offset in -level_size..=level_size {
                    for z_offset in -level_size..=level_size {
                        let on_edge_x = x_offset.abs() == level_size;
                        let on_edge_z = z_offset.abs() == level_size;

                        let in_square =
                            x_offset.abs() <= level_size && z_offset.abs() <= level_size;

                        // For hollow, only include edge blocks
                        let include = if hollow {
                            in_square && (on_edge_x || on_edge_z || level_size == 0)
                        } else {
                            in_square
                        };

                        if include {
                            let pos =
                                Vector3::new(base_center.x + x_offset, y, base_center.z + z_offset);
                            if !positions.contains(&pos) {
                                positions.push(pos);
                            }
                        }
                    }
                }
            }
        }
    }

    positions
}

/// Estimate the number of blocks in a cone/pyramid.
#[allow(dead_code)]
pub fn estimate_block_count(base_size: i32, height: i32, shape: ConeShape, hollow: bool) -> usize {
    match shape {
        ConeShape::Cone => {
            // Volume of cone = (1/3) * π * r² * h
            // For voxels, approximate
            let base_area = (std::f64::consts::PI * (base_size as f64).powi(2)) as usize;
            if hollow {
                // Shell only - perimeter * height / 2 (roughly)
                let perimeter = (2.0 * std::f64::consts::PI * base_size as f64) as usize;
                perimeter * height as usize / 2
            } else {
                base_area * height as usize / 3
            }
        }
        ConeShape::Pyramid => {
            // Volume of pyramid = (1/3) * base² * h
            let base_area = (2 * base_size + 1).pow(2) as usize;
            if hollow {
                // Shell only
                let perimeter = 4 * (2 * base_size + 1) as usize;
                perimeter * height as usize / 2
            } else {
                base_area * height as usize / 3
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cone_basic() {
        let base = Vector3::new(0, 0, 0);
        let positions = generate_cone_positions(base, 3, 5, ConeShape::Cone, false, false);

        // Should generate some positions
        assert!(!positions.is_empty());

        // Base should be at y=0
        assert!(positions.iter().any(|p| p.y == 0));

        // Apex should be at y=4 (height-1)
        assert!(positions.iter().any(|p| p.y == 4));
    }

    #[test]
    fn test_pyramid_basic() {
        let base = Vector3::new(0, 0, 0);
        let positions = generate_cone_positions(base, 3, 5, ConeShape::Pyramid, false, false);

        // Should generate some positions
        assert!(!positions.is_empty());

        // Check that base level has square shape
        let base_positions: Vec<_> = positions.iter().filter(|p| p.y == 0).collect();
        assert!(!base_positions.is_empty());

        // Corners of base should exist
        assert!(base_positions.iter().any(|p| p.x == 3 && p.z == 3));
        assert!(base_positions.iter().any(|p| p.x == -3 && p.z == 3));
        assert!(base_positions.iter().any(|p| p.x == 3 && p.z == -3));
        assert!(base_positions.iter().any(|p| p.x == -3 && p.z == -3));
    }

    #[test]
    fn test_inverted_cone() {
        let base = Vector3::new(0, 5, 0);
        let positions = generate_cone_positions(base, 3, 5, ConeShape::Cone, false, true);

        // Should generate some positions
        assert!(!positions.is_empty());

        // Base (widest part) should be at y=5
        let top_positions: Vec<_> = positions.iter().filter(|p| p.y == 5).collect();
        assert!(!top_positions.is_empty());

        // Apex (narrowest part) should be at y=1
        let bottom_positions: Vec<_> = positions.iter().filter(|p| p.y == 1).collect();
        // Should have fewer blocks at bottom
        assert!(bottom_positions.len() <= top_positions.len());
    }

    #[test]
    fn test_hollow_cone() {
        let base = Vector3::new(0, 0, 0);
        let solid = generate_cone_positions(base, 5, 6, ConeShape::Cone, false, false);
        let hollow = generate_cone_positions(base, 5, 6, ConeShape::Cone, true, false);

        // Hollow should have fewer blocks than solid
        assert!(hollow.len() < solid.len());
    }

    #[test]
    fn test_hollow_pyramid() {
        let base = Vector3::new(0, 0, 0);
        let solid = generate_cone_positions(base, 4, 5, ConeShape::Pyramid, false, false);
        let hollow = generate_cone_positions(base, 4, 5, ConeShape::Pyramid, true, false);

        // Hollow should have fewer blocks than solid
        assert!(hollow.len() < solid.len());
    }

    #[test]
    fn test_minimum_size() {
        let base = Vector3::new(0, 0, 0);

        // Size 1, height 1 should create single block
        let positions = generate_cone_positions(base, 1, 1, ConeShape::Cone, false, false);
        assert!(!positions.is_empty());

        // Size 0 or height 0 should create nothing
        let empty1 = generate_cone_positions(base, 0, 5, ConeShape::Cone, false, false);
        assert!(empty1.is_empty());

        let empty2 = generate_cone_positions(base, 5, 0, ConeShape::Cone, false, false);
        assert!(empty2.is_empty());
    }

    #[test]
    fn test_tapering() {
        let base = Vector3::new(0, 0, 0);
        let positions = generate_cone_positions(base, 5, 6, ConeShape::Pyramid, false, false);

        // Count blocks at each level - should decrease as we go up
        let mut level_counts: Vec<usize> = Vec::new();
        for y in 0..6 {
            let count = positions.iter().filter(|p| p.y == y).count();
            level_counts.push(count);
        }

        // Each level should have same or fewer blocks than the one below
        for i in 1..level_counts.len() {
            assert!(
                level_counts[i] <= level_counts[i - 1],
                "Level {} ({}) should have <= blocks than level {} ({})",
                i,
                level_counts[i],
                i - 1,
                level_counts[i - 1]
            );
        }
    }
}

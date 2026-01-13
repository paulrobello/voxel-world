//! Arch generation algorithm.
//!
//! This module provides functions to generate arch shapes for doorways,
//! windows, and architectural features.

use nalgebra::Vector3;

/// Style of the arch curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArchStyle {
    /// Perfect semicircle arch.
    #[default]
    Semicircle,
    /// Two arcs meeting at a point (Gothic style).
    Pointed,
    /// Flattened arc (less than semicircle).
    Segmental,
}

impl ArchStyle {
    /// Get all available arch styles.
    pub fn all() -> &'static [ArchStyle] {
        &[
            ArchStyle::Semicircle,
            ArchStyle::Pointed,
            ArchStyle::Segmental,
        ]
    }

    /// Get display name for the style.
    pub fn name(&self) -> &'static str {
        match self {
            ArchStyle::Semicircle => "Semicircle",
            ArchStyle::Pointed => "Pointed (Gothic)",
            ArchStyle::Segmental => "Segmental (Flat)",
        }
    }
}

/// Orientation of the arch (which direction it faces).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArchOrientation {
    /// Arch spans X axis, faces Z direction.
    #[default]
    FacingZ,
    /// Arch spans Z axis, faces X direction.
    FacingX,
}

impl ArchOrientation {
    /// Get all available orientations.
    pub fn all() -> &'static [ArchOrientation] {
        &[ArchOrientation::FacingZ, ArchOrientation::FacingX]
    }

    /// Get display name for the orientation.
    pub fn name(&self) -> &'static str {
        match self {
            ArchOrientation::FacingZ => "N-S (facing Z)",
            ArchOrientation::FacingX => "E-W (facing X)",
        }
    }
}

/// Generate positions for an arch.
///
/// # Arguments
/// * `base_center` - Center of the arch base (bottom middle)
/// * `width` - Total width of the arch opening (must be >= 2)
/// * `height` - Height of the arch from base to apex
/// * `thickness` - Depth/thickness of the arch wall (into the scene)
/// * `style` - Arch curve style
/// * `orientation` - Which direction the arch faces
/// * `hollow` - If true, only outer shell; if false, solid arch
///
/// # Returns
/// Vector of block positions forming the arch
pub fn generate_arch_positions(
    base_center: Vector3<i32>,
    width: i32,
    height: i32,
    thickness: i32,
    style: ArchStyle,
    orientation: ArchOrientation,
    hollow: bool,
) -> Vec<Vector3<i32>> {
    let mut positions = Vec::new();

    if width < 2 || height < 1 || thickness < 1 {
        return positions;
    }

    let half_width = width as f64 / 2.0;

    // Generate the 2D arch profile (in the plane perpendicular to thickness)
    // We'll iterate over a 2D grid and check if each point is part of the arch
    for y_offset in 0..=height {
        for x_offset in -width / 2..=(width + 1) / 2 {
            let x = x_offset as f64;
            let y = y_offset as f64;

            // Check if this point is part of the arch curve
            let in_arch = is_point_in_arch(x, y, half_width, height as f64, style);
            let in_inner = if hollow && thickness > 1 {
                // For hollow, check if point is inside the inner curve (1 block inward)
                is_point_in_arch_inner(x, y, half_width, height as f64, style)
            } else {
                false
            };

            if in_arch && !in_inner {
                // Extrude along thickness direction
                for t in 0..thickness {
                    let pos = match orientation {
                        ArchOrientation::FacingZ => Vector3::new(
                            base_center.x + x_offset,
                            base_center.y + y_offset,
                            base_center.z + t,
                        ),
                        ArchOrientation::FacingX => Vector3::new(
                            base_center.x + t,
                            base_center.y + y_offset,
                            base_center.z + x_offset,
                        ),
                    };
                    if !positions.contains(&pos) {
                        positions.push(pos);
                    }
                }
            }
        }
    }

    positions
}

/// Check if a point (x, y) is part of the arch shape.
/// x is horizontal offset from center, y is height from base.
fn is_point_in_arch(x: f64, y: f64, half_width: f64, height: f64, style: ArchStyle) -> bool {
    // The arch curve defines the top boundary
    // Below the curve and above the base, we need to determine if the point is
    // on the arch wall (not in the opening)

    match style {
        ArchStyle::Semicircle => {
            // Semicircle: curve is a half-ellipse with width and height
            // The arch opening is below the curve
            // Points are part of the arch if they're on the curve line (the arch itself)

            // For a semicircle arch, the curve equation is:
            // (x/half_width)^2 + (y/height)^2 = 1 (for the outer curve at y >= 0)

            // Check if point is on the arch curve (within 1 block of the ellipse)
            let x_norm = x / half_width;
            let y_norm = y / height;
            let dist_sq = x_norm * x_norm + y_norm * y_norm;

            // Point is part of arch if it's on the curve (between inner and outer)
            // or if it's the vertical sides below spring line (y = 0 to where curve starts)
            if y < 0.5 {
                // Below spring line - this is the vertical portion (jambs)
                x.abs() >= half_width - 0.5 && x.abs() <= half_width + 0.5
            } else {
                // On the curved portion
                (0.7..=1.3).contains(&dist_sq)
            }
        }
        ArchStyle::Pointed => {
            // Pointed arch: two circular arcs meeting at apex
            // Each arc originates from outside the arch width
            let apex_y = height;

            if y < 0.5 {
                // Vertical jambs
                x.abs() >= half_width - 0.5 && x.abs() <= half_width + 0.5
            } else {
                // The pointed arch uses two arcs with centers offset outward
                // Arc radius is typically height (or can be computed from geometry)
                let arc_radius = (half_width * half_width + height * height) / (2.0 * height);
                let center_offset = arc_radius - height;

                // Left arc center is at (-half_width, center_offset)
                // Right arc center is at (half_width, center_offset)
                let left_dist =
                    ((x + half_width).powi(2) + (y - center_offset).powi(2)).sqrt() - arc_radius;
                let right_dist =
                    ((x - half_width).powi(2) + (y - center_offset).powi(2)).sqrt() - arc_radius;

                // Point is on arch if it's close to either arc and on the correct side
                let on_left_arc = left_dist.abs() < 0.7 && x <= 0.0 && y >= 0.0 && y <= apex_y;
                let on_right_arc = right_dist.abs() < 0.7 && x >= 0.0 && y >= 0.0 && y <= apex_y;

                on_left_arc || on_right_arc
            }
        }
        ArchStyle::Segmental => {
            // Segmental arch: flattened arc (larger radius than semicircle)
            // The arc is shallower - compute radius from chord (width) and rise (height)
            // For a segmental arch: radius = (width^2 / 8*height) + (height/2)
            let full_width = half_width * 2.0;
            let arc_radius = (full_width * full_width) / (8.0 * height) + height / 2.0;
            let center_y = height - arc_radius; // Center is below the apex

            if y < 0.5 {
                // Vertical jambs
                x.abs() >= half_width - 0.5 && x.abs() <= half_width + 0.5
            } else {
                // Check distance from arc center
                let dist = (x.powi(2) + (y - center_y).powi(2)).sqrt();
                let on_arc = (dist - arc_radius).abs() < 0.7;

                // Must be in the correct region (above center, within width)
                on_arc && y >= center_y.max(0.0) && x.abs() <= half_width + 0.5
            }
        }
    }
}

/// Check if a point is inside the inner curve (for hollow arches).
fn is_point_in_arch_inner(x: f64, y: f64, half_width: f64, height: f64, style: ArchStyle) -> bool {
    // Inner curve is 1 block inward from outer
    let inner_half_width = (half_width - 1.0).max(0.5);
    let inner_height = (height - 1.0).max(0.5);

    match style {
        ArchStyle::Semicircle => {
            if y < 0.5 {
                // Inner jambs
                x.abs() < inner_half_width - 0.5
            } else {
                let x_norm = x / inner_half_width;
                let y_norm = y / inner_height;
                let dist_sq = x_norm * x_norm + y_norm * y_norm;
                dist_sq < 0.7
            }
        }
        ArchStyle::Pointed => {
            if y < 0.5 {
                x.abs() < inner_half_width - 0.5
            } else {
                let arc_radius = (inner_half_width * inner_half_width
                    + inner_height * inner_height)
                    / (2.0 * inner_height);
                let center_offset = arc_radius - inner_height;

                let left_dist = ((x + inner_half_width).powi(2) + (y - center_offset).powi(2))
                    .sqrt()
                    - arc_radius;
                let right_dist = ((x - inner_half_width).powi(2) + (y - center_offset).powi(2))
                    .sqrt()
                    - arc_radius;

                (left_dist > 0.3 || x > 0.0) && (right_dist > 0.3 || x < 0.0)
            }
        }
        ArchStyle::Segmental => {
            let full_width = inner_half_width * 2.0;
            let arc_radius = (full_width * full_width) / (8.0 * inner_height) + inner_height / 2.0;
            let center_y = inner_height - arc_radius;

            if y < 0.5 {
                x.abs() < inner_half_width - 0.5
            } else {
                let dist = (x.powi(2) + (y - center_y).powi(2)).sqrt();
                dist > arc_radius + 0.3
            }
        }
    }
}

/// Estimate the number of blocks in an arch.
#[allow(dead_code)]
pub fn estimate_block_count(width: i32, height: i32, thickness: i32, hollow: bool) -> usize {
    // Rough estimate based on arch perimeter
    let perimeter = (width + height * 2) as usize;
    let shell = perimeter * thickness as usize;
    if hollow {
        shell
    } else {
        shell * 2 // Rough estimate for solid
    }
}

/// Calculate arch dimensions for display.
#[allow(dead_code)]
pub fn calculate_dimensions(width: i32, height: i32, thickness: i32) -> (i32, i32, i32) {
    (width, height, thickness)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semicircle_arch() {
        let base = Vector3::new(0, 0, 0);
        let positions = generate_arch_positions(
            base,
            5,
            3,
            1,
            ArchStyle::Semicircle,
            ArchOrientation::FacingZ,
            false,
        );

        // Should generate some positions
        assert!(!positions.is_empty());

        // All positions should be at z=0 (thickness 1)
        assert!(positions.iter().all(|p| p.z == 0));
    }

    #[test]
    fn test_pointed_arch() {
        let base = Vector3::new(0, 0, 0);
        let positions = generate_arch_positions(
            base,
            4,
            5,
            1,
            ArchStyle::Pointed,
            ArchOrientation::FacingZ,
            false,
        );

        assert!(!positions.is_empty());
    }

    #[test]
    fn test_segmental_arch() {
        let base = Vector3::new(0, 0, 0);
        let positions = generate_arch_positions(
            base,
            6,
            2,
            1,
            ArchStyle::Segmental,
            ArchOrientation::FacingZ,
            false,
        );

        assert!(!positions.is_empty());
    }

    #[test]
    fn test_arch_with_thickness() {
        let base = Vector3::new(0, 0, 0);
        let positions = generate_arch_positions(
            base,
            4,
            3,
            3,
            ArchStyle::Semicircle,
            ArchOrientation::FacingZ,
            false,
        );

        // Should have positions at z=0, 1, 2
        let z_values: std::collections::HashSet<_> = positions.iter().map(|p| p.z).collect();
        assert!(z_values.contains(&0));
        assert!(z_values.contains(&1));
        assert!(z_values.contains(&2));
    }

    #[test]
    fn test_arch_orientation() {
        let base = Vector3::new(0, 0, 0);
        let facing_z = generate_arch_positions(
            base,
            4,
            3,
            2,
            ArchStyle::Semicircle,
            ArchOrientation::FacingZ,
            false,
        );
        let facing_x = generate_arch_positions(
            base,
            4,
            3,
            2,
            ArchStyle::Semicircle,
            ArchOrientation::FacingX,
            false,
        );

        // FacingZ should vary in Z for thickness
        let z_values: std::collections::HashSet<_> = facing_z.iter().map(|p| p.z).collect();
        assert!(z_values.len() > 1 || facing_z.is_empty());

        // FacingX should vary in X for thickness
        let x_values: std::collections::HashSet<_> = facing_x.iter().map(|p| p.x).collect();
        assert!(x_values.len() > 1 || facing_x.is_empty());
    }

    #[test]
    fn test_hollow_arch() {
        let base = Vector3::new(0, 0, 0);
        let solid = generate_arch_positions(
            base,
            6,
            4,
            2,
            ArchStyle::Semicircle,
            ArchOrientation::FacingZ,
            false,
        );
        let hollow = generate_arch_positions(
            base,
            6,
            4,
            2,
            ArchStyle::Semicircle,
            ArchOrientation::FacingZ,
            true,
        );

        // Hollow should have fewer or equal blocks
        assert!(hollow.len() <= solid.len());
    }

    #[test]
    fn test_invalid_dimensions() {
        let base = Vector3::new(0, 0, 0);

        // Width too small
        let positions = generate_arch_positions(
            base,
            1,
            3,
            1,
            ArchStyle::Semicircle,
            ArchOrientation::FacingZ,
            false,
        );
        assert!(positions.is_empty());

        // Height zero
        let positions = generate_arch_positions(
            base,
            4,
            0,
            1,
            ArchStyle::Semicircle,
            ArchOrientation::FacingZ,
            false,
        );
        assert!(positions.is_empty());

        // Thickness zero
        let positions = generate_arch_positions(
            base,
            4,
            3,
            0,
            ArchStyle::Semicircle,
            ArchOrientation::FacingZ,
            false,
        );
        assert!(positions.is_empty());
    }
}

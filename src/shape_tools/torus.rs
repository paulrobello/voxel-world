//! Torus (donut/ring) generation algorithm and tool state.
//!
//! This module provides functions to generate torus block positions and the
//! TorusToolState for managing the torus placement tool.

use crate::gpu_resources::MAX_STENCIL_BLOCKS;
use nalgebra::Vector3;

/// Orientation plane for the torus.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TorusPlane {
    /// Torus lies flat in XZ plane (horizontal ring, like a table).
    #[default]
    XZ,
    /// Torus lies in XY plane (vertical ring facing Z).
    XY,
    /// Torus lies in YZ plane (vertical ring facing X).
    YZ,
}

impl TorusPlane {
    /// Get a short name for the plane.
    pub fn name(&self) -> &'static str {
        match self {
            TorusPlane::XZ => "XZ",
            TorusPlane::XY => "XY",
            TorusPlane::YZ => "YZ",
        }
    }

    /// Get a display name for the plane.
    #[allow(dead_code)]
    pub fn display_name(&self) -> &'static str {
        match self {
            TorusPlane::XZ => "Horizontal (XZ)",
            TorusPlane::XY => "Vertical XY",
            TorusPlane::YZ => "Vertical YZ",
        }
    }

    /// Cycle to the next plane.
    #[allow(dead_code)]
    pub fn next(&self) -> Self {
        match self {
            TorusPlane::XZ => TorusPlane::XY,
            TorusPlane::XY => TorusPlane::YZ,
            TorusPlane::YZ => TorusPlane::XZ,
        }
    }
}

/// State for the torus placement tool.
#[derive(Clone, Debug)]
pub struct TorusToolState {
    /// Whether the torus tool is currently active.
    pub active: bool,
    /// Major radius (center to tube center) in blocks (2-50).
    pub major_radius: i32,
    /// Minor radius (tube thickness) in blocks (1-20).
    pub minor_radius: i32,
    /// Orientation plane for the torus.
    pub plane: TorusPlane,
    /// Arc angle in degrees (0-360). 360 = full torus.
    pub arc_angle: i32,
    /// Whether to create a hollow tube instead of solid.
    pub hollow: bool,
    /// Cached preview positions for GPU upload.
    pub preview_positions: Vec<Vector3<i32>>,
    /// Current preview center position (if targeting a block).
    pub preview_center: Option<Vector3<i32>>,
    /// Total block count for the full torus (may differ from preview if truncated).
    pub total_blocks: usize,
    /// Whether the preview was truncated due to exceeding buffer limit.
    pub preview_truncated: bool,
    // Cached settings for detecting changes
    cached_major_radius: i32,
    cached_minor_radius: i32,
    cached_plane: TorusPlane,
    cached_arc_angle: i32,
    cached_hollow: bool,
}

impl Default for TorusToolState {
    fn default() -> Self {
        Self {
            active: false,
            major_radius: 8,
            minor_radius: 3,
            plane: TorusPlane::XZ,
            arc_angle: 360,
            hollow: false,
            preview_positions: Vec::new(),
            preview_center: None,
            total_blocks: 0,
            preview_truncated: false,
            cached_major_radius: 8,
            cached_minor_radius: 3,
            cached_plane: TorusPlane::XZ,
            cached_arc_angle: 360,
            cached_hollow: false,
        }
    }
}

impl TorusToolState {
    /// Check if settings have changed since last preview generation.
    pub fn settings_changed(&self) -> bool {
        self.major_radius != self.cached_major_radius
            || self.minor_radius != self.cached_minor_radius
            || self.plane != self.cached_plane
            || self.arc_angle != self.cached_arc_angle
            || self.hollow != self.cached_hollow
    }

    /// Update cached settings after regenerating preview.
    pub fn update_cache(&mut self) {
        self.cached_major_radius = self.major_radius;
        self.cached_minor_radius = self.minor_radius;
        self.cached_plane = self.plane;
        self.cached_arc_angle = self.arc_angle;
        self.cached_hollow = self.hollow;
    }

    /// Clear the preview state.
    pub fn clear_preview(&mut self) {
        self.preview_positions.clear();
        self.preview_center = None;
        self.preview_truncated = false;
        self.total_blocks = 0;
    }

    /// Deactivate the tool and clear preview.
    pub fn deactivate(&mut self) {
        self.active = false;
        self.clear_preview();
    }

    /// Update the torus preview at the given target position.
    pub fn update_preview(&mut self, target: Vector3<i32>) {
        // Only regenerate if center or settings changed
        let needs_regen = self.preview_center != Some(target) || self.settings_changed();

        if needs_regen {
            self.preview_center = Some(target);
            self.update_cache();

            let all_positions = generate_torus_positions(
                target,
                self.major_radius,
                self.minor_radius,
                self.plane,
                self.arc_angle,
                self.hollow,
            );

            // Track total count and truncation status
            self.total_blocks = all_positions.len();
            self.preview_truncated = all_positions.len() > MAX_STENCIL_BLOCKS;

            // Truncate for preview (full list used for actual placement)
            if all_positions.len() > MAX_STENCIL_BLOCKS {
                self.preview_positions = all_positions[..MAX_STENCIL_BLOCKS].to_vec();
            } else {
                self.preview_positions = all_positions;
            }
        }
    }
}

/// Generate all block positions for a torus.
///
/// Uses parametric torus equations to sample the surface and fill volume.
///
/// # Arguments
/// * `center` - Center position of the torus in world coordinates
/// * `major_radius` - Distance from torus center to tube center (R)
/// * `minor_radius` - Radius of the tube itself (r)
/// * `plane` - Orientation plane (XZ=horizontal, XY/YZ=vertical)
/// * `arc_angle` - Arc angle in degrees (360 = full torus)
/// * `hollow` - If true, only generate tube shell (thickness = 1 block)
///
/// # Returns
/// Vector of all block positions that make up the torus.
pub fn generate_torus_positions(
    center: Vector3<i32>,
    major_radius: i32,
    minor_radius: i32,
    plane: TorusPlane,
    arc_angle: i32,
    hollow: bool,
) -> Vec<Vector3<i32>> {
    let mut positions = Vec::new();

    if major_radius < 1 || minor_radius < 1 {
        return positions;
    }

    let r = major_radius as f64; // Major radius
    let r_tube = minor_radius as f64; // Minor (tube) radius
    let r_tube_sq = r_tube * r_tube;
    let inner_r_sq = if hollow && minor_radius > 1 {
        ((minor_radius - 1) as f64).powi(2)
    } else {
        -1.0
    };

    // Arc angle in radians
    let arc_rad = (arc_angle.clamp(1, 360) as f64).to_radians();

    // Bounding box for iteration (full cube to handle all orientations)
    let outer_extent = major_radius + minor_radius;

    // Use a HashSet to avoid duplicates (torus sampling can cause overlap)
    let mut visited = std::collections::HashSet::new();

    // Always compute torus in canonical XZ plane, then transform output positions
    // This ensures consistent block counts across all orientations
    for lx in -outer_extent..=outer_extent {
        for ly in -minor_radius..=minor_radius {
            for lz in -outer_extent..=outer_extent {
                // Check if point is on/in the torus (in local XZ-plane coordinates)
                // Torus equation: (sqrt(x² + z²) - R)² + y² <= r²
                let px = lx as f64;
                let py = ly as f64;
                let pz = lz as f64;

                let dist_xz = (px * px + pz * pz).sqrt();
                let dist_from_ring = dist_xz - r;
                let dist_sq = dist_from_ring * dist_from_ring + py * py;

                if dist_sq <= r_tube_sq {
                    // For hollow, check if inside inner surface
                    if hollow && dist_sq < inner_r_sq {
                        continue;
                    }

                    // Check arc angle (only for partial torus)
                    if arc_angle < 360 {
                        let angle = pz.atan2(px);
                        let normalized_angle = if angle < 0.0 {
                            angle + std::f64::consts::TAU
                        } else {
                            angle
                        };
                        if normalized_angle > arc_rad {
                            continue;
                        }
                    }

                    // Transform local coordinates to world coordinates based on plane
                    // Local XZ plane -> target plane
                    let (dx, dy, dz) = match plane {
                        TorusPlane::XZ => (lx, ly, lz), // No transformation needed
                        TorusPlane::XY => (lx, lz, ly), // Rotate so torus lies in XY
                        TorusPlane::YZ => (ly, lx, lz), // Rotate so torus lies in YZ
                    };

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

/// Estimate torus volume (for confirmation dialogs).
///
/// Uses the mathematical formula for torus volume: 2π²Rr²
///
/// # Arguments
/// * `major_radius` - Major radius (R)
/// * `minor_radius` - Minor radius (r)
/// * `arc_angle` - Arc angle in degrees
/// * `hollow` - If true, calculate shell volume only
///
/// # Returns
/// Estimated number of blocks.
#[allow(dead_code)]
pub fn estimate_volume(major_radius: i32, minor_radius: i32, arc_angle: i32, hollow: bool) -> u64 {
    let r = major_radius as f64;
    let r_tube = minor_radius as f64;
    let arc_fraction = (arc_angle.clamp(1, 360) as f64) / 360.0;

    if hollow {
        let outer_vol = 2.0 * std::f64::consts::PI.powi(2) * r * r_tube.powi(2);
        let inner_r = (minor_radius - 1).max(0) as f64;
        let inner_vol = 2.0 * std::f64::consts::PI.powi(2) * r * inner_r.powi(2);
        ((outer_vol - inner_vol) * arc_fraction) as u64
    } else {
        (2.0 * std::f64::consts::PI.powi(2) * r * r_tube.powi(2) * arc_fraction) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_torus() {
        let positions = generate_torus_positions(
            Vector3::new(0, 0, 0),
            5, // major radius
            2, // minor radius
            TorusPlane::XZ,
            360,
            false,
        );
        assert!(!positions.is_empty());
        // Torus should be roughly symmetric around the center
    }

    #[test]
    fn test_hollow_torus() {
        let center = Vector3::new(10, 10, 10);
        let solid = generate_torus_positions(center, 6, 3, TorusPlane::XZ, 360, false);
        let hollow = generate_torus_positions(center, 6, 3, TorusPlane::XZ, 360, true);

        // Hollow should have fewer blocks than solid
        assert!(hollow.len() < solid.len());
        // Both should contain some blocks
        assert!(!solid.is_empty());
        assert!(!hollow.is_empty());
    }

    #[test]
    fn test_partial_arc() {
        let full =
            generate_torus_positions(Vector3::new(0, 0, 0), 5, 2, TorusPlane::XZ, 360, false);
        let half =
            generate_torus_positions(Vector3::new(0, 0, 0), 5, 2, TorusPlane::XZ, 180, false);

        // Half arc should have roughly half the blocks
        assert!(half.len() < full.len());
        assert!(half.len() > full.len() / 4);
    }

    #[test]
    fn test_vertical_plane() {
        let horizontal =
            generate_torus_positions(Vector3::new(0, 0, 0), 5, 2, TorusPlane::XZ, 360, false);
        let vertical =
            generate_torus_positions(Vector3::new(0, 0, 0), 5, 2, TorusPlane::XY, 360, false);

        // Same parameters should give similar block counts
        assert_eq!(horizontal.len(), vertical.len());
    }

    #[test]
    fn test_invalid_radii() {
        let positions =
            generate_torus_positions(Vector3::new(0, 0, 0), 0, 0, TorusPlane::XZ, 360, false);
        assert!(positions.is_empty());
    }
}

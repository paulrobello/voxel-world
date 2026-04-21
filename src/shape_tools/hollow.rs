//! Hollow tool for converting solid selections to hollow shells.
//!
//! This module provides functions to identify interior blocks within a selection
//! that can be removed to create a hollow shell with a specified wall thickness.

use nalgebra::Vector3;

/// State for the hollow tool.
#[derive(Debug, Clone)]
#[allow(clippy::type_complexity)]
pub struct HollowToolState {
    /// Whether the tool is active.
    pub active: bool,
    /// Wall thickness (1-5 blocks).
    pub thickness: i32,
    /// Preview positions to remove (interior blocks).
    pub preview_positions: Vec<Vector3<i32>>,
    /// Total blocks in the selection.
    pub total_blocks: usize,
    /// Blocks that would be hollowed out.
    pub hollow_count: usize,
    /// Whether preview was truncated.
    pub preview_truncated: bool,
    /// Cached parameters to detect changes.
    cached_params: (i32, Option<(Vector3<i32>, Vector3<i32>)>),
}

impl Default for HollowToolState {
    fn default() -> Self {
        Self {
            active: false,
            thickness: 1,
            preview_positions: Vec::new(),
            total_blocks: 0,
            hollow_count: 0,
            preview_truncated: false,
            cached_params: (1, None),
        }
    }
}

impl HollowToolState {
    /// Update preview for hollow operation within a selection.
    ///
    /// # Arguments
    /// * `selection_bounds` - Optional (min, max) bounds of the selection
    pub fn update_preview(&mut self, selection_bounds: Option<(Vector3<i32>, Vector3<i32>)>) {
        let params = (self.thickness, selection_bounds);

        // Skip if nothing changed
        if params == self.cached_params && !self.preview_positions.is_empty() {
            return;
        }

        self.cached_params = params;
        self.preview_positions.clear();
        self.preview_truncated = false;

        let Some((min, max)) = selection_bounds else {
            self.total_blocks = 0;
            self.hollow_count = 0;
            return;
        };

        let positions = calculate_interior_positions(min, max, self.thickness);

        self.total_blocks =
            ((max.x - min.x + 1) * (max.y - min.y + 1) * (max.z - min.z + 1)) as usize;
        self.hollow_count = positions.len();

        // Limit preview to prevent GPU buffer overflow
        const MAX_PREVIEW: usize = 4096;
        if positions.len() > MAX_PREVIEW {
            self.preview_positions = positions[..MAX_PREVIEW].to_vec();
            self.preview_truncated = true;
        } else {
            self.preview_positions = positions;
        }
    }

    /// Clear preview data.
    pub fn clear_preview(&mut self) {
        self.preview_positions.clear();
        self.total_blocks = 0;
        self.hollow_count = 0;
        self.preview_truncated = false;
        self.cached_params = (1, None);
    }
}

/// Calculate interior positions that should be removed for a hollow shell.
///
/// A block is interior if its distance from all six faces is greater than the thickness.
///
/// # Arguments
/// * `min` - Minimum corner of the selection
/// * `max` - Maximum corner of the selection
/// * `thickness` - Wall thickness in blocks
///
/// # Returns
/// Vector of positions that are in the interior (should be removed/made air)
pub fn calculate_interior_positions(
    min: Vector3<i32>,
    max: Vector3<i32>,
    thickness: i32,
) -> Vec<Vector3<i32>> {
    let thickness = thickness.max(1);
    let mut positions = Vec::new();

    // The interior starts at min + thickness and ends at max - thickness
    let interior_min = Vector3::new(min.x + thickness, min.y + thickness, min.z + thickness);
    let interior_max = Vector3::new(max.x - thickness, max.y - thickness, max.z - thickness);

    // If the selection is too small, no interior exists
    if interior_min.x > interior_max.x
        || interior_min.y > interior_max.y
        || interior_min.z > interior_max.z
    {
        return positions;
    }

    for x in interior_min.x..=interior_max.x {
        for y in interior_min.y..=interior_max.y {
            for z in interior_min.z..=interior_max.z {
                positions.push(Vector3::new(x, y, z));
            }
        }
    }

    positions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hollow_3x3x3() {
        let min = Vector3::new(0, 0, 0);
        let max = Vector3::new(2, 2, 2);
        let interior = calculate_interior_positions(min, max, 1);

        // 3x3x3 with thickness 1: only the center block is interior
        assert_eq!(interior.len(), 1);
        assert_eq!(interior[0], Vector3::new(1, 1, 1));
    }

    #[test]
    fn test_hollow_5x5x5() {
        let min = Vector3::new(0, 0, 0);
        let max = Vector3::new(4, 4, 4);
        let interior = calculate_interior_positions(min, max, 1);

        // 5x5x5 with thickness 1: 3x3x3 = 27 interior blocks
        assert_eq!(interior.len(), 27);
    }

    #[test]
    fn test_hollow_thickness_2() {
        let min = Vector3::new(0, 0, 0);
        let max = Vector3::new(4, 4, 4);
        let interior = calculate_interior_positions(min, max, 2);

        // 5x5x5 with thickness 2: only center block is interior
        assert_eq!(interior.len(), 1);
        assert_eq!(interior[0], Vector3::new(2, 2, 2));
    }

    #[test]
    fn test_too_small_for_hollow() {
        let min = Vector3::new(0, 0, 0);
        let max = Vector3::new(1, 1, 1);
        let interior = calculate_interior_positions(min, max, 1);

        // 2x2x2 with thickness 1: no interior
        assert_eq!(interior.len(), 0);
    }

    #[test]
    fn test_flat_selection() {
        let min = Vector3::new(0, 0, 0);
        let max = Vector3::new(4, 0, 4);
        let interior = calculate_interior_positions(min, max, 1);

        // Flat 5x1x5 with thickness 1: no interior (Y dimension too small)
        assert_eq!(interior.len(), 0);
    }

    #[test]
    fn test_asymmetric() {
        let min = Vector3::new(0, 0, 0);
        let max = Vector3::new(4, 2, 6);
        let interior = calculate_interior_positions(min, max, 1);

        // 5x3x7 with thickness 1: 3x1x5 = 15 interior blocks
        assert_eq!(interior.len(), 15);
    }

    #[test]
    fn test_state_preview() {
        let mut state = HollowToolState::default();
        let min = Vector3::new(0, 0, 0);
        let max = Vector3::new(4, 4, 4);

        state.update_preview(Some((min, max)));

        assert_eq!(state.total_blocks, 125); // 5x5x5
        assert_eq!(state.hollow_count, 27); // 3x3x3 interior
        assert_eq!(state.preview_positions.len(), 27);
    }
}

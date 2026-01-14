//! Scatter brush tool for paint-style block placement.
//!
//! This module provides functions to scatter blocks within a brush radius,
//! supporting surface-only placement and height variation.

use nalgebra::Vector3;

/// State for the scatter brush tool.
#[derive(Debug, Clone)]
pub struct ScatterToolState {
    /// Whether the tool is active.
    pub active: bool,
    /// Brush radius in blocks.
    pub radius: i32,
    /// Density percentage (1-100).
    pub density: i32,
    /// Whether to only place on surfaces.
    pub surface_only: bool,
    /// Height variation range (0-5).
    pub height_variation: i32,
    /// Whether currently painting (right-click held).
    pub painting: bool,
    /// Last paint position to avoid duplicate placement.
    last_paint_pos: Option<Vector3<i32>>,
    /// Cached seed for consistent random during a paint stroke.
    random_seed: u64,
}

impl Default for ScatterToolState {
    fn default() -> Self {
        Self {
            active: false,
            radius: 3,
            density: 50,
            surface_only: true,
            height_variation: 0,
            painting: false,
            last_paint_pos: None,
            random_seed: 0,
        }
    }
}

impl ScatterToolState {
    /// Start a new paint stroke.
    pub fn start_painting(&mut self) {
        self.painting = true;
        self.last_paint_pos = None;
        // Generate new seed for this stroke
        self.random_seed = self.random_seed.wrapping_add(987654321);
    }

    /// End the current paint stroke.
    pub fn stop_painting(&mut self) {
        self.painting = false;
        self.last_paint_pos = None;
    }

    /// Check if we should paint at this position (prevents duplicate placement).
    pub fn should_paint_at(&mut self, pos: Vector3<i32>) -> bool {
        if let Some(last) = self.last_paint_pos {
            // Only paint if moved at least 1 block
            if last == pos {
                return false;
            }
        }
        self.last_paint_pos = Some(pos);
        true
    }

    /// Get the current random seed for consistent placement.
    pub fn seed(&self) -> u64 {
        self.random_seed
    }
}

/// Simple hash function for pseudo-random effects based on position and seed.
fn hash_position(x: i32, y: i32, z: i32, seed: u64) -> u64 {
    let mut hash = seed;
    hash = hash.wrapping_mul(0x517cc1b727220a95);
    hash ^= x as u64;
    hash = hash.wrapping_mul(0x517cc1b727220a95);
    hash ^= y as u64;
    hash = hash.wrapping_mul(0x517cc1b727220a95);
    hash ^= z as u64;
    hash = hash.wrapping_mul(0x517cc1b727220a95);
    hash
}

/// Generate scatter positions within a brush radius around a center point.
///
/// # Arguments
/// * `center` - Center of the brush
/// * `radius` - Brush radius in blocks
/// * `density` - Percentage of positions to fill (1-100)
/// * `seed` - Random seed for consistent placement
///
/// # Returns
/// Vector of positions that should have blocks placed
pub fn generate_scatter_positions(
    center: Vector3<i32>,
    radius: i32,
    density: i32,
    seed: u64,
) -> Vec<Vector3<i32>> {
    let radius = radius.max(1);
    let density = density.clamp(1, 100);
    let mut positions = Vec::new();

    // Iterate over a circular area (XZ plane)
    let r_sq = (radius * radius) as f32;
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            // Check if within circular radius
            let dist_sq = (dx * dx + dz * dz) as f32;
            if dist_sq > r_sq {
                continue;
            }

            let x = center.x + dx;
            let z = center.z + dz;

            // Use hash for deterministic random
            let hash = hash_position(x, center.y, z, seed);
            let roll = (hash % 100) as i32 + 1;

            if roll <= density {
                positions.push(Vector3::new(x, center.y, z));
            }
        }
    }

    positions
}

/// Generate scatter positions with height variation.
///
/// # Arguments
/// * `center` - Center of the brush
/// * `radius` - Brush radius in blocks
/// * `density` - Percentage of positions to fill (1-100)
/// * `height_variation` - Maximum height offset (0-5)
/// * `seed` - Random seed for consistent placement
///
/// # Returns
/// Vector of positions that should have blocks placed
pub fn generate_scatter_positions_with_height(
    center: Vector3<i32>,
    radius: i32,
    density: i32,
    height_variation: i32,
    seed: u64,
) -> Vec<Vector3<i32>> {
    let radius = radius.max(1);
    let density = density.clamp(1, 100);
    let height_variation = height_variation.clamp(0, 5);
    let mut positions = Vec::new();

    let r_sq = (radius * radius) as f32;
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            let dist_sq = (dx * dx + dz * dz) as f32;
            if dist_sq > r_sq {
                continue;
            }

            let x = center.x + dx;
            let z = center.z + dz;

            // Use hash for density check
            let hash = hash_position(x, center.y, z, seed);
            let roll = (hash % 100) as i32 + 1;

            if roll <= density {
                // Calculate height offset using a different hash
                let height_hash = hash_position(x, center.y + 1000, z, seed);
                let height_offset = if height_variation > 0 {
                    (height_hash % (height_variation as u64 * 2 + 1)) as i32 - height_variation
                } else {
                    0
                };

                positions.push(Vector3::new(x, center.y + height_offset, z));
            }
        }
    }

    positions
}

/// Generate preview positions showing the brush area.
///
/// # Arguments
/// * `center` - Center of the brush
/// * `radius` - Brush radius in blocks
///
/// # Returns
/// Vector of positions within the brush radius
#[allow(dead_code)]
pub fn generate_brush_preview(center: Vector3<i32>, radius: i32) -> Vec<Vector3<i32>> {
    let radius = radius.max(1);
    let mut positions = Vec::new();

    let r_sq = (radius * radius) as f32;
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            let dist_sq = (dx * dx + dz * dz) as f32;
            if dist_sq > r_sq {
                continue;
            }

            positions.push(Vector3::new(center.x + dx, center.y, center.z + dz));
        }
    }

    positions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_scatter() {
        let center = Vector3::new(0, 0, 0);
        let positions = generate_scatter_positions(center, 3, 100, 12345);

        // With 100% density and radius 3, should fill the circle
        // Approximate area = pi * r^2 = ~28 blocks
        assert!(positions.len() >= 20);
        assert!(positions.len() <= 40);

        // All positions should be within radius
        for pos in &positions {
            let dx = pos.x - center.x;
            let dz = pos.z - center.z;
            assert!(dx * dx + dz * dz <= 9);
        }
    }

    #[test]
    fn test_density_affects_count() {
        let center = Vector3::new(0, 0, 0);
        let high_density = generate_scatter_positions(center, 5, 100, 42);
        let low_density = generate_scatter_positions(center, 5, 25, 42);

        // Lower density should produce fewer positions
        assert!(low_density.len() < high_density.len());
    }

    #[test]
    fn test_height_variation() {
        let center = Vector3::new(0, 10, 0);
        let positions = generate_scatter_positions_with_height(center, 5, 100, 3, 42);

        // Check heights are within variation range
        for pos in &positions {
            let dy = (pos.y - center.y).abs();
            assert!(dy <= 3, "Height {} outside variation range", pos.y);
        }
    }

    #[test]
    fn test_zero_height_variation() {
        let center = Vector3::new(0, 10, 0);
        let positions = generate_scatter_positions_with_height(center, 5, 100, 0, 42);

        // All positions should be at center.y
        for pos in &positions {
            assert_eq!(pos.y, center.y);
        }
    }

    #[test]
    fn test_brush_preview() {
        let center = Vector3::new(5, 5, 5);
        let preview = generate_brush_preview(center, 2);

        // Radius 2 circle should have ~13 blocks
        assert!(preview.len() >= 10);
        assert!(preview.len() <= 20);

        // All should be at center.y
        for pos in &preview {
            assert_eq!(pos.y, center.y);
        }
    }

    #[test]
    fn test_paint_dedup() {
        let mut state = ScatterToolState::default();
        state.start_painting();

        let pos = Vector3::new(10, 20, 30);
        assert!(state.should_paint_at(pos));
        assert!(!state.should_paint_at(pos)); // Same position should return false

        let pos2 = Vector3::new(11, 20, 30);
        assert!(state.should_paint_at(pos2)); // Different position should work
    }
}

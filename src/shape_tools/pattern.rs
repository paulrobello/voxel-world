//! Pattern fill algorithm for applying patterns to block selections.
//!
//! This module provides functions to generate patterned block placements
//! within a selection region, alternating between two block types.

use nalgebra::Vector3;

/// Available pattern types for filling selections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PatternType {
    /// Alternating blocks in 3D checkerboard pattern: (x+y+z) % period
    #[default]
    Checkerboard,
    /// Stripes along X axis: x % period
    StripesX,
    /// Stripes along Y axis: y % period
    StripesY,
    /// Stripes along Z axis: z % period
    StripesZ,
    /// Diagonal stripes in XZ plane: (x+z) % period
    Diagonal,
    /// Random placement with given percentage for block A
    Random,
    /// Gradient blend from block A to block B along Y axis
    GradientY,
}

impl PatternType {
    /// Get all pattern types for UI iteration.
    pub const ALL: &'static [PatternType] = &[
        PatternType::Checkerboard,
        PatternType::StripesX,
        PatternType::StripesY,
        PatternType::StripesZ,
        PatternType::Diagonal,
        PatternType::Random,
        PatternType::GradientY,
    ];

    /// Get display name for the pattern.
    pub fn name(&self) -> &'static str {
        match self {
            PatternType::Checkerboard => "Checkerboard",
            PatternType::StripesX => "Stripes X",
            PatternType::StripesY => "Stripes Y",
            PatternType::StripesZ => "Stripes Z",
            PatternType::Diagonal => "Diagonal",
            PatternType::Random => "Random",
            PatternType::GradientY => "Gradient Y",
        }
    }
}

/// Block assignment for a position in the pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternBlock {
    /// Place block A (primary block)
    BlockA,
    /// Place block B (secondary block / air)
    BlockB,
}

/// State for the pattern fill tool.
#[derive(Debug, Clone)]
#[allow(clippy::type_complexity)]
pub struct PatternFillState {
    /// Whether the tool is active.
    pub active: bool,
    /// Current pattern type.
    pub pattern_type: PatternType,
    /// Pattern period (1-10) for stripes/checkerboard.
    pub period: i32,
    /// Random percentage for block A (1-99%) when using Random pattern.
    pub random_percent: i32,
    /// Preview positions with their block assignments.
    pub preview_a: Vec<Vector3<i32>>,
    pub preview_b: Vec<Vector3<i32>>,
    /// Total block count for the current operation.
    pub total_blocks: usize,
    /// Whether preview was truncated.
    pub preview_truncated: bool,
    /// Cached parameters to detect changes.
    cached_params: (PatternType, i32, i32, Option<(Vector3<i32>, Vector3<i32>)>),
    /// Cached RNG seed for stable random preview.
    random_seed: u64,
}

impl Default for PatternFillState {
    fn default() -> Self {
        Self {
            active: false,
            pattern_type: PatternType::Checkerboard,
            period: 1,
            random_percent: 50,
            preview_a: Vec::new(),
            preview_b: Vec::new(),
            total_blocks: 0,
            preview_truncated: false,
            cached_params: (PatternType::Checkerboard, 1, 50, None),
            random_seed: 0,
        }
    }
}

impl PatternFillState {
    /// Update preview for pattern fill within a selection.
    ///
    /// # Arguments
    /// * `selection_bounds` - Optional (min, max) bounds of the selection
    pub fn update_preview(&mut self, selection_bounds: Option<(Vector3<i32>, Vector3<i32>)>) {
        let params = (
            self.pattern_type,
            self.period,
            self.random_percent,
            selection_bounds,
        );

        // Skip if nothing changed
        if params == self.cached_params && !self.preview_a.is_empty() {
            return;
        }

        // Regenerate seed when pattern changes for stable random preview
        if params.0 != self.cached_params.0 {
            // Use wrapping add to generate a new seed deterministically
            self.random_seed = self.random_seed.wrapping_add(12345678901234567);
        }

        self.cached_params = params;
        self.preview_a.clear();
        self.preview_b.clear();
        self.preview_truncated = false;

        let Some((min, max)) = selection_bounds else {
            self.total_blocks = 0;
            return;
        };

        let (positions_a, positions_b) = generate_pattern_positions(
            min,
            max,
            self.pattern_type,
            self.period,
            self.random_percent,
            self.random_seed,
        );

        self.total_blocks = positions_a.len() + positions_b.len();

        // Limit preview to prevent GPU buffer overflow
        const MAX_PREVIEW: usize = 4096;
        let max_per_type = MAX_PREVIEW / 2;

        if positions_a.len() > max_per_type {
            self.preview_a = positions_a[..max_per_type].to_vec();
            self.preview_truncated = true;
        } else {
            self.preview_a = positions_a;
        }

        if positions_b.len() > max_per_type {
            self.preview_b = positions_b[..max_per_type].to_vec();
            self.preview_truncated = true;
        } else {
            self.preview_b = positions_b;
        }
    }

    /// Clear preview data.
    pub fn clear_preview(&mut self) {
        self.preview_a.clear();
        self.preview_b.clear();
        self.total_blocks = 0;
        self.preview_truncated = false;
        self.cached_params = (PatternType::Checkerboard, 1, 50, None);
    }
}

/// Simple hash function for pseudo-random effects based on position and seed.
fn hash_position(x: i32, y: i32, z: i32, seed: u64) -> u64 {
    // Mix position with seed using a simple hash
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

/// Generate pattern positions within a bounding box.
///
/// # Arguments
/// * `min` - Minimum corner of the region
/// * `max` - Maximum corner of the region
/// * `pattern_type` - Type of pattern to apply
/// * `period` - Pattern period for stripes/checkerboard
/// * `random_percent` - Percentage for block A in random mode
/// * `seed` - Random seed for stable random pattern
///
/// # Returns
/// Tuple of (positions for block A, positions for block B)
pub fn generate_pattern_positions(
    min: Vector3<i32>,
    max: Vector3<i32>,
    pattern_type: PatternType,
    period: i32,
    random_percent: i32,
    seed: u64,
) -> (Vec<Vector3<i32>>, Vec<Vector3<i32>>) {
    let period = period.max(1);
    let mut positions_a = Vec::new();
    let mut positions_b = Vec::new();

    // For gradient, calculate height range
    let height_range = (max.y - min.y) as f32;

    for x in min.x..=max.x {
        for y in min.y..=max.y {
            for z in min.z..=max.z {
                let pos = Vector3::new(x, y, z);
                let assignment = match pattern_type {
                    PatternType::Checkerboard => {
                        let sum = x + y + z;
                        if (sum / period) % 2 == 0 {
                            PatternBlock::BlockA
                        } else {
                            PatternBlock::BlockB
                        }
                    }
                    PatternType::StripesX => {
                        if (x / period) % 2 == 0 {
                            PatternBlock::BlockA
                        } else {
                            PatternBlock::BlockB
                        }
                    }
                    PatternType::StripesY => {
                        if (y / period) % 2 == 0 {
                            PatternBlock::BlockA
                        } else {
                            PatternBlock::BlockB
                        }
                    }
                    PatternType::StripesZ => {
                        if (z / period) % 2 == 0 {
                            PatternBlock::BlockA
                        } else {
                            PatternBlock::BlockB
                        }
                    }
                    PatternType::Diagonal => {
                        let sum = x + z;
                        if (sum / period) % 2 == 0 {
                            PatternBlock::BlockA
                        } else {
                            PatternBlock::BlockB
                        }
                    }
                    PatternType::Random => {
                        // Use simple hash for deterministic random
                        let hash = hash_position(x, y, z, seed);
                        let roll = (hash % 100) as i32 + 1;
                        if roll <= random_percent {
                            PatternBlock::BlockA
                        } else {
                            PatternBlock::BlockB
                        }
                    }
                    PatternType::GradientY => {
                        // Blend from A at bottom to B at top based on period
                        // Period controls the blend sharpness (higher = more gradual)
                        if height_range <= 0.0 {
                            PatternBlock::BlockA
                        } else {
                            let t = (y - min.y) as f32 / height_range;
                            // Add randomness based on period for dithered gradient
                            let hash = hash_position(x, y, z, seed);
                            let noise = (hash % 1000) as f32 / 1000.0 - 0.5;
                            let threshold = t + noise * (0.1 * period as f32);
                            if threshold < 0.5 {
                                PatternBlock::BlockA
                            } else {
                                PatternBlock::BlockB
                            }
                        }
                    }
                };

                match assignment {
                    PatternBlock::BlockA => positions_a.push(pos),
                    PatternBlock::BlockB => positions_b.push(pos),
                }
            }
        }
    }

    (positions_a, positions_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkerboard_alternates() {
        let min = Vector3::new(0, 0, 0);
        let max = Vector3::new(3, 0, 3);
        let (a, b) = generate_pattern_positions(min, max, PatternType::Checkerboard, 1, 50, 0);

        // 4x4 = 16 blocks, should be roughly half and half
        assert_eq!(a.len() + b.len(), 16);
        assert!(!a.is_empty());
        assert!(!b.is_empty());
    }

    #[test]
    fn test_stripes_x() {
        let min = Vector3::new(0, 0, 0);
        let max = Vector3::new(3, 0, 0);
        let (a, b) = generate_pattern_positions(min, max, PatternType::StripesX, 1, 50, 0);

        // 4 blocks along X, period 1 = alternating
        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 2);
    }

    #[test]
    fn test_stripes_period() {
        let min = Vector3::new(0, 0, 0);
        let max = Vector3::new(7, 0, 0);
        let (a, b) = generate_pattern_positions(min, max, PatternType::StripesX, 2, 50, 0);

        // 8 blocks, period 2 = 4 of each (0-1 A, 2-3 B, 4-5 A, 6-7 B)
        assert_eq!(a.len(), 4);
        assert_eq!(b.len(), 4);
    }

    #[test]
    fn test_random_distribution() {
        let min = Vector3::new(0, 0, 0);
        let max = Vector3::new(9, 9, 9);
        let (a, _b) = generate_pattern_positions(min, max, PatternType::Random, 1, 75, 12345);

        // 1000 blocks, 75% chance for A, should be roughly 750
        let ratio = a.len() as f32 / 1000.0;
        assert!(
            ratio > 0.65 && ratio < 0.85,
            "Expected ~75% but got {}",
            ratio
        );
    }

    #[test]
    fn test_gradient_y() {
        let min = Vector3::new(0, 0, 0);
        let max = Vector3::new(0, 99, 0);
        let (a, b) = generate_pattern_positions(min, max, PatternType::GradientY, 1, 50, 42);

        // 100 blocks vertically, should trend from A at bottom to B at top
        assert_eq!(a.len() + b.len(), 100);

        // Count A blocks in bottom vs top half
        let bottom_a = a.iter().filter(|p| p.y < 50).count();
        let top_a = a.iter().filter(|p| p.y >= 50).count();
        assert!(bottom_a > top_a, "Gradient should have more A at bottom");
    }
}

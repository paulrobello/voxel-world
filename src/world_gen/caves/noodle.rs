//! Noodle caves - fine network of narrow passages.
//!
//! Uses same dual noise intersection as spaghetti caves but with
//! higher frequency and tighter thresholds for a dense web of
//! narrow, interconnected passages.

use noise::{NoiseFn, Perlin};

/// Noodle cave generator for fine tunnel networks.
#[derive(Clone)]
pub struct NoodleCaves {
    /// First noise layer for intersection
    noise1: Perlin,
    /// Second noise layer for intersection
    noise2: Perlin,
}

impl NoodleCaves {
    /// Creates a new noodle cave generator with the given seed.
    pub fn new(seed: u32) -> Self {
        Self {
            noise1: Perlin::new(seed + 300),
            noise2: Perlin::new(seed + 301),
        }
    }

    /// Check if a position is within a noodle cave tunnel.
    ///
    /// Noodle caves use the same dual noise intersection as spaghetti,
    /// but with higher frequency and tighter threshold for finer tunnels.
    ///
    /// # Arguments
    /// * `world_x`, `world_y`, `world_z` - World coordinates
    /// * `surface_height` - Height of terrain at this XZ position
    ///
    /// # Returns
    /// `true` if this position should be carved as a noodle cave
    pub fn is_cave(&self, world_x: i32, world_y: i32, world_z: i32, surface_height: i32) -> bool {
        // Don't carve near surface (minimum 20 blocks below - noodles are deeper)
        if world_y > surface_height - 20 {
            return false;
        }

        // Don't carve near bedrock
        if world_y <= 1 {
            return false;
        }

        // Noodle caves are most common in mid-depths
        // Rare at very deep or shallow levels
        let optimal_depth = 50;
        let depth_from_optimal = ((world_y - optimal_depth).abs() as f64 / 40.0).clamp(0.0, 1.0);

        // Reduce noodle cave density away from optimal depth
        if depth_from_optimal > 0.8 {
            return false;
        }

        let x = world_x as f64;
        let y = world_y as f64;
        let z = world_z as f64;

        // Medium-high frequency for interconnected passages (was 0.1, now 0.06)
        let n1 = self.noise1.get([x * 0.06, y * 0.08, z * 0.06]);
        let n2 = self
            .noise2
            .get([x * 0.06 + 500.0, y * 0.08, z * 0.06 + 500.0]);

        // Threshold for medium-width tunnels - wider than before for walkable passages
        let threshold = 0.09;

        // Both noise values must be close to zero
        n1.abs() < threshold && n2.abs() < threshold
    }

    /// Check if a position is a potential noodle-spaghetti connection point.
    ///
    /// These are areas where noodle caves are more likely to connect
    /// with larger spaghetti tunnels.
    #[allow(dead_code)]
    pub fn is_connection_point(&self, world_x: i32, world_y: i32, world_z: i32) -> bool {
        let x = world_x as f64;
        let y = world_y as f64;
        let z = world_z as f64;

        // Check for medium frequency intersection (between noodle and spaghetti)
        let n1 = self.noise1.get([x * 0.07, y * 0.1, z * 0.07]);
        let n2 = self
            .noise2
            .get([x * 0.07 + 500.0, y * 0.1, z * 0.07 + 500.0]);

        // Slightly wider threshold at connection points
        n1.abs() < 0.025 && n2.abs() < 0.025
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noodle_caves_surface_protection() {
        let caves = NoodleCaves::new(12345);

        // Should not carve near surface (noodles need more depth)
        assert!(!caves.is_cave(0, 60, 0, 75));
        assert!(!caves.is_cave(0, 58, 0, 75));

        // Should not carve at bedrock
        assert!(!caves.is_cave(0, 1, 0, 75));
    }

    #[test]
    fn test_noodle_caves_consistency() {
        let caves = NoodleCaves::new(12345);

        let result1 = caves.is_cave(100, 40, 100, 75);
        let result2 = caves.is_cave(100, 40, 100, 75);
        assert_eq!(result1, result2);
    }
}

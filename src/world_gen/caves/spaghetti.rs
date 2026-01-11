//! Spaghetti caves - long winding tunnel networks.
//!
//! Uses dual 3D Perlin noise intersection to create long,
//! continuous tunnel networks similar to traditional cave systems.

use noise::{NoiseFn, Perlin};

/// Spaghetti cave generator for tunnel networks.
#[derive(Clone)]
pub struct SpaghettiCaves {
    /// First noise layer for intersection
    noise1: Perlin,
    /// Second noise layer for intersection
    noise2: Perlin,
    /// Regional density variation noise
    density_noise: Perlin,
}

impl SpaghettiCaves {
    /// Creates a new spaghetti cave generator with the given seed.
    pub fn new(seed: u32) -> Self {
        Self {
            noise1: Perlin::new(seed + 200),
            noise2: Perlin::new(seed + 201),
            density_noise: Perlin::new(seed + 202),
        }
    }

    /// Check if a position is within a spaghetti cave tunnel.
    ///
    /// Spaghetti caves are created by finding intersections of two
    /// 3D noise fields. Where both noise values are close to zero,
    /// a tunnel forms.
    ///
    /// # Arguments
    /// * `world_x`, `world_y`, `world_z` - World coordinates
    /// * `surface_height` - Height of terrain at this XZ position
    ///
    /// # Returns
    /// `true` if this position should be carved as a spaghetti cave
    pub fn is_cave(&self, world_x: i32, world_y: i32, world_z: i32, surface_height: i32) -> bool {
        // Don't carve near surface (minimum 12 blocks below)
        if world_y > surface_height - 12 {
            return false;
        }

        // Don't carve near bedrock
        if world_y <= 1 {
            return false;
        }

        let x = world_x as f64;
        let y = world_y as f64;
        let z = world_z as f64;

        // Lower frequency for wider tunnels (was 0.05, now 0.03)
        // Y axis stretched for more horizontal tunnels
        let n1 = self.noise1.get([x * 0.03, y * 0.05, z * 0.03]);
        let n2 = self
            .noise2
            .get([x * 0.03 + 1000.0, y * 0.05, z * 0.03 + 1000.0]);

        // Regional density affects threshold
        let density = self.density_noise.get([x * 0.01, z * 0.01]) * 0.5 + 0.5;

        // Depth bonus - more caves deeper down
        let depth_factor = ((surface_height - world_y) as f64 / 50.0).clamp(0.0, 1.0);
        let depth_bonus = depth_factor * 0.02;

        // Base threshold with density variation
        // Higher threshold = wider caves (easier to pass intersection test)
        let threshold = 0.12 - (density * 0.02) - depth_bonus;

        // Intersection: both noise values must be close to zero
        // Use looser threshold for more cave coverage
        n1.abs() < threshold && n2.abs() < threshold
    }

    /// Check if this is a cave entrance location.
    ///
    /// Entrances allow spaghetti caves to breach the surface.
    pub fn is_entrance(&self, world_x: i32, world_z: i32) -> bool {
        let x = world_x as f64;
        let z = world_z as f64;
        let entrance_value = self.density_noise.get([x * 0.02, z * 0.02]);
        entrance_value > 0.45
    }

    /// Check for a cave with entrance support.
    ///
    /// Entrances reduce the surface buffer to allow caves to breach.
    pub fn is_cave_with_entrance(
        &self,
        world_x: i32,
        world_y: i32,
        world_z: i32,
        surface_height: i32,
    ) -> bool {
        let is_entrance = self.is_entrance(world_x, world_z);
        let surface_buffer = if is_entrance { 0 } else { 12 };

        if world_y > surface_height - surface_buffer || world_y <= 1 {
            return false;
        }

        let x = world_x as f64;
        let y = world_y as f64;
        let z = world_z as f64;

        let n1 = self.noise1.get([x * 0.03, y * 0.05, z * 0.03]);
        let n2 = self
            .noise2
            .get([x * 0.03 + 1000.0, y * 0.05, z * 0.03 + 1000.0]);

        let density = self.density_noise.get([x * 0.01, z * 0.01]) * 0.5 + 0.5;
        let depth_factor = ((surface_height - world_y) as f64 / 50.0).clamp(0.0, 1.0);
        let threshold = 0.12 - (density * 0.02) - (depth_factor * 0.02);

        n1.abs() < threshold && n2.abs() < threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spaghetti_caves_surface_protection() {
        let caves = SpaghettiCaves::new(12345);

        // Should not carve near surface (without entrance)
        assert!(!caves.is_cave(0, 70, 0, 75));
        assert!(!caves.is_cave(0, 68, 0, 75));

        // Should not carve at bedrock
        assert!(!caves.is_cave(0, 1, 0, 75));
        assert!(!caves.is_cave(0, 0, 0, 75));
    }

    #[test]
    fn test_spaghetti_caves_consistency() {
        let caves = SpaghettiCaves::new(12345);

        let result1 = caves.is_cave(100, 30, 100, 75);
        let result2 = caves.is_cave(100, 30, 100, 75);
        assert_eq!(result1, result2);
    }
}

//! Carved caves - traditional worm-carved tunnels and ravines.
//!
//! Uses noise-guided carving to create traditional Minecraft-style
//! carved tunnels and vertical ravine features.

use noise::{NoiseFn, Perlin};

/// Carved cave generator using noise-guided "worm" carving.
#[derive(Clone)]
pub struct CarvedCaves {
    /// Noise for determining carver presence
    carver_presence: Perlin,
    /// Noise for carver path direction
    direction_noise: Perlin,
    /// Noise for carver radius variation
    radius_noise: Perlin,
    /// Noise for ravine detection
    ravine_noise: Perlin,
}

impl CarvedCaves {
    /// Creates a new carved cave generator with the given seed.
    pub fn new(seed: u32) -> Self {
        Self {
            carver_presence: Perlin::new(seed + 400),
            direction_noise: Perlin::new(seed + 401),
            radius_noise: Perlin::new(seed + 402),
            ravine_noise: Perlin::new(seed + 403),
        }
    }

    /// Check if a position is within a carved cave or ravine.
    ///
    /// Carved caves simulate worm-carved tunnels by using noise
    /// to determine carver presence and influence. Unlike cheese
    /// or spaghetti caves, carved caves can create distinct
    /// tunnel shapes and ravines.
    ///
    /// # Arguments
    /// * `world_x`, `world_y`, `world_z` - World coordinates
    /// * `surface_height` - Height of terrain at this XZ position
    ///
    /// # Returns
    /// `true` if this position should be carved
    pub fn is_cave(&self, world_x: i32, world_y: i32, world_z: i32, surface_height: i32) -> bool {
        // Check regular carved tunnel
        if self.is_carved_tunnel(world_x, world_y, world_z, surface_height) {
            return true;
        }

        // Check for ravine
        if self.is_ravine(world_x, world_y, world_z, surface_height) {
            return true;
        }

        false
    }

    /// Check if this position is in a carved tunnel.
    fn is_carved_tunnel(
        &self,
        world_x: i32,
        world_y: i32,
        world_z: i32,
        surface_height: i32,
    ) -> bool {
        // Don't carve near surface
        if world_y > surface_height - 10 {
            return false;
        }

        // Don't carve near bedrock
        if world_y <= 2 {
            return false;
        }

        let x = world_x as f64;
        let y = world_y as f64;
        let z = world_z as f64;

        // Check if a carver path passes near this point
        // We simulate this by checking if the position is close to
        // a noise-defined "center line" of a tunnel

        // Carver presence determines if there's a tunnel nearby
        let presence = self.carver_presence.get([x * 0.03, y * 0.05, z * 0.03]);

        // Only carve in regions where presence is high enough
        // Lower threshold = more carved tunnels
        if presence < 0.40 {
            return false;
        }

        // Direction noise determines tunnel shape
        let dir_x = self
            .direction_noise
            .get([x * 0.04, y * 0.06, z * 0.04 + 100.0]);
        let dir_z = self
            .direction_noise
            .get([x * 0.04 + 200.0, y * 0.06, z * 0.04]);

        // Calculate distance from tunnel center
        // The center meanders based on direction noise
        let offset_x = dir_x * 3.0;
        let offset_z = dir_z * 3.0;

        // Effective position relative to tunnel center
        let rel_x = (x % 16.0) - 8.0 + offset_x;
        let rel_z = (z % 16.0) - 8.0 + offset_z;
        let distance_sq = rel_x * rel_x + rel_z * rel_z;

        // Radius varies with noise
        let radius_factor = self.radius_noise.get([x * 0.05, y * 0.08, z * 0.05]) * 0.5 + 0.5;
        let base_radius = 2.5 + radius_factor * 2.0;
        let radius_sq = base_radius * base_radius;

        distance_sq < radius_sq
    }

    /// Check if this position is in a ravine.
    ///
    /// Ravines are tall, narrow carved features that can extend
    /// close to the surface.
    fn is_ravine(&self, world_x: i32, world_y: i32, world_z: i32, surface_height: i32) -> bool {
        // Ravines can get closer to surface than regular caves
        if world_y > surface_height - 5 {
            return false;
        }

        // Don't carve near bedrock
        if world_y <= 3 {
            return false;
        }

        let x = world_x as f64;
        let y = world_y as f64;
        let z = world_z as f64;

        // Ravine presence - ravines are rare
        let presence = self.ravine_noise.get([x * 0.01, z * 0.01]);

        // Only 5% of areas have ravines
        if presence < 0.85 {
            return false;
        }

        // Ravine shape: narrow in X, tall in Y
        let ravine_x = self
            .ravine_noise
            .get([x * 0.06, y * 0.02, z * 0.06 + 500.0]);

        // Ravines are very narrow (threshold close to 0)
        if ravine_x.abs() > 0.02 {
            return false;
        }

        // Ravine width varies with depth
        let depth_factor = (surface_height - world_y) as f64 / surface_height as f64;
        let width_threshold = 0.02 + depth_factor * 0.01;

        ravine_x.abs() < width_threshold
    }

    /// Check if this position is a potential carved cave entrance.
    ///
    /// Carved entrances are areas where carved tunnels breach the surface.
    #[allow(dead_code)]
    pub fn is_entrance(&self, world_x: i32, world_z: i32) -> bool {
        let x = world_x as f64;
        let z = world_z as f64;

        // Use presence noise at surface to find entrance points
        let entrance = self.carver_presence.get([x * 0.02, 0.0, z * 0.02]);
        entrance > 0.75
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_carved_caves_surface_protection() {
        let caves = CarvedCaves::new(12345);

        // Tunnels should not carve near surface
        // (ravines can get closer)
        assert!(!caves.is_carved_tunnel(0, 70, 0, 75));

        // Should not carve at bedrock
        assert!(!caves.is_cave(0, 1, 0, 75));
        assert!(!caves.is_cave(0, 2, 0, 75));
    }

    #[test]
    fn test_carved_caves_consistency() {
        let caves = CarvedCaves::new(12345);

        let result1 = caves.is_cave(100, 30, 100, 75);
        let result2 = caves.is_cave(100, 30, 100, 75);
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_ravine_rarity() {
        let caves = CarvedCaves::new(12345);

        // Count ravines in a sample area
        let mut ravine_count = 0;
        for x in 0..100 {
            for z in 0..100 {
                if caves.is_ravine(x, 40, z, 75) {
                    ravine_count += 1;
                }
            }
        }

        // Ravines should be rare (less than 5% of blocks)
        assert!(ravine_count < 500, "Too many ravines: {ravine_count}/10000");
    }
}

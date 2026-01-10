use crate::terrain_gen::BiomeType;
use noise::{NoiseFn, Perlin};

/// Cave generation system with biome-specific characteristics.
#[derive(Clone)]
pub struct CaveGenerator {
    /// 3D Perlin noise for cave tunnels ("spaghetti caves")
    cave_noise: Perlin,
    /// 2D noise for regional cave density variation
    cave_mask_noise: Perlin,
    /// 2D noise for determining cave entrances
    entrance_noise: Perlin,
    /// 3D noise for cave decoration placement (stalactites/stalagmites)
    decoration_noise: Perlin,
}

impl CaveGenerator {
    /// Creates a new cave generator with the given seed.
    pub fn new(seed: u32) -> Self {
        Self {
            cave_noise: Perlin::new(seed + 3),
            cave_mask_noise: Perlin::new(seed + 4),
            entrance_noise: Perlin::new(seed + 5),
            decoration_noise: Perlin::new(seed + 8),
        }
    }

    /// Check if a position is at a cave entrance (allows caves to breach surface).
    pub fn is_entrance(&self, world_x: i32, world_z: i32) -> bool {
        let x = world_x as f64;
        let z = world_z as f64;
        let entrance_value = self.entrance_noise.get([x * 0.02, z * 0.02]);
        // ~25% of cave areas become entrances
        entrance_value > 0.45
    }

    /// Check if a position should be carved out as a cave.
    ///
    /// # Arguments
    /// * `world_x`, `world_y`, `world_z` - World coordinates of the block
    /// * `surface_height` - Terrain height at this XZ position
    /// * `biome` - Biome type for biome-specific cave characteristics
    ///
    /// # Returns
    /// `true` if this block should be carved out as cave space
    pub fn is_cave(
        &self,
        world_x: i32,
        world_y: i32,
        world_z: i32,
        surface_height: i32,
        biome: BiomeType,
    ) -> bool {
        // Determine surface buffer based on whether this is an entrance location
        // Entrances reduce the buffer to allow caves to breach the surface
        // Buffer of 12 prevents caves from breaching near-surface layers (SEA_LEVEL=75)
        let is_entrance = self.is_entrance(world_x, world_z);
        let surface_buffer = if is_entrance { 0 } else { 12 };

        // Don't carve near surface unless at entrance, and never below y=1 (protect bedrock at y=0)
        if world_y > surface_height - surface_buffer || world_y < 1 {
            return false;
        }

        let x = world_x as f64;
        let y = world_y as f64;
        let z = world_z as f64;

        // Regional cave density (some areas have more caves)
        let cave_density = self.cave_mask_noise.get([x * 0.01, z * 0.01]) * 0.5 + 0.5;

        // Biome-specific cave density multipliers
        let biome_density_multiplier = match biome {
            BiomeType::Mountains => 2.0, // Much more caves in mountains (increased from 1.5 for lava)
            BiomeType::Desert => 0.6,    // Fewer caves in desert
            BiomeType::Swamp => 0.8,     // Slightly fewer caves
            BiomeType::Snow => 0.9,      // Slightly fewer caves
            BiomeType::Grassland => 1.0, // Normal cave density
        };

        // 3D cave noise - "spaghetti" style caves
        // Stretched in Y for more horizontal tunnels
        let cave_value = self.cave_noise.get([x * 0.05, y * 0.08, z * 0.05]);

        // Threshold varies by depth (more caves deeper down)
        let depth_factor = ((surface_height - world_y) as f64 / 30.0).clamp(0.0, 1.0);
        let threshold =
            0.55 - (depth_factor * 0.15) - (cave_density * 0.1 * biome_density_multiplier);

        cave_value.abs() > threshold
    }

    /// Determine what should fill a cave block based on biome and depth.
    ///
    /// # Returns
    /// * `CaveFillType::Air` - Empty cave space
    /// * `CaveFillType::Water(WaterType)` - Water-filled cave
    /// * `CaveFillType::Lava` - Lava-filled cave (mountain caves at low depths)
    pub fn get_cave_fill(&self, biome: BiomeType, world_y: i32, sea_level: i32) -> CaveFillType {
        // All biomes: lava lakes at Y: 2-10
        // Use noise to create pockets rather than filling entire floor
        if (2..=10).contains(&world_y) {
            let depth_factor = (10 - world_y) as f64 / 8.0; // 0.0 at Y=10, 1.0 at Y=2
            let lava_threshold = 0.7 - (depth_factor * 0.4); // 0.7 at top, 0.3 at bottom

            // Use noise to determine if this specific position should have lava
            let noise_value = self.decoration_noise.get([
                (world_y as f64) * 0.3,
                (world_y as f64) * 0.2,
                (world_y as f64) * 0.25,
            ]);

            if noise_value.abs() > lava_threshold {
                return CaveFillType::Lava;
            }
        }

        match biome {
            BiomeType::Desert => {
                // Desert caves are always dry (no water)
                CaveFillType::Air
            }
            BiomeType::Swamp => {
                // Swamp caves are heavily flooded
                if world_y <= sea_level + 5 {
                    CaveFillType::Water(biome.water_type())
                } else {
                    CaveFillType::Air
                }
            }
            BiomeType::Snow => {
                // Ice caves: Empty caves with ice walls (underground is ice in snow biome)
                CaveFillType::Air
            }
            BiomeType::Mountains => {
                // Mountain caves: lava lakes all the way to sea level
                // Use noise to create pockets rather than filling entire floor
                if world_y <= sea_level {
                    let depth_factor = (sea_level - world_y) as f64 / sea_level as f64;
                    let lava_threshold = 0.7 - (depth_factor * 0.4); // More lava deeper down

                    let noise_value = self.decoration_noise.get([
                        (world_y as f64) * 0.05,
                        (world_y as f64) * 0.05,
                        (world_y as f64) * 0.05,
                    ]);

                    if noise_value.abs() > lava_threshold {
                        CaveFillType::Lava
                    } else if world_y <= sea_level {
                        CaveFillType::Water(biome.water_type())
                    } else {
                        CaveFillType::Air
                    }
                } else {
                    CaveFillType::Air
                }
            }
            BiomeType::Grassland => {
                // Grassland: no water filling in caves
                CaveFillType::Air
            }
        }
    }

    /// Check if lava lakes should spawn at this cave position.
    ///
    /// All biomes have lava lakes at Y: 2-10.
    /// Mountains have additional lava lakes up to sea level (Y: 75).
    /// Uses 3D noise to create pockets of lava rather than filling all caves.
    pub fn should_spawn_lava(
        &self,
        world_x: i32,
        biome: BiomeType,
        world_y: i32,
        world_z: i32,
    ) -> bool {
        let x = world_x as f64;
        let y = world_y as f64;
        let z = world_z as f64;

        // All biomes: lava lakes at Y: 2-10
        if (2..=10).contains(&world_y) {
            let depth_factor = (10 - world_y) as f64 / 8.0;
            let lava_threshold = 0.7 - (depth_factor * 0.4);

            let noise_value =
                self.decoration_noise
                    .get([x * 0.05 + 1000.0, y * 0.3, z * 0.05 + 1000.0]);

            return noise_value.abs() > lava_threshold;
        }

        // Mountains only: lava lakes up to sea level
        if matches!(biome, BiomeType::Mountains) && world_y <= 75 {
            let depth_factor = (75 - world_y) as f64 / 75.0;
            let lava_threshold = 0.7 - (depth_factor * 0.4);

            let noise_value =
                self.decoration_noise
                    .get([x * 0.05 + 1000.0, y * 0.05, z * 0.05 + 1000.0]);

            return noise_value.abs() > lava_threshold;
        }

        false
    }

    /// Get the model ID for a stalactite (hanging from ceiling) based on biome.
    ///
    /// Model IDs:
    /// - 106: Stone stalactite
    /// - 108: Ice stalactite (for snow biome)
    pub fn get_stalactite_model(&self, biome: BiomeType) -> u8 {
        match biome {
            BiomeType::Snow => 108, // Ice stalactite
            _ => 106,               // Stone stalactite
        }
    }

    /// Get the model ID for a stalagmite (growing from floor) based on biome.
    ///
    /// Model IDs:
    /// - 107: Stone stalagmite
    /// - 109: Ice stalagmite (for snow biome)
    pub fn get_stalagmite_model(&self, biome: BiomeType) -> u8 {
        match biome {
            BiomeType::Snow => 109, // Ice stalagmite
            _ => 107,               // Stone stalagmite
        }
    }

    /// Check if a stalactite should be placed at this ceiling position.
    ///
    /// Returns `Some(model_id)` if a stalactite should be placed, `None` otherwise.
    ///
    /// # Arguments
    /// * `world_x`, `world_y`, `world_z` - Position of the cave ceiling block
    /// * `biome` - Biome type for selecting appropriate model
    ///
    /// Stalactites spawn on ~15% of cave ceiling blocks.
    pub fn should_place_stalactite(
        &self,
        world_x: i32,
        world_y: i32,
        world_z: i32,
        biome: BiomeType,
    ) -> Option<u8> {
        let x = world_x as f64;
        let y = world_y as f64;
        let z = world_z as f64;

        // Use 3D noise for varied distribution
        let noise_value = self.decoration_noise.get([x * 0.1, y * 0.1, z * 0.1]);

        // ~15% spawn rate
        if noise_value > 0.7 {
            Some(self.get_stalactite_model(biome))
        } else {
            None
        }
    }

    /// Check if a stalagmite should be placed at this floor position.
    ///
    /// Returns `Some(model_id)` if a stalagmite should be placed, `None` otherwise.
    ///
    /// # Arguments
    /// * `world_x`, `world_y`, `world_z` - Position of the cave floor block
    /// * `biome` - Biome type for selecting appropriate model
    ///
    /// Stalagmites spawn on ~15% of cave floor blocks.
    pub fn should_place_stalagmite(
        &self,
        world_x: i32,
        world_y: i32,
        world_z: i32,
        biome: BiomeType,
    ) -> Option<u8> {
        let x = world_x as f64;
        let y = world_y as f64;
        let z = world_z as f64;

        // Use 3D noise for varied distribution (offset slightly from stalactites)
        let noise_value = self
            .decoration_noise
            .get([x * 0.1 + 100.0, y * 0.1, z * 0.1 + 100.0]);

        // ~15% spawn rate
        if noise_value > 0.7 {
            Some(self.get_stalagmite_model(biome))
        } else {
            None
        }
    }
}

/// Type of content to fill a cave with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaveFillType {
    /// Empty cave space (air)
    Air,
    /// Filled with water of a specific type
    Water(crate::chunk::WaterType),
    /// Filled with lava (mountain caves at low depths)
    Lava,
}

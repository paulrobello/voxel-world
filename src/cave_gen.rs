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
    /// 3D noise for ice cave determination in snow biomes
    ice_cave_noise: Perlin,
}

impl CaveGenerator {
    /// Creates a new cave generator with the given seed.
    pub fn new(seed: u32) -> Self {
        Self {
            cave_noise: Perlin::new(seed + 3),
            cave_mask_noise: Perlin::new(seed + 4),
            entrance_noise: Perlin::new(seed + 5),
            decoration_noise: Perlin::new(seed + 8),
            ice_cave_noise: Perlin::new(seed + 9),
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
        // Increased buffer from 5 to 12 to account for raised terrain (SEA_LEVEL=124)
        let is_entrance = self.is_entrance(world_x, world_z);
        let surface_buffer = if is_entrance { 0 } else { 12 };

        // Don't carve near surface unless at entrance, and never below y=2
        if world_y > surface_height - surface_buffer || world_y < 2 {
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
        // All biomes: possible lava in 5 layers above bedrock (Y: 3-7)
        if (3..=7).contains(&world_y) {
            let depth_factor = (7 - world_y) as f64 / 5.0;
            if depth_factor > 0.4 {
                // ~60% chance at Y=3, decreasing to ~40% at Y=7
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
                // Ice caves: ice blocks fill some caves for frozen cave effect
                // Use 3D noise to create pockets of ice vs air caves
                let ice_chance =
                    self.ice_cave_noise
                        .get([(world_y as f64) * 0.15, world_y as f64 * 0.08, 0.0]);

                // ~60% of caves have ice, 40% are air caves
                if ice_chance > 0.2 {
                    CaveFillType::Ice
                } else {
                    CaveFillType::Air
                }
            }
            BiomeType::Mountains => {
                // Mountain caves: lava lakes at mid-to-deep depths (Y < 100)
                let lava_depth_chance = if world_y < 100 {
                    let depth_factor = (100 - world_y) as f64 / 98.0;
                    depth_factor > 0.2
                } else {
                    false
                };

                if lava_depth_chance {
                    CaveFillType::Lava
                } else if world_y <= sea_level {
                    CaveFillType::Water(biome.water_type())
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
    /// Mountain caves have lava lakes at mid-to-deep depths (Y < 100).
    /// Uses 3D noise to create pockets of lava rather than filling all caves.
    pub fn should_spawn_lava(
        &self,
        world_x: i32,
        biome: BiomeType,
        world_y: i32,
        world_z: i32,
    ) -> bool {
        if !matches!(biome, BiomeType::Mountains) || world_y >= 100 {
            return false;
        }

        // Lava becomes more common the deeper you go
        // At y=99: ~20% chance, at y=50: ~50% chance, at y=2: ~80% chance
        let depth_factor = (100 - world_y) as f64 / 98.0; // 0.0 at y=99, 1.0 at y=2
        let lava_threshold = 0.7 - (depth_factor * 0.5); // 0.7 at top, 0.2 at bottom

        // Use proper 3D coordinates for noise to create varied lava pockets
        // Offset coordinates to get different noise pattern than cave decorations
        let x = world_x as f64;
        let y = world_y as f64;
        let z = world_z as f64;
        let noise_value =
            self.decoration_noise
                .get([x * 0.05 + 1000.0, y * 0.05, z * 0.05 + 1000.0]);

        noise_value.abs() > lava_threshold
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
    /// Filled with ice (snow biome caves)
    Ice,
}

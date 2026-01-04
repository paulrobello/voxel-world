use crate::chunk::{BlockType, CHUNK_SIZE, Chunk};
use crate::config::WorldGenType;
use nalgebra::Vector3;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin, RidgedMulti};

// Terrain generation constants
/// Sea level for water filling (blocks below this in valleys become water)
pub const SEA_LEVEL: i32 = 28;

/// Terrain generator using multiple noise layers for varied landscapes
#[derive(Clone)]
pub struct TerrainGenerator {
    height_noise: Fbm<Perlin>,
    detail_noise: Perlin,
    mountain_noise: RidgedMulti<Perlin>,
    biome_noise: Perlin,
    cave_noise: Perlin,
    cave_mask_noise: Perlin,
    entrance_noise: Perlin,
}

impl TerrainGenerator {
    pub fn new(seed: u32) -> Self {
        // Base continental noise for large-scale terrain features
        let height_noise = Fbm::<Perlin>::new(seed)
            .set_octaves(4)
            .set_frequency(0.003) // Very low frequency for continent-scale features
            .set_lacunarity(2.0)
            .set_persistence(0.5);

        let detail_noise = Perlin::new(seed.wrapping_add(1));

        // Mountain ridges using RidgedMulti for sharp peaks
        let mountain_noise = RidgedMulti::<Perlin>::new(seed.wrapping_add(2))
            .set_octaves(5)
            .set_frequency(0.008) // Mountain-scale features
            .set_lacunarity(2.2)
            .set_persistence(0.5);

        // Biome noise - determines flat plains vs hilly vs mountainous regions
        // Very low frequency for large biome regions
        let biome_noise = Perlin::new(seed.wrapping_add(6));

        // 3D noise for cave carving
        let cave_noise = Perlin::new(seed.wrapping_add(3));

        // Regional variation in cave density
        let cave_mask_noise = Perlin::new(seed.wrapping_add(4));

        // Noise for cave entrance locations (~25% of cave areas get entrances)
        let entrance_noise = Perlin::new(seed.wrapping_add(5));

        Self {
            height_noise,
            detail_noise,
            mountain_noise,
            biome_noise,
            cave_noise,
            cave_mask_noise,
            entrance_noise,
        }
    }

    /// Get terrain height at world coordinates
    pub fn get_height(&self, world_x: i32, world_z: i32) -> i32 {
        let x = world_x as f64;
        let z = world_z as f64;

        // Biome type: determines flat plains (-1) vs rolling hills (0) vs mountains (+1)
        // Very low frequency for large coherent regions
        let biome_raw = self.biome_noise.get([x * 0.004, z * 0.004]);

        // Create distinct biome zones with sharper transitions
        // Values < -0.3 = flat plains, > 0.3 = mountains, between = rolling hills
        let biome_type = if biome_raw < -0.3 {
            0.0 // Flat plains
        } else if biome_raw > 0.3 {
            1.0 // Mountains
        } else {
            // Smooth transition zone (rolling hills)
            ((biome_raw + 0.3) / 0.6).clamp(0.0, 1.0)
        };

        // Base continental terrain (large smooth features)
        let base = self.height_noise.get([x, z]);

        // Mountain ridges (sharp peaks)
        let ridges = self.mountain_noise.get([x, z]);

        // Detail noise for subtle variation
        let detail = self.detail_noise.get([x * 0.02, z * 0.02]);

        // Calculate height based on biome type:
        // - Flat plains: height 32-36 with minimal variation
        // - Rolling hills: height 28-45 with moderate variation
        // - Mountains: height 32-90 with dramatic peaks
        let height = if biome_type < 0.1 {
            // Flat plains - very little variation
            32.0 + detail * 2.0
        } else if biome_type > 0.9 {
            // Mountain biome - dramatic peaks
            let mountain_height = ridges * 55.0;
            32.0 + base * 6.0 + mountain_height
        } else {
            // Transition zone - blend between plains and mountains
            let plains_height = 32.0 + detail * 2.0;
            let hills_height = 32.0 + base * 10.0 + detail * 3.0;
            let mountain_height = 32.0 + base * 6.0 + ridges * 55.0;

            // Smooth blend based on biome_type
            if biome_type < 0.5 {
                // Plains to hills transition
                let t = biome_type / 0.5;
                plains_height * (1.0 - t) + hills_height * t
            } else {
                // Hills to mountains transition
                let t = (biome_type - 0.5) / 0.5;
                hills_height * (1.0 - t) + mountain_height * t
            }
        };

        height.round() as i32
    }

    /// Check if a location is a cave entrance point (~25% of cave areas)
    fn is_entrance(&self, world_x: i32, world_z: i32) -> bool {
        let x = world_x as f64;
        let z = world_z as f64;

        // Low frequency noise for sparse, grouped entrance locations
        // Use multiple octaves for varied entrance sizes
        let entrance_value = self.entrance_noise.get([x * 0.02, z * 0.02]);

        // Threshold of 0.45 gives roughly 25-30% coverage
        // Higher threshold = fewer entrances
        entrance_value > 0.45
    }

    /// Check if a position should be carved out as a cave
    fn is_cave(&self, world_x: i32, world_y: i32, world_z: i32, surface_height: i32) -> bool {
        // Determine surface buffer based on whether this is an entrance location
        // Entrances reduce the buffer to allow caves to breach the surface
        let is_entrance = self.is_entrance(world_x, world_z);
        let surface_buffer = if is_entrance { 0 } else { 5 };

        // Don't carve near surface unless at entrance, and never below y=2
        if world_y > surface_height - surface_buffer || world_y < 2 {
            return false;
        }

        let x = world_x as f64;
        let y = world_y as f64;
        let z = world_z as f64;

        // Regional cave density (some areas have more caves)
        let cave_density = self.cave_mask_noise.get([x * 0.01, z * 0.01]) * 0.5 + 0.5;

        // 3D cave noise - "spaghetti" style caves
        // Stretched in Y for more horizontal tunnels
        let cave_value = self.cave_noise.get([x * 0.05, y * 0.08, z * 0.05]);

        // Threshold varies by depth (more caves deeper down)
        let depth_factor = ((surface_height - world_y) as f64 / 30.0).clamp(0.0, 1.0);
        let threshold = 0.55 - (depth_factor * 0.15) - (cave_density * 0.1);

        cave_value.abs() > threshold
    }

    /// Simple hash for tree placement randomness
    fn hash(&self, x: i32, z: i32) -> i32 {
        let mut h = (x.wrapping_mul(374761393)) ^ (z.wrapping_mul(668265263));
        h = (h ^ (h >> 13)).wrapping_mul(1274126177);
        (h ^ (h >> 16)).abs()
    }
}

/// Generates terrain for a single chunk at the given position.
pub fn generate_chunk_terrain(
    terrain: &TerrainGenerator,
    chunk_pos: Vector3<i32>,
    world_gen_type: WorldGenType,
) -> Chunk {
    match world_gen_type {
        WorldGenType::Normal => generate_normal_chunk(terrain, chunk_pos),
        WorldGenType::Flat => generate_flat_chunk(chunk_pos),
    }
}

/// Generates a flat world chunk (2 chunks = 64 blocks high).
/// Layers from top to bottom: grass (1), dirt (7), stone (55), bedrock (1)
fn generate_flat_chunk(chunk_pos: Vector3<i32>) -> Chunk {
    let mut chunk = Chunk::new();
    let chunk_world_y = chunk_pos.y * CHUNK_SIZE as i32;

    // Flat world height constants (2 chunks = 64 blocks, Y=0 to Y=63)
    const FLAT_HEIGHT: i32 = 63; // Top surface at Y=63
    const GRASS_LAYERS: i32 = 1; // 1 layer of grass (Y=63)
    const DIRT_LAYERS: i32 = 7; // 7 layers of dirt (Y=56-62)

    // Only generate blocks in first two chunk layers (Y=0 and Y=1)
    if chunk_pos.y >= 2 {
        // Above flat world - all air (chunk is already air by default)
        chunk.update_metadata();
        chunk.persistence_dirty = false;
        return chunk;
    }

    for lx in 0..CHUNK_SIZE {
        for lz in 0..CHUNK_SIZE {
            for ly in 0..CHUNK_SIZE {
                let world_y = chunk_world_y + ly as i32;

                let block_type = if world_y > FLAT_HEIGHT {
                    // Above surface
                    BlockType::Air
                } else if world_y == 0 {
                    // Bedrock floor
                    BlockType::Bedrock
                } else if world_y == FLAT_HEIGHT {
                    // Top surface - grass
                    BlockType::Grass
                } else if world_y > FLAT_HEIGHT - GRASS_LAYERS - DIRT_LAYERS {
                    // Dirt layers (Y=56 to Y=62)
                    BlockType::Dirt
                } else {
                    // Stone (Y=1 to Y=55)
                    BlockType::Stone
                };

                chunk.set_block(lx, ly, lz, block_type);
            }
        }
    }

    chunk.update_metadata();
    chunk.persistence_dirty = false;
    chunk
}

/// Generates normal terrain with biomes, caves, and trees.
fn generate_normal_chunk(terrain: &TerrainGenerator, chunk_pos: Vector3<i32>) -> Chunk {
    let mut chunk = Chunk::new();
    let chunk_world_x = chunk_pos.x * CHUNK_SIZE as i32;
    let chunk_world_y = chunk_pos.y * CHUNK_SIZE as i32;
    let chunk_world_z = chunk_pos.z * CHUNK_SIZE as i32;

    // Generate terrain for this chunk
    for lx in 0..CHUNK_SIZE {
        for lz in 0..CHUNK_SIZE {
            let world_x = chunk_world_x + lx as i32;
            let world_z = chunk_world_z + lz as i32;
            let height = terrain.get_height(world_x, world_z);

            for ly in 0..CHUNK_SIZE {
                let world_y = chunk_world_y + ly as i32;

                let block_type = if world_y == 0 {
                    // Bedrock floor - unbreakable, prevents falling out of world
                    BlockType::Bedrock
                } else if world_y > height && world_y > SEA_LEVEL {
                    // Above terrain and above sea level = air
                    BlockType::Air
                } else if world_y > height && world_y <= SEA_LEVEL {
                    // Above terrain but below sea level = water (flat lake surface)
                    BlockType::Water
                } else if terrain.is_cave(world_x, world_y, world_z, height) {
                    // Carved out cave - fill with water if below sea level
                    if world_y <= SEA_LEVEL {
                        BlockType::Water
                    } else {
                        BlockType::Air
                    }
                } else if world_y == height {
                    // Surface block - varies by elevation (biome)
                    if height > 70 {
                        BlockType::Snow // Snow-capped peaks
                    } else if height > 55 {
                        BlockType::Stone // Rocky mountain surface
                    } else if height <= SEA_LEVEL + 2 {
                        BlockType::Sand // Beach/shore near water level
                    } else {
                        BlockType::Grass // Normal grassland
                    }
                } else if world_y > height - 3 {
                    // Subsurface layer
                    if height > 55 {
                        BlockType::Stone // Mountains: stone all the way
                    } else if height <= SEA_LEVEL + 2 {
                        BlockType::Sand // Sandy beach substrate
                    } else {
                        BlockType::Dirt // Normal: dirt layer
                    }
                } else {
                    BlockType::Stone // Deep underground
                };
                chunk.set_block(lx, ly, lz, block_type);
            }
        }
    }

    // Add trees deterministically based on chunk position
    // Trees are placed if hash of position within chunk meets threshold
    for lx in (2..CHUNK_SIZE - 2).step_by(8) {
        for lz in (2..CHUNK_SIZE - 2).step_by(8) {
            let world_x = chunk_world_x + lx as i32;
            let world_z = chunk_world_z + lz as i32;
            let height = terrain.get_height(world_x, world_z);

            // Only place trees in grassland areas (not on mountains)
            if height > 55 {
                continue;
            }

            // Deterministic tree placement
            if terrain.hash(world_x, world_z) % 100 < 15 {
                let local_base_y = height - chunk_world_y;

                // Only place tree if the base is in this chunk
                if local_base_y >= 0 && local_base_y < CHUNK_SIZE as i32 - 6 {
                    let trunk_height = 5 + (terrain.hash(world_x, world_z).abs() % 3);

                    // Tree trunk
                    for dy in 1..=trunk_height {
                        let ly = (local_base_y + dy) as usize;
                        if ly < CHUNK_SIZE {
                            chunk.set_block(lx, ly, lz, BlockType::Log);
                        }
                    }

                    // Simple canopy
                    let canopy_base = (local_base_y + trunk_height) as usize;
                    for dx in -2i32..=2 {
                        for dz in -2i32..=2 {
                            for dy in 0..3 {
                                let nlx = lx as i32 + dx;
                                let nly = canopy_base as i32 + dy;
                                let nlz = lz as i32 + dz;

                                if nlx >= 0
                                    && nlx < CHUNK_SIZE as i32
                                    && nly >= 0
                                    && nly < CHUNK_SIZE as i32
                                    && nlz >= 0
                                    && nlz < CHUNK_SIZE as i32
                                {
                                    let dist =
                                        ((dx * dx + dz * dz) as f32).sqrt() + (dy as f32 * 0.5);
                                    if dist <= 2.5 {
                                        let block = chunk.get_block(
                                            nlx as usize,
                                            nly as usize,
                                            nlz as usize,
                                        );
                                        if block == BlockType::Air {
                                            chunk.set_block(
                                                nlx as usize,
                                                nly as usize,
                                                nlz as usize,
                                                BlockType::Leaves,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    chunk.update_metadata();
    // Procedurally generated chunk is not dirty for persistence until modified.
    chunk.persistence_dirty = false;
    chunk
}

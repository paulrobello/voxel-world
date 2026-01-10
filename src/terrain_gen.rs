// Cross-chunk terrain generation requires many parameters (chunk coords, overflow blocks)
// which exceeds clippy's default limit. This is intentional for the overflow block system.
#![allow(clippy::too_many_arguments)]
#![allow(clippy::ptr_arg)]

use crate::cave_gen::{CaveFillType, CaveGenerator};
use crate::chunk::{BlockType, CHUNK_SIZE, Chunk, WaterType};
use crate::config::WorldGenType;
use nalgebra::Vector3;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin, RidgedMulti};

/// Represents a block that should be placed outside the current chunk
#[derive(Clone, Debug)]
pub struct OverflowBlock {
    pub world_pos: Vector3<i32>,
    pub block_type: BlockType,
}

/// Result of chunk terrain generation including overflow blocks
pub struct ChunkGenerationResult {
    pub chunk: Chunk,
    pub overflow_blocks: Vec<OverflowBlock>,
}

// Terrain generation constants
/// Sea level for water filling (blocks below this in valleys become water)
pub const SEA_LEVEL: i32 = 75;

// Model IDs for ground cover
const MODEL_TALL_GRASS: u8 = 100;
const MODEL_FLOWER_RED: u8 = 101;
const MODEL_FLOWER_YELLOW: u8 = 102;
const MODEL_LILY_PAD: u8 = 103;
const MODEL_MUSHROOM_BROWN: u8 = 104;
const MODEL_MUSHROOM_RED: u8 = 105;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BiomeInfo {
    pub elevation: f64,
    pub temperature: f64,
    pub rainfall: f64,
    pub biome: BiomeType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BiomeType {
    Grassland,
    Mountains,
    Desert,
    Swamp,
    Snow,
}

impl BiomeType {
    pub fn water_type(&self) -> WaterType {
        match self {
            BiomeType::Grassland => WaterType::Lake,
            BiomeType::Mountains => WaterType::Spring,
            BiomeType::Desert => WaterType::River, // Sparse rivers in desert
            BiomeType::Swamp => WaterType::Swamp,
            BiomeType::Snow => WaterType::River, // Icy rivers
        }
    }
}

/// Terrain generator using multiple noise layers for varied landscapes
#[derive(Clone)]
pub struct TerrainGenerator {
    height_noise: Fbm<Perlin>,
    detail_noise: Perlin,
    mountain_noise: RidgedMulti<Perlin>,
    // biome_noise replaced by temperature/rainfall logic
    temperature_noise: Perlin,
    rainfall_noise: Perlin,
    mountain_region_noise: Perlin, // Large-scale mountain region definition
    cave_generator: CaveGenerator,
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

        // Temperature noise - large scale variation
        let temperature_noise = Perlin::new(seed.wrapping_add(6));
        // Rainfall noise - large scale variation
        let rainfall_noise = Perlin::new(seed.wrapping_add(7));
        // Mountain region noise - VERY large scale to create long mountain ranges and plateaus
        // Using extremely low frequency (0.0005) for continent-sized features
        let mountain_region_noise = Perlin::new(seed.wrapping_add(8));

        // Cave generation system
        let cave_generator = CaveGenerator::new(seed);

        Self {
            height_noise,
            detail_noise,
            mountain_noise,
            temperature_noise,
            rainfall_noise,
            mountain_region_noise,
            cave_generator,
        }
    }

    /// Get biome info (elevation, temp, rain) at world coordinates
    pub fn get_biome_info(&self, world_x: i32, world_z: i32) -> BiomeInfo {
        let x = world_x as f64;
        let z = world_z as f64;

        // Base noise values (-1.0 to 1.0)
        let temp_raw = self.temperature_noise.get([x * 0.002, z * 0.002]);
        let rain_raw = self.rainfall_noise.get([x * 0.002, z * 0.002]);

        // Normalize to 0.0 to 1.0
        let temp = temp_raw * 0.5 + 0.5;
        let rain = rain_raw * 0.5 + 0.5;

        // Get approximate height for temperature lapse rate
        let base_height = self.height_noise.get([x, z]); // -1 to 1

        // Adjust temperature by elevation (higher = colder)
        let elevation_cooling = base_height.max(0.0) * 0.4;
        let adjusted_temp = (temp - elevation_cooling).clamp(0.0, 1.0);

        // Check mountain region early for snow biome logic
        // Use very low frequency (0.0005) for continent-sized mountain ranges
        // Perlin noise ranges from -1 to 1, threshold of -0.3 gives ~35% coverage but in large blocks
        let mountain_region = self.mountain_region_noise.get([x * 0.0005, z * 0.0005]);
        let in_mountain_region = mountain_region > -0.3;

        // Determine biome based on temperature and rainfall (hard boundaries)
        let biome = if adjusted_temp < 0.3 {
            // Snow biome (cold regions)
            BiomeType::Snow
        } else if adjusted_temp > 0.7 && rain < 0.3 {
            // Desert biome (hot and dry)
            BiomeType::Desert
        } else if adjusted_temp > 0.6 && rain > 0.7 {
            // Swamp biome (warm and wet)
            BiomeType::Swamp
        } else {
            // Mountain biome determination - use regional noise for contiguous ranges
            // Mountains require BOTH elevated terrain AND being in a mountain region
            // This creates large contiguous ranges and plateaus instead of fragmented peaks
            // Height threshold of 0.25 ensures mountains are elevated (prevents sea-level stone patches)
            let is_mountains = in_mountain_region && base_height > 0.25;

            if is_mountains {
                BiomeType::Mountains
            } else {
                // Grassland (default)
                BiomeType::Grassland
            }
        };

        BiomeInfo {
            elevation: base_height,
            temperature: adjusted_temp,
            rainfall: rain,
            biome,
        }
    }

    /// Get biome type at world coordinates
    pub fn get_biome(&self, world_x: i32, world_z: i32) -> BiomeType {
        self.get_biome_info(world_x, world_z).biome
    }

    /// Get reference to the cave generator
    pub fn cave_generator(&self) -> &CaveGenerator {
        &self.cave_generator
    }

    /// Calculate height for a specific biome (helper for blending)
    fn calculate_biome_height(&self, biome: BiomeType, base: f64, ridges: f64, detail: f64) -> f64 {
        match biome {
            BiomeType::Grassland => 128.0 + detail * 2.0 + base * 4.0,
            BiomeType::Mountains => 128.0 + base * 10.0 + ridges * 55.0,
            BiomeType::Desert => 128.0 + detail * 1.0 + base * 2.0,
            BiomeType::Swamp => 128.0 + detail * 2.0,
            BiomeType::Snow => {
                if base > 0.5 {
                    128.0 + base * 8.0 + ridges * 40.0
                } else {
                    128.0 + detail * 2.0
                }
            }
        }
    }

    /// Get terrain height at world coordinates with smooth transitions at biome boundaries
    pub fn get_height(&self, world_x: i32, world_z: i32) -> i32 {
        let x = world_x as f64;
        let z = world_z as f64;

        // Base continental terrain (large smooth features)
        let base = self.height_noise.get([x, z]);

        // Mountain ridges (sharp peaks)
        let ridges = self.mountain_noise.get([x, z]);

        // Detail noise for subtle variation
        let detail = self.detail_noise.get([x * 0.02, z * 0.02]);

        // Get biome at this location
        let center_biome = self.get_biome(world_x, world_z);

        // Sample neighboring biomes to detect boundaries
        // Use smaller offset for smoother detection
        const SAMPLE_OFFSET: i32 = 4;
        let neighbors = [
            self.get_biome(world_x + SAMPLE_OFFSET, world_z),
            self.get_biome(world_x - SAMPLE_OFFSET, world_z),
            self.get_biome(world_x, world_z + SAMPLE_OFFSET),
            self.get_biome(world_x, world_z - SAMPLE_OFFSET),
        ];

        // Check if we're at a boundary
        let at_boundary = neighbors.iter().any(|&b| b != center_biome);

        if !at_boundary {
            // Not near a boundary - use single biome height
            let height = self.calculate_biome_height(center_biome, base, ridges, detail);
            return height.round() as i32;
        }

        // At a boundary - calculate weighted blend based on all nearby biomes
        // Sample in a small grid to get blend weights
        const BLEND_SAMPLES: i32 = 3;
        let mut biome_heights: std::collections::HashMap<BiomeType, (f64, f64)> =
            std::collections::HashMap::new();

        for dx in -BLEND_SAMPLES..=BLEND_SAMPLES {
            for dz in -BLEND_SAMPLES..=BLEND_SAMPLES {
                let sample_biome = self.get_biome(world_x + dx, world_z + dz);
                let dist = ((dx * dx + dz * dz) as f64).sqrt();
                // Weight by inverse distance (closer = more weight)
                let weight = if dist > 0.0 { 1.0 / dist } else { 4.0 };

                let entry = biome_heights.entry(sample_biome).or_insert((0.0, 0.0));
                entry.0 += weight;
                entry.1 = self.calculate_biome_height(sample_biome, base, ridges, detail);
            }
        }

        // Weighted average of all biome heights
        let total_weight: f64 = biome_heights.values().map(|(w, _)| w).sum();
        let blended_height: f64 = biome_heights
            .values()
            .map(|(weight, height)| weight * height)
            .sum::<f64>()
            / total_weight;

        blended_height.round() as i32
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
) -> ChunkGenerationResult {
    match world_gen_type {
        WorldGenType::Normal => generate_normal_chunk(terrain, chunk_pos),
        WorldGenType::Flat => generate_flat_chunk(chunk_pos),
    }
}

/// Generates a flat world chunk (2 chunks = 64 blocks high).
/// Layers from top to bottom: grass (1), dirt (7), stone (55), bedrock (1)
fn generate_flat_chunk(chunk_pos: Vector3<i32>) -> ChunkGenerationResult {
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
        return ChunkGenerationResult {
            chunk,
            overflow_blocks: Vec::new(),
        };
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

    // Explicitly do NOT generate trees, ground cover, or caves for flat worlds
    // generate_trees(...);
    // generate_ground_cover(...);
    // caves are part of generate_normal_chunk loop, not here.

    chunk.update_metadata();
    chunk.persistence_dirty = false;
    ChunkGenerationResult {
        chunk,
        overflow_blocks: Vec::new(),
    }
}

/// Generates normal terrain with biomes, caves, and trees.
fn generate_normal_chunk(
    terrain: &TerrainGenerator,
    chunk_pos: Vector3<i32>,
) -> ChunkGenerationResult {
    let mut chunk = Chunk::new();
    let mut overflow_blocks = Vec::new();
    let chunk_world_x = chunk_pos.x * CHUNK_SIZE as i32;
    let chunk_world_y = chunk_pos.y * CHUNK_SIZE as i32;
    let chunk_world_z = chunk_pos.z * CHUNK_SIZE as i32;

    // Generate terrain for this chunk
    for lx in 0..CHUNK_SIZE {
        for lz in 0..CHUNK_SIZE {
            let world_x = chunk_world_x + lx as i32;
            let world_z = chunk_world_z + lz as i32;
            let height = terrain.get_height(world_x, world_z);
            let biome = terrain.get_biome(world_x, world_z);

            for ly in 0..CHUNK_SIZE {
                let world_y = chunk_world_y + ly as i32;

                // Check if this is a cave first (before surface generation)
                let is_cave = terrain
                    .cave_generator
                    .is_cave(world_x, world_y, world_z, height, biome);

                let block_type = if world_y == 0 {
                    // Bedrock floor - unbreakable, prevents falling out of world
                    BlockType::Bedrock
                } else if world_y > height && world_y > SEA_LEVEL {
                    // Above terrain and above sea level = air
                    BlockType::Air
                } else if world_y > height && world_y <= SEA_LEVEL {
                    // Above terrain but below sea level = water
                    BlockType::Water
                } else if is_cave {
                    // Carved out cave - fill based on biome-specific rules
                    match terrain
                        .cave_generator
                        .get_cave_fill(biome, world_y, SEA_LEVEL)
                    {
                        CaveFillType::Air => BlockType::Air,
                        CaveFillType::Water(water_type) => {
                            // Set water with specific type for cave
                            chunk.set_water_block(lx, ly, lz, water_type);
                            continue; // Skip the default set_block below
                        }
                        CaveFillType::Lava => {
                            // Mountain caves at low depths have lava lakes
                            BlockType::Lava
                        }
                        CaveFillType::Ice => {
                            // Snow biome caves filled with ice
                            BlockType::Ice
                        }
                    }
                } else if world_y == height {
                    // Surface block - varies by biome
                    match biome {
                        BiomeType::Snow => BlockType::Snow,
                        BiomeType::Desert => BlockType::Sand,
                        BiomeType::Mountains => BlockType::Stone,
                        BiomeType::Swamp => {
                            // Swamps always have muddy surface
                            chunk.set_block(lx, ly, lz, BlockType::Mud);
                            continue; // Skip default set_block
                        }
                        BiomeType::Grassland => {
                            if world_y <= SEA_LEVEL + 2 {
                                BlockType::Sand // Beach
                            } else {
                                BlockType::Grass
                            }
                        }
                    }
                } else if world_y > height - 3 {
                    // Subsurface layer
                    match biome {
                        BiomeType::Desert => {
                            // Sandstone subsurface layer
                            chunk.set_block(lx, ly, lz, BlockType::Sandstone);
                            continue; // Skip default set_block
                        }
                        BiomeType::Mountains => BlockType::Stone,
                        BiomeType::Snow => BlockType::Stone,
                        _ => {
                            if height <= SEA_LEVEL + 2 {
                                BlockType::Sand // Beach substrate
                            } else {
                                BlockType::Dirt
                            }
                        }
                    }
                } else {
                    BlockType::Stone // Deep underground
                };

                if block_type == BlockType::Water {
                    chunk.set_water_block(lx, ly, lz, biome.water_type());
                } else {
                    chunk.set_block(lx, ly, lz, block_type);
                }
            }
        }
    }

    // Generate trees
    generate_trees(
        &mut chunk,
        terrain,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        &mut overflow_blocks,
    );

    // Generate ground cover
    generate_ground_cover(
        &mut chunk,
        terrain,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        &mut overflow_blocks,
    );

    // Generate cave decorations (stalactites/stalagmites)
    generate_cave_decorations(
        &mut chunk,
        terrain,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        &mut overflow_blocks,
    );

    chunk.update_metadata();
    // Procedurally generated chunk is not dirty for persistence until modified.
    chunk.persistence_dirty = false;
    ChunkGenerationResult {
        chunk,
        overflow_blocks,
    }
}

/// Generates ground cover (grass, flowers, etc.) based on biome
fn generate_ground_cover(
    chunk: &mut Chunk,
    terrain: &TerrainGenerator,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    _overflow_blocks: &mut Vec<OverflowBlock>,
) {
    for lx in 0..CHUNK_SIZE {
        for lz in 0..CHUNK_SIZE {
            let world_x = chunk_world_x + lx as i32;
            let world_z = chunk_world_z + lz as i32;
            let height = terrain.get_height(world_x, world_z);
            let biome = terrain.get_biome(world_x, world_z);
            let local_y = height - chunk_world_y;

            // Check if surface is in this chunk
            if local_y >= 0 && local_y < (CHUNK_SIZE as i32 - 1) {
                let y = (local_y + 1) as usize; // Block above surface
                let surface_block = chunk.get_block(lx, local_y as usize, lz);

                // Skip if already occupied (e.g. by tree trunk)
                if chunk.get_block(lx, y, lz) != BlockType::Air {
                    continue;
                }

                let hash = terrain.hash(world_x, world_z);

                match biome {
                    BiomeType::Grassland => {
                        if surface_block == BlockType::Grass {
                            if hash % 100 < 10 {
                                // 10% Tall Grass
                                chunk.set_model_block(lx, y, lz, MODEL_TALL_GRASS, 0, false);
                            } else if hash % 100 < 12 {
                                // 2% Flowers
                                let flower = if hash % 2 == 0 {
                                    MODEL_FLOWER_RED
                                } else {
                                    MODEL_FLOWER_YELLOW
                                };
                                chunk.set_model_block(lx, y, lz, flower, 0, false);
                            }
                        }
                    }
                    BiomeType::Swamp => {
                        if surface_block == BlockType::Grass || surface_block == BlockType::Dirt {
                            if hash % 100 < 15 {
                                chunk.set_model_block(lx, y, lz, MODEL_TALL_GRASS, 0, false);
                            } else if hash % 100 < 20 {
                                let mush = if hash % 2 == 0 {
                                    MODEL_MUSHROOM_BROWN
                                } else {
                                    MODEL_MUSHROOM_RED
                                };
                                chunk.set_model_block(lx, y, lz, mush, 0, false);
                            }
                        } else if surface_block == BlockType::Water {
                            // Lily pads on water (must be top water block)
                            // Check if block above is air (which it is, since we're at y=local_y+1)
                            if hash % 100 < 5 {
                                // Lily pad sits IN the water block space but renders on top?
                                // No, model blocks replace the block. If we replace water with lily pad, we lose water.
                                // Lily pad should be placed ABOVE water.
                                // Or use waterlogged model.
                                // Our system supports waterlogged models!
                                // Wait, if I place it at `y` (above water), it floats in air if water level is exactly at `local_y`.
                                // If water is at `local_y`, placing at `local_y` replaces water.
                                // I should place it at `local_y` and set waterlogged=true.
                                // But `local_y` is the height. If height <= SEA_LEVEL, it's water.
                                // So `surface_block` is water.
                                // I should place lily pad AT `local_y`.
                                chunk.set_model_block(
                                    lx,
                                    local_y as usize,
                                    lz,
                                    MODEL_LILY_PAD,
                                    (hash % 4) as u8,
                                    true,
                                );
                            }
                        }
                    }
                    BiomeType::Mountains => {
                        // Snow coverage at high altitudes (above Y=155)
                        let world_y = chunk_world_y + local_y;
                        if world_y > 155 && surface_block == BlockType::Stone {
                            // Replace stone surface with snow at high altitude
                            chunk.set_block(lx, local_y as usize, lz, BlockType::Snow);
                        } else if surface_block == BlockType::Grass && hash % 100 < 5 {
                            chunk.set_model_block(lx, y, lz, MODEL_TALL_GRASS, 0, false);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Post-process: Convert lava blocks adjacent to water into cobblestone
    convert_lava_water_contacts(chunk);
}

/// Converts lava blocks adjacent to water blocks into cobblestone.
/// This prevents lava from appearing next to water during terrain generation.
fn convert_lava_water_contacts(chunk: &mut Chunk) {
    let mut conversions = Vec::new();

    // Scan all blocks in chunk
    for lx in 0..CHUNK_SIZE {
        for ly in 0..CHUNK_SIZE {
            for lz in 0..CHUNK_SIZE {
                let block = chunk.get_block(lx, ly, lz);
                if block != BlockType::Lava {
                    continue;
                }

                // Check all 6 adjacent neighbors
                let neighbors = [
                    (lx.wrapping_sub(1), ly, lz),
                    (lx + 1, ly, lz),
                    (lx, ly.wrapping_sub(1), lz),
                    (lx, ly + 1, lz),
                    (lx, ly, lz.wrapping_sub(1)),
                    (lx, ly, lz + 1),
                ];

                for (nx, ny, nz) in neighbors {
                    // Check bounds
                    if nx >= CHUNK_SIZE || ny >= CHUNK_SIZE || nz >= CHUNK_SIZE {
                        continue;
                    }

                    let neighbor = chunk.get_block(nx, ny, nz);
                    if neighbor == BlockType::Water {
                        // Lava touching water - convert to cobblestone
                        conversions.push((lx, ly, lz));
                        break;
                    }
                }
            }
        }
    }

    // Apply conversions
    for (lx, ly, lz) in conversions {
        chunk.set_block(lx, ly, lz, BlockType::Cobblestone);
    }
}

/// Generates trees based on biome
fn generate_trees(
    chunk: &mut Chunk,
    terrain: &TerrainGenerator,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    // Trees can now span chunks freely with overflow support
    // No boundary guards - trees can spawn at chunk edges and overflow into neighbors
    for lx in (0..CHUNK_SIZE).step_by(4) {
        for lz in (0..CHUNK_SIZE).step_by(4) {
            let world_x = chunk_world_x + lx as i32;
            let world_z = chunk_world_z + lz as i32;
            let height = terrain.get_height(world_x, world_z);
            // Use primary biome for tree type selection to avoid blending issues
            // Swamps blend heavily with grasslands, causing inconsistent tree spawning
            let biome_info = terrain.get_biome_info(world_x, world_z);
            let biome = biome_info.biome; // Primary biome, not blended
            let local_base_y = height - chunk_world_y;

            // Check if tree base is in this chunk
            // Buffer at top ensures enough of tree is in this chunk for proper generation
            // (overflow handles canopy extending into next chunk)
            if local_base_y < 0 || local_base_y >= (CHUNK_SIZE as i32 - 5) {
                continue;
            }

            // Don't spawn trees underwater (only prevent if below sea level)
            if height < SEA_LEVEL {
                continue;
            }

            // Check terrain slope - prevent trees on steep terrain
            let height_n = terrain.get_height(world_x, world_z + 4);
            let height_s = terrain.get_height(world_x, world_z - 4);
            let height_e = terrain.get_height(world_x + 4, world_z);
            let height_w = terrain.get_height(world_x - 4, world_z);
            let max_height_diff = (height_n - height)
                .abs()
                .max((height_s - height).abs())
                .max((height_e - height).abs())
                .max((height_w - height).abs());

            // Skip if terrain is too steep (more than 3 blocks difference in 4 block radius)
            if max_height_diff > 3 {
                continue;
            }

            // Randomness
            let hash = terrain.hash(world_x, world_z);

            match biome {
                BiomeType::Grassland => {
                    // Oak trees (Standard) - Moderate density
                    if hash % 100 < 5 {
                        generate_oak(
                            chunk,
                            lx as i32,
                            local_base_y,
                            lz as i32,
                            hash,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    }
                }
                BiomeType::Mountains => {
                    // Pine trees - Low density, only below snow line
                    if height < 80 && hash % 100 < 3 {
                        generate_pine(
                            chunk,
                            lx as i32,
                            local_base_y,
                            lz as i32,
                            hash,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    }
                }
                BiomeType::Snow => {
                    // Dead trees (logs only, no leaves) - Very low density
                    if hash % 100 < 2 {
                        generate_dead_tree(
                            chunk,
                            lx as i32,
                            local_base_y,
                            lz as i32,
                            hash,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    }
                }
                BiomeType::Swamp => {
                    // Willow/Swamp trees - Moderate density
                    if hash % 100 < 12 {
                        generate_willow(
                            chunk,
                            lx as i32,
                            local_base_y,
                            lz as i32,
                            hash,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    }
                }
                BiomeType::Desert => {
                    // Cactus - Sparse
                    if hash % 100 < 2 {
                        generate_cactus(
                            chunk,
                            lx as i32,
                            local_base_y,
                            lz as i32,
                            hash,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    }
                }
            }
        }
    }
}

fn generate_oak(
    chunk: &mut Chunk,
    x: i32,
    y: i32,
    z: i32,
    hash: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    // Check if this should be a giant multi-deck tree (rare: ~10% chance)
    let is_giant = (hash % 10) == 0;

    if is_giant {
        generate_giant_oak(
            chunk,
            x,
            y,
            z,
            hash,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    } else {
        generate_normal_oak(
            chunk,
            x,
            y,
            z,
            hash,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }
}

fn generate_normal_oak(
    chunk: &mut Chunk,
    x: i32,
    y: i32,
    z: i32,
    hash: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    // Check if there's solid ground below (no floating trees over caves)
    // Check surface and 2 blocks below to ensure stable foundation
    for check_y in (y.saturating_sub(2))..=y {
        if let Some(block) = get_block_safe(chunk, x, check_y, z) {
            if !block.is_solid() {
                return; // Don't place tree on air/water or above cave
            }
        } else {
            return; // Don't place tree if we can't check
        }
    }

    // More variation: height 4-9, with different canopy sizes
    let height = 4 + (hash % 6);
    let canopy_size = (hash / 7) % 3; // 0=small, 1=medium, 2=large
    let trunk_offset = (hash / 13) % 2; // 0=normal, 1=extra trunk before canopy
    let canopy_shape = (hash / 17) % 4; // 0=tapered, 1=cube, 2=blob, 3=round

    // Canopy layers - varies by size
    let layers = match canopy_size {
        0 => 3, // Small: 3 layers
        1 => 4, // Medium: 4 layers
        _ => 5, // Large: 5 layers
    };

    // Canopy placement - ensure top layer is above trunk
    let canopy_base = if height <= 5 {
        y + height - 2 // Short trees - canopy starts 2 below trunk top
    } else {
        y + height - 2 - trunk_offset // Taller trees can have longer trunk
    };

    // Trunk should stop at canopy_base to ensure it's fully covered by leaves
    // The canopy generation will skip center column up to trunk_top_y
    let trunk_top = canopy_base;

    // Trunk - extends to canopy base
    for dy in 1..trunk_top {
        set_block_safe(
            chunk,
            x,
            y + dy,
            z,
            BlockType::Log,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }

    // Add 1-2 branches for taller trees with large canopies
    if height >= 7 && canopy_size == 2 && (hash % 3) == 0 {
        let branch_y = trunk_top - 3;
        let num_branches = 1 + ((hash / 43) % 2); // 1-2 branches

        for branch_idx in 0..num_branches {
            let branch_dir = (hash / (47 + branch_idx * 11)) % 4;
            let branch_len = 2 + ((hash / (53 + branch_idx * 7)) % 2); // 2-3 blocks

            let (dx, dz) = match branch_dir {
                0 => (1, 0),
                1 => (-1, 0),
                2 => (0, 1),
                _ => (0, -1),
            };

            // Place horizontal branch
            for i in 1..=branch_len {
                set_block_safe(
                    chunk,
                    x + dx * i,
                    branch_y,
                    z + dz * i,
                    BlockType::Log,
                    chunk_world_x,
                    chunk_world_y,
                    chunk_world_z,
                    overflow_blocks,
                );
            }

            // Small canopy at branch tip
            let tip_x = x + dx * branch_len;
            let tip_z = z + dz * branch_len;
            let branch_shape = (hash / (59 + branch_idx * 13)) % 4;
            generate_oak_canopy(
                chunk,
                tip_x,
                branch_y,
                tip_z,
                0,
                3,
                branch_y,
                branch_shape,
                chunk_world_x,
                chunk_world_y,
                chunk_world_z,
                overflow_blocks,
            );
        }
    }

    generate_oak_canopy(
        chunk,
        x,
        canopy_base,
        z,
        canopy_size,
        layers,
        trunk_top - 1,
        canopy_shape,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        overflow_blocks,
    );
}

fn generate_giant_oak(
    chunk: &mut Chunk,
    x: i32,
    y: i32,
    z: i32,
    hash: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    // Check if there's solid ground below (no floating trees over caves)
    // Check surface and 2 blocks below to ensure stable foundation
    for check_y in (y.saturating_sub(2))..=y {
        if let Some(block) = get_block_safe(chunk, x, check_y, z) {
            if !block.is_solid() {
                return; // Don't place tree on air/water or above cave
            }
        } else {
            return; // Don't place tree if we can't check
        }
    }

    // Giant trees: 2-3 decks, each deck separated by trunk blocks
    let num_decks = 2 + ((hash / 19) % 2); // 2 or 3 decks

    // First, calculate all deck positions and total height
    let mut deck_positions = Vec::new();
    let mut current_y = y;

    for deck_idx in 0..num_decks {
        let trunk_section = 3 + ((hash / (23 + deck_idx)) % 3); // 3-5 blocks (reduced from 5-8)
        current_y += trunk_section;

        // Randomize canopy sizes with bias toward larger decks lower down
        let canopy_size = if deck_idx == 0 {
            // Bottom deck: 2-3 (large to huge)
            2 + ((hash / (29 + deck_idx)) % 2)
        } else if deck_idx == num_decks - 1 {
            // Top deck: 1-2 (medium to large)
            1 + ((hash / (31 + deck_idx)) % 2)
        } else {
            // Middle deck: 1-3 (medium to huge)
            1 + ((hash / (37 + deck_idx)) % 3)
        };

        let layers = match canopy_size {
            3 => 6, // Huge: 6 layers
            2 => 5, // Large: 5 layers
            _ => 4, // Medium: 4 layers
        };

        deck_positions.push((current_y, canopy_size, layers, trunk_section, deck_idx));
        current_y += layers;
    }

    // Find the highest canopy base to determine where trunk should stop
    let highest_canopy_base = deck_positions
        .iter()
        .map(|(canopy_y, _, _, _, _)| *canopy_y)
        .max()
        .unwrap_or(y);

    // Trunk should stop at the base of the highest canopy
    let trunk_top = highest_canopy_base;

    // Build continuous trunk - stops at highest canopy base so leaves can cover it
    for dy in 1..trunk_top {
        set_block_safe(
            chunk,
            x,
            y + dy,
            z,
            BlockType::Log,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }

    // Place canopies at each deck position
    for &(canopy_y, canopy_size, layers, trunk_section, deck_idx) in &deck_positions {
        let deck_shape = (hash / (71 + deck_idx)) % 4;
        generate_oak_canopy(
            chunk,
            x,
            canopy_y,
            z,
            canopy_size,
            layers,
            trunk_top - 1,
            deck_shape,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );

        // Add branches from trunk section (not on the top deck)
        if deck_idx < num_decks - 1 {
            // More branches on lower decks
            let max_branches = if deck_idx == 0 { 4 } else { 3 };
            let num_branches = 2 + ((hash / (31 + deck_idx)) % max_branches);

            // Track vertical support positions for cross-bracing
            let mut vertical_supports: Vec<(i32, i32, i32, i32)> = Vec::new(); // (x, y, z, height)

            for branch_idx in 0..num_branches {
                let branch_dir = (hash / (37 + branch_idx * 7)) % 4;

                // Extra long horizontal branches: bottom=3-7, upper=1-5
                let min_len = if deck_idx == 0 { 3 } else { 1 };
                let max_len = if deck_idx == 0 { 7 } else { 5 };
                let branch_len =
                    min_len + ((hash / (41 + branch_idx * 5)) % (max_len - min_len + 1));

                // Vary branch height within trunk section
                let height_offset =
                    ((hash / (59 + branch_idx * 3)) % trunk_section.max(3)).min(trunk_section - 1);
                let branch_y = canopy_y - trunk_section + height_offset;

                let (dx, dz) = match branch_dir {
                    0 => (1, 0),
                    1 => (-1, 0),
                    2 => (0, 1),
                    _ => (0, -1),
                };

                // Place horizontal branch
                for i in 1..=branch_len {
                    set_block_safe(
                        chunk,
                        x + dx * i,
                        branch_y,
                        z + dz * i,
                        BlockType::Log,
                        chunk_world_x,
                        chunk_world_y,
                        chunk_world_z,
                        overflow_blocks,
                    );
                }

                let tip_x = x + dx * branch_len;
                let tip_z = z + dz * branch_len;

                // Add vertical extension upward from branch tip (70% chance)
                let has_vertical = (hash / (67 + branch_idx * 11)) % 10 < 7;
                if has_vertical {
                    // Vertical support column extends upward 4-10 blocks
                    let vertical_height = 4 + ((hash / (79 + branch_idx * 13)) % 7);
                    for vy in 1..=vertical_height {
                        set_block_safe(
                            chunk,
                            tip_x,
                            branch_y + vy,
                            tip_z,
                            BlockType::Log,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    }

                    // Track this vertical support for cross-bracing
                    vertical_supports.push((tip_x, branch_y, tip_z, vertical_height));

                    // Add small canopy at top of vertical support
                    let vert_top_y = branch_y + vertical_height;
                    let branch_canopy_size = 1 + ((hash / (89 + branch_idx * 7)) % 2); // 1-2
                    let branch_layers = 3 + ((hash / (97 + branch_idx * 5)) % 2); // 3-4
                    let branch_shape = (hash / (73 + branch_idx * 17)) % 4;
                    generate_oak_canopy(
                        chunk,
                        tip_x,
                        vert_top_y,
                        tip_z,
                        branch_canopy_size,
                        branch_layers,
                        vert_top_y,
                        branch_shape,
                        chunk_world_x,
                        chunk_world_y,
                        chunk_world_z,
                        overflow_blocks,
                    );
                } else {
                    // No vertical - just canopy at horizontal branch tip
                    let branch_canopy_size = (hash / (67 + branch_idx * 11)) % 3;
                    let branch_layers = match branch_canopy_size {
                        0 => 3,
                        1 => 4,
                        _ => 5,
                    };
                    let branch_shape = (hash / (73 + branch_idx * 17)) % 4;
                    generate_oak_canopy(
                        chunk,
                        tip_x,
                        branch_y,
                        tip_z,
                        branch_canopy_size,
                        branch_layers,
                        branch_y,
                        branch_shape,
                        chunk_world_x,
                        chunk_world_y,
                        chunk_world_z,
                        overflow_blocks,
                    );
                }
            }

            // Add horizontal cross-bracing between nearby vertical supports
            for i in 0..vertical_supports.len() {
                for j in (i + 1)..vertical_supports.len() {
                    let (x1, y1, z1, h1) = vertical_supports[i];
                    let (x2, y2, z2, h2) = vertical_supports[j];

                    // Check if supports are aligned (same X or same Z) and close enough
                    let dx = (x2 - x1).abs();
                    let dz = (z2 - z1).abs();

                    // Connect if aligned on one axis and within reasonable distance
                    if (dx == 0 && dz > 0 && dz <= 12) || (dz == 0 && dx > 0 && dx <= 12) {
                        // Pick a height somewhere in the middle third of the shorter support
                        let min_height = h1.min(h2);
                        let brace_height_offset = min_height / 3
                            + ((hash / (101 + i as i32 * 7)) % (min_height / 3).max(1));
                        let brace_y = y1.max(y2) + brace_height_offset;

                        // Place horizontal log connection
                        if dx == 0 {
                            // Same X, connect along Z
                            let z_start = z1.min(z2);
                            let z_end = z1.max(z2);
                            for bz in z_start..=z_end {
                                set_block_safe(
                                    chunk,
                                    x1,
                                    brace_y,
                                    bz,
                                    BlockType::Log,
                                    chunk_world_x,
                                    chunk_world_y,
                                    chunk_world_z,
                                    overflow_blocks,
                                );
                            }
                        } else {
                            // Same Z, connect along X
                            let x_start = x1.min(x2);
                            let x_end = x1.max(x2);
                            for bx in x_start..=x_end {
                                set_block_safe(
                                    chunk,
                                    bx,
                                    brace_y,
                                    z1,
                                    BlockType::Log,
                                    chunk_world_x,
                                    chunk_world_y,
                                    chunk_world_z,
                                    overflow_blocks,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn generate_oak_canopy(
    chunk: &mut Chunk,
    x: i32,
    base_y: i32,
    z: i32,
    canopy_size: i32,
    layers: i32,
    trunk_top_y: i32,
    shape: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    // Get max radius for this canopy size
    let max_radius = match canopy_size {
        0 => 2.5, // Small
        1 => 3.0, // Medium
        2 => 4.0, // Large
        _ => 5.0, // Huge
    };

    let height = layers as f32;

    // Use spherical/ellipsoid shape instead of stacked discs
    for dx in -(max_radius as i32)..=(max_radius as i32) {
        for dz in -(max_radius as i32)..=(max_radius as i32) {
            for dy in 0..layers {
                let dist_xz_squared = (dx * dx + dz * dz) as f32;

                // Normalize position within canopy (0.0 at bottom, 1.0 at top)
                let y_norm = dy as f32 / height;

                // Create ellipsoid shape: wider in middle, narrower at top/bottom
                // Adjust radius based on height position
                let radius_at_height = match shape {
                    0 => {
                        // Round/spherical - widest in middle
                        let t = y_norm - 0.5;
                        max_radius * (1.0 - 4.0 * t * t).max(0.3)
                    }
                    1 => {
                        // Cylinder - consistent width
                        max_radius * (1.0 - 0.3 * y_norm.abs())
                    }
                    2 => {
                        // Cone - wider at bottom
                        max_radius * (1.0 - 0.7 * y_norm)
                    }
                    _ => {
                        // Inverted cone - wider at top
                        max_radius * (0.4 + 0.6 * y_norm)
                    }
                };

                let r_squared_at_height = radius_at_height * radius_at_height;

                // Use distance check for organic shape
                if dist_xz_squared <= r_squared_at_height {
                    // Add some random gaps for natural look (10% chance)
                    let hash =
                        ((x + dx) * 73856093) ^ ((base_y + dy) * 19349663) ^ ((z + dz) * 83492791);
                    if (hash.abs() % 10) == 0 {
                        continue; // Skip this block for variation
                    }

                    let ly = base_y + dy;
                    // Don't replace trunk
                    if dx == 0 && dz == 0 && ly <= trunk_top_y {
                        continue;
                    }
                    set_block_safe(
                        chunk,
                        x + dx,
                        ly,
                        z + dz,
                        BlockType::Leaves,
                        chunk_world_x,
                        chunk_world_y,
                        chunk_world_z,
                        overflow_blocks,
                    );
                }
            }
        }
    }
}

fn generate_pine(
    chunk: &mut Chunk,
    x: i32,
    y: i32,
    z: i32,
    hash: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    // Check if this should be a giant multi-deck tree (rare: ~10% chance)
    let is_giant = (hash % 10) == 0;

    if is_giant {
        generate_giant_pine(
            chunk,
            x,
            y,
            z,
            hash,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    } else {
        generate_normal_pine(
            chunk,
            x,
            y,
            z,
            hash,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }
}

fn generate_normal_pine(
    chunk: &mut Chunk,
    x: i32,
    y: i32,
    z: i32,
    hash: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    // Check if there's solid ground below (no floating trees over caves)
    // Check surface and 2 blocks below to ensure stable foundation
    for check_y in (y.saturating_sub(2))..=y {
        if let Some(block) = get_block_safe(chunk, x, check_y, z) {
            if !block.is_solid() {
                return; // Don't place tree on air/water or above cave
            }
        } else {
            return; // Don't place tree if we can't check
        }
    }

    // More variation: height 6-13 blocks
    let height = 6 + (hash % 8);
    let cone_width = (hash / 11) % 3; // 0=narrow, 1=medium, 2=wide

    // Pine trees start foliage low (about 1/3 up the trunk)
    let start_leaves = 2 + (height / 4);

    // Calculate cone height
    let cone_height = height - start_leaves + 2; // Extends above trunk

    // Calculate max_radius based on cone_height for proper taper
    let max_radius = match cone_width {
        0 => (cone_height / 3).max(2),       // Narrow: height/3, min 2
        1 => ((cone_height * 2) / 5).max(2), // Medium: height/2.5, min 2
        _ => (cone_height / 2).max(3),       // Wide: height/2, min 3
    };

    // Trunk extends to full height
    for dy in 1..height {
        set_block_safe(
            chunk,
            x,
            y + dy,
            z,
            BlockType::PineLog,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }
    generate_pine_cone(
        chunk,
        x,
        y + start_leaves,
        z,
        max_radius,
        cone_height,
        y + height - 1,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        overflow_blocks,
    );
}

fn generate_giant_pine(
    chunk: &mut Chunk,
    x: i32,
    y: i32,
    z: i32,
    hash: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    // Check if there's solid ground below (no floating trees over caves)
    // Check surface and 2 blocks below to ensure stable foundation
    for check_y in (y.saturating_sub(2))..=y {
        if let Some(block) = get_block_safe(chunk, x, check_y, z) {
            if !block.is_solid() {
                return; // Don't place tree on air/water or above cave
            }
        } else {
            return; // Don't place tree if we can't check
        }
    }

    // Giant pines: single large cone, much taller
    let height = 15 + ((hash / 19) % 8); // 15-22 blocks tall
    let cone_width = (hash / 23) % 2; // 0=wide, 1=very wide

    // Start foliage about 1/4 up
    let start_leaves = 4 + (height / 6);

    // Calculate cone height
    let cone_height = height - start_leaves + 2;

    // Calculate max_radius based on cone_height for proper taper
    let max_radius = if cone_width == 0 {
        ((cone_height * 2) / 5).max(3) // Wide: height/2.5, min 3
    } else {
        (cone_height / 2).max(4) // Very wide: height/2, min 4
    };

    // Build trunk to full height
    for dy in 1..height {
        set_block_safe(
            chunk,
            x,
            y + dy,
            z,
            BlockType::PineLog,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }
    generate_pine_cone(
        chunk,
        x,
        y + start_leaves,
        z,
        max_radius,
        cone_height,
        y + height - 1,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        overflow_blocks,
    );
}

fn generate_pine_cone(
    chunk: &mut Chunk,
    x: i32,
    base_y: i32,
    z: i32,
    max_radius: i32,
    cone_height: i32,
    trunk_top_y: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    for dy in 0..cone_height {
        // Calculate radius with linear taper for classic pine shape
        let radius: i32 = {
            let t = 1.0 - (dy as f32 / cone_height as f32);
            // Simple linear taper from bottom to top
            let calculated = (t * max_radius as f32) as i32;

            // Keep minimum radius of 1 except at very top
            if calculated == 0 && dy < cone_height - 1 {
                1
            } else {
                calculated
            }
        };

        // Place circular layers
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                let dist_sq = dx * dx + dz * dz;
                let radius_sq = radius * radius;

                // Circular cross-section
                if dist_sq > radius_sq {
                    continue;
                }

                let ly = base_y + dy;
                // Don't replace trunk
                if dx == 0 && dz == 0 && ly <= trunk_top_y {
                    continue;
                }
                set_block_safe(
                    chunk,
                    x + dx,
                    ly,
                    z + dz,
                    BlockType::PineLeaves,
                    chunk_world_x,
                    chunk_world_y,
                    chunk_world_z,
                    overflow_blocks,
                );
            }
        }
    }

    // Place tip leaf above trunk to ensure it's covered
    set_block_safe(
        chunk,
        x,
        trunk_top_y + 1,
        z,
        BlockType::PineLeaves,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        overflow_blocks,
    );
}

/// Generates a dead tree (logs only, no leaves) with snow coverage for snow biome
fn generate_dead_tree(
    chunk: &mut Chunk,
    x: i32,
    y: i32,
    z: i32,
    hash: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    // Check if there's solid ground below
    for check_y in (y.saturating_sub(2))..=y {
        if let Some(block) = get_block_safe(chunk, x, check_y, z) {
            if !block.is_solid() {
                return; // Don't place tree on air/water or above cave
            }
        } else {
            return;
        }
    }

    // Dead trees are shorter (4-8 blocks)
    let height = 4 + (hash % 5);

    // Just the trunk, no branches or leaves
    for dy in 1..height {
        set_block_safe(
            chunk,
            x,
            y + dy,
            z,
            BlockType::PineLog,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }

    // Add snow on top and around the trunk
    // Snow cap on top
    set_block_safe(
        chunk,
        x,
        y + height,
        z,
        BlockType::Snow,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
        overflow_blocks,
    );

    // Optional: Add snow around base for drifts (50% chance)
    if hash % 2 == 0 {
        for dx in -1..=1 {
            for dz in -1..=1 {
                if dx == 0 && dz == 0 {
                    continue; // Skip center (trunk is there)
                }
                // Place snow around base
                set_block_safe(
                    chunk,
                    x + dx,
                    y + 1,
                    z + dz,
                    BlockType::Snow,
                    chunk_world_x,
                    chunk_world_y,
                    chunk_world_z,
                    overflow_blocks,
                );
            }
        }
    }

    // Add some snow partway up the trunk (30% chance per level)
    for dy in 2..height {
        if (hash.wrapping_add(dy) % 10) < 3 {
            // Random side for snow accumulation
            let side = (hash.wrapping_add(dy * 7)) % 4;
            let (dx, dz) = match side {
                0 => (1, 0),
                1 => (-1, 0),
                2 => (0, 1),
                _ => (0, -1),
            };
            set_block_safe(
                chunk,
                x + dx,
                y + dy,
                z + dz,
                BlockType::Snow,
                chunk_world_x,
                chunk_world_y,
                chunk_world_z,
                overflow_blocks,
            );
        }
    }
}

fn generate_willow(
    chunk: &mut Chunk,
    x: i32,
    y: i32,
    z: i32,
    hash: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    let height = 4 + (hash % 3);

    // Trunk
    for dy in 1..=height {
        set_block_safe(
            chunk,
            x,
            y + dy,
            z,
            BlockType::WillowLog,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }

    // Rounded canopy - dome shape with multiple layers
    let canopy_y = y + height;

    // Bottom layer - widest (radius 3)
    for dx in -3i32..=3 {
        for dz in -3i32..=3 {
            let dist_sq = dx * dx + dz * dz;
            if dist_sq <= 9 {
                // Euclidean distance <= 3
                set_block_safe(
                    chunk,
                    x + dx,
                    canopy_y,
                    z + dz,
                    BlockType::WillowLeaves,
                    chunk_world_x,
                    chunk_world_y,
                    chunk_world_z,
                    overflow_blocks,
                );

                // Hanging vines from outer edge
                let dist = (dist_sq as f32).sqrt();
                if dist > 1.5 && (hash.wrapping_add(dx * 30 + dz) % 3 == 0) {
                    let vine_len = 1 + (hash % 3);
                    for v in 1..=vine_len {
                        set_block_safe(
                            chunk,
                            x + dx,
                            canopy_y - v,
                            z + dz,
                            BlockType::WillowLeaves,
                            chunk_world_x,
                            chunk_world_y,
                            chunk_world_z,
                            overflow_blocks,
                        );
                    }
                }
            }
        }
    }

    // Middle layer - medium (radius 2.5)
    for dx in -2i32..=2 {
        for dz in -2i32..=2 {
            let dist_sq = dx * dx + dz * dz;
            if dist_sq <= 6 {
                // Euclidean distance <= ~2.5
                set_block_safe(
                    chunk,
                    x + dx,
                    canopy_y + 1,
                    z + dz,
                    BlockType::WillowLeaves,
                    chunk_world_x,
                    chunk_world_y,
                    chunk_world_z,
                    overflow_blocks,
                );
            }
        }
    }

    // Top layer - smallest (radius 1.5)
    for dx in -1i32..=1 {
        for dz in -1i32..=1 {
            let dist_sq = dx * dx + dz * dz;
            if dist_sq <= 2 {
                // Euclidean distance <= ~1.5
                set_block_safe(
                    chunk,
                    x + dx,
                    canopy_y + 2,
                    z + dz,
                    BlockType::WillowLeaves,
                    chunk_world_x,
                    chunk_world_y,
                    chunk_world_z,
                    overflow_blocks,
                );
            }
        }
    }
}

fn generate_cactus(
    chunk: &mut Chunk,
    x: i32,
    y: i32,
    z: i32,
    hash: i32,
    _chunk_world_x: i32,
    _chunk_world_y: i32,
    _chunk_world_z: i32,
    _overflow_blocks: &mut Vec<OverflowBlock>,
) {
    let height = 3 + (hash % 3);

    // Main column (trunk)
    for dy in 1..=height {
        let block_y = y + dy;
        if x >= 0
            && x < CHUNK_SIZE as i32
            && block_y >= 0
            && block_y < CHUNK_SIZE as i32
            && z >= 0
            && z < CHUNK_SIZE as i32
            && chunk
                .get_block(x as usize, block_y as usize, z as usize)
                .is_transparent()
        {
            chunk.set_block(x as usize, block_y as usize, z as usize, BlockType::Cactus);
        }
    }

    // Add branches for taller cacti (height >= 4)
    if height >= 4 {
        // Determine branch direction and height based on hash
        let branch_dir = hash % 4; // 0=N, 1=S, 2=E, 3=W
        let branch_height = y + 2 + (hash % 2); // Branch starts 2-3 blocks up

        // Branch offsets for each direction
        let (dx, dz) = match branch_dir {
            0 => (0, -1), // North
            1 => (0, 1),  // South
            2 => (1, 0),  // East
            _ => (-1, 0), // West
        };

        // Place branch (1-2 blocks long)
        let branch_len = 1 + ((hash / 7) % 2);
        for i in 1..=branch_len {
            let branch_x = x + dx * i;
            let branch_z = z + dz * i;
            if branch_x >= 0
                && branch_x < CHUNK_SIZE as i32
                && branch_height >= 0
                && branch_height < CHUNK_SIZE as i32
                && branch_z >= 0
                && branch_z < CHUNK_SIZE as i32
                && chunk
                    .get_block(branch_x as usize, branch_height as usize, branch_z as usize)
                    .is_transparent()
            {
                chunk.set_block(
                    branch_x as usize,
                    branch_height as usize,
                    branch_z as usize,
                    BlockType::Cactus,
                );
            }
        }

        // Add vertical growth on branch tip (0-1 blocks)
        if (hash / 13) % 2 == 0 {
            let tip_x = x + dx * branch_len;
            let tip_y = branch_height + 1;
            let tip_z = z + dz * branch_len;
            if tip_x >= 0
                && tip_x < CHUNK_SIZE as i32
                && tip_y >= 0
                && tip_y < CHUNK_SIZE as i32
                && tip_z >= 0
                && tip_z < CHUNK_SIZE as i32
                && chunk
                    .get_block(tip_x as usize, tip_y as usize, tip_z as usize)
                    .is_transparent()
            {
                chunk.set_block(
                    tip_x as usize,
                    tip_y as usize,
                    tip_z as usize,
                    BlockType::Cactus,
                );
            }
        }

        // Optionally add a second branch on the opposite side for very tall cacti
        if height >= 5 && (hash / 11) % 2 == 0 {
            let branch2_height = branch_height + 1;
            let (dx2, dz2) = match (branch_dir + 2) % 4 {
                0 => (0, -1), // North
                1 => (0, 1),  // South
                2 => (1, 0),  // East
                _ => (-1, 0), // West
            };

            let branch2_x = x + dx2;
            let branch2_z = z + dz2;
            if branch2_x >= 0
                && branch2_x < CHUNK_SIZE as i32
                && branch2_height >= 0
                && branch2_height < CHUNK_SIZE as i32
                && branch2_z >= 0
                && branch2_z < CHUNK_SIZE as i32
                && chunk
                    .get_block(
                        branch2_x as usize,
                        branch2_height as usize,
                        branch2_z as usize,
                    )
                    .is_transparent()
            {
                chunk.set_block(
                    branch2_x as usize,
                    branch2_height as usize,
                    branch2_z as usize,
                    BlockType::Cactus,
                );
            }
        }
    }
}
// Helper to set blocks safely within chunk bounds
fn get_block_safe(chunk: &Chunk, x: i32, y: i32, z: i32) -> Option<BlockType> {
    if x >= 0
        && x < CHUNK_SIZE as i32
        && y >= 0
        && y < CHUNK_SIZE as i32
        && z >= 0
        && z < CHUNK_SIZE as i32
    {
        Some(chunk.get_block(x as usize, y as usize, z as usize))
    } else {
        None
    }
}

fn set_block_safe(
    chunk: &mut Chunk,
    x: i32,
    y: i32,
    z: i32,
    block: BlockType,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    if x >= 0
        && x < CHUNK_SIZE as i32
        && y >= 0
        && y < CHUNK_SIZE as i32
        && z >= 0
        && z < CHUNK_SIZE as i32
    {
        // Within chunk bounds - place directly
        let existing = chunk.get_block(x as usize, y as usize, z as usize);
        // Tree structure (logs/leaves) can replace surface terrain for proper generation
        let can_replace = existing == BlockType::Air
            || existing.is_transparent()
            || (block.is_tree_structure() && existing.is_replaceable_terrain());
        if can_replace {
            chunk.set_block(x as usize, y as usize, z as usize, block);
        }
    } else {
        // Out of bounds - add to overflow for neighboring chunk
        let world_pos = Vector3::new(chunk_world_x + x, chunk_world_y + y, chunk_world_z + z);
        overflow_blocks.push(OverflowBlock {
            world_pos,
            block_type: block,
        });
    }
}

/// Generates cave decorations (stalactites/stalagmites) in underground caves.
fn generate_cave_decorations(
    chunk: &mut Chunk,
    terrain: &TerrainGenerator,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    _overflow_blocks: &mut Vec<OverflowBlock>,
) {
    // Cave decorations can now span chunks with overflow support
    if chunk_world_y > SEA_LEVEL {
        return; // Above sea level, unlikely to have deep caves
    }

    for lx in 0..CHUNK_SIZE {
        for lz in 0..CHUNK_SIZE {
            let world_x = chunk_world_x + lx as i32;
            let world_z = chunk_world_z + lz as i32;
            let biome = terrain.get_biome(world_x, world_z);

            for ly in 0..CHUNK_SIZE {
                let world_y = chunk_world_y + ly as i32;
                let block = chunk.get_block(lx, ly, lz);

                // Check for cave ceiling (solid block with air below)
                if block == BlockType::Stone && ly > 0 {
                    let below = chunk.get_block(lx, ly - 1, lz);
                    if below == BlockType::Air {
                        // This is a cave ceiling
                        if let Some(model_id) = terrain
                            .cave_generator
                            .should_place_stalactite(world_x, world_y, world_z, biome)
                        {
                            // Place stalactite in the air block below the ceiling
                            chunk.set_model_block(lx, ly - 1, lz, model_id, 0, false);
                        }
                    }
                }

                // Check for cave floor (solid block with air above)
                if block == BlockType::Stone && ly < CHUNK_SIZE - 1 {
                    let above = chunk.get_block(lx, ly + 1, lz);
                    if above == BlockType::Air {
                        // This is a cave floor
                        if let Some(model_id) = terrain
                            .cave_generator
                            .should_place_stalagmite(world_x, world_y, world_z, biome)
                        {
                            // Place stalagmite in the air block above the floor
                            chunk.set_model_block(lx, ly + 1, lz, model_id, 0, false);
                        }
                    }
                }
            }
        }
    }
}

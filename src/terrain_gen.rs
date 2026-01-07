use crate::cave_gen::{CaveFillType, CaveGenerator};
use crate::chunk::{BlockType, CHUNK_SIZE, Chunk, WaterType};
use crate::config::WorldGenType;
use nalgebra::Vector3;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin, RidgedMulti};

// Terrain generation constants
/// Sea level for water filling (blocks below this in valleys become water)
pub const SEA_LEVEL: i32 = 28;

// Model IDs for ground cover
const MODEL_TALL_GRASS: u8 = 100;
const MODEL_FLOWER_RED: u8 = 101;
const MODEL_FLOWER_YELLOW: u8 = 102;
const MODEL_LILY_PAD: u8 = 103;
const MODEL_MUSHROOM_BROWN: u8 = 104;
const MODEL_MUSHROOM_RED: u8 = 105;

// Texture indices (from common.glsl/materials.glsl)
const TEX_CACTUS: u8 = 23;
const TEX_MUD: u8 = 24;
const TEX_SANDSTONE: u8 = 25;

// Tint indices (from chunk.rs TINT_PALETTE)
const TINT_WHITE: u8 = 12;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BiomeInfo {
    pub elevation: f64,
    pub temperature: f64,
    pub rainfall: f64,
    pub biome: BiomeType,
}

#[derive(Clone, Copy, Debug, PartialEq)]
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

        // Cave generation system
        let cave_generator = CaveGenerator::new(seed);

        Self {
            height_noise,
            detail_noise,
            mountain_noise,
            temperature_noise,
            rainfall_noise,
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

        let biome = if adjusted_temp < 0.3 {
            BiomeType::Snow
        } else if adjusted_temp > 0.7 && rain < 0.3 {
            BiomeType::Desert
        } else if adjusted_temp > 0.6 && rain > 0.7 {
            BiomeType::Swamp
        } else if base_height > 0.6 {
            BiomeType::Mountains
        } else {
            BiomeType::Grassland
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

    /// Get terrain height at world coordinates
    pub fn get_height(&self, world_x: i32, world_z: i32) -> i32 {
        let x = world_x as f64;
        let z = world_z as f64;

        // Base continental terrain (large smooth features)
        let base = self.height_noise.get([x, z]);

        // Mountain ridges (sharp peaks)
        let ridges = self.mountain_noise.get([x, z]);

        // Detail noise for subtle variation
        let detail = self.detail_noise.get([x * 0.02, z * 0.02]);

        let biome = self.get_biome(world_x, world_z);

        let height = match biome {
            BiomeType::Grassland => 32.0 + detail * 2.0 + base * 4.0,
            BiomeType::Mountains => 32.0 + base * 10.0 + ridges * 55.0,
            BiomeType::Desert => 32.0 + detail * 1.0 + base * 2.0, // Flatter
            BiomeType::Swamp => 28.0 + detail * 1.0,               // Low, near sea level (28)
            BiomeType::Snow => {
                // High peaks or flat tundra depending on base height
                if base > 0.5 {
                    32.0 + base * 8.0 + ridges * 40.0 // Snowy peaks
                } else {
                    32.0 + detail * 2.0 // Tundra
                }
            }
        };

        height.round() as i32
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

    // Explicitly do NOT generate trees, ground cover, or caves for flat worlds
    // generate_trees(...);
    // generate_ground_cover(...);
    // caves are part of generate_normal_chunk loop, not here.

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
                    }
                } else if world_y == height {
                    // Surface block - varies by biome
                    match biome {
                        BiomeType::Snow => BlockType::Snow,
                        BiomeType::Desert => BlockType::Sand,
                        BiomeType::Mountains => BlockType::Stone,
                        BiomeType::Swamp => {
                            if world_y <= SEA_LEVEL + 1 {
                                // Muddy look (Painted block with Mud texture)
                                chunk.set_painted_block(lx, ly, lz, TEX_MUD, TINT_WHITE);
                                continue; // Skip default set_block
                            } else {
                                BlockType::Grass
                            }
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
                            // Sandstone (Painted block)
                            chunk.set_painted_block(lx, ly, lz, TEX_SANDSTONE, TINT_WHITE);
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
    );

    // Generate ground cover
    generate_ground_cover(
        &mut chunk,
        terrain,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
    );

    // Generate cave decorations (stalactites/stalagmites)
    generate_cave_decorations(
        &mut chunk,
        terrain,
        chunk_world_x,
        chunk_world_y,
        chunk_world_z,
    );

    chunk.update_metadata();
    // Procedurally generated chunk is not dirty for persistence until modified.
    chunk.persistence_dirty = false;
    chunk
}

/// Generates ground cover (grass, flowers, etc.) based on biome
fn generate_ground_cover(
    chunk: &mut Chunk,
    terrain: &TerrainGenerator,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
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
                        if surface_block == BlockType::Grass && hash % 100 < 5 {
                            chunk.set_model_block(lx, y, lz, MODEL_TALL_GRASS, 0, false);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Generates trees based on biome
fn generate_trees(
    chunk: &mut Chunk,
    terrain: &TerrainGenerator,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
) {
    // Sparse grid for trees - larger buffer to prevent chunk boundary clipping
    // Buffer of 6 blocks accounts for max tree radius (4) + branches (up to 5)
    for lx in (6..CHUNK_SIZE - 6).step_by(4) {
        for lz in (6..CHUNK_SIZE - 6).step_by(4) {
            let world_x = chunk_world_x + lx as i32;
            let world_z = chunk_world_z + lz as i32;
            let height = terrain.get_height(world_x, world_z);
            let biome = terrain.get_biome(world_x, world_z);
            let local_base_y = height - chunk_world_y;

            // Check if tree base is in this chunk (with some buffer for leaves)
            if local_base_y < 0 || local_base_y >= (CHUNK_SIZE as i32 - 10) {
                continue;
            }

            // Randomness
            let hash = terrain.hash(world_x, world_z);

            match biome {
                BiomeType::Grassland => {
                    // Oak trees (Standard) - Moderate density
                    if hash % 100 < 5 {
                        generate_oak(chunk, lx as i32, local_base_y, lz as i32, hash);
                    }
                }
                BiomeType::Mountains => {
                    // Pine trees - Low density, only below snow line
                    if height < 80 && hash % 100 < 3 {
                        generate_pine(chunk, lx as i32, local_base_y, lz as i32, hash);
                    }
                }
                BiomeType::Snow => {
                    // Pine trees - Very low density
                    if hash % 100 < 2 {
                        generate_pine(chunk, lx as i32, local_base_y, lz as i32, hash);
                    }
                }
                BiomeType::Swamp => {
                    // Willow/Swamp trees - Moderate density
                    if hash % 100 < 8 {
                        generate_willow(chunk, lx as i32, local_base_y, lz as i32, hash);
                    }
                }
                BiomeType::Desert => {
                    // Cactus - Sparse
                    if hash % 100 < 2 {
                        generate_cactus(chunk, lx as i32, local_base_y, lz as i32, hash);
                    }
                }
            }
        }
    }
}

fn generate_oak(chunk: &mut Chunk, x: i32, y: i32, z: i32, hash: i32) {
    // Check if this should be a giant multi-deck tree (rare: ~10% chance)
    let is_giant = (hash % 10) == 0;

    if is_giant {
        generate_giant_oak(chunk, x, y, z, hash);
    } else {
        generate_normal_oak(chunk, x, y, z, hash);
    }
}

fn generate_normal_oak(chunk: &mut Chunk, x: i32, y: i32, z: i32, hash: i32) {
    // Check if there's solid ground below (no floating trees over caves)
    if let Some(block_below) = get_block_safe(chunk, x, y, z) {
        if !block_below.is_solid() {
            return; // Don't place tree on air/water
        }
    } else {
        return; // Don't place tree if we can't check
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

    // Ensure canopy top is at least 1 above trunk
    let canopy_top = canopy_base + layers - 1;
    let tree_top = if canopy_top < y + height {
        canopy_top + 1 // Extend tree to ensure leaves cover trunk
    } else {
        y + height
    };

    // Trunk - stops 1 block before top so leaves cover it
    for dy in 1..tree_top {
        set_block_safe(chunk, x, y + dy, z, BlockType::Log);
    }

    // Add 1-2 branches for taller trees with large canopies
    if height >= 7 && canopy_size == 2 && (hash % 3) == 0 {
        let branch_y = tree_top - 3;
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
                set_block_safe(chunk, x + dx * i, branch_y, z + dz * i, BlockType::Log);
            }

            // Small canopy at branch tip
            let tip_x = x + dx * branch_len;
            let tip_z = z + dz * branch_len;
            let branch_shape = (hash / (59 + branch_idx * 13)) % 4;
            generate_oak_canopy(chunk, tip_x, branch_y, tip_z, 0, 3, branch_y, branch_shape);
        }
    }

    generate_oak_canopy(
        chunk,
        x,
        canopy_base,
        z,
        canopy_size,
        layers,
        tree_top - 1,
        canopy_shape,
    );
}

fn generate_giant_oak(chunk: &mut Chunk, x: i32, y: i32, z: i32, hash: i32) {
    // Check if there's solid ground below (no floating trees over caves)
    if let Some(block_below) = get_block_safe(chunk, x, y, z) {
        if !block_below.is_solid() {
            return; // Don't place tree on air/water
        }
    } else {
        return; // Don't place tree if we can't check
    }

    // Giant trees: 2-3 decks, each deck separated by trunk blocks
    let num_decks = 2 + ((hash / 19) % 2); // 2 or 3 decks

    // First, calculate all deck positions and total height
    let mut deck_positions = Vec::new();
    let mut current_y = y;

    for deck_idx in 0..num_decks {
        let trunk_section = 5 + ((hash / (23 + deck_idx)) % 4); // 5-8 blocks
        current_y += trunk_section;

        // Scale canopy size: bottom deck largest, scales down as we go up
        // For 2-deck: bottom=3 (huge), top=1 (medium)
        // For 3-deck: bottom=3 (huge), middle=2 (large), top=1 (medium)
        let canopy_size = if deck_idx == 0 {
            3 // Bottom deck is huge
        } else if deck_idx == num_decks - 1 {
            1 // Top deck is medium
        } else {
            2 // Middle deck is large
        };

        let layers = match canopy_size {
            3 => 6, // Huge: 6 layers
            2 => 5, // Large: 5 layers
            _ => 4, // Medium: 4 layers
        };

        deck_positions.push((current_y, canopy_size, layers, trunk_section, deck_idx));
        current_y += layers;
    }

    let total_height = current_y - y;

    // Build continuous trunk - stops 1 block before top so leaves cover it
    for dy in 1..total_height {
        set_block_safe(chunk, x, y + dy, z, BlockType::Log);
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
            total_height + y - 1,
            deck_shape,
        );

        // Add branches from trunk section (not on the top deck)
        if deck_idx < num_decks - 1 && trunk_section >= 5 {
            // More branches on lower decks
            let max_branches = if deck_idx == 0 { 3 } else { 2 };
            let num_branches = 1 + ((hash / (31 + deck_idx)) % max_branches);

            for branch_idx in 0..num_branches {
                let branch_dir = (hash / (37 + branch_idx * 7)) % 4;

                // Longer branches on lower decks: bottom=3-5, upper=2-4
                let min_len = if deck_idx == 0 { 3 } else { 2 };
                let max_len = if deck_idx == 0 { 5 } else { 4 };
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
                    set_block_safe(chunk, x + dx * i, branch_y, z + dz * i, BlockType::Log);
                }

                // Branch canopy size varies: 0=small, 1=medium, 2=large
                let branch_canopy_size = (hash / (67 + branch_idx * 11)) % 3;
                let branch_layers = match branch_canopy_size {
                    0 => 3,
                    1 => 4,
                    _ => 5,
                };

                let tip_x = x + dx * branch_len;
                let tip_z = z + dz * branch_len;
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
                );
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
) {
    for dy in 0..layers {
        // Get base radius for this canopy size
        let base_radius = match canopy_size {
            0 => 2, // Small
            1 => 2, // Medium
            2 => 3, // Large
            _ => 4, // Huge
        };

        // Apply shape variation to radius pattern
        let radius: i32 = match shape {
            0 => {
                // Tapered: classic shape
                if canopy_size <= 1 {
                    if dy == 0 || dy == layers - 1 { 1 } else { 2 }
                } else if canopy_size == 2 {
                    if dy == 0 || dy == layers - 1 {
                        1
                    } else if dy == 2 {
                        3
                    } else {
                        2
                    }
                } else {
                    // Huge
                    if dy == layers - 1 {
                        1
                    } else if dy == 0 || dy == layers - 2 {
                        2
                    } else if dy == 2 {
                        4
                    } else {
                        3
                    }
                }
            }
            1 => {
                // Cube: consistent radius
                if dy == 0 || dy == layers - 1 {
                    base_radius.max(1) - 1 // Slightly smaller top/bottom
                } else {
                    base_radius
                }
            }
            2 => {
                // Blob: bulges in middle
                if dy == 0 || dy == layers - 1 {
                    base_radius.max(1) - 1
                } else if dy == layers / 2 {
                    base_radius + 1 // Extra wide in middle
                } else {
                    base_radius
                }
            }
            _ => {
                // Round: smooth taper
                if dy == 0 || dy == layers - 1 {
                    1
                } else if dy < layers / 2 {
                    (dy + 1).min(base_radius)
                } else {
                    (layers - dy).min(base_radius)
                }
            }
        };

        for dx in -radius..=radius {
            for dz in -radius..=radius {
                // Skip corners for rounded look
                if dx.abs() == radius && dz.abs() == radius && radius > 1 {
                    continue;
                }
                let ly = base_y + dy;
                // Don't replace trunk
                if dx == 0 && dz == 0 && ly <= trunk_top_y {
                    continue;
                }
                set_block_safe(chunk, x + dx, ly, z + dz, BlockType::Leaves);
            }
        }
    }
}

fn generate_pine(chunk: &mut Chunk, x: i32, y: i32, z: i32, hash: i32) {
    // Check if this should be a giant multi-deck tree (rare: ~10% chance)
    let is_giant = (hash % 10) == 0;

    if is_giant {
        generate_giant_pine(chunk, x, y, z, hash);
    } else {
        generate_normal_pine(chunk, x, y, z, hash);
    }
}

fn generate_normal_pine(chunk: &mut Chunk, x: i32, y: i32, z: i32, hash: i32) {
    // Check if there's solid ground below (no floating trees over caves)
    if let Some(block_below) = get_block_safe(chunk, x, y, z) {
        if !block_below.is_solid() {
            return; // Don't place tree on air/water
        }
    } else {
        return; // Don't place tree if we can't check
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
        set_block_safe(chunk, x, y + dy, z, BlockType::PineLog);
    }
    generate_pine_cone(
        chunk,
        x,
        y + start_leaves,
        z,
        max_radius,
        cone_height,
        y + height - 1,
    );
}

fn generate_giant_pine(chunk: &mut Chunk, x: i32, y: i32, z: i32, hash: i32) {
    // Check if there's solid ground below (no floating trees over caves)
    if let Some(block_below) = get_block_safe(chunk, x, y, z) {
        if !block_below.is_solid() {
            return; // Don't place tree on air/water
        }
    } else {
        return; // Don't place tree if we can't check
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
        set_block_safe(chunk, x, y + dy, z, BlockType::PineLog);
    }
    generate_pine_cone(
        chunk,
        x,
        y + start_leaves,
        z,
        max_radius,
        cone_height,
        y + height - 1,
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
                set_block_safe(chunk, x + dx, ly, z + dz, BlockType::PineLeaves);
            }
        }
    }

    // Place tip leaf above trunk to ensure it's covered
    set_block_safe(chunk, x, trunk_top_y + 1, z, BlockType::PineLeaves);
}

fn generate_willow(chunk: &mut Chunk, x: i32, y: i32, z: i32, hash: i32) {
    let height = 4 + (hash % 3);

    // Trunk
    for dy in 1..=height {
        set_block_safe(chunk, x, y + dy, z, BlockType::WillowLog);
    }

    // Wide canopy
    let canopy_y = y + height;
    for dx in -3i32..=3 {
        for dz in -3i32..=3 {
            let dist = dx.abs() + dz.abs();
            if dist <= 3 {
                set_block_safe(chunk, x + dx, canopy_y, z + dz, BlockType::WillowLeaves);
                set_block_safe(chunk, x + dx, canopy_y + 1, z + dz, BlockType::WillowLeaves);

                // Hanging vines
                if dist > 1 && (hash.wrapping_add(dx * 30 + dz) % 3 == 0) {
                    let vine_len = 1 + (hash % 3);
                    for v in 1..=vine_len {
                        set_block_safe(
                            chunk,
                            x + dx,
                            canopy_y - v,
                            z + dz,
                            BlockType::WillowLeaves,
                        );
                    }
                }
            }
        }
    }
}

fn generate_cactus(chunk: &mut Chunk, x: i32, y: i32, z: i32, hash: i32) {
    let height = 3 + (hash % 3);

    // Main column (trunk)
    for dy in 1..=height {
        set_painted_block_safe(chunk, x, y + dy, z, TEX_CACTUS, TINT_WHITE);
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
            set_painted_block_safe(
                chunk,
                x + dx * i,
                branch_height,
                z + dz * i,
                TEX_CACTUS,
                TINT_WHITE,
            );
        }

        // Add vertical growth on branch tip (0-1 blocks)
        if (hash / 13) % 2 == 0 {
            set_painted_block_safe(
                chunk,
                x + dx * branch_len,
                branch_height + 1,
                z + dz * branch_len,
                TEX_CACTUS,
                TINT_WHITE,
            );
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

            set_painted_block_safe(
                chunk,
                x + dx2,
                branch2_height,
                z + dz2,
                TEX_CACTUS,
                TINT_WHITE,
            );
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

fn set_block_safe(chunk: &mut Chunk, x: i32, y: i32, z: i32, block: BlockType) {
    if x >= 0
        && x < CHUNK_SIZE as i32
        && y >= 0
        && y < CHUNK_SIZE as i32
        && z >= 0
        && z < CHUNK_SIZE as i32
        && (chunk.get_block(x as usize, y as usize, z as usize) == BlockType::Air
            || chunk
                .get_block(x as usize, y as usize, z as usize)
                .is_transparent())
    {
        chunk.set_block(x as usize, y as usize, z as usize, block);
    }
}

// Helper for painted blocks
fn set_painted_block_safe(chunk: &mut Chunk, x: i32, y: i32, z: i32, tex: u8, tint: u8) {
    if x >= 0
        && x < CHUNK_SIZE as i32
        && y >= 0
        && y < CHUNK_SIZE as i32
        && z >= 0
        && z < CHUNK_SIZE as i32
        && (chunk.get_block(x as usize, y as usize, z as usize) == BlockType::Air
            || chunk
                .get_block(x as usize, y as usize, z as usize)
                .is_transparent())
    {
        chunk.set_painted_block(x as usize, y as usize, z as usize, tex, tint);
    }
}

/// Generates cave decorations (stalactites/stalagmites) in underground caves.
fn generate_cave_decorations(
    chunk: &mut Chunk,
    terrain: &TerrainGenerator,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
) {
    // Only check chunks that are underground (potential for caves)
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

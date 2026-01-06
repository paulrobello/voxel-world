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
const TEX_LOG: u8 = 10;
const TEX_LEAVES: u8 = 5;
const TEX_CACTUS: u8 = 23;
const TEX_MUD: u8 = 24;
const TEX_SANDSTONE: u8 = 25;

// Tint indices (from chunk.rs TINT_PALETTE)
const TINT_WHITE: u8 = 12;
const TINT_BROWN: u8 = 15;
const TINT_DARK_BROWN: u8 = 28;
const TINT_DARK_GREEN: u8 = 29;
const TINT_OLIVE: u8 = 19;

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

        // Temperature noise - large scale variation
        let temperature_noise = Perlin::new(seed.wrapping_add(6));
        // Rainfall noise - large scale variation
        let rainfall_noise = Perlin::new(seed.wrapping_add(7));

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
            temperature_noise,
            rainfall_noise,
            cave_noise,
            cave_mask_noise,
            entrance_noise,
        }
    }

    /// Get biome type at world coordinates
    pub fn get_biome(&self, world_x: i32, world_z: i32) -> BiomeType {
        let x = world_x as f64;
        let z = world_z as f64;

        // Base noise values (-1.0 to 1.0)
        let temp_raw = self.temperature_noise.get([x * 0.002, z * 0.002]);
        let rain_raw = self.rainfall_noise.get([x * 0.002, z * 0.002]);

        // Normalize to 0.0 to 1.0
        let temp = temp_raw * 0.5 + 0.5;
        let rain = rain_raw * 0.5 + 0.5;

        // Get approximate height for temperature lapse rate
        // We use a simplified height lookup to avoid recursion (since get_height calls this in future)
        // For now, assume sea level for biome distribution, modify later if needed.
        // Or better: Use height noise directly here without full detail.
        let base_height = self.height_noise.get([x, z]); // -1 to 1

        // Adjust temperature by elevation (higher = colder)
        // base_height of 1.0 (mountain) reduces temp by 0.4
        let elevation_cooling = base_height.max(0.0) * 0.4;
        let adjusted_temp = (temp - elevation_cooling).clamp(0.0, 1.0);

        if adjusted_temp < 0.3 {
            BiomeType::Snow
        } else if adjusted_temp > 0.7 && rain < 0.3 {
            BiomeType::Desert
        } else if adjusted_temp > 0.6 && rain > 0.7 {
            BiomeType::Swamp
        } else if base_height > 0.6 {
            BiomeType::Mountains
        } else {
            BiomeType::Grassland
        }
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
            let biome = terrain.get_biome(world_x, world_z);

            for ly in 0..CHUNK_SIZE {
                let world_y = chunk_world_y + ly as i32;

                let block_type = if world_y == 0 {
                    // Bedrock floor - unbreakable, prevents falling out of world
                    BlockType::Bedrock
                } else if world_y > height && world_y > SEA_LEVEL {
                    // Above terrain and above sea level = air
                    BlockType::Air
                } else if world_y > height && world_y <= SEA_LEVEL {
                    // Above terrain but below sea level = water
                    BlockType::Water
                } else if terrain.is_cave(world_x, world_y, world_z, height) {
                    // Carved out cave - fill with water if below sea level
                    if world_y <= SEA_LEVEL {
                        BlockType::Water
                    } else {
                        BlockType::Air
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
    // Sparse grid for trees
    for lx in (2..CHUNK_SIZE - 2).step_by(4) {
        for lz in (2..CHUNK_SIZE - 2).step_by(4) {
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
    let height = 5 + (hash % 3);

    // Trunk
    for dy in 1..=height {
        set_block_safe(chunk, x, y + dy, z, BlockType::Log);
    }

    // Leaves (Blob)
    let canopy_base = y + height - 2;
    for dy in 0..4 {
        let radius: i32 = if dy == 0 || dy == 3 { 1 } else { 2 };
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                // Skip corners for rounded look
                if dx.abs() == radius && dz.abs() == radius && radius > 1 {
                    continue;
                }
                let ly = canopy_base + dy;
                // Don't replace trunk
                if dx == 0 && dz == 0 && ly <= y + height {
                    continue;
                }
                set_block_safe(chunk, x + dx, ly, z + dz, BlockType::Leaves);
            }
        }
    }
}

fn generate_pine(chunk: &mut Chunk, x: i32, y: i32, z: i32, hash: i32) {
    let height = 6 + (hash % 4);

    // Trunk (Darker)
    for dy in 1..=height {
        set_painted_block_safe(chunk, x, y + dy, z, TEX_LOG, TINT_DARK_BROWN);
    }

    // Leaves (Cone layers)
    let start_leaves = 3;
    for dy in start_leaves..=height + 1 {
        let h_idx = dy - start_leaves; // 0 at bottom of leaves
        // Radius decreases as we go up: 2, 2, 1, 1, 0
        let radius: i32 = if dy > height {
            0
        } else if h_idx < 2 {
            2
        } else {
            1
        };

        for dx in -radius..=radius {
            for dz in -radius..=radius {
                if dx.abs() == radius && dz.abs() == radius && radius > 0 {
                    continue; // Skip corners
                }
                // Don't replace trunk
                if dx == 0 && dz == 0 && dy <= height {
                    continue;
                }
                set_painted_block_safe(chunk, x + dx, y + dy, z + dz, TEX_LEAVES, TINT_DARK_GREEN);
            }
        }
    }
    // Top tip
    set_painted_block_safe(chunk, x, y + height + 2, z, TEX_LEAVES, TINT_DARK_GREEN);
}

fn generate_willow(chunk: &mut Chunk, x: i32, y: i32, z: i32, hash: i32) {
    let height = 4 + (hash % 3);

    // Trunk (Brown)
    for dy in 1..=height {
        set_painted_block_safe(chunk, x, y + dy, z, TEX_LOG, TINT_BROWN);
    }

    // Wide canopy
    let canopy_y = y + height;
    for dx in -3i32..=3 {
        for dz in -3i32..=3 {
            let dist = dx.abs() + dz.abs();
            if dist <= 3 {
                set_painted_block_safe(chunk, x + dx, canopy_y, z + dz, TEX_LEAVES, TINT_OLIVE);
                set_painted_block_safe(chunk, x + dx, canopy_y + 1, z + dz, TEX_LEAVES, TINT_OLIVE);

                // Hanging vines
                if dist > 1 && (hash.wrapping_add(dx * 30 + dz) % 3 == 0) {
                    let vine_len = 1 + (hash % 3);
                    for v in 1..=vine_len {
                        set_painted_block_safe(
                            chunk,
                            x + dx,
                            canopy_y - v,
                            z + dz,
                            TEX_LEAVES,
                            TINT_OLIVE,
                        );
                    }
                }
            }
        }
    }
}

fn generate_cactus(chunk: &mut Chunk, x: i32, y: i32, z: i32, hash: i32) {
    let height = 2 + (hash % 3);

    // Main column
    for dy in 1..=height {
        // Use Cactus texture
        set_painted_block_safe(chunk, x, y + dy, z, TEX_CACTUS, TINT_WHITE);
    }
}
// Helper to set blocks safely within chunk bounds
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

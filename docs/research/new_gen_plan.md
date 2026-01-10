# Voxel World Generation Upgrade Plan

This document outlines the changes needed to upgrade voxel_world's terrain, biome, and cave generation to be more like Minecraft 1.18+, while keeping the current 512-block height range.

---

## Executive Summary

### Current State vs Target

| Feature | Current | Target (MC-Style) |
|---------|---------|-------------------|
| **Biomes** | 5 (hard thresholds) | 15+ (multinoise) |
| **Climate Params** | 2 (temp, rain) | 5 (temp, humidity, cont., erosion, weird.) |
| **Cave Types** | 1 (spaghetti-like) | 4 (cheese, spaghetti, noodle, carved) |
| **Rivers** | None (water type only) | Carved river channels |
| **3D Biomes** | No | Yes (underground biomes) |
| **Height Range** | 512 blocks | 512 blocks (keep) |
| **Sea Level** | Y=75 | Y=75 (keep) |

### Scope of Changes

1. **Generation Code** - Major rewrite of terrain_gen.rs, cave_gen.rs
2. **New Block Types** - 8-12 new blocks for biome variety
3. **Shader Updates** - Texture atlas expansion, new block constants
4. **Fluid System** - River source generation, carving integration
5. **No Core Engine Changes** - Chunk system, rendering pipeline unchanged

---

## Part 1: New Biome System

### Proposed Biome List (15 Biomes)

#### Surface Biomes (12)

| ID | Biome | Temperature | Humidity | Erosion | Surface | Trees |
|----|-------|-------------|----------|---------|---------|-------|
| 0 | Ocean | Any | Any | Any | Water | None |
| 1 | Beach | Mid | Any | High | Sand | None |
| 2 | Plains | Mid | Low-Mid | High | Grass | Sparse oak |
| 3 | Forest | Mid | Mid | Mid | Grass | Dense oak/birch |
| 4 | Dark Forest | Mid | High | Mid | Grass | Dense dark oak |
| 5 | Birch Forest | Mid | Mid | Mid | Grass | Birch only |
| 6 | Taiga | Cold | Mid | Mid | Grass+Snow | Spruce |
| 7 | Snowy Plains | Cold | Low | High | Snow | Sparse |
| 8 | Snowy Taiga | Cold | Mid-High | Mid | Snow | Dense spruce |
| 9 | Desert | Hot | Very Low | High | Sand | Cactus |
| 10 | Savanna | Hot | Low | Mid-High | Grass | Acacia |
| 11 | Swamp | Mid-Warm | High | Very High | Mud | Willow |
| 12 | Mountains | Any | Any | Very Low | Stone | Alpine |
| 13 | Meadow | Mid | Mid | Low | Grass+Flowers | Sparse |
| 14 | Jungle | Hot | High | Mid | Grass | Dense jungle |

#### Underground Biomes (3)

| ID | Biome | Depth | Humidity | Features |
|----|-------|-------|----------|----------|
| 15 | Lush Caves | Mid | High | Glow berries, moss, azalea |
| 16 | Dripstone Caves | Mid | Mid | Stalactites, stalagmites |
| 17 | Deep Dark | Deep (Y<32) | Any | Sculk, darkness |

### Climate Parameter System

Replace 2-parameter system with 5-parameter multinoise:

```
┌─────────────────────────────────────────────────────────────┐
│                    MULTINOISE SYSTEM                         │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Temperature (3D fBm)                                       │
│    └─► Hot/Cold biome selection                             │
│                                                             │
│  Humidity (3D fBm)                                          │
│    └─► Wet/Dry biome selection                              │
│                                                             │
│  Continentalness (2D fBm)                                   │
│    └─► Ocean ◄─────────► Inland                             │
│    └─► Controls base terrain height                         │
│                                                             │
│  Erosion (2D fBm)                                           │
│    └─► Flat ◄──────────► Mountainous                        │
│    └─► Controls height variation amplitude                  │
│                                                             │
│  Weirdness (2D fBm)                                         │
│    └─► Normal ◄────────► Variant biomes                     │
│    └─► Enables rare biome variants                          │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Biome Selection Algorithm

```rust
struct ClimatePoint {
    temperature: f64,      // -1.0 to 1.0
    humidity: f64,         // -1.0 to 1.0
    continentalness: f64,  // -1.0 (ocean) to 1.0 (inland)
    erosion: f64,          // -1.0 (peaks) to 1.0 (flat)
    weirdness: f64,        // -1.0 to 1.0 (variant selector)
}

// Each biome defines ideal climate ranges
struct BiomeClimate {
    temp_min: f64, temp_max: f64,
    humidity_min: f64, humidity_max: f64,
    continentalness_min: f64, continentalness_max: f64,
    erosion_min: f64, erosion_max: f64,
    weirdness_threshold: Option<f64>,  // For variants
}

fn select_biome(point: ClimatePoint) -> BiomeType {
    // Find biome with smallest "distance" in climate space
    BIOMES.iter()
        .min_by_key(|b| climate_distance(point, b.climate))
        .unwrap()
}
```

### Underground Biome Selection

```rust
fn get_biome_3d(x: i32, y: i32, z: i32, surface_height: i32) -> BiomeType {
    let depth_below_surface = surface_height - y;

    if depth_below_surface < 16 {
        // Near surface: use surface biome
        return get_surface_biome(x, z);
    }

    // Sample 3D climate for underground
    let humidity_3d = humidity_noise.get([x, y, z]);

    if y < 32 && is_deep_dark_region(x, z) {
        return BiomeType::DeepDark;
    } else if humidity_3d > 0.5 && has_lush_cave_conditions(x, y, z) {
        return BiomeType::LushCaves;
    } else if is_dripstone_region(x, y, z) {
        return BiomeType::DripstoneCaves;
    }

    // Default: inherit surface biome characteristics
    get_surface_biome(x, z)
}
```

---

## Part 2: Enhanced Cave System

### Four Cave Types

#### 1. Cheese Caves (Large Caverns)

```
Method: 3D Perlin noise with high threshold
Frequency: 0.02 (very large features)
Threshold: noise > 0.65 (carve where noise is high)
Result: Swiss-cheese caverns with irregular shapes
```

```rust
fn is_cheese_cave(x: i32, y: i32, z: i32) -> bool {
    let noise = cheese_noise.get([
        x as f64 * 0.02,
        y as f64 * 0.015,  // Stretched vertically
        z as f64 * 0.02
    ]);
    noise > 0.65
}
```

#### 2. Spaghetti Caves (Long Tunnels)

```
Method: Dual 3D Perlin noise intersection
Frequency: 0.05 (medium features)
Threshold: |noise1| < 0.03 AND |noise2| < 0.03
Result: Long, winding tubular passages
```

```rust
fn is_spaghetti_cave(x: i32, y: i32, z: i32) -> bool {
    let n1 = spaghetti_noise1.get([x * 0.05, y * 0.07, z * 0.05]);
    let n2 = spaghetti_noise2.get([x * 0.05 + 1000.0, y * 0.07, z * 0.05 + 1000.0]);
    n1.abs() < 0.03 && n2.abs() < 0.03
}
```

#### 3. Noodle Caves (Fine Network)

```
Method: Same as spaghetti but higher frequency, tighter threshold
Frequency: 0.1 (fine features)
Threshold: |noise1| < 0.015 AND |noise2| < 0.015
Result: Dense web of narrow passages
```

```rust
fn is_noodle_cave(x: i32, y: i32, z: i32) -> bool {
    let n1 = noodle_noise1.get([x * 0.1, y * 0.12, z * 0.1]);
    let n2 = noodle_noise2.get([x * 0.1 + 500.0, y * 0.12, z * 0.1 + 500.0]);
    n1.abs() < 0.015 && n2.abs() < 0.015
}
```

#### 4. Carved Caves (Perlin Worms)

```
Method: Procedural worm paths with varying radius
Steps: Generate starting points, simulate worm movement
Result: Traditional carved tunnels and ravines
```

```rust
struct CaveWorm {
    position: Vec3,
    direction: Vec3,
    radius: f32,
    remaining_length: i32,
}

fn generate_carved_caves(chunk_x: i32, chunk_z: i32) -> Vec<CarvedSegment> {
    let mut segments = Vec::new();

    // Deterministic worm starts based on chunk position
    for worm in get_worm_starts(chunk_x, chunk_z) {
        let mut pos = worm.start;
        let mut dir = worm.initial_direction;

        for _ in 0..worm.length {
            // Carve sphere at current position
            segments.push(CarvedSegment { pos, radius: worm.radius });

            // Update direction using noise gradient
            dir = bend_direction(dir, pos);
            pos += dir * step_size;
        }
    }

    segments
}
```

### Combined Cave Generation

```rust
fn is_cave(x: i32, y: i32, z: i32, surface_height: i32) -> bool {
    // Surface protection
    if y > surface_height - 8 { return false; }

    // Bedrock protection
    if y <= 1 { return false; }

    // Check all cave types (OR together)
    is_cheese_cave(x, y, z) ||
    is_spaghetti_cave(x, y, z) ||
    is_noodle_cave(x, y, z) ||
    is_in_carved_cave(x, y, z)
}
```

### Cave Biome Features

| Cave Biome | Floor | Ceiling | Decorations |
|------------|-------|---------|-------------|
| **Standard** | Stone | Stone | Stalactites (15%) |
| **Lush Caves** | Moss | Glow lichen | Azalea, dripleaf, glow berries |
| **Dripstone** | Stone | Dripstone | Dense stalactites/stalagmites |
| **Deep Dark** | Sculk | Deepslate | Sculk sensors, shriekers |

---

## Part 3: River System

### River Generation Algorithm

Rivers are generated as carved channels connecting high elevation to ocean.

#### Phase 1: River Path Generation

```rust
fn generate_river_paths(region_x: i32, region_z: i32) -> Vec<RiverPath> {
    let mut rivers = Vec::new();

    // Find river sources (high elevation, high rainfall)
    let sources = find_river_sources(region_x, region_z);

    for source in sources {
        let path = trace_river_path(source);
        rivers.push(path);
    }

    rivers
}

fn trace_river_path(start: Vec2) -> RiverPath {
    let mut path = vec![start];
    let mut pos = start;

    loop {
        // Find steepest downhill direction
        let next = find_lowest_neighbor(pos);

        if next.height <= SEA_LEVEL {
            // Reached ocean
            break;
        }

        // Add noise to prevent perfectly straight rivers
        let offset = river_noise.get([pos.x * 0.01, pos.z * 0.01]) * 4.0;
        pos = next + offset;
        path.push(pos);
    }

    RiverPath { points: path, width: calculate_width(&path) }
}
```

#### Phase 2: River Carving

```rust
fn carve_river(path: &RiverPath, terrain: &mut TerrainGenerator) {
    for (i, point) in path.points.iter().enumerate() {
        // Width increases toward mouth
        let width = 2.0 + (i as f32 / path.points.len() as f32) * 4.0;

        // Depth based on width
        let depth = (width * 0.5).min(6.0);

        // Carve channel
        for dx in -width as i32..=width as i32 {
            for dz in -width as i32..=width as i32 {
                let dist = ((dx*dx + dz*dz) as f32).sqrt();
                if dist <= width {
                    let x = point.x + dx;
                    let z = point.z + dz;

                    // Lower terrain by depth (smoothed at edges)
                    let edge_factor = 1.0 - (dist / width);
                    terrain.lower_height(x, z, depth * edge_factor);
                }
            }
        }
    }
}
```

#### Phase 3: Water Placement

```rust
fn fill_river(path: &RiverPath, world: &mut World) {
    for point in &path.points {
        let terrain_height = world.get_height(point.x, point.z);

        // Place water at carved level
        let water_level = terrain_height;

        for y in (terrain_height - 2)..=water_level {
            world.set_water_block(
                point.x, y, point.z,
                WaterType::River,
                1.0  // Full mass
            );
        }

        // Place river source at start
        if point == path.points.first() {
            world.place_water_source(point.x, water_level, point.z, WaterType::River);
        }
    }
}
```

### River Integration with Biomes

```rust
// Rivers force Beach biome along their banks
fn get_biome_with_rivers(x: i32, z: i32, river_distance: f32) -> BiomeType {
    if river_distance < 3.0 {
        return BiomeType::Beach;  // River bank
    }

    // Normal biome selection
    get_surface_biome(x, z)
}
```

---

## Part 4: New Block Types Required

### Blocks to Add (12 New Types)

| ID | Name | Texture | Purpose |
|----|------|---------|---------|
| 33 | Deepslate | deepslate_64x64.png | Deep underground stone |
| 34 | Moss | moss_64x64.png | Lush cave floors |
| 35 | MossyCobblestone | mossy_cobble_64x64.png | Lush cave decoration |
| 36 | Clay | clay_64x64.png | River beds, lakes |
| 37 | Dripstone | dripstone_64x64.png | Dripstone cave material |
| 38 | Calcite | calcite_64x64.png | Geode borders |
| 39 | Terracotta | terracotta_64x64.png | Badlands/mesa biome |
| 40 | PackedIce | packed_ice_64x64.png | Frozen ocean, glaciers |
| 41 | Podzol | podzol_64x64.png | Taiga forest floor |
| 42 | Mycelium | mycelium_64x64.png | Mushroom biome |
| 43 | CoarseDirt | coarse_dirt_64x64.png | Savanna, badlands |
| 44 | RootedDirt | rooted_dirt_64x64.png | Under azalea trees |

### Block Properties

```rust
// New blocks with properties
BlockType::Deepslate => {
    is_solid: true,
    is_transparent: false,
    break_time: 1.8,  // Harder than stone
    color: [0.3, 0.3, 0.35],
}

BlockType::Moss => {
    is_solid: true,
    is_transparent: false,
    break_time: 0.3,  // Soft
    color: [0.2, 0.5, 0.2],
}

BlockType::Clay => {
    is_solid: true,
    is_transparent: false,
    break_time: 0.6,
    color: [0.6, 0.6, 0.7],
}

// ... etc
```

### New Model Types

| Model ID | Name | Purpose |
|----------|------|---------|
| 110 | Glow Berries | Lush cave ceiling vines |
| 111 | Hanging Roots | Rooted dirt decoration |
| 112 | Small Dripleaf | Lush cave floor plant |
| 113 | Big Dripleaf | Lush cave platform |
| 114 | Azalea Bush | Lush cave shrub |
| 115 | Sculk Sensor | Deep dark detection |
| 116 | Sculk Shrieker | Deep dark alarm |
| 117 | Pointed Dripstone | Stalactite variant |

---

## Part 5: Engine Changes Required

### Changes Needed

#### 1. Shader Updates (Required)

**common.glsl** - Add block constants:
```glsl
const uint BLOCK_DEEPSLATE = 33;
const uint BLOCK_MOSS = 34;
const uint BLOCK_MOSSY_COBBLESTONE = 35;
const uint BLOCK_CLAY = 36;
const uint BLOCK_DRIPSTONE = 37;
const uint BLOCK_CALCITE = 38;
const uint BLOCK_TERRACOTTA = 39;
const uint BLOCK_PACKED_ICE = 40;
const uint BLOCK_PODZOL = 41;
const uint BLOCK_MYCELIUM = 42;
const uint BLOCK_COARSE_DIRT = 43;
const uint BLOCK_ROOTED_DIRT = 44;
```

**materials.glsl** - Update atlas:
```glsl
const float ATLAS_TILE_COUNT = 43.0;  // Was 31.0
```

**Add texture index mapping** for new blocks.

#### 2. Texture Atlas Expansion (Required)

Current: 31 textures (1984×64 pixels)
New: 43 textures (2752×64 pixels)

```bash
# Regenerate atlas with new textures
cd textures
magick [existing textures...] \
  deepslate_64x64.png moss_64x64.png mossy_cobble_64x64.png \
  clay_64x64.png dripstone_64x64.png calcite_64x64.png \
  terracotta_64x64.png packed_ice_64x64.png podzol_64x64.png \
  mycelium_64x64.png coarse_dirt_64x64.png rooted_dirt_64x64.png \
  +append texture_atlas.png
```

#### 3. Rust Code Updates (Required)

**chunk.rs** - Extend BlockType enum:
```rust
#[repr(u8)]
pub enum BlockType {
    // ... existing 0-32 ...
    Deepslate = 33,
    Moss = 34,
    MossyCobblestone = 35,
    Clay = 36,
    Dripstone = 37,
    Calcite = 38,
    Terracotta = 39,
    PackedIce = 40,
    Podzol = 41,
    Mycelium = 42,
    CoarseDirt = 43,
    RootedDirt = 44,
}
```

**Update all property methods** (is_solid, color, break_time, etc.)

#### 4. Sprite Generation (Required)

**sprite_gen.rs** - Add new blocks to generation list.

### No Changes Needed

| System | Why No Change |
|--------|---------------|
| **Chunk Storage** | Still u8 per block (capacity: 256) |
| **GPU Buffers** | Same format, just more textures |
| **Ray Marching** | Block type lookup unchanged |
| **Lighting** | No new emissive blocks in this plan |
| **Fluid Simulation** | River uses existing WaterType::River |
| **Model System** | New models use existing infrastructure |
| **Save/Load** | Block types serialize as u8 |

---

## Part 6: Modular Architecture

### Current Problem

`terrain_gen.rs` is **2532 lines** with poor separation of concerns:

```
Current terrain_gen.rs breakdown:
├── Structs/Enums (BiomeType, BiomeInfo, etc.)     ~80 lines
├── TerrainGenerator struct + noise setup          ~100 lines
├── Biome detection (get_biome_info, get_height)   ~180 lines
├── Chunk generation (normal, flat)                ~250 lines
├── Ground cover generation                        ~100 lines
├── Lava-water conversion                          ~45 lines
├── TREE GENERATION                                ~1770 lines (70%!)
│   ├── generate_oak + variants                    ~450 lines
│   ├── generate_pine + variants                   ~250 lines
│   ├── generate_willow + variants                 ~420 lines
│   ├── generate_snow_pine + dead_tree             ~285 lines
│   └── generate_cactus                            ~130 lines
├── Cave decorations                               ~55 lines
└── Utilities (get_block_safe, set_block_safe)     ~50 lines
```

### Proposed Module Structure

```
src/
├── world_gen/
│   ├── mod.rs                 # Re-exports, WorldGenerator facade
│   │
│   ├── biome/
│   │   ├── mod.rs             # BiomeType enum, BiomeInfo struct
│   │   ├── climate.rs         # ClimatePoint, multinoise sampling
│   │   ├── selection.rs       # Biome selection algorithm
│   │   └── profiles.rs        # Per-biome climate profiles
│   │
│   ├── terrain/
│   │   ├── mod.rs             # TerrainGenerator struct
│   │   ├── noise.rs           # Noise generator setup
│   │   ├── height.rs          # Height calculation, blending
│   │   ├── surface.rs         # Surface/subsurface block selection
│   │   └── chunk.rs           # generate_chunk_terrain entry point
│   │
│   ├── caves/
│   │   ├── mod.rs             # CaveGenerator struct, is_cave()
│   │   ├── cheese.rs          # Cheese cave generation
│   │   ├── spaghetti.rs       # Spaghetti cave generation
│   │   ├── noodle.rs          # Noodle cave generation
│   │   ├── carved.rs          # Perlin worm caves
│   │   ├── aquifer.rs         # Underground water/lava
│   │   ├── decorations.rs     # Stalactites, stalagmites
│   │   └── biomes.rs          # Underground biome selection
│   │
│   ├── trees/
│   │   ├── mod.rs             # Tree dispatcher, spawn logic
│   │   ├── oak.rs             # Oak trees (normal + giant)
│   │   ├── birch.rs           # Birch trees (new)
│   │   ├── pine.rs            # Pine trees (normal + giant)
│   │   ├── spruce.rs          # Spruce/taiga trees (new)
│   │   ├── willow.rs          # Willow trees (small/med/large)
│   │   ├── snow.rs            # Snow pine + dead trees
│   │   ├── jungle.rs          # Jungle trees (new)
│   │   ├── acacia.rs          # Acacia trees (new)
│   │   └── cactus.rs          # Desert cactus
│   │
│   ├── vegetation/
│   │   ├── mod.rs             # Ground cover dispatcher
│   │   ├── grass.rs           # Tall grass, flowers
│   │   ├── mushrooms.rs       # Surface + cave mushrooms
│   │   ├── aquatic.rs         # Lily pads, kelp
│   │   └── cave_plants.rs     # Glow berries, moss, dripleaf
│   │
│   ├── rivers/
│   │   ├── mod.rs             # River generation coordinator
│   │   ├── path.rs            # River path tracing
│   │   ├── carving.rs         # Terrain carving
│   │   └── water.rs           # Water placement + sources
│   │
│   └── utils/
│       ├── mod.rs             # Common utilities
│       ├── overflow.rs        # OverflowBlock system
│       └── block_helpers.rs   # get_block_safe, set_block_safe
│
├── cave_gen.rs                # DEPRECATED - migrate to world_gen/caves/
└── terrain_gen.rs             # DEPRECATED - migrate to world_gen/
```

### Module Size Guidelines

| Module | Target Lines | Purpose |
|--------|--------------|---------|
| **biome/mod.rs** | <100 | Enum definitions, re-exports |
| **biome/climate.rs** | ~150 | ClimatePoint, noise sampling |
| **biome/selection.rs** | ~200 | Distance calc, biome lookup |
| **biome/profiles.rs** | ~150 | Static biome definitions |
| **terrain/noise.rs** | ~100 | Noise generator construction |
| **terrain/height.rs** | ~200 | Height calc with blending |
| **terrain/surface.rs** | ~150 | Block selection by biome |
| **terrain/chunk.rs** | ~200 | Main generation loop |
| **caves/*.rs** | ~100-200 each | One cave type per file |
| **trees/*.rs** | ~150-300 each | One tree type per file |
| **vegetation/*.rs** | ~100-150 each | Ground cover types |
| **rivers/*.rs** | ~150-200 each | River generation phases |

**Total**: ~3000-4000 lines across ~30 files (avg ~100-130 lines/file)

### Migration Strategy

#### Step 1: Extract Trees First (Biggest Win)

```rust
// src/world_gen/trees/mod.rs
mod oak;
mod pine;
mod willow;
mod snow;
mod cactus;

pub use oak::generate_oak;
pub use pine::generate_pine;
// ...

pub fn generate_trees(
    chunk: &mut Chunk,
    terrain: &TerrainGenerator,
    // ...
) {
    // Existing dispatcher logic
}
```

This immediately removes ~1770 lines from terrain_gen.rs.

#### Step 2: Extract Biome System

Move BiomeType, BiomeInfo, and selection logic to `world_gen/biome/`.

#### Step 3: Extract Cave System

Move cave_gen.rs contents to `world_gen/caves/` and split by cave type.

#### Step 4: Extract Remaining Terrain

Move height calculation, surface selection to `world_gen/terrain/`.

#### Step 5: Add New Features

Add rivers, new biomes, new cave types in their respective modules.

### Interface Design

```rust
// src/world_gen/mod.rs

/// Main entry point for world generation
pub struct WorldGenerator {
    biome_sampler: BiomeSampler,
    terrain_gen: TerrainGenerator,
    cave_gen: CaveGenerator,
    tree_gen: TreeGenerator,
    vegetation_gen: VegetationGenerator,
    river_gen: Option<RiverGenerator>,
}

impl WorldGenerator {
    pub fn new(seed: u32) -> Self { /* ... */ }

    /// Generate a complete chunk with all features
    pub fn generate_chunk(&self, chunk_pos: Vector3<i32>) -> ChunkGenerationResult {
        let mut chunk = Chunk::new();

        // 1. Sample biomes for chunk
        let biome_map = self.biome_sampler.sample_chunk(chunk_pos);

        // 2. Generate base terrain
        self.terrain_gen.generate(&mut chunk, chunk_pos, &biome_map);

        // 3. Carve caves
        self.cave_gen.carve(&mut chunk, chunk_pos, &biome_map);

        // 4. Apply rivers (if in river region)
        if let Some(river_gen) = &self.river_gen {
            river_gen.apply(&mut chunk, chunk_pos);
        }

        // 5. Generate trees
        let overflow = self.tree_gen.generate(&mut chunk, chunk_pos, &biome_map);

        // 6. Add ground cover
        self.vegetation_gen.generate(&mut chunk, chunk_pos, &biome_map);

        ChunkGenerationResult { chunk, overflow_blocks: overflow }
    }
}
```

### Backward Compatibility

During migration, maintain the old API:

```rust
// src/terrain_gen.rs (temporary facade)
pub use crate::world_gen::{
    BiomeType, BiomeInfo, TerrainGenerator,
    generate_chunk_terrain, SEA_LEVEL,
};

// Deprecation warning
#[deprecated(note = "Use world_gen module directly")]
pub fn old_function() { /* ... */ }
```

---

## Part 7: Implementation Phases

### Phase 0: Modular Refactoring (First Priority) ✅ COMPLETED

Before adding new features, restructure existing code for maintainability.

**0.1 Create world_gen module structure** ✅
- Create `src/world_gen/` directory
- Create `mod.rs` with re-exports
- Create subdirectory stubs (biome/, terrain/, caves/, trees/, vegetation/, utils/)

**0.2 Extract tree generation (~1770 lines)** ✅
- Move `generate_trees` dispatcher to `world_gen/trees/mod.rs`
- Move `generate_oak` + helpers to `world_gen/trees/oak.rs`
- Move `generate_pine` + helpers to `world_gen/trees/pine.rs`
- Move `generate_willow` + helpers to `world_gen/trees/willow.rs`
- Move `generate_snow_pine` + `generate_dead_tree` to `world_gen/trees/snow.rs`
- Move `generate_cactus` to `world_gen/trees/cactus.rs`
- Update imports in terrain_gen.rs

**0.3 Extract biome system** ✅
- Move `BiomeType`, `BiomeInfo` to `world_gen/biome/mod.rs`
- Move `get_biome_info`, `get_biome` to `world_gen/biome/selection.rs`

**0.4 Extract cave system** ✅
- Move `cave_gen.rs` contents to `world_gen/caves/mod.rs`
- Split into `decorations.rs`, `aquifer.rs`

**0.5 Extract utilities** ✅
- Move `OverflowBlock`, `ChunkGenerationResult` to `world_gen/utils/overflow.rs`
- Move `get_block_safe`, `set_block_safe` to `world_gen/utils/block_helpers.rs`

**0.6 Create facade and deprecate old files** ✅
- Make `terrain_gen.rs` a thin re-export facade
- Make `cave_gen.rs` a thin re-export facade
- Add deprecation warnings

**Checkpoint**: Run `make checkall`, commit working refactor ✅

---

### Phase 1: Foundation ✅ COMPLETED

**1.1 Add new block types** ✅ (partial - core blocks added)
- Extend BlockType enum in `chunk.rs`
- Update shader constants in `common.glsl`
- Generate textures using `/voxel-texture` skill
- Update texture atlas
- Add to sprite generation

**1.2 Create multinoise climate system** ✅
- Create `world_gen/biome/climate.rs`
- Add `ClimatePoint` struct
- Add continentalness, erosion, weirdness noise generators
- Implement `sample_climate()` function

**1.3 Expand biome system** ✅
- Update `BiomeType` enum (15 surface + 3 underground)
- Create `world_gen/biome/profiles.rs` with climate ranges
- Implement nearest-neighbor biome selection

**Checkpoint**: Run `make checkall`, commit ✅

---

### Phase 2: Terrain Generation ✅ COMPLETED

**2.1 Update height calculation** ✅
- Create `world_gen/terrain/height.rs`
- Use continentalness for ocean/inland base height
- Use erosion for height amplitude
- Keep existing blending logic

**2.2 Update surface materials** ✅
- Create `world_gen/terrain/surface.rs`
- Add new surface blocks per biome (Podzol, CoarseDirt, etc.)
- Update subsurface layers

**2.3 Add underground biome selection** ✅
- Create `world_gen/caves/biomes.rs`
- Implement 3D biome sampling
- Add depth-based biome switching

**Checkpoint**: Run `make checkall`, commit ✅

---

### Phase 3: Cave System ✅ COMPLETED

**3.1 Implement cheese caves** ✅
- Create `world_gen/caves/cheese.rs`
- Large cavern noise with pillar preservation
- Integrate with existing cave system

**3.2 Implement spaghetti caves** ✅
- Create `world_gen/caves/spaghetti.rs`
- Dual noise intersection algorithm
- Y-axis stretching for horizontal preference

**3.3 Implement noodle caves** ✅
- Create `world_gen/caves/noodle.rs`
- Fine network with higher frequency
- Connect to other cave types

**3.4 Add carved caves (Perlin worms)** ✅
- Create `world_gen/caves/carved.rs`
- Worm path generation
- Ravine variants

**3.5 Update cave coordinator** ✅
- Modify `world_gen/caves/mod.rs`
- Combine all four cave types
- Add biome-specific density

**Checkpoint**: Run `make checkall`, commit ✅

---

### Phase 4: Rivers ✅ COMPLETED

**4.1 River path generation** ✅
- Create `world_gen/rivers/mod.rs`
- Noise-based river detection using RidgedMulti
- Biome-specific river thresholds

**4.2 River carving** ✅
- Terrain height modification integrated into TerrainGenerator
- Rivers carve 2-4 blocks into terrain

**4.3 River water placement** ✅
- Water fills carved channels at generation time
- Integration with existing fluid system

**4.4 River coordinator** ✅
- RiverGenerator with biome-aware thresholds
- River types: MainRiver, Tributary, MountainStream

**Checkpoint**: Run `make checkall`, commit ✅

---

### Phase 5: New Trees and Vegetation ✅ COMPLETED

**5.1 Add new tree types** ✅
- Create `world_gen/trees/birch.rs` - Tall thin trees
- Create `world_gen/trees/jungle.rs` - Normal and giant variants with vines
- Create `world_gen/trees/acacia.rs` - Bent trunk, umbrella canopy
- Update tree dispatcher for new biomes

**5.2 Add cave vegetation** ✅
- 5 new cave vegetation models (IDs 110-114):
  - Moss carpet, glow lichen, hanging roots, glow berry vines, glow mushroom
- Biome-specific placement (LushCaves, DeepDark, DripstoneCaves)

**5.3 Update ground cover** ✅
- 4 new surface vegetation models (IDs 115-118):
  - Fern, dead bush, seagrass, blue flower
- Enhanced biome-specific vegetation placement
- Desert dead bushes, ocean seagrass, taiga/jungle ferns

**Checkpoint**: Run `make checkall`, commit ✅

---

### Phase 6: Polish and Testing

**6.1 Underground biome features**
- Lush cave moss floors
- Dripstone formations
- Deep dark sculk (if adding)

**6.2 Balancing**
- Biome distribution testing
- Cave density tuning
- River frequency adjustment

**6.3 Performance optimization**
- Noise caching at boundaries
- LOD for distant generation
- Profile and optimize hot paths

**6.4 Final cleanup**
- Remove deprecated facades
- Update documentation
- Final `make checkall`

**Checkpoint**: Commit, tag release

---

## Part 8: File Change Summary

### Files to Modify

| File | Changes |
|------|---------|
| `src/chunk.rs` | Add 12 new BlockType variants, properties |
| `src/terrain_gen.rs` | Convert to thin facade re-exporting world_gen |
| `src/cave_gen.rs` | Convert to thin facade re-exporting world_gen |
| `src/main.rs` | Update imports to use world_gen module |
| `src/lib.rs` | Add world_gen module declaration |
| `shaders/common.glsl` | Add 12 new BLOCK_* constants |
| `shaders/materials.glsl` | Update ATLAS_TILE_COUNT, add mappings |
| `src/sprite_gen.rs` | Add new blocks to sprite generation |
| `src/ui/palette.rs` | Add new blocks to palette |

### New Module Structure to Create

```
src/world_gen/
├── mod.rs                      # WorldGenerator facade, re-exports
│
├── biome/
│   ├── mod.rs                  # BiomeType enum, BiomeInfo
│   ├── climate.rs              # ClimatePoint, multinoise sampling
│   ├── selection.rs            # Biome selection algorithm
│   └── profiles.rs             # Per-biome climate definitions
│
├── terrain/
│   ├── mod.rs                  # TerrainGenerator struct
│   ├── noise.rs                # Noise generator setup
│   ├── height.rs               # Height calculation + blending
│   ├── surface.rs              # Surface/subsurface blocks
│   └── chunk.rs                # Main chunk generation
│
├── caves/
│   ├── mod.rs                  # CaveGenerator, is_cave()
│   ├── cheese.rs               # Large caverns
│   ├── spaghetti.rs            # Long tunnels
│   ├── noodle.rs               # Fine networks
│   ├── carved.rs               # Perlin worms
│   ├── aquifer.rs              # Water/lava fills
│   ├── decorations.rs          # Stalactites, etc.
│   └── biomes.rs               # Underground biome selection
│
├── trees/
│   ├── mod.rs                  # Tree dispatcher
│   ├── oak.rs                  # Oak (existing)
│   ├── birch.rs                # Birch (new)
│   ├── pine.rs                 # Pine (existing)
│   ├── spruce.rs               # Spruce (new)
│   ├── willow.rs               # Willow (existing)
│   ├── snow.rs                 # Snow pine + dead (existing)
│   ├── jungle.rs               # Jungle (new)
│   ├── acacia.rs               # Acacia (new)
│   └── cactus.rs               # Cactus (existing)
│
├── vegetation/
│   ├── mod.rs                  # Ground cover dispatcher
│   ├── grass.rs                # Grass, flowers
│   ├── mushrooms.rs            # Mushrooms
│   ├── aquatic.rs              # Lily pads, kelp
│   └── cave_plants.rs          # Glow berries, moss
│
├── rivers/
│   ├── mod.rs                  # River coordinator
│   ├── path.rs                 # Path tracing
│   ├── carving.rs              # Terrain modification
│   └── water.rs                # Water placement
│
└── utils/
    ├── mod.rs                  # Common utilities
    ├── overflow.rs             # OverflowBlock system
    └── block_helpers.rs        # Block access helpers
```

**Total new files**: ~35 Rust files
**Average size**: ~100-200 lines per file

### Textures to Generate (12 new)

```
deepslate_64x64.png     - Dark layered stone
moss_64x64.png          - Green mossy texture
mossy_cobble_64x64.png  - Cobblestone with moss patches
clay_64x64.png          - Gray-blue clay
dripstone_64x64.png     - Tan pointed stone
calcite_64x64.png       - White crystalline
terracotta_64x64.png    - Orange-brown clay
packed_ice_64x64.png    - Solid blue ice
podzol_64x64.png        - Dark forest floor
mycelium_64x64.png      - Purple fungal surface
coarse_dirt_64x64.png   - Rocky dirt
rooted_dirt_64x64.png   - Dirt with roots
```

---

## Part 9: Risk Assessment

### Low Risk
- Adding new block types (well-understood process)
- Texture atlas expansion (simple concatenation)
- New noise generators (existing crate supports all needed types)

### Medium Risk
- Biome blending (may need iteration to avoid artifacts)
- Cave type balancing (cheese caves can be too large)
- River path generation (may create unrealistic paths)

### High Risk
- Cross-chunk river continuity (overflow system exists but untested for rivers)
- Underground biome transitions (3D biome boundaries are complex)
- Performance (more noise samples per block)

### Mitigation Strategies

1. **Performance**: Cache noise at chunk boundaries, LOD for distant chunks
2. **River continuity**: Generate river network at region level (16×16 chunks)
3. **Biome transitions**: Use existing boundary blending, extend to 3D

---

## Appendix: Noise Generator Reference

### Required Noise Generators

| Name | Type | Freq | Octaves | Purpose |
|------|------|------|---------|---------|
| temperature_noise | 3D fBm | 0.002 | 4 | Climate temp |
| humidity_noise | 3D fBm | 0.002 | 4 | Climate humidity |
| continentalness_noise | 2D fBm | 0.001 | 6 | Ocean-inland |
| erosion_noise | 2D fBm | 0.002 | 4 | Flat-mountain |
| weirdness_noise | 2D fBm | 0.003 | 3 | Biome variants |
| cheese_noise | 3D Perlin | 0.02 | 1 | Large caves |
| spaghetti_noise1 | 3D Perlin | 0.05 | 1 | Tunnel caves |
| spaghetti_noise2 | 3D Perlin | 0.05 | 1 | Tunnel caves |
| noodle_noise1 | 3D Perlin | 0.10 | 1 | Fine caves |
| noodle_noise2 | 3D Perlin | 0.10 | 1 | Fine caves |
| river_noise | 2D Perlin | 0.01 | 1 | River meandering |

---

*This plan maintains voxel_world's 512-block height while adopting Minecraft 1.18+'s multinoise biome system, four-type cave generation, and carved river channels.*

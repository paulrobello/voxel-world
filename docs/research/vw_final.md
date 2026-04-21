# Voxel World Terrain Generation - Implementation Specification

This document details the current terrain, cave, and biome generation system in voxel_world.

---

## Overview

Voxel World uses a **Perlin noise-based terrain generation system** with five distinct biomes determined by temperature and rainfall parameters. The system features biome-specific terrain heights, cave generation with depth-based variation, and procedural vegetation.

### Key Characteristics
- **World Height**: 512 blocks (Y = 0 to Y = 511)
- **Sea Level**: Y = 75
- **Chunk Size**: 32×32×32 blocks
- **Biome Count**: 5 (Grassland, Mountains, Desert, Swamp, Snow)
- **Generation**: Noise-based with biome boundary blending

---

## Phase 1: Climate Parameter Generation

The biome system uses **two primary climate parameters** generated via Perlin noise.

### Noise Generators

| Noise Type | Seed Offset | Frequency | Purpose |
|------------|-------------|-----------|---------|
| `temperature_noise` | seed + 6 | 0.002 | Large-scale temperature variation |
| `rainfall_noise` | seed + 7 | 0.002 | Large-scale precipitation patterns |
| `mountain_region_noise` | seed + 8 | 0.0005 | Continent-scale mountain placement |

### Temperature Calculation

Raw temperature is modified by elevation (lapse rate):

```
raw_temp = (temperature_noise.get([x * 0.002, z * 0.002]) + 1.0) / 2.0
elevation_cooling = base_height.max(0.0) * 0.4
adjusted_temp = raw_temp - elevation_cooling
```

**Effect**: Higher terrain becomes colder, enabling snow-capped peaks.

### Rainfall Calculation

```
rainfall = (rainfall_noise.get([x * 0.002, z * 0.002]) + 1.0) / 2.0
```

**Range**: 0.0 (arid) to 1.0 (wet)

---

## Phase 2: Biome Selection

Biomes are selected using **hard threshold boundaries** on temperature and rainfall.

### BiomeType Enum

```rust
pub enum BiomeType {
    Grassland,   // Default temperate
    Mountains,   // High elevation + mountain region
    Desert,      // Hot and dry
    Swamp,       // Warm and wet
    Snow,        // Cold regions
}
```

### Selection Logic

| Priority | Condition | Biome |
|----------|-----------|-------|
| 1 | `adjusted_temp < 0.3` | Snow |
| 2 | `adjusted_temp > 0.7 && rainfall < 0.3` | Desert |
| 3 | `adjusted_temp > 0.6 && rainfall > 0.7` | Swamp |
| 4 | `in_mountain_region && base_height > 0.25` | Mountains |
| 5 | Default | Grassland |

### Mountain Region Detection

Mountains require **two conditions**:

```
mountain_region = mountain_region_noise.get([x * 0.0005, z * 0.0005])
in_mountain_region = mountain_region > -0.3  // ~35% world coverage
is_mountains = in_mountain_region && base_height > 0.25
```

**Purpose**: Creates large, contiguous mountain ranges rather than isolated peaks.

### Biome Climate Grid

```
              Temperature
         Cold ─────────────────► Hot
    Wet   │ Snow │  Swamp  │ Swamp  │ Swamp │
      ▲   ├──────┼─────────┼────────┼───────┤
      │   │ Snow │Grassland│Grassland│ Desert│
Rainfall  ├──────┼─────────┼────────┼───────┤
      │   │ Snow │Grassland│Grassland│ Desert│
      ▼   ├──────┼─────────┼────────┼───────┤
    Dry   │ Snow │Grassland│Grassland│ Desert│
          └──────┴─────────┴────────┴───────┘

    Note: Mountains overlay based on region noise + elevation
```

---

## Phase 3: Terrain Height Generation

Height is calculated using **multiple noise layers** with biome-specific formulas.

### Noise Layers

| Layer | Type | Octaves | Frequency | Purpose |
|-------|------|---------|-----------|---------|
| `height_noise` | Fbm<Perlin> | 4 | 0.003 | Continental-scale terrain |
| `detail_noise` | Perlin | 1 | 0.02 | Fine surface variation |
| `mountain_noise` | RidgedMulti<Perlin> | 5 | 0.008 | Sharp mountain peaks |

### Fbm (Fractal Brownian Motion) Parameters

```
Lacunarity: 2.0
Persistence: 0.5
Octaves: 4
```

### RidgedMulti Parameters (Mountains)

```
Lacunarity: 2.2
Persistence: 0.5
Octaves: 5
```

**Effect**: Creates sharp, ridge-like peaks characteristic of mountain ranges.

### Height Formulas by Biome

| Biome | Formula | Typical Range |
|-------|---------|---------------|
| **Grassland** | `128 + detail × 2 + base × 4` | 120-140 |
| **Mountains** | `128 + base × 10 + ridges × 55` | 128-240+ |
| **Desert** | `128 + detail × 1 + base × 2` | 125-135 |
| **Swamp** | `128 + detail × 2` | 126-132 |
| **Snow** (low) | `128 + detail × 2` | 126-132 |
| **Snow** (high) | `128 + base × 8 + ridges × 40` | 128-200+ |

Snow biome switches formula when `base > 0.5` to create snowy peaks.

### Biome Boundary Blending

At biome boundaries, height is **interpolated** to prevent sharp cliffs:

```
1. Sample biome at center position
2. Sample biomes at 4 neighbors (±4 blocks in X and Z)
3. If any neighbor differs from center:
   a. Sample 7×7 grid around position
   b. Calculate inverse-distance-weighted average
   c. Blend heights from all contributing biomes
```

**Sample Offset**: 4 blocks
**Blend Grid**: 7×7 samples

---

## Phase 4: Surface Material Placement

Surface blocks are determined by biome type and elevation.

### Surface Block Rules

| Biome | Surface Block | Condition |
|-------|---------------|-----------|
| Snow | `BlockType::Snow` | Always |
| Desert | `BlockType::Sand` | Always |
| Mountains | `BlockType::Stone` | Always |
| Swamp | `BlockType::Mud` | Always |
| Grassland | `BlockType::Sand` | If `height <= SEA_LEVEL + 2` (beach) |
| Grassland | `BlockType::Grass` | Otherwise |

### Subsurface Layers (4 blocks deep)

| Biome | Subsurface Block |
|-------|------------------|
| Desert | `BlockType::Sandstone` |
| Mountains | `BlockType::Stone` |
| Snow | `BlockType::Ice` |
| Beach | `BlockType::Sand` |
| Default | `BlockType::Dirt` |

### Deep Underground

| Biome | Underground Block |
|-------|-------------------|
| Snow | `BlockType::Ice` (fully frozen) |
| All others | `BlockType::Stone` |

### Special Layers

- **Y = 0**: `BlockType::Bedrock` (unbreakable floor)
- **Y > height && Y <= SEA_LEVEL**: `BlockType::Water`

---

## Phase 5: Cave Generation

Caves use **3D Perlin noise** with depth-based threshold variation.

### Cave Noise Generators

| Noise | Seed Offset | Frequency | Purpose |
|-------|-------------|-----------|---------|
| `cave_noise` | seed + 3 | 0.05, 0.08, 0.05 | 3D cave carving |
| `cave_mask_noise` | seed + 4 | 0.01 | Regional density variation |
| `entrance_noise` | seed + 5 | 0.02 | Surface entrance placement |
| `decoration_noise` | seed + 8 | 0.1 | Stalactite/stalagmite placement |

### Cave Carving Algorithm

```
cave_value = cave_noise.get([x * 0.05, y * 0.08, z * 0.05])

// Y-axis stretched (0.08 vs 0.05) for horizontal tunnel preference

depth_factor = ((surface_height - y) / 30.0).clamp(0.0, 1.0)
cave_density = cave_mask_noise.get([x * 0.01, z * 0.01]) * 0.5 + 0.5

threshold = 0.55 - (depth_factor × 0.15) - (cave_density × 0.1 × biome_multiplier)

is_cave = |cave_value| > threshold
```

### Threshold Behavior

| Depth | Threshold | Cave Frequency |
|-------|-----------|----------------|
| Surface | 0.55 | Rare |
| 15 blocks deep | 0.475 | Moderate |
| 30+ blocks deep | 0.40 | Common |

### Biome Cave Density Multipliers

| Biome | Multiplier | Effect |
|-------|------------|--------|
| Mountains | 2.0× | Much more caves (for lava lakes) |
| Grassland | 1.0× | Baseline |
| Snow | 0.9× | Slightly fewer |
| Swamp | 0.8× | Fewer caves |
| Desert | 0.6× | Significantly fewer |

### Surface Protection

- **Normal areas**: 12-block buffer below surface
- **Cave entrances**: 0-block buffer (caves breach surface)
- **Entrance detection**: `entrance_noise.get([x * 0.02, z * 0.02]) > 0.45`

### Cave Types Comparison to Minecraft

```
┌─────────────────────────────────────────────────────────────┐
│              VOXEL WORLD CAVE SYSTEM                        │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Single noise-based system (similar to MC "Spaghetti")      │
│                                                             │
│     ════════╲                                               │
│              ╲════════                                      │
│    ═══════════╲       ╲═════════                           │
│                ═══════════════                              │
│                                                             │
│  - 3D Perlin noise with threshold                           │
│  - Y-axis stretched for horizontal preference               │
│  - Depth-based frequency increase                           │
│  - Regional density variation via 2D mask                   │
│  - No cheese caves or noodle caves (yet)                    │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## Phase 6: Cave Fill Types

Caves are filled differently based on biome and depth.

### Universal Lava Lakes (All Biomes)

**Y Range**: 2-10

```
depth_factor = (10 - y) / 8.0
lava_threshold = 0.7 - (depth_factor × 0.4)

// At Y=10: threshold = 0.7 (rare lava)
// At Y=2:  threshold = 0.3 (common lava)
```

### Biome-Specific Cave Fill

| Biome | Fill Type | Y Range | Details |
|-------|-----------|---------|---------|
| **Desert** | Air | All | Completely dry caves |
| **Swamp** | Water | ≤ SEA_LEVEL + 5 | Heavily flooded |
| **Snow** | Air | All | Ice walls, empty interior |
| **Mountains** | Lava | 2-75 | Extended lava lakes |
| **Grassland** | Air | All | No water filling |

### Mountain Extended Lava

Mountains have additional lava from Y=11 to SEA_LEVEL (75):

```
depth_factor = (SEA_LEVEL - y) / SEA_LEVEL
lava_threshold = 0.7 - (depth_factor × 0.4)
```

---

## Phase 7: Cave Decorations

### Stalactites (Ceiling)

**Spawn Condition**: Air block below stone ceiling
**Spawn Rate**: ~15% where `decoration_noise > 0.7`
**Noise Frequency**: 0.1

| Biome | Model ID |
|-------|----------|
| Snow | 108 (Ice stalactite) |
| Others | 106 (Stone stalactite) |

### Stalagmites (Floor)

**Spawn Condition**: Air block above stone floor
**Spawn Rate**: ~15% where `decoration_noise > 0.7`
**Noise Offset**: +100.0 in X and Z (prevents alignment with stalactites)

| Biome | Model ID |
|-------|----------|
| Snow | 109 (Ice stalagmite) |
| Others | 107 (Stone stalagmite) |

---

## Phase 8: Tree Generation

Trees spawn on a **4-block grid** with hash-based randomness.

### Tree Spawn Conditions

1. Surface must be above sea level (`height >= SEA_LEVEL`)
2. Terrain slope must be gentle (max height diff ≤ 3 blocks in 4-block radius)
3. Foundation must be solid (2 blocks below tree base)

### Tree Types by Biome

| Biome | Tree Type | Density | Height Range |
|-------|-----------|---------|--------------|
| **Grassland** | Oak | 5% | 4-9 (normal), multi-deck (giant) |
| **Mountains** | Pine | 3% | 6-13 (normal), 15-22 (giant) |
| **Snow** | Snow Pine | 6% | 8-14 |
| **Snow** | Dead Tree | 8% | 6-12 |
| **Swamp** | Willow | 12% | 4-7 (S), 7-9 (M), 10-13 (L) |
| **Desert** | Cactus | 2% | 3-5 |

### Oak Trees (Grassland)

- **Variants**: Normal (90%) or Giant (10%)
- **Normal**: Single trunk, varied canopy shapes (tapered, cube, blob, round)
- **Giant**: 2-3 decks with cross-bracing branches
- **Block Types**: Log, Leaves

### Pine Trees (Mountains)

- **Variants**: Normal (90%) or Giant (10%)
- **Shape**: Conical taper, foliage starts ~1/3 up trunk
- **Restriction**: Only spawns below Y=80
- **Block Types**: PineLog, PineLeaves

### Snow Pine (Snow Biome)

- **Shape**: Layered conical with heavy snow coverage
- **Snow Coverage**: 60% of leaf positions replaced with snow
- **Features**: Snow cap, snow drifts around base
- **Block Types**: PineLog, PineLeaves, Snow

### Dead Trees (Snow Biome)

- **Shape**: Bare trunk with 2-4 horizontal branches
- **Features**: Snow accumulation on branches and trunk
- **Block Types**: Log (no leaves)

### Willow Trees (Swamp)

- **Variants**: Small (60%), Medium (30%), Large (10%)
- **Shape**: Hollow draping canopy with hanging vines
- **Features**: Vines extend from canopy edge toward ground
- **Block Types**: WillowLog, WillowLeaves

### Cactus (Desert)

- **Shape**: Main column with 1-2 branches on tall specimens
- **Branches**: Horizontal extensions with vertical growth
- **Block Types**: Cactus

---

## Phase 9: Ground Cover

Surface decorations placed as model blocks above terrain.

### Decoration by Biome

| Biome | Decoration | Chance | Model IDs |
|-------|------------|--------|-----------|
| **Grassland** | Tall Grass | 10% | 100 |
| **Grassland** | Flowers | 2% | 101 (red), 102 (yellow) |
| **Swamp** | Tall Grass | 15% | 100 |
| **Swamp** | Mushrooms | 5% | 104 (brown), 105 (red) |
| **Swamp** | Lily Pads | 5% | 103 (on water) |
| **Mountains** | Tall Grass | 5% | 100 (on grass only) |
| **Mountains** | Snow | - | Replaces stone above Y=155 |
| **Desert** | None | - | - |
| **Snow** | None | - | - |

### Lily Pad Placement

- Placed on water surface blocks
- Uses waterlogged model blocks
- Rotation varies by position hash

---

## Phase 10: Water Types

Each biome has an associated water type for visual/gameplay variation.

| Biome | Water Type | Typical Use |
|-------|------------|-------------|
| Grassland | Lake | Still, clear water |
| Mountains | Spring | Cool, clear water |
| Desert | River | Sparse water features |
| Swamp | Swamp | Murky, dark water |
| Snow | River | Icy, cold water |

---

## Generation Pipeline Summary

```
┌─────────────────────────────────────────────────────────────┐
│                    CHUNK GENERATION FLOW                     │
└─────────────────────────────────────────────────────────────┘

1. CLIMATE SAMPLING
   ├── Sample temperature_noise at (x × 0.002, z × 0.002)
   ├── Sample rainfall_noise at (x × 0.002, z × 0.002)
   └── Sample mountain_region_noise at (x × 0.0005, z × 0.0005)

2. BIOME DETERMINATION
   ├── Calculate adjusted_temp (with elevation lapse)
   ├── Apply threshold rules (Snow → Desert → Swamp → Mountains → Grassland)
   └── Store biome for chunk

3. HEIGHT CALCULATION
   ├── Sample height_noise (Fbm, continental features)
   ├── Sample mountain_noise (RidgedMulti, peaks)
   ├── Sample detail_noise (fine variation)
   ├── Apply biome-specific height formula
   └── Blend at biome boundaries (7×7 weighted average)

4. BLOCK PLACEMENT (per column)
   ├── Y=0: Bedrock
   ├── Y > height && Y > SEA_LEVEL: Air
   ├── Y > height && Y ≤ SEA_LEVEL: Water
   ├── Check cave_generator.is_cave()
   │   ├── If cave: Apply cave fill type
   │   └── If not cave: Continue
   ├── Y == height: Surface block (biome-specific)
   ├── Y in [height-4, height-1]: Subsurface
   └── Y < height-4: Deep underground (Stone or Ice)

5. TREE GENERATION
   ├── Iterate 4-block grid
   ├── Check spawn conditions (elevation, slope, foundation)
   ├── Select tree type by biome
   └── Generate tree structure (may overflow to neighbors)

6. GROUND COVER
   ├── Iterate surface blocks
   ├── Apply biome-specific decoration chances
   └── Place model blocks

7. CAVE DECORATIONS
   ├── Scan for stone-air transitions
   ├── Place stalactites below ceilings
   └── Place stalagmites above floors

8. POST-PROCESSING
   └── Convert lava-water contacts to cobblestone
```

---

## Block Type Reference

### Terrain Blocks

| ID | Name | Usage |
|----|------|-------|
| 0 | Air | Empty space |
| 1 | Stone | Underground, mountains |
| 2 | Dirt | Subsurface |
| 3 | Grass | Grassland surface |
| 6 | Sand | Desert, beaches |
| 7 | Gravel | Scattered deposits |
| 8 | Water | Oceans, lakes, caves |
| 13 | Snow | Snow biome surface |
| 16 | Bedrock | World floor (Y=0) |
| 19 | Lava | Deep caves |
| 27 | Ice | Snow biome underground |
| 28 | Mud | Swamp surface |
| 29 | Sandstone | Desert subsurface |
| 30 | Cactus | Desert vegetation |

### Tree Blocks

| ID | Name | Biome |
|----|------|-------|
| 10 | Log | Oak (Grassland) |
| 5 | Leaves | Oak (Grassland) |
| 23 | PineLog | Pine (Mountains, Snow) |
| 25 | PineLeaves | Pine (Mountains, Snow) |
| 24 | WillowLog | Willow (Swamp) |
| 26 | WillowLeaves | Willow (Swamp) |

### Gravity-Affected Blocks

- Sand
- Gravel
- Snow

---

## Noise Function Summary

| Function | Dimensions | Frequency | Octaves | Use |
|----------|------------|-----------|---------|-----|
| Fbm<Perlin> | 2D | 0.003 | 4 | Continental terrain |
| RidgedMulti | 2D | 0.008 | 5 | Mountain peaks |
| Perlin | 2D | 0.02 | 1 | Surface detail |
| Perlin | 2D | 0.002 | 1 | Temperature |
| Perlin | 2D | 0.002 | 1 | Rainfall |
| Perlin | 2D | 0.0005 | 1 | Mountain regions |
| Perlin | 3D | 0.05/0.08/0.05 | 1 | Cave carving |
| Perlin | 2D | 0.01 | 1 | Cave density mask |
| Perlin | 2D | 0.02 | 1 | Cave entrances |
| Perlin | 3D | 0.1 | 1 | Cave decorations |

---

## Key Source Files

| File | Lines | Purpose |
|------|-------|---------|
| `src/terrain_gen.rs` | 2533 | Main terrain generation |
| `src/cave_gen.rs` | 308 | Cave system |
| `src/chunk.rs` | ~300 | Block types, chunk storage |
| `src/world/tree_logic.rs` | ~200 | Tree integrity validation |

---

## Comparison: Voxel World vs Minecraft 1.18+

| Feature | Voxel World | Minecraft 1.18+ |
|---------|-------------|-----------------|
| **Biome Count** | 5 | 60+ |
| **Climate Parameters** | 2 (temp, rain) | 5 (temp, humidity, cont., erosion, weird.) |
| **Biome Selection** | Hard thresholds | Nearest-neighbor in 5D space |
| **3D Biomes** | No | Yes |
| **Height Range** | 512 blocks | 384 blocks |
| **Cave Types** | 1 (spaghetti-like) | 4 (cheese, spaghetti, noodle, worms) |
| **Underground Biomes** | No | Yes (lush caves, dripstone, deep dark) |
| **Terrain Noise** | Fbm + RidgedMulti | Multi-spline density |
| **Boundary Blending** | Distance-weighted | Smooth via noise interpolation |

---

*This specification documents voxel_world's terrain generation system as currently implemented. The system provides a solid foundation with room for expansion toward Minecraft-style complexity.*

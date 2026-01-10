# Minecraft Biome & Terrain Generation - Final Implementation Specification

This document distills the Minecraft terrain generation system as of version 1.18+ (Caves & Cliffs Update), providing a language-agnostic implementation specification.

---

## Overview

The modern Minecraft terrain generation uses a **multinoise-based system** that unifies biome selection and terrain shaping through five climate parameters. This replaces the older layer-stack approach with a more coherent 3D system that supports underground biomes.

### Key Characteristics
- **World Height**: 320 blocks total (Y = -64 to Y = 256)
- **Biome Resolution**: 4×4×4 block cells (quarter-chunks)
- **3D Biomes**: Underground areas can have different biomes than the surface
- **Unified Climate System**: Five noise parameters drive both terrain shape and biome selection

---

## Phase 1: Climate Parameter Generation

The foundation of the system is five **3D climate noise maps** generated using fractal Brownian motion (fBm) noise.

### The Five Climate Parameters

| Parameter | Noise Type | Controls |
|-----------|------------|----------|
| **Temperature** | 3D fBm | Hot vs cold biomes |
| **Humidity** | 3D fBm | Wet vs dry biomes |
| **Continentalness** | 2D fBm | Distance from coast; ocean vs inland |
| **Erosion** | 2D fBm | Flat plains vs mountainous terrain |
| **Weirdness** | 2D fBm | Biome variants and unusual formations |

### Fractal Brownian Motion (fBm) Noise

fBm is constructed by summing multiple octaves of Perlin noise:

```
fBm(x, y, z) = Σ (amplitude_i × perlin(x × frequency_i, y × frequency_i, z × frequency_i))
```

**Octave Properties:**
- Each successive octave has **2× the frequency** of the previous
- Each successive octave has **0.5× the amplitude** of the previous
- Typical octave count: 4-8 octaves
- Lower octaves create large-scale features
- Higher octaves add fine detail

**Implementation Notes:**
- Sample noise at quarter-chunk resolution (4×4×4 blocks) for performance
- Interpolate between samples for smooth transitions
- Use different seed offsets for each parameter to ensure independence

---

## Phase 2: Biome Selection

Biomes are selected by finding the **closest match** between the climate parameters at a location and predefined ideal values for each biome.

### Biome Climate Profiles

Each biome defines ideal values for all five parameters. Selection uses nearest-neighbor matching in 5D climate space.

### Temperature-Humidity Biome Grid

Based on the biome chart, here is the biome selection matrix (temperature increases left-to-right, humidity increases bottom-to-top):

#### Low Humidity (Dry)
| Temperature | Biome |
|-------------|-------|
| Freezing | Snowy Tundra, Ice Spikes* |
| Cold | Plains |
| Temperate | Plains |
| Warm | Savanna |
| Hot | Desert |

#### Medium-Low Humidity
| Temperature | Biome |
|-------------|-------|
| Freezing | Snowy Tundra |
| Cold | Plains |
| Temperate | Plains, Sunflower Plains* |
| Warm | Savanna |
| Hot | Desert |

#### Medium Humidity
| Temperature | Biome |
|-------------|-------|
| Freezing | Snowy Tundra, Snowy Taiga* |
| Cold | Forest |
| Temperate | Forest, Flower Forest* |
| Warm | Forest, Plains* |
| Hot | Desert |

#### Medium-High Humidity
| Temperature | Biome |
|-------------|-------|
| Freezing | Snowy Taiga |
| Cold | Taiga |
| Temperate | Birch Forest, Tall Birch Forest* |
| Warm | Jungle, Plains* |
| Hot | Jungle Edge, Plains* |

#### High Humidity (Wet)
| Temperature | Biome |
|-------------|-------|
| Freezing | Snowy Taiga |
| Cold | Giant Spruce Taiga, Giant Tree Taiga* |
| Temperate | Dark Forest |
| Warm | Jungle |
| Hot | Jungle, Bamboo Jungle* |

*\* indicates variant selected when Weirdness parameter exceeds threshold*

### Special Biomes by Continentalness

| Continentalness Value | Resulting Biome Type |
|-----------------------|---------------------|
| Very Low (< -0.5) | Deep Ocean |
| Low (-0.5 to -0.2) | Ocean / Shallow Ocean |
| Medium-Low (-0.2 to 0.0) | Coast / Beach |
| Medium (0.0 to 0.3) | Inland lowlands |
| High (0.3 to 0.7) | Inland / Plains / Forest |
| Very High (> 0.7) | Far inland / Mountains |

### Erosion Effects on Biome Variants

| Erosion Value | Terrain Type | Biome Modifier |
|---------------|--------------|----------------|
| Very Low (< -0.5) | Extreme peaks | Mountain variants |
| Low (-0.5 to -0.2) | Mountains | Hilly variants |
| Medium (-0.2 to 0.3) | Rolling hills | Standard biomes |
| High (0.3 to 0.6) | Flat terrain | Plains-like |
| Very High (> 0.6) | Very flat | River valleys, swamps |

---

## Phase 3: Terrain Height Generation

Terrain height is determined by combining biome parameters with 3D density noise.

### Height Calculation Pipeline

```
1. Get climate parameters for (x, z) at surface level
2. Determine base_height from Continentalness
3. Determine height_variation from Erosion
4. Apply Peaks & Valleys modifier from Weirdness
5. For each Y level from top to bottom:
   a. Calculate 3D density = base_density + height_bias
   b. If density >= 0, this Y becomes solid
   c. First solid Y = terrain surface height
```

### Depth Parameter

The **Depth** value represents how far below the ideal surface a point is:
- `depth = ideal_surface_height - current_y`
- Positive depth = underground
- Negative depth = above ground
- Used to bias density calculations toward solid below surface

### Scale Parameter

The **Scale** parameter controls height variation amplitude:
- Low scale = flat terrain (plains, beaches)
- High scale = dramatic height changes (mountains)
- Derived from biome type and erosion value

### Terrain Density Function

```
density(x, y, z) =
    fractal_noise_3d(x, y, z)           // Base terrain shape
    + depth_bias(y, target_height)       // Push toward surface height
    + biome_modifier(biome, y)           // Per-biome adjustments
```

Where:
- `fractal_noise_3d` = blended combination of multiple fBm noise samples
- `depth_bias` = linear function pushing density positive below target, negative above
- `biome_modifier` = per-biome height adjustments (e.g., mountains get positive bias at high Y)

### Blended Fractal Noise

Three fBm noise maps with different characteristics are blended:

| Noise Map | Characteristics | Purpose |
|-----------|-----------------|---------|
| **Low Noise** | Low frequency, high amplitude | Major terrain features |
| **Main Noise** | Medium frequency/amplitude | Standard terrain detail |
| **High Noise** | High frequency, low amplitude | Fine surface detail |

**Blending Formula:**
```
blend_factor = selector_noise(x, y, z)  // Another fBm noise, range [0, 1]
fractal_noise = lerp(low_noise, high_noise, blend_factor) + main_noise
```

---

## Phase 4: Cave Generation

The 1.18+ system uses **four distinct cave generation methods** operating simultaneously.

### Cave Type 1: Cheese Caves

**Purpose:** Large open caverns with irregular shapes

**Algorithm:**
1. Generate 3D Perlin noise at low frequency
2. Apply threshold: where `noise(x,y,z) > cheese_threshold`, carve air
3. Results in swiss-cheese-like large voids

**Parameters:**
- Frequency: Very low (large features)
- Threshold: ~0.6-0.7 (only strongest peaks become caves)
- Vertical bias: Slightly favor mid-depths

### Cave Type 2: Spaghetti Caves

**Purpose:** Long, winding tunnel systems

**Algorithm:**
1. Generate two 3D Perlin noise fields (offset from each other)
2. Cave exists where: `|noise1(x,y,z)| < spaghetti_width AND |noise2(x,y,z)| < spaghetti_width`
3. The intersection of two "zero-crossings" creates tubular passages

**Parameters:**
- Frequency: Medium
- Width threshold: ~0.03-0.05 (narrow tubes)
- Creates long continuous tunnels

### Cave Type 3: Noodle Caves

**Purpose:** Fine network of small interconnected passages

**Algorithm:**
- Same as spaghetti caves but with:
  - Higher frequency (smaller scale)
  - Narrower width threshold
  - Creates dense web of tiny tunnels

**Parameters:**
- Frequency: High
- Width threshold: ~0.01-0.02 (very narrow)

### Cave Type 4: Perlin Worms (Carved Caves)

**Purpose:** Traditional carved tunnel systems and ravines

**Algorithm:**
1. Select random starting points underground
2. For each starting point, simulate a "worm" that:
   - Moves in a direction influenced by Perlin noise
   - Carves a spherical or elliptical void as it moves
   - Gradually changes direction based on noise gradients
   - Has limited lifetime (tunnel length)

**Parameters:**
- Worm radius: 2-8 blocks (varies along path)
- Path length: 50-200 blocks
- Direction noise frequency: Low (smooth curves)
- Ravines use vertically-stretched ellipsoids

### Cave Floor Features: Aquifers

**Purpose:** Underground water and lava lakes

**Rules:**
- Below Y=0: Caves can fill with water (aquifers)
- Below Y=-54: Caves fill with lava instead
- Aquifer boundaries determined by separate noise field
- Creates underground lakes and flooded caverns

### Cave Pillars

**Purpose:** Structural features inside large caves

**Algorithm:**
- In cheese caves, generate vertical noise columns
- Where column noise exceeds threshold, preserve stone
- Creates stalactite/stalagmite-like pillars

---

## Phase 5: Surface Decoration

After base terrain and caves, surface materials and features are applied.

### Surface Material Rules

| Biome | Surface Block | Subsurface Block | Depth |
|-------|---------------|------------------|-------|
| Most land | Grass | Dirt | 3-5 blocks |
| Desert | Sand | Sandstone | 4-6 blocks |
| Beach | Sand | Sand/Stone | 3-4 blocks |
| Snowy | Snow layer + Grass | Dirt | 3-5 blocks |
| Badlands | Terracotta layers | Terracotta | Variable |
| Ocean floor | Sand/Gravel | Stone | 2-3 blocks |

### Surface Noise

A 2D **surface noise** map determines material layer thickness variation:
```
surface_depth(x, z) = base_depth + surface_noise(x, z) × variation
```

---

## Phase 6: World Features

Features are placed after terrain generation, respecting biome boundaries.

### Feature Categories

1. **Vegetation**
   - Trees (type and density per biome)
   - Grass and flowers
   - Mushrooms (giant and small)
   - Cacti (desert)
   - Vines (jungle, swamp)

2. **Water Features**
   - Rivers (carved post-generation)
   - Lakes (surface and underground)
   - Ocean monuments

3. **Geological Features**
   - Ore veins (depth and biome dependent)
   - Amethyst geodes
   - Fossils
   - Dripstone formations

4. **Structures**
   - Villages (plains, desert, taiga, etc.)
   - Temples (jungle, desert)
   - Ocean ruins
   - Mineshafts
   - Strongholds

### River Generation (Legacy System Still Used)

Rivers use the classic layer-stack approach:
1. Generate white noise map
2. Scale up through zoom layers
3. Apply edge detection to find boundaries between noise regions
4. These boundaries become river paths
5. Carve rivers into terrain as separate pass
6. Rivers in cold biomes become frozen rivers

---

## Phase 7: Underground Biomes

The 3D biome system enables distinct underground environments.

### Underground Biome Types

| Biome | Characteristics | Generation Condition |
|-------|-----------------|---------------------|
| **Lush Caves** | Glow berries, azalea, dripleaf | High humidity underground |
| **Dripstone Caves** | Stalactites, stalagmites | Medium humidity, specific noise |
| **Deep Dark** | Sculk, ancient cities | Very deep (Y < -20), low erosion |

### Underground Biome Selection

```
underground_biome(x, y, z) =
    if y > surface_height - 8:
        return surface_biome(x, z)  // Near surface, use surface biome
    else:
        return select_cave_biome(
            temperature(x, y, z),
            humidity(x, y, z),
            depth_below_surface
        )
```

---

## Implementation Summary

### Generation Order

1. **Climate Noise Generation**
   - Generate 5 climate parameter noise maps
   - Sample at quarter-chunk (4×4×4) resolution

2. **Biome Assignment**
   - For each quarter-chunk, find nearest biome in climate space
   - Store 3D biome map

3. **Terrain Height**
   - Generate blended fractal density noise
   - Apply depth/scale modifiers from biome
   - Determine solid/air for each block

4. **Cave Carving**
   - Apply cheese, spaghetti, noodle cave noise
   - Generate carved caves via Perlin worms
   - Add aquifers and lava pools

5. **Surface Materials**
   - Apply biome-specific surface blocks
   - Vary depth with surface noise

6. **Feature Placement**
   - Place trees, vegetation, ores
   - Generate structures

### Key Noise Functions Required

| Function | Dimensions | Usage |
|----------|------------|-------|
| Perlin Noise | 2D and 3D | Base for all noise |
| fBm (Fractal) | 2D and 3D | Climate parameters, terrain |
| Voronoi | 2D | Biome cell boundaries (optional smoothing) |

### Performance Optimizations

1. **Chunk-based Generation**: Generate in 16×16×16 or 32×32×32 chunks
2. **LOD for Noise**: Use lower octave counts for distant terrain
3. **Lazy Evaluation**: Only generate chunks as needed
4. **Noise Caching**: Cache noise values at cell boundaries for interpolation
5. **Parallel Generation**: Each chunk can be generated independently

---

## Appendix: Biome Reference Table

### Surface Biomes

| Biome | Temp | Humidity | Continentalness | Erosion | Trees | Surface |
|-------|------|----------|-----------------|---------|-------|---------|
| Plains | Mid | Low-Mid | Mid-High | High | Sparse oak | Grass |
| Forest | Mid | Mid | Mid-High | Mid | Dense oak/birch | Grass |
| Desert | Hot | Very Low | High | Mid-High | None | Sand |
| Snowy Tundra | Cold | Low | Mid-High | High | None | Snow |
| Taiga | Cold | Mid | Mid-High | Mid | Spruce | Grass+Snow |
| Jungle | Hot | High | High | Mid | Dense jungle | Grass |
| Savanna | Hot | Low-Mid | High | Mid-High | Acacia | Grass |
| Swamp | Mid | High | Low-Mid | Very High | Oak+vines | Grass+Water |
| Mountains | Any | Any | High | Very Low | Sparse | Stone/Grass |
| Ocean | Any | Any | Very Low | Any | None | Water |
| Beach | Any | Any | ~0 (coast) | High | None | Sand |

### Temperature Thresholds

| Category | Noise Value Range |
|----------|-------------------|
| Freezing | < -0.45 |
| Cold | -0.45 to -0.15 |
| Temperate | -0.15 to 0.2 |
| Warm | 0.2 to 0.55 |
| Hot | > 0.55 |

### Humidity Thresholds

| Category | Noise Value Range |
|----------|-------------------|
| Arid | < -0.35 |
| Dry | -0.35 to -0.1 |
| Neutral | -0.1 to 0.1 |
| Wet | 0.1 to 0.3 |
| Humid | > 0.3 |

---

## Diagrams Reference

### Terrain Generation Flow (1.18+)

```
                    ┌─────────────┐
                    │ Multinoise  │
                    └──────┬──────┘
                           │
         ┌─────────────────┼─────────────────┐
         │                 │                 │
         ▼                 ▼                 ▼
   ┌───────────┐    ┌───────────┐    ┌───────────┐
   │Temperature│    │ Humidity  │    │Continent- │
   │ (3D fBm)  │    │ (3D fBm)  │    │  alness   │
   └─────┬─────┘    └─────┬─────┘    │ (2D fBm)  │
         │                │          └─────┬─────┘
         │                │                │
         ▼                ▼                ▼
         └────────┬───────┴────────┬───────┘
                  │                │
                  ▼                ▼
           ┌────────────┐   ┌────────────┐
           │ 3D Biome   │   │   Depth    │◄── Erosion (2D fBm)
           │    Map     │   │            │◄── Weirdness (2D fBm)
           └─────┬──────┘   └─────┬──────┘
                 │                │
                 └───────┬────────┘
                         │
                         ▼
                 ┌───────────────┐
                 │Terrain Height │
                 └───────────────┘
```

### Cave Generation Types

```
┌─────────────────┬─────────────────┬─────────────────┐
│  CHEESE CAVES   │ SPAGHETTI CAVES │  NOODLE CAVES   │
├─────────────────┼─────────────────┼─────────────────┤
│                 │                 │                 │
│   ░░░░░░░░░░    │     ═══════     │    ─┬─┬─┬─      │
│  ░░░    ░░░░    │    ╱       ╲    │    ─┼─┼─┼─      │
│ ░░        ░░    │   ╱    ═════    │    ─┼─┼─┼─      │
│  ░░░    ░░░     │  ═════╱         │    ─┴─┴─┴─      │
│   ░░░░░░░░      │       ╲═════    │                 │
│                 │                 │                 │
│ Large irregular │  Long winding   │ Fine network    │
│    caverns      │    tunnels      │  of passages    │
│                 │                 │                 │
│ Low frequency   │ Medium freq,    │ High frequency  │
│ threshold noise │ dual noise      │ dual noise      │
│                 │ intersection    │ intersection    │
└─────────────────┴─────────────────┴─────────────────┘
```

---

*This specification represents Minecraft's terrain generation as of version 1.18+. The system continues to evolve, but these core principles form the foundation of modern voxel world generation.*

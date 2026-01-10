---
name: layer-viz
description: >
  Generate professional HTML infographics showing depth-dependent features across all voxel world biomes.
  Creates interactive visualization with layer structure, vegetation, light sources, and cave decorations.
  Triggers: "layer visualization", "biome layers", "/layer-viz"
---

# Layer Visualization Generator

Generate professional HTML infographics showing depth-dependent features across all voxel world biomes.

## Usage

```
/layer-viz
```

Generates a single interactive HTML file with tabs for all biomes.

## What it does

Creates `layer_viz.html` with interactive visualization showing:

**Overview Tab:**
- 🌍 **Biome Distribution**: All biomes at-a-glance comparison
- **Elevation Bar Chart**: Visual comparison of biome surface heights
- **Cave Water Filling**: Complete reference for all biomes (water, ice, dry)
- **Lava Distribution**: Y: 2-10 lakes in all biomes, Y: 2-75 in mountains
- **Mountain Snow Caps**: Stone surfaces above Y: 155 convert to snow
- **Key Depth Markers**: Important Y-levels (bedrock Y:0, lava Y:2-10, sea level Y:75, snow line Y:155)

**Layer Structure (Per-Biome):**
- World height and layer structure (Y: 0-511)
- Sea level and surface height for each biome
- Explorable dry cave zones (above sea level)
- Water-filled cave zones (below sea level)
- Biome-specific features (lava lakes, dry caves, flooding)
- Color-coded visualization with hover effects
- Statistical overview of explorable vs flooded space

**Depth-Dependent Features:**
- 🌳 **Vegetation**: Tree types, heights, and undergrowth by biome
- 💡 **Natural Light Sources**: Glowstone, crystals, glowmushroom spawn zones
- 🏔️ **Cave Decorations**: Stalactites, stalagmites, icicles distribution

**Interactive Features:**
- Tab-based switching with emoji icons (🌍 🌱 ⛰️ 🏜️ 🌿 ❄️)
- Smooth fade-in animations
- Hover effects on all layers and elevation bars
- Biome-specific color coding

## Biome Details

**Grassland** 🌱: Y: ~132, oak trees (8-12 tall), always dry caves, 122 blocks explorable (Y: 11-132)
**Mountains** ⛰️: Y: ~165, pine trees (12-18 tall), **snow caps above Y: 155 (stone→snow)**, lava/water Y: 11-75, dry Y: 76-165, 90 blocks dry caves
**Desert** 🏜️: Y: ~130, sparse cacti, always dry - ALL caves explorable, 120 blocks (Y: 11-130)
**Swamp** 🌿: Y: ~129, willow trees (10-14 tall with hollow draping canopies), heavily flooded below Y: 80, 49 blocks dry caves
**Snow** ❄️: Y: ~148, **snow-covered pines (6%, 8-14 tall) + dead trees (2%, 4-8 tall)**, **fully icy underground (4-block subsurface + all deep stone = ice)**, ice caves with air interior and ice walls at all depths, 138 blocks explorable caves (Y: 11-148)

**Universal:** Bedrock at Y: 0 (single unbreakable layer), lava lakes Y: 2-10 (all biomes, noise-based pockets)

---

## Implementation

When invoked, run the Python script:

```bash
python3 scripts/generate_layer_viz.py
```

The script is located at `scripts/generate_layer_viz.py` and contains full biome data with:
- Layer structure and cave water filling
- Vegetation types and heights
- Natural light source distributions
- Cave decoration details

See `scripts/README.md` for detailed documentation.

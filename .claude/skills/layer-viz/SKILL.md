# Layer Visualization Generator

Generate professional HTML infographics showing depth-dependent features across all voxel world biomes.

## Usage

```
/layer-viz
```

Generates a single interactive HTML file with tabs for all biomes.

## What it does

Creates `layer_viz.html` with interactive visualization showing:

**Overview Tab (NEW):**
- 🌍 **Biome Distribution**: All biomes at-a-glance comparison
- **Elevation Bar Chart**: Visual comparison of biome surface heights
- **Cave Water Filling**: Complete reference for all biomes
- **Lava Distribution**: Y: 3-7 pockets in all biomes, Y<100 lakes in mountains
- **Key Depth Markers**: Important Y-levels (bedrock, lava zone, sea level)

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

## Biome Details (Updated)

**Grassland** 🌱: Y: ~132, oak trees (8-12 tall), always dry caves, 125 blocks explorable
**Mountains** ⛰️: Y: ~165, pine trees (12-18 tall), lava Y<100, water below Y: 124, 41 blocks dry caves
**Desert** 🏜️: Y: ~130, sparse cacti, always dry - ALL caves explorable, 123 blocks
**Swamp** 🌿: Y: ~129, willow trees (10-14 tall), heavily flooded below Y: 129, 0 blocks dry caves
**Snow** ❄️: Y: ~148, sparse pines, ice caves (~60% ice-filled), 141 blocks explorable

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

# Layer Visualization Generator

Generate professional HTML infographics showing depth-dependent features across all voxel world biomes.

## Usage

```
/layer-viz
```

Generates a single interactive HTML file with tabs for all biomes.

## What it does

Creates `layer_viz.html` with interactive visualization showing:

**Layer Structure:**
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
- Tab-based biome switching with emoji icons (🌱 ⛰️ 🏜️ 🌿 ❄️)
- Smooth fade-in animations
- Hover effects on all layers
- Biome-specific color coding

## Biome Details

**Grassland** 🌱: Oak trees (8-12 tall), rare glowstone/crystals, 4 blocks dry caves
**Mountains** ⛰️: Pine trees (12-18 tall), lava below Y=100, abundant crystals, 30 blocks dry caves
**Desert** 🏜️: Sparse cacti, exposed crystals, 125 blocks ALL dry caves
**Swamp** 🌿: Willow trees (10-14 tall), glowmushrooms, underwater crystals, 0 blocks dry caves
**Snow** ❄️: Sparse pine trees, ice crystals + icicles, 15 blocks dry caves

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

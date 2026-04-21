# Scripts

Utility scripts for voxel world development and visualization.

## generate_layer_viz.py

Generate professional HTML infographics showing depth-dependent features across all voxel world biomes.

### Usage

```bash
# Generate interactive visualization for all biomes
python3 scripts/generate_layer_viz.py
```

### Output

Creates `layer_viz.html` in the project root with interactive tabs for all biomes showing:

**Layer Structure:**
- World height and layer structure (Y: 0-511)
- Sea level and biome-specific surface heights
- Explorable dry cave zones (green)
- Water-filled cave zones (blue/red)
- Lava zones for mountains (orange)
- Statistical overview of explorable vs flooded space

**Depth-Dependent Features:**
- 🌳 **Vegetation**: Tree types, heights, and undergrowth by biome
- 💡 **Natural Light Sources**: Glowstone, crystals, and glowmushroom spawning zones
- 🏔️ **Cave Decorations**: Stalactites, stalagmites, icicles distribution

**Interactive Features:**
- Tab-based biome switching with emoji icons
- Hover effects on all layers
- Color-coded depth zones
- Biome-specific badges

### Biome Characteristics

| Biome | Surface Y | Dry Caves | Vegetation | Light Sources | Cave Features |
|-------|-----------|-----------|------------|---------------|---------------|
| Grassland 🌱 | ~128 | 121 blocks | Oak trees (8-12 tall) | Rare glowstone, crystals | All dry, lava Y: 3-7 |
| Mountains ⛰️ | ~155 | 30 blocks | Pine trees (12-18 tall) | Abundant crystals in peaks | Lava Y: 3-7 & <100, water below sea level |
| Desert 🏜️ | ~128 | 121 blocks | Sparse cacti | Common exposed crystals | All dry, lava Y: 3-7 |
| Swamp 🌿 | ~124 | 0 blocks | Willow trees (10-14 tall) | Glowmushrooms, underwater | Flooded to Y: 129, lava Y: 3-7 |
| Snow ❄️ | ~140 | 133 blocks | Sparse pine trees | Ice crystals abundant | Ice caves throughout, lava Y: 3-7 |

**Cave Filling Rules:**
- All biomes: Possible lava pockets at Y: 3-7 (just above bedrock)
- Grassland: No water, all caves dry and explorable
- Mountains: Lava lakes below Y: 100, water below sea level (Y: 124)
- Desert: No water, all caves dry (most explorable)
- Swamp: Water flooding up to Y: 129 (5 blocks above sea level)
- Snow: No water, ice caves possible anywhere

### Claude Code Skill

You can also invoke this using the Claude Code skill:

```
/layer-viz grassland
/layer-viz mountains
/layer-viz all
```

See `.claude/skills/layer-viz.md` for details.

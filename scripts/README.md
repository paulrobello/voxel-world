# Scripts

Utility scripts for voxel world development and visualization.

## generate_layer_viz.py

Generate professional HTML infographics showing vertical layer structure for voxel world biomes.

### Usage

```bash
# Generate for a specific biome
python3 scripts/generate_layer_viz.py grassland
python3 scripts/generate_layer_viz.py mountains
python3 scripts/generate_layer_viz.py desert
python3 scripts/generate_layer_viz.py swamp
python3 scripts/generate_layer_viz.py snow

# Generate for all biomes at once
python3 scripts/generate_layer_viz.py all
```

### Output

Creates `layer_viz_{biome}.html` files in the project root showing:
- World height and layer structure (Y: 0-511)
- Sea level and biome-specific surface heights
- Explorable dry cave zones (green)
- Water-filled cave zones (blue/red)
- Lava zones for mountains (orange)
- Statistical overview of explorable vs flooded space
- Interactive hover effects

### Biome Characteristics

| Biome | Surface Y | Explorable Dry Caves | Special Features |
|-------|-----------|---------------------|------------------|
| Grassland | ~128 | 4 blocks (Y: 125-128) | Standard terrain |
| Mountains | ~155 | 30 blocks (Y: 125-155) | Lava lakes below Y: 100 |
| Desert | ~128 | 125 blocks (Y: 3-128) | ALL caves dry - no water |
| Swamp | ~124 | 0 blocks | Flooded to Y: 129 |
| Snow | ~140 | 15 blocks (Y: 125-140) | Tundra and peaks |

### Claude Code Skill

You can also invoke this using the Claude Code skill:

```
/layer-viz grassland
/layer-viz mountains
/layer-viz all
```

See `.claude/skills/layer-viz.md` for details.

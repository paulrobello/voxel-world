---
name: layer-viz
description: >
  Generate professional HTML infographics for all current voxel world biomes (15 surface + 3 underground).
  Shows layer structure, elevation bands, vegetation, cave fill, light sources, and decorations.
  Triggers: "layer visualization", "biome layers", "/layer-viz"
---

# Layer Visualization Generator

Generate professional HTML infographics showing depth-dependent features across every biome.

## Usage

```
/layer-viz
```

Generates a single interactive HTML file with tabs for all biomes.

## What it does

Creates `layer_viz.html` with interactive visualization showing:

**Overview Tab:**
- 🌍 Biome distribution across all 18 biomes (grouped by category)
- Elevation reference: world height, sea level, lava zone, snow line
- Cave fill guide: dry vs flooded vs ice vs lava by biome
- Underground spread: Lush/Dripstone (Y:11-74), Deep Dark (Y:0-31)
- Key depth markers: bedrock Y:0, lava Y:2-10, sea level Y:75, snow line Y:155, build height Y:511

**Layer Structure (Per-Biome):**
- World height 0-511 with biome-specific surface Y
- Dry / flooded / ice caves and lava distribution by biome
- Snow caps where applicable (mountains above Y:155)
- Color-coded layers with hover details

**Depth-Dependent Features:**
- 🌳 Vegetation: tree species, height ranges, densities
- 💡 Natural light sources: glowstone, crystals, glowmushroom zones
- 🏔️ Cave decorations: stalactites, stalagmites, icicles distribution

**Interactive Features:**
- Tab-based switching with emoji icons
- Smooth fade-in animations
- Hover effects on layers and elevation bars
- Biome-specific color coding

## World & Elevation Reference

- World height: 512 blocks (Y:0-511)
- Sea level: Y:75
- Lava lakes: Y:2-10 in every biome; mountains keep lava/water caves up to sea level
- Snow line: stone above Y:155 in mountains converts to snow
- Underground spans: Lush/Dripstone typically Y:11-74; Deep Dark Y:0-31
- Bedrock: single unbreakable layer at Y:0

## Biome Reference (current data from `scripts/generate_layer_viz.py`)

**Temperate**
- Plains 🌾 — Surface ~Y100; grass over dirt; oak 6-10 (5%); tall grass + red/yellow/blue flowers 10-13%; dry caves; sparse oaks & wildflowers.
- Meadow 🌸 — Surface ~Y105; grass/dirt; oak 6-10 (3%, very sparse); many flowers + tall grass 20%; dry caves; rolling hills.
- Forest 🌳 — Surface ~Y100; grass/dirt; oak 6-10 (25%); grass/ferns/blue flowers 10%; dry caves; dense canopy.
- Birch Forest 🌲 — Surface ~Y100; grass/dirt; birch 10-14 (20%); grass/ferns/blue flowers 10%; dry caves; bright white trunks.
- Dark Forest 🌑 — Surface ~Y100; grass over coarse dirt/dirt; oak 6-10 (35% very dense); mushrooms/ferns/sparse grass 18%; dry caves; low-light canopy.

**Hot & Dry**
- Desert 🏜️ — Surface ~Y100; sand over sandstone; cacti 3-6 (2%); dead bushes 3%; caves fully dry; sand dunes/sandstone subsurface.
- Savanna 🦁 — Surface ~Y100; grass/coarse dirt over coarse dirt/dirt; acacia 8-12 (6%, bent umbrellas); sparse grass + dead bushes 10%; dry caves; flat with coarse patches.

**Tropical**
- Jungle 🌴 — Surface ~Y100; grass/dirt; jungle trees 15-25 (40% dense); dense grass/ferns/flowers 28%; dry caves; towering canopy & undergrowth.
- Swamp 🐊 — Surface ~Y76 (just above sea); mud over clay/mud; willows 10-14 (12% drooping); tall grass/mushrooms/lily pads 22%; water-filled caves below Y80 (SEA+5); flooded terrain.

**Cold**
- Taiga 🌲 — Surface ~Y100; grass/podzol over dirt; pine 12-18 (18%); ferns/grass 12%; dry caves; tall pines with podzol patches.
- Snowy Taiga 🎄 — Surface ~Y100; snow over podzol/dirt; snow pines 8-14 (20%); sparse ferns/grass 6%; ice-walled caves; snow-covered branches.
- Snowy Plains ❄️ — Surface ~Y100; snow over packed ice/ice; snow pines + dead trees 6-14 (14% total: 6% pines, 8% dead); sparse grass 2%; ice caves (air interior, ice walls); frozen tundra.
- Mountains ⛰️ — Surface ~Y165; stone/gravel over stone; pines 12-18 (3%, only below Y80); sparse grass + blue flowers 6%; deep caves mix lava/water up to sea level; snow caps above Y155; exposed stone peaks.

**Aquatic**
- Ocean 🌊 — Seafloor ~Y40; sand/gravel/clay over sand/clay; no trees; seagrass 12%; caves fully submerged (water to sea level).
- Beach 🏖️ — Surface ~Y76; sand over sand/clay; no trees or vegetation; dry caves; coastal transition with clay pockets.

**Underground**
- Lush Caves 🌿 — Moss/rooted dirt over stone; bioluminescent vegetation (moss carpet, hanging roots, glow berries, glow lichen, mushrooms ~35%); dry caves; main lush zone Y:32-74, mixed lush/stone Y:11-31.
- Dripstone Caves 🗻 — Dripstone/calcite over stone; extra-dense stalactites & stalagmites; dry caves; dripstone zone Y:32-74, dense formations Y:11-31.
- Deep Dark 🕳️ — Deepslate throughout; sparse glow lichen/mushrooms (~8%); dry caves; dominant at Y:0-31.

---

## Implementation

When invoked, run the Python script:

```bash
python3 scripts/generate_layer_viz.py
```

The script at `scripts/generate_layer_viz.py` is the source of truth for biome layers, surface heights, cave fill, vegetation, light sources, and decorations. See `scripts/README.md` for more notes.

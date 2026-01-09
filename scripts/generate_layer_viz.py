#!/usr/bin/env python3
"""Generate layer visualization infographic for voxel world biomes."""

import sys
from pathlib import Path

# World constants (from src/constants.rs and src/terrain_gen.rs)
WORLD_HEIGHT = 512  # 16 chunks * 32 blocks
SEA_LEVEL = 124
CHUNK_SIZE = 32

# Biome data: (avg_surface_height, min_height, max_height, cave_fill_desc, special_notes)
BIOME_DATA = {
    "grassland": {
        "name": "Grassland",
        "surface": 128,
        "min_height": 122,
        "max_height": 134,
        "water_fill": SEA_LEVEL,
        "water_fill_desc": "Water below Y: 124",
        "dry_cave_range": "Y: 125-128",
        "explorable_blocks": 4,
        "special": None,
        "color": "#3cb371"
    },
    "mountains": {
        "name": "Mountains",
        "surface": 155,
        "min_height": 135,
        "max_height": 190,
        "water_fill": SEA_LEVEL,
        "water_fill_desc": "Water below Y: 124, Lava below Y: 100",
        "dry_cave_range": "Y: 125-155",
        "explorable_blocks": 30,
        "special": "Lava lakes at Y < 100",
        "color": "#8b7355"
    },
    "desert": {
        "name": "Desert",
        "surface": 128,
        "min_height": 126,
        "max_height": 132,
        "water_fill": None,
        "water_fill_desc": "No water - all caves dry",
        "dry_cave_range": "Y: 3-128",
        "explorable_blocks": 125,
        "special": "ALL caves explorable (no water)",
        "color": "#daa520"
    },
    "swamp": {
        "name": "Swamp",
        "surface": 124,
        "min_height": 123,
        "max_height": 126,
        "water_fill": 129,  # sea_level + 5
        "water_fill_desc": "Water below Y: 129 (5 blocks above sea level)",
        "dry_cave_range": "None",
        "explorable_blocks": 0,
        "special": "Almost no dry caves - heavily flooded",
        "color": "#556b2f"
    },
    "snow": {
        "name": "Snow",
        "surface": 140,
        "min_height": 128,
        "max_height": 170,
        "water_fill": SEA_LEVEL,
        "water_fill_desc": "Water below Y: 124",
        "dry_cave_range": "Y: 125-140",
        "explorable_blocks": 15,
        "special": "Tundra and snowy peaks",
        "color": "#e0f2f7"
    }
}


def generate_html(biome_key):
    """Generate HTML infographic for a biome."""
    biome = BIOME_DATA[biome_key]

    # Calculate layer heights for visualization
    bedrock_height = 60

    if biome["water_fill"] is None:
        # Desert - all dry
        deep_dry_height = 240
        mid_dry_height = 120
        water_height = 0
    else:
        # Calculate water-filled and dry zones
        water_blocks = biome["water_fill"] - 3  # 3 blocks of bedrock
        dry_blocks = biome["surface"] - biome["water_fill"]

        # Scale for visualization (total available: ~420px after bedrock/surface/sky)
        total_cave_px = 360
        water_height = int((water_blocks / (water_blocks + dry_blocks)) * total_cave_px) if water_blocks > 0 else 0
        dry_height = total_cave_px - water_height

        deep_dry_height = 0
        mid_dry_height = dry_height

    surface_height = 8
    above_surface_height = 40
    sky_height = 150

    # Generate badges HTML
    water_badge = '❌ WATER-FILLED' if biome["water_fill"] else ''
    dry_badge = '✅ DRY - EXPLORABLE!' if biome["explorable_blocks"] > 0 else '❌ NO DRY CAVES'
    lava_badge = '🔥 LAVA LAKES' if biome_key == "mountains" else ''

    html = f'''<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Voxel World - {biome["name"]} Biome Vertical Structure</title>
    <style>
        * {{
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }}

        body {{
            font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif;
            background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%);
            display: flex;
            justify-content: center;
            align-items: center;
            min-height: 100vh;
            padding: 20px;
        }}

        .container {{
            background: #0f1419;
            border-radius: 20px;
            padding: 40px;
            box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
            max-width: 900px;
            width: 100%;
        }}

        h1 {{
            color: #00d9ff;
            text-align: center;
            margin-bottom: 10px;
            font-size: 2.5em;
            text-shadow: 0 0 20px rgba(0, 217, 255, 0.5);
        }}

        .subtitle {{
            color: #88ccff;
            text-align: center;
            margin-bottom: 40px;
            font-size: 1.2em;
        }}

        .diagram-container {{
            display: flex;
            gap: 30px;
            margin-bottom: 30px;
        }}

        .vertical-scale {{
            display: flex;
            flex-direction: column-reverse;
            justify-content: space-between;
            width: 80px;
            position: relative;
        }}

        .scale-marker {{
            color: #aaa;
            font-size: 0.9em;
            text-align: right;
            padding-right: 10px;
            font-weight: bold;
            position: relative;
        }}

        .scale-marker::after {{
            content: '';
            position: absolute;
            right: 0;
            top: 50%;
            width: 8px;
            height: 2px;
            background: #666;
        }}

        .layers {{
            flex: 1;
            display: flex;
            flex-direction: column-reverse;
            border: 2px solid #444;
            border-radius: 10px;
            overflow: hidden;
            box-shadow: 0 0 30px rgba(0, 0, 0, 0.7);
        }}

        .layer {{
            position: relative;
            display: flex;
            align-items: center;
            justify-content: flex-start;
            padding-left: 20px;
            transition: all 0.3s ease;
            cursor: pointer;
        }}

        .layer:hover {{
            filter: brightness(1.2);
            transform: translateX(-5px);
        }}

        .layer-label {{
            color: white;
            font-weight: bold;
            text-shadow: 2px 2px 4px rgba(0, 0, 0, 0.8);
            z-index: 1;
            font-size: 1.1em;
            padding: 10px;
            text-align: left;
        }}

        .layer-details {{
            font-size: 0.85em;
            color: rgba(255, 255, 255, 0.9);
            text-shadow: 1px 1px 2px rgba(0, 0, 0, 0.8);
        }}

        .badge {{
            position: absolute;
            right: 10px;
            top: 50%;
            transform: translateY(-50%);
            padding: 5px 15px;
            border-radius: 20px;
            font-weight: bold;
            font-size: 0.9em;
            z-index: 2;
        }}

        .badge-green {{
            background: #22c55e;
            color: white;
            box-shadow: 0 0 10px rgba(34, 197, 94, 0.5);
        }}

        .badge-red {{
            background: #ef4444;
            color: white;
            box-shadow: 0 0 10px rgba(239, 68, 68, 0.5);
        }}

        .badge-orange {{
            background: #f97316;
            color: white;
            box-shadow: 0 0 10px rgba(249, 115, 22, 0.5);
        }}

        .sky {{
            height: {sky_height}px;
            background: linear-gradient(180deg, #87ceeb 0%, #b0d9f1 100%);
        }}

        .above-surface {{
            height: {above_surface_height}px;
            background: linear-gradient(180deg, #b0d9f1 0%, #90ee90 100%);
            border-bottom: 3px solid #228b22;
        }}

        .surface {{
            height: {surface_height}px;
            background: linear-gradient(180deg, {biome["color"]} 0%, {biome["color"]}dd 100%);
            border-bottom: 2px solid {biome["color"]}88;
        }}

        .dry-caves {{
            background: linear-gradient(180deg, #8b7355 0%, #6b5344 100%);
        }}

        .water-caves {{
            background: repeating-linear-gradient(
                45deg,
                #4682b4,
                #4682b4 10px,
                #5a9fd4 10px,
                #5a9fd4 20px
            );
        }}

        .lava-caves {{
            background: repeating-linear-gradient(
                45deg,
                #dc2626,
                #dc2626 10px,
                #ef4444 10px,
                #ef4444 20px
            );
        }}

        .deep-caves {{
            background: repeating-linear-gradient(
                45deg,
                #1e3a8a,
                #1e3a8a 10px,
                #2563eb 10px,
                #2563eb 20px
            );
        }}

        .bedrock {{
            height: {bedrock_height}px;
            background: linear-gradient(135deg, #1a1a1a 0%, #2d2d2d 25%, #1a1a1a 50%, #2d2d2d 75%, #1a1a1a 100%);
            border-top: 3px solid #000;
        }}

        .stats {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 15px;
            margin-bottom: 30px;
        }}

        .stat-box {{
            background: linear-gradient(135deg, #1e3a8a 0%, #3b82f6 100%);
            padding: 15px;
            border-radius: 10px;
            border: 2px solid #60a5fa;
            text-align: center;
        }}

        .stat-label {{
            color: #93c5fd;
            font-size: 0.9em;
            margin-bottom: 5px;
        }}

        .stat-value {{
            color: white;
            font-size: 1.5em;
            font-weight: bold;
        }}

        .key-insight {{
            margin-top: 30px;
            padding: 20px;
            background: linear-gradient(135deg, #fbbf24 0%, #f59e0b 100%);
            border-radius: 10px;
            border-left: 5px solid #d97706;
            color: #1a1a1a;
        }}

        .key-insight h3 {{
            margin-bottom: 10px;
            font-size: 1.3em;
        }}

        .key-insight p {{
            line-height: 1.6;
            font-size: 1.05em;
        }}
    </style>
</head>
<body>
    <div class="container">
        <h1>Voxel World Layer Structure</h1>
        <div class="subtitle">{biome["name"]} Biome - Vertical Cross-Section</div>

        <div class="stats">
            <div class="stat-box">
                <div class="stat-label">Total World Height</div>
                <div class="stat-value">{WORLD_HEIGHT} blocks</div>
            </div>
            <div class="stat-box">
                <div class="stat-label">Sea Level</div>
                <div class="stat-value">Y: {SEA_LEVEL}</div>
            </div>
            <div class="stat-box">
                <div class="stat-label">{biome["name"]} Surface</div>
                <div class="stat-value">Y: ~{biome["surface"]}</div>
            </div>
            <div class="stat-box">
                <div class="stat-label">Explorable Cave Space</div>
                <div class="stat-value">{biome["explorable_blocks"]} blocks</div>
            </div>
        </div>

        <div class="diagram-container">
            <div class="vertical-scale">
                <div class="scale-marker">Y: 0</div>
                <div class="scale-marker">Y: 50</div>
                <div class="scale-marker">Y: 100</div>
                <div class="scale-marker" style="color: #3b82f6; font-weight: bold;">Y: {SEA_LEVEL}</div>
                <div class="scale-marker" style="color: #22c55e; font-weight: bold;">Y: {biome["surface"]}</div>
                <div class="scale-marker">Y: {biome["surface"] + 1}</div>
                <div class="scale-marker">Y: 200</div>
                <div class="scale-marker">Y: 300</div>
                <div class="scale-marker">Y: 400</div>
                <div class="scale-marker">Y: 511</div>
            </div>

            <div class="layers">
                <div class="layer bedrock">
                    <div class="layer-label">Bedrock Layer&nbsp;&nbsp;<span class="layer-details">(Y: 0-2)</span></div>
                </div>
'''

    # Add cave layers based on biome
    if biome_key == "desert":
        # Desert: all caves are dry
        html += f'''                <div class="layer dry-caves" style="height: {deep_dry_height}px;">
                    <div class="layer-label">Deep Caves&nbsp;&nbsp;<span class="layer-details">(Y: 3-99)</span></div>
                    <div class="badge badge-green">{dry_badge}</div>
                </div>
                <div class="layer dry-caves" style="height: {mid_dry_height}px;">
                    <div class="layer-label">Mid Caves&nbsp;&nbsp;<span class="layer-details">(Y: 100-{biome["surface"]})</span></div>
                    <div class="badge badge-green">{dry_badge}</div>
                </div>
'''
    elif biome_key == "mountains":
        # Mountains: lava below 100, water 100-124, dry above
        html += f'''                <div class="layer lava-caves" style="height: 120px;">
                    <div class="layer-label">Deep Caves (Lava)&nbsp;&nbsp;<span class="layer-details">(Y: 3-99)</span></div>
                    <div class="badge badge-orange">🔥 LAVA LAKES</div>
                </div>
                <div class="layer water-caves" style="height: 100px;">
                    <div class="layer-label">Mid Caves&nbsp;&nbsp;<span class="layer-details">(Y: 100-{SEA_LEVEL})</span></div>
                    <div class="badge badge-red">{water_badge}</div>
                </div>
                <div class="layer dry-caves" style="height: {mid_dry_height}px;">
                    <div class="layer-label">Near-Surface Caves&nbsp;&nbsp;<span class="layer-details">(Y: {SEA_LEVEL+1}-{biome["surface"]})</span></div>
                    <div class="badge badge-green">{dry_badge}</div>
                </div>
'''
    elif biome_key == "swamp":
        # Swamp: water all the way to surface (129)
        html += f'''                <div class="layer deep-caves" style="height: 240px;">
                    <div class="layer-label">Deep Caves&nbsp;&nbsp;<span class="layer-details">(Y: 3-99)</span></div>
                    <div class="badge badge-red">{water_badge}</div>
                </div>
                <div class="layer water-caves" style="height: 120px;">
                    <div class="layer-label">Mid Caves&nbsp;&nbsp;<span class="layer-details">(Y: 100-{biome["water_fill"]})</span></div>
                    <div class="badge badge-red">{water_badge}</div>
                </div>
'''
    else:
        # Grassland, Snow: standard water below sea level, dry above
        html += f'''                <div class="layer deep-caves" style="height: 240px;">
                    <div class="layer-label">Deep Caves&nbsp;&nbsp;<span class="layer-details">(Y: 3-99)</span></div>
                    <div class="badge badge-red">{water_badge}</div>
                </div>
                <div class="layer water-caves" style="height: 120px;">
                    <div class="layer-label">Mid Caves&nbsp;&nbsp;<span class="layer-details">(Y: 100-{SEA_LEVEL})</span></div>
                    <div class="badge badge-red">{water_badge}</div>
                </div>
                <div class="layer dry-caves" style="height: {mid_dry_height}px;">
                    <div class="layer-label">Near-Surface Caves&nbsp;&nbsp;<span class="layer-details">(Y: {SEA_LEVEL+1}-{biome["surface"]})</span></div>
                    <div class="badge badge-green">{dry_badge}</div>
                </div>
'''

    # Add surface and sky
    html += f'''                <div class="layer surface">
                    <div class="layer-label" style="font-size: 0.9em;">Surface&nbsp;&nbsp;<span class="layer-details">(Y: ~{biome["surface"]})</span></div>
                </div>
                <div class="layer above-surface">
                    <div class="layer-label">Above Surface&nbsp;&nbsp;<span class="layer-details">(Y: {biome["surface"]+1}-200)</span></div>
                </div>
                <div class="layer sky">
                    <div class="layer-label">Sky&nbsp;&nbsp;<span class="layer-details">(Y: 201-511)</span></div>
                </div>
            </div>
        </div>

        <div class="key-insight">
            <h3>🔍 Key Insight - {biome["name"]} Biome</h3>
            <p>
                {biome["water_fill_desc"]}<br>
                <strong>Dry cave range:</strong> {biome["dry_cave_range"]}
                (<strong>{biome["explorable_blocks"]} blocks</strong> of explorable vertical space)
'''

    if biome["special"]:
        html += f'<br><strong>Special feature:</strong> {biome["special"]}'

    html += '''
            </p>
        </div>
    </div>
</body>
</html>
'''

    return html


def main():
    """Main entry point."""
    biome = sys.argv[1] if len(sys.argv) > 1 else "grassland"

    if biome == "all":
        # Generate for all biomes
        for biome_key in BIOME_DATA.keys():
            html = generate_html(biome_key)
            output_file = Path(f"layer_viz_{biome_key}.html")
            output_file.write_text(html, encoding="utf-8")
            print(f"✅ Generated {output_file}")
    else:
        biome = biome.lower()
        if biome not in BIOME_DATA:
            print(f"❌ Unknown biome: {biome}")
            print(f"Available: {', '.join(BIOME_DATA.keys())}, all")
            sys.exit(1)

        html = generate_html(biome)
        output_file = Path(f"layer_viz_{biome}.html")
        output_file.write_text(html, encoding="utf-8")
        print(f"✅ Generated {output_file}")
        print(f"📊 {BIOME_DATA[biome]['name']} biome: {BIOME_DATA[biome]['explorable_blocks']} blocks of dry caves")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Generate layer visualization infographic for voxel world biomes."""

from pathlib import Path

# World constants (from src/constants.rs and src/terrain_gen.rs)
WORLD_HEIGHT = 512  # 16 chunks * 32 blocks
SEA_LEVEL = 124
CHUNK_SIZE = 32

# Biome data
BIOME_DATA = {
    "grassland": {
        "name": "Grassland",
        "surface": 128,
        "water_fill": SEA_LEVEL,
        "water_fill_desc": "Water below Y: 124",
        "dry_cave_range": "Y: 125-128",
        "explorable_blocks": 4,
        "special": None,
        "color": "#3cb371",
        "icon": "🌱",
        "vegetation": {
            "tree_type": "Oak trees",
            "tree_height": "8-12 blocks",
            "undergrowth": "Grass, flowers (Y: 128-129)"
        },
        "light_sources": {
            "glowstone": "Rare in caves (Y: 20-100)",
            "crystal": "Occasional clusters (Y: 10-80)"
        },
        "decorations": {
            "stalactites": "Common in caves (Y: 30-120)",
            "stalagmites": "Common in caves (Y: 30-120)"
        }
    },
    "mountains": {
        "name": "Mountains",
        "surface": 155,
        "water_fill": SEA_LEVEL,
        "water_fill_desc": "Water below Y: 124, Lava below Y: 100",
        "dry_cave_range": "Y: 125-155",
        "explorable_blocks": 30,
        "special": "Lava lakes at Y < 100",
        "color": "#8b7355",
        "icon": "⛰️",
        "vegetation": {
            "tree_type": "Pine trees",
            "tree_height": "12-18 blocks (tall)",
            "undergrowth": "Sparse grass (Y: 155-156)"
        },
        "light_sources": {
            "glowstone": "Common near lava (Y: 10-100)",
            "crystal": "Abundant in peaks (Y: 140-160)"
        },
        "decorations": {
            "stalactites": "Very common (Y: 30-155)",
            "stalagmites": "Very common (Y: 30-155)"
        }
    },
    "desert": {
        "name": "Desert",
        "surface": 128,
        "water_fill": None,
        "water_fill_desc": "No water - all caves dry",
        "dry_cave_range": "Y: 3-128",
        "explorable_blocks": 125,
        "special": "ALL caves explorable (no water)",
        "color": "#daa520",
        "icon": "🏜️",
        "vegetation": {
            "tree_type": "None (sparse cacti)",
            "tree_height": "N/A",
            "undergrowth": "Dead bushes (Y: 128)"
        },
        "light_sources": {
            "glowstone": "Very rare (Y: 20-100)",
            "crystal": "Common, exposed in caves (Y: 10-120)"
        },
        "decorations": {
            "stalactites": "Minimal (Y: 50-120)",
            "stalagmites": "Minimal (Y: 50-120)"
        }
    },
    "swamp": {
        "name": "Swamp",
        "surface": 124,
        "water_fill": 129,
        "water_fill_desc": "Water below Y: 129 (5 blocks above sea level)",
        "dry_cave_range": "None",
        "explorable_blocks": 0,
        "special": "Almost no dry caves - heavily flooded",
        "color": "#556b2f",
        "icon": "🌿",
        "vegetation": {
            "tree_type": "Willow trees",
            "tree_height": "10-14 blocks (drooping)",
            "undergrowth": "Lily pads, mushrooms (Y: 124-130)"
        },
        "light_sources": {
            "glowstone": "Rare underwater (Y: 20-100)",
            "glowmushroom": "Common near water (Y: 110-130)",
            "crystal": "Underwater clusters (Y: 10-120)"
        },
        "decorations": {
            "stalactites": "Dripping with water (Y: 30-129)",
            "stalagmites": "Submerged (Y: 30-129)"
        }
    },
    "snow": {
        "name": "Snow",
        "surface": 140,
        "water_fill": SEA_LEVEL,
        "water_fill_desc": "Water below Y: 124",
        "dry_cave_range": "Y: 125-140",
        "explorable_blocks": 15,
        "special": "Tundra and snowy peaks",
        "color": "#e0f2f7",
        "icon": "❄️",
        "vegetation": {
            "tree_type": "Pine trees (sparse)",
            "tree_height": "10-15 blocks",
            "undergrowth": "Snow-covered grass (Y: 140-141)"
        },
        "light_sources": {
            "glowstone": "Rare in ice caves (Y: 20-100)",
            "crystal": "Ice crystals abundant (Y: 80-140)"
        },
        "decorations": {
            "stalactites": "Ice stalactites common (Y: 30-140)",
            "stalagmites": "Ice stalagmites common (Y: 30-140)",
            "icicles": "Hanging from ceilings (Y: 125-140)"
        }
    }
}


def generate_biome_content(biome_key):
    """Generate HTML content for a single biome."""
    biome = BIOME_DATA[biome_key]

    water_badge = '❌ WATER-FILLED' if biome["water_fill"] else ''
    dry_badge = '✅ DRY - EXPLORABLE!' if biome["explorable_blocks"] > 0 else '❌ NO DRY CAVES'

    # Build layers HTML
    layers_html = ''

    # Bedrock
    layers_html += '''
                <div class="layer bedrock">
                    <div class="layer-label">Bedrock Layer&nbsp;&nbsp;<span class="layer-details">(Y: 0-2)</span></div>
                </div>
'''

    # Cave layers
    if biome_key == "desert":
        layers_html += f'''
                <div class="layer dry-caves" style="height: 240px;">
                    <div class="layer-label">Deep Caves&nbsp;&nbsp;<span class="layer-details">(Y: 3-99)</span></div>
                    <div class="badge badge-green">{dry_badge}</div>
                </div>
                <div class="layer dry-caves" style="height: 120px;">
                    <div class="layer-label">Mid Caves&nbsp;&nbsp;<span class="layer-details">(Y: 100-{biome["surface"]})</span></div>
                    <div class="badge badge-green">{dry_badge}</div>
                </div>
'''
    elif biome_key == "mountains":
        layers_html += f'''
                <div class="layer lava-caves" style="height: 120px;">
                    <div class="layer-label">Deep Caves (Lava)&nbsp;&nbsp;<span class="layer-details">(Y: 3-99)</span></div>
                    <div class="badge badge-orange">🔥 LAVA LAKES</div>
                </div>
                <div class="layer water-caves" style="height: 100px;">
                    <div class="layer-label">Mid Caves&nbsp;&nbsp;<span class="layer-details">(Y: 100-{SEA_LEVEL})</span></div>
                    <div class="badge badge-red">{water_badge}</div>
                </div>
                <div class="layer dry-caves" style="height: 140px;">
                    <div class="layer-label">Near-Surface Caves&nbsp;&nbsp;<span class="layer-details">(Y: {SEA_LEVEL+1}-{biome["surface"]})</span></div>
                    <div class="badge badge-green">{dry_badge}</div>
                </div>
'''
    elif biome_key == "swamp":
        layers_html += f'''
                <div class="layer deep-caves" style="height: 240px;">
                    <div class="layer-label">Deep Caves&nbsp;&nbsp;<span class="layer-details">(Y: 3-99)</span></div>
                    <div class="badge badge-red">{water_badge}</div>
                </div>
                <div class="layer water-caves" style="height: 120px;">
                    <div class="layer-label">Mid Caves&nbsp;&nbsp;<span class="layer-details">(Y: 100-{biome["water_fill"]})</span></div>
                    <div class="badge badge-red">{water_badge}</div>
                </div>
'''
    else:  # grassland, snow
        layers_html += f'''
                <div class="layer deep-caves" style="height: 240px;">
                    <div class="layer-label">Deep Caves&nbsp;&nbsp;<span class="layer-details">(Y: 3-99)</span></div>
                    <div class="badge badge-red">{water_badge}</div>
                </div>
                <div class="layer water-caves" style="height: 120px;">
                    <div class="layer-label">Mid Caves&nbsp;&nbsp;<span class="layer-details">(Y: 100-{SEA_LEVEL})</span></div>
                    <div class="badge badge-red">{water_badge}</div>
                </div>
                <div class="layer dry-caves" style="height: 60px;">
                    <div class="layer-label">Near-Surface Caves&nbsp;&nbsp;<span class="layer-details">(Y: {SEA_LEVEL+1}-{biome["surface"]})</span></div>
                    <div class="badge badge-green">{dry_badge}</div>
                </div>
'''

    # Surface and sky
    layers_html += f'''
                <div class="layer surface" style="background: linear-gradient(180deg, {biome["color"]} 0%, {biome["color"]}dd 100%); border-bottom: 2px solid {biome["color"]}88;">
                    <div class="layer-label" style="font-size: 0.9em;">Surface&nbsp;&nbsp;<span class="layer-details">(Y: ~{biome["surface"]})</span></div>
                </div>
                <div class="layer above-surface">
                    <div class="layer-label">Above Surface&nbsp;&nbsp;<span class="layer-details">(Y: {biome["surface"]+1}-200)</span></div>
                </div>
                <div class="layer sky">
                    <div class="layer-label">Sky&nbsp;&nbsp;<span class="layer-details">(Y: 201-511)</span></div>
                </div>
'''

    special_html = f'<br><strong>Special feature:</strong> {biome["special"]}' if biome["special"] else ''

    content = f'''
        <div class="biome-content" id="content-{biome_key}">
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
                    {layers_html}
                </div>
            </div>

            <div class="key-insight">
                <h3>🔍 Cave Exploration - {biome["name"]} Biome</h3>
                <p>
                    {biome["water_fill_desc"]}<br>
                    <strong>Dry cave range:</strong> {biome["dry_cave_range"]}
                    (<strong>{biome["explorable_blocks"]} blocks</strong> of explorable vertical space){special_html}
                </p>
            </div>

            <div class="features-grid">
                <div class="feature-box">
                    <h3>🌳 Vegetation</h3>
                    <p><strong>Trees:</strong> {biome["vegetation"]["tree_type"]}</p>
                    <p><strong>Height:</strong> {biome["vegetation"]["tree_height"]}</p>
                    <p><strong>Undergrowth:</strong> {biome["vegetation"]["undergrowth"]}</p>
                </div>

                <div class="feature-box">
                    <h3>💡 Natural Light Sources</h3>'''

    # Add light sources dynamically
    light_sources_html = ''
    for source, desc in biome["light_sources"].items():
        emoji = "✨" if source == "glowstone" else "🔮" if source == "crystal" else "🍄"
        light_sources_html += f'''
                    <p><strong>{emoji} {source.title()}:</strong> {desc}</p>'''

    content += light_sources_html + '''
                </div>

                <div class="feature-box">
                    <h3>🏔️ Cave Decorations</h3>'''

    # Add decorations dynamically
    decorations_html = ''
    for deco, desc in biome["decorations"].items():
        emoji = "⬇️" if deco == "stalactites" else "⬆️" if deco == "stalagmites" else "🧊"
        decorations_html += f'''
                    <p><strong>{emoji} {deco.title()}:</strong> {desc}</p>'''

    content += decorations_html + '''
                </div>
            </div>
        </div>
'''
    return content


def generate_html():
    """Generate complete HTML with all biomes."""

    # Generate tabs
    tabs_html = ''
    for i, (key, biome) in enumerate(BIOME_DATA.items()):
        active = 'active' if i == 0 else ''
        tabs_html += f'''
                <button class="tab {active}" data-biome="{key}">
                    <span class="tab-icon">{biome["icon"]}</span>
                    <span class="tab-name">{biome["name"]}</span>
                </button>
'''

    # Generate content for all biomes
    contents_html = ''
    for i, key in enumerate(BIOME_DATA.keys()):
        active = 'active' if i == 0 else ''
        contents_html += generate_biome_content(key).replace('class="biome-content"', f'class="biome-content {active}"')

    html = f'''<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Voxel World - Biome Layer Structure</title>
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
            margin-bottom: 30px;
            font-size: 1.2em;
        }}

        .tabs {{
            display: flex;
            justify-content: center;
            gap: 10px;
            margin-bottom: 30px;
            flex-wrap: wrap;
        }}

        .tab {{
            background: linear-gradient(135deg, #1e3a8a 0%, #3b82f6 100%);
            border: 2px solid #60a5fa;
            border-radius: 10px;
            padding: 12px 20px;
            color: white;
            cursor: pointer;
            display: flex;
            align-items: center;
            gap: 8px;
            transition: all 0.3s ease;
            font-size: 1em;
            font-weight: bold;
        }}

        .tab:hover {{
            transform: translateY(-2px);
            box-shadow: 0 5px 15px rgba(59, 130, 246, 0.4);
        }}

        .tab.active {{
            background: linear-gradient(135deg, #f59e0b 0%, #fbbf24 100%);
            border-color: #fbbf24;
            color: #1a1a1a;
            box-shadow: 0 5px 15px rgba(251, 191, 36, 0.4);
        }}

        .tab-icon {{
            font-size: 1.2em;
        }}

        .biome-content {{
            display: none;
        }}

        .biome-content.active {{
            display: block;
            animation: fadeIn 0.3s ease;
        }}

        @keyframes fadeIn {{
            from {{ opacity: 0; transform: translateY(10px); }}
            to {{ opacity: 1; transform: translateY(0); }}
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
            height: 150px;
            background: linear-gradient(180deg, #87ceeb 0%, #b0d9f1 100%);
        }}

        .above-surface {{
            height: 40px;
            background: linear-gradient(180deg, #b0d9f1 0%, #90ee90 100%);
            border-bottom: 3px solid #228b22;
        }}

        .surface {{
            height: 8px;
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
            height: 60px;
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

        .features-grid {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
            gap: 20px;
            margin-top: 30px;
        }}

        .feature-box {{
            background: linear-gradient(135deg, #1e293b 0%, #334155 100%);
            padding: 20px;
            border-radius: 10px;
            border: 2px solid #475569;
            color: white;
        }}

        .feature-box h3 {{
            color: #60a5fa;
            margin-bottom: 15px;
            font-size: 1.2em;
            border-bottom: 2px solid #60a5fa;
            padding-bottom: 8px;
        }}

        .feature-box p {{
            margin-bottom: 10px;
            line-height: 1.5;
            font-size: 0.95em;
        }}

        .feature-box p:last-child {{
            margin-bottom: 0;
        }}

        .feature-box strong {{
            color: #93c5fd;
        }}
    </style>
</head>
<body>
    <div class="container">
        <h1>Voxel World Layer Structure</h1>
        <div class="subtitle">Depth-Dependent Features by Biome</div>

        <div class="tabs">
            {tabs_html}
        </div>

        {contents_html}
    </div>

    <script>
        // Tab switching
        document.querySelectorAll('.tab').forEach(tab => {{
            tab.addEventListener('click', () => {{
                const biome = tab.dataset.biome;

                // Update active tab
                document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
                tab.classList.add('active');

                // Update active content
                document.querySelectorAll('.biome-content').forEach(c => c.classList.remove('active'));
                document.getElementById(`content-${{biome}}`).classList.add('active');
            }});
        }});
    </script>
</body>
</html>
'''
    return html


def main():
    """Main entry point."""
    html = generate_html()
    output_file = Path("layer_viz.html")
    output_file.write_text(html, encoding="utf-8")
    print(f"✅ Generated {output_file}")
    print(f"📊 Interactive visualization with all {len(BIOME_DATA)} biomes")


if __name__ == "__main__":
    main()

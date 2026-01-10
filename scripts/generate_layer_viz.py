#!/usr/bin/env python3
"""Generate comprehensive layer visualization for all voxel world biomes.

This script creates an interactive HTML infographic showing:
- All 15 surface biomes + 3 underground biomes
- Layer structure and depth-dependent features
- Vegetation, trees, and ground cover by biome
- Cave systems and underground biome distribution
- Climate parameters and biome selection
"""

from pathlib import Path
from dataclasses import dataclass
from typing import Optional

# World constants (from src/constants.rs and src/world_gen/)
WORLD_HEIGHT = 512  # 16 chunks * 32 blocks
SEA_LEVEL = 75
CHUNK_SIZE = 32


@dataclass
class TreeInfo:
    """Tree information for a biome."""
    tree_type: str
    height: str
    density: str  # e.g., "5%" or "25%"
    special: str = ""


@dataclass
class VegetationInfo:
    """Ground cover vegetation info."""
    types: list[str]
    density: str


@dataclass
class CaveInfo:
    """Cave characteristics for a biome."""
    fill_type: str  # "dry", "water", "lava", "ice"
    water_level: Optional[int]
    special: str = ""
    cave_density: str = "normal"


@dataclass
class BiomeData:
    """Complete biome data."""
    name: str
    icon: str
    color: str
    surface_height: int
    surface_block: str
    subsurface_block: str
    trees: Optional[TreeInfo]
    vegetation: VegetationInfo
    caves: CaveInfo
    climate: dict  # temperature, humidity ranges
    special_features: list[str]
    category: str  # temperate, cold, hot, tropical, aquatic, underground


# Complete biome database from src/terrain_gen.rs and src/world_gen/
BIOMES: dict[str, BiomeData] = {
    # === TEMPERATE BIOMES ===
    "plains": BiomeData(
        name="Plains",
        icon="🌾",
        color="#90EE90",
        surface_height=100,
        surface_block="Grass",
        subsurface_block="Dirt",
        trees=TreeInfo("Oak", "6-10 blocks", "5%"),
        vegetation=VegetationInfo(["Tall grass", "Red/Yellow/Blue flowers"], "10-13%"),
        caves=CaveInfo("dry", None, "Standard caves with stalactites"),
        climate={"temp": "medium", "humidity": "medium"},
        special_features=["Sparse oak trees", "Colorful wildflowers"],
        category="temperate"
    ),
    "meadow": BiomeData(
        name="Meadow",
        icon="🌸",
        color="#98FB98",
        surface_height=105,
        surface_block="Grass",
        subsurface_block="Dirt",
        trees=TreeInfo("Oak", "6-10 blocks", "3%", "Very sparse"),
        vegetation=VegetationInfo(["Many flowers", "Tall grass"], "20%"),
        caves=CaveInfo("dry", None),
        climate={"temp": "medium", "humidity": "medium-high"},
        special_features=["Abundant wildflowers", "Rolling hills"],
        category="temperate"
    ),
    "forest": BiomeData(
        name="Forest",
        icon="🌳",
        color="#228B22",
        surface_height=100,
        surface_block="Grass",
        subsurface_block="Dirt",
        trees=TreeInfo("Oak", "6-10 blocks", "25%"),
        vegetation=VegetationInfo(["Grass", "Ferns", "Blue flowers"], "10%"),
        caves=CaveInfo("dry", None),
        climate={"temp": "medium", "humidity": "high"},
        special_features=["Dense oak canopy", "Shaded forest floor"],
        category="temperate"
    ),
    "birch_forest": BiomeData(
        name="Birch Forest",
        icon="🌲",
        color="#9ACD32",
        surface_height=100,
        surface_block="Grass",
        subsurface_block="Dirt",
        trees=TreeInfo("Birch", "10-14 blocks", "20%", "Tall thin white bark"),
        vegetation=VegetationInfo(["Grass", "Ferns", "Blue flowers"], "10%"),
        caves=CaveInfo("dry", None),
        climate={"temp": "medium", "humidity": "high"},
        special_features=["Distinctive white bark trees", "Bright canopy"],
        category="temperate"
    ),
    "dark_forest": BiomeData(
        name="Dark Forest",
        icon="🌑",
        color="#2F4F2F",
        surface_height=100,
        surface_block="Grass",
        subsurface_block="Coarse Dirt/Dirt",
        trees=TreeInfo("Oak", "6-10 blocks", "35%", "Very dense canopy"),
        vegetation=VegetationInfo(["Mushrooms", "Ferns", "Sparse grass"], "18%"),
        caves=CaveInfo("dry", None),
        climate={"temp": "medium", "humidity": "very high"},
        special_features=["Dense dark canopy", "Mushroom patches", "Low light"],
        category="temperate"
    ),

    # === HOT/DRY BIOMES ===
    "desert": BiomeData(
        name="Desert",
        icon="🏜️",
        color="#DEB887",
        surface_height=100,
        surface_block="Sand",
        subsurface_block="Sandstone",
        trees=TreeInfo("Cactus", "3-6 blocks", "2%"),
        vegetation=VegetationInfo(["Dead bushes"], "3%"),
        caves=CaveInfo("dry", None, "All caves completely dry"),
        climate={"temp": "hot", "humidity": "very low"},
        special_features=["Sand dunes", "Sandstone subsurface", "Sparse cacti"],
        category="hot"
    ),
    "savanna": BiomeData(
        name="Savanna",
        icon="🦁",
        color="#BDB76B",
        surface_height=100,
        surface_block="Grass/Coarse Dirt",
        subsurface_block="Coarse Dirt/Dirt",
        trees=TreeInfo("Acacia", "8-12 blocks", "6%", "Bent trunk, umbrella canopy"),
        vegetation=VegetationInfo(["Sparse grass", "Dead bushes"], "10%"),
        caves=CaveInfo("dry", None),
        climate={"temp": "hot", "humidity": "low"},
        special_features=["Flat terrain", "Coarse dirt patches", "Iconic acacia trees"],
        category="hot"
    ),

    # === TROPICAL BIOMES ===
    "jungle": BiomeData(
        name="Jungle",
        icon="🌴",
        color="#006400",
        surface_height=100,
        surface_block="Grass",
        subsurface_block="Dirt",
        trees=TreeInfo("Jungle", "15-25 blocks", "40%", "Very tall with large canopy"),
        vegetation=VegetationInfo(["Dense grass", "Ferns", "Flowers"], "28%"),
        caves=CaveInfo("dry", None, "Dense vegetation above caves"),
        climate={"temp": "hot", "humidity": "very high"},
        special_features=["Towering trees", "Dense undergrowth", "Lush canopy"],
        category="tropical"
    ),
    "swamp": BiomeData(
        name="Swamp",
        icon="🐊",
        color="#556B2F",
        surface_height=76,  # Just above sea level
        surface_block="Mud",
        subsurface_block="Clay/Mud",
        trees=TreeInfo("Willow", "10-14 blocks", "12%", "Drooping leaves"),
        vegetation=VegetationInfo(["Tall grass", "Mushrooms", "Lily pads"], "22%"),
        caves=CaveInfo("water", SEA_LEVEL + 5, "Flooded below Y:80"),
        climate={"temp": "medium", "humidity": "very high"},
        special_features=["Flooded terrain", "Willow trees", "Lily pads on water"],
        category="tropical"
    ),

    # === COLD BIOMES ===
    "taiga": BiomeData(
        name="Taiga",
        icon="🌲",
        color="#2E8B57",
        surface_height=100,
        surface_block="Grass/Podzol",
        subsurface_block="Dirt",
        trees=TreeInfo("Pine", "12-18 blocks", "18%"),
        vegetation=VegetationInfo(["Ferns", "Grass"], "12%"),
        caves=CaveInfo("dry", None),
        climate={"temp": "cold", "humidity": "medium"},
        special_features=["Tall pine forests", "Podzol floor patches"],
        category="cold"
    ),
    "snowy_taiga": BiomeData(
        name="Snowy Taiga",
        icon="🎄",
        color="#E0FFFF",
        surface_height=100,
        surface_block="Snow (over Podzol)",
        subsurface_block="Podzol/Dirt",
        trees=TreeInfo("Snow Pine", "8-14 blocks", "20%", "Snow-covered branches"),
        vegetation=VegetationInfo(["Sparse ferns", "Grass"], "6%"),
        caves=CaveInfo("ice", None, "Ice-walled caves"),
        climate={"temp": "freezing", "humidity": "medium"},
        special_features=["Snow-covered pines", "Frozen landscape"],
        category="cold"
    ),
    "snowy_plains": BiomeData(
        name="Snowy Plains",
        icon="❄️",
        color="#FFFAFA",
        surface_height=100,
        surface_block="Snow",
        subsurface_block="Packed Ice/Ice",
        trees=TreeInfo("Snow Pine + Dead Trees", "6-14 blocks", "14%", "6% pines, 8% dead trees"),
        vegetation=VegetationInfo(["Sparse grass"], "2%"),
        caves=CaveInfo("ice", None, "100% ice caves - air interior with ice walls"),
        climate={"temp": "freezing", "humidity": "low"},
        special_features=["Frozen tundra", "Dead trees with branches", "Ice subsurface"],
        category="cold"
    ),
    "mountains": BiomeData(
        name="Mountains",
        icon="⛰️",
        color="#808080",
        surface_height=165,
        surface_block="Stone/Gravel",
        subsurface_block="Stone",
        trees=TreeInfo("Pine", "12-18 blocks", "3%", "Only below Y:80"),
        vegetation=VegetationInfo(["Sparse grass", "Blue flowers"], "6%"),
        caves=CaveInfo("lava", SEA_LEVEL, "Lava lakes up to sea level"),
        climate={"temp": "cold", "humidity": "low"},
        special_features=["Snow caps above Y:155", "Exposed stone peaks", "Extended lava zone"],
        category="cold"
    ),

    # === AQUATIC BIOMES ===
    "ocean": BiomeData(
        name="Ocean",
        icon="🌊",
        color="#1E90FF",
        surface_height=40,  # Ocean floor
        surface_block="Sand/Gravel/Clay",
        subsurface_block="Sand/Clay",
        trees=None,
        vegetation=VegetationInfo(["Seagrass"], "12%"),
        caves=CaveInfo("water", SEA_LEVEL, "Fully submerged"),
        climate={"temp": "medium", "humidity": "n/a"},
        special_features=["Deep water", "Varied seafloor", "No surface trees"],
        category="aquatic"
    ),
    "beach": BiomeData(
        name="Beach",
        icon="🏖️",
        color="#F4A460",
        surface_height=76,
        surface_block="Sand",
        subsurface_block="Sand/Clay",
        trees=None,
        vegetation=VegetationInfo([], "0%"),
        caves=CaveInfo("dry", None),
        climate={"temp": "medium", "humidity": "high"},
        special_features=["Coastal transition", "Clay pockets"],
        category="aquatic"
    ),

    # === UNDERGROUND BIOMES ===
    "lush_caves": BiomeData(
        name="Lush Caves",
        icon="🌿",
        color="#32CD32",
        surface_height=-1,  # Underground only
        surface_block="Moss/Rooted Dirt",
        subsurface_block="Stone",
        trees=None,
        vegetation=VegetationInfo(["Moss carpet", "Hanging roots", "Glow berries", "Glow lichen", "Mushrooms"], "35%"),
        caves=CaveInfo("dry", None, "Bioluminescent vegetation"),
        climate={"temp": "medium", "humidity": "very high"},
        special_features=["Glow berry vines", "Moss carpets", "Underground oasis"],
        category="underground"
    ),
    "dripstone_caves": BiomeData(
        name="Dripstone Caves",
        icon="🗻",
        color="#8B8682",
        surface_height=-1,
        surface_block="Dripstone/Calcite",
        subsurface_block="Stone",
        trees=None,
        vegetation=VegetationInfo(["Dense stalactites", "Dense stalagmites"], "extra dense"),
        caves=CaveInfo("dry", None, "Extra dense formations"),
        climate={"temp": "medium", "humidity": "medium"},
        special_features=["Pointed dripstone", "Calcite bands", "Dripping water effects"],
        category="underground"
    ),
    "deep_dark": BiomeData(
        name="Deep Dark",
        icon="🕳️",
        color="#1a1a2e",
        surface_height=-1,
        surface_block="Deepslate",
        subsurface_block="Deepslate",
        trees=None,
        vegetation=VegetationInfo(["Sparse glow lichen", "Glow mushrooms"], "8%"),
        caves=CaveInfo("dry", None, "Ancient depths"),
        climate={"temp": "cold", "humidity": "low"},
        special_features=["Deepslate terrain", "Y < 32", "Minimal light sources"],
        category="underground"
    ),
}

# Biome category organization
CATEGORIES = {
    "temperate": {"name": "Temperate", "icon": "🌳", "biomes": ["plains", "meadow", "forest", "birch_forest", "dark_forest"]},
    "hot": {"name": "Hot & Dry", "icon": "🌵", "biomes": ["desert", "savanna"]},
    "tropical": {"name": "Tropical", "icon": "🌴", "biomes": ["jungle", "swamp"]},
    "cold": {"name": "Cold", "icon": "❄️", "biomes": ["taiga", "snowy_taiga", "snowy_plains", "mountains"]},
    "aquatic": {"name": "Aquatic", "icon": "🌊", "biomes": ["ocean", "beach"]},
    "underground": {"name": "Underground", "icon": "⛏️", "biomes": ["lush_caves", "dripstone_caves", "deep_dark"]},
}


def generate_css() -> str:
    """Generate comprehensive CSS styles."""
    return """
        * { margin: 0; padding: 0; box-sizing: border-box; }

        body {
            font-family: 'Segoe UI', system-ui, sans-serif;
            background: linear-gradient(135deg, #0f0f1a 0%, #1a1a2e 50%, #16213e 100%);
            min-height: 100vh;
            padding: 20px;
            color: #e0e0e0;
        }

        .container {
            max-width: 1200px;
            margin: 0 auto;
            background: rgba(15, 20, 30, 0.95);
            border-radius: 20px;
            padding: 30px;
            box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
            border: 1px solid rgba(100, 150, 255, 0.1);
        }

        h1 {
            text-align: center;
            font-size: 2.5em;
            background: linear-gradient(135deg, #00d9ff, #00ff88);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
            margin-bottom: 10px;
        }

        .subtitle {
            text-align: center;
            color: #88aacc;
            margin-bottom: 30px;
            font-size: 1.1em;
        }

        /* Navigation */
        .nav-container {
            display: flex;
            gap: 10px;
            flex-wrap: wrap;
            justify-content: center;
            margin-bottom: 30px;
        }

        .nav-group {
            display: flex;
            gap: 5px;
            padding: 8px;
            background: rgba(30, 40, 60, 0.5);
            border-radius: 12px;
            border: 1px solid rgba(100, 150, 255, 0.2);
        }

        .nav-label {
            display: flex;
            align-items: center;
            padding: 0 10px;
            color: #88aacc;
            font-size: 0.85em;
            font-weight: bold;
        }

        .tab {
            padding: 10px 16px;
            border: none;
            border-radius: 8px;
            background: rgba(50, 60, 80, 0.5);
            color: #ccc;
            cursor: pointer;
            transition: all 0.3s ease;
            font-size: 0.9em;
            display: flex;
            align-items: center;
            gap: 6px;
        }

        .tab:hover {
            background: rgba(80, 100, 140, 0.6);
            transform: translateY(-2px);
        }

        .tab.active {
            background: linear-gradient(135deg, #3b82f6, #60a5fa);
            color: white;
            box-shadow: 0 4px 15px rgba(59, 130, 246, 0.4);
        }

        .tab-overview {
            background: linear-gradient(135deg, #f59e0b, #fbbf24);
            color: #1a1a1a;
        }

        .tab-overview.active {
            background: linear-gradient(135deg, #f59e0b, #fbbf24);
            box-shadow: 0 4px 15px rgba(251, 191, 36, 0.4);
        }

        /* Content panels */
        .content-panel {
            display: none;
            animation: fadeIn 0.3s ease;
        }

        .content-panel.active { display: block; }

        @keyframes fadeIn {
            from { opacity: 0; transform: translateY(10px); }
            to { opacity: 1; transform: translateY(0); }
        }

        /* Biome header */
        .biome-header {
            display: flex;
            align-items: center;
            gap: 20px;
            padding: 20px;
            background: linear-gradient(135deg, rgba(30, 40, 60, 0.8), rgba(40, 50, 70, 0.8));
            border-radius: 15px;
            margin-bottom: 25px;
            border-left: 4px solid var(--biome-color);
        }

        .biome-icon { font-size: 3em; }

        .biome-title h2 {
            font-size: 1.8em;
            margin-bottom: 5px;
        }

        .biome-category {
            color: #88aacc;
            font-size: 0.95em;
        }

        /* Stats grid */
        .stats-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 15px;
            margin-bottom: 25px;
        }

        .stat-card {
            background: linear-gradient(135deg, rgba(30, 60, 100, 0.4), rgba(40, 70, 120, 0.4));
            padding: 15px;
            border-radius: 12px;
            border: 1px solid rgba(100, 150, 255, 0.2);
        }

        .stat-label {
            color: #88aacc;
            font-size: 0.8em;
            text-transform: uppercase;
            letter-spacing: 0.5px;
            margin-bottom: 5px;
        }

        .stat-value {
            font-size: 1.3em;
            font-weight: bold;
            color: #fff;
        }

        /* Layer diagram */
        .layer-diagram {
            display: flex;
            gap: 20px;
            margin-bottom: 25px;
            background: rgba(20, 25, 35, 0.5);
            padding: 20px;
            border-radius: 15px;
        }

        .y-scale {
            display: flex;
            flex-direction: column;
            justify-content: space-between;
            width: 60px;
            font-size: 0.75em;
            color: #888;
        }

        .y-marker {
            text-align: right;
            padding-right: 8px;
            position: relative;
        }

        .y-marker.highlight { color: #00d9ff; font-weight: bold; }

        .layers-container {
            flex: 1;
            display: flex;
            flex-direction: column;
            border-radius: 10px;
            overflow: hidden;
            border: 2px solid #333;
        }

        .layer {
            display: flex;
            align-items: center;
            justify-content: space-between;
            padding: 0 15px;
            transition: all 0.3s ease;
            position: relative;
        }

        .layer:hover {
            filter: brightness(1.2);
            transform: scaleX(1.02);
        }

        .layer-name {
            font-weight: bold;
            color: white;
            text-shadow: 1px 1px 3px rgba(0,0,0,0.8);
        }

        .layer-range {
            font-size: 0.85em;
            color: rgba(255,255,255,0.8);
            background: rgba(0,0,0,0.3);
            padding: 2px 8px;
            border-radius: 4px;
        }

        .layer-badge {
            position: absolute;
            right: 15px;
            padding: 4px 12px;
            border-radius: 20px;
            font-size: 0.8em;
            font-weight: bold;
        }

        .badge-dry { background: #22c55e; color: white; }
        .badge-water { background: #3b82f6; color: white; }
        .badge-lava { background: #ef4444; color: white; }
        .badge-ice { background: #67e8f9; color: #1a1a1a; }

        /* Feature sections */
        .features-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
            gap: 20px;
            margin-bottom: 25px;
        }

        .feature-card {
            background: linear-gradient(135deg, rgba(25, 35, 55, 0.8), rgba(35, 45, 65, 0.8));
            padding: 20px;
            border-radius: 12px;
            border: 1px solid rgba(100, 150, 255, 0.15);
        }

        .feature-card h3 {
            color: #60a5fa;
            margin-bottom: 15px;
            padding-bottom: 10px;
            border-bottom: 2px solid rgba(96, 165, 250, 0.3);
            font-size: 1.1em;
        }

        .feature-item {
            margin-bottom: 10px;
            display: flex;
            gap: 8px;
        }

        .feature-item:last-child { margin-bottom: 0; }

        .feature-label { color: #88aacc; min-width: 80px; }
        .feature-value { color: #e0e0e0; }

        /* Special features list */
        .special-features {
            display: flex;
            flex-wrap: wrap;
            gap: 10px;
            margin-top: 15px;
        }

        .special-tag {
            background: linear-gradient(135deg, rgba(251, 191, 36, 0.2), rgba(251, 191, 36, 0.1));
            border: 1px solid rgba(251, 191, 36, 0.3);
            padding: 6px 12px;
            border-radius: 20px;
            font-size: 0.85em;
            color: #fbbf24;
        }

        /* Overview page styles */
        .overview-section {
            background: rgba(25, 35, 55, 0.5);
            padding: 25px;
            border-radius: 15px;
            margin-bottom: 25px;
        }

        .overview-section h3 {
            color: #00d9ff;
            margin-bottom: 20px;
            font-size: 1.3em;
        }

        .biome-grid {
            display: grid;
            grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
            gap: 15px;
        }

        .biome-mini-card {
            background: rgba(30, 40, 60, 0.6);
            padding: 15px;
            border-radius: 10px;
            border-left: 3px solid var(--biome-color);
            cursor: pointer;
            transition: all 0.3s ease;
        }

        .biome-mini-card:hover {
            transform: translateY(-3px);
            box-shadow: 0 5px 20px rgba(0,0,0,0.3);
        }

        .mini-card-header {
            display: flex;
            align-items: center;
            gap: 8px;
            margin-bottom: 8px;
        }

        .mini-card-icon { font-size: 1.3em; }
        .mini-card-name { font-weight: bold; color: #fff; }

        .mini-card-stats {
            font-size: 0.8em;
            color: #88aacc;
        }

        /* Depth markers */
        .depth-markers {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 15px;
        }

        .depth-marker {
            display: flex;
            align-items: center;
            gap: 12px;
            padding: 12px;
            background: rgba(30, 40, 60, 0.5);
            border-radius: 8px;
        }

        .depth-y {
            font-weight: bold;
            color: #00d9ff;
            font-size: 1.1em;
            min-width: 60px;
        }

        .depth-desc { color: #ccc; }

        /* Cave types */
        .cave-types {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
            gap: 15px;
        }

        .cave-type-card {
            padding: 15px;
            background: rgba(40, 30, 50, 0.5);
            border-radius: 10px;
            border: 1px solid rgba(150, 100, 200, 0.2);
        }

        .cave-type-name {
            font-weight: bold;
            color: #c084fc;
            margin-bottom: 8px;
        }

        .cave-type-desc {
            font-size: 0.9em;
            color: #aaa;
        }

        /* Underground biome depths */
        .underground-depth-chart {
            display: flex;
            flex-direction: column;
            gap: 10px;
            margin-top: 15px;
        }

        .depth-bar {
            display: flex;
            align-items: center;
            gap: 15px;
        }

        .depth-bar-label {
            min-width: 120px;
            font-weight: bold;
        }

        .depth-bar-visual {
            flex: 1;
            height: 30px;
            background: rgba(30, 30, 40, 0.8);
            border-radius: 5px;
            overflow: hidden;
            position: relative;
        }

        .depth-bar-fill {
            height: 100%;
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 0.8em;
            font-weight: bold;
            color: white;
        }
    """


def generate_biome_content(biome_key: str, biome: BiomeData) -> str:
    """Generate HTML content for a single biome."""

    # Determine layer configuration based on biome
    if biome.category == "underground":
        layers_html = generate_underground_layers(biome_key, biome)
    else:
        layers_html = generate_surface_layers(biome_key, biome)

    # Tree info
    tree_html = ""
    if biome.trees:
        tree_html = f"""
            <div class="feature-item">
                <span class="feature-label">Type:</span>
                <span class="feature-value">{biome.trees.tree_type}</span>
            </div>
            <div class="feature-item">
                <span class="feature-label">Height:</span>
                <span class="feature-value">{biome.trees.height}</span>
            </div>
            <div class="feature-item">
                <span class="feature-label">Density:</span>
                <span class="feature-value">{biome.trees.density}</span>
            </div>
        """
        if biome.trees.special:
            tree_html += f"""
            <div class="feature-item">
                <span class="feature-label">Special:</span>
                <span class="feature-value">{biome.trees.special}</span>
            </div>
            """
    else:
        tree_html = '<div class="feature-item"><span class="feature-value">No trees in this biome</span></div>'

    # Vegetation list
    veg_items = ", ".join(biome.vegetation.types) if biome.vegetation.types else "None"

    # Special features tags
    special_tags = "".join(f'<span class="special-tag">{feat}</span>' for feat in biome.special_features)

    # Cave badge
    cave_badge_class = {
        "dry": "badge-dry",
        "water": "badge-water",
        "lava": "badge-lava",
        "ice": "badge-ice"
    }.get(biome.caves.fill_type, "badge-dry")

    cave_badge_text = {
        "dry": "DRY",
        "water": "FLOODED",
        "lava": "LAVA",
        "ice": "ICE"
    }.get(biome.caves.fill_type, "DRY")

    return f"""
        <div class="content-panel" id="panel-{biome_key}">
            <div class="biome-header" style="--biome-color: {biome.color}">
                <div class="biome-icon">{biome.icon}</div>
                <div class="biome-title">
                    <h2>{biome.name}</h2>
                    <div class="biome-category">{biome.category.title()} Biome</div>
                </div>
            </div>

            <div class="stats-grid">
                <div class="stat-card">
                    <div class="stat-label">Surface Height</div>
                    <div class="stat-value">{"Underground" if biome.surface_height < 0 else f"Y: ~{biome.surface_height}"}</div>
                </div>
                <div class="stat-card">
                    <div class="stat-label">Surface Block</div>
                    <div class="stat-value">{biome.surface_block}</div>
                </div>
                <div class="stat-card">
                    <div class="stat-label">Subsurface</div>
                    <div class="stat-value">{biome.subsurface_block}</div>
                </div>
                <div class="stat-card">
                    <div class="stat-label">Cave Type</div>
                    <div class="stat-value">{biome.caves.fill_type.title()} <span class="{cave_badge_class}" style="font-size:0.7em;padding:2px 6px;border-radius:4px;">{cave_badge_text}</span></div>
                </div>
            </div>

            <div class="layer-diagram">
                {layers_html}
            </div>

            <div class="features-grid">
                <div class="feature-card">
                    <h3>🌳 Trees & Vegetation</h3>
                    {tree_html}
                </div>

                <div class="feature-card">
                    <h3>🌿 Ground Cover</h3>
                    <div class="feature-item">
                        <span class="feature-label">Types:</span>
                        <span class="feature-value">{veg_items}</span>
                    </div>
                    <div class="feature-item">
                        <span class="feature-label">Density:</span>
                        <span class="feature-value">{biome.vegetation.density}</span>
                    </div>
                </div>

                <div class="feature-card">
                    <h3>⛏️ Cave System</h3>
                    <div class="feature-item">
                        <span class="feature-label">Fill:</span>
                        <span class="feature-value">{biome.caves.fill_type.title()}</span>
                    </div>
                    {"<div class='feature-item'><span class='feature-label'>Water at:</span><span class='feature-value'>Y: " + str(biome.caves.water_level) + "</span></div>" if biome.caves.water_level else ""}
                    <div class="feature-item">
                        <span class="feature-label">Notes:</span>
                        <span class="feature-value">{biome.caves.special or "Standard cave generation"}</span>
                    </div>
                </div>
            </div>

            <div class="feature-card">
                <h3>✨ Special Features</h3>
                <div class="special-features">
                    {special_tags}
                </div>
            </div>
        </div>
    """


def generate_surface_layers(biome_key: str, biome: BiomeData) -> str:
    """Generate layer diagram for surface biomes."""
    surface_y = biome.surface_height

    # Calculate explorable cave space
    lava_top = 10
    water_level = biome.caves.water_level or 0

    if biome.caves.fill_type == "water":
        dry_start = water_level + 1
        explorable = surface_y - water_level
    elif biome.caves.fill_type == "lava":
        dry_start = max(lava_top + 1, water_level + 1) if water_level else lava_top + 1
        explorable = surface_y - dry_start + 1
    else:
        dry_start = lava_top + 1
        explorable = surface_y - lava_top

    layers = []

    # Sky layer
    layers.append(('sky', 'Sky', f'Y: {surface_y + 50}-511', 100,
                   'linear-gradient(180deg, #87CEEB 0%, #B0E0E6 100%)', None))

    # Above surface
    layers.append(('above', 'Above Surface', f'Y: {surface_y + 1}-{surface_y + 49}', 40,
                   'linear-gradient(180deg, #B0E0E6 0%, #98FB98 100%)', None))

    # Surface
    layers.append(('surface', 'Surface', f'Y: ~{surface_y}', 12,
                   f'linear-gradient(180deg, {biome.color} 0%, {biome.color}dd 100%)', None))

    # Upper caves (dry)
    if biome.caves.fill_type in ("dry", "ice"):
        layers.append(('upper-caves', 'Upper Caves', f'Y: {SEA_LEVEL + 1}-{surface_y - 1}', 80,
                       '#5D4E37' if biome.caves.fill_type == "dry" else '#B3E5FC', 'badge-dry' if biome.caves.fill_type == "dry" else 'badge-ice'))
    elif biome.caves.fill_type == "water":
        layers.append(('upper-caves', 'Upper Caves (Dry)', f'Y: {water_level + 1}-{surface_y - 1}', 60,
                       '#5D4E37', 'badge-dry'))
    elif biome.caves.fill_type == "lava":
        layers.append(('upper-caves', 'Upper Caves (Dry)', f'Y: {SEA_LEVEL + 1}-{surface_y - 1}', 60,
                       '#5D4E37', 'badge-dry'))

    # Deep caves
    if biome.caves.fill_type == "water":
        layers.append(('deep-flooded', 'Deep Caves (Flooded)', f'Y: {lava_top + 1}-{water_level}', 80,
                       'repeating-linear-gradient(45deg, #4682B4, #4682B4 10px, #5A9FD4 10px, #5A9FD4 20px)', 'badge-water'))
    elif biome.caves.fill_type == "lava":
        layers.append(('deep-lava', 'Deep Caves (Lava/Water)', f'Y: {lava_top + 1}-{SEA_LEVEL}', 80,
                       'repeating-linear-gradient(45deg, #DC2626, #DC2626 10px, #4682B4 10px, #4682B4 20px)', 'badge-lava'))
    elif biome.caves.fill_type == "ice":
        layers.append(('deep-ice', 'Deep Ice Caves', f'Y: {lava_top + 1}-{SEA_LEVEL}', 80,
                       'repeating-linear-gradient(45deg, #B3E5FC, #B3E5FC 10px, #81D4FA 10px, #81D4FA 20px)', 'badge-ice'))
    else:
        layers.append(('deep-caves', 'Deep Caves', f'Y: {lava_top + 1}-{SEA_LEVEL}', 80,
                       '#4A3728', 'badge-dry'))

    # Lava layer
    layers.append(('lava', 'Lava Lakes', 'Y: 2-10', 50,
                   'repeating-linear-gradient(45deg, #DC2626, #DC2626 8px, #EF4444 8px, #EF4444 16px)', 'badge-lava'))

    # Bedrock
    layers.append(('bedrock', 'Bedrock', 'Y: 0', 25,
                   'linear-gradient(135deg, #1a1a1a 0%, #2d2d2d 25%, #1a1a1a 50%, #2d2d2d 75%)', None))

    return _render_layers(layers, surface_y)


def generate_underground_layers(biome_key: str, biome: BiomeData) -> str:
    """Generate layer diagram for underground biomes."""

    layers = []

    if biome_key == "lush_caves":
        layers.append(('surface-ref', 'Surface (varies)', 'Y: 75-165', 30, '#90EE90', None))
        layers.append(('lush-upper', 'Lush Cave Zone', 'Y: 32-74', 100, '#32CD32', 'badge-dry'))
        layers.append(('lush-lower', 'Mixed Lush/Stone', 'Y: 11-31', 60, '#228B22', 'badge-dry'))
    elif biome_key == "dripstone_caves":
        layers.append(('surface-ref', 'Surface (varies)', 'Y: 75-165', 30, '#808080', None))
        layers.append(('drip-upper', 'Dripstone Zone', 'Y: 32-74', 100, '#8B8682', 'badge-dry'))
        layers.append(('drip-lower', 'Dense Formations', 'Y: 11-31', 60, '#696969', 'badge-dry'))
    elif biome_key == "deep_dark":
        layers.append(('surface-ref', 'Surface (varies)', 'Y: 75-165', 30, '#808080', None))
        layers.append(('stone-zone', 'Stone Caves', 'Y: 32-74', 60, '#5D4E37', 'badge-dry'))
        layers.append(('deep-dark', 'Deep Dark Zone', 'Y: 0-31', 100, '#1a1a2e', 'badge-dry'))

    # Lava layer (all have this)
    layers.append(('lava', 'Lava Lakes', 'Y: 2-10', 40,
                   'repeating-linear-gradient(45deg, #DC2626, #DC2626 8px, #EF4444 8px, #EF4444 16px)', 'badge-lava'))

    # Bedrock
    layers.append(('bedrock', 'Bedrock', 'Y: 0', 25,
                   'linear-gradient(135deg, #1a1a1a 0%, #2d2d2d 25%, #1a1a1a 50%, #2d2d2d 75%)', None))

    return _render_layers(layers, 75)


def _render_layers(layers: list, ref_height: int) -> str:
    """Render layers to HTML."""

    # Y scale
    y_scale = f"""
        <div class="y-scale">
            <div class="y-marker">Y: 511</div>
            <div class="y-marker">Y: 200</div>
            <div class="y-marker highlight">Y: {ref_height}</div>
            <div class="y-marker highlight">Y: {SEA_LEVEL}</div>
            <div class="y-marker">Y: 32</div>
            <div class="y-marker">Y: 10</div>
            <div class="y-marker">Y: 0</div>
        </div>
    """

    # Layers (reverse order for visual stacking)
    layers_html = '<div class="layers-container">'
    for layer_id, name, y_range, height, bg, badge in reversed(layers):
        badge_html = f'<span class="layer-badge {badge}">{badge.replace("badge-", "").upper()}</span>' if badge else ''
        layers_html += f'''
            <div class="layer" style="height: {height}px; background: {bg};">
                <span class="layer-name">{name}</span>
                <span class="layer-range">{y_range}</span>
                {badge_html}
            </div>
        '''
    layers_html += '</div>'

    return y_scale + layers_html


def generate_overview() -> str:
    """Generate the overview panel."""

    # Biome mini cards by category
    biome_cards_html = ""
    for cat_key, cat_data in CATEGORIES.items():
        cards = ""
        for biome_key in cat_data["biomes"]:
            biome = BIOMES[biome_key]
            height_str = "Underground" if biome.surface_height < 0 else f"Y: ~{biome.surface_height}"
            cards += f'''
                <div class="biome-mini-card" style="--biome-color: {biome.color}" onclick="showBiome('{biome_key}')">
                    <div class="mini-card-header">
                        <span class="mini-card-icon">{biome.icon}</span>
                        <span class="mini-card-name">{biome.name}</span>
                    </div>
                    <div class="mini-card-stats">
                        {height_str} | {biome.caves.fill_type.title()} caves
                    </div>
                </div>
            '''
        biome_cards_html += f'''
            <div class="overview-section">
                <h3>{cat_data["icon"]} {cat_data["name"]} Biomes</h3>
                <div class="biome-grid">{cards}</div>
            </div>
        '''

    return f"""
        <div class="content-panel active" id="panel-overview">
            <div class="overview-section">
                <h3>🌍 World Overview</h3>
                <div class="stats-grid">
                    <div class="stat-card">
                        <div class="stat-label">World Height</div>
                        <div class="stat-value">{WORLD_HEIGHT} blocks</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-label">Sea Level</div>
                        <div class="stat-value">Y: {SEA_LEVEL}</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-label">Surface Biomes</div>
                        <div class="stat-value">15</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-label">Underground Biomes</div>
                        <div class="stat-value">3</div>
                    </div>
                </div>
            </div>

            <div class="overview-section">
                <h3>📍 Key Depth Markers</h3>
                <div class="depth-markers">
                    <div class="depth-marker">
                        <span class="depth-y">Y: 0</span>
                        <span class="depth-desc">Bedrock (indestructible)</span>
                    </div>
                    <div class="depth-marker">
                        <span class="depth-y">Y: 2-10</span>
                        <span class="depth-desc">Lava lake zone (all biomes)</span>
                    </div>
                    <div class="depth-marker">
                        <span class="depth-y">Y: 0-31</span>
                        <span class="depth-desc">Deep Dark biome region</span>
                    </div>
                    <div class="depth-marker">
                        <span class="depth-y">Y: {SEA_LEVEL}</span>
                        <span class="depth-desc">Sea level</span>
                    </div>
                    <div class="depth-marker">
                        <span class="depth-y">Y: 155+</span>
                        <span class="depth-desc">Snow line (mountains only)</span>
                    </div>
                    <div class="depth-marker">
                        <span class="depth-y">Y: 511</span>
                        <span class="depth-desc">Build height limit</span>
                    </div>
                </div>
            </div>

            <div class="overview-section">
                <h3>🕳️ Cave System Types</h3>
                <div class="cave-types">
                    <div class="cave-type-card">
                        <div class="cave-type-name">🧀 Cheese Caves</div>
                        <div class="cave-type-desc">Large irregular caverns with natural pillars. Most common in high-humidity biomes.</div>
                    </div>
                    <div class="cave-type-card">
                        <div class="cave-type-name">🍝 Spaghetti Caves</div>
                        <div class="cave-type-desc">Long winding tunnel networks connecting different areas.</div>
                    </div>
                    <div class="cave-type-card">
                        <div class="cave-type-name">🍜 Noodle Caves</div>
                        <div class="cave-type-desc">Fine web of narrow passages. Only in high-density cave biomes.</div>
                    </div>
                    <div class="cave-type-card">
                        <div class="cave-type-name">⚒️ Carved Caves</div>
                        <div class="cave-type-desc">Traditional tunnels and ravines cutting through terrain.</div>
                    </div>
                </div>
            </div>

            <div class="overview-section">
                <h3>⛏️ Underground Biome Distribution</h3>
                <div class="underground-depth-chart">
                    <div class="depth-bar">
                        <span class="depth-bar-label" style="color: #32CD32;">🌿 Lush Caves</span>
                        <div class="depth-bar-visual">
                            <div class="depth-bar-fill" style="width: 60%; background: linear-gradient(90deg, #32CD32, #228B22); margin-left: 10%;">Y: 11-74</div>
                        </div>
                    </div>
                    <div class="depth-bar">
                        <span class="depth-bar-label" style="color: #8B8682;">🗻 Dripstone</span>
                        <div class="depth-bar-visual">
                            <div class="depth-bar-fill" style="width: 60%; background: linear-gradient(90deg, #8B8682, #696969); margin-left: 10%;">Y: 11-74</div>
                        </div>
                    </div>
                    <div class="depth-bar">
                        <span class="depth-bar-label" style="color: #4a4a6a;">🕳️ Deep Dark</span>
                        <div class="depth-bar-visual">
                            <div class="depth-bar-fill" style="width: 30%; background: linear-gradient(90deg, #1a1a2e, #2a2a4e);">Y: 0-31</div>
                        </div>
                    </div>
                </div>
            </div>

            {biome_cards_html}
        </div>
    """


def generate_navigation() -> str:
    """Generate the navigation tabs."""

    nav_html = '<div class="nav-container">'

    # Overview tab
    nav_html += '''
        <div class="nav-group">
            <button class="tab tab-overview active" data-panel="overview">
                <span>🌍</span> Overview
            </button>
        </div>
    '''

    # Category groups
    for cat_key, cat_data in CATEGORIES.items():
        nav_html += f'<div class="nav-group"><span class="nav-label">{cat_data["icon"]}</span>'
        for biome_key in cat_data["biomes"]:
            biome = BIOMES[biome_key]
            nav_html += f'''
                <button class="tab" data-panel="{biome_key}">
                    <span>{biome.icon}</span> {biome.name}
                </button>
            '''
        nav_html += '</div>'

    nav_html += '</div>'
    return nav_html


def generate_html() -> str:
    """Generate the complete HTML document."""

    css = generate_css()
    nav = generate_navigation()
    overview = generate_overview()

    # Generate all biome panels
    biome_panels = ""
    for biome_key, biome in BIOMES.items():
        biome_panels += generate_biome_content(biome_key, biome)

    return f"""<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Voxel World - Biome Layer Structure</title>
    <style>{css}</style>
</head>
<body>
    <div class="container">
        <h1>Voxel World Biome System</h1>
        <div class="subtitle">Complete Layer Structure & Depth-Dependent Features</div>

        {nav}

        {overview}
        {biome_panels}
    </div>

    <script>
        // Tab switching
        document.querySelectorAll('.tab').forEach(tab => {{
            tab.addEventListener('click', () => {{
                const panelId = tab.dataset.panel;
                showBiome(panelId);
            }});
        }});

        function showBiome(panelId) {{
            // Update tabs
            document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
            document.querySelector(`[data-panel="${{panelId}}"]`)?.classList.add('active');

            // Update panels
            document.querySelectorAll('.content-panel').forEach(p => p.classList.remove('active'));
            document.getElementById(`panel-${{panelId}}`)?.classList.add('active');
        }}
    </script>
</body>
</html>
"""


def main():
    """Main entry point."""
    html = generate_html()
    output_file = Path("layer_viz.html")
    output_file.write_text(html, encoding="utf-8")

    print(f"Generated {output_file}")
    print(f"  - {len(BIOMES)} biomes ({len([b for b in BIOMES.values() if b.category != 'underground'])} surface + {len([b for b in BIOMES.values() if b.category == 'underground'])} underground)")
    print(f"  - {len(CATEGORIES)} biome categories")


if __name__ == "__main__":
    main()

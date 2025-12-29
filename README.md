# Voxel Ray Traversal Engine

A GPU-accelerated voxel sandbox using Vulkan compute shaders for real-time ray marching. This is a **creative mode** experience focused on building and exploration—no health, no death, just pure creativity. Features procedural terrain, day/night cycle, swimming, particles, and interactive world editing.

## Features

### Rendering
- **Vulkan Compute Shader Rendering** - Ray marching through voxel data entirely on GPU
- **Texture Atlas** - AI-generated seamless tileable textures (18 textures, 64x64 each)
- **Ambient Occlusion** - Classic voxel AO with smooth corner darkening
- **Distance-Based LOD** - AO, shadows, and lighting optimized by distance for 90+ FPS
- **Day/Night Cycle** - Dynamic sun position, sky colors, and lighting
- **Torch Point Lighting** - Torches emit flickering light that illuminates nearby blocks
- **Animated Clouds** - Procedural clouds that drift with wind
- **Stars at Night** - Twinkling stars visible after sunset
- **Animated Water** - Flowing waves, caustics, and refraction effects
- **Fog** - Distance-based atmospheric fog
- **Shadow Rays** - Directional sunlight shadows
- **Particle System** - Block break particles, water splashes, walking dust
- **Hot Reload Shaders** - Edit shaders while running, changes apply instantly

### World
- **Large Dynamic World** - 512x128x512 block world (16x4x16 chunks, 32³ blocks each)
- **Chunk Streaming** - Chunks load/unload based on player position (view distance: 6 chunks)
- **Per-Chunk GPU Uploads** - Only modified chunks uploaded to GPU (~32KB vs 32MB full world)
- **Procedural Terrain** - Biome-based generation with distinct regions
  - **Flat plains**: Gentle grasslands with minimal height variation
  - **Rolling hills**: Moderate terrain with gradual slopes
  - **Mountains**: Dramatic peaks using RidgedMulti noise (up to 90 blocks)
  - Smooth transitions between biome types
- **Cave Systems** - 3D Perlin noise for spaghetti-style caves
  - ~25% of caves have surface-exposed entrances
  - Caves below sea level fill with water
- **Water Lakes** - Flat water surface at sea level (Y=28) fills valleys
- **Sandy Beaches** - Sand at water's edge
- **Trees** - Procedurally placed trees with rounded canopies
- **16 Block Types** - Stone, dirt, grass, wood, glass, water, torch, and more
- **Chunk Stats HUD** - Live display of loaded chunks, dirty chunks, and player position

### Creative Mode
- **No Health or Death** - Pure sandbox building and exploration
- **Player Physics** - Gravity, AABB collision detection, jump, sprint
- **Fly Mode** - Toggle free flight for creative building (F key)
- **Swimming** - Water detection, buoyancy, drag, swim controls
- **World Editing** - Place and break blocks in real-time
- **Variable Break Time** - Different blocks take different times to break
- **Block Break Cracks** - Progressive crack overlay as blocks are broken
- **Block Preview** - Ghost block with wireframe shows placement location
- **Block Outline** - Wireframe highlight on targeted block
- **Head Bob** - Subtle camera motion while walking
- **Hotbar** - 9-slot block selection with textures
- **Coordinates HUD** - Live X/Y/Z position display

## Getting Started

### Prerequisites

Follow the setup guide for [Vulkano](https://github.com/vulkano-rs/vulkano) to install Vulkan dependencies.

### Building & Running

```bash
cargo build --release
cargo run --release
```

On macOS with Homebrew MoltenVK:
```bash
DYLD_LIBRARY_PATH=/opt/homebrew/lib cargo run --release
```

Or use the Makefile:
```bash
make run
```

## Controls

| Key | Action |
|-----|--------|
| **Click** | Focus window (grab cursor) |
| **WASD** | Move |
| **Space** | Jump / Fly up / Swim up |
| **Shift** | Fly down / Swim down |
| **Ctrl** | Toggle sprint (2x speed, 4x in fly mode) |
| **Mouse** | Look around |
| **Left Click** (hold) | Break block |
| **Right Click** | Place block |
| **1-9** | Select hotbar slot |
| **Scroll Wheel** | Cycle hotbar |
| **F** | Toggle fly mode |
| **B** | Toggle chunk boundaries |
| **M** | Cycle render modes |
| **Esc** | Release cursor |

### Hotbar Blocks

| Slot | Block |
|------|-------|
| 1 | Stone |
| 2 | Dirt |
| 3 | Grass |
| 4 | Planks |
| 5 | Log |
| 6 | Cobblestone |
| 7 | Glass |
| 8 | Torch |
| 9 | Water |

## Architecture

```
src/
├── main.rs          # Vulkan setup, render loop, input, physics, HUD
├── chunk.rs         # Chunk storage (32³), BlockType enum, bit-packing
├── world.rs         # Multi-chunk management, terrain generation
├── camera.rs        # Pixel-to-ray matrix for GPU ray casting
├── raycast.rs       # CPU-side DDA for block picking
├── particles.rs     # Particle system (break effects, splashes)
└── hot_reload.rs    # Shader hot reloading

shaders/
├── traverse.comp    # GPU ray marching, lighting, AO, particles
└── resample.comp    # Image resampling

textures/
└── texture_atlas.png  # Combined block textures (1152x64)
```

## Block Types

| ID | Type | Break Time | Description |
|----|------|------------|-------------|
| 0 | Air | - | Empty space |
| 1 | Stone | 0.8s | Gray rocky surface |
| 2 | Dirt | 0.3s | Brown soil |
| 3 | Grass | 0.5s | Green grass top, dirt sides |
| 4 | Planks | 0.5s | Wooden floor planks |
| 5 | Leaves | 0.15s | Tree foliage (transparent) |
| 6 | Sand | 0.3s | Beach sand |
| 7 | Gravel | 0.3s | Small rocks |
| 8 | Water | - | Blue water (swimmable) |
| 9 | Glass | 0.5s | Transparent glass |
| 10 | Log | 0.5s | Tree bark with rings on top |
| 11 | Torch | 0.15s | Light source |
| 12 | Brick | 0.8s | Red brick pattern |
| 13 | Snow | 0.3s | White snow cover |
| 14 | Cobblestone | 0.8s | Rough stone blocks |
| 15 | Iron | 1.2s | Metallic iron block |

## Render Modes

Cycle through with **M** key:

- **Textured** (default) - Full rendering with textures, lighting, shadows, and AO
- **Normal** - Surface normal visualization (RGB = XYZ)
- **Coord** - RGB visualization of block coordinates
- **Steps** - Heat map of ray march iterations
- **UV** - Texture coordinate visualization
- **Depth** - Distance-based shading

## Technical Details

### Ray Marching Algorithm

Based on the 1987 paper by Amanatides and Woo: [A Fast Voxel Traversal Algorithm for Ray Tracing](http://www.cse.yorku.ca/~amana/research/grid.pdf).

The compute shader (`traverse.comp`) performs DDA ray marching through a 3D texture containing block type data. Each pixel spawns a ray, traverses until hitting a solid block, then samples the texture atlas and applies lighting.

**Performance Optimizations:**
- Per-ray dynamic step limit: calculates optimal DDA steps using ray direction (`|dx| + |dy| + |dz|`)
- Configurable max ray steps (128-1024) via HUD slider
- Distance-based LOD: AO (48 blocks), shadows (64), point lights (32), sky exposure (48)
- Per-chunk GPU uploads: only modified 32KB chunks uploaded, not entire 32MB world

### Ambient Occlusion

Uses the classic Minecraft-style vertex AO algorithm:
- Sample 3 neighbors at each face corner (2 edges + 1 diagonal)
- If both edge neighbors are solid, corner is fully occluded
- Bilinear interpolation across the face for smooth gradients
- Only calculated for blocks within 48 units for performance

### Day/Night Cycle

- Sun orbits around the world over a configurable period (default 120 seconds)
- Sky color transitions from blue (day) to dark blue/black (night)
- Directional lighting intensity varies with sun position
- Ambient light adjusts to maintain visibility at night

### Particle System

- Block break: 20-36 particles with physics, gravity, and collision
- Water splash: Upward spray when entering water
- Walking dust: Small puffs when moving on ground
- Particles fade out after landing or timeout

## License

MIT

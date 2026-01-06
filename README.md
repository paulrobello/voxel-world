# Voxel World

A GPU-accelerated voxel sandbox using Vulkan compute shaders for real-time ray marching. This is a **creative mode** experience focused on building and exploration—no health, no death, just pure creativity. Features procedural terrain, day/night cycle, swimming, particles, and interactive world editing.

## Features

### Rendering
- **Vulkan Compute Shader Rendering** - Ray marching through voxel data entirely on GPU
- **Sub-Voxel Model System** - 16³ resolution models for detailed blocks (torches, fences, gates, ladders, doors, crystals)
  - Model registry pattern: reusable models stored once, referenced by blocks
  - Per-model 16-color palettes with emission support
  - 4³ collision masks for accurate player/model collision
  - LOD: sub-voxel detail rendered within configurable distance
  - Cyan glow highlighting when targeting sub-voxel models
  - **Fence System** - 16 fence variants that dynamically connect to neighbors
  - **Fence Gates** - 8 gate variants (open/closed) that connect to adjacent fences
  - **Ladder System** - Climbable ladders with auto-rotation toward player on placement
  - **Door System** - 5 door types with upper/lower halves, hinge sides, open/closed states
  - **Crystal Blocks** - 32 tinted crystal variants with colored point light emission
  - **Ground Support** - Fences, gates, torches, and ladders break when block below is removed
- **Texture Atlas** - AI-generated seamless tileable textures (23 textures, 64x64 each)
- **Ambient Occlusion** - Classic voxel AO with smooth corner darkening
- **Distance-Based LOD** - AO, shadows, and lighting optimized by distance for 90+ FPS
- **Day/Night Cycle** - Dynamic sun position, sky colors, and lighting
- **Point Light System** - Torches, lava, glowstone, glowmushroom, and crystals emit dynamic light
- **Animated Clouds** - Procedural clouds that drift with wind
- **Stars at Night** - Twinkling stars visible after sunset
- **Animated Water** - Flowing waves, caustics, and refraction effects
- **Translucent Glass** - See-through glass with visible frame borders and fresnel reflections
- **Tinted Glass** - 32 color variants with colored shadow casting
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
- **23 Block Types** - Stone, dirt, grass, wood, glass, water, lava, glowstone, crystal, and more
- **Painted Blocks** - 19 textures × 32 tints = 608 customizable block variants
- **Chunk Stats HUD** - Live display of loaded chunks, dirty chunks, and player position

### Creative Mode
- **No Health or Death** - Pure sandbox building and exploration
- **Player Physics** - Gravity, AABB collision detection, jump, sprint
- **Fly Mode** - Toggle free flight for creative building (F key)
- **Swimming** - Water detection, buoyancy, drag, swim controls
- **Climbing** - Ladder detection, vertical movement with Space/Shift
- **World Editing** - Place and break blocks in real-time
- **Line-Locked Building** - Hold right-click to build straight lines; direction locks after first two blocks
- **Player Collision** - Cannot place blocks inside your own hitbox
- **Configurable Cooldowns** - Adjustable break/place cooldown timers in settings
- **Variable Break Time** - Different blocks take different times to break
- **Block Break Cracks** - Progressive crack overlay as blocks are broken
- **Block Preview** - Ghost block with wireframe shows placement location
- **Block Outline** - Cyan wireframe on targeted blocks; cyan glow for sub-voxel models
- **Head Bob** - Subtle camera motion while walking
- **Hotbar** - 9-slot block selection with textures
- **Block Palette** - Full block selection with categories and tint picker
- **In-Game Model Editor** - Create custom 16³ sub-voxel models (N key)
- **Coordinates HUD** - Live X/Y/Z position display

### Block Physics
- **Falling Blocks** - Sand and gravel fall when unsupported
- **Tree Chopping** - Break a tree trunk and the entire tree falls
  - Trees detect ground support through connected logs
  - Orphaned leaves (not connected to any log) also fall
  - Satisfying physics as logs and leaves tumble down
- **Block Stacking** - Falling blocks land and stack naturally
- **Water Flow** - Dynamic water simulation using cellular automata
  - Water flows down (gravity), then spreads horizontally, then rises under pressure
  - Player-placed water becomes an infinite source block
  - Breaking blocks near water triggers flow into empty space
  - Water spreads ~7-10 blocks before becoming too thin and evaporating
  - Simulation runs only within 64 blocks of player for performance
- **Lava Flow** - Similar to water but slower, with orange glow and light emission
- **Frame-Distributed Updates** - Physics checks spread across frames to prevent FPS spikes
  - Priority queue processes nearby blocks first
  - Configurable updates per frame (16-128) via settings

### Minimap
- **Toggle** - Press M to show/hide minimap (bottom-right corner)
- **Rotate with Player** - Map rotates so "up" is always your facing direction
- **Size Options** - Small (128px), Medium (192px), Large (256px)
- **Color Modes** - Block colors, height shading, or both combined
- **Triangle Indicator** - Shows player position and direction
- **Throttled Updates** - Efficient caching with position/rotation-based refresh

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
| **Space** | Jump / Fly up / Swim up / Climb up |
| **Shift** | Fly down / Swim down / Climb down |
| **Ctrl** | Toggle sprint (2x speed, 4x in fly mode) |
| **Mouse** | Look around |
| **Left Click** (hold) | Break block |
| **Right Click** (hold) | Place block (line-locks after 2 blocks) |
| **Middle Click** | Pick block type (switches to or replaces hotbar slot) |
| **1-9** | Select hotbar slot |
| **Scroll Wheel** | Cycle hotbar |
| **F** | Toggle fly mode |
| **B** | Toggle chunk boundaries |
| **M** | Toggle minimap |
| **N** | Open model editor |
| **/** | Open console |
| **Esc** | Release cursor / Open settings panel |

### Settings Panel

Press **Esc** to open the settings panel with:
- Render mode selection (Textured, Normal, Coord, Steps, UV, Depth, BrickDebug)
- FOV and render scale sliders
- Day/night cycle controls
- Break/place cooldown adjustments
- Physics updates per frame (cascade speed)
- Minimap options (size, colors, rotation)
- Performance toggles (AO, shadows, fog, etc.)

### Hotbar Blocks

Default hotbar (customizable via palette):

| Slot | Block |
|------|-------|
| 1 | Stone |
| 2 | Dirt |
| 3 | Grass |
| 4 | Sand |
| 5 | Log |
| 6 | Fence (16³ sub-voxel model, connects to neighbors) |
| 7 | Gate (16³ sub-voxel model, connects to fences) |
| 8 | Ladder (16³ sub-voxel model, climbable) |
| 9 | Torch (16³ sub-voxel model with flame) |

## Architecture

```
src/
├── main.rs              # Vulkan setup, render loop, input handling
├── block_interaction.rs # Block placement/breaking, hotbar, palette UI
├── block_update.rs      # Frame-distributed physics update queue
├── chunk.rs             # Chunk storage (32³), BlockType enum, bit-packing
├── chunk_loader.rs      # Async chunk generation with thread pool
├── world.rs             # Multi-chunk management, terrain generation
├── camera.rs            # Pixel-to-ray matrix for GPU ray casting
├── raycast.rs           # CPU-side DDA for block picking
├── falling_block.rs     # Falling block entities with physics
├── particles.rs         # Particle system (break effects, splashes)
├── sub_voxel.rs         # Sub-voxel model system (16³ models, registry, palettes)
├── sub_voxel_builtins.rs # Built-in model definitions (doors, fences, etc.)
├── svt.rs               # SVT-64 sparse voxel tree for ray skipping
├── water.rs             # Water flow simulation (cellular automata)
├── lava.rs              # Lava flow simulation
├── hot_reload.rs        # Shader hot reloading
├── editor/              # In-game sub-voxel model editor
│   ├── mod.rs           # Editor state and tools
│   ├── ui.rs            # egui interface
│   └── rasterizer.rs    # Software renderer for preview
└── console/             # Command console system

shaders/
├── traverse.comp    # Main GPU ray marching shader
├── common.glsl      # Block types, push constants, buffer layouts
├── models.glsl      # Sub-voxel model ray marching
├── lighting.glsl    # Point lights, shadows, AO
├── materials.glsl   # Texture sampling, emission colors
└── resample.comp    # Image resampling

textures/
└── texture_atlas.png  # Combined block textures (1472x64, 23 tiles)
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
| 6 | Sand | 0.3s | Beach sand (falls) |
| 7 | Gravel | 0.3s | Small rocks (falls) |
| 8 | Water | - | Blue water (swimmable) |
| 9 | Glass | 0.5s | Transparent glass |
| 10 | Log | 0.5s | Tree bark with rings on top |
| 11 | Model | 0.15s | 16³ sub-voxel model blocks |
| 12 | Brick | 0.8s | Red brick pattern |
| 13 | Snow | 0.3s | White snow cover |
| 14 | Cobblestone | 0.8s | Rough stone blocks |
| 15 | Iron | 1.2s | Metallic iron block |
| 16 | Bedrock | - | Indestructible foundation |
| 17 | TintedGlass | 0.5s | Colored glass (32 tints) |
| 18 | Painted | 0.5s | Textured block with tint |
| 19 | Lava | - | Glowing orange fluid |
| 20 | GlowStone | 0.5s | Bright warm light source |
| 21 | GlowMushroom | 0.15s | Soft cyan cave light |
| 22 | Crystal | 0.5s | Tinted crystal with light emission |

## Render Modes

Selectable in settings panel:

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
- Empty chunk skip: Rays skip entire 32³ chunks that contain only air (4.6x FPS improvement)
- SVT-64 brick skip: Each chunk divided into 64 bricks (8³), rays skip empty bricks using distance fields
- Per-ray dynamic step limit: calculates optimal DDA steps using ray direction (`|dx| + |dy| + |dz|`)
- Configurable max ray steps (128-1024) via HUD slider
- Distance-based LOD: AO (48 blocks), shadows (64), point lights (32), sky exposure (48)
- Per-chunk GPU uploads: only modified 32KB chunks uploaded, not entire 32MB world
- Async chunk generation: 4-thread pool for background terrain generation
- Frame-distributed physics: block updates processed via priority queue (32/frame default)

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

### Water Flow Simulation

Uses a mass-based cellular automata system (W-Shadow algorithm):

- **Storage**: Sparse HashMap for water cells (only positions with water are tracked)
- **Cell properties**: Mass (0.0-1.0+), source flag, stability counter
- **Flow priority**: Down (gravity) → Horizontal (equalization) → Up (pressure only)
- **Spread distance**: Controlled by mass conservation and evaporation threshold
  - `MIN_MASS = 0.001`: Water below this evaporates
  - `FLOW_DAMPING = 0.5`: Each transfer is dampened to prevent oscillation
  - Result: ~7-10 block spread from a source before water thins and evaporates

**Boundary handling**:
- World bounds (Y < 0): Water drains into void and is destroyed
- Unloaded chunks: Water is blocked (preserved until chunk loads)
- Simulation radius: Only processes water within 64 blocks of player

**Performance**: Frame-distributed updates (64 cells/frame default), priority queue favors nearby water.

## License

MIT

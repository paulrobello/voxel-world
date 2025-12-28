# Voxel Ray Traversal Engine

A GPU-accelerated voxel game engine using Vulkan compute shaders for real-time ray marching. Features procedural terrain generation, textured blocks, ambient occlusion, and interactive world editing.

## Features

- **Vulkan Compute Shader Rendering** - Ray marching through voxel data entirely on GPU
- **Multi-Chunk World** - 96x32x96 block world (3x1x3 chunks, 32³ blocks each)
- **Procedural Terrain** - FBM Perlin noise for natural-looking landscapes
- **Texture Atlas** - AI-generated seamless tileable textures for all block types
- **Ambient Occlusion** - Classic voxel AO with smooth corner darkening
- **Hot Reload Shaders** - Edit shaders while running, changes apply instantly
- **Player Physics** - Gravity, collision detection, and fly mode
- **World Editing** - Place and break blocks in real-time
- **Trees & Lakes** - Procedurally placed trees with rounded canopies and water bodies

## Screenshots

The engine renders voxel terrain with textured blocks, directional lighting, and ambient occlusion for depth.

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

## Controls

| Key | Action |
|-----|--------|
| **Click** | Focus window (grab cursor) |
| **WASD** | Move horizontally |
| **Space** | Jump / Fly up |
| **Shift** | Fly down |
| **Mouse** | Look around |
| **Left Click** | Break block |
| **Right Click** | Place block |
| **1-8** | Select block type |
| **F** | Toggle fly mode |
| **B** | Toggle chunk boundaries |
| **M** | Cycle render modes |
| **Esc** | Release cursor |

### Block Selection

| Key | Block |
|-----|-------|
| 1 | Stone |
| 2 | Dirt |
| 3 | Grass |
| 4 | Planks |
| 5 | Log |
| 6 | Leaves |
| 7 | Sand |
| 8 | Glass |

## Architecture

```
src/
├── main.rs          # Application, rendering, input handling
├── chunk.rs         # Chunk data structure and block types
├── world.rs         # Multi-chunk world management
├── camera.rs        # Camera and pixel-to-ray matrix
├── raycast.rs       # CPU-side raycasting for block interaction
└── hot_reload.rs    # Shader hot reloading

shaders/
└── traverse.comp    # GPU ray marching compute shader

textures/
└── texture_atlas.png  # Combined block textures (704x64)
```

## Block Types

| ID | Type | Description |
|----|------|-------------|
| 0 | Air | Empty space |
| 1 | Stone | Gray rocky surface |
| 2 | Dirt | Brown soil |
| 3 | Grass | Green grass top |
| 4 | Planks | Wooden floor planks |
| 5 | Leaves | Tree foliage |
| 6 | Sand | Beach sand |
| 7 | Gravel | Small rocks |
| 8 | Water | Blue water surface |
| 9 | Glass | Transparent glass |
| 10 | Log | Tree bark (vertical grain) |

## Render Modes

Cycle through with **M** key:

- **Normal** - Textured with lighting and AO
- **Coord** - RGB visualization of block coordinates
- **Steps** - Heat map of ray march iterations
- **UV** - Texture coordinate visualization
- **Depth** - Distance-based shading

## Technical Details

### Ray Marching Algorithm

Based on the 1987 paper by Amanatides and Woo: [A Fast Voxel Traversal Algorithm for Ray Tracing](http://www.cse.yorku.ca/~amana/research/grid.pdf).

The compute shader (`traverse.comp`) performs DDA ray marching through a 3D texture containing block type data. Each pixel spawns a ray, traverses until hitting a solid block, then samples the texture atlas and applies lighting.

### Ambient Occlusion

Uses the classic Minecraft-style vertex AO algorithm:
- Sample 3 neighbors at each face corner (2 edges + 1 diagonal)
- If both edge neighbors are solid, corner is fully occluded
- Bilinear interpolation across the face for smooth gradients

### Texture Atlas

All block textures are packed into a single 704x64 atlas (11 textures, 64x64 each). The shader calculates UV offsets based on block type for efficient single-texture sampling.

## License

MIT

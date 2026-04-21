# Quickstart

Get Voxel World running in under five minutes — from cloning the repo to building and exploring your first world.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Install & Build](#install--build)
- [First Launch](#first-launch)
- [Controls](#controls)
- [Building Basics](#building-basics)
- [Exploring the World](#exploring-the-world)
- [Next Steps](#next-steps)
- [Related Documentation](#related-documentation)

## Prerequisites

| Requirement | Notes |
|-------------|-------|
| **Rust 1.94.1+** | Install via [rustup](https://rustup.rs): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| **Vulkan driver 1.2+** | GPU must support Vulkan compute shaders |

### Platform Setup

**macOS:**

```bash
brew install molten-vk
```

**Linux:**

```bash
# Ubuntu/Debian
sudo apt install libvulkan-dev mesa-vulkan-drivers

# Fedora
sudo dnf install vulkan-loader-devel mesa-vulkan-drivers
```

**Windows:**

Install the [Vulkan SDK](https://vulkan.lunarg.com/sdk/home) from LunarG.

## Install & Build

```bash
# Clone the repository
git clone https://github.com/paulrobello/voxel-world.git
cd voxel-world

# Build and run (Makefile handles macOS Vulkan env vars)
make run
```

The first build compiles shaders and generates GLSL constants — expect 1–3 minutes. Subsequent builds are incremental.

> **Tip:** If running the binary directly on macOS, you must set Vulkan environment variables. The Makefile handles this automatically; see [CLI Reference](CLI.md#vulkan-setup-macos) for the required values.

### Quality Presets

If performance is low on your hardware, try a lower preset:

```bash
make run-potato    # Minimum quality
make run-low       # Basic lighting
make run-medium    # Default balance
make run-high      # High quality
make run-ultra     # Maximum quality (GPU-intensive)
```

## First Launch

When the game starts:

1. A window opens at 1200×1080 with a procedurally generated world
2. **Click the window** to grab the cursor and enter the game
3. You spawn on solid ground near the world center
4. The hotbar at the bottom shows your selected blocks
5. A crosshair in the center shows where you are aiming
6. Your X/Y/Z coordinates appear in the top-left corner

```mermaid
graph LR
    Launch[Launch Game] --> Click[Click Window]
    Click --> Explore[Explore World]
    Explore --> Build[Build with Blocks]
    Build --> Tools[Use Creative Tools]

    style Launch fill:#e65100,stroke:#ff9800,stroke-width:3px,color:#ffffff
    style Click fill:#1b5e20,stroke:#4caf50,stroke-width:2px,color:#ffffff
    style Explore fill:#1b5e20,stroke:#4caf50,stroke-width:2px,color:#ffffff
    style Build fill:#0d47a1,stroke:#2196f3,stroke-width:2px,color:#ffffff
    style Tools fill:#4a148c,stroke:#9c27b0,stroke-width:2px,color:#ffffff
```

## Controls

### Movement

| Key | Action |
|-----|--------|
| **WASD** | Move |
| **Mouse** | Look around |
| **Space** | Jump / Fly up / Swim up / Climb ladder up |
| **Shift** | Fly down / Swim down / Climb ladder down |
| **Ctrl** | Toggle sprint (2× walk speed, 4× in fly mode) |
| **F** | Toggle fly mode |

### Block Interaction

| Input | Action |
|-------|--------|
| **Left Click** (hold) | Break block |
| **Right Click** (hold) | Place block (line-locks direction after 2 blocks) |
| **Middle Click** | Pick block type into current hotbar slot |
| **1–9** | Select hotbar slot |
| **Scroll Wheel** | Cycle through hotbar |

### UI & Tools

| Key | Action |
|-----|--------|
| **Esc** | Release cursor / Open settings panel |
| **B** | Toggle chunk boundary visualization |
| **M** | Toggle minimap |
| **J** | Toggle player torch light |
| **N** | Open sub-voxel model editor |
| **L** | Open template library browser |
| **/** | Open console |

## Building Basics

### Default Hotbar

| Slot | Block |
|------|-------|
| 1 | Stone |
| 2 | Dirt |
| 3 | Grass |
| 4 | Sand |
| 5 | Log |
| 6 | Fence |
| 7 | Gate |
| 8 | Ladder |
| 9 | Torch |

### Placing Blocks

1. Select a block from the hotbar (**1–9** or scroll)
2. Aim the crosshair at an existing block face
3. **Hold right-click** to place the selected block on that face
4. After placing two blocks in a row, the direction locks — continue holding right-click to build a straight line

### Breaking Blocks

1. Aim the crosshair at any block
2. **Hold left-click** to break it
3. Break progress appears as cracks on the block
4. Different blocks have different break times

### Picking Blocks

**Middle-click** any block in the world to copy it into your current hotbar slot. This works for all block types including sub-voxel models.

### Block Palette

Press **Esc** to open the settings panel, then use the palette to browse all 47 block types, painted block variants (19 textures × 32 tints), and tinted glass colors.

## Exploring the World

### Fly Mode

Press **F** to toggle fly mode. In fly mode:

- Gravity is disabled
- **Space** moves up, **Shift** moves down
- **Ctrl** activates sprint (4× speed)
- Collision detection is off by default (configurable in settings)

Fly mode is the best way to explore the terrain, find caves, and survey large areas.

### World Features

The default world (`--world-gen normal`) generates:

- **Biomes:** Plains, hills, mountains, deserts, swamps, and more
- **Caves:** Underground networks with surface entrances (~25% have openings)
- **Trees:** Oak, birch, pine, willow, jungle, and cactus
- **Water:** Lakes and rivers at sea level
- **Day/night cycle:** Dynamic sun position with stars and clouds at night

### Finding Your Way

- **Coordinates HUD** (top-left) shows your X/Y/Z position
- **Minimap** (**M** key) shows a bird's-eye view that rotates with your facing direction
- **Compass** at the top of the screen shows cardinal directions
- **Chunk boundaries** (**B** key) visualize the 32³ chunk grid

### Custom Worlds

```bash
# Flat world for building
make new-flat

# Fresh normal world
make new-normal

# Custom seed
make run ARGS="--seed 42"

# Flat world with specific seed
make run ARGS="--world-gen flat --seed 12345"
```

## Next Steps

### Creative Tools

Voxel World includes 20+ building tools beyond basic block placement:

- **Shape tools:** Cube, sphere, cylinder, torus, arch, bridge, helix, stairs
- **Modifiers:** Hollow, mirror, replace, pattern, clone
- **Terrain tools:** Terrain brush (sculpt, flatten, smooth), scatter

Access tools through the settings panel (**Esc**) or see [CLI Reference](CLI.md) for console commands.

### Model Editor

Press **N** to open the sub-voxel model editor. Create custom models at 8³, 16³, or 32³ resolution with:

- 8 tools: pencil, eraser, eyedropper, fill, cube, sphere, color change, paint bucket
- Mirror mode for symmetrical builds
- Undo/redo (100 steps)

### Multiplayer

```bash
# Terminal 1: Start host
make run-host

# Terminal 2: Join as client
make run-client
```

The host runs an integrated server. Up to 4 players on LAN. See [CLI Reference](CLI.md#multiplayer) for remote connections.

### Console Commands

Press **/** to open the in-game console:

| Command | Description |
|---------|-------------|
| `tp <x> <y> <z>` | Teleport to coordinates |
| `fill <x1> <y1> <z1> <x2> <y2> <z2> <block>` | Fill a region with blocks |
| `sphere <x> <y> <z> <radius> <block>` | Create a sphere |
| `locate <biome>` | Find nearest biome |

### Settings

Press **Esc** to open the settings panel and configure:

- Render mode and quality
- FOV and render scale
- Day/night cycle
- Break/place cooldowns
- Physics simulation speed
- Minimap size and colors
- Lighting features (AO, shadows, point lights)

## Related Documentation

- [CLI Reference](CLI.md) — Complete command-line options and Makefile targets
- [Architecture](ARCHITECTURE.md) — System design and module organization
- [README.md](../README.md) — Full feature list and technical documentation

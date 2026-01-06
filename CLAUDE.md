# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
make build          # Build release (default)
make run            # Build and run release
make run-debug      # Build and run debug with RUST_BACKTRACE=1
make test           # Run tests
make fmt            # Format code
make lint           # Run clippy linter
make checkall       # Format, lint, and test (run after making changes)
make sprite-gen     # Generate palette/hotbar sprites
make new-flat       # Reset and create flat world
```

The Makefile sets `DYLD_LIBRARY_PATH` and `VK_ICD_FILENAMES` for macOS MoltenVK.

## CLI Options

```bash
make run ARGS="--seed 42"           # Custom terrain seed (-S)
make run ARGS="--fly-mode"          # Start in fly mode (-f)
make run ARGS="--spawn-x 100 --spawn-z 200"  # Custom spawn (-x, -z)
make run ARGS="--time-of-day 0.5"   # Pause at noon (-t)
make run ARGS="--view-distance 8"   # Increase view distance (-v)
make run ARGS="--render-mode depth" # Start in depth mode (-r)
make run ARGS="--world-gen flat"    # Flat world generation
make run ARGS="--verbose"           # Debug output
```

## Development Workflow

### ⚠️ PRIORITY ONE: Commit After Every Batch of Work

**CRITICAL**: To enable rollback and prevent loss of working states:

1. After completing a logical batch of changes, run `make checkall`
2. Fix any errors or warnings found
3. **Commit immediately** with a descriptive message
4. Do NOT accumulate multiple unrelated changes before committing

```bash
make checkall                    # Must pass before committing
git add -A
git commit -m "type: description"
```

### Code Quality Check

**IMPORTANT**: After making any code changes, always run:
```bash
make checkall
```

The project is not ready until `make checkall` passes without errors.

## Architecture Overview

Vulkan compute shader voxel engine with GPU ray marching. See README.md for detailed technical documentation.

**Key source files:**
- `main.rs` - Vulkan setup, render loop, input handling
- `chunk.rs` - BlockType enum, chunk storage (32³ blocks)
- `world.rs` - Multi-chunk management, terrain generation, block metadata
- `sub_voxel.rs` - Multi-resolution sub-voxel model system (8³/16³/32³), model registry, 32-color palettes
- `sub_voxel_builtins.rs` - Built-in model definitions (doors, fences, torches, etc.) - all use 8³ resolution
- `block_interaction.rs` - Block placement/breaking, hotbar, palette UI
- `water.rs` / `lava.rs` - Fluid simulation (cellular automata)
- `editor/` - In-game sub-voxel model editor (N key)

**Shader files:**
- `traverse.comp` - Main ray marching shader
- `common.glsl` - Block type constants, push constants, buffer layouts
- `models.glsl` - Sub-voxel model ray marching
- `lighting.glsl` - Point lights, shadows, AO
- `materials.glsl` - Texture sampling, emission colors

**Coordinate systems:**
- World coordinates: Global block positions (i32)
- Chunk coordinates: Chunk grid positions (i32), each chunk is 32³
- Local coordinates: Position within chunk (0-31)
- Sub-voxel: 8³, 16³, or 32³ voxels per block for models (per-model resolution)
- Conversion: `World::world_to_chunk()`, `World::world_to_local()`

## Adding New Block Types

1. Generate texture: `/voxel-texture <name>`
2. Add variant to `BlockType` enum in `chunk.rs`
3. Update `From<u8>` impl, `color()`, `break_time()`, and property methods
4. Add `BLOCK_<NAME>` constant in `common.glsl`
5. Update `ATLAS_TILE_COUNT` in `materials.glsl` if adding new texture slot
6. Regenerate atlas:
```bash
cd textures
magick air_64x64.png stone_64x64.png dirt_64x64.png grass_64x64.png planks_64x64.png \
  leaves_64x64.png sand_64x64.png gravel_64x64.png water_64x64.png glass_64x64.png \
  log_64x64.png torch_64x64.png brick_64x64.png snow_64x64.png cobblestone_64x64.png \
  iron_64x64.png bedrock_64x64.png grass_side_64x64.png log_top_64x64.png \
  lava_64x64.png glowstone_64x64.png glowmushroom_64x64.png crystal_64x64.png \
  +append texture_atlas.png
```

## Block Type Sync

BlockType enum in `chunk.rs` must match constants in `common.glsl`:
```
0=Air, 1=Stone, 2=Dirt, 3=Grass, 4=Planks, 5=Leaves, 6=Sand, 7=Gravel,
8=Water, 9=Glass, 10=Log, 11=Model, 12=Brick, 13=Snow, 14=Cobblestone, 15=Iron, 16=Bedrock,
17=TintedGlass, 18=Painted, 19=Lava, 20=GlowStone, 21=GlowMushroom, 22=Crystal
```
Extra texture slots: 17=grass_side, 18=log_top

**Emissive blocks** (19-22): Lava, GlowStone, GlowMushroom, Crystal emit light and have visual glow. Crystal blocks use tint_data for 32 colored variations with tinted point lights.

**Tinted blocks**: TintedGlass and Crystal use `tint_data` (0-31) for color from `TINT_PALETTE` in `common.glsl`.

**Painted blocks**: Use `paint_data` to store texture index and tint index (19 textures × 32 tints = 608 variants).

## Sub-Voxel Model System

Models support three resolutions (8³, 16³, 32³) with 32-color palettes and per-slot emission. Key types in `sub_voxel.rs`:
- `ModelResolution` - Low (8³), Medium (16³), High (32³)
- `LightMode` - 10 animated light modes (Steady, Pulse, Flicker, Candle, Strobe, Breathe, Sparkle, Wave, WarmUp, Arc)
- `FIRST_CUSTOM_MODEL_ID = 100` - IDs 0-99 reserved for built-ins
- Built-in models use Low (8³) resolution for optimal performance

**Model IDs:**
- 0-19: Torch variants (rotation × flame states)
- 20-35: Fence variants (connection states)
- 36-51: Fence gate variants
- 52-67: Ladder variants
- 68-98: Door/trapdoor variants
- 99: Crystal model (tinted by block metadata)
- 100+: Custom user models

**Adding built-in models:** Edit `sub_voxel_builtins.rs`, call `create_*()` functions in `register_builtins()`.

**Model editor** (N key): Tools include pencil, eraser, fill, eyedropper, rotate, mirror, cube, sphere.

## Key Constants

- `CHUNK_SIZE = 32` (chunk.rs)
- `BRICK_SIZE = 8` (svt.rs)
- `ModelResolution::Low/Medium/High` = 8/16/32 (sub_voxel.rs)
- `ATLAS_TILE_COUNT = 23.0` (materials.glsl)
- World: 16x4x16 chunks = 512x128x512 blocks
- View distance: 6 chunks

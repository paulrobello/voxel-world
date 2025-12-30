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
```

The Makefile sets `DYLD_LIBRARY_PATH` and `VK_ICD_FILENAMES` for macOS MoltenVK.

## CLI Options

```bash
make run -- --seed 42           # Custom terrain seed (-S)
make run -- --fly-mode          # Start in fly mode (-f)
make run -- --spawn-x 100 --spawn-z 200  # Custom spawn (-x, -z)
make run -- --time-of-day 0.5   # Pause at noon (-t)
make run -- --view-distance 8   # Increase view distance (-v)
make run -- --render-mode depth # Start in depth mode (-r)
make run -- --verbose           # Debug output
```

## Development Workflow

**IMPORTANT**: After making any code changes, always run:
```bash
make checkall
```

The project is not ready until `make checkall` passes without errors.

## Architecture Overview

Vulkan compute shader voxel engine with GPU ray marching. See README.md for detailed technical documentation.

**Key files:**
- `main.rs` - Vulkan setup, render loop, input, physics, HUD
- `chunk.rs` - BlockType enum, chunk storage (32³)
- `world.rs` - Multi-chunk management, terrain generation
- `shaders/traverse.comp` - GPU ray marching, lighting, AO

**Coordinate systems:**
- World coordinates: Global block positions (i32)
- Chunk coordinates: Chunk grid positions (i32), each chunk is 32³
- Local coordinates: Position within chunk (0-31)
- Conversion: `World::world_to_chunk()`, `World::world_to_local()`

## Adding New Block Types

1. Generate texture: `/voxel-texture <name>`
2. Add variant to `BlockType` enum in `chunk.rs`
3. Update `From<u8>` impl, `color()`, `break_time()`, and property methods
4. Add `BLOCK_<NAME>` constant in `traverse.comp`
5. Update `ATLAS_TILE_COUNT` in shader if adding new texture slot
6. Regenerate atlas:
```bash
cd textures
magick air_64x64.png stone_64x64.png dirt_64x64.png grass_64x64.png planks_64x64.png \
  leaves_64x64.png sand_64x64.png gravel_64x64.png water_64x64.png glass_64x64.png \
  log_64x64.png torch_64x64.png brick_64x64.png snow_64x64.png cobblestone_64x64.png \
  iron_64x64.png grass_side_64x64.png log_top_64x64.png +append texture_atlas.png
```

## Block Type Sync

BlockType enum in `chunk.rs` must match constants in `traverse.comp`:
```
0=Air, 1=Stone, 2=Dirt, 3=Grass, 4=Planks, 5=Leaves, 6=Sand, 7=Gravel,
8=Water, 9=Glass, 10=Log, 11=Torch, 12=Brick, 13=Snow, 14=Cobblestone, 15=Iron
```
Extra texture slots: 16=grass_side, 17=log_top

## Key Constants

- `CHUNK_SIZE = 32` (chunk.rs)
- `BRICK_SIZE = 8` (svt.rs)
- `ATLAS_TILE_COUNT = 18.0` (traverse.comp)
- World: 16x4x16 chunks = 512x128x512 blocks
- View distance: 6 chunks

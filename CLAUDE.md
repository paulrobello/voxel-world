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
make check          # Check formatting and lint (no modifications)
make checkall       # Format, lint, and test (run after making changes)
```

The Makefile sets `DYLD_LIBRARY_PATH` and `VK_ICD_FILENAMES` for macOS MoltenVK.

## Development Workflow

**IMPORTANT**: After making any code changes, always run:
```bash
make checkall
```

This will format code, run clippy with warnings as errors, and run all tests. The project is not ready until `make checkall` passes without errors.

## Architecture

This is a Vulkan compute shader voxel engine. Rendering happens entirely on the GPU via ray marching.

### Data Flow

1. **World** (`world.rs`) manages chunks in a HashMap keyed by chunk position
2. **Chunk** (`chunk.rs`) stores 32³ blocks as `BlockType` enum values (u8)
3. **Dirty chunk tracking**: Modified chunks queued for GPU upload via `dirty_chunks` vector
4. **Per-chunk GPU upload**: Only dirty chunks uploaded (~32KB each) to 3D texture (`R8_UINT`)
5. **Compute shader** (`shaders/traverse.comp`) ray marches through the 3D texture
6. Hits sample from **texture atlas** (18 tiles at 64x64 each)

### Source Files

- `main.rs` - Vulkan setup, render loop, input handling, egui HUD, player physics
- `chunk.rs` - `BlockType` enum (16 types), chunk storage (32³), bit-packing for GPU
- `world.rs` - Multi-chunk management, coordinate conversion, terrain generation
- `camera.rs` - Pixel-to-ray matrix generation for GPU ray casting
- `raycast.rs` - CPU-side DDA for block picking (break/place interaction)
- `particles.rs` - Particle system for block break effects, water splashes
- `hot_reload.rs` - Watches shader files and recompiles on save

### GPU Pipeline

- Push constants pass camera matrix, world dimensions, time, lighting params to shader
- Three descriptor sets: render target (set 0), block data (set 1), texture atlas (set 2)
- Particle data uploaded via storage buffer
- `HotReloadComputePipeline` watches `shaders/traverse.comp` and recompiles on save

### GPU Upload System

Block edits use efficient per-chunk uploads instead of full world re-upload:

1. `World::set_block()` marks the containing chunk as dirty (adds to `dirty_chunks` queue)
2. `upload_world_to_gpu()` drains dirty queue and calls `upload_chunks_batched()`
3. `upload_chunks_batched()` creates staging buffers and issues `BufferImageCopy` for each chunk
4. Each chunk upload is ~32KB (32³ bytes) vs 32MB for full world

Key functions in `main.rs`:
- `upload_world_to_gpu()` - Drains dirty chunks, uploads only modified data
- `upload_chunks_batched()` - Batched GPU upload with region-specific copies
- `upload_all_dirty_chunks()` - Initial bulk upload at startup

### Coordinate Systems

- **World coordinates**: Global block positions (i32)
- **Chunk coordinates**: Chunk grid positions (i32), each chunk is 32³
- **Local coordinates**: Position within a chunk (0-31)
- Conversion: `World::world_to_chunk()`, `World::world_to_local()`

### Block Types

Defined in `chunk.rs` as `BlockType` enum (0-15). Must match constants in `traverse.comp`:
```
0=Air, 1=Stone, 2=Dirt, 3=Grass, 4=Planks, 5=Leaves, 6=Sand, 7=Gravel,
8=Water, 9=Glass, 10=Log, 11=Torch, 12=Brick, 13=Snow, 14=Cobblestone, 15=Iron
```
Additional texture slots: 16=grass_side, 17=log_top (for multi-face blocks)

### Shader Structure

`traverse.comp` implements:
- DDA ray marching (Amanatides & Woo algorithm) with dynamic step limit
- Distance-based LOD for expensive operations (AO, shadows, point lights, sky exposure)
- Texture atlas sampling via block type index
- Ambient occlusion (corner neighbor sampling with bilinear interpolation)
- Day/night cycle with sun position and sky colors
- Fog with distance-based blending
- Shadow rays for directional sunlight
- Block preview rendering (ghost block with wireframe)
- Target block outline (wireframe on block being looked at)
- Particle billboard rendering
- Multiple render modes (normal, coord, steps, UV, depth)

### Player Systems (main.rs)

- **Physics**: Gravity, collision (AABB vs voxels), jump, sprint
- **Swimming**: Water detection, buoyancy, drag, swim controls
- **Head bob**: Sine wave camera offset while walking
- **Block interaction**: Break (hold left click with progress), place (right click)
- **Hotbar**: 9 slots, keys 1-9 or scroll wheel to select

## Key Constants

- `CHUNK_SIZE = 32` (chunk.rs)
- `ATLAS_TILE_COUNT = 18.0` (traverse.comp)
- World size: `WORLD_CHUNKS_X/Y/Z` in main.rs (16x4x16 chunks = 512x128x512 blocks)
- View distance: `VIEW_DISTANCE = 6` chunks around player
- LOD distances in shader: AO=48, Shadow=64, PointLight=32, SkyExposure=48 blocks

## Texture Workflow

Textures in `textures/` folder. Atlas order must match BlockType enum indices.

### Creating New Textures

Use the `/voxel-texture` skill to generate new block textures:
```
/voxel-texture <block_name>
```

### Regenerating Atlas

After adding textures, regenerate the atlas (order must match BlockType + extra textures):
```bash
cd textures
magick air_64x64.png stone_64x64.png dirt_64x64.png grass_64x64.png planks_64x64.png \
  leaves_64x64.png sand_64x64.png gravel_64x64.png water_64x64.png glass_64x64.png \
  log_64x64.png torch_64x64.png brick_64x64.png snow_64x64.png cobblestone_64x64.png \
  iron_64x64.png grass_side_64x64.png log_top_64x64.png +append texture_atlas.png
```

### Adding New Block Types

1. Generate texture with `/voxel-texture <name>`
2. Add variant to `BlockType` enum in `chunk.rs`
3. Update `From<u8>` impl, `color()`, `break_time()`, and property methods
4. Add `BLOCK_<NAME>` constant in `traverse.comp`
5. Update `ATLAS_TILE_COUNT` in shader if adding new texture slot
6. Regenerate atlas with new texture appended

## Controls

| Key | Action |
|-----|--------|
| WASD | Move |
| Space | Jump / Swim up |
| Shift | Sprint / Swim down |
| Mouse | Look |
| Left Click (hold) | Break block |
| Right Click | Place block |
| 1-9 | Select hotbar slot |
| Scroll | Cycle hotbar |
| F | Toggle fly mode |
| B | Toggle chunk boundaries |
| M | Cycle render modes |
| Esc | Release cursor |

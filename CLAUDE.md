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

This will:
1. Format the code with `cargo fmt`
2. Run clippy linter with warnings as errors
3. Run all tests

Resolve all issues before considering the work complete. The project is not ready until `make checkall` passes without errors.

## Architecture

This is a Vulkan compute shader voxel engine. Rendering happens entirely on the GPU via ray marching.

### Data Flow

1. **World** (`world.rs`) manages chunks in a HashMap keyed by chunk position
2. **Chunk** (`chunk.rs`) stores 32³ blocks as `BlockType` enum values (u8)
3. World data uploads to a 3D texture (`R8_UINT`) on the GPU
4. **Compute shader** (`shaders/traverse.comp`) ray marches through the 3D texture
5. Hits sample from **texture atlas** (704x64, 11 textures in a row)

### GPU Pipeline

- `main.rs` creates Vulkan resources: device, queues, descriptor sets, compute pipeline
- Push constants pass camera matrix and world dimensions to shader
- Three descriptor sets: render target (set 0), block data (set 1), texture atlas (set 2)
- `HotReloadComputePipeline` (`hot_reload.rs`) watches shader files and recompiles on save

### Coordinate Systems

- **World coordinates**: Global block positions (i32)
- **Chunk coordinates**: Chunk grid positions (i32), each chunk is 32³
- **Local coordinates**: Position within a chunk (0-31)
- Conversion: `World::world_to_chunk()`, `World::world_to_local()`

### Block Types

Defined in `chunk.rs` as `BlockType` enum. Must match constants in `traverse.comp`:
- Index corresponds to texture atlas position (0=air, 1=stone, ..., 10=log)
- Multi-face blocks (grass, log) use additional texture slots (11=grass_side, 12=log_top)
- Add new blocks: update enum, `From<u8>`, shader constants, and regenerate atlas

### Shader Structure

`traverse.comp` implements:
- DDA ray marching (Amanatides & Woo algorithm)
- Texture atlas sampling via block type index
- Ambient occlusion (corner neighbor sampling with bilinear interpolation)
- Multiple render modes (normal, coord, steps, UV, depth)

### Player Interaction

- `raycast.rs`: CPU-side DDA for block picking (break/place)
- `camera.rs`: Pixel-to-ray matrix generation for GPU
- Input handled via `winit_input_helper` in main loop

## Key Constants

- `CHUNK_SIZE = 32` (chunk.rs)
- `ATLAS_TILE_COUNT = 13.0` (traverse.comp) - includes multi-face variants
- World size configured in main.rs: `WORLD_SIZE_X/Y/Z`

## Texture Workflow

Textures in `textures/` folder. Atlas order must match BlockType enum indices.

### Creating New Textures

Use the `/voxel-texture` skill to generate new block textures:
```
/voxel-texture <block_name>
```

The skill uses nanobanana MCP to generate seamless tileable textures optimized for voxel rendering, then processes them with ImageMagick to create 64x64 versions.

### Regenerating Atlas

After adding textures, regenerate the atlas:
```bash
cd textures
magick air_64x64.png stone_64x64.png dirt_64x64.png grass_64x64.png planks_64x64.png \
  leaves_64x64.png sand_64x64.png gravel_64x64.png water_64x64.png glass_64x64.png \
  log_64x64.png grass_side_64x64.png log_top_64x64.png +append texture_atlas.png
```

### Adding New Block Types

1. Generate texture with `/voxel-texture <name>`
2. Add variant to `BlockType` enum in `chunk.rs`
3. Update `From<u8>` impl and `color()` method
4. Add `BLOCK_<NAME>` constant in `traverse.comp`
5. Update `ATLAS_TILE_COUNT` in shader
6. Regenerate atlas with new texture appended

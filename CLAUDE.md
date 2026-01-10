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
make new-flat       # Reset and create flat world (seed 123456)
make new-normal     # Reset and create normal world (seed 123456)
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
- `world/tree_logic.rs` - Tree detection, orphaned leaf detection, ground support checks
- `sub_voxel.rs` - Multi-resolution sub-voxel model system (8³/16³/32³), model registry, 32-color palettes
- `sub_voxel_builtins.rs` - Built-in model definitions (doors, fences, torches, etc.) - all use 8³ resolution
- `block_interaction.rs` - Block placement/breaking, hotbar, palette UI
- `water.rs` / `lava.rs` - Fluid simulation (cellular automata)
- `falling_block.rs` - Gravity physics for sand, gravel, snow, and orphaned leaves
- `block_update.rs` - Frame-distributed physics checks (gravity, tree support, orphaned leaves)
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

## Image Generation Tools

This project uses specialized skills and MCP servers for different types of image generation. **Always use the appropriate tool for the task:**

### 1. Block Textures: `/voxel-texture` Skill

**Use for:** Creating flat, tileable 64x64 block textures for the texture atlas.

**Examples:** Stone, dirt, grass, ice, sand, wood, etc.

**How to use:**
```bash
/voxel-texture ice
/voxel-texture sandstone
/voxel-texture obsidian
```

**What it does:**
- Generates flat, seamless tileable texture patterns (NOT 3D cubes)
- Automatically resizes to exact 64x64 dimensions
- Creates tiled preview for verification
- Saves to `textures/{name}_64x64.png`
- Uses strong negative prompts to prevent 3D renders

**Important:** This skill enforces MCP availability and will stop if nanobanana is not running. Do NOT fall back to other methods.

### 2. Sprite Icons: `/game-sprite` Skill

**Use for:** Creating sprite icons for UI elements (hotbar, palette, inventory).

**Examples:** Item icons, block preview sprites, UI elements.

**Note:** In this project, sprites are generated via `make sprite-gen` which renders 3D voxel previews of blocks and models. The `/game-sprite` skill is available for custom 2D sprite assets if needed.

**Sprite generation workflow:**
```bash
make sprite-gen  # Generates all block/model sprites automatically
```

This creates `textures/rendered/block_*.png` and `textures/rendered/model_*.png` files.

### 3. General Images: nanobanana MCP

**Use for:** Any other image generation not covered by the above skills.

**Examples:**
- Concept art
- Reference images
- Documentation diagrams (when not using mermaid)
- Promotional materials
- Custom artwork

**How to use:**
```python
mcp__nanobanana__generate_image(
    prompt="detailed description of image",
    aspect_ratio="16:9",  # or appropriate ratio
    resolution="high",
    model_tier="pro",     # or "flash" for speed
    output_path="/path/to/output.png"
)
```

### Tool Selection Decision Tree

```
Need an image?
├─ Is it a flat, tileable block texture for the game?
│  └─ YES → Use `/voxel-texture <name>`
│
├─ Is it a 2D sprite icon for UI (hotbar/palette)?
│  ├─ Block or model preview? → Use `make sprite-gen`
│  └─ Custom sprite asset? → Use `/game-sprite`
│
└─ Is it something else (concept art, diagrams, etc.)?
   └─ YES → Use nanobanana MCP directly
```

### Critical Rules

1. **NEVER** bypass the `/voxel-texture` skill for block textures
   - It has essential safeguards against 3D renders
   - It enforces proper dimensions and seamless tiling
   - It validates MCP availability

2. **NEVER** manually create block textures with generic prompts
   - The skill uses tested prompt templates
   - It prevents common AI generation failures

3. **ALWAYS** regenerate sprites after adding new blocks
   - Run `make sprite-gen` after texture atlas updates
   - This ensures palette UI shows correct icons

4. **VERIFY** textures before committing
   - Check for 3D perspective (should be flat)
   - Verify seamless tiling with 2x2 preview
   - Confirm exact 64x64 dimensions

## Adding New Block Types

1. Generate texture: `/voxel-texture <name>`
2. Add variant to `BlockType` enum in `chunk.rs`
3. Update `From<u8>` impl, `color()`, `break_time()`, and property methods
4. Add `BLOCK_<NAME>` constant in `common.glsl`
5. Update `ATLAS_TILE_COUNT` in `materials.glsl` if adding new texture slot
6. Add block to palette: Update `BLOCK_PALETTE` array in `src/ui/palette.rs`
7. Add to sprite generation: Update blocks array in `src/sprite_gen.rs`
8. Regenerate atlas:
```bash
cd textures
magick air_64x64.png stone_64x64.png dirt_64x64.png grass_64x64.png planks_64x64.png \
  leaves_64x64.png sand_64x64.png gravel_64x64.png water_64x64.png glass_64x64.png \
  log_64x64.png torch_64x64.png brick_64x64.png snow_64x64.png cobblestone_64x64.png \
  iron_64x64.png bedrock_64x64.png grass_side_64x64.png log_top_64x64.png \
  lava_64x64.png glowstone_64x64.png glowmushroom_64x64.png crystal_64x64.png \
  cactus_64x64.png mud_64x64.png sandstone_64x64.png ice_64x64.png pine_leaves_64x64.png \
  decorative_stone_64x64.png willow_leaves_64x64.png concrete_64x64.png \
  +append texture_atlas.png
```
9. Update `blockTypeToAtlasIndex()` function in `shaders/materials.glsl` if needed
10. Regenerate sprites:
```bash
make sprite-gen
```
11. Test in-game and verify texture renders correctly

## ⚠️ Painted Block - User-Only

**CRITICAL**: The `Painted` block type (BlockType::Painted = 18) is **ONLY** for player customization.

**DO NOT use painted blocks in world/terrain generation code.**

If you need a block for world generation:
1. Create a dedicated BlockType variant
2. Add it to the enum with proper properties
3. Generate or assign a texture
4. Follow the "Adding New Block Types" workflow above

**Why this matters:**
- Painted blocks store texture and tint in per-block metadata
- This is memory-intensive and intended for player creativity
- World generation should use efficient, dedicated block types
- Painted blocks discovered in terrain_gen.rs should be replaced

**Examples of correct approach:**
- ✅ `BlockType::Mud` for swamp surfaces (dedicated block)
- ✅ `BlockType::Sandstone` for desert subsurface (dedicated block)
- ✅ `BlockType::Cactus` for desert plants (dedicated block)
- ❌ `set_painted_block(TEX_MUD, TINT_WHITE)` in terrain generation

## Block Type Sync

BlockType enum in `chunk.rs` must match constants in `common.glsl`:
```
0=Air, 1=Stone, 2=Dirt, 3=Grass, 4=Planks, 5=Leaves, 6=Sand, 7=Gravel,
8=Water, 9=Glass, 10=Log, 11=Model, 12=Brick, 13=Snow, 14=Cobblestone, 15=Iron, 16=Bedrock,
17=TintedGlass, 18=Painted, 19=Lava, 20=GlowStone, 21=GlowMushroom, 22=Crystal,
23=PineLog, 24=WillowLog, 25=PineLeaves, 26=WillowLeaves, 27=Ice,
28=Mud, 29=Sandstone, 30=Cactus, 31=DecorativeStone, 32=Concrete,
33=Deepslate, 34=Moss, 35=MossyCobblestone, 36=Clay, 37=Dripstone, 38=Calcite,
39=Terracotta, 40=PackedIce, 41=Podzol, 42=Mycelium, 43=CoarseDirt, 44=RootedDirt
```

**Texture Atlas Mapping:**
- Positions 0-16: Direct mapping (Air through Bedrock)
- Position 17: grass_side (special texture)
- Position 18: log_top (special texture)
- Positions 19-22: Emissive blocks (Lava, GlowStone, GlowMushroom, Crystal)
- Positions 23-30: Biome textures (Cactus, Mud, Sandstone, Ice, PineLeaves, DecorativeStone, WillowLeaves, Concrete)
- Positions 31-42: Cave/biome blocks (Deepslate, Moss, MossyCobble, Clay, Dripstone, Calcite, Terracotta, PackedIce, Podzol, Mycelium, CoarseDirt, RootedDirt)
- Total: 43 textures in atlas (2752x64 pixels)

**Important:** BlockType enum values DO NOT directly map to atlas positions for all blocks. The shader uses `blockTypeToAtlasIndex()` function in `materials.glsl` to perform the mapping.

**Emissive blocks** (19-22): Lava, GlowStone, GlowMushroom, Crystal emit light and have visual glow. Crystal blocks use tint_data for 32 colored variations with tinted point lights.

**Tinted blocks**: TintedGlass and Crystal use `tint_data` (0-31) for color from `TINT_PALETTE` in `common.glsl`.

**Painted blocks**: Use `paint_data` to store texture index and tint index (19 textures × 32 tints = 608 variants).

## Sub-Voxel Model System

Models support three resolutions (8³, 16³, 32³) with 32-color palettes and per-slot emission. Key types in `sub_voxel.rs`:
- `ModelResolution` - Low (8³), Medium (16³), High (32³)
- `LightMode` - 10 animated light modes (Steady, Pulse, Flicker, Candle, Strobe, Breathe, Sparkle, Wave, WarmUp, Arc)
- `FIRST_CUSTOM_MODEL_ID = 110` - IDs 0-109 reserved for built-ins
- Built-in models use Low (8³) resolution for optimal performance

**Model IDs:**
- 0: Empty/placeholder
- 1: Torch
- 2-3: Slabs
- 4-19: Fence variants (connection states)
- 20-27: Gate variants (closed/open)
- 28-38: Stairs variants (straight/corners, floor/ceiling)
- 39-98: Door and window variants (plain, windowed, paneled, fancy, glass, trapdoors, windows)
- 99: Crystal model (tinted by block metadata)
- 100-105: Vegetation models (grass, flowers, lily pad, mushrooms)
- 106-109: Cave decorations (stalactite, stalagmite, ice variants)
- 110+: Custom user models

**Adding built-in models:** Edit `sub_voxel_builtins.rs`, call `create_*()` functions in `register_builtins()`.

**Model editor** (N key): Tools include pencil, eraser, fill, eyedropper, rotate, mirror, cube, sphere.

## Key Constants

- `CHUNK_SIZE = 32` (chunk.rs)
- `BRICK_SIZE = 8` (svt.rs)
- `ModelResolution::Low/Medium/High` = 8/16/32 (sub_voxel.rs)
- `ATLAS_TILE_COUNT = 31.0` (materials.glsl)
- World: 16x16x16 chunks loaded = 512x512x512 blocks (Y bounded 0-511, X/Z infinite via streaming)
- View distance: 6 chunks

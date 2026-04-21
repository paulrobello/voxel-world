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
make run-cap-exit   # Run, screenshot at 3s, exit at 4s (for visual debugging)
make new-flat       # Reset and create flat world (seed 314159)
make new-normal     # Reset and create normal world (seed 314159)
```

The Makefile sets `DYLD_LIBRARY_PATH` and `VK_ICD_FILENAMES` for macOS MoltenVK — run the binary via `make` or replicate those env vars when invoking `./target/release/voxel-world` directly.

### Multiplayer

```bash
make run-host       # Start as LAN host on default port
make run-client     # Join LAN host
make reset-host     # Reset host-side save/state
make reset-client   # Reset client-side save/state
```

### Benchmarking

All benchmarks use `--auto-fly` so the player auto-travels through the world and exercises chunk streaming / origin shifts. CSV profile data is written to `profiles/`.

```bash
make benchmark          # Flat terrain, 45s @ 2x speed
make benchmark-hills    # Hilly terrain, 45s
make benchmark-spiral   # Spiral flight pattern, 90s
make benchmark-normal   # Realistic normal terrain, 45s
make benchmark-stress   # Full-speed stress run
make benchmark-cap      # Short run with screenshot
make benchmark-compare ARGS="profiles/a.csv profiles/b.csv"  # Thermal-aware A/B
```

Benchmark runs sample Apple Silicon thermals and on-battery state into the CSV; `scripts/bench_stats.sh` filters noisy samples when comparing.

### Quality & Profile Targets

```bash
make run-potato | run-low | run-medium | run-high | run-ultra   # Quality presets (--quality flag)
make auto-profile-flat                                          # 45s automated feature-flag cycle (flat)
make auto-profile-normal                                        # 45s automated feature-flag cycle (normal)
make run-p1     # or run-p2                                     # Multi-instance (isolated data_p1/, data_p2/)
```

## CLI Options

```bash
make run ARGS="--seed 42"                   # Custom terrain seed (-S)
make run ARGS="--fly-mode"                  # Start in fly mode (-f)
make run ARGS="--spawn-x 100 --spawn-z 200" # Custom spawn (-x, -z)
make run ARGS="--time-of-day 0.5"           # Pause at noon (-t)
make run ARGS="--view-distance 8"           # Increase view distance (-v)
make run ARGS="--render-mode depth"         # Start in depth mode (-r)
make run ARGS="--world-gen flat"            # Flat world generation
make run ARGS="--auto-fly"                  # Auto-traverse — used by benchmarks
make run ARGS="--verbose"                   # Debug output
make run ARGS="--screenshot-delay 5"        # Screenshot after 5s (-s)
make run ARGS="--exit-delay 10"             # Exit after 10s (-e)
```

### Perf Tuning Env Vars

Chunk streaming and GPU upload budgets are overridable at runtime — useful for reproducing stalls on slower machines or isolating a regression.

| Variable | Default | Purpose |
|---|---|---|
| `ORIGIN_SHIFT_PROFILE=1` | off | Per-shift `[ShiftProfile]` / `[UploadProfile]` timing on stderr |
| `ORIGIN_SHIFT_NEAR_RADIUS` | `view_distance` | Sync-upload radius on origin shift. Smaller = less stall but visible pop-in |
| `METADATA_CHUNKS_PER_FRAME` | 128 | Chunks whose SVT metadata is rebuilt per frame |
| `METADATA_RESET_BUDGET` | 256 | Per-frame budget during a full metadata reseed after a shift |
| `REUPLOAD_PER_FRAME` | 256 | Chunks drained from `reupload_queue` per frame after a shift |
| `UPLOADS_PER_FRAME` | 256 | Dirty chunks uploaded to GPU per frame |

## Development Workflow

### WARNING: PRIORITY ONE: Commit After Every Batch of Work

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

### Logging

Production code uses the `log` crate macros (`log::debug!` / `log::info!` / `log::warn!` / `log::error!`). `println!` and `eprintln!` appear only inside `#[cfg(test)]` blocks — do not reintroduce them in production paths. Existing prefixes like `[Server]`, `[Client]`, `[World Gen]` are kept inside the format string; the severity is carried by the macro choice.

### `.unwrap()` policy

`.unwrap()` / `.expect()` are fine inside `#[cfg(test)]` blocks. Production code should return a `Result` or handle the error explicitly. Vulkan init paths in `gpu_resources.rs` / `vulkan_context.rs` are the known exception — GPU failures at those call sites mean the game cannot continue regardless.

## Architecture Overview

Vulkan compute shader voxel engine with GPU ray marching. See README.md for detailed technical documentation.

### Rust ↔ GLSL constant sync

**`build.rs` auto-generates `shaders/generated_constants.glsl`** from `src/chunk.rs`, `src/constants.rs`, `src/render_mode.rs`, and `src/svt.rs`. `BLOCK_*`, `RENDER_MODE_*`, `ATLAS_TILE_COUNT`, `CHUNK_SIZE`, `CHUNKS_X/Y/Z`, `BRICK_SIZE`, `BRICKS_PER_AXIS`, `BRICKS_PER_CHUNK`, and `TINT_PALETTE` are NOT manually edited in GLSL — edit the Rust-side source of truth and the shader constant updates on next build. `common.glsl` `#include`s the generated file.

### Key source files

- `main.rs` — Vulkan setup, render loop, input handling
- `chunk.rs` — `BlockType` enum, chunk storage (32³ blocks), `TINT_PALETTE`, `Chunk::mutation_epoch` (funnel for set-block invalidation)
- `world/` — multi-chunk management module: `storage.rs` (chunk map), `query.rs`, `connections.rs`, `lighting.rs`, `stair_logic.rs`, `tree_logic.rs`, `world_gen.rs`
- `app/` — Central `App` split across 12 impl files (`core`, `init`, `render`, `update`, `input`, `event_handler`, `network_sync`, `hud`, `minimap`, `stats`, `helpers`, plus submodules)
- `net/` — Multiplayer: ~21 per-subsystem sync modules (`chunk_sync`, `block_sync`, `fluid_sync`, `tree_fall_sync`, `player_sync`, `extended_gameplay_sync`, …) over a client/server/channel core. See "Multiplayer architecture" below for key patterns. Integration test `test_loopback_connect_send_receive` in `src/net/client.rs` is the pattern for future end-to-end tests.
- `app_state/multiplayer.rs` — Multiplayer state orchestration: host/client startup, message routing between server ↔ client ↔ game world, originator-excluded broadcasts, epoch-aware chunk dedup.
- `sub_voxel/` — Sub-voxel model system (8³/16³/32³) in `model.rs` + `registry.rs` + `types.rs` + `builtins/`. `PaletteTable` in `registry.rs` dedupes palettes across models; GPU upload in `gpu_resources::upload_model_registry_incremental` consumes `dirty_model_ids`/`dirty_palette_ids`/`tier_dirty` to avoid full atlas repacks.
- `block_interaction.rs` — Block placement/breaking, hotbar, palette UI
- `water.rs` / `lava.rs` — Fluid simulation (cellular automata)
- `falling_block.rs`, `block_update.rs` — Gravity physics and frame-distributed physics queue
- `editor/` — In-game sub-voxel model editor (N key)
- `storage/metadata.rs` — `WorldMetadata` round-trips `time_of_day`, `day_cycle_paused`, seed, spawn_pos, world_gen type via `level.dat`

### Multiplayer architecture

The host runs both a `GameServer` and a loopback `GameClient` connected to itself. Remote clients connect over the network.

**Key patterns in `net/server.rs`:**
- **Dual player tracking**: `host_player: Option<PlayerInfo>` tracks the host separately from `self.players` (remote clients). The host's position is updated via `update_host_player()` each frame, not through the anti-cheat delta check.
- **Broadcast with originator exclusion**: `broadcast_encoded_except(channel, label, msg, exclude_client_id)` sends to all clients except one. Block changes, door toggles, and bulk operations use this to avoid echoing actions back to the originating client. The host's world is updated by receiving its own broadcasts as `NetworkEvent` messages through the normal client pipeline.
- **Epoch-aware chunk dedup**: `recently_sent_chunks` tracks `(sent_time, mutation_epoch)` per client per chunk position. `should_send_chunk_with_epoch()` skips re-sending a chunk if the epoch hasn't changed and the resend window hasn't expired. This prevents redundant chunk traffic when multiple clients request the same chunk.
- **Host loopback detection**: `update_player_state()` checks `host_client_id` and bypasses anti-cheat validation for the host's own client, allowing spawn-teleport and fly-mode without false positives. Remote clients get a `first_update` pass on their first position report.

**Authentication (`net/auth.rs`):**
- Secure mode with per-session `ServerAuthentication::Secure { private_key }`. The server generates a random 32-byte key at startup via `generate_random_key()`.
- Clients connect with a `ConnectToken` signed by the server's private key. The host's loopback client receives the key directly via `GameClient::with_key()`.
- `CONNECTION_TIMEOUT_MS = 30_000` — generous enough to survive frame stalls from chunk generation and GPU uploads.
- `PROTOCOL_ID` is a compile-time FNV-1a hash of `PROTOCOL_VERSION` ("voxel-world-2"), ensuring mismatched binaries fail at the netcode handshake.

**Client (`net/client.rs`):**
- `GameClient::with_key(addr, key)` for host loopback (key is known at startup).
- `GameClient::new(addr)` for remote clients (key obtained out-of-band, e.g. pairing code).
- `receive_messages()` enforces `MAX_INBOUND_MESSAGE_SIZE` on both raw bytes and bincode decode to prevent OOM from hostile peers.

### Chunk streaming & GPU upload pipeline

Performance-critical; the "origin shift" hot path runs through this trio:

- `chunk_loader.rs` — Background thread pool generates chunks off the main thread; epoch-bumping cancels in-flight work on origin shift.
- `world_streaming.rs` — Owns `MetadataState` (CPU-side SVT / brick-mask buffers with a pending queue). `check_and_shift_texture_origin()` performs the async texture clear, partitions chunks into near (immediate bulk upload) and far (queued in `reupload_queue`), and drains the queue across subsequent frames via `upload_world_to_gpu()`.
- `gpu_resources.rs` — `upload_chunks_batched()` packs chunks into staging buffers and issues merged `BufferImageCopy` regions. Zero-slice skip (`skip_zero_slices`) avoids memcpy/region emission for all-zero `model_metadata` / `custom_data` when the destination is known-zero (post-clear only). Z-adjacent chunks at the same (y, x) are merged into single multi-depth regions. Thread-local `STAGING_POOL` + `TRANSFER_RING` reuse HOST-visible buffers across uploads; `prewarm_staging_pool()` is called at init so the first origin shift never pays cold-alloc cost. `MultiplayerState::chunk_compression_cache` memoizes LZ4 output keyed on `Chunk::mutation_epoch` so the same chunk isn't re-compressed when streaming to multiple clients.
- `svt.rs` — `ChunkSVT` 64-bit brick mask + per-brick distance field for ray skipping.
- **Shift-reset optimization** — on origin-shift the metadata reset only pushes the tiny `chunk_bits` buffer (all-empty placeholder) to GPU; the larger `brick_masks` / `brick_distances` buffers stay stale for empty slots because `shaders/accel.glsl::isChunkEmpty` short-circuits before reading them. The incremental path writes brick data *before* flipping a chunk's bit to "not empty" to maintain that invariant.

### Shader files

- `traverse.comp` — Main ray marching shader
- `common.glsl` — Push constants, buffer layouts (includes `generated_constants.glsl`)
- `models.glsl` — Sub-voxel model ray marching
- `lighting.glsl` — Point lights, shadows, AO
- `materials.glsl` — Texture sampling, emission colors, `blockTypeToAtlasIndex()`

### Coordinate systems

- World coordinates: global block positions (i32)
- Chunk coordinates: chunk grid positions (i32), each chunk is 32³ blocks
- Local coordinates: position within chunk (0–31)
- Texture coordinates: block offset inside the 512×512×512 resident window; chunk N at (cx, cy, cz) lives at (cx·32 − origin.x, cy·32, cz·32 − origin.z)
- Sub-voxel: 8³ / 16³ / 32³ voxels per block, per-model resolution
- Helpers: `World::world_to_chunk()`, `World::world_to_local()`, `world_streaming::world_pos_to_chunk_index()`

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

- **Never** bypass `/voxel-texture` for block textures — it has safeguards against 3D renders and enforces flat 64×64 seamless tiling.
- Run `make sprite-gen` after every atlas change so the palette UI shows correct icons.
- Before committing a new texture: verify flat (not 3D-rendered), exact 64×64, seamless (check the 2×2 preview the skill writes).

## Adding New Block Types

1. Generate the 64×64 tileable texture: `/voxel-texture <name>` (writes to `textures/<name>_64x64.png`).
2. Add the variant to `BlockType` in `chunk.rs`, update `From<u8>`, `color()`, `break_time()`, and any property methods.
3. Update `BLOCK_PALETTE` in `src/ui/palette.rs` and the blocks array in `src/sprite_gen.rs`.
4. Rebuild `textures/texture_atlas.png` by appending the new 64×64 PNG at the correct atlas index (the order defines `blockTypeToAtlasIndex()` — see the atlas mapping table below). Any `magick +append <list> texture_atlas.png` invocation works; just keep the order in sync with the shader function.
5. Update `blockTypeToAtlasIndex()` in `shaders/materials.glsl` if the new slot isn't a direct `BlockType` → index mapping.
6. `make sprite-gen` to regenerate palette/hotbar sprites.
7. Test in-game.

**Note:** `BLOCK_<NAME>` constants and `ATLAS_TILE_COUNT` in GLSL are auto-generated by `build.rs` from `chunk.rs` / `constants.rs` — do not edit them manually.

## WARNING: Painted Block - User-Only

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
- CORRECT: `BlockType::Mud` for swamp surfaces (dedicated block)
- CORRECT: `BlockType::Sandstone` for desert subsurface (dedicated block)
- CORRECT: `BlockType::Cactus` for desert plants (dedicated block)
- WRONG: `set_painted_block(TEX_MUD, TINT_WHITE)` in terrain generation

## Block Types & Atlas Mapping

`BlockType` lives in `chunk.rs`. `build.rs` generates the matching `BLOCK_*` GLSL defines — do not hand-edit them. Current enum IDs:

```
0=Air, 1=Stone, 2=Dirt, 3=Grass, 4=Planks, 5=Leaves, 6=Sand, 7=Gravel,
8=Water, 9=Glass, 10=Log, 11=Model, 12=Brick, 13=Snow, 14=Cobblestone, 15=Iron, 16=Bedrock,
17=TintedGlass, 18=Painted, 19=Lava, 20=GlowStone, 21=GlowMushroom, 22=Crystal,
23=PineLog, 24=WillowLog, 25=PineLeaves, 26=WillowLeaves, 27=Ice,
28=Mud, 29=Sandstone, 30=Cactus, 31=DecorativeStone, 32=Concrete,
33=Deepslate, 34=Moss, 35=MossyCobblestone, 36=Clay, 37=Dripstone, 38=Calcite,
39=Terracotta, 40=PackedIce, 41=Podzol, 42=Mycelium, 43=CoarseDirt, 44=RootedDirt,
45=BirchLog, 46=BirchLeaves
```

**Block ID ≠ atlas index.** The atlas has 45 tiles (2880×64). `blockTypeToAtlasIndex()` in `shaders/materials.glsl` maps block IDs to tile positions; special slots 17/18 hold `grass_side` and `log_top`, which have no BlockType. When the mapping isn't a direct passthrough you must update that function alongside the atlas image.

Special metadata channels:
- **Emissive** (Lava, GlowStone, GlowMushroom, Crystal): emit light and glow.
- **Tinted** (TintedGlass, Crystal): `tint_data` (0–31) picks a color from `TINT_PALETTE` in `common.glsl`.
- **Painted**: `paint_data` packs texture index + tint index (19 textures × 32 tints = 608 variants).

## Sub-Voxel Model System

Models support three resolutions (8³, 16³, 32³) with 32-color palettes and per-slot emission. Key types in `sub_voxel.rs`:
- `ModelResolution` - Low (8³), Medium (16³), High (32³)
- `LightMode` - 10 animated light modes (Steady, Pulse, Flicker, Candle, Strobe, Breathe, Sparkle, Wave, WarmUp, Arc)
- `FIRST_CUSTOM_MODEL_ID = 176` - IDs 0-175 reserved for built-ins
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
- 110-118: Additional vegetation (moss carpet, glow lichen, roots, fern, dead bush, seagrass, blue flower)
- 119-134: Horizontal glass panes (16 connection variants)
- 135-150: Vertical glass panes (16 connection variants, rotatable)
- 176+: Custom user models

**Adding built-in models:** Edit `src/sub_voxel/builtins/mod.rs`, call `create_*()` functions in `register_builtins()`.

**Model editor** (N key): Tools include pencil, eraser, fill, eyedropper, rotate, mirror, cube, sphere.

## Key Constants

- `CHUNK_SIZE = 32` (chunk.rs)
- `BRICK_SIZE = 8` (svt.rs)
- `ModelResolution::Low/Medium/High` = 8/16/32 (sub_voxel.rs)
- `ATLAS_TILE_COUNT = 45.0` (materials.glsl and src/constants.rs)
- World: 16x16x16 chunks loaded = 512x512x512 blocks (Y bounded 0-511, X/Z infinite via streaming)
- View distance: 6 chunks (`VIEW_DISTANCE` in `constants.rs`)

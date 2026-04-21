# CLI Reference

Command-line options, environment variables, and Makefile targets for the Voxel World engine.

## Table of Contents

- [Overview](#overview)
- [Global Flags](#global-flags)
- [World & Spawn](#world--spawn)
- [Rendering & Quality](#rendering--quality)
- [Profiling & Benchmarking](#profiling--benchmarking)
- [Multiplayer](#multiplayer)
- [Automation](#automation)
- [Environment Variables](#environment-variables)
- [User Preferences](#user-preferences)
- [Makefile Targets](#makefile-targets)
- [Data Directory Layout](#data-directory-layout)
- [Related Documentation](#related-documentation)

## Overview

Voxel World uses `clap` for argument parsing. All options are passed as flags to the `voxel-world` binary. The Makefile wraps common invocations with the correct macOS Vulkan environment.

```bash
# Basic usage via Makefile (recommended)
make run

# Direct binary (macOS requires Vulkan env vars)
./target/release/voxel-world [OPTIONS]

# Pass extra args through Makefile
make run ARGS="--seed 42 --fly-mode"
```

Run `voxel-world --help` for the authoritative flag list.

## Global Flags

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--verbose` | | bool | off | Print debug output to console |
| `--debug-interval` | `-d` | u32 | `0` | Print debug info every N frames (0 = off) |
| `--generate-sprites` | | bool | off | Generate palette/hotbar sprites and exit |
| `--data-dir` | `-D` | string | current dir | Data directory for worlds, preferences, models |
| `--world` | `-w` | string | `"default"` | World name to load or create |

## World & Spawn

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--seed` | `-S` | u32 | `314159` | Seed for terrain generation |
| `--world-gen` | `-g` | enum | `normal` | World generation type |
| `--spawn-x` | `-x` | i32 | auto | Spawn X coordinate (auto-finds safe location) |
| `--spawn-z` | `-z` | i32 | auto | Spawn Z coordinate (auto-finds safe location) |

### World Generation Types

| Value | Description |
|-------|-------------|
| `normal` | Full biome-based terrain with caves, mountains, trees |
| `flat` | 2-chunk-thick world with grass/dirt/stone layers |
| `benchmark` | Controlled terrain with point lights and glass for profiling |

## Rendering & Quality

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--quality` | `-q` | enum | `medium` | Graphics quality preset |
| `--render-mode` | `-r` | string | `textured` | Debug render mode |
| `--view-distance` | `-v` | i32 | `6` | View distance in chunks |
| `--time-of-day` | `-t` | f64 | — | Pause day/night at time (0.0–1.0, 0.5 = noon) |
| `--show-chunk-boundaries` | `-b` | bool | off | Show chunk boundary visualization |
| `--fly-mode` | `-f` | bool | off | Start in fly mode |

### Quality Presets

Each preset configures render scale, lighting features, LOD distances, view distance, and atmosphere settings.

| Preset | Scale | AO | Shadows | Point Lights | View Dist | Ray Steps | Notes |
|--------|-------|-----|---------|-------------|-----------|-----------|-------|
| `potato` | 0.5 | off | off | off | 3 | 128 | Minimap off, water off, fog heavy |
| `low` | 0.6 | off | on | on | 4 | 192 | Clouds off, water off, aggressive LODs |
| `medium` | 0.75 | on | on | on | 6 | 256 | Default; balanced |
| `high` | 1.0 | on | on | on | 8 | 384 | Tinted shadows, long LODs |
| `ultra` | 1.5 | on | on | on | 12 | 512 | Supersampled, extreme LODs |

### Render Modes

| Value | Description |
|-------|-------------|
| `textured` | Normal textured rendering (default) |
| `normal` | Surface normal visualization |
| `coord` | Block coordinate display |
| `steps` | Ray step count heat map |
| `uv` | UV coordinate visualization |
| `depth` | Depth buffer visualization |
| `brickdebug` | SVT brick mask debug |
| `shadowdebug` | Shadow map debug |

## Profiling & Benchmarking

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--profile` | `-p` | bool | off | Write per-second CSV samples to `profiles/` |
| `--auto-profile` | `-P` | bool | off | Cycle each feature flag (5s off, 5s on) then exit |
| `--benchmark-duration` | | f64 | — | Auto-exit after N seconds |
| `--benchmark-terrain` | | enum | `flat` | Benchmark terrain style |

### Benchmark Terrain Styles

Only used with `--world-gen benchmark`:

| Value | Description |
|-------|-------------|
| `flat` | Flat terrain at Y=100 |
| `hills` | Rolling hills with sine-wave variation Y=90–110 |

## Multiplayer

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--multiplayer` | bool | off | Enable multiplayer mode |
| `--host` | bool | off | Host an integrated server |
| `--connect` | string | — | Connect to server at `ADDRESS:PORT` |
| `--port` | u16 | `5000` | Server port for hosting or connecting |

### Multiplayer Examples

```bash
# Host a LAN game on port 12345
make run-host

# Connect to a host
make run-client

# Custom host configuration
./target/release/voxel-world --host --port 8080 --seed 42

# Connect to remote server
./target/release/voxel-world --connect 192.168.1.100:5000
```

The host runs both a `GameServer` and a loopback `GameClient`. Remote clients connect over encrypted UDP via the Netcode protocol. Max 4 players, 20 Hz tick rate. LAN discovery broadcasts on port 5001.

## Automation

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--auto-fly` | | bool | off | Auto-move player for benchmarking (implies `--fly-mode`) |
| `--auto-fly-speed` | | f64 | `20.0` | Auto-fly speed in blocks/second |
| `--auto-fly-pattern` | | enum | `straight` | Movement pattern |
| `--screenshot-delay` | `-s` | f64 | — | Take screenshot after N seconds |
| `--exit-delay` | `-e` | f64 | — | Exit after N seconds |

### Auto-Fly Patterns

| Value | Description |
|-------|-------------|
| `straight` | Move in +X direction |
| `spiral` | Outward spiral pattern |
| `grid` | Zig-zag grid pattern |

### Screenshot Automation

Combine `--screenshot-delay` and `--exit-delay` for automated captures:

```bash
# Screenshot at 3s, exit at 4s
make run-cap-exit

# Custom timing
./target/release/voxel-world --screenshot-delay 5 --exit-delay 6 --seed 42
```

Screenshot saved to `voxel-world_screen_shot.png`.

## Environment Variables

### Vulkan Setup (macOS)

The Makefile sets these automatically. Set them manually when running the binary directly:

| Variable | Value | Purpose |
|----------|-------|---------|
| `DYLD_LIBRARY_PATH` | `/opt/homebrew/lib:/opt/homebrew/opt/vulkan-loader/lib` | MoltenVK library search path |
| `DYLD_FALLBACK_LIBRARY_PATH` | `/opt/homebrew/lib:/opt/homebrew/opt/vulkan-loader/lib` | Fallback library path |
| `VK_ICD_FILENAMES` | `/opt/homebrew/etc/vulkan/icd.d/MoltenVK_icd.json` | Vulkan ICD (MoltenVK) |

### Perf Tuning

Chunk streaming and GPU upload budgets are overridable at runtime:

| Variable | Default | Purpose |
|----------|---------|---------|
| `ORIGIN_SHIFT_PROFILE` | off | Per-shift timing profile on stderr |
| `ORIGIN_SHIFT_NEAR_RADIUS` | `view_distance` | Sync-upload radius on origin shift |
| `METADATA_CHUNKS_PER_FRAME` | `128` | SVT metadata rebuilds per frame |
| `METADATA_RESET_BUDGET` | `256` | Per-frame budget during full metadata reseed |
| `REUPLOAD_PER_FRAME` | `256` | Chunks drained from reupload queue per frame |
| `UPLOADS_PER_FRAME` | `256` | Dirty chunks uploaded to GPU per frame |

Smaller values reduce frame stalls but may cause visible pop-in after origin shifts.

## User Preferences

Settings are persisted to `user_prefs.json` in the data directory. The file is created automatically with defaults on first run.

### In-Game Settings (Esc Panel)

| Setting | Default | Description |
|---------|---------|-------------|
| Ambient Occlusion | on | Smooth corner darkening |
| Shadows | on | Directional sunlight shadows |
| Model Shadows | on | Shadows on sub-voxel models |
| Point Lights | on | Dynamic lights from torches, lava, etc. |
| Tinted Shadows | off | Colored shadows through tinted glass |
| Hide Ground Cover | off | Remove grass/flowers for visibility |
| Water Simulation | on | Dynamic water/lava flow |
| Instant Break | on | No break cooldown |
| Instant Place | on | No place cooldown |
| Break Cooldown | 0.05s | Delay between breaks (when not instant) |
| Place Cooldown | 0.5s | Delay between places (when not instant) |
| Fly Collision | off | Collision detection in fly mode |
| Dynamic Render Scale | off | Auto-adjust render scale for target FPS |
| Max Custom Textures | 32 | Texture slots for hosted servers |

### Persistence

Preferences are saved automatically on exit and restored on next launch:

| Data | Storage |
|------|---------|
| Settings, hotbar layout | `user_prefs.json` |
| Per-world position/rotation | `user_prefs.json` → `world_player_data` |
| Saved positions (`/locate`) | `user_prefs.json` → `saved_positions` |
| Console history | `user_prefs.json` → `console_history` |
| Author name | `user_prefs.json` → `author` |
| Recently played worlds | `user_prefs.json` → `recent_worlds` (max 10) |

## Makefile Targets

### Build & Run

| Target | Description |
|--------|-------------|
| `make build` | Build release (default) |
| `make build-release` | `cargo build --release` |
| `make build-debug` | `cargo build` (debug) |
| `make run` | Build and run release |
| `make run-debug` | Build and run debug with `RUST_BACKTRACE=1` |

### Quality Presets

| Target | Description |
|--------|-------------|
| `make run-potato` | `--quality potato` |
| `make run-low` | `--quality low` |
| `make run-medium` | `--quality medium` |
| `make run-high` | `--quality high` |
| `make run-ultra` | `--quality ultra` |

### Quality Checks

| Target | Description |
|--------|-------------|
| `make fmt` | Format code (`cargo fmt`) |
| `make lint` | Run clippy linter (`cargo clippy -- -D warnings`) |
| `make test` | Run tests (`cargo test`) |
| `make checkall` | Format, lint, and test — run after making changes |

### Multiplayer

| Target | Description |
|--------|-------------|
| `make run-host` | Start as LAN host (port 12345, isolated `data_host/`) |
| `make run-client` | Join LAN host (connects to `127.0.0.1:12345`) |
| `make reset-host` | Reset host-side save/state |
| `make reset-client` | Reset client-side save/state |

### Multi-Instance

| Target | Description |
|--------|-------------|
| `make run-p1` | Player 1 (isolated `data_p1/`, seed 314159) |
| `make run-p2` | Player 2 (isolated `data_p2/`, seed 99999) |
| `make reset-p1` | Reset player 1 data |
| `make reset-p2` | Reset player 2 data |

### Benchmarking

All benchmarks use `--auto-fly` and write CSV profile data to `profiles/`. Durations are kept to ~45s to stay inside sustained thermal envelope on Apple Silicon.

| Target | Description |
|--------|-------------|
| `make benchmark` | Flat terrain, 45s at 2x speed |
| `make benchmark-hills` | Hilly terrain, 45s |
| `make benchmark-spiral` | Spiral flight pattern, 90s |
| `make benchmark-normal` | Normal terrain, 45s |
| `make benchmark-stress` | Full-speed stress run |
| `make benchmark-cap` | Short run with screenshot at 25s |
| `make benchmark-compare` | Thermal-aware A/B comparison |

```bash
# Compare two profile CSVs
make benchmark-compare ARGS="profiles/run_a.csv profiles/run_b.csv"
```

### Profiling

| Target | Description |
|--------|-------------|
| `make profile` | Verbose profile run (view distance 8, fly mode) |
| `make auto-profile-flat` | 45s feature-flag cycle on flat terrain |
| `make auto-profile-normal` | 45s feature-flag cycle on normal terrain |

### Utilities

| Target | Description |
|--------|-------------|
| `make sprite-gen` | Generate palette/hotbar sprites to `textures/rendered/` |
| `make run-cap-exit` | Run, screenshot at 3s, exit at 4s |
| `make new-flat` | Reset and create flat world (seed 314159) |
| `make new-normal` | Reset and create normal world (seed 314159) |
| `make clean` | `cargo clean` |
| `make reset-world` | Delete `worlds/` and `user_prefs.json` |
| `make reset-profiles` | Delete `profiles/` |
| `make reset` | Delete all data (worlds, prefs, profiles) — confirms first |

## Data Directory Layout

```
data_dir/
├── user_prefs.json        # Settings, hotbar, positions, console history
├── worlds/
│   └── default/           # World save data
│       ├── level.dat      # World metadata (seed, gen type, spawn)
│       └── region/        # Region files (.vxr)
├── user_models/           # Custom sub-voxel models (.vxm + PNG preview)
├── user_stencils/         # Custom stencils (.vxs + PNG preview)
├── user_templates/        # Custom build templates
└── profiles/              # CSV performance profiles from benchmarking
```

Override the data directory with `--data-dir` or `-D`. Multiplayer and multi-instance targets use isolated directories (`data_host/`, `data_client/`, `data_p1/`, `data_p2/`).

## Related Documentation

- [Architecture](ARCHITECTURE.md) — System design and module organization
- [Documentation Style Guide](DOCUMENTATION_STYLE_GUIDE.md) — Standards for project documentation
- [README.md](../README.md) — Project overview, features, and getting started

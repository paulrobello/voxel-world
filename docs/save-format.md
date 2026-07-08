# Save Format

On-disk persistence format for Voxel World — region files, chunk serialization, and the sidecar files that hold world-level state. Intended for maintainers who need to read or migrate save data.

## Table of Contents

- [Overview](#overview)
- [Directory Layout](#directory-layout)
- [Region Files (.vxr)](#region-files-vxr)
- [Chunk Serialization](#chunk-serialization)
- [level.dat (World Metadata)](#leveldat-world-metadata)
- [models.dat (Custom Model Store)](#modelsdat-custom-model-store)
- [door_pairs.dat](#door_pairsdat)
- [fluid_sources.json](#fluid_sourcesjson)
- [stencil_state.json](#stencil_statejson)
- [.vxm Library Files](#vxm-library-files)
- [Atomic Writes](#atomic-writes)
- [Per-Server Cache Directory](#per-server-cache-directory)
- [Versioning & Migration](#versioning--migration)
- [Related Documentation](#related-documentation)

## Overview

**Purpose:** Voxel World persists a world as a directory of files: a `region/` subtree of chunked region files plus several sidecar files for metadata, models, fluids, and stencils. Chunks are the bulk of the data; everything else is small and rewritten wholesale via atomic temp-file-and-rename.

**Key design properties:**

- Region files use a fixed-size header with a location table — chunks are addressed by sector, not appended linearly, so writes are O(1) per chunk.
- Chunks are `postcard`-serialized then `zstd`-compressed (level 3) before being written into a region sector.
- Small sidecars (`level.dat`, `models.dat`, `door_pairs.dat`, `fluid_sources.json`, `stencil_state.json`) are written atomically: write to `<name>.tmp`, `fsync`, `rename` over the target.
- Two independent version counters exist: `REGION_VERSION` (region file header marker) and `FORMAT_VERSION` (chunk payload, inside the postcard). See [Versioning & Migration](#versioning--migration).

**Tech stack:** `postcard` (compact Rust serialization), `zstd` (chunk compression), `serde_json` (human-readable sidecars), `bincode` is NOT used for storage (it was migrated to postcard).

## Directory Layout

A world lives in a single directory: `world_dir = worlds_directory.join(world_name)` (`src/app/init.rs:100`). The default world name is `default`. A legacy top-level `world/` directory is migrated to `<worlds_dir>/default` on first run (`src/app/init.rs:103-109`).

```text
<world_dir>/
├── level.dat              # World metadata (JSON, atomic write)
├── models.dat             # Custom sub-voxel models (postcard, atomic write)
├── door_pairs.dat         # Custom door pair definitions (postcard, atomic write)
├── fluid_sources.json     # Water/lava source block positions (JSON, atomic write)
├── stencil_state.json     # Active stencil placements (JSON, atomic write)
└── region/
    ├── r.0.0.vxr          # Region file for region coords (rx=0, rz=0)
    ├── r.0.1.vxr
    └── ...
```

The `worlds_directory` root is configured at startup (`src/app/init.rs:39`). Multi-instance runs use isolated roots: `data_p1/` and `data_p2/` for `make run-p1` / `make run-p2`. Remote-client sessions use a per-server cache directory (see [Per-Server Cache Directory](#per-server-cache-directory)).

Region files are named `r.<rx>.<rz>.vxr` and live under `<world_dir>/region/` (`src/storage/worker.rs:54-58`). Each region covers a 32×32 column of chunks spanning the full world height (16 chunks, Y=0..15 → block Y=0..511).

## Region Files (.vxr)

Source: `src/storage/region.rs`.

A region file stores a 32×32×16 column of chunks (16,384 chunk slots). The on-disk layout (v2) is:

```text
[ location table  : CHUNKS_PER_REGION * 4 bytes (u32 BE each)        ]
[ timestamp table : CHUNKS_PER_REGION * 4 bytes (u32 BE each)        ]
[ marker          : 4-byte magic "VXRF" + 4-byte version (BE) = 2    ]
[ generation      : 4-byte write-generation counter (BE)             ]
[ zero-padded to the next SECTOR_SIZE boundary                       ]
[ data sectors    : SECTOR_SIZE each                                  ]
```

Constants (`src/storage/region.rs:37-60`):

| Constant | Value | Meaning |
|---|---|---|
| `CHUNKS_PER_REGION_SIDE` | 32 | Chunks per region along X and Z |
| `REGION_HEIGHT` | 16 | Chunks per region along Y (full world height) |
| `CHUNKS_PER_REGION` | 16,384 | Total chunk slots per region file |
| `SECTOR_SIZE` | 4,096 | Bytes per sector (allocation unit) |
| `REGION_MAGIC` | `b"VXRF"` | Marker written at `MARKER_OFFSET` |
| `REGION_VERSION` | 2 | Region file format version |
| `HEADER_SIZE` | 135,168 (33 × 4096) | Padded header size |
| `MARKER_OFFSET` | 131,072 | Byte offset of magic in header |
| `GENERATION_OFFSET` | 131,080 | Byte offset of generation counter |

### Location table

Each of the 16,384 entries is a packed `u32` (big-endian): the high 24 bits are the sector offset, the low 8 bits are the sector count. A zero entry means the chunk slot is empty. The chunk index is computed as:

```text
chunk_index(x, y, z) = lx + lz * 32 + ly * 1024
```

where `lx = x.rem_euclid(32)`, `ly = y`, `lz = z.rem_euclid(32)` (`src/storage/region.rs:310-323`). Y is clamped to `[0, REGION_HEIGHT)`; out-of-range Y returns `Err` rather than aliasing — this is the STOR-001 fix (the old v1 formula used `REGION_HEIGHT = 8`, which clamped Y=8..15 into slots 0..7 and silently aliased edits at block Y≥256 onto the chunk 256 blocks below).

### Data sector layout

Each allocated sector range holds:

```text
[ data_len : u32 BE ][ compressed chunk data ][ zero padding to sector boundary ]
```

The 4-byte `data_len` prefix gives the exact byte count of the compressed chunk; the rest of the last sector is zero padding (`src/storage/region.rs:368-401`). Reads validate that `data_len` fits within `sector_count * SECTOR_SIZE - 4` (`region.rs:347-359`).

### Sector allocation

Allocation is append-only: if the existing slot is too small (or empty), the writer allocates fresh sectors at end of file (`region.rs:380-387`). There is no sector free-list; old sectors become dead space. This keeps writes O(1) but means region files grow monotonically — compaction is not implemented.

### Write-generation counter (STOR-003)

The 4-byte `generation` counter at `GENERATION_OFFSET` is bumped by `write_chunk` **after** the location entry is durable and flushed (`region.rs:422-431`). It occupies bytes that were zero padding in STOR-001 v2 files, so it is a backward-compatible additive refinement — no `REGION_VERSION` bump.

A separate read handle (e.g. `ParallelStorageReader`) caches the location table at open. Before each read it calls `refresh_if_stale`, which reads the 4-byte on-disk generation and, if it advanced, re-reads the full location + timestamp tables (`region.rs:444-482`). This is how a reader on one thread sees chunks a writer on another thread appended after the reader opened the file.

## Chunk Serialization

Source: `src/storage/format.rs`, `src/storage/mod.rs`.

A chunk is serialized into the `SerializedChunk` struct, then encoded with `postcard`, then compressed with `zstd` at level 3 (`src/storage/mod.rs:17-24`). The compressed bytes are what the region file stores.

```text
SerializedChunk
  │
  ▼ postcard::to_stdvec
postcard bytes
  │
  ▼ zstd::encode_all(.., 3)
compressed bytes  ──►  region data sector
```

### SerializedChunk fields

`FORMAT_VERSION = 4` (`src/storage/format.rs:6`). The version is stored as the `version` field **inside** the postcard payload, not as a separate external byte — readers deserialize the postcard first, then check `serialized.version` (`src/storage/mod.rs:96-101`). This is the current behavior; a version mismatch (`> FORMAT_VERSION` or `== 0`) is rejected.

| Field | Type | Meaning |
|---|---|---|
| `version` | `u8` | `FORMAT_VERSION = 4` |
| `flags` | `u8` | Bitmask; bit 0 = `FLAG_GENERATED` |
| `block_data` | `Vec<u8>` | 32³ = 32,768 bytes, one `BlockType` byte per block |
| `metadata` | `Vec<BlockMeta>` | Sparse model-block metadata |
| `tinted` | `Vec<TintMeta>` | Sparse tinted-glass metadata (added v2) |
| `painted` | `Vec<PaintMeta>` | Sparse painted-block metadata (added v2) |
| `frames` | `Vec<FrameMeta>` | Sparse model custom_data e.g. picture frames (added v3) |

The `tinted`, `painted`, and `frames` vectors use `#[serde(default)]` so older chunks (versions 1–2) load cleanly with empty defaults for the newer channels (`src/storage/format.rs:82-89`).

### BlockMeta packing

`BlockMeta.data` is a packed `u16` (`src/storage/format.rs:18-39`):

| Bits | Field |
|---|---|
| 0–7 | `model_id` (8 bits) |
| 8–9 | rotation facing (2 bits) |
| 10 | waterlogged flag (1 bit) |
| 11–14 | frame edge mask (4 bits, stored as bits 3–6 of the rotation value) |

## level.dat (World Metadata)

Source: `src/storage/metadata.rs`.

`level.dat` is a **pretty-printed JSON** file (not postcard) holding the `WorldMetadata` struct (`src/storage/metadata.rs:18-41`). It is written atomically (see [Atomic Writes](#atomic-writes)).

| Field | Type | Default | Meaning |
|---|---|---|---|
| `seed` | `u32` | — | Terrain generation seed |
| `spawn_pos` | `[f64; 3]` | — | World spawn position |
| `version` | `u32` | — | On-disk metadata version; current writers emit `2` |
| `time_of_day` | `f32` | `14.0/24.0` (2pm) | Current time of day, 0.0–1.0 |
| `day_cycle_paused` | `bool` | `false` | Whether the day/night cycle is paused |
| `world_gen` | `WorldGenType` | `Normal` | `Normal` or `Flat` terrain generation |
| `measurement_markers` | `Vec<[i32; 3]>` | `[]` | Rangefinder tool marker positions |
| `player_modified` | `bool` | `false` | True once the world has genuine local edits (STOR-004) |

The `player_modified` flag gates client-side saves: a cached/downloaded server world does not overwrite a different local world's data (`metadata.rs:34-40`). It defaults to `false` via `#[serde(default)]` so old `level.dat` files (version 1, no field) load cleanly as "not player-modified".

`version = 1` files predate the `player_modified` flag; `version = 2` is the current writer. There is no automatic migration — the version is informational and the `#[serde(default)]` fields handle backwards compatibility.

## models.dat (Custom Model Store)

Source: `src/storage/model_format.rs`.

`models.dat` stores custom sub-voxel models (IDs ≥ `FIRST_CUSTOM_MODEL_ID = 176`) so they survive world reload. It is a **postcard**-serialized `WorldModelStore`, written atomically (`src/storage/model_format.rs:323-365`).

```rust
pub struct WorldModelStore {
    pub version: u16,           // currently 1
    pub first_custom_id: u8,    // 176 — the first custom model ID
    pub models: Vec<VxmFile>,   // custom models in ID order; ID = first_custom_id + index
}
```

The store holds custom models **in ID order** — store index `i` maps to model ID `first_custom_id + i` (`model_format.rs:343-357`). This is the MDL-001 / STOR-001 stable-ID fix: snapshotting in ID order means the same model gets the same ID across reloads even if the library directory order changes.

`models.dat` is loaded **before** the `.vxm` library at world open (`src/app/init.rs:299-321`), so saved custom models re-register at their stable IDs before any library-only models acquire the next free IDs.

## door_pairs.dat

Source: `src/storage/model_format.rs:402-466`.

`door_pairs.dat` stores custom door pair definitions as a **postcard**-serialized `DoorPairStore`, written atomically.

```rust
pub struct DoorPairStore {
    pub version: u16,              // currently 1
    pub door_pairs: Vec<SimpleDoorPair>,  // ID = index
}
```

## fluid_sources.json

Source: `src/storage/fluid_sources.rs`.

`fluid_sources.json` is a **pretty-printed JSON** file holding water and lava source block positions so fluid simulation continues correctly after reload (`fluid_sources.rs:13-39`). Written atomically.

```rust
pub struct FluidSources {
    pub water: Vec<[i32; 3]>,  // water source block positions
    pub lava: Vec<[i32; 3]>,   // lava source block positions
}
```

File name constant: `FluidSources::FILE_NAME = "fluid_sources.json"` (`fluid_sources.rs:24`). A missing file loads as empty (backwards compatibility).

## stencil_state.json

Source: `src/storage/stencil_state.rs`.

`stencil_state.json` is a **pretty-printed JSON** file holding active holographic stencil placements (`stencil_state.rs:14-108`). Written atomically.

```rust
pub struct StencilState {
    pub active_stencils: Vec<PlacedStencil>,
    pub next_id: u64,
    pub global_opacity: f32,      // default 0.5
    pub render_mode: StencilRenderMode,  // wireframe or solid
}
```

File name constant: `StencilState::FILE_NAME = "stencil_state.json"` (`stencil_state.rs:51`). A missing file loads as default state.

## .vxm Library Files

Source: `src/storage/model_format.rs:17-178`.

`.vxm` files are the **portable, shareable** model format stored in the user-wide library directory (`user_models_dir`), not per-world. They are **postcard**-serialized `VxmFile` structs (`model_format.rs:222`).

```text
Magic: "VXM2" (4 bytes, VXM_MAGIC)
Version: 3 (VXM_VERSION)
```

```rust
pub struct VxmFile {
    pub magic: [u8; 4],              // "VXM2"
    pub version: u16,                // 3
    pub resolution: u8,              // 8, 16, or 32
    pub name: String,
    pub author: String,
    pub creation_date: u64,          // unix epoch seconds
    pub palette: Vec<u32>,           // 32 RGBA8888 packed entries
    pub palette_emission: Vec<f32>,  // 32 per-slot emission intensities
    pub voxels: Vec<u8>,             // resolution³ palette indices
    pub properties: ModelProps,      // collision, light, rotation flags
}
```

The library directory is shared across all worlds. `LibraryManager::list_models` returns names **sorted** so new-model ID assignment is reproducible across runs and platforms (`model_format.rs:248-268`). Filenames are sanitized to `[A-Za-z0-9_-]` (`model_format.rs:206-216`).

## Atomic Writes

Source: `src/storage/atomic.rs`.

All small sidecar files (`level.dat`, `models.dat`, `door_pairs.dat`, `fluid_sources.json`, `stencil_state.json`) are written via `atomic_write_bytes` (`src/storage/atomic.rs:21-30`), the STOR-006 fix:

1. Write the full payload to `<filename>.tmp` in the same directory.
2. `fsync` the temp file.
3. `std::fs::rename` the temp file over the target.

Because the temp file is a sibling of the destination, the rename stays on the same filesystem and is atomic. A crash leaves either the old file or the fully-written new file, never a half-written one. A stale `.tmp` from a prior crashed run is overwritten cleanly on the next write (`atomic.rs:18-20`).

Region files do **not** use this path — they are mutated in place (location table + sector appends) because they are large and updated incrementally.

## Per-Server Cache Directory

Source: `src/user_prefs.rs:128-136` (STOR-005).

When a client connects to a remote server, its downloaded/cached world is stored in a per-server directory so different servers do not collide:

```text
<worlds_dir>/remote__<sanitized_addr>__<fnv1a_hex>
```

- `<sanitized_addr>` — the server address with every character not in `[A-Za-z0-9_.-]` replaced by `_` (`sanitize_server_addr`, `user_prefs.rs:109-124`). For example `127.0.0.1:12345` → `127.0.0.1_12345`.
- `<fnv1a_hex>` — 8 lowercase hex digits of the FNV-1a 32-bit hash of the **raw** (unsanitized) address (`fnv1a_32`, `user_prefs.rs:95-107`). This suffix disambiguates addresses that sanitize to the same string (e.g. `:` and `;` both become `_`).

`enter_remote_client_mode` swaps the active `world_dir` and storage handle to this per-server directory (`src/app_state/multiplayer.rs`), so all sidecar files and region writes land there. Client saves are additionally gated by the `player_modified` flag (STOR-004) so a cached server world is not written back unless the player genuinely edited it locally.

## Versioning & Migration

There are **two independent** version counters. They are not related:

### Region file version (`REGION_VERSION`)

- Stored in the header marker at `MARKER_OFFSET` alongside the `VXRF` magic (`region.rs:50-51, 110-116`).
- Current value: `2`.
- v1 files (pre-STOR-001, 8-height, 64 KiB header, no marker) are detected on open and atomically rebuilt into the v2 layout (`region.rs:172-308`). The old index formula was identical (`lx + lz*32 + ly*1024` with `ly` in 0..7), so old slots map 1:1 onto the same indices in the new larger table; the high (y≥8) half held aliased/corrupt data and is intentionally dropped. Live chunk records are copied byte-for-byte.
- The write-generation counter (STOR-003) occupies bytes that were zero padding in v2 files written by STOR-001, so it did not bump `REGION_VERSION`.

### Chunk payload version (`FORMAT_VERSION`)

- Stored as the `version` field **inside** the `SerializedChunk` postcard payload (`format.rs:6, 73-74`).
- Current value: `4`.
- Readers deserialize the postcard first, then check `serialized.version` (`mod.rs:96-101`). A version `> FORMAT_VERSION` or `== 0` is rejected; older versions load with `#[serde(default)]` filling in newer channels.
- History: v1 = block data + model metadata; v2 = added `tinted` + `painted`; v3 = added `frames` (custom_data for picture frames); v4 = current.
- Note (STOR-M03): because the version lives inside the postcard payload rather than in an external byte, a corrupt or truncated chunk cannot be rejected by version alone — the postcard must decode first. This is the current behavior.

### Sidecar versions

`WorldMetadata.version`, `WorldModelStore.version`, and `DoorPairStore.version` are each `u16`/`u32` fields inside their respective payloads. They are informational; backwards compatibility is handled by `#[serde(default)]` rather than by branching on version. Current writer values: `level.dat` version `2`, `models.dat` version `1`, `door_pairs.dat` version `1`.

## Related Documentation

- [Architecture](ARCHITECTURE.md) — system design, including the chunk streaming pipeline that consumes these files
- [Networking](NETWORKING.md) — chunk sync protocol (uses the same `SerializedChunk` over the wire)
- [Model Editor](MODEL_EDITOR.md) — the `.vxm` library and custom model workflow
- `CLAUDE.md` — "Sub-Voxel Model System" section covers model IDs and the `FIRST_CUSTOM_MODEL_ID` boundary

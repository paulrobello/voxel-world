# Refactoring Plan: Voxel World

This document outlines the strategy for decomposing large files into smaller, more manageable modules.

## Status Summary

| File | Lines (Original) | Status | Refactored To |
|------|------------------|--------|---------------|
| `src/sub_voxel.rs` | 2166 | ✅ Done | `src/sub_voxel/` |
| `src/sub_voxel_builtins.rs` | 1800 | ✅ Done | `src/sub_voxel/builtins/` |
| `src/main.rs` | 2128 | ⏳ Pending | `src/app/` (Logic), `src/render/` (Setup) |
| `src/world.rs` | 2104 | ✅ Done | `src/world/` |
| `src/hud_render.rs` | 2005 | ⏳ Pending | `src/ui/` |

---

## 1. Sub-Voxel System (COMPLETED)

Successfully decomposed `src/sub_voxel.rs` and `src/sub_voxel_builtins.rs` into a modular structure under `src/sub_voxel/`.

### Structure:
- `src/sub_voxel/mod.rs`: Re-exports and high-level API.
- `src/sub_voxel/types.rs`: Core enums and structs (`ModelResolution`, `LightMode`, etc.).
- `src/sub_voxel/model.rs`: `SubVoxelModel` implementation and voxel manipulation.
- `src/sub_voxel/registry.rs`: `ModelRegistry` management and GPU packing.
- `src/sub_voxel/builtins/`: Categorized built-in models.

---

## 2. World Management (COMPLETED)

Successfully decomposed `src/world.rs` into a modular structure under `src/world/`.

### Structure:
- `src/world/mod.rs`: Re-exports and type definitions (`ChunkPos`, `WorldPos`).
- `src/world/storage.rs`: `World` struct, chunk storage, dirty tracking, block accessors.
- `src/world/lighting.rs`: Light collection and emission logic (`collect_torch_lights`).
- `src/world/query.rs`: Height cache and minimap generation.
- `src/world/connections.rs`: Fence, gate, and window connection logic.
- `src/world/stair_logic.rs`: Stair corner shape calculation.
- `src/world/tree_logic.rs`: Tree detection and validation.
- `src/world/world_gen.rs`: World generation methods.
- `src/world/tests.rs`: Complete test suite (all 19 tests passing).

---

## 3. World Management (Original Notes)

`src/world.rs` currently manages chunk storage, light collection, height caches, and block-level access.

### Proposed Structure (`src/world/`):
- `mod.rs`: Re-exports.
- `storage.rs`: `World` struct and chunk `HashMap`.
- `lighting.rs`: `collect_torch_lights` and emission logic.
- `query.rs`: Block access, raycasting integration, height cache.
- `stair_logic.rs`: Complex stair shape auto-calculation.

---

## 3. HUD and UI (`src/hud_render.rs`)

This file is a massive collection of `egui` code.

### Proposed Structure (`src/ui/`):
- `mod.rs`: Renderer entry point.
- `palette.rs`: Block/Model palette selection UI.
- `stats.rs`: Performance and debug overlays.
- `settings.rs`: Atmosphere and game settings menus.
- `hotbar.rs`: Inventory/Hotbar rendering.
- `minimap.rs`: Minimap integration.

---

## 4. Main Application (`src/main.rs`)

`src/main.rs` is over 2000 lines and contains Vulkan setup, window event handling, and game loop logic.

### Proposed Structure:
- `src/main.rs`: Minimal entry point and event loop.
- `src/app/mod.rs`: `VoxelApp` struct managing game state.
- `src/app/events.rs`: Keyboard/Mouse input handling.
- `src/render/mod.rs`: Vulkan device/swapchain management.
- `src/render/pipelines.rs`: Pipeline initialization.
- `src/render/frame.rs`: Frame-by-frame command buffer building.

---

## 5. Verification Process

After each phase:
1. Run `cargo check` / `make lint`.
2. Run `make test`.
3. Verify in-game functionality.
4. Commit before starting next phase.

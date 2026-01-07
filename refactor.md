# Refactoring Plan: Voxel World

This document outlines the strategy for decomposing large files into smaller, more manageable modules.

## Status Summary

| File | Lines (Original) | Status | Refactored To |
|------|------------------|--------|---------------|
| `src/sub_voxel.rs` | 2166 | ✅ Done | `src/sub_voxel/` |
| `src/sub_voxel_builtins.rs` | 1800 | ✅ Done | `src/sub_voxel/builtins/` |
| `src/main.rs` | 2128 | ✅ Done | `src/app/`, `src/app_state/`, `src/world_init/` |
| `src/world.rs` | 2104 | ✅ Done | `src/world/` |
| `src/hud_render.rs` | 2005 | ✅ Done | `src/ui/` |

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

## 3. HUD and UI (COMPLETED)

Successfully decomposed `src/hud_render.rs` into a modular structure under `src/ui/`.

### Structure:
- `src/ui/mod.rs`: HUDRenderer struct, render coordination, and module exports.
- `src/ui/helpers.rs`: Shared utility functions (tint_color, sprite_for_item, atlas_tile_for, etc.).
- `src/ui/time.rs`: Time parsing and formatting utilities with comprehensive tests.
- `src/ui/stats.rs`: Performance overlays (FPS, chunks, fluid stats, position, biome debug).
- `src/ui/console.rs`: Command console with history navigation and fluid debug output.
- `src/ui/palette.rs`: Block/model palette window with drag-and-drop support.
- `src/ui/hotbar.rs`: 9-slot hotbar with drag preview and block name display.
- `src/ui/minimap.rs`: Minimap and compass rendering.
- `src/ui/settings.rs`: Comprehensive settings window with multiple collapsible sections.

### Key Improvements:
- **Modularity**: Each UI component has its own file with dedicated struct
- **Reusability**: Shared utilities reduce code duplication
- **Maintainability**: Smaller, focused files (125-469 lines each)
- **Type Safety**: All functions properly typed with clear signatures
- **Documentation**: Module-level doc comments for each component

---

## 4. World Management (Original Notes)

`src/world.rs` currently manages chunk storage, light collection, height caches, and block-level access.

### Proposed Structure (`src/world/`):
- `mod.rs`: Re-exports.
- `storage.rs`: `World` struct and chunk `HashMap`.
- `lighting.rs`: `collect_torch_lights` and emission logic.
- `query.rs`: Block access, raycasting integration, height cache.
- `stair_logic.rs`: Complex stair shape auto-calculation.

---

## 5. HUD and UI (Original Notes)

This file is a massive collection of `egui` code.

### Proposed Structure (`src/ui/`):
- `mod.rs`: Renderer entry point.
- `palette.rs`: Block/Model palette selection UI.
- `stats.rs`: Performance and debug overlays.
- `settings.rs`: Atmosphere and game settings menus.
- `hotbar.rs`: Inventory/Hotbar rendering.
- `minimap.rs`: Minimap integration.

---

## 4. Main Application (COMPLETED)

Successfully decomposed `src/main.rs` into modular structures under `src/app/`, `src/app_state/`, and `src/world_init/`.

### Structure:

**`src/app/` - Application Logic (1847 lines total)**
- `mod.rs`: Module organization and re-exports
- `core.rs`: App struct definition and core methods (selected_block, resolve_player_overlap, toggle_palette_panel, save_preferences)
- `init.rs`: App::new() - Massive Vulkan initialization (~456 lines)
- `update.rs`: App::update() - Game loop update logic (~290 lines)
- `render.rs`: App::render() - Rendering pipeline (~565 lines)
- `event_handler.rs`: ApplicationHandler implementation - Window/device events (~158 lines)
- `input.rs`: Input handling methods (moved from app_input.rs, ~500 lines)
- `hud.rs`: HUD rendering helpers (moved from app_hud.rs, ~160 lines)
- `minimap.rs`: Minimap update logic (moved from app_minimap.rs, ~60 lines)
- `stats.rs`: Statistics collection (moved from app_stats.rs, ~140 lines)
- `helpers.rs`: Future helper functions (empty placeholder)

**`src/app_state/` - State Structures (345 lines total)**
- `mod.rs`: Re-exports
- `graphics.rs`: Graphics struct - All Vulkan resources (~50 lines)
- `simulation.rs`: WorldSim struct + save methods (~120 lines)
- `ui_state.rs`: UiState struct - All UI state (~70 lines)
- `input_state.rs`: InputState struct + Deref/DerefMut (~30 lines)
- `palette.rs`: PaletteItem + PaletteTab types (~20 lines)
- `profiling.rs`: AutoProfileFeature enum (~40 lines)

**`src/world_init/` - World Generation (150 lines total)**
- `mod.rs`: Re-exports
- `spawn.rs`: find_ground_level() function (~20 lines)
- `generation.rs`: create_initial_world_with_seed(), create_game_world_full() (~120 lines)

**`src/main.rs` - Entry Point (92 lines, 96% reduction from 2131)**
- macOS cursor helper functions
- Module declarations
- main() function
- Day/night cycle constants moved to `constants.rs`

### Key Improvements:
- **Massive reduction**: main.rs from 2131 → 92 lines (96% reduction)
- **Modular organization**: Clear separation of concerns across 3 module families
- **Maintainability**: Smaller, focused files (20-565 lines each)
- **Existing code preserved**: Moved app_* modules into src/app/ directory
- **Clean architecture**: App logic, state management, and initialization clearly separated
- **Type safety**: All imports and dependencies properly managed

---

## 5. Main Application (Original Notes)

`src/main.rs` is over 2000 lines and contains Vulkan setup, window event handling, and game loop logic.

### Proposed Structure:
- `src/main.rs`: Minimal entry point and event loop.
- `src/app/mod.rs`: `VoxelApp` struct managing game state.
- `src/app/events.rs`: Keyboard/Mouse input handling.
- `src/render/mod.rs`: Vulkan device/swapchain management.
- `src/render/pipelines.rs`: Pipeline initialization.
- `src/render/frame.rs`: Frame-by-frame command buffer building.

---

## 7. Verification Process

After each phase:
1. Run `cargo check` / `make lint`.
2. Run `make test`.
3. Verify in-game functionality.
4. Commit before starting next phase.

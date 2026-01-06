# Refactoring Plan: Sub-Voxel System

The goal of this refactor is to decompose the large `src/sub_voxel.rs` (2166 lines) and `src/sub_voxel_builtins.rs` (1800 lines) files into a modular `sub_voxel` directory. This will improve code navigability, maintainability, and compilation parallelism.

## 1. Create Directory Structure

We will transition from:
```
src/sub_voxel.rs
src/sub_voxel_builtins.rs
```

To:
```
src/sub_voxel/
├── mod.rs              # Re-exports for backward compatibility
├── types.rs            # LightMode, Color, LightBlocking, ModelResolution
├── model.rs            # SubVoxelModel struct and impl
├── registry.rs         # ModelRegistry struct and impl
├── builtins/           # Derived from sub_voxel_builtins.rs
│   ├── mod.rs          # Registration logic
│   ├── basic.rs        # Empty, basic helpers
│   ├── lighting.rs     # Torch, crystal
│   ├── fences.rs       # Fences, gates
│   ├── stairs.rs       # Stairs, ladders
│   ├── doors.rs        # Doors, trapdoors, windows
│   └── vegetation.rs   # Plants, mushrooms
└── texture_atlas.rs    # (Optional) If texture logic is separable
```

## 2. Phase 1: Core Types and Model Definition

1.  **Create `src/sub_voxel/types.rs`**:
    *   Move `ModelResolution` enum and constants.
    *   Move `LightMode` enum.
    *   Move `Color` struct.
    *   Move `LightBlocking` enum.

2.  **Create `src/sub_voxel/model.rs`**:
    *   Move `SubVoxelModel` struct.
    *   Move methods related to model manipulation (setting voxels, collision masks).

3.  **Create `src/sub_voxel/registry.rs`**:
    *   Move `ModelRegistry` struct.
    *   Move serialization/deserialization logic.

4.  **Create `src/sub_voxel/mod.rs`**:
    *   Re-export all types to match the original `crate::sub_voxel::*` API surface to minimize breakage in other files.

## 3. Phase 2: Built-in Models

1.  **Create `src/sub_voxel/builtins/` directory**.

2.  **Split `src/sub_voxel_builtins.rs`**:
    *   **`basic.rs`**: `create_empty`, scaling helpers (`set_scaled`, `fill_scaled`), `inverted_copy`.
    *   **`lighting.rs`**: `create_torch`, `create_crystal`.
    *   **`fences.rs`**: `create_fence`, `create_gate_closed`, `create_gate_open`.
    *   **`stairs.rs`**: `create_stairs_*`, `create_ladder`.
    *   **`doors.rs`**: `create_door_*`, `create_trapdoor_*`, `create_window_*`.
    *   **`vegetation.rs`**: `create_tall_grass`, `create_flower_*`, `create_mushroom_*`.

3.  **Create `src/sub_voxel/builtins/mod.rs`**:
    *   Implement `register_builtins(registry: &mut ModelRegistry)`.
    *   Call functions from the sub-modules.

## 4. Updates

*   Update `src/lib.rs` (if it exists) or `src/main.rs` to use `mod sub_voxel;` pointing to the directory instead of the file.
*   Update imports in `src/sub_voxel_builtins.rs` (which will move) to point to the new locations of `SubVoxelModel` etc.
*   Ensure `src/sub_voxel_builtins.rs` is deleted and replaced by `src/sub_voxel/builtins/mod.rs`.

## 5. Verification

*   Run `make checkall` to ensure no breaking changes.
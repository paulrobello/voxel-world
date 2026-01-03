# Refactor & Logic Review (2026-01-03)

## Workflow (must follow each batch)
- Run `make checkall`.
- Fix any issues it reports.
- Have the user verify nothing is broken.
- Update this checklist with outcomes/notes.
- Commit all work before moving to the next item.

# Phase 3: World Persistence (Completed)
*See git history for implementation details. Region-based saving/loading is active.*

# Phase 5: In-Game Model Editor & Shared Library

## Objectives
- **In-Game Creation**: Enable players to design 8x8x8 sub-voxel models without external tools.
- **Portability**: Save/Load models as individual files (`.vxm`) to share between worlds.
- **Usability**: Intuitive UI (using `egui`) for palette selection, voxel placement, and library management.

## 1. File Format Specification (`.vxm`)

### 1.1 Purpose
A portable binary format for a single sub-voxel model, allowing users to build a library of assets ("chairs", "tables", "fences") that can be imported into any world.

### 1.2 Format Structure
**Path**: `user_models/{category}/{model_name}.vxm` (Global Library)

```rust
struct VxmFile {
    magic: [u8; 4],         // "VXM1"
    version: u16,           // 1
    name: String,           // UTF-8 Display Name
    author: String,         // UTF-8 Author Name
    creation_date: u64,     // Unix timestamp
    palette: [u32; 16],     // RGBA8888 color palette
    voxels: [u8; 512],      // 8x8x8 grid (indices into palette, 0=air)
    properties: ModelProps, // Collision flags, light emission, etc.
}

struct ModelProps {
    collision_mask: u64,    // 64-bit mask (1 bit per 2x2x2 block? or just solid/pass)
    is_transparent: bool,
    light_level: u8,
}
```

## 2. Systems Architecture

### 2.1 Model Registry & World Storage
The world needs its own mapping of `ModelId` to `ModelData` to ensure consistency.

- **`ModelRegistry`**:
    - Runtime: `HashMap<u16, SubVoxelModel>`.
    - Persistence: `worlds/{name}/models.dat` (Mapping `u16` -> `ModelData`).
    - **Limit**: Expand `BlockType` model ID support from `u8` to `u16` if not already done.

- **`LibraryManager`**:
    - Scans `user_models/` directory on startup.
    - Provides a list of available "Blueprints".
    - `import_model(name)`: Loads `.vxm`, registers it in the current world's `ModelRegistry`, returns new `ModelId`.

### 2.2 Editor System (`src/editor/`)
A new state/system that overrides standard player input.

- **`EditorState`**:
    - `active: bool`
    - `target_model_id: Option<u16>` (Editing existing or new?)
    - `scratch_pad: SubVoxelModel` (The working copy)
    - `camera_orbit: (f32, f32, f32)` (Orbiting the model being edited)

- **`EditorMode`**:
    - When active:
        - Disable Player Physics/Movement.
        - Switch Camera to "Orbit Mode" around the editing target.
        - Render the `scratch_pad` model at a fixed position or overlay.
        - Enable Mouse Cursor (unlock).

### 2.3 UI Layer (`egui`)
Utilize `egui_winit_vulkano` for tool windows.

- **Panels**:
    - **Tools**: Pencil, Eraser, Fill, Eyedropper.
    - **Palette**: Grid of 16 colors. Click to select, Right-click to edit RGBA.
    - **Library**: List of `.vxm` files. "Import", "Export", "Save".
    - **Preview**: 2D render of the model (optional, maybe later).

## 3. Detailed Implementation Plan

### Step 1: Data Structures & IO (`src/storage/model_format.rs`) ✅ COMPLETE
- [x] Define `VxmFile` struct and serialization (using `bincode`).
- [x] Create `user_models/` directory creation logic.
- [x] Implement `save_vxm(path, model)` and `load_vxm(path)`.
- [x] **Test**: Round-trip serialization test.

### Step 2: Model Registry Upgrade ✅ COMPLETE
- [x] Ensure `Chunk` and `World` use `u16` for Model IDs (currently `u8` in `BlockType`?).
    - *Check `chunk.rs`: Model IDs are u8 with MAX_MODELS=256. Storage format uses u16 packing with room for expansion. Keeping u8 for now - 256 models is sufficient for editor use case.*
- [x] Implement `models.dat` saving/loading for the world (persisting the registry).
    - *Added `WorldModelStore` struct for serialization*
    - *Added `ModelRegistry::load_from_store()` and `save_to_store()` methods*
- [x] Add `LibraryManager` to list external `.vxm` files.
    - *Already implemented in Step 1 with `list_models()`, `save_model()`, `load_model()`*

### Step 3: Editor Logic & Camera (`src/editor/mod.rs`) ✅ COMPLETE
- [x] Create `EditorSystem` resource.
    - *Added `EditorState` struct with scratch_pad, palette, tools, orbit camera*
- [x] Implement `OrbitCamera` for inspecting the 8x8x8 grid.
    - *`camera_position()`, `camera_target()`, `update_orbit()` methods*
- [x] Implement `Raycast` against the 8x8x8 grid (AABB check -> Voxel check).
    - *`raycast_voxel()` method with DDA algorithm*
    - *Returns voxel position and face normal for placement*

### Step 4: UI Implementation ✅ COMPLETE
- [x] **Main Menu / HUD**: Add keybind ('N') to toggle editor.
    - *'N' key toggles editor on/off, Escape also closes*
    - *EditorState added to UiState in main.rs*
- [x] **Egui Integration**:
    - Palette Window: 4x4 color grid with color picker.
    - File Window: List box for `user_models/` with Load buttons.
    - Tools Window: Pencil, Eraser, Eyedropper tools.
    - Preview Window: 2D isometric projection of model.
- [x] **Interaction** (planned for mouse integration):
    - Left Click: Place Voxel (Current Color).
    - Right Click: Remove Voxel.
    - Middle Click: Pick Color.
- *Note: Full mouse interaction with 3D viewport pending - requires render pipeline changes.*

### Step 5: World Integration
- [ ] "Place in World": When saving/exiting, update the `ModelRegistry` and place the block in the world at the player's previous target.

## 4. Expansion Hooks
- **Online Gallery**: `LibraryManager` could eventually fetch `.vxm` from a web API.
- **Copy/Paste**: Clipboard support for voxel data.
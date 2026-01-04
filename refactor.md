# Refactor & Logic Review (2026-01-03)

## Workflow (must follow each batch)
- Run `make checkall`.
- Fix any issues it reports.
- Have the user verify nothing is broken.
- Update this checklist with outcomes/notes.
- Commit all work before moving to the next item.

# Phase 3: World Persistence (Completed)
*See git history for implementation details. Region-based saving/loading is active.*

# Phase 5: In-Game Model Editor & Shared Library (Completed)
*See git history for implementation details.*

**Features implemented:**
- `.vxm` file format for portable model storage
- `LibraryManager` for saving/loading models from `user_models/` directory
- `EditorState` with scratch pad, palette, orbit camera
- Isometric 3D viewport with software rasterizer and z-buffer
- Tools: Pencil, Eraser, Eyedropper, Fill, Rotate (90° Y-axis)
- 16-color palette with RGBA color picker
- Library browser with Load functionality
- Save to Library with overwrite confirmation
- Runtime sprite generation for HUD icons
- GPU buffer refresh when models are edited
- Custom models appear in E-key palette
- Auto-rotation to face player when placing custom models
- Right-click to rotate placed custom models

## Expansion Hooks
- **Online Gallery**: `LibraryManager` could eventually fetch `.vxm` from a web API.
- **Copy/Paste**: Clipboard support for voxel data.
- **Undo/Redo**: History stack for editor operations.

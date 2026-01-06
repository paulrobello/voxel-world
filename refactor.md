# Refactor & Logic Review

## Workflow (must follow each batch)
- Run `make checkall`.
- Fix any issues it reports.
- Have the user verify nothing is broken.
- Update this checklist with outcomes/notes.
- Commit all work before moving to the next item.

---

## Sub-Voxel Resolution Increase: 8³ → 16³

### Goal
Increase sub-voxel model resolution from 8×8×8 to 16×16×16 for higher detail models (especially crystals). Make resolution changes easier in the future by eliminating hardcoded values.

### Design Principles
1. **Single source of truth**: All resolution-dependent values derive from `SUB_VOXEL_SIZE`
2. **Computed constants**: Use `SUB_VOXEL_SIZE / 2` for center, `SUB_VOXEL_SIZE - 1` for max index, etc.
3. **Scale-aware builtins**: Model creation functions should use relative coordinates or scale factors

### Files to Modify

#### Phase 1: Core Constants (Foundation)
- [ ] `src/sub_voxel.rs`
  - Change `SUB_VOXEL_SIZE: usize = 8` → `16`
  - `SUB_VOXEL_VOLUME` auto-updates (derived)
  - Add helper constants:
    ```rust
    pub const SUB_VOXEL_CENTER: usize = SUB_VOXEL_SIZE / 2;
    pub const SUB_VOXEL_MAX: usize = SUB_VOXEL_SIZE - 1;
    pub const SUB_VOXEL_CENTER_F32: f32 = SUB_VOXEL_SIZE as f32 / 2.0;
    ```

- [ ] `shaders/common.glsl`
  - Change `const uint SUB_VOXEL_SIZE = 8;` → `16`
  - (Derived values like `SUB_VOXEL_SCALE` auto-update)

#### Phase 2: GPU Resources & Atlas
- [ ] `src/gpu_resources.rs`
  - `MODEL_ATLAS_WIDTH`, `MODEL_ATLAS_DEPTH`, `MODEL_ATLAS_HEIGHT` are already derived from `SUB_VOXEL_SIZE` ✓
  - Verify texture creation uses these constants
  - Update comments: "128×8×128" → use constant expressions

- [ ] `shaders/models.glsl`
  - Already uses `SUB_VOXEL_SIZE` constant ✓
  - Update comments: "8×8×8" references

#### Phase 3: Editor (Hardcoded Values)
- [ ] `src/editor/mod.rs`
  - Line 315: `let center = Vector3::new(4.0, 4.0, 4.0)` → use `SUB_VOXEL_CENTER_F32`
  - Line 388: `Vector3::new(8.001, 8.001, 8.001)` → use `SUB_VOXEL_SIZE as f32 + 0.001`
  - Line 24: `VoxelSnapshot` type uses `SUB_VOXEL_SIZE` ✓
  - Line 354, 362: Update comments "8x8x8" references
  - Line 511: Update comment about mirroring

- [ ] `src/editor/rasterizer.rs`
  - Line 315: `let mirror_center = 4.0` → use `SUB_VOXEL_CENTER_F32`
  - Uses `SUB_VOXEL_SIZE` constant elsewhere ✓

#### Phase 4: Builtin Models (Scale-Up)
- [ ] `src/sub_voxel_builtins.rs`
  - **Option A**: Create a scaling layer that multiplies coordinates by 2
  - **Option B**: Manually double all coordinate values (tedious, error-prone)
  - **Option C**: Define models using normalized 0.0-1.0 coordinates, then scale

  **Recommended: Option A** - Add helper function:
  ```rust
  fn scale_model(model: &mut SubVoxelModel, factor: usize) {
      // Scale all voxel positions by factor
      // For 8→16 transition, doubles all coordinates
  }
  ```

  Models to update:
  - `create_torch()` - torch shape
  - `create_slab_bottom/top()` - half-blocks
  - `create_fence()` - fence posts (16 variants)
  - `create_gate_*()` - gate variants (8 variants)
  - `create_stairs_*()` - all stair variants (12+ variants)
  - `create_ladder()` - ladder rungs
  - `create_trapdoor_*()` - trapdoor variants (4 variants)
  - `create_window()` - window frames (16 variants)
  - `create_door_*()` - all door variants (40+ variants)

#### Phase 5: Storage Format
- [ ] `src/storage/model_format.rs`
  - Uses `SUB_VOXEL_VOLUME` constant ✓
  - May need format version bump if saved models are resolution-dependent
  - Add migration logic for loading old 8³ models into 16³ format (scale up)

#### Phase 6: Sprite Generator
- [ ] `src/sprite_gen.rs`
  - Uses `ModelRegistry` which handles resolution internally
  - Verify sprite rendering works at new resolution
  - May need to adjust sprite size or detail level

#### Phase 7: Property Calculations
- [ ] `src/sub_voxel.rs` - `pack_model_properties()`
  - Line 1149: `let y = ((idx / 8) % 8) as u8;` - uses hardcoded 8
  - Change to use `SUB_VOXEL_SIZE`

### Testing Checklist
- [ ] `make checkall` passes
- [ ] Existing models render correctly (scaled up)
- [ ] Editor works: place/erase/mirror/rotate voxels
- [ ] Sprites generate correctly
- [ ] Model atlas uploads to GPU correctly
- [ ] Sub-voxel raycast/collision works
- [ ] Saved models load correctly (test both old and new format)
- [ ] Performance acceptable (16³ = 4096 voxels vs 8³ = 512)

### Future-Proofing Additions
After refactor, add to `src/sub_voxel.rs`:
```rust
// Resolution configuration - change SUB_VOXEL_SIZE to adjust model detail
pub const SUB_VOXEL_SIZE: usize = 16;  // Options: 8, 16, 32
pub const SUB_VOXEL_VOLUME: usize = SUB_VOXEL_SIZE * SUB_VOXEL_SIZE * SUB_VOXEL_SIZE;
pub const SUB_VOXEL_CENTER: usize = SUB_VOXEL_SIZE / 2;
pub const SUB_VOXEL_MAX: usize = SUB_VOXEL_SIZE - 1;
pub const SUB_VOXEL_CENTER_F32: f32 = SUB_VOXEL_SIZE as f32 / 2.0;
pub const SUB_VOXEL_BOUNDS_F32: f32 = SUB_VOXEL_SIZE as f32 + 0.001;
```

### Notes
- Memory impact: Model atlas grows from 131KB to ~1MB (16x larger)
- GPU upload time may increase slightly
- Consider LOD system if performance is affected at distance

---

## Completed Tasks
(Move items here when done)

---

## Next Tasks
1. Phase 1: Update core constants
2. Phase 2: Verify GPU resources
3. Phase 3: Fix editor hardcoded values
4. Phase 4: Scale up builtin models
5. Phase 5: Update storage format
6. Phase 6: Verify sprite generator
7. Phase 7: Fix property calculations
8. Add crystal sub-voxel model

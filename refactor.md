# Refactor & Logic Review (2026-01-02)

## Workflow (must follow each batch)
- Run `make checkall`.
- Fix any issues it reports.
- Have the user verify nothing is broken.
- Update this checklist with outcomes/notes.
- Commit all work before moving to the next item.

## Open Findings

### Voxel Engine Traversal & Intersection (2026-01-03)

#### Critical Correctness Issues
- **CPU vs GPU Intersection Mismatch**: The CPU `raycast` function (`src/raycast.rs`) treats `BLOCK_MODEL` types as solid 1x1x1 cubes, causing "ghost" interactions.
    - **Recommendation**: Update `src/raycast.rs` to perform local ray-model intersection tests using the `SubVoxelModel` data.

#### Optimization Opportunities
- **Missing Coarse Mask Acceleration**: `shaders/traverse.comp` does not use the 4x4x4 `collision_mask` for sub-voxels.
    - **Recommendation**: Call `modelMaskBlocksRay` before `marchSubVoxelModel` to skip empty sub-voxel regions.
- **Shadow Ray Optimization**: `marchSubVoxelShadow` in `shaders/models.glsl` should also use the coarse mask for early-out.

#### Code Quality & Maintenance
- **Dead Code**: Remove unused `findSubVoxelHit` in `shaders/traverse.comp`.
- **Rotation Verification**: `rotateModelPos` and inverse functions in `shaders/models.glsl` are verified correct.

#### Action Plan

1. [x] **Refactor CPU Raycast**: Implement sub-voxel intersection in Rust.

2. [ ] **Optimize Shaders**: Inject coarse mask checks in GLSL.

3. [ ] **Cleanup**: Remove unused shader functions.



## Completed today

- [x] **Refactor CPU Raycast**: Accurate sub-voxel intersection on the CPU to match GPU rendering.

# Refactor & Logic Review (2026-01-02)

## Workflow (must follow each batch)
- Run `make checkall`.
- Fix any issues it reports.
- Have the user verify nothing is broken.
- Update this checklist with outcomes/notes.
- Commit all work before moving to the next item.

## Open Findings
- **Shader bottlenecks (still open)**
  - Water shading stacks many procedural noise calls per pixel: UV flow (6× noise), caustics (3×), wave normals (9× + gradients). On water-heavy scenes this dominates ALU. Consider baking a small tiled normal/flowmap in the atlas or sharing a reusable 2D FBM helper with memoized samples per hit.
  - Shadow rays still march a fixed 128 steps with a 256‑block distance cap. Consider reusing chunk/brick skipping and deriving an adaptive iteration cap from sun elevation/world extent.
- **Future DRY**
  - Shadow + sky DDA loops are very similar; a shared “march until predicate” helper would reduce duplicate fixes.

## Completed today
- Fixed sky-exposure bug when `texture_origin` slides (stay in texture space).
- Corrected particle/falling-block depth comparisons to use world distance (`length(dir)` scale).
- Added shared `makeSafeDir`/`rayBoxHit` helper; removed unused brick/chunk distance helpers.
- Split shader into includes: `shaders/common.glsl`, `accel.glsl`, `util.glsl`, `lighting.glsl`, `materials.glsl`, `overlays.glsl`; `traverse.comp` now holds traversal/sub-voxel + main.
- `cargo build --release` succeeds after the split (no runtime fog).
- Shadow rays now reuse chunk/brick skipping with adaptive step/distance caps to cut wasted iterations.
- Water shading now uses a compact 3-octave FBM for flow/caustics/waves, reducing per-pixel noise calls.

# Refactor & Logic Review (2026-01-02)

## Workflow (must follow each batch)
- Run `make checkall`.
- Fix any issues it reports.
- Have the user verify nothing is broken.
- Update this checklist with outcomes/notes.
- Commit all work before moving to the next item.

## Open Findings
- **Shader bottlenecks (still open)**
  - Shadow rays: now chunk/brick-skipping by default. Need perf/visual check across sun angles and dense foliage; consider adaptive step cap if artifacts return.
- **Future DRY**
  - Shadow + sky DDA loops are very similar; a shared “march until predicate” helper would reduce duplicate fixes.

## Completed today
- Fixed sky-exposure bug when `texture_origin` slides (stay in texture space).
- Corrected particle/falling-block depth comparisons to use world distance (`length(dir)` scale).
- Added shared `makeSafeDir`/`rayBoxHit` helper; removed unused brick/chunk distance helpers.
- Split shader into includes: `shaders/common.glsl`, `accel.glsl`, `util.glsl`, `lighting.glsl`, `materials.glsl`, `overlays.glsl`; `traverse.comp` now holds traversal/sub-voxel + main.
- `cargo build --release` succeeds after the split (no runtime fog).
- Shadow rays chunk/brick-skip by default (fixed 128 steps / 256 dist, with skips).
- Water shading now uses a compact 3-octave FBM for flow/caustics/waves (5 samples per hit instead of 6+).

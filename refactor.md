# Refactor & Logic Review (2026-01-02)

## Workflow (must follow each batch)
- Run `make checkall`.
- Fix any issues it reports.
- Have the user verify nothing is broken.
- Update this checklist with outcomes/notes.
- Commit all work before moving to the next item.

## Open Findings
- **Shader bottlenecks (still open)**
  - Shadow rays: chunk/brick-skipping + angle-aware step budget (96–256). Need perf/visual check across sun angles and dense foliage with new cap; tune if artifacts remain.
- **Future DRY**
  - Shadow + sky DDA loops are very similar; a shared “march until predicate” helper would reduce duplicate fixes.

## Completed today
- Fixed sky-exposure bug when `texture_origin` slides (stay in texture space).
- Corrected particle/falling-block depth comparisons to use world distance (`length(dir)` scale).
- Added shared `makeSafeDir`/`rayBoxHit` helper; removed unused brick/chunk distance helpers.
- Split shader into includes: `shaders/common.glsl`, `accel.glsl`, `util.glsl`, `lighting.glsl`, `materials.glsl`, `overlays.glsl`; `traverse.comp` now holds traversal/sub-voxel + main.
- `cargo build --release` succeeds after the split (no runtime fog).
- Shadow rays chunk/brick-skip by default (adaptive 96–256 step budget, 256 dist cap).
- Water shading now uses a compact 3-octave FBM for flow/caustics/waves (5 samples per hit instead of 6+).
- Profiled at higher quality (render 900x810 in a 1200x1080 window; baseline in `profile.csv`):
  - Run 3 (post shadow fixes, 777 samples): avg 82 fps (13.51 ms), 1% low 48 fps, min 46 fps; max 332 fps.
    - Render avg 12.62 ms, p90 16.30 ms; chunkload avg 0.87 ms; ~1,898 chunks resident on average.
  - Run 2 (post shadow fixes, 728 samples): avg 82 fps (13.52 ms), 1% low 48 fps, min 46 fps; max 332 fps.
    - Render avg 12.61 ms, p90 16.32 ms; chunkload avg 0.89 ms; ~1,891 chunks resident.
  - Run 1 (prior baseline, 306 samples): avg 102 fps (10.46 ms), 1% low 64 fps, min 58 fps; max 332 fps.
    - Render avg 9.12 ms, p90 13.5 ms; chunkload avg 1.29 ms; ~1,764 chunks resident.
  - Time-of-day sweeps:
    - Run 4 (ladder sun shadows ON, 37 samples): avg 76.7 fps (14.03 ms), 1% low 53.8 fps, min 55 fps; max 279 fps. Render avg 14.03 ms, p90 16.47 ms; chunkload avg 0.56 ms; ~1,772 chunks resident.
    - Run 5 (ladder sun shadows OFF, 62 samples): avg 83.6 fps (12.67 ms), 1% low 62 fps, min 62 fps; max 333 fps. Render avg 12.67 ms, p90 15.00 ms; chunkload avg 0.54 ms; ~1,823 chunks resident.
    - Run 6 (ladder stress mix, sun shadows ON, more models placed, 92 samples): avg 77.0 fps (12.87 ms), 1% low 63.9 fps, min 64 fps; max 271 fps. Render avg 12.87 ms, p90 14.00 ms; chunkload avg 0.56 ms; ~1,780 chunks resident.
- Shadow rays now use an angle-aware adaptive step cap (96–256 steps based on sun direction) to avoid horizon artifacts while keeping the distance cap at 256.
- Sub-voxel (model) shadows now use a capped shadow-only marcher (16 steps) for occlusion, keeping geometry/rotation correct while bounding cost.

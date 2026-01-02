# Refactor & Logic Review (2026-01-02)

## Workflow (must follow each batch)
- Run `make checkall`.
- Fix any issues it reports.
- Have the user verify nothing is broken.
- Update this checklist with outcomes/notes.
- Commit all work before moving to the next item.

## Open Findings
- _(none; monitor shadows during next playtest for horizon/foliage artifacts with new step cap)_

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
- Model shadow occlusion now uses camera-distance LOD cutoff (32 m) in world coords (texture origin aware) to match model render culling; shadows disappear when models do.
- Fixed push-constant padding for the added `camera_pos` field to satisfy the pipeline’s push-constant range and prevent runtime panics.
- Shadow + sky DDA stepping now share a `ddaAdvance` helper to keep traversal logic in sync and reduce duplicated fixes.
- Profiling (render 900x810 in 1200x1080, latest time sweep with added models):
  - Run 7 (61 samples): avg 123.5 fps (7.49 ms), 1% low 91.0 fps, min 99 fps; max 131 fps. Render avg 7.49 ms, p90 7.81 ms; chunkload avg 0.48 ms; ~2,019 chunks resident.
  - Run 8 (42 samples, models + time sweep after shadow step-cap bounds): avg 120.1 fps (8.33 ms), 1% low 107.0 fps, min 98 fps; max 122 fps. Render avg 7.71 ms, p90 7.82 ms; chunkload avg 0.47 ms; ~2,053 chunks resident.
  - Δ vs Run 7: avg fps -3.4 (-2.7%), 1% low -4.6 fps; render p90 +0.03 ms; chunkload -0.01 ms; resident chunks +34 (scene a bit heavier).
- Shadow rays: step budget now bounds to the world exit distance (still capped at 256) so the angle-aware range actually hits 96–256 steps; keeps chunk/brick skipping and adds world-bounds awareness to avoid under/over-stepping at steep sun angles. `make checkall` passes.

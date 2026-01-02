# Refactor & Logic Review (2026-01-02)

## Workflow (must follow each batch)
- Run `make checkall`.
- Fix any issues it reports.
- Have the user verify nothing is broken.
- Update this checklist with outcomes/notes.
- Commit all work before moving to the next item.

## Open Findings
- **Shader correctness**
  - `getSkyExposure` is fed texture-space coords but calls `worldToTexture` before bounds/block reads (lines 653-674 in `shaders/traverse.comp`), so once `texture_origin` slides away from (0,0,0) the function subtracts the origin twice and treats interior voxels as out-of-bounds, forcing sky exposure to 1.0. Needs to operate entirely in texture space (or pass true world coords explicitly) to restore underground ambient attenuation.
  - Particle and falling-block overlays compare raw DDA `t` (derived from unnormalized `dir`) against world-distance `hitDistance` and also write it back directly (lines 1170-1209, 1216-1280). This under-reports their depth and can render fog/order incorrectly when `|dir| != 1`. Scale `t` by `length(dir)` for comparisons and outputs (matches how `traversal` sets `hitDistance`).
- **Shader bottlenecks**
  - Water shading stacks many procedural noise calls per pixel: UV flow (6× noise, lines 903-929), caustics (3× noise, lines 931-949), wave normals (9× noise plus gradients, lines 1029-1070). On water-heavy scenes this dominates ALU. Consider baking a small tiled normal/flowmap in the atlas or sharing a reusable 2D FBM helper with memoized samples per hit.
  - Shadow rays always run a 128-iteration voxel DDA with per-step model checks and a hard 256-unit cap (lines ~449-640). Near-grazing sun directions can still exhaust iterations while inside the loaded window, wasting time and sometimes ending lit prematurely. Reuse the chunk/brick skip acceleration from `traversal`, and derive the iteration cap from world extents and sun elevation instead of a flat 128/256.
- **DRY / structure**
  - The “safe direction + ray-box” pattern is duplicated across `rayBlockIntersect` (1314-1344), `findSubVoxelHit` (~1495-1556) and `raySubVoxelIntersect` (1563-1594). Factor a small inline helper for safe inverse directions and a generic `rayBoxHit` so particles/falling blocks/models all share one path.
  - Dead helpers: `getChunkExitT`, `getBrickExitT`, and `getBrickDistance` are defined but unused (around lines 209-310). Remove or move them into an acceleration include to trim compile time and reduce cognitive load.
  - Shadow casting and sky-exposure marching duplicate nearly the same DDA logic with slightly different exit conditions. Extract a shared “march until predicate” utility so fixes (bounds, step counts) land once.
  - File is 2.4k lines; propose splitting into includes that mirror responsibilities already emerging: `common.glsl` (constants/push constants), `accel.glsl` (coord transforms, chunk/brick masks), `lighting.glsl` (sun dir/daylight, shadow + sky exposure), `materials.glsl` (block sampling, AO, water/glass effects), `overlays.glsl` (particles/falling/preview/target), leaving `traverse.comp` to wire traversal + main. This keeps `#include` use consistent with existing `models.glsl`/`sky.glsl` and should make future GPU-side profiling/refactors manageable.

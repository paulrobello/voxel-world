# Sub-Voxel Rendering & Shadows Debug Log

## Current Status (2025-12-31)
- Single fence post shadows: **working** (thin footprint, no self-intersection).
- Connected fences: shadows present via collision-mask fallback, but model shading still has issues (missing top faces and unstable normals on rotated/stacked geometry).
- Top faces of vertical post models can disappear; normals on sub-voxel geometry still misaligned.

## What We Changed Recently
1. **Shadow rays**
   - March in texture space; skip start voxel to avoid self-shadow, but allow model shadows via sub-voxel march + collision-mask fallback.
   - Added coarse 4×4×4 collision mask DDA and AABB fallback for partial blockers to avoid full-block shadows.
2. **Model shadows**
   - Partial blockers: soft shadow (0.4) when mask/AABB hits; otherwise light passes.
3. **Sub-voxel normals**
   - Normal now determined per-step (axis of min tMax) and inverse-rotated to match model rotation.
   - Reverted step-axis tracking variants after artifacts; normals still not fully stable.
4. **Target outline**
   - Default `show_target_outline` set to **false**.

## Remaining Problems
- Missing top faces on tall/stacked sub-voxel models (post/column) in normal and shadow debug views.
- Normals still flip or appear incorrect on some faces; shading inconsistent.
- Connected fences: shadows work but need verification with correct geometry shading.

## Hypotheses
- marchSubVoxelModel normal selection may choose wrong axis when hitting top/bottom due to tMax tie/epsilon; may need tie-breaking using stepped_axis with a stable initialization based on entry face.
- Entry point/step epsilon may skip the very first layer (top face) when ray origin is slightly inside.
- Rotation handling: normal inverse rotation ok, but voxel stepping rotation mismatch could skip voxels on top.

## Next Investigation Ideas
- Tie-break normals: when tMax axes are within a small epsilon, prefer the axis actually stepped last (track stepped_axis again but only update when move happens, init from entry face).
- Explicit top/bottom hit detection: if hit voxel.y==7 and dir.y>0, force normal=+Y; if voxel.y==0 and dir.y<0, normal=-Y.
- Revisit entry nudge: reduce SUB_VOXEL_EPS for origin; ensure startPos clamp not clipping top voxel.
- Add debug output (colors per hitAxis) to NORMAL mode for models only to verify axis selection per face.
- Compare against earlier version (before normal fixes) where faces existed but shading flipped; diff marchSubVoxelModel between versions.

## Files Touched
- `shaders/traverse.comp`
  - marchSubVoxelModel: normal calc variants, collision-mask shadow fallback, inverse normal rotation.
  - Shadow ray: mask/AABB partial fallback, start-voxel skip logic.
- `src/main.rs`
  - Default `show_target_outline` set false.

## How to Reproduce
- Place single fence post and connected fence (2 posts, rails). Toggle shadow debug.
- Observe missing top faces on tall post (8-block stack) in NORMAL mode.

## Open Questions for Next Agent
- What epsilon / tie-break logic keeps top faces while avoiding flicker?
- Should normals derive from gradient of voxel occupancy instead of step axis?
- Is rotation of sampling vs. rotation of ray causing misses on top layer?

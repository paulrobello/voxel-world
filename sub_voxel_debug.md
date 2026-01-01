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

## Fix Applied (2025-12-31)
- **Shadow Trim Fix (Fine AABB)**: The shadow optimization fallback previously calculated the AABB from the coarse 4x4x4 collision mask, which inflated object bounds to 2-voxel increments (e.g., 2 voxels became 4). This caused shadows to be "twice as big" for thin objects. I updated `SubVoxelModel` to compute the **exact voxel AABB** (0..8 range) and store it in the `GpuModelProperties` padding (offset 8). The shader now uses this tight AABB for intersection checks.
- **Shadow Logic Refinement**: Updated `castShadowRayInternal` to require intersection with **BOTH** the fine AABB and the Collision Mask. This trims the shadow to the exact bounding box while preserving holes defined by the mask.
- **Alignment Fix**: Corrected `GpuModelProperties` struct alignment to match std430 (48 bytes), ensuring `flags` are read correctly.
- **Logic Fix**: Forced rotation 0 for fences to prevent misalignment.
- **Normal/Hit Fixes**: Implemented entry-based normal selection and entry-t return.

## Verification
- Single fence post should cast correct, tight shadow (matching geometry width).
- Connected fences should cast correct shadows.
- Visuals stable.

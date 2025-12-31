# Ladder Rendering Debug Notes

## Problem Summary

Ladders (and open gates) have angle-dependent rendering artifacts where parts of the model disappear when viewed from certain angles, especially when placed against a wall. Freestanding ladders in open areas render correctly.

## Key Observations

1. **Freestanding ladder works fine** - no rendering issues
2. **Ladder against wall has issues** - parts disappear at grazing angles
3. **Viewing angle matters** - ladder visible from straight-on, disappears at oblique angles
4. **Normals are correct** - verified in Normal debug mode
5. **Disabling AO and shadows doesn't fix it** - ruled out lighting as cause

## Debug Visualization Added

In Normal render mode (press N), added color coding:
- **Green tint**: Sub-voxel march hit successfully
- **Magenta**: Sub-voxel march MISSED but hit wall behind (THE BUG!)
- **Red**: Sub-voxel march missed completely
- **Blue**: March was skipped (LOD or no model_id)

**Result**: Magenta appears where ladder should be visible, confirming `marchSubVoxelModel()` returns false when it should return true.

## Root Cause

The `marchSubVoxelModel()` function in `shaders/traverse.comp` is incorrectly returning `false` (miss) for rays that should hit the ladder geometry. This happens specifically at grazing angles when the ladder is against a wall.

## What Was Tried

### 1. Entry Point Calculation Fixes
- Tried using `prevT` instead of computed formula - didn't help
- Tried clamping localEntry to `0.001-0.999` - didn't help
- Tried different entry point calculations for instant-hit vs DDA cases - didn't help

### 2. Restructuring Model Handling
- Originally: `instantHit = false` for BLOCK_MODEL (allow DDA to continue on miss)
- This caused the bug - marchSubVoxelModel was missing geometry at angles
- Reverted to: `instantHit = true` for BLOCK_MODEL
- Added pre-loop camera-inside handling
- Added post-loop sub-voxel march for normal hits

### 3. What Didn't Cause It
- Thin geometry (1 voxel thick) - user confirmed this wasn't the issue
- Normals - verified correct in debug mode
- AO/shadows - disabling didn't fix it
- Entry point precision - clamping didn't help

## Current Code Structure

### Pre-loop (lines ~1964-1996)
```glsl
// Special case: camera starts inside a model block
if (blockType == BLOCK_MODEL) {
    // Check if camera actually inside, if so march from camera position
    // If miss, set instantHit = false to continue DDA
}
```

### Main loop
- Models treated as instant hits (break on hit like solid blocks)
- No special model handling inside loop

### Post-loop (lines ~2157-2194)
```glsl
// Handle sub-voxel marching for model blocks hit from outside
if (blockType == BLOCK_MODEL && !subVoxelHit) {
    // Calculate entry point based on instantHit or DDA
    // Call marchSubVoxelModel()
}
```

## Key Files

- `shaders/traverse.comp` - Main ray traversal, sub-voxel marching
- `src/sub_voxel.rs` - Ladder model definition (voxels at Z=7)

## marchSubVoxelModel Function (lines 230-342)

This is where the bug likely is. The function:
1. Scales entry point to 0-8 sub-voxel space
2. Calculates box entry/exit t values
3. DDA marches through 8³ grid
4. At each step, rotates position and samples model
5. Returns true on hit, false if exits box

### Suspect Areas
1. **Entry calculation** (lines 243-260) - might be computing wrong start position
2. **Rotation handling** (line 290) - `rotateModelPos()` might interact badly with certain angles
3. **Bounds checking** (line 285) - might exit early incorrectly
4. **Step logic** (lines 318-338) - DDA stepping might miss voxels

## Ladder Model Definition

```rust
// In src/sub_voxel.rs - ladder()
// Vertical rails on sides (at Z=7, against wall)
for y in 0..8 {
    model.set_voxel(1, y, 7, 1);  // Left rail
    model.set_voxel(6, y, 7, 1);  // Right rail
}
// Horizontal rungs (at Z=7)
for y in [1, 3, 5, 7] {
    for x in 2..6 {
        model.set_voxel(x, y, 7, 1);
    }
}
```

Ladder geometry is at Z=7 (back of block). When rotated, this maps to different world-space positions.

## Rotation Functions

```glsl
// rotateModelPos - rotates sampling position
ivec3 rotateModelPos(ivec3 pos, uint rotation) {
    switch (rotation) {
        case 1u: return ivec3(7 - pos.z, pos.y, pos.x);
        case 2u: return ivec3(7 - pos.x, pos.y, 7 - pos.z);
        case 3u: return ivec3(pos.z, pos.y, 7 - pos.x);
        default: return pos;
    }
}
```

Note: Ray direction is NOT rotated - only the sampling position. This should work but might have edge cases.

## Suggested Debug Approach

1. **Add debug output for marchSubVoxelModel**:
   - Log entry position, direction, rotation
   - Log each step's voxel position and sample result
   - Identify exactly where/why it returns false

2. **Test with rotation=0**:
   - Temporarily disable rotation to isolate the issue
   - If it works without rotation, bug is in rotation handling

3. **Visualize entry point**:
   - Color-code based on localEntry values
   - Verify entry point is where expected

4. **Compare with working case**:
   - Freestanding ladder works - what's different about its rays?
   - Capture ray parameters for working vs broken case

## Commands

```bash
make build          # Build
make run            # Run
make checkall       # Lint, format, test
```

Press N in-game to toggle Normal render mode (with debug colors).

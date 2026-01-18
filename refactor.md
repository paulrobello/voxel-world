# Chunk Streaming Performance Refactor

## Problem Statement
Chunk generation and streaming cannot maintain 60 FPS during normal flight on M4 Max.
- Current: 27-40 FPS while flying, 60 FPS stationary
- Target: 50+ FPS sustained during flight

## Root Causes Identified

### 1. Unbounded GPU Upload in update_chunk_loading()
**File**: `src/world_streaming.rs`
- All completed chunks uploaded in single frame without budget
- Causes 1.7-2.0ms upload spikes during batch completions
- **Fix**: Budget to 64 chunks/frame, queue remainder
- **Status**: IMPLEMENTED

### 2. Blocking Texture Clear on Origin Shift
**File**: `src/world_streaming.rs`
- `.wait(None)` blocks CPU 5-10ms waiting for GPU
- Origin shifts occur every ~8 seconds during flight
- **Fix**: Async fence polling, delay re-upload until complete
- **Status**: IMPLEMENTED

### 3. Double SVT Computation
**File**: `src/world_streaming.rs`
- Workers compute SVT in chunk_loader.rs:235
- Main thread recomputes same SVT for metadata (~130K ops/chunk)
- **Issue**: Pre-computed SVT is invalid if overflow blocks modify chunk during insert
- **Status**: SKIPPED (complexity vs benefit - metadata_ms only 0.5-1ms normally)

### 4. Origin Shift Queue Drops
**Profile**: 462-1123 chunks dropped per shift
- In-flight chunks become stale on epoch bump
- Workers waste cycles on discarded work
- **Fix**: Predictive shifting based on player velocity
- **Status**: IMPLEMENTED

## Profile Summary (Before Optimization)

| Phase | Normal Flight | Origin Shift |
|-------|---------------|--------------|
| FPS | 35-40 | 25-29 |
| render_ms | 20-25ms | 28-35ms |
| upload_ms | 0.01ms | 1.7-2.0ms |
| metadata_ms | 0.5-1ms | 4-9ms |
| queue_drops | 0 | 462-1123 |

## Implementation Summary

### Completed Optimizations

#### 1. Budget Completed Chunk Uploads
**Files Modified**: `src/constants.rs`, `src/world_streaming.rs`, `src/app_state/simulation.rs`

- Added `MAX_COMPLETED_UPLOADS_PER_FRAME = 64` constant
- Added `deferred_uploads: VecDeque<ChunkResult>` to WorldSim
- Modified `update_chunk_loading()` to:
  - First drain deferred uploads from previous frames
  - Receive new completed chunks
  - Sort all by distance to player
  - Process up to 64, defer the rest
- Deferred queue is cleared during origin shifts

#### 2. Async Texture Clear
**Files Modified**: `src/world_streaming.rs`, `src/app_state/simulation.rs`

- Added `ClearFence` type alias
- Added `pending_clear_fence: Option<ClearFence>` to WorldSim
- New `clear_voxel_texture_async()` returns fence without waiting
- New `wait_for_pending_clear()` polls fence with 1ms timeout
- Fence is checked before any GPU upload operation

#### 3. Predictive Origin Shifting
**Files Modified**: `src/world_streaming.rs`

- Calculate player velocity direction
- Check if player is moving toward texture edge
- Use smaller threshold (1/6) when moving toward edge
- Use normal threshold (1/4) otherwise
- Reduces chunks dropped during shift by shifting earlier

#### 4. Inlined Brick Empty Check
**Files Modified**: `shaders/accel.glsl`, `shaders/traverse.comp`

- Added `isBrickEmptyFast()` that takes pre-computed chunkPos
- Avoids redundant chunkPos division (already computed for chunk-level check)
- `getBrickDistance()` helper exists but sphere-tracing is **not usable** (see below)

**Results**: +5.3% average FPS, +53% worst-case FPS improvement

#### Sphere-Tracing Investigation (Not Implemented)
**Status**: INVESTIGATED - NOT VIABLE WITH CURRENT ARCHITECTURE

Attempted to use `brick_distances` for sphere-tracing (skipping multiple bricks at once).
This caused severe visual artifacts (black grid patterns, missing geometry).

**Root Cause**: The `brick_distances` field is computed per-chunk only (`svt.rs:191`).
It calculates Manhattan distance to the nearest solid brick **within the same chunk**.
Empty chunks have all distances = 255, but neighboring chunks may have solid blocks
directly adjacent. Sphere-tracing based on these distances skips into solid geometry.

**Required for sphere-tracing to work**:
- Cross-chunk distance field propagation
- Or: Clamp distance based on proximity to chunk boundaries
- Both add significant complexity and CPU overhead

**Conclusion**: Keep the simple brick-skip (one brick at a time) which is already
optimized with `isBrickEmptyFast()`. The +5.3% FPS improvement from avoiding
redundant chunkPos division is retained.

## Success Metrics

- Sustained 50+ FPS at view_distance=8
- No FPS drops below 40 during origin shifts
- queue_drops < 100 per origin shift
- upload_ms spikes < 1.0ms

## Files Changed

1. `src/constants.rs` - Added MAX_COMPLETED_UPLOADS_PER_FRAME
2. `src/app_state/simulation.rs` - Added ClearFence, deferred_uploads, pending_clear_fence
3. `src/app_state/mod.rs` - Exported ClearFence
4. `src/world_streaming.rs` - All optimization implementations
5. `src/utils.rs` - Added deferred_uploads to ChunkStats
6. `src/app/init.rs` - Initialize new fields
7. `shaders/accel.glsl` - Added isBrickEmptyFast(), getBrickDistance() helpers
8. `shaders/traverse.comp` - Use inlined brick check with pre-computed chunkPos

## Verification

Run benchmarks to verify improvements:
```bash
make benchmark-normal  # Full benchmark with profiling
make run ARGS="--auto-fly --profile"  # Quick test with auto-fly
```

Compare profile CSVs for:
- Average FPS during flight
- FPS variance (stddev)
- queue_drops during origin shifts
- upload_ms spikes

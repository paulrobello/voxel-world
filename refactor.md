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

### 3. Duplicate Metadata Queue Calls
**File**: `src/world_streaming.rs`
- After immediate metadata update (SVT computation + GPU buffer write), chunks were
  being queued for background refresh via `queue_many()`
- This caused `update_metadata_buffers()` to recompute SVT redundantly
- **Fix**: Removed `queue_many()` calls after immediate updates since CPU and GPU
  buffers are already correctly updated
- **Status**: IMPLEMENTED

**Note on pre-computed SVT from workers**: Workers compute SVT in chunk_loader.rs:235,
but this cannot be reused on main thread because overflow blocks may modify the chunk
after worker computation. The main thread computes SVT after all overflow is applied.

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

#### 5. Shadow Ray Brick Skipping
**Files Modified**: `shaders/lighting.glsl`

- Enabled brick skipping for shadow rays even with model shadows enabled
- Models (BLOCK_MODEL=11) are non-zero, so they make bricks non-empty
- Empty bricks are safe to skip regardless of model shadow setting
- Used `isBrickEmptyFast()` with pre-computed chunkPos to avoid redundant division
- Previously brick skipping was incorrectly disabled when model shadows were on

**Impact**: Reduces shadow ray iterations in empty areas (sky, caves, etc.)

#### 6. Eliminate Duplicate Metadata Queue Calls
**Files Modified**: `src/world_streaming.rs`

- Removed redundant `queue_many()` calls after immediate metadata updates
- In `update_chunk_loading()`: After immediate SVT computation and GPU buffer update,
  chunks were being queued for background refresh which would recompute SVT again
- In `upload_world_to_gpu()`: Same issue for dirty chunk uploads
- The immediate update already writes to both CPU `metadata_state` and GPU buffers,
  so queueing for background refresh caused duplicate ~130K operations per chunk

**Impact**: Reduces metadata_ms overhead during chunk streaming, especially during flight

#### 7. Increase Transfer Ring Buffer Size
**Files Modified**: `src/gpu_resources.rs`

- Increased transfer ring buffer from 3 to 6 slots
- Each frame can have up to 3 upload calls (completed, unloaded, dirty chunks)
- With only 3 slots, if any previous transfer was in-flight, main thread would block
- 6 slots provides 2 frames of headroom before blocking occurs
- Also increased staging buffer pool size from 6 to 12

**Impact**: Reduces potential main thread blocking during chunk streaming

#### 8. Optimize Vegetation Generation (30x Speedup)
**Files Modified**: `src/world_gen/vegetation/mod.rs`, `src/terrain_gen.rs`

**Root Cause**: The vegetation generation functions were doing redundant expensive noise lookups:
- `generate_ground_cover()` called `terrain.get_height()` and `terrain.get_biome()` for each of 1,024 columns, despite this data already being cached in `ColumnDataCache`
- `generate_cave_decorations()` called `terrain.get_biome_3d()` for ALL 32,768 blocks in the chunk, even when no cave ceiling/floor was present

**Optimizations**:
1. Modified `generate_ground_cover()` to accept the pre-computed `ColumnDataCache` and use cached height/biome/hash values
2. Restructured `generate_cave_decorations()` to:
   - First check if block is Stone/Deepslate (early exit for most blocks)
   - Only call `get_biome_3d()` when an actual ceiling/floor is found (after confirming solid block + adjacent air)

**Results**:
| Phase | Before | After | Speedup |
|-------|--------|-------|---------|
| Vegetation | 155-171ms | 3.0-3.5ms | **55x** |
| Total chunk | 170-220ms | 7.2-7.5ms | **30x** |
| Max chunk | 1,400-1,600ms | 62-73ms | **22x** |

**Impact**: Chunk generation throughput increased from ~75 chunks/sec to ~2,100 chunks/sec (with 15 workers)

## Performance Analysis: Flight vs Stationary

**Key Finding**: The FPS difference between flight and stationary is primarily due to:

1. **Scene complexity dominates**: Normal terrain (caves, trees, lighting) takes 20-27ms to render vs 8ms for flat terrain
2. **Streaming overhead**: ~2-5ms additional render time during flight (visible in flat terrain: 8ms→13ms)
3. **GPU memory bandwidth**: Chunk uploads compete with shader texture reads on unified memory

| Scenario | Stationary FPS | Flying FPS | Render (stationary) | Render (flying) |
|----------|----------------|------------|---------------------|-----------------|
| Flat     | 120            | 72-89      | 8ms                 | 10-13ms         |
| Normal   | 40-50          | 35-50      | 18-24ms             | 17-27ms         |

**Conclusion**: The render_ms variance (17-27ms on normal terrain) is inherent to ray marching
complex geometry. Further optimization requires shader-level improvements or level-of-detail systems

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
5. `src/utils.rs` - Added deferred_uploads to ChunkStats, generation timing output
6. `src/app/init.rs` - Initialize new fields
7. `shaders/accel.glsl` - Added isBrickEmptyFast(), getBrickDistance() helpers
8. `shaders/traverse.comp` - Use inlined brick check with pre-computed chunkPos
9. `src/world_gen/vegetation/mod.rs` - Optimized vegetation generation with column cache
10. `src/terrain_gen.rs` - Pass column cache to vegetation functions, added phase timing
11. `src/chunk_loader.rs` - Added generation timing instrumentation

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

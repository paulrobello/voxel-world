# Picture Frame Texture Integration

## Mission Statement

Enable players to display custom artwork in their worlds by integrating the in-game texture editor with picture frames. Players can create pixel art in the texture editor, then place picture frames that render the custom artwork on the frame interior voxels.

---

## Overview

**Vision**: Picture frames are not just empty borders - they display custom pixel art created by the player. A single 1×1 frame shows a 128×128 picture. Frames automatically form clusters when placed adjacent to each other.

**Key Features**:
- Variable canvas sizes (1×1 to 128×128 pixels)
- Preset sizes: 32×32, 64×64, 128×128 (square); 32×64, 64×128 (tall); 64×32, 128×64 (wide); 16×128 (banner)
- Custom size support (any dimension 1-128)
- Pictures exported at actual size (no forced upscaling)
- Picture selection UI when placing frames
- Delete button for removing unwanted pictures
- Picture persistence across sessions
- Console commands for picture management
- Frames receive shadows but don't cast them

---

## Technical Approach

### Coordinate Systems

**Picture Space** (Texture Editor):
- Variable canvas sizes: 1×1 to 128×128 pixels
- Common presets: 32×32, 64×64, 128×128, 32×64, 64×128, 64×32, 128×64, 16×128
- Custom dimensions supported (1-128 range for width/height)
- Pixel coordinates: (0,0) to (width-1, height-1) in editor
- RGBA colors (256 levels per channel)

**Sub-Voxel Space** (Frame Model):
- 8×8 voxels on the front face (z=7)
- Each voxel represents 16×16 pixels of the picture (at 128×128 resolution)
- Voxel coordinates: (0,0) to (7,7)

**World Space** (Frame Cluster):
- 1×1 to 3×3 blocks
- Cluster dimensions: width × height blocks
- Each block displays a 128×128 tile of the full picture

### Data Flow

```
Texture Editor → Picture Library → Picture Atlas → Frame Metadata → Shader Render
 (variable)     (pictures.bin)      (GPU)         (picture_id)     (voxel colors)
       ↓
    (actual size, no upscaling)
```

1. **Creation**: Player draws pixel art in texture editor with custom dimensions (P key)
2. **Export**: Saved to picture library at actual canvas size
3. **Storage**: Picture saved to `~/.voxel_world/pictures.bin`
4. **Selection**: Player selects picture when placing frame
5. **Encoding**: `picture_id` stored in frame metadata
6. **Rendering**: Shader maps picture pixels to frame interior voxels

---

## Existing Infrastructure (✅ Complete)

### 1. Frame Models ✅
- **File**: `src/sub_voxel/builtins/frames.rs`
- 16 edge mask variants (model IDs 160-175)
- 8×8 sub-voxel resolution with picture area (color index 4)
- Wood border rendering (color indices 1-3)
- `MAX_FRAME_DIM = 3` (supports 3×3 clusters)
- `DESIGN_SIZE = 8` (8³ voxels per frame model)

### 2. Frame Metadata ✅
- **File**: `src/sub_voxel/builtins/frames/metadata.rs`
- `encode(picture_id, offset_x, offset_y, width, height, facing)`
- `decode_picture_id(data)` → 20-bit ID (~1 million pictures)
- `decode_offset_x/y` → 2-bit offsets (0-3, for tile position in cluster)
- `decode_width/height` → 2-bit dimensions (1-3 blocks)
- `decode_facing` → 2-bit rotation (0=North, 1=East, 2=South, 3=West)

### 3. Picture Library ✅
- **File**: `src/pictures/library.rs`
- `Picture` struct: id, name, width, height, pixels (RGBA)
- `MAX_PICTURE_SIZE = 128` (current implementation)
- Global storage: `~/.voxel_world/pictures.bin`
- zstd compression for efficient storage
- `PictureLibrary` manager for add/get/delete/list/remove

### 4. Picture Atlas ✅
- **File**: `src/pictures/atlas.rs`
- `MAX_GPU_PICTURES = 64` pictures on GPU at once
- GPU atlas: 8192×128 pixels (64 slots × 128 pixels wide)
- LRU eviction when full
- Slot-based GPU texture management
- Per-frame dirty tracking for efficient updates

### 5. Texture Editor ✅
- **File**: `src/ui/texture_generator.rs`
- P key to open editor
- Drawing tools: pencil, brush, eraser, fill, eyedropper, line, rectangle, circle
- **Variable canvas sizes**: 1×1 to 128×128 pixels
- Size presets: 32×32, 64×64, 128×128, 32×64, 64×128, 64×32, 128×64, 16×128, plus custom sizes
- Undo/redo support
- Color picker with 32-color palette
- Mirror mode (X/Y) for symmetric patterns
- **Export**: Pictures saved at actual canvas size (no forced upscaling)

### 6. Frame Auto-Sizing ✅
- **File**: `src/world/connections.rs`
- `update_frame_cluster()` function
- BFS detection of adjacent frames
- Automatic cluster size calculation
- Edge mask updates based on cluster position

### 7. GPU Rendering Pipeline ✅
- **File**: `shaders/models.glsl`
- Sub-voxel ray marching
- Per-voxel color lookup from picture atlas
- Rotation support via `transformFramePos()`
- UV coordinate calculation for picture sampling
- Fixed UV edge wrapping issue

### 8. Shadow Casting ✅
- **File**: `shaders/lighting.glsl`
- Picture frames **do not cast shadows** (skipped in shadow ray casting)
- Picture frames **still receive shadows** (normal lighting calculation)
- Identified by model ID range (160-175)

---

## Implementation Plan

### Phase 20.1: Core Picture-to-Voxel Mapping ✅

**Goal**: Map picture pixels to frame interior voxels.

**Status**: ✅ COMPLETE (via GPU shader rendering)

**Implementation Note**: Pictures are rendered directly from the GPU texture atlas in the fragment shader. This provides:
- Full 32-bit RGBA color per pixel
- Better performance (no CPU-side processing)
- Simpler code path

The shader handles:
- Pixel sampling from picture atlas
- UV coordinate calculation for correct orientation
- Rotation and offset handling
- Direct color output to fragment shader

---

### Phase 20.2: Frame Placement UI ✅

**Goal**: Enable picture selection when placing frames.

**Status**: ✅ COMPLETE

#### 20.2.1 Picture Browser Integration ✅
- [x] Picture browser with thumbnails
- [x] Search/filter by name
- [x] List view with picture dimensions
- [x] Set picture as active for placement
- [x] Clear selection (for empty frames)
- [x] **Delete button** to remove unwanted pictures

#### 20.2.2 Picture Selection State ✅
- [x] Store selected picture in `UIState` struct
- [x] Persist selection across session (user_prefs.json)
- [x] UI indicator in picture browser

#### 20.2.3 Frame Placement with Picture ✅
- [x] When placing frame, use selected picture_id
- [x] First frame in cluster sets the picture_id for all frames
- [x] Adjacent frames inherit picture_id from cluster

#### 20.2.4 Console Commands ✅
- [x] `/frame picture list` - List all pictures with cluster recommendations
- [x] `/frame picture set <id>` - Select picture for placement
- [x] `/frame picture clear` - Deselect (place empty frames)
- [x] `/frame picture debug` - Show cluster size guide

---

### Phase 20.3: Shader Integration ✅

**Goal**: Render picture colors on frame voxels in GPU shader.

**Status**: ✅ COMPLETE

#### 20.3.1 Picture Atlas Binding ✅
- [x] Add picture atlas descriptor set to shader
- [x] Pass picture atlas to shader during render
- [x] Update descriptor set layout in `traverse.comp`

#### 20.3.2 Picture Metadata in Push Constants ✅
- [x] Add picture_id lookup to push constants
- [x] Support per-model picture data (max 64 pictures)
- [x] custom_data encoding: picture_id (20 bits), offset_x/y (4 bits each), width/height (4 bits each), facing (2 bits)

#### 20.3.3 Pixel Sampling in Shader ✅
- [x] `samplePictureColor(picture_id, voxel_uv)` function
- [x] Apply picture color to interior voxels
- [x] Fall back to wood color for non-picture faces

#### 20.3.4 UV Calculation ✅
- [x] Calculate correct UV based on frame position
- [x] Account for 1-voxel borders (picture area is 6×6 for 8³ frames)
- [x] Fixed edge wrapping issue with corrected UV formula
- [x] All four rotations supported (North, East, South, West)

---

### Phase 20.4: Multi-Frame Cluster Support ✅

**Goal**: Display larger pictures across multiple frames.

**Status**: ✅ COMPLETE

**How it Works**: Pictures automatically scale to fit any cluster size:
- 1×1 cluster: 128×128 picture displayed on 1 frame (full resolution)
- 2×2 cluster: 128×128 picture divided across 4 frames (each shows 64×64 region)
- 3×3 cluster: 128×128 picture divided across 9 frames (each shows ~43×43 region)

The shader's UV calculation automatically divides the picture into a grid based on cluster dimensions. No need for larger pictures - the same 128×128 picture works for all cluster sizes!

#### 20.4.1 Cluster Picture Sizing ✅
- [x] Support for 1×1, 2×2, and 3×3 frame clusters
- [x] Automatic cluster detection via BFS
- [x] Picture scales across cluster dimensions automatically
- [x] All four rotations supported

#### 20.4.2 Picture Tiling Metadata ✅
- [x] Store cluster dimensions in frame metadata
- [x] Store offset position within cluster
- [x] All frames in cluster share same picture_id
- [x] `update_frame_cluster()` propagates picture info

#### 20.4.3 Cluster UV Mapping ✅
- [x] Calculate per-frame UV offsets for shader
- [x] Single frames use full picture
- [x] Multi-frame samples correct region based on offset
- [x] Proper row ordering (offset_y inverted for correct top-to-bottom)

---

### Phase 20.5: Performance & Optimization

**Goal**: Maintain 90+ FPS with picture frames.

**Status**: NOT STARTED

#### 20.5.1 Picture Atlas Caching
- [x] LRU eviction when atlas is full
- [ ] Pre-load pictures for visible chunks
- [ ] Async loading for large worlds

#### 20.5.2 Level of Detail (LOD)
- [ ] Downsample pictures for distant frames
- [ ] Update shader to use LOD mipmaps

#### 20.5.3 Batching Optimization
- [ ] Group frames by picture_id for efficient rendering

---

### Phase 20.6: Polish & UX ✅

**Goal**: Smooth user experience for picture frame workflow.

**Status**: ✅ COMPLETE

#### 20.6.1 Picture Editor Integration ✅
- [x] Export button in texture editor
- [x] **Variable canvas sizes** (1×1 to 128×128)
- [x] **Size presets**: 32×32, 64×64, 128×128, 32×64, 64×128, 64×32, 128×64, 16×128
- [x] **Custom size dialog** for arbitrary dimensions
- [x] **Export at actual size** (no forced upscaling)
- [x] Saves to picture library automatically

#### 20.6.2 Frame Picture Management ✅
- [x] Console commands for all operations
- [x] UI controls in picture browser
- [x] **Delete button** with thumbnail cache clearing
- [x] Refresh picture list after deletion

#### 20.6.3 Visual Feedback ✅
- [x] Picture browser shows thumbnails
- [x] Status messages for export/delete operations
- [x] Picture dimensions shown in list

#### 20.6.4 Shadow Behavior ✅
- [x] Frames receive shadows (normal lighting)
- [x] Frames do NOT cast shadows (skipped in shadow ray)
- [x] Identified by model ID range (160-175) in shader

---

## Future Enhancements (Post-MVP)

### 20.7: Advanced Features
- [ ] Animated pictures (GIF-like sequences)
- [ ] Picture transparency (see-through frames)
- [ ] Custom frame wood colors via paint system
- [ ] Picture borders and overlays

### 20.8: Social Features
- [ ] Share pictures between players (multiplayer)
- [ ] Community picture library browser

---

## Testing Checklist

### Unit Tests
- [x] `test_picture_metadata_encode_decode()` - Validate picture_id storage
- [ ] `test_multi_frame_uvs()` - Check UV calculations for each cluster position
- [ ] `test_delete_picture()` - Verify deletion removes from library

### Integration Tests
- [x] Place single frame with picture → verify voxels show correct colors
- [ ] Place 2×2 frame cluster → verify picture tiled correctly (needs 256×256 picture)
- [ ] Rotate frame → verify picture rotates correctly
- [ ] Break frame → verify cluster updates

### Performance Tests
- [ ] Place 100 picture frames → verify <5 FPS impact
- [ ] Load picture atlas with 64 pictures → verify memory usage (~4 MB)

### Manual Tests
- [x] Create picture in texture editor
- [x] **Select canvas size** from presets or custom dialog
- [x] Export to picture library at actual size
- [x] Place frame with selected picture
- [x] Delete picture from browser
- [ ] Build 2×2 frame cluster with 256×256 picture (when larger pictures supported)

---

## Success Criteria

### Phase 20 Complete When:
- [x] Frame models exist (8×8 sub-voxel with borders)
- [x] Frame metadata supports picture_id (20-bit ID)
- [x] Picture library can store RGBA pictures
- [x] Picture atlas supports GPU rendering (64 slots × 128×128)
- [x] Texture editor creates pixel art
- [x] **Variable canvas sizes** (1×1 to 128×128)
- [x] **Size presets** for common dimensions
- [x] **Custom size dialog** for arbitrary dimensions
- [x] **Export at actual size** (no forced upscaling)
- [x] Atlas centers smaller pictures in 128×128 slots
- [x] Picture pixels map to frame voxels (GPU shader)
- [x] Single frame clusters work correctly
- [x] All four rotations supported
- [x] Shader renders picture colors on frame voxels
- [x] GPU custom_data buffer properly uploads on frame placement
- [x] Shader hot reload works for all .glsl files
- [x] UI enables picture selection during frame placement
- [x] **Delete button removes pictures from library**
- [x] **Frames don't cast shadows**
- [x] Save/load persists picture_id in frame metadata
- [x] Console commands for picture management

---

## Known Issues & Limitations

### Current Limitations:
1. **Picture resolution**: Maximum 128×128 pixels per picture
   - Texture generator supports variable sizes (1×1 to 128×128)
   - Common presets: 32×32, 64×64, 128×128 (square); 32×64, 64×128 (tall); 64×32, 128×64 (wide); 16×128 (banner)
   - Custom dimensions supported (any size 1-128)
   - Pictures exported at actual size (no forced upscaling)
   - Larger clusters display the same picture at lower resolution per frame
   - 3×3 cluster shows each pixel at ~3× scale on each frame (for 128×128 pictures)
   - Use larger pictures (e.g., 256×256 when supported) for better multi-frame cluster quality

2. **Picture atlas** is fixed at 128×128 per slot:
   - 64 slots available (~4 MB VRAM)
   - LRU eviction when atlas is full
   - Smaller pictures are centered in 128×128 slots
   - Larger pictures are rejected (must be ≤128×128)

### Design Decisions:
- **128×128 maximum**: Balance of quality and memory
- **Variable canvas sizes**: Flexibility for different use cases (icons, banners, tall portraits, wide landscapes)
- **No forced upscaling**: Pictures exported at actual size for optimal quality
- **Automatic scaling**: Same picture works for all cluster sizes
- **Nearest-neighbor upscaling**: Previously used for 64×64→128×128, now no longer needed
- **No shadow casting**: Frames are decorative wall items
- **Global picture storage**: Pictures shared across all worlds

---

## Implementation Notes (2026-01-23):

**Completed Features:**
- Single frame rendering (128×128 pictures)
- GPU shader-based picture sampling
- Picture browser with delete functionality
- Console commands for picture management
- **Variable canvas sizes** (1×1 to 128×128) with preset options
- Size presets: 32×32, 64×64, 128×128, 32×64, 64×128, 64×32, 128×64, 16×128
- Custom size support via drag value inputs
- **Export at actual size** (no forced upscaling)
- Fixed UV edge wrapping issue
- Disabled shadow casting for frames
- LRU eviction for picture atlas
- Atlas centers smaller pictures in 128×128 slots

**Recent Changes (Variable Canvas Size):**
- Added `CanvasSize` struct with validation (1-128 range)
- Updated `CanvasState` to use dynamic size instead of `TEXTURE_SIZE` constant
- Added size selector UI with preset buttons and custom dialog
- Removed upscaling - pictures export at actual canvas size
- Updated atlas loading to center smaller pictures in 128×128 slots

**Rotation Mapping:**
- rotation 0 = North
- rotation 1 = West
- rotation 2 = South
- rotation 3 = East

**Bug Fixes:**
- Removed outdated 32×32 resize check in GPU upload
- Fixed UV calculation to prevent edge wrapping
- Changed from `(uv - 0.5) / size` to `(uv - 1.0) / (size - 1.0)`
- This ensures UV coordinates stay within [0, 1] without artifacts

**Atlas Configuration:**
- 64 slots × 128×128 pixels = 8192×128 atlas
- ~4 MB VRAM usage
- LRU eviction when full
- Smaller pictures centered in slots

**Canvas Size System:**
- `CanvasSize` struct with presets and custom size validation
- All drawing tools support variable dimensions
- Resize preserves existing pixels (centered when upsizing, clipped when downsizing)
- Undo history cleared when changing canvas size

---

*Last Updated: 2026-01-23*
*Phase Version: 2.6 - Variable Canvas Size Implementation*
**Status**: Fully Functional
- Single frames: ✓ Working
- Multi-frame clusters: ✓ Working (auto-scaling)
- Variable canvas sizes: ✓ Working (1-128 range)
- Size presets: ✓ Working (8 common sizes + custom)
- Export at actual size: ✓ Working
- Delete button: ✓ Working
- Shadow behavior: ✓ Working
- All console commands: ✓ Working

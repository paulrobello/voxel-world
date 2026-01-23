# Picture Frame Texture Integration

## Mission Statement

Enable players to display custom artwork in their worlds by integrating the in-game texture editor with picture frames. Players can create up to 384×384 pixel art in the texture editor, then place picture frames that render the custom artwork on the frame interior voxels using multi-frame clusters for larger displays.

---

## Overview

**Vision**: Picture frames are not just empty borders - they display custom pixel art created by the player. A single 1×1 frame shows a 128×128 picture. Multiple adjacent frames automatically form clusters to display larger pictures (e.g., a 2×3 frame cluster shows a 256×384 picture, and a 3×3 cluster shows a 384×384 picture).

**Key Features**:
- 128×128 pixel art per frame (supports up to 384×384 pictures for multi-frame clusters)
- Multi-frame clusters up to 3×3 for larger pictures (up to 384×384 pixels)
- Picture selection UI when placing frames
- Picture scaling across frame clusters
- Persistence: picture_id stored in frame metadata

---

## Technical Approach

### Coordinate Systems

**Picture Space** (Texture Editor):
- Up to 384×384 pixels per picture (for 3×3 frame clusters)
- Pixel coordinates: (0,0) to (383,383) for maximum size
- RGBA colors (256 levels per channel)

**Sub-Voxel Space** (Frame Model):
- 8×8 voxels on the front face (z=7)
- Each voxel represents 16×16 pixels of the picture (at 128×128 per frame resolution)
- Voxel coordinates: (0,0) to (7,7)

**World Space** (Frame Cluster):
- 1×1 to 3×3 blocks
- Cluster dimensions: width × height blocks
- Each block displays a 128×128 tile of the full picture

### Data Flow

```
Texture Editor → Picture Library → Picture Atlas → Frame Metadata → Shader Render
    (up to 384×384)  (pictures.bin)      (GPU)         (picture_id)     (voxel colors)
```

1. **Creation**: Player draws up to 384×384 art in texture editor (P key)
2. **Storage**: Picture saved to `~/.voxel_world/pictures.bin`
3. **Selection**: Player selects picture when placing frame
4. **Encoding**: `picture_id` stored in frame metadata
5. **Rendering**: Shader maps picture pixels to frame interior voxels

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
- `MAX_PICTURE_SIZE = 384` (supports 3×3 frame clusters)
- Global storage: `~/.voxel_world/pictures.bin`
- zstd compression for efficient storage
- `PictureLibrary` manager for add/get/delete/list

### 4. Picture Atlas ✅
- **File**: `src/pictures/atlas.rs`
- `MAX_GPU_PICTURES = 64` pictures on GPU at once
- GPU atlas: 8192×128 pixels (64 slots × 128 pixels wide)
- LRU eviction when full
- Slot-based GPU texture management
- Per-frame dirty tracking for efficient updates

### 5. Texture Editor ✅
- **File**: `src/pictures/editor.rs`
- P key to open editor
- Drawing tools: pencil, eraser, fill, eyedropper
- Canvas up to 128×128 pixels
- Undo/redo support
- Color picker with palette

### 6. Frame Auto-Sizing ✅
- **File**: `src/world/connections.rs`
- `update_frame_cluster()` function
- BFS detection of adjacent frames
- Automatic cluster size calculation
- Edge mask updates based on cluster position

### 7. GPU Rendering Pipeline ✅
- **File**: `shaders/models.glsl`
- Sub-voxel ray marching
- Per-voxel color lookup from palette
- Rotation support via `transformFramePos()`
- Model atlas with 8³/16³/32³ tiers

---

## Implementation Plan

### Phase 20.1: Core Picture-to-Voxel Mapping ✅

**Goal**: Map picture pixels to frame interior voxels at 4×4 resolution.

**Status**: ✅ COMPLETE (via GPU shader rendering)

**Implementation Note**: This phase was originally designed to use CPU-side color palette quantization and per-voxel color indexing. The actual implementation bypasses this entirely - pictures are rendered directly from the GPU texture atlas in the fragment shader. This provides:
- Full 32-bit RGBA color per pixel (no palette quantization needed)
- Better performance (no CPU-side processing)
- Simpler code path

The shader handles:
- Pixel sampling from picture atlas
- UV coordinate calculation for multi-frame clusters
- Rotation and offset handling
- Direct color output to fragment shader

---

### Phase 20.2: Frame Placement UI ✅

**Goal**: Enable picture selection when placing frames.

**Status**: ✅ COMPLETE

#### 20.2.1 Picture Browser Integration ✅
- [x] Add "Picture" button to frame placement HUD
  - Shows current selection: "No Picture" or picture name
  - Opens picture browser when clicked
- [x] Picture browser (similar to template/stencil browsers)
  - List of available pictures with thumbnails
  - Search/filter by name
  - Preview selected picture at 32×32 resolution

#### 20.2.2 Picture Selection State ✅
- [x] Store selected picture in `UIState` struct
  ```rust
  pub selected_picture_id: Option<u32>,  // None = empty frame
  ```
- [x] Persist selection across session (user_prefs.json)
- [x] UI indicator: "🖼️ Picture: [name or None]" in frame mode

#### 20.2.3 Frame Placement with Picture ✅
- [x] When placing frame, use selected picture_id
  ```rust
  let picture_id = self.ui.selected_picture_id.unwrap_or(0);
  let custom_data = frames::metadata::encode(
      picture_id,
      offset_x,
      offset_y,
      width,
      height,
      facing,
  );
  ```
- [x] First frame in cluster sets the picture_id for all frames
- [x] Adjacent frames inherit picture_id from cluster

---

### Phase 20.3: Shader Integration

**Goal**: Render picture colors on frame voxels in GPU shader.

**Status**: ✅ COMPLETE

#### 20.3.1 Picture Atlas Binding ✅
- [x] Add picture atlas descriptor set to shader
  ```glsl
  layout(set = 5, binding = 0) uniform texture2D pictureAtlas;
  ```
- [x] Pass picture atlas to shader during render
- [x] Update descriptor set layout in `traverse.comp`

#### 20.3.2 Picture Metadata in Push Constants ✅
- [x] Add picture_id lookup to push constants
  ```glsl
  layout(set = 0, binding = 14) uniform utexture2D blockCustomData;  // R32_UINT per block
  ```
- [x] Support per-model picture data (max 64 pictures)
- [x] custom_data encoding: picture_id (20 bits), offset_x/y (4 bits each), width/height (4 bits each), facing (2 bits)

#### 20.3.3 Pixel Sampling in Shader ✅
- [x] `samplePictureColor(picture_id, voxel_uv)` function in `shaders/models.glsl`
  ```glsl
  vec4 samplePictureColor(uint picture_id, vec2 uv) {
      if (picture_id == 0u) return vec4(1.0, 1.0, 1.0, 1.0);  // Empty frame
      picture_id = min(picture_id, PICTURE_ATLAS_SLOT_COUNT - 1u);
      vec2 atlas_uv = vec2(
          (float(picture_id) * PICTURE_ATLAS_SIZE + uv.x * PICTURE_ATLAS_SIZE) / float(PICTURE_ATLAS_WIDTH),
          uv.y
      );
      return texture(pictureAtlas, atlas_uv);
  }
  ```
- [x] Apply picture color to interior voxels via `getFramePictureColor()`
- [x] Fall back to wood color for non-picture faces

#### 20.3.4 Multi-Frame UV Calculation ✅
- [x] Calculate correct UV based on frame offset in cluster
  ```glsl
  // Account for 1-voxel borders (picture area is 30×30 for 32³ frames)
  float picture_size = float(res) - 2.0;
  float base_u = (float(uv_x) - 1.0 - 0.5) / picture_size;
  float local_v = 1.0 - (float(uv_y) - 1.0 - 0.5) / picture_size;

  // Multi-frame cluster offset handling
  float inverted_offset_x = float(offset_x);
  float inverted_offset_y = float(offset_y);
  if (cluster_width > 1u || cluster_height > 1u) {
      inverted_offset_y = float(cluster_height - 1u - offset_y);
      if (rotation == 2u || rotation == 3u) {  // South/East
          inverted_offset_x = float(cluster_width - 1u - offset_x);
      }
  }
  float picture_u = (base_u + inverted_offset_x) / float(cluster_width);

  // Flip horizontally for North/West to fix mirroring
  if (rotation == 0u || rotation == 1u) {
      picture_u = 1.0 - picture_u;
  }
  float picture_v = (local_v + inverted_offset_y) / float(cluster_height);
  ```
- [x] Single frames (cluster_width=1) use offsets directly (no inversion)
- [x] Multi-frame clusters apply offset inversion for correct ordering

---

### Phase 20.4: Multi-Frame Cluster Support

**Goal**: Display larger pictures across multiple frames.

**Status**: ✅ COMPLETE

#### 20.4.1 Cluster Picture Sizing ✅
- [x] Support for 1×1, 2×2, and 3×3 frame clusters
  - Single frames (1×1) display full 32×32 picture
  - 2×2 clusters display 64×64 pictures
  - 3×3 clusters display 96×96 pictures (max)
- [x] Picture scales across cluster dimensions automatically
- [x] All four rotations supported (North, East, South, West)

#### 20.4.2 Picture Tiling Metadata ✅
- [x] Store cluster dimensions in frame metadata
  - `width` and `height` encode cluster size (1-3)
  - `offset_x` and `offset_y` encode position within cluster (0-2)
- [x] `update_frame_cluster()` in `src/world/connections.rs` propagates picture info
  - All frames in cluster share same picture_id
  - Each frame stores its offset in the cluster
  - Automatic cluster detection via BFS

#### 20.4.3 Cluster UV Mapping ✅
- [x] Calculate per-frame UV offsets for shader
  - Single frames: offset=0, shows full picture
  - Multi-frame: offset determines which 32×32 region to display
  - Proper row ordering (offset_y inverted for correct top-to-bottom)
  - Direction-specific column ordering (East/South offset_x inverted)
- [x] Pass offsets to GPU via custom_data buffer (R32_UINT per block)
- [x] GPU upload bug fixed: `set_model_block_with_data` now marks `custom_data_dirty`

---

### Phase 20.5: Performance & Optimization

**Goal**: Maintain 90+ FPS with picture frames.

**Status**: NOT STARTED

#### 20.5.1 Picture Atlas Caching
- [ ] Pre-load pictures for visible chunks
- [ ] LRU eviction based on frame visibility
- [ ] Async loading for large worlds

#### 20.5.2 Level of Detail (LOD)
- [ ] Downsample pictures for distant frames
  - Near (<32 blocks): Full 32×32
  - Medium (32-64): 16×16
  - Far (>64): 8×8 or solid color
- [ ] Update shader to use LOD mipmaps

#### 20.5.3 Batching Optimization
- [ ] Group frames by picture_id for efficient rendering
- [ ] Minimize texture binds between frame draws

---

### Phase 20.6: Polish & UX

**Goal**: Smooth user experience for picture frame workflow.

**Status**: IN PROGRESS

#### 20.6.1 Picture Editor Integration
- [ ] Add "Use as Frame Picture" button in texture editor
  - Saves picture and selects it for frame placement
  - Smooth workflow: Draw → Save → Place
- [ ] Show recommended dimensions (32×32, 64×64, 96×96)

#### 20.6.2 Frame Picture Management ✅
- [x] Console commands:
  - `/frame picture list` - List all pictures
  - `/frame picture set <id>` - Select picture for placement
  - `/frame picture clear` - Deselect (place empty frames)
- [x] UI controls in picture browser:
  - [x] "Set as Active" button (via picture browser)
  - [x] Preview in frame context
  - [ ] Delete unused pictures

#### 20.6.3 Visual Feedback
- [ ] Frame preview shows selected picture
  - Ghost preview has picture rendered
  - Holographic preview via stencil buffer
- [ ] Error messages:
  - "Picture too large for frame cluster"
  - "Invalid picture dimensions"
- [ ] Success feedback:
  - "Picture [name] applied to frame"

---

## Future Enhancements (Post-MVP)

### 20.7: Advanced Features
- [ ] Animated pictures (GIF-like sequences)
- [ ] Picture transparency (see-through frames)
- [ ] Picture scaling/stretching options
- [ ] Custom frame wood colors via paint system
- [ ] Rotated pictures (portrait vs landscape orientation)
- [ ] Picture borders and overlays

### 20.8: Social Features
- [ ] Share pictures between players (multiplayer)
- [ ] Picture marketplace/trading
- [ ] Community picture library browser
- [ ] Artist attribution for pictures

---

## Testing Checklist

### Unit Tests
- [ ] `test_pixel_sampling_algorithm()` - Verify 4×4 pixel → voxel mapping
- [ ] `test_multi_frame_uvs()` - Check UV calculations for each cluster position
- [ ] `test_picture_metadata_encode_decode()` - Validate picture_id storage
- [ ] `test_palette_quantization()` - Test color reduction to 32 colors

### Integration Tests
- [ ] Place single frame with picture → verify voxels show correct colors
- [ ] Place 2×2 frame cluster → verify picture tiled correctly
- [ ] Place 3×3 frame cluster → verify full 96×96 picture
- [ ] Rotate frame → verify picture rotates correctly
- [ ] Break frame → verify cluster updates

### Performance Tests
- [ ] Place 100 picture frames → verify <5 FPS impact
- [ ] View 1000 picture frames → verify stable 60+ FPS
- [ ] Load picture atlas with 64 pictures → verify memory usage

### Manual Tests
- [ ] Create picture in texture editor
- [ ] Place frame with selected picture
- [ ] Build 2×2 frame cluster
- [ ] Rotate cluster and verify orientation
- [ ] Save/load world with picture frames
- [ ] Delete picture from library → verify frames handle missing picture

---

## Success Criteria

### Phase 20 Complete When:
- [x] Frame models exist (8×8 sub-voxel with borders)
- [x] Frame metadata supports picture_id (20-bit ID)
- [x] Picture library can store RGBA pictures
- [x] Picture atlas supports GPU rendering
- [x] Texture editor creates 32×32 pixel art
- [x] Picture pixels map to frame voxels at correct resolution (GPU shader)
- [x] Multi-frame clusters display tiled pictures (1×1, 2×2, 3×3)
- [x] All four rotations supported (North, East, South, West)
- [x] Shader renders picture colors on frame voxels
- [x] GPU custom_data buffer properly uploads on frame placement
- [x] Shader hot reload works for all .glsl files
- [x] UI enables picture selection during frame placement
- [ ] Performance maintained at 90+ FPS
- [x] Save/load persists picture_id in frame metadata
- [x] Console commands for picture management
- [ ] Error handling for invalid/missing pictures

### Implementation Notes (2026-01-22):

**Completed Features:**
- Multi-frame cluster rendering with proper UV coordinate calculation
- Single frame rendering (offset inversion only for multi-frame clusters)
- GPU custom_data upload fix (was breaking 2×2 placement)
- Shader hot reload now watches all .glsl files in shaders directory
- Picture selection UI implemented (picture browser, persistence, auto-scaling)
- Picture rendering via GPU shader (bypasses CPU-side palette quantization)
- Increased resolution to 384×384 per picture (supports 3×3 frame clusters)

**Rotation Mapping Discovered:**
- rotation 0 = North
- rotation 1 = West
- rotation 2 = South
- rotation 3 = East

**Known Limitations:**
- Maximum cluster size: 3×3 (384×384 pixels total)
- Maximum single picture: 384×384 pixels
- Performance optimization (LOD, batching) not yet implemented

---

## Open Questions

1. **Color Palette**: Should we use the existing 32-color sub-voxel palette, or create a dedicated picture palette?
   - **Decision**: Use existing 32-color palette with quantization for MVP, consider custom palette later

2. **Picture Storage**: Should pictures be per-world or global?
   - **Current**: Global (`~/.voxel_world/pictures.bin`)
   - **Decision**: Keep global for simplicity, add per-world export/import later

3. **Maximum Picture Size**: Is 384×384 per picture sufficient?
   - **Decision**: Yes, 384×384 per picture supports 3×3 frame clusters = 384×384 total
   - 64 pictures can be stored simultaneously (~37.5 MB VRAM for atlas)

4. **Picture Rotation**: Should pictures rotate with frames?
   - **Decision**: Yes, use existing `facing` metadata for rotation

5. **Empty Frames**: Should picture_id = 0 be valid (empty frame)?
   - **Decision**: Yes, allows placement of frames without pictures

---

*Last Updated: 2026-01-23*
*Phase Version: 2.3 - Resolution Increased to 384×384*

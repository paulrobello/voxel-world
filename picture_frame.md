# Picture Frame Texture Integration

## Mission Statement

Enable players to display custom artwork in their worlds by integrating the in-game texture editor with picture frames. Players can create 32×32 pixel art in the texture editor, then place picture frames that render the custom artwork on the frame interior voxels using multi-frame clusters for larger displays.

---

## Overview

**Vision**: Picture frames are not just empty borders - they display custom pixel art created by the player. A single 1×1 frame shows a 32×32 picture. Multiple adjacent frames automatically form clusters to display larger pictures (e.g., a 2×3 frame cluster shows a 64×96 picture).

**Key Features**:
- 32×32 pixel art per frame (8×8 sub-voxels × 4 pixels per voxel)
- Multi-frame clusters up to 3×3 for larger pictures (up to 96×96 pixels)
- Picture selection UI when placing frames
- Picture scaling across frame clusters
- Persistence: picture_id stored in frame metadata

---

## Technical Approach

### Coordinate Systems

**Picture Space** (Texture Editor):
- 32×32 pixels per frame
- Pixel coordinates: (0,0) to (31,31)
- RGBA colors (256 levels per channel)

**Sub-Voxel Space** (Frame Model):
- 8×8 voxels on the front face (z=7)
- Each voxel represents 4×4 pixels of the picture
- Voxel coordinates: (0,0) to (7,7)

**World Space** (Frame Cluster):
- 1×1 to 3×3 blocks
- Cluster dimensions: width × height blocks
- Each block displays a 32×32 tile of the full picture

### Data Flow

```
Texture Editor → Picture Library → Picture Atlas → Frame Metadata → Shader Render
    (32×32)        (pictures.bin)      (GPU)         (picture_id)     (voxel colors)
```

1. **Creation**: Player draws 32×32 art in texture editor (P key)
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
- `MAX_PICTURE_SIZE = 256`
- Global storage: `~/.voxel_world/pictures.bin`
- zstd compression for efficient storage
- `PictureLibrary` manager for add/get/delete/list

### 4. Picture Atlas ✅
- **File**: `src/pictures/atlas.rs`
- `MAX_GPU_PICTURES = 64` pictures on GPU at once
- LRU eviction when full
- Slot-based GPU texture management
- Per-frame dirty tracking for efficient updates

### 5. Texture Editor ✅
- **File**: `src/pictures/editor.rs`
- P key to open editor
- Drawing tools: pencil, eraser, fill, eyedropper
- 32×32 canvas (can be extended to 32×N for multi-frame)
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

**Status**: NOT STARTED

#### 20.1.1 Pixel Sampling Algorithm
- [ ] `sample_pixel_for_voxel(picture, voxel_x, voxel_y, frame_x, frame_y, picture_id)` function
  - Input: picture data, voxel position (0-7), frame cluster position
  - Output: RGBA color for the voxel
  - Algorithm:
    ```rust
    // Voxel (vx, vy) covers pixels (vx*4 .. vx*4+3, vy*4 .. vy*4+3)
    // Use center pixel for color: (vx*4 + 2, vy*4 + 2)
    let pixel_x = (cluster_x * 32) + (voxel_x * 4) + 2;
    let pixel_y = (cluster_y * 32) + (voxel_y * 4) + 2;
    picture.get_pixel(pixel_x, pixel_y)
    ```

#### 20.1.2 Multi-Frame Picture Tiling
- [ ] Extend picture library to support non-square dimensions
  - Store width/height separately (currently max 256×256)
  - Support 32×N, 64×N, 96×N dimensions
- [ ] Calculate tile position from frame metadata
  - `offset_x` (0-2) = column in cluster
  - `offset_y` (0-2) = row in cluster
  - Sample correct 32×32 region from source picture

#### 20.1.3 Voxel Color Generation
- [ ] Generate dynamic color palette for picture frames
  - Analyze picture colors during placement
  - Create 32-color palette using k-means or quantization
  - Fall back to default 32 colors if picture has >32 unique colors
- [ ] Set voxel color indices instead of hardcoded color 4
  ```rust
  for y in 0..8 {
      for x in 0..8 {
          let color = sample_pixel_for_voxel(...);
          let palette_index = quantize_to_palette(color);
          model.set_voxel(x, y, 7, palette_index);
      }
  }
  ```

---

### Phase 20.2: Frame Placement UI

**Goal**: Enable picture selection when placing frames.

**Status**: NOT STARTED

#### 20.2.1 Picture Browser Integration
- [ ] Add "Picture" button to frame placement HUD
  - Shows current selection: "No Picture" or picture name
  - Opens picture browser when clicked
- [ ] Picture browser (similar to template/stencil browsers)
  - List of available pictures with thumbnails
  - Search/filter by name
  - Preview selected picture at 32×32 resolution

#### 20.2.2 Picture Selection State
- [ ] Store selected picture in `BlockInteraction` struct
  ```rust
  struct BlockInteraction {
      selected_picture_id: Option<u32>,  // None = empty frame
      // ... existing fields
  }
  ```
- [ ] Persist selection across session (user_prefs.json)
- [ ] UI indicator: "🖼️ Picture: [name or None]" in frame mode

#### 20.2.3 Frame Placement with Picture
- [ ] When placing frame, use selected picture_id
  ```rust
  let picture_id = self.selected_picture_id.unwrap_or(0);
  let custom_data = frames::metadata::encode(
      picture_id,
      offset_x,
      offset_y,
      width,
      height,
      facing,
  );
  ```
- [ ] First frame in cluster sets the picture_id for all frames
- [ ] Adjacent frames inherit picture_id from cluster

---

### Phase 20.3: Shader Integration

**Goal**: Render picture colors on frame voxels in GPU shader.

**Status**: NOT STARTED

#### 20.3.1 Picture Atlas Binding
- [ ] Add picture atlas descriptor set to shader
  ```glsl
  layout(set = 5, binding = 0) uniform texture2D pictureAtlas;
  layout(set = 5, binding = 1) uniform sampler pictureSampler;
  ```
- [ ] Pass picture atlas to shader during render
- [ ] Update descriptor set layout in `traverse.comp`

#### 20.3.2 Picture Metadata in Push Constants
- [ ] Add picture_id lookup to push constants
  ```glsl
  struct PictureInfo {
      uint picture_id;
      uint atlas_slot;
      vec2 uv_offset;  // For multi-frame clusters
  };
  ```
- [ ] Support per-model picture data (max 64 pictures)

#### 20.3.3 Pixel Sampling in Shader
- [ ] `samplePictureColor(picture_id, voxel_uv)` function
  ```glsl
  vec4 samplePictureColor(uint picture_id, vec2 uv) {
      uint slot = pictureAtlasSlots[picture_id];
      vec2 atlas_uv = slot_to_uv(slot, uv);
      return texture(pictureAtlas, atlas_uv);
  }
  ```
- [ ] Apply picture color to interior voxels (color index 4)
- [ ] Fall back to palette lookup if picture_id = 0

#### 20.3.4 Multi-Frame UV Calculation
- [ ] Calculate correct UV based on frame offset in cluster
  ```glsl
  vec2 getPictureUV(vec3 voxel_pos, uint offset_x, uint offset_y) {
      // voxel_pos is in model space (0-7)
      // offset_x/y are tile coordinates (0-2)
      vec2 local_uv = voxel_pos.xy / 8.0;  // 0-1 within frame
      vec2 cluster_offset = vec2(offset_x, offset_y) / 3.0;
      return local_uv / 3.0 + cluster_offset;
  }
  ```

---

### Phase 20.4: Multi-Frame Cluster Support

**Goal**: Display larger pictures across multiple frames.

**Status**: NOT STARTED

#### 20.4.1 Cluster Picture Sizing
- [ ] Detect picture size vs cluster size
  - 32×32 → 1×1 frame
  - 64×32 → 2×1 frames
  - 64×64 → 2×2 frames
  - 96×96 → 3×3 frames (max)
- [ ] Validate cluster fits picture dimensions
  - Warn if picture larger than cluster
  - Center or crop picture if mismatch

#### 20.4.2 Picture Tiling Metadata
- [ ] Store picture dimensions in frame metadata
  - Reuse `width` and `height` fields (currently for cluster size)
  - Or add new metadata format version
- [ ] Update `update_frame_cluster` to propagate picture info
  - All frames in cluster share same picture_id
  - Each frame stores its offset in the cluster

#### 20.4.3 Cluster UV Mapping
- [ ] Calculate per-frame UV offsets for shader
  - Frame at (0, 0) shows top-left 32×32 region
  - Frame at (1, 0) shows top-middle 32×32 region
  - etc.
- [ ] Pass offsets to GPU via model metadata

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

**Status**: NOT STARTED

#### 20.6.1 Picture Editor Integration
- [ ] Add "Use as Frame Picture" button in texture editor
  - Saves picture and selects it for frame placement
  - Smooth workflow: Draw → Save → Place
- [ ] Show recommended dimensions (32×32, 64×64, 96×96)

#### 20.6.2 Frame Picture Management
- [ ] Console commands:
  - `/frame picture list` - List all pictures
  - `/frame picture set <id>` - Select picture for placement
  - `/frame picture clear` - Deselect (place empty frames)
- [ ] UI controls in picture browser:
  - "Set as Active" button
  - Preview in frame context
  - Delete unused pictures

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
- [ ] Picture pixels map to frame voxels at 4×4 resolution
- [ ] Multi-frame clusters display tiled pictures
- [ ] UI enables picture selection during frame placement
- [ ] Shader renders picture colors on frame voxels
- [ ] Performance maintained at 90+ FPS
- [ ] Save/load persists picture_id in frame metadata
- [ ] Console commands for picture management
- [ ] Error handling for invalid/missing pictures

---

## Open Questions

1. **Color Palette**: Should we use the existing 32-color sub-voxel palette, or create a dedicated picture palette?
   - **Decision**: Use existing 32-color palette with quantization for MVP, consider custom palette later

2. **Picture Storage**: Should pictures be per-world or global?
   - **Current**: Global (`~/.voxel_world/pictures.bin`)
   - **Decision**: Keep global for simplicity, add per-world export/import later

3. **Maximum Picture Size**: Is 96×96 (3×3 frames) sufficient?
   - **Decision**: Yes for MVP, consider 4×4 or larger in future

4. **Picture Rotation**: Should pictures rotate with frames?
   - **Decision**: Yes, use existing `facing` metadata for rotation

5. **Empty Frames**: Should picture_id = 0 be valid (empty frame)?
   - **Decision**: Yes, allows placement of frames without pictures

---

*Last Updated: 2026-01-22*
*Phase Version: 1.0 - Initial Planning*

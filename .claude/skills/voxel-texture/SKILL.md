---
name: voxel-texture
description: >
  Generate seamless tileable block textures for voxel games using nanobanana MCP.
  Use when: creating game textures, generating block assets, building texture atlases.
  Triggers: "generate texture", "create block texture", "voxel texture", "/voxel-texture"
---

# Voxel Texture Generator

Generate professional seamless tileable block textures for voxel games using nanobanana MCP and ImageMagick processing.

## CRITICAL: MCP Availability Requirement

**⚠️ STOP IMMEDIATELY if nanobanana MCP is not available.**

This skill REQUIRES the nanobanana MCP server for texture generation. Do NOT:
- Fall back to other image generation methods
- Attempt to use WebSearch or WebFetch for textures
- Generate placeholder textures
- Skip texture generation

If nanobanana MCP is unavailable:
1. Check if the server is running with MCP tools list
2. Inform the user the texture cannot be generated without nanobanana
3. Wait for user to restart the MCP server
4. Do not proceed with texture integration until generation succeeds

## Philosophy: Flat Patterns, Not 3D Objects

**The #1 mistake**: AI image generators often create 3D cube renders instead of flat tileable patterns.

Before generating, understand:
- **We need FLAT 2D texture patterns**, not 3D objects
- **Top-down orthographic view**, like looking straight down at a surface
- **No perspective, no cube faces, no isometric angles**
- **Seamless tiling on all four edges** is non-negotiable
- **Texture resolution matters**: This project uses 64x64 as the standard

**Core principles**:
1. **Flat first**: If it looks 3D or has perspective, it's wrong - regenerate
2. **Seamless mandatory**: Test with tiled preview before accepting
3. **Pixel art crispness**: Use `-filter point -resize 64x64!` for exact sizing
4. **Material authenticity**: Each texture should capture material essence in flat pattern form
5. **Verification before integration**: Always display and visually check before adding to atlas

## Workflow

### Step 1: Verify MCP Availability

```bash
# Check if nanobanana MCP is available
# Should see mcp__nanobanana__generate_image in tools list
```

If not available, STOP and inform user.

### Step 2: Generate with Nanobanana MCP

**Use this exact prompt structure** to avoid 3D renders:

```
Create a flat, seamless tileable {block_type} texture pattern for a voxel block game (top-down orthographic view, NOT a 3D cube).

The texture should be:
- A FLAT 2D pattern that tiles seamlessly on all edges
- {material_color_description}
- {material_pattern_description}
- {material_detail_description}
- Sharp, clean pixel art style suitable for 64x64 resolution
- Should look like looking straight down at a flat {material} surface, not a 3D object
- No perspective, no cube faces, no depth illusion

Style: Minecraft-style pixel art texture, flat top-down orthographic view, seamless edges for perfect tiling. This is a TEXTURE PATTERN, not a 3D rendered object.
```

**Critical MCP Parameters**:
```python
mcp__nanobanana__generate_image(
    prompt="<detailed prompt above>",
    aspect_ratio="1:1",          # Always square
    resolution="high",            # High quality for textures
    model_tier="pro",            # Pro model for better quality
    negative_prompt="3D render, cube, isometric, perspective, shadows, lighting effects, depth, dimensional, rendered object, 3D model, box, block shape, geometric solid",
    output_path="/Users/probello/Repos/voxel_world/textures/{block_type}_64x64.png"
)
```

**Why these negative prompts matter**:
- `3D render, cube, isometric` - Prevents 3D object generation
- `perspective, depth, dimensional` - Enforces flat orthographic view
- `shadows, lighting effects` - Avoids baked-in lighting
- `rendered object, 3D model` - Explicitly rejects 3D interpretations

### Step 3: Verify Generation Quality

**CRITICAL CHECKS** before proceeding:

1. **Check dimensions**: Generated image may not be 64x64
   ```bash
   identify {block_type}_64x64.png
   # Should show: 64x64 or will need resizing
   ```

2. **Visual inspection**: Does it look flat or 3D?
   - ✅ GOOD: Flat pattern, could be wallpaper, tiles seamlessly
   - ❌ BAD: Looks like a cube, has perspective, edges suggest 3D shape

3. **If it looks 3D**: Regenerate immediately with stronger negative prompts

### Step 4: Process with ImageMagick

If dimensions are wrong or need adjustment:

```bash
cd /Users/probello/Repos/voxel_world/textures

# Get actual dimensions first
identify {block_type}_64x64.png

# If not 64x64, resize with exact dimensions and point filter for crisp pixels
magick {block_type}_64x64.png -filter point -resize 64x64! {block_type}_64x64.png

# Verify result
identify {block_type}_64x64.png
# Must output: {block_type}_64x64.png PNG 64x64 ...

# Create tiled preview to check seamless edges (2x2 grid)
magick {block_type}_64x64.png \( +clone \) +append \( +clone \) -append {block_type}_tiled_preview.png
```

**Why `-filter point -resize 64x64!`**:
- `-filter point`: Nearest-neighbor sampling preserves pixel art sharpness
- `64x64!`: Exclamation forces exact dimensions, ignores aspect ratio
- Without `!`: ImageMagick might produce 64x63 or similar off-by-one errors

### Step 5: Visual Verification

Display both:
1. **Single tile**: `{block_type}_64x64.png` - Check if pattern is flat and clear
2. **Tiled 2x2**: `{block_type}_tiled_preview.png` - Verify NO visible seams at edges

**Red flags requiring regeneration**:
- Visible seams where tiles meet
- 3D cube appearance or perspective
- Blurry edges (wrong resize filter used)
- Dimensions not exactly 64x64

### Step 6: Integrate into Texture Atlas

Once verified, add to the texture atlas:

```bash
cd /Users/probello/Repos/voxel_world/textures

# Regenerate atlas with new texture (see CLAUDE.md for current atlas order)
# Example with ice at position 26:
magick air_64x64.png stone_64x64.png dirt_64x64.png grass_64x64.png planks_64x64.png \
  leaves_64x64.png sand_64x64.png gravel_64x64.png water_64x64.png glass_64x64.png \
  log_64x64.png torch_64x64.png brick_64x64.png snow_64x64.png cobblestone_64x64.png \
  iron_64x64.png bedrock_64x64.png grass_side_64x64.png log_top_64x64.png \
  lava_64x64.png glowstone_64x64.png glowmushroom_64x64.png crystal_64x64.png \
  cactus_64x64.png mud_64x64.png sandstone_64x64.png {block_type}_64x64.png \
  +append texture_atlas.png

# Verify atlas dimensions (should be N*64 x 64 where N is number of textures)
identify texture_atlas.png
```

**Important**: The atlas position determines the TEX_* constant in `shaders/materials.glsl`. Coordinate with CLAUDE.md for proper block type integration.

## Block Type Material Descriptions

Use these as templates for the `{material_*_description}` fields:

### Stone
```
- Color: Medium gray (#808080 to #A0A0A0) with subtle blue-gray variations
- Pattern: Random cracks, small mineral flecks, rough rocky surface texture
- Details: Irregular weathering patterns, slight color mottling, hard surface feel
```

### Dirt
```
- Color: Rich brown (#6B4423 to #8B5A2B) with darker organic matter
- Pattern: Small pebbles, bits of organic material, soil grain texture
- Details: Earthy clumps, root fragments, natural soil variation
```

### Grass (top view)
```
- Color: Vibrant green (#4CAF50 to #66BB6A) with yellow-green highlights
- Pattern: Individual grass blades visible, overlapping leaf texture
- Details: Varied blade directions, slight color variation for depth, organic randomness
```

### Ice
```
- Color: Light blue-white (#B3D9E6 to #E0F2F7), semi-transparent appearance
- Pattern: Crystalline frost patterns, internal fracture lines, frozen striations
- Details: Air bubbles frozen inside, crystal formations, subtle brightness variation
```

### Sand
```
- Color: Light tan/beige (#EDC9AF to #F5DEB3) with warm golden tones
- Pattern: Fine grain texture, tiny pebbles scattered, desert sand feel
- Details: Subtle wind-swept patterns, slight color speckling, granular appearance
```

### Wood/Planks
```
- Color: Warm brown oak (#8B6F47 to #A0826D) with natural variation
- Pattern: Wood grain lines, knots, plank texture with subtle splits
- Details: Growth rings visible, grain direction, natural wood character
```

### Leaves
```
- Color: Forest green (#2E7D32 to #4CAF50) with varied leaf tones
- Pattern: Dense overlapping foliage, individual leaf shapes suggested
- Details: Light filtering effect, leaf veins, organic clustering
```

### Water
```
- Color: Translucent blue (#2196F3 to #64B5F6) with depth variation
- Pattern: Subtle ripple texture, gentle wave undulation
- Details: Light refraction suggestion, movement feel, liquid surface
```

### Glass
```
- Color: Very light blue tint (#E3F2FD to #F0F8FF), mostly clear
- Pattern: Subtle reflections, very faint surface texture
- Details: Transparency suggestion, slight highlights, clean manufactured feel
```

### Gravel
```
- Color: Gray-brown mix (#708090 to #8B7D6B) with stone variation
- Pattern: Small rocks and pebbles scattered, varied sizes
- Details: Individual stones visible, gaps between pebbles, rough texture
```

## Output Location

**Project path**: `/Users/probello/Repos/voxel_world/textures/`

**File naming convention**:
- `{block_type}_64x64.png` - Final game-ready 64x64 texture (REQUIRED)
- `{block_type}_tiled_preview.png` - 2x2 tiling verification (RECOMMENDED)
- `texture_atlas.png` - Complete atlas with all textures horizontally concatenated

**Do NOT create**:
- `{block_type}_original.png` - Not needed, MCP output goes directly to 64x64
- Multiple resolution variants (16x16, 32x32) - This project uses 64x64 only

## Anti-Patterns to Avoid

### 1. Accepting 3D Cube Renders
**Why bad**: Textures with cube perspective don't tile and look wrong on flat surfaces
**How to avoid**:
- Use strong negative prompts: `3D render, cube, isometric, perspective`
- Explicitly state "flat top-down view, NOT a 3D cube" in prompt
- Regenerate if output shows any 3D characteristics

### 2. Skipping Dimension Verification
**Why bad**: AI often generates 1408x768 or other non-square dimensions
**How to avoid**:
- Always run `identify` to check actual dimensions
- Use `magick -filter point -resize 64x64!` to force exact size
- Verify with second `identify` after resize

### 3. Using Wrong Resize Filter
**Why bad**: Default bilinear filter blurs pixel art, ruins crisp edges
**How to avoid**: Always use `-filter point` for nearest-neighbor sampling

### 4. Forgetting the `!` in Resize
**Why bad**: ImageMagick may produce 64x63 or 63x64 without exact flag
**How to avoid**: Always use `64x64!` (with exclamation) to force exact dimensions

### 5. Skipping Tiled Preview
**Why bad**: Seams only visible when tiles repeat, single tile looks fine
**How to avoid**: Always create and display 2x2 tiled preview for verification

### 6. Generic Prompts
**Why bad**: "ice texture" produces wildly inconsistent results
**How to avoid**: Use full structured prompt with color, pattern, and detail descriptions

### 7. Proceeding Without MCP
**Why bad**: Wastes time, produces unusable results, requires regeneration
**How to avoid**: Check MCP availability FIRST, stop if unavailable

## Troubleshooting

### Problem: Generated image is a 3D cube render
**Solution**:
1. Add stronger negative prompts emphasizing "NOT a 3D cube"
2. Emphasize "flat 2D pattern" and "top-down view" multiple times in prompt
3. Use phrase "texture pattern, not a 3D rendered object" explicitly
4. Regenerate with `model_tier="pro"` for better prompt following

### Problem: Texture has visible seams when tiled
**Solution**:
1. This is an AI generation failure - the texture wasn't truly seamless
2. Regenerate with emphasis on "tiles seamlessly on all edges"
3. Check if resize operation introduced seams (use `-filter point`)
4. Display tiled preview to user and request regeneration if seams visible

### Problem: Texture dimensions are 1408x768 or other wrong size
**Solution**:
```bash
magick {block_type}_64x64.png -filter point -resize 64x64! {block_type}_64x64.png
identify {block_type}_64x64.png  # Verify shows 64x64
```

### Problem: Texture looks blurry after resize
**Solution**:
- You forgot `-filter point`, ImageMagick used default bilinear
- Regenerate the resize operation with `-filter point` flag

### Problem: Atlas generation fails or looks wrong
**Solution**:
1. Verify ALL textures are exactly 64x64: `identify *_64x64.png`
2. Check for non-64x64 files: `identify *_64x64.png | grep -v "64x64"`
3. Fix any wrong-sized textures before regenerating atlas
4. Verify atlas is `(N*64)x64` dimensions where N = texture count

## Workflow Example: Ice Texture

```bash
# Step 1: Check MCP availability (done via tool check)

# Step 2: Generate with MCP
mcp__nanobanana__generate_image(
    prompt="""Create a flat, seamless tileable ice texture pattern for a voxel block game (top-down orthographic view, NOT a 3D cube).

The texture should be:
- A FLAT 2D pattern that tiles seamlessly on all edges
- Light blue-white color (#B3D9E6 to #E0F2F7), semi-transparent appearance
- Crystalline frost patterns and ice formations distributed across the surface
- Small air bubbles and natural ice striations
- Sharp, clean pixel art style suitable for 64x64 resolution
- Should look like looking straight down at a frozen ice surface, not a 3D object
- No perspective, no cube faces, no depth illusion

Style: Minecraft-style pixel art texture, flat top-down orthographic view, seamless edges for perfect tiling. This is a TEXTURE PATTERN, not a 3D rendered object.""",
    aspect_ratio="1:1",
    resolution="high",
    model_tier="pro",
    negative_prompt="3D render, cube, isometric, perspective, shadows, lighting effects, depth, dimensional, rendered object, 3D model, box, block shape, geometric solid",
    output_path="/Users/probello/Repos/voxel_world/textures/ice_64x64.png"
)

# Step 3: Verify dimensions
identify ice_64x64.png
# If not 64x64, resize:

# Step 4: Force correct size with point filter
magick ice_64x64.png -filter point -resize 64x64! ice_64x64.png

# Step 5: Create tiled preview
magick ice_64x64.png \( +clone \) +append \( +clone \) -append ice_tiled_preview.png

# Step 6: Display for verification
# (Show both ice_64x64.png and ice_tiled_preview.png to user)

# Step 7: If approved, integrate into atlas
magick air_64x64.png stone_64x64.png dirt_64x64.png grass_64x64.png planks_64x64.png \
  leaves_64x64.png sand_64x64.png gravel_64x64.png water_64x64.png glass_64x64.png \
  log_64x64.png torch_64x64.png brick_64x64.png snow_64x64.png cobblestone_64x64.png \
  iron_64x64.png bedrock_64x64.png grass_side_64x64.png log_top_64x64.png \
  lava_64x64.png glowstone_64x64.png glowmushroom_64x64.png crystal_64x64.png \
  cactus_64x64.png mud_64x64.png sandstone_64x64.png ice_64x64.png \
  +append texture_atlas.png

identify texture_atlas.png  # Should be 1728x64 (27 textures)
```

## Remember

**Quality gates** - verify at each step:
1. ✅ MCP available before starting
2. ✅ Generated image is FLAT, not 3D
3. ✅ Dimensions are exactly 64x64 pixels
4. ✅ Tiled preview shows NO seams
5. ✅ Texture captures material essence
6. ✅ Atlas regenerates correctly

**If any gate fails**: Stop, fix the issue, and re-verify before proceeding.

Good textures are the foundation of visual quality. A single bad texture stands out immediately in-game. Take the time to verify flatness, seamlessness, and correct sizing before integration.

**The texture is not done until**:
- It's flat (no 3D perspective)
- It tiles seamlessly (verified with 2x2 preview)
- It's exactly 64x64 pixels
- It's integrated into the atlas
- The atlas is regenerated and verified

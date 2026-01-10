---
name: voxel-texture
description: >
  Generate seamless tileable block textures for voxel games using nanobanana MCP.
  Use when: creating game textures, generating block assets, building texture atlases.
  Triggers: "generate texture", "create block texture", "voxel texture", "/voxel-texture"
---

# Voxel Texture Generator

Generate professional seamless tileable block textures for voxel games using nanobanana MCP and ImageMagick processing.

## CRITICAL REQUIREMENT

**⚠️ This skill REQUIRES the nanobanana MCP server.**

If nanobanana MCP is unavailable:
1. Check if the server is running
2. Inform the user the texture cannot be generated without nanobanana
3. Do not proceed with texture integration until generation succeeds

## Philosophy: Flat Patterns, Not 3D Objects

**The number one mistake**: AI image generators often create 3D cube renders instead of flat tileable patterns.

Core principles to remember:
- We need FLAT 2D texture patterns, not 3D objects
- Top-down orthographic view, like looking straight down at a surface
- No perspective, no cube faces, no isometric angles
- Seamless tiling on all four edges is non-negotiable
- Texture resolution standard is 64x64 pixels

## Workflow

When invoked with a block type (e.g., "/voxel-texture dirt"):

### Step 1: Generate with Nanobanana MCP

Call `mcp__nanobanana__generate_image` with these parameters:

```python
mcp__nanobanana__generate_image(
    prompt="""Create a flat, seamless tileable {block_type} texture pattern for a voxel block game (top-down orthographic view, NOT a 3D cube).

The texture should be:
- A FLAT 2D pattern that tiles seamlessly on all edges
- {material_color_description}
- {material_pattern_description}
- Sharp, clean pixel art style suitable for 64x64 resolution
- Should look like looking straight down at a flat {material} surface, not a 3D object
- No perspective, no cube faces, no depth illusion

Style: Minecraft-style pixel art texture, flat top-down orthographic view, seamless edges for perfect tiling. This is a TEXTURE PATTERN, not a 3D rendered object.""",
    aspect_ratio="1:1",
    resolution="high",
    model_tier="pro",
    negative_prompt="3D render, cube, isometric, perspective, shadows, lighting effects, depth, dimensional, rendered object, 3D model, box, block shape, geometric solid",
    output_path="/Users/probello/Repos/voxel_world/textures/{block_type}_64x64.png"
)
```

### Step 2: Process with Script

After generation completes, run the processing script:

```bash
/Users/probello/Repos/voxel_world/.claude/skills/voxel-texture/scripts/process_texture.sh {block_type}
```

The script will:
- Check actual dimensions
- Resize to exact 64x64 if needed (using point filter for crisp pixels)
- Create 2x2 tiled preview to verify seamless edges
- Provide verification checklist

### Step 3: Manual Verification

Examine the generated files:
- `{block_type}_64x64.png` - Check if pattern is flat and clear (not 3D)
- `{block_type}_tiled_preview.png` - Look for visible seams at edges

Red flags requiring regeneration:
- 3D cube appearance or perspective
- Visible seams where tiles meet
- Blurry edges (should be crisp pixel art)

### Step 4: Integrate into Atlas

After verification passes:
1. Add texture to atlas in correct position (see CLAUDE.md for atlas order)
2. Regenerate texture atlas
3. Run `make sprite-gen` to update UI sprites
4. Test in-game

## Material Descriptions

Use these descriptions when generating textures:

### Dirt
- **Color**: Rich brown (#6B4423 to #8B5A2B) with darker organic matter
- **Pattern**: Small pebbles, bits of organic material, soil grain texture
- **Details**: Earthy clumps, root fragments, natural soil variation

### Stone
- **Color**: Medium gray (#808080 to #A0A0A0) with subtle blue-gray variations
- **Pattern**: Random cracks, small mineral flecks, rough rocky surface texture
- **Details**: Irregular weathering patterns, slight color mottling, hard surface feel

### Grass (top view)
- **Color**: Vibrant green (#4CAF50 to #66BB6A) with yellow-green highlights
- **Pattern**: Individual grass blades visible, overlapping leaf texture
- **Details**: Varied blade directions, slight color variation for depth, organic randomness

### Ice
- **Color**: Light blue-white (#B3D9E6 to #E0F2F7), semi-transparent appearance
- **Pattern**: Crystalline frost patterns, internal fracture lines, frozen striations
- **Details**: Air bubbles frozen inside, crystal formations, subtle brightness variation

### Sand
- **Color**: Light tan/beige (#EDC9AF to #F5DEB3) with warm golden tones
- **Pattern**: Fine grain texture, tiny pebbles scattered, desert sand feel
- **Details**: Subtle wind-swept patterns, slight color speckling, granular appearance

### Wood/Planks
- **Color**: Warm brown oak (#8B6F47 to #A0826D) with natural variation
- **Pattern**: Wood grain lines, knots, plank texture with subtle splits
- **Details**: Growth rings visible, grain direction, natural wood character

### Leaves
- **Color**: Forest green (#2E7D32 to #4CAF50) with varied leaf tones
- **Pattern**: Dense overlapping foliage, individual leaf shapes suggested
- **Details**: Light filtering effect, leaf veins, organic clustering

### Water
- **Color**: Translucent blue (#2196F3 to #64B5F6) with depth variation
- **Pattern**: Subtle ripple texture, gentle wave undulation
- **Details**: Light refraction suggestion, movement feel, liquid surface

### Glass
- **Color**: Very light blue tint (#E3F2FD to #F0F8FF), mostly clear
- **Pattern**: Subtle reflections, very faint surface texture
- **Details**: Transparency suggestion, slight highlights, clean manufactured feel

### Gravel
- **Color**: Gray-brown mix (#708090 to #8B7D6B) with stone variation
- **Pattern**: Small rocks and pebbles scattered, varied sizes
- **Details**: Individual stones visible, gaps between pebbles, rough texture

## Anti-Patterns to Avoid

### Accepting 3D Cube Renders
**Why it's bad**: Textures with cube perspective don't tile and look wrong on flat surfaces
**How to avoid**: Use strong negative prompts: `3D render, cube, isometric, perspective`. Explicitly state "flat top-down view, NOT a 3D cube" in prompt. Regenerate if output shows any 3D characteristics.

### Skipping Dimension Verification
**Why it's bad**: AI often generates 1408x768 or other non-square dimensions
**How to avoid**: Always run the processing script which checks and fixes dimensions. Use `-filter point -resize 64x64!` to force exact size.

### Using Wrong Resize Filter
**Why it's bad**: Default bilinear filter blurs pixel art, ruins crisp edges
**How to avoid**: The processing script always uses `-filter point` for nearest-neighbor sampling.

### Forgetting the Exclamation Mark
**Why it's bad**: ImageMagick may produce 64x63 or 63x64 without exact flag
**How to avoid**: The processing script always uses `64x64!` (with exclamation) to force exact dimensions.

### Skipping Tiled Preview
**Why it's bad**: Seams only visible when tiles repeat, single tile looks fine
**How to avoid**: The processing script automatically creates 2x2 tiled preview for verification.

### Using Generic Prompts
**Why it's bad**: "ice texture" produces wildly inconsistent results
**How to avoid**: Use full structured prompt with color, pattern, and detail descriptions from the material descriptions above.

### Proceeding Without MCP
**Why it's bad**: Wastes time, produces unusable results, requires regeneration
**How to avoid**: Check MCP availability FIRST at the beginning of this skill, stop if unavailable.

## Troubleshooting

### Problem: Generated image is a 3D cube render

Solution:
1. Add stronger negative prompts emphasizing "NOT a 3D cube"
2. Emphasize "flat 2D pattern" and "top-down view" multiple times in prompt
3. Use phrase "texture pattern, not a 3D rendered object" explicitly
4. Regenerate with `model_tier="pro"` for better prompt following

### Problem: Texture has visible seams when tiled

Solution:
1. This is an AI generation failure - the texture wasn't truly seamless
2. Regenerate with emphasis on "tiles seamlessly on all edges"
3. The processing script creates tiled preview to verify seamlessness
4. Show tiled preview to user and request regeneration if seams visible

### Problem: Texture looks blurry after processing

Solution:
- The processing script uses `-filter point` which prevents blurring
- If texture is still blurry, the AI generated a blurry image - regenerate

### Problem: Atlas generation fails

Solution:
1. Run `identify *_64x64.png` in textures directory to verify ALL textures are exactly 64x64
2. Check for non-64x64 files: `identify *_64x64.png | grep -v "64x64"`
3. Fix any wrong-sized textures by running process_texture.sh on them
4. Verify atlas is `(N*64)x64` dimensions where N = texture count

## Output Locations

**Texture directory**: `/Users/probello/Repos/voxel_world/textures/`

**File naming**:
- `{block_type}_64x64.png` - Final game-ready 64x64 texture (REQUIRED)
- `{block_type}_tiled_preview.png` - 2x2 tiling verification (RECOMMENDED)
- `texture_atlas.png` - Complete atlas with all textures horizontally concatenated

## Quality Gates

Before proceeding to atlas integration, verify:
1. ✅ MCP was available and generation succeeded
2. ✅ Generated image is FLAT, not 3D
3. ✅ Dimensions are exactly 64x64 pixels
4. ✅ Tiled preview shows NO seams
5. ✅ Texture captures material essence
6. ✅ Pixel art is crisp (not blurry)

**If any gate fails**: Stop, fix the issue, and re-verify before proceeding.

The texture is not done until:
- It's flat (no 3D perspective)
- It tiles seamlessly (verified with 2x2 preview)
- It's exactly 64x64 pixels
- It's integrated into the atlas
- The atlas is regenerated and verified

## Integration with Project

After texture generation and verification:
1. Update `CLAUDE.md` with new block type and atlas position
2. Add block to `BlockType` enum in `src/chunk.rs`
3. Update shader constants in `shaders/common.glsl`
4. Add to palette in `src/ui/palette.rs`
5. Run `make sprite-gen` to generate UI sprite
6. Add to `BLOCK_FILES` in `src/gpu_resources.rs`
7. Run `make checkall` to verify compilation
8. Test in-game

See `CLAUDE.md` for complete block addition workflow.

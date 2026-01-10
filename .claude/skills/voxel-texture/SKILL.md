---
name: voxel-texture
description: >
  Generate seamless tileable block textures for voxel games using nanobanana MCP.
  Use when: creating game textures, generating block assets, building texture atlases.
  Triggers: "generate texture", "create block texture", "voxel texture", "/voxel-texture"
---

# Voxel Texture Generator

Generate 64x64 seamless tileable block textures using nanobanana MCP.

## CRITICAL REQUIREMENT

This skill REQUIRES nanobanana MCP. If unavailable, stop immediately and inform the user.

## Workflow

When invoked with a block type (e.g., "/voxel-texture dirt"):

### 1. Generate with Nanobanana MCP

Use mcp__nanobanana__generate_image with:
- Square aspect ratio (1:1)
- High resolution
- Pro model tier for better quality
- Strong negative prompts to prevent 3D renders

Prompt template:
```
Create a flat, seamless tileable {block_type} texture pattern for a voxel block game (top-down orthographic view, NOT a 3D cube).

The texture should be a FLAT 2D pattern that tiles seamlessly on all edges.
{color_description}
{pattern_description}
Sharp, clean pixel art style suitable for 64x64 resolution.
This is a TEXTURE PATTERN, not a 3D rendered object.
```

Negative prompt:
```
3D render, cube, isometric, perspective, shadows, lighting effects, depth, dimensional, rendered object, 3D model, box, block shape, geometric solid
```

Output path: `/Users/probello/Repos/voxel_world/textures/{block_type}_64x64.png`

### 2. Process with ImageMagick

```bash
cd /Users/probello/Repos/voxel_world/textures

# Check dimensions
magick identify {block_type}_64x64.png

# Resize to exact 64x64 if needed
magick {block_type}_64x64.png -filter point -resize 64x64! {block_type}_64x64.png

# Create tiled preview (2x2) to verify seamless edges
magick {block_type}_64x64.png \( +clone \) +append \( +clone \) -append {block_type}_tiled_preview.png
```

### 3. Verify Quality

Check both files:
- {block_type}_64x64.png should be exactly 64x64 and look flat (not 3D)
- {block_type}_tiled_preview.png should show NO visible seams

If seams visible or looks 3D, regenerate with stronger prompts.

### 4. Update Texture Atlas

After verification, regenerate the atlas with the new texture in the correct position.
See CLAUDE.md for current atlas order and integration instructions.

## Material Descriptions

Use these for common block types:

Dirt: Rich brown (#6B4423 to #8B5A2B), small pebbles, bits of organic material, soil grain texture
Stone: Medium gray (#808080 to #A0A0A0), random cracks, small mineral flecks, rough rocky surface
Grass: Vibrant green (#4CAF50 to #66BB6A), individual grass blades visible, overlapping leaf texture
Ice: Light blue-white (#B3D9E6 to #E0F2F7), crystalline frost patterns, air bubbles, frozen striations
Sand: Light tan/beige (#EDC9AF to #F5DEB3), fine grain texture, tiny pebbles, subtle wind patterns

## Anti-Patterns

DO NOT accept 3D cube renders - if output looks 3D, regenerate immediately
DO NOT skip dimension verification - always check and resize if needed
DO NOT use default resize filter - always use `-filter point` for pixel art
DO NOT skip the exclamation mark in `64x64!` - ImageMagick needs it for exact size

## References

For detailed documentation and troubleshooting, see the full guide in the voxel-texture directory.

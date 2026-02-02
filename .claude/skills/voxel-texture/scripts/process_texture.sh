#!/bin/bash
# Process a voxel block texture after AI generation
# Ensures exact 64x64 dimensions and creates tiled preview
#
# Usage: process_texture.sh <block_type>
#
# Arguments:
#   block_type - Name of the block (e.g., "dirt", "stone", "ice")
#
# Examples:
#   process_texture.sh dirt
#   process_texture.sh stone

set -e

BLOCK_TYPE="${1:?Usage: process_texture.sh <block_type>}"
TEXTURES_DIR="/Users/probello/Repos/voxel-world/textures"
INPUT="${TEXTURES_DIR}/${BLOCK_TYPE}_64x64.png"
PREVIEW="${TEXTURES_DIR}/${BLOCK_TYPE}_tiled_preview.png"

# Check if input exists
if [[ ! -f "$INPUT" ]]; then
    echo "Error: Input file '$INPUT' not found"
    echo "Generate the texture with nanobanana MCP first"
    exit 1
fi

# Check if ImageMagick is available
if ! command -v magick &> /dev/null; then
    echo "Error: ImageMagick 'magick' command not found"
    echo "Install with: brew install imagemagick"
    exit 1
fi

echo "Processing texture: $BLOCK_TYPE"
echo ""

# Get current dimensions
CURRENT_DIMS=$(magick "$INPUT" -format "%wx%h" info:)
echo "Current dimensions: $CURRENT_DIMS"

# Resize to exact 64x64 if needed
if [[ "$CURRENT_DIMS" != "64x64" ]]; then
    echo "Resizing to 64x64 with point filter..."
    magick "$INPUT" -filter point -resize 64x64! "$INPUT"

    # Verify resize worked
    NEW_DIMS=$(magick "$INPUT" -format "%wx%h" info:)
    if [[ "$NEW_DIMS" == "64x64" ]]; then
        echo "✓ Resized successfully to 64x64"
    else
        echo "✗ Resize failed: got $NEW_DIMS instead of 64x64"
        exit 1
    fi
else
    echo "✓ Already 64x64, no resize needed"
fi

# Create 2x2 tiled preview to check for seams
echo ""
echo "Creating tiled preview (2x2)..."
magick "$INPUT" \( +clone \) +append \( +clone \) -append "$PREVIEW"
echo "✓ Created: $PREVIEW"

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "VERIFICATION CHECKLIST:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "1. Open: $INPUT"
echo "   → Should be FLAT 2D pattern (not a 3D cube)"
echo "   → Should be 64x64 pixels"
echo ""
echo "2. Open: $PREVIEW"
echo "   → Check for visible seams at tile edges"
echo "   → Pattern should repeat seamlessly"
echo ""
echo "If texture has issues:"
echo "  - 3D cube appearance → Regenerate with stronger prompts"
echo "  - Visible seams → Regenerate emphasizing seamless edges"
echo "  - Blurry → Already fixed with point filter"
echo ""
echo "If texture looks good:"
echo "  - Regenerate texture atlas (see CLAUDE.md)"
echo "  - Update sprite with: make sprite-gen"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

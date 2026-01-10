#!/bin/bash
# Generate voxel block texture using nanobanana MCP
# Usage: generate_texture.sh <block_type>

set -e

BLOCK_TYPE="${1:?Usage: generate_texture.sh <block_type>}"
TEXTURES_DIR="/Users/probello/Repos/voxel_world/textures"
OUTPUT_FILE="${TEXTURES_DIR}/${BLOCK_TYPE}_64x64.png"

echo "Generating ${BLOCK_TYPE} texture..."
echo "This script requires Claude to invoke nanobanana MCP directly"
echo "Output will be: ${OUTPUT_FILE}"
echo ""
echo "After generation, run these commands:"
echo ""
echo "cd ${TEXTURES_DIR}"
echo "magick identify ${BLOCK_TYPE}_64x64.png"
echo "magick ${BLOCK_TYPE}_64x64.png -filter point -resize 64x64! ${BLOCK_TYPE}_64x64.png"
echo "magick ${BLOCK_TYPE}_64x64.png \\( +clone \\) +append \\( +clone \\) -append ${BLOCK_TYPE}_tiled_preview.png"
echo ""
echo "Then rebuild the atlas (see CLAUDE.md for atlas order)"

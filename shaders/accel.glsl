// Coordinate, bounds, and occupancy helpers

// Check if a chunk at given texture-space chunk position is empty
bool isChunkEmpty(ivec3 chunkPos) {
    if (chunkPos.x < 0 || chunkPos.x >= int(CHUNKS_X) ||
        chunkPos.y < 0 || chunkPos.y >= int(CHUNKS_Y) ||
        chunkPos.z < 0 || chunkPos.z >= int(CHUNKS_Z)) {
        return true;
    }

    uint idx = uint(chunkPos.x) + uint(chunkPos.z) * CHUNKS_X + uint(chunkPos.y) * CHUNKS_X * CHUNKS_Z;
    uint wordIdx = idx / 32u;
    uint bitIdx = idx % 32u;
    return (chunk_flags[wordIdx] & (1u << bitIdx)) != 0u;
}

// Check if a brick at given voxel position is empty
bool isBrickEmpty(ivec3 coord) {
    ivec3 chunkPos = coord / int(CHUNK_SIZE);
    if (chunkPos.x < 0 || chunkPos.x >= int(CHUNKS_X) ||
        chunkPos.y < 0 || chunkPos.y >= int(CHUNKS_Y) ||
        chunkPos.z < 0 || chunkPos.z >= int(CHUNKS_Z)) {
        return true;
    }

    ivec3 localVoxel = coord - chunkPos * int(CHUNK_SIZE);
    ivec3 brickPos = localVoxel / int(BRICK_SIZE);
    uint brickIdx = uint(brickPos.x) + uint(brickPos.y) * BRICKS_PER_AXIS
                  + uint(brickPos.z) * BRICKS_PER_AXIS * BRICKS_PER_AXIS;

    uint chunkIdx = uint(chunkPos.x) + uint(chunkPos.z) * CHUNKS_X
                  + uint(chunkPos.y) * CHUNKS_X * CHUNKS_Z;

    uint maskOffset = chunkIdx * 2u;
    uint wordIdx = brickIdx / 32u;
    uint bitIdx = brickIdx % 32u;
    uint mask = brick_masks[maskOffset + wordIdx];

    return (mask & (1u << bitIdx)) == 0u;
}

// Get the world position of the brick containing this voxel
ivec3 getBrickWorldPos(ivec3 voxelCoord) {
    return (voxelCoord / int(BRICK_SIZE)) * int(BRICK_SIZE);
}

// Texture/world sizing helpers
vec3 textureSize3D() { return vec3(pc.texture_size_x, pc.texture_size_y, pc.texture_size_z); }
uvec3 textureSizeU() { return uvec3(pc.texture_size_x, pc.texture_size_y, pc.texture_size_z); }
ivec3 textureOrigin() { return ivec3(pc.texture_origin_x, pc.texture_origin_y, pc.texture_origin_z); }
ivec3 worldToTexture(ivec3 worldCoord) { return worldCoord - textureOrigin(); }
bool isInTextureBounds(ivec3 texCoord) {
    return texCoord.x >= 0 && texCoord.x < int(pc.texture_size_x) &&
           texCoord.y >= 0 && texCoord.y < int(pc.texture_size_y) &&
           texCoord.z >= 0 && texCoord.z < int(pc.texture_size_z);
}
vec3 worldSize() { return textureSize3D(); }
uvec3 worldSizeU() { return textureSizeU(); }

// Block readers
uint readBlockTypeAtTexCoord(ivec3 texCoord) {
    if (!isInTextureBounds(texCoord)) {
        return BLOCK_AIR;
    }
    return imageLoad(blockImage, texCoord).r;
}
uint readBlockType(ivec3 worldCoord) {
    return readBlockTypeAtTexCoord(worldToTexture(worldCoord));
}
bool isSolid(ivec3 worldCoord) { return readBlockType(worldCoord) != BLOCK_AIR; }
bool isSolidAtTexCoord(ivec3 texCoord) { return readBlockTypeAtTexCoord(texCoord) != BLOCK_AIR; }

// Light occluder helpers (exclude transparent blocks)
bool isOccluderAtTexCoord(ivec3 texCoord) {
    uint b = readBlockTypeAtTexCoord(texCoord);
    return b != BLOCK_AIR && b != BLOCK_WATER && b != BLOCK_GLASS && b != BLOCK_MODEL;
}
bool isSolidSafe(ivec3 texCoord) {
    if (any(lessThan(texCoord, ivec3(0))) || any(greaterThanEqual(texCoord, ivec3(worldSizeU())))) {
        return false;
    }
    return isSolidAtTexCoord(texCoord);
}
bool isOccluderSafe(ivec3 texCoord) {
    if (any(lessThan(texCoord, ivec3(0))) || any(greaterThanEqual(texCoord, ivec3(worldSizeU())))) {
        return false;
    }
    return isOccluderAtTexCoord(texCoord);
}

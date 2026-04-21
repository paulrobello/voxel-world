// Small utility helpers shared across traversal

const float DIR_EPS = 1e-4;

// Prevent NaN/INF when a ray component is ~0
vec3 makeSafeDir(vec3 dir) {
    vec3 safeDir = dir;
    safeDir.x = (abs(safeDir.x) < DIR_EPS) ? (safeDir.x >= 0.0 ? DIR_EPS : -DIR_EPS) : safeDir.x;
    safeDir.y = (abs(safeDir.y) < DIR_EPS) ? (safeDir.y >= 0.0 ? DIR_EPS : -DIR_EPS) : safeDir.y;
    safeDir.z = (abs(safeDir.z) < DIR_EPS) ? (safeDir.z >= 0.0 ? DIR_EPS : -DIR_EPS) : safeDir.z;
    return safeDir;
}

// Generic axis-aligned box hit test
bool rayBoxHit(vec3 rayOrigin, vec3 rayDir, vec3 boxMin, vec3 boxMax, out float tHit, out vec3 hitNormal) {
    vec3 safeDir = makeSafeDir(rayDir);
    vec3 invDir = 1.0 / safeDir;

    vec3 t1 = (boxMin - rayOrigin) * invDir;
    vec3 t2 = (boxMax - rayOrigin) * invDir;

    vec3 tmin = min(t1, t2);
    vec3 tmax = max(t1, t2);

    float tNear = max(max(tmin.x, tmin.y), tmin.z);
    float tFar = min(min(tmax.x, tmax.y), tmax.z);

    if (tNear > tFar || tFar < 0.0) {
        return false;
    }

    tHit = tNear > 0.0 ? tNear : tFar;

    hitNormal = vec3(0.0);
    if (tmin.x >= tmin.y && tmin.x >= tmin.z) {
        hitNormal.x = -sign(rayDir.x);
    } else if (tmin.y >= tmin.z) {
        hitNormal.y = -sign(rayDir.y);
    } else {
        hitNormal.z = -sign(rayDir.z);
    }

    return true;
}

// Inferno palette for step debugging
vec3 inferno(float t) {
    const vec3 c0 = vec3(0.0002189403691192265, 0.001651004631001012, -0.01948089843709184);
    const vec3 c1 = vec3(0.1065134194856116, 0.5639564367884091, 3.932712388889277);
    const vec3 c2 = vec3(11.60249308247187, -3.972853965665698, -15.9423941062914);
    const vec3 c3 = vec3(-41.70399613139459, 17.43639888205313, 44.35414519872813);
    const vec3 c4 = vec3(77.162935699427, -33.40235894210092, -81.80730925738993);
    const vec3 c5 = vec3(-71.31942824499214, 32.62606426397723, 73.20951985803202);
    const vec3 c6 = vec3(25.13112622477341, -12.24266895238567, -23.07032500287172);

    return c0+t*(c1+t*(c2+t*(c3+t*(c4+t*(c5+t*c6)))));
}
vec3 stepsToInferno(uint steps, uint start, uint end) {
    float t = float(int(steps - start)) / (end - start);
    return inferno(clamp(t, 0, 1));
}

// AABB helpers
float rayAABBIntersect(vec3 rayOrigin, vec3 rayDir, vec3 boxCenter, float boxHalfSize, out vec3 hitNormal) {
    float tHit;
    if (!rayBoxHit(rayOrigin, rayDir, boxCenter - vec3(boxHalfSize), boxCenter + vec3(boxHalfSize), tHit, hitNormal)) {
        return -1.0;
    }
    return tHit;
}

float rayBlockIntersect(vec3 rayOrigin, vec3 rayDir, ivec3 blockPos, out vec3 hitNormal, out vec3 localHit) {
    float tHit;
    if (!rayBoxHit(rayOrigin, rayDir, vec3(blockPos), vec3(blockPos) + vec3(1.0), tHit, hitNormal)) {
        return -1.0;
    }
    vec3 hitPoint = rayOrigin + rayDir * tHit;
    localHit = clamp(hitPoint - vec3(blockPos), vec3(0.0), vec3(1.0));
    return tHit;
}

float raySubVoxelIntersect(vec3 origin, vec3 dir, vec3 boxMin, vec3 boxMax, out vec3 hitNormal, out vec3 localHit) {
    float tHit;
    if (!rayBoxHit(origin, dir, boxMin, boxMax, tHit, hitNormal)) {
        return -1.0;
    }
    vec3 hitPoint = origin + dir * tHit;
    localHit = (hitPoint - boxMin) / (boxMax - boxMin);
    return tHit;
}

// Check if a texture coordinate is within the cutaway chunks (debug feature)
// Returns true if the coordinate should be treated as transparent
// Hides both the player's current chunk AND the chunk in front of them
bool isInCutawayChunk(ivec3 texCoord) {
    if (pc.cutaway_enabled == 0u) {
        return false;
    }
    // Get the chunk position (in texture space, blocks divided by CHUNK_SIZE)
    ivec3 chunkPos = texCoord / int(CHUNK_SIZE);

    // Check if in the chunk ahead of player (facing direction)
    ivec3 cutawayChunkStart = ivec3(pc.cutaway_chunk_x, 0, pc.cutaway_chunk_z);
    ivec3 cutawayChunkPos = cutawayChunkStart / int(CHUNK_SIZE);
    bool inFrontChunk = (chunkPos.x == cutawayChunkPos.x && chunkPos.z == cutawayChunkPos.z);

    // Check if in player's current chunk
    ivec3 playerChunkStart = ivec3(pc.cutaway_player_chunk_x, 0, pc.cutaway_player_chunk_z);
    ivec3 playerChunkPos = playerChunkStart / int(CHUNK_SIZE);
    bool inPlayerChunk = (chunkPos.x == playerChunkPos.x && chunkPos.z == playerChunkPos.z);

    // Hide both chunks (all Y levels)
    return inFrontChunk || inPlayerChunk;
}

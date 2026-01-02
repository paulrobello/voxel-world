uint sampleModelVoxel(uint model_id, ivec3 local_pos) {
    // Model atlas layout: 16 models per row, 16 rows
    // Each model is 8×8×8, so atlas is 128×8×128
    uint model_x = model_id % 16u;
    uint model_z = model_id / 16u;

    ivec3 atlas_pos = ivec3(
        int(model_x * SUB_VOXEL_SIZE) + local_pos.x,
        local_pos.y,
        int(model_z * SUB_VOXEL_SIZE) + local_pos.z
    );

    return imageLoad(modelAtlas, atlas_pos).r;
}

// Get model color from palette
// model_id: 0-255
// palette_idx: 0-15 (palette index within the model)
vec4 getModelPaletteColor(uint model_id, uint palette_idx) {
    // Palette texture is 256×16 (model_id × palette_idx)
    vec2 uv = vec2(
        (float(model_id) + 0.5) / 256.0,
        (float(palette_idx) + 0.5) / 16.0
    );
    return texture(modelPalettes, uv);
}

// Rotate a position within the model based on rotation value (0-3 = 0°/90°/180°/270°)
ivec3 rotateModelPos(ivec3 pos, uint rotation) {
    int cx = int(SUB_VOXEL_SIZE) / 2;  // Center = 4
    int px = pos.x - cx;
    int pz = pos.z - cx;

    switch (rotation) {
        case 1u:  // 90° clockwise
            return ivec3(cx - pz - 1, pos.y, cx + px);
        case 2u:  // 180°
            return ivec3(cx - px - 1, pos.y, cx - pz - 1);
        case 3u:  // 270° clockwise (90° counter-clockwise)
            return ivec3(cx + pz, pos.y, cx - px - 1);
        default:  // 0° (no rotation)
            return pos;
    }
}

// Sample a model voxel with rotation and bounds checks.
// Returns true if the rotated position is inside the model and non-empty.
bool sampleModelFilled(uint model_id, ivec3 local_pos, uint rotation) {
    if (any(lessThan(local_pos, ivec3(0))) || any(greaterThanEqual(local_pos, ivec3(int(SUB_VOXEL_SIZE))))) {
        return false;
    }
    ivec3 rotated = rotateModelPos(local_pos, rotation);
    rotated = clamp(rotated, ivec3(0), ivec3(int(SUB_VOXEL_SIZE) - 1));
    return sampleModelVoxel(model_id, rotated) != 0u;
}

// Sample with awareness of fence connections across block boundaries.
// Returns 1.0 for a solid voxel inside this model, 0.0 otherwise.
float sampleModelOcclusion(uint model_id, ivec3 local_pos, uint rotation) {
    bool inside = all(greaterThanEqual(local_pos, ivec3(0))) &&
                  all(lessThan(local_pos, ivec3(int(SUB_VOXEL_SIZE))));
    if (inside) {
        return sampleModelFilled(model_id, local_pos, rotation) ? 1.0 : 0.0;
    }
    return 0.0;
}

// Compute fine voxel bounds for a model (in block-local 0-1 space)
void modelCollisionBounds(uint model_id, out vec3 minB, out vec3 maxB) {
    ModelProperties props = model_properties[model_id];
    uvec3 minU = uvec3(props.aabb_min & 0xFF, (props.aabb_min >> 8) & 0xFF, (props.aabb_min >> 16) & 0xFF);
    uvec3 maxU = uvec3(props.aabb_max & 0xFF, (props.aabb_max >> 8) & 0xFF, (props.aabb_max >> 16) & 0xFF);
    minB = vec3(minU) / 8.0;
    maxB = vec3(maxU) / 8.0;
}

// Quick coarse 4x4x4 mask ray test (block-local 0-1 space). Returns true on hit.
bool modelMaskBlocksRay(vec3 origin, vec3 dir, uint model_id) {
    ModelProperties props = model_properties[model_id];
    uint hi = props.collision_mask.y;
    uint lo = props.collision_mask.x;

    // Transform to 4x4x4 grid
    vec3 pos = origin * 4.0;
    vec3 safeDir = dir;
    const float DIR_EPS = 1e-4;
    safeDir.x = (abs(safeDir.x) < DIR_EPS) ? (safeDir.x >= 0.0 ? DIR_EPS : -DIR_EPS) : safeDir.x;
    safeDir.y = (abs(safeDir.y) < DIR_EPS) ? (safeDir.y >= 0.0 ? DIR_EPS : -DIR_EPS) : safeDir.y;
    safeDir.z = (abs(safeDir.z) < DIR_EPS) ? (safeDir.z >= 0.0 ? DIR_EPS : -DIR_EPS) : safeDir.z;
    vec3 invDir = 1.0 / safeDir;

    ivec3 voxel = ivec3(clamp(floor(pos), vec3(0.0), vec3(3.0)));
    ivec3 step = ivec3(sign(safeDir));
    vec3 tDelta = abs(invDir);
    vec3 tMax;
    tMax.x = (step.x > 0) ? (float(voxel.x + 1) - pos.x) * invDir.x
                          : (step.x < 0) ? (pos.x - float(voxel.x)) * (-invDir.x)
                                         : 1e30;
    tMax.y = (step.y > 0) ? (float(voxel.y + 1) - pos.y) * invDir.y
                          : (step.y < 0) ? (pos.y - float(voxel.y)) * (-invDir.y)
                                         : 1e30;
    tMax.z = (step.z > 0) ? (float(voxel.z + 1) - pos.z) * invDir.z
                          : (step.z < 0) ? (pos.z - float(voxel.z)) * (-invDir.z)
                                         : 1e30;

    for (int i = 0; i < 16; i++) {
        if (any(lessThan(voxel, ivec3(0))) || any(greaterThan(voxel, ivec3(3)))) break;
        int idx = voxel.x + voxel.y * 4 + voxel.z * 16;
        bool bit = (idx < 32) ? ((lo & (1u << idx)) != 0u) : ((hi & (1u << (idx - 32))) != 0u);
        if (bit) return true;

        if (tMax.x < tMax.y) {
            if (tMax.x < tMax.z) {
                voxel.x += step.x;
                tMax.x += tDelta.x;
            } else {
                voxel.z += step.z;
                tMax.z += tDelta.z;
            }
        } else {
            if (tMax.y < tMax.z) {
                voxel.y += step.y;
                tMax.y += tDelta.y;
            } else {
                voxel.z += step.z;
                tMax.z += tDelta.z;
            }
        }
    }
    return false;
}

// Inverse-rotate a normal to match model rotation
// Since positions are rotated CW, normals need to be rotated CCW (inverse) to get world-space normal
vec3 inverseRotateNormal(vec3 n, uint rotation) {
    switch (rotation) {
        case 1u:  // Position was 90° CW, so normal needs 90° CCW
            return vec3(n.z, n.y, -n.x);
        case 2u:  // 180° inverse is 180°
            return vec3(-n.x, n.y, -n.z);
        case 3u:  // Position was 270° CW, so normal needs 270° CCW (= 90° CW)
            return vec3(-n.z, n.y, n.x);
        default:  // 0° (no rotation)
            return n;
    }
}

// Inverse-rotate a block-local position (0-1 range) back to model base orientation
vec3 inverseRotatePosition(vec3 p, uint rotation) {
    switch (rotation) {
        case 1u: // 90° CW -> rotate 90° CCW around center
            return vec3(1.0 - p.z, p.y, p.x);
        case 2u: // 180°
            return vec3(1.0 - p.x, p.y, 1.0 - p.z);
        case 3u: // 270° CW -> rotate 90° CW
            return vec3(p.z, p.y, 1.0 - p.x);
        default:
            return p;
    }
}

// Inverse-rotate a direction vector back to model base orientation
vec3 inverseRotateDirection(vec3 d, uint rotation) {
    switch (rotation) {
        case 1u:
            return vec3(d.z, d.y, -d.x);
        case 2u:
            return vec3(-d.x, d.y, -d.z);
        case 3u:
            return vec3(-d.z, d.y, d.x);
        default:
            return d;
    }
}

// Sub-voxel AO removed (no ambient occlusion applied to sub-voxel models).

// Forward declaration for block ray intersection (used before definition in shadow/sky)
float rayBlockIntersect(vec3 rayOrigin, vec3 rayDir, ivec3 blockPos, out vec3 hitNormal, out vec3 localHit);
// Forward declaration for sub-voxel hit test (used for model shadows/sky)
bool findSubVoxelHit(vec3 origin, vec3 dir, uint model_id, uint rotation,
                     out ivec3 hitSubVoxel, out vec3 hitNormal, out float hitT);

// March through sub-voxel model using DDA
// Returns true if hit, false if ray passed through
// origin: ray origin in block-local coordinates (0-1)
// dir: normalized ray direction
// model_id: model to sample from
// rotation: 0-3 for Y-axis rotation
// out_color: hit color from palette
// out_normal: hit surface normal
// out_t: distance to hit within block
bool marchSubVoxelModel(
    vec3 origin,
    vec3 dir,
    uint model_id,
    uint rotation,
    out vec3 out_color,
    out vec3 out_normal,
    out float out_t
) {
    // Scale to sub-voxel coordinates (0-8)
    vec3 pos = origin * float(SUB_VOXEL_SIZE);

    // Avoid infinities / NaNs for near-zero components
    vec3 safeDir = dir;
    const float DIR_EPS = 1e-4;
    safeDir.x = (abs(safeDir.x) < DIR_EPS) ? (safeDir.x >= 0.0 ? DIR_EPS : -DIR_EPS) : safeDir.x;
    safeDir.y = (abs(safeDir.y) < DIR_EPS) ? (safeDir.y >= 0.0 ? DIR_EPS : -DIR_EPS) : safeDir.y;
    safeDir.z = (abs(safeDir.z) < DIR_EPS) ? (safeDir.z >= 0.0 ? DIR_EPS : -DIR_EPS) : safeDir.z;
    vec3 invDir = 1.0 / safeDir;

    // Calculate entry t (may need to enter the 0-8 box from outside)
    vec3 tMin = (vec3(-SUB_VOXEL_EPS) - pos) * invDir;
    vec3 tMax = (vec3(float(SUB_VOXEL_SIZE) + SUB_VOXEL_EPS) - pos) * invDir;

    vec3 t1 = min(tMin, tMax);
    vec3 t2 = max(tMin, tMax);

    float tNear = max(max(t1.x, t1.y), t1.z);
    float tFar = min(min(t2.x, t2.y), t2.z);

    // Miss check
    if (tNear > tFar || tFar < 0.0) {
        return false;
    }

    // Identify which axis we entered through (the one with the largest tMin value)
    uint entryAxis;
    if (t1.x >= t1.y && t1.x >= t1.z) {
        entryAxis = 0u;
    } else if (t1.y >= t1.z) {
        entryAxis = 1u;
    } else {
        entryAxis = 2u;
    }

    // Start from entry point if outside, else from origin
    float startT = max(tNear, 0.0);
    vec3 startPos = pos + dir * startT;
    // Nudge slightly along the ray to land inside first cell and avoid boundary misses
    startPos += safeDir * SUB_VOXEL_EPS;
    startPos = clamp(startPos, vec3(SUB_VOXEL_EPS), vec3(float(SUB_VOXEL_SIZE) - SUB_VOXEL_EPS));

    // Current voxel position
    ivec3 voxel = ivec3(floor(startPos));
    voxel = clamp(voxel, ivec3(0), ivec3(int(SUB_VOXEL_SIZE) - 1));

    // Step direction and initial t values
    ivec3 step = ivec3(sign(safeDir));
    vec3 tDelta = abs(invDir);

    // Calculate initial tMax for each axis
    vec3 tMaxAxis;
    tMaxAxis.x = (step.x > 0) ? (float(voxel.x + 1) - startPos.x) * invDir.x
                              : (step.x < 0) ? (startPos.x - float(voxel.x)) * (-invDir.x)
                                             : 1e30;
    tMaxAxis.y = (step.y > 0) ? (float(voxel.y + 1) - startPos.y) * invDir.y
                              : (step.y < 0) ? (startPos.y - float(voxel.y)) * (-invDir.y)
                                             : 1e30;
    tMaxAxis.z = (step.z > 0) ? (float(voxel.z + 1) - startPos.z) * invDir.z
                              : (step.z < 0) ? (startPos.z - float(voxel.z)) * (-invDir.z)
                                             : 1e30;

    // Track which axis was last crossed (for normal calculation)
    uint stepped_axis = 0u;

    // March through sub-voxels
    for (int i = 0; i < 24; i++) {  // 8*3 max steps
        // Check bounds
        if (any(lessThan(voxel, ivec3(0))) || any(greaterThanEqual(voxel, ivec3(int(SUB_VOXEL_SIZE))))) {
            break;
        }

        // Apply rotation and sample
        ivec3 rotatedPos = rotateModelPos(voxel, rotation);
        rotatedPos = clamp(rotatedPos, ivec3(0), ivec3(int(SUB_VOXEL_SIZE) - 1));
        uint palette_idx = sampleModelVoxel(model_id, rotatedPos);

        // Hit if not air (palette index 0 = transparent)
        if (palette_idx != 0u) {
            // Get color from palette
            vec4 paletteColor = getModelPaletteColor(model_id, palette_idx);
            out_color = paletteColor.rgb;

            // Add emission glow if model has emission (e.g., torch flame)
            ModelProperties props = model_properties[model_id];
            if (props.emission.a > 0.0) {
                out_color += props.emission.rgb * props.emission.a * 0.5;
            }

            // Surface normal: use stepped axis after the first move; first voxel picks entry axis
            uint hitAxis = (i == 0) ? entryAxis : stepped_axis;
            out_normal = vec3(0.0);
            out_normal[hitAxis] = -float(step[hitAxis]);
            // Rotate normal to match model orientation
            out_normal = inverseRotateNormal(out_normal, rotation);

            // Calculate t value (in block-local 0-1 space)
            // Use entry distance to current voxel
            float voxelDist = (i == 0) ? 0.0 : (tMaxAxis[stepped_axis] - tDelta[stepped_axis]);
            float t = startT + voxelDist;
            out_t = t / float(SUB_VOXEL_SIZE);

            return true;
        }

        // Step to next sub-voxel
        if (tMaxAxis.x < tMaxAxis.y) {
            if (tMaxAxis.x < tMaxAxis.z) {
                voxel.x += step.x;
                tMaxAxis.x += tDelta.x;
                stepped_axis = 0u;
            } else {
                voxel.z += step.z;
                tMaxAxis.z += tDelta.z;
                stepped_axis = 2u;
            }
        } else {
            if (tMaxAxis.y < tMaxAxis.z) {
                voxel.y += step.y;
                tMaxAxis.y += tDelta.y;
                stepped_axis = 1u;
            } else {
                voxel.z += step.z;
                tMaxAxis.z += tDelta.z;
                stepped_axis = 2u;
            }
        }
    }

    return false;  // Ray passed through without hitting
}

// Shadow-only sub-voxel march: returns true on any occupancy hit.
// Uses a capped number of steps since we only need to know whether the model blocks light.
bool marchSubVoxelShadow(
    vec3 origin,
    vec3 dir,
    uint model_id,
    uint rotation,
    int maxSteps
) {
    // Clamp step budget to the maximum possible voxels we could traverse in 8^3 grid.
    int stepsLeft = clamp(maxSteps, 1, 24);

    // Scale to sub-voxel coordinates (0-8)
    vec3 pos = origin * float(SUB_VOXEL_SIZE);

    vec3 safeDir = makeSafeDir(dir);
    vec3 invDir = 1.0 / safeDir;

    // Calculate entry t into the 0-8 cube
    vec3 tMin = (vec3(-SUB_VOXEL_EPS) - pos) * invDir;
    vec3 tMax = (vec3(float(SUB_VOXEL_SIZE) + SUB_VOXEL_EPS) - pos) * invDir;

    vec3 t1 = min(tMin, tMax);
    vec3 t2 = max(tMin, tMax);

    float tNear = max(max(t1.x, t1.y), t1.z);
    float tFar = min(min(t2.x, t2.y), t2.z);
    if (tNear > tFar || tFar < 0.0) {
        return false;
    }

    float startT = max(tNear, 0.0);
    vec3 startPos = pos + dir * startT;
    startPos += safeDir * SUB_VOXEL_EPS;
    startPos = clamp(startPos, vec3(SUB_VOXEL_EPS), vec3(float(SUB_VOXEL_SIZE) - SUB_VOXEL_EPS));

    ivec3 voxel = ivec3(clamp(floor(startPos), vec3(0.0), vec3(float(SUB_VOXEL_SIZE - 1))));

    ivec3 step = ivec3(sign(safeDir));
    vec3 tDelta = abs(invDir);

    vec3 tMaxAxis;
    tMaxAxis.x = (step.x > 0) ? (float(voxel.x + 1) - startPos.x) * invDir.x
                              : (step.x < 0) ? (startPos.x - float(voxel.x)) * (-invDir.x)
                                             : 1e30;
    tMaxAxis.y = (step.y > 0) ? (float(voxel.y + 1) - startPos.y) * invDir.y
                              : (step.y < 0) ? (startPos.y - float(voxel.y)) * (-invDir.y)
                                             : 1e30;
    tMaxAxis.z = (step.z > 0) ? (float(voxel.z + 1) - startPos.z) * invDir.z
                              : (step.z < 0) ? (startPos.z - float(voxel.z)) * (-invDir.z)
                                             : 1e30;

    for (int i = 0; i < stepsLeft; i++) {
        if (any(lessThan(voxel, ivec3(0))) || any(greaterThanEqual(voxel, ivec3(int(SUB_VOXEL_SIZE))))) {
            break;
        }

        ivec3 rotatedPos = rotateModelPos(voxel, rotation);
        rotatedPos = clamp(rotatedPos, ivec3(0), ivec3(int(SUB_VOXEL_SIZE) - 1));
        uint palette_idx = sampleModelVoxel(model_id, rotatedPos);
        if (palette_idx != 0u) {
            return true;
        }

        // Step to next sub-voxel
        if (tMaxAxis.x < tMaxAxis.y) {
            if (tMaxAxis.x < tMaxAxis.z) {
                voxel.x += step.x;
                tMaxAxis.x += tDelta.x;
            } else {
                voxel.z += step.z;
                tMaxAxis.z += tDelta.z;
            }
        } else {
            if (tMaxAxis.y < tMaxAxis.z) {
                voxel.y += step.y;
                tMaxAxis.y += tDelta.y;
            } else {
                voxel.z += step.z;
                tMaxAxis.z += tDelta.z;
            }
        }
    }

    return false;
}

// Read model metadata at a texture coordinate
// Returns: model_id in .r, rotation in .g
uvec2 readModelMetadata(ivec3 texCoord) {
    return imageLoad(modelMetadata, texCoord).rg;
}

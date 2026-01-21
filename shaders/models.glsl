// Sample voxel from the model atlas (three tiers: 8³, 16³, 32³).
// Model layout: 16 models per row, 16 rows = 256 models max.
// Automatically selects the correct atlas based on model resolution.
const uint FRAME_MODEL_ID = 160u;

// Custom transform for frames: orient voxel coordinates so the frame sits flush against
// the mounting wall. Frame model has voxels at z=6..7 (2 voxels deep for border), extending toward wall.
// rotation/facing: 0=North(-Z), 1=East(+X), 2=South(+Z), 3=West(-X)
//
// Transform mapping:
// - North: z preserved, z=7 scales to z=0.875 (close to -Z wall)
// - South: z flipped (maxv - z), z=7→0 scales to z=0.0 (at +Z wall)
// - East: z becomes x, z=7→7 at wall, z=6→6 extending in
// - West: z becomes x, z=7→0 at wall, z=6→1 extending in
ivec3 transformFramePos(ivec3 pos, uint rotation, uint res) {
    int maxv = int(res) - 1; // 7 for 8³
    int x = pos.x;
    int y = pos.y;
    int z = pos.z;

    switch (rotation & 3u) {
        case 0u: // North (-Z wall): frame extends into block (+Z)
            return ivec3(x, y, z);
        case 2u: // South (+Z wall): frame extends into block (-Z)
            return ivec3(maxv - x, y, maxv - z);
        case 1u: // East (+X wall): frame extends into block (-X)
            // z becomes x (direct mapping), x becomes z (mirrored)
            return ivec3(z, y, maxv - x);
        case 3u: // West (-X wall): frame extends into block (+X)
            // z becomes x (direct mapping), x becomes z (mirrored)
            return ivec3(maxv - z, y, x);
        default:
            return pos;
    }
}

// Inverse transform for block-local positions (0-1 range) back to model base orientation
vec3 inverseTransformFramePosition(vec3 p, uint rotation, uint res) {
    switch (rotation & 3u) {
        case 0u: // North (-Z): identity
            return p;
        case 2u: // South (+Z): (maxv-x, y, maxv-z) → (1-x, y, 1-z)
            return vec3(1.0 - p.x, p.y, 1.0 - p.z);
        case 1u: // East (+X): (maxv-z-1, y, x) → (z+1 → x, y, x → z)
            // Forward: u = maxv - z - 1, w = x
            // Inverse: given p (block space of u, v, w), find original (x, y, z)
            // p.x = (maxv - z - 1)/res → z/res = 1.0 - p.x
            // p.z = x/res → x/res = p.z
            return vec3(1.0 - p.x, p.y, p.z);
        case 3u: // West (-X): (z+1, y, maxv-x)
            // Forward: u = z + 1, w = maxv - x
            // Inverse: p.x = (z+1)/res → z/res = p.x - 1/res
            //         p.z = (maxv-x)/res → x/res = 1.0 - p.z
            // Note: the -1/res offset is a sub-voxel shift, approximately p.x - 0.125
            return vec3(p.x - 1.0, p.y, 1.0 - p.z);
        default:
            return p;
    }
}

// Inverse transform for directions (no translation component)
vec3 inverseTransformFrameDirection(vec3 d, uint rotation) {
    switch (rotation & 3u) {
        case 0u: // North: identity
            return d;
        case 2u: // South: negate x and z
            return vec3(-d.x, d.y, -d.z);
        case 1u: // East: swap x and z, negate new x
            return vec3(-d.z, d.y, d.x);
        case 3u: // West: swap x and z, negate new z
            return vec3(d.z, d.y, -d.x);
        default:
            return d;
    }
}

uint sampleModelVoxel(uint model_id, ivec3 local_pos) {
    uint model_x = model_id % 16u;
    uint model_z = model_id / 16u;

    // Get model resolution from properties
    uint res = model_properties[model_id].resolution;

    // Calculate atlas position based on resolution
    ivec3 atlas_pos;
    if (res == 8u) {
        atlas_pos = ivec3(
            int(model_x * 8u) + local_pos.x,
            local_pos.y,
            int(model_z * 8u) + local_pos.z
        );
        return imageLoad(modelAtlas8, atlas_pos).r;
    } else if (res == 32u) {
        atlas_pos = ivec3(
            int(model_x * 32u) + local_pos.x,
            local_pos.y,
            int(model_z * 32u) + local_pos.z
        );
        return imageLoad(modelAtlas32, atlas_pos).r;
    } else {
        // Default to 16³
        atlas_pos = ivec3(
            int(model_x * 16u) + local_pos.x,
            local_pos.y,
            int(model_z * 16u) + local_pos.z
        );
        return imageLoad(modelAtlas16, atlas_pos).r;
    }
}

// Get model color from palette
// model_id: 0-255
// palette_idx: 0-31 (palette index within the model, 32 colors per model)
vec4 getModelPaletteColor(uint model_id, uint palette_idx) {
    // Palette texture is 256×32 (model_id × palette_idx)
    vec2 uv = vec2(
        (float(model_id) + 0.5) / 256.0,
        (float(palette_idx) + 0.5) / 32.0
    );
    return texture(modelPalettes, uv);
}

// Get emission value for a palette entry (0-1)
// model_id: 0-255
// palette_idx: 0-31
float getModelPaletteEmission(uint model_id, uint palette_idx) {
    vec2 uv = vec2(
        (float(model_id) + 0.5) / 256.0,
        (float(palette_idx) + 0.5) / 32.0
    );
    return texture(modelPaletteEmission, uv).r;
}

// Get model resolution from properties (8, 16, or 32)
uint getModelResolution(uint model_id) {
    return model_properties[model_id].resolution;
}

// Rotate a position within the model based on rotation value (0-3 = 0°/90°/180°/270°)
// res: model resolution (8, 16, or 32)
ivec3 rotateModelPos(ivec3 pos, uint rotation, uint res) {
    int cx = int(res) / 2;  // Center of model
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

// Inverse-rotate a normal to match model rotation
// Since positions are rotated CW, normals need to be rotated CCW (inverse) to get world-space normal
vec3 inverseRotateNormal(vec3 n, uint rotation) {
    switch (rotation) {
        case 1u:  // Inverse of 90° CW is 90° CCW: (x,z) -> (-z, x)
            return vec3(-n.z, n.y, n.x);
        case 2u:  // 180° inverse is 180°
            return vec3(-n.x, n.y, -n.z);
        case 3u:  // Inverse of 90° CCW is 90° CW: (x,z) -> (z, -x)
            return vec3(n.z, n.y, -n.x);
        default:  // 0° (no rotation)
            return n;
    }
}

// Inverse-rotate a block-local position (0-1 range) back to model base orientation
vec3 inverseRotatePosition(vec3 p, uint rotation) {
    switch (rotation) {
        case 1u: // 90° CW -> rotate 90° CCW: (x,z) -> (1-z, x)
            return vec3(1.0 - p.z, p.y, p.x);
        case 2u: // 180°
            return vec3(1.0 - p.x, p.y, 1.0 - p.z);
        case 3u: // 270° CW -> rotate 90° CW: (x,z) -> (z, 1-x)
            return vec3(p.z, p.y, 1.0 - p.x);
        default:
            return p;
    }
}

// Inverse-rotate a direction vector back to model base orientation
vec3 inverseRotateDirection(vec3 d, uint rotation) {
    switch (rotation) {
        case 1u: // 90° CCW
            return vec3(-d.z, d.y, d.x);
        case 2u:
            return vec3(-d.x, d.y, -d.z);
        case 3u: // 90° CW
            return vec3(d.z, d.y, -d.x);
        default:
            return d;
    }
}

// Sample a model voxel with rotation and bounds checks.
// Returns true if the rotated position is inside the model and non-empty.
// Supports 8³, 16³, and 32³ resolutions.
bool sampleModelFilled(uint model_id, ivec3 local_pos, uint rotation) {
    uint res = model_properties[model_id].resolution;
    if (any(lessThan(local_pos, ivec3(0))) || any(greaterThanEqual(local_pos, ivec3(int(res))))) {
        return false;
    }
    ivec3 rotated = (model_id == FRAME_MODEL_ID)
        ? transformFramePos(local_pos, rotation, res)
        : rotateModelPos(local_pos, rotation, res);
    rotated = clamp(rotated, ivec3(0), ivec3(int(res) - 1));
    return sampleModelVoxel(model_id, rotated) != 0u;
}

// Sample with awareness of fence connections across block boundaries.
// Returns 1.0 for a solid voxel inside this model, 0.0 otherwise.
float sampleModelOcclusion(uint model_id, ivec3 local_pos, uint rotation) {
    uint res = model_properties[model_id].resolution;
    bool inside = all(greaterThanEqual(local_pos, ivec3(0))) &&
                  all(lessThan(local_pos, ivec3(int(res))));
    if (inside) {
        return sampleModelFilled(model_id, local_pos, rotation) ? 1.0 : 0.0;
    }
    return 0.0;
}

// Compute fine voxel bounds for a model (in block-local 0-1 space)
void modelCollisionBounds(uint model_id, out vec3 minB, out vec3 maxB) {
    ModelProperties props = model_properties[model_id];
    uint res = props.resolution;
    uvec3 minU = uvec3(props.aabb_min & 0xFF, (props.aabb_min >> 8) & 0xFF, (props.aabb_min >> 16) & 0xFF);
    uvec3 maxU = uvec3(props.aabb_max & 0xFF, (props.aabb_max >> 8) & 0xFF, (props.aabb_max >> 16) & 0xFF);
    minB = vec3(minU) / float(res);
    maxB = vec3(maxU) / float(res);
}

// Quick coarse 4x4x4 mask ray test (block-local 0-1 space). Returns true on hit.
bool modelMaskBlocksRay(vec3 origin, vec3 dir, uint model_id, uint rotation) {
    ModelProperties props = model_properties[model_id];
    uint hi = props.collision_mask.y;
    uint lo = props.collision_mask.x;

    // Transform ray to model space (inverse of model rotation)
    vec3 modelOrigin = inverseRotatePosition(origin, rotation);
    vec3 modelDir = inverseRotateDirection(dir, rotation);

    // Transform to 4x4x4 grid
    vec3 pos = modelOrigin * 4.0;
    vec3 safeDir = modelDir;
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
    uint rotation_and_flags,
    uint frame_mask,
    out vec3 out_color,
    out vec3 out_normal,
    out float out_t,
    out float out_alpha
) {
    // Get actual model resolution (8, 16, or 32)
    uint res = model_properties[model_id].resolution;
    uint rotation = rotation_and_flags & 3u;
    float fres = float(res);
    int maxSteps = int(res) * 3;
    bool isFrame = (model_id == FRAME_MODEL_ID);

    // Initialize alpha for translucency accumulation
    out_alpha = 0.0;
    vec3 accumulatedColor = vec3(0.0);
    float accumulatedAlpha = 0.0;
    // Scale to sub-voxel coordinates (0 to 16)
    vec3 pos = origin * fres;

    // Avoid infinities / NaNs for near-zero components
    vec3 safeDir = dir;
    const float DIR_EPS = 1e-4;
    safeDir.x = (abs(safeDir.x) < DIR_EPS) ? (safeDir.x >= 0.0 ? DIR_EPS : -DIR_EPS) : safeDir.x;
    safeDir.y = (abs(safeDir.y) < DIR_EPS) ? (safeDir.y >= 0.0 ? DIR_EPS : -DIR_EPS) : safeDir.y;
    safeDir.z = (abs(safeDir.z) < DIR_EPS) ? (safeDir.z >= 0.0 ? DIR_EPS : -DIR_EPS) : safeDir.z;
    vec3 invDir = 1.0 / safeDir;

    // Calculate entry t (may need to enter the model box from outside)
    // Expand bounds more for thin models like frames to ensure entry from all angles
    float boundsEps = isFrame ? 0.01 : SUB_VOXEL_EPS;
    vec3 tMin = (vec3(-boundsEps) - pos) * invDir;
    vec3 tMax = (vec3(fres + boundsEps) - pos) * invDir;

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
    startPos += safeDir * boundsEps;
    startPos = clamp(startPos, vec3(boundsEps), vec3(fres - boundsEps));

    // Current voxel position
    ivec3 voxel = ivec3(floor(startPos));
    voxel = clamp(voxel, ivec3(0), ivec3(int(res) - 1));

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
    // Track previous palette index to avoid internal face artifacts in translucent volumes
    uint prev_palette_idx = 0u;

    // March through sub-voxels
    for (int i = 0; i < maxSteps; i++) {
        // Check bounds
        if (any(lessThan(voxel, ivec3(0))) || any(greaterThanEqual(voxel, ivec3(int(res))))) {
            break;
        }

        // Apply rotation and sample
        ivec3 rotatedPos = isFrame
            ? transformFramePos(voxel, rotation, res)
            : rotateModelPos(voxel, rotation, res);
        rotatedPos = clamp(rotatedPos, ivec3(0), ivec3(int(res) - 1));
        uint palette_idx = sampleModelVoxel(model_id, rotatedPos);

        // Frame border masking for merged frames
        // Check if this edge should be stripped based on frame_mask
        // Use rotatedPos for coordinate checks (after frame rotation/flips)
        bool at_interior_edge = false;
        if (isFrame) {
            if (rotatedPos.x == 0 && (frame_mask & 1u) == 0u) {
                at_interior_edge = true;
            }
            if (rotatedPos.x == int(res) - 1 && (frame_mask & 2u) == 0u) {
                at_interior_edge = true;
            }
            if (rotatedPos.y == 0 && (frame_mask & 4u) == 0u) {
                at_interior_edge = true;
            }
            if (rotatedPos.y == int(res) - 1 && (frame_mask & 8u) == 0u) {
                at_interior_edge = true;
            }
        }

        // Skip ALL voxels at interior edges (both border and picture area)
        // This allows the ray to pass through and hit the adjacent frame's picture area
        if (at_interior_edge && isFrame && palette_idx != 0u) {
            continue;
        }

        // Default brown color for frame borders
        bool is_border_voxel = isFrame && (palette_idx >= 1u && palette_idx <= 3u);
        vec3 frame_debug_color = vec3(0.5, 0.3, 0.1);
        if (is_border_voxel) {
            frame_debug_color = vec3(0.5, 0.3, 0.1);  // Normal wood color
        }

        // Hit if not air (palette index 0 = transparent)
        if (palette_idx != 0u) {
            // Get color from palette
            vec4 paletteColor = getModelPaletteColor(model_id, palette_idx);

            // Use debug color for frame borders, otherwise use palette color
            vec3 final_color;
            if (is_border_voxel) {
                final_color = frame_debug_color;
            } else {
                final_color = paletteColor.rgb;
            }

            // Add per-voxel emission glow (e.g., torch flame)
            float emission = getModelPaletteEmission(model_id, palette_idx);
            if (emission > 0.0) {
                final_color += final_color * emission * 0.8;  // Glow using palette color
            }
            vec3 voxelColor = final_color;

            // Calculate surface info for this voxel
            uint hitAxis = (i == 0) ? entryAxis : stepped_axis;
            vec3 hitNormal = vec3(0.0);
            hitNormal[hitAxis] = -float(step[hitAxis]);
            float voxelDist = (i == 0) ? 0.0 : (tMaxAxis[stepped_axis] - tDelta[stepped_axis]);
            float hitT = (startT + voxelDist) / fres;

            // Check if this voxel is translucent (alpha < 1.0)
            if (paletteColor.a < 0.99) {
                // Only accumulate at surface boundaries, not internal faces
                // Internal face = same translucent material as previous voxel
                bool isInternalFace = (palette_idx == prev_palette_idx);

                if (!isInternalFace) {
                    // Entering new translucent region: blend and continue
                    float remainingAlpha = 1.0 - accumulatedAlpha;
                    float contribution = paletteColor.a * remainingAlpha;
                    accumulatedColor += voxelColor * contribution;
                    accumulatedAlpha += contribution;

                    // Record first hit surface info
                    if (out_alpha == 0.0) {
                        out_normal = hitNormal;
                        out_t = hitT;
                    }

                    // Early out if nearly opaque
                    if (accumulatedAlpha > 0.99) {
                        out_color = accumulatedColor;
                        out_alpha = accumulatedAlpha;
                        return true;
                    }
                }
                // Continue marching through translucent voxels
            } else {
                // Opaque voxel: final hit
                // Blend opaque color behind any accumulated translucency
                float remainingAlpha = 1.0 - accumulatedAlpha;
                out_color = accumulatedColor + voxelColor * remainingAlpha;
                out_alpha = 1.0;

                // Use first hit surface if we accumulated translucency, else this hit
                if (accumulatedAlpha == 0.0) {
                    out_normal = hitNormal;
                    out_t = hitT;
                }

                return true;
            }
        }

        prev_palette_idx = palette_idx;

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

    // If we accumulated any translucent voxels, return that
    if (accumulatedAlpha > 0.0) {
        out_color = accumulatedColor;
        out_alpha = accumulatedAlpha;
        return true;
    }

    return false;  // Ray passed through without hitting
}

// Shadow-only sub-voxel march: returns transmission factor (0 = full block, 1 = no block).
// Accumulates alpha and tint from translucent voxels for colored shadows.
// Uses a capped number of steps since we only need to know how much the model blocks light.
float marchSubVoxelShadow(
    vec3 origin,
    vec3 dir,
    uint model_id,
    uint rotation_and_flags,
    uint frame_mask,
    int maxSteps,
    out vec3 accumulatedTint
) {
    accumulatedTint = vec3(1.0);

    // Get actual model resolution (8, 16, or 32)
    uint res = model_properties[model_id].resolution;
    uint rotation = rotation_and_flags & 3u;
    float fres = float(res);

    bool isFrame = (model_id == FRAME_MODEL_ID);

    // Early out using coarse mask - skip for glass pane models (119-150) since their
    // thin frame geometry (1 voxel thick) can be missed by the 4x4x4 coarse mask DDA
    bool isGlassPane = (model_id >= 119u && model_id <= 150u);
    bool skipCoarse = isFrame;
    if (!isGlassPane && !skipCoarse && !modelMaskBlocksRay(origin, dir, model_id, rotation)) {
        return 1.0; // No blocking
    }

    // Clamp step budget to the maximum possible voxels we could traverse.
    int stepsLeft = clamp(maxSteps, 1, int(res) * 3);

    // Track accumulated transmission (starts at 1.0 = full light)
    float transmission = 1.0;

    // Scale to sub-voxel coordinates (0 to 16)
    vec3 pos = origin * fres;

    vec3 safeDir = makeSafeDir(dir);
    vec3 invDir = 1.0 / safeDir;

    // Calculate entry t into the model cube
    // Expand bounds more for thin models like frames
    float boundsEps = isFrame ? 0.01 : SUB_VOXEL_EPS;
    vec3 tMin = (vec3(-boundsEps) - pos) * invDir;
    vec3 tMax = (vec3(fres + boundsEps) - pos) * invDir;

    vec3 t1 = min(tMin, tMax);
    vec3 t2 = max(tMin, tMax);

    float tNear = max(max(t1.x, t1.y), t1.z);
    float tFar = min(min(t2.x, t2.y), t2.z);
    if (tNear > tFar || tFar < 0.0) {
        return 1.0; // No blocking
    }

    float startT = max(tNear, 0.0);
    vec3 startPos = pos + dir * startT;
    startPos += safeDir * boundsEps;
    startPos = clamp(startPos, vec3(boundsEps), vec3(fres - boundsEps));

    ivec3 voxel = ivec3(clamp(floor(startPos), vec3(0.0), vec3(float(res - 1u))));

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

    // Track previous palette index to avoid internal face artifacts
    uint prev_palette_idx = 0u;

    for (int i = 0; i < stepsLeft; i++) {
        if (any(lessThan(voxel, ivec3(0))) || any(greaterThanEqual(voxel, ivec3(int(res))))) {
            break;
        }

        ivec3 rotatedPos = isFrame
            ? transformFramePos(voxel, rotation, res)
            : rotateModelPos(voxel, rotation, res);
        rotatedPos = clamp(rotatedPos, ivec3(0), ivec3(int(res) - 1));
        uint palette_idx = sampleModelVoxel(model_id, rotatedPos);
        if (palette_idx != 0u) {
            // Get color and alpha from palette for translucency
            vec4 paletteColor = getModelPaletteColor(model_id, palette_idx);
            if (paletteColor.a >= 0.99) {
                // Opaque voxel: full shadow
                return 0.0;
            }
            // Only accumulate at surface boundaries, not internal faces
            bool isInternalFace = (palette_idx == prev_palette_idx);
            if (!isInternalFace) {
                // Translucent voxel: reduce transmission and accumulate tint
                float absorbed = paletteColor.a;
                // Glass pane glass should cast minimal shadow (mostly transparent for light)
                // while keeping visual translucency. Reduce absorption to 15% for shadow rays.
                if (isGlassPane) {
                    absorbed *= 0.15;
                }
                transmission *= (1.0 - absorbed);
                // Blend voxel color into tint based on how much light it absorbs
                accumulatedTint *= mix(vec3(1.0), paletteColor.rgb, absorbed);
                if (transmission < 0.05) {
                    return 0.0; // Early out when nearly opaque
                }
            }
        }

        prev_palette_idx = palette_idx;

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

    return transmission;
}

// Read model metadata at a texture coordinate
// Returns: model_id in .r, rotation and flags in .g
// Rotation: bits 0-1
// Waterlogged: bit 2
uvec2 readModelMetadata(ivec3 texCoord) {
    return imageLoad(modelMetadata, texCoord).rg;
}

bool isModelWaterlogged(uint metadataGreen) {
    return (metadataGreen & 4u) != 0u;
}

// Door model detection helpers (match sub_voxel.rs)
// Door model ID ranges: 39-46, 67-74, 75-82, 83-90, 91-98
bool isDoorModel(uint model_id) {
    return (model_id >= 39u && model_id <= 46u) ||
           (model_id >= 67u && model_id <= 74u) ||
           (model_id >= 75u && model_id <= 82u) ||
           (model_id >= 83u && model_id <= 90u) ||
           (model_id >= 91u && model_id <= 98u);
}

// Get the base model ID for a door type
uint doorTypeBase(uint model_id) {
    if (model_id >= 39u && model_id <= 46u) return 39u;
    if (model_id >= 67u && model_id <= 74u) return 67u;
    if (model_id >= 75u && model_id <= 82u) return 75u;
    if (model_id >= 83u && model_id <= 90u) return 83u;
    if (model_id >= 91u && model_id <= 98u) return 91u;
    return 0u;
}

// Check if a door model is the upper half
bool isDoorUpper(uint model_id) {
    uint base = doorTypeBase(model_id);
    if (base == 0u) return false;
    uint offset = model_id - base;
    return (offset == 2u || offset == 3u || offset == 6u || offset == 7u);
}

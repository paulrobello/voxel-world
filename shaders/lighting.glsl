// Sun/sky lighting helpers and point lights

// Shadow acceleration (chunk/brick skipping) now enabled by default
const bool SHADOW_SKIP = true;

// Helper: test whether a ray segment through a model block hits its sub-voxel geometry
// Returns transmission factor (0 = full block, 1 = no block) and accumulated tint from translucent voxels
float modelBlocksRay(vec3 rayOrigin, vec3 dir, ivec3 blockPos, uint model_id, uint rotation, out vec3 modelTint) {
    vec3 localOrigin = rayOrigin - vec3(blockPos);
    vec3 safeDir = makeSafeDir(dir);

    // If origin is outside block, compute proper entry point via ray-box intersection
    // This avoids aggressive clamping that can cause inconsistencies at block boundaries
    if (any(lessThan(localOrigin, vec3(0.0))) || any(greaterThan(localOrigin, vec3(1.0)))) {
        vec3 t1 = -localOrigin / safeDir;
        vec3 t2 = (vec3(1.0) - localOrigin) / safeDir;
        vec3 tNear = min(t1, t2);
        vec3 tFar = max(t1, t2);
        float tEntry = max(max(tNear.x, tNear.y), tNear.z);
        float tExit = min(min(tFar.x, tFar.y), tFar.z);

        if (tEntry > tExit || tExit < 0.0) {
            modelTint = vec3(1.0);
            return 1.0;  // Ray misses block entirely
        }
        localOrigin = localOrigin + safeDir * max(0.0, tEntry + SUB_VOXEL_EPS * 0.1);
    }

    localOrigin = clamp(localOrigin, vec3(SUB_VOXEL_EPS), vec3(1.0 - SUB_VOXEL_EPS));
    // Shadow path: cheaper, capped marcher—only cares if any occupied voxel blocks light.
    // Limit steps to reduce worst-case cost through thin models; still covers the 8^3 grid.
    const int SHADOW_MODEL_MAX_STEPS = 16;
    return marchSubVoxelShadow(localOrigin, dir, model_id, rotation, SHADOW_MODEL_MAX_STEPS, modelTint);
}

// Advance a DDA to the next voxel along the ray.
// Updates pos, rayPos, tMax; returns stepped axis and step distance.
void ddaAdvance(
    inout ivec3 pos,
    inout vec3 rayPos,
    inout vec3 tMax,
    vec3 tDelta,
    ivec3 stepDir,
    vec3 dir,
    out int stepAxis,
    out float stepDist
) {
    stepDist = tMax.x;
    stepAxis = 0;
    if (tMax.y < stepDist) {
        stepDist = tMax.y;
        stepAxis = 1;
    }
    if (tMax.z < stepDist) {
        stepDist = tMax.z;
        stepAxis = 2;
    }

    rayPos += dir * stepDist;
    tMax -= vec3(stepDist);

    if (stepAxis == 0) {
        tMax.x += tDelta.x;
        pos.x += stepDir.x;
    } else if (stepAxis == 1) {
        tMax.y += tDelta.y;
        pos.y += stepDir.y;
    } else {
        tMax.z += tDelta.z;
        pos.z += stepDir.z;
    }
}

// Cast shadow ray from a point toward the sun
// Returns shadow factor (0 = full shadow, 1 = no shadow)
// shadowTint returns the accumulated tint from tinted glass (or 1.0 if none)
float castShadowRayInternal(vec3 origin, bool ignoreStartModel, out uint debugFlag, out vec3 shadowTint) {
    debugFlag = 0u;
    shadowTint = vec3(1.0);
    vec3 sunDir = getCurrentSunDir();

    if (sunDir.y < 0.0) {
        return 0.3;  // Dim ambient at night
    }

    vec3 dir = sunDir;
    vec3 inv_dir = clamp(1.0 / dir, vec3(-FLT_MAX), vec3(FLT_MAX));
    bool allowSkip = SHADOW_SKIP && (pc.enable_model_shadows == 0u);

    // Track accumulated shadow from partial blockers - only applies if ray reaches sky
    float accumulatedPartialShadow = 1.0;

    vec3 rayPos = origin;
    ivec3 pos = ivec3(floor(rayPos));
    ivec3 startPos = pos;
    ivec3 stepDir = ivec3(sign(dir));

    vec3 tMax = (vec3(pos) + 0.5 + 0.5 * vec3(stepDir) - rayPos) * inv_dir;
    vec3 tDelta = abs(inv_dir);

    if (ignoreStartModel) {
        float exitT = min(tMax.x, min(tMax.y, tMax.z)) + 0.002;
        rayPos += dir * exitT;
        pos = ivec3(floor(rayPos));
        startPos = pos;
        tMax = (vec3(pos) + 0.5 + 0.5 * vec3(stepDir) - rayPos) * inv_dir;
    }

    const float MAX_SHADOW_DIST = 256.0;
    float totalDist = 0.0;
    float maxAbsDir = max(abs(dir.x), max(abs(dir.y), abs(dir.z)));
    // Angle-aware step budget: scale with both view angle and the distance needed to leave the loaded world.
    vec3 worldMin = vec3(0.0);
    vec3 worldMax = worldSize();
    vec3 tExitWorld = mix((worldMin - rayPos) * inv_dir,
                          (worldMax - rayPos) * inv_dir,
                          step(vec3(0.0), dir));
    float exitDist = min(min(tExitWorld.x, tExitWorld.y), tExitWorld.z);
    float maxTravel = min(MAX_SHADOW_DIST, max(exitDist, 0.0));

    // Distance-based step reduction: fewer steps for shadows far from camera
    float camDist = length(origin + textureOrigin() - pc.camera_pos.xyz);
    float distanceFactor = clamp(1.0 - (camDist - 16.0) / 64.0, 0.4, 1.0); // 100% at <16, 40% at >80
    float baseSteps = float(pc.shadow_max_steps) * distanceFactor;
    int maxSteps = int(clamp(min(maxTravel * maxAbsDir + 4.0, baseSteps), 32.0, float(pc.shadow_max_steps)));

    for (int i = 0; i < maxSteps; i++) {
        // Optional coarse skipping: empty chunks/bricks
        if (allowSkip) {
            ivec3 chunkPos = pos / int(CHUNK_SIZE);
            uvec2 metaHere = readModelMetadata(pos);
            bool hasModelHere = metaHere.r != 0u;
            if (isChunkEmpty(chunkPos) && !hasModelHere) {
                vec3 chunkMin = vec3(chunkPos) * float(CHUNK_SIZE);
                vec3 chunkMax = chunkMin + float(CHUNK_SIZE);
                vec3 tExit = mix((chunkMin - rayPos) * inv_dir,
                                 (chunkMax - rayPos) * inv_dir,
                                 step(vec3(0.0), dir));
                float minExit = min(min(tExit.x, tExit.y), tExit.z);
                rayPos += dir * (minExit + 0.001);
                pos = ivec3(floor(rayPos));
                tMax = (vec3(pos) + 0.5 + 0.5 * vec3(stepDir) - rayPos) * inv_dir;
                totalDist += minExit;
                continue;
            }
        }
        // Disable brick skipping entirely when model shadows are enabled to avoid missing thin geometry.
        if (allowSkip && isBrickEmpty(pos)) {
            ivec3 brickWorldPos = getBrickWorldPos(pos);
            vec3 brickMin = vec3(brickWorldPos);
            vec3 brickMax = brickMin + float(BRICK_SIZE);
            vec3 tExit = mix((brickMin - rayPos) * inv_dir,
                             (brickMax - rayPos) * inv_dir,
                             step(vec3(0.0), dir));
            float minExit = min(min(tExit.x, tExit.y), tExit.z);
            rayPos += dir * (minExit + 0.001);
            pos = ivec3(floor(rayPos));
            tMax = (vec3(pos) + 0.5 + 0.5 * vec3(stepDir) - rayPos) * inv_dir;
            totalDist += minExit;
            continue;
        }

        bool oob = !isInTextureBounds(pos);
        if (oob) {
            debugFlag = 7u; // out of loaded area = sky
            return accumulatedPartialShadow;  // Apply partial blockers if ray escaped
        }
        uint blockType = readBlockTypeAtTexCoord(pos);

        bool skipSelf = all(equal(pos, startPos));
        if (skipSelf && blockType == BLOCK_MODEL && !ignoreStartModel) {
            skipSelf = false;
        }

        if (!skipSelf) {
            vec3 blockOrigin = rayPos + dir * 0.001; // texture space
            if (blockType == BLOCK_MODEL) {
                if (pc.enable_model_shadows == 0u) {
                    // Treat model as non-blocking when disabled
                } else {
                    // Match render LOD: skip sub-voxel shadowing when model is beyond camera LOD range.
                    // Compare camera distance in world space so LOD matches render culling.
                    float camDist = length(vec3(pos) + vec3(0.5 + textureOrigin()) - pc.camera_pos.xyz);
                    uvec2 meta = readModelMetadata(pos);
                    uint model_id = meta.r;
                    uint rotation = meta.g & 3u;
                    const float MODEL_PARTIAL_SHADOW = 0.4;
                    if (model_id == 0u) {
                        // Invalid model - accumulate partial shadow and continue to check for full blockers
                        accumulatedPartialShadow *= MODEL_PARTIAL_SHADOW;
                    } else {
                        ModelProperties props = model_properties[model_id];

                        bool forceFine = (model_id == 2u || model_id == 3u); // slabs need fine shadowing even far
                        if (camDist > pc.lod_model_distance && !forceFine) {
                            // Far: skip fine march but still honor coarse light-blocking flags.
                            if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_FULL) != 0u) {
                                debugFlag = 5u;
                                return 0.0;
                            }
                            if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_PARTIAL) != 0u) {
                                // Accumulate partial shadow and continue - don't return early
                                debugFlag = 5u;
                                accumulatedPartialShadow *= MODEL_PARTIAL_SHADOW;
                            }
                        } else {
                            if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_FULL) != 0u ||
                                (props.flags & MODEL_FLAG_LIGHT_BLOCK_PARTIAL) != 0u) {
                                vec3 modelTint;
                                float transmission = modelBlocksRay(blockOrigin, dir, pos, model_id, rotation, modelTint);
                                bool hitGeo = (transmission < 1.0);
                                if (hitGeo) {
                                    // Accumulate model tint for translucent sub-voxels
                                    shadowTint *= modelTint;
                                    if (transmission < 0.05) {
                                        // Nearly opaque hit
                                        if (model_id == 2u || model_id == 3u) {
                                            debugFlag = 2u;
                                            return 0.0; // slabs: block fully where geometry exists
                                        }
                                        if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_PARTIAL) != 0u) {
                                            // Accumulate partial shadow and continue to check for full blockers behind
                                            debugFlag = 3u;
                                            accumulatedPartialShadow *= MODEL_PARTIAL_SHADOW;
                                        } else {
                                            debugFlag = 2u;
                                            return 0.0;
                                        }
                                    }
                                    // Partial transmission through translucent sub-voxels - continue ray
                                }
                                // For full blockers, conservative: still block if not hit due to precision.
                                if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_FULL) != 0u && transmission >= 1.0) {
                                    debugFlag = 4u;
                                    return 0.0;
                                }
                                // For partial blockers, only geometry should occlude; skip coarse mask fallback to avoid over-occluding thin models (e.g., ladders).
                            }
                        }
                    }
                }
            } else if (blockType != BLOCK_AIR && blockType != BLOCK_LEAVES && blockType != BLOCK_GLASS && blockType != BLOCK_TINTED_GLASS && blockType != BLOCK_WATER) {
                debugFlag = 1u;
                return 0.0;
            }

            // Accumulate tint from tinted glass for colored shadows
            if (blockType == BLOCK_TINTED_GLASS && pc.enable_tinted_shadows != 0u) {
                uvec2 meta = readModelMetadata(pos);
                uint tintIndex = meta.g & 0x1Fu;
                shadowTint *= TINT_PALETTE[tintIndex] * 0.85; // Slight attenuation
            }

            if (blockType == BLOCK_LEAVES) {
                debugFlag = 6u;
                return 0.5;
            }
        }
        else {
            float advanceT = min(tMax.x, min(tMax.y, tMax.z)) + 0.001;
            rayPos += dir * advanceT;
            totalDist += advanceT;
            pos = ivec3(floor(rayPos));
            tMax = (vec3(pos) + 0.5 + 0.5 * vec3(stepDir) - rayPos) * inv_dir;
            continue;
        }

        int stepAxis;
        float stepDist;
        ddaAdvance(pos, rayPos, tMax, tDelta, stepDir, dir, stepAxis, stepDist);
        totalDist += stepDist;

        if (totalDist > MAX_SHADOW_DIST) {
            debugFlag = 8u;
            return accumulatedPartialShadow;  // Apply partial blockers if ray reached max distance
        }
    }

    return accumulatedPartialShadow;  // Apply partial blockers only if ray escaped to sky
}

float castShadowRay(vec3 origin) {
    uint dbg;
    vec3 tint;
    return castShadowRayInternal(origin, false, dbg, tint);
}

// Cast shadow ray and return tint for colored shadows
float castShadowRayWithTint(vec3 origin, out vec3 shadowTint) {
    uint dbg;
    return castShadowRayInternal(origin, false, dbg, shadowTint);
}

// Sky exposure for ambient light
float getSkyExposure(vec3 origin) {
    vec3 dir = vec3(0.0, 1.0, 0.0);
    vec3 rayPos = origin;
    ivec3 pos = ivec3(floor(rayPos));
    ivec3 startPos = pos;
    ivec3 stepDir = ivec3(0, 1, 0);
    vec3 tMax = vec3(1e30);
    tMax.y = (float(pos.y) + 1.0 - rayPos.y);
    vec3 tDelta = vec3(1e30);
    tDelta.y = 1.0;

    // Track accumulated exposure from partial blockers - only applies if ray reaches sky
    float accumulatedPartialExposure = 1.0;
    const float MODEL_PARTIAL_EXPOSURE = 0.4;

    for (int i = 0; i < 128; i++) {
        if (!isInTextureBounds(pos)) {
            return accumulatedPartialExposure;  // Apply partial blockers if ray escaped
        }

        vec3 blockOrigin = rayPos + dir * 0.01;
        uint blockType = readBlockTypeAtTexCoord(pos);
        bool skipSelf = all(equal(pos, startPos));

        if (!skipSelf) {
            if (blockType == BLOCK_MODEL) {
                if (pc.enable_model_shadows == 0u) {
                    // Treat model as non-blocking for sky when disabled
                } else {
                    uvec2 meta = readModelMetadata(pos);
                    uint model_id = meta.r;
                    uint rotation = meta.g & 3u;
                    if (model_id == 0u) {
                        // Invalid model - accumulate partial exposure and continue
                        accumulatedPartialExposure *= MODEL_PARTIAL_EXPOSURE;
                    } else {
                        ModelProperties props = model_properties[model_id];

                        float transmission = 1.0;
                        if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_FULL) != 0u ||
                            (props.flags & MODEL_FLAG_LIGHT_BLOCK_PARTIAL) != 0u) {
                            vec3 dummyTint;
                            transmission = modelBlocksRay(blockOrigin, dir, pos, model_id, rotation, dummyTint);
                        }
                        bool blocksRay = (transmission < 1.0);

                        if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_FULL) != 0u ||
                            (props.flags & MODEL_FLAG_LIGHT_BLOCK_PARTIAL) != 0u) {
                            if (blocksRay && transmission < 0.05) {
                                // Nearly opaque hit
                                if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_FULL) != 0u) {
                                    return 0.0;
                                }
                                if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_PARTIAL) != 0u) {
                                    // Accumulate and continue to check for full blockers behind
                                    accumulatedPartialExposure *= MODEL_PARTIAL_EXPOSURE;
                                }
                            } else if (blocksRay) {
                                // Partial transmission - accumulate and continue
                                accumulatedPartialExposure *= transmission;
                            } else {
                                if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_FULL) != 0u) {
                                    return 0.0;
                                }
                                // For partial blockers that don't block this ray, continue without penalty
                            }
                        }
                    }
                }
            } else if (blockType != BLOCK_AIR && blockType != BLOCK_WATER &&
                       blockType != BLOCK_GLASS && blockType != BLOCK_TINTED_GLASS && blockType != BLOCK_LEAVES) {
                return 0.0;
            }

            if (blockType == BLOCK_LEAVES) {
                // Leaves accumulate partial exposure and continue
                accumulatedPartialExposure *= 0.4;
            }
        }
        else {
            int stepAxis;
            float stepDist;
            ddaAdvance(pos, rayPos, tMax, tDelta, stepDir, dir, stepAxis, stepDist);
            continue;
        }

        // Advance ray for transparent blocks (AIR, WATER, GLASS, TINTED_GLASS)
        int stepAxis;
        float stepDist;
        ddaAdvance(pos, rayPos, tMax, tDelta, stepDir, dir, stepAxis, stepDist);
    }

    return accumulatedPartialExposure;  // Apply partial blockers if ray completed without full block
}

// Light animation modes (encoded in intensity: mode = floor(intensity), actual = fract(intensity) * 2)
// Mode 0: Steady (no animation)
// Mode 1: Slow pulse (gentle breathing)
// Mode 2: Torch flicker (fast, erratic)
const uint LIGHT_MODE_STEADY = 0;
const uint LIGHT_MODE_PULSE = 1;
const uint LIGHT_MODE_FLICKER = 2;

// Point light accumulation
vec3 calculatePointLights(vec3 worldPos, vec3 normal) {
    vec3 totalLight = vec3(0.0);

    for (uint i = 0; i < pc.light_count; i++) {
        PointLight light = lights[i];
        vec3 lightPos = light.pos_radius.xyz;
        float lightRadius = light.pos_radius.w;
        vec3 lightColor = light.color.rgb;
        float encodedIntensity = light.color.a;

        // Decode mode and intensity: mode = floor(value), intensity = fract(value) * 2
        uint mode = uint(floor(encodedIntensity));
        float intensity = fract(encodedIntensity) * 2.0;

        vec3 toLight = lightPos - worldPos;
        float distance = length(toLight);
        if (distance > lightRadius) continue;

        vec3 lightDir = toLight / distance;
        float attenuation = 1.0 - smoothstep(0.0, lightRadius, distance);
        attenuation = attenuation * attenuation;

        float diffuse = max(0.0, dot(normal, lightDir));
        float ambient = 0.15;

        vec3 lightContrib = lightColor * intensity * attenuation * (diffuse + ambient);

        // Apply animation based on mode
        if (mode == LIGHT_MODE_PULSE) {
            // Slow, gentle breathing effect
            float pulse = 0.92 + 0.08 * sin(pc.animation_time * 1.5 + float(i) * 2.1);
            lightContrib *= pulse;
        } else if (mode == LIGHT_MODE_FLICKER) {
            // Fast, erratic torch flicker
            float flicker = 0.95 + 0.05 * sin(pc.animation_time * 15.0 + float(i) * 7.3);
            flicker *= 0.97 + 0.03 * sin(pc.animation_time * 23.0 + float(i) * 11.1);
            lightContrib *= flicker;
        }
        // Mode 0 (STEADY): no animation applied

        totalLight += lightContrib;
    }

    return totalLight;
}

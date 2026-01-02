// Sun/sky lighting helpers and point lights

// Shadow acceleration (chunk/brick skipping) now enabled by default
const bool SHADOW_SKIP = true;

// Helper: test whether a ray segment through a model block hits its sub-voxel geometry
bool modelBlocksRay(vec3 rayOrigin, vec3 dir, ivec3 blockPos, uint model_id, uint rotation) {
    vec3 localOrigin = clamp(rayOrigin - vec3(blockPos), vec3(SUB_VOXEL_EPS), vec3(1.0 - SUB_VOXEL_EPS));
    // Shadow path: cheaper, capped marcher—only cares if any occupied voxel blocks light.
    // Limit steps to reduce worst-case cost through thin models; still covers the 8^3 grid.
    const int SHADOW_MODEL_MAX_STEPS = 16;
    return marchSubVoxelShadow(localOrigin, dir, model_id, rotation, SHADOW_MODEL_MAX_STEPS);
}

// Cast shadow ray from a point toward the sun
float castShadowRayInternal(vec3 origin, bool ignoreStartModel, out uint debugFlag) {
    debugFlag = 0u;
    vec3 sunDir = getCurrentSunDir();

    if (sunDir.y < 0.0) {
        return 0.3;  // Dim ambient at night
    }

    vec3 dir = sunDir;
    vec3 inv_dir = clamp(1.0 / dir, vec3(-FLT_MAX), vec3(FLT_MAX));

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
    int maxSteps = int(clamp(MAX_SHADOW_DIST * maxAbsDir + 4.0, 96.0, 256.0)); // angle-aware cap; higher when sun is low

    for (int i = 0; i < maxSteps; i++) {
        // Optional coarse skipping: empty chunks/bricks
        if (SHADOW_SKIP) {
            ivec3 chunkPos = pos / int(CHUNK_SIZE);
            if (isChunkEmpty(chunkPos)) {
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
            if (isBrickEmpty(pos)) {
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
        }

        bool oob = !isInTextureBounds(pos);
        if (oob) {
            debugFlag = 7u; // out of loaded area = sky
            return 1.0;
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
                    if (camDist > SUB_VOXEL_LOD_DISTANCE) {
                        // No detailed model shadow beyond LOD distance.
                    } else {
                        uvec2 meta = readModelMetadata(pos);
                        uint model_id = meta.r;
                        uint rotation = meta.g & 3u;
                        const float MODEL_PARTIAL_SHADOW = 0.4;
                        if (model_id == 0u) {
                            return MODEL_PARTIAL_SHADOW;
                        }
                        ModelProperties props = model_properties[model_id];

                        if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_FULL) != 0u ||
                            (props.flags & MODEL_FLAG_LIGHT_BLOCK_PARTIAL) != 0u) {
                            bool hitGeo = modelBlocksRay(blockOrigin, dir, pos, model_id, rotation);
                            if (hitGeo) {
                                if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_PARTIAL) != 0u) {
                                    debugFlag = 3u;
                                    return MODEL_PARTIAL_SHADOW;
                                } else {
                                    debugFlag = 2u;
                                    return 0.0;
                                }
                            }
                            // For full blockers, conservative: still block if not hit due to precision.
                            if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_FULL) != 0u) {
                                debugFlag = 4u;
                                return 0.0;
                            }
                            // For partial blockers, only geometry should occlude; skip coarse mask fallback to avoid over-occluding thin models (e.g., ladders).
                        }
                    }
                }
            } else if (blockType != BLOCK_AIR && blockType != BLOCK_LEAVES && blockType != BLOCK_GLASS && blockType != BLOCK_WATER) {
                debugFlag = 1u;
                return 0.0;
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

        float stepDist = tMax.x;
        int stepAxis = 0;
        if (tMax.y < stepDist) {
            stepDist = tMax.y;
            stepAxis = 1;
        }
        if (tMax.z < stepDist) {
            stepDist = tMax.z;
            stepAxis = 2;
        }

        rayPos += dir * stepDist;
        totalDist += stepDist;

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

        if (totalDist > MAX_SHADOW_DIST) {
            debugFlag = 8u;
            return 1.0;
        }
    }

    return 1.0;
}

float castShadowRay(vec3 origin) {
    uint dbg;
    return castShadowRayInternal(origin, false, dbg);
}

// Sky exposure for ambient light
float getSkyExposure(vec3 origin) {
    vec3 dir = vec3(0.0, 1.0, 0.0);
    vec3 rayPos = origin;
    ivec3 pos = ivec3(floor(rayPos));
    ivec3 startPos = pos;
    float tMax = (float(pos.y) + 1.0 - rayPos.y);
    float tDelta = 1.0;

    for (int i = 0; i < 128; i++) {
        if (!isInTextureBounds(pos)) {
            return 1.0;
        }

        vec3 blockOrigin = rayPos + dir * 0.01;
        uint blockType = readBlockTypeAtTexCoord(pos);
        bool skipSelf = all(equal(pos, startPos));

        if (!skipSelf) {
            const float MODEL_PARTIAL_EXPOSURE = 0.4;
            if (blockType == BLOCK_MODEL) {
                if (pc.enable_model_shadows == 0u) {
                    // Treat model as non-blocking for sky when disabled
                } else {
                    uvec2 meta = readModelMetadata(pos);
                    uint model_id = meta.r;
                    uint rotation = meta.g & 3u;
                    if (model_id == 0u) {
                        return MODEL_PARTIAL_EXPOSURE;
                    }
                    ModelProperties props = model_properties[model_id];

                    bool blocksRay = false;
                    if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_FULL) != 0u ||
                        (props.flags & MODEL_FLAG_LIGHT_BLOCK_PARTIAL) != 0u) {
                        blocksRay = modelBlocksRay(blockOrigin, dir, pos, model_id, rotation);
                    }

                    if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_FULL) != 0u ||
                        (props.flags & MODEL_FLAG_LIGHT_BLOCK_PARTIAL) != 0u) {
                        if (blocksRay) {
                            if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_FULL) != 0u) {
                                return 0.0;
                            }
                            if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_PARTIAL) != 0u) {
                                return MODEL_PARTIAL_EXPOSURE;
                            }
                        } else {
                            if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_FULL) != 0u) {
                                return 0.0;
                            }
                            if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_PARTIAL) != 0u) {
                                return 1.0;
                            }
                        }
                    }
                }
            } else if (blockType != BLOCK_AIR && blockType != BLOCK_WATER &&
                       blockType != BLOCK_GLASS && blockType != BLOCK_LEAVES) {
                return 0.0;
            }

            if (blockType == BLOCK_LEAVES) {
                return 0.4;
            }
        }
        else {
            rayPos += dir * (tMax + 0.001);
            pos = ivec3(floor(rayPos));
            tMax = 1.0;
            continue;
        }

        pos.y += 1;
        rayPos += dir * tDelta;
    }

    return 1.0;
}

// Point light accumulation
vec3 calculatePointLights(vec3 worldPos, vec3 normal) {
    vec3 totalLight = vec3(0.0);

    for (uint i = 0; i < pc.light_count; i++) {
        PointLight light = lights[i];
        vec3 lightPos = light.pos_radius.xyz;
        float lightRadius = light.pos_radius.w;
        vec3 lightColor = light.color.rgb;
        float intensity = light.color.a;

        vec3 toLight = lightPos - worldPos;
        float distance = length(toLight);
        if (distance > lightRadius) continue;

        vec3 lightDir = toLight / distance;
        float attenuation = 1.0 - smoothstep(0.0, lightRadius, distance);
        attenuation = attenuation * attenuation;

        float diffuse = max(0.0, dot(normal, lightDir));
        float ambient = 0.15;

        vec3 lightContrib = lightColor * intensity * attenuation * (diffuse + ambient);

        float flicker = 0.95 + 0.05 * sin(pc.animation_time * 15.0 + float(i) * 7.3);
        flicker *= 0.97 + 0.03 * sin(pc.animation_time * 23.0 + float(i) * 11.1);
        lightContrib *= flicker;

        totalLight += lightContrib;
    }

    return totalLight;
}

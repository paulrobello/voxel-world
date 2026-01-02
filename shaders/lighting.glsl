// Sun/sky lighting helpers and point lights

// Helper: test whether a ray segment through a model block hits its sub-voxel geometry
bool modelBlocksRay(vec3 rayOrigin, vec3 dir, ivec3 blockPos, uint model_id, uint rotation) {
    vec3 localOrigin = clamp(rayOrigin - vec3(blockPos), vec3(SUB_VOXEL_EPS), vec3(1.0 - SUB_VOXEL_EPS));

    ivec3 subHit;
    vec3 n;
    float t;
    return findSubVoxelHit(localOrigin, dir, model_id, rotation, subHit, n, t);
}

// Generic voxel march with a user predicate. Returns true when predicate says stop.
// The predicate signature:
//   bool pred(ivec3 pos, bool skipSelf, vec3 rayPos, out uint dbg, out float result)
// - pos: current voxel
// - skipSelf: true on the first voxel when ignoreStartModel is requested
// - rayPos: current ray position in voxel space
// - dbg: optional debug flag (only used by shadow)
// - result: predicate writes the final factor (shadow or exposure). If pred returns true, march stops and result is returned.
// Returns the factor produced by predicate, or 1.0 if the march exits world/limits.
float marchUntil(
    vec3 origin,
    vec3 dir,
    bool ignoreStartModel,
    bool forShadow,
    out uint debugFlag,
    bool isSkyPass
) {
    debugFlag = 0u;
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

    float maxShadowDist = 256.0;
    float totalDist = 0.0;
    int maxSteps = 128;

    for (int i = 0; i < maxSteps; i++) {
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
            if (forShadow) {
                vec3 blockOrigin = rayPos + dir * 0.001;
                if (blockType == BLOCK_MODEL) {
                    if (pc.enable_model_shadows == 0u) {
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
                            if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_FULL) != 0u) {
                                debugFlag = 4u;
                                return 0.0;
                            }
                            if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_PARTIAL) != 0u) {
                                vec3 localO = blockOrigin - vec3(pos);
                                vec3 testO = clamp(localO, vec3(0.001), vec3(0.999));
                                vec3 testDir = dir;

                                testO = inverseRotatePosition(testO, rotation);
                                testDir = inverseRotateDirection(testDir, rotation);

                                vec3 bbMin, bbMax;
                                modelCollisionBounds(model_id, bbMin, bbMax);

                                vec3 safeTestDir = makeSafeDir(testDir);
                                vec3 invd = 1.0 / safeTestDir;
                                vec3 t1b = (bbMin - testO) * invd;
                                vec3 t2b = (bbMax - testO) * invd;
                                vec3 tminb = min(t1b, t2b);
                                vec3 tmaxb = max(t1b, t2b);
                                float tNear = max(max(tminb.x, tminb.y), tminb.z);
                                float tFar = min(min(tmaxb.x, tmaxb.y), tmaxb.z);
                                
                                if (tNear <= tFar && tFar > 0.0) {
                                    if (modelMaskBlocksRay(testO, testDir, model_id)) {
                                        debugFlag = 5u;
                                        return MODEL_PARTIAL_SHADOW;
                                    }
                                }
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
            } else {
                // sky pass: any solid stops exposure
                if (blockType != BLOCK_AIR && blockType != BLOCK_WATER &&
                    blockType != BLOCK_GLASS && blockType != BLOCK_LEAVES) {
                    return 0.0;
                }
                if (blockType == BLOCK_LEAVES) {
                    return 0.4;
                }
                if (blockType == BLOCK_MODEL && pc.enable_model_shadows != 0u) {
                    uvec2 meta = readModelMetadata(pos);
                    uint model_id = meta.r;
                    uint rotation = meta.g & 3u;
                    if (model_id == 0u) {
                        return 0.4;
                    }
                    ModelProperties props = model_properties[model_id];
                    bool blocksRay = false;
                    if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_FULL) != 0u ||
                        (props.flags & MODEL_FLAG_LIGHT_BLOCK_PARTIAL) != 0u) {
                        blocksRay = modelBlocksRay(rayPos + dir * 0.01, dir, pos, model_id, rotation);
                    }
                    if (blocksRay) {
                        if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_FULL) != 0u) return 0.0;
                        if ((props.flags & MODEL_FLAG_LIGHT_BLOCK_PARTIAL) != 0u) return 0.4;
                    }
                }
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

        if (totalDist > maxShadowDist) {
            debugFlag = 8u;
            return 1.0;
        }
    }

    return 1.0;
}

float castShadowRay(vec3 origin) {
    uint dbg;
    vec3 sunDir = getCurrentSunDir();
    if (sunDir.y < 0.0) return 0.3;
    return marchUntil(origin, sunDir, false, true, dbg, false);
}

// Backward-compatible helper with debug flag (used by traversal for debug mode)
float castShadowRayInternal(vec3 origin, bool ignoreStartModel, out uint debugFlag) {
    vec3 sunDir = getCurrentSunDir();
    if (sunDir.y < 0.0) {
        debugFlag = 0u;
        return 0.3;
    }
    return marchUntil(origin, sunDir, ignoreStartModel, true, debugFlag, false);
}

// Sky exposure for ambient light
float getSkyExposure(vec3 origin) {
    uint dbg;
    return marchUntil(origin, vec3(0.0, 1.0, 0.0), false, false, dbg, true);
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

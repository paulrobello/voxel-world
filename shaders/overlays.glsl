// Particle, preview, and overlay rendering helpers

bool hasPreviewBlock() { return pc.preview_block_x >= 0; }
bool hasTargetBlock() { return pc.target_block_x >= 0; }
bool isTargetBlock(ivec3 texCoord) {
    return hasTargetBlock() &&
           texCoord.x == pc.target_block_x &&
           texCoord.y == pc.target_block_y &&
           texCoord.z == pc.target_block_z;
}

vec3 applyTargetHighlight(vec3 color) {
    vec3 brightened = color * 1.45 + vec3(0.15);
    vec3 cyanTint = vec3(0.55, 1.0, 1.0);
    return min(brightened * cyanTint, vec3(1.0));
}

float getWireframeFactor(vec3 localHit, vec3 normal) {
    vec2 edgeCoords;
    if (abs(normal.x) > 0.5) {
        edgeCoords = localHit.yz;
    } else if (abs(normal.y) > 0.5) {
        edgeCoords = localHit.xz;
    } else {
        edgeCoords = localHit.xy;
    }

    // Thin but visible outline; edgeWidth controls thickness in UV space
    float edgeWidth = 0.02;
    float dx = min(edgeCoords.x, 1.0 - edgeCoords.x);
    float dy = min(edgeCoords.y, 1.0 - edgeCoords.y);
    float edgeDist = min(dx, dy);

    return 1.0 - smoothstep(0.0, edgeWidth, edgeDist);
}

// Render particles and composite over the scene
bool renderParticles(vec3 origin, vec3 dir, inout vec3 color, inout float hitDistance) {
    bool anyHit = false;
    float dirLen = length(dir);
    if (dirLen < 1e-6) return false;

    float closestT = hitDistance;
    vec3 closestColor = color;
    float closestAlpha = 0.0;

    for (uint i = 0; i < pc.particle_count; i++) {
        Particle p = particles[i];
        vec3 particlePos = p.pos_size.xyz;
        float particleSize = p.pos_size.w;
        vec3 particleColor = p.color_alpha.rgb;
        float particleAlpha = p.color_alpha.a;

        vec3 faceNormal;
        float t = rayAABBIntersect(origin, dir, particlePos, particleSize, faceNormal);
        float tWorld = t * dirLen;
        if (t > 0.0 && tWorld < closestT) {
            vec3 sunDir = getCurrentSunDir();
            float daylight = getDaylightFactor(pc.time_of_day);
            float sunLight = max(0.0, dot(faceNormal, sunDir));
            float ambient = mix(0.2, 0.4, daylight);
            float direct = mix(0.1, 0.6, daylight) * sunLight;
            float lighting = ambient + direct;

            closestT = tWorld;
            closestColor = particleColor * lighting;
            closestAlpha = particleAlpha;
            anyHit = true;
        }
    }

    if (anyHit) {
        color = mix(color, closestColor, closestAlpha);
        hitDistance = min(hitDistance, closestT);
    }

    return anyHit;
}

// Render falling blocks and composite over the scene
bool renderFallingBlocks(vec3 origin, vec3 dir, inout vec3 color, inout float hitDistance) {
    bool anyHit = false;
    float dirLen = length(dir);
    if (dirLen < 1e-6) return false;

    float closestT = hitDistance;
    vec3 closestColor = color;

    for (uint i = 0; i < pc.falling_block_count; i++) {
        FallingBlock fb = falling_blocks[i];
        vec3 blockCenter = fb.pos_type.xyz;
        uint blockType = uint(fb.pos_type.w);

        vec3 faceNormal;
        float t = rayAABBIntersect(origin, dir, blockCenter, 0.5, faceNormal);
        float tWorld = t * dirLen;
        if (t > 0.0 && tWorld < closestT) {
            vec3 hitPos = origin + dir * t;
            vec3 localHit = hitPos - (blockCenter - vec3(0.5));

            vec2 uv;
            uint stepped_axis;
            if (abs(faceNormal.x) > 0.5) {
                uv = vec2(localHit.z, 1.0 - localHit.y);
                stepped_axis = 0;
            } else if (abs(faceNormal.y) > 0.5) {
                uv = vec2(localHit.x, localHit.z);
                stepped_axis = 1;
            } else {
                uv = vec2(localHit.x, 1.0 - localHit.y);
                stepped_axis = 2;
            }

            vec3 texColor = sampleTexture(blockType, uv);

            vec3 sunDir = getCurrentSunDir();
            float daylight = getDaylightFactor(pc.time_of_day);
            float sunLight = max(0.0, dot(faceNormal, sunDir));
            float baseAmbient = mix(pc.ambient_light, pc.ambient_light + 0.25, daylight);
            float ambient = baseAmbient;
            float shadow = castShadowRay(hitPos + faceNormal * 0.01);
            float direct = mix(0.2, 0.65, daylight) * sunLight * shadow;
            float lighting = ambient + direct;

            closestT = tWorld;
            closestColor = texColor * lighting;
            anyHit = true;
        }
    }

    if (anyHit) {
        color = closestColor;
        hitDistance = closestT;
    }

    return anyHit;
}

// Render block placement preview
bool renderPreviewBlock(vec3 origin, vec3 dir, inout vec3 color, float sceneHitDistance) {
    if (!hasPreviewBlock()) return false;

    ivec3 previewPos = ivec3(pc.preview_block_x, pc.preview_block_y, pc.preview_block_z);

    vec3 hitNormal;
    vec3 localHit;
    float t = rayBlockIntersect(origin, dir, previewPos, hitNormal, localHit);

    float tWorld = t * length(dir);
    if (t < 0.0 || tWorld > sceneHitDistance) {
        return false;
    }

    uint steppedAxis = 0;
    if (abs(hitNormal.y) > 0.5) steppedAxis = 1;
    else if (abs(hitNormal.z) > 0.5) steppedAxis = 2;

    vec3 hitPoint = origin + dir * t;
    vec3 previewColor = getBlockColor(pc.preview_block_type, localHit, hitNormal, steppedAxis, hitPoint, 0u);

    vec3 sunDir = getCurrentSunDir();
    float daylight = getDaylightFactor(pc.time_of_day);
    float sunLight = max(0.0, dot(hitNormal, sunDir));
    float ambient = mix(0.3, 0.5, daylight);
    float direct = mix(0.2, 0.5, daylight) * sunLight;
    previewColor *= (ambient + direct);

    float wireframe = getWireframeFactor(localHit, hitNormal);
    float baseAlpha = 0.75;
    float wireAlpha = 0.95;
    vec3 wireColor = vec3(0.8, 1.0, 1.0);

    vec3 finalPreview = mix(previewColor, wireColor, wireframe);
    float finalAlpha = mix(baseAlpha, wireAlpha, wireframe);
    float pulse = 0.9 + 0.1 * sin(pc.animation_time * 4.0);
    finalAlpha *= pulse;

    color = mix(color, finalPreview, finalAlpha);
    return true;
}

// Render water/lava source debug markers
bool renderWaterSourceMarkers(vec3 origin, vec3 dir, inout vec3 color, float sceneHitDistance) {
    if (pc.show_water_sources == 0 || pc.water_source_count == 0) {
        return false;
    }

    bool anyHit = false;
    float dirLen = length(dir);
    if (dirLen < 1e-6) return false;

    for (uint i = 0; i < pc.water_source_count; i++) {
        WaterSource src = water_sources[i];
        ivec3 sourcePos = ivec3(src.position.xyz);
        float sourceType = src.position.w; // 0=water, 1=lava

        vec3 boxMin = vec3(sourcePos);
        vec3 boxMax = vec3(sourcePos) + vec3(1.0);

        vec3 hitNormal;
        float tHit;
        if (!rayBoxHit(origin, dir, boxMin, boxMax, tHit, hitNormal)) {
            continue;
        }

        float tWorld = tHit * dirLen;
        if (tHit < 0.0 || tWorld > sceneHitDistance + 0.5) {
            continue;
        }

        // Compute local hit position
        vec3 hitPoint = origin + dir * tHit;
        vec3 localHit = hitPoint - boxMin;
        localHit = clamp(localHit, vec3(0.0), vec3(1.0));

        float wireframe = getWireframeFactor(localHit, hitNormal);
        if (wireframe > 0.1) {
            // Water sources = blue, Lava sources = orange
            vec3 outlineColor = sourceType < 0.5 ? vec3(0.2, 0.5, 1.0) : vec3(1.0, 0.5, 0.1);
            // Pulsing effect to make them more visible
            float pulse = 0.7 + 0.3 * sin(pc.animation_time * 3.0 + float(i) * 0.5);
            float outlineAlpha = wireframe * 0.9 * pulse;
            color = mix(color, outlineColor, outlineAlpha);
            anyHit = true;
        }
    }

    return anyHit;
}

// Render target block outline (wireframe only)
bool renderTargetBlockOutline(vec3 origin, vec3 dir, inout vec3 color, float sceneHitDistance) {
    if (!hasTargetBlock()) {
        return false;
    }

    ivec3 targetPos = ivec3(pc.target_block_x, pc.target_block_y, pc.target_block_z);
    uint blockType = readBlockTypeAtTexCoord(targetPos);

    // Define bounding box for outline
    vec3 boxMin = vec3(targetPos);
    vec3 boxMax = vec3(targetPos) + vec3(1.0);

    // Handle multi-block models (doors)
    if (blockType == BLOCK_MODEL) {
        uvec2 metadata = readModelMetadata(targetPos);
        uint model_id = metadata.r;

        if (isDoorModel(model_id)) {
            // Check if this is upper or lower half
            bool isUpper = isDoorUpper(model_id);
            ivec3 otherHalf = targetPos + ivec3(0, isUpper ? -1 : 1, 0);

            // Verify the other half exists and is also a door
            if (otherHalf.y >= 0 && otherHalf.y < int(pc.texture_size_y)) {
                uint otherBlockType = readBlockTypeAtTexCoord(otherHalf);
                if (otherBlockType == BLOCK_MODEL) {
                    uvec2 otherMetadata = readModelMetadata(otherHalf);
                    uint otherModelId = otherMetadata.r;
                    if (isDoorModel(otherModelId)) {
                        // Extend bounding box to encompass both halves
                        boxMin.y = float(min(targetPos.y, otherHalf.y));
                        boxMax.y = float(max(targetPos.y, otherHalf.y)) + 1.0;
                    }
                }
            }
        }
    }

    // Ray-box intersection
    vec3 hitNormal;
    float tHit;
    if (!rayBoxHit(origin, dir, boxMin, boxMax, tHit, hitNormal)) {
        return false;
    }

    float tWorld = tHit * length(dir);
    if (tHit < 0.0) {
        return false;
    }

    // For sub-voxel models, render outline even if ray passes through empty space
    // For solid blocks, only render if we're looking at the target block
    bool shouldRender = false;
    if (blockType == BLOCK_MODEL) {
        // Always render outline for models within reasonable distance
        shouldRender = (tWorld < sceneHitDistance + 2.0);
    } else {
        // For solid blocks, only render if this is what we hit
        shouldRender = (abs(tWorld - sceneHitDistance) < 0.1);
    }

    if (!shouldRender) {
        return false;
    }

    // Compute local hit position within the bounding box
    vec3 hitPoint = origin + dir * tHit;
    vec3 localHit = (hitPoint - boxMin) / (boxMax - boxMin);
    localHit = clamp(localHit, vec3(0.0), vec3(1.0));

    float wireframe = getWireframeFactor(localHit, hitNormal);
    if (wireframe > 0.1) {
        vec3 outlineColor = vec3(0.0, 1.0, 1.0); // cyan outline
        float outlineAlpha = wireframe * 0.8;
        color = mix(color, outlineColor, outlineAlpha);
    }

    return true;
}

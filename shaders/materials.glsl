// Material sampling, AO, and surface helpers

// Texture atlas constants
// Slots 0-16: standard blocks (Air through Bedrock)
// Slot 17: grass_side, Slot 18: log_top
// Slots 19-22: emissive blocks (Lava, GlowStone, GlowMushroom, Crystal)
// Slots 23-28: new textures (Cactus, Mud, Sandstone, Ice, PineLeaves, DecorativeStone)
const float ATLAS_TILE_COUNT = 29.0;
const float ATLAS_TILE_SIZE = 1.0 / ATLAS_TILE_COUNT;
const uint TEX_GRASS_SIDE = 17;
const uint TEX_LOG_TOP = 18;
const uint TEX_LAVA = 19;
const uint TEX_GLOWSTONE = 20;
const uint TEX_GLOWMUSHROOM = 21;
const uint TEX_CRYSTAL = 22;
const uint TEX_CACTUS = 23;
const uint TEX_MUD = 24;
const uint TEX_SANDSTONE = 25;
const uint TEX_ICE = 26;
const uint TEX_PINE_LEAVES = 27;
const uint TEX_DECORATIVESTONE = 28;

// Map BlockType enum values to texture atlas positions
// This is needed because enum values don't directly correspond to atlas positions
uint blockTypeToAtlasIndex(uint blockType) {
    // Blocks 0-16 map directly (Air through Bedrock)
    if (blockType <= BLOCK_BEDROCK) {
        return blockType;
    }

    // Special cases for blocks that share textures or have no texture
    switch (blockType) {
        case BLOCK_TINTED_GLASS:
            return BLOCK_GLASS;  // Use glass texture
        case BLOCK_PAINTED:
            return BLOCK_STONE;  // Painted blocks use paint_data for texture selection
        case BLOCK_LAVA:
            return TEX_LAVA;
        case BLOCK_GLOWSTONE:
            return TEX_GLOWSTONE;
        case BLOCK_GLOWMUSHROOM:
            return TEX_GLOWMUSHROOM;
        case BLOCK_CRYSTAL:
            return TEX_CRYSTAL;
        case BLOCK_PINE_LOG:
        case BLOCK_WILLOW_LOG:
            return BLOCK_LOG;  // All logs share log texture
        case BLOCK_PINE_LEAVES:
            return TEX_PINE_LEAVES;  // Pine leaves have dark green needle texture
        case BLOCK_WILLOW_LEAVES:
            return BLOCK_LEAVES;  // Willow leaves use regular leaves texture
        case BLOCK_ICE:
            return TEX_ICE;
        case BLOCK_MUD:
            return TEX_MUD;
        case BLOCK_SANDSTONE:
            return TEX_SANDSTONE;
        case BLOCK_CACTUS:
            return TEX_CACTUS;
        case BLOCK_DECORATIVESTONE:
            return TEX_DECORATIVESTONE;
        default:
            return BLOCK_AIR;  // Fallback for unknown blocks
    }
}

// Check if a block type is emissive
bool isEmissiveBlock(uint blockType) {
    return blockType == BLOCK_LAVA ||
           blockType == BLOCK_GLOWSTONE ||
           blockType == BLOCK_GLOWMUSHROOM ||
           blockType == BLOCK_CRYSTAL;
}

// Get emission color for an emissive block
// For Crystal blocks, the tintIndex can be used to override the emission color
vec3 getEmissionColor(uint blockType, uint tintIndex) {
    switch (blockType) {
        case BLOCK_LAVA:
            return EMISSION_LAVA;
        case BLOCK_GLOWSTONE:
            return EMISSION_GLOWSTONE;
        case BLOCK_GLOWMUSHROOM:
            return EMISSION_GLOWMUSHROOM;
        case BLOCK_CRYSTAL:
            // Crystal uses tint palette for colored crystals
            if (tintIndex < 32u) {
                return TINT_PALETTE[tintIndex];
            }
            return EMISSION_CRYSTAL;
        default:
            return vec3(0.0);
    }
}

// Get emission strength for an emissive block
float getEmissionStrength(uint blockType) {
    switch (blockType) {
        case BLOCK_LAVA:
            return EMISSION_STRENGTH_LAVA;
        case BLOCK_GLOWSTONE:
            return EMISSION_STRENGTH_GLOWSTONE;
        case BLOCK_GLOWMUSHROOM:
            return EMISSION_STRENGTH_GLOWMUSHROOM;
        case BLOCK_CRYSTAL:
            return EMISSION_STRENGTH_CRYSTAL;
        default:
            return 0.0;
    }
}

// Water rendering settings
const vec3 WATER_COLOR = vec3(0.2, 0.5, 0.8);
const vec3 WATER_DEEP_COLOR = vec3(0.05, 0.15, 0.3);
const float WATER_CLARITY = 8.0;
const float WATER_REFLECTIVITY = 0.3;
const float WATER_FRESNEL_POWER = 3.0;

// Water type definitions (colors)
// Ocean: Deep blue
const vec3 WATER_TINT_OCEAN = vec3(1.0, 1.0, 1.0); // Base color
// Lake: Blue-green
const vec3 WATER_TINT_LAKE = vec3(0.7, 1.0, 0.9);
// River: Light blue
const vec3 WATER_TINT_RIVER = vec3(0.9, 1.0, 1.1);
// Swamp: Murky green-brown
const vec3 WATER_TINT_SWAMP = vec3(0.6, 0.7, 0.4);
// Spring: Crystal clear
const vec3 WATER_TINT_SPRING = vec3(1.0, 1.1, 1.2);

// Get tint color for a water type
vec3 getWaterTintFn(uint waterType) {
    if (waterType == WATER_TYPE_LAKE) return WATER_TINT_LAKE;
    if (waterType == WATER_TYPE_RIVER) return WATER_TINT_RIVER;
    if (waterType == WATER_TYPE_SWAMP) return WATER_TINT_SWAMP;
    if (waterType == WATER_TYPE_SPRING) return WATER_TINT_SPRING;
    return WATER_TINT_OCEAN;
}

// Get clarity for a water type
float getWaterClarity(uint waterType) {
    if (waterType == WATER_TYPE_SWAMP) return 3.0; // Murky
    if (waterType == WATER_TYPE_SPRING) return 12.0; // Clear
    return WATER_CLARITY;
}

// Sample a specific texture from the atlas
vec3 sampleTexture(uint textureIndex, vec2 uv) {
    float atlasU = (float(textureIndex) + fract(uv.x)) * ATLAS_TILE_SIZE;
    float atlasV = fract(uv.y);
    return texture(textureAtlas, vec2(atlasU, atlasV)).rgb;
}

// Lightweight 3-octave FBM (shared for water)
float fbmWater(vec2 p) {
    float v = 0.0;
    float a = 0.55;
    float f = 1.0;
    for (int i = 0; i < 3; i++) {
        v += a * noise2D(p * f);
        f *= 2.0;
        a *= 0.5;
    }
    return v;
}

// Reusable 2D flow vector from two FBM samples
vec2 fbmWaterFlow(vec2 p) {
    return vec2(fbmWater(p), fbmWater(p + vec2(23.17, -11.31))) - vec2(0.5);
}

vec2 getWaterUVAnimation(vec2 uv, vec3 texPos, float time) {
    vec3 worldPos = texPos + vec3(textureOrigin());
    float t = time * 0.55;
    vec2 base = worldPos.xz * 0.5 + vec2(t * 0.18, -t * 0.14);
    vec2 flow = fbmWaterFlow(base) * 0.08;
    return uv + flow;
}

// Get caustic light pattern for underwater surfaces
float getWaterCaustics(vec3 texPos, float time) {
    vec3 worldPos = texPos + vec3(textureOrigin());
    float t = time * 0.8;
    vec2 pos = worldPos.xz;

    float c = fbmWater(pos * 1.4 + vec2(t * 0.22, t * 0.16));
    c = smoothstep(0.4, 0.65, c);

    return c * 0.3;
}

// Get block color by sampling from texture atlas, with multi-face support
vec3 getBlockColor(uint blockType, vec3 local_hit, vec3 normal, uint stepped_axis, vec3 worldPos, uint extraData) {
    uint textureIndex = blockTypeToAtlasIndex(blockType);
    vec2 uv;

    if (stepped_axis == 0) {
        uv = vec2(local_hit.z, 1.0 - local_hit.y);
    } else if (stepped_axis == 1) {
        uv = vec2(local_hit.x, local_hit.z);
    } else {
        uv = vec2(local_hit.x, 1.0 - local_hit.y);
    }

    if (blockType == BLOCK_GRASS) {
        if (normal.y > 0.5) {
            textureIndex = BLOCK_GRASS;
        } else if (normal.y < -0.5) {
            textureIndex = BLOCK_DIRT;
        } else {
            vec3 dirtColor = sampleTexture(BLOCK_DIRT, uv);
            vec3 grassColor = sampleTexture(BLOCK_GRASS, uv);
            float grassEdge = smoothstep(0.75, 0.95, local_hit.y);
            return mix(dirtColor, grassColor, grassEdge);
        }
    } else if (blockType == BLOCK_LOG || blockType == BLOCK_PINE_LOG || blockType == BLOCK_WILLOW_LOG) {
        // Always use bark texture on all sides for natural branch appearance
        textureIndex = BLOCK_LOG;
    } else if (blockType == BLOCK_WILLOW_LEAVES) {
        // Willow leaves use regular leaves texture (pine leaves have their own texture)
        textureIndex = BLOCK_LEAVES;
    } else if (blockType == BLOCK_WATER) {
        vec2 animatedUV = getWaterUVAnimation(uv, worldPos, pc.animation_time);
        vec3 waterColor = sampleTexture(BLOCK_WATER, animatedUV);
        if (normal.y > 0.5) {
            float caustics = getWaterCaustics(worldPos, pc.animation_time);
            waterColor += vec3(caustics);
        }
        // Apply water tint based on type
        return waterColor * getWaterTintFn(extraData);
    } else if (blockType == BLOCK_LAVA) {
        // Animated lava surface with slow flow
        float t = pc.animation_time * 0.3;
        vec2 animatedUV = uv + vec2(t * 0.1, -t * 0.05);
        animatedUV += fbmWaterFlow(worldPos.xz * 0.3 + vec2(t * 0.1)) * 0.1;
        vec3 lavaColor = sampleTexture(TEX_LAVA, animatedUV);
        // Add bright veins
        float veins = fbmWater(worldPos.xz * 2.0 + vec2(t * 0.2, t * 0.15));
        veins = smoothstep(0.4, 0.6, veins);
        lavaColor = mix(lavaColor, vec3(1.0, 0.8, 0.3), veins * 0.4);
        return lavaColor;
    } else if (blockType == BLOCK_ICE) {
        textureIndex = TEX_ICE;
    }

    return sampleTexture(textureIndex, uv);
}

const float WAVE_SPEED = 1.2;
const float WAVE_SCALE = 0.8;
const float WAVE_AMPLITUDE = 0.15;
const float WAVE_NORMAL_STRENGTH = 0.35;

float waveHeight(vec2 p) {
    return fbmWater(p) * 2.0 - 1.0;
}

vec3 getWaterWaveNormal(vec3 texPos, float time) {
    vec3 worldPos = texPos + vec3(textureOrigin());
    vec2 pos = worldPos.xz;
    float t = time * WAVE_SPEED;

    float h = waveHeight(pos * WAVE_SCALE + vec2(t * 0.25, t * 0.2));
    float delta = 0.12;
    float hpx = waveHeight((pos + vec2(delta, 0.0)) * WAVE_SCALE + vec2(t * 0.25, t * 0.2));
    float hpz = waveHeight((pos + vec2(0.0, delta)) * WAVE_SCALE + vec2(t * 0.25, t * 0.2));

    float dx = (hpx - h) / delta;
    float dz = (hpz - h) / delta;

    vec3 waveNormal = normalize(vec3(
        -dx * WAVE_AMPLITUDE * WAVE_NORMAL_STRENGTH,
        1.0,
        -dz * WAVE_AMPLITUDE * WAVE_NORMAL_STRENGTH
    ));

    return waveNormal;
}

// Crack pattern for block breaking
float getCrackPattern(vec2 uv, float progress) {
    if (progress <= 0.0) return 0.0;

    vec2 crackUV = uv * 4.0;
    float n1 = noise2D(crackUV * 2.0);
    float n2 = noise2D(crackUV * 4.0 + 1.23);
    float n3 = noise2D(crackUV * 8.0 + 4.56);

    float noise = n1 * 0.5 + n2 * 0.3 + n3 * 0.2;
    float crackLines = 1.0 - abs(noise * 2.0 - 1.0);
    crackLines = pow(crackLines, 2.0);

    float threshold = 1.0 - progress;
    float crack = smoothstep(threshold, threshold + 0.3, crackLines);

    if (progress > 0.7) {
        float chunks = noise2D(crackUV * 1.5 + 7.89);
        chunks = step(0.6 + (1.0 - progress) * 0.5, chunks);
        crack = max(crack, chunks * (progress - 0.7) / 0.3);
    }

    return crack * progress;
}

// Ambient occlusion for a face
float calculateAO(ivec3 blockCoord, vec3 normal, vec2 uv) {
    ivec3 inormal = ivec3(normal);
    ivec3 facePos = blockCoord + inormal;

    // Early exit: if the chunk containing the face is empty, no AO needed
    ivec3 chunkPos = facePos / int(CHUNK_SIZE);
    if (isChunkEmpty(chunkPos)) {
        return 1.0;  // No occlusion in empty chunk
    }

    ivec3 tangent1, tangent2;

    if (abs(inormal.x) > 0) {
        tangent1 = ivec3(0, 1, 0);
        tangent2 = ivec3(0, 0, 1);
    } else if (abs(inormal.y) > 0) {
        tangent1 = ivec3(1, 0, 0);
        tangent2 = ivec3(0, 0, 1);
    } else {
        tangent1 = ivec3(1, 0, 0);
        tangent2 = ivec3(0, 1, 0);
    }

    float ao[4];
    for (int i = 0; i < 4; i++) {
        int s1 = (i & 1) * 2 - 1;
        int s2 = ((i >> 1) & 1) * 2 - 1;

        bool side1 = isOccluderSafe(facePos + tangent1 * s1);
        bool side2 = isOccluderSafe(facePos + tangent2 * s2);
        bool corner = isOccluderSafe(facePos + tangent1 * s1 + tangent2 * s2);

        if (side1 && side2) {
            ao[i] = 0.0;
        } else {
            ao[i] = 1.0 - float(int(side1) + int(side2) + int(corner)) / 3.0;
        }
    }

    float u = uv.x;
    float v = uv.y;

    float ao_bottom = mix(ao[0], ao[1], u);
    float ao_top = mix(ao[2], ao[3], u);
    float ao_final = mix(ao_bottom, ao_top, v);

    return 0.3 + ao_final * 0.7;
}

// Check if a block position is at a chunk boundary edge
float chunkBoundaryFactor(vec3 hitPos, uint steppedAxis) {
    vec3 chunkLocal = mod(hitPos, float(CHUNK_SIZE));
    float edgeThreshold = 0.1;
    float factor = 0.0;

    for (int i = 0; i < 3; i++) {
        if (i == int(steppedAxis)) continue;

        float localCoord = chunkLocal[i];
        if (localCoord < edgeThreshold || localCoord > float(CHUNK_SIZE) - edgeThreshold) {
            factor = 1.0;
        }
    }

    return factor;
}

// Material sampling, AO, and surface helpers

// Texture atlas constants
const float ATLAS_TILE_COUNT = 19.0;
const float ATLAS_TILE_SIZE = 1.0 / ATLAS_TILE_COUNT;
const uint TEX_GRASS_SIDE = 17;
const uint TEX_LOG_TOP = 18;

// Sample a specific texture from the atlas
vec3 sampleTexture(uint textureIndex, vec2 uv) {
    float atlasU = (float(textureIndex) + fract(uv.x)) * ATLAS_TILE_SIZE;
    float atlasV = fract(uv.y);
    return texture(textureAtlas, vec2(atlasU, atlasV)).rgb;
}

// Get animated water UV distortion
// Lightweight 3-octave FBM
float fbmWater(vec2 p) {
    float v = 0.0;
    float a = 0.5;
    float f = 1.0;
    for (int i = 0; i < 3; i++) {
        v += a * noise2D(p * f);
        f *= 2.0;
        a *= 0.55;
    }
    return v;
}

vec2 getWaterUVAnimation(vec2 uv, vec3 texPos, float time) {
    vec3 worldPos = texPos + vec3(textureOrigin());
    float t = time * 0.6;
    vec2 base = worldPos.xz * 0.5 + vec2(t * 0.2, -t * 0.15);
    float flow = fbmWater(base) - 0.5;
    float flow2 = fbmWater(base * 1.7 + vec2(37.0, -19.0)) - 0.5;
    return uv + vec2(flow, flow2) * 0.08;
}

// Get caustic light pattern for underwater surfaces
float getWaterCaustics(vec3 texPos, float time) {
    vec3 worldPos = texPos + vec3(textureOrigin());
    float t = time * 0.9;
    vec2 pos = worldPos.xz;

    float c1 = fbmWater(pos * 1.2 + vec2(t * 0.25, t * 0.18));
    float c2 = fbmWater(pos * 2.1 + vec2(-t * 0.3, t * 0.32) + 30.0);

    float caustic = c1 * c2;
    caustic = smoothstep(0.35, 0.65, caustic);

    return caustic * 0.3;
}

// Get block color by sampling from texture atlas, with multi-face support
vec3 getBlockColor(uint blockType, vec3 local_hit, vec3 normal, uint stepped_axis, vec3 worldPos) {
    uint textureIndex = blockType;
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
    } else if (blockType == BLOCK_LOG) {
        if (abs(normal.y) > 0.5) {
            textureIndex = TEX_LOG_TOP;
        } else {
            textureIndex = BLOCK_LOG;
        }
    } else if (blockType == BLOCK_WATER) {
        vec2 animatedUV = getWaterUVAnimation(uv, worldPos, pc.animation_time);
        vec3 waterColor = sampleTexture(BLOCK_WATER, animatedUV);
        if (normal.y > 0.5) {
            float caustics = getWaterCaustics(worldPos, pc.animation_time);
            waterColor += vec3(caustics);
        }
        return waterColor;
    }

    return sampleTexture(textureIndex, uv);
}

// Water rendering settings
const vec3 WATER_COLOR = vec3(0.2, 0.5, 0.8);
const vec3 WATER_DEEP_COLOR = vec3(0.05, 0.15, 0.3);
const float WATER_CLARITY = 8.0;
const float WATER_REFLECTIVITY = 0.3;
const float WATER_FRESNEL_POWER = 3.0;

const float WAVE_SPEED = 1.2;
const float WAVE_SCALE = 0.8;
const float WAVE_AMPLITUDE = 0.15;
const float WAVE_NORMAL_STRENGTH = 0.4;

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

    ivec3 facePos = blockCoord + inormal;

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

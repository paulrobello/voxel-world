// Shared constants, layouts, and resources for traverse.comp

// Render modes (must match RenderMode enum in main.rs)
const uint COORD = 0;
const uint STEPS = 1;
const uint TEXTURED = 2;
const uint NORMAL = 3;
const uint UV = 4;
const uint DEPTH = 5;
const uint BRICK_DEBUG = 6;
const uint SHADOW_DEBUG = 7;

// Block types (must match chunk.rs BlockType enum)
const uint BLOCK_AIR = 0;
const uint BLOCK_STONE = 1;
const uint BLOCK_DIRT = 2;
const uint BLOCK_GRASS = 3;
const uint BLOCK_PLANKS = 4;
const uint BLOCK_LEAVES = 5;
const uint BLOCK_SAND = 6;
const uint BLOCK_GRAVEL = 7;
const uint BLOCK_WATER = 8;
const uint BLOCK_GLASS = 9;
const uint BLOCK_LOG = 10;
const uint BLOCK_MODEL = 11;
const uint BLOCK_BRICK = 12;
const uint BLOCK_SNOW = 13;
const uint BLOCK_COBBLESTONE = 14;
const uint BLOCK_IRON = 15;
const uint BLOCK_BEDROCK = 16;
const uint BLOCK_TINTED_GLASS = 17;
const uint BLOCK_PAINTED = 18;
const uint BLOCK_LAVA = 19;
const uint BLOCK_GLOWSTONE = 20;
const uint BLOCK_GLOWMUSHROOM = 21;
const uint BLOCK_CRYSTAL = 22;
const uint BLOCK_PINE_LOG = 23;
const uint BLOCK_WILLOW_LOG = 24;
const uint BLOCK_PINE_LEAVES = 25;
const uint BLOCK_WILLOW_LEAVES = 26;

// Sub-voxel model IDs (must match Rust ModelRegistry IDs)
const uint CRYSTAL_MODEL_ID = 99u;

// Water types (must match Rust WaterType enum)
const uint WATER_TYPE_OCEAN = 0u;
const uint WATER_TYPE_LAKE = 1u;
const uint WATER_TYPE_RIVER = 2u;
const uint WATER_TYPE_SWAMP = 3u;
const uint WATER_TYPE_SPRING = 4u;

// Emission colors for emissive blocks (RGB)
const vec3 EMISSION_LAVA = vec3(1.0, 0.4, 0.1);        // Orange-red
const vec3 EMISSION_GLOWSTONE = vec3(1.0, 0.95, 0.8);  // Warm white
const vec3 EMISSION_GLOWMUSHROOM = vec3(0.3, 0.9, 1.0); // Cyan
const vec3 EMISSION_CRYSTAL = vec3(0.8, 0.8, 1.0);     // White-blue (default, tint overrides)

// Emission strengths (0-1)
const float EMISSION_STRENGTH_LAVA = 0.9;
const float EMISSION_STRENGTH_GLOWSTONE = 1.0;
const float EMISSION_STRENGTH_GLOWMUSHROOM = 0.6;
const float EMISSION_STRENGTH_CRYSTAL = 0.7;

// Tint palette for tinted glass (32 colors)
const vec3 TINT_PALETTE[32] = vec3[32](
    vec3(1.0, 0.2, 0.2),   // 0: Red
    vec3(1.0, 0.5, 0.2),   // 1: Orange
    vec3(1.0, 1.0, 0.2),   // 2: Yellow
    vec3(0.5, 1.0, 0.2),   // 3: Lime
    vec3(0.2, 1.0, 0.2),   // 4: Green
    vec3(0.2, 1.0, 0.5),   // 5: Teal
    vec3(0.2, 1.0, 1.0),   // 6: Cyan
    vec3(0.2, 0.5, 1.0),   // 7: Sky blue
    vec3(0.2, 0.2, 1.0),   // 8: Blue
    vec3(0.5, 0.2, 1.0),   // 9: Purple
    vec3(1.0, 0.2, 1.0),   // 10: Magenta
    vec3(1.0, 0.2, 0.5),   // 11: Pink
    vec3(0.95, 0.95, 0.95),// 12: White
    vec3(0.6, 0.6, 0.6),   // 13: Light gray
    vec3(0.3, 0.3, 0.3),   // 14: Dark gray
    vec3(0.4, 0.25, 0.1),  // 15: Brown
    vec3(0.8, 0.4, 0.4),   // 16: Light red
    vec3(0.8, 0.6, 0.4),   // 17: Peach
    vec3(0.8, 0.8, 0.4),   // 18: Light yellow
    vec3(0.6, 0.8, 0.4),   // 19: Light lime
    vec3(0.4, 0.8, 0.4),   // 20: Light green
    vec3(0.4, 0.8, 0.6),   // 21: Light teal
    vec3(0.4, 0.8, 0.8),   // 22: Light cyan
    vec3(0.4, 0.6, 0.8),   // 23: Light sky
    vec3(0.4, 0.4, 0.8),   // 24: Light blue
    vec3(0.6, 0.4, 0.8),   // 25: Light purple
    vec3(0.8, 0.4, 0.8),   // 26: Light magenta
    vec3(0.8, 0.4, 0.6),   // 27: Light pink
    vec3(0.2, 0.15, 0.1),  // 28: Dark brown
    vec3(0.1, 0.2, 0.1),   // 29: Dark green
    vec3(0.1, 0.1, 0.2),   // 30: Dark blue
    vec3(0.2, 0.1, 0.2)    // 31: Dark purple
);

// World/chunk sizes (must mirror Rust)
const uint CHUNK_SIZE = 32;
const uint CHUNKS_X = 16;
const uint CHUNKS_Y = 8;  // Must match WORLD_CHUNKS_Y in constants.rs (8 × 32 = 256 blocks)
const uint CHUNKS_Z = 16;

// Brick metadata
const uint BRICK_SIZE = 8;
const uint BRICKS_PER_AXIS = 4;
const uint BRICKS_PER_CHUNK = 64;

// Numeric constants
const float FLT_MAX = 3.4028235e+38;

// Push constants
layout(push_constant) uniform PushConstants {
    mat4 pixelToRay;
    uint texture_size_x;
    uint texture_size_y;
    uint texture_size_z;
    uint render_mode;
    uint show_chunk_boundaries;
    uint player_in_water;
    float time_of_day;
    float animation_time;
    float cloud_speed;
    float cloud_coverage;
    float cloud_color_r;
    float cloud_color_g;
    float cloud_color_b;
    uint clouds_enabled;
    int break_block_x;
    int break_block_y;
    int break_block_z;
    float break_progress;
    uint particle_count;
    int preview_block_x;
    int preview_block_y;
    int preview_block_z;
    uint preview_block_type;
    uint light_count;
    float ambient_light;
    float fog_density;
    float fog_start;
    float fog_overlay_scale;
    int target_block_x;
    int target_block_y;
    int target_block_z;
    uint max_ray_steps;
    uint shadow_max_steps;
    int texture_origin_x;
    int texture_origin_y;
    int texture_origin_z;
    uint enable_ao;
    uint enable_shadows;
    uint enable_model_shadows;
    uint enable_point_lights;
    uint enable_tinted_shadows;
    uint transparent_background;
    uint pass_mode;
    float lod_ao_distance;
    float lod_shadow_distance;
    float lod_point_light_distance;
    float lod_model_distance;
    uint falling_block_count;
    uint show_water_sources;
    uint water_source_count;
    uint template_block_count;
    int template_preview_min_x;
    int template_preview_min_y;
    int template_preview_min_z;
    int template_preview_max_x;
    int template_preview_max_y;
    int template_preview_max_z;
    vec4 camera_pos; // world-space camera position (already 16-byte aligned at offset 272)
    int selection_pos1_x;
    int selection_pos1_y;
    int selection_pos1_z;
    int selection_pos2_x;
    int selection_pos2_y;
    int selection_pos2_z;
} pc;

// Particles (set 3)
struct Particle {
    vec4 pos_size;     // xyz = position, w = size
    vec4 color_alpha;  // rgb = color, a = alpha
};
layout(set = 3, binding = 0) readonly buffer ParticleBuffer {
    Particle particles[];
};

// Falling blocks share the particle set
struct FallingBlock {
    vec4 pos_type;      // xyz = center, w = block type
    vec4 velocity_age;  // xyz unused, w = age
};
layout(set = 3, binding = 1) readonly buffer FallingBlockBuffer {
    FallingBlock falling_blocks[];
};

// Water source positions for debug visualization (shares particle set)
struct WaterSource {
    vec4 position;  // xyz = block position, w = type (0=water, 1=lava)
};
layout(set = 3, binding = 2) readonly buffer WaterSourceBuffer {
    WaterSource water_sources[];
};

// Template preview block positions (shares particle set)
struct TemplateBlock {
    vec4 position;  // xyz = block world position, w = unused
};
layout(set = 3, binding = 3) readonly buffer TemplateBlockBuffer {
    TemplateBlock template_blocks[];
};

// Point lights (set 4)
struct PointLight {
    vec4 pos_radius;   // xyz = position, w = radius
    vec4 color;        // rgb = color, a = intensity
};
layout(set = 4, binding = 0) readonly buffer LightBuffer {
    PointLight lights[];
};

// Chunk empty flags (set 5)
layout(set = 5, binding = 0) readonly buffer ChunkMetadata {
    uint chunk_flags[];
};

// Two-pass distance buffer (set 6)
layout(set = 6, binding = 0, r32f) uniform image2D distanceImage;

// Brick metadata (set 7)
layout(set = 7, binding = 0) readonly buffer BrickMasks { uint brick_masks[]; };
layout(set = 7, binding = 1) readonly buffer BrickDistances { uint brick_distances[]; };

// Sub-voxel models (set 7, bindings 2-6)
// Single 16³ atlas - all models resampled to this resolution for performance
layout(set = 7, binding = 2, r8ui) readonly uniform uimage3D modelAtlas;
layout(set = 7, binding = 3) uniform sampler2D modelPalettes;
layout(set = 7, binding = 4, rg8ui) readonly uniform uimage3D modelMetadata;
struct ModelProperties {
    uvec2 collision_mask;   // 8 bytes - 4×4×4 collision grid
    uint aabb_min;          // 4 bytes - packed xyz
    uint aabb_max;          // 4 bytes - packed xyz
    vec4 emission;          // 16 bytes - RGB + intensity
    uint flags;             // 4 bytes - rotatable, light_blocking, is_light_source, light_mode
    uint resolution;        // 4 bytes - 8, 16, or 32
    float light_radius;     // 4 bytes - light radius in blocks
    float light_intensity;  // 4 bytes - light intensity multiplier
};
layout(set = 7, binding = 5) readonly buffer ModelPropertiesBuffer {
    ModelProperties model_properties[];
};
layout(set = 7, binding = 6) uniform sampler2D modelPaletteEmission;

// Sub-voxel constants
// Default resolution (medium) - for backward compatibility
const uint SUB_VOXEL_SIZE = 16;
const float SUB_VOXEL_SCALE = 1.0 / float(SUB_VOXEL_SIZE);
const float SUB_VOXEL_EPS = 1e-3;
const int SUB_VOXEL_MAX_STEPS = int(SUB_VOXEL_SIZE) * 3;  // Max DDA steps through model

// Model flags (must match Rust pack_properties_for_gpu)
const uint MODEL_FLAG_ROTATABLE = 1u << 0;
const uint MODEL_FLAG_LIGHT_BLOCK_PARTIAL = 1u << 1;
const uint MODEL_FLAG_LIGHT_BLOCK_FULL = 1u << 2;
const uint MODEL_FLAG_IS_LIGHT_SOURCE = 1u << 3;
// Light mode is in bits 4-7: (flags >> 4) & 0xF

// Light modes (must match Rust LightMode enum)
const uint LIGHT_MODE_STEADY = 0u;
const uint LIGHT_MODE_PULSE = 1u;
const uint LIGHT_MODE_FLICKER = 2u;
const uint LIGHT_MODE_CANDLE = 3u;
const uint LIGHT_MODE_STROBE = 4u;
const uint LIGHT_MODE_BREATHE = 5u;
const uint LIGHT_MODE_SPARKLE = 6u;
const uint LIGHT_MODE_WAVE = 7u;
const uint LIGHT_MODE_WARMUP = 8u;
const uint LIGHT_MODE_ARC = 9u;

const float SUB_VOXEL_LOD_DISTANCE = 32.0;
const float SUB_VOXEL_MIN_DISTANCE = 0.4;

// Images and textures
layout(set = 0, binding = 0, rgba8) writeonly uniform image2D targetImage;
layout(set = 1, binding = 0, r8ui) readonly uniform uimage3D blockImage;
layout(set = 2, binding = 0) uniform sampler2D textureAtlas;

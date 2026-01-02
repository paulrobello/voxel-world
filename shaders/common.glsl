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

// World/chunk sizes (must mirror Rust)
const uint CHUNK_SIZE = 32;
const uint CHUNKS_X = 16;
const uint CHUNKS_Y = 4;
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
    int texture_origin_x;
    int texture_origin_y;
    int texture_origin_z;
    uint enable_ao;
    uint enable_shadows;
    uint enable_model_shadows;
    uint enable_point_lights;
    uint pass_mode;
    float lod_ao_distance;
    float lod_shadow_distance;
    float lod_point_light_distance;
    uint falling_block_count;
    vec4 camera_pos; // world-space camera position
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

// Sub-voxel models (set 7, bindings 2-5)
layout(set = 7, binding = 2, r8ui) readonly uniform uimage3D modelAtlas;
layout(set = 7, binding = 3) uniform sampler2D modelPalettes;
layout(set = 7, binding = 4, rg8ui) readonly uniform uimage3D modelMetadata;
struct ModelProperties {
    uvec2 collision_mask;
    uint aabb_min;
    uint aabb_max;
    vec4 emission;
    uint flags;
    uint _pad0;
    uint _pad1;
    uint _pad2;
};
layout(set = 7, binding = 5) readonly buffer ModelPropertiesBuffer {
    ModelProperties model_properties[];
};

// Sub-voxel constants
const uint SUB_VOXEL_SIZE = 8;
const float SUB_VOXEL_SCALE = 1.0 / float(SUB_VOXEL_SIZE);
const float SUB_VOXEL_EPS = 1e-3;
const uint MODEL_FLAG_ROTATABLE = 1u << 0;
const uint MODEL_FLAG_LIGHT_BLOCK_FULL = 1u << 2;
const uint MODEL_FLAG_LIGHT_BLOCK_PARTIAL = 1u << 1;
const float SUB_VOXEL_LOD_DISTANCE = 32.0;
const float SUB_VOXEL_MIN_DISTANCE = 0.4;

// Images and textures
layout(set = 0, binding = 0, rgba8) writeonly uniform image2D targetImage;
layout(set = 1, binding = 0, r8ui) readonly uniform uimage3D blockImage;
layout(set = 2, binding = 0) uniform sampler2D textureAtlas;

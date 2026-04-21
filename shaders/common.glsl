// Shared constants, layouts, and resources for traverse.comp

// Auto-generated constants: BLOCK_*, ATLAS_TILE_COUNT/ATLAS_TILE_SIZE, RENDER_MODE_*,
// CHUNK_SIZE/CHUNKS_X/Y/Z, BRICK_SIZE/BRICKS_PER_AXIS/BRICKS_PER_CHUNK, TINT_PALETTE.
// Source of truth lives in Rust source files; build.rs regenerates this on change.
#include "generated_constants.glsl"

// Render mode short aliases (GLSL convention used throughout shaders).
// The generated file provides RENDER_MODE_* defines; these map the short names.
#define COORD        RENDER_MODE_COORD
#define STEPS        RENDER_MODE_STEPS
#define TEXTURED     RENDER_MODE_TEXTURED
#define NORMAL       RENDER_MODE_NORMAL
#define UV           RENDER_MODE_UV
#define DEPTH        RENDER_MODE_DEPTH
#define BRICK_DEBUG  RENDER_MODE_BRICK_DEBUG
#define SHADOW_DEBUG RENDER_MODE_SHADOW_DEBUG

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

// TINT_PALETTE, CHUNK_SIZE, CHUNKS_X/Y/Z, BRICK_SIZE, BRICKS_PER_AXIS, and
// BRICKS_PER_CHUNK all come from `generated_constants.glsl` above — edit the
// Rust-side source of truth instead (see build.rs for the parse map).

// Numeric constants
const float FLT_MAX = 3.4028235e+38;

// Push constants
layout(push_constant) uniform PushConstants {
    mat4 pixelToRay;             // Camera pixel-to-ray transform (inverse projection * view)
    uint texture_size_x;         // 3D block texture width in voxels (chunks * CHUNK_SIZE)
    uint texture_size_y;         // 3D block texture height in voxels
    uint texture_size_z;         // 3D block texture depth in voxels
    uint render_mode;            // Active render mode (TEXTURED, NORMAL, STEPS, etc.)
    uint show_chunk_boundaries;  // 1 = overlay chunk boundary grid
    uint player_in_water;        // 1 = player is submerged (enables water tint)
    float time_of_day;           // 0.0 = midnight, 0.5 = noon, 1.0 = next midnight
    float animation_time;        // Monotonically increasing time for animated effects (seconds)
    float cloud_speed;           // Cloud drift speed multiplier
    float cloud_coverage;        // Cloud density (0.0 = clear, 1.0 = overcast)
    float cloud_color_r;         // Cloud color R component
    float cloud_color_g;         // Cloud color G component
    float cloud_color_b;         // Cloud color B component
    uint clouds_enabled;         // 1 = render clouds
    int break_block_x;           // World X of block currently being broken (-1 = none)
    int break_block_y;           // World Y of block currently being broken
    int break_block_z;           // World Z of block currently being broken
    float break_progress;        // Break progress 0.0-1.0 (controls crack overlay)
    uint particle_count;         // Number of active particles in particle buffer
    int preview_block_x;         // World X of ghost block preview (-1 = none)
    int preview_block_y;         // World Y of ghost block preview
    int preview_block_z;         // World Z of ghost block preview
    uint preview_block_type;     // BlockType of ghost block preview
    uint light_count;            // Number of active point lights in light buffer
    float ambient_light;         // Base ambient light level (0.0-1.0)
    float fog_density;           // Exponential fog density coefficient
    float fog_start;             // Distance at which fog begins (world units)
    float fog_overlay_scale;     // Fog color overlay blend strength
    int target_block_x;          // World X of block under cursor (-1 = none)
    int target_block_y;          // World Y of block under cursor
    int target_block_z;          // World Z of block under cursor
    uint max_ray_steps;          // Maximum DDA steps per ray (128-1024)
    uint shadow_max_steps;       // Maximum DDA steps for shadow rays
    int texture_origin_x;        // World X origin of loaded 3D texture region
    int texture_origin_y;        // World Y origin of loaded 3D texture region
    int texture_origin_z;        // World Z origin of loaded 3D texture region
    uint enable_ao;              // 1 = compute ambient occlusion
    uint enable_shadows;         // 1 = cast directional shadow rays
    uint enable_model_shadows;   // 1 = sub-voxel models cast shadows
    uint enable_point_lights;    // 1 = compute point light contributions
    uint enable_tinted_shadows;  // 1 = tinted glass casts colored shadows
    uint transparent_background; // 1 = render sky as transparent (screenshot mode)
    uint pass_mode;              // Render pass type (0=opaque, 1=translucent)
    float lod_ao_distance;       // Max distance to compute AO (world units)
    float lod_shadow_distance;   // Max distance to cast shadow rays
    float lod_point_light_distance; // Max distance for point light contributions
    float lod_model_distance;    // Max distance to render sub-voxel model detail
    uint falling_block_count;    // Number of active falling block entities
    uint show_water_sources;     // 1 = highlight water source blocks
    uint water_source_count;     // Number of water source positions in buffer
    uint template_block_count;   // Number of blocks in active template preview
    int template_preview_min_x;  // Template preview bounding box min X
    int template_preview_min_y;  // Template preview bounding box min Y
    int template_preview_min_z;  // Template preview bounding box min Z
    int template_preview_max_x;  // Template preview bounding box max X
    int template_preview_max_y;  // Template preview bounding box max Y
    int template_preview_max_z;  // Template preview bounding box max Z
    vec4 camera_pos; // world-space camera position (already 16-byte aligned at offset 272)
    int selection_pos1_x;        // Selection box corner 1 X (block selection tool)
    int selection_pos1_y;        // Selection box corner 1 Y
    int selection_pos1_z;        // Selection box corner 1 Z
    int selection_pos2_x;        // Selection box corner 2 X
    int selection_pos2_y;        // Selection box corner 2 Y
    int selection_pos2_z;        // Selection box corner 2 Z
    uint hide_ground_cover;      // 1 = hide surface decoration blocks (grass, snow)
    uint cutaway_enabled;        // 1 = enable cutaway slice view
    int cutaway_chunk_x;         // Cutaway slice chunk X
    int cutaway_chunk_y;         // Cutaway slice chunk Y
    int cutaway_chunk_z;         // Cutaway slice chunk Z
    int cutaway_player_chunk_x;  // Player's current chunk X (for cutaway orientation)
    int cutaway_player_chunk_z;  // Player's current chunk Z
    // Measurement markers (up to 4 positions)
    uint measurement_marker_count;
    int measurement_marker_0_x;
    int measurement_marker_0_y;
    int measurement_marker_0_z;
    int measurement_marker_1_x;
    int measurement_marker_1_y;
    int measurement_marker_1_z;
    int measurement_marker_2_x;
    int measurement_marker_2_y;
    int measurement_marker_2_z;
    int measurement_marker_3_x;
    int measurement_marker_3_y;
    int measurement_marker_3_z;
    // Stencil rendering
    uint stencil_block_count;
    float stencil_opacity;
    uint stencil_render_mode; // 0=wireframe, 1=solid
    // Measurement laser color
    float laser_color_r;
    float laser_color_g;
    float laser_color_b;
    // Sky colors (day)
    float sky_zenith_r;
    float sky_zenith_g;
    float sky_zenith_b;
    float sky_horizon_r;
    float sky_horizon_g;
    float sky_horizon_b;
    // Picture frame rendering
    uint selected_picture_id;  // Currently selected picture for frame placement (0 = no picture)
    // Remote player rendering
    uint remote_player_count;
    // Custom texture count for multiplayer
    uint custom_texture_count;
    // Pre-computed animated pulses (host-side, once per frame)
    float mushroom_pulse;    // 0.95 + 0.05 * sin(animation_time * 1.5) — position-independent
    float lava_time_phase;   // animation_time * 2.0 — saves one mul per emissive lava fragment
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

// Stencil block positions for holographic guides (shares particle set)
struct StencilBlock {
    vec4 position;  // xyz = block world position, w = stencil_id (for per-stencil color)
};
layout(set = 3, binding = 4) readonly buffer StencilBlockBuffer {
    StencilBlock stencil_blocks[];
};

// Remote players for multiplayer (shares particle set)
struct RemotePlayer {
    vec4 pos_color;      // xyz = feet position, w = color index (0-7)
    vec4 height_padding; // x = height (typically 1.8), yzw = padding
};
layout(set = 3, binding = 5) readonly buffer RemotePlayerBuffer {
    RemotePlayer remote_players[];
};

// Point lights (set 4)
struct PointLight {
    vec4 pos_radius;   // xyz = position, w = radius
    vec4 color;        // rgb = color, a = intensity (raw value)
    vec4 animation;    // x = mode, y = reserved, z = reserved, w = pre-computed animation factor
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

// Sub-voxel models (set 7, bindings 2-9)
// Three tiered atlases - models at native resolutions (8³, 16³, 32³)
layout(set = 7, binding = 2, r8ui) readonly uniform uimage3D modelAtlas8;   // 8³ resolution
layout(set = 7, binding = 3, r8ui) readonly uniform uimage3D modelAtlas16;  // 16³ resolution
layout(set = 7, binding = 4, r8ui) readonly uniform uimage3D modelAtlas32;  // 32³ resolution
layout(set = 7, binding = 5) uniform sampler2D modelPalettes;
layout(set = 7, binding = 6, rg8ui) readonly uniform uimage3D modelMetadata;
// Per-block custom data (e.g., picture_id, offset_x, offset_y for frames)
layout(set = 7, binding = 9, r32ui) readonly uniform uimage3D blockCustomData;
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
layout(set = 7, binding = 7) readonly buffer ModelPropertiesBuffer {
    ModelProperties model_properties[];
};
layout(set = 7, binding = 8) uniform sampler2D modelPaletteEmission;

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
const uint MODEL_FLAG_IS_GROUND_COVER = 1u << 8;
// palette_id is in bits 16-23: (flags >> 16) & 0xFF (shared palette atlas indirection)

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
layout(set = 2, binding = 1) uniform sampler2D customTextureAtlas;
layout(set = 2, binding = 2) uniform sampler2D pictureAtlas;

// Custom texture atlas has 16 slots (indices 0-15)
// Custom texture indices are flagged by having bit 7 set (128+)
const uint CUSTOM_TEXTURE_FLAG = 128u;
const float CUSTOM_ATLAS_TILE_COUNT = 16.0;
const float CUSTOM_ATLAS_TILE_SIZE = 1.0 / 16.0;

// Picture atlas has 64 slots for up to 64 pictures
// Each picture is stored in a fixed slot (0-63)
// Each picture slot is 128×128 pixels
const uint PICTURE_ATLAS_SLOT_COUNT = 64u;
const uint PICTURE_ATLAS_SIZE = 128u;        // Each picture slot is 128x128 pixels
const uint PICTURE_ATLAS_WIDTH = 8192u;      // 64 slots * 128 pixels = 8192
const float PICTURE_ATLAS_SLOT_SIZE = 1.0 / 64.0;

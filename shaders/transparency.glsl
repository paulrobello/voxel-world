/*
 * transparency.glsl -- Post-hit overlay functions for water, glass, and tinted glass.
 *
 * These helpers are called after the primary ray hit has been shaded.
 * They apply transparency-system effects (reflections, tints, depth fog)
 * as additive post-processing passes on top of the already-computed color.
 *
 * Included by traverse.comp. Requires common.glsl, sky.glsl, overlays.glsl,
 * and materials.glsl to be included first.
 */

// ---------------------------------------------------------------------------
// Water surface overlay
//
// Applies wave-animated surface normals, Fresnel reflection, and depth-based
// color absorption for a ray that entered water before hitting a solid block.
//
// Parameters:
//   dir                  - Ray direction (need not be normalized)
//   inWater              - Whether the ray entered a water volume
//   waterDepth           - World-space distance travelled inside water
//   waterSurfaceHit      - World position of the water surface crossing
//   waterSurfaceNormal   - Face normal at the water entry crossing
//   currentWaterType     - Water variant (ocean, river, lava-adjacent, etc.)
//   color (inout)        - Current pixel color, modified in place
// ---------------------------------------------------------------------------
void applyWaterOverlay(
    vec3 dir,
    bool inWater,
    float waterDepth,
    vec3 waterSurfaceHit,
    vec3 waterSurfaceNormal,
    uint currentWaterType,
    inout vec3 color)
{
    if (!inWater || pc.player_in_water != 0) {
        return;
    }

    // Animate the surface normal with wave noise
    vec3 animatedNormal = getWaterWaveNormal(waterSurfaceHit, pc.animation_time);
    float normalBlend = abs(waterSurfaceNormal.y);
    vec3 finalNormal = normalize(mix(waterSurfaceNormal, animatedNormal, normalBlend));

    // Fresnel reflection at the water surface
    float viewDotNormal = abs(dot(normalize(dir), finalNormal));
    float fresnel = WATER_REFLECTIVITY + (1.0 - WATER_REFLECTIVITY) * pow(1.0 - viewDotNormal, WATER_FRESNEL_POWER);
    vec3 reflectDir = reflect(normalize(dir), finalNormal);
    vec3 reflectionColor = getSkyColor(reflectDir);
    vec3 tint = getWaterTintFn(uint(currentWaterType));
    color = mix(color, reflectionColor * tint, fresnel * 0.6);

    // Depth-based absorption / tinting
    if (waterDepth > 0.0) {
        float clarity = getWaterClarity(uint(currentWaterType));
        float depthFactor = 1.0 - exp(-waterDepth / clarity);
        vec3 waterColor = mix(WATER_COLOR * tint, WATER_DEEP_COLOR * tint, depthFactor * 0.5);
        color = mix(color, waterColor, depthFactor * 0.7);
    }
}

// ---------------------------------------------------------------------------
// Shared glass-overlay core: edge frame highlight + Fresnel sky reflection.
//
// Both the plain-glass and tinted-glass overlays compute the same face-UV edge
// frame and Fresnel reflection; only the frame colours and the sky-reflection
// tint differ. `reflectionTint` multiplies the sky reflection (vec3(1.0) for
// plain glass, the accumulated tint for stained glass).
// ---------------------------------------------------------------------------
void applyGlassFrameReflection(
    vec3 dir,
    vec3 surfaceNormal,
    vec3 firstLocalHit,
    vec3 frameColor,
    vec3 highlightColor,
    vec3 reflectionTint,
    inout vec3 color)
{
    vec3 localPos = firstLocalHit;

    // Compute face UV from surface normal direction
    vec2 faceUV;
    if (abs(surfaceNormal.x) > 0.5) {
        faceUV = localPos.yz;
    } else if (abs(surfaceNormal.y) > 0.5) {
        faceUV = localPos.xz;
    } else {
        faceUV = localPos.xy;
    }

    // Edge frame highlight
    const float frameWidth = 0.012;
    const float innerFrameWidth = 0.03;

    float distFromEdgeX = min(faceUV.x, 1.0 - faceUV.x);
    float distFromEdgeY = min(faceUV.y, 1.0 - faceUV.y);
    float distFromEdge  = min(distFromEdgeX, distFromEdgeY);

    float outerFrame = 1.0 - smoothstep(0.0, frameWidth * 0.8, distFromEdge);
    float innerFrame  = smoothstep(frameWidth * 0.7, innerFrameWidth, distFromEdge) *
                        (1.0 - smoothstep(innerFrameWidth, innerFrameWidth + 0.01, distFromEdge));

    color = mix(color, frameColor,     outerFrame * 0.35);
    color = mix(color, highlightColor, innerFrame * 0.22);

    // Fresnel sky reflection
    float viewDotNormal = abs(dot(normalize(dir), surfaceNormal));
    float fresnel = 0.05 + 0.15 * pow(1.0 - viewDotNormal, 2.0);
    vec3 reflectDir      = reflect(normalize(dir), surfaceNormal);
    vec3 reflectionColor = getSkyColor(reflectDir) * reflectionTint;
    color = mix(color, reflectionColor, fresnel);
}

// ---------------------------------------------------------------------------
// Shared target-block selection wireframe, drawn on the first crossed block.
// Identical for plain and tinted glass.
// ---------------------------------------------------------------------------
void applyGlassTargetWireframe(
    vec3 firstLocalHit,
    vec3 surfaceNormal,
    ivec3 firstCoord,
    inout vec3 color)
{
    if (!hasTargetBlock()) {
        return;
    }
    ivec3 targetPos = ivec3(pc.target_block_x, pc.target_block_y, pc.target_block_z);
    if (firstCoord != targetPos) {
        return;
    }
    float wireframe = getWireframeFactor(firstLocalHit, surfaceNormal);
    if (wireframe > 0.1) {
        vec3  outlineColor = vec3(0.0, 1.0, 1.0);
        float outlineAlpha = wireframe * 0.9;
        color = mix(color, outlineColor, outlineAlpha);
    }
}

// ---------------------------------------------------------------------------
// Glass surface overlay
//
// Applies the cyan-tinted edge highlight, Fresnel sky reflection, depth-based
// tinting for stacked glass, and (optionally) the target-block wireframe for
// the first glass block the ray crossed.
// ---------------------------------------------------------------------------
void applyGlassOverlay(
    vec3 dir,
    bool passedGlass,
    float glassDepth,
    vec3 glassSurfaceNormal,
    vec3 firstGlassLocalHit,
    ivec3 firstGlassCoord,
    inout vec3 color)
{
    if (!passedGlass) {
        return;
    }

    applyGlassFrameReflection(
        dir,
        glassSurfaceNormal,
        firstGlassLocalHit,
        vec3(0.6, 0.9, 0.95),    // cyan-ish frame
        vec3(0.95, 0.99, 1.0),   // lighter rim
        vec3(1.0),               // plain sky reflection
        color);

    // Depth tinting through stacked glass
    if (glassDepth > 1.0) {
        vec3  glassTint   = vec3(0.92, 0.96, 1.0);
        float depthFactor = min((glassDepth - 1.0) * 0.08, 0.25);
        color = mix(color, color * glassTint, depthFactor);
    }

    applyGlassTargetWireframe(firstGlassLocalHit, glassSurfaceNormal, firstGlassCoord, color);
}

// ---------------------------------------------------------------------------
// Tinted glass overlay
//
// Applies edge frame highlight (using the accumulated tint colour), Fresnel
// sky reflection, and the multiplicative colour tint accumulated from all
// tinted glass blocks the ray crossed.
// ---------------------------------------------------------------------------
void applyTintedGlassOverlay(
    vec3 dir,
    bool passedTintedGlass,
    vec3 accumulatedTint,
    vec3 tintedGlassSurfaceNormal,
    vec3 firstTintedGlassLocalHit,
    ivec3 firstTintedGlassCoord,
    inout vec3 color)
{
    if (!passedTintedGlass) {
        return;
    }

    vec3 frameColor     = accumulatedTint * 0.8;
    vec3 highlightColor = mix(accumulatedTint, vec3(1.0), 0.5);

    applyGlassFrameReflection(
        dir,
        tintedGlassSurfaceNormal,
        firstTintedGlassLocalHit,
        frameColor,
        highlightColor,
        accumulatedTint,         // tinted sky reflection
        color);

    // Multiplicative tint pass (colour absorption through stained glass)
    color *= accumulatedTint;

    applyGlassTargetWireframe(firstTintedGlassLocalHit, tintedGlassSurfaceNormal, firstTintedGlassCoord, color);
}

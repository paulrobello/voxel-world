// Day sky colors
const vec3 DAY_SKY_HORIZON = vec3(0.6, 0.75, 0.95);   // Light blue at horizon
const vec3 DAY_SKY_ZENITH = vec3(0.25, 0.45, 0.85);   // Deeper blue overhead
const vec3 DAY_SKY_GROUND = vec3(0.4, 0.45, 0.5);     // Gray-blue below horizon

// Sunrise/sunset sky colors
const vec3 SUNSET_HORIZON = vec3(1.0, 0.5, 0.2);      // Orange at horizon
const vec3 SUNSET_ZENITH = vec3(0.4, 0.3, 0.6);       // Purple overhead
const vec3 SUNSET_GROUND = vec3(0.3, 0.25, 0.3);      // Dark purple below

// Night sky colors
const vec3 NIGHT_SKY_HORIZON = vec3(0.05, 0.08, 0.15); // Dark blue at horizon
const vec3 NIGHT_SKY_ZENITH = vec3(0.02, 0.02, 0.08);  // Near black overhead
const vec3 NIGHT_SKY_GROUND = vec3(0.02, 0.02, 0.05);  // Very dark below

// Sun settings
const vec3 SUN_COLOR = vec3(1.0, 0.95, 0.8);           // Warm white sun
const vec3 SUNSET_SUN_COLOR = vec3(1.0, 0.6, 0.3);     // Orange sunset sun
const float SUN_SIZE = 0.04;                            // Angular size of sun disk

// Moon settings
const vec3 MOON_COLOR = vec3(0.9, 0.9, 1.0);           // Cool white moon
const float MOON_SIZE = 0.03;                           // Slightly smaller than sun

// Cloud settings
const float CLOUD_HEIGHT = 240.0;      // Height of cloud layer (raised +96 to match terrain)
const float CLOUD_SCALE = 0.02;        // Scale of cloud noise
const float CLOUD_COVERAGE = 0.45;     // 0-1, higher = more clouds
const vec3 CLOUD_COLOR = vec3(1.0, 1.0, 1.0);   // White clouds
const vec2 CLOUD_WIND = vec2(0.8, 0.3);  // Wind direction and speed for cloud movement

// Calculate sun direction based on time of day
// time: 0.0 = midnight, 0.25 = sunrise (east), 0.5 = noon (overhead), 0.75 = sunset (west)
vec3 getSunDirection(float time) {
    float angle = time * 2.0 * 3.14159265;  // Full rotation
    // Sun rises in east (+X), goes overhead (+Y), sets in west (-X)
    float y = -cos(angle);  // -1 at midnight, +1 at noon
    float xz = sin(angle);  // 0 at midnight/noon, +1 at sunrise, -1 at sunset
    return normalize(vec3(xz, y, 0.3));  // Slight Z offset for visual interest
}

// Calculate moon direction (opposite of sun)
vec3 getMoonDirection(float time) {
    return -getSunDirection(time);
}

// Get daylight factor (0 = full night, 1 = full day)
float getDaylightFactor(float time) {
    // Use sun's actual Y position for proper sync
    vec3 sunDir = getSunDirection(time);
    // Smooth transition: starts getting light when sun Y > -0.2, full day when Y > 0.3
    return smoothstep(-0.2, 0.3, sunDir.y);
}

// Get sunrise/sunset factor (peaks when sun is near horizon)
float getSunsetFactor(float time) {
    vec3 sunDir = getSunDirection(time);
    // Peak when sun is near horizon (Y close to 0)
    float horizonProximity = 1.0 - smoothstep(0.0, 0.4, abs(sunDir.y));
    // Only show sunset colors when sun is visible (not deep below horizon)
    float sunVisible = smoothstep(-0.3, 0.0, sunDir.y);
    return horizonProximity * sunVisible;
}

// Get current fog color based on time
vec3 getFogColor(float time) {
    float daylight = getDaylightFactor(time);
    float sunset = getSunsetFactor(time);

    vec3 dayFog = vec3(0.65, 0.78, 0.95);
    vec3 nightFog = vec3(0.05, 0.08, 0.15);
    vec3 sunsetFog = vec3(0.6, 0.4, 0.3);

    vec3 baseFog = mix(nightFog, dayFog, daylight);
    return mix(baseFog, sunsetFog, sunset * 0.5);
}

// Get current sun color (blends from white to orange at sunset)
vec3 getSunColor() {
    float sunset = getSunsetFactor(pc.time_of_day);
    return mix(SUN_COLOR, SUNSET_SUN_COLOR, sunset);
}

// Simple hash for noise
float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

// 2D value noise
float noise2D(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);  // Smoothstep

    float a = hash(i);
    float b = hash(i + vec2(1.0, 0.0));
    float c = hash(i + vec2(0.0, 1.0));
    float d = hash(i + vec2(1.0, 1.0));

    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

// Fractal Brownian Motion for clouds
float fbm(vec2 p) {
    float value = 0.0;
    float amplitude = 0.5;
    float frequency = 1.0;

    for (int i = 0; i < 4; i++) {
        value += amplitude * noise2D(p * frequency);
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    return value;
}

// Calculate cloud density at a point (animated with wind)
float getCloudDensity(vec3 rayOrigin, vec3 rayDir) {
    // Only render clouds above horizon
    if (rayDir.y <= 0.01) return 0.0;

    // Find intersection with cloud plane
    float t = (CLOUD_HEIGHT - rayOrigin.y) / rayDir.y;
    if (t < 0.0) return 0.0;

    vec3 cloudPos = rayOrigin + rayDir * t;

    // Convert to world coordinates so clouds don't shift when texture origin moves
    vec3 worldCloudPos = cloudPos + vec3(textureOrigin());

    // Animate clouds with wind - offset UV by time (scaled by user speed)
    vec2 wind = CLOUD_WIND * pc.cloud_speed;
    vec2 cloudUV = worldCloudPos.xz * CLOUD_SCALE + pc.animation_time * wind * CLOUD_SCALE;

    // Sample cloud noise with multiple octaves for detail
    float density = fbm(cloudUV);

    // Add secondary layer moving at different speed for depth
    vec2 cloudUV2 = worldCloudPos.xz * CLOUD_SCALE * 0.5 + pc.animation_time * wind * CLOUD_SCALE * 0.6;
    float density2 = fbm(cloudUV2) * 0.3;
    density = density + density2;

    // Apply coverage threshold (invert so 0=no clouds, 1=full coverage)
    float threshold = 1.0 - pc.cloud_coverage;
    density = smoothstep(threshold, threshold + 0.3, density);

    // Fade clouds at edges and with distance
    float distFade = 1.0 - smoothstep(200.0, 400.0, t);
    float horizonFade = smoothstep(0.01, 0.15, rayDir.y);

    return density * distFade * horizonFade;
}

// Generate stars based on direction
float getStars(vec3 dir) {
    // Use direction as seed for star positions
    vec2 starCoord = vec2(atan(dir.x, dir.z), asin(dir.y)) * 100.0;
    float star = hash(floor(starCoord));
    // Only show brightest "stars"
    star = step(0.995, star);
    // Twinkle effect
    star *= 0.5 + 0.5 * sin(hash(floor(starCoord) + 0.5) * 100.0);
    return star;
}

// Calculate sky color with sun, moon, stars and clouds
// Set underwaterView to true to disable sun/clouds (for underwater rendering)
vec3 getSkyColorEx(vec3 rayDir, bool underwaterView) {
    vec3 dir = normalize(rayDir);
    float y = dir.y;
    float time = pc.time_of_day;

    // Get time-based factors
    float daylight = getDaylightFactor(time);
    float sunset = getSunsetFactor(time);
    vec3 sunDir = getSunDirection(time);
    vec3 moonDir = getMoonDirection(time);

    // Interpolate sky colors based on time
    vec3 horizonColor = mix(NIGHT_SKY_HORIZON, DAY_SKY_HORIZON, daylight);
    vec3 zenithColor = mix(NIGHT_SKY_ZENITH, DAY_SKY_ZENITH, daylight);
    vec3 groundColor = mix(NIGHT_SKY_GROUND, DAY_SKY_GROUND, daylight);

    // Blend in sunset colors
    horizonColor = mix(horizonColor, SUNSET_HORIZON, sunset);
    zenithColor = mix(zenithColor, SUNSET_ZENITH, sunset * 0.5);
    groundColor = mix(groundColor, SUNSET_GROUND, sunset * 0.3);

    // Base sky gradient
    vec3 skyColor;
    if (y > 0.0) {
        float t = pow(y, 0.5);
        skyColor = mix(horizonColor, zenithColor, t);
    } else {
        float t = pow(-y, 0.7);
        skyColor = mix(horizonColor, groundColor, t);
    }

    // Skip celestial bodies and clouds when underwater
    if (underwaterView) {
        return skyColor;
    }

    // Add stars at night (only visible when dark)
    if (daylight < 0.5 && y > 0.0) {
        float starBrightness = (1.0 - daylight * 2.0) * getStars(dir);
        skyColor += vec3(starBrightness);
    }

    // Add moon at night
    float moonDot = dot(dir, moonDir);
    if (moonDot > 0.0 && daylight < 0.8) {
        float moonFactor = smoothstep(1.0 - MOON_SIZE, 1.0, moonDot);
        float moonBrightness = 1.0 - daylight;
        skyColor = mix(skyColor, MOON_COLOR, moonFactor * moonBrightness);

        // Subtle moon glow
        float moonGlow = pow(max(0.0, moonDot), 16.0) * 0.3 * moonBrightness;
        skyColor += MOON_COLOR * moonGlow;
    }

    // Add sun (only when above horizon-ish)
    float sunDot = dot(dir, sunDir);
    if (sunDot > 0.0 && sunDir.y > -0.2) {
        // Blend sun color from white to orange at sunset
        vec3 currentSunColor = mix(SUN_COLOR, SUNSET_SUN_COLOR, sunset);

        // Sun disk
        float sunFactor = smoothstep(1.0 - SUN_SIZE, 1.0, sunDot);
        skyColor = mix(skyColor, currentSunColor, sunFactor);

        // Sun glow (larger at sunset)
        float glowSize = 8.0 - sunset * 4.0;  // Larger glow at sunset
        float glowFactor = pow(max(0.0, sunDot), glowSize) * (0.5 + sunset * 0.3);
        skyColor += currentSunColor * glowFactor;
    }

    // Add clouds (only above horizon, dimmer at night)
    if (pc.clouds_enabled != 0u && y > 0.0) {
        vec3 worldOrigin = pc.pixelToRay[3].xyz;
        float cloudDensity = getCloudDensity(worldOrigin, dir);

        // Cloud brightness based on time and sun direction
        float sunLight = max(0.2, dot(dir, sunDir) * 0.5 + 0.5);
        float cloudBrightness = mix(0.15, 1.0, daylight) * sunLight;

        // Use cloud color from push constants
        vec3 baseCloudColor = vec3(pc.cloud_color_r, pc.cloud_color_g, pc.cloud_color_b);

        // Tint clouds orange at sunset
        vec3 cloudColor = mix(baseCloudColor * cloudBrightness, SUNSET_SUN_COLOR, sunset * 0.4);

        skyColor = mix(skyColor, cloudColor, cloudDensity * 0.9);
    }

    return skyColor;
}

// Default sky color (not underwater)
vec3 getSkyColor(vec3 rayDir) {
    return getSkyColorEx(rayDir, false);
}

// Calculate fog factor based on distance using exponential fog (0 = no fog, 1 = full fog)
float getFogFactor(float distance) {
    // No fog before fog_start distance
    if (distance < pc.fog_start) {
        return 0.0;
    }
    // Exponential fog starting from fog_start: 1 - exp(-density * (distance - fog_start))
    // pc.fog_density controls thickness (0 = no fog, 0.02 = light, 0.05 = medium, 0.1 = thick)
    float effectiveDistance = distance - pc.fog_start;
    return 1.0 - exp(-pc.fog_density * effectiveDistance);
}

// Get current sun direction for lighting calculations
vec3 getCurrentSunDir() {
    return getSunDirection(pc.time_of_day);
}

// Apply fog to a color
vec3 applyFog(vec3 color, float distance, vec3 rayDir) {
    float fogFactor = getFogFactor(distance);
    // Blend toward the actual sky color in the ray direction to avoid flat tinted fog.
    vec3 fogColor = getSkyColor(normalize(rayDir));
    return mix(color, fogColor, fogFactor);
}

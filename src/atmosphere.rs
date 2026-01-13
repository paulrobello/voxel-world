/// Atmospheric lighting and fog settings.
#[derive(Debug, Clone)]
pub struct AtmosphereSettings {
    pub ambient_light: f32,
    pub fog_density: f32,
    pub fog_start: f32,
    pub fog_overlay_scale: f32,
    pub cloud_speed: f32,
    pub cloud_coverage: f32,
    pub cloud_color: [f32; 3],
    pub clouds_enabled: bool,
    /// Day sky color at zenith (overhead). RGB values 0-1.
    pub sky_color_zenith: [f32; 3],
    /// Day sky color at horizon. RGB values 0-1.
    pub sky_color_horizon: [f32; 3],
}

impl Default for AtmosphereSettings {
    fn default() -> Self {
        Self {
            ambient_light: 0.1,
            fog_density: 0.03,
            fog_start: 100.0,
            fog_overlay_scale: 1.0,
            cloud_speed: 1.0,
            cloud_coverage: 0.45,
            cloud_color: [1.0, 1.0, 1.0],
            clouds_enabled: true,
            // Default day sky colors (matches original shader constants)
            sky_color_zenith: [0.25, 0.45, 0.85], // Deep blue overhead
            sky_color_horizon: [0.6, 0.75, 0.95], // Light blue at horizon
        }
    }
}

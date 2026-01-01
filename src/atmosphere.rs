/// Atmospheric lighting and fog settings.
#[derive(Debug, Clone)]
pub struct AtmosphereSettings {
    pub ambient_light: f32,
    pub fog_density: f32,
    pub fog_start: f32,
    pub fog_affects_sky: bool,
    pub fog_overlay_scale: f32,
}

impl Default for AtmosphereSettings {
    fn default() -> Self {
        Self {
            ambient_light: 0.1,
            fog_density: 0.01,
            fog_start: 128.0,
            fog_affects_sky: false,
            fog_overlay_scale: 1.0,
        }
    }
}

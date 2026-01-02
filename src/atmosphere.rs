/// Atmospheric lighting and fog settings.
#[derive(Debug, Clone)]
pub struct AtmosphereSettings {
    pub ambient_light: f32,
    pub fog_density: f32,
    pub fog_start: f32,
    pub fog_overlay_scale: f32,
    pub cloud_speed: f32,
}

impl Default for AtmosphereSettings {
    fn default() -> Self {
        Self {
            ambient_light: 0.1,
            fog_density: 0.03,
            fog_start: 100.0,
            fog_overlay_scale: 1.0,
            cloud_speed: 1.0,
        }
    }
}

use crate::chunk::BlockType;
use crate::terrain_gen::BiomeInfo;
use egui_winit_vulkano::egui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum MinimapMode {
    Blocks,
    Height,
    Combined,
    Elevation,
    Temperature,
    Rainfall,
}

pub struct Minimap {
    pub size: u32,
    pub mode: MinimapMode,
    pub rotate: bool,
    pub zoom: f32,
    /// When true, skip Model blocks (flowers, grass, torches) to show terrain underneath.
    /// Tree leaves are intentionally NOT skipped as they're important navigation landmarks.
    pub skip_decorative: bool,
}

impl Minimap {
    pub fn new() -> Self {
        Self {
            size: 256,
            mode: MinimapMode::Combined,
            rotate: true,
            zoom: 2.0,
            skip_decorative: true,
        }
    }

    pub fn get_color(
        &self,
        block: BlockType,
        height: i32,
        info: Option<BiomeInfo>,
    ) -> egui::Color32 {
        let base_color = block.color();
        let (r, g, b) = (base_color[0], base_color[1], base_color[2]);

        match self.mode {
            MinimapMode::Blocks => {
                // Block colors only
                egui::Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
            }
            MinimapMode::Height => {
                // Height shading only (grayscale)
                let brightness = ((height as f32 / 128.0) * 200.0 + 55.0).min(255.0) as u8;
                egui::Color32::from_rgb(brightness, brightness, brightness)
            }
            MinimapMode::Combined => {
                // Both: block colors with height brightness
                let height_factor = 0.5 + (height as f32 / 128.0) * 0.5;
                egui::Color32::from_rgb(
                    (r * 255.0 * height_factor).min(255.0) as u8,
                    (g * 255.0 * height_factor).min(255.0) as u8,
                    (b * 255.0 * height_factor).min(255.0) as u8,
                )
            }
            MinimapMode::Elevation => {
                if let Some(info) = info {
                    // Map -1.0..1.0 to 0..255 (grayscale)
                    let v = ((info.elevation + 1.0) * 0.5).clamp(0.0, 1.0);
                    let c = (v * 255.0) as u8;
                    egui::Color32::from_rgb(c, c, c)
                } else {
                    egui::Color32::BLACK
                }
            }
            MinimapMode::Temperature => {
                if let Some(info) = info {
                    // Blue (cold) to Red (hot)
                    let t = info.temperature.clamp(0.0, 1.0);
                    let r = (t * 255.0) as u8;
                    let b = ((1.0 - t) * 255.0) as u8;
                    egui::Color32::from_rgb(r, 0, b)
                } else {
                    egui::Color32::BLACK
                }
            }
            MinimapMode::Rainfall => {
                if let Some(info) = info {
                    // White (dry) to Blue (wet)
                    let rain = info.rainfall.clamp(0.0, 1.0);
                    let r = ((1.0 - rain) * 255.0) as u8;
                    let g = ((1.0 - rain) * 255.0) as u8;
                    let b = 255;
                    egui::Color32::from_rgb(r, g, b)
                } else {
                    egui::Color32::BLACK
                }
            }
        }
    }
}

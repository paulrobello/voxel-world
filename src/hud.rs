use crate::chunk::BlockType;
use egui_winit_vulkano::egui;

pub struct Minimap {
    pub size: u32,
    pub color_mode: u8,
    pub rotate: bool,
    pub zoom: f32,
}

impl Minimap {
    pub fn new() -> Self {
        Self {
            size: 256,
            color_mode: 2,
            rotate: true,
            zoom: 0.5,
        }
    }

    pub fn get_color(&self, block: BlockType, height: i32) -> egui::Color32 {
        let base_color = block.color();
        let (r, g, b) = (base_color[0], base_color[1], base_color[2]);

        match self.color_mode {
            0 => {
                // Block colors only
                egui::Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
            }
            1 => {
                // Height shading only (grayscale)
                let brightness = ((height as f32 / 128.0) * 200.0 + 55.0).min(255.0) as u8;
                egui::Color32::from_rgb(brightness, brightness, brightness)
            }
            _ => {
                // Both: block colors with height brightness
                let height_factor = 0.5 + (height as f32 / 128.0) * 0.5;
                egui::Color32::from_rgb(
                    (r * 255.0 * height_factor).min(255.0) as u8,
                    (g * 255.0 * height_factor).min(255.0) as u8,
                    (b * 255.0 * height_factor).min(255.0) as u8,
                )
            }
        }
    }
}

//! Core paint system types: HSV adjustment, blend modes, and paint configuration.

use serde::{Deserialize, Serialize};

/// Blend modes for combining tint color with texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum BlendMode {
    /// Standard multiply blend (current behavior): tint * texture.
    #[default]
    Multiply = 0,
    /// Overlay blend: enhances contrast while preserving highlights/shadows.
    Overlay = 1,
    /// Soft light: gentle highlight/shadow adjustment.
    SoftLight = 2,
    /// Screen blend: lightens the result.
    Screen = 3,
    /// Color only: applies tint hue/saturation, keeps texture luminance.
    ColorOnly = 4,
}

impl BlendMode {
    /// Returns all available blend modes.
    pub const ALL: [BlendMode; 5] = [
        BlendMode::Multiply,
        BlendMode::Overlay,
        BlendMode::SoftLight,
        BlendMode::Screen,
        BlendMode::ColorOnly,
    ];

    /// Returns a human-readable name for the blend mode.
    pub fn display_name(self) -> &'static str {
        match self {
            BlendMode::Multiply => "Multiply",
            BlendMode::Overlay => "Overlay",
            BlendMode::SoftLight => "Soft Light",
            BlendMode::Screen => "Screen",
            BlendMode::ColorOnly => "Color Only",
        }
    }

    /// Returns a brief description of the blend mode effect.
    pub fn description(self) -> &'static str {
        match self {
            BlendMode::Multiply => "Darkens texture with tint color",
            BlendMode::Overlay => "Enhances contrast with tint influence",
            BlendMode::SoftLight => "Gentle highlight/shadow adjustment",
            BlendMode::Screen => "Lightens and adds tint glow",
            BlendMode::ColorOnly => "Applies tint hue, keeps texture detail",
        }
    }

    /// Converts from u8 value.
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => BlendMode::Multiply,
            1 => BlendMode::Overlay,
            2 => BlendMode::SoftLight,
            3 => BlendMode::Screen,
            4 => BlendMode::ColorOnly,
            _ => BlendMode::Multiply,
        }
    }
}

/// HSV color adjustment parameters.
///
/// Applied in the shader to modify the final color after blend mode application.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HsvAdjustment {
    /// Hue shift in degrees (-180 to +180).
    pub hue_shift: f32,
    /// Saturation multiplier (0.0 to 2.0, where 1.0 = no change).
    pub saturation_mult: f32,
    /// Value (brightness) multiplier (0.0 to 2.0, where 1.0 = no change).
    pub value_mult: f32,
}

impl Default for HsvAdjustment {
    fn default() -> Self {
        Self {
            hue_shift: 0.0,
            saturation_mult: 1.0,
            value_mult: 1.0,
        }
    }
}

impl HsvAdjustment {
    /// Creates a new HSV adjustment with the given values.
    pub fn new(hue_shift: f32, saturation_mult: f32, value_mult: f32) -> Self {
        Self {
            hue_shift: hue_shift.clamp(-180.0, 180.0),
            saturation_mult: saturation_mult.clamp(0.0, 2.0),
            value_mult: value_mult.clamp(0.0, 2.0),
        }
    }

    /// Returns true if this adjustment has no effect (identity).
    pub fn is_identity(&self) -> bool {
        (self.hue_shift.abs() < 0.001)
            && (self.saturation_mult - 1.0).abs() < 0.001
            && (self.value_mult - 1.0).abs() < 0.001
    }

    /// Packs the HSV adjustment into a u32 for GPU upload.
    /// Format: hue_shift (10 bits, signed) | sat_mult (11 bits) | val_mult (11 bits)
    pub fn pack(&self) -> u32 {
        // Hue: -180..180 -> 0..1023 (10 bits)
        let hue_normalized = ((self.hue_shift + 180.0) / 360.0 * 1023.0) as u32;
        // Saturation: 0..2 -> 0..2047 (11 bits)
        let sat_normalized = (self.saturation_mult / 2.0 * 2047.0) as u32;
        // Value: 0..2 -> 0..2047 (11 bits)
        let val_normalized = (self.value_mult / 2.0 * 2047.0) as u32;

        (hue_normalized & 0x3FF)
            | ((sat_normalized & 0x7FF) << 10)
            | ((val_normalized & 0x7FF) << 21)
    }

    /// Unpacks a u32 back into HSV adjustment values.
    pub fn unpack(packed: u32) -> Self {
        let hue_normalized = (packed & 0x3FF) as f32;
        let sat_normalized = ((packed >> 10) & 0x7FF) as f32;
        let val_normalized = ((packed >> 21) & 0x7FF) as f32;

        Self {
            hue_shift: hue_normalized / 1023.0 * 360.0 - 180.0,
            saturation_mult: sat_normalized / 2047.0 * 2.0,
            value_mult: val_normalized / 2047.0 * 2.0,
        }
    }
}

/// Complete paint configuration combining texture, tint, HSV, and blend mode.
///
/// This is the full specification for how a painted block should appear.
/// HSV and blend mode are stored per-preset, not per-block, to keep block
/// metadata minimal (blocks only store texture_idx and tint_idx).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PaintConfig {
    /// Atlas texture index (0-44).
    pub texture_idx: u8,
    /// Tint palette index (0-31).
    pub tint_idx: u8,
    /// HSV color adjustment.
    pub hsv: HsvAdjustment,
    /// Blend mode for combining tint with texture.
    pub blend_mode: BlendMode,
}

impl Default for PaintConfig {
    fn default() -> Self {
        Self {
            texture_idx: 1, // Stone
            tint_idx: 12,   // White
            hsv: HsvAdjustment::default(),
            blend_mode: BlendMode::default(),
        }
    }
}

impl PaintConfig {
    /// Creates a new paint config with all parameters.
    pub fn new(texture_idx: u8, tint_idx: u8, hsv: HsvAdjustment, blend_mode: BlendMode) -> Self {
        Self {
            texture_idx,
            tint_idx: tint_idx & 0x1F,
            hsv,
            blend_mode,
        }
    }

    /// Creates a simple paint config with just texture and tint (default HSV/blend).
    pub fn simple(texture_idx: u8, tint_idx: u8) -> Self {
        Self {
            texture_idx,
            tint_idx: tint_idx & 0x1F,
            hsv: HsvAdjustment::default(),
            blend_mode: BlendMode::default(),
        }
    }

    /// Returns true if this config uses only basic settings (identity HSV, multiply blend).
    pub fn is_basic(&self) -> bool {
        self.hsv.is_identity() && self.blend_mode == BlendMode::Multiply
    }
}

/// Convert RGB color to HSV.
/// Input/output: all values in 0.0-1.0 range (hue is 0-1 representing 0-360°).
pub fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let v = max;
    let s = if max > 0.0 { delta / max } else { 0.0 };

    let h = if delta < 0.0001 {
        0.0
    } else if max == r {
        ((g - b) / delta).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / delta + 2.0) / 6.0
    } else {
        ((r - g) / delta + 4.0) / 6.0
    };

    (h, s, v)
}

/// Convert HSV color to RGB.
/// Input: h in 0-1 (representing 0-360°), s and v in 0-1.
/// Output: RGB values in 0-1 range.
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    if s < 0.0001 {
        return (v, v, v);
    }

    let h = h.rem_euclid(1.0) * 6.0;
    let i = h.floor() as i32;
    let f = h - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));

    match i % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

/// Apply HSV adjustment to an RGB color.
/// Input RGB should be in 0-1 range.
pub fn apply_hsv_adjustment(r: f32, g: f32, b: f32, adj: &HsvAdjustment) -> (f32, f32, f32) {
    let (h, s, v) = rgb_to_hsv(r, g, b);

    // Apply adjustments
    let h_adjusted = (h + adj.hue_shift / 360.0).rem_euclid(1.0);
    let s_adjusted = (s * adj.saturation_mult).clamp(0.0, 1.0);
    let v_adjusted = (v * adj.value_mult).clamp(0.0, 1.0);

    hsv_to_rgb(h_adjusted, s_adjusted, v_adjusted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blend_mode_roundtrip() {
        for mode in BlendMode::ALL {
            assert_eq!(BlendMode::from_u8(mode as u8), mode);
        }
    }

    #[test]
    fn test_hsv_adjustment_default_is_identity() {
        let adj = HsvAdjustment::default();
        assert!(adj.is_identity());
    }

    #[test]
    fn test_hsv_adjustment_pack_unpack() {
        let original = HsvAdjustment::new(45.0, 1.5, 0.8);
        let packed = original.pack();
        let unpacked = HsvAdjustment::unpack(packed);

        // Allow small precision loss due to packing
        assert!((original.hue_shift - unpacked.hue_shift).abs() < 1.0);
        assert!((original.saturation_mult - unpacked.saturation_mult).abs() < 0.01);
        assert!((original.value_mult - unpacked.value_mult).abs() < 0.01);
    }

    #[test]
    fn test_rgb_hsv_roundtrip() {
        let test_colors = [
            (1.0, 0.0, 0.0), // Red
            (0.0, 1.0, 0.0), // Green
            (0.0, 0.0, 1.0), // Blue
            (0.5, 0.5, 0.5), // Gray
            (1.0, 1.0, 0.0), // Yellow
        ];

        for (r, g, b) in test_colors {
            let (h, s, v) = rgb_to_hsv(r, g, b);
            let (r2, g2, b2) = hsv_to_rgb(h, s, v);

            assert!(
                (r - r2).abs() < 0.001,
                "Red mismatch for ({}, {}, {})",
                r,
                g,
                b
            );
            assert!(
                (g - g2).abs() < 0.001,
                "Green mismatch for ({}, {}, {})",
                r,
                g,
                b
            );
            assert!(
                (b - b2).abs() < 0.001,
                "Blue mismatch for ({}, {}, {})",
                r,
                g,
                b
            );
        }
    }

    #[test]
    fn test_apply_hsv_identity() {
        let adj = HsvAdjustment::default();
        let (r, g, b) = (0.8, 0.3, 0.5);
        let (r2, g2, b2) = apply_hsv_adjustment(r, g, b, &adj);

        assert!((r - r2).abs() < 0.001);
        assert!((g - g2).abs() < 0.001);
        assert!((b - b2).abs() < 0.001);
    }

    #[test]
    fn test_paint_config_basic() {
        let basic = PaintConfig::simple(1, 0);
        assert!(basic.is_basic());

        let complex = PaintConfig::new(
            1,
            0,
            HsvAdjustment::new(30.0, 1.0, 1.0),
            BlendMode::Multiply,
        );
        assert!(!complex.is_basic());
    }
}

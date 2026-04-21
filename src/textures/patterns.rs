//! Procedural texture pattern algorithms.

use serde::{Deserialize, Serialize};

/// Available procedural texture patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum TexturePattern {
    /// Solid color fill.
    #[default]
    Solid = 0,
    /// Horizontal stripes.
    HorizontalStripes = 1,
    /// Vertical stripes.
    VerticalStripes = 2,
    /// Diagonal stripes (45°).
    DiagonalStripes = 3,
    /// Checkerboard pattern.
    Checkerboard = 4,
    /// Horizontal gradient.
    GradientH = 5,
    /// Vertical gradient.
    GradientV = 6,
    /// Radial gradient from center.
    GradientRadial = 7,
    /// Perlin-like noise.
    Noise = 8,
    /// Brick/masonry pattern.
    Brick = 9,
    /// Dot/polka pattern.
    Dots = 10,
    /// Grid lines.
    Grid = 11,
    /// Diamond/rhombus pattern.
    Diamond = 12,
    /// Herringbone pattern.
    Herringbone = 13,
    /// Crosshatch pattern.
    Crosshatch = 14,
    /// Wavy horizontal lines.
    Waves = 15,
}

impl TexturePattern {
    /// Returns all available patterns.
    pub const fn all() -> [TexturePattern; 16] {
        [
            TexturePattern::Solid,
            TexturePattern::HorizontalStripes,
            TexturePattern::VerticalStripes,
            TexturePattern::DiagonalStripes,
            TexturePattern::Checkerboard,
            TexturePattern::GradientH,
            TexturePattern::GradientV,
            TexturePattern::GradientRadial,
            TexturePattern::Noise,
            TexturePattern::Brick,
            TexturePattern::Dots,
            TexturePattern::Grid,
            TexturePattern::Diamond,
            TexturePattern::Herringbone,
            TexturePattern::Crosshatch,
            TexturePattern::Waves,
        ]
    }

    /// Returns display name for UI.
    pub const fn display_name(&self) -> &'static str {
        match *self {
            TexturePattern::Solid => "Solid",
            TexturePattern::HorizontalStripes => "H. Stripes",
            TexturePattern::VerticalStripes => "V. Stripes",
            TexturePattern::DiagonalStripes => "Diagonal",
            TexturePattern::Checkerboard => "Checker",
            TexturePattern::GradientH => "Gradient H",
            TexturePattern::GradientV => "Gradient V",
            TexturePattern::GradientRadial => "Radial",
            TexturePattern::Noise => "Noise",
            TexturePattern::Brick => "Brick",
            TexturePattern::Dots => "Dots",
            TexturePattern::Grid => "Grid",
            TexturePattern::Diamond => "Diamond",
            TexturePattern::Herringbone => "Herring",
            TexturePattern::Crosshatch => "Cross",
            TexturePattern::Waves => "Waves",
        }
    }

    /// Returns a brief description.
    pub const fn description(&self) -> &'static str {
        match *self {
            TexturePattern::Solid => "Solid color fill",
            TexturePattern::HorizontalStripes => "Horizontal stripe lines",
            TexturePattern::VerticalStripes => "Vertical stripe lines",
            TexturePattern::DiagonalStripes => "45° diagonal stripes",
            TexturePattern::Checkerboard => "Alternating squares",
            TexturePattern::GradientH => "Left-to-right color blend",
            TexturePattern::GradientV => "Top-to-bottom color blend",
            TexturePattern::GradientRadial => "Center-outward color blend",
            TexturePattern::Noise => "Organic noise texture",
            TexturePattern::Brick => "Brick/masonry layout",
            TexturePattern::Dots => "Polka dot pattern",
            TexturePattern::Grid => "Grid lines",
            TexturePattern::Diamond => "Diamond/rhombus tiles",
            TexturePattern::Herringbone => "Herringbone weave",
            TexturePattern::Crosshatch => "Crossed diagonal lines",
            TexturePattern::Waves => "Wavy horizontal lines",
        }
    }

    /// Generates the pattern blend factor at (x, y) for a 64x64 texture.
    /// Returns 0.0 for color1, 1.0 for color2, or values in between for blending.
    pub fn sample(&self, x: u32, y: u32, scale: f32, seed: u32) -> f32 {
        let size = 64.0;
        let fx = x as f32 / size;
        let fy = y as f32 / size;
        let scaled_x = (x as f32 * scale) as u32;
        let scaled_y = (y as f32 * scale) as u32;

        match *self {
            TexturePattern::Solid => 0.0,

            TexturePattern::HorizontalStripes => {
                let period = (8.0 / scale).max(2.0) as u32;
                if scaled_y % period < period / 2 {
                    0.0
                } else {
                    1.0
                }
            }

            TexturePattern::VerticalStripes => {
                let period = (8.0 / scale).max(2.0) as u32;
                if scaled_x % period < period / 2 {
                    0.0
                } else {
                    1.0
                }
            }

            TexturePattern::DiagonalStripes => {
                let period = (16.0 / scale).max(4.0) as u32;
                if (scaled_x + scaled_y) % period < period / 2 {
                    0.0
                } else {
                    1.0
                }
            }

            TexturePattern::Checkerboard => {
                let period = (8.0 / scale).max(2.0) as u32;
                let cx = scaled_x / period;
                let cy = scaled_y / period;
                if (cx + cy).is_multiple_of(2) {
                    0.0
                } else {
                    1.0
                }
            }

            TexturePattern::GradientH => fx,

            TexturePattern::GradientV => fy,

            TexturePattern::GradientRadial => {
                let dx = fx - 0.5;
                let dy = fy - 0.5;
                let dist = (dx * dx + dy * dy).sqrt() * 2.0;
                dist.min(1.0)
            }

            TexturePattern::Noise => {
                // Simple value noise using hash
                Self::value_noise(x, y, scale, seed)
            }

            TexturePattern::Brick => {
                let brick_h = (8.0 / scale).max(3.0) as u32;
                let brick_w = brick_h * 2;
                let mortar = 1;

                let row = scaled_y / brick_h;
                let offset = if row.is_multiple_of(2) {
                    0
                } else {
                    brick_w / 2
                };
                let bx = (scaled_x + offset) % brick_w;
                let by = scaled_y % brick_h;

                // Mortar lines
                if by < mortar || bx < mortar { 1.0 } else { 0.0 }
            }

            TexturePattern::Dots => {
                let spacing = (12.0 / scale).max(4.0);
                let radius = spacing * 0.3;
                let cx = (fx * 64.0 + spacing / 2.0) % spacing - spacing / 2.0;
                let cy = (fy * 64.0 + spacing / 2.0) % spacing - spacing / 2.0;
                let dist = (cx * cx + cy * cy).sqrt();
                if dist < radius { 0.0 } else { 1.0 }
            }

            TexturePattern::Grid => {
                let spacing = (8.0 / scale).max(2.0) as u32;
                let line_width = 1;
                if scaled_x % spacing < line_width || scaled_y % spacing < line_width {
                    0.0
                } else {
                    1.0
                }
            }

            TexturePattern::Diamond => {
                let size_d = (16.0 / scale).max(4.0) as u32;
                let half = size_d / 2;
                let dx = (scaled_x % size_d) as i32 - half as i32;
                let dy = (scaled_y % size_d) as i32 - half as i32;
                if dx.abs() + dy.abs() < half as i32 {
                    0.0
                } else {
                    1.0
                }
            }

            TexturePattern::Herringbone => {
                let block = (8.0 / scale).max(2.0) as u32;
                let segment = scaled_x / block + scaled_y / block;
                if segment.is_multiple_of(2) {
                    if scaled_x % block < scaled_y % block {
                        0.0
                    } else {
                        1.0
                    }
                } else if scaled_x % block > scaled_y % block {
                    0.0
                } else {
                    1.0
                }
            }

            TexturePattern::Crosshatch => {
                let spacing = (12.0 / scale).max(3.0) as u32;
                let d1 = (scaled_x + scaled_y) % spacing;
                let d2 = (scaled_x + spacing - scaled_y % spacing) % spacing;
                if d1 < 2 || d2 < 2 { 0.0 } else { 1.0 }
            }

            TexturePattern::Waves => {
                let freq = scale * 0.5;
                let amp = 4.0 / scale;
                let wave_y = (fy * 64.0 + (fx * 64.0 * freq).sin() * amp) as u32;
                let spacing = (8.0 / scale).max(2.0) as u32;
                if wave_y % spacing < spacing / 2 {
                    0.0
                } else {
                    1.0
                }
            }
        }
    }

    /// Simple value noise for organic textures.
    fn value_noise(x: u32, y: u32, scale: f32, seed: u32) -> f32 {
        let grid_size = (8.0 / scale).max(2.0);
        let gx = (x as f32 / grid_size) as u32;
        let gy = (y as f32 / grid_size) as u32;

        // Fractional position within grid cell
        let fx = (x as f32 / grid_size).fract();
        let fy = (y as f32 / grid_size).fract();

        // Smooth interpolation
        let sx = fx * fx * (3.0 - 2.0 * fx);
        let sy = fy * fy * (3.0 - 2.0 * fy);

        // Hash corners
        let n00 = Self::hash(gx, gy, seed);
        let n10 = Self::hash(gx + 1, gy, seed);
        let n01 = Self::hash(gx, gy + 1, seed);
        let n11 = Self::hash(gx + 1, gy + 1, seed);

        // Bilinear interpolation
        let nx0 = n00 * (1.0 - sx) + n10 * sx;
        let nx1 = n01 * (1.0 - sx) + n11 * sx;
        nx0 * (1.0 - sy) + nx1 * sy
    }

    /// Simple hash function for noise.
    fn hash(x: u32, y: u32, seed: u32) -> f32 {
        let n =
            x.wrapping_mul(374761393) ^ y.wrapping_mul(668265263) ^ seed.wrapping_mul(1013904223);
        let n = n.wrapping_mul(n);
        (n as f32) / (u32::MAX as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_sample_bounds() {
        for pattern in TexturePattern::all() {
            for x in 0..64 {
                for y in 0..64 {
                    let v = pattern.sample(x, y, 1.0, 0);
                    assert!(
                        (0.0..=1.0).contains(&v),
                        "{:?} out of bounds at ({}, {}): {}",
                        pattern,
                        x,
                        y,
                        v
                    );
                }
            }
        }
    }

    #[test]
    fn test_solid_is_uniform() {
        for x in 0..64 {
            for y in 0..64 {
                assert_eq!(TexturePattern::Solid.sample(x, y, 1.0, 0), 0.0);
            }
        }
    }

    #[test]
    fn test_checkerboard_alternates() {
        // At scale 1.0, period is 8, so check corners of 8x8 blocks
        let a = TexturePattern::Checkerboard.sample(0, 0, 1.0, 0);
        let b = TexturePattern::Checkerboard.sample(8, 0, 1.0, 0);
        assert_ne!(a, b, "Adjacent checker blocks should differ");
    }

    #[test]
    fn test_gradient_h_increases() {
        let left = TexturePattern::GradientH.sample(0, 32, 1.0, 0);
        let right = TexturePattern::GradientH.sample(63, 32, 1.0, 0);
        assert!(right > left, "Gradient should increase left to right");
    }

    #[test]
    fn test_gradient_v_increases() {
        let top = TexturePattern::GradientV.sample(32, 0, 1.0, 0);
        let bottom = TexturePattern::GradientV.sample(32, 63, 1.0, 0);
        assert!(bottom > top, "Gradient should increase top to bottom");
    }

    #[test]
    fn test_noise_varies() {
        let mut values = std::collections::HashSet::new();
        for x in 0..64 {
            let v = (TexturePattern::Noise.sample(x, 0, 1.0, 12345) * 100.0) as u32;
            values.insert(v);
        }
        assert!(values.len() > 5, "Noise should have variety");
    }
}

//! CPU-based software rasterizer for stencil thumbnails.
//!
//! Generates 64×64 PNG thumbnails with a wireframe/point cloud style
//! to distinguish stencils from templates.

use crate::stencils::format::StencilFile;
use image::{ImageBuffer, Rgba};
use std::io;
use std::path::Path;

/// Default thumbnail size for stencil sprites.
pub const THUMBNAIL_SIZE: u32 = 64;

/// Stencil wireframe color (cyan, matches default stencil color).
const STENCIL_COLOR: [u8; 4] = [0, 255, 255, 255];

/// Simple depth buffer for wireframe rendering.
struct WireframeRenderer {
    color_buffer: Vec<u8>,
    depth_buffer: Vec<f32>,
    width: usize,
    height: usize,
}

impl WireframeRenderer {
    fn new(width: usize, height: usize) -> Self {
        Self {
            color_buffer: vec![0; width * height * 4],
            depth_buffer: vec![f32::MAX; width * height],
            width,
            height,
        }
    }

    fn clear(&mut self, bg_color: [u8; 4]) {
        for i in 0..self.width * self.height {
            self.color_buffer[i * 4] = bg_color[0];
            self.color_buffer[i * 4 + 1] = bg_color[1];
            self.color_buffer[i * 4 + 2] = bg_color[2];
            self.color_buffer[i * 4 + 3] = bg_color[3];
            self.depth_buffer[i] = f32::MAX;
        }
    }

    fn set_pixel_with_depth(&mut self, x: usize, y: usize, depth: f32, color: [u8; 4]) {
        if x < self.width && y < self.height {
            let idx = y * self.width + x;
            if depth < self.depth_buffer[idx] {
                self.depth_buffer[idx] = depth;
                self.color_buffer[idx * 4] = color[0];
                self.color_buffer[idx * 4 + 1] = color[1];
                self.color_buffer[idx * 4 + 2] = color[2];
                self.color_buffer[idx * 4 + 3] = color[3];
            }
        }
    }

    fn color_buffer(&self) -> &[u8] {
        &self.color_buffer
    }
}

/// Generates a thumbnail image for a stencil and saves it to the specified path.
///
/// Uses a wireframe/point style to visually distinguish from template thumbnails.
pub fn generate_stencil_thumbnail(stencil: &StencilFile, output_path: &Path) -> io::Result<()> {
    let size = THUMBNAIL_SIZE as usize;
    let mut renderer = WireframeRenderer::new(size, size);

    // Dark transparent background
    renderer.clear([20, 30, 40, 200]);

    // Calculate adaptive cell size based on stencil dimensions
    let max_dim = stencil.width.max(stencil.height).max(stencil.depth);
    let cell_size = calculate_cell_size(max_dim, size);

    // Center the stencil in the image
    let stencil_center_x = stencil.width as f32 / 2.0;
    let stencil_center_y = stencil.height as f32 / 2.0;
    let stencil_center_z = stencil.depth as f32 / 2.0;

    let center_x = size as f32 / 2.0;
    let center_y = size as f32 / 2.0 + size as f32 * 0.1;

    // Fixed 3/4 view angle (45 degrees)
    let orbit_yaw: f32 = std::f32::consts::PI / 4.0;
    let cos_yaw = orbit_yaw.cos();
    let sin_yaw = orbit_yaw.sin();

    let iso_y = [0.0, -cell_size];
    let base_x = [cell_size * 0.866, cell_size * 0.5];
    let base_z = [-cell_size * 0.866, cell_size * 0.5];

    let iso_x = [
        base_x[0] * cos_yaw - base_z[0] * sin_yaw,
        base_x[1] * cos_yaw - base_z[1] * sin_yaw,
    ];
    let iso_z = [
        base_x[0] * sin_yaw + base_z[0] * cos_yaw,
        base_x[1] * sin_yaw + base_z[1] * cos_yaw,
    ];

    // Project 3D point to screen coordinates with depth
    let project = |x: f32, y: f32, z: f32| -> [f32; 3] {
        let cx = x - stencil_center_x;
        let cy = y - stencil_center_y;
        let cz = z - stencil_center_z;

        let screen_x = center_x + iso_x[0] * cx + iso_z[0] * cz;
        let screen_y = center_y + iso_x[1] * cx + iso_z[1] * cz + iso_y[1] * cy;
        let depth = -(cx * cos_yaw + cz * sin_yaw) - (cz * cos_yaw - cx * sin_yaw) + cy * 0.01;

        [screen_x, screen_y, depth]
    };

    // Draw all positions as wireframe cubes
    for pos in &stencil.positions {
        let x_f = pos.x as f32;
        let y_f = pos.y as f32;
        let z_f = pos.z as f32;

        // Draw wireframe edges for each position
        draw_wireframe_cube(&mut renderer, x_f, y_f, z_f, &project, STENCIL_COLOR);
    }

    // Save to PNG
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(size as u32, size as u32, renderer.color_buffer().to_vec())
            .ok_or_else(|| io::Error::other("Failed to create image buffer"))?;

    img.save(output_path)
        .map_err(|e| io::Error::other(format!("Failed to save thumbnail: {}", e)))?;

    Ok(())
}

/// Calculate adaptive cell size based on stencil dimensions.
fn calculate_cell_size(max_dimension: u8, target_size: usize) -> f32 {
    let scale_factor = 1.5;
    target_size as f32 / (max_dimension as f32 * scale_factor)
}

/// Draws a wireframe cube at the given position.
fn draw_wireframe_cube<F>(
    renderer: &mut WireframeRenderer,
    x: f32,
    y: f32,
    z: f32,
    project: &F,
    color: [u8; 4],
) where
    F: Fn(f32, f32, f32) -> [f32; 3],
{
    // 8 corners of the cube
    let corners = [
        project(x, y, z),
        project(x + 1.0, y, z),
        project(x + 1.0, y + 1.0, z),
        project(x, y + 1.0, z),
        project(x, y, z + 1.0),
        project(x + 1.0, y, z + 1.0),
        project(x + 1.0, y + 1.0, z + 1.0),
        project(x, y + 1.0, z + 1.0),
    ];

    // 12 edges of a cube
    let edges: [(usize, usize); 12] = [
        // Bottom face
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        // Top face
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        // Vertical edges
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];

    for (i, j) in edges {
        draw_line(renderer, corners[i], corners[j], color);
    }
}

/// Draws a line between two 3D points.
fn draw_line(renderer: &mut WireframeRenderer, from: [f32; 3], to: [f32; 3], color: [u8; 4]) {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let dz = to[2] - from[2];

    let steps = (dx.abs().max(dy.abs()) as i32).max(1);

    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = from[0] + dx * t;
        let y = from[1] + dy * t;
        let z = from[2] + dz * t;

        let px = x.round() as i32;
        let py = y.round() as i32;

        if px >= 0 && px < renderer.width as i32 && py >= 0 && py < renderer.height as i32 {
            renderer.set_pixel_with_depth(px as usize, py as usize, z, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stencils::format::StencilPosition;
    use std::env;

    #[test]
    fn test_generate_thumbnail() {
        let mut stencil = StencilFile::new(
            "test_stencil".to_string(),
            "test_author".to_string(),
            3,
            3,
            3,
        );
        stencil.positions.push(StencilPosition { x: 0, y: 0, z: 0 });
        stencil.positions.push(StencilPosition { x: 1, y: 1, z: 1 });
        stencil.positions.push(StencilPosition { x: 2, y: 2, z: 2 });

        let temp_dir = env::temp_dir().join("voxel_world_stencil_thumb_test");
        let _ = std::fs::create_dir_all(&temp_dir);
        let output_path = temp_dir.join("test_stencil.png");

        generate_stencil_thumbnail(&stencil, &output_path).unwrap();

        assert!(output_path.exists());

        // Verify image dimensions
        let img = image::open(&output_path).unwrap();
        assert_eq!(img.width(), THUMBNAIL_SIZE);
        assert_eq!(img.height(), THUMBNAIL_SIZE);

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

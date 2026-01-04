//! Software rasterizer with z-buffer for editor viewport rendering.
//!
//! Renders 3D voxel cubes to a 2D image with proper depth testing,
//! avoiding the depth sorting issues of painter's algorithm.

use crate::sub_voxel::{SUB_VOXEL_SIZE, SubVoxelModel};
use egui_winit_vulkano::egui;
use image::{ImageBuffer, Rgba};
use std::path::Path;

/// A simple software rasterizer with z-buffer.
pub struct Rasterizer {
    /// Color buffer (RGBA)
    color_buffer: Vec<u8>,
    /// Depth buffer (f32, smaller = closer)
    depth_buffer: Vec<f32>,
    /// Width in pixels
    width: usize,
    /// Height in pixels
    height: usize,
}

/// A 3D vertex with position and color.
#[derive(Clone, Copy)]
struct Vertex {
    pos: [f32; 3],
    color: [u8; 4],
}

impl Rasterizer {
    /// Creates a new rasterizer with the given dimensions.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            color_buffer: vec![0; width * height * 4],
            depth_buffer: vec![f32::MAX; width * height],
            width,
            height,
        }
    }

    /// Clears the buffers with a background color.
    pub fn clear(&mut self, bg_color: [u8; 4]) {
        for i in 0..self.width * self.height {
            self.color_buffer[i * 4] = bg_color[0];
            self.color_buffer[i * 4 + 1] = bg_color[1];
            self.color_buffer[i * 4 + 2] = bg_color[2];
            self.color_buffer[i * 4 + 3] = bg_color[3];
            self.depth_buffer[i] = f32::MAX;
        }
    }

    /// Resizes the rasterizer if dimensions changed.
    pub fn resize(&mut self, width: usize, height: usize) {
        if width != self.width || height != self.height {
            self.width = width;
            self.height = height;
            self.color_buffer = vec![0; width * height * 4];
            self.depth_buffer = vec![f32::MAX; width * height];
        }
    }

    /// Gets the rendered image as an egui ColorImage.
    pub fn get_image(&self) -> egui::ColorImage {
        egui::ColorImage::from_rgba_unmultiplied([self.width, self.height], &self.color_buffer)
    }

    /// Draws a filled triangle with z-buffer depth testing.
    fn draw_triangle(&mut self, v0: Vertex, v1: Vertex, v2: Vertex) {
        // Bounding box
        let min_x = v0.pos[0].min(v1.pos[0]).min(v2.pos[0]).max(0.0) as i32;
        let max_x = v0.pos[0]
            .max(v1.pos[0])
            .max(v2.pos[0])
            .min(self.width as f32 - 1.0) as i32;
        let min_y = v0.pos[1].min(v1.pos[1]).min(v2.pos[1]).max(0.0) as i32;
        let max_y = v0.pos[1]
            .max(v1.pos[1])
            .max(v2.pos[1])
            .min(self.height as f32 - 1.0) as i32;

        // Edge function for barycentric coordinates
        let edge = |a: &[f32; 3], b: &[f32; 3], c: (f32, f32)| -> f32 {
            (c.0 - a[0]) * (b[1] - a[1]) - (c.1 - a[1]) * (b[0] - a[0])
        };

        let area = edge(&v0.pos, &v1.pos, (v2.pos[0], v2.pos[1]));
        if area.abs() < 0.001 {
            return; // Degenerate triangle
        }
        let inv_area = 1.0 / area;

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;

                // Barycentric coordinates
                let w0 = edge(&v1.pos, &v2.pos, (px, py)) * inv_area;
                let w1 = edge(&v2.pos, &v0.pos, (px, py)) * inv_area;
                let w2 = edge(&v0.pos, &v1.pos, (px, py)) * inv_area;

                // Check if point is inside triangle
                if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                    // Interpolate depth
                    let depth = w0 * v0.pos[2] + w1 * v1.pos[2] + w2 * v2.pos[2];

                    let idx = y as usize * self.width + x as usize;
                    if depth < self.depth_buffer[idx] {
                        self.depth_buffer[idx] = depth;

                        // Use flat shading (v0's color for the whole face)
                        self.color_buffer[idx * 4] = v0.color[0];
                        self.color_buffer[idx * 4 + 1] = v0.color[1];
                        self.color_buffer[idx * 4 + 2] = v0.color[2];
                        self.color_buffer[idx * 4 + 3] = v0.color[3];
                    }
                }
            }
        }
    }

    /// Draws a quad (two triangles) with z-buffer depth testing.
    fn draw_quad(&mut self, v0: Vertex, v1: Vertex, v2: Vertex, v3: Vertex) {
        self.draw_triangle(v0, v1, v2);
        self.draw_triangle(v0, v2, v3);
    }

    /// Draws a line with z-buffer depth testing.
    fn draw_line(&mut self, v0: Vertex, v1: Vertex) {
        let dx = (v1.pos[0] - v0.pos[0]).abs();
        let dy = (v1.pos[1] - v0.pos[1]).abs();
        let steps = dx.max(dy) as i32 + 1;

        for i in 0..=steps {
            let t = if steps > 0 {
                i as f32 / steps as f32
            } else {
                0.0
            };
            let x = (v0.pos[0] + t * (v1.pos[0] - v0.pos[0])) as i32;
            let y = (v0.pos[1] + t * (v1.pos[1] - v0.pos[1])) as i32;
            let z = v0.pos[2] + t * (v1.pos[2] - v0.pos[2]);

            if x >= 0 && x < self.width as i32 && y >= 0 && y < self.height as i32 {
                let idx = y as usize * self.width + x as usize;
                // Lines get slight depth bias to draw on top
                if z - 0.001 < self.depth_buffer[idx] {
                    self.color_buffer[idx * 4] = v0.color[0];
                    self.color_buffer[idx * 4 + 1] = v0.color[1];
                    self.color_buffer[idx * 4 + 2] = v0.color[2];
                    self.color_buffer[idx * 4 + 3] = v0.color[3];
                }
            }
        }
    }
}

/// Renders the model and returns render info for interaction.
pub struct RenderResult {
    /// The rendered image
    pub image: egui::ColorImage,
    /// Map from screen position to (voxel_pos, face_normal)
    /// Stored as flat array [width * height], each entry is Option<(voxel, normal)>
    pub hit_map: Vec<Option<HitInfo>>,
    /// Width of the image
    pub width: usize,
    /// Height of the image
    pub height: usize,
}

/// Hit map with depth tracking for proper closest-face detection.
struct HitMapBuilder {
    hits: Vec<Option<HitInfo>>,
    depths: Vec<f32>,
    width: usize,
}

impl HitMapBuilder {
    fn new(width: usize, height: usize) -> Self {
        Self {
            hits: vec![None; width * height],
            depths: vec![f32::MAX; width * height],
            width,
        }
    }

    /// Store hit info only if this face is closer than what's already stored.
    fn store_if_closer(&mut self, x: usize, y: usize, depth: f32, info: HitInfo) {
        let idx = y * self.width + x;
        if depth < self.depths[idx] {
            self.depths[idx] = depth;
            self.hits[idx] = Some(info);
        }
    }

    fn into_hits(self) -> Vec<Option<HitInfo>> {
        self.hits
    }
}

/// Information about what was hit at a screen position.
#[derive(Clone, Copy)]
pub struct HitInfo {
    /// Voxel coordinates (x, y, z)
    pub voxel: [i32; 3],
    /// Face normal (which face was hit)
    pub normal: [i32; 3],
    /// Whether this is a floor tile (not a voxel)
    pub is_floor: bool,
}

/// Renders the model with proper 3D and returns interactive hit information.
pub fn render_model(
    model: &SubVoxelModel,
    width: usize,
    height: usize,
    orbit_yaw: f32,
    hovered_voxel: Option<[i32; 3]>,
    hovered_normal: Option<[i32; 3]>,
    mirror_axes: [bool; 3],
) -> RenderResult {
    let mut rasterizer = Rasterizer::new(width, height);
    let mut hit_map_builder = HitMapBuilder::new(width, height);

    // Background color
    rasterizer.clear([30, 30, 30, 255]);

    // Isometric projection setup
    let size = width.min(height) as f32 - 20.0;
    let cell_size = size / 16.0; // Zoomed out slightly to fit rotated models
    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;

    let iso_y = [0.0, -cell_size];

    // Calculate rotated isometric axes
    let cos_yaw = orbit_yaw.cos();
    let sin_yaw = orbit_yaw.sin();

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

    let model_center = 4.0;

    // Project 3D point to screen coordinates with depth
    let project = |x: f32, y: f32, z: f32| -> [f32; 3] {
        let cx = x - model_center;
        let cy = y - model_center;
        let cz = z - model_center;

        let screen_x = center_x + iso_x[0] * cx + iso_z[0] * cz;
        let screen_y = center_y + iso_x[1] * cx + iso_z[1] * cz + iso_y[1] * cy;

        // Depth: use rotated position for proper sorting
        let depth = -(cx * cos_yaw + cz * sin_yaw) - (cz * cos_yaw - cx * sin_yaw) + cy * 0.01;

        [screen_x, screen_y, depth]
    };

    // Helper to create a vertex
    let make_vertex = |x: f32, y: f32, z: f32, color: [u8; 4]| -> Vertex {
        let pos = project(x, y, z);
        Vertex { pos, color }
    };

    // Draw floor grid first (at Y=0)
    for z in 0..SUB_VOXEL_SIZE {
        for x in 0..SUB_VOXEL_SIZE {
            let x_f = x as f32;
            let z_f = z as f32;

            let checker = if (x + z) % 2 == 0 {
                [50, 55, 65, 180]
            } else {
                [60, 70, 80, 200]
            };

            let v0 = make_vertex(x_f, 0.0, z_f, checker);
            let v1 = make_vertex(x_f + 1.0, 0.0, z_f, checker);
            let v2 = make_vertex(x_f + 1.0, 0.0, z_f + 1.0, checker);
            let v3 = make_vertex(x_f, 0.0, z_f + 1.0, checker);

            rasterizer.draw_quad(v0, v1, v2, v3);

            // Store hit info for floor tiles (if empty above)
            if model.get_voxel(x, 0, z) == 0 {
                store_quad_hits_with_depth(
                    &mut hit_map_builder,
                    width,
                    height,
                    v0.pos,
                    v1.pos,
                    v2.pos,
                    v3.pos,
                    HitInfo {
                        voxel: [x as i32, 0, z as i32],
                        normal: [0, 1, 0],
                        is_floor: true,
                    },
                );
            }
        }
    }

    // Draw mirror plane indicators (wireframe only)
    let mirror_center = 4.0; // Center of 8x8x8 grid
    let mirror_size = 8.0;

    // X mirror plane (YZ plane at x=4) - Red wireframe
    if mirror_axes[0] {
        let line_color = [255, 80, 80, 255];
        let l0 = make_vertex(mirror_center, 0.0, 0.0, line_color);
        let l1 = make_vertex(mirror_center, mirror_size, 0.0, line_color);
        let l2 = make_vertex(mirror_center, mirror_size, mirror_size, line_color);
        let l3 = make_vertex(mirror_center, 0.0, mirror_size, line_color);
        rasterizer.draw_line(l0, l1);
        rasterizer.draw_line(l1, l2);
        rasterizer.draw_line(l2, l3);
        rasterizer.draw_line(l3, l0);
        // Cross lines for visibility
        rasterizer.draw_line(l0, l2);
        rasterizer.draw_line(l1, l3);
    }

    // Y mirror plane (XZ plane at y=4) - Green wireframe
    if mirror_axes[1] {
        let line_color = [80, 255, 80, 255];
        let l0 = make_vertex(0.0, mirror_center, 0.0, line_color);
        let l1 = make_vertex(mirror_size, mirror_center, 0.0, line_color);
        let l2 = make_vertex(mirror_size, mirror_center, mirror_size, line_color);
        let l3 = make_vertex(0.0, mirror_center, mirror_size, line_color);
        rasterizer.draw_line(l0, l1);
        rasterizer.draw_line(l1, l2);
        rasterizer.draw_line(l2, l3);
        rasterizer.draw_line(l3, l0);
        // Cross lines for visibility
        rasterizer.draw_line(l0, l2);
        rasterizer.draw_line(l1, l3);
    }

    // Z mirror plane (XY plane at z=4) - Blue wireframe
    if mirror_axes[2] {
        let line_color = [80, 80, 255, 255];
        let l0 = make_vertex(0.0, 0.0, mirror_center, line_color);
        let l1 = make_vertex(mirror_size, 0.0, mirror_center, line_color);
        let l2 = make_vertex(mirror_size, mirror_size, mirror_center, line_color);
        let l3 = make_vertex(0.0, mirror_size, mirror_center, line_color);
        rasterizer.draw_line(l0, l1);
        rasterizer.draw_line(l1, l2);
        rasterizer.draw_line(l2, l3);
        rasterizer.draw_line(l3, l0);
        // Cross lines for visibility
        rasterizer.draw_line(l0, l2);
        rasterizer.draw_line(l1, l3);
    }

    // Draw axis lines
    let axis_origin = make_vertex(0.0, 0.0, 0.0, [255, 255, 255, 255]);
    let axis_x = make_vertex(2.0, 0.0, 0.0, [255, 0, 0, 255]);
    let axis_y = make_vertex(0.0, 2.0, 0.0, [0, 255, 0, 255]);
    let axis_z = make_vertex(0.0, 0.0, 2.0, [0, 0, 255, 255]);

    // Draw axis with red origin
    let red_origin = Vertex {
        pos: axis_origin.pos,
        color: [255, 0, 0, 255],
    };
    let green_origin = Vertex {
        pos: axis_origin.pos,
        color: [0, 255, 0, 255],
    };
    let blue_origin = Vertex {
        pos: axis_origin.pos,
        color: [0, 0, 255, 255],
    };

    rasterizer.draw_line(red_origin, axis_x);
    rasterizer.draw_line(green_origin, axis_y);
    rasterizer.draw_line(blue_origin, axis_z);

    // Draw all voxels with proper z-buffer - render ALL 6 faces
    for y in 0..SUB_VOXEL_SIZE {
        for z in 0..SUB_VOXEL_SIZE {
            for x in 0..SUB_VOXEL_SIZE {
                let idx = model.get_voxel(x, y, z);
                if idx == 0 {
                    continue;
                }

                let color = &model.palette[idx as usize];
                let x_f = x as f32;
                let y_f = y as f32;
                let z_f = z as f32;

                // Face colors with shading (lighting from upper-left)
                let top_color = [color.r, color.g, color.b, color.a];
                let bottom_color = [
                    (color.r as f32 * 0.4) as u8,
                    (color.g as f32 * 0.4) as u8,
                    (color.b as f32 * 0.4) as u8,
                    color.a,
                ];
                let xp_color = [
                    (color.r as f32 * 0.7) as u8,
                    (color.g as f32 * 0.7) as u8,
                    (color.b as f32 * 0.7) as u8,
                    color.a,
                ];
                let xm_color = [
                    (color.r as f32 * 0.8) as u8,
                    (color.g as f32 * 0.8) as u8,
                    (color.b as f32 * 0.8) as u8,
                    color.a,
                ];
                let zp_color = [
                    (color.r as f32 * 0.6) as u8,
                    (color.g as f32 * 0.6) as u8,
                    (color.b as f32 * 0.6) as u8,
                    color.a,
                ];
                let zm_color = [
                    (color.r as f32 * 0.85) as u8,
                    (color.g as f32 * 0.85) as u8,
                    (color.b as f32 * 0.85) as u8,
                    color.a,
                ];

                // 8 corners of the cube
                let p000 = make_vertex(x_f, y_f, z_f, top_color);
                let p100 = make_vertex(x_f + 1.0, y_f, z_f, top_color);
                let p110 = make_vertex(x_f + 1.0, y_f + 1.0, z_f, top_color);
                let p010 = make_vertex(x_f, y_f + 1.0, z_f, top_color);
                let p001 = make_vertex(x_f, y_f, z_f + 1.0, top_color);
                let p101 = make_vertex(x_f + 1.0, y_f, z_f + 1.0, top_color);
                let p111 = make_vertex(x_f + 1.0, y_f + 1.0, z_f + 1.0, top_color);
                let p011 = make_vertex(x_f, y_f + 1.0, z_f + 1.0, top_color);

                // Top face (+Y)
                draw_face(
                    &mut rasterizer,
                    &mut hit_map_builder,
                    width,
                    height,
                    [p010.pos, p110.pos, p111.pos, p011.pos],
                    top_color,
                    HitInfo {
                        voxel: [x as i32, y as i32, z as i32],
                        normal: [0, 1, 0],
                        is_floor: false,
                    },
                );

                // Bottom face (-Y)
                draw_face(
                    &mut rasterizer,
                    &mut hit_map_builder,
                    width,
                    height,
                    [p000.pos, p001.pos, p101.pos, p100.pos],
                    bottom_color,
                    HitInfo {
                        voxel: [x as i32, y as i32, z as i32],
                        normal: [0, -1, 0],
                        is_floor: false,
                    },
                );

                // X+ face
                draw_face(
                    &mut rasterizer,
                    &mut hit_map_builder,
                    width,
                    height,
                    [p100.pos, p101.pos, p111.pos, p110.pos],
                    xp_color,
                    HitInfo {
                        voxel: [x as i32, y as i32, z as i32],
                        normal: [1, 0, 0],
                        is_floor: false,
                    },
                );

                // X- face
                draw_face(
                    &mut rasterizer,
                    &mut hit_map_builder,
                    width,
                    height,
                    [p000.pos, p010.pos, p011.pos, p001.pos],
                    xm_color,
                    HitInfo {
                        voxel: [x as i32, y as i32, z as i32],
                        normal: [-1, 0, 0],
                        is_floor: false,
                    },
                );

                // Z+ face
                draw_face(
                    &mut rasterizer,
                    &mut hit_map_builder,
                    width,
                    height,
                    [p001.pos, p011.pos, p111.pos, p101.pos],
                    zp_color,
                    HitInfo {
                        voxel: [x as i32, y as i32, z as i32],
                        normal: [0, 0, 1],
                        is_floor: false,
                    },
                );

                // Z- face
                draw_face(
                    &mut rasterizer,
                    &mut hit_map_builder,
                    width,
                    height,
                    [p000.pos, p100.pos, p110.pos, p010.pos],
                    zm_color,
                    HitInfo {
                        voxel: [x as i32, y as i32, z as i32],
                        normal: [0, 0, -1],
                        is_floor: false,
                    },
                );
            }
        }
    }

    // Draw highlight for hovered voxel/face
    if let (Some(voxel), Some(normal)) = (hovered_voxel, hovered_normal) {
        let highlight_color = [255, 255, 0, 180];
        let x_f = voxel[0] as f32;
        let y_f = voxel[1] as f32;
        let z_f = voxel[2] as f32;

        // Check if this is a floor tile (empty voxel position)
        let is_floor = voxel[0] >= 0
            && voxel[0] < SUB_VOXEL_SIZE as i32
            && voxel[1] >= 0
            && voxel[1] < SUB_VOXEL_SIZE as i32
            && voxel[2] >= 0
            && voxel[2] < SUB_VOXEL_SIZE as i32
            && model.get_voxel(voxel[0] as usize, voxel[1] as usize, voxel[2] as usize) == 0;

        // Get face vertices based on normal
        let face_verts: Option<[Vertex; 4]> = if is_floor {
            // Floor tile: draw highlight at Y=0 (floor level)
            Some([
                make_vertex(x_f, 0.0, z_f, highlight_color),
                make_vertex(x_f + 1.0, 0.0, z_f, highlight_color),
                make_vertex(x_f + 1.0, 0.0, z_f + 1.0, highlight_color),
                make_vertex(x_f, 0.0, z_f + 1.0, highlight_color),
            ])
        } else {
            match (normal[0], normal[1], normal[2]) {
                (0, 1, 0) => {
                    // Top face (+Y)
                    Some([
                        make_vertex(x_f, y_f + 1.0, z_f, highlight_color),
                        make_vertex(x_f + 1.0, y_f + 1.0, z_f, highlight_color),
                        make_vertex(x_f + 1.0, y_f + 1.0, z_f + 1.0, highlight_color),
                        make_vertex(x_f, y_f + 1.0, z_f + 1.0, highlight_color),
                    ])
                }
                (1, 0, 0) => {
                    // +X face
                    Some([
                        make_vertex(x_f + 1.0, y_f, z_f, highlight_color),
                        make_vertex(x_f + 1.0, y_f, z_f + 1.0, highlight_color),
                        make_vertex(x_f + 1.0, y_f + 1.0, z_f + 1.0, highlight_color),
                        make_vertex(x_f + 1.0, y_f + 1.0, z_f, highlight_color),
                    ])
                }
                (-1, 0, 0) => {
                    // -X face
                    Some([
                        make_vertex(x_f, y_f, z_f, highlight_color),
                        make_vertex(x_f, y_f + 1.0, z_f, highlight_color),
                        make_vertex(x_f, y_f + 1.0, z_f + 1.0, highlight_color),
                        make_vertex(x_f, y_f, z_f + 1.0, highlight_color),
                    ])
                }
                (0, 0, 1) => {
                    // +Z face
                    Some([
                        make_vertex(x_f, y_f, z_f + 1.0, highlight_color),
                        make_vertex(x_f + 1.0, y_f, z_f + 1.0, highlight_color),
                        make_vertex(x_f + 1.0, y_f + 1.0, z_f + 1.0, highlight_color),
                        make_vertex(x_f, y_f + 1.0, z_f + 1.0, highlight_color),
                    ])
                }
                (0, 0, -1) => {
                    // -Z face
                    Some([
                        make_vertex(x_f, y_f, z_f, highlight_color),
                        make_vertex(x_f + 1.0, y_f, z_f, highlight_color),
                        make_vertex(x_f + 1.0, y_f + 1.0, z_f, highlight_color),
                        make_vertex(x_f, y_f + 1.0, z_f, highlight_color),
                    ])
                }
                _ => None,
            }
        };

        if let Some(verts) = face_verts {
            // Draw with slight depth bias to show on top
            let mut biased_verts = verts;
            for v in &mut biased_verts {
                v.pos[2] -= 0.1; // Bias toward camera
            }
            rasterizer.draw_quad(
                biased_verts[0],
                biased_verts[1],
                biased_verts[2],
                biased_verts[3],
            );
        }
    }

    RenderResult {
        image: rasterizer.get_image(),
        hit_map: hit_map_builder.into_hits(),
        width,
        height,
    }
}

/// Helper to draw a face and store hit info.
fn draw_face(
    rasterizer: &mut Rasterizer,
    hit_map_builder: &mut HitMapBuilder,
    width: usize,
    height: usize,
    positions: [[f32; 3]; 4],
    color: [u8; 4],
    hit_info: HitInfo,
) {
    let v0 = Vertex {
        pos: positions[0],
        color,
    };
    let v1 = Vertex {
        pos: positions[1],
        color,
    };
    let v2 = Vertex {
        pos: positions[2],
        color,
    };
    let v3 = Vertex {
        pos: positions[3],
        color,
    };
    rasterizer.draw_quad(v0, v1, v2, v3);
    store_quad_hits_with_depth(
        hit_map_builder,
        width,
        height,
        positions[0],
        positions[1],
        positions[2],
        positions[3],
        hit_info,
    );
}

/// Stores hit information for pixels inside a quad, using depth to keep closest face.
#[allow(clippy::too_many_arguments)]
fn store_quad_hits_with_depth(
    hit_map_builder: &mut HitMapBuilder,
    width: usize,
    height: usize,
    p0: [f32; 3],
    p1: [f32; 3],
    p2: [f32; 3],
    p3: [f32; 3],
    info: HitInfo,
) {
    // Bounding box
    let min_x = p0[0].min(p1[0]).min(p2[0]).min(p3[0]).max(0.0) as i32;
    let max_x = p0[0]
        .max(p1[0])
        .max(p2[0])
        .max(p3[0])
        .min((width - 1) as f32) as i32;
    let min_y = p0[1].min(p1[1]).min(p2[1]).min(p3[1]).max(0.0) as i32;
    let max_y = p0[1]
        .max(p1[1])
        .max(p2[1])
        .max(p3[1])
        .min((height - 1) as f32) as i32;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;

            // Check if point is in quad and compute barycentric coords for depth interpolation
            if let Some(depth) = point_in_quad_with_depth((px, py), p0, p1, p2, p3) {
                hit_map_builder.store_if_closer(x as usize, y as usize, depth, info);
            }
        }
    }
}

/// Check if a point is inside a 2D quadrilateral and return interpolated depth.
/// Returns Some(depth) if inside, None if outside.
fn point_in_quad_with_depth(
    p: (f32, f32),
    v0: [f32; 3],
    v1: [f32; 3],
    v2: [f32; 3],
    v3: [f32; 3],
) -> Option<f32> {
    // Try triangle v0-v1-v2
    if let Some(depth) = point_in_triangle_with_depth(p, v0, v1, v2) {
        return Some(depth);
    }
    // Try triangle v0-v2-v3
    if let Some(depth) = point_in_triangle_with_depth(p, v0, v2, v3) {
        return Some(depth);
    }
    None
}

/// Check if a point is inside a triangle and return interpolated depth using barycentric coordinates.
fn point_in_triangle_with_depth(
    p: (f32, f32),
    v0: [f32; 3],
    v1: [f32; 3],
    v2: [f32; 3],
) -> Option<f32> {
    // Edge function for barycentric coordinates
    let edge = |a: [f32; 3], b: [f32; 3], c: (f32, f32)| -> f32 {
        (c.0 - a[0]) * (b[1] - a[1]) - (c.1 - a[1]) * (b[0] - a[0])
    };

    let area = edge(v0, v1, (v2[0], v2[1]));
    if area.abs() < 0.001 {
        return None; // Degenerate triangle
    }
    let inv_area = 1.0 / area;

    // Barycentric coordinates
    let w0 = edge(v1, v2, p) * inv_area;
    let w1 = edge(v2, v0, p) * inv_area;
    let w2 = edge(v0, v1, p) * inv_area;

    // Check if point is inside triangle
    if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
        // Interpolate depth
        let depth = w0 * v0[2] + w1 * v1[2] + w2 * v2[2];
        Some(depth)
    } else {
        None
    }
}

/// Default sprite size for palette icons.
pub const SPRITE_SIZE: u32 = 64;

/// Generates a sprite image for a model and saves it to the specified path.
///
/// Uses a fixed 3/4 isometric view angle matching the GPU sprite generator.
pub fn generate_model_sprite(model: &SubVoxelModel, output_path: &Path) -> std::io::Result<()> {
    let size = SPRITE_SIZE as usize;
    let mut rasterizer = Rasterizer::new(size, size);

    // Transparent background
    rasterizer.clear([0, 0, 0, 0]);

    // Isometric projection setup - 3/4 view like GPU sprites
    let cell_size = size as f32 / 12.0;
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

    let model_center = 4.0;

    // Project 3D point to screen coordinates with depth
    let project = |x: f32, y: f32, z: f32| -> [f32; 3] {
        let cx = x - model_center;
        let cy = y - model_center;
        let cz = z - model_center;

        let screen_x = center_x + iso_x[0] * cx + iso_z[0] * cz;
        let screen_y = center_y + iso_x[1] * cx + iso_z[1] * cz + iso_y[1] * cy;
        let depth = -(cx * cos_yaw + cz * sin_yaw) - (cz * cos_yaw - cx * sin_yaw) + cy * 0.01;

        [screen_x, screen_y, depth]
    };

    // Draw all voxels with proper z-buffer
    for y in 0..SUB_VOXEL_SIZE {
        for z in 0..SUB_VOXEL_SIZE {
            for x in 0..SUB_VOXEL_SIZE {
                let idx = model.get_voxel(x, y, z);
                if idx == 0 {
                    continue;
                }

                let color = &model.palette[idx as usize];
                let x_f = x as f32;
                let y_f = y as f32;
                let z_f = z as f32;

                // Face colors with shading
                let top_color = [color.r, color.g, color.b, color.a];
                let bottom_color = [
                    (color.r as f32 * 0.4) as u8,
                    (color.g as f32 * 0.4) as u8,
                    (color.b as f32 * 0.4) as u8,
                    color.a,
                ];
                let xp_color = [
                    (color.r as f32 * 0.7) as u8,
                    (color.g as f32 * 0.7) as u8,
                    (color.b as f32 * 0.7) as u8,
                    color.a,
                ];
                let xm_color = [
                    (color.r as f32 * 0.8) as u8,
                    (color.g as f32 * 0.8) as u8,
                    (color.b as f32 * 0.8) as u8,
                    color.a,
                ];
                let zp_color = [
                    (color.r as f32 * 0.6) as u8,
                    (color.g as f32 * 0.6) as u8,
                    (color.b as f32 * 0.6) as u8,
                    color.a,
                ];
                let zm_color = [
                    (color.r as f32 * 0.85) as u8,
                    (color.g as f32 * 0.85) as u8,
                    (color.b as f32 * 0.85) as u8,
                    color.a,
                ];

                // 8 corners of the cube
                let p000 = project(x_f, y_f, z_f);
                let p100 = project(x_f + 1.0, y_f, z_f);
                let p110 = project(x_f + 1.0, y_f + 1.0, z_f);
                let p010 = project(x_f, y_f + 1.0, z_f);
                let p001 = project(x_f, y_f, z_f + 1.0);
                let p101 = project(x_f + 1.0, y_f, z_f + 1.0);
                let p111 = project(x_f + 1.0, y_f + 1.0, z_f + 1.0);
                let p011 = project(x_f, y_f + 1.0, z_f + 1.0);

                // Top face (+Y)
                draw_sprite_face(&mut rasterizer, [p010, p110, p111, p011], top_color);

                // Bottom face (-Y)
                draw_sprite_face(&mut rasterizer, [p000, p001, p101, p100], bottom_color);

                // X+ face
                draw_sprite_face(&mut rasterizer, [p100, p101, p111, p110], xp_color);

                // X- face
                draw_sprite_face(&mut rasterizer, [p000, p010, p011, p001], xm_color);

                // Z+ face
                draw_sprite_face(&mut rasterizer, [p001, p011, p111, p101], zp_color);

                // Z- face
                draw_sprite_face(&mut rasterizer, [p000, p100, p110, p010], zm_color);
            }
        }
    }

    // Save to PNG
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(size as u32, size as u32, rasterizer.color_buffer.clone())
            .ok_or_else(|| std::io::Error::other("Failed to create image buffer"))?;

    img.save(output_path)
        .map_err(|e| std::io::Error::other(format!("Failed to save sprite: {}", e)))?;

    Ok(())
}

/// Helper to draw a face for sprite generation (no hit map needed).
fn draw_sprite_face(rasterizer: &mut Rasterizer, positions: [[f32; 3]; 4], color: [u8; 4]) {
    let v0 = Vertex {
        pos: positions[0],
        color,
    };
    let v1 = Vertex {
        pos: positions[1],
        color,
    };
    let v2 = Vertex {
        pos: positions[2],
        color,
    };
    let v3 = Vertex {
        pos: positions[3],
        color,
    };
    rasterizer.draw_quad(v0, v1, v2, v3);
}

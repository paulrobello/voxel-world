//! CPU-based software rasterizer for template thumbnails.
//!
//! Generates 64×64 PNG thumbnails using the same isometric projection
//! and z-buffer approach as custom model sprites.

use crate::chunk::BlockType;
use crate::editor::rasterizer::Rasterizer;
use crate::templates::format::{
    TemplateBlock, TemplatePaintData, TemplateTintData, TemplateWaterData, VxtFile,
};
use image::{ImageBuffer, Rgba};
use std::io;
use std::path::Path;

/// Default thumbnail size for template sprites.
pub const THUMBNAIL_SIZE: u32 = 64;

/// Generates a thumbnail image for a template and saves it to the specified path.
///
/// Uses a fixed 3/4 isometric view angle matching model sprite generation.
pub fn generate_template_thumbnail(template: &VxtFile, output_path: &Path) -> io::Result<()> {
    let size = THUMBNAIL_SIZE as usize;
    let mut rasterizer = Rasterizer::new(size, size);

    // Transparent background
    rasterizer.clear([0, 0, 0, 0]);

    // Calculate adaptive cell size based on template dimensions
    let max_dim = template.width.max(template.height).max(template.depth);
    let cell_size = calculate_cell_size(max_dim, size);

    // Center the template in the image
    let template_center_x = template.width as f32 / 2.0;
    let template_center_y = template.height as f32 / 2.0;
    let template_center_z = template.depth as f32 / 2.0;

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
        let cx = x - template_center_x;
        let cy = y - template_center_y;
        let cz = z - template_center_z;

        let screen_x = center_x + iso_x[0] * cx + iso_z[0] * cz;
        let screen_y = center_y + iso_x[1] * cx + iso_z[1] * cz + iso_y[1] * cy;
        let depth = -(cx * cos_yaw + cz * sin_yaw) - (cz * cos_yaw - cx * sin_yaw) + cy * 0.01;

        [screen_x, screen_y, depth]
    };

    // Draw all blocks with proper z-buffer
    for block in &template.blocks {
        let block_type = BlockType::from(block.block_type);
        let base_color = get_block_color(block_type, template, block);

        let x_f = block.x as f32;
        let y_f = block.y as f32;
        let z_f = block.z as f32;

        // Face colors with shading (same as model sprites)
        let top_color = [base_color[0], base_color[1], base_color[2], 255];
        let bottom_color = [
            (base_color[0] as f32 * 0.4) as u8,
            (base_color[1] as f32 * 0.4) as u8,
            (base_color[2] as f32 * 0.4) as u8,
            255,
        ];
        let xp_color = [
            (base_color[0] as f32 * 0.7) as u8,
            (base_color[1] as f32 * 0.7) as u8,
            (base_color[2] as f32 * 0.7) as u8,
            255,
        ];
        let xm_color = [
            (base_color[0] as f32 * 0.8) as u8,
            (base_color[1] as f32 * 0.8) as u8,
            (base_color[2] as f32 * 0.8) as u8,
            255,
        ];
        let zp_color = [
            (base_color[0] as f32 * 0.6) as u8,
            (base_color[1] as f32 * 0.6) as u8,
            (base_color[2] as f32 * 0.6) as u8,
            255,
        ];
        let zm_color = [
            (base_color[0] as f32 * 0.85) as u8,
            (base_color[1] as f32 * 0.85) as u8,
            (base_color[2] as f32 * 0.85) as u8,
            255,
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
        draw_template_face(&mut rasterizer, [p010, p110, p111, p011], top_color);

        // Bottom face (-Y)
        draw_template_face(&mut rasterizer, [p000, p001, p101, p100], bottom_color);

        // X+ face
        draw_template_face(&mut rasterizer, [p100, p101, p111, p110], xp_color);

        // X- face
        draw_template_face(&mut rasterizer, [p000, p010, p011, p001], xm_color);

        // Z+ face
        draw_template_face(&mut rasterizer, [p001, p011, p111, p101], zp_color);

        // Z- face
        draw_template_face(&mut rasterizer, [p000, p100, p110, p010], zm_color);
    }

    // Save to PNG
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(size as u32, size as u32, rasterizer.color_buffer().to_vec())
            .ok_or_else(|| io::Error::other("Failed to create image buffer"))?;

    img.save(output_path)
        .map_err(|e| io::Error::other(format!("Failed to save thumbnail: {}", e)))?;

    Ok(())
}

/// Calculate adaptive cell size based on template dimensions.
///
/// Smaller templates get larger cells, larger templates get smaller cells,
/// so they all fit nicely in the 64×64 thumbnail.
fn calculate_cell_size(max_dimension: u8, target_size: usize) -> f32 {
    // Use same scaling factor as model sprites (1.5x for isometric projection)
    let scale_factor = 1.5;
    target_size as f32 / (max_dimension as f32 * scale_factor)
}

/// Get the display color for a block, handling metadata for special block types.
fn get_block_color(block_type: BlockType, template: &VxtFile, block: &TemplateBlock) -> [u8; 3] {
    match block_type {
        BlockType::TintedGlass | BlockType::Crystal => {
            // Find tint_data for this position
            if let Some(tint) = find_tint_data(template, block) {
                return get_tint_color(tint.tint_index);
            }
            let color = block_type.color();
            [
                (color[0] * 255.0) as u8,
                (color[1] * 255.0) as u8,
                (color[2] * 255.0) as u8,
            ]
        }
        BlockType::Painted => {
            // Find paint_data for this position
            if let Some(paint) = find_paint_data(template, block) {
                return get_painted_color(paint);
            }
            let color = block_type.color();
            [
                (color[0] * 255.0) as u8,
                (color[1] * 255.0) as u8,
                (color[2] * 255.0) as u8,
            ]
        }
        BlockType::Water => {
            // Find water_data for water type color
            if let Some(water) = find_water_data(template, block) {
                return get_water_color(water.water_type);
            }
            let color = block_type.color();
            [
                (color[0] * 255.0) as u8,
                (color[1] * 255.0) as u8,
                (color[2] * 255.0) as u8,
            ]
        }
        BlockType::Model => {
            // Models are complex - use a neutral gray for thumbnails
            // Could enhance later to render actual model voxels
            [128, 128, 128]
        }
        _ => {
            let color = block_type.color();
            [
                (color[0] * 255.0) as u8,
                (color[1] * 255.0) as u8,
                (color[2] * 255.0) as u8,
            ]
        }
    }
}

/// Find tint metadata for a specific block position.
fn find_tint_data<'a>(
    template: &'a VxtFile,
    block: &TemplateBlock,
) -> Option<&'a TemplateTintData> {
    template
        .tint_data
        .iter()
        .find(|t| t.x == block.x && t.y == block.y && t.z == block.z)
}

/// Find paint metadata for a specific block position.
fn find_paint_data<'a>(
    template: &'a VxtFile,
    block: &TemplateBlock,
) -> Option<&'a TemplatePaintData> {
    template
        .paint_data
        .iter()
        .find(|p| p.x == block.x && p.y == block.y && p.z == block.z)
}

/// Find water metadata for a specific block position.
fn find_water_data<'a>(
    template: &'a VxtFile,
    block: &TemplateBlock,
) -> Option<&'a TemplateWaterData> {
    template
        .water_data
        .iter()
        .find(|w| w.x == block.x && w.y == block.y && w.z == block.z)
}

/// Get RGB color for a tint index (matches shader TINT_PALETTE).
fn get_tint_color(tint_index: u8) -> [u8; 3] {
    // Same colors as common.glsl TINT_PALETTE
    const TINT_PALETTE: [[u8; 3]; 32] = [
        [255, 255, 255], // 0: White
        [255, 0, 0],     // 1: Red
        [0, 255, 0],     // 2: Green
        [0, 0, 255],     // 3: Blue
        [255, 255, 0],   // 4: Yellow
        [255, 0, 255],   // 5: Magenta
        [0, 255, 255],   // 6: Cyan
        [255, 128, 0],   // 7: Orange
        [128, 0, 255],   // 8: Purple
        [0, 255, 128],   // 9: Spring Green
        [255, 128, 128], // 10: Light Red
        [128, 255, 128], // 11: Light Green
        [128, 128, 255], // 12: Light Blue
        [255, 192, 128], // 13: Peach
        [192, 128, 255], // 14: Lavender
        [128, 255, 192], // 15: Mint
        [64, 64, 64],    // 16: Dark Gray
        [128, 128, 128], // 17: Gray
        [192, 192, 192], // 18: Light Gray
        [64, 32, 16],    // 19: Brown
        [128, 64, 32],   // 20: Tan
        [192, 128, 64],  // 21: Beige
        [255, 192, 192], // 22: Pink
        [192, 255, 192], // 23: Pale Green
        [192, 192, 255], // 24: Pale Blue
        [255, 224, 192], // 25: Cream
        [224, 192, 255], // 26: Lilac
        [192, 255, 224], // 27: Aqua
        [96, 64, 32],    // 28: Dark Brown
        [48, 48, 48],    // 29: Charcoal
        [224, 224, 224], // 30: Off White
        [160, 160, 160], // 31: Medium Gray
    ];

    TINT_PALETTE[tint_index as usize % 32]
}

/// Get RGB color for a painted block (simplified - just use texture color).
fn get_painted_color(paint: &TemplatePaintData) -> [u8; 3] {
    // For thumbnails, just blend the tint color
    // Could enhance with texture sampling later
    get_tint_color(paint.tint_idx)
}

/// Get RGB color for water type (matches shader water colors).
fn get_water_color(water_type: u8) -> [u8; 3] {
    // WaterType enum values from chunk.rs
    const WATER_OCEAN: u8 = 0;
    const WATER_LAKE: u8 = 1;
    const WATER_RIVER: u8 = 2;
    const WATER_SWAMP: u8 = 3;
    const WATER_SPRING: u8 = 4;

    match water_type {
        WATER_OCEAN => [41, 128, 185],   // Deep blue
        WATER_LAKE => [52, 152, 219],    // Medium blue
        WATER_RIVER => [93, 173, 226],   // Light blue
        WATER_SWAMP => [86, 101, 62],    // Murky green-brown
        WATER_SPRING => [102, 204, 255], // Crystal blue
        _ => [93, 173, 226],             // Default to river color
    }
}

/// Helper to draw a face for template thumbnail (wraps editor rasterizer).
fn draw_template_face(rasterizer: &mut Rasterizer, positions: [[f32; 3]; 4], color: [u8; 4]) {
    use crate::editor::rasterizer::Vertex;

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

    // Draw as quad (two triangles)
    rasterizer.draw_quad(v0, v1, v2, v3);
}

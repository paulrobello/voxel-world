//! Picture frame model for displaying user-created pictures.
//!
//! A single frame model (ID 160) now auto-sizes itself up to 3×3 blocks by
//! inspecting neighboring frame blocks. All blocks in a multi-block frame
//! share the same model ID; per-block metadata records width, height, offsets,
//! and facing so the shader can sample the correct portion of the picture and
//! draw only the appropriate border edges.

use crate::sub_voxel::{Color, LightBlocking, ModelResolution, SubVoxelModel};

use super::basic::DESIGN_SIZE;

/// Model ID for picture frames (auto-sized via metadata).
pub const FRAME_MODEL_ID: u8 = 160;

/// Alias for references.
pub const FRAME_1X1_ID: u8 = FRAME_MODEL_ID;

/// First frame model ID.
pub const FIRST_FRAME_ID: u8 = 160;

/// Last frame model ID (same as first; legacy multi-ID range removed).
pub const LAST_FRAME_ID: u8 = 160;

/// Maximum frame dimension (in blocks) supported by auto-sizing.
pub const MAX_FRAME_DIM: u8 = 3;

/// Frame wood color (dark brown).
const FRAME_WOOD: Color = Color::rgb(101, 67, 33);

/// Frame wood highlight color (lighter brown for inner edge).
const FRAME_WOOD_LIGHT: Color = Color::rgb(139, 90, 43);

/// Frame wood shadow color (darker for outer edge).
const FRAME_WOOD_DARK: Color = Color::rgb(71, 47, 23);

/// Picture area color (magenta - replaced by shader with picture texture).
/// Using a distinctive color that's easy to identify in shader.
const PICTURE_AREA: Color = Color::rgb(255, 0, 255);

/// Frame depth in voxels.
/// Border is 2 voxels deep (to reach wall), picture area is 1 voxel deep (recessed).
const FRAME_DEPTH: usize = 2;

/// Frame border width in voxels.
const BORDER_WIDTH: usize = 1;

/// Picture area depth (1 voxel, recessed from frame front)
const PICTURE_DEPTH: usize = 1;

/// Returns true if this model ID is a picture frame.
#[inline]
pub const fn is_frame_model(model_id: u8) -> bool {
    model_id == FRAME_MODEL_ID
}

/// Creates a picture frame model.
///
/// All frame models use the same basic structure:
/// - A flat back plate
/// - Wooden border around the edge (at z=6, 1 voxel deep)
/// - Picture area filling the entire front face (z=7) including edges
///
/// This allows merged frames to have seamless picture areas that extend
/// all the way to the frame edges.
fn create_frame(name: &str) -> SubVoxelModel {
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::Low, name);

    // Palette setup:
    // 1 = Frame wood (main)
    // 2 = Frame wood light (inner highlight)
    // 3 = Frame wood dark (outer shadow)
    // 4 = Picture area (magenta, replaced by shader)
    model.palette[1] = FRAME_WOOD;
    model.palette[2] = FRAME_WOOD_LIGHT;
    model.palette[3] = FRAME_WOOD_DARK;
    model.palette[4] = PICTURE_AREA;

    let max = DESIGN_SIZE - 1; // 7 for 8³

    // Frame has different depths for border vs picture:
    // - Border: 1 voxel deep (z=6) for structure behind the picture
    // - Picture area: 1 voxel deep (z=7) filling the entire front face
    let z_start = max - FRAME_DEPTH + 1; // 6 for DEPTH=2
    let z_picture = max; // 7 (front face)

    // Fill picture area (entire front face, including edges)
    // This allows merged frames to have picture extending to frame edges
    for y in 0..DESIGN_SIZE {
        for x in 0..DESIGN_SIZE {
            model.set_voxel(x, y, z_picture, 4); // Picture area (magenta)
        }
    }

    // Add wooden border on all edges (1 voxel deep at z=6, behind picture)
    // Left border (1 voxel deep at z=6)
    for y in 0..DESIGN_SIZE {
        model.set_voxel(0, y, z_start, 1); // Wood
    }

    // Right border (1 voxel deep at z=6)
    for y in 0..DESIGN_SIZE {
        model.set_voxel(max, y, z_start, 1); // Wood
    }

    // Bottom border (1 voxel deep at z=6)
    for x in 0..DESIGN_SIZE {
        model.set_voxel(x, 0, z_start, 1); // Wood
    }

    // Top border (1 voxel deep at z=6)
    for x in 0..DESIGN_SIZE {
        model.set_voxel(x, max, z_start, 1); // Wood
    }

    // Add inner highlight on the border (at z=6)
    let b = BORDER_WIDTH;
    for y in b..(DESIGN_SIZE - b) {
        model.set_voxel(b, y, z_start, 2); // Left inner highlight
        model.set_voxel(max - b, y, z_start, 2); // Right inner highlight
    }
    for x in b..(DESIGN_SIZE - b) {
        model.set_voxel(x, b, z_start, 2); // Bottom inner highlight
        model.set_voxel(x, max, z_start, 2); // Top inner highlight
    }

    // Add corner posts at front face (z=7) for visual completeness
    // These appear at the corners of the frame
    model.set_voxel(0, 0, max, 1); // Top-left corner
    model.set_voxel(max, 0, max, 1); // Top-right corner
    model.set_voxel(0, max, max, 1); // Bottom-left corner
    model.set_voxel(max, max, max, 1); // Bottom-right corner

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.no_collision = true; // Walk through frames
    model.compute_collision_mask();
    model
}

/// Creates the auto-sizing picture frame (ID 160).
pub fn create_frame_auto() -> SubVoxelModel {
    create_frame("frame_auto")
}

/// Metadata encoding for frame blocks.
/// custom_data layout (30 bits used):
/// - bits 0-19:  picture_id (20 bits, supports up to 1M pictures)
/// - bits 20-21: offset_x (2 bits, 0-3)
/// - bits 22-23: offset_y (2 bits, 0-3)
/// - bits 24-25: width_minus_one (2 bits, stores 1-4 → 0-3; clamped to MAX_FRAME_DIM)
/// - bits 26-27: height_minus_one (2 bits, stores 1-4 → 0-3; clamped to MAX_FRAME_DIM)
/// - bits 28-29: facing (2 bits, 0=North, 1=East, 2=South, 3=West)
pub mod metadata {
    use super::MAX_FRAME_DIM;

    /// Encodes frame metadata into a u32.
    /// Width and height are clamped to `MAX_FRAME_DIM` and stored as (value - 1).
    pub const fn encode(
        picture_id: u32,
        offset_x: u8,
        offset_y: u8,
        width: u8,
        height: u8,
        facing: u8,
    ) -> u32 {
        let clamped_w = {
            let w = if width == 0 {
                1
            } else if width > MAX_FRAME_DIM {
                MAX_FRAME_DIM
            } else {
                width
            };
            w - 1
        };
        let clamped_h = {
            let h = if height == 0 {
                1
            } else if height > MAX_FRAME_DIM {
                MAX_FRAME_DIM
            } else {
                height
            };
            h - 1
        };

        (picture_id & 0xFFFFF)
            | ((offset_x as u32 & 0x3) << 20)
            | ((offset_y as u32 & 0x3) << 22)
            | ((clamped_w as u32 & 0x3) << 24)
            | ((clamped_h as u32 & 0x3) << 26)
            | ((facing as u32 & 0x3) << 28)
    }

    /// Decodes picture_id from frame metadata.
    pub const fn decode_picture_id(data: u32) -> u32 {
        data & 0xFFFFF
    }

    /// Decodes offset_x from frame metadata.
    pub const fn decode_offset_x(data: u32) -> u8 {
        ((data >> 20) & 0x3) as u8
    }

    /// Decodes offset_y from frame metadata.
    pub const fn decode_offset_y(data: u32) -> u8 {
        ((data >> 22) & 0x3) as u8
    }

    /// Decodes width from frame metadata (defaults to 1 if unset/zero).
    pub const fn decode_width(data: u32) -> u8 {
        (((data >> 24) & 0x3) as u8) + 1
    }

    /// Decodes height from frame metadata (defaults to 1 if unset/zero).
    pub const fn decode_height(data: u32) -> u8 {
        (((data >> 26) & 0x3) as u8) + 1
    }

    /// Decodes facing direction from frame metadata.
    /// Supports legacy encoding (bits 24-25) for backward compatibility.
    pub const fn decode_facing(data: u32) -> u8 {
        let new_bits = ((data >> 28) & 0x3) as u8;
        if new_bits != 0 {
            new_bits
        } else {
            ((data >> 24) & 0x3) as u8
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_encode_decode() {
            let data = encode(12345, 2, 1, 2, 3, 3);
            assert_eq!(decode_picture_id(data), 12345);
            assert_eq!(decode_offset_x(data), 2);
            assert_eq!(decode_offset_y(data), 1);
            assert_eq!(decode_width(data), 2);
            assert_eq!(decode_height(data), 3);
            assert_eq!(decode_facing(data), 3);
        }

        #[test]
        fn test_max_picture_id() {
            let max_id = 0xFFFFF; // 1,048,575
            let data = encode(max_id, 3, 3, 3, 3, 3);
            assert_eq!(decode_picture_id(data), max_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_frame_model() {
        assert!(is_frame_model(FRAME_MODEL_ID));
        assert!(is_frame_model(LAST_FRAME_ID));
        assert!(!is_frame_model(159));
    }

    #[test]
    fn test_frame_auto() {
        let frame = create_frame_auto();
        assert_eq!(frame.name, "frame_auto");
        assert!(frame.rotatable);
        assert!(frame.no_collision);
        assert!(frame.voxels.iter().any(|&v| v != 0));
    }

    fn inverse_transform_frame_position(p: [f32; 3], rotation: u8) -> [f32; 3] {
        match rotation & 3 {
            0 => p,
            2 => [1.0 - p[0], p[1], 1.0 - p[2]],
            1 => [p[2], p[1], 1.0 - p[0]],
            3 => [1.0 - p[2], p[1], p[0]],
            _ => p,
        }
    }

    #[test]
    fn frame_inverse_transform_faces_align() {
        let eps = 1e-6;
        // North (-Z): wall at z=0
        let north = inverse_transform_frame_position([0.4, 0.5, 0.0], 0);
        assert!(north[2].abs() < eps);
        // South (+Z): wall at z=1
        let south = inverse_transform_frame_position([0.4, 0.5, 1.0], 2);
        assert!(south[2].abs() < eps);
        // East (+X): wall at x=1
        let east = inverse_transform_frame_position([1.0, 0.5, 0.25], 1);
        assert!(east[2].abs() < eps);
        // West (-X): wall at x=0
        let west = inverse_transform_frame_position([0.0, 0.5, 0.75], 3);
        assert!(west[2].abs() < eps);

        // Left/right ordering preserves picture orientation per facing
        let south_left = inverse_transform_frame_position([0.95, 0.5, 1.0], 2)[0];
        let south_right = inverse_transform_frame_position([0.05, 0.5, 1.0], 2)[0];
        assert!(south_left < south_right);

        let east_left = inverse_transform_frame_position([1.0, 0.5, 0.0], 1)[0];
        let east_right = inverse_transform_frame_position([1.0, 0.5, 1.0], 1)[0];
        assert!(east_left < east_right);

        let west_left = inverse_transform_frame_position([0.0, 0.5, 1.0], 3)[0];
        let west_right = inverse_transform_frame_position([0.0, 0.5, 0.0], 3)[0];
        assert!(west_left < west_right);
    }
}

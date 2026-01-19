//! Picture frame models for displaying user-created pictures.
//!
//! Frame models come in 9 sizes from 1×1 to 3×3 blocks.
//! Each block of a multi-block frame uses the same model ID.
//! The shader uses block metadata (offset_x, offset_y) to:
//! 1. Sample the correct portion of the picture
//! 2. Draw borders on the appropriate edges
//!
//! Model IDs:
//! - 160: 1×1 frame
//! - 161: 1×2 frame (1 wide × 2 tall)
//! - 162: 1×3 frame (1 wide × 3 tall)
//! - 163: 2×1 frame (2 wide × 1 tall)
//! - 164: 2×2 frame
//! - 165: 2×3 frame
//! - 166: 3×1 frame (3 wide × 1 tall)
//! - 167: 3×2 frame
//! - 168: 3×3 frame

use crate::sub_voxel::{Color, LightBlocking, ModelResolution, SubVoxelModel};

use super::basic::DESIGN_SIZE;

/// Model ID for the 1×1 picture frame.
pub const FRAME_1X1_ID: u8 = 160;

/// First frame model ID.
pub const FIRST_FRAME_ID: u8 = 160;

/// Last frame model ID.
pub const LAST_FRAME_ID: u8 = 168;

/// Frame wood color (dark brown).
const FRAME_WOOD: Color = Color::rgb(101, 67, 33);

/// Frame wood highlight color (lighter brown for inner edge).
const FRAME_WOOD_LIGHT: Color = Color::rgb(139, 90, 43);

/// Frame wood shadow color (darker for outer edge).
const FRAME_WOOD_DARK: Color = Color::rgb(71, 47, 23);

/// Picture area color (magenta - replaced by shader with picture texture).
/// Using a distinctive color that's easy to identify in shader.
const PICTURE_AREA: Color = Color::rgb(255, 0, 255);

/// Frame depth (how far it sticks out from wall) in voxels.
const FRAME_DEPTH: usize = 1;

/// Frame border width in voxels.
const BORDER_WIDTH: usize = 1;

/// Returns the frame size for a given model ID.
/// Returns (width, height) in blocks.
pub const fn frame_size(model_id: u8) -> Option<(u8, u8)> {
    match model_id {
        160 => Some((1, 1)),
        161 => Some((1, 2)),
        162 => Some((1, 3)),
        163 => Some((2, 1)),
        164 => Some((2, 2)),
        165 => Some((2, 3)),
        166 => Some((3, 1)),
        167 => Some((3, 2)),
        168 => Some((3, 3)),
        _ => None,
    }
}

/// Returns the frame model ID for a given size.
/// Returns None if size is invalid (not 1-3 in each dimension).
pub const fn frame_model_id(width: u8, height: u8) -> Option<u8> {
    match (width, height) {
        (1, 1) => Some(160),
        (1, 2) => Some(161),
        (1, 3) => Some(162),
        (2, 1) => Some(163),
        (2, 2) => Some(164),
        (2, 3) => Some(165),
        (3, 1) => Some(166),
        (3, 2) => Some(167),
        (3, 3) => Some(168),
        _ => None,
    }
}

/// Returns true if this model ID is a picture frame.
#[inline]
pub const fn is_frame_model(model_id: u8) -> bool {
    model_id >= FIRST_FRAME_ID && model_id <= LAST_FRAME_ID
}

/// Creates a picture frame model.
///
/// All frame models use the same basic structure:
/// - A flat back plate
/// - Wooden border around the edge
/// - Picture area in the center (magenta, replaced by shader)
///
/// The shader uses block metadata to determine:
/// - Which portion of the picture to display
/// - Which borders to draw (based on position within multi-block frame)
fn create_frame(name: &str, _width: u8, _height: u8) -> SubVoxelModel {
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

    // Frame is flat on the front face (z = max - FRAME_DEPTH + 1 to z = max)
    // For a wall-mounted frame, the back is at z=max and faces outward
    let z_start = max + 1 - FRAME_DEPTH;

    // Fill the entire front with picture area first
    for y in 0..DESIGN_SIZE {
        for x in 0..DESIGN_SIZE {
            for z in z_start..=max {
                model.set_voxel(x, y, z, 4); // Picture area
            }
        }
    }

    // Add wooden border on all edges
    // This is the complete border for a 1×1 frame
    // For multi-block frames, the shader will mask out inner edges

    // Left border
    for y in 0..DESIGN_SIZE {
        for z in z_start..=max {
            model.set_voxel(0, y, z, 1);
        }
    }

    // Right border
    for y in 0..DESIGN_SIZE {
        for z in z_start..=max {
            model.set_voxel(max, y, z, 1);
        }
    }

    // Bottom border
    for x in 0..DESIGN_SIZE {
        for z in z_start..=max {
            model.set_voxel(x, 0, z, 1);
        }
    }

    // Top border
    for x in 0..DESIGN_SIZE {
        for z in z_start..=max {
            model.set_voxel(x, max, z, 1);
        }
    }

    // Add inner highlight on the border
    let b = BORDER_WIDTH;
    for y in b..(DESIGN_SIZE - b) {
        model.set_voxel(b, y, max, 2); // Left inner
        model.set_voxel(max - b, y, max, 2); // Right inner
    }
    for x in b..(DESIGN_SIZE - b) {
        model.set_voxel(x, b, max, 2); // Bottom inner
        model.set_voxel(x, max - b, max, 2); // Top inner
    }

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.no_collision = true; // Walk through frames
    model.compute_collision_mask();
    model
}

/// Creates the 1×1 picture frame.
pub fn create_frame_1x1() -> SubVoxelModel {
    create_frame("frame_1x1", 1, 1)
}

/// Creates the 1×2 picture frame.
pub fn create_frame_1x2() -> SubVoxelModel {
    create_frame("frame_1x2", 1, 2)
}

/// Creates the 1×3 picture frame.
pub fn create_frame_1x3() -> SubVoxelModel {
    create_frame("frame_1x3", 1, 3)
}

/// Creates the 2×1 picture frame.
pub fn create_frame_2x1() -> SubVoxelModel {
    create_frame("frame_2x1", 2, 1)
}

/// Creates the 2×2 picture frame.
pub fn create_frame_2x2() -> SubVoxelModel {
    create_frame("frame_2x2", 2, 2)
}

/// Creates the 2×3 picture frame.
pub fn create_frame_2x3() -> SubVoxelModel {
    create_frame("frame_2x3", 2, 3)
}

/// Creates the 3×1 picture frame.
pub fn create_frame_3x1() -> SubVoxelModel {
    create_frame("frame_3x1", 3, 1)
}

/// Creates the 3×2 picture frame.
pub fn create_frame_3x2() -> SubVoxelModel {
    create_frame("frame_3x2", 3, 2)
}

/// Creates the 3×3 picture frame.
pub fn create_frame_3x3() -> SubVoxelModel {
    create_frame("frame_3x3", 3, 3)
}

/// Metadata encoding for frame blocks.
/// custom_data layout (26 bits used):
/// - bits 0-19:  picture_id (20 bits, supports up to 1M pictures)
/// - bits 20-21: offset_x (2 bits, 0-3)
/// - bits 22-23: offset_y (2 bits, 0-3)
/// - bits 24-25: facing (2 bits, 0=North, 1=East, 2=South, 3=West)
pub mod metadata {
    /// Encodes frame metadata into a u32.
    pub const fn encode(picture_id: u32, offset_x: u8, offset_y: u8, facing: u8) -> u32 {
        (picture_id & 0xFFFFF)
            | ((offset_x as u32 & 0x3) << 20)
            | ((offset_y as u32 & 0x3) << 22)
            | ((facing as u32 & 0x3) << 24)
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

    /// Decodes facing direction from frame metadata.
    pub const fn decode_facing(data: u32) -> u8 {
        ((data >> 24) & 0x3) as u8
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_encode_decode() {
            let data = encode(12345, 2, 1, 3);
            assert_eq!(decode_picture_id(data), 12345);
            assert_eq!(decode_offset_x(data), 2);
            assert_eq!(decode_offset_y(data), 1);
            assert_eq!(decode_facing(data), 3);
        }

        #[test]
        fn test_max_picture_id() {
            let max_id = 0xFFFFF; // 1,048,575
            let data = encode(max_id, 3, 3, 3);
            assert_eq!(decode_picture_id(data), max_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_sizes() {
        assert_eq!(frame_size(160), Some((1, 1)));
        assert_eq!(frame_size(161), Some((1, 2)));
        assert_eq!(frame_size(168), Some((3, 3)));
        assert_eq!(frame_size(159), None);
        assert_eq!(frame_size(169), None);
    }

    #[test]
    fn test_frame_model_id() {
        assert_eq!(frame_model_id(1, 1), Some(160));
        assert_eq!(frame_model_id(3, 3), Some(168));
        assert_eq!(frame_model_id(0, 1), None);
        assert_eq!(frame_model_id(4, 4), None);
    }

    #[test]
    fn test_is_frame_model() {
        assert!(is_frame_model(160));
        assert!(is_frame_model(164));
        assert!(is_frame_model(168));
        assert!(!is_frame_model(159));
        assert!(!is_frame_model(169));
    }

    #[test]
    fn test_frame_1x1() {
        let frame = create_frame_1x1();
        assert_eq!(frame.name, "frame_1x1");
        assert!(frame.rotatable);
        assert!(frame.no_collision);
        // Check that frame has voxels
        assert!(frame.voxels.iter().any(|&v| v != 0));
    }
}

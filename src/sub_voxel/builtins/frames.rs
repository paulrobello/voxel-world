//! Picture frame model for displaying user-created pictures.
//!
//! Frames auto-size themselves up to 3×3 blocks by inspecting neighboring
//! frame blocks. Each frame block uses a model ID (160-175) that corresponds
//! to its edge configuration: model_id = 160 + edge_mask.
//!
//! edge_mask bits indicate which edges have borders (exterior edges):
//! - bit 0: left edge, bit 1: right edge, bit 2: bottom, bit 3: top
//! - Set bit (1) = KEEP border (exterior), Cleared bit (0) = interior edge
//!
//! Per-block metadata records picture_id, width, height, offsets, and facing.

use crate::sub_voxel::{Color, LightBlocking, ModelResolution, SubVoxelModel};

use super::basic::DESIGN_SIZE;

/// Base model ID for picture frames.
pub const FRAME_MODEL_ID_BASE: u8 = 160;

/// Last frame model ID (160 + 15 = 175 for edge_mask 0b1111).
pub const LAST_FRAME_ID: u8 = 175;

/// Alias for references (standalone frame with all edges).
pub const FRAME_1X1_ID: u8 = FRAME_MODEL_ID_BASE;

/// First frame model ID.
pub const FIRST_FRAME_ID: u8 = 160;

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

/// Frame border width in voxels.
const BORDER_WIDTH: usize = 1;

/// Returns true if this model ID is a picture frame (160-175).
#[inline]
pub const fn is_frame_model(model_id: u8) -> bool {
    model_id >= FRAME_MODEL_ID_BASE && model_id <= LAST_FRAME_ID
}

/// Extracts the edge_mask from a frame model ID.
/// Returns None if not a frame model.
#[inline]
pub const fn frame_model_id_to_edge_mask(model_id: u8) -> Option<u8> {
    if is_frame_model(model_id) {
        Some(model_id - FRAME_MODEL_ID_BASE)
    } else {
        None
    }
}

/// Converts an edge_mask to a frame model ID.
#[inline]
pub const fn edge_mask_to_frame_model_id(edge_mask: u8) -> u8 {
    FRAME_MODEL_ID_BASE + (edge_mask & 0x0F)
}

/// Creates a picture frame model for the specified edge mask.
///
/// edge_mask bits indicate which edges have borders:
/// - bit 0: left edge, bit 1: right edge, bit 2: bottom, bit 3: top
/// - Set bit (1) = border present (exterior edge)
/// - Cleared bit (0) = no border (interior edge where frames merge)
///
/// Uses 32³ resolution for 32×32 picture display:
/// - Picture area fills interior (30×30 voxels for display)
/// - Border is 1 voxel thick on exterior edges
/// - Frame is 1 voxel deep (z=31 is the front face)
fn create_frame_for_edge_mask(edge_mask: u8) -> SubVoxelModel {
    let name = format!("frame_edge_mask_{}", edge_mask);
    let mut model = SubVoxelModel::with_resolution_and_name(ModelResolution::High, &name);

    // Palette setup:
    // 1 = Frame wood (dark brown)
    // 4 = Picture area (magenta, replaced by shader with picture texture)
    model.palette[1] = FRAME_WOOD;
    model.palette[4] = PICTURE_AREA;

    // 32³ resolution: 32×32 picture area per frame
    let resolution = 32usize;
    let front_z = resolution - 1; // z=31 is the front face

    // Fill entire front face with picture area
    for y in 0..resolution {
        for x in 0..resolution {
            model.set_voxel(x, y, front_z, 4);
        }
    }

    // Add borders on exterior edges (1 voxel thick)
    // All at front face (z=31)

    // Left border (bit 0)
    if edge_mask & 1 != 0 {
        for y in 0..resolution {
            model.set_voxel(0, y, front_z, 1);
        }
    }

    // Right border (bit 1)
    if edge_mask & 2 != 0 {
        for y in 0..resolution {
            model.set_voxel(resolution - 1, y, front_z, 1);
        }
    }

    // Bottom border (bit 2)
    if edge_mask & 4 != 0 {
        for x in 0..resolution {
            model.set_voxel(x, 0, front_z, 1);
        }
    }

    // Top border (bit 3)
    if edge_mask & 8 != 0 {
        for x in 0..resolution {
            model.set_voxel(x, resolution - 1, front_z, 1);
        }
    }

    model.light_blocking = LightBlocking::Partial;
    model.rotatable = true;
    model.no_collision = true; // Walk through frames
    model.compute_collision_mask();
    model
}

/// Creates all 16 frame variants and registers them.
/// Call this during built-in model registration.
pub fn register_all_frame_variants(registry: &mut crate::sub_voxel::ModelRegistry) {
    for edge_mask in 0..16u8 {
        let model = create_frame_for_edge_mask(edge_mask);
        registry.register(model);
    }
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
        ((data >> 28) & 0x3) as u8
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
        assert!(is_frame_model(FIRST_FRAME_ID));
        assert!(is_frame_model(LAST_FRAME_ID));
        assert!(!is_frame_model(159));
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

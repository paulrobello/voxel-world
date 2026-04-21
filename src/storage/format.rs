use serde::{Deserialize, Serialize};

/// Version of the serialization format.
/// v2: Added tinted and painted metadata
/// v3: Added frame metadata (custom_data for models)
pub const FORMAT_VERSION: u8 = 3;

/// Metadata for a single block in a chunk.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BlockMeta {
    /// Flattened index in the chunk (0 to CHUNK_VOLUME-1).
    pub index: u16,
    /// Packed data: model_id (8 bits) | rotation (2 bits) | waterlogged (1 bit) | frame mask (4 bits) | extra (4 bits).
    pub data: u16,
}

impl BlockMeta {
    pub fn pack(model_id: u8, rotation: u8, waterlogged: bool) -> Self {
        let mut data = model_id as u16;
        // Bits 8-9: rotation (facing)
        data |= (rotation as u16 & 0x03) << 8;
        // Bits 11-14: frame edge mask (bits 3-6 of rotation value)
        let frame_mask = ((rotation >> 3) & 0x0F) as u16;
        data |= frame_mask << 11;
        if waterlogged {
            data |= 1 << 10;
        }
        Self { index: 0, data }
    }

    pub fn unpack(&self) -> (u8, u8, bool) {
        let model_id = (self.data & 0xFF) as u8;
        let rotation_facing = ((self.data >> 8) & 0x03) as u8;
        let frame_mask = ((self.data >> 11) & 0x0F) as u8;
        let rotation = rotation_facing | (frame_mask << 3);
        let waterlogged = (self.data & (1 << 10)) != 0;
        (model_id, rotation, waterlogged)
    }
}

/// Metadata for tinted glass blocks.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TintMeta {
    /// Flattened index in the chunk.
    pub index: u16,
    /// Tint palette index (0-31).
    pub tint: u8,
}

/// Metadata for painted blocks (texture + tint).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PaintMeta {
    /// Flattened index in the chunk.
    pub index: u16,
    /// Atlas texture index (0-based).
    pub texture: u8,
    /// Tint palette index (0-31).
    pub tint: u8,
}

/// Metadata for model blocks with custom data (e.g., picture frames).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FrameMeta {
    /// Flattened index in the chunk.
    pub index: u16,
    /// Custom data (for frames: picture_id, offset, facing).
    pub custom_data: u32,
}

/// A chunk serialized for storage or network transmission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedChunk {
    /// Format version.
    pub version: u8,
    /// Bitmask flags (e.g., is_generated).
    pub flags: u8,
    /// Block types (32^3 bytes).
    pub block_data: Vec<u8>,
    /// Sparse metadata for model blocks.
    pub metadata: Vec<BlockMeta>,
    /// Sparse metadata for tinted glass blocks.
    #[serde(default)]
    pub tinted: Vec<TintMeta>,
    /// Sparse metadata for painted blocks.
    #[serde(default)]
    pub painted: Vec<PaintMeta>,
    /// Sparse metadata for model blocks with custom data (frames, etc.).
    #[serde(default)]
    pub frames: Vec<FrameMeta>,
}

impl SerializedChunk {
    pub const FLAG_GENERATED: u8 = 1 << 0;
}

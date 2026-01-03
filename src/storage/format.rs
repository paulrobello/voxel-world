use serde::{Deserialize, Serialize};

/// Version of the serialization format.
pub const FORMAT_VERSION: u8 = 1;

/// Metadata for a single block in a chunk.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BlockMeta {
    /// Flattened index in the chunk (0 to CHUNK_VOLUME-1).
    pub index: u16,
    /// Packed data: model_id (8 bits) | rotation (2 bits) | waterlogged (1 bit) | extra (5 bits).
    pub data: u16,
}

impl BlockMeta {
    pub fn pack(model_id: u8, rotation: u8, waterlogged: bool) -> Self {
        let mut data = model_id as u16;
        data |= (rotation as u16 & 0x03) << 8;
        if waterlogged {
            data |= 1 << 10;
        }
        Self { index: 0, data }
    }

    pub fn unpack(&self) -> (u8, u8, bool) {
        let model_id = (self.data & 0xFF) as u8;
        let rotation = ((self.data >> 8) & 0x03) as u8;
        let waterlogged = (self.data & (1 << 10)) != 0;
        (model_id, rotation, waterlogged)
    }
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
}

impl SerializedChunk {
    pub const FLAG_GENERATED: u8 = 1 << 0;
}

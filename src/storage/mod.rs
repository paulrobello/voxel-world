pub mod format;
pub mod metadata;
pub mod region;
pub mod worker;

pub use crate::chunk::Chunk;
pub use format::{BlockMeta, FORMAT_VERSION, SerializedChunk};

use crate::chunk::{BlockType, CHUNK_VOLUME};

/// Compresses a SerializedChunk using Zstd.
pub fn compress_chunk(chunk: &SerializedChunk) -> Result<Vec<u8>, String> {
    let binary =
        bincode::serialize(chunk).map_err(|e| format!("Bincode serialization failed: {}", e))?;

    let compressed =
        zstd::encode_all(&binary[..], 3).map_err(|e| format!("Zstd compression failed: {}", e))?;

    Ok(compressed)
}

/// Decompresses a SerializedChunk using Zstd.
pub fn decompress_chunk(data: &[u8]) -> Result<SerializedChunk, String> {
    let decompressed =
        zstd::decode_all(data).map_err(|e| format!("Zstd decompression failed: {}", e))?;

    let chunk: SerializedChunk = bincode::deserialize(&decompressed)
        .map_err(|e| format!("Bincode deserialization failed: {}", e))?;

    Ok(chunk)
}

impl From<&Chunk> for SerializedChunk {
    fn from(chunk: &Chunk) -> Self {
        let block_data = chunk.block_bytes().to_vec();
        let mut metadata = Vec::new();

        for (idx, data) in chunk.model_entries() {
            let mut meta = BlockMeta::pack(data.model_id, data.rotation, data.waterlogged);
            meta.index = *idx as u16;
            metadata.push(meta);
        }

        // Sort metadata by index for deterministic output and potential compression benefits
        metadata.sort_by_key(|m| m.index);

        Self {
            version: FORMAT_VERSION,
            flags: SerializedChunk::FLAG_GENERATED,
            block_data,
            metadata,
        }
    }
}

impl TryFrom<SerializedChunk> for Chunk {
    type Error = String;

    fn try_from(serialized: SerializedChunk) -> Result<Self, Self::Error> {
        if serialized.version != FORMAT_VERSION {
            return Err(format!(
                "Unsupported format version: expected {}, got {}",
                FORMAT_VERSION, serialized.version
            ));
        }

        if serialized.block_data.len() != CHUNK_VOLUME {
            return Err(format!(
                "Invalid block data length: expected {}, got {}",
                CHUNK_VOLUME,
                serialized.block_data.len()
            ));
        }

        let mut chunk = Chunk::new();
        // Set blocks
        for (idx, &block_byte) in serialized.block_data.iter().enumerate() {
            let (x, y, z) = Chunk::index_to_coords(idx);
            chunk.set_block(x, y, z, BlockType::from(block_byte));
        }

        // Set metadata
        for meta in serialized.metadata {
            let (model_id, rotation, waterlogged) = meta.unpack();
            let (x, y, z) = Chunk::index_to_coords(meta.index as usize);
            chunk.set_model_block(x, y, z, model_id, rotation, waterlogged);
        }

        chunk.update_metadata();
        chunk.mark_dirty();
        chunk.persistence_dirty = false;

        Ok(chunk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{BlockType, Chunk};

    #[test]
    fn test_serialization_roundtrip() {
        let mut chunk = Chunk::new();
        chunk.set_block(0, 0, 0, BlockType::Stone);
        chunk.set_block(1, 2, 3, BlockType::Dirt);
        chunk.set_model_block(5, 5, 5, 10, 2, true);

        let serialized = SerializedChunk::from(&chunk);
        assert_eq!(serialized.metadata.len(), 1);
        assert_eq!(serialized.block_data[0], BlockType::Stone as u8);

        let deserialized = Chunk::try_from(serialized).unwrap();
        assert_eq!(deserialized.get_block(0, 0, 0), BlockType::Stone);
        assert_eq!(deserialized.get_block(1, 2, 3), BlockType::Dirt);
        assert_eq!(deserialized.get_block(5, 5, 5), BlockType::Model);

        let model_data = deserialized.get_model_data(5, 5, 5).unwrap();
        assert_eq!(model_data.model_id, 10);
        assert_eq!(model_data.rotation, 2);
        assert!(model_data.waterlogged);
    }
}

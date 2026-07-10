use serde::{Deserialize, Serialize};

/// Version of the serialization format.
/// v2: Added tinted and painted metadata
/// v3: Added frame metadata (custom_data for models)
pub const FORMAT_VERSION: u8 = 4;

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
    ///
    /// STOR-M03 (load-bearing invariant): this MUST remain the first field of
    /// the struct AND remain a `u8`. Postcard encodes struct fields in
    /// declaration order with no framing, and a `u8` serialises as exactly one
    /// byte, so byte 0 of every postcard payload of `SerializedChunk` equals
    /// this version. `SerializedChunk::peek_version` relies on that so it can
    /// branch on the version WITHOUT a full `postcard::from_bytes` decode -- the
    /// enabling primitive for forward migration. Promoting `version` to a
    /// wider int or prepending a field above it would silently break the peek.
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

    /// STOR-M03: Peeks the format version out of a postcard-serialised
    /// `SerializedChunk` body WITHOUT running a full `postcard::from_bytes`
    /// decode.
    ///
    /// Background: the format version historically lived INSIDE the postcard
    /// payload (it is the struct's first field) and was only validated AFTER
    /// the full decode inside `Chunk::try_from`. That ordering makes format
    /// migration effectively impossible -- once the struct shape changes
    /// between versions, the old postcard decode fails before the version can
    /// be inspected, so a migrator never gets the chance to run.
    ///
    /// Because postcard encodes `version: u8` as a single leading byte (see the
    /// load-bearing invariant on the `version` field), this helper reads byte 0
    /// and returns it. Callers can therefore branch on the version BEFORE
    /// committing to a deserialize: route a known-old payload through a
    /// migrator, or reject an unknown-future version, without relying on the
    /// full decoder. This is the primitive that unblocks forward migration.
    ///
    /// `payload` is the DECOMPRESSED postcard body (i.e. the bytes Zstd
    /// produced from the on-disk record), not the raw on-disk bytes. Peeking
    /// the version pre-decompression would require a version prefix in the
    /// on-disk wire format; that prefix is owned by `compress_chunk` /
    /// `decompress_chunk` in `storage::mod` (a different file) and is the
    /// deferred remainder of STOR-M03 -- see the remediation report.
    //
    // `expect(dead_code)`: this is the migration API surface, intended to be
    // wired into the `storage::mod` decode path (separate file, out of scope
    // here). Once that caller lands the attribute must be removed -- `expect`
    // will then flag it, unlike a silent `allow`.
    #[allow(
        dead_code,
        reason = "STOR-M03 migration API; not yet wired into the decode path"
    )]
    pub fn peek_version(payload: &[u8]) -> Option<u8> {
        payload.first().copied()
    }

    /// STOR-M03: Classifies a peeked version against what this build can decode
    /// natively. Mirrors the accept/reject contract in `Chunk::try_from`:
    /// current and older non-zero versions are supported; version `0` is a
    /// reserved sentinel and rejected, as is anything newer than
    /// `FORMAT_VERSION`.
    #[allow(
        dead_code,
        reason = "STOR-M03 migration API; not yet wired into the decode path"
    )]
    pub fn is_supported_version(version: u8) -> bool {
        version != 0 && version <= FORMAT_VERSION
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// STOR-M03: the leading byte of a postcard-serialised `SerializedChunk`
    /// must equal its `version` field, so `peek_version` reads the version
    /// without a full decode. This pins both the peek contract and the
    /// load-bearing "version stays first + u8" invariant on the struct.
    #[test]
    fn peek_version_reads_leading_byte_without_full_decode() {
        let serialized = SerializedChunk {
            version: FORMAT_VERSION,
            flags: SerializedChunk::FLAG_GENERATED,
            // Length is irrelevant for the peek; only the leading byte matters.
            block_data: vec![0u8; 10],
            metadata: vec![],
            tinted: vec![],
            painted: vec![],
            frames: vec![],
        };
        let bytes = postcard::to_stdvec(&serialized).expect("serialize");

        // Leading byte == version, read without a full `postcard::from_bytes`.
        assert_eq!(SerializedChunk::peek_version(&bytes), Some(FORMAT_VERSION));

        // Empty payload -> None (no panic, no decode).
        assert_eq!(SerializedChunk::peek_version(&[]), None);

        // is_supported_version mirrors Chunk::try_from's contract.
        assert!(SerializedChunk::is_supported_version(FORMAT_VERSION));
        assert!(!SerializedChunk::is_supported_version(0));
        assert!(!SerializedChunk::is_supported_version(FORMAT_VERSION + 1));

        // A stale older-version payload still peeks its own version, so a
        // migrator can branch on it before attempting a native decode.
        let mut older = serialized;
        older.version = FORMAT_VERSION - 1; // 4 -> 3, a valid prior version
        let older_bytes = postcard::to_stdvec(&older).expect("serialize older");
        assert_eq!(
            SerializedChunk::peek_version(&older_bytes),
            Some(FORMAT_VERSION - 1)
        );
        assert!(SerializedChunk::is_supported_version(FORMAT_VERSION - 1));
    }
}

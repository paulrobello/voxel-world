/// Portable file format for stencil guides (.vxs).
///
/// Stencils store only block positions (not types/metadata) for creating
/// holographic building guides. They are simpler than templates and used
/// for repeating structural patterns.
use crate::chunk::BlockType;
use crate::templates::format::VxtFile;
use nalgebra::Vector3;
use serde::{Deserialize, Serialize};

/// Magic bytes for VXS format "STCL"
const VXS_MAGIC: [u8; 4] = *b"STCL";

/// Current version of the VXS format.
const VXS_VERSION: u16 = 1;

/// Maximum stencil dimension in blocks (128×128×128).
pub const MAX_STENCIL_SIZE: u8 = 128;

/// A portable file format for stencil guides (.vxs).
/// Stencils only store positions, not block types or metadata.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StencilFile {
    pub magic: [u8; 4],
    pub version: u16,

    // Metadata
    pub name: String,
    pub author: String,
    pub tags: Vec<String>,
    pub creation_date: u64, // Unix timestamp

    // Dimensions (1-128 per axis)
    pub width: u8,
    pub height: u8,
    pub depth: u8,

    // Sparse position data (only non-air block positions)
    pub positions: Vec<StencilPosition>,
}

/// A single position in the stencil (relative to origin).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StencilPosition {
    pub x: u8,
    pub y: u8,
    pub z: u8,
}

#[allow(dead_code)]
impl StencilFile {
    /// Creates a new empty stencil.
    pub fn new(name: String, author: String, width: u8, height: u8, depth: u8) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            magic: VXS_MAGIC,
            version: VXS_VERSION,
            name,
            author,
            tags: Vec::new(),
            creation_date: timestamp,
            width,
            height,
            depth,
            positions: Vec::new(),
        }
    }

    /// Creates a stencil from a template, extracting just the block positions.
    ///
    /// This discards block type and metadata information from the template,
    /// keeping only the spatial structure for use as a building guide.
    ///
    /// # Arguments
    /// * `template` - The source template to convert
    /// * `name` - Optional new name (uses template name with "-stencil" suffix if None)
    ///
    /// # Returns
    /// A StencilFile containing all non-air block positions from the template.
    pub fn from_template(template: &VxtFile, name: Option<String>) -> Self {
        let stencil_name = name.unwrap_or_else(|| format!("{}-stencil", template.name));

        let mut stencil = Self::new(
            stencil_name,
            template.author.clone(),
            template.width,
            template.height,
            template.depth,
        );

        // Copy tags from template
        stencil.tags = template.tags.clone();

        // Extract positions from template blocks (ignoring block types)
        stencil.positions = template
            .blocks
            .iter()
            .map(|b| StencilPosition {
                x: b.x,
                y: b.y,
                z: b.z,
            })
            .collect();

        // Sort for compression efficiency
        stencil.sort_positions();

        stencil
    }

    /// Creates a stencil from a world region, capturing ALL non-air block positions.
    ///
    /// # Arguments
    /// * `world` - The world to capture from
    /// * `name` - Stencil name
    /// * `author` - Stencil author
    /// * `min` - Minimum corner (inclusive)
    /// * `max` - Maximum corner (inclusive)
    ///
    /// # Returns
    /// A StencilFile with all non-air block positions from the region.
    pub fn from_world_region(
        world: &crate::world::World,
        name: String,
        author: String,
        min: Vector3<i32>,
        max: Vector3<i32>,
    ) -> Result<Self, String> {
        // Calculate dimensions
        let width = (max.x - min.x + 1) as u8;
        let height = (max.y - min.y + 1) as u8;
        let depth = (max.z - min.z + 1) as u8;

        // Validate dimensions
        if width == 0 || height == 0 || depth == 0 {
            return Err("Region dimensions must be at least 1×1×1".to_string());
        }

        if width > MAX_STENCIL_SIZE || height > MAX_STENCIL_SIZE || depth > MAX_STENCIL_SIZE {
            return Err(format!(
                "Region too large ({}×{}×{}). Maximum is {}×{}×{}",
                width, height, depth, MAX_STENCIL_SIZE, MAX_STENCIL_SIZE, MAX_STENCIL_SIZE
            ));
        }

        let mut builder = StencilBuilder::new(name, author, min, width, height, depth);

        // Iterate through all positions in the region
        for y in min.y..=max.y {
            for z in min.z..=max.z {
                for x in min.x..=max.x {
                    let pos = Vector3::new(x, y, z);
                    let block_type = world.get_block(pos).unwrap_or(BlockType::Air);

                    // Add position if not air (capture ALL non-air blocks)
                    builder.add_position(pos, block_type);
                }
            }
        }

        Ok(builder.build())
    }

    /// Validates stencil dimensions.
    pub fn validate_dimensions(&self) -> Result<(), String> {
        if self.width == 0 || self.height == 0 || self.depth == 0 {
            return Err("Stencil dimensions must be at least 1×1×1".to_string());
        }

        if self.width > MAX_STENCIL_SIZE
            || self.height > MAX_STENCIL_SIZE
            || self.depth > MAX_STENCIL_SIZE
        {
            return Err(format!(
                "Stencil dimensions exceed maximum of {}×{}×{}",
                MAX_STENCIL_SIZE, MAX_STENCIL_SIZE, MAX_STENCIL_SIZE
            ));
        }

        Ok(())
    }

    /// Validates magic bytes and version.
    pub fn validate(&self) -> Result<(), String> {
        if self.magic != VXS_MAGIC {
            return Err("Invalid magic bytes for VXS file".to_string());
        }

        if self.version > VXS_VERSION {
            return Err(format!(
                "VXS version {} is newer than supported version {}",
                self.version, VXS_VERSION
            ));
        }

        self.validate_dimensions()?;

        Ok(())
    }

    /// Calculates total volume in blocks.
    pub fn volume(&self) -> usize {
        self.width as usize * self.height as usize * self.depth as usize
    }

    /// Returns number of non-air positions.
    pub fn position_count(&self) -> usize {
        self.positions.len()
    }

    /// Sorts positions by (z, y, x) for deterministic serialization.
    /// This improves compression ratios.
    pub fn sort_positions(&mut self) {
        self.positions.sort_by_key(|p| (p.z, p.y, p.x));
    }

    /// Saves this stencil to compressed binary data.
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        // Validate before serialization
        self.validate()?;

        // Serialize with bincode
        let binary = bincode::serde::encode_to_vec(self, bincode::config::legacy())
            .map_err(|e| format!("Bincode serialization failed: {}", e))?;

        // Compress with zstd level 3
        let compressed = zstd::encode_all(&binary[..], 3)
            .map_err(|e| format!("Zstd compression failed: {}", e))?;

        Ok(compressed)
    }

    /// Loads a stencil from compressed binary data.
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        // Decompress with zstd
        let decompressed =
            zstd::decode_all(data).map_err(|e| format!("Zstd decompression failed: {}", e))?;

        // Deserialize with bincode
        let (stencil, _): (StencilFile, _) =
            bincode::serde::decode_from_slice(&decompressed, bincode::config::legacy())
                .map_err(|e| format!("Bincode deserialization failed: {}", e))?;

        // Validate after deserialization
        stencil.validate()?;

        Ok(stencil)
    }

    /// Checks if a position is within stencil bounds.
    pub fn contains(&self, x: u8, y: u8, z: u8) -> bool {
        x < self.width && y < self.height && z < self.depth
    }

    /// Checks if a relative position has a block in the stencil.
    pub fn has_position(&self, x: u8, y: u8, z: u8) -> bool {
        self.positions
            .iter()
            .any(|p| p.x == x && p.y == y && p.z == z)
    }

    /// Iterates over all positions in the stencil.
    pub fn iter_positions(&self) -> impl Iterator<Item = (u8, u8, u8)> + '_ {
        self.positions.iter().map(|p| (p.x, p.y, p.z))
    }
}

/// Helper struct for building stencils from world regions.
pub struct StencilBuilder {
    stencil: StencilFile,
    origin: Vector3<i32>, // World origin for relative coords
}

#[allow(dead_code)]
impl StencilBuilder {
    /// Creates a new stencil builder with specified dimensions.
    pub fn new(
        name: String,
        author: String,
        origin: Vector3<i32>,
        width: u8,
        height: u8,
        depth: u8,
    ) -> Self {
        Self {
            stencil: StencilFile::new(name, author, width, height, depth),
            origin,
        }
    }

    /// Adds a position to the stencil (if block is not air).
    pub fn add_position(&mut self, pos: Vector3<i32>, block_type: BlockType) {
        if block_type == BlockType::Air {
            return;
        }

        let (x, y, z) = self.world_to_stencil(pos);
        if !self.stencil.contains(x, y, z) {
            return;
        }

        self.stencil.positions.push(StencilPosition { x, y, z });
    }

    /// Adds tags to the stencil.
    pub fn add_tags(&mut self, tags: Vec<String>) {
        self.stencil.tags = tags;
    }

    /// Converts world position to stencil-relative position.
    fn world_to_stencil(&self, pos: Vector3<i32>) -> (u8, u8, u8) {
        let rel = pos - self.origin;
        (rel.x as u8, rel.y as u8, rel.z as u8)
    }

    /// Finalizes the stencil, sorting positions for compression.
    pub fn build(mut self) -> StencilFile {
        self.stencil.sort_positions();
        self.stencil
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stencil_validation() {
        let valid = StencilFile::new("test".to_string(), "author".to_string(), 10, 10, 10);
        assert!(valid.validate().is_ok());

        let oversized = StencilFile::new("test".to_string(), "author".to_string(), 255, 10, 10);
        assert!(oversized.validate().is_err());

        let zero_dim = StencilFile::new("test".to_string(), "author".to_string(), 0, 10, 10);
        assert!(zero_dim.validate().is_err());
    }

    #[test]
    fn test_serialization_round_trip() {
        let mut stencil = StencilFile::new("test".to_string(), "author".to_string(), 5, 5, 5);
        stencil.positions.push(StencilPosition { x: 1, y: 2, z: 3 });
        stencil.positions.push(StencilPosition { x: 2, y: 3, z: 4 });

        let bytes = stencil.to_bytes().unwrap();
        let loaded = StencilFile::from_bytes(&bytes).unwrap();

        assert_eq!(stencil.name, loaded.name);
        assert_eq!(stencil.width, loaded.width);
        assert_eq!(stencil.positions.len(), loaded.positions.len());
    }

    #[test]
    fn test_builder() {
        let origin = Vector3::new(100, 64, 200);
        let mut builder =
            StencilBuilder::new("test".to_string(), "author".to_string(), origin, 3, 3, 3);

        builder.add_position(Vector3::new(100, 64, 200), BlockType::Stone);
        builder.add_position(Vector3::new(101, 64, 200), BlockType::Dirt);
        builder.add_position(Vector3::new(102, 64, 200), BlockType::Air); // Should be ignored

        let stencil = builder.build();
        assert_eq!(stencil.positions.len(), 2);
        assert!(stencil.has_position(0, 0, 0));
        assert!(stencil.has_position(1, 0, 0));
        assert!(!stencil.has_position(2, 0, 0)); // Air was ignored
    }

    #[test]
    fn test_position_sorting() {
        let mut stencil = StencilFile::new("test".to_string(), "author".to_string(), 3, 3, 3);
        stencil.positions.push(StencilPosition { x: 2, y: 1, z: 0 });
        stencil.positions.push(StencilPosition { x: 0, y: 0, z: 0 });
        stencil.positions.push(StencilPosition { x: 1, y: 2, z: 1 });

        stencil.sort_positions();

        // Should be sorted by (z, y, x)
        assert_eq!(stencil.positions[0], StencilPosition { x: 0, y: 0, z: 0 });
        assert_eq!(stencil.positions[1], StencilPosition { x: 2, y: 1, z: 0 });
        assert_eq!(stencil.positions[2], StencilPosition { x: 1, y: 2, z: 1 });
    }
}

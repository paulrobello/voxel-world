/// Portable file format for world region templates (.vxt).
///
/// This module defines the VxtFile format for saving and loading world regions
/// with all block metadata (models, tint, paint, water sources).
use crate::chunk::{BlockType, WaterType};
use nalgebra::Vector3;
use serde::{Deserialize, Serialize};

/// Magic bytes for VXT format "VXT1"
const VXT_MAGIC: [u8; 4] = *b"VXT1";

/// Current version of the VXT format.
const VXT_VERSION: u16 = 1;

/// Maximum template dimension in blocks (128×128×128 = 2,097,152 blocks).
pub const MAX_TEMPLATE_SIZE: u8 = 128;

/// A portable file format for world region templates (.vxt).
/// This allows regions to be copied, saved, and placed with rotation.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VxtFile {
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

    // Sparse block data (only non-air blocks)
    pub blocks: Vec<TemplateBlock>,

    // Sparse metadata arrays
    pub model_data: Vec<TemplateModelData>,
    pub tint_data: Vec<TemplateTintData>,
    pub paint_data: Vec<TemplatePaintData>,
    pub water_data: Vec<TemplateWaterData>,
}

/// A single non-air block in the template.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct TemplateBlock {
    pub x: u8,
    pub y: u8,
    pub z: u8,
    pub block_type: u8, // BlockType as u8
}

/// Model block metadata (for BlockType::Model).
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct TemplateModelData {
    pub x: u8,
    pub y: u8,
    pub z: u8,
    pub model_id: u8,
    pub rotation: u8, // 0-3 (0°/90°/180°/270°)
    pub waterlogged: bool,
}

/// Tinted glass/crystal metadata (for BlockType::TintedGlass, BlockType::Crystal).
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct TemplateTintData {
    pub x: u8,
    pub y: u8,
    pub z: u8,
    pub tint_index: u8, // 0-31
}

/// Painted block metadata (for BlockType::Painted).
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct TemplatePaintData {
    pub x: u8,
    pub y: u8,
    pub z: u8,
    pub texture_idx: u8,
    pub tint_idx: u8,
}

/// Water block metadata (for BlockType::Water).
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct TemplateWaterData {
    pub x: u8,
    pub y: u8,
    pub z: u8,
    pub water_type: u8, // WaterType as u8
    pub is_source: bool,
}

impl VxtFile {
    /// Creates a new empty template.
    pub fn new(name: String, author: String, width: u8, height: u8, depth: u8) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            magic: VXT_MAGIC,
            version: VXT_VERSION,
            name,
            author,
            tags: Vec::new(),
            creation_date: timestamp,
            width,
            height,
            depth,
            blocks: Vec::new(),
            model_data: Vec::new(),
            tint_data: Vec::new(),
            paint_data: Vec::new(),
            water_data: Vec::new(),
        }
    }

    /// Validates template dimensions.
    pub fn validate_dimensions(&self) -> Result<(), String> {
        if self.width == 0 || self.height == 0 || self.depth == 0 {
            return Err("Template dimensions must be at least 1×1×1".to_string());
        }

        if self.width > MAX_TEMPLATE_SIZE
            || self.height > MAX_TEMPLATE_SIZE
            || self.depth > MAX_TEMPLATE_SIZE
        {
            return Err(format!(
                "Template dimensions exceed maximum of {}×{}×{}",
                MAX_TEMPLATE_SIZE, MAX_TEMPLATE_SIZE, MAX_TEMPLATE_SIZE
            ));
        }

        Ok(())
    }

    /// Validates magic bytes and version.
    pub fn validate(&self) -> Result<(), String> {
        if self.magic != VXT_MAGIC {
            return Err("Invalid magic bytes for VXT file".to_string());
        }

        if self.version > VXT_VERSION {
            return Err(format!(
                "VXT version {} is newer than supported version {}",
                self.version, VXT_VERSION
            ));
        }

        self.validate_dimensions()?;

        Ok(())
    }

    /// Calculates total volume in blocks.
    pub fn volume(&self) -> usize {
        self.width as usize * self.height as usize * self.depth as usize
    }

    /// Calculates number of non-air blocks.
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Sorts all metadata by position for deterministic serialization.
    /// This improves compression ratios.
    pub fn sort_metadata(&mut self) {
        self.blocks.sort_by_key(|b| (b.z, b.y, b.x));
        self.model_data.sort_by_key(|m| (m.z, m.y, m.x));
        self.tint_data.sort_by_key(|t| (t.z, t.y, t.x));
        self.paint_data.sort_by_key(|p| (p.z, p.y, p.x));
        self.water_data.sort_by_key(|w| (w.z, w.y, w.x));
    }

    /// Saves this template to compressed binary data.
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        // Validate before serialization
        self.validate()?;

        // Serialize with bincode
        let binary =
            bincode::serialize(self).map_err(|e| format!("Bincode serialization failed: {}", e))?;

        // Compress with zstd level 3
        let compressed = zstd::encode_all(&binary[..], 3)
            .map_err(|e| format!("Zstd compression failed: {}", e))?;

        Ok(compressed)
    }

    /// Loads a template from compressed binary data.
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        // Decompress with zstd
        let decompressed =
            zstd::decode_all(data).map_err(|e| format!("Zstd decompression failed: {}", e))?;

        // Deserialize with bincode
        let template: VxtFile = bincode::deserialize(&decompressed)
            .map_err(|e| format!("Bincode deserialization failed: {}", e))?;

        // Validate after deserialization
        template.validate()?;

        Ok(template)
    }

    /// Checks if a position is within template bounds.
    pub fn contains(&self, x: u8, y: u8, z: u8) -> bool {
        x < self.width && y < self.height && z < self.depth
    }

    /// Gets the block type at a position, or Air if not set.
    pub fn get_block(&self, x: u8, y: u8, z: u8) -> BlockType {
        self.blocks
            .iter()
            .find(|b| b.x == x && b.y == y && b.z == z)
            .map(|b| BlockType::from(b.block_type))
            .unwrap_or(BlockType::Air)
    }

    /// Gets model data at a position, if it exists.
    pub fn get_model_data(&self, x: u8, y: u8, z: u8) -> Option<TemplateModelData> {
        self.model_data
            .iter()
            .find(|m| m.x == x && m.y == y && m.z == z)
            .copied()
    }

    /// Gets tint data at a position, if it exists.
    pub fn get_tint_data(&self, x: u8, y: u8, z: u8) -> Option<u8> {
        self.tint_data
            .iter()
            .find(|t| t.x == x && t.y == y && t.z == z)
            .map(|t| t.tint_index)
    }

    /// Gets paint data at a position, if it exists.
    pub fn get_paint_data(&self, x: u8, y: u8, z: u8) -> Option<(u8, u8)> {
        self.paint_data
            .iter()
            .find(|p| p.x == x && p.y == y && p.z == z)
            .map(|p| (p.texture_idx, p.tint_idx))
    }

    /// Gets water data at a position, if it exists.
    pub fn get_water_data(&self, x: u8, y: u8, z: u8) -> Option<(WaterType, bool)> {
        self.water_data
            .iter()
            .find(|w| w.x == x && w.y == y && w.z == z)
            .map(|w| {
                let water_type = match w.water_type {
                    0 => WaterType::Ocean,
                    1 => WaterType::Lake,
                    2 => WaterType::River,
                    3 => WaterType::Swamp,
                    4 => WaterType::Spring,
                    _ => WaterType::Ocean, // Default fallback
                };
                (water_type, w.is_source)
            })
    }
}

/// Helper struct for building templates from world regions.
pub struct TemplateBuilder {
    template: VxtFile,
    origin: Vector3<i32>, // World origin for relative coords
}

impl TemplateBuilder {
    /// Creates a new template builder with specified dimensions.
    pub fn new(
        name: String,
        author: String,
        origin: Vector3<i32>,
        width: u8,
        height: u8,
        depth: u8,
    ) -> Self {
        Self {
            template: VxtFile::new(name, author, width, height, depth),
            origin,
        }
    }

    /// Adds a block to the template (if not air).
    pub fn add_block(&mut self, pos: Vector3<i32>, block_type: BlockType) {
        if block_type == BlockType::Air {
            return;
        }

        let (x, y, z) = self.world_to_template(pos);
        if !self.template.contains(x, y, z) {
            return;
        }

        self.template.blocks.push(TemplateBlock {
            x,
            y,
            z,
            block_type: block_type as u8,
        });
    }

    /// Adds model metadata.
    pub fn add_model_data(
        &mut self,
        pos: Vector3<i32>,
        model_id: u8,
        rotation: u8,
        waterlogged: bool,
    ) {
        let (x, y, z) = self.world_to_template(pos);
        if !self.template.contains(x, y, z) {
            return;
        }

        self.template.model_data.push(TemplateModelData {
            x,
            y,
            z,
            model_id,
            rotation,
            waterlogged,
        });
    }

    /// Adds tint metadata.
    pub fn add_tint_data(&mut self, pos: Vector3<i32>, tint_index: u8) {
        let (x, y, z) = self.world_to_template(pos);
        if !self.template.contains(x, y, z) {
            return;
        }

        self.template.tint_data.push(TemplateTintData {
            x,
            y,
            z,
            tint_index,
        });
    }

    /// Adds paint metadata.
    pub fn add_paint_data(&mut self, pos: Vector3<i32>, texture_idx: u8, tint_idx: u8) {
        let (x, y, z) = self.world_to_template(pos);
        if !self.template.contains(x, y, z) {
            return;
        }

        self.template.paint_data.push(TemplatePaintData {
            x,
            y,
            z,
            texture_idx,
            tint_idx,
        });
    }

    /// Adds water metadata.
    pub fn add_water_data(&mut self, pos: Vector3<i32>, water_type: WaterType, is_source: bool) {
        let (x, y, z) = self.world_to_template(pos);
        if !self.template.contains(x, y, z) {
            return;
        }

        self.template.water_data.push(TemplateWaterData {
            x,
            y,
            z,
            water_type: water_type as u8,
            is_source,
        });
    }

    /// Adds tags to the template.
    pub fn add_tags(&mut self, tags: Vec<String>) {
        self.template.tags = tags;
    }

    /// Converts world position to template-relative position.
    fn world_to_template(&self, pos: Vector3<i32>) -> (u8, u8, u8) {
        let rel = pos - self.origin;
        (rel.x as u8, rel.y as u8, rel.z as u8)
    }

    /// Finalizes the template, sorting metadata for compression.
    pub fn build(mut self) -> VxtFile {
        self.template.sort_metadata();
        self.template
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_validation() {
        let valid = VxtFile::new("test".to_string(), "author".to_string(), 10, 10, 10);
        assert!(valid.validate().is_ok());

        let oversized = VxtFile::new("test".to_string(), "author".to_string(), 255, 10, 10);
        assert!(oversized.validate().is_err());

        let zero_dim = VxtFile::new("test".to_string(), "author".to_string(), 0, 10, 10);
        assert!(zero_dim.validate().is_err());
    }

    #[test]
    fn test_serialization_round_trip() {
        let mut template = VxtFile::new("test".to_string(), "author".to_string(), 5, 5, 5);
        template.blocks.push(TemplateBlock {
            x: 1,
            y: 2,
            z: 3,
            block_type: BlockType::Stone as u8,
        });

        let bytes = template.to_bytes().unwrap();
        let loaded = VxtFile::from_bytes(&bytes).unwrap();

        assert_eq!(template.name, loaded.name);
        assert_eq!(template.width, loaded.width);
        assert_eq!(template.blocks.len(), loaded.blocks.len());
    }

    #[test]
    fn test_builder() {
        let origin = Vector3::new(100, 64, 200);
        let mut builder =
            TemplateBuilder::new("test".to_string(), "author".to_string(), origin, 3, 3, 3);

        builder.add_block(Vector3::new(100, 64, 200), BlockType::Stone);
        builder.add_block(Vector3::new(101, 64, 200), BlockType::Dirt);

        let template = builder.build();
        assert_eq!(template.blocks.len(), 2);
        assert_eq!(template.get_block(0, 0, 0), BlockType::Stone);
        assert_eq!(template.get_block(1, 0, 0), BlockType::Dirt);
    }
}

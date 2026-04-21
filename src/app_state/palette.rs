use crate::chunk::BlockType;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PaletteTab {
    #[default]
    All,
    Blocks,
    Models,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaletteItem {
    pub block: BlockType,
    /// For non-Model blocks this is 0; for Model blocks this is the registry model_id.
    pub model_id: u8,
    /// For TintedGlass blocks, the tint color index (0-31). Ignored for other block types.
    pub tint_index: u8,
    /// For Painted blocks, the atlas texture index (0-based). Ignored for other block types.
    pub paint_texture_idx: u8,
    /// For Water blocks, the water type index (0-4).
    pub water_type: crate::chunk::WaterType,
}

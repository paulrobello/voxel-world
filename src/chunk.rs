//! Chunk data structure for voxel storage.
//!
//! Each chunk is a 32³ grid of blocks. Blocks are stored as u8 values
//! where 0 = air and other values represent different block types.
//!
//! Blocks of type `Model` use sparse metadata storage to associate
//! a model_id and rotation with each model block.

#![allow(dead_code)]

use std::cell::{Cell, Ref, RefCell};
use std::collections::HashMap;
use std::slice;
use std::sync::Arc;
use vulkano::image::view::ImageView;

/// Size of a chunk in each dimension (32³ = 32,768 blocks per chunk).
pub const CHUNK_SIZE: usize = 32;

/// Total number of blocks in a chunk.
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

/// Tint color palette matching TINT_PALETTE in shaders/common.glsl.
/// RGB values for 32 tint colors (indices 0-31).
pub const TINT_PALETTE: [[f32; 3]; 32] = [
    [1.0, 0.2, 0.2],    // 0: Red
    [1.0, 0.5, 0.2],    // 1: Orange
    [1.0, 1.0, 0.2],    // 2: Yellow
    [0.5, 1.0, 0.2],    // 3: Lime
    [0.2, 1.0, 0.2],    // 4: Green
    [0.2, 1.0, 0.5],    // 5: Teal
    [0.2, 1.0, 1.0],    // 6: Cyan
    [0.2, 0.5, 1.0],    // 7: Sky blue
    [0.2, 0.2, 1.0],    // 8: Blue
    [0.5, 0.2, 1.0],    // 9: Purple
    [1.0, 0.2, 1.0],    // 10: Magenta
    [1.0, 0.2, 0.5],    // 11: Pink
    [0.95, 0.95, 0.95], // 12: White
    [0.6, 0.6, 0.6],    // 13: Light gray
    [0.3, 0.3, 0.3],    // 14: Dark gray
    [0.4, 0.25, 0.1],   // 15: Brown
    [0.8, 0.4, 0.4],    // 16: Light red
    [0.8, 0.6, 0.4],    // 17: Peach
    [0.8, 0.8, 0.4],    // 18: Light yellow
    [0.6, 0.8, 0.4],    // 19: Light lime
    [0.4, 0.8, 0.4],    // 20: Light green
    [0.4, 0.8, 0.6],    // 21: Light teal
    [0.4, 0.8, 0.8],    // 22: Light cyan
    [0.4, 0.6, 0.8],    // 23: Light sky
    [0.4, 0.4, 0.8],    // 24: Light blue
    [0.6, 0.4, 0.8],    // 25: Light purple
    [0.8, 0.4, 0.8],    // 26: Light magenta
    [0.8, 0.4, 0.6],    // 27: Light pink
    [0.2, 0.15, 0.1],   // 28: Dark brown
    [0.1, 0.2, 0.1],    // 29: Dark green
    [0.1, 0.1, 0.2],    // 30: Dark blue
    [0.2, 0.1, 0.2],    // 31: Dark purple
];

/// Returns the RGB tint color for a given tint index.
/// Returns a default gray for indices >= 32.
pub fn tint_color(tint_index: u8) -> [f32; 3] {
    if (tint_index as usize) < TINT_PALETTE.len() {
        TINT_PALETTE[tint_index as usize]
    } else {
        [0.5, 0.5, 0.5]
    }
}

/// Block types that can exist in the world.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Hash, serde::Serialize, serde::Deserialize,
)]
#[repr(u8)]
pub enum BlockType {
    #[default]
    Air = 0,
    Stone = 1,
    Dirt = 2,
    Grass = 3,
    Planks = 4,
    Leaves = 5,
    Sand = 6,
    Gravel = 7,
    Water = 8,
    Glass = 9,
    Log = 10,
    /// Sub-voxel model block. Use BlockModelData to get model_id and rotation.
    Model = 11,
    Brick = 12,
    Snow = 13,
    Cobblestone = 14,
    Iron = 15,
    Bedrock = 16,
    /// Tinted glass block. Use tint_data to get color index (0-31).
    TintedGlass = 17,
    /// Paintable block. Texture and tint are stored per-block in metadata.
    /// **USER-ONLY**: This block is for player customization only.
    /// NEVER use this block in world/terrain generation - create dedicated block types instead.
    Painted = 18,
    /// Lava block - glowing orange/red, decorative (no damage).
    Lava = 19,
    /// GlowStone - bright warm white light source.
    GlowStone = 20,
    /// Glowing mushroom - soft cyan/blue glow for caves.
    GlowMushroom = 21,
    /// Crystal block - colored glowing crystal. Uses tint_data for color (0-31).
    Crystal = 22,
    /// Pine tree log (darker brown).
    PineLog = 23,
    /// Willow tree log (brown).
    WillowLog = 24,
    /// Pine tree leaves (dark green).
    PineLeaves = 25,
    /// Willow tree leaves (olive green).
    WillowLeaves = 26,
    /// Ice block - transparent frozen water.
    Ice = 27,
    /// Mud block - thick liquid that slows movement.
    Mud = 28,
    /// Sandstone block - desert subsurface.
    Sandstone = 29,
    /// Cactus block - desert plant.
    Cactus = 30,
    /// Decorative stone - polished stone with patterns for building.
    DecorativeStone = 31,
    /// Concrete block - smooth manufactured gray material.
    Concrete = 32,
    /// Deepslate - dark stone found deep underground.
    Deepslate = 33,
    /// Moss block - soft green plant material.
    Moss = 34,
    /// Mossy cobblestone - cobblestone with moss growth.
    MossyCobblestone = 35,
    /// Clay block - soft gray-blue sedimentary material.
    Clay = 36,
    /// Dripstone block - cave formation material.
    Dripstone = 37,
    /// Calcite - white crystalline mineral.
    Calcite = 38,
    /// Terracotta - fired clay in natural orange-brown.
    Terracotta = 39,
    /// Packed ice - dense, opaque ice.
    PackedIce = 40,
    /// Podzol - forest floor soil with decomposing matter.
    Podzol = 41,
    /// Mycelium - purple-gray fungal surface.
    Mycelium = 42,
    /// Coarse dirt - rough dirt that doesn't grow grass.
    CoarseDirt = 43,
    /// Rooted dirt - dirt with visible roots.
    RootedDirt = 44,
    /// Birch tree log (white bark).
    BirchLog = 45,
    /// Birch tree leaves (light green).
    BirchLeaves = 46,
}

/// Water types for enhanced water system.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Hash, serde::Serialize, serde::Deserialize,
)]
#[repr(u8)]
pub enum WaterType {
    #[default]
    Ocean = 0,
    Lake = 1,
    River = 2,
    Swamp = 3,
    Spring = 4,
}

impl WaterType {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => WaterType::Ocean,
            1 => WaterType::Lake,
            2 => WaterType::River,
            3 => WaterType::Swamp,
            4 => WaterType::Spring,
            _ => WaterType::Ocean,
        }
    }
}

/// Metadata for a block that uses a sub-voxel model.
///
/// This is stored sparsely in chunks - only blocks of type `Model` have metadata.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BlockModelData {
    /// Model ID from the model registry (1 = torch, 2 = slab_bottom, etc.).
    pub model_id: u8,

    /// Rotation around Y axis (0-3 = 0°/90°/180°/270°).
    pub rotation: u8,

    /// Whether this block is waterlogged (contains water in the same space).
    pub waterlogged: bool,

    /// Custom data for special model types (e.g., picture frames).
    /// For frames: picture_id (20 bits) | offset_x (2 bits) | offset_y (2 bits)
    /// | width_minus_one (2 bits) | height_minus_one (2 bits) | facing (2 bits)
    pub custom_data: u32,
}

/// Metadata for a paintable block (per-block texture + tint + blend mode).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BlockPaintData {
    /// Atlas texture index to sample (0-based, or 128+ for custom textures).
    pub texture_idx: u8,
    /// Tint palette index (0-31).
    pub tint_idx: u8,
    /// Blend mode (0=Multiply, 1=Overlay, 2=SoftLight, 3=Screen, 4=ColorOnly).
    pub blend_mode: u8,
}

impl BlockPaintData {
    /// Creates new paint data with all parameters.
    pub fn new(texture_idx: u8, tint_idx: u8, blend_mode: u8) -> Self {
        Self {
            texture_idx,
            tint_idx: tint_idx & 0x1F,
            blend_mode: blend_mode.min(4),
        }
    }

    /// Creates simple paint data with default multiply blend.
    pub fn simple(texture_idx: u8, tint_idx: u8) -> Self {
        Self {
            texture_idx,
            tint_idx: tint_idx & 0x1F,
            blend_mode: 0,
        }
    }

    /// Packs tint_idx and blend_mode into a single byte for GPU metadata.
    /// bits 0-4: tint_idx, bits 5-7: blend_mode
    pub fn packed_tint_blend(&self) -> u8 {
        (self.tint_idx & 0x1F) | ((self.blend_mode & 0x07) << 5)
    }
}

impl BlockType {
    /// Canonical block names used for parsing and autocomplete (no aliases).
    pub const NAME_TABLE: &[(Self, &str)] = &[
        (Self::Air, "air"),
        (Self::Stone, "stone"),
        (Self::Dirt, "dirt"),
        (Self::Grass, "grass"),
        (Self::Planks, "planks"),
        (Self::Leaves, "leaves"),
        (Self::Sand, "sand"),
        (Self::Gravel, "gravel"),
        (Self::Water, "water"),
        (Self::Glass, "glass"),
        (Self::Log, "log"),
        (Self::Brick, "brick"),
        (Self::Snow, "snow"),
        (Self::Ice, "ice"),
        (Self::Cobblestone, "cobblestone"),
        (Self::Iron, "iron"),
        (Self::Bedrock, "bedrock"),
        (Self::TintedGlass, "tintedglass"),
        (Self::Painted, "painted"),
        (Self::Lava, "lava"),
        (Self::GlowStone, "glowstone"),
        (Self::GlowMushroom, "glowmushroom"),
        (Self::Crystal, "crystal"),
        (Self::PineLog, "pinelog"),
        (Self::WillowLog, "willowlog"),
        (Self::PineLeaves, "pineleaves"),
        (Self::WillowLeaves, "willowleaves"),
        (Self::BirchLog, "birchlog"),
        (Self::BirchLeaves, "birchleaves"),
        (Self::Mud, "mud"),
        (Self::Sandstone, "sandstone"),
        (Self::Cactus, "cactus"),
        (Self::DecorativeStone, "decorativestone"),
        (Self::Concrete, "concrete"),
        (Self::Deepslate, "deepslate"),
        (Self::Moss, "moss"),
        (Self::MossyCobblestone, "mossycobblestone"),
        (Self::Clay, "clay"),
        (Self::Dripstone, "dripstone"),
        (Self::Calcite, "calcite"),
        (Self::Terracotta, "terracotta"),
        (Self::PackedIce, "packedice"),
        (Self::Podzol, "podzol"),
        (Self::Mycelium, "mycelium"),
        (Self::CoarseDirt, "coarsedirt"),
        (Self::RootedDirt, "rooteddirt"),
    ];

    /// Returns true if this block type is solid (not air, water, glass, or model blocks).
    /// Note: Model blocks may have sub-voxel collision, but are not solid at block level.
    #[inline]
    pub fn is_solid(self) -> bool {
        !matches!(
            self,
            BlockType::Air
                | BlockType::Water
                | BlockType::Model
                | BlockType::Glass
                | BlockType::TintedGlass
                | BlockType::Lava
                | BlockType::Ice
        )
    }

    /// Returns true if this block can be targeted by raycast for breaking/interaction.
    /// Includes Model blocks which are not solid but can still be broken.
    #[inline]
    pub fn is_targetable(self) -> bool {
        !matches!(self, BlockType::Air | BlockType::Water)
    }

    /// Returns true if this block type is affected by gravity (sand, gravel, snow).
    #[inline]
    pub fn is_affected_by_gravity(self) -> bool {
        matches!(self, BlockType::Sand | BlockType::Gravel | BlockType::Snow)
    }

    /// Returns true if this block is a log (tree trunk).
    #[inline]
    pub fn is_log(self) -> bool {
        matches!(
            self,
            BlockType::Log | BlockType::PineLog | BlockType::WillowLog | BlockType::BirchLog
        )
    }

    /// Returns true if this block is part of a tree (log or leaves).
    #[inline]
    pub fn is_tree_part(self) -> bool {
        matches!(
            self,
            BlockType::Log
                | BlockType::Leaves
                | BlockType::PineLog
                | BlockType::WillowLog
                | BlockType::BirchLog
                | BlockType::PineLeaves
                | BlockType::WillowLeaves
                | BlockType::BirchLeaves
        )
    }

    /// Returns true if this block type is transparent.
    #[inline]
    pub fn is_transparent(self) -> bool {
        matches!(
            self,
            BlockType::Air
                | BlockType::Water
                | BlockType::Glass
                | BlockType::TintedGlass
                | BlockType::Leaves
                | BlockType::PineLeaves
                | BlockType::WillowLeaves
                | BlockType::BirchLeaves
                | BlockType::Model
                | BlockType::Lava
                | BlockType::Ice
                | BlockType::Mud
        )
    }

    /// Returns true if this block type emits point light onto surroundings.
    /// Note: Lava self-illuminates but doesn't cast point lights (too many blocks).
    /// For Model blocks, check the model's emission property instead.
    #[inline]
    pub fn is_light_source(self) -> bool {
        matches!(
            self,
            BlockType::GlowStone | BlockType::GlowMushroom | BlockType::Crystal
        )
    }

    /// Returns the light color and intensity for point light-emitting blocks.
    /// Returns (color RGB, intensity) or None if not a point light source.
    /// Note: Lava self-illuminates in shader but doesn't use point lights.
    /// For Model blocks, use the model registry to get emission properties.
    #[inline]
    pub fn light_properties(self) -> Option<([f32; 3], f32)> {
        match self {
            BlockType::GlowStone => Some(([1.0, 0.95, 0.8], 1.0)), // Warm white, full intensity
            BlockType::GlowMushroom => Some(([0.3, 0.9, 1.0], 0.6)), // Cyan, medium intensity
            BlockType::Crystal => Some(([0.8, 0.8, 1.0], 0.7)), // Default white-blue (tint overrides)
            _ => None,
        }
    }

    /// Returns the emission color for emissive blocks (RGB, 0-1 range).
    /// Returns None if the block doesn't emit light.
    #[inline]
    pub fn emission_color(self) -> Option<[f32; 3]> {
        self.light_properties().map(|(color, _)| color)
    }

    /// Returns the emission strength for emissive blocks (0-1 range).
    /// Returns 0.0 if the block doesn't emit light.
    #[inline]
    pub fn emission_strength(self) -> f32 {
        self.light_properties()
            .map(|(_, strength)| strength)
            .unwrap_or(0.0)
    }

    /// Returns true if this block is part of a tree structure (logs or leaves).
    /// Used for overflow block placement priority.
    #[inline]
    pub fn is_tree_structure(self) -> bool {
        matches!(
            self,
            BlockType::Log
                | BlockType::Leaves
                | BlockType::PineLog
                | BlockType::PineLeaves
                | BlockType::WillowLog
                | BlockType::WillowLeaves
                | BlockType::BirchLog
                | BlockType::BirchLeaves
        )
    }

    /// Returns true if this block can be replaced by tree structure during overflow.
    /// Allows trees to replace surface terrain like grass and dirt.
    #[inline]
    pub fn is_replaceable_terrain(self) -> bool {
        matches!(self, BlockType::Grass | BlockType::Dirt | BlockType::Sand)
    }

    /// Returns the light radius in blocks for dynamic point light emission.
    /// Only used when dynamic lighting is enabled.
    /// Note: Lava self-illuminates but doesn't cast point lights.
    #[inline]
    pub fn light_radius(self) -> f32 {
        match self {
            BlockType::GlowStone => 16.0,
            BlockType::GlowMushroom => 8.0,
            BlockType::Crystal => 10.0,
            _ => 0.0,
        }
    }

    /// Returns the light animation mode for point lights.
    /// 0 = steady, 1 = slow pulse, 2 = torch flicker
    #[inline]
    pub fn light_mode(self) -> u8 {
        match self {
            BlockType::GlowStone => 0,    // Steady
            BlockType::GlowMushroom => 1, // Slow pulse
            BlockType::Crystal => 1,      // Slow pulse
            _ => 2,                       // Default to flicker for torches etc
        }
    }

    /// Returns the color for this block type (RGB, 0-1 range).
    /// Note: Model blocks use their sub-voxel palette for coloring.
    #[inline]
    pub fn color(self) -> [f32; 3] {
        match self {
            BlockType::Air => [0.0, 0.0, 0.0],
            BlockType::Stone => [0.5, 0.5, 0.5],
            BlockType::Dirt => [0.6, 0.4, 0.2],
            BlockType::Grass => [0.3, 0.7, 0.2],
            BlockType::Planks => [0.6, 0.4, 0.2],
            BlockType::Leaves => [0.2, 0.6, 0.1],
            BlockType::Sand => [0.9, 0.8, 0.5],
            BlockType::Gravel => [0.4, 0.4, 0.4],
            BlockType::Water => [0.2, 0.4, 0.8],
            BlockType::Glass => [0.8, 0.9, 1.0],
            BlockType::Log => [0.4, 0.3, 0.2],
            BlockType::Model => [0.5, 0.5, 0.5], // Fallback gray (uses sub-voxel colors)
            BlockType::Brick => [0.7, 0.35, 0.3],
            BlockType::Snow => [0.95, 0.95, 0.98],
            BlockType::Cobblestone => [0.45, 0.45, 0.45],
            BlockType::Iron => [0.75, 0.75, 0.78],
            BlockType::Bedrock => [0.2, 0.2, 0.2], // Dark gray, nearly black
            BlockType::TintedGlass => [0.7, 0.8, 0.9], // Light blue-gray base
            BlockType::Painted => [0.8, 0.8, 0.8], // Neutral base; actual color comes from metadata
            BlockType::Lava => [1.0, 0.4, 0.1],    // Molten orange-red
            BlockType::GlowStone => [1.0, 0.95, 0.8], // Warm yellow-white
            BlockType::GlowMushroom => [0.3, 0.9, 1.0], // Cyan-blue
            BlockType::Crystal => [0.8, 0.8, 1.0], // Light blue-white (tint overrides)
            BlockType::PineLog => [0.35, 0.25, 0.15], // Darker brown
            BlockType::WillowLog => [0.45, 0.35, 0.25], // Brown
            BlockType::PineLeaves => [0.15, 0.5, 0.1], // Dark green
            BlockType::WillowLeaves => [0.4, 0.5, 0.2], // Olive green
            BlockType::Ice => [0.7, 0.85, 0.95],   // Light blue-white, transparent ice
            BlockType::Mud => [0.4, 0.3, 0.2],     // Dark brown, muddy
            BlockType::Sandstone => [0.9, 0.8, 0.6], // Light tan
            BlockType::Cactus => [0.3, 0.6, 0.3],  // Green
            BlockType::DecorativeStone => [0.6, 0.6, 0.6], // Medium gray with patterns
            BlockType::Concrete => [0.55, 0.55, 0.55], // Smooth gray
            BlockType::Deepslate => [0.25, 0.25, 0.3], // Dark gray-blue
            BlockType::Moss => [0.3, 0.55, 0.2],   // Forest green
            BlockType::MossyCobblestone => [0.4, 0.5, 0.35], // Gray-green
            BlockType::Clay => [0.6, 0.6, 0.7],    // Blue-gray
            BlockType::Dripstone => [0.55, 0.5, 0.45], // Tan-brown
            BlockType::Calcite => [0.9, 0.9, 0.85], // Off-white
            BlockType::Terracotta => [0.7, 0.45, 0.35], // Orange-brown
            BlockType::PackedIce => [0.6, 0.75, 0.9], // Blue-white
            BlockType::Podzol => [0.45, 0.35, 0.25], // Brown-orange
            BlockType::Mycelium => [0.5, 0.45, 0.55], // Purple-gray
            BlockType::CoarseDirt => [0.5, 0.35, 0.2], // Brown
            BlockType::RootedDirt => [0.55, 0.4, 0.25], // Brown with roots
            BlockType::BirchLog => [0.85, 0.82, 0.75], // White-gray bark
            BlockType::BirchLeaves => [0.45, 0.7, 0.3], // Light green
        }
    }

    /// Returns the time in seconds to break this block type.
    /// Higher values = takes longer to break.
    #[inline]
    pub fn break_time(self) -> f32 {
        match self {
            BlockType::Air => 0.0,
            // Very fast (instant)
            BlockType::Leaves
            | BlockType::PineLeaves
            | BlockType::WillowLeaves
            | BlockType::BirchLeaves
            | BlockType::Model
            | BlockType::Cactus
            | BlockType::Moss => 0.15,
            // Fast
            BlockType::Dirt
            | BlockType::Sand
            | BlockType::Gravel
            | BlockType::Snow
            | BlockType::Mud
            | BlockType::Clay
            | BlockType::CoarseDirt
            | BlockType::RootedDirt
            | BlockType::Podzol
            | BlockType::Mycelium => 0.3,
            // Normal
            BlockType::Grass
            | BlockType::Planks
            | BlockType::Log
            | BlockType::PineLog
            | BlockType::WillowLog
            | BlockType::BirchLog
            | BlockType::Glass
            | BlockType::TintedGlass
            | BlockType::Painted
            | BlockType::Ice
            | BlockType::PackedIce
            | BlockType::Terracotta => 0.5,
            // Slow
            BlockType::Stone
            | BlockType::Cobblestone
            | BlockType::MossyCobblestone
            | BlockType::Brick
            | BlockType::Sandstone
            | BlockType::DecorativeStone
            | BlockType::Concrete
            | BlockType::Dripstone
            | BlockType::Calcite => 0.8,
            // Very slow
            BlockType::Iron | BlockType::Deepslate => 1.2,
            // Emissive blocks (medium difficulty)
            BlockType::GlowStone | BlockType::Crystal => 0.6,
            BlockType::GlowMushroom => 0.2, // Soft mushroom breaks easily
            // Special (can't break or shouldn't)
            BlockType::Water | BlockType::Lava => 0.0, // Fluids can't be broken normally
            // Indestructible
            BlockType::Bedrock => 0.0,
        }
    }

    /// Returns true if this block type uses sub-voxel model rendering.
    #[inline]
    pub fn is_model(self) -> bool {
        matches!(self, BlockType::Model)
    }

    /// Parse a block type from its name (case-insensitive).
    ///
    /// Returns `None` for unrecognized names.
    /// Note: Model blocks still require model metadata to be set separately.
    pub fn from_name(name: &str) -> Option<Self> {
        let lower = name.to_lowercase();
        Self::NAME_TABLE
            .iter()
            .find(|(_, n)| *n == lower)
            .map(|(b, _)| *b)
    }

    /// Returns a list of all valid block names for autocomplete.
    ///
    /// Returns primary names only (no aliases).
    pub fn all_block_names() -> Vec<&'static str> {
        Self::NAME_TABLE.iter().map(|(_, n)| *n).collect()
    }
}

impl From<u8> for BlockType {
    fn from(value: u8) -> Self {
        match value {
            0 => BlockType::Air,
            1 => BlockType::Stone,
            2 => BlockType::Dirt,
            3 => BlockType::Grass,
            4 => BlockType::Planks,
            5 => BlockType::Leaves,
            6 => BlockType::Sand,
            7 => BlockType::Gravel,
            8 => BlockType::Water,
            9 => BlockType::Glass,
            10 => BlockType::Log,
            11 => BlockType::Model,
            12 => BlockType::Brick,
            13 => BlockType::Snow,
            14 => BlockType::Cobblestone,
            15 => BlockType::Iron,
            16 => BlockType::Bedrock,
            17 => BlockType::TintedGlass,
            18 => BlockType::Painted,
            19 => BlockType::Lava,
            20 => BlockType::GlowStone,
            21 => BlockType::GlowMushroom,
            22 => BlockType::Crystal,
            23 => BlockType::PineLog,
            24 => BlockType::WillowLog,
            25 => BlockType::PineLeaves,
            26 => BlockType::WillowLeaves,
            27 => BlockType::Ice,
            28 => BlockType::Mud,
            29 => BlockType::Sandstone,
            30 => BlockType::Cactus,
            31 => BlockType::DecorativeStone,
            32 => BlockType::Concrete,
            33 => BlockType::Deepslate,
            34 => BlockType::Moss,
            35 => BlockType::MossyCobblestone,
            36 => BlockType::Clay,
            37 => BlockType::Dripstone,
            38 => BlockType::Calcite,
            39 => BlockType::Terracotta,
            40 => BlockType::PackedIce,
            41 => BlockType::Podzol,
            42 => BlockType::Mycelium,
            43 => BlockType::CoarseDirt,
            44 => BlockType::RootedDirt,
            45 => BlockType::BirchLog,
            46 => BlockType::BirchLeaves,
            _ => BlockType::Air,
        }
    }
}

/// A chunk of blocks in the voxel world.
///
/// Chunks are 32³ grids of blocks that can be individually loaded,
/// modified, and uploaded to the GPU.
pub struct Chunk {
    /// Block data stored as a flat array.
    /// Index = x + y * CHUNK_SIZE + z * CHUNK_SIZE * CHUNK_SIZE
    blocks: Box<[BlockType; CHUNK_VOLUME]>,

    /// Sparse storage for sub-voxel model metadata.
    /// Only blocks of type `Model` have entries here.
    /// Key: block index, Value: model_id and rotation.
    model_data: HashMap<usize, BlockModelData>,

    /// Sparse storage for tinted glass color indices.
    /// Only blocks of type `TintedGlass` have entries here.
    /// Key: block index, Value: color index (0-31).
    tint_data: HashMap<usize, u8>,

    /// Sparse storage for painted block metadata (texture + tint).
    /// Only blocks of type `Painted` have entries here.
    /// Key: block index, Value: BlockPaintData.
    painted_data: HashMap<usize, BlockPaintData>,

    /// Sparse storage for water type metadata.
    /// Only blocks of type `Water` have entries here.
    /// Key: block index, Value: WaterType (u8).
    water_data: HashMap<usize, WaterType>,

    /// Reusable RG8 buffer for model metadata uploads (len = CHUNK_VOLUME * 2).
    model_metadata_buf: RefCell<Vec<u8>>,
    /// Whether the cached model metadata buffer needs recomputing.
    model_metadata_dirty: Cell<bool>,

    /// Reusable R32 buffer for custom data uploads (len = CHUNK_VOLUME * 4).
    /// Stores per-block custom data (e.g., picture_id, offset_x, offset_y for frames).
    custom_data_buf: RefCell<Vec<u8>>,
    /// Whether the cached custom data buffer needs recomputing.
    custom_data_dirty: Cell<bool>,

    /// Count of non-model light-emitting block types (for quick skip).
    light_block_count: usize,

    /// Whether this chunk has been modified since last GPU upload.
    pub dirty: bool,

    /// Whether this chunk has been modified since last save to disk.
    pub persistence_dirty: bool,

    /// Cached GPU texture for this chunk (if uploaded).
    pub gpu_texture: Option<Arc<ImageView>>,

    /// Cached: true if all blocks are air (for ray skip optimization).
    cached_is_empty: bool,

    /// Cached: true if all blocks are solid (for ray skip optimization).
    cached_is_fully_solid: bool,

    /// Whether cached_is_empty/cached_is_fully_solid need recalculation.
    metadata_dirty: bool,

    /// Cached SVT brick mask (64-bit mask for 4x4x4 bricks).
    cached_brick_mask: u64,

    /// Cached SVT brick distances (64 bytes, one per brick).
    cached_brick_distances: [u8; 64],

    /// Whether the cached SVT data needs recalculation.
    svt_dirty: bool,

    /// Bitmask of which bricks have changed (for incremental SVT updates).
    /// Each bit corresponds to one of the 64 bricks in the chunk.
    dirty_bricks: u64,
}

impl Default for Chunk {
    fn default() -> Self {
        Self::new()
    }
}

impl Chunk {
    /// Creates a new empty chunk (all air).
    pub fn new() -> Self {
        Self {
            blocks: Box::new([BlockType::Air; CHUNK_VOLUME]),
            model_data: HashMap::new(),
            tint_data: HashMap::new(),
            painted_data: HashMap::new(),
            water_data: HashMap::new(),
            model_metadata_buf: RefCell::new(vec![0u8; CHUNK_VOLUME * 2]),
            model_metadata_dirty: Cell::new(false),
            custom_data_buf: RefCell::new(vec![0u8; CHUNK_VOLUME * 4]),
            custom_data_dirty: Cell::new(false),
            light_block_count: 0,
            dirty: true,
            persistence_dirty: true,
            gpu_texture: None,
            cached_is_empty: true,
            cached_is_fully_solid: false,
            metadata_dirty: false,
            cached_brick_mask: 0,
            cached_brick_distances: [255; 64], // 255 = max distance (all air)
            svt_dirty: false,                  // Empty chunk has valid SVT (mask=0)
            dirty_bricks: 0,
        }
    }

    /// Creates a chunk filled with a single block type.
    pub fn filled(block_type: BlockType) -> Self {
        let is_empty = block_type == BlockType::Air;
        let is_solid = block_type.is_solid();
        let light_block_count = if block_type.is_light_source() {
            CHUNK_VOLUME
        } else {
            0
        };
        // For a filled chunk, all bricks are either empty (air) or solid
        let (brick_mask, brick_distances) = if is_empty {
            (0u64, [255u8; 64]) // All empty, max distance
        } else {
            (u64::MAX, [0u8; 64]) // All solid, zero distance
        };
        Self {
            blocks: Box::new([block_type; CHUNK_VOLUME]),
            model_data: HashMap::new(),
            tint_data: HashMap::new(),
            painted_data: HashMap::new(),
            water_data: HashMap::new(),
            model_metadata_buf: RefCell::new(vec![0u8; CHUNK_VOLUME * 2]),
            model_metadata_dirty: Cell::new(false),
            custom_data_buf: RefCell::new(vec![0u8; CHUNK_VOLUME * 4]),
            custom_data_dirty: Cell::new(false),
            light_block_count,
            dirty: true,
            persistence_dirty: true,
            gpu_texture: None,
            cached_is_empty: is_empty,
            cached_is_fully_solid: is_solid,
            metadata_dirty: false,
            cached_brick_mask: brick_mask,
            cached_brick_distances: brick_distances,
            svt_dirty: false, // Filled chunk has valid SVT
            dirty_bricks: 0,
        }
    }

    /// Creates a chunk from network data (received from server).
    /// This is used when loading chunks from multiplayer.
    pub fn from_network_data(
        blocks: Box<[BlockType; CHUNK_VOLUME]>,
        model_data: HashMap<usize, BlockModelData>,
        tint_data: HashMap<usize, u8>,
        painted_data: HashMap<usize, BlockPaintData>,
        water_data: HashMap<usize, WaterType>,
        light_block_count: usize,
    ) -> Self {
        // Calculate caches
        let is_empty = blocks.iter().all(|&b| b == BlockType::Air);
        let is_solid = !is_empty && blocks.iter().all(|&b| b.is_solid());

        Self {
            blocks,
            model_data,
            tint_data,
            painted_data,
            water_data,
            model_metadata_buf: RefCell::new(vec![0u8; CHUNK_VOLUME * 2]),
            model_metadata_dirty: Cell::new(true), // Need to compute on first request
            custom_data_buf: RefCell::new(vec![0u8; CHUNK_VOLUME * 4]),
            custom_data_dirty: Cell::new(true),
            light_block_count,
            dirty: true,
            persistence_dirty: false, // Network chunks are not locally modified
            gpu_texture: None,
            cached_is_empty: is_empty,
            cached_is_fully_solid: is_solid,
            metadata_dirty: false,
            cached_brick_mask: 0,
            cached_brick_distances: [255; 64],
            svt_dirty: true,        // Need to compute SVT
            dirty_bricks: u64::MAX, // All bricks are potentially dirty
        }
    }

    /// Converts local coordinates to a flat array index.
    #[inline]
    fn index(x: usize, y: usize, z: usize) -> usize {
        debug_assert!(x < CHUNK_SIZE && y < CHUNK_SIZE && z < CHUNK_SIZE);
        x + y * CHUNK_SIZE + z * CHUNK_SIZE * CHUNK_SIZE
    }

    /// Converts a flat array index back to local coordinates.
    #[inline]
    pub fn index_to_coords(idx: usize) -> (usize, usize, usize) {
        debug_assert!(idx < CHUNK_VOLUME);
        let x = idx % CHUNK_SIZE;
        let y = (idx / CHUNK_SIZE) % CHUNK_SIZE;
        let z = idx / (CHUNK_SIZE * CHUNK_SIZE);
        (x, y, z)
    }

    /// Gets the block at the given local coordinates.
    #[inline]
    pub fn get_block(&self, x: usize, y: usize, z: usize) -> BlockType {
        self.blocks[Self::index(x, y, z)]
    }

    /// Sets the block at the given local coordinates.
    #[inline]
    pub fn set_block(&mut self, x: usize, y: usize, z: usize, block: BlockType) {
        self.set_block_internal(x, y, z, block, true);
    }

    /// Sets a block during procedural generation (e.g., overflow blocks from trees).
    ///
    /// Unlike `set_block`, this does NOT mark `persistence_dirty`, so the chunk
    /// won't be auto-saved to disk unless the player makes actual modifications.
    /// This prevents newly generated chunks with tree overflow from triggering saves.
    #[inline]
    pub fn set_block_generated(&mut self, x: usize, y: usize, z: usize, block: BlockType) {
        self.set_block_internal(x, y, z, block, false);
    }

    /// Internal implementation for setting blocks.
    /// `mark_persistence` controls whether to set `persistence_dirty`.
    #[inline]
    fn set_block_internal(
        &mut self,
        x: usize,
        y: usize,
        z: usize,
        block: BlockType,
        mark_persistence: bool,
    ) {
        let idx = Self::index(x, y, z);
        let old = self.blocks[idx];
        if old != block {
            // Maintain light block count
            if old.is_light_source() && self.light_block_count > 0 {
                self.light_block_count -= 1;
            }
            if block.is_light_source() {
                self.light_block_count += 1;
            }

            self.blocks[idx] = block;
            self.dirty = true;
            if mark_persistence {
                self.persistence_dirty = true;
            }
            self.metadata_dirty = true;

            // Track which brick is dirty for incremental SVT updates
            // Brick size is 8, chunk has 4x4x4 bricks
            let brick_x = x / 8;
            let brick_y = y / 8;
            let brick_z = z / 8;
            let brick_idx = brick_x + brick_y * 4 + brick_z * 16;
            self.dirty_bricks |= 1u64 << brick_idx;
            self.svt_dirty = true;

            // Clean up model data if block is no longer a Model
            if block != BlockType::Model {
                self.model_data.remove(&idx);
                self.model_metadata_dirty.set(true);
            }
            // Clean up tint data if block is no longer TintedGlass or Crystal
            if block != BlockType::TintedGlass && block != BlockType::Crystal {
                self.tint_data.remove(&idx);
                self.model_metadata_dirty.set(true);
            }
            // Clean up painted data if block is no longer Painted
            if block != BlockType::Painted {
                self.painted_data.remove(&idx);
                self.model_metadata_dirty.set(true);
            }
            // Clean up water data if block is no longer Water
            if block != BlockType::Water {
                self.water_data.remove(&idx);
                self.model_metadata_dirty.set(true);
            }
        } else if block.is_light_source() {
            // No change, keep counts stable
        }
    }

    /// Sets a model block with its metadata at the given local coordinates.
    #[inline]
    pub fn set_model_block(
        &mut self,
        x: usize,
        y: usize,
        z: usize,
        model_id: u8,
        rotation: u8,
        waterlogged: bool,
    ) {
        let idx = Self::index(x, y, z);
        self.blocks[idx] = BlockType::Model;
        self.model_data.insert(
            idx,
            BlockModelData {
                model_id,
                rotation,
                waterlogged,
                custom_data: 0,
            },
        );
        self.dirty = true;
        self.persistence_dirty = true;
        self.metadata_dirty = true;
        self.model_metadata_dirty.set(true);
    }

    /// Sets a model block with custom data (for frames, etc.).
    #[allow(clippy::too_many_arguments)]
    pub fn set_model_block_with_data(
        &mut self,
        x: usize,
        y: usize,
        z: usize,
        model_id: u8,
        rotation: u8,
        waterlogged: bool,
        custom_data: u32,
    ) {
        let idx = Self::index(x, y, z);
        self.blocks[idx] = BlockType::Model;
        self.model_data.insert(
            idx,
            BlockModelData {
                model_id,
                rotation,
                waterlogged,
                custom_data,
            },
        );
        self.dirty = true;
        self.persistence_dirty = true;
        self.metadata_dirty = true;
        self.model_metadata_dirty.set(true);
        self.custom_data_dirty.set(true);
    }

    /// Gets the model data for a block at the given local coordinates.
    /// Returns None if the block is not a Model type.
    #[inline]
    pub fn get_model_data(&self, x: usize, y: usize, z: usize) -> Option<BlockModelData> {
        let idx = Self::index(x, y, z);
        self.model_data.get(&idx).copied()
    }

    /// Sets the custom_data field for an existing model block.
    /// Does nothing if the block is not a Model type.
    pub fn set_model_custom_data(&mut self, x: usize, y: usize, z: usize, custom_data: u32) {
        let idx = Self::index(x, y, z);
        if let Some(data) = self.model_data.get_mut(&idx) {
            data.custom_data = custom_data;
            self.dirty = true;
            self.persistence_dirty = true;
            self.model_metadata_dirty.set(true);
            self.custom_data_dirty.set(true);
        }
    }

    /// Sets the model data for a block at the given local coordinates.
    /// The block should already be of type Model.
    #[inline]
    pub fn set_model_data(&mut self, x: usize, y: usize, z: usize, data: BlockModelData) {
        let idx = Self::index(x, y, z);
        self.model_data.insert(idx, data);
        self.dirty = true;
        self.persistence_dirty = true;
        self.model_metadata_dirty.set(true);
    }

    /// Recomputes frame edge masks from custom_data metadata.
    /// This ensures frames loaded from storage have correct edge masks.
    pub fn recompute_frame_edge_masks(&mut self) {
        use crate::sub_voxel::ModelRegistry;
        use crate::sub_voxel::builtins::frames;

        let mut updates = Vec::new();

        for (&idx, data) in &self.model_data {
            if ModelRegistry::is_frame_model(data.model_id) {
                let (_x, _y, _z) = Self::index_to_coords(idx);
                let custom_data = data.custom_data;

                // Extract frame metadata
                let offset_x = frames::metadata::decode_offset_x(custom_data);
                let offset_y = frames::metadata::decode_offset_y(custom_data);
                let width = frames::metadata::decode_width(custom_data);
                let height = frames::metadata::decode_height(custom_data);
                let facing = frames::metadata::decode_facing(custom_data);

                // Compute edge mask from position in cluster
                let mask_left = offset_x == 0;
                let mask_right = offset_x + 1 == width;
                let mask_bottom = offset_y == 0;
                let mask_top = offset_y + 1 == height;
                let edge_mask: u8 = (mask_left as u8)
                    | ((mask_right as u8) << 1)
                    | ((mask_bottom as u8) << 2)
                    | ((mask_top as u8) << 3);

                // Update rotation with edge mask
                let rotation = (facing & 0x03) | (edge_mask << 3);

                if rotation != data.rotation {
                    updates.push((
                        idx,
                        BlockModelData {
                            model_id: data.model_id,
                            rotation,
                            waterlogged: data.waterlogged,
                            custom_data,
                        },
                    ));
                }
            }
        }

        for (idx, data) in updates {
            self.model_data.insert(idx, data);
            self.model_metadata_dirty.set(true);
        }
    }

    /// Sets a tinted glass block with its color index at the given local coordinates.
    #[inline]
    pub fn set_tinted_glass_block(&mut self, x: usize, y: usize, z: usize, tint_index: u8) {
        let idx = Self::index(x, y, z);
        self.blocks[idx] = BlockType::TintedGlass;
        self.tint_data.insert(idx, tint_index & 0x1F); // Clamp to 0-31
        self.dirty = true;
        self.persistence_dirty = true;
        self.metadata_dirty = true;
        self.model_metadata_dirty.set(true);
    }

    /// Sets a crystal block with its color index at the given local coordinates.
    /// Crystal blocks are emissive and use the tint palette for color variation.
    #[inline]
    pub fn set_crystal_block(&mut self, x: usize, y: usize, z: usize, tint_index: u8) {
        let idx = Self::index(x, y, z);
        let old = self.blocks[idx];
        // Update light block count
        if !old.is_light_source() {
            self.light_block_count += 1;
        }
        self.blocks[idx] = BlockType::Crystal;
        self.tint_data.insert(idx, tint_index & 0x1F); // Clamp to 0-31
        self.dirty = true;
        self.persistence_dirty = true;
        self.metadata_dirty = true;
        self.model_metadata_dirty.set(true);
    }

    /// Sets a painted block with its texture + tint metadata at the given local coordinates.
    /// Uses default multiply blend mode.
    #[inline]
    pub fn set_painted_block(
        &mut self,
        x: usize,
        y: usize,
        z: usize,
        texture_idx: u8,
        tint_idx: u8,
    ) {
        self.set_painted_block_full(x, y, z, texture_idx, tint_idx, 0);
    }

    /// Sets a painted block with full metadata including blend mode.
    #[inline]
    pub fn set_painted_block_full(
        &mut self,
        x: usize,
        y: usize,
        z: usize,
        texture_idx: u8,
        tint_idx: u8,
        blend_mode: u8,
    ) {
        let idx = Self::index(x, y, z);
        self.blocks[idx] = BlockType::Painted;
        self.painted_data
            .insert(idx, BlockPaintData::new(texture_idx, tint_idx, blend_mode));
        self.dirty = true;
        self.persistence_dirty = true;
        self.metadata_dirty = true;
        self.model_metadata_dirty.set(true);
    }

    /// Gets the tint color index for a tinted glass or crystal block at the given local coordinates.
    /// Returns None if the block does not use tint data (TintedGlass or Crystal).
    #[inline]
    pub fn get_tint_index(&self, x: usize, y: usize, z: usize) -> Option<u8> {
        let idx = Self::index(x, y, z);
        self.tint_data.get(&idx).copied()
    }

    /// Gets paint metadata for a painted block at the given local coordinates.
    #[inline]
    pub fn get_paint_data(&self, x: usize, y: usize, z: usize) -> Option<BlockPaintData> {
        let idx = Self::index(x, y, z);
        self.painted_data.get(&idx).copied()
    }

    /// Sets a water block with its type at the given local coordinates.
    #[inline]
    pub fn set_water_block(&mut self, x: usize, y: usize, z: usize, water_type: WaterType) {
        let idx = Self::index(x, y, z);
        self.blocks[idx] = BlockType::Water;
        self.water_data.insert(idx, water_type);
        self.dirty = true;
        self.persistence_dirty = true;
        self.metadata_dirty = true;
        self.model_metadata_dirty.set(true);
    }

    /// Gets the water type for a block at the given local coordinates.
    #[inline]
    pub fn get_water_type(&self, x: usize, y: usize, z: usize) -> Option<WaterType> {
        let idx = Self::index(x, y, z);
        self.water_data.get(&idx).copied()
    }

    /// Returns the number of model blocks in this chunk.
    #[inline]
    pub fn model_count(&self) -> usize {
        self.model_data.len()
    }

    /// Returns true if this chunk may contain non-model light sources.
    #[inline]
    pub fn light_block_count(&self) -> usize {
        self.light_block_count
    }

    /// Iterates over all model block entries (index -> metadata).
    #[inline]
    pub fn model_entries(&self) -> impl Iterator<Item = (&usize, &BlockModelData)> {
        self.model_data.iter()
    }

    /// Iterates over all painted block entries (index -> metadata).
    #[inline]
    pub fn painted_entries(&self) -> impl Iterator<Item = (&usize, &BlockPaintData)> {
        self.painted_data.iter()
    }

    /// Iterates over all tinted glass entries (index -> tint idx).
    #[inline]
    pub fn tinted_entries(&self) -> impl Iterator<Item = (&usize, &u8)> {
        self.tint_data.iter()
    }

    /// Iterates over all blocks with their flat index.
    #[inline]
    pub fn iter_blocks(&self) -> impl Iterator<Item = (usize, BlockType)> + '_ {
        self.blocks.iter().copied().enumerate()
    }

    /// Checks if a block is solid at the given local coordinates.
    #[inline]
    pub fn is_solid(&self, x: usize, y: usize, z: usize) -> bool {
        self.get_block(x, y, z).is_solid()
    }

    /// Converts the chunk to bit-packed format.
    ///
    /// LEGACY: This method is currently unused. The actual GPU acceleration structure
    /// is built using the `svt` module (Sparse Voxel Tree), which generates a
    /// 64-bit brick mask (split into two u32s) per chunk, not this u128 format.
    pub fn to_bit_packed(&self) -> Vec<u128> {
        let packed_size = CHUNK_VOLUME / 128;
        let mut packed = vec![0u128; packed_size];

        for z in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    if self.get_block(x, y, z).is_solid() {
                        // Match the bit layout from voxelize.rs
                        let texel = (x + ((y + (z / 8) * CHUNK_SIZE) / 4) * CHUNK_SIZE) / 4;
                        let bit = (x % 4) * 32 + (y % 4) + (z % 8) * 4;
                        packed[texel] |= 1u128 << bit;
                    }
                }
            }
        }

        packed
    }

    /// Converts the chunk to a format that includes block type information.
    ///
    /// This returns a Vec<u8> with one byte per block, suitable for
    /// uploading to an R8_UINT 3D texture.
    pub fn to_block_data(&self) -> Vec<u8> {
        self.blocks.iter().map(|&b| b as u8).collect()
    }

    /// Returns a zero-copy view of the chunk blocks as raw u8 bytes.
    #[inline]
    pub fn block_bytes(&self) -> &[u8] {
        // SAFETY: BlockType is #[repr(u8)] and blocks is a contiguous array.
        unsafe { slice::from_raw_parts(self.blocks.as_ptr() as *const u8, CHUNK_VOLUME) }
    }

    /// Returns a pooled Vec<u8> containing block bytes, reusing the provided buffer if large enough.
    pub fn write_block_bytes_into(&self, out: &mut Vec<u8>) {
        out.clear();
        if out.capacity() < CHUNK_VOLUME {
            out.reserve(CHUNK_VOLUME - out.capacity());
        }
        // SAFETY: block_bytes returns contiguous u8 slice of CHUNK_VOLUME length.
        out.extend_from_slice(self.block_bytes());
    }

    /// Converts the chunk's model metadata to GPU format.
    ///
    /// Returns a Vec<u8> with 2 bytes per block (RG8 format) suitable for upload.
    pub fn to_model_metadata(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(CHUNK_VOLUME * 2);
        self.write_model_metadata_into(&mut out);
        out
    }

    /// Writes model metadata into provided Vec, reusing its capacity.
    pub fn write_model_metadata_into(&self, out: &mut Vec<u8>) {
        let buf = self.model_metadata_bytes();
        out.clear();
        if out.capacity() < buf.len() {
            out.reserve(buf.len() - out.capacity());
        }
        out.extend_from_slice(&buf);
    }

    /// Returns a cached RG8 view of the model metadata (2 bytes per voxel).
    /// The buffer is rebuilt only when model or tint data changes.
    ///
    /// Layout:
    /// - For Model blocks: R = model_id, G = rotation (bits 0-1) | waterlogged (bit 2)
    /// - For TintedGlass blocks: R = 0, G = tint_index (bits 0-4)
    /// - For Painted blocks: R = texture_idx, G = tint_index (bits 0-4)
    #[inline]
    pub fn model_metadata_bytes(&self) -> Ref<'_, [u8]> {
        if self.model_metadata_dirty.get() {
            {
                let mut buf = self.model_metadata_buf.borrow_mut();
                buf.fill(0);
                // Pack model data
                for (idx, data) in &self.model_data {
                    let offset = idx * 2;
                    buf[offset] = data.model_id;
                    // Pack rotation (bits 0-1), frame edge mask (bits 3-6), and waterlogged (bit 2).
                    // Bit 7 remains unused.
                    let mut packed_meta = data.rotation & 0xFB; // keep custom flag bits, clear bit 2 (waterlogged slot)
                    if data.waterlogged {
                        packed_meta |= 0x04;
                    }
                    buf[offset + 1] = packed_meta;
                }
                // Pack tint data for TintedGlass blocks
                for (idx, &tint_index) in &self.tint_data {
                    let offset = idx * 2;
                    buf[offset] = 0; // R = 0 (no model_id)
                    buf[offset + 1] = tint_index & 0x1F; // G = tint_index (bits 0-4)
                }
                // Pack painted block data (texture_idx in R, tint+blend in G)
                for (idx, data) in &self.painted_data {
                    let offset = idx * 2;
                    buf[offset] = data.texture_idx;
                    buf[offset + 1] = data.packed_tint_blend();
                }
                // Pack water data
                for (idx, &water_type) in &self.water_data {
                    let offset = idx * 2;
                    buf[offset] = 0; // R = 0 (no model_id)
                    buf[offset + 1] = water_type as u8; // G = water type
                }
            }
            self.model_metadata_dirty.set(false);
        }
        Ref::map(self.model_metadata_buf.borrow(), |v| v.as_slice())
    }

    /// Returns the custom data buffer for GPU upload.
    /// Each block uses 4 bytes (u32) for custom data.
    /// For frames: stores picture_id, offset_x, offset_y, width, height, facing.
    #[inline]
    pub fn custom_data_bytes(&self) -> Ref<'_, [u8]> {
        if self.custom_data_dirty.get() {
            {
                let mut buf = self.custom_data_buf.borrow_mut();
                buf.fill(0);
                // Pack custom data from model_data
                for (idx, data) in &self.model_data {
                    let offset = idx * 4;
                    let bytes = data.custom_data.to_le_bytes();
                    buf[offset..offset + 4].copy_from_slice(&bytes);
                }
            }
            self.custom_data_dirty.set(false);
        }
        Ref::map(self.custom_data_buf.borrow(), |v| v.as_slice())
    }

    /// Returns the number of non-air blocks in the chunk.
    pub fn block_count(&self) -> usize {
        self.blocks.iter().filter(|&&b| b != BlockType::Air).count()
    }

    /// Returns an immutable view of the chunk's block storage.
    #[inline]
    pub fn block_slice(&self) -> &[BlockType; CHUNK_VOLUME] {
        &self.blocks
    }

    /// Clones the chunk's block storage into a new boxed array.
    /// Useful for off-thread processing without borrowing the chunk.
    pub fn clone_blocks(&self) -> Box<[BlockType; CHUNK_VOLUME]> {
        self.blocks.clone()
    }

    /// Returns true if the chunk is completely empty (all air).
    /// Uses cached value if available, otherwise recomputes.
    pub fn is_empty(&self) -> bool {
        if self.metadata_dirty {
            // Recompute if dirty (but don't cache in immutable method)
            self.blocks.iter().all(|&b| b == BlockType::Air)
        } else {
            self.cached_is_empty
        }
    }

    /// Returns true if the chunk is completely solid (no air/transparent blocks).
    /// Uses cached value if available, otherwise recomputes.
    pub fn is_fully_solid(&self) -> bool {
        if self.metadata_dirty {
            self.blocks.iter().all(|&b| b.is_solid())
        } else {
            self.cached_is_fully_solid
        }
    }

    /// Updates the cached metadata (is_empty, is_fully_solid).
    /// Call this after bulk modifications to avoid repeated recalculation.
    pub fn update_metadata(&mut self) {
        if self.metadata_dirty {
            self.cached_is_empty = self.blocks.iter().all(|&b| b == BlockType::Air);
            self.cached_is_fully_solid = self.blocks.iter().all(|&b| b.is_solid());
            self.metadata_dirty = false;
        }
    }

    /// Returns the cached is_empty flag directly (for GPU upload).
    /// Call update_metadata() first to ensure accuracy.
    #[inline]
    pub fn cached_is_empty(&self) -> bool {
        self.cached_is_empty
    }

    /// Returns the cached is_fully_solid flag directly (for GPU upload).
    /// Call update_metadata() first to ensure accuracy.
    #[inline]
    pub fn cached_is_fully_solid(&self) -> bool {
        self.cached_is_fully_solid
    }

    /// Marks the chunk as needing GPU re-upload.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Marks the chunk as synced with GPU.
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Returns the cached SVT brick mask (64-bit mask for 4x4x4 bricks).
    /// Call `update_svt()` first if you need current data.
    #[inline]
    pub fn cached_brick_mask(&self) -> u64 {
        self.cached_brick_mask
    }

    /// Returns a reference to the cached SVT brick distances (64 bytes).
    /// Call `update_svt()` first if you need current data.
    #[inline]
    pub fn cached_brick_distances(&self) -> &[u8; 64] {
        &self.cached_brick_distances
    }

    /// Returns true if the SVT cache needs to be updated.
    #[inline]
    pub fn is_svt_dirty(&self) -> bool {
        self.svt_dirty
    }

    /// Updates the cached SVT data (brick mask and distances).
    /// Only recomputes bricks that have changed since the last update.
    pub fn update_svt(&mut self) {
        if !self.svt_dirty {
            return;
        }

        // If all 64 bricks are dirty (e.g., newly loaded chunk), do a full rebuild
        if self.dirty_bricks == u64::MAX {
            self.rebuild_svt_full();
        } else {
            self.update_svt_incremental();
        }

        self.dirty_bricks = 0;
        self.svt_dirty = false;
    }

    /// Full SVT rebuild (used for newly loaded chunks or when all bricks dirty).
    fn rebuild_svt_full(&mut self) {
        let mut brick_mask = 0u64;
        let mut brick_has_solid = [false; 64];

        // Check each of the 64 bricks
        for bz in 0..4 {
            for by in 0..4 {
                for bx in 0..4 {
                    let brick_idx = bx + by * 4 + bz * 16;
                    let mut has_solid = false;

                    // Check all 512 voxels in this brick
                    'brick: for vz in 0..8 {
                        for vy in 0..8 {
                            for vx in 0..8 {
                                let world_x = bx * 8 + vx;
                                let world_y = by * 8 + vy;
                                let world_z = bz * 8 + vz;
                                let block = self.get_block(world_x, world_y, world_z);
                                if block != BlockType::Air {
                                    has_solid = true;
                                    break 'brick;
                                }
                            }
                        }
                    }

                    if has_solid {
                        brick_mask |= 1u64 << brick_idx;
                        brick_has_solid[brick_idx] = true;
                    }
                }
            }
        }

        self.cached_brick_mask = brick_mask;
        self.cached_brick_distances = Self::calculate_brick_distances(&brick_has_solid);
    }

    /// Incremental SVT update - only recomputes dirty bricks.
    fn update_svt_incremental(&mut self) {
        let dirty = self.dirty_bricks;
        let mut brick_mask = self.cached_brick_mask;
        let mut brick_has_solid = [false; 64];

        // Initialize brick_has_solid from current mask
        for (i, solid) in brick_has_solid.iter_mut().enumerate() {
            *solid = (brick_mask & (1u64 << i)) != 0;
        }

        // Update only dirty bricks
        for (brick_idx, solid) in brick_has_solid.iter_mut().enumerate() {
            if (dirty & (1u64 << brick_idx)) == 0 {
                continue;
            }

            let bx = brick_idx % 4;
            let by = (brick_idx / 4) % 4;
            let bz = brick_idx / 16;
            let mut has_solid = false;

            // Check all 512 voxels in this brick
            'brick: for vz in 0..8 {
                for vy in 0..8 {
                    for vx in 0..8 {
                        let world_x = bx * 8 + vx;
                        let world_y = by * 8 + vy;
                        let world_z = bz * 8 + vz;
                        let block = self.get_block(world_x, world_y, world_z);
                        if block != BlockType::Air {
                            has_solid = true;
                            break 'brick;
                        }
                    }
                }
            }

            // Update mask bit
            if has_solid {
                brick_mask |= 1u64 << brick_idx;
            } else {
                brick_mask &= !(1u64 << brick_idx);
            }
            *solid = has_solid;
        }

        self.cached_brick_mask = brick_mask;
        // Recalculate distances (unfortunately still needs all bricks for propagation)
        self.cached_brick_distances = Self::calculate_brick_distances(&brick_has_solid);
    }

    /// Calculates Manhattan distance from each brick to nearest solid brick.
    fn calculate_brick_distances(has_solid: &[bool; 64]) -> [u8; 64] {
        let mut distances = [255u8; 64];

        // Initialize solid bricks with distance 0
        for (idx, &solid) in has_solid.iter().enumerate() {
            if solid {
                distances[idx] = 0;
            }
        }

        // Propagate distances (simple 3D BFS-like propagation)
        for _pass in 0..4 {
            let mut changed = false;
            for bz in 0..4 {
                for by in 0..4 {
                    for bx in 0..4 {
                        let idx = bx + by * 4 + bz * 16;
                        if distances[idx] == 0 {
                            continue;
                        }

                        let mut min_neighbor = 255u8;

                        // Check 6-connected neighbors
                        if bx > 0 {
                            min_neighbor = min_neighbor.min(distances[(bx - 1) + by * 4 + bz * 16]);
                        }
                        if bx < 3 {
                            min_neighbor = min_neighbor.min(distances[(bx + 1) + by * 4 + bz * 16]);
                        }
                        if by > 0 {
                            min_neighbor = min_neighbor.min(distances[bx + (by - 1) * 4 + bz * 16]);
                        }
                        if by < 3 {
                            min_neighbor = min_neighbor.min(distances[bx + (by + 1) * 4 + bz * 16]);
                        }
                        if bz > 0 {
                            min_neighbor = min_neighbor.min(distances[bx + by * 4 + (bz - 1) * 16]);
                        }
                        if bz < 3 {
                            min_neighbor = min_neighbor.min(distances[bx + by * 4 + (bz + 1) * 16]);
                        }

                        let new_dist = min_neighbor.saturating_add(1);
                        if new_dist < distances[idx] {
                            distances[idx] = new_dist;
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }

        distances
    }

    /// Marks the entire SVT as dirty (e.g., for newly loaded chunks).
    pub fn mark_svt_fully_dirty(&mut self) {
        self.dirty_bricks = u64::MAX;
        self.svt_dirty = true;
    }

    /// Clears SVT dirty state (e.g., after upload with external SVT calculation).
    pub fn clear_svt_dirty(&mut self) {
        self.dirty_bricks = 0;
        self.svt_dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_new() {
        let chunk = Chunk::new();
        assert!(chunk.is_empty());
        assert!(chunk.dirty);
    }

    #[test]
    fn test_chunk_set_get() {
        let mut chunk = Chunk::new();
        chunk.set_block(5, 10, 15, BlockType::Stone);
        assert_eq!(chunk.get_block(5, 10, 15), BlockType::Stone);
        assert_eq!(chunk.get_block(0, 0, 0), BlockType::Air);
    }

    #[test]
    fn test_chunk_bit_packed() {
        let mut chunk = Chunk::new();
        chunk.set_block(0, 0, 0, BlockType::Stone);
        chunk.set_block(1, 0, 0, BlockType::Dirt);

        let packed = chunk.to_bit_packed();
        assert!(!packed.is_empty());

        // First two bits should be set
        assert!(packed[0] & 1 != 0); // (0,0,0)
        assert!(packed[0] & (1 << 32) != 0); // (1,0,0) - x % 4 = 1, so bit 32
    }

    #[test]
    fn test_block_count() {
        let mut chunk = Chunk::new();
        assert_eq!(chunk.block_count(), 0);

        chunk.set_block(0, 0, 0, BlockType::Stone);
        chunk.set_block(1, 1, 1, BlockType::Dirt);
        assert_eq!(chunk.block_count(), 2);
    }
}

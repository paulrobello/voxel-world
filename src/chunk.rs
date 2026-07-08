#![allow(dead_code)]

//! Chunk data structure for voxel storage.
//!
//! Each chunk is a 32³ grid of blocks. Blocks are stored as u8 values
//! where 0 = air and other values represent different block types.
//!
//! Blocks of type `Model` use sparse metadata storage to associate
//! a model_id and rotation with each model block.

use std::cell::{Cell, Ref, RefCell};
use std::collections::HashMap;

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
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    bytemuck::NoUninit,
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

// SEC-017: Compile-time assertion that BlockType fits in a single byte.
// The chunk storage, GPU transfer, and network protocol all rely on this
// invariant; a silent size change would corrupt data silently at runtime.
const _: () = assert!(
    std::mem::size_of::<BlockType>() == 1,
    "BlockType must be exactly 1 byte (u8); adding variants beyond 255 breaks chunk storage"
);

/// Number of [`BlockType`] variants and the inclusive upper bound of its ID range.
///
/// `BlockType` round-trips through `u8` (via `as u8` / [`From<u8>`]) for chunk
/// storage, GPU upload, the network protocol, and world saves. The IDs are
/// contiguous `0..=NUM_BLOCK_TYPES_ID_MAX` (`Air`=0 .. `BirchLeaves`=46). Adding
/// a variant WITHOUT updating [`From<u8>`], the per-variant tables in `impl
/// BlockType` (`is_log`, `is_leaves`, `is_tree_part`, `NAME_TABLE`,
/// `break_time`, `color`, …), and the `BLOCK_*` GLSL defines auto-generated by
/// `build.rs` silently breaks block persistence — the new variant decodes as
/// `Air`. The compile-time checks below make that a build error instead.
pub const NUM_BLOCK_TYPES: usize = 47;
/// Inclusive maximum discriminant of [`BlockType`] (mirrors `BirchLeaves = 46`).
pub const NUM_BLOCK_TYPES_ID_MAX: u8 = 46;

const _: () = assert!(
    NUM_BLOCK_TYPES as u8 == NUM_BLOCK_TYPES_ID_MAX + 1,
    "BlockType ID range is no longer contiguous 0..=46 — update From<u8> and tables"
);

/// Compile-time exhaustiveness guard for [`BlockType`].
///
/// Never called at runtime; it exists solely so that adding a new variant makes
/// this match non-exhaustive and fails the build. When you add a variant,
/// extend this list AND every per-variant table referenced in the doc on
/// [`NUM_BLOCK_TYPES`]. (`std::mem::variant_count` would replace this once it
/// stabilizes — see issue rust-lang/rust#73662.)
#[allow(dead_code)]
fn _block_type_completeness_guard(b: BlockType) -> u8 {
    match b {
        BlockType::Air => 0,
        BlockType::Stone => 1,
        BlockType::Dirt => 2,
        BlockType::Grass => 3,
        BlockType::Planks => 4,
        BlockType::Leaves => 5,
        BlockType::Sand => 6,
        BlockType::Gravel => 7,
        BlockType::Water => 8,
        BlockType::Glass => 9,
        BlockType::Log => 10,
        BlockType::Model => 11,
        BlockType::Brick => 12,
        BlockType::Snow => 13,
        BlockType::Cobblestone => 14,
        BlockType::Iron => 15,
        BlockType::Bedrock => 16,
        BlockType::TintedGlass => 17,
        BlockType::Painted => 18,
        BlockType::Lava => 19,
        BlockType::GlowStone => 20,
        BlockType::GlowMushroom => 21,
        BlockType::Crystal => 22,
        BlockType::PineLog => 23,
        BlockType::WillowLog => 24,
        BlockType::PineLeaves => 25,
        BlockType::WillowLeaves => 26,
        BlockType::Ice => 27,
        BlockType::Mud => 28,
        BlockType::Sandstone => 29,
        BlockType::Cactus => 30,
        BlockType::DecorativeStone => 31,
        BlockType::Concrete => 32,
        BlockType::Deepslate => 33,
        BlockType::Moss => 34,
        BlockType::MossyCobblestone => 35,
        BlockType::Clay => 36,
        BlockType::Dripstone => 37,
        BlockType::Calcite => 38,
        BlockType::Terracotta => 39,
        BlockType::PackedIce => 40,
        BlockType::Podzol => 41,
        BlockType::Mycelium => 42,
        BlockType::CoarseDirt => 43,
        BlockType::RootedDirt => 44,
        BlockType::BirchLog => 45,
        BlockType::BirchLeaves => 46,
    }
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
    /// Converts a u8 to a WaterType. Prefer `WaterType::from(v)` for new code.
    pub fn from_u8(v: u8) -> Self {
        Self::from(v)
    }
}

impl From<u8> for WaterType {
    fn from(v: u8) -> Self {
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

/// Per-block metadata stored sparsely in the chunk's unified metadata map.
///
/// Each block index has at most one metadata variant, determined by the block type:
/// - `Model` blocks carry `BlockMetadata::Model`
/// - `TintedGlass` / `Crystal` blocks carry `BlockMetadata::Tint`
/// - `Painted` blocks carry `BlockMetadata::Painted`
/// - `Water` blocks carry `BlockMetadata::Water`
///
/// Replaces the four separate `HashMap`s that previously stored each variant in its own
/// table, saving ~3 allocator entries per chunk and simplifying the cleanup/iteration paths.
#[derive(Debug, Clone, Copy)]
pub enum BlockMetadata {
    Model(BlockModelData),
    Tint(u8),
    Painted(BlockPaintData),
    Water(WaterType),
}

impl BlockMetadata {
    /// Returns true if this metadata variant is valid for the given block type.
    ///
    /// Used by `set_block_internal` to decide whether to keep or discard existing metadata
    /// when a block's type changes.
    #[inline]
    pub fn matches_block_type(&self, block: BlockType) -> bool {
        match self {
            BlockMetadata::Model(_) => block == BlockType::Model,
            BlockMetadata::Tint(_) => {
                block == BlockType::TintedGlass || block == BlockType::Crystal
            }
            BlockMetadata::Painted(_) => block == BlockType::Painted,
            BlockMetadata::Water(_) => block == BlockType::Water,
        }
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
    ///
    /// This is the legacy catch-all predicate. Call sites with a specific role should
    /// prefer one of `blocks_movement`, `stops_fluid`, `provides_support`,
    /// `connects_to_fences`, or `is_buildable_ground` so the intent is explicit and
    /// future block additions can diverge per-role without touching every call site.
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

    /// Returns true if this block stops entity movement (player, falling block,
    /// particle) or a raycast used for placement/breaking.
    ///
    /// Role: collision. Fluids, air, glass, tinted glass, ice, and Model blocks
    /// are passable; everything else stops movement. Truth table currently
    /// matches `is_solid` (the audit's "glass should block movement" item is a
    /// separate behavior change outside PHY-005's scope).
    #[inline]
    pub fn blocks_movement(self) -> bool {
        self.is_solid()
    }

    /// Returns true if this block stops water/lava from flowing into its cell.
    ///
    /// Role: fluid-simulation barrier. Fluids flow through air, other fluids,
    /// glass, tinted glass, ice, and Model blocks; everything else blocks flow.
    /// Truth table currently matches `is_solid`.
    #[inline]
    pub fn stops_fluid(self) -> bool {
        self.is_solid()
    }

    /// Returns true if this block can hold up an adjacent block above it.
    ///
    /// Role: structural support — used by Model ground-support checks
    /// (`block_update`) and tree ground-support checks (`tree_logic`).
    /// Fluids, air, glass, tinted glass, ice, and Model blocks do not provide
    /// support; solids (including logs and leaves) do. Truth table currently
    /// matches `is_solid`.
    #[inline]
    pub fn provides_support(self) -> bool {
        self.is_solid()
    }

    /// Returns true if this block causes a fence, wall, or glass pane to render
    /// a connection post against it.
    ///
    /// Role: fence/pane connection. Glass and tinted glass do NOT connect via
    /// this predicate; `is_window_connectable` adds them separately for panes.
    /// Truth table currently matches `is_solid`.
    #[inline]
    pub fn connects_to_fences(self) -> bool {
        self.is_solid()
    }

    /// Returns true if this block counts as ground that a tree can root into
    /// during world generation.
    ///
    /// Role: tree-generation ground detection. Snow-biome trees additionally
    /// accept Ice, handled at the call site via an explicit `!= Ice` check.
    /// Truth table currently matches `is_solid`.
    #[inline]
    pub fn is_buildable_ground(self) -> bool {
        self.is_solid()
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

    /// Returns true if this block is a leaf/canopy block (any tree species).
    ///
    /// Covers every canopy variant so call sites don't need to be updated when
    /// a new tree species is added. `find_leaf_cluster_and_check_log` relies on
    /// this to traverse the full canopy when a log breaks.
    #[inline]
    pub fn is_leaves(self) -> bool {
        matches!(
            self,
            BlockType::Leaves
                | BlockType::PineLeaves
                | BlockType::WillowLeaves
                | BlockType::BirchLeaves
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
            _ => {
                log::warn!("Unknown BlockType value: {}; falling back to Air", value);
                BlockType::Air
            }
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

    /// Unified sparse storage for per-block metadata.
    ///
    /// Key: block index (0..CHUNK_VOLUME). Value: the `BlockMetadata` variant that matches
    /// the block at that index (Model / Tint / Painted / Water). Each block has at most
    /// one entry. Replaces four separate HashMaps — one allocation instead of four.
    metadata: HashMap<usize, BlockMetadata>,

    /// Reusable RG8 buffer for model metadata uploads (len = CHUNK_VOLUME * 2).
    model_metadata_buf: RefCell<Vec<u8>>,
    /// Whether the cached model metadata buffer needs recomputing.
    model_metadata_dirty: Cell<bool>,
    /// Block indices written in last metadata rebuild for partial zeroing.
    metadata_written_indices: RefCell<Vec<usize>>,
    /// Whether first metadata rebuild has occurred (forces full zero on first pass).
    metadata_first_rebuild: Cell<bool>,

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

    /// Monotonic mutation counter. Bumped on every call to [`mark_mutated`],
    /// which all public `set_*` / `remove_*` methods funnel through. External
    /// memoization layers (e.g. chunk compression cache in multiplayer
    /// streaming) can observe this to invalidate cached bytes without having
    /// to trust `persistence_dirty`, which gets cleared by saves even though
    /// the chunk state they represent is still the same bytes.
    mutation_epoch: u64,

    /// Cached: true if all blocks are air (for ray skip optimization).
    cached_is_empty: bool,

    /// Cached: true if all blocks are solid (for ray skip optimization).
    cached_is_fully_solid: bool,

    /// Whether cached_is_empty/cached_is_fully_solid need recalculation.
    metadata_dirty: bool,
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
            metadata: HashMap::with_capacity(32),
            model_metadata_buf: RefCell::new(vec![0u8; CHUNK_VOLUME * 2]),
            model_metadata_dirty: Cell::new(false),
            metadata_written_indices: RefCell::new(Vec::new()),
            metadata_first_rebuild: Cell::new(true),
            custom_data_buf: RefCell::new(vec![0u8; CHUNK_VOLUME * 4]),
            custom_data_dirty: Cell::new(false),
            light_block_count: 0,
            dirty: true,
            persistence_dirty: true,
            mutation_epoch: 0,
            cached_is_empty: true,
            cached_is_fully_solid: false,
            metadata_dirty: false,
        }
    }

    /// Marks the chunk as mutated. Funnel for every `set_*` / `remove_*` call
    /// — sets `persistence_dirty = true` and bumps the `mutation_epoch`
    /// counter so external memoization (e.g. compressed-chunk cache in
    /// multiplayer streaming) knows its cached bytes are stale.
    #[inline]
    pub fn mark_mutated(&mut self) {
        self.persistence_dirty = true;
        self.mutation_epoch = self.mutation_epoch.wrapping_add(1);
    }

    /// Returns the current mutation epoch. Callers cache this alongside any
    /// derived data (e.g. compressed bytes) and re-derive when it diverges.
    #[inline]
    pub fn mutation_epoch(&self) -> u64 {
        self.mutation_epoch
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
        Self {
            blocks: Box::new([block_type; CHUNK_VOLUME]),
            metadata: HashMap::with_capacity(32),
            model_metadata_buf: RefCell::new(vec![0u8; CHUNK_VOLUME * 2]),
            model_metadata_dirty: Cell::new(false),
            metadata_written_indices: RefCell::new(Vec::new()),
            metadata_first_rebuild: Cell::new(true),
            custom_data_buf: RefCell::new(vec![0u8; CHUNK_VOLUME * 4]),
            custom_data_dirty: Cell::new(false),
            light_block_count,
            dirty: true,
            persistence_dirty: true,
            mutation_epoch: 0,
            cached_is_empty: is_empty,
            cached_is_fully_solid: is_solid,
            metadata_dirty: false,
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

        // Merge four input HashMaps into the unified metadata store.
        let mut metadata: HashMap<usize, BlockMetadata> = HashMap::with_capacity(
            model_data.len() + tint_data.len() + painted_data.len() + water_data.len(),
        );
        metadata.extend(
            model_data
                .into_iter()
                .map(|(k, v)| (k, BlockMetadata::Model(v))),
        );
        metadata.extend(
            tint_data
                .into_iter()
                .map(|(k, v)| (k, BlockMetadata::Tint(v))),
        );
        metadata.extend(
            painted_data
                .into_iter()
                .map(|(k, v)| (k, BlockMetadata::Painted(v))),
        );
        metadata.extend(
            water_data
                .into_iter()
                .map(|(k, v)| (k, BlockMetadata::Water(v))),
        );

        Self {
            blocks,
            metadata,
            model_metadata_buf: RefCell::new(vec![0u8; CHUNK_VOLUME * 2]),
            model_metadata_dirty: Cell::new(true), // Need to compute on first request
            metadata_written_indices: RefCell::new(Vec::new()),
            metadata_first_rebuild: Cell::new(true),
            custom_data_buf: RefCell::new(vec![0u8; CHUNK_VOLUME * 4]),
            custom_data_dirty: Cell::new(true),
            light_block_count,
            dirty: true,
            persistence_dirty: false, // Network chunks are not locally modified
            mutation_epoch: 0,
            cached_is_empty: is_empty,
            cached_is_fully_solid: is_solid,
            metadata_dirty: false,
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
            // Always bump mutation_epoch so external memoization (e.g. the
            // compressed-chunk cache in multiplayer streaming) invalidates on
            // any block change. Only flip persistence_dirty when the caller
            // asked for it — generation paths skip persistence to avoid
            // re-saving freshly-loaded world tiles.
            self.mutation_epoch = self.mutation_epoch.wrapping_add(1);
            if mark_persistence {
                self.persistence_dirty = true;
            }
            self.metadata_dirty = true;

            // Drop any metadata whose variant no longer matches the new block type.
            // Each block has at most one metadata entry, so a single lookup + variant
            // check replaces four separate HashMap removals.
            let drop_metadata = self
                .metadata
                .get(&idx)
                .is_some_and(|m| !m.matches_block_type(block));
            if drop_metadata {
                self.metadata.remove(&idx);
                self.model_metadata_dirty.set(true);
            }
        } else if block.is_light_source() {
            // No change, keep counts stable
        }
    }

    /// Single funnel for every typed metadata setter (QA-002).
    ///
    /// Replaces the block at (`x`,`y`,`z`) with `block` and attaches `metadata`
    /// to it. Routing every setter through here — instead of each one writing
    /// `self.blocks[idx]` directly — keeps `light_block_count` and
    /// `mutation_epoch` consistent regardless of which setter the caller used.
    ///
    /// The block-type change goes through [`set_block_internal`], which compares
    /// the old vs new block and adjusts `light_block_count` (decrement if the
    /// old block was emissive, increment if the new one is). This is what fixes
    /// the drift where overwriting a `GlowStone` with a `Crystal` via
    /// `set_crystal_block` used to leave the count inflated, forcing
    /// `collect_torch_lights` into a permanent full-chunk scan.
    ///
    /// `set_block_internal` early-returns when `old == block`, so for the
    /// same-block case (e.g. re-painting an existing painted block) the dirty
    /// flags and `mutation_epoch` bump are applied here — the metadata bytes
    /// genuinely changed and downstream caches must invalidate.
    #[inline]
    fn set_block_with_metadata(
        &mut self,
        x: usize,
        y: usize,
        z: usize,
        block: BlockType,
        metadata: BlockMetadata,
    ) {
        let idx = Self::index(x, y, z);
        let unchanged = self.blocks[idx] == block;
        self.set_block_internal(x, y, z, block, true);
        self.metadata.insert(idx, metadata);
        self.model_metadata_dirty.set(true);
        if unchanged {
            self.dirty = true;
            self.metadata_dirty = true;
            self.mark_mutated();
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
        self.set_block_with_metadata(
            x,
            y,
            z,
            BlockType::Model,
            BlockMetadata::Model(BlockModelData {
                model_id,
                rotation,
                waterlogged,
                custom_data: 0,
            }),
        );
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
        self.set_block_with_metadata(
            x,
            y,
            z,
            BlockType::Model,
            BlockMetadata::Model(BlockModelData {
                model_id,
                rotation,
                waterlogged,
                custom_data,
            }),
        );
        self.custom_data_dirty.set(true);
    }

    /// Gets the model data for a block at the given local coordinates.
    /// Returns None if the block is not a Model type.
    #[inline]
    pub fn get_model_data(&self, x: usize, y: usize, z: usize) -> Option<BlockModelData> {
        let idx = Self::index(x, y, z);
        match self.metadata.get(&idx) {
            Some(BlockMetadata::Model(data)) => Some(*data),
            _ => None,
        }
    }

    /// Sets the custom_data field for an existing model block.
    /// Does nothing if the block is not a Model type.
    pub fn set_model_custom_data(&mut self, x: usize, y: usize, z: usize, custom_data: u32) {
        let idx = Self::index(x, y, z);
        if let Some(BlockMetadata::Model(data)) = self.metadata.get_mut(&idx) {
            data.custom_data = custom_data;
            self.dirty = true;
            self.mark_mutated();
            self.model_metadata_dirty.set(true);
            self.custom_data_dirty.set(true);
        }
    }

    /// Sets the model data for a block at the given local coordinates.
    /// The block should already be of type Model.
    #[inline]
    pub fn set_model_data(&mut self, x: usize, y: usize, z: usize, data: BlockModelData) {
        let idx = Self::index(x, y, z);
        self.metadata.insert(idx, BlockMetadata::Model(data));
        self.dirty = true;
        self.mark_mutated();
        self.model_metadata_dirty.set(true);
    }

    /// Recomputes frame edge masks from custom_data metadata.
    /// This ensures frames loaded from storage have correct edge masks.
    pub fn recompute_frame_edge_masks(&mut self) {
        use crate::sub_voxel::ModelRegistry;
        use crate::sub_voxel::builtins::frames;

        let mut updates = Vec::new();

        for (&idx, meta) in &self.metadata {
            let BlockMetadata::Model(data) = meta else {
                continue;
            };
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
            self.metadata.insert(idx, BlockMetadata::Model(data));
            self.model_metadata_dirty.set(true);
        }
    }

    /// Sets a tinted glass block with its color index at the given local coordinates.
    #[inline]
    pub fn set_tinted_glass_block(&mut self, x: usize, y: usize, z: usize, tint_index: u8) {
        self.set_block_with_metadata(
            x,
            y,
            z,
            BlockType::TintedGlass,
            BlockMetadata::Tint(tint_index & 0x1F), // Clamp to 0-31
        );
    }

    /// Sets a crystal block with its color index at the given local coordinates.
    /// Crystal blocks are emissive and use the tint palette for color variation.
    #[inline]
    pub fn set_crystal_block(&mut self, x: usize, y: usize, z: usize, tint_index: u8) {
        self.set_block_with_metadata(
            x,
            y,
            z,
            BlockType::Crystal,
            BlockMetadata::Tint(tint_index & 0x1F), // Clamp to 0-31
        );
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
        self.set_block_with_metadata(
            x,
            y,
            z,
            BlockType::Painted,
            BlockMetadata::Painted(BlockPaintData::new(texture_idx, tint_idx, blend_mode)),
        );
    }

    /// Gets the tint color index for a tinted glass or crystal block at the given local coordinates.
    /// Returns None if the block does not use tint data (TintedGlass or Crystal).
    #[inline]
    pub fn get_tint_index(&self, x: usize, y: usize, z: usize) -> Option<u8> {
        let idx = Self::index(x, y, z);
        match self.metadata.get(&idx) {
            Some(BlockMetadata::Tint(t)) => Some(*t),
            _ => None,
        }
    }

    /// Gets paint metadata for a painted block at the given local coordinates.
    #[inline]
    pub fn get_paint_data(&self, x: usize, y: usize, z: usize) -> Option<BlockPaintData> {
        let idx = Self::index(x, y, z);
        match self.metadata.get(&idx) {
            Some(BlockMetadata::Painted(p)) => Some(*p),
            _ => None,
        }
    }

    /// Sets a water block with its type at the given local coordinates.
    #[inline]
    pub fn set_water_block(&mut self, x: usize, y: usize, z: usize, water_type: WaterType) {
        self.set_block_with_metadata(x, y, z, BlockType::Water, BlockMetadata::Water(water_type));
    }

    /// Gets the water type for a block at the given local coordinates.
    #[inline]
    pub fn get_water_type(&self, x: usize, y: usize, z: usize) -> Option<WaterType> {
        let idx = Self::index(x, y, z);
        match self.metadata.get(&idx) {
            Some(BlockMetadata::Water(w)) => Some(*w),
            _ => None,
        }
    }

    /// Returns the number of model blocks in this chunk.
    #[inline]
    pub fn model_count(&self) -> usize {
        self.metadata
            .values()
            .filter(|m| matches!(m, BlockMetadata::Model(_)))
            .count()
    }

    /// Returns true if this chunk may contain non-model light sources.
    #[inline]
    pub fn light_block_count(&self) -> usize {
        self.light_block_count
    }

    /// Iterates over all model block entries (index -> metadata).
    #[inline]
    pub fn model_entries(&self) -> impl Iterator<Item = (&usize, &BlockModelData)> {
        self.metadata.iter().filter_map(|(idx, m)| match m {
            BlockMetadata::Model(data) => Some((idx, data)),
            _ => None,
        })
    }

    /// Iterates over all painted block entries (index -> metadata).
    #[inline]
    pub fn painted_entries(&self) -> impl Iterator<Item = (&usize, &BlockPaintData)> {
        self.metadata.iter().filter_map(|(idx, m)| match m {
            BlockMetadata::Painted(data) => Some((idx, data)),
            _ => None,
        })
    }

    /// Iterates over all tinted glass entries (index -> tint idx).
    #[inline]
    pub fn tinted_entries(&self) -> impl Iterator<Item = (&usize, &u8)> {
        self.metadata.iter().filter_map(|(idx, m)| match m {
            BlockMetadata::Tint(t) => Some((idx, t)),
            _ => None,
        })
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

    /// Converts the chunk to a format that includes block type information.
    ///
    /// This returns a Vec<u8> with one byte per block, suitable for
    /// uploading to an R8_UINT 3D texture.
    pub fn to_block_data(&self) -> Vec<u8> {
        self.blocks.iter().map(|&b| b as u8).collect()
    }

    /// Returns a zero-copy view of the chunk blocks as raw u8 bytes.
    ///
    /// Uses `bytemuck::cast_slice` which is safe because `BlockType` is `#[repr(u8)]`
    /// and derives `bytemuck::NoUninit`, guaranteeing no uninitialized bytes.
    #[inline]
    pub fn block_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(self.blocks.as_ref())
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
                let mut prev_indices = self.metadata_written_indices.borrow_mut();

                if self.metadata_first_rebuild.get() {
                    // First rebuild: zero the entire buffer to ensure clean state
                    buf.fill(0);
                    self.metadata_first_rebuild.set(false);
                } else {
                    // Subsequent rebuilds: only zero previously-written entries
                    for &idx in prev_indices.iter() {
                        let offset = idx * 2;
                        buf[offset] = 0;
                        buf[offset + 1] = 0;
                    }
                }

                // Collect new written indices and pack data
                prev_indices.clear();

                // Pack all metadata variants into the RG8 buffer.
                for (idx, meta) in &self.metadata {
                    let offset = idx * 2;
                    match meta {
                        BlockMetadata::Model(data) => {
                            buf[offset] = data.model_id;
                            // Pack rotation (bits 0-1), frame edge mask (bits 3-6),
                            // and waterlogged (bit 2). Bit 7 remains unused.
                            let mut packed_meta = data.rotation & 0xFB; // preserve custom flag bits, clear bit 2
                            if data.waterlogged {
                                packed_meta |= 0x04;
                            }
                            buf[offset + 1] = packed_meta;
                        }
                        BlockMetadata::Tint(tint_index) => {
                            buf[offset] = 0; // R = 0 (no model_id)
                            buf[offset + 1] = tint_index & 0x1F; // G = tint_index (bits 0-4)
                        }
                        BlockMetadata::Painted(data) => {
                            buf[offset] = data.texture_idx;
                            buf[offset + 1] = data.packed_tint_blend();
                        }
                        BlockMetadata::Water(water_type) => {
                            buf[offset] = 0; // R = 0 (no model_id)
                            buf[offset + 1] = *water_type as u8; // G = water type
                        }
                    }
                    prev_indices.push(*idx);
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
                // Pack custom data from model-variant metadata entries.
                for (idx, meta) in &self.metadata {
                    if let BlockMetadata::Model(data) = meta {
                        let offset = idx * 4;
                        let bytes = data.custom_data.to_le_bytes();
                        buf[offset..offset + 4].copy_from_slice(&bytes);
                    }
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
    fn test_mutation_epoch_advances_on_set_block() {
        let mut chunk = Chunk::new();
        let e0 = chunk.mutation_epoch();
        chunk.set_block(1, 2, 3, BlockType::Stone);
        let e1 = chunk.mutation_epoch();
        assert_ne!(e0, e1, "set_block must advance epoch");

        // set_block is a no-op when the new value equals the old — epoch
        // should NOT advance in that case so external caches aren't
        // invalidated by meaningless writes.
        chunk.set_block(1, 2, 3, BlockType::Stone);
        assert_eq!(chunk.mutation_epoch(), e1);

        // A real change advances it again.
        chunk.set_block(1, 2, 3, BlockType::Dirt);
        assert_ne!(chunk.mutation_epoch(), e1);
    }

    #[test]
    fn test_mutation_epoch_advances_on_set_block_generated() {
        // set_block_generated skips persistence_dirty but MUST still bump
        // mutation_epoch so the multiplayer compression cache invalidates
        // on generated terrain changes.
        let mut chunk = Chunk::new();
        let e0 = chunk.mutation_epoch();
        chunk.set_block_generated(0, 0, 0, BlockType::Stone);
        assert_ne!(chunk.mutation_epoch(), e0);
    }

    #[test]
    fn test_chunk_set_get() {
        let mut chunk = Chunk::new();
        chunk.set_block(5, 10, 15, BlockType::Stone);
        assert_eq!(chunk.get_block(5, 10, 15), BlockType::Stone);
        assert_eq!(chunk.get_block(0, 0, 0), BlockType::Air);
    }

    #[test]
    fn test_block_count() {
        let mut chunk = Chunk::new();
        assert_eq!(chunk.block_count(), 0);

        chunk.set_block(0, 0, 0, BlockType::Stone);
        chunk.set_block(1, 1, 1, BlockType::Dirt);
        assert_eq!(chunk.block_count(), 2);
    }

    #[test]
    fn test_is_leaves_covers_all_canopy_variants() {
        // BUG-001 regression: every leaf/canopy species must be recognized so
        // `find_leaf_cluster_and_check_log` traverses the full canopy when a
        // log breaks. Previously only `Leaves`/`PineLeaves`/`WillowLeaves`
        // matched, leaving birch canopies orphaned.
        assert!(BlockType::Leaves.is_leaves());
        assert!(BlockType::PineLeaves.is_leaves());
        assert!(BlockType::WillowLeaves.is_leaves());
        assert!(BlockType::BirchLeaves.is_leaves());

        // Grass is ground terrain, NOT a canopy block.
        assert!(!BlockType::Grass.is_leaves());
        // Logs are trunks, not leaves.
        assert!(!BlockType::Log.is_leaves());
        assert!(!BlockType::PineLog.is_leaves());
        assert!(!BlockType::WillowLog.is_leaves());
        assert!(!BlockType::BirchLog.is_leaves());
        // Misc non-tree blocks.
        assert!(!BlockType::Air.is_leaves());
        assert!(!BlockType::Stone.is_leaves());
        assert!(!BlockType::Dirt.is_leaves());
    }

    #[test]
    fn test_block_type_u8_round_trip_is_lossless() {
        // CHK-001: every valid discriminant must survive a u8 round-trip.
        // Catches the silent-decode-as-Air failure mode when a variant is
        // added to the enum without extending `From<u8>`.
        for id in 0..=NUM_BLOCK_TYPES_ID_MAX {
            let block = BlockType::from(id);
            assert_eq!(
                block as u8, id,
                "BlockType::from({id}) round-trip failed — got {block:?}; `From<u8>` is missing an arm"
            );
        }
        // Out-of-range IDs fall back to Air (forward-compat with newer saves).
        assert_eq!(BlockType::from(200), BlockType::Air);
    }

    #[test]
    fn test_num_block_types_constants_agree() {
        // CHK-001: count constants and the actual last discriminant stay in
        // sync. The compile-time `_block_type_completeness_guard` catches
        // added variants; this test catches drift in the count constants.
        assert_eq!(NUM_BLOCK_TYPES, NUM_BLOCK_TYPES_ID_MAX as usize + 1);
        assert_eq!(NUM_BLOCK_TYPES_ID_MAX, BlockType::BirchLeaves as u8);
    }

    #[test]
    fn light_block_count_decrements_when_emissive_overwritten() {
        // QA-002: overwriting a GlowStone with a non-emissive block must
        // drop the per-chunk emissive count so `collect_torch_lights` can
        // skip the chunk again instead of scanning it forever.
        let mut chunk = Chunk::new();
        chunk.set_block(0, 0, 0, BlockType::GlowStone);
        assert_eq!(chunk.light_block_count(), 1);

        chunk.set_block(0, 0, 0, BlockType::Water);
        assert_eq!(
            chunk.light_block_count(),
            0,
            "GlowStone -> Water must decrement light_block_count"
        );
    }

    #[test]
    fn light_block_count_increments_when_emissive_placed() {
        // QA-002: placing an emissive block on a non-emissive one must
        // increment the count.
        let mut chunk = Chunk::new();
        chunk.set_block(0, 0, 0, BlockType::Stone);
        assert_eq!(chunk.light_block_count(), 0);

        chunk.set_block(0, 0, 0, BlockType::GlowStone);
        assert_eq!(
            chunk.light_block_count(),
            1,
            "Stone -> GlowStone must increment light_block_count"
        );
    }

    #[test]
    fn light_block_count_stable_on_same_emissive() {
        // QA-002: re-setting the same emissive block type at a position must
        // not double-count.
        let mut chunk = Chunk::new();
        chunk.set_block(0, 0, 0, BlockType::GlowStone);
        assert_eq!(chunk.light_block_count(), 1);

        chunk.set_block(0, 0, 0, BlockType::GlowStone);
        assert_eq!(
            chunk.light_block_count(),
            1,
            "GlowStone -> GlowStone must not change light_block_count"
        );
    }

    #[test]
    fn light_block_count_via_metadata_setter() {
        // QA-002: the typed metadata setters must route through
        // `set_block_with_metadata` so they maintain `light_block_count`
        // correctly. This is the regression that previously drifted —
        // `set_crystal_block` only ever incremented, and the other setters
        // touched `self.blocks` without adjusting the count at all.
        let mut chunk = Chunk::new();

        // Overwriting GlowStone with Crystal (emissive -> emissive) is net 0.
        chunk.set_block(0, 0, 0, BlockType::GlowStone);
        chunk.set_crystal_block(0, 0, 0, 5);
        assert_eq!(
            chunk.light_block_count(),
            1,
            "GlowStone -> Crystal must keep count at 1 (emissive -> emissive)"
        );

        // Overwriting Crystal with TintedGlass (emissive -> non-emissive)
        // via a metadata setter must decrement.
        chunk.set_tinted_glass_block(0, 0, 0, 7);
        assert_eq!(
            chunk.light_block_count(),
            0,
            "Crystal -> TintedGlass via setter must decrement light_block_count"
        );

        // Overwriting a freshly-placed GlowStone with a Painted block via
        // `set_painted_block` must also decrement.
        chunk.set_block(1, 0, 0, BlockType::GlowStone);
        assert_eq!(chunk.light_block_count(), 1);
        chunk.set_painted_block(1, 0, 0, 0, 12);
        assert_eq!(
            chunk.light_block_count(),
            0,
            "GlowStone -> Painted via setter must decrement light_block_count"
        );

        // And `set_water_block` overwriting an emissive block must decrement.
        chunk.set_block(2, 0, 0, BlockType::GlowStone);
        assert_eq!(chunk.light_block_count(), 1);
        chunk.set_water_block(2, 0, 0, WaterType::Lake);
        assert_eq!(
            chunk.light_block_count(),
            0,
            "GlowStone -> Water via setter must decrement light_block_count"
        );
    }

    // ---- PHY-005: role-specific predicate truth tables ----
    //
    // Each predicate currently delegates to `is_solid`, so the truth tables
    // match. These tests document each role's contract so a future block
    // addition (or a deliberate per-role divergence) cannot silently change
    // one role while leaving the others untouched.

    /// Representative block set spanning every is_solid category:
    /// passable (Air, Water, Lava, Glass, TintedGlass, Ice, Model),
    /// solids (Stone, Dirt, Sand, Gravel, Mud), and tree parts that ARE
    /// solid (Log, PineLog, Leaves, PineLeaves). PackedIce is solid; Ice is
    /// not. Painted is solid.
    fn role_test_set() -> &'static [(BlockType, bool)] {
        &[
            // --- passable (is_solid == false) ---
            (BlockType::Air, false),
            (BlockType::Water, false),
            (BlockType::Lava, false),
            (BlockType::Glass, false),
            (BlockType::TintedGlass, false),
            (BlockType::Ice, false),
            (BlockType::Model, false),
            // --- solid (is_solid == true) ---
            (BlockType::Stone, true),
            (BlockType::Dirt, true),
            (BlockType::Grass, true),
            (BlockType::Sand, true),
            (BlockType::Gravel, true),
            (BlockType::Mud, true),
            (BlockType::PackedIce, true),
            (BlockType::Painted, true),
            // --- tree parts (solid per is_solid) ---
            (BlockType::Log, true),
            (BlockType::PineLog, true),
            (BlockType::Leaves, true),
            (BlockType::PineLeaves, true),
        ]
    }

    #[test]
    fn test_blocks_movement_truth_table() {
        for &(block, expected) in role_test_set() {
            assert_eq!(
                block.blocks_movement(),
                expected,
                "blocks_movement({:?}) should be {}",
                block,
                expected
            );
        }
    }

    #[test]
    fn test_stops_fluid_truth_table() {
        for &(block, expected) in role_test_set() {
            assert_eq!(
                block.stops_fluid(),
                expected,
                "stops_fluid({:?}) should be {}",
                block,
                expected
            );
        }
    }

    #[test]
    fn test_provides_support_truth_table() {
        for &(block, expected) in role_test_set() {
            assert_eq!(
                block.provides_support(),
                expected,
                "provides_support({:?}) should be {}",
                block,
                expected
            );
        }
    }

    #[test]
    fn test_connects_to_fences_truth_table() {
        for &(block, expected) in role_test_set() {
            assert_eq!(
                block.connects_to_fences(),
                expected,
                "connects_to_fences({:?}) should be {}",
                block,
                expected
            );
        }
    }

    #[test]
    fn test_is_buildable_ground_truth_table() {
        for &(block, expected) in role_test_set() {
            assert_eq!(
                block.is_buildable_ground(),
                expected,
                "is_buildable_ground({:?}) should be {}",
                block,
                expected
            );
        }
    }

    // ---- Role-specific contract tests reflecting real call sites ----

    #[test]
    fn test_blocks_movement_falling_block_lands_on_stone_not_water() {
        // Mirrors falling_block.rs collision: a falling block stops on stone
        // but would keep falling through water (entities pass through fluids).
        assert!(
            BlockType::Stone.blocks_movement(),
            "falling block must stop on Stone"
        );
        assert!(
            !BlockType::Water.blocks_movement(),
            "falling block must pass through Water"
        );
        assert!(
            !BlockType::Air.blocks_movement(),
            "falling block must fall through Air"
        );
    }

    #[test]
    fn test_stops_fluid_water_spreads_into_air_blocked_by_stone() {
        // Mirrors water.rs calculate_flow: `!stops_fluid(below)` gates whether
        // water flows down into a cell. Water spreads into Air, not into Stone.
        assert!(!BlockType::Air.stops_fluid(), "water must flow into Air");
        assert!(
            BlockType::Stone.stops_fluid(),
            "water must be blocked by Stone"
        );
        assert!(
            !BlockType::Water.stops_fluid(),
            "water must merge into Water (no self-block)"
        );
    }

    #[test]
    fn test_provides_support_model_and_tree_ground() {
        // Mirrors block_update.rs Model ground-support check and
        // tree_logic.rs `tree_has_ground_support`. A Model block above Stone
        // has support; above Air it does not.
        assert!(
            BlockType::Stone.provides_support(),
            "Stone must provide ground support"
        );
        assert!(
            !BlockType::Air.provides_support(),
            "Air must not provide ground support"
        );
        assert!(
            !BlockType::Water.provides_support(),
            "Water must not provide ground support"
        );
    }

    #[test]
    fn test_connects_to_fences_fence_attach_to_stone_not_air() {
        // Mirrors connections.rs `is_fence_connectable`. A fence renders a
        // connection post toward Stone but not toward Air or Glass.
        assert!(
            BlockType::Stone.connects_to_fences(),
            "fence must connect to Stone"
        );
        assert!(
            !BlockType::Air.connects_to_fences(),
            "fence must not connect to Air"
        );
        assert!(
            !BlockType::Glass.connects_to_fences(),
            "fence must not connect to Glass (panes handle Glass separately)"
        );
    }

    #[test]
    fn test_is_buildable_ground_tree_roots_on_stone_not_air() {
        // Mirrors world_gen/trees/oak.rs `!block.is_buildable_ground()` guard:
        // a tree aborts placement if any block in the root column is non-solid.
        assert!(
            BlockType::Stone.is_buildable_ground(),
            "tree must root on Stone"
        );
        assert!(
            !BlockType::Air.is_buildable_ground(),
            "tree must not root on Air"
        );
        assert!(
            !BlockType::Water.is_buildable_ground(),
            "tree must not root on Water"
        );
    }

    #[test]
    fn test_predicates_match_is_solid_for_role_test_set() {
        // PHY-005 invariant: every repointed predicate currently has the same
        // truth table as `is_solid`. If this test fails, a predicate diverged
        // — confirm the divergence is intended and update the per-predicate
        // truth-table test above, not this one.
        for &(block, _) in role_test_set() {
            assert_eq!(block.blocks_movement(), block.is_solid(), "{:?}", block);
            assert_eq!(block.stops_fluid(), block.is_solid(), "{:?}", block);
            assert_eq!(block.provides_support(), block.is_solid(), "{:?}", block);
            assert_eq!(block.connects_to_fences(), block.is_solid(), "{:?}", block);
            assert_eq!(block.is_buildable_ground(), block.is_solid(), "{:?}", block);
        }
    }
}

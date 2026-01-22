use serde::{Deserialize, Serialize};

// =============================================================================
// SUB-VOXEL RESOLUTION CONFIGURATION
// =============================================================================
// Models now support per-model resolution selection.
// Three tiers: Low (8³), Medium (16³), High (32³)

/// Model resolution tiers.
///
/// Each model can independently choose its resolution for the right balance
/// of detail vs. performance:
/// - Low (8³): Simple models like torches, slabs - fastest rendering
/// - Medium (16³): Standard models like doors, fences - balanced
/// - High (32³): Detailed models like statues, decorations - highest quality
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum ModelResolution {
    /// 8×8×8 voxels (512 total) - fastest, good for simple shapes
    Low = 8,
    /// 16×16×16 voxels (4096 total) - balanced detail and performance
    #[default]
    Medium = 16,
    /// 32×32×32 voxels (32768 total) - highest detail
    High = 32,
}

impl ModelResolution {
    /// Returns the size (N) of the model grid (N×N×N).
    #[inline]
    pub const fn size(&self) -> usize {
        *self as u8 as usize
    }

    /// Returns the total number of voxels (N³).
    #[inline]
    pub const fn volume(&self) -> usize {
        let s = self.size();
        s * s * s
    }

    /// Returns the center coordinate (N/2).
    #[inline]
    pub const fn center(&self) -> usize {
        self.size() / 2
    }

    /// Returns the maximum valid coordinate index (N-1).
    #[inline]
    pub const fn max_idx(&self) -> usize {
        self.size() - 1
    }

    /// Returns the center as f32 for ray calculations.
    #[inline]
    pub const fn center_f32(&self) -> f32 {
        self.size() as f32 / 2.0
    }

    /// Returns the bounds with epsilon for ray intersection.
    #[inline]
    pub const fn bounds_f32(&self) -> f32 {
        self.size() as f32 + 0.001
    }

    /// Returns the maximum DDA steps for ray marching (N * 3).
    #[inline]
    pub const fn max_steps(&self) -> usize {
        self.size() * 3
    }

    /// Returns the GPU atlas tier index (0=Low, 1=Medium, 2=High).
    #[inline]
    pub const fn tier(&self) -> usize {
        match *self {
            ModelResolution::Low => 0,
            ModelResolution::Medium => 1,
            ModelResolution::High => 2,
        }
    }

    /// Creates resolution from size value (8, 16, or 32).
    /// Returns Medium if invalid size.
    pub const fn from_size(size: usize) -> Self {
        match size {
            8 => ModelResolution::Low,
            32 => ModelResolution::High,
            _ => ModelResolution::Medium,
        }
    }

    /// Creates resolution from tier index (0, 1, or 2).
    /// Returns Medium if invalid tier.
    pub const fn from_tier(tier: usize) -> Self {
        match tier {
            0 => ModelResolution::Low,
            2 => ModelResolution::High,
            _ => ModelResolution::Medium,
        }
    }

    /// Returns all resolution variants.
    pub const fn all() -> [ModelResolution; 3] {
        [
            ModelResolution::Low,
            ModelResolution::Medium,
            ModelResolution::High,
        ]
    }

    /// Returns display name for UI.
    pub const fn display_name(&self) -> &'static str {
        match *self {
            ModelResolution::Low => "Low (8³)",
            ModelResolution::Medium => "Medium (16³)",
            ModelResolution::High => "High (32³)",
        }
    }
}

// =============================================================================
// CONSTANTS
// =============================================================================

/// Default resolution of sub-voxel models (backward compatibility).
pub const SUB_VOXEL_SIZE: usize = 16;

/// Total voxels per model at default resolution (backward compatibility).
pub const SUB_VOXEL_VOLUME: usize = SUB_VOXEL_SIZE * SUB_VOXEL_SIZE * SUB_VOXEL_SIZE;

/// Center coordinate at default resolution (backward compatibility).
pub const SUB_VOXEL_CENTER: usize = SUB_VOXEL_SIZE / 2;

/// Maximum valid coordinate at default resolution (backward compatibility).
pub const SUB_VOXEL_MAX: usize = SUB_VOXEL_SIZE - 1;

/// Center as f32 at default resolution (backward compatibility).
pub const SUB_VOXEL_CENTER_F32: f32 = SUB_VOXEL_SIZE as f32 / 2.0;

/// Grid bounds at default resolution (backward compatibility).
pub const SUB_VOXEL_BOUNDS_F32: f32 = SUB_VOXEL_SIZE as f32 + 0.001;

/// Maximum unique models in registry.
pub const MAX_MODELS: usize = 256;

/// Colors per model palette (expanded from 16 to 32 for more variety).
pub const PALETTE_SIZE: usize = 32;

/// Number of atlas tiers (Low, Medium, High).
pub const NUM_RESOLUTION_TIERS: usize = 3;

/// First model ID available for custom/user models.
/// Built-in models occupy IDs 0-175; custom models start at 176.
/// - IDs 0-118: Original built-ins (torch, slabs, fences, doors, etc.)
/// - IDs 119-134: Horizontal glass panes (16 connection variants)
/// - IDs 135-150: Vertical glass panes (16 connection variants)
/// - IDs 151-159: Reserved placeholders
/// - IDs 160-175: Picture frames (16 edge mask variants)
pub const CRYSTAL_MODEL_ID: u8 = 99;
pub const FIRST_CUSTOM_MODEL_ID: u8 = 176;

/// RGBA color for sub-voxel palette.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    /// Creates an opaque RGB color.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Creates an RGBA color.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Fully transparent (air).
    pub const fn transparent() -> Self {
        Self {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        }
    }

    /// Converts to [r, g, b, a] array.
    pub const fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

/// How a model blocks light.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub enum LightBlocking {
    /// Doesn't block light (air-like).
    #[default]
    None,
    /// Partially blocks light (leaves-like).
    Partial,
    /// Fully blocks light (solid).
    Full,
}

/// Light animation modes for emissive models.
///
/// These modes control how the light intensity varies over time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum LightMode {
    /// Constant brightness (no animation).
    #[default]
    Steady = 0,
    /// Smooth sine wave oscillation (gentle pulsing).
    Pulse = 1,
    /// Random noise-based flickering (fire/torch-like).
    Flicker = 2,
    /// Slower, more subtle flickering (candle-like).
    Candle = 3,
    /// Fast on/off blinking.
    Strobe = 4,
    /// Very slow pulse (like breathing).
    Breathe = 5,
    /// Occasional random bright flashes (sparkle effect).
    Sparkle = 6,
    /// Position-based wave pattern (for synchronized light chains).
    Wave = 7,
    /// Gradual warm-up then steady (like incandescent bulb).
    WarmUp = 8,
    /// Electrical arc/welding effect (irregular bright bursts).
    Arc = 9,
}

impl LightMode {
    /// Returns all available light modes.
    pub const fn all() -> [LightMode; 10] {
        [
            LightMode::Steady,
            LightMode::Pulse,
            LightMode::Flicker,
            LightMode::Candle,
            LightMode::Strobe,
            LightMode::Breathe,
            LightMode::Sparkle,
            LightMode::Wave,
            LightMode::WarmUp,
            LightMode::Arc,
        ]
    }

    /// Returns display name for UI.
    pub const fn display_name(&self) -> &'static str {
        match *self {
            LightMode::Steady => "Steady",
            LightMode::Pulse => "Pulse",
            LightMode::Flicker => "Flicker",
            LightMode::Candle => "Candle",
            LightMode::Strobe => "Strobe",
            LightMode::Breathe => "Breathe",
            LightMode::Sparkle => "Sparkle",
            LightMode::Wave => "Wave",
            LightMode::WarmUp => "Warm Up",
            LightMode::Arc => "Arc",
        }
    }

    /// Returns a brief description for tooltips.
    pub const fn description(&self) -> &'static str {
        match *self {
            LightMode::Steady => "Constant brightness",
            LightMode::Pulse => "Smooth sine wave pulsing",
            LightMode::Flicker => "Fire/torch-like random flickering",
            LightMode::Candle => "Subtle candle-like flickering",
            LightMode::Strobe => "Fast on/off blinking",
            LightMode::Breathe => "Very slow, gentle pulsing",
            LightMode::Sparkle => "Occasional random bright flashes",
            LightMode::Wave => "Synchronized wave pattern",
            LightMode::WarmUp => "Gradual warm-up then steady",
            LightMode::Arc => "Electrical arc/welding effect",
        }
    }

    /// Returns the animation speed multiplier for this mode.
    pub const fn speed(&self) -> f32 {
        match *self {
            LightMode::Steady => 0.0,
            LightMode::Pulse => 2.0,
            LightMode::Flicker => 10.0,
            LightMode::Candle => 4.0,
            LightMode::Strobe => 15.0,
            LightMode::Breathe => 0.5,
            LightMode::Sparkle => 8.0,
            LightMode::Wave => 1.0,
            LightMode::WarmUp => 0.3,
            LightMode::Arc => 20.0,
        }
    }

    /// Returns the intensity range (min, max) for this mode.
    pub const fn intensity_range(&self) -> (f32, f32) {
        match *self {
            LightMode::Steady => (1.0, 1.0),
            LightMode::Pulse => (0.5, 1.0),
            LightMode::Flicker => (0.3, 1.0),
            LightMode::Candle => (0.6, 1.0),
            LightMode::Strobe => (0.0, 1.0),
            LightMode::Breathe => (0.4, 1.0),
            LightMode::Sparkle => (0.7, 1.5), // Can flash brighter
            LightMode::Wave => (0.3, 1.0),
            LightMode::WarmUp => (0.0, 1.0),
            LightMode::Arc => (0.2, 2.0), // Very bright bursts
        }
    }
}

/// Shape variants for stair models.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StairShape {
    Straight,
    InnerLeft,
    InnerRight,
    OuterLeft,
    OuterRight,
}

// =============================================================================
// CUSTOM DOOR PAIRS
// =============================================================================

/// A simplified door pair linking 4 custom models together as a functional door.
///
/// This allows users to create custom door models in the editor and link them
/// to create fully functional doors that open/close when right-clicked.
///
/// Unlike built-in doors which have 8 variants (hinge left/right), custom doors
/// use a simple 4-model system:
/// - lower_closed: Bottom half when door is closed
/// - upper_closed: Top half when door is closed
/// - lower_open: Bottom half when door is open
/// - upper_open: Top half when door is open
///
/// The door pair itself doesn't have a model ID - it references 4 existing models
/// and provides the toggle logic to swap between closed/open states.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleDoorPair {
    /// Unique identifier for this door pair (auto-assigned, starts at 0).
    pub id: u16,
    /// Display name for the door pair (shown in palette).
    pub name: String,
    /// Model ID for the lower half when closed.
    pub lower_closed: u8,
    /// Model ID for the upper half when closed.
    pub upper_closed: u8,
    /// Model ID for the lower half when open.
    pub lower_open: u8,
    /// Model ID for the upper half when open.
    pub upper_open: u8,
}

impl SimpleDoorPair {
    /// Creates a new door pair with the given models.
    pub fn new(
        name: impl Into<String>,
        lower_closed: u8,
        upper_closed: u8,
        lower_open: u8,
        upper_open: u8,
    ) -> Self {
        Self {
            id: 0, // Will be set by registry
            name: name.into(),
            lower_closed,
            upper_closed,
            lower_open,
            upper_open,
        }
    }

    /// Returns true if the given model ID is part of this door pair.
    pub fn contains_model(&self, model_id: u8) -> bool {
        model_id == self.lower_closed
            || model_id == self.upper_closed
            || model_id == self.lower_open
            || model_id == self.upper_open
    }

    /// Returns true if the given model ID is the upper half of the door.
    pub fn is_upper(&self, model_id: u8) -> bool {
        model_id == self.upper_closed || model_id == self.upper_open
    }

    /// Returns true if the given model ID represents the open state.
    pub fn is_open(&self, model_id: u8) -> bool {
        model_id == self.lower_open || model_id == self.upper_open
    }

    /// Returns the toggled version of a door model (open <-> closed).
    pub fn toggle(&self, model_id: u8) -> u8 {
        if model_id == self.lower_closed {
            self.lower_open
        } else if model_id == self.upper_closed {
            self.upper_open
        } else if model_id == self.lower_open {
            self.lower_closed
        } else if model_id == self.upper_open {
            self.upper_closed
        } else {
            model_id // Not part of this door
        }
    }

    /// Returns the other half of the door (upper <-> lower), in the same state.
    pub fn other_half(&self, model_id: u8) -> u8 {
        if model_id == self.lower_closed {
            self.upper_closed
        } else if model_id == self.upper_closed {
            self.lower_closed
        } else if model_id == self.lower_open {
            self.upper_open
        } else if model_id == self.upper_open {
            self.lower_open
        } else {
            model_id // Not part of this door
        }
    }

    /// Returns all model IDs that should be placed in the palette.
    /// (Returns the lower_closed ID for initial placement.)
    pub fn palette_model_id(&self) -> u8 {
        self.lower_closed
    }

    /// Validates that all referenced model IDs exist in the registry.
    pub fn validate(&self, registry: &super::registry::ModelRegistry) -> Result<(), String> {
        for (model_id, role) in [
            (self.lower_closed, "lower_closed"),
            (self.upper_closed, "upper_closed"),
            (self.lower_open, "lower_open"),
            (self.upper_open, "upper_open"),
        ] {
            if registry.get(model_id).is_none() {
                return Err(format!(
                    "Model ID {} ({}) not found in registry",
                    model_id, role
                ));
            }
        }
        Ok(())
    }
}

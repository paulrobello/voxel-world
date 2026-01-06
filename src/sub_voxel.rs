//! Sub-voxel model system for detailed block models.
//!
//! Models (torch, slab, fence, stairs, etc.) are stored once in a registry and
//! referenced by blocks. Each model is an N×N×N voxel grid with a 16-color palette.
//! Resolution is per-model: Low (8³), Medium (16³), or High (32³).
//!
//! Memory efficiency:
//! - Three tiered GPU atlases for different resolutions
//! - Registry footprint scales with model complexity choices
//! - Sparse metadata: only Model blocks store model_id + rotation

#![allow(dead_code)] // Some editor-facing APIs are still planned; rendering/interaction use this today.

use nalgebra::Vector3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

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
// LEGACY CONSTANTS (for compatibility during transition)
// =============================================================================
// These constants use Medium resolution (16³) as the default.
// New code should use ModelResolution methods instead.

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
/// Built-in models use IDs 0-98:
/// - 0: Empty
/// - 1: Torch
/// - 2-3: Slabs
/// - 4-19: Fences (16 variants)
/// - 20-27: Gates (8 variants)
/// - 28: Stairs
/// - 29: Ladder
/// - 30: Inverted stairs
/// - 31-38: Corner stairs (8 variants)
/// - 39-46: Plain Doors (8 variants: lower/upper × hinge left/right × closed/open)
/// - 47-50: Trapdoors (4 variants: floor/ceiling × closed/open)
/// - 51-66: Windows (16 connection variants)
/// - 67-74: Windowed Doors (8 variants)
/// - 75-82: Paneled Doors (8 variants)
/// - 83-90: Windowed+Paneled Doors (8 variants)
/// - 91-98: Full Glass Doors (8 variants)
/// - 99: Crystal
pub const CRYSTAL_MODEL_ID: u8 = 99;
pub const FIRST_CUSTOM_MODEL_ID: u8 = 100;

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

/// A single sub-voxel model definition.
///
/// Models are N×N×N voxel grids (where N is determined by resolution) and each
/// voxel is a palette index (0 = air). The 32-color palette with per-slot
/// emission allows for rich visual variety including glowing elements.
#[derive(Debug, Clone)]
pub struct SubVoxelModel {
    /// Model ID (assigned by registry).
    pub id: u8,

    /// Human-readable name for debugging and editor.
    pub name: String,

    /// Model resolution (8³, 16³, or 32³).
    pub resolution: ModelResolution,

    /// N³ voxel grid as palette indices (0 = air/transparent).
    /// Index = x + y * N + z * N² where N = resolution.size().
    /// Length is always resolution.volume().
    pub voxels: Vec<u8>,

    /// 32-color RGBA palette for this model.
    /// Index 0 is always transparent (air).
    /// Alpha channel controls transparency (0=fully transparent, 255=opaque).
    pub palette: [Color; PALETTE_SIZE],

    /// Per-palette-slot emission intensity (0.0 = no glow, 1.0 = full emission).
    /// Allows individual palette colors to emit light (e.g., torch flames, glowing crystals).
    /// Index 0 is always 0.0 (air doesn't emit).
    pub palette_emission: [f32; PALETTE_SIZE],

    /// 4×4×4 collision bitmask (64 bits).
    /// Each bit represents an (N/4)³ region of the model.
    /// Bit index = cx + cy * 4 + cz * 16 where cx,cy,cz in 0..4.
    pub collision_mask: u64,

    /// How this model blocks light.
    pub light_blocking: LightBlocking,

    /// Whether this model can be rotated (90° increments around Y).
    pub rotatable: bool,

    /// Overall model emission color (legacy, for simple emissive models).
    /// For per-voxel emission, use palette_emission instead.
    pub emission: Option<Color>,

    /// Whether this model acts as a point light source.
    /// When enabled, the model emits light into the surrounding area.
    pub is_light_source: bool,

    /// Light animation mode (only used when is_light_source is true).
    pub light_mode: LightMode,

    /// Light radius in blocks (how far the light reaches).
    /// Default is 8.0 blocks. Range: 1.0 - 32.0.
    pub light_radius: f32,

    /// Light intensity multiplier (0.0 - 2.0).
    /// Default is 1.0. Values > 1.0 create brighter lights.
    pub light_intensity: f32,

    /// Whether this model requires ground support (breaks if block below is removed).
    pub requires_ground_support: bool,
}

impl Default for SubVoxelModel {
    fn default() -> Self {
        Self::with_resolution(ModelResolution::Medium)
    }
}

impl SubVoxelModel {
    /// Creates a new empty model with default (Medium) resolution.
    pub fn new(name: &str) -> Self {
        Self::with_resolution_and_name(ModelResolution::Medium, name)
    }

    /// Creates a new empty model with the specified resolution.
    pub fn with_resolution(resolution: ModelResolution) -> Self {
        let mut palette = [Color::transparent(); PALETTE_SIZE];
        palette[0] = Color::transparent(); // Index 0 is always air

        Self {
            id: 0,
            name: String::new(),
            resolution,
            voxels: vec![0; resolution.volume()],
            palette,
            palette_emission: [0.0; PALETTE_SIZE], // No emission by default
            collision_mask: 0,
            light_blocking: LightBlocking::None,
            rotatable: false,
            emission: None,
            is_light_source: false,
            light_mode: LightMode::Steady,
            light_radius: 8.0,
            light_intensity: 1.0,
            requires_ground_support: false,
        }
    }

    /// Sets the emission intensity for a palette slot.
    /// Emission makes the color glow and emit light (0.0 = none, 1.0 = full).
    pub fn set_palette_emission(&mut self, palette_idx: usize, emission: f32) {
        if palette_idx < PALETTE_SIZE {
            self.palette_emission[palette_idx] = emission.clamp(0.0, 1.0);
        }
    }

    /// Gets the emission intensity for a palette slot.
    pub fn get_palette_emission(&self, palette_idx: usize) -> f32 {
        if palette_idx < PALETTE_SIZE {
            self.palette_emission[palette_idx]
        } else {
            0.0
        }
    }

    /// Configures this model as a light source.
    pub fn set_light_source(
        &mut self,
        enabled: bool,
        mode: LightMode,
        radius: f32,
        intensity: f32,
    ) {
        self.is_light_source = enabled;
        self.light_mode = mode;
        self.light_radius = radius.clamp(1.0, 32.0);
        self.light_intensity = intensity.clamp(0.0, 2.0);
    }

    /// Enables this model as a simple steady light source.
    pub fn enable_light(&mut self, radius: f32, intensity: f32) {
        self.set_light_source(true, LightMode::Steady, radius, intensity);
    }

    /// Enables this model as a flickering light source (torch/fire-like).
    pub fn enable_flickering_light(&mut self, radius: f32, intensity: f32) {
        self.set_light_source(true, LightMode::Flicker, radius, intensity);
    }

    /// Returns true if this model has any emissive palette entries.
    pub fn has_palette_emission(&self) -> bool {
        self.palette_emission.iter().any(|&e| e > 0.0)
    }

    /// Returns the dominant emission color from the palette.
    /// Used for light source color when is_light_source is enabled.
    pub fn dominant_emission_color(&self) -> Option<Color> {
        let mut max_emission = 0.0f32;
        let mut dominant_idx = None;

        for (idx, &emission) in self.palette_emission.iter().enumerate() {
            if emission > max_emission && self.palette[idx].a > 0 {
                max_emission = emission;
                dominant_idx = Some(idx);
            }
        }

        dominant_idx.map(|idx| self.palette[idx])
    }

    /// Creates a new empty model with the specified resolution and name.
    pub fn with_resolution_and_name(resolution: ModelResolution, name: &str) -> Self {
        let mut model = Self::with_resolution(resolution);
        model.name = name.to_string();
        model
    }

    /// Returns the size (N) of this model's voxel grid.
    #[inline]
    pub fn size(&self) -> usize {
        self.resolution.size()
    }

    /// Returns the total number of voxels in this model.
    #[inline]
    pub fn volume(&self) -> usize {
        self.resolution.volume()
    }

    /// Gets voxel palette index at (x, y, z).
    #[inline]
    pub fn get_voxel(&self, x: usize, y: usize, z: usize) -> u8 {
        let size = self.size();
        debug_assert!(x < size && y < size && z < size);
        self.voxels[x + y * size + z * size * size]
    }

    /// Sets voxel palette index at (x, y, z).
    #[inline]
    pub fn set_voxel(&mut self, x: usize, y: usize, z: usize, palette_idx: u8) {
        let size = self.size();
        debug_assert!(x < size && y < size && z < size);
        debug_assert!((palette_idx as usize) < PALETTE_SIZE);
        self.voxels[x + y * size + z * size * size] = palette_idx;
    }

    /// Fills a box region with a palette index.
    #[allow(clippy::too_many_arguments)]
    pub fn fill_box(
        &mut self,
        x0: usize,
        y0: usize,
        z0: usize,
        x1: usize,
        y1: usize,
        z1: usize,
        palette_idx: u8,
    ) {
        let max_idx = self.resolution.max_idx();
        for z in z0..=z1.min(max_idx) {
            for y in y0..=y1.min(max_idx) {
                for x in x0..=x1.min(max_idx) {
                    self.set_voxel(x, y, z, palette_idx);
                }
            }
        }
    }

    /// Computes the 4×4×4 collision mask from the voxel data.
    ///
    /// Each bit in the 64-bit mask represents a (N/4)³ region where N is the resolution.
    /// A bit is set if ANY voxel in that region is solid (non-zero).
    pub fn compute_collision_mask(&mut self) {
        self.collision_mask = 0;
        let cell_size = self.size() / 4;

        for cz in 0..4 {
            for cy in 0..4 {
                for cx in 0..4 {
                    let mut has_solid = false;

                    // Check cell_size³ region
                    'region: for dz in 0..cell_size {
                        for dy in 0..cell_size {
                            for dx in 0..cell_size {
                                let vx = cx * cell_size + dx;
                                let vy = cy * cell_size + dy;
                                let vz = cz * cell_size + dz;

                                if self.get_voxel(vx, vy, vz) != 0 {
                                    has_solid = true;
                                    break 'region;
                                }
                            }
                        }
                    }

                    if has_solid {
                        let bit = cx + cy * 4 + cz * 16;
                        self.collision_mask |= 1u64 << bit;
                    }
                }
            }
        }
    }

    /// Checks if a point collides with this model using the collision mask.
    ///
    /// Point coordinates are in model-local space (0.0 to 1.0).
    #[inline]
    pub fn point_collides(&self, x: f32, y: f32, z: f32) -> bool {
        let range = 0.0_f32..1.0_f32;
        if !range.contains(&x) || !range.contains(&y) || !range.contains(&z) {
            return false;
        }

        // Scale to 4×4×4 collision grid
        let cx = (x * 4.0) as usize;
        let cy = (y * 4.0) as usize;
        let cz = (z * 4.0) as usize;

        let bit = cx + cy * 4 + cz * 16;
        (self.collision_mask & (1u64 << bit)) != 0
    }

    /// Performs a DDA-based ray intersection test against this model.
    ///
    /// origin: ray origin in block-local coordinates (0-1)
    /// dir: normalized ray direction
    /// rotation: 0-3 for Y-axis rotation
    /// Returns: Some(hit_distance) if hit, None otherwise
    pub fn ray_intersects(
        &self,
        origin: Vector3<f32>,
        dir: Vector3<f32>,
        rotation: u8,
    ) -> Option<(f32, Vector3<i32>)> {
        let center = self.resolution.center() as i32;
        let size = self.size() as i32;
        let size_f = self.size() as f32;
        let max_steps = self.resolution.max_steps();

        // Helper closure for rotation (captures center)
        let rotate_pos = |pos: Vector3<i32>, rot: u8| -> Vector3<i32> {
            let px = pos.x - center;
            let pz = pos.z - center;
            match rot & 3 {
                1 => Vector3::new(center - pz - 1, pos.y, center + px),
                2 => Vector3::new(center - px - 1, pos.y, center - pz - 1),
                3 => Vector3::new(center + pz, pos.y, center - px - 1),
                _ => pos,
            }
        };

        // Scale to sub-voxel coordinates (0 to size)
        let pos = origin * size_f;

        // Avoid division by zero
        let safe_dir = Vector3::new(
            if dir.x.abs() < 1e-6 {
                1e-6 * dir.x.signum()
            } else {
                dir.x
            },
            if dir.y.abs() < 1e-6 {
                1e-6 * dir.y.signum()
            } else {
                dir.y
            },
            if dir.z.abs() < 1e-6 {
                1e-6 * dir.z.signum()
            } else {
                dir.z
            },
        );
        let inv_dir = Vector3::new(1.0 / safe_dir.x, 1.0 / safe_dir.y, 1.0 / safe_dir.z);

        // Calculate entry/exit t for the model cube
        let t_min_v = (Vector3::new(-0.001, -0.001, -0.001) - pos).component_mul(&inv_dir);
        let t_max_v = (Vector3::new(size_f + 0.001, size_f + 0.001, size_f + 0.001) - pos)
            .component_mul(&inv_dir);

        let t1 = Vector3::new(
            t_min_v.x.min(t_max_v.x),
            t_min_v.y.min(t_max_v.y),
            t_min_v.z.min(t_max_v.z),
        );
        let t2 = Vector3::new(
            t_min_v.x.max(t_max_v.x),
            t_min_v.y.max(t_max_v.y),
            t_min_v.z.max(t_max_v.z),
        );

        let t_near = t1.x.max(t1.y).max(t1.z);
        let t_far = t2.x.min(t2.y).min(t2.z);

        if t_near > t_far || t_far < 0.0 {
            return None;
        }

        let entry_axis = if t1.x >= t1.y && t1.x >= t1.z {
            0
        } else if t1.y >= t1.z {
            1
        } else {
            2
        };

        let start_t = t_near.max(0.0);
        let mut current_pos = pos + safe_dir * start_t;
        current_pos += safe_dir * 0.001; // nudge
        current_pos = current_pos.map(|v| v.clamp(0.001, size_f - 0.001));

        let mut voxel = Vector3::new(
            current_pos.x.floor() as i32,
            current_pos.y.floor() as i32,
            current_pos.z.floor() as i32,
        );
        let step = safe_dir.map(|v| if v >= 0.0 { 1 } else { -1 });
        let t_delta = inv_dir.map(|v| v.abs());

        let mut t_max = Vector3::new(
            if step.x > 0 {
                (voxel.x + 1) as f32 - current_pos.x
            } else {
                current_pos.x - voxel.x as f32
            }
            .abs()
                * t_delta.x,
            if step.y > 0 {
                (voxel.y + 1) as f32 - current_pos.y
            } else {
                current_pos.y - voxel.y as f32
            }
            .abs()
                * t_delta.y,
            if step.z > 0 {
                (voxel.z + 1) as f32 - current_pos.z
            } else {
                current_pos.z - voxel.z as f32
            }
            .abs()
                * t_delta.z,
        );

        let mut stepped_axis = entry_axis;

        for i in 0..max_steps {
            if voxel.x < 0
                || voxel.x >= size
                || voxel.y < 0
                || voxel.y >= size
                || voxel.z < 0
                || voxel.z >= size
            {
                break;
            }

            let rotated = rotate_pos(voxel, rotation);
            if self.get_voxel(rotated.x as usize, rotated.y as usize, rotated.z as usize) != 0 {
                let hit_axis = if i == 0 { entry_axis } else { stepped_axis };
                let mut normal = Vector3::zeros();
                normal[hit_axis] = -step[hit_axis];

                let voxel_dist = if i == 0 {
                    0.0
                } else {
                    t_max[stepped_axis] - t_delta[stepped_axis]
                };
                let t = (start_t + voxel_dist) / size_f;
                return Some((t, normal));
            }

            if t_max.x < t_max.y {
                if t_max.x < t_max.z {
                    voxel.x += step.x;
                    stepped_axis = 0;
                    t_max.x += t_delta.x;
                } else {
                    voxel.z += step.z;
                    stepped_axis = 2;
                    t_max.z += t_delta.z;
                }
            } else if t_max.y < t_max.z {
                voxel.y += step.y;
                stepped_axis = 1;
                t_max.y += t_delta.y;
            } else {
                voxel.z += step.z;
                stepped_axis = 2;
                t_max.z += t_delta.z;
            }
        }

        None
    }

    /// Packs palette colors for GPU upload (128 bytes = 32 × RGBA).
    pub fn pack_palette(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(PALETTE_SIZE * 4);
        for color in &self.palette {
            data.extend_from_slice(&color.to_array());
        }
        data
    }

    /// Packs palette emission values for GPU upload (32 floats = 128 bytes).
    pub fn pack_palette_emission(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(PALETTE_SIZE * 4);
        for &emission in &self.palette_emission {
            data.extend_from_slice(&emission.to_le_bytes());
        }
        data
    }

    /// Packs combined palette data for GPU upload (RGBA + emission per slot).
    /// Format: 32 entries × 5 bytes (R, G, B, A, emission_u8)
    pub fn pack_palette_combined(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(PALETTE_SIZE * 5);
        for (color, &emission) in self.palette.iter().zip(self.palette_emission.iter()) {
            data.extend_from_slice(&color.to_array());
            // Pack emission as u8 (0-255, scaled from 0.0-1.0)
            data.push((emission * 255.0) as u8);
        }
        data
    }
}

/// Global registry of all sub-voxel models.
///
/// Models are registered at startup and referenced by ID in block metadata.
/// The registry provides efficient GPU data packing for shader access.
/// Supports three resolution tiers (Low/8³, Medium/16³, High/32³).
pub struct ModelRegistry {
    /// All registered models (index = model_id).
    models: Vec<SubVoxelModel>,

    /// Lookup by name for editor/tools.
    name_to_id: HashMap<String, u8>,

    /// Whether GPU buffers need update (general flag).
    gpu_dirty: bool,

    /// Per-tier dirty flags [Low, Medium, High].
    /// Each tier's atlas is updated independently for efficiency.
    tier_dirty: [bool; NUM_RESOLUTION_TIERS],
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelRegistry {
    /// Creates a new registry with built-in models.
    pub fn new() -> Self {
        let mut registry = Self {
            models: Vec::with_capacity(MAX_MODELS),
            name_to_id: HashMap::new(),
            gpu_dirty: true,
            tier_dirty: [true; NUM_RESOLUTION_TIERS], // All tiers need initial upload
        };

        // Register built-in models
        registry.register_builtins();
        registry
    }

    /// Registers built-in models.
    fn register_builtins(&mut self) {
        use crate::sub_voxel_builtins::{
            create_door_lower_closed_left, create_door_lower_closed_right,
            create_door_lower_open_left, create_door_lower_open_right,
            create_door_upper_closed_left, create_door_upper_closed_right,
            create_door_upper_open_left, create_door_upper_open_right, create_empty,
            create_fancy_door_lower_closed_left, create_fancy_door_lower_closed_right,
            create_fancy_door_lower_open_left, create_fancy_door_lower_open_right,
            create_fancy_door_upper_closed_left, create_fancy_door_upper_closed_right,
            create_fancy_door_upper_open_left, create_fancy_door_upper_open_right, create_fence,
            create_gate_closed, create_gate_open, create_glass_door_lower_closed_left,
            create_glass_door_lower_closed_right, create_glass_door_lower_open_left,
            create_glass_door_lower_open_right, create_glass_door_upper_closed_left,
            create_glass_door_upper_closed_right, create_glass_door_upper_open_left,
            create_glass_door_upper_open_right, create_ladder,
            create_paneled_door_lower_closed_left, create_paneled_door_lower_closed_right,
            create_paneled_door_lower_open_left, create_paneled_door_lower_open_right,
            create_paneled_door_upper_closed_left, create_paneled_door_upper_closed_right,
            create_paneled_door_upper_open_left, create_paneled_door_upper_open_right,
            create_slab_bottom, create_slab_top, create_stairs_inner_left,
            create_stairs_inner_left_inverted, create_stairs_inner_right,
            create_stairs_inner_right_inverted, create_stairs_north, create_stairs_north_inverted,
            create_stairs_outer_left, create_stairs_outer_left_inverted, create_stairs_outer_right,
            create_stairs_outer_right_inverted, create_torch, create_trapdoor_ceiling_closed,
            create_trapdoor_ceiling_open, create_trapdoor_floor_closed, create_trapdoor_floor_open,
            create_window, create_windowed_door_lower_closed_left,
            create_windowed_door_lower_closed_right, create_windowed_door_lower_open_left,
            create_windowed_door_lower_open_right, create_windowed_door_upper_closed_left,
            create_windowed_door_upper_closed_right, create_windowed_door_upper_open_left,
            create_windowed_door_upper_open_right,
        };

        // ID 0: Empty/placeholder (no model)
        self.register(create_empty());

        // ID 1: Torch
        self.register(create_torch());

        // ID 2-3: Slabs
        self.register(create_slab_bottom());
        self.register(create_slab_top());

        // ID 4-19: Fence variants (16 connection combinations)
        // Connection bitmask: N=1, S=2, E=4, W=8
        for connections in 0..16u8 {
            self.register(create_fence(connections));
        }

        // ID 20-23: Closed gate variants (4 connection combinations)
        // Connection bitmask: W=1, E=2
        for connections in 0..4u8 {
            self.register(create_gate_closed(connections));
        }

        // ID 24-27: Open gate variants (4 connection combinations)
        for connections in 0..4u8 {
            self.register(create_gate_open(connections));
        }

        // ID 28: Stairs
        self.register(create_stairs_north());

        // ID 29: Ladder
        self.register(create_ladder());

        // ID 30: Upside-down stairs
        self.register(create_stairs_north_inverted());

        // ID 31-34: Inner/outer stairs (upright)
        self.register(create_stairs_inner_left());
        self.register(create_stairs_inner_right());
        self.register(create_stairs_outer_left());
        self.register(create_stairs_outer_right());

        // ID 35-38: Inner/outer stairs (inverted)
        self.register(create_stairs_inner_left_inverted());
        self.register(create_stairs_inner_right_inverted());
        self.register(create_stairs_outer_left_inverted());
        self.register(create_stairs_outer_right_inverted());

        // ID 39-46: Doors (8 variants)
        // Order: lower closed left, lower closed right, upper closed left, upper closed right,
        //        lower open left, lower open right, upper open left, upper open right
        self.register(create_door_lower_closed_left());
        self.register(create_door_lower_closed_right());
        self.register(create_door_upper_closed_left());
        self.register(create_door_upper_closed_right());
        self.register(create_door_lower_open_left());
        self.register(create_door_lower_open_right());
        self.register(create_door_upper_open_left());
        self.register(create_door_upper_open_right());

        // ID 47-50: Trapdoors (4 variants)
        self.register(create_trapdoor_floor_closed());
        self.register(create_trapdoor_ceiling_closed());
        self.register(create_trapdoor_floor_open());
        self.register(create_trapdoor_ceiling_open());

        // ID 51-66: Windows (16 connection variants)
        for connections in 0..16u8 {
            self.register(create_window(connections));
        }

        // ID 67-74: Windowed Doors (8 variants)
        self.register(create_windowed_door_lower_closed_left());
        self.register(create_windowed_door_lower_closed_right());
        self.register(create_windowed_door_upper_closed_left());
        self.register(create_windowed_door_upper_closed_right());
        self.register(create_windowed_door_lower_open_left());
        self.register(create_windowed_door_lower_open_right());
        self.register(create_windowed_door_upper_open_left());
        self.register(create_windowed_door_upper_open_right());

        // ID 75-82: Paneled Doors (8 variants)
        self.register(create_paneled_door_lower_closed_left());
        self.register(create_paneled_door_lower_closed_right());
        self.register(create_paneled_door_upper_closed_left());
        self.register(create_paneled_door_upper_closed_right());
        self.register(create_paneled_door_lower_open_left());
        self.register(create_paneled_door_lower_open_right());
        self.register(create_paneled_door_upper_open_left());
        self.register(create_paneled_door_upper_open_right());

        // ID 83-90: Windowed+Paneled Doors (8 variants)
        self.register(create_fancy_door_lower_closed_left());
        self.register(create_fancy_door_lower_closed_right());
        self.register(create_fancy_door_upper_closed_left());
        self.register(create_fancy_door_upper_closed_right());
        self.register(create_fancy_door_lower_open_left());
        self.register(create_fancy_door_lower_open_right());
        self.register(create_fancy_door_upper_open_left());
        self.register(create_fancy_door_upper_open_right());

        // ID 91-98: Full Glass Doors (8 variants)
        self.register(create_glass_door_lower_closed_left());
        self.register(create_glass_door_lower_closed_right());
        self.register(create_glass_door_upper_closed_left());
        self.register(create_glass_door_upper_closed_right());
        self.register(create_glass_door_lower_open_left());
        self.register(create_glass_door_lower_open_right());
        self.register(create_glass_door_upper_open_left());
        self.register(create_glass_door_upper_open_right());

        // ID 99: Crystal (neutral color, tinted by shader based on tint_index)
        use crate::sub_voxel_builtins::create_crystal;
        self.register(create_crystal(Color::rgb(220, 220, 220)));
    }

    /// Gets the model ID for a fence with the given connections.
    /// Connection bitmask: N=1, S=2, E=4, W=8
    pub fn fence_model_id(connections: u8) -> u8 {
        4 + (connections & 0x0F)
    }

    /// Checks if a model ID is a fence (IDs 4-19).
    pub fn is_fence_model(model_id: u8) -> bool {
        (4..20).contains(&model_id)
    }

    /// Gets the connection mask from a fence model ID.
    /// Returns None if not a fence model.
    pub fn fence_connections(model_id: u8) -> Option<u8> {
        if Self::is_fence_model(model_id) {
            Some(model_id - 4)
        } else {
            None
        }
    }

    /// Gets the model ID for a closed gate with the given connections.
    /// Connection bitmask: W=1, E=2
    pub fn gate_closed_model_id(connections: u8) -> u8 {
        20 + (connections & 0x03)
    }

    /// Gets the model ID for an open gate with the given connections.
    /// Connection bitmask: W=1, E=2
    pub fn gate_open_model_id(connections: u8) -> u8 {
        24 + (connections & 0x03)
    }

    /// Checks if a model ID is a closed gate (IDs 20-23).
    pub fn is_gate_closed_model(model_id: u8) -> bool {
        (20..24).contains(&model_id)
    }

    /// Checks if a model ID is an open gate (IDs 24-27).
    pub fn is_gate_open_model(model_id: u8) -> bool {
        (24..28).contains(&model_id)
    }

    /// Checks if a model ID is any gate (IDs 20-27).
    pub fn is_gate_model(model_id: u8) -> bool {
        (20..28).contains(&model_id)
    }

    /// Gets the connection mask from a gate model ID.
    /// Returns None if not a gate model.
    pub fn gate_connections(model_id: u8) -> Option<u8> {
        if Self::is_gate_model(model_id) {
            Some((model_id - 20) & 0x03)
        } else {
            None
        }
    }

    /// Checks if a model is a fence or gate (connectable blocks).
    pub fn is_fence_or_gate(model_id: u8) -> bool {
        Self::is_fence_model(model_id) || Self::is_gate_model(model_id)
    }

    /// Gets the model ID for a ladder.
    pub fn ladder_model_id() -> u8 {
        29
    }

    /// Checks if a model ID is a ladder (ID 29).
    pub fn is_ladder_model(model_id: u8) -> bool {
        model_id == 29
    }

    /// Returns the model ID for the upside-down stairs.
    pub fn stairs_inverted_model_id() -> u8 {
        30
    }

    /// Returns true if model_id is any stair variant.
    pub fn is_stairs_model(model_id: u8) -> bool {
        (28..=38).contains(&model_id)
    }

    /// Returns true if the stair model is upside-down.
    pub fn is_stairs_inverted(model_id: u8) -> bool {
        matches!(model_id, 30 | 35 | 36 | 37 | 38)
    }

    /// Returns the shape for a stair model_id.
    pub fn stairs_shape(model_id: u8) -> Option<StairShape> {
        match model_id {
            28 | 30 => Some(StairShape::Straight),
            31 | 35 => Some(StairShape::InnerLeft),
            32 | 36 => Some(StairShape::InnerRight),
            33 | 37 => Some(StairShape::OuterLeft),
            34 | 38 => Some(StairShape::OuterRight),
            _ => None,
        }
    }

    /// Returns the model ID for the requested stair shape and orientation.
    pub fn stairs_model_id(shape: StairShape, inverted: bool) -> u8 {
        match (shape, inverted) {
            (StairShape::Straight, false) => 28,
            (StairShape::Straight, true) => 30,
            (StairShape::InnerLeft, false) => 31,
            (StairShape::InnerLeft, true) => 35,
            (StairShape::InnerRight, false) => 32,
            (StairShape::InnerRight, true) => 36,
            (StairShape::OuterLeft, false) => 33,
            (StairShape::OuterLeft, true) => 37,
            (StairShape::OuterRight, false) => 34,
            (StairShape::OuterRight, true) => 38,
        }
    }

    // ========================================================================
    // DOOR HELPERS
    // ========================================================================

    /// Returns the base ID for a door type from any of its variants.
    /// Returns the ID of the lower_closed_left variant (base of each 8-variant group).
    pub fn door_type_base(model_id: u8) -> Option<u8> {
        match model_id {
            39..=46 => Some(39), // Plain doors
            67..=74 => Some(67), // Windowed doors
            75..=82 => Some(75), // Paneled doors
            83..=90 => Some(83), // Fancy doors
            91..=98 => Some(91), // Glass doors
            _ => None,
        }
    }

    /// Returns the model ID for a door of a specific type.
    /// - `base_id`: The base ID for the door type (39, 67, 75, 83, or 91)
    /// - `is_upper`: true for upper half, false for lower half
    /// - `hinge_left`: true for left hinge, false for right hinge
    /// - `is_open`: true for open, false for closed
    pub fn door_model_id_with_base(
        base_id: u8,
        is_upper: bool,
        hinge_left: bool,
        is_open: bool,
    ) -> u8 {
        // Order: lower closed left (0), lower closed right (1),
        //        upper closed left (2), upper closed right (3),
        //        lower open left (4), lower open right (5),
        //        upper open left (6), upper open right (7)
        let mut offset = 0u8;
        if is_upper {
            offset += 2;
        }
        if !hinge_left {
            offset += 1;
        }
        if is_open {
            offset += 4;
        }
        base_id + offset
    }

    /// Returns the model ID for a plain door (backwards compatibility).
    /// - `is_upper`: true for upper half, false for lower half
    /// - `hinge_left`: true for left hinge, false for right hinge
    /// - `is_open`: true for open, false for closed
    pub fn door_model_id(is_upper: bool, hinge_left: bool, is_open: bool) -> u8 {
        Self::door_model_id_with_base(39, is_upper, hinge_left, is_open)
    }

    /// Checks if a model ID is any door variant (all types).
    pub fn is_door_model(model_id: u8) -> bool {
        matches!(
            model_id,
            39..=46 | 67..=74 | 75..=82 | 83..=90 | 91..=98
        )
    }

    /// Checks if a door model is the upper half.
    pub fn is_door_upper(model_id: u8) -> bool {
        if let Some(base) = Self::door_type_base(model_id) {
            let offset = model_id - base;
            matches!(offset, 2 | 3 | 6 | 7)
        } else {
            false
        }
    }

    /// Checks if a door model is open.
    pub fn is_door_open(model_id: u8) -> bool {
        if let Some(base) = Self::door_type_base(model_id) {
            let offset = model_id - base;
            offset >= 4
        } else {
            false
        }
    }

    /// Checks if a door model has left hinge.
    pub fn is_door_hinge_left(model_id: u8) -> bool {
        if let Some(base) = Self::door_type_base(model_id) {
            let offset = model_id - base;
            matches!(offset, 0 | 2 | 4 | 6)
        } else {
            false
        }
    }

    /// Returns the toggled (open/closed) version of a door model.
    pub fn door_toggled(model_id: u8) -> u8 {
        if !Self::is_door_model(model_id) {
            return model_id;
        }
        if Self::is_door_open(model_id) {
            model_id - 4 // Open -> Closed
        } else {
            model_id + 4 // Closed -> Open
        }
    }

    /// Returns the corresponding upper or lower door half model.
    pub fn door_other_half(model_id: u8) -> u8 {
        if !Self::is_door_model(model_id) {
            return model_id;
        }
        if Self::is_door_upper(model_id) {
            model_id - 2 // Upper -> Lower
        } else {
            model_id + 2 // Lower -> Upper
        }
    }

    // ========================================================================
    // TRAPDOOR HELPERS
    // ========================================================================

    /// Returns the model ID for a trapdoor.
    /// - `is_ceiling`: true for ceiling-attached, false for floor-attached
    /// - `is_open`: true for open, false for closed
    pub fn trapdoor_model_id(is_ceiling: bool, is_open: bool) -> u8 {
        // Base: 47 (floor closed)
        // Order: floor closed (47), ceiling closed (48), floor open (49), ceiling open (50)
        let mut id = 47u8;
        if is_ceiling {
            id += 1;
        }
        if is_open {
            id += 2;
        }
        id
    }

    /// Checks if a model ID is any trapdoor variant (IDs 47-50).
    pub fn is_trapdoor_model(model_id: u8) -> bool {
        (47..=50).contains(&model_id)
    }

    /// Checks if a trapdoor model is open.
    pub fn is_trapdoor_open(model_id: u8) -> bool {
        matches!(model_id, 49 | 50)
    }

    /// Checks if a trapdoor is ceiling-attached.
    pub fn is_trapdoor_ceiling(model_id: u8) -> bool {
        matches!(model_id, 48 | 50)
    }

    /// Returns the toggled (open/closed) version of a trapdoor model.
    pub fn trapdoor_toggled(model_id: u8) -> u8 {
        if !Self::is_trapdoor_model(model_id) {
            return model_id;
        }
        if Self::is_trapdoor_open(model_id) {
            model_id - 2 // Open -> Closed
        } else {
            model_id + 2 // Closed -> Open
        }
    }

    // ========================================================================
    // WINDOW HELPERS
    // ========================================================================

    /// Returns the model ID for a window with the given connections.
    /// Connection bitmask: N=1, S=2, E=4, W=8 (same as fences).
    pub fn window_model_id(connections: u8) -> u8 {
        51 + (connections & 0x0F)
    }

    /// Checks if a model ID is any window variant (IDs 51-66).
    pub fn is_window_model(model_id: u8) -> bool {
        (51..=66).contains(&model_id)
    }

    /// Gets the connection mask from a window model ID.
    pub fn window_connections(model_id: u8) -> Option<u8> {
        if Self::is_window_model(model_id) {
            Some(model_id - 51)
        } else {
            None
        }
    }

    /// Checks if a model is a window or fence (connectable thin blocks).
    pub fn is_window_connectable(model_id: u8) -> bool {
        Self::is_window_model(model_id)
    }

    /// Checks if a model requires ground support (breaks if block below removed).
    pub fn requires_ground_support(&self, model_id: u8) -> bool {
        self.get(model_id)
            .map(|m| m.requires_ground_support)
            .unwrap_or(false)
    }

    /// Registers a model and returns its ID.
    pub fn register(&mut self, mut model: SubVoxelModel) -> u8 {
        let id = self.models.len() as u8;
        model.id = id;
        let tier = model.resolution.tier();
        self.name_to_id.insert(model.name.clone(), id);
        self.models.push(model);
        self.gpu_dirty = true;
        self.tier_dirty[tier] = true;
        id
    }

    /// Gets a model by ID.
    #[inline]
    pub fn get(&self, id: u8) -> Option<&SubVoxelModel> {
        self.models.get(id as usize)
    }

    /// Gets a model by name.
    pub fn get_by_name(&self, name: &str) -> Option<&SubVoxelModel> {
        self.name_to_id.get(name).and_then(|&id| self.get(id))
    }

    /// Gets model ID by name.
    pub fn get_id(&self, name: &str) -> Option<u8> {
        self.name_to_id.get(name).copied()
    }

    /// Returns the number of registered models.
    pub fn len(&self) -> usize {
        self.models.len()
    }

    /// Returns true if registry is empty.
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    /// Checks if a model ID is a custom (user-created) model.
    pub fn is_custom_model(model_id: u8) -> bool {
        model_id >= FIRST_CUSTOM_MODEL_ID
    }

    /// Returns an iterator over custom (user-created) models.
    ///
    /// Custom models have IDs >= FIRST_CUSTOM_MODEL_ID (39+).
    pub fn iter_custom_models(&self) -> impl Iterator<Item = &SubVoxelModel> {
        let start_id = FIRST_CUSTOM_MODEL_ID as usize;
        self.models.iter().skip(start_id)
    }

    /// Returns the number of custom models registered.
    pub fn custom_model_count(&self) -> usize {
        let start_id = FIRST_CUSTOM_MODEL_ID as usize;
        if self.models.len() > start_id {
            self.models.len() - start_id
        } else {
            0
        }
    }

    /// Updates an existing model by name, or registers it as new.
    ///
    /// Returns the model ID (existing or newly assigned).
    pub fn update_or_register(&mut self, model: SubVoxelModel) -> u8 {
        if let Some(&existing_id) = self.name_to_id.get(&model.name) {
            // Update existing model
            let mut updated = model;
            updated.id = existing_id;
            let tier = updated.resolution.tier();
            self.models[existing_id as usize] = updated;
            self.gpu_dirty = true;
            self.tier_dirty[tier] = true;
            existing_id
        } else {
            // Register as new
            self.register(model)
        }
    }

    /// Loads all models from a library directory into the registry.
    ///
    /// Returns the number of models loaded, or an error if the directory
    /// cannot be read. Individual file errors are logged but don't stop loading.
    pub fn load_library_models(&mut self, library_path: &Path) -> std::io::Result<usize> {
        use crate::storage::model_format::LibraryManager;

        if !library_path.exists() {
            return Ok(0);
        }

        let library = LibraryManager::new(library_path);
        let model_names = library.list_models()?;
        let mut loaded = 0;

        for name in model_names {
            match library.load_model(&name) {
                Ok(model) => {
                    self.register(model);
                    loaded += 1;
                }
                Err(e) => {
                    eprintln!("Warning: Failed to load library model '{}': {}", name, e);
                }
            }
        }

        Ok(loaded)
    }

    /// Returns true if GPU data needs update.
    pub fn is_gpu_dirty(&self) -> bool {
        self.gpu_dirty
    }

    /// Returns true if a specific tier needs GPU update.
    pub fn is_tier_dirty(&self, tier: usize) -> bool {
        tier < NUM_RESOLUTION_TIERS && self.tier_dirty[tier]
    }

    /// Returns true if any tier needs GPU update.
    pub fn is_any_tier_dirty(&self) -> bool {
        self.tier_dirty.iter().any(|&d| d)
    }

    /// Clears the GPU dirty flag.
    pub fn clear_gpu_dirty(&mut self) {
        self.gpu_dirty = false;
    }

    /// Clears a specific tier's dirty flag.
    pub fn clear_tier_dirty(&mut self, tier: usize) {
        if tier < NUM_RESOLUTION_TIERS {
            self.tier_dirty[tier] = false;
        }
    }

    /// Clears all tier dirty flags.
    pub fn clear_all_tier_dirty(&mut self) {
        self.tier_dirty = [false; NUM_RESOLUTION_TIERS];
    }

    /// Packs voxel data for a specific resolution tier.
    ///
    /// Models are arranged in a 16×16 grid (256 models max per tier).
    /// Atlas dimensions: (16 * res) × res × (16 * res) where res = 8, 16, or 32.
    pub fn pack_voxels_for_tier(&self, tier: usize) -> Vec<u8> {
        let res = match tier {
            0 => 8,  // Low resolution
            1 => 16, // Medium resolution
            2 => 32, // High resolution
            _ => 16, // Default to medium
        };

        let atlas_width = 16 * res;
        let atlas_height = res;
        let atlas_depth = 16 * res;
        let mut data = vec![0u8; atlas_width * atlas_height * atlas_depth];

        for (model_id, model) in self.models.iter().enumerate() {
            // Only pack models that match this tier's resolution
            if model.resolution.tier() != tier {
                continue;
            }

            let model_res = model.resolution.size();
            // Model position in the 16×16 grid
            let model_x = model_id % 16;
            let model_z = model_id / 16;

            // Copy each voxel to the correct position in the atlas
            for lz in 0..model_res {
                for ly in 0..model_res {
                    for lx in 0..model_res {
                        let src_idx = lx + ly * model_res + lz * model_res * model_res;
                        let voxel = if src_idx < model.voxels.len() {
                            model.voxels[src_idx]
                        } else {
                            0
                        };

                        let atlas_x = model_x * res + lx;
                        let atlas_y = ly;
                        let atlas_z = model_z * res + lz;
                        let dst_idx =
                            atlas_x + atlas_y * atlas_width + atlas_z * atlas_width * atlas_height;

                        if dst_idx < data.len() {
                            data[dst_idx] = voxel;
                        }
                    }
                }
            }
        }

        data
    }

    /// Legacy function - packs all models to medium resolution atlas for backward compatibility.
    /// Prefer using `pack_voxels_for_tier()` for multi-resolution support.
    pub fn pack_voxels_for_gpu(&self) -> Vec<u8> {
        // For backward compatibility, pack all models into a single medium-res atlas
        // Models with different resolutions will be resampled
        const ATLAS_WIDTH: usize = 16 * SUB_VOXEL_SIZE;
        const ATLAS_HEIGHT: usize = SUB_VOXEL_SIZE;
        const ATLAS_DEPTH: usize = 16 * SUB_VOXEL_SIZE;
        let mut data = vec![0u8; ATLAS_WIDTH * ATLAS_HEIGHT * ATLAS_DEPTH];

        for (model_id, model) in self.models.iter().enumerate() {
            let model_x = model_id % 16;
            let model_z = model_id / 16;
            let model_res = model.resolution.size();

            // Copy voxels with scaling if necessary
            for ly in 0..SUB_VOXEL_SIZE {
                for lz in 0..SUB_VOXEL_SIZE {
                    for lx in 0..SUB_VOXEL_SIZE {
                        // Scale coordinates based on model resolution
                        let src_x = lx * model_res / SUB_VOXEL_SIZE;
                        let src_y = ly * model_res / SUB_VOXEL_SIZE;
                        let src_z = lz * model_res / SUB_VOXEL_SIZE;
                        let src_idx = src_x + src_y * model_res + src_z * model_res * model_res;

                        let voxel = if src_idx < model.voxels.len() {
                            model.voxels[src_idx]
                        } else {
                            0
                        };

                        let atlas_x = model_x * SUB_VOXEL_SIZE + lx;
                        let atlas_y = ly;
                        let atlas_z = model_z * SUB_VOXEL_SIZE + lz;
                        let dst_idx =
                            atlas_x + atlas_y * ATLAS_WIDTH + atlas_z * ATLAS_WIDTH * ATLAS_HEIGHT;

                        data[dst_idx] = voxel;
                    }
                }
            }
        }

        data
    }

    /// Packs all model palettes for GPU upload as a 2D texture.
    ///
    /// The texture layout is 256×32 (model_id × palette_idx).
    /// Buffer index formula: (model_id + palette_idx * 256) * 4 bytes
    pub fn pack_palettes_for_gpu(&self) -> Vec<u8> {
        // Texture dimensions: 256 width (model_id) × 32 height (palette_idx)
        const TEX_WIDTH: usize = MAX_MODELS; // 256
        const TEX_HEIGHT: usize = PALETTE_SIZE; // 32
        let mut data = vec![0u8; TEX_WIDTH * TEX_HEIGHT * 4];

        for (model_id, model) in self.models.iter().enumerate() {
            for (palette_idx, color) in model.palette.iter().enumerate() {
                // Buffer offset for texel at (model_id, palette_idx)
                let dst_idx = (model_id + palette_idx * TEX_WIDTH) * 4;
                data[dst_idx..dst_idx + 4].copy_from_slice(&color.to_array());
            }
        }

        data
    }

    /// Packs model properties for GPU upload.
    ///
    /// Layout per model (48 bytes) matching GLSL std430:
    /// - collision_mask: u64 (8 bytes)
    /// - aabb_min: u32 (4 bytes) - packed xyz
    /// - aabb_max: u32 (4 bytes) - packed xyz
    /// - emission: vec4 (16 bytes) - RGB + intensity
    /// - flags: u32 (4 bytes) - rotatable, light_blocking, light_source, light_mode
    /// - resolution: u32 (4 bytes) - 8, 16, or 32
    /// - light_radius: f32 (4 bytes)
    /// - light_intensity: f32 (4 bytes)
    pub fn pack_properties_for_gpu(&self) -> Vec<u8> {
        const PROPS_SIZE: usize = 48;
        let mut data = vec![0u8; MAX_MODELS * PROPS_SIZE];

        for (i, model) in self.models.iter().enumerate() {
            let offset = i * PROPS_SIZE;
            let res = model.resolution.size();

            // Collision mask (8 bytes) at offset 0
            data[offset..offset + 8].copy_from_slice(&model.collision_mask.to_le_bytes());

            // Compute Fine AABB using model's actual resolution
            let mut min = [res as u8, res as u8, res as u8];
            let mut max = [0u8, 0, 0];
            // Iterate all voxels to find bounds
            for (idx, &voxel) in model.voxels.iter().enumerate() {
                if voxel != 0 {
                    let x = (idx % res) as u8;
                    let y = ((idx / res) % res) as u8;
                    let z = (idx / (res * res)) as u8;

                    if x < min[0] {
                        min[0] = x;
                    }
                    if y < min[1] {
                        min[1] = y;
                    }
                    if z < min[2] {
                        min[2] = z;
                    }

                    if x + 1 > max[0] {
                        max[0] = x + 1;
                    }
                    if y + 1 > max[1] {
                        max[1] = y + 1;
                    }
                    if z + 1 > max[2] {
                        max[2] = z + 1;
                    }
                }
            }
            // Handle empty model
            if min[0] > max[0] {
                min = [0, 0, 0];
                max = [0, 0, 0];
            }

            let aabb_min_packed =
                (min[0] as u32) | ((min[1] as u32) << 8) | ((min[2] as u32) << 16);
            let aabb_max_packed =
                (max[0] as u32) | ((max[1] as u32) << 8) | ((max[2] as u32) << 16);

            // Store AABB at offset 8
            data[offset + 8..offset + 12].copy_from_slice(&aabb_min_packed.to_le_bytes());
            data[offset + 12..offset + 16].copy_from_slice(&aabb_max_packed.to_le_bytes());

            // Emission (16 bytes as 4 floats) at offset 16
            if let Some(emission) = &model.emission {
                let r = emission.r as f32 / 255.0;
                let g = emission.g as f32 / 255.0;
                let b = emission.b as f32 / 255.0;
                let intensity = model.light_intensity;

                data[offset + 16..offset + 20].copy_from_slice(&r.to_le_bytes());
                data[offset + 20..offset + 24].copy_from_slice(&g.to_le_bytes());
                data[offset + 24..offset + 28].copy_from_slice(&b.to_le_bytes());
                data[offset + 28..offset + 32].copy_from_slice(&intensity.to_le_bytes());
            }

            // Flags (4 bytes) at offset 32
            // Bits 0: rotatable, 1-2: light_blocking, 3: is_light_source, 4-7: light_mode
            let mut flags: u32 = 0;
            if model.rotatable {
                flags |= 1;
            }
            flags |= match model.light_blocking {
                LightBlocking::None => 0,
                LightBlocking::Partial => 2,
                LightBlocking::Full => 4,
            };
            if model.is_light_source {
                flags |= 8; // bit 3
            }
            flags |= (model.light_mode as u32) << 4; // bits 4-7
            data[offset + 32..offset + 36].copy_from_slice(&flags.to_le_bytes());

            // Resolution (4 bytes) at offset 36
            data[offset + 36..offset + 40].copy_from_slice(&(res as u32).to_le_bytes());

            // Light radius (4 bytes) at offset 40
            data[offset + 40..offset + 44].copy_from_slice(&model.light_radius.to_le_bytes());

            // Light intensity (4 bytes) at offset 44 - separate from emission intensity
            data[offset + 44..offset + 48].copy_from_slice(&model.light_intensity.to_le_bytes());
        }

        data
    }

    /// Returns an iterator over all models.
    pub fn iter(&self) -> impl Iterator<Item = &SubVoxelModel> {
        self.models.iter()
    }

    /// Returns the number of built-in models.
    /// Custom models start at this ID.
    pub fn builtin_count(&self) -> u8 {
        // Built-in models are IDs 0-66 (67 total)
        FIRST_CUSTOM_MODEL_ID
    }

    /// Loads custom models from a WorldModelStore.
    /// Models are registered in order to preserve their IDs.
    pub fn load_from_store(&mut self, store: &crate::storage::model_format::WorldModelStore) {
        for (_id, model) in store.iter() {
            self.register(model);
        }
    }

    /// Creates a WorldModelStore containing all custom models.
    /// Returns None if there are no custom models.
    pub fn save_to_store(
        &self,
        author: &str,
    ) -> Option<crate::storage::model_format::WorldModelStore> {
        let first_custom = self.builtin_count();
        if self.models.len() <= first_custom as usize {
            return None;
        }

        let mut store = crate::storage::model_format::WorldModelStore::new(first_custom);
        for model in &self.models[first_custom as usize..] {
            store.add_model(model, author);
        }
        Some(store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_creation() {
        let c = Color::rgb(255, 128, 64);
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 128);
        assert_eq!(c.b, 64);
        assert_eq!(c.a, 255);

        let t = Color::transparent();
        assert_eq!(t.a, 0);
    }

    #[test]
    fn test_model_voxel_access() {
        let mut model = SubVoxelModel::new("test");

        model.set_voxel(3, 4, 5, 7);
        assert_eq!(model.get_voxel(3, 4, 5), 7);
        assert_eq!(model.get_voxel(0, 0, 0), 0);
    }

    #[test]
    fn test_model_fill_box() {
        let mut model = SubVoxelModel::new("test");

        model.fill_box(0, 0, 0, 3, 3, 3, 1);

        // Check filled region
        assert_eq!(model.get_voxel(0, 0, 0), 1);
        assert_eq!(model.get_voxel(3, 3, 3), 1);

        // Check outside region
        assert_eq!(model.get_voxel(4, 0, 0), 0);
    }

    #[test]
    fn test_collision_mask() {
        let mut model = SubVoxelModel::new("test");

        // Fill entire model
        model.fill_box(0, 0, 0, SUB_VOXEL_MAX, SUB_VOXEL_MAX, SUB_VOXEL_MAX, 1);
        model.compute_collision_mask();

        // All bits should be set (4×4×4 = 64 bits)
        assert_eq!(model.collision_mask, u64::MAX);

        // Clear model
        model.voxels = vec![0; model.resolution.volume()];
        model.compute_collision_mask();
        assert_eq!(model.collision_mask, 0);
    }

    #[test]
    fn test_point_collision() {
        let mut model = SubVoxelModel::new("test");

        // Fill bottom half only (y = 0 to half-1)
        let half = SUB_VOXEL_SIZE / 2 - 1;
        model.fill_box(0, 0, 0, SUB_VOXEL_MAX, half, SUB_VOXEL_MAX, 1);
        model.compute_collision_mask();

        // Bottom half should collide
        assert!(model.point_collides(0.5, 0.25, 0.5));

        // Top half should not collide
        assert!(!model.point_collides(0.5, 0.75, 0.5));
    }

    #[test]
    fn test_torch_model() {
        use crate::sub_voxel_builtins::create_torch;
        let torch = create_torch();

        assert_eq!(torch.name, "torch");
        assert!(torch.emission.is_some());
        assert!(!torch.rotatable);
        assert_eq!(torch.resolution, ModelResolution::Low);

        // Check stick exists at design coord (3,0,3) - native 8³
        assert_ne!(torch.get_voxel(3, 0, 3), 0);

        // Check flame exists at design coord (3,6,3) - native 8³
        assert_ne!(torch.get_voxel(3, 6, 3), 0);
    }

    #[test]
    fn test_slab_models() {
        use crate::sub_voxel_builtins::{create_slab_bottom, create_slab_top};
        let bottom = create_slab_bottom();
        let top = create_slab_top();

        assert_eq!(bottom.resolution, ModelResolution::Low);
        assert_eq!(top.resolution, ModelResolution::Low);

        // Bottom slab: filled y=0-3, empty y=4-7 (native 8³)
        assert_ne!(bottom.get_voxel(0, 0, 0), 0);
        assert_eq!(bottom.get_voxel(0, 4, 0), 0);

        // Top slab: empty y=0-3, filled y=4-7 (native 8³)
        assert_eq!(top.get_voxel(0, 0, 0), 0);
        assert_ne!(top.get_voxel(0, 4, 0), 0);
    }

    #[test]
    fn test_registry_builtins() {
        let registry = ModelRegistry::new();

        // Check built-in models exist
        assert!(registry.get(0).is_some()); // empty
        assert!(registry.get(1).is_some()); // torch
        assert!(registry.get(2).is_some()); // slab_bottom
        assert!(registry.get(3).is_some()); // slab_top

        // Check name lookup
        assert_eq!(registry.get_id("torch"), Some(1));
        assert_eq!(registry.get_id("slab_bottom"), Some(2));
    }

    #[test]
    fn test_registry_gpu_packing() {
        let registry = ModelRegistry::new();

        let voxels = registry.pack_voxels_for_gpu();
        assert_eq!(voxels.len(), MAX_MODELS * SUB_VOXEL_VOLUME);

        let palettes = registry.pack_palettes_for_gpu();
        assert_eq!(palettes.len(), MAX_MODELS * PALETTE_SIZE * 4);

        let props = registry.pack_properties_for_gpu();
        assert_eq!(props.len(), MAX_MODELS * 48);
    }
}

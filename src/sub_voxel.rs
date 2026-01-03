//! Sub-voxel model system for detailed 8³ block models.
//!
//! Models (torch, slab, fence, stairs, etc.) are stored once in a registry and
//! referenced by blocks. Each model is an 8×8×8 voxel grid with a 16-color palette.
//!
//! Memory efficiency:
//! - Registry footprint: ~256KB fixed for up to 256 models
//! - Sparse metadata: only Model blocks store model_id + rotation

#![allow(dead_code)] // Some editor-facing APIs are still planned; rendering/interaction use this today.

use nalgebra::Vector3;
use std::collections::HashMap;

/// Resolution of sub-voxel models (8×8×8).
pub const SUB_VOXEL_SIZE: usize = 8;

/// Total voxels per model (8³ = 512).
pub const SUB_VOXEL_VOLUME: usize = SUB_VOXEL_SIZE * SUB_VOXEL_SIZE * SUB_VOXEL_SIZE;

/// Maximum unique models in registry.
pub const MAX_MODELS: usize = 256;

/// Colors per model palette.
pub const PALETTE_SIZE: usize = 16;

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
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum LightBlocking {
    /// Doesn't block light (air-like).
    #[default]
    None,
    /// Partially blocks light (leaves-like).
    Partial,
    /// Fully blocks light (solid).
    Full,
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
/// Models are 8×8×8 voxel grids where each voxel is a palette index (0 = air).
/// The 16-color palette allows for efficient storage while providing enough
/// variety for detailed models.
#[derive(Debug, Clone)]
pub struct SubVoxelModel {
    /// Model ID (assigned by registry).
    pub id: u8,

    /// Human-readable name for debugging and editor.
    pub name: String,

    /// 8×8×8 voxel grid as palette indices (0 = air/transparent).
    /// Index = x + y * 8 + z * 64.
    pub voxels: [u8; SUB_VOXEL_VOLUME],

    /// 16-color palette for this model.
    /// Index 0 is always transparent (air).
    pub palette: [Color; PALETTE_SIZE],

    /// 4×4×4 collision bitmask (64 bits).
    /// Each bit represents a 2×2×2 region of the model.
    /// Bit index = cx + cy * 4 + cz * 16 where cx,cy,cz in 0..4.
    pub collision_mask: u64,

    /// How this model blocks light.
    pub light_blocking: LightBlocking,

    /// Whether this model can be rotated (90° increments around Y).
    pub rotatable: bool,

    /// Light emission color (None = doesn't emit light).
    pub emission: Option<Color>,

    /// Whether this model requires ground support (breaks if block below is removed).
    pub requires_ground_support: bool,
}

impl Default for SubVoxelModel {
    fn default() -> Self {
        let mut palette = [Color::transparent(); PALETTE_SIZE];
        palette[0] = Color::transparent(); // Index 0 is always air

        Self {
            id: 0,
            name: String::new(),
            voxels: [0; SUB_VOXEL_VOLUME],
            palette,
            collision_mask: 0,
            light_blocking: LightBlocking::None,
            rotatable: false,
            emission: None,
            requires_ground_support: false,
        }
    }
}

impl SubVoxelModel {
    /// Creates a new empty model with the given name.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            ..Default::default()
        }
    }

    /// Gets voxel palette index at (x, y, z).
    #[inline]
    pub fn get_voxel(&self, x: usize, y: usize, z: usize) -> u8 {
        debug_assert!(x < SUB_VOXEL_SIZE && y < SUB_VOXEL_SIZE && z < SUB_VOXEL_SIZE);
        self.voxels[x + y * SUB_VOXEL_SIZE + z * SUB_VOXEL_SIZE * SUB_VOXEL_SIZE]
    }

    /// Sets voxel palette index at (x, y, z).
    #[inline]
    pub fn set_voxel(&mut self, x: usize, y: usize, z: usize, palette_idx: u8) {
        debug_assert!(x < SUB_VOXEL_SIZE && y < SUB_VOXEL_SIZE && z < SUB_VOXEL_SIZE);
        debug_assert!((palette_idx as usize) < PALETTE_SIZE);
        self.voxels[x + y * SUB_VOXEL_SIZE + z * SUB_VOXEL_SIZE * SUB_VOXEL_SIZE] = palette_idx;
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
        for z in z0..=z1.min(SUB_VOXEL_SIZE - 1) {
            for y in y0..=y1.min(SUB_VOXEL_SIZE - 1) {
                for x in x0..=x1.min(SUB_VOXEL_SIZE - 1) {
                    self.set_voxel(x, y, z, palette_idx);
                }
            }
        }
    }

    /// Computes the 4×4×4 collision mask from the voxel data.
    ///
    /// Each bit in the 64-bit mask represents a 2×2×2 region.
    /// A bit is set if ANY voxel in that region is solid (non-zero).
    pub fn compute_collision_mask(&mut self) {
        self.collision_mask = 0;

        for cz in 0..4 {
            for cy in 0..4 {
                for cx in 0..4 {
                    let mut has_solid = false;

                    // Check 2×2×2 region
                    'region: for dz in 0..2 {
                        for dy in 0..2 {
                            for dx in 0..2 {
                                let vx = cx * 2 + dx;
                                let vy = cy * 2 + dy;
                                let vz = cz * 2 + dz;

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
        fn rotate_pos(pos: Vector3<i32>, rotation: u8) -> Vector3<i32> {
            let cx = 4;
            let px = pos.x - cx;
            let pz = pos.z - cx;
            match rotation & 3 {
                1 => Vector3::new(cx - pz - 1, pos.y, cx + px),
                2 => Vector3::new(cx - px - 1, pos.y, cx - pz - 1),
                3 => Vector3::new(cx + pz, pos.y, cx - px - 1),
                _ => pos,
            }
        }

        fn inverse_rotate_normal(n: Vector3<i32>, rotation: u8) -> Vector3<i32> {
            match rotation & 3 {
                1 => Vector3::new(-n.z, n.y, n.x),
                2 => Vector3::new(-n.x, n.y, -n.z),
                3 => Vector3::new(n.z, n.y, -n.x),
                _ => n,
            }
        }

        // Scale to sub-voxel coordinates (0-8)
        let pos = origin * SUB_VOXEL_SIZE as f32;

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

        // Calculate entry/exit t for the 0-8 cube
        let t_min_v = (Vector3::new(-0.001, -0.001, -0.001) - pos).component_mul(&inv_dir);
        let t_max_v = (Vector3::new(8.001, 8.001, 8.001) - pos).component_mul(&inv_dir);

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
        current_pos = current_pos.map(|v| v.clamp(0.001, 7.999));

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

        for i in 0..24 {
            if voxel.x < 0
                || voxel.x >= 8
                || voxel.y < 0
                || voxel.y >= 8
                || voxel.z < 0
                || voxel.z >= 8
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
                let t = (start_t + voxel_dist) / 8.0;
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

    /// Packs palette for GPU upload (64 bytes = 16 × RGBA).
    pub fn pack_palette(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(PALETTE_SIZE * 4);
        for color in &self.palette {
            data.extend_from_slice(&color.to_array());
        }
        data
    }
}

/// Global registry of all sub-voxel models.
///
/// Models are registered at startup and referenced by ID in block metadata.
/// The registry provides efficient GPU data packing for shader access.
pub struct ModelRegistry {
    /// All registered models (index = model_id).
    models: Vec<SubVoxelModel>,

    /// Lookup by name for editor/tools.
    name_to_id: HashMap<String, u8>,

    /// Whether GPU buffers need update.
    gpu_dirty: bool,
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
        };

        // Register built-in models
        registry.register_builtins();
        registry
    }

    /// Registers built-in models.
    fn register_builtins(&mut self) {
        use crate::sub_voxel_builtins::*;

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
        self.name_to_id.insert(model.name.clone(), id);
        self.models.push(model);
        self.gpu_dirty = true;
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

    /// Returns true if GPU data needs update.
    pub fn is_gpu_dirty(&self) -> bool {
        self.gpu_dirty
    }

    /// Clears the GPU dirty flag.
    pub fn clear_gpu_dirty(&mut self) {
        self.gpu_dirty = false;
    }

    /// Packs all model voxel data for GPU upload as a 3D texture.
    ///
    /// The texture layout is 128×8×128 (16 models × 8 per row, 8 height, 16 models × 8 depth).
    /// Models are arranged in a 16×16 grid, each occupying an 8×8×8 region.
    ///
    /// Buffer index formula: x + y * 128 + z * 1024
    pub fn pack_voxels_for_gpu(&self) -> Vec<u8> {
        // Atlas dimensions: 128×8×128 = 131072 bytes
        const ATLAS_WIDTH: usize = 128;
        const ATLAS_HEIGHT: usize = 8;
        const ATLAS_DEPTH: usize = 128;
        let mut data = vec![0u8; ATLAS_WIDTH * ATLAS_HEIGHT * ATLAS_DEPTH];

        for (model_id, model) in self.models.iter().enumerate() {
            // Model position in the 16×16 grid
            let model_x = model_id % 16;
            let model_z = model_id / 16;

            // Copy each voxel to the correct position in the atlas
            for lz in 0..SUB_VOXEL_SIZE {
                for ly in 0..SUB_VOXEL_SIZE {
                    for lx in 0..SUB_VOXEL_SIZE {
                        // Source: model.voxels indexed as x + y*8 + z*64
                        let src_idx =
                            lx + ly * SUB_VOXEL_SIZE + lz * SUB_VOXEL_SIZE * SUB_VOXEL_SIZE;
                        let voxel = model.voxels[src_idx];

                        // Destination: atlas indexed as x + y*128 + z*1024
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
    /// The texture layout is 256×16 (model_id × palette_idx).
    /// Buffer index formula: (model_id + palette_idx * 256) * 4 bytes
    pub fn pack_palettes_for_gpu(&self) -> Vec<u8> {
        // Texture dimensions: 256 width (model_id) × 16 height (palette_idx)
        const TEX_WIDTH: usize = MAX_MODELS; // 256
        const TEX_HEIGHT: usize = PALETTE_SIZE; // 16
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
    /// - padding: 8 bytes (aligns next vec4 to 16 bytes)
    /// - emission: vec4 (16 bytes) - RGB + intensity
    /// - flags: u32 (4 bytes) - rotatable, light_blocking
    /// - padding: 12 bytes (aligns struct to 16 bytes)
    pub fn pack_properties_for_gpu(&self) -> Vec<u8> {
        const PROPS_SIZE: usize = 48;
        let mut data = vec![0u8; MAX_MODELS * PROPS_SIZE];

        for (i, model) in self.models.iter().enumerate() {
            let offset = i * PROPS_SIZE;

            // Collision mask (8 bytes) at offset 0
            data[offset..offset + 8].copy_from_slice(&model.collision_mask.to_le_bytes());

            // Compute Fine AABB
            let mut min = [8u8, 8, 8];
            let mut max = [0u8, 0, 0];
            // Iterate all voxels to find bounds
            for (idx, &voxel) in model.voxels.iter().enumerate() {
                if voxel != 0 {
                    let x = (idx % 8) as u8;
                    let y = ((idx / 8) % 8) as u8;
                    let z = (idx / 64) as u8;

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

            // Store AABB in padding at offset 8
            data[offset + 8..offset + 12].copy_from_slice(&aabb_min_packed.to_le_bytes());
            data[offset + 12..offset + 16].copy_from_slice(&aabb_max_packed.to_le_bytes());

            // Emission (16 bytes as 4 floats) at offset 16
            if let Some(emission) = &model.emission {
                let r = emission.r as f32 / 255.0;
                let g = emission.g as f32 / 255.0;
                let b = emission.b as f32 / 255.0;
                let intensity = 1.0f32;

                data[offset + 16..offset + 20].copy_from_slice(&r.to_le_bytes());
                data[offset + 20..offset + 24].copy_from_slice(&g.to_le_bytes());
                data[offset + 24..offset + 28].copy_from_slice(&b.to_le_bytes());
                data[offset + 28..offset + 32].copy_from_slice(&intensity.to_le_bytes());
            }

            // Flags (4 bytes) at offset 32
            let mut flags: u32 = 0;
            if model.rotatable {
                flags |= 1;
            }
            flags |= match model.light_blocking {
                LightBlocking::None => 0,
                LightBlocking::Partial => 2,
                LightBlocking::Full => 4,
            };
            data[offset + 32..offset + 36].copy_from_slice(&flags.to_le_bytes());

            // Padding (12 bytes) at offset 36 - already zeros
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
        // Built-in models are IDs 0-38 (39 total)
        39
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
        model.fill_box(0, 0, 0, 7, 7, 7, 1);
        model.compute_collision_mask();

        // All bits should be set (4×4×4 = 64 bits)
        assert_eq!(model.collision_mask, u64::MAX);

        // Clear model
        model.voxels = [0; SUB_VOXEL_VOLUME];
        model.compute_collision_mask();
        assert_eq!(model.collision_mask, 0);
    }

    #[test]
    fn test_point_collision() {
        let mut model = SubVoxelModel::new("test");

        // Fill bottom half only
        model.fill_box(0, 0, 0, 7, 3, 7, 1);
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

        // Check stick exists
        assert_ne!(torch.get_voxel(3, 0, 3), 0);

        // Check flame exists
        assert_ne!(torch.get_voxel(3, 6, 3), 0);
    }

    #[test]
    fn test_slab_models() {
        use crate::sub_voxel_builtins::{create_slab_bottom, create_slab_top};
        let bottom = create_slab_bottom();
        let top = create_slab_top();

        // Bottom slab: filled 0-3, empty 4-7
        assert_ne!(bottom.get_voxel(0, 0, 0), 0);
        assert_eq!(bottom.get_voxel(0, 4, 0), 0);

        // Top slab: empty 0-3, filled 4-7
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

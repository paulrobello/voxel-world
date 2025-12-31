//! Sub-voxel model system for detailed 8³ block models.
//!
//! This module implements a model registry pattern where reusable sub-voxel models
//! (torch, slab, fence, stairs, etc.) are stored once and referenced by blocks.
//! Each model is an 8×8×8 voxel grid with a 16-color palette.
//!
//! Memory efficiency is achieved through:
//! - Model registry: ~256KB fixed for up to 256 unique models
//! - Sparse block metadata: Only blocks with models store model_id + rotation

#![allow(dead_code)] // Module under construction - will be integrated in Phase 4.2+

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

    /// Creates an empty model (placeholder, id 0).
    pub fn empty() -> Self {
        Self::new("empty")
    }

    /// Creates a torch model with stick and flame.
    pub fn torch() -> Self {
        let mut model = Self::new("torch");

        // Palette
        model.palette[1] = Color::rgb(101, 67, 33); // Dark wood brown
        model.palette[2] = Color::rgb(139, 90, 43); // Wood brown
        model.palette[3] = Color::rgb(255, 200, 50); // Flame yellow
        model.palette[4] = Color::rgb(255, 100, 20); // Flame orange

        // Stick (center, bottom 5 voxels) - 2×2 cross-section
        for y in 0..5 {
            model.set_voxel(3, y, 3, 1);
            model.set_voxel(4, y, 3, 2);
            model.set_voxel(3, y, 4, 2);
            model.set_voxel(4, y, 4, 1);
        }

        // Flame core (voxels 5-7)
        for y in 5..8 {
            for dx in 3..5 {
                for dz in 3..5 {
                    model.set_voxel(dx, y, dz, 3);
                }
            }
        }

        // Flame outer (y=5,6 expanded)
        for y in 5..7 {
            model.set_voxel(2, y, 3, 4);
            model.set_voxel(5, y, 3, 4);
            model.set_voxel(3, y, 2, 4);
            model.set_voxel(3, y, 5, 4);
            model.set_voxel(4, y, 2, 4);
            model.set_voxel(4, y, 5, 4);
            model.set_voxel(2, y, 4, 4);
            model.set_voxel(5, y, 4, 4);
        }

        model.emission = Some(Color::rgb(255, 180, 80));
        // Open fence lets some light through but should cast soft shadows
        model.light_blocking = LightBlocking::Partial;
        model.rotatable = false;
        model.requires_ground_support = true;

        model.compute_collision_mask();
        model
    }

    /// Creates a bottom slab (half-block on bottom).
    pub fn slab_bottom() -> Self {
        let mut model = Self::new("slab_bottom");

        model.palette[1] = Color::rgb(128, 128, 128); // Stone gray

        // Fill bottom half (Y 0-3)
        model.fill_box(0, 0, 0, 7, 3, 7, 1);

        model.light_blocking = LightBlocking::Full;
        model.rotatable = false;

        model.compute_collision_mask();
        model
    }

    /// Creates a top slab (half-block on top).
    pub fn slab_top() -> Self {
        let mut model = Self::new("slab_top");

        model.palette[1] = Color::rgb(128, 128, 128); // Stone gray

        // Fill top half (Y 4-7)
        model.fill_box(0, 4, 0, 7, 7, 7, 1);

        model.light_blocking = LightBlocking::Full;
        model.rotatable = false;

        model.compute_collision_mask();
        model
    }

    /// Creates a fence with the specified connection mask.
    ///
    /// Connection bitmask:
    /// - Bit 0 (1): North (-Z)
    /// - Bit 1 (2): South (+Z)
    /// - Bit 2 (4): East (+X)
    /// - Bit 3 (8): West (-X)
    pub fn fence(connections: u8) -> Self {
        let name = format!("fence_{}", connections);
        let mut model = Self::new(&name);

        model.palette[1] = Color::rgb(139, 90, 43); // Wood brown (post)
        model.palette[2] = Color::rgb(160, 110, 60); // Lighter brown (rails)

        // Center post (2×8×2 at center)
        model.fill_box(3, 0, 3, 4, 7, 4, 1);

        // Add rails based on connections
        // Rails are at Y=2-3 (lower) and Y=5-6 (upper)
        let rail_y_ranges = [(2, 3), (5, 6)];

        for &(y0, y1) in &rail_y_ranges {
            // North rail (-Z direction)
            if connections & 1 != 0 {
                model.fill_box(3, y0, 0, 4, y1, 2, 2);
            }
            // South rail (+Z direction)
            if connections & 2 != 0 {
                model.fill_box(3, y0, 5, 4, y1, 7, 2);
            }
            // East rail (+X direction)
            if connections & 4 != 0 {
                model.fill_box(5, y0, 3, 7, y1, 4, 2);
            }
            // West rail (-X direction)
            if connections & 8 != 0 {
                model.fill_box(0, y0, 3, 2, y1, 4, 2);
            }
        }

        // Fences should cast soft shadows while still letting light through gaps
        model.light_blocking = LightBlocking::Partial;
        model.rotatable = false;
        model.requires_ground_support = true;

        model.compute_collision_mask();
        model
    }

    /// Creates a fence post with no connections (convenience alias).
    pub fn fence_post() -> Self {
        Self::fence(0)
    }

    /// Creates a fence gate with connection mask.
    ///
    /// Connection bitmask (for gate spanning X axis, facing north):
    /// - Bit 0 (1): West side (-X)
    /// - Bit 1 (2): East side (+X)
    pub fn gate_closed_with_connections(_connections: u8) -> Self {
        // Note: connections parameter kept for API compatibility but gates no longer
        // have connection rails - adjacent fences connect to the gate posts directly
        let name = format!("gate_closed_{}", _connections);
        let mut model = Self::new(&name);

        model.palette[1] = Color::rgb(139, 90, 43); // Wood brown (posts)
        model.palette[2] = Color::rgb(160, 110, 60); // Lighter brown (door)
        model.palette[3] = Color::rgb(60, 60, 65); // Iron gray (hardware)

        // Fixed gate posts on sides (at X=0-1 and X=6-7, centered at Z=3-4)
        // These posts NEVER move - they are the hinge points
        model.fill_box(0, 0, 3, 1, 7, 4, 1);
        model.fill_box(6, 0, 3, 7, 7, 4, 1);

        // Door panels (closed position - spanning between posts)
        // Left door panel attached to left post
        model.fill_box(2, 2, 3, 3, 3, 4, 2); // Lower rail
        model.fill_box(2, 5, 3, 3, 6, 4, 2); // Upper rail
        model.fill_box(3, 4, 3, 3, 4, 4, 2); // Middle bar

        // Right door panel attached to right post
        model.fill_box(4, 2, 3, 5, 3, 4, 2); // Lower rail
        model.fill_box(4, 5, 3, 5, 6, 4, 2); // Upper rail
        model.fill_box(4, 4, 3, 4, 4, 4, 2); // Middle bar

        // Iron hinges on posts
        model.set_voxel(1, 3, 2, 3); // Left post lower hinge
        model.set_voxel(1, 5, 2, 3); // Left post upper hinge
        model.set_voxel(6, 3, 2, 3); // Right post lower hinge
        model.set_voxel(6, 5, 2, 3); // Right post upper hinge

        // Iron latch where doors meet in middle
        model.set_voxel(3, 4, 2, 3); // Latch left side
        model.set_voxel(4, 4, 2, 3); // Latch right side

        // Gate posts/rails should cast soft shadows
        model.light_blocking = LightBlocking::Partial;
        model.rotatable = true;
        model.requires_ground_support = true;

        model.compute_collision_mask();
        model
    }

    /// Creates an open fence gate with connection mask.
    pub fn gate_open_with_connections(_connections: u8) -> Self {
        // Note: connections parameter kept for API compatibility but gates no longer
        // have connection rails - adjacent fences connect to the gate posts directly
        let name = format!("gate_open_{}", _connections);
        let mut model = Self::new(&name);

        model.palette[1] = Color::rgb(139, 90, 43); // Wood brown (posts)
        model.palette[2] = Color::rgb(160, 110, 60); // Lighter brown (door)
        model.palette[3] = Color::rgb(60, 60, 65); // Iron gray (hardware)

        // Fixed gate posts on sides (SAME position as closed gate!)
        // These posts NEVER move - they are the hinge points
        model.fill_box(0, 0, 3, 1, 7, 4, 1);
        model.fill_box(6, 0, 3, 7, 7, 4, 1);

        // Door panels (open position - swung toward -Z/front)
        // Left door swings from left post toward front
        model.fill_box(0, 2, 0, 1, 3, 2, 2); // Lower rail
        model.fill_box(0, 5, 0, 1, 6, 2, 2); // Upper rail
        model.fill_box(0, 4, 0, 1, 4, 0, 2); // End bar

        // Right door swings from right post toward front
        model.fill_box(6, 2, 0, 7, 3, 2, 2); // Lower rail
        model.fill_box(6, 5, 0, 7, 6, 2, 2); // Upper rail
        model.fill_box(6, 4, 0, 7, 4, 0, 2); // End bar

        // Iron hinges on posts (same position as closed)
        model.set_voxel(1, 3, 2, 3); // Left post lower hinge
        model.set_voxel(1, 5, 2, 3); // Left post upper hinge
        model.set_voxel(6, 3, 2, 3); // Right post lower hinge
        model.set_voxel(6, 5, 2, 3); // Right post upper hinge

        model.light_blocking = LightBlocking::Partial;
        model.rotatable = true;
        model.requires_ground_support = true;

        model.compute_collision_mask();
        model
    }

    /// Creates a closed fence gate (no connections).
    pub fn gate_closed() -> Self {
        Self::gate_closed_with_connections(0)
    }

    /// Creates an open fence gate (no connections).
    pub fn gate_open() -> Self {
        Self::gate_open_with_connections(0)
    }

    /// Creates stairs facing north (step in back/+Z).
    pub fn stairs_north() -> Self {
        let mut model = Self::new("stairs_north");

        model.palette[1] = Color::rgb(128, 128, 128); // Stone gray

        // Bottom slab (full width, Y 0-3)
        model.fill_box(0, 0, 0, 7, 3, 7, 1);

        // Top step (back half, Y 4-7, Z 4-7)
        model.fill_box(0, 4, 4, 7, 7, 7, 1);

        model.light_blocking = LightBlocking::Partial;
        model.rotatable = true;

        model.compute_collision_mask();
        model
    }

    /// Creates a ladder (thin vertical rungs against wall).
    pub fn ladder() -> Self {
        let mut model = Self::new("ladder");

        model.palette[1] = Color::rgb(139, 90, 43); // Wood brown

        // Vertical rails on sides (at Z=7, against wall)
        for y in 0..8 {
            model.set_voxel(1, y, 7, 1);
            model.set_voxel(6, y, 7, 1);
        }

        // Horizontal rungs (thin, only at Z=7)
        for y in [1, 3, 5, 7] {
            for x in 2..6 {
                model.set_voxel(x, y, 7, 1);
            }
        }

        // Let ladders participate in sunlight/shadow (but still allow light through gaps)
        model.light_blocking = LightBlocking::Partial;
        model.rotatable = true;
        model.requires_ground_support = true;

        // Compute collision from voxels - only the back half (where rails/rungs are) blocks
        // The front half is empty and walkable
        model.compute_collision_mask();
        model
    }

    /// Packs voxel data for GPU upload (512 bytes).
    pub fn pack_voxels(&self) -> Vec<u8> {
        self.voxels.to_vec()
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
        // ID 0: Empty/placeholder (no model)
        self.register(SubVoxelModel::empty());

        // ID 1: Torch
        self.register(SubVoxelModel::torch());

        // ID 2-3: Slabs
        self.register(SubVoxelModel::slab_bottom());
        self.register(SubVoxelModel::slab_top());

        // ID 4-19: Fence variants (16 connection combinations)
        // Connection bitmask: N=1, S=2, E=4, W=8
        for connections in 0..16u8 {
            self.register(SubVoxelModel::fence(connections));
        }

        // ID 20-23: Closed gate variants (4 connection combinations)
        // Connection bitmask: W=1, E=2
        for connections in 0..4u8 {
            self.register(SubVoxelModel::gate_closed_with_connections(connections));
        }

        // ID 24-27: Open gate variants (4 connection combinations)
        for connections in 0..4u8 {
            self.register(SubVoxelModel::gate_open_with_connections(connections));
        }

        // ID 28: Stairs
        self.register(SubVoxelModel::stairs_north());

        // ID 29: Ladder
        self.register(SubVoxelModel::ladder());
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
    /// Layout per model (32 bytes):
    /// - collision_mask: u64 (8 bytes)
    /// - emission: vec4 (16 bytes) - RGB + intensity, or zeros
    /// - flags: u32 (4 bytes) - rotatable, light_blocking
    /// - padding: 4 bytes
    pub fn pack_properties_for_gpu(&self) -> Vec<u8> {
        const PROPS_SIZE: usize = 32;
        let mut data = vec![0u8; MAX_MODELS * PROPS_SIZE];

        for (i, model) in self.models.iter().enumerate() {
            let offset = i * PROPS_SIZE;

            // Collision mask (8 bytes)
            data[offset..offset + 8].copy_from_slice(&model.collision_mask.to_le_bytes());

            // Emission (16 bytes as 4 floats)
            if let Some(emission) = &model.emission {
                let r = emission.r as f32 / 255.0;
                let g = emission.g as f32 / 255.0;
                let b = emission.b as f32 / 255.0;
                let intensity = 1.0f32;

                data[offset + 8..offset + 12].copy_from_slice(&r.to_le_bytes());
                data[offset + 12..offset + 16].copy_from_slice(&g.to_le_bytes());
                data[offset + 16..offset + 20].copy_from_slice(&b.to_le_bytes());
                data[offset + 20..offset + 24].copy_from_slice(&intensity.to_le_bytes());
            }

            // Flags (4 bytes)
            let mut flags: u32 = 0;
            if model.rotatable {
                flags |= 1;
            }
            flags |= match model.light_blocking {
                LightBlocking::None => 0,
                LightBlocking::Partial => 2,
                LightBlocking::Full => 4,
            };
            data[offset + 24..offset + 28].copy_from_slice(&flags.to_le_bytes());

            // Padding (4 bytes) - already zeros
        }

        data
    }

    /// Returns an iterator over all models.
    pub fn iter(&self) -> impl Iterator<Item = &SubVoxelModel> {
        self.models.iter()
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
        let torch = SubVoxelModel::torch();

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
        let bottom = SubVoxelModel::slab_bottom();
        let top = SubVoxelModel::slab_top();

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
        assert_eq!(props.len(), MAX_MODELS * 32);
    }
}

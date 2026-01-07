use super::builtins;
use super::model::SubVoxelModel;
use super::types::{
    FIRST_CUSTOM_MODEL_ID, LightBlocking, MAX_MODELS, ModelResolution, NUM_RESOLUTION_TIERS,
    StairShape,
};
use std::collections::HashMap;
use std::path::Path;

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
        builtins::register_builtins(&mut registry);
        registry
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

    /// Legacy: Pack all models into a single 16³ atlas.
    /// Resamples 8³ models up and 32³ models down.
    pub fn pack_voxels_for_gpu(&self) -> Vec<u8> {
        let res = 16; // Standard resolution
        let atlas_width = 16 * res;
        let atlas_height = res;
        let atlas_depth = 16 * res;
        let mut data = vec![0u8; atlas_width * atlas_height * atlas_depth];

        println!("[DEBUG] Packing {} models for GPU atlas", self.models.len());

        for (model_id, model) in self.models.iter().enumerate() {
            // Model position in the 16×16 grid
            let model_x = model_id % 16;
            let model_z = model_id / 16;

            // We need a 16³ version of the model
            let model_16 = if model.resolution == ModelResolution::Medium {
                // Already 16³ - borrow
                std::borrow::Cow::Borrowed(model)
            } else if model.resolution == ModelResolution::Low {
                // Upscale 8 -> 16
                if let Some(upscaled) = model.upscale(ModelResolution::Medium) {
                    std::borrow::Cow::Owned(upscaled)
                } else {
                    std::borrow::Cow::Borrowed(model) // Should not happen
                }
            } else {
                // Downscale 32 -> 16
                if let Some(downscaled) = model.downscale(ModelResolution::Medium) {
                    std::borrow::Cow::Owned(downscaled)
                } else {
                    std::borrow::Cow::Borrowed(model) // Should not happen
                }
            };

            let model_res = 16;
            // Copy each voxel to the correct position in the atlas
            let mut non_zero_count = 0;
            for lz in 0..model_res {
                for ly in 0..model_res {
                    for lx in 0..model_res {
                        let src_idx = lx + ly * model_res + lz * model_res * model_res;
                        let voxel = if src_idx < model_16.voxels.len() {
                            model_16.voxels[src_idx]
                        } else {
                            0
                        };

                        if voxel != 0 {
                            non_zero_count += 1;
                        }

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

            if model_id == 1 {
                println!(
                    "[DEBUG] Model ID 1 (torch): packed {} non-zero voxels from {:?} resolution",
                    non_zero_count, model.resolution
                );
            }
        }
        data
    }

    /// Packs palettes for all models.
    pub fn pack_palettes_for_gpu(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(self.models.len() * 32 * 5);
        for model in &self.models {
            data.extend(model.pack_palette_combined());
        }
        // Pad if we have fewer than MAX_MODELS (though usually we resize buffer on GPU side)
        // But for safety, let's ensure we match what expected
        while data.len() < MAX_MODELS * 32 * 5 {
            data.push(0);
        }
        data
    }

    /// Packs properties for all models.
    pub fn pack_properties_for_gpu(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(self.models.len() * 48);
        for model in &self.models {
            // collision_mask (8 bytes)
            data.extend_from_slice(&model.collision_mask.to_le_bytes());

            // aabb (8 bytes) - Mock placeholder for now as we don't store it
            // 0,0,0 to size,size,size
            let min_u32 = 0u32;
            let _max_u32 = 0u32; // TODO: Encode AABB properly if needed
            data.extend_from_slice(&min_u32.to_le_bytes()); // 4
            data.extend_from_slice(&min_u32.to_le_bytes()); // 8 (Wait, should be 2x u32 for min, 2x u32 for max? No, AABB usually 3 floats or packed. 
            // In GPU resources, it unpacked 4 bytes for min and 4 bytes for max.
            // "props.aabb_min = u32::from_le_bytes([chunk[8]..chunk[11]]);"
            // So AABB is indeed 4 bytes min, 4 bytes max. 8 bytes total.
            // My previous write added 4 extend calls (16 bytes). I need only 2.

            // emission (16 bytes)
            if let Some(c) = model.emission {
                data.extend_from_slice(&(c.r as f32 / 255.0).to_le_bytes());
                data.extend_from_slice(&(c.g as f32 / 255.0).to_le_bytes());
                data.extend_from_slice(&(c.b as f32 / 255.0).to_le_bytes());
                data.extend_from_slice(&1.0f32.to_le_bytes()); // Alpha/Intensity?
            } else {
                data.extend_from_slice(&0.0f32.to_le_bytes());
                data.extend_from_slice(&0.0f32.to_le_bytes());
                data.extend_from_slice(&0.0f32.to_le_bytes());
                data.extend_from_slice(&0.0f32.to_le_bytes());
            }

            // flags (4 bytes)
            let mut flags = 0u32;
            if model.light_blocking != LightBlocking::None {
                flags |= 1; // Blocks light
            }
            if !model.no_collision {
                flags |= 2; // Has collision
            }
            data.extend_from_slice(&flags.to_le_bytes());

            // resolution (4 bytes)
            data.extend_from_slice(&(model.resolution.size() as u32).to_le_bytes());

            // light_radius (4 bytes)
            data.extend_from_slice(&model.light_radius.to_le_bytes());

            // light_intensity (4 bytes)
            data.extend_from_slice(&model.light_intensity.to_le_bytes());
        }

        // Pad
        while data.len() < MAX_MODELS * 48 {
            data.push(0);
        }

        data
    }

    // ========================================================================
    // MODEL ID HELPERS
    // ========================================================================

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
}

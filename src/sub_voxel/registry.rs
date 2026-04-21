use super::builtins;
use super::model::SubVoxelModel;
use super::types::{
    Color, FIRST_CUSTOM_MODEL_ID, LightBlocking, MAX_MODELS, ModelResolution, NUM_RESOLUTION_TIERS,
    PALETTE_SIZE, SimpleDoorPair, StairShape,
};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Maximum number of custom door pairs.
pub const MAX_CUSTOM_DOOR_PAIRS: usize = 64;

/// Byte-size of a palette key (palette bytes + emission bit-pattern bytes).
const PALETTE_KEY_BYTES: usize = PALETTE_SIZE * 4 + PALETTE_SIZE * 4;

/// Packs model flags into the 32-bit `flags` field of `GpuModelProperties`.
///
/// Bit layout (must match `shaders/common.glsl::ModelProperties::flags`):
/// - bit 0: rotatable
/// - bits 1-2: light_blocking (0=None, 1=Partial, 2=Full)
/// - bit 3: is_light_source
/// - bits 4-7: light_mode
/// - bit 8: is_ground_cover
/// - bits 16-23: palette_id (0..=255)
fn pack_model_flags(model: &SubVoxelModel, palette_id: u8) -> u32 {
    let mut flags = 0u32;
    if model.rotatable {
        flags |= 1;
    }
    flags |= match model.light_blocking {
        LightBlocking::None => 0,
        LightBlocking::Partial => 2,
        LightBlocking::Full => 4,
    };
    if model.is_light_source {
        flags |= 8;
    }
    flags |= (model.light_mode as u32) << 4;
    if model.is_ground_cover {
        flags |= 256;
    }
    flags |= (palette_id as u32) << 16;
    flags
}

/// A deduplicated palette + emission pair in the shared `PaletteTable`.
#[derive(Debug, Clone)]
struct PaletteEntry {
    palette: [Color; PALETTE_SIZE],
    emission: [f32; PALETTE_SIZE],
}

/// Shared palette atlas — deduplicates `(palette, palette_emission)` pairs across models.
///
/// Each unique pair is stored once and assigned a `palette_id` (0..=255). Models store a
/// `palette_id` into this table instead of their own 256 byte palette/emission arrays.
/// Built-in model families (fences, stairs, doors) share a handful of palettes, yielding
/// large GPU memory savings.
///
/// Orphaned entries (ref_count == 0) are reclaimed on the next insert that does not find
/// an existing match. No compaction of in-use entries ever occurs, so `palette_id`s are
/// stable for the lifetime of a model reference.
#[derive(Debug, Default)]
pub(crate) struct PaletteTable {
    entries: Vec<PaletteEntry>,
    key_to_id: HashMap<[u8; PALETTE_KEY_BYTES], u8>,
    ref_count: Vec<u32>,
}

impl PaletteTable {
    fn make_key(
        palette: &[Color; PALETTE_SIZE],
        emission: &[f32; PALETTE_SIZE],
    ) -> [u8; PALETTE_KEY_BYTES] {
        let mut key = [0u8; PALETTE_KEY_BYTES];
        for (i, c) in palette.iter().enumerate() {
            let o = i * 4;
            key[o..o + 4].copy_from_slice(&c.to_array());
        }
        let offset = PALETTE_SIZE * 4;
        for (i, e) in emission.iter().enumerate() {
            let o = offset + i * 4;
            key[o..o + 4].copy_from_slice(&e.to_bits().to_le_bytes());
        }
        key
    }

    /// Interns a (palette, emission) pair, returning `(palette_id, newly_allocated)`.
    ///
    /// If the pair already exists, its ref count is incremented and `newly_allocated` is
    /// `false`. If new, an orphaned slot is reclaimed when available or a new slot is
    /// allocated. Returns `None` if no slot is available (table is full with all entries
    /// referenced — should not happen given MAX_MODELS = 256 entries, since there can be
    /// at most 256 models referencing at most 256 distinct palettes).
    fn intern(
        &mut self,
        palette: &[Color; PALETTE_SIZE],
        emission: &[f32; PALETTE_SIZE],
    ) -> Option<(u8, bool)> {
        let key = Self::make_key(palette, emission);
        if let Some(&id) = self.key_to_id.get(&key) {
            let idx = id as usize;
            self.ref_count[idx] = self.ref_count[idx].saturating_add(1);
            return Some((id, false));
        }

        // Try to reclaim an orphaned slot first.
        if let Some(idx) = self.ref_count.iter().position(|&c| c == 0)
            && idx < self.entries.len()
        {
            // Remove the old key mapping for this slot (scan; table is small).
            let old_key_opt = self
                .key_to_id
                .iter()
                .find_map(|(k, &v)| if v as usize == idx { Some(*k) } else { None });
            if let Some(old_key) = old_key_opt {
                self.key_to_id.remove(&old_key);
            }
            self.entries[idx] = PaletteEntry {
                palette: *palette,
                emission: *emission,
            };
            self.ref_count[idx] = 1;
            let id = idx as u8;
            self.key_to_id.insert(key, id);
            return Some((id, true));
        }

        // Allocate a fresh slot.
        if self.entries.len() >= MAX_MODELS {
            return None;
        }
        let id = self.entries.len() as u8;
        self.entries.push(PaletteEntry {
            palette: *palette,
            emission: *emission,
        });
        self.ref_count.push(1);
        self.key_to_id.insert(key, id);
        Some((id, true))
    }

    /// Decrements the ref count for a palette_id. Orphaned slots may be reclaimed by a
    /// later `intern` call.
    fn release(&mut self, palette_id: u8) {
        let idx = palette_id as usize;
        if let Some(rc) = self.ref_count.get_mut(idx) {
            *rc = rc.saturating_sub(1);
        }
    }

    fn get(&self, palette_id: u8) -> Option<&PaletteEntry> {
        self.entries.get(palette_id as usize)
    }

    fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Supports three resolution tiers (Low/8³, Medium/16³, High/32³).
pub struct ModelRegistry {
    /// All registered models (index = model_id).
    models: Vec<SubVoxelModel>,

    /// Lookup by name for editor/tools.
    name_to_id: HashMap<String, u8>,

    /// Whether a full GPU resync is required (set until the first upload completes).
    full_resync_needed: bool,

    /// Model IDs with pending GPU updates since the last upload.
    /// Empty set + `!full_resync_needed` means GPU is in sync.
    dirty_model_ids: HashSet<u8>,

    /// Shared deduplicated palettes. Models reference these via `model_palette_ids`.
    palette_table: PaletteTable,

    /// Per-model palette_id (indexed by model_id). Kept in sync with `models`.
    model_palette_ids: Vec<u8>,

    /// Palette IDs with pending GPU upload (palette texture column needs refresh).
    dirty_palette_ids: HashSet<u8>,

    /// Per-tier dirty flags [Low, Medium, High].
    /// Each tier's atlas is updated independently for efficiency.
    tier_dirty: [bool; NUM_RESOLUTION_TIERS],

    /// Custom door pairs (user-created doors).
    custom_door_pairs: Vec<SimpleDoorPair>,

    /// Lookup from model ID to custom door pair ID.
    model_to_door_pair: HashMap<u8, u16>,
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
            full_resync_needed: true,
            dirty_model_ids: HashSet::new(),
            palette_table: PaletteTable::default(),
            model_palette_ids: Vec::with_capacity(MAX_MODELS),
            dirty_palette_ids: HashSet::new(),
            tier_dirty: [true; NUM_RESOLUTION_TIERS], // All tiers need initial upload
            custom_door_pairs: Vec::new(),
            model_to_door_pair: HashMap::new(),
        };

        // Register built-in models
        builtins::register_builtins(&mut registry);
        registry
    }

    /// Registers a model and returns its ID.
    ///
    /// Returns `None` if the registry is full (MAX_MODELS = 256 entries).
    /// The model ID is a `u8`, so registering a 257th model would silently
    /// wrap to 0 without this check, corrupting the empty/air slot.
    #[must_use]
    pub fn register(&mut self, mut model: SubVoxelModel) -> Option<u8> {
        if self.models.len() >= MAX_MODELS {
            log::warn!(
                "[ModelRegistry] Cannot register '{}': registry full ({} / {})",
                model.name,
                self.models.len(),
                MAX_MODELS
            );
            return None;
        }
        let id = self.models.len() as u8;
        model.id = id;
        let tier = model.resolution.tier();
        let (palette_id, newly_allocated) = self
            .palette_table
            .intern(&model.palette, model.palette_emission_slice())
            .expect("PaletteTable capacity exceeded — cannot exceed MAX_MODELS distinct palettes");
        self.name_to_id.insert(model.name.clone(), id);
        self.models.push(model);
        debug_assert!(self.model_palette_ids.len() == id as usize);
        self.model_palette_ids.push(palette_id);
        if newly_allocated {
            self.dirty_palette_ids.insert(palette_id);
        }
        self.dirty_model_ids.insert(id);
        self.tier_dirty[tier] = true;
        Some(id)
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
    /// Custom models have IDs >= FIRST_CUSTOM_MODEL_ID (161+).
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
    /// Returns the model ID (existing or newly assigned), or `None` if the
    /// registry is full and the model is not already registered.
    #[must_use]
    pub fn update_or_register(&mut self, model: SubVoxelModel) -> Option<u8> {
        if let Some(&existing_id) = self.name_to_id.get(&model.name) {
            // Update existing model
            let mut updated = model;
            updated.id = existing_id;
            let tier = updated.resolution.tier();
            // Release the old palette slot first. If the new palette bytes are identical,
            // `intern` will reclaim the same slot via its key_to_id lookup and ref_count
            // is restored — no spurious dirty flag or duplicate slot.
            let old_palette_id = self.model_palette_ids[existing_id as usize];
            self.palette_table.release(old_palette_id);
            let (new_palette_id, newly_allocated) = self
                .palette_table
                .intern(&updated.palette, updated.palette_emission_slice())
                .expect("PaletteTable capacity exceeded");
            self.model_palette_ids[existing_id as usize] = new_palette_id;
            if newly_allocated {
                self.dirty_palette_ids.insert(new_palette_id);
            }
            self.models[existing_id as usize] = updated;
            self.dirty_model_ids.insert(existing_id);
            self.tier_dirty[tier] = true;
            Some(existing_id)
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
                    if self.register(model).is_some() {
                        loaded += 1;
                    }
                }
                Err(e) => {
                    log::warn!("Warning: Failed to load library model '{}': {}", name, e);
                }
            }
        }

        Ok(loaded)
    }

    /// Returns true if GPU data needs update (full resync or any dirty model/palette).
    pub fn is_gpu_dirty(&self) -> bool {
        self.full_resync_needed
            || !self.dirty_model_ids.is_empty()
            || !self.dirty_palette_ids.is_empty()
    }

    /// Returns true if a full atlas resync is required (e.g. first upload).
    pub fn needs_full_resync(&self) -> bool {
        self.full_resync_needed
    }

    /// Returns the set of model IDs with pending GPU updates.
    pub fn dirty_model_ids(&self) -> &HashSet<u8> {
        &self.dirty_model_ids
    }

    /// Returns the set of palette IDs with pending GPU upload.
    pub fn dirty_palette_ids(&self) -> &HashSet<u8> {
        &self.dirty_palette_ids
    }

    /// Returns the palette_id bound to a model.
    pub fn model_palette_id(&self, model_id: u8) -> Option<u8> {
        self.model_palette_ids.get(model_id as usize).copied()
    }

    /// Returns the number of distinct palette slots in use.
    #[cfg(test)]
    pub fn palette_count(&self) -> usize {
        self.palette_table.len()
    }

    /// Returns true if a specific tier needs GPU update.
    pub fn is_tier_dirty(&self, tier: usize) -> bool {
        tier < NUM_RESOLUTION_TIERS && self.tier_dirty[tier]
    }

    /// Returns true if any tier needs GPU update.
    pub fn is_any_tier_dirty(&self) -> bool {
        self.tier_dirty.iter().any(|&d| d)
    }

    /// Clears all GPU dirty tracking after a successful upload.
    pub fn clear_gpu_dirty(&mut self) {
        self.full_resync_needed = false;
        self.dirty_model_ids.clear();
        self.dirty_palette_ids.clear();
        self.tier_dirty = [false; NUM_RESOLUTION_TIERS];
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

    /// Pack all models into a single 16³ atlas using inline resampling.
    /// Models with different resolutions are resampled on-the-fly during packing.
    pub fn pack_voxels_for_gpu(&self) -> Vec<u8> {
        const SUB_VOXEL_SIZE: usize = 16; // Target atlas resolution
        const ATLAS_WIDTH: usize = 16 * SUB_VOXEL_SIZE;
        const ATLAS_HEIGHT: usize = SUB_VOXEL_SIZE;
        const ATLAS_DEPTH: usize = 16 * SUB_VOXEL_SIZE;
        let mut data = vec![0u8; ATLAS_WIDTH * ATLAS_HEIGHT * ATLAS_DEPTH];

        log::debug!("[DEBUG] Packing {} models for GPU atlas", self.models.len());

        for (model_id, model) in self.models.iter().enumerate() {
            let model_x = model_id % 16;
            let model_z = model_id / 16;
            let model_res = model.resolution.size();

            // Count source voxels for debugging (especially for 32³ models)
            let source_non_zero_count = if model_res == 32 {
                model.voxels.iter().filter(|&&v| v != 0).count()
            } else {
                0
            };

            // Copy voxels with inline resampling if necessary
            let mut non_zero_count = 0;
            for ly in 0..SUB_VOXEL_SIZE {
                for lz in 0..SUB_VOXEL_SIZE {
                    for lx in 0..SUB_VOXEL_SIZE {
                        let voxel = if model_res > SUB_VOXEL_SIZE {
                            // Downsampling: use max pooling to preserve all voxels
                            // Check all voxels in the source region and pick first non-zero
                            let scale = model_res / SUB_VOXEL_SIZE;
                            let base_x = lx * scale;
                            let base_y = ly * scale;
                            let base_z = lz * scale;

                            let mut result = 0u8;
                            'outer: for oz in 0..scale {
                                for oy in 0..scale {
                                    for ox in 0..scale {
                                        let src_x = base_x + ox;
                                        let src_y = base_y + oy;
                                        let src_z = base_z + oz;
                                        let src_idx = src_x
                                            + src_y * model_res
                                            + src_z * model_res * model_res;

                                        if src_idx < model.voxels.len() {
                                            let v = model.voxels[src_idx];
                                            if v != 0 {
                                                result = v;
                                                break 'outer;
                                            }
                                        }
                                    }
                                }
                            }
                            result
                        } else {
                            // Upsampling or same size: nearest neighbor
                            let src_x = lx * model_res / SUB_VOXEL_SIZE;
                            let src_y = ly * model_res / SUB_VOXEL_SIZE;
                            let src_z = lz * model_res / SUB_VOXEL_SIZE;
                            let src_idx = src_x + src_y * model_res + src_z * model_res * model_res;

                            if src_idx < model.voxels.len() {
                                model.voxels[src_idx]
                            } else {
                                0
                            }
                        };

                        if voxel != 0 {
                            non_zero_count += 1;
                        }

                        let atlas_x = model_x * SUB_VOXEL_SIZE + lx;
                        let atlas_y = ly;
                        let atlas_z = model_z * SUB_VOXEL_SIZE + lz;
                        let dst_idx =
                            atlas_x + atlas_y * ATLAS_WIDTH + atlas_z * ATLAS_WIDTH * ATLAS_HEIGHT;

                        data[dst_idx] = voxel;
                    }
                }
            }

            // Debug output for 32³ models
            if model_res == 32 && source_non_zero_count > 0 {
                let loss_pct = if source_non_zero_count > 0 {
                    100.0 * (1.0 - (non_zero_count as f32 / source_non_zero_count as f32))
                } else {
                    0.0
                };
                log::debug!(
                    "[DEBUG] Model ID {} (32³→16³): {} source voxels → {} packed voxels ({:.1}% loss)",
                    model_id,
                    source_non_zero_count,
                    non_zero_count,
                    loss_pct
                );
            }

            if model_id == 1 {
                log::debug!(
                    "[DEBUG] Model ID 1 (torch): packed {} non-zero voxels from {:?} resolution (inline resampling)",
                    non_zero_count,
                    model.resolution
                );

                // Sample a few voxels from torch in the atlas to verify placement
                // Torch is model_id=1, so model_x=1, model_z=0
                // Torch stick at (3,3,3) in 8³ → (6,6,6) in 16³
                // Atlas position: (22, 6, 6)
                let atlas_x = 22;
                let atlas_y = 6;
                let atlas_z = 6;
                let sample_idx =
                    atlas_x + atlas_y * ATLAS_WIDTH + atlas_z * ATLAS_WIDTH * ATLAS_HEIGHT;
                if sample_idx < data.len() {
                    log::debug!(
                        "[DEBUG]   Sample atlas voxel at ({},{},{}): {}",
                        atlas_x,
                        atlas_y,
                        atlas_z,
                        data[sample_idx]
                    );
                }
            }
        }

        log::debug!("[DEBUG] Total atlas size: {} bytes", data.len());
        log::debug!("[DEBUG] >>> USING FIXED INLINE RESAMPLING CODE <<<");
        data
    }

    /// Packs palettes for the shared palette atlas.
    /// Format: 256 palette slots × 32 colors × 4 bytes (RGBA) = 32,768 bytes
    /// Rows indexed by `palette_id` (from `PaletteTable`), not `model_id`.
    pub fn pack_palettes_for_gpu(&self) -> Vec<u8> {
        const TEX_WIDTH: usize = MAX_MODELS; // 256 palette slots
        const TEX_HEIGHT: usize = PALETTE_SIZE; // 32
        let mut data = vec![0u8; TEX_WIDTH * TEX_HEIGHT * 4];

        for (palette_id, entry) in self.palette_table.entries.iter().enumerate() {
            for (palette_idx, color) in entry.palette.iter().enumerate() {
                let dst_idx = (palette_id + palette_idx * TEX_WIDTH) * 4;
                data[dst_idx..dst_idx + 4].copy_from_slice(&color.to_array());
            }
        }

        data
    }

    /// Packs palette emission data for the shared palette atlas.
    /// Format: 256 palette slots × 32 palette indices × 1 byte (R8) = 8,192 bytes
    /// Rows indexed by `palette_id`, not `model_id`.
    pub fn pack_palette_emission_for_gpu(&self) -> Vec<u8> {
        const TEX_WIDTH: usize = MAX_MODELS;
        const TEX_HEIGHT: usize = PALETTE_SIZE;
        let mut data = vec![0u8; TEX_WIDTH * TEX_HEIGHT];

        for (palette_id, entry) in self.palette_table.entries.iter().enumerate() {
            for (palette_idx, &emission) in entry.emission.iter().enumerate() {
                let dst_idx = palette_id + palette_idx * TEX_WIDTH;
                data[dst_idx] = (emission * 255.0) as u8;
            }
        }

        data
    }

    /// Packs a single model's voxel slab for incremental GPU upload.
    ///
    /// Returns `(atlas_offset_xyz, extent_xyz, data)`:
    /// - `atlas_offset_xyz`: destination offset within the tier's 3D atlas texture
    /// - `extent_xyz`: copy region size (equal to the model's resolution in each axis)
    /// - `data`: tight `res³` byte buffer in x-fastest order (`x + y*res + z*res²`)
    ///
    /// Returns `None` if `model_id` is out of range.
    pub fn pack_model_voxel_region(&self, model_id: u8) -> Option<([u32; 3], [u32; 3], Vec<u8>)> {
        let model = self.models.get(model_id as usize)?;
        let res = model.resolution.size();
        let model_x = (model_id as usize) % 16;
        let model_z = (model_id as usize) / 16;
        let atlas_offset = [(model_x * res) as u32, 0u32, (model_z * res) as u32];
        let extent = [res as u32, res as u32, res as u32];
        let volume = res * res * res;
        let mut data = vec![0u8; volume];
        let copy_len = volume.min(model.voxels.len());
        data[..copy_len].copy_from_slice(&model.voxels[..copy_len]);
        Some((atlas_offset, extent, data))
    }

    /// Packs a single palette slot's column (32 RGBA texels = 128 bytes).
    /// Destination in the palette texture is column `palette_id`, rows 0..32.
    pub fn pack_palette_column(&self, palette_id: u8) -> Option<Vec<u8>> {
        let entry = self.palette_table.get(palette_id)?;
        let mut data = Vec::with_capacity(PALETTE_SIZE * 4);
        for color in entry.palette.iter() {
            data.extend_from_slice(&color.to_array());
        }
        Some(data)
    }

    /// Packs a single palette slot's emission column (32 R8 texels = 32 bytes).
    /// Destination in the emission texture is column `palette_id`, rows 0..32.
    pub fn pack_palette_emission_column(&self, palette_id: u8) -> Option<Vec<u8>> {
        let entry = self.palette_table.get(palette_id)?;
        let mut data = Vec::with_capacity(PALETTE_SIZE);
        for &emission in entry.emission.iter() {
            data.push((emission * 255.0) as u8);
        }
        Some(data)
    }

    /// Packs a single model's property record (48 bytes) matching the layout in
    /// `pack_properties_for_gpu`.
    pub fn pack_model_properties(&self, model_id: u8) -> Option<[u8; 48]> {
        let model = self.models.get(model_id as usize)?;
        let mut buf = [0u8; 48];
        buf[0..8].copy_from_slice(&model.collision_mask.to_le_bytes());
        // aabb placeholder (8 bytes of zeros): kept in sync with pack_properties_for_gpu
        if let Some(c) = model.emission {
            buf[16..20].copy_from_slice(&(c.r as f32 / 255.0).to_le_bytes());
            buf[20..24].copy_from_slice(&(c.g as f32 / 255.0).to_le_bytes());
            buf[24..28].copy_from_slice(&(c.b as f32 / 255.0).to_le_bytes());
            buf[28..32].copy_from_slice(&1.0f32.to_le_bytes());
        }
        let palette_id = self
            .model_palette_ids
            .get(model_id as usize)
            .copied()
            .unwrap_or(0);
        let flags = pack_model_flags(model, palette_id);
        buf[32..36].copy_from_slice(&flags.to_le_bytes());
        buf[36..40].copy_from_slice(&(model.resolution.size() as u32).to_le_bytes());
        buf[40..44].copy_from_slice(&model.light_radius.to_le_bytes());
        buf[44..48].copy_from_slice(&model.light_intensity.to_le_bytes());
        Some(buf)
    }

    /// Returns the atlas tier (0/1/2) for a model ID, or `None` if invalid.
    pub fn model_tier(&self, model_id: u8) -> Option<usize> {
        self.models
            .get(model_id as usize)
            .map(|m| m.resolution.tier())
    }

    /// Packs properties for all models.
    pub fn pack_properties_for_gpu(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(self.models.len() * 48);
        for (model_id, model) in self.models.iter().enumerate() {
            // collision_mask (8 bytes)
            data.extend_from_slice(&model.collision_mask.to_le_bytes());

            // aabb placeholder (8 bytes of zeros; not currently used by the shader).
            let zero = 0u32.to_le_bytes();
            data.extend_from_slice(&zero); // aabb_min
            data.extend_from_slice(&zero); // aabb_max

            // emission (16 bytes)
            if let Some(c) = model.emission {
                data.extend_from_slice(&(c.r as f32 / 255.0).to_le_bytes());
                data.extend_from_slice(&(c.g as f32 / 255.0).to_le_bytes());
                data.extend_from_slice(&(c.b as f32 / 255.0).to_le_bytes());
                data.extend_from_slice(&1.0f32.to_le_bytes());
            } else {
                data.extend_from_slice(&0.0f32.to_le_bytes());
                data.extend_from_slice(&0.0f32.to_le_bytes());
                data.extend_from_slice(&0.0f32.to_le_bytes());
                data.extend_from_slice(&0.0f32.to_le_bytes());
            }

            // flags (4 bytes) — see `pack_model_flags` for bit layout.
            let palette_id = self.model_palette_ids.get(model_id).copied().unwrap_or(0);
            let flags = pack_model_flags(model, palette_id);
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

    // ========================================================================
    // GLASS PANE HELPERS
    // ========================================================================

    /// Returns the model ID for a horizontal glass pane with the given connections.
    /// Connection bitmask: N=1, S=2, E=4, W=8
    pub fn horizontal_glass_pane_model_id(connections: u8) -> u8 {
        119 + (connections & 0x0F)
    }

    /// Returns the model ID for a vertical glass pane with the given connections.
    /// Connection bitmask: N=1 (+Y), S=2 (-Y), E=4, W=8
    /// Use rotation to switch between XY and YZ orientations.
    pub fn vertical_glass_pane_model_id(connections: u8) -> u8 {
        135 + (connections & 0x0F)
    }

    /// Checks if a model ID is a horizontal glass pane (IDs 119-134).
    pub fn is_horizontal_glass_pane_model(model_id: u8) -> bool {
        (119..135).contains(&model_id)
    }

    /// Checks if a model ID is a vertical glass pane (IDs 135-150).
    pub fn is_vertical_glass_pane_model(model_id: u8) -> bool {
        (135..151).contains(&model_id)
    }

    /// Checks if a model ID is any glass pane (IDs 119-150).
    pub fn is_glass_pane_model(model_id: u8) -> bool {
        (119..151).contains(&model_id)
    }

    /// Gets the connection mask from a horizontal glass pane model ID.
    /// Returns None if not a horizontal glass pane model.
    pub fn horizontal_glass_pane_connections(model_id: u8) -> Option<u8> {
        if Self::is_horizontal_glass_pane_model(model_id) {
            Some(model_id - 119)
        } else {
            None
        }
    }

    /// Gets the connection mask from a vertical glass pane model ID.
    /// Returns None if not a vertical glass pane model.
    pub fn vertical_glass_pane_connections(model_id: u8) -> Option<u8> {
        if Self::is_vertical_glass_pane_model(model_id) {
            Some(model_id - 135)
        } else {
            None
        }
    }

    // ========================================================================
    // CUSTOM DOOR PAIR HELPERS
    // ========================================================================

    /// Registers a custom door pair and returns its ID.
    /// Returns None if the maximum number of door pairs has been reached.
    pub fn register_door_pair(&mut self, mut door_pair: SimpleDoorPair) -> Option<u16> {
        if self.custom_door_pairs.len() >= MAX_CUSTOM_DOOR_PAIRS {
            return None;
        }

        // Check for duplicate name
        if self
            .custom_door_pairs
            .iter()
            .any(|dp| dp.name == door_pair.name)
        {
            return None;
        }

        let id = self.custom_door_pairs.len() as u16;
        door_pair.id = id;

        // Build reverse lookup from model IDs to this door pair
        for model_id in [
            door_pair.lower_closed,
            door_pair.upper_closed,
            door_pair.lower_open,
            door_pair.upper_open,
        ] {
            self.model_to_door_pair.insert(model_id, id);
        }

        self.custom_door_pairs.push(door_pair);
        Some(id)
    }

    /// Gets a custom door pair by ID.
    pub fn get_door_pair(&self, id: u16) -> Option<&SimpleDoorPair> {
        self.custom_door_pairs.get(id as usize)
    }

    /// Gets a custom door pair by name.
    pub fn get_door_pair_by_name(&self, name: &str) -> Option<&SimpleDoorPair> {
        self.custom_door_pairs.iter().find(|dp| dp.name == name)
    }

    /// Finds the custom door pair containing a model ID.
    pub fn get_door_pair_for_model(&self, model_id: u8) -> Option<&SimpleDoorPair> {
        self.model_to_door_pair
            .get(&model_id)
            .and_then(|&id| self.get_door_pair(id))
    }

    /// Checks if a model ID is part of any custom door pair.
    pub fn is_custom_door_model(&self, model_id: u8) -> bool {
        self.model_to_door_pair.contains_key(&model_id)
    }

    /// Returns an iterator over all custom door pairs.
    pub fn iter_door_pairs(&self) -> impl Iterator<Item = &SimpleDoorPair> {
        self.custom_door_pairs.iter()
    }

    /// Returns the number of registered custom door pairs.
    pub fn door_pair_count(&self) -> usize {
        self.custom_door_pairs.len()
    }

    /// Removes a custom door pair by ID.
    /// Returns the removed door pair, or None if not found.
    pub fn remove_door_pair(&mut self, id: u16) -> Option<SimpleDoorPair> {
        if id as usize >= self.custom_door_pairs.len() {
            return None;
        }

        let removed = self.custom_door_pairs.remove(id as usize);

        // Remove model mappings
        for model_id in [
            removed.lower_closed,
            removed.upper_closed,
            removed.lower_open,
            removed.upper_open,
        ] {
            self.model_to_door_pair.remove(&model_id);
        }

        // Update IDs for remaining door pairs
        for (idx, dp) in self.custom_door_pairs.iter_mut().enumerate() {
            if dp.id > id {
                dp.id = idx as u16;
                // Update model mappings
                for model_id in [
                    dp.lower_closed,
                    dp.upper_closed,
                    dp.lower_open,
                    dp.upper_open,
                ] {
                    self.model_to_door_pair.insert(model_id, dp.id);
                }
            }
        }

        Some(removed)
    }

    /// Toggles a custom door model and returns the new model ID.
    /// Returns the original model_id if not part of a custom door.
    pub fn custom_door_toggled(&self, model_id: u8) -> u8 {
        if let Some(door_pair) = self.get_door_pair_for_model(model_id) {
            door_pair.toggle(model_id)
        } else {
            model_id
        }
    }

    /// Returns the other half of a custom door model.
    /// Returns the original model_id if not part of a custom door.
    pub fn custom_door_other_half(&self, model_id: u8) -> u8 {
        if let Some(door_pair) = self.get_door_pair_for_model(model_id) {
            door_pair.other_half(model_id)
        } else {
            model_id
        }
    }

    /// Checks if a custom door model is the upper half.
    pub fn is_custom_door_upper(&self, model_id: u8) -> bool {
        if let Some(door_pair) = self.get_door_pair_for_model(model_id) {
            door_pair.is_upper(model_id)
        } else {
            false
        }
    }

    /// Checks if a custom door model is in the open state.
    pub fn is_custom_door_open(&self, model_id: u8) -> bool {
        if let Some(door_pair) = self.get_door_pair_for_model(model_id) {
            door_pair.is_open(model_id)
        } else {
            false
        }
    }

    // ========================================================================
    // PICTURE FRAME HELPERS
    // ========================================================================

    /// First picture frame model ID (edge_mask 0).
    pub const FIRST_FRAME_ID: u8 = 160;

    /// Last picture frame model ID (edge_mask 15, all edges).
    pub const LAST_FRAME_ID: u8 = 175;

    /// Checks if a model ID is a picture frame (160-175, 16 edge mask variants).
    pub fn is_frame_model(model_id: u8) -> bool {
        (Self::FIRST_FRAME_ID..=Self::LAST_FRAME_ID).contains(&model_id)
    }

    /// Returns the frame size for a given model ID.
    /// Returns (1,1) for frame models; actual size comes from metadata.
    pub fn frame_size(model_id: u8) -> Option<(u8, u8)> {
        if (Self::FIRST_FRAME_ID..=Self::LAST_FRAME_ID).contains(&model_id) {
            Some((1, 1))
        } else {
            None
        }
    }

    /// Returns the frame model ID for a given size (all valid sizes map to 160).
    pub fn frame_model_id(width: u8, height: u8) -> Option<u8> {
        match (width, height) {
            (1..=3, 1..=3) => Some(160),
            _ => None,
        }
    }

    /// Returns all block positions for a frame, given one block's position and its metadata.
    /// Uses the frame's size and the block's offset within the frame to find all blocks.
    ///
    /// # Arguments
    /// * `pos` - World position of the known frame block
    /// * `model_id` - Model ID of the frame (160)
    /// * `custom_data` - The block's custom_data containing offset and facing
    ///
    /// # Returns
    /// A Vec of all world positions that make up this frame (including the input position).
    pub fn frame_block_positions(
        pos: nalgebra::Vector3<i32>,
        model_id: u8,
        custom_data: u32,
    ) -> Vec<nalgebra::Vector3<i32>> {
        use crate::sub_voxel::builtins::frames::metadata;

        // Size is stored in metadata; fallback to single model if missing.
        let meta_width = metadata::decode_width(custom_data);
        let meta_height = metadata::decode_height(custom_data);
        let (width, height) = match (meta_width, meta_height) {
            (w, h) if w > 0 && h > 0 => (w, h),
            _ => {
                if let Some((w, h)) = Self::frame_size(model_id) {
                    (w, h)
                } else {
                    return vec![pos];
                }
            }
        };

        let offset_x = metadata::decode_offset_x(custom_data);
        let offset_y = metadata::decode_offset_y(custom_data);
        let facing = metadata::decode_facing(custom_data);

        // Calculate anchor position (bottom-left of frame)
        let (dx, dz): (i32, i32) = match facing {
            0 => (1, 0),  // +X direction
            1 => (0, 1),  // +Z direction
            2 => (-1, 0), // -X direction
            3 => (0, -1), // -Z direction
            _ => (1, 0),
        };

        // Calculate anchor from known block position and its offset
        let anchor_x = pos.x - (offset_x as i32 * dx);
        let anchor_y = pos.y - offset_y as i32;
        let anchor_z = pos.z - (offset_x as i32 * dz);

        // Generate all block positions
        let mut positions = Vec::with_capacity((width * height) as usize);
        for ox in 0..width {
            for oy in 0..height {
                positions.push(nalgebra::Vector3::new(
                    anchor_x + (ox as i32 * dx),
                    anchor_y + oy as i32,
                    anchor_z + (ox as i32 * dz),
                ));
            }
        }

        positions
    }

    /// Gets custom door pairs data for persistence.
    pub fn get_custom_door_pairs(&self) -> &[SimpleDoorPair] {
        &self.custom_door_pairs
    }

    /// Loads custom door pairs from saved data.
    /// This should be called after loading the model registry.
    pub fn load_door_pairs(&mut self, door_pairs: Vec<SimpleDoorPair>) {
        for dp in door_pairs {
            if let Err(e) = dp.validate(self) {
                log::warn!("Warning: Skipping invalid door pair '{}': {}", dp.name, e);
                continue;
            }
            if self.register_door_pair(dp).is_none() {
                log::warn!("Warning: Failed to register door pair (max reached or duplicate)");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_dedup_across_builtins() {
        // Built-in families (fences, stairs, doors, etc.) share palettes — the palette
        // table should hold far fewer entries than models.
        let reg = ModelRegistry::new();
        println!(
            "palette dedup: {} palettes for {} models",
            reg.palette_count(),
            reg.len()
        );
        assert!(
            reg.palette_count() < reg.len(),
            "expected palette dedup: {} palettes vs {} models",
            reg.palette_count(),
            reg.len()
        );
    }

    #[test]
    fn palette_release_reclaims_on_update() {
        let mut reg = ModelRegistry::new();
        let initial_count = reg.palette_count();
        // Re-register a built-in with the same palette — should not grow the table.
        let existing = reg.get(1).cloned().expect("model 1 exists");
        let _ = reg.update_or_register(existing);
        assert_eq!(
            reg.palette_count(),
            initial_count,
            "re-registering identical palette should not allocate a new slot",
        );
    }
}

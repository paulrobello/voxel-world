use super::builtins;
use super::model::SubVoxelModel;
use super::types::{
    Color, FIRST_CUSTOM_MODEL_ID, LightBlocking, MAX_MODELS, ModelResolution, PALETTE_SIZE,
    SimpleDoorPair, StairShape,
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

    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
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
            custom_door_pairs: Vec::new(),
            model_to_door_pair: HashMap::new(),
        };

        // Register built-in models
        builtins::register_builtins(&mut registry);

        // MDL-003: guard against builtin add/remove/reorder. IDs are assigned
        // sequentially in `register_builtins`, so any drift shifts every anchor
        // after it (torch=1, crystal=99, fence base=4, door base=39, glass panes
        // 119/135, frames 160-175) and silently corrupts saved chunks + the
        // shader-side ID mappings. `debug_assert!` so release builds stay cheap;
        // the `builtin_model_anchor_ids_are_stable` test enforces this in CI.
        debug_assert_eq!(
            registry.models.len(),
            FIRST_CUSTOM_MODEL_ID as usize,
            "builtin model count drift: expected {} builtins, got {}; \
             register_builtins added/removed/reordered an entry",
            FIRST_CUSTOM_MODEL_ID,
            registry.models.len(),
        );

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
        Some(id)
    }

    /// Registers a model at a server-authoritative ID (multiplayer sync).
    ///
    /// Unlike [`register`](Self::register), which always appends, this places
    /// `model` at exactly index `id`. This is required when a client applies a
    /// `WorldModelStore` / `ModelAdded` from the server: the server's IDs must
    /// be preserved verbatim so every client resolves a given ID to the same
    /// model.
    ///
    /// - `id == len()`: appends (mirrors `register()`'s palette intern).
    /// - `id < len()`: overwrites the existing slot (mirrors
    ///   `update_or_register`'s overwrite branch — releases the old palette,
    ///   interns the new one, and fixes `name_to_id` so the old occupant's name
    ///   no longer resolves to this id).
    /// - `id > len()`: returns `None` (gap — should not happen with contiguous
    ///   server data; logs a `warn!`).
    ///
    /// Returns `Some(id)` on success, `None` on gap or `MAX_MODELS` overflow.
    #[must_use]
    pub fn register_at(&mut self, id: u8, mut model: SubVoxelModel) -> Option<u8> {
        let target = id as usize;
        if target >= MAX_MODELS {
            log::warn!(
                "[ModelRegistry] register_at({}) rejected: exceeds MAX_MODELS ({})",
                id,
                MAX_MODELS
            );
            return None;
        }
        model.id = id;

        if target == self.models.len() {
            // Append — mirror register().
            let (palette_id, newly_allocated) = self
                .palette_table
                .intern(&model.palette, model.palette_emission_slice())
                .expect(
                    "PaletteTable capacity exceeded — cannot exceed MAX_MODELS distinct palettes",
                );
            self.name_to_id.insert(model.name.clone(), id);
            self.models.push(model);
            debug_assert!(self.model_palette_ids.len() == target);
            self.model_palette_ids.push(palette_id);
            if newly_allocated {
                self.dirty_palette_ids.insert(palette_id);
            }
            self.dirty_model_ids.insert(id);
            Some(id)
        } else if target < self.models.len() {
            // Overwrite — mirror update_or_register's overwrite branch.
            let old_palette_id = self.model_palette_ids[target];
            self.palette_table.release(old_palette_id);
            let (new_palette_id, newly_allocated) = self
                .palette_table
                .intern(&model.palette, model.palette_emission_slice())
                .expect("PaletteTable capacity exceeded");
            self.model_palette_ids[target] = new_palette_id;
            if newly_allocated {
                self.dirty_palette_ids.insert(new_palette_id);
            }
            // Drop the old occupant's name→id mapping (only if it still points
            // at THIS slot — defensive against duplicate names elsewhere).
            let old_name = self.models[target].name.clone();
            if self.name_to_id.get(&old_name).copied() == Some(id) {
                self.name_to_id.remove(&old_name);
            }
            self.name_to_id.insert(model.name.clone(), id);
            self.models[target] = model;
            self.dirty_model_ids.insert(id);
            Some(id)
        } else {
            // target > len() — gap in the ID range.
            log::warn!(
                "[ModelRegistry] register_at({}) rejected: gap (current len {})",
                id,
                self.models.len()
            );
            None
        }
    }

    /// Applies a full model-registry sync from the server (client-side).
    ///
    /// `models_data` is LZ4-compressed (`compress_prepend_size`) postcard-
    /// serialized [`crate::storage::model_format::WorldModelStore`]; `door_pairs_data` is the same for
    /// [`crate::storage::model_format::DoorPairStore`]. Each model is placed at its server-authoritative ID
    /// via [`register_at`](Self::register_at), and door pairs are loaded via
    /// [`load_door_pairs`](Self::load_door_pairs).
    ///
    /// Robust to empty or corrupt payloads: a `warn!` is logged and that
    /// payload is skipped (no panic). This is the pure parse-and-register step
    /// shared with the production multiplayer apply path and unit tests.
    pub fn apply_registry_sync(&mut self, models_data: &[u8], door_pairs_data: &[u8]) {
        use crate::storage::model_format::{DoorPairStore, WorldModelStore};
        use lz4_flex::decompress_size_prepended;

        if !models_data.is_empty() {
            match decompress_size_prepended(models_data) {
                Ok(bytes) => match postcard::from_bytes::<WorldModelStore>(&bytes) {
                    Ok(store) => {
                        for (id, model) in store.iter() {
                            if self.register_at(id, model).is_none() {
                                log::warn!(
                                    "[ModelRegistry] apply_registry_sync: could not place model at id {} (current len {})",
                                    id,
                                    self.models.len()
                                );
                            }
                        }
                    }
                    Err(e) => log::warn!(
                        "[ModelRegistry] apply_registry_sync: WorldModelStore deserialize failed: {:?}",
                        e
                    ),
                },
                Err(e) => log::warn!(
                    "[ModelRegistry] apply_registry_sync: models_data decompress failed: {:?}",
                    e
                ),
            }
        }

        if !door_pairs_data.is_empty() {
            match decompress_size_prepended(door_pairs_data) {
                Ok(bytes) => match postcard::from_bytes::<DoorPairStore>(&bytes) {
                    Ok(store) => {
                        self.load_door_pairs(store.get_all().to_vec());
                    }
                    Err(e) => log::warn!(
                        "[ModelRegistry] apply_registry_sync: DoorPairStore deserialize failed: {:?}",
                        e
                    ),
                },
                Err(e) => log::warn!(
                    "[ModelRegistry] apply_registry_sync: door_pairs_data decompress failed: {:?}",
                    e
                ),
            }
        }
    }

    /// Gets a model by ID.
    #[inline]
    pub fn get(&self, id: u8) -> Option<&SubVoxelModel> {
        self.models.get(id as usize)
    }

    /// Gets a model by name.
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
    pub fn get_by_name(&self, name: &str) -> Option<&SubVoxelModel> {
        self.name_to_id.get(name).and_then(|&id| self.get(id))
    }

    /// Gets model ID by name.
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
    pub fn get_id(&self, name: &str) -> Option<u8> {
        self.name_to_id.get(name).copied()
    }

    /// Returns the number of registered models.
    pub fn len(&self) -> usize {
        self.models.len()
    }

    /// Returns true if registry is empty.
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    /// Checks if a model ID is a custom (user-created) model.
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
    pub fn is_custom_model(model_id: u8) -> bool {
        model_id >= FIRST_CUSTOM_MODEL_ID
    }

    /// Returns an iterator over custom (user-created) models.
    ///
    /// Custom models have IDs >= FIRST_CUSTOM_MODEL_ID (176+).
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

    /// Snapshots custom models (in ID order) into a `WorldModelStore` for
    /// persistence to `models.dat`.
    ///
    /// The store assigns IDs as `first_custom_id + index`, which exactly matches
    /// the registry's custom-model ID assignment (contiguous from
    /// `FIRST_CUSTOM_MODEL_ID`) when iterated in ID order. The result is safe to
    /// round-trip through `models.dat`: `store.models[i]` corresponds to registry
    /// ID `FIRST_CUSTOM_MODEL_ID + i`.
    ///
    /// Custom IDs are contiguous by construction (`register()` always appends and
    /// there is no public model-deletion API), so a length mismatch here would
    /// indicate a gap in the custom-ID range and would corrupt `models.dat`
    /// round-trips — caught by the `debug_assert!`.
    pub fn to_world_store(&self) -> crate::storage::model_format::WorldModelStore {
        let mut store = crate::storage::model_format::WorldModelStore::new(FIRST_CUSTOM_MODEL_ID);
        for model in self.iter_custom_models() {
            store.add_model(model, "world");
        }
        debug_assert_eq!(
            store.len(),
            self.custom_model_count(),
            "custom model count mismatch: store {} vs registry {}",
            store.len(),
            self.custom_model_count()
        );
        store
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
    ///
    /// Idempotent: a library model whose `name` is already registered (e.g.
    /// loaded earlier from `models.dat`) is skipped instead of re-registered
    /// under a new ID. This makes the world-open sequence safe — `models.dat`
    /// loads first with stable IDs, then the library only adds genuinely new
    /// models at the next free IDs.
    pub fn load_library_models(&mut self, library_path: &Path) -> std::io::Result<usize> {
        use crate::storage::model_format::LibraryManager;

        if !library_path.exists() {
            return Ok(0);
        }

        let library = LibraryManager::new(library_path);
        let model_names = library.list_models()?;
        let mut loaded = 0;
        let mut skipped = 0;

        for name in model_names {
            match library.load_model(&name) {
                Ok(model) => {
                    if self.name_to_id.contains_key(&model.name) {
                        skipped += 1;
                        continue;
                    }
                    if self.register(model).is_some() {
                        loaded += 1;
                    }
                }
                Err(e) => {
                    log::warn!("Warning: Failed to load library model '{}': {}", name, e);
                }
            }
        }

        if skipped > 0 {
            log::debug!(
                "[ModelRegistry] Skipped {} library models already registered",
                skipped
            );
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
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
    pub fn model_palette_id(&self, model_id: u8) -> Option<u8> {
        self.model_palette_ids.get(model_id as usize).copied()
    }

    /// Returns the number of distinct palette slots in use.
    #[cfg(test)]
    pub fn palette_count(&self) -> usize {
        self.palette_table.len()
    }

    /// Clears all GPU dirty tracking after a successful upload.
    pub fn clear_gpu_dirty(&mut self) {
        self.full_resync_needed = false;
        self.dirty_model_ids.clear();
        self.dirty_palette_ids.clear();
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
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
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
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
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
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
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
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
    pub fn ladder_model_id() -> u8 {
        29
    }

    /// Checks if a model ID is a ladder (ID 29).
    pub fn is_ladder_model(model_id: u8) -> bool {
        model_id == 29
    }

    /// Returns the model ID for the upside-down stairs.
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
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
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
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
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
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
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
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
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
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
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
    pub fn window_connections(model_id: u8) -> Option<u8> {
        if Self::is_window_model(model_id) {
            Some(model_id - 51)
        } else {
            None
        }
    }

    /// Checks if a model is a window or fence (connectable thin blocks).
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
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
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
    pub fn horizontal_glass_pane_connections(model_id: u8) -> Option<u8> {
        if Self::is_horizontal_glass_pane_model(model_id) {
            Some(model_id - 119)
        } else {
            None
        }
    }

    /// Gets the connection mask from a vertical glass pane model ID.
    /// Returns None if not a vertical glass pane model.
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
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
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
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
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
    pub fn door_pair_count(&self) -> usize {
        self.custom_door_pairs.len()
    }

    /// Removes a custom door pair by ID.
    /// Returns the removed door pair, or None if not found.
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
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
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
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
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
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
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
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
    #[allow(dead_code)] // reason: sub-voxel API — kept for future use / API completeness
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

    /// MDL-003: builtin model-ID stability guard.
    ///
    /// Builtin model IDs are assigned imperatively by `register_builtins` calling
    /// `register()` in sequence, so any reordering, addition, or removal shifts
    /// every ID after it. Saved chunks, GLSL/shader code, and multiplayer sync all
    /// reference specific IDs (torch=1, crystal=99, fence base=4, first door=39,
    /// glass panes at 119/135, frames at 160-175). This test pins the canonical
    /// anchor layout so drift fails CI instead of silently corrupting saved worlds.
    ///
    /// To add a new builtin WITHOUT shifting these anchors, replace one of the
    /// reserved placeholders (IDs 151-159) or extend a connection-mask family and
    /// update both this table and CLAUDE.md in the same change.
    #[test]
    fn builtin_model_anchor_ids_are_stable() {
        let reg = ModelRegistry::new();

        // Total builtin count: builtins must fill exactly 0..FIRST_CUSTOM_MODEL_ID.
        assert_eq!(
            reg.len(),
            FIRST_CUSTOM_MODEL_ID as usize,
            "builtin count drift: expected {} builtins, got {}",
            FIRST_CUSTOM_MODEL_ID,
            reg.len(),
        );

        // Pin canonical anchor names to their expected IDs. Each row is
        // (expected_id, expected_name). If a row fails, a builtin was inserted,
        // removed, or reordered at or before that ID — investigate the cause
        // before updating this table, because saved chunks reference these IDs
        // directly and a silent shift would corrupt every existing world.
        const BUILTIN_MODEL_ANCHORS: &[(u8, &str)] = &[
            (0, "empty"),
            (1, "torch"),
            (2, "slab_bottom"),
            (3, "slab_top"),
            (4, "fence_0"),
            (19, "fence_15"),
            (20, "gate_closed_0"),
            (27, "gate_open_3"),
            (28, "stairs_north"),
            (29, "ladder"),
            (38, "stairs_outer_right_inverted"),
            (39, "door_lower_closed_left"),
            (47, "trapdoor_floor_closed"),
            (50, "trapdoor_ceiling_open"),
            (51, "window_0"),
            (67, "windowed_door_lower_closed_left"),
            (75, "paneled_door_lower_closed_left"),
            (83, "fancy_door_lower_closed_left"),
            (91, "glass_door_lower_closed_left"),
            (99, "crystal"),
            (100, "tall_grass"),
            (105, "mushroom_red"),
            (106, "stalactite"),
            (109, "ice_stalagmite"),
            (110, "moss_carpet"),
            (118, "flower_blue"),
            (119, "glass_pane_horizontal_0"),
            (134, "glass_pane_horizontal_15"),
            (135, "glass_pane_vertical_0"),
            (150, "glass_pane_vertical_15"),
            (151, "reserved_151"),
            (159, "reserved_159"),
            (160, "frame_edge_mask_0"),
            (175, "frame_edge_mask_15"),
        ];

        for &(id, expected_name) in BUILTIN_MODEL_ANCHORS {
            let model = reg.get(id).unwrap_or_else(|| {
                panic!(
                    "builtin anchor id {} missing from registry (len={})",
                    id,
                    reg.len()
                )
            });
            assert_eq!(
                model.name, expected_name,
                "builtin anchor id {} name drift: expected {:?}, got {:?}",
                id, expected_name, model.name,
            );
            // Cross-check the name -> id lookup agrees with the slot's id.
            assert_eq!(
                reg.get_id(expected_name),
                Some(id),
                "builtin anchor name {:?} should resolve to id {}, got {:?}",
                expected_name,
                id,
                reg.get_id(expected_name),
            );
        }

        // Every id in 0..FIRST_CUSTOM_MODEL_ID must be an occupied builtin slot
        // and must NOT be classified as custom.
        for id in 0..FIRST_CUSTOM_MODEL_ID {
            assert!(
                reg.get(id).is_some(),
                "builtin slot id {} must be occupied",
                id
            );
            assert!(
                !ModelRegistry::is_custom_model(id),
                "builtin id {} must not be classified as custom",
                id,
            );
        }
    }

    /// MDL-003: custom registrations must start at `FIRST_CUSTOM_MODEL_ID` and not
    /// collide with the builtin range. Catches a builtin under-count that would let
    /// a custom model land on a builtin slot (or vice versa).
    #[test]
    fn custom_models_start_at_first_custom_id() {
        let mut reg = ModelRegistry::new();
        // Sanity: registry is exactly full of builtins before any custom add.
        assert_eq!(reg.len(), FIRST_CUSTOM_MODEL_ID as usize);

        let model =
            SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "test_custom_anchor_one");
        let id = reg.register(model).expect("register first custom model");
        assert_eq!(
            id, FIRST_CUSTOM_MODEL_ID,
            "first custom model must get FIRST_CUSTOM_MODEL_ID, got {}",
            id,
        );
        assert!(
            ModelRegistry::is_custom_model(id),
            "id {} must be classified as custom",
            id,
        );
        assert!(reg.get(id).is_some(), "custom model id {} must resolve", id);

        // The next custom model continues immediately after, with no gap.
        let model2 =
            SubVoxelModel::with_resolution_and_name(ModelResolution::Low, "test_custom_anchor_two");
        let id2 = reg.register(model2).expect("register second custom model");
        assert_eq!(id2, FIRST_CUSTOM_MODEL_ID + 1);
    }

    /// MDL-001 acceptance test: custom-model IDs survive library churn across a
    /// save/reload cycle when `models.dat` is the source of truth.
    ///
    /// Reproduces the bug scenario at the unit level:
    ///   1. Register "Alpha" (ID = FIRST_CUSTOM_MODEL_ID) and "Beta" (ID + 1).
    ///   2. Snapshot to `models.dat` via `to_world_store` + `save`.
    ///   3. Rebuild the registry from `models.dat`, then load a library that has
    ///      "Alpha" + a NEW "Gamma" but is MISSING "Beta".
    ///   4. A saved reference to Beta's ID must still resolve to Beta, because
    ///      `models.dat` carried it even though the library file was deleted.
    #[test]
    fn custom_model_ids_survive_library_churn_via_models_dat() {
        use crate::storage::model_format::{LibraryManager, WorldModelStore};
        use tempfile::tempdir;

        let world_dir = tempdir().expect("tempdir").keep();

        // --- Phase 1: register Alpha + Beta in the original session. ---
        let mut reg = ModelRegistry::new();
        assert_eq!(
            reg.len(),
            FIRST_CUSTOM_MODEL_ID as usize,
            "builtins must fill 0..FIRST_CUSTOM_MODEL_ID"
        );

        let mut alpha = SubVoxelModel::new("Alpha");
        alpha.set_voxel(0, 0, 0, 1);
        let mut beta = SubVoxelModel::new("Beta");
        beta.set_voxel(1, 0, 0, 2);

        let alpha_id = reg.register(alpha).expect("register Alpha");
        let beta_id = reg.register(beta).expect("register Beta");
        assert_eq!(alpha_id, FIRST_CUSTOM_MODEL_ID);
        assert_eq!(beta_id, FIRST_CUSTOM_MODEL_ID + 1);

        // Snapshot + save models.dat.
        let store = reg.to_world_store();
        assert_eq!(store.len(), 2);
        store.save(&world_dir).expect("save models.dat");

        // --- Phase 2: rebuild registry from models.dat, then a churned library. ---
        let mut reg2 = ModelRegistry::new();

        let loaded = WorldModelStore::load(&world_dir)
            .expect("load ok")
            .expect("store present");
        assert_eq!(loaded.len(), 2);
        for (_id, model) in loaded.iter() {
            reg2.register(model).expect("re-register from models.dat");
        }
        assert_eq!(reg2.get_id("Alpha"), Some(FIRST_CUSTOM_MODEL_ID));
        assert_eq!(reg2.get_id("Beta"), Some(FIRST_CUSTOM_MODEL_ID + 1));

        // Library now has Alpha (duplicate) + Gamma (new); Beta file deleted.
        let lib_dir = tempdir().expect("lib tempdir").keep();
        let lib = LibraryManager::new(&lib_dir);
        lib.init().expect("init lib");
        let mut alpha_lib = SubVoxelModel::new("Alpha");
        alpha_lib.set_voxel(0, 0, 0, 1);
        let mut gamma_lib = SubVoxelModel::new("Gamma");
        gamma_lib.set_voxel(2, 0, 0, 3);
        lib.save_model(&alpha_lib, "tester").expect("save Alpha");
        lib.save_model(&gamma_lib, "tester").expect("save Gamma");

        let loaded_count = reg2.load_library_models(&lib_dir).expect("load library");
        // Alpha skipped (already from models.dat), Gamma newly loaded.
        assert_eq!(loaded_count, 1, "only Gamma should be newly loaded");

        // --- Phase 3: acceptance assertions. ---
        // Alpha kept its ID despite the library still containing it.
        assert_eq!(reg2.get_id("Alpha"), Some(FIRST_CUSTOM_MODEL_ID));
        // Beta — whose library file was DELETED — still resolves at its saved ID
        // because models.dat carried it. This is the core MDL-001 acceptance.
        let beta_model = reg2
            .get(FIRST_CUSTOM_MODEL_ID + 1)
            .expect("Beta model present at saved ID");
        assert_eq!(beta_model.name, "Beta");
        assert_eq!(beta_model.get_voxel(1, 0, 0), 2);
        // Gamma got the next free ID after the models.dat models.
        assert_eq!(reg2.get_id("Gamma"), Some(FIRST_CUSTOM_MODEL_ID + 2));
        // No duplicate IDs: exactly 3 custom models (Alpha, Beta, Gamma).
        assert_eq!(reg2.len(), FIRST_CUSTOM_MODEL_ID as usize + 3);
    }

    /// MDL-001 editor-reload variant: deleting one model and rebuilding the
    /// registry — mirroring the world-open sequence (models.dat first, then
    /// library) while skipping the deleted name — must NOT drop models that
    /// exist only in `models.dat`.
    ///
    /// Reproduces the bug fixed in the `EditorAction::ModelDeleted` handler: a
    /// bare `clear()` + `load_library_models()` reload loses any model with no
    /// `.vxm` file, because the library walk cannot see it.
    #[test]
    fn editor_reload_preserves_models_dat_only_models_after_delete() {
        use crate::storage::model_format::{LibraryManager, WorldModelStore};
        use tempfile::tempdir;

        let world_dir = tempdir().expect("tempdir").keep();
        let lib_dir = tempdir().expect("lib tempdir").keep();

        // Original session: Alpha + Gamma live in the library; Beta is a
        // placed custom model never saved to a .vxm file (models.dat is its
        // only home).
        let lib = LibraryManager::new(&lib_dir);
        lib.init().expect("init lib");
        let mut alpha = SubVoxelModel::new("Alpha");
        alpha.set_voxel(0, 0, 0, 1);
        let mut beta = SubVoxelModel::new("Beta");
        beta.set_voxel(1, 0, 0, 2);
        let mut gamma = SubVoxelModel::new("Gamma");
        gamma.set_voxel(2, 0, 0, 3);
        lib.save_model(&alpha, "tester").expect("save Alpha");
        lib.save_model(&gamma, "tester").expect("save Gamma");

        let mut reg = ModelRegistry::new();
        let alpha_id = reg.register(alpha).expect("Alpha");
        let beta_id = reg.register(beta).expect("Beta");
        let _gamma_id = reg.register(gamma).expect("Gamma");
        assert_eq!(alpha_id, FIRST_CUSTOM_MODEL_ID);
        assert_eq!(beta_id, FIRST_CUSTOM_MODEL_ID + 1);

        // Persist the full set (including models.dat-only Beta) to models.dat.
        reg.to_world_store()
            .save(&world_dir)
            .expect("save models.dat");

        // User deletes Gamma from the library, then the editor rebuilds.
        lib.delete_model("Gamma").expect("delete Gamma .vxm");

        // Rebuild mirroring world-open (init.rs), EXCLUDING the deleted name —
        // exactly what EditorAction::ModelDeleted now does.
        let mut reg2 = ModelRegistry::new();
        let store = WorldModelStore::load(&world_dir)
            .expect("load ok")
            .expect("store present");
        for (_id, model) in store.iter() {
            if model.name == "Gamma" {
                continue;
            }
            reg2.register(model).expect("re-register from models.dat");
        }
        let loaded = reg2.load_library_models(&lib_dir).expect("load library");
        // Alpha already came from models.dat (skipped); Gamma was deleted from
        // the library, so the library adds nothing new.
        assert_eq!(loaded, 0, "library should add no new models");

        // Acceptance: Alpha + models.dat-only Beta survive at stable IDs;
        // Gamma is gone.
        assert_eq!(reg2.get_id("Alpha"), Some(alpha_id));
        assert_eq!(
            reg2.get_id("Beta"),
            Some(beta_id),
            "models.dat-only Beta must survive the editor rebuild"
        );
        assert_eq!(reg2.get_id("Gamma"), None, "deleted Gamma must be absent");
    }

    /// `load_library_models` must be idempotent: calling it twice does not
    /// duplicate models or shift IDs.
    #[test]
    fn load_library_models_is_idempotent() {
        use crate::storage::model_format::LibraryManager;
        use tempfile::tempdir;

        let lib_dir = tempdir().expect("tempdir").keep();
        let lib = LibraryManager::new(&lib_dir);
        lib.init().expect("init lib");

        let mut model = SubVoxelModel::new("Lamp");
        model.set_voxel(0, 0, 0, 1);
        lib.save_model(&model, "tester").expect("save");

        let mut reg = ModelRegistry::new();
        let first = reg.load_library_models(&lib_dir).expect("first load");
        assert_eq!(first, 1);
        let id_after_first = reg.get_id("Lamp").expect("Lamp registered");

        let second = reg.load_library_models(&lib_dir).expect("second load");
        assert_eq!(
            second, 0,
            "second load should skip the already-registered model"
        );
        let id_after_second = reg.get_id("Lamp").expect("Lamp still registered");
        assert_eq!(
            id_after_first, id_after_second,
            "ID must not shift on reload"
        );
        assert_eq!(
            reg.len(),
            FIRST_CUSTOM_MODEL_ID as usize + 1,
            "no duplicate model registered"
        );
    }

    /// `to_world_store` produces a store aligned with the registry's custom IDs.
    #[test]
    fn to_world_store_aligns_with_registry_ids() {
        use crate::storage::model_format::WorldModelStore;

        let mut reg = ModelRegistry::new();
        let mut a = SubVoxelModel::new("A");
        a.set_voxel(0, 0, 0, 1);
        let mut b = SubVoxelModel::new("B");
        b.set_voxel(1, 0, 0, 2);
        let a_id = reg.register(a).expect("register A");
        let b_id = reg.register(b).expect("register B");

        let store = reg.to_world_store();
        assert_eq!(store.first_custom_id, FIRST_CUSTOM_MODEL_ID);
        assert_eq!(store.len(), 2);
        // Store index 0 -> registry FIRST_CUSTOM_MODEL_ID, index 1 -> +1.
        let m0 = store.get_model(FIRST_CUSTOM_MODEL_ID).expect("model at A");
        assert_eq!(m0.name, "A");
        let m1 = store
            .get_model(FIRST_CUSTOM_MODEL_ID + 1)
            .expect("model at B");
        assert_eq!(m1.name, "B");
        // Out-of-range IDs return None.
        assert!(store.get_model(a_id - 1).is_none());
        assert!(store.get_model(b_id + 1).is_none());
    }

    /// MDL-002: `register_at` places custom models at server-authoritative IDs,
    /// supports in-place overwrite (fixing `name_to_id`), and rejects gaps.
    #[test]
    fn register_at_places_and_overwrites() {
        let mut reg = ModelRegistry::new();
        let initial_len = reg.len();
        assert_eq!(initial_len, FIRST_CUSTOM_MODEL_ID as usize);

        // Append two models at the next two IDs.
        let mut a = SubVoxelModel::new("Alpha");
        a.set_voxel(0, 0, 0, 1);
        let mut b = SubVoxelModel::new("Beta");
        b.set_voxel(1, 0, 0, 2);
        assert_eq!(
            reg.register_at(FIRST_CUSTOM_MODEL_ID, a).unwrap(),
            FIRST_CUSTOM_MODEL_ID
        );
        assert_eq!(
            reg.register_at(FIRST_CUSTOM_MODEL_ID + 1, b).unwrap(),
            FIRST_CUSTOM_MODEL_ID + 1
        );
        assert_eq!(reg.get(FIRST_CUSTOM_MODEL_ID).unwrap().name, "Alpha");
        assert_eq!(
            reg.get(FIRST_CUSTOM_MODEL_ID).unwrap().get_voxel(0, 0, 0),
            1
        );
        assert_eq!(reg.get(FIRST_CUSTOM_MODEL_ID + 1).unwrap().name, "Beta");
        assert_eq!(
            reg.get(FIRST_CUSTOM_MODEL_ID + 1)
                .unwrap()
                .get_voxel(1, 0, 0),
            2
        );
        assert_eq!(reg.len(), initial_len + 2);

        // Overwrite slot FIRST_CUSTOM_MODEL_ID with a different model. The old
        // occupant's name ("Alpha") must no longer resolve; the new one must.
        let mut a2 = SubVoxelModel::new("AlphaTwo");
        a2.set_voxel(2, 0, 0, 3);
        assert_eq!(
            reg.register_at(FIRST_CUSTOM_MODEL_ID, a2).unwrap(),
            FIRST_CUSTOM_MODEL_ID
        );
        assert_eq!(reg.get(FIRST_CUSTOM_MODEL_ID).unwrap().name, "AlphaTwo");
        assert_eq!(
            reg.get(FIRST_CUSTOM_MODEL_ID).unwrap().get_voxel(2, 0, 0),
            3
        );
        assert!(
            reg.get_id("Alpha").is_none(),
            "old occupant's name must be cleared from name_to_id after overwrite"
        );
        assert_eq!(reg.get_id("AlphaTwo"), Some(FIRST_CUSTOM_MODEL_ID));
        // No duplicate IDs: len unchanged after overwrite.
        assert_eq!(reg.len(), initial_len + 2);
        // Beta untouched.
        assert_eq!(reg.get(FIRST_CUSTOM_MODEL_ID + 1).unwrap().name, "Beta");
        // palette bookkeeping stays consistent (model_palette_ids parallels models).
        assert_eq!(reg.model_palette_ids.len(), reg.models.len());

        // Gap rejection: id beyond len+1 returns None and does not mutate state.
        let len_before = reg.len();
        let mut orphan = SubVoxelModel::new("Orphan");
        orphan.set_voxel(0, 0, 0, 1);
        assert_eq!(
            reg.register_at(255, orphan),
            None,
            "gap id must be rejected"
        );
        assert_eq!(
            reg.len(),
            len_before,
            "rejected register_at must not grow the registry"
        );
        assert!(reg.get_id("Orphan").is_none());
    }

    /// MDL-002 acceptance: after applying a serialized `WorldModelStore`, a
    /// fresh registry's IDs exactly match the store's `first_custom_id + index`.
    /// This pins the client/host ID-match contract without a real socket — the
    /// same `apply_registry_sync` code runs in the production multiplayer path.
    #[test]
    fn apply_registry_sync_roundtrip_matches_server_ids() {
        use crate::storage::model_format::WorldModelStore;
        use lz4_flex::compress_prepend_size;

        // Build a store with two distinguishable models (server-side construction).
        let mut store = WorldModelStore::new(FIRST_CUSTOM_MODEL_ID);
        let mut a = SubVoxelModel::new("HostA");
        a.set_voxel(0, 0, 0, 1);
        let mut b = SubVoxelModel::new("HostB");
        b.set_voxel(1, 0, 0, 2);
        store.add_model(&a, "host");
        store.add_model(&b, "host");

        // Serialize + compress exactly like the server (send_model_registry).
        let models_data = {
            let serialized = postcard::to_stdvec(&store).expect("serialize store");
            compress_prepend_size(&serialized)
        };
        // No door pairs in this test — empty payload is a no-op.
        let door_pairs_data: Vec<u8> = Vec::new();

        // Client side: fresh registry applies the sync.
        let mut client_reg = ModelRegistry::new();
        client_reg.apply_registry_sync(&models_data, &door_pairs_data);

        // Each store model must land at first_custom_id + index with matching voxels.
        assert_eq!(
            client_reg
                .get(FIRST_CUSTOM_MODEL_ID)
                .expect("HostA placed")
                .name,
            "HostA"
        );
        assert_eq!(
            client_reg
                .get(FIRST_CUSTOM_MODEL_ID)
                .expect("HostA placed")
                .get_voxel(0, 0, 0),
            1
        );
        assert_eq!(
            client_reg
                .get(FIRST_CUSTOM_MODEL_ID + 1)
                .expect("HostB placed")
                .name,
            "HostB"
        );
        assert_eq!(
            client_reg
                .get(FIRST_CUSTOM_MODEL_ID + 1)
                .expect("HostB placed")
                .get_voxel(1, 0, 0),
            2
        );
        // Registry length now spans builtins + both custom models.
        assert_eq!(client_reg.len(), FIRST_CUSTOM_MODEL_ID as usize + 2);

        // Round-trip stability: re-applying the same sync overwrites cleanly
        // (server re-sends on reconnect) without corrupting bookkeeping.
        client_reg.apply_registry_sync(&models_data, &door_pairs_data);
        assert_eq!(
            client_reg.len(),
            FIRST_CUSTOM_MODEL_ID as usize + 2,
            "re-applying identical sync must not grow the registry"
        );
        assert_eq!(client_reg.get(FIRST_CUSTOM_MODEL_ID).unwrap().name, "HostA");
    }
}

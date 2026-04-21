use super::types::{Color, LightBlocking, LightMode, ModelResolution, PALETTE_SIZE};
use nalgebra::Vector3;

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
    ///
    /// `None` represents the all-zeros case (no emissive slots). Most built-in models fall
    /// into this case so we skip the 128-byte array allocation entirely. The boxed form
    /// is lazily created on first non-zero write via `palette_emission_mut`.
    pub palette_emission: Option<Box<[f32; PALETTE_SIZE]>>,

    /// 4×4×4 collision bitmask (64 bits).
    /// Each bit represents an (N/4)³ region of the model.
    /// Bit index = cx + cy * 4 + cz * 16 where cx,cy,cz in 0..4.
    pub collision_mask: u64,

    /// Whether collision_mask needs recomputation due to voxel changes.
    pub(crate) collision_mask_dirty: bool,

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

    /// Whether this model has collision enabled.
    /// If true (default), physics will ignore this model (walk through).
    pub no_collision: bool,

    /// Whether this model is ground cover (grass, flowers, mushrooms, etc.).
    /// Ground cover can be hidden with a setting to see terrain more clearly.
    pub is_ground_cover: bool,
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
            palette_emission: None, // All-zero (sparse); lazily boxed on first write
            collision_mask: 0,
            collision_mask_dirty: true,
            light_blocking: LightBlocking::None,
            rotatable: false,
            emission: None,
            is_light_source: false,
            light_mode: LightMode::Steady,
            light_radius: 8.0,
            light_intensity: 1.0,
            requires_ground_support: false,
            no_collision: false,
            is_ground_cover: false,
        }
    }

    /// Zeroed emission array returned when `palette_emission` is `None` (all-zero).
    const ZERO_EMISSION: [f32; PALETTE_SIZE] = [0.0; PALETTE_SIZE];

    /// Returns a view of the 32-slot emission array, substituting zeros when sparse.
    #[inline]
    pub fn palette_emission_slice(&self) -> &[f32; PALETTE_SIZE] {
        self.palette_emission
            .as_deref()
            .unwrap_or(&Self::ZERO_EMISSION)
    }

    /// Returns a mutable view of the 32-slot emission array, allocating the backing
    /// box on first call if the model was previously all-zero.
    #[inline]
    fn palette_emission_mut_array(&mut self) -> &mut [f32; PALETTE_SIZE] {
        self.palette_emission
            .get_or_insert_with(|| Box::new([0.0; PALETTE_SIZE]))
            .as_mut()
    }

    /// Sets the emission intensity for a palette slot.
    /// Emission makes the color glow and emit light (0.0 = none, 1.0 = full).
    pub fn set_palette_emission(&mut self, palette_idx: usize, emission: f32) {
        if palette_idx < PALETTE_SIZE {
            let clamped = emission.clamp(0.0, 1.0);
            // Don't allocate just to write a zero into an already-sparse model.
            if clamped == 0.0 && self.palette_emission.is_none() {
                return;
            }
            self.palette_emission_mut_array()[palette_idx] = clamped;
            // Collapse to sparse if the write made every slot zero again.
            if self
                .palette_emission
                .as_deref()
                .is_some_and(|arr| arr.iter().all(|&e| e == 0.0))
            {
                self.palette_emission = None;
            }
        }
    }

    /// Gets the emission intensity for a palette slot.
    pub fn get_palette_emission(&self, palette_idx: usize) -> f32 {
        if palette_idx < PALETTE_SIZE {
            self.palette_emission_slice()[palette_idx]
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
        self.palette_emission
            .as_deref()
            .is_some_and(|arr| arr.iter().any(|&e| e > 0.0))
    }

    /// Returns the dominant emission color from the palette.
    /// Used for light source color when is_light_source is enabled.
    pub fn dominant_emission_color(&self) -> Option<Color> {
        let arr = self.palette_emission.as_deref()?;
        let mut max_emission = 0.0f32;
        let mut dominant_idx = None;

        for (idx, &emission) in arr.iter().enumerate() {
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
        self.collision_mask_dirty = true;
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
        let size = self.size();
        for z in z0..=z1.min(max_idx) {
            for y in y0..=y1.min(max_idx) {
                for x in x0..=x1.min(max_idx) {
                    self.voxels[x + y * size + z * size * size] = palette_idx;
                }
            }
        }
        self.collision_mask_dirty = true;
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
        self.collision_mask_dirty = false;
    }

    /// Ensures the collision mask is up-to-date, recomputing only if dirty.
    pub fn ensure_collision_mask(&mut self) {
        if self.collision_mask_dirty {
            self.compute_collision_mask();
        }
    }

    /// Checks if a point collides with this model using the collision mask.
    ///
    /// Point coordinates are in model-local space (0.0 to 1.0).
    #[inline]
    pub fn point_collides(&self, x: f32, y: f32, z: f32) -> bool {
        if self.no_collision {
            return false;
        }

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
        for &emission in self.palette_emission_slice() {
            data.extend_from_slice(&emission.to_le_bytes());
        }
        data
    }

    /// Packs combined palette data for GPU upload (RGBA + emission per slot).
    /// Format: 32 entries × 5 bytes (R, G, B, A, emission_u8)
    pub fn pack_palette_combined(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(PALETTE_SIZE * 5);
        for (color, &emission) in self
            .palette
            .iter()
            .zip(self.palette_emission_slice().iter())
        {
            data.extend_from_slice(&color.to_array());
            // Pack emission as u8 (0-255, scaled from 0.0-1.0)
            data.push((emission * 255.0) as u8);
        }
        data
    }

    /// Upscales the model to a higher resolution by subdividing each voxel.
    ///
    /// Each voxel becomes a cube of voxels in the new resolution:
    /// - 8³ → 16³: each voxel becomes 2×2×2 (8 voxels)
    /// - 8³ → 32³: each voxel becomes 4×4×4 (64 voxels)
    /// - 16³ → 32³: each voxel becomes 2×2×2 (8 voxels)
    ///
    /// Returns None if target resolution is not higher than current.
    pub fn upscale(&self, target: ModelResolution) -> Option<Self> {
        let old_size = self.size();
        let new_size = target.size();

        if new_size <= old_size {
            return None;
        }

        let scale = new_size / old_size;
        let mut new_voxels = vec![0u8; target.volume()];

        // For each voxel in the old model, fill a scale×scale×scale cube in the new model
        for oz in 0..old_size {
            for oy in 0..old_size {
                for ox in 0..old_size {
                    let palette_idx = self.get_voxel(ox, oy, oz);
                    if palette_idx == 0 {
                        continue; // Air stays air
                    }

                    // Fill the corresponding cube in new model
                    let base_x = ox * scale;
                    let base_y = oy * scale;
                    let base_z = oz * scale;

                    for dz in 0..scale {
                        for dy in 0..scale {
                            for dx in 0..scale {
                                let nx = base_x + dx;
                                let ny = base_y + dy;
                                let nz = base_z + dz;
                                new_voxels[nx + ny * new_size + nz * new_size * new_size] =
                                    palette_idx;
                            }
                        }
                    }
                }
            }
        }

        let mut new_model = Self {
            id: self.id,
            name: self.name.clone(),
            resolution: target,
            voxels: new_voxels,
            palette: self.palette,
            palette_emission: self.palette_emission.clone(),
            collision_mask: 0,
            collision_mask_dirty: true,
            light_blocking: self.light_blocking,
            rotatable: self.rotatable,
            emission: self.emission,
            is_light_source: self.is_light_source,
            light_mode: self.light_mode,
            light_radius: self.light_radius,
            light_intensity: self.light_intensity,
            requires_ground_support: self.requires_ground_support,
            no_collision: self.no_collision,
            is_ground_cover: self.is_ground_cover,
        };
        new_model.compute_collision_mask();

        // Debug for model ID 1 (torch)
        if self.id == 1 {
            let orig_count = self.voxels.iter().filter(|&&v| v != 0).count();
            let new_count = new_model.voxels.iter().filter(|&&v| v != 0).count();
            log::debug!(
                "[DEBUG] Upscale model ID 1: {:?} -> {:?}, voxels {} -> {}",
                self.resolution,
                target,
                orig_count,
                new_count
            );
        }

        Some(new_model)
    }

    /// Downscales the model to a lower resolution by sampling.
    ///
    /// Uses nearest-neighbor sampling (takes the voxel at the center of each region).
    /// This may lose detail - consider warning the user before downscaling.
    ///
    /// - 16³ → 8³: samples every 2nd voxel
    /// - 32³ → 8³: samples every 4th voxel
    /// - 32³ → 16³: samples every 2nd voxel
    ///
    /// Returns None if target resolution is not lower than current.
    pub fn downscale(&self, target: ModelResolution) -> Option<Self> {
        let old_size = self.size();
        let new_size = target.size();

        if new_size >= old_size {
            return None;
        }

        let scale = old_size / new_size;
        let offset = scale / 2; // Sample from center of each region
        let mut new_voxels = vec![0u8; target.volume()];

        // For each voxel in the new model, sample from center of corresponding region
        for nz in 0..new_size {
            for ny in 0..new_size {
                for nx in 0..new_size {
                    let ox = nx * scale + offset;
                    let oy = ny * scale + offset;
                    let oz = nz * scale + offset;

                    let palette_idx = self.get_voxel(ox, oy, oz);
                    new_voxels[nx + ny * new_size + nz * new_size * new_size] = palette_idx;
                }
            }
        }

        let mut new_model = Self {
            id: self.id,
            name: self.name.clone(),
            resolution: target,
            voxels: new_voxels,
            palette: self.palette,
            palette_emission: self.palette_emission.clone(),
            collision_mask: 0,
            collision_mask_dirty: true,
            light_blocking: self.light_blocking,
            rotatable: self.rotatable,
            emission: self.emission,
            is_light_source: self.is_light_source,
            light_mode: self.light_mode,
            light_radius: self.light_radius,
            light_intensity: self.light_intensity,
            requires_ground_support: self.requires_ground_support,
            no_collision: self.no_collision,
            is_ground_cover: self.is_ground_cover,
        };
        new_model.compute_collision_mask();

        Some(new_model)
    }

    /// Changes the resolution of the model, upscaling or downscaling as needed.
    pub fn change_resolution(&self, target: ModelResolution) -> Option<Self> {
        if self.resolution == target {
            return Some(self.clone());
        }
        if target.size() > self.resolution.size() {
            self.upscale(target)
        } else {
            self.downscale(target)
        }
    }
}

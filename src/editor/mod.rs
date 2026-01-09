//! In-game model editor for creating sub-voxel models.
//!
//! When active, the editor:
//! - Disables player movement and physics
//! - Switches to an orbit camera around the model being edited
//! - Shows egui panels for tools, palette, and library
//! - Allows placing/removing voxels with mouse clicks
//! - Supports undo/redo for all editing operations

#![allow(dead_code)] // WIP: Full integration pending

pub mod rasterizer;
pub mod ui;

use crate::sub_voxel::{
    Color, ModelResolution, SUB_VOXEL_BOUNDS_F32, SUB_VOXEL_CENTER_F32, SubVoxelModel,
};
use nalgebra::Vector3;

pub use ui::{EditorAction, draw_editor_ui, draw_model_preview};

/// Maximum number of undo states to keep per stack.
const MAX_UNDO_HISTORY: usize = 100;

/// Type alias for voxel state snapshot (variable size based on resolution).
type VoxelSnapshot = Vec<u8>;

/// Manages undo/redo history for the model editor.
///
/// Uses a dual-stack approach with separate undo and redo stacks.
/// Each stack stores full voxel snapshots (variable size based on resolution).
#[derive(Debug, Clone)]
pub struct UndoHistory {
    /// Stack of states you can undo to.
    undo_stack: Vec<VoxelSnapshot>,
    /// Stack of states you can redo to.
    redo_stack: Vec<VoxelSnapshot>,
}

impl Default for UndoHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl UndoHistory {
    /// Creates a new empty undo history.
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::with_capacity(MAX_UNDO_HISTORY),
            redo_stack: Vec::with_capacity(MAX_UNDO_HISTORY / 2),
        }
    }

    /// Clears all history (call when resolution changes).
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// Saves the current state before a modification.
    ///
    /// This clears the redo stack since we're branching from this point.
    pub fn save(&mut self, current_voxels: &[u8]) {
        // Clear redo stack - we're branching
        self.redo_stack.clear();

        // Add to undo stack
        self.undo_stack.push(current_voxels.to_vec());

        // Trim oldest states if we exceed max
        if self.undo_stack.len() > MAX_UNDO_HISTORY {
            self.undo_stack.remove(0);
        }
    }

    /// Performs an undo operation.
    ///
    /// Takes the current state and returns the previous state to restore.
    /// Returns `None` if there's nothing to undo.
    pub fn undo(&mut self, current_voxels: &[u8]) -> Option<VoxelSnapshot> {
        if let Some(previous) = self.undo_stack.pop() {
            // Save current for redo
            self.redo_stack.push(current_voxels.to_vec());
            Some(previous)
        } else {
            None
        }
    }

    /// Performs a redo operation.
    ///
    /// Takes the current state and returns the next state to restore.
    /// Returns `None` if there's nothing to redo.
    pub fn redo(&mut self, current_voxels: &[u8]) -> Option<VoxelSnapshot> {
        if let Some(next) = self.redo_stack.pop() {
            // Save current for undo
            self.undo_stack.push(current_voxels.to_vec());
            Some(next)
        } else {
            None
        }
    }

    /// Returns true if undo is available.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Returns true if redo is available.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Returns the number of undo steps available.
    pub fn undo_count(&self) -> usize {
        self.undo_stack.len()
    }

    /// Returns the number of redo steps available.
    pub fn redo_count(&self) -> usize {
        self.redo_stack.len()
    }
}

/// The currently selected editing tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorTool {
    #[default]
    Pencil,
    Eraser,
    Fill,
    Eyedropper,
    /// Place a filled cube shape.
    Cube,
    /// Place a filled sphere shape.
    Sphere,
    /// Change the color of existing voxels.
    ColorChange,
    /// Flood fill connected voxels with a color.
    PaintBucket,
    /// Draw a line of voxels between two points.
    Bridge,
}

/// Mirror axis for symmetrical editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorAxis {
    X,
    Y,
    Z,
}

/// State for the in-game model editor.
#[derive(Debug)]
pub struct EditorState {
    /// Whether the editor is currently active.
    pub active: bool,

    /// The model being edited (working copy).
    pub scratch_pad: SubVoxelModel,

    /// Currently selected palette index (1-15, 0 is air).
    pub selected_palette_index: u8,

    /// Current editing tool.
    pub tool: EditorTool,

    /// Mirror mode axes (X, Y, Z) - when enabled, edits are mirrored across the center.
    pub mirror_axes: [bool; 3],

    /// Orbit camera yaw angle (radians).
    pub orbit_yaw: f32,

    /// Orbit camera pitch angle (radians).
    pub orbit_pitch: f32,

    /// Distance from center to camera.
    pub orbit_distance: f32,

    /// Whether we're currently dragging to rotate.
    pub is_dragging: bool,

    /// Last mouse position for drag calculation.
    pub last_mouse_pos: Option<[f32; 2]>,

    /// The voxel position currently hovered (if any).
    pub hovered_voxel: Option<Vector3<i32>>,

    /// The face normal of the hovered voxel (for placing adjacent).
    pub hovered_normal: Option<Vector3<i32>>,

    /// Saved world position where the model will be placed on save.
    /// Set when the editor is opened based on player's target.
    pub saved_target_pos: Option<Vector3<i32>>,

    /// Whether to show the overwrite confirmation dialog.
    pub show_overwrite_confirm: bool,

    /// Model name pending deletion (triggers confirmation dialog).
    pub pending_delete_model: Option<String>,

    /// Whether to show the new model resolution dialog.
    pub show_new_model_dialog: bool,

    /// Selected resolution for new model dialog.
    pub new_model_resolution: ModelResolution,

    /// Pending resolution change (triggers confirmation dialog).
    pub pending_resolution_change: Option<ModelResolution>,

    /// Undo/redo history for voxel operations.
    pub history: UndoHistory,

    /// Size of shapes when using Cube or Sphere tool (diameter in voxels).
    pub shape_size: usize,

    /// First point for Bridge tool (set on first click, cleared on second or tool change).
    pub bridge_first_point: Option<Vector3<i32>>,
}

impl Default for EditorState {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorState {
    /// Creates a new editor state with default values.
    pub fn new() -> Self {
        let mut scratch_pad = SubVoxelModel::new("untitled");
        // Set up a basic palette
        scratch_pad.palette[0] = Color::transparent();
        scratch_pad.palette[1] = Color::rgb(180, 180, 180); // Light gray
        scratch_pad.palette[2] = Color::rgb(100, 60, 40); // Wood brown
        scratch_pad.palette[3] = Color::rgb(200, 50, 50); // Red
        scratch_pad.palette[4] = Color::rgb(50, 200, 50); // Green
        scratch_pad.palette[5] = Color::rgb(50, 50, 200); // Blue
        scratch_pad.palette[6] = Color::rgb(200, 200, 50); // Yellow
        scratch_pad.palette[7] = Color::rgb(200, 100, 50); // Orange
        scratch_pad.palette[8] = Color::rgb(150, 50, 200); // Purple
        scratch_pad.palette[9] = Color::rgb(50, 200, 200); // Cyan
        scratch_pad.palette[10] = Color::rgb(255, 192, 203); // Pink
        scratch_pad.palette[11] = Color::rgb(255, 255, 255); // White
        scratch_pad.palette[12] = Color::rgb(64, 64, 64); // Dark gray
        scratch_pad.palette[13] = Color::rgb(0, 0, 0); // Black
        scratch_pad.palette[14] = Color::rgba(100, 150, 200, 180); // Glass-like
        scratch_pad.palette[15] = Color::rgb(255, 200, 100); // Warm light

        Self {
            active: false,
            scratch_pad,
            selected_palette_index: 1,
            tool: EditorTool::default(),
            mirror_axes: [false, false, false],
            orbit_yaw: std::f32::consts::FRAC_PI_4,
            orbit_pitch: std::f32::consts::FRAC_PI_6,
            orbit_distance: 40.0, // Default zoom (increased to see whole model)
            is_dragging: false,
            last_mouse_pos: None,
            hovered_voxel: None,
            hovered_normal: None,
            saved_target_pos: None,
            show_overwrite_confirm: false,
            pending_delete_model: None,
            show_new_model_dialog: false,
            new_model_resolution: ModelResolution::Medium,
            pending_resolution_change: None,
            history: UndoHistory::new(),
            shape_size: 3, // Default 3x3 cube/sphere
            bridge_first_point: None,
        }
    }

    /// Toggles the editor on/off.
    pub fn toggle(&mut self) {
        self.active = !self.active;
        if self.active {
            self.is_dragging = false;
            self.last_mouse_pos = None;
        } else {
            // Clear saved position when closing
            self.saved_target_pos = None;
        }
    }

    /// Sets the target position where the model will be placed.
    pub fn set_target_pos(&mut self, pos: Vector3<i32>) {
        self.saved_target_pos = Some(pos);
    }

    /// Resets the scratch pad to a new empty model.
    pub fn new_model(&mut self, name: &str) {
        self.new_model_with_resolution(name, ModelResolution::Medium);
    }

    /// Creates a new model with a specific resolution.
    pub fn new_model_with_resolution(&mut self, name: &str, resolution: ModelResolution) {
        let palette = self.scratch_pad.palette;
        let palette_emission = self.scratch_pad.palette_emission;
        self.scratch_pad = SubVoxelModel::with_resolution_and_name(resolution, name);
        self.scratch_pad.palette = palette;
        self.scratch_pad.palette_emission = palette_emission;
        self.hovered_voxel = None;
        self.history.clear();
        // Adjust camera distance based on resolution
        self.orbit_distance = match resolution {
            ModelResolution::Low => 25.0,
            ModelResolution::Medium => 40.0,
            ModelResolution::High => 70.0,
        };
    }

    /// Loads a model into the scratch pad for editing.
    pub fn load_model(&mut self, model: &SubVoxelModel) {
        self.scratch_pad = model.clone();
        self.hovered_voxel = None;
        self.history.clear();
        // Adjust camera distance based on loaded model's resolution
        self.orbit_distance = match model.resolution {
            ModelResolution::Low => 25.0,
            ModelResolution::Medium => 40.0,
            ModelResolution::High => 70.0,
        };
    }

    /// Changes the model resolution, preserving voxel data when possible.
    ///
    /// - Upscaling (higher res): subdivides each voxel into a cube
    /// - Downscaling (lower res): samples from center of each region (may lose detail)
    ///
    /// Returns true if resolution was changed.
    pub fn change_resolution(&mut self, target: ModelResolution) -> bool {
        if let Some(new_model) = self.scratch_pad.change_resolution(target) {
            self.scratch_pad = new_model;
            self.hovered_voxel = None;
            self.history.clear();
            // Adjust camera distance for new resolution
            self.orbit_distance = match target {
                ModelResolution::Low => 25.0,
                ModelResolution::Medium => 40.0,
                ModelResolution::High => 70.0,
            };
            true
        } else {
            false
        }
    }

    /// Saves the current voxel state to the undo history.
    ///
    /// Call this before making any modifications to the model.
    pub fn save_state(&mut self) {
        self.history.save(&self.scratch_pad.voxels);
    }

    /// Undoes the last voxel operation.
    ///
    /// Returns true if an undo was performed.
    pub fn undo(&mut self) -> bool {
        if let Some(previous) = self.history.undo(&self.scratch_pad.voxels) {
            self.scratch_pad.voxels = previous;
            true
        } else {
            false
        }
    }

    /// Redoes the last undone operation.
    ///
    /// Returns true if a redo was performed.
    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.history.redo(&self.scratch_pad.voxels) {
            self.scratch_pad.voxels = next;
            true
        } else {
            false
        }
    }

    /// Returns true if undo is available.
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// Returns true if redo is available.
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Calculates the orbit camera position.
    pub fn camera_position(&self) -> Vector3<f32> {
        let center = Vector3::new(
            SUB_VOXEL_CENTER_F32,
            SUB_VOXEL_CENTER_F32,
            SUB_VOXEL_CENTER_F32,
        );

        let cos_pitch = self.orbit_pitch.cos();
        let sin_pitch = self.orbit_pitch.sin();
        let cos_yaw = self.orbit_yaw.cos();
        let sin_yaw = self.orbit_yaw.sin();

        let offset = Vector3::new(
            self.orbit_distance * cos_pitch * sin_yaw,
            self.orbit_distance * sin_pitch,
            self.orbit_distance * cos_pitch * cos_yaw,
        );

        center + offset
    }

    /// Gets the camera look-at target (center of model).
    pub fn camera_target(&self) -> Vector3<f32> {
        Vector3::new(
            SUB_VOXEL_CENTER_F32,
            SUB_VOXEL_CENTER_F32,
            SUB_VOXEL_CENTER_F32,
        )
    }

    /// Updates orbit camera based on mouse drag.
    pub fn update_orbit(&mut self, mouse_pos: [f32; 2], sensitivity: f32) {
        if let Some(last_pos) = self.last_mouse_pos {
            let dx = mouse_pos[0] - last_pos[0];
            let dy = mouse_pos[1] - last_pos[1];

            self.orbit_yaw += dx * sensitivity;
            self.orbit_pitch += dy * sensitivity;

            // Clamp pitch to avoid gimbal lock
            self.orbit_pitch = self.orbit_pitch.clamp(
                -std::f32::consts::FRAC_PI_2 + 0.1,
                std::f32::consts::FRAC_PI_2 - 0.1,
            );
        }
        self.last_mouse_pos = Some(mouse_pos);
    }

    /// Performs a ray-voxel intersection test against the sub-voxel grid.
    ///
    /// Returns (voxel_pos, face_normal) if a solid voxel was hit.
    pub fn raycast_voxel(
        &self,
        origin: Vector3<f32>,
        direction: Vector3<f32>,
    ) -> Option<(Vector3<i32>, Vector3<i32>)> {
        // DDA algorithm for sub-voxel grid
        let dir = direction.normalize();

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

        // Calculate entry/exit t for the sub-voxel cube
        let t_min_v = (Vector3::new(-0.001, -0.001, -0.001) - origin).component_mul(&inv_dir);
        let t_max_v = (Vector3::new(
            SUB_VOXEL_BOUNDS_F32,
            SUB_VOXEL_BOUNDS_F32,
            SUB_VOXEL_BOUNDS_F32,
        ) - origin)
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

        // Find entry axis for initial normal
        let entry_axis = if t1.x >= t1.y && t1.x >= t1.z {
            0
        } else if t1.y >= t1.z {
            1
        } else {
            2
        };

        let start_t = t_near.max(0.0);
        let mut current_pos = origin + safe_dir * start_t;
        current_pos += safe_dir * 0.001; // nudge
        let model_size = self.scratch_pad.size();
        current_pos = current_pos.map(|v| v.clamp(0.001, model_size as f32 - 0.001));

        let mut voxel = Vector3::new(
            current_pos.x.floor() as i32,
            current_pos.y.floor() as i32,
            current_pos.z.floor() as i32,
        );

        let step = Vector3::new(
            if safe_dir.x >= 0.0 { 1 } else { -1 },
            if safe_dir.y >= 0.0 { 1 } else { -1 },
            if safe_dir.z >= 0.0 { 1 } else { -1 },
        );

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
        let max_steps = model_size * 3;

        for i in 0..max_steps {
            if voxel.x < 0
                || voxel.x >= model_size as i32
                || voxel.y < 0
                || voxel.y >= model_size as i32
                || voxel.z < 0
                || voxel.z >= model_size as i32
            {
                break;
            }

            if self
                .scratch_pad
                .get_voxel(voxel.x as usize, voxel.y as usize, voxel.z as usize)
                != 0
            {
                let hit_axis = if i == 0 { entry_axis } else { stepped_axis };
                let mut normal = Vector3::zeros();
                normal[hit_axis] = -step[hit_axis];
                return Some((voxel, normal));
            }

            // Step to next voxel
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

    /// Computes all positions affected by the current mirror settings.
    ///
    /// For an NxNxN grid, mirroring across axis A means: `mirrored = (N-1) - original`
    /// Returns up to 8 positions when all three axes are mirrored.
    fn get_mirrored_positions(&self, pos: Vector3<i32>) -> Vec<Vector3<i32>> {
        let mut positions = vec![pos];
        let max_idx = (self.scratch_pad.size() - 1) as i32;

        // X mirror
        if self.mirror_axes[0] {
            let len = positions.len();
            for i in 0..len {
                let p = positions[i];
                let mirrored = Vector3::new(max_idx - p.x, p.y, p.z);
                if !positions.contains(&mirrored) {
                    positions.push(mirrored);
                }
            }
        }

        // Y mirror
        if self.mirror_axes[1] {
            let len = positions.len();
            for i in 0..len {
                let p = positions[i];
                let mirrored = Vector3::new(p.x, max_idx - p.y, p.z);
                if !positions.contains(&mirrored) {
                    positions.push(mirrored);
                }
            }
        }

        // Z mirror
        if self.mirror_axes[2] {
            let len = positions.len();
            for i in 0..len {
                let p = positions[i];
                let mirrored = Vector3::new(p.x, p.y, max_idx - p.z);
                if !positions.contains(&mirrored) {
                    positions.push(mirrored);
                }
            }
        }

        positions
    }

    /// Returns true if any mirror axis is enabled.
    pub fn is_mirror_enabled(&self) -> bool {
        self.mirror_axes[0] || self.mirror_axes[1] || self.mirror_axes[2]
    }

    /// Toggles mirror mode for the given axis.
    pub fn toggle_mirror(&mut self, axis: MirrorAxis) {
        let idx = match axis {
            MirrorAxis::X => 0,
            MirrorAxis::Y => 1,
            MirrorAxis::Z => 2,
        };
        self.mirror_axes[idx] = !self.mirror_axes[idx];
    }

    /// Places a voxel at the given position with the current palette index.
    ///
    /// Saves state to undo history if any voxel value actually changes.
    /// When mirror mode is enabled, places at all mirrored positions.
    pub fn place_voxel(&mut self, pos: Vector3<i32>) {
        let positions = self.get_mirrored_positions(pos);
        let model_size = self.scratch_pad.size() as i32;

        // Check if any position will actually change
        let mut any_change = false;
        for p in &positions {
            if p.x >= 0
                && p.x < model_size
                && p.y >= 0
                && p.y < model_size
                && p.z >= 0
                && p.z < model_size
            {
                let current = self
                    .scratch_pad
                    .get_voxel(p.x as usize, p.y as usize, p.z as usize);
                if current != self.selected_palette_index {
                    any_change = true;
                    break;
                }
            }
        }

        // Only save state and apply if something actually changes
        if any_change {
            self.save_state();
            for p in positions {
                if p.x >= 0
                    && p.x < model_size
                    && p.y >= 0
                    && p.y < model_size
                    && p.z >= 0
                    && p.z < model_size
                {
                    self.scratch_pad.set_voxel(
                        p.x as usize,
                        p.y as usize,
                        p.z as usize,
                        self.selected_palette_index,
                    );
                }
            }
        }
    }

    /// Removes a voxel at the given position.
    ///
    /// Saves state to undo history if there was a voxel to remove.
    /// When mirror mode is enabled, erases at all mirrored positions.
    pub fn erase_voxel(&mut self, pos: Vector3<i32>) {
        let positions = self.get_mirrored_positions(pos);
        let model_size = self.scratch_pad.size() as i32;

        // Check if any position has something to erase
        let mut any_change = false;
        for p in &positions {
            if p.x >= 0
                && p.x < model_size
                && p.y >= 0
                && p.y < model_size
                && p.z >= 0
                && p.z < model_size
            {
                let current = self
                    .scratch_pad
                    .get_voxel(p.x as usize, p.y as usize, p.z as usize);
                if current != 0 {
                    any_change = true;
                    break;
                }
            }
        }

        // Only save state and apply if something actually changes
        if any_change {
            self.save_state();
            for p in positions {
                if p.x >= 0
                    && p.x < model_size
                    && p.y >= 0
                    && p.y < model_size
                    && p.z >= 0
                    && p.z < model_size
                {
                    self.scratch_pad
                        .set_voxel(p.x as usize, p.y as usize, p.z as usize, 0);
                }
            }
        }
    }

    /// Picks the color at the given voxel position.
    pub fn pick_color(&mut self, pos: Vector3<i32>) {
        let model_size = self.scratch_pad.size() as i32;
        if pos.x >= 0
            && pos.x < model_size
            && pos.y >= 0
            && pos.y < model_size
            && pos.z >= 0
            && pos.z < model_size
        {
            let idx = self
                .scratch_pad
                .get_voxel(pos.x as usize, pos.y as usize, pos.z as usize);
            if idx != 0 {
                self.selected_palette_index = idx;
            }
        }
    }

    /// Changes the color of an existing voxel (without mirroring).
    pub fn change_voxel_color(&mut self, pos: Vector3<i32>) {
        let model_size = self.scratch_pad.size() as i32;
        if pos.x >= 0
            && pos.x < model_size
            && pos.y >= 0
            && pos.y < model_size
            && pos.z >= 0
            && pos.z < model_size
        {
            let current =
                self.scratch_pad
                    .get_voxel(pos.x as usize, pos.y as usize, pos.z as usize);
            // Only change if there's a voxel there and it's a different color
            if current != 0 && current != self.selected_palette_index {
                self.save_state();
                self.scratch_pad.set_voxel(
                    pos.x as usize,
                    pos.y as usize,
                    pos.z as usize,
                    self.selected_palette_index,
                );
            }
        }
    }

    /// Flood fills connected voxels starting from the given position.
    ///
    /// Fills all voxels of the same color that are connected (6-way connectivity).
    pub fn flood_fill(&mut self, start_pos: Vector3<i32>) {
        let model_size = self.scratch_pad.size() as i32;

        // Bounds check
        if start_pos.x < 0
            || start_pos.x >= model_size
            || start_pos.y < 0
            || start_pos.y >= model_size
            || start_pos.z < 0
            || start_pos.z >= model_size
        {
            return;
        }

        let start_color = self.scratch_pad.get_voxel(
            start_pos.x as usize,
            start_pos.y as usize,
            start_pos.z as usize,
        );

        // Can't fill air or if already the target color
        if start_color == 0 || start_color == self.selected_palette_index {
            return;
        }

        self.save_state();

        // Use BFS for flood fill
        let mut queue = std::collections::VecDeque::new();
        let mut visited = std::collections::HashSet::new();

        queue.push_back(start_pos);
        visited.insert((start_pos.x, start_pos.y, start_pos.z));

        // 6-way connectivity offsets
        let offsets = [
            Vector3::new(1, 0, 0),
            Vector3::new(-1, 0, 0),
            Vector3::new(0, 1, 0),
            Vector3::new(0, -1, 0),
            Vector3::new(0, 0, 1),
            Vector3::new(0, 0, -1),
        ];

        while let Some(pos) = queue.pop_front() {
            // Set this voxel to the new color
            self.scratch_pad.set_voxel(
                pos.x as usize,
                pos.y as usize,
                pos.z as usize,
                self.selected_palette_index,
            );

            // Check all 6 neighbors
            for offset in &offsets {
                let next = pos + offset;

                // Bounds check
                if next.x < 0
                    || next.x >= model_size
                    || next.y < 0
                    || next.y >= model_size
                    || next.z < 0
                    || next.z >= model_size
                {
                    continue;
                }

                // Skip if already visited
                if visited.contains(&(next.x, next.y, next.z)) {
                    continue;
                }

                // Check if same color as start
                let color =
                    self.scratch_pad
                        .get_voxel(next.x as usize, next.y as usize, next.z as usize);

                if color == start_color {
                    visited.insert((next.x, next.y, next.z));
                    queue.push_back(next);
                }
            }
        }
    }

    /// Places a filled cube centered at the given position.
    ///
    /// The cube size is determined by `shape_size` (diameter).
    /// Saves state to undo history before placing.
    pub fn place_cube(&mut self, center: Vector3<i32>) {
        let half = (self.shape_size / 2) as i32;
        let model_size = self.scratch_pad.size() as i32;
        let mut any_change = false;

        // Check if any position will actually change (including mirrored positions)
        for dz in -(half)..=(half) {
            for dy in -(half)..=(half) {
                for dx in -(half)..=(half) {
                    let pos = center + Vector3::new(dx, dy, dz);
                    let mirrored_positions = self.get_mirrored_positions(pos);
                    for p in &mirrored_positions {
                        if p.x >= 0
                            && p.x < model_size
                            && p.y >= 0
                            && p.y < model_size
                            && p.z >= 0
                            && p.z < model_size
                        {
                            let current = self.scratch_pad.get_voxel(
                                p.x as usize,
                                p.y as usize,
                                p.z as usize,
                            );
                            if current != self.selected_palette_index {
                                any_change = true;
                                break;
                            }
                        }
                    }
                    if any_change {
                        break;
                    }
                }
                if any_change {
                    break;
                }
            }
            if any_change {
                break;
            }
        }

        if any_change {
            self.save_state();
            for dz in -(half)..=(half) {
                for dy in -(half)..=(half) {
                    for dx in -(half)..=(half) {
                        let pos = center + Vector3::new(dx, dy, dz);
                        // Apply mirroring to each voxel position
                        let mirrored_positions = self.get_mirrored_positions(pos);
                        for p in mirrored_positions {
                            if p.x >= 0
                                && p.x < model_size
                                && p.y >= 0
                                && p.y < model_size
                                && p.z >= 0
                                && p.z < model_size
                            {
                                self.scratch_pad.set_voxel(
                                    p.x as usize,
                                    p.y as usize,
                                    p.z as usize,
                                    self.selected_palette_index,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// Places a filled sphere centered at the given position.
    ///
    /// The sphere diameter is determined by `shape_size`.
    /// Saves state to undo history before placing.
    pub fn place_sphere(&mut self, center: Vector3<i32>) {
        let radius = self.shape_size as f32 / 2.0;
        let radius_sq = radius * radius;
        let half = (self.shape_size / 2) as i32 + 1; // +1 to include edge voxels
        let model_size = self.scratch_pad.size() as i32;
        let mut any_change = false;

        // Check if any position will actually change (including mirrored positions)
        for dz in -(half)..=(half) {
            for dy in -(half)..=(half) {
                for dx in -(half)..=(half) {
                    let dist_sq = (dx as f32 + 0.5).powi(2)
                        + (dy as f32 + 0.5).powi(2)
                        + (dz as f32 + 0.5).powi(2);
                    if dist_sq <= radius_sq {
                        let pos = center + Vector3::new(dx, dy, dz);
                        let mirrored_positions = self.get_mirrored_positions(pos);
                        for p in &mirrored_positions {
                            if p.x >= 0
                                && p.x < model_size
                                && p.y >= 0
                                && p.y < model_size
                                && p.z >= 0
                                && p.z < model_size
                            {
                                let current = self.scratch_pad.get_voxel(
                                    p.x as usize,
                                    p.y as usize,
                                    p.z as usize,
                                );
                                if current != self.selected_palette_index {
                                    any_change = true;
                                    break;
                                }
                            }
                        }
                        if any_change {
                            break;
                        }
                    }
                }
                if any_change {
                    break;
                }
            }
            if any_change {
                break;
            }
        }

        if any_change {
            self.save_state();
            for dz in -(half)..=(half) {
                for dy in -(half)..=(half) {
                    for dx in -(half)..=(half) {
                        let dist_sq = (dx as f32 + 0.5).powi(2)
                            + (dy as f32 + 0.5).powi(2)
                            + (dz as f32 + 0.5).powi(2);
                        if dist_sq <= radius_sq {
                            let pos = center + Vector3::new(dx, dy, dz);
                            // Apply mirroring to each voxel position
                            let mirrored_positions = self.get_mirrored_positions(pos);
                            for p in mirrored_positions {
                                if p.x >= 0
                                    && p.x < model_size
                                    && p.y >= 0
                                    && p.y < model_size
                                    && p.z >= 0
                                    && p.z < model_size
                                {
                                    self.scratch_pad.set_voxel(
                                        p.x as usize,
                                        p.y as usize,
                                        p.z as usize,
                                        self.selected_palette_index,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Draws a line of voxels between two points using 3D Bresenham algorithm.
    ///
    /// Saves state to undo history before drawing.
    pub fn draw_bridge(&mut self, start: Vector3<i32>, end: Vector3<i32>) {
        let model_size = self.scratch_pad.size() as i32;

        // Calculate deltas
        let dx = (end.x - start.x).abs();
        let dy = (end.y - start.y).abs();
        let dz = (end.z - start.z).abs();

        // Calculate step directions
        let sx = if end.x > start.x { 1 } else { -1 };
        let sy = if end.y > start.y { 1 } else { -1 };
        let sz = if end.z > start.z { 1 } else { -1 };

        // Determine which axis is dominant
        let dm = dx.max(dy).max(dz);

        // Calculate error terms
        let mut x = start.x;
        let mut y = start.y;
        let mut z = start.z;

        let mut err_x = dm / 2;
        let mut err_y = dm / 2;
        let mut err_z = dm / 2;

        // Save state before drawing
        self.save_state();

        // Draw line using 3D Bresenham
        for _ in 0..=dm {
            let pos = Vector3::new(x, y, z);

            // Apply mirroring
            let mirrored_positions = self.get_mirrored_positions(pos);
            for p in mirrored_positions {
                if p.x >= 0
                    && p.x < model_size
                    && p.y >= 0
                    && p.y < model_size
                    && p.z >= 0
                    && p.z < model_size
                {
                    self.scratch_pad.set_voxel(
                        p.x as usize,
                        p.y as usize,
                        p.z as usize,
                        self.selected_palette_index,
                    );
                }
            }

            // Update error terms and positions
            err_x += dx;
            if err_x >= dm {
                err_x -= dm;
                x += sx;
            }

            err_y += dy;
            if err_y >= dm {
                err_y -= dm;
                y += sy;
            }

            err_z += dz;
            if err_z >= dm {
                err_z -= dm;
                z += sz;
            }
        }
    }

    /// Handles a left-click action based on current tool.
    pub fn on_left_click(&mut self) {
        match self.tool {
            EditorTool::Pencil => {
                if let Some(voxel) = self.hovered_voxel {
                    // Check if there's already a voxel at this position
                    let existing = self.scratch_pad.get_voxel(
                        voxel.x as usize,
                        voxel.y as usize,
                        voxel.z as usize,
                    );
                    if existing != 0 {
                        // Place adjacent to existing voxel based on hovered face normal
                        if let Some(normal) = self.hovered_normal {
                            let place_pos = voxel + normal;
                            let model_size = self.scratch_pad.size();
                            // Check bounds
                            if place_pos.x >= 0
                                && (place_pos.x as usize) < model_size
                                && place_pos.y >= 0
                                && (place_pos.y as usize) < model_size
                                && place_pos.z >= 0
                                && (place_pos.z as usize) < model_size
                            {
                                self.place_voxel(place_pos);
                            }
                        }
                    } else {
                        // Place at empty/floor position
                        self.place_voxel(voxel);
                    }
                }
            }
            EditorTool::Eraser => {
                if let Some(voxel) = self.hovered_voxel {
                    self.erase_voxel(voxel);
                }
            }
            EditorTool::Eyedropper => {
                if let Some(voxel) = self.hovered_voxel {
                    self.pick_color(voxel);
                    self.tool = EditorTool::Pencil; // Switch back to pencil
                }
            }
            EditorTool::Fill => {
                // Legacy fill tool - not used, replaced by PaintBucket
            }
            EditorTool::ColorChange => {
                if let Some(voxel) = self.hovered_voxel {
                    self.change_voxel_color(voxel);
                }
            }
            EditorTool::PaintBucket => {
                if let Some(voxel) = self.hovered_voxel {
                    self.flood_fill(voxel);
                }
            }
            EditorTool::Cube => {
                if let Some(voxel) = self.hovered_voxel {
                    // Check if there's already a voxel at this position
                    let existing = self.scratch_pad.get_voxel(
                        voxel.x as usize,
                        voxel.y as usize,
                        voxel.z as usize,
                    );
                    if existing != 0 {
                        // Place adjacent to existing voxel based on hovered face normal
                        if let Some(normal) = self.hovered_normal {
                            let place_pos = voxel + normal;
                            // Offset center so shape doesn't clip into existing voxel
                            let half = (self.shape_size / 2) as i32;
                            let center = place_pos + normal * half;
                            self.place_cube(center);
                        }
                    } else {
                        // Place at empty/floor position - offset so bottom touches floor
                        let half = (self.shape_size / 2) as i32;
                        let center = voxel + Vector3::new(0, half, 0);
                        self.place_cube(center);
                    }
                }
            }
            EditorTool::Sphere => {
                if let Some(voxel) = self.hovered_voxel {
                    // Check if there's already a voxel at this position
                    let existing = self.scratch_pad.get_voxel(
                        voxel.x as usize,
                        voxel.y as usize,
                        voxel.z as usize,
                    );
                    if existing != 0 {
                        // Place adjacent to existing voxel based on hovered face normal
                        if let Some(normal) = self.hovered_normal {
                            let place_pos = voxel + normal;
                            // Offset center so shape doesn't clip into existing voxel
                            let half = (self.shape_size / 2) as i32;
                            let center = place_pos + normal * half;
                            self.place_sphere(center);
                        }
                    } else {
                        // Place at empty/floor position - offset so bottom touches floor
                        let half = (self.shape_size / 2) as i32;
                        let center = voxel + Vector3::new(0, half, 0);
                        self.place_sphere(center);
                    }
                }
            }
            EditorTool::Bridge => {
                if let Some(voxel) = self.hovered_voxel {
                    if let Some(first_point) = self.bridge_first_point {
                        // Second click - draw line from first_point to voxel
                        self.draw_bridge(first_point, voxel);
                        self.bridge_first_point = None; // Clear for next bridge
                    } else {
                        // First click - store position
                        self.bridge_first_point = Some(voxel);
                    }
                }
            }
        }
    }

    /// Handles a right-click action (erase or cancel bridge).
    pub fn on_right_click(&mut self) {
        // Bridge tool: right-click cancels the bridge
        if self.tool == EditorTool::Bridge {
            self.bridge_first_point = None;
            return;
        }

        // All other tools: erase voxel
        if let Some(voxel) = self.hovered_voxel {
            self.erase_voxel(voxel);
        }
    }

    /// Handles a middle-click action (always pick color).
    pub fn on_middle_click(&mut self) {
        if let Some(voxel) = self.hovered_voxel {
            self.pick_color(voxel);
        }
    }

    /// Rotates the entire model 90 degrees clockwise around the Y axis.
    ///
    /// Transformation: (x, y, z) -> (SIZE-1-z, y, x)
    /// Saves state to undo history before rotating.
    pub fn rotate_model_y90(&mut self) {
        self.save_state();

        let size = self.scratch_pad.size();
        let mut new_voxels = vec![0u8; self.scratch_pad.volume()];

        for z in 0..size {
            for y in 0..size {
                for x in 0..size {
                    let old_idx = x + y * size + z * size * size;
                    let voxel = self.scratch_pad.voxels[old_idx];

                    // Rotate 90° CW around Y: (x, y, z) -> (SIZE-1-z, y, x)
                    let new_x = size - 1 - z;
                    let new_z = x;
                    let new_idx = new_x + y * size + new_z * size * size;

                    new_voxels[new_idx] = voxel;
                }
            }
        }

        self.scratch_pad.voxels = new_voxels;
    }

    /// Clears all voxels in the model.
    ///
    /// Saves state to undo history before clearing (if there are any voxels).
    pub fn clear_voxels(&mut self) {
        // Check if there's anything to clear
        let has_voxels = self.scratch_pad.voxels.iter().any(|&v| v != 0);
        if has_voxels {
            self.save_state();
            self.scratch_pad.voxels = vec![0; self.scratch_pad.volume()];
        }
    }

    /// Finalizes the model by computing collision mask.
    pub fn finalize_model(&mut self) {
        self.scratch_pad.compute_collision_mask();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_editor_state_new() {
        let editor = EditorState::new();
        assert!(!editor.active);
        assert_eq!(editor.selected_palette_index, 1);
        assert_eq!(editor.tool, EditorTool::Pencil);
    }

    #[test]
    fn test_toggle() {
        let mut editor = EditorState::new();
        assert!(!editor.active);
        editor.toggle();
        assert!(editor.active);
        editor.toggle();
        assert!(!editor.active);
    }

    #[test]
    fn test_camera_position() {
        let editor = EditorState::new();
        let pos = editor.camera_position();
        // Should be some distance from center
        let center = Vector3::new(
            SUB_VOXEL_CENTER_F32,
            SUB_VOXEL_CENTER_F32,
            SUB_VOXEL_CENTER_F32,
        );
        let dist = (pos - center).magnitude();
        assert!((dist - editor.orbit_distance).abs() < 0.1);
    }

    #[test]
    fn test_place_and_erase() {
        let mut editor = EditorState::new();
        let pos = Vector3::new(3, 3, 3);

        // Initially empty
        assert_eq!(editor.scratch_pad.get_voxel(3, 3, 3), 0);

        // Place voxel
        editor.place_voxel(pos);
        assert_eq!(editor.scratch_pad.get_voxel(3, 3, 3), 1);

        // Erase voxel
        editor.erase_voxel(pos);
        assert_eq!(editor.scratch_pad.get_voxel(3, 3, 3), 0);
    }

    #[test]
    fn test_pick_color() {
        let mut editor = EditorState::new();
        let pos = Vector3::new(2, 2, 2);

        // Place a voxel with palette index 5
        editor.selected_palette_index = 5;
        editor.place_voxel(pos);

        // Change selected color
        editor.selected_palette_index = 1;

        // Pick color from the placed voxel
        editor.pick_color(pos);
        assert_eq!(editor.selected_palette_index, 5);
    }

    #[test]
    fn test_raycast_empty() {
        let editor = EditorState::new();
        let origin = Vector3::new(-5.0, 4.0, 4.0);
        let direction = Vector3::new(1.0, 0.0, 0.0);

        // Empty model, should not hit anything
        let result = editor.raycast_voxel(origin, direction);
        assert!(result.is_none());
    }

    #[test]
    fn test_raycast_hit() {
        let mut editor = EditorState::new();

        // Place a voxel in the center
        editor.place_voxel(Vector3::new(4, 4, 4));

        // Cast ray from outside toward center
        let origin = Vector3::new(-5.0, 4.5, 4.5);
        let direction = Vector3::new(1.0, 0.0, 0.0);

        let result = editor.raycast_voxel(origin, direction);
        assert!(result.is_some());

        let (voxel, normal) = result.unwrap();
        assert_eq!(voxel, Vector3::new(4, 4, 4));
        assert_eq!(normal, Vector3::new(-1, 0, 0)); // Hit from -X side
    }

    #[test]
    fn test_undo_history_basic() {
        let mut history = UndoHistory::new();
        // Use small test vectors for simplicity
        let state1 = vec![1u8; 64];
        let state2 = vec![2u8; 64];
        let current = vec![3u8; 64];

        // Initially empty
        assert!(!history.can_undo());
        assert!(!history.can_redo());

        // Save a state
        history.save(&state1);
        assert!(history.can_undo());
        assert!(!history.can_redo());
        assert_eq!(history.undo_count(), 1);

        // Save another state
        history.save(&state2);
        assert_eq!(history.undo_count(), 2);

        // Undo once
        let restored = history.undo(&current);
        assert!(restored.is_some());
        assert_eq!(restored.unwrap(), state2);
        assert!(history.can_undo());
        assert!(history.can_redo());
        assert_eq!(history.undo_count(), 1);
        assert_eq!(history.redo_count(), 1);

        // Undo again
        let restored = history.undo(&vec![3u8; 64]);
        assert!(restored.is_some());
        assert_eq!(restored.unwrap(), state1);
        assert!(!history.can_undo());
        assert!(history.can_redo());

        // Redo
        let restored = history.redo(&vec![1u8; 64]);
        assert!(restored.is_some());
        assert!(history.can_undo());
    }

    #[test]
    fn test_undo_redo_truncates_on_new_action() {
        let mut history = UndoHistory::new();
        // Use small test vectors for simplicity
        let state1 = vec![1u8; 64];
        let state2 = vec![2u8; 64];
        let state3 = vec![3u8; 64];
        let current = vec![4u8; 64];

        // Build up some history
        history.save(&state1);
        history.save(&state2);
        history.save(&state3);
        assert_eq!(history.undo_count(), 3);

        // Undo twice
        history.undo(&current);
        history.undo(&vec![3u8; 64]);
        assert_eq!(history.redo_count(), 2);

        // Save a new state - should clear redo stack
        history.save(&vec![5u8; 64]);
        assert!(!history.can_redo());
        assert_eq!(history.redo_count(), 0);
    }

    #[test]
    fn test_editor_undo_place() {
        let mut editor = EditorState::new();
        let pos = Vector3::new(3, 3, 3);

        // Initially empty, no undo available
        assert!(!editor.can_undo());
        assert_eq!(editor.scratch_pad.get_voxel(3, 3, 3), 0);

        // Place a voxel
        editor.place_voxel(pos);
        assert_eq!(editor.scratch_pad.get_voxel(3, 3, 3), 1);
        assert!(editor.can_undo());

        // Undo
        let undone = editor.undo();
        assert!(undone);
        assert_eq!(editor.scratch_pad.get_voxel(3, 3, 3), 0);
        assert!(editor.can_redo());

        // Redo
        let redone = editor.redo();
        assert!(redone);
        assert_eq!(editor.scratch_pad.get_voxel(3, 3, 3), 1);
    }

    #[test]
    fn test_editor_undo_erase() {
        let mut editor = EditorState::new();
        let pos = Vector3::new(4, 4, 4);

        // Place a voxel first (don't count this in our test)
        editor.place_voxel(pos);
        assert_eq!(editor.scratch_pad.get_voxel(4, 4, 4), 1);

        // Clear undo history for clean test
        editor.history.clear();
        assert!(!editor.can_undo());

        // Erase the voxel
        editor.erase_voxel(pos);
        assert_eq!(editor.scratch_pad.get_voxel(4, 4, 4), 0);
        assert!(editor.can_undo());

        // Undo the erase
        editor.undo();
        assert_eq!(editor.scratch_pad.get_voxel(4, 4, 4), 1);
    }

    #[test]
    fn test_editor_undo_rotate() {
        let mut editor = EditorState::new();
        let max = (editor.scratch_pad.size() - 1) as i32;

        // Place a voxel at corner
        editor.place_voxel(Vector3::new(0, 0, 0));
        assert_eq!(editor.scratch_pad.get_voxel(0, 0, 0), 1);

        // Clear history
        editor.history.clear();

        // Rotate - voxel should move from (0,0,0) to (max,0,0) after 90° CW rotation
        editor.rotate_model_y90();
        assert_eq!(editor.scratch_pad.get_voxel(0, 0, 0), 0);
        assert_eq!(editor.scratch_pad.get_voxel(max as usize, 0, 0), 1);
        assert!(editor.can_undo());

        // Undo rotation
        editor.undo();
        assert_eq!(editor.scratch_pad.get_voxel(0, 0, 0), 1);
        assert_eq!(editor.scratch_pad.get_voxel(max as usize, 0, 0), 0);
    }

    #[test]
    fn test_editor_undo_clear() {
        let mut editor = EditorState::new();

        // Place some voxels
        editor.place_voxel(Vector3::new(1, 1, 1));
        editor.place_voxel(Vector3::new(2, 2, 2));
        editor.place_voxel(Vector3::new(3, 3, 3));

        // Clear history and then clear voxels
        editor.history.clear();
        editor.clear_voxels();

        // All voxels should be gone
        assert_eq!(editor.scratch_pad.get_voxel(1, 1, 1), 0);
        assert_eq!(editor.scratch_pad.get_voxel(2, 2, 2), 0);
        assert_eq!(editor.scratch_pad.get_voxel(3, 3, 3), 0);
        assert!(editor.can_undo());

        // Undo clear
        editor.undo();
        assert_eq!(editor.scratch_pad.get_voxel(1, 1, 1), 1);
        assert_eq!(editor.scratch_pad.get_voxel(2, 2, 2), 1);
        assert_eq!(editor.scratch_pad.get_voxel(3, 3, 3), 1);
    }

    #[test]
    fn test_editor_no_duplicate_undo_entries() {
        let mut editor = EditorState::new();
        let pos = Vector3::new(3, 3, 3);

        // Place a voxel
        editor.place_voxel(pos);
        assert_eq!(editor.history.undo_count(), 1);

        // Try to place same voxel with same color - should not create new undo entry
        editor.place_voxel(pos);
        assert_eq!(editor.history.undo_count(), 1);

        // Try to erase from empty position - should not create new undo entry
        editor.erase_voxel(Vector3::new(5, 5, 5));
        assert_eq!(editor.history.undo_count(), 1);
    }

    #[test]
    fn test_new_model_clears_history() {
        let mut editor = EditorState::new();

        // Build up some history
        editor.place_voxel(Vector3::new(1, 1, 1));
        editor.place_voxel(Vector3::new(2, 2, 2));
        assert!(editor.can_undo());

        // Create new model
        editor.new_model("test");
        assert!(!editor.can_undo());
        assert!(!editor.can_redo());
    }

    #[test]
    fn test_mirror_x_axis() {
        let mut editor = EditorState::new();
        let max_idx = editor.scratch_pad.size() - 1;
        let mirror_x = max_idx - 1; // Mirror of 1

        // Enable X mirroring
        editor.toggle_mirror(super::MirrorAxis::X);
        assert!(editor.mirror_axes[0]);
        assert!(editor.is_mirror_enabled());

        // Place voxel at (1, 2, 3) - should also place at (mirror_x, 2, 3)
        editor.place_voxel(Vector3::new(1, 2, 3));
        assert_eq!(editor.scratch_pad.get_voxel(1, 2, 3), 1);
        assert_eq!(editor.scratch_pad.get_voxel(mirror_x, 2, 3), 1);

        // Erase should also be mirrored
        editor.erase_voxel(Vector3::new(1, 2, 3));
        assert_eq!(editor.scratch_pad.get_voxel(1, 2, 3), 0);
        assert_eq!(editor.scratch_pad.get_voxel(mirror_x, 2, 3), 0);
    }

    #[test]
    fn test_mirror_y_axis() {
        let mut editor = EditorState::new();
        let max_idx = editor.scratch_pad.size() - 1;
        let mirror_y = max_idx - 1; // Mirror of 1

        // Enable Y mirroring
        editor.toggle_mirror(super::MirrorAxis::Y);
        assert!(editor.mirror_axes[1]);

        // Place voxel at (2, 1, 3) - should also place at (2, mirror_y, 3)
        editor.place_voxel(Vector3::new(2, 1, 3));
        assert_eq!(editor.scratch_pad.get_voxel(2, 1, 3), 1);
        assert_eq!(editor.scratch_pad.get_voxel(2, mirror_y, 3), 1);
    }

    #[test]
    fn test_mirror_z_axis() {
        let mut editor = EditorState::new();
        let max_idx = editor.scratch_pad.size() - 1;
        let mirror_z = max_idx - 1; // Mirror of 1

        // Enable Z mirroring
        editor.toggle_mirror(super::MirrorAxis::Z);
        assert!(editor.mirror_axes[2]);

        // Place voxel at (2, 3, 1) - should also place at (2, 3, mirror_z)
        editor.place_voxel(Vector3::new(2, 3, 1));
        assert_eq!(editor.scratch_pad.get_voxel(2, 3, 1), 1);
        assert_eq!(editor.scratch_pad.get_voxel(2, 3, mirror_z), 1);
    }

    #[test]
    fn test_mirror_multiple_axes() {
        let mut editor = EditorState::new();
        let m = editor.scratch_pad.size() - 1 - 1; // Mirror of 1

        // Enable X and Y mirroring (should place 4 voxels)
        editor.toggle_mirror(super::MirrorAxis::X);
        editor.toggle_mirror(super::MirrorAxis::Y);

        // Place voxel at (1, 1, 3)
        editor.place_voxel(Vector3::new(1, 1, 3));

        // Should have 4 voxels: original + X mirror + Y mirror + XY mirror
        assert_eq!(editor.scratch_pad.get_voxel(1, 1, 3), 1); // Original
        assert_eq!(editor.scratch_pad.get_voxel(m, 1, 3), 1); // X mirror
        assert_eq!(editor.scratch_pad.get_voxel(1, m, 3), 1); // Y mirror
        assert_eq!(editor.scratch_pad.get_voxel(m, m, 3), 1); // XY mirror
    }

    #[test]
    fn test_mirror_all_axes() {
        let mut editor = EditorState::new();
        let m = editor.scratch_pad.size() - 1 - 1; // Mirror of 1

        // Enable all three axes (should place 8 voxels)
        editor.toggle_mirror(super::MirrorAxis::X);
        editor.toggle_mirror(super::MirrorAxis::Y);
        editor.toggle_mirror(super::MirrorAxis::Z);

        // Place voxel at (1, 1, 1)
        editor.place_voxel(Vector3::new(1, 1, 1));

        // Should have 8 voxels (2^3)
        assert_eq!(editor.scratch_pad.get_voxel(1, 1, 1), 1); // Original
        assert_eq!(editor.scratch_pad.get_voxel(m, 1, 1), 1); // X
        assert_eq!(editor.scratch_pad.get_voxel(1, m, 1), 1); // Y
        assert_eq!(editor.scratch_pad.get_voxel(1, 1, m), 1); // Z
        assert_eq!(editor.scratch_pad.get_voxel(m, m, 1), 1); // XY
        assert_eq!(editor.scratch_pad.get_voxel(m, 1, m), 1); // XZ
        assert_eq!(editor.scratch_pad.get_voxel(1, m, m), 1); // YZ
        assert_eq!(editor.scratch_pad.get_voxel(m, m, m), 1); // XYZ
    }

    #[test]
    fn test_mirror_center_voxel() {
        let mut editor = EditorState::new();
        let model_size = editor.scratch_pad.size();
        let max_idx = model_size - 1;
        let near_center = model_size / 2 - 1;
        let mirror_near_center = max_idx - near_center;

        // Enable X mirroring
        editor.toggle_mirror(super::MirrorAxis::X);

        // Place voxel near center X - mirror should be symmetric
        // Since we use max_idx - x, position near_center mirrors to mirror_near_center
        editor.place_voxel(Vector3::new(near_center as i32, 3, 3));
        assert_eq!(editor.scratch_pad.get_voxel(near_center, 3, 3), 1);
        assert_eq!(editor.scratch_pad.get_voxel(mirror_near_center, 3, 3), 1);
    }

    #[test]
    fn test_mirror_toggle() {
        let mut editor = EditorState::new();

        // Initially all disabled
        assert!(!editor.is_mirror_enabled());

        // Toggle X on
        editor.toggle_mirror(super::MirrorAxis::X);
        assert!(editor.mirror_axes[0]);
        assert!(editor.is_mirror_enabled());

        // Toggle X off
        editor.toggle_mirror(super::MirrorAxis::X);
        assert!(!editor.mirror_axes[0]);
        assert!(!editor.is_mirror_enabled());
    }

    #[test]
    fn test_mirror_undo_all_placements() {
        let mut editor = EditorState::new();
        let m = editor.scratch_pad.size() - 1 - 1; // Mirror of 1

        // Enable X mirroring
        editor.toggle_mirror(super::MirrorAxis::X);

        // Place voxel (should create 2 voxels)
        editor.place_voxel(Vector3::new(1, 2, 3));
        assert_eq!(editor.scratch_pad.get_voxel(1, 2, 3), 1);
        assert_eq!(editor.scratch_pad.get_voxel(m, 2, 3), 1);
        assert_eq!(editor.history.undo_count(), 1); // Only one undo entry

        // Undo should remove both voxels
        editor.undo();
        assert_eq!(editor.scratch_pad.get_voxel(1, 2, 3), 0);
        assert_eq!(editor.scratch_pad.get_voxel(m, 2, 3), 0);
    }
}

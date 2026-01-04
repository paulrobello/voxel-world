//! In-game model editor for creating 8x8x8 sub-voxel models.
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

use crate::sub_voxel::{Color, SUB_VOXEL_SIZE, SubVoxelModel};
use nalgebra::Vector3;

pub use ui::{EditorAction, draw_editor_ui, draw_model_preview};

/// Maximum number of undo states to keep per stack.
const MAX_UNDO_HISTORY: usize = 100;

/// Type alias for voxel state snapshot.
type VoxelSnapshot = [u8; SUB_VOXEL_SIZE * SUB_VOXEL_SIZE * SUB_VOXEL_SIZE];

/// Manages undo/redo history for the model editor.
///
/// Uses a dual-stack approach with separate undo and redo stacks.
/// Each stack stores full voxel array snapshots (512 bytes each).
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

    /// Saves the current state before a modification.
    ///
    /// This clears the redo stack since we're branching from this point.
    pub fn save(&mut self, current_voxels: VoxelSnapshot) {
        // Clear redo stack - we're branching
        self.redo_stack.clear();

        // Add to undo stack
        self.undo_stack.push(current_voxels);

        // Trim oldest states if we exceed max
        if self.undo_stack.len() > MAX_UNDO_HISTORY {
            self.undo_stack.remove(0);
        }
    }

    /// Performs an undo operation.
    ///
    /// Takes the current state and returns the previous state to restore.
    /// Returns `None` if there's nothing to undo.
    pub fn undo(&mut self, current_voxels: VoxelSnapshot) -> Option<VoxelSnapshot> {
        if let Some(previous) = self.undo_stack.pop() {
            // Save current for redo
            self.redo_stack.push(current_voxels);
            Some(previous)
        } else {
            None
        }
    }

    /// Performs a redo operation.
    ///
    /// Takes the current state and returns the next state to restore.
    /// Returns `None` if there's nothing to redo.
    pub fn redo(&mut self, current_voxels: VoxelSnapshot) -> Option<VoxelSnapshot> {
        if let Some(next) = self.redo_stack.pop() {
            // Save current for undo
            self.undo_stack.push(current_voxels);
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

    /// Clears all history.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
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

    /// Undo/redo history for voxel operations.
    pub history: UndoHistory,
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
            orbit_yaw: std::f32::consts::FRAC_PI_4,
            orbit_pitch: std::f32::consts::FRAC_PI_6,
            orbit_distance: 12.0,
            is_dragging: false,
            last_mouse_pos: None,
            hovered_voxel: None,
            hovered_normal: None,
            saved_target_pos: None,
            show_overwrite_confirm: false,
            history: UndoHistory::new(),
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
        let palette = self.scratch_pad.palette;
        self.scratch_pad = SubVoxelModel::new(name);
        self.scratch_pad.palette = palette;
        self.hovered_voxel = None;
        self.history.clear();
    }

    /// Loads a model into the scratch pad for editing.
    pub fn load_model(&mut self, model: &SubVoxelModel) {
        self.scratch_pad = model.clone();
        self.hovered_voxel = None;
        self.history.clear();
    }

    /// Saves the current voxel state to the undo history.
    ///
    /// Call this before making any modifications to the model.
    pub fn save_state(&mut self) {
        self.history.save(self.scratch_pad.voxels);
    }

    /// Undoes the last voxel operation.
    ///
    /// Returns true if an undo was performed.
    pub fn undo(&mut self) -> bool {
        if let Some(previous) = self.history.undo(self.scratch_pad.voxels) {
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
        if let Some(next) = self.history.redo(self.scratch_pad.voxels) {
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
        let center = Vector3::new(4.0, 4.0, 4.0); // Center of 8x8x8 grid

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
        Vector3::new(4.0, 4.0, 4.0)
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

    /// Performs a ray-voxel intersection test against the 8x8x8 grid.
    ///
    /// Returns (voxel_pos, face_normal) if a solid voxel was hit.
    pub fn raycast_voxel(
        &self,
        origin: Vector3<f32>,
        direction: Vector3<f32>,
    ) -> Option<(Vector3<i32>, Vector3<i32>)> {
        // DDA algorithm for 8x8x8 grid
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

        // Calculate entry/exit t for the 0-8 cube
        let t_min_v = (Vector3::new(-0.001, -0.001, -0.001) - origin).component_mul(&inv_dir);
        let t_max_v = (Vector3::new(8.001, 8.001, 8.001) - origin).component_mul(&inv_dir);

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
        current_pos = current_pos.map(|v| v.clamp(0.001, 7.999));

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

    /// Places a voxel at the given position with the current palette index.
    ///
    /// Saves state to undo history if the voxel value actually changes.
    pub fn place_voxel(&mut self, pos: Vector3<i32>) {
        if pos.x >= 0
            && pos.x < SUB_VOXEL_SIZE as i32
            && pos.y >= 0
            && pos.y < SUB_VOXEL_SIZE as i32
            && pos.z >= 0
            && pos.z < SUB_VOXEL_SIZE as i32
        {
            let current =
                self.scratch_pad
                    .get_voxel(pos.x as usize, pos.y as usize, pos.z as usize);
            // Only save state if actually changing the voxel
            if current != self.selected_palette_index {
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

    /// Removes a voxel at the given position.
    ///
    /// Saves state to undo history if there was a voxel to remove.
    pub fn erase_voxel(&mut self, pos: Vector3<i32>) {
        if pos.x >= 0
            && pos.x < SUB_VOXEL_SIZE as i32
            && pos.y >= 0
            && pos.y < SUB_VOXEL_SIZE as i32
            && pos.z >= 0
            && pos.z < SUB_VOXEL_SIZE as i32
        {
            let current =
                self.scratch_pad
                    .get_voxel(pos.x as usize, pos.y as usize, pos.z as usize);
            // Only save state if there's actually something to erase
            if current != 0 {
                self.save_state();
                self.scratch_pad
                    .set_voxel(pos.x as usize, pos.y as usize, pos.z as usize, 0);
            }
        }
    }

    /// Picks the color at the given voxel position.
    pub fn pick_color(&mut self, pos: Vector3<i32>) {
        if pos.x >= 0
            && pos.x < SUB_VOXEL_SIZE as i32
            && pos.y >= 0
            && pos.y < SUB_VOXEL_SIZE as i32
            && pos.z >= 0
            && pos.z < SUB_VOXEL_SIZE as i32
        {
            let idx = self
                .scratch_pad
                .get_voxel(pos.x as usize, pos.y as usize, pos.z as usize);
            if idx != 0 {
                self.selected_palette_index = idx;
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
                            // Check bounds
                            if place_pos.x >= 0
                                && (place_pos.x as usize) < SUB_VOXEL_SIZE
                                && place_pos.y >= 0
                                && (place_pos.y as usize) < SUB_VOXEL_SIZE
                                && place_pos.z >= 0
                                && (place_pos.z as usize) < SUB_VOXEL_SIZE
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
                // TODO: Implement flood fill
            }
        }
    }

    /// Handles a right-click action (always erase).
    pub fn on_right_click(&mut self) {
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

        let mut new_voxels = [0u8; SUB_VOXEL_SIZE * SUB_VOXEL_SIZE * SUB_VOXEL_SIZE];

        for z in 0..SUB_VOXEL_SIZE {
            for y in 0..SUB_VOXEL_SIZE {
                for x in 0..SUB_VOXEL_SIZE {
                    let old_idx = x + y * SUB_VOXEL_SIZE + z * SUB_VOXEL_SIZE * SUB_VOXEL_SIZE;
                    let voxel = self.scratch_pad.voxels[old_idx];

                    // Rotate 90° CW around Y: (x, y, z) -> (SIZE-1-z, y, x)
                    let new_x = SUB_VOXEL_SIZE - 1 - z;
                    let new_z = x;
                    let new_idx =
                        new_x + y * SUB_VOXEL_SIZE + new_z * SUB_VOXEL_SIZE * SUB_VOXEL_SIZE;

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
            self.scratch_pad.voxels = [0; SUB_VOXEL_SIZE * SUB_VOXEL_SIZE * SUB_VOXEL_SIZE];
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
        let center = Vector3::new(4.0, 4.0, 4.0);
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
        let state1 = [1u8; 512];
        let state2 = [2u8; 512];
        let current = [3u8; 512];

        // Initially empty
        assert!(!history.can_undo());
        assert!(!history.can_redo());

        // Save a state
        history.save(state1);
        assert!(history.can_undo());
        assert!(!history.can_redo());
        assert_eq!(history.undo_count(), 1);

        // Save another state
        history.save(state2);
        assert_eq!(history.undo_count(), 2);

        // Undo once
        let restored = history.undo(current);
        assert!(restored.is_some());
        assert_eq!(restored.unwrap(), state2);
        assert!(history.can_undo());
        assert!(history.can_redo());
        assert_eq!(history.undo_count(), 1);
        assert_eq!(history.redo_count(), 1);

        // Undo again
        let restored = history.undo([3u8; 512]);
        assert!(restored.is_some());
        assert_eq!(restored.unwrap(), state1);
        assert!(!history.can_undo());
        assert!(history.can_redo());

        // Redo
        let restored = history.redo([1u8; 512]);
        assert!(restored.is_some());
        assert!(history.can_undo());
    }

    #[test]
    fn test_undo_redo_truncates_on_new_action() {
        let mut history = UndoHistory::new();
        let state1 = [1u8; 512];
        let state2 = [2u8; 512];
        let state3 = [3u8; 512];
        let current = [4u8; 512];

        // Build up some history
        history.save(state1);
        history.save(state2);
        history.save(state3);
        assert_eq!(history.undo_count(), 3);

        // Undo twice
        history.undo(current);
        history.undo([3u8; 512]);
        assert_eq!(history.redo_count(), 2);

        // Save a new state - should clear redo stack
        history.save([5u8; 512]);
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

        // Place a voxel at corner
        editor.place_voxel(Vector3::new(0, 0, 0));
        assert_eq!(editor.scratch_pad.get_voxel(0, 0, 0), 1);

        // Clear history
        editor.history.clear();

        // Rotate - voxel should move from (0,0,0) to (7,0,0) after 90° CW rotation
        editor.rotate_model_y90();
        assert_eq!(editor.scratch_pad.get_voxel(0, 0, 0), 0);
        assert_eq!(editor.scratch_pad.get_voxel(7, 0, 0), 1);
        assert!(editor.can_undo());

        // Undo rotation
        editor.undo();
        assert_eq!(editor.scratch_pad.get_voxel(0, 0, 0), 1);
        assert_eq!(editor.scratch_pad.get_voxel(7, 0, 0), 0);
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
}

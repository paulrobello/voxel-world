//! In-game model editor for creating 8x8x8 sub-voxel models.
//!
//! When active, the editor:
//! - Disables player movement and physics
//! - Switches to an orbit camera around the model being edited
//! - Shows egui panels for tools, palette, and library
//! - Allows placing/removing voxels with mouse clicks

#![allow(dead_code)] // WIP: Full integration pending

pub mod ui;

use crate::sub_voxel::{Color, SUB_VOXEL_SIZE, SubVoxelModel};
use nalgebra::Vector3;

pub use ui::{draw_editor_ui, draw_model_preview};

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
        }
    }

    /// Toggles the editor on/off.
    pub fn toggle(&mut self) {
        self.active = !self.active;
        if self.active {
            self.is_dragging = false;
            self.last_mouse_pos = None;
        }
    }

    /// Resets the scratch pad to a new empty model.
    pub fn new_model(&mut self, name: &str) {
        let palette = self.scratch_pad.palette;
        self.scratch_pad = SubVoxelModel::new(name);
        self.scratch_pad.palette = palette;
        self.hovered_voxel = None;
    }

    /// Loads a model into the scratch pad for editing.
    pub fn load_model(&mut self, model: &SubVoxelModel) {
        self.scratch_pad = model.clone();
        self.hovered_voxel = None;
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
    pub fn place_voxel(&mut self, pos: Vector3<i32>) {
        if pos.x >= 0
            && pos.x < SUB_VOXEL_SIZE as i32
            && pos.y >= 0
            && pos.y < SUB_VOXEL_SIZE as i32
            && pos.z >= 0
            && pos.z < SUB_VOXEL_SIZE as i32
        {
            self.scratch_pad.set_voxel(
                pos.x as usize,
                pos.y as usize,
                pos.z as usize,
                self.selected_palette_index,
            );
        }
    }

    /// Removes a voxel at the given position.
    pub fn erase_voxel(&mut self, pos: Vector3<i32>) {
        if pos.x >= 0
            && pos.x < SUB_VOXEL_SIZE as i32
            && pos.y >= 0
            && pos.y < SUB_VOXEL_SIZE as i32
            && pos.z >= 0
            && pos.z < SUB_VOXEL_SIZE as i32
        {
            self.scratch_pad
                .set_voxel(pos.x as usize, pos.y as usize, pos.z as usize, 0);
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
                // Place voxel adjacent to hovered face
                if let (Some(voxel), Some(normal)) = (self.hovered_voxel, self.hovered_normal) {
                    let place_pos = voxel + normal;
                    self.place_voxel(place_pos);
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
}

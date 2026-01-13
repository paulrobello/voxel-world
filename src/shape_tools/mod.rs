//! Shape tools for placing geometric shapes in the world.
//!
//! This module provides tools for placing spheres, cubes, cylinders, bridges, and other shapes
//! with holographic previews and configurable parameters.

pub mod bridge;
pub mod cube;
pub mod cylinder;
pub mod sphere;

use nalgebra::Vector3;

/// Placement mode for shape tools.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlacementMode {
    /// Shape center at target position.
    #[default]
    Center,
    /// Shape bottom rests on target surface (center offset by +size in Y).
    Base,
}

/// State for the sphere placement tool.
#[derive(Clone, Debug)]
pub struct SphereToolState {
    /// Whether the sphere tool is currently active.
    pub active: bool,
    /// Sphere radius in blocks (1-50).
    pub radius: i32,
    /// Whether to create a hollow shell instead of solid sphere.
    pub hollow: bool,
    /// Whether to create only the top half (dome mode).
    pub dome: bool,
    /// Placement mode (center or base).
    pub placement_mode: PlacementMode,
    /// Cached preview positions for GPU upload.
    pub preview_positions: Vec<Vector3<i32>>,
    /// Current preview center position (if targeting a block).
    pub preview_center: Option<Vector3<i32>>,
    /// Total block count for the full sphere (may differ from preview if truncated).
    pub total_blocks: usize,
    /// Whether the preview was truncated due to exceeding buffer limit.
    pub preview_truncated: bool,
    /// Cached radius for detecting when to regenerate preview.
    cached_radius: i32,
    /// Cached hollow setting for detecting when to regenerate preview.
    cached_hollow: bool,
    /// Cached dome setting for detecting when to regenerate preview.
    cached_dome: bool,
    /// Cached placement mode for detecting when to regenerate preview.
    cached_placement_mode: PlacementMode,
}

impl Default for SphereToolState {
    fn default() -> Self {
        Self {
            active: false,
            radius: 5,
            hollow: false,
            dome: false,
            placement_mode: PlacementMode::Center,
            preview_positions: Vec::new(),
            preview_center: None,
            total_blocks: 0,
            preview_truncated: false,
            cached_radius: 5,
            cached_hollow: false,
            cached_dome: false,
            cached_placement_mode: PlacementMode::Center,
        }
    }
}

impl SphereToolState {
    /// Check if settings have changed since last preview generation.
    pub fn settings_changed(&self) -> bool {
        self.radius != self.cached_radius
            || self.hollow != self.cached_hollow
            || self.dome != self.cached_dome
            || self.placement_mode != self.cached_placement_mode
    }

    /// Update cached settings after regenerating preview.
    pub fn update_cache(&mut self) {
        self.cached_radius = self.radius;
        self.cached_hollow = self.hollow;
        self.cached_dome = self.dome;
        self.cached_placement_mode = self.placement_mode;
    }

    /// Clear the preview state.
    pub fn clear_preview(&mut self) {
        self.preview_positions.clear();
        self.preview_center = None;
        self.preview_truncated = false;
        self.total_blocks = 0;
    }

    /// Deactivate the tool and clear preview.
    pub fn deactivate(&mut self) {
        self.active = false;
        self.clear_preview();
    }

    /// Update the sphere preview at the given target position.
    ///
    /// Regenerates preview positions when target or settings change.
    pub fn update_preview(&mut self, target: Vector3<i32>) {
        use crate::gpu_resources::MAX_STENCIL_BLOCKS;

        let center = sphere::calculate_center(target, self.radius, self.placement_mode);

        // Only regenerate if center or settings changed
        let needs_regen = self.preview_center != Some(center) || self.settings_changed();

        if needs_regen {
            self.preview_center = Some(center);
            self.update_cache();

            let all_positions =
                sphere::generate_sphere_positions(center, self.radius, self.hollow, self.dome);

            // Track total count and truncation status
            self.total_blocks = all_positions.len();
            self.preview_truncated = all_positions.len() > MAX_STENCIL_BLOCKS;

            // Truncate for preview (full list used for actual placement)
            if all_positions.len() > MAX_STENCIL_BLOCKS {
                self.preview_positions = all_positions[..MAX_STENCIL_BLOCKS].to_vec();
            } else {
                self.preview_positions = all_positions;
            }
        }
    }
}

/// State for the cube placement tool.
#[derive(Clone, Debug)]
pub struct CubeToolState {
    /// Whether the cube tool is currently active.
    pub active: bool,
    /// Half-size in X direction (full width = size_x * 2 + 1).
    pub size_x: i32,
    /// Half-size in Y direction (full height = size_y * 2 + 1).
    pub size_y: i32,
    /// Half-size in Z direction (full depth = size_z * 2 + 1).
    pub size_z: i32,
    /// Whether to create a hollow shell instead of solid cube.
    pub hollow: bool,
    /// Whether to create only the top half (dome mode).
    pub dome: bool,
    /// Placement mode (center or base).
    pub placement_mode: PlacementMode,
    /// Cached preview positions for GPU upload.
    pub preview_positions: Vec<Vector3<i32>>,
    /// Current preview center position (if targeting a block).
    pub preview_center: Option<Vector3<i32>>,
    /// Total block count for the full cube (may differ from preview if truncated).
    pub total_blocks: usize,
    /// Whether the preview was truncated due to exceeding buffer limit.
    pub preview_truncated: bool,
    /// Cached size_x for detecting when to regenerate preview.
    cached_size_x: i32,
    /// Cached size_y for detecting when to regenerate preview.
    cached_size_y: i32,
    /// Cached size_z for detecting when to regenerate preview.
    cached_size_z: i32,
    /// Cached hollow setting for detecting when to regenerate preview.
    cached_hollow: bool,
    /// Cached dome setting for detecting when to regenerate preview.
    cached_dome: bool,
    /// Cached placement mode for detecting when to regenerate preview.
    cached_placement_mode: PlacementMode,
}

impl Default for CubeToolState {
    fn default() -> Self {
        Self {
            active: false,
            size_x: 5,
            size_y: 5,
            size_z: 5,
            hollow: false,
            dome: false,
            placement_mode: PlacementMode::Center,
            preview_positions: Vec::new(),
            preview_center: None,
            total_blocks: 0,
            preview_truncated: false,
            cached_size_x: 5,
            cached_size_y: 5,
            cached_size_z: 5,
            cached_hollow: false,
            cached_dome: false,
            cached_placement_mode: PlacementMode::Center,
        }
    }
}

impl CubeToolState {
    /// Check if settings have changed since last preview generation.
    pub fn settings_changed(&self) -> bool {
        self.size_x != self.cached_size_x
            || self.size_y != self.cached_size_y
            || self.size_z != self.cached_size_z
            || self.hollow != self.cached_hollow
            || self.dome != self.cached_dome
            || self.placement_mode != self.cached_placement_mode
    }

    /// Update cached settings after regenerating preview.
    pub fn update_cache(&mut self) {
        self.cached_size_x = self.size_x;
        self.cached_size_y = self.size_y;
        self.cached_size_z = self.size_z;
        self.cached_hollow = self.hollow;
        self.cached_dome = self.dome;
        self.cached_placement_mode = self.placement_mode;
    }

    /// Clear the preview state.
    pub fn clear_preview(&mut self) {
        self.preview_positions.clear();
        self.preview_center = None;
        self.preview_truncated = false;
        self.total_blocks = 0;
    }

    /// Deactivate the tool and clear preview.
    pub fn deactivate(&mut self) {
        self.active = false;
        self.clear_preview();
    }

    /// Update the cube preview at the given target position.
    ///
    /// Regenerates preview positions when target or settings change.
    pub fn update_preview(&mut self, target: Vector3<i32>) {
        use crate::gpu_resources::MAX_STENCIL_BLOCKS;

        let center = cube::calculate_center(target, self.size_y, self.placement_mode);

        // Only regenerate if center or settings changed
        let needs_regen = self.preview_center != Some(center) || self.settings_changed();

        if needs_regen {
            self.preview_center = Some(center);
            self.update_cache();

            let all_positions = cube::generate_cube_positions(
                center,
                self.size_x,
                self.size_y,
                self.size_z,
                self.hollow,
                self.dome,
            );

            // Track total count and truncation status
            self.total_blocks = all_positions.len();
            self.preview_truncated = all_positions.len() > MAX_STENCIL_BLOCKS;

            // Truncate for preview (full list used for actual placement)
            if all_positions.len() > MAX_STENCIL_BLOCKS {
                self.preview_positions = all_positions[..MAX_STENCIL_BLOCKS].to_vec();
            } else {
                self.preview_positions = all_positions;
            }
        }
    }
}

/// State for the bridge (line) placement tool.
#[derive(Clone, Debug, Default)]
pub struct BridgeToolState {
    /// Whether the bridge tool is currently active.
    pub active: bool,
    /// Starting position for the bridge (set on first right-click).
    pub start_position: Option<Vector3<i32>>,
    /// Cached preview positions for GPU upload.
    pub preview_positions: Vec<Vector3<i32>>,
    /// Current end position for preview (crosshair target).
    pub preview_end: Option<Vector3<i32>>,
    /// Total block count for the line.
    pub total_blocks: usize,
}

impl BridgeToolState {
    /// Clear the preview state.
    pub fn clear_preview(&mut self) {
        self.preview_positions.clear();
        self.preview_end = None;
        self.total_blocks = 0;
    }

    /// Cancel the bridge (clear start position and preview).
    pub fn cancel(&mut self) {
        self.start_position = None;
        self.clear_preview();
    }

    /// Deactivate the tool and clear all state.
    pub fn deactivate(&mut self) {
        self.active = false;
        self.start_position = None;
        self.clear_preview();
    }

    /// Update the bridge preview from start to the given target position.
    ///
    /// Only generates preview if start_position is set.
    pub fn update_preview(&mut self, target: Vector3<i32>) {
        if let Some(start) = self.start_position {
            // Only regenerate if end changed
            if self.preview_end != Some(target) {
                self.preview_end = Some(target);
                self.preview_positions = bridge::generate_line_positions(start, target);
                self.total_blocks = self.preview_positions.len();
            }
        } else {
            self.clear_preview();
        }
    }
}

/// State for the cylinder placement tool.
#[derive(Clone, Debug)]
pub struct CylinderToolState {
    /// Whether the cylinder tool is currently active.
    pub active: bool,
    /// Cylinder radius in blocks (1-50).
    pub radius: i32,
    /// Cylinder height/length in blocks (1-100).
    pub height: i32,
    /// Whether to create a hollow tube instead of solid cylinder.
    pub hollow: bool,
    /// Axis orientation (Y=vertical, X/Z=horizontal).
    pub axis: cylinder::CylinderAxis,
    /// Placement mode (center or base).
    pub placement_mode: PlacementMode,
    /// Cached preview positions for GPU upload.
    pub preview_positions: Vec<Vector3<i32>>,
    /// Current preview center position (if targeting a block).
    pub preview_center: Option<Vector3<i32>>,
    /// Total block count for the full cylinder (may differ from preview if truncated).
    pub total_blocks: usize,
    /// Whether the preview was truncated due to exceeding buffer limit.
    pub preview_truncated: bool,
    /// Cached radius for detecting when to regenerate preview.
    cached_radius: i32,
    /// Cached height for detecting when to regenerate preview.
    cached_height: i32,
    /// Cached hollow setting for detecting when to regenerate preview.
    cached_hollow: bool,
    /// Cached axis for detecting when to regenerate preview.
    cached_axis: cylinder::CylinderAxis,
    /// Cached placement mode for detecting when to regenerate preview.
    cached_placement_mode: PlacementMode,
}

impl Default for CylinderToolState {
    fn default() -> Self {
        Self {
            active: false,
            radius: 3,
            height: 10,
            hollow: false,
            axis: cylinder::CylinderAxis::Y,
            placement_mode: PlacementMode::Base,
            preview_positions: Vec::new(),
            preview_center: None,
            total_blocks: 0,
            preview_truncated: false,
            cached_radius: 3,
            cached_height: 10,
            cached_hollow: false,
            cached_axis: cylinder::CylinderAxis::Y,
            cached_placement_mode: PlacementMode::Base,
        }
    }
}

impl CylinderToolState {
    /// Check if settings have changed since last preview generation.
    pub fn settings_changed(&self) -> bool {
        self.radius != self.cached_radius
            || self.height != self.cached_height
            || self.hollow != self.cached_hollow
            || self.axis != self.cached_axis
            || self.placement_mode != self.cached_placement_mode
    }

    /// Update cached settings after regenerating preview.
    pub fn update_cache(&mut self) {
        self.cached_radius = self.radius;
        self.cached_height = self.height;
        self.cached_hollow = self.hollow;
        self.cached_axis = self.axis;
        self.cached_placement_mode = self.placement_mode;
    }

    /// Clear the preview state.
    pub fn clear_preview(&mut self) {
        self.preview_positions.clear();
        self.preview_center = None;
        self.preview_truncated = false;
        self.total_blocks = 0;
    }

    /// Deactivate the tool and clear preview.
    pub fn deactivate(&mut self) {
        self.active = false;
        self.clear_preview();
    }

    /// Update the cylinder preview at the given target position.
    ///
    /// Regenerates preview positions when target or settings change.
    pub fn update_preview(&mut self, target: Vector3<i32>) {
        use crate::gpu_resources::MAX_STENCIL_BLOCKS;

        let center = cylinder::calculate_center(
            target,
            self.radius,
            self.height,
            self.axis,
            self.placement_mode,
        );

        // Only regenerate if center or settings changed
        let needs_regen = self.preview_center != Some(center) || self.settings_changed();

        if needs_regen {
            self.preview_center = Some(center);
            self.update_cache();

            let all_positions = cylinder::generate_cylinder_positions(
                center,
                self.radius,
                self.height,
                self.hollow,
                self.axis,
            );

            // Track total count and truncation status
            self.total_blocks = all_positions.len();
            self.preview_truncated = all_positions.len() > MAX_STENCIL_BLOCKS;

            // Truncate for preview (full list used for actual placement)
            if all_positions.len() > MAX_STENCIL_BLOCKS {
                self.preview_positions = all_positions[..MAX_STENCIL_BLOCKS].to_vec();
            } else {
                self.preview_positions = all_positions;
            }
        }
    }
}

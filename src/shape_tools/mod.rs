//! Shape tools for placing geometric shapes in the world.
//!
//! This module provides tools for placing spheres, cubes, cylinders, bridges, and other shapes
//! with holographic previews and configurable parameters.

pub mod arch;
pub mod bezier;
pub mod bridge;
pub mod circle;
pub mod clone;
pub mod cone;
pub mod cube;
pub mod cylinder;
pub mod floor;
pub mod helix;
pub mod hollow;
pub mod mirror;
pub mod pattern;
pub mod polygon;
pub mod replace;
pub mod scatter;
pub mod sphere;
pub mod stairs;
pub mod torus;
pub mod wall;

// Re-export tool state structs from their respective modules
pub use bezier::BezierToolState;
pub use helix::HelixToolState;
pub use hollow::HollowToolState;
pub use pattern::{PatternFillState, PatternType};
pub use polygon::PolygonToolState;
pub use scatter::ScatterToolState;
pub use sphere::SphereToolState;
pub use torus::TorusToolState;

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

/// State for the wall placement tool (two-click workflow).
#[derive(Clone, Debug)]
pub struct WallToolState {
    /// Whether the wall tool is currently active.
    pub active: bool,
    /// Starting corner position (set on first right-click).
    pub start_position: Option<Vector3<i32>>,
    /// Wall thickness in blocks (1-5).
    pub thickness: i32,
    /// Whether to use manual height mode.
    pub use_manual_height: bool,
    /// Manual height value for UI slider.
    pub height_value: i32,
    /// Cached preview positions for GPU upload.
    pub preview_positions: Vec<Vector3<i32>>,
    /// Current end position for preview (crosshair target).
    pub preview_end: Option<Vector3<i32>>,
    /// Total block count for the wall.
    pub total_blocks: usize,
    /// Whether the preview was truncated due to exceeding buffer limit.
    pub preview_truncated: bool,
    /// Cached thickness for detecting changes.
    cached_thickness: i32,
    /// Cached manual height for detecting changes.
    cached_manual_height: Option<i32>,
}

impl Default for WallToolState {
    fn default() -> Self {
        Self {
            active: false,
            start_position: None,
            thickness: 1,
            use_manual_height: false,
            height_value: 5,
            preview_positions: Vec::new(),
            preview_end: None,
            total_blocks: 0,
            preview_truncated: false,
            cached_thickness: 1,
            cached_manual_height: None,
        }
    }
}

impl WallToolState {
    /// Get the effective manual height based on UI toggle.
    pub fn effective_manual_height(&self) -> Option<i32> {
        if self.use_manual_height {
            Some(self.height_value)
        } else {
            None
        }
    }

    /// Clear the preview state.
    pub fn clear_preview(&mut self) {
        self.preview_positions.clear();
        self.preview_end = None;
        self.total_blocks = 0;
        self.preview_truncated = false;
    }

    /// Cancel the wall (clear start position and preview).
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

    /// Check if settings have changed since last preview generation.
    fn settings_changed(&self) -> bool {
        self.thickness != self.cached_thickness
            || self.effective_manual_height() != self.cached_manual_height
    }

    /// Update cached settings after regenerating preview.
    fn update_cache(&mut self) {
        self.cached_thickness = self.thickness;
        self.cached_manual_height = self.effective_manual_height();
    }

    /// Update the wall preview from start to the given target position.
    ///
    /// Only generates preview if start_position is set.
    pub fn update_preview(&mut self, target: Vector3<i32>) {
        use crate::gpu_resources::MAX_STENCIL_BLOCKS;

        if let Some(start) = self.start_position {
            // Check if regeneration needed
            let needs_regen = self.preview_end != Some(target) || self.settings_changed();

            if needs_regen {
                self.preview_end = Some(target);
                self.update_cache();

                let all_positions = wall::generate_wall_positions(
                    start,
                    target,
                    self.thickness,
                    self.effective_manual_height(),
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
        } else {
            self.clear_preview();
        }
    }
}

/// State for the floor/platform placement tool (two-click workflow).
#[derive(Clone, Debug)]
pub struct FloorToolState {
    /// Whether the floor tool is currently active.
    pub active: bool,
    /// Starting corner position (set on first right-click).
    pub start_position: Option<Vector3<i32>>,
    /// Floor thickness in blocks (1-5).
    pub thickness: i32,
    /// Build direction (Floor=down, Ceiling=up).
    pub direction: floor::FloorDirection,
    /// Cached preview positions for GPU upload.
    pub preview_positions: Vec<Vector3<i32>>,
    /// Current end position for preview (crosshair target).
    pub preview_end: Option<Vector3<i32>>,
    /// Total block count for the floor.
    pub total_blocks: usize,
    /// Whether the preview was truncated due to exceeding buffer limit.
    pub preview_truncated: bool,
    /// Cached thickness for detecting changes.
    cached_thickness: i32,
    /// Cached direction for detecting changes.
    cached_direction: floor::FloorDirection,
}

impl Default for FloorToolState {
    fn default() -> Self {
        Self {
            active: false,
            start_position: None,
            thickness: 1,
            direction: floor::FloorDirection::Floor,
            preview_positions: Vec::new(),
            preview_end: None,
            total_blocks: 0,
            preview_truncated: false,
            cached_thickness: 1,
            cached_direction: floor::FloorDirection::Floor,
        }
    }
}

impl FloorToolState {
    /// Clear the preview state.
    pub fn clear_preview(&mut self) {
        self.preview_positions.clear();
        self.preview_end = None;
        self.total_blocks = 0;
        self.preview_truncated = false;
    }

    /// Cancel the floor (clear start position and preview).
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

    /// Check if settings have changed since last preview generation.
    fn settings_changed(&self) -> bool {
        self.thickness != self.cached_thickness || self.direction != self.cached_direction
    }

    /// Update cached settings after regenerating preview.
    fn update_cache(&mut self) {
        self.cached_thickness = self.thickness;
        self.cached_direction = self.direction;
    }

    /// Update the floor preview from start to the given target position.
    ///
    /// Only generates preview if start_position is set.
    pub fn update_preview(&mut self, target: Vector3<i32>) {
        use crate::gpu_resources::MAX_STENCIL_BLOCKS;

        if let Some(start) = self.start_position {
            // Check if regeneration needed
            let needs_regen = self.preview_end != Some(target) || self.settings_changed();

            if needs_regen {
                self.preview_end = Some(target);
                self.update_cache();

                let all_positions =
                    floor::generate_floor_positions(start, target, self.thickness, self.direction);

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
        } else {
            self.clear_preview();
        }
    }
}

/// State for the replace tool (uses selection system).
#[derive(Clone, Debug)]
pub struct ReplaceToolState {
    /// Whether the replace tool is currently active.
    pub active: bool,
    /// Source block type to find.
    pub source_block: crate::chunk::BlockType,
    /// Target block type to replace with.
    pub target_block: crate::chunk::BlockType,
    /// Source tint index (for TintedGlass, Crystal, Painted).
    pub source_tint: u8,
    /// Target tint index (for TintedGlass, Crystal, Painted).
    pub target_tint: u8,
    /// Source paint texture (for Painted blocks).
    pub source_texture: u8,
    /// Target paint texture (for Painted blocks).
    pub target_texture: u8,
    /// Number of matching blocks found (for preview).
    pub match_count: usize,
    /// Cached preview positions for GPU upload (matching blocks).
    pub preview_positions: Vec<Vector3<i32>>,
    /// Whether the preview was truncated due to exceeding buffer limit.
    pub preview_truncated: bool,
    /// Flag: user requested to scan/preview matching blocks.
    pub preview_requested: bool,
    /// Flag: user requested to execute the replacement.
    pub execute_requested: bool,
}

impl Default for ReplaceToolState {
    fn default() -> Self {
        Self {
            active: false,
            source_block: crate::chunk::BlockType::Stone,
            target_block: crate::chunk::BlockType::Cobblestone,
            source_tint: 0,
            target_tint: 0,
            source_texture: 0,
            target_texture: 0,
            match_count: 0,
            preview_positions: Vec::new(),
            preview_truncated: false,
            preview_requested: false,
            execute_requested: false,
        }
    }
}

impl ReplaceToolState {
    /// Clear the preview state.
    pub fn clear_preview(&mut self) {
        self.preview_positions.clear();
        self.match_count = 0;
        self.preview_truncated = false;
    }

    /// Deactivate the tool and clear all state.
    pub fn deactivate(&mut self) {
        self.active = false;
        self.clear_preview();
    }

    /// Get the source block identity for matching.
    pub fn source_identity(&self) -> replace::BlockIdentity {
        match self.source_block {
            crate::chunk::BlockType::Painted => replace::BlockIdentity::Painted {
                texture: self.source_texture,
                tint: self.source_tint,
            },
            crate::chunk::BlockType::TintedGlass => replace::BlockIdentity::TintedGlass {
                tint: self.source_tint,
            },
            crate::chunk::BlockType::Crystal => replace::BlockIdentity::Crystal {
                tint: self.source_tint,
            },
            _ => replace::BlockIdentity::Type(self.source_block),
        }
    }

    /// Update the preview by finding matching blocks in the selection.
    ///
    /// # Arguments
    /// * `world` - World to search in
    /// * `selection` - Current template selection
    pub fn update_preview(
        &mut self,
        world: &crate::world::World,
        selection: &crate::templates::TemplateSelection,
    ) {
        use crate::gpu_resources::MAX_STENCIL_BLOCKS;

        if selection.pos1.is_none() || selection.pos2.is_none() {
            self.clear_preview();
            return;
        }

        let source_id = self.source_identity();
        let all_positions =
            replace::find_matching_blocks(world, selection, &source_id, MAX_STENCIL_BLOCKS + 1);

        self.match_count = if all_positions.len() > MAX_STENCIL_BLOCKS {
            // Count all matches (expensive but accurate)
            replace::count_matching_blocks(world, selection, &source_id)
        } else {
            all_positions.len()
        };

        self.preview_truncated = all_positions.len() > MAX_STENCIL_BLOCKS;

        if all_positions.len() > MAX_STENCIL_BLOCKS {
            self.preview_positions = all_positions[..MAX_STENCIL_BLOCKS].to_vec();
        } else {
            self.preview_positions = all_positions;
        }
    }
}

/// State for the circle/ellipse placement tool.
#[derive(Clone, Debug)]
pub struct CircleToolState {
    /// Whether the circle tool is currently active.
    pub active: bool,
    /// Primary radius (X for XZ/XY, Y for YZ plane).
    pub radius_a: i32,
    /// Secondary radius (Z for XZ, Y for XY, Z for YZ plane).
    pub radius_b: i32,
    /// Whether to use ellipse mode (two independent radii).
    pub ellipse_mode: bool,
    /// Whether to fill the interior (true) or outline only (false).
    pub filled: bool,
    /// Orientation plane for the circle.
    pub plane: circle::CirclePlane,
    /// Placement mode: Center or Base (for wall modes).
    pub placement_mode: PlacementMode,
    /// Cached preview positions for GPU upload.
    pub preview_positions: Vec<Vector3<i32>>,
    /// Current preview center position (if targeting a block).
    pub preview_center: Option<Vector3<i32>>,
    /// Total block count for the shape.
    pub total_blocks: usize,
    /// Whether the preview was truncated due to exceeding buffer limit.
    pub preview_truncated: bool,
    /// Cached radius_a for detecting changes.
    cached_radius_a: i32,
    /// Cached radius_b for detecting changes.
    cached_radius_b: i32,
    /// Cached ellipse mode for detecting changes.
    cached_ellipse_mode: bool,
    /// Cached filled mode for detecting changes.
    cached_filled: bool,
    /// Cached plane for detecting changes.
    cached_plane: circle::CirclePlane,
    /// Cached placement mode for detecting changes.
    cached_placement_mode: PlacementMode,
}

impl Default for CircleToolState {
    fn default() -> Self {
        Self {
            active: false,
            radius_a: 5,
            radius_b: 5,
            ellipse_mode: false,
            filled: true,
            plane: circle::CirclePlane::XZ,
            placement_mode: PlacementMode::Center,
            preview_positions: Vec::new(),
            preview_center: None,
            total_blocks: 0,
            preview_truncated: false,
            cached_radius_a: 5,
            cached_radius_b: 5,
            cached_ellipse_mode: false,
            cached_filled: true,
            cached_plane: circle::CirclePlane::XZ,
            cached_placement_mode: PlacementMode::Center,
        }
    }
}

impl CircleToolState {
    /// Get the effective secondary radius (same as primary if not in ellipse mode).
    pub fn effective_radius_b(&self) -> i32 {
        if self.ellipse_mode {
            self.radius_b
        } else {
            self.radius_a
        }
    }

    /// Clear the preview state.
    pub fn clear_preview(&mut self) {
        self.preview_positions.clear();
        self.preview_center = None;
        self.total_blocks = 0;
        self.preview_truncated = false;
    }

    /// Deactivate the tool and clear all state.
    #[allow(dead_code)]
    pub fn deactivate(&mut self) {
        self.active = false;
        self.clear_preview();
    }

    /// Check if settings have changed since last preview generation.
    fn settings_changed(&self) -> bool {
        self.radius_a != self.cached_radius_a
            || self.effective_radius_b() != self.cached_radius_b
            || self.ellipse_mode != self.cached_ellipse_mode
            || self.filled != self.cached_filled
            || self.plane != self.cached_plane
            || self.placement_mode != self.cached_placement_mode
    }

    /// Update cached settings after regenerating preview.
    fn update_cache(&mut self) {
        self.cached_radius_a = self.radius_a;
        self.cached_radius_b = self.effective_radius_b();
        self.cached_ellipse_mode = self.ellipse_mode;
        self.cached_filled = self.filled;
        self.cached_plane = self.plane;
        self.cached_placement_mode = self.placement_mode;
    }

    /// Check if the current plane is a wall mode (vertical).
    pub fn is_wall_mode(&self) -> bool {
        matches!(
            self.plane,
            circle::CirclePlane::XY | circle::CirclePlane::YZ
        )
    }

    /// Update the circle preview centered on the given target position.
    pub fn update_preview(&mut self, target: Vector3<i32>) {
        use crate::gpu_resources::MAX_STENCIL_BLOCKS;

        // Check if regeneration needed
        let needs_regen = self.preview_center != Some(target) || self.settings_changed();

        if needs_regen {
            self.preview_center = Some(target);
            self.update_cache();

            // Adjust center based on placement mode for wall planes
            let adjusted_center = self.adjust_center_for_placement(target);

            let all_positions = circle::generate_circle_positions(
                adjusted_center,
                self.radius_a,
                self.effective_radius_b(),
                self.plane,
                self.filled,
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

    /// Adjust the center position based on placement mode.
    /// For wall modes with Base placement, offset the center up by the vertical radius.
    pub fn adjust_center_for_placement(&self, target: Vector3<i32>) -> Vector3<i32> {
        if self.placement_mode == PlacementMode::Base && self.is_wall_mode() {
            // For wall modes, the vertical component is radius_b (Y in both XY and YZ planes)
            Vector3::new(target.x, target.y + self.effective_radius_b(), target.z)
        } else {
            target
        }
    }
}

// ============================================================================
// MirrorToolState
// ============================================================================

/// State for the mirror/symmetry tool.
#[derive(Clone, Debug)]
pub struct MirrorToolState {
    /// Whether mirror mode is currently active.
    pub active: bool,
    /// Position that defines the mirror plane.
    pub plane_position: Vector3<i32>,
    /// Which axis/axes to mirror across.
    pub axis: mirror::MirrorAxis,
    /// Whether to show the mirror plane visual indicator.
    pub show_plane: bool,
    /// Whether a plane position has been set.
    pub plane_set: bool,
}

impl Default for MirrorToolState {
    fn default() -> Self {
        Self {
            active: false,
            plane_position: Vector3::new(0, 0, 0),
            axis: mirror::MirrorAxis::X,
            show_plane: true,
            plane_set: false,
        }
    }
}

impl MirrorToolState {
    /// Set the mirror plane position.
    pub fn set_plane(&mut self, position: Vector3<i32>) {
        self.plane_position = position;
        self.plane_set = true;
    }

    /// Clear the mirror plane.
    pub fn clear_plane(&mut self) {
        self.plane_set = false;
    }

    /// Deactivate the tool and reset state.
    #[allow(dead_code)]
    pub fn deactivate(&mut self) {
        self.active = false;
        self.plane_set = false;
    }

    /// Cycle to the next axis.
    pub fn cycle_axis(&mut self) {
        self.axis = match self.axis {
            mirror::MirrorAxis::X => mirror::MirrorAxis::Z,
            mirror::MirrorAxis::Z => mirror::MirrorAxis::Both,
            mirror::MirrorAxis::Both => mirror::MirrorAxis::X,
        };
    }

    /// Get mirrored positions for a single position.
    pub fn mirror_position(&self, pos: Vector3<i32>) -> Vec<Vector3<i32>> {
        if !self.active || !self.plane_set {
            return vec![pos];
        }
        mirror::get_mirrored_positions(pos, self.plane_position, self.axis)
    }

    /// Get mirrored positions for multiple positions.
    #[allow(dead_code)]
    pub fn mirror_positions(&self, positions: &[Vector3<i32>]) -> Vec<Vector3<i32>> {
        if !self.active || !self.plane_set {
            return positions.to_vec();
        }
        mirror::get_all_mirrored_positions(positions, self.plane_position, self.axis)
    }
}

// ============================================================================
// StairsToolState
// ============================================================================

/// State for the stairs/staircase generator tool.
#[derive(Clone, Debug)]
pub struct StairsToolState {
    /// Whether the stairs tool is currently active.
    pub active: bool,
    /// Starting position (first click).
    pub start_pos: Option<Vector3<i32>>,
    /// Staircase width in blocks (1-5).
    pub width: i32,
    /// Cached preview positions for GPU upload.
    pub preview_positions: Vec<Vector3<i32>>,
    /// Total block count for the full staircase.
    pub total_blocks: usize,
    /// Whether the preview was truncated due to exceeding buffer limit.
    pub preview_truncated: bool,
    /// Height difference (steps).
    pub step_count: i32,
    /// Horizontal distance.
    pub horizontal_dist: i32,
}

impl Default for StairsToolState {
    fn default() -> Self {
        Self {
            active: false,
            start_pos: None,
            width: 1,
            preview_positions: Vec::new(),
            total_blocks: 0,
            preview_truncated: false,
            step_count: 0,
            horizontal_dist: 0,
        }
    }
}

impl StairsToolState {
    /// Clear the preview and start position.
    pub fn clear_preview(&mut self) {
        self.preview_positions.clear();
        self.total_blocks = 0;
        self.preview_truncated = false;
        self.step_count = 0;
        self.horizontal_dist = 0;
    }

    /// Reset to initial state (clear start position and preview).
    pub fn reset(&mut self) {
        self.start_pos = None;
        self.clear_preview();
    }

    /// Deactivate the tool and reset state.
    #[allow(dead_code)]
    pub fn deactivate(&mut self) {
        self.active = false;
        self.reset();
    }

    /// Update the stairs preview given the current target position.
    pub fn update_preview(&mut self, target: Vector3<i32>) {
        use crate::gpu_resources::MAX_STENCIL_BLOCKS;

        if let Some(start) = self.start_pos {
            let (height, horizontal, _steps) = stairs::calculate_dimensions(start, target);
            self.step_count = height;
            self.horizontal_dist = horizontal;

            let all_positions = stairs::generate_stair_positions(start, target, self.width);

            // Track total count and truncation status
            self.total_blocks = all_positions.len();
            self.preview_truncated = all_positions.len() > MAX_STENCIL_BLOCKS;

            // Truncate for preview
            if all_positions.len() > MAX_STENCIL_BLOCKS {
                self.preview_positions = all_positions[..MAX_STENCIL_BLOCKS].to_vec();
            } else {
                self.preview_positions = all_positions;
            }
        } else {
            self.clear_preview();
        }
    }
}

// ============================================================================
// ArchToolState
// ============================================================================

/// State for the arch placement tool.
#[derive(Clone, Debug)]
pub struct ArchToolState {
    /// Whether the arch tool is currently active.
    pub active: bool,
    /// Width of the arch opening (2-50 blocks).
    pub width: i32,
    /// Height of the arch from base to apex (1-50 blocks).
    pub height: i32,
    /// Thickness/depth of the arch (1-10 blocks).
    pub thickness: i32,
    /// Arch curve style.
    pub style: arch::ArchStyle,
    /// Arch orientation (which direction it faces).
    pub orientation: arch::ArchOrientation,
    /// Whether to create a hollow arch (passageway).
    pub hollow: bool,
    /// Whether to use two-click mode (like bridge tool).
    pub two_click_mode: bool,
    /// Starting position for two-click mode (first jamb base).
    pub start_position: Option<Vector3<i32>>,
    /// Cached preview positions for GPU upload.
    pub preview_positions: Vec<Vector3<i32>>,
    /// Current preview base position (if targeting a block).
    pub preview_center: Option<Vector3<i32>>,
    /// Preview end position for two-click mode.
    pub preview_end: Option<Vector3<i32>>,
    /// Total block count for the full arch.
    pub total_blocks: usize,
    /// Whether the preview was truncated due to exceeding buffer limit.
    pub preview_truncated: bool,
}

impl Default for ArchToolState {
    fn default() -> Self {
        Self {
            active: false,
            width: 4,
            height: 3,
            thickness: 1,
            style: arch::ArchStyle::Semicircle,
            orientation: arch::ArchOrientation::FacingZ,
            hollow: false,
            two_click_mode: false,
            start_position: None,
            preview_positions: Vec::new(),
            preview_center: None,
            preview_end: None,
            total_blocks: 0,
            preview_truncated: false,
        }
    }
}

impl ArchToolState {
    /// Clear the preview.
    pub fn clear_preview(&mut self) {
        self.preview_positions.clear();
        self.preview_center = None;
        self.preview_end = None;
        self.total_blocks = 0;
        self.preview_truncated = false;
    }

    /// Cancel two-click mode (clear start position and preview).
    pub fn cancel(&mut self) {
        self.start_position = None;
        self.clear_preview();
    }

    /// Deactivate the tool and reset state.
    #[allow(dead_code)]
    pub fn deactivate(&mut self) {
        self.active = false;
        self.start_position = None;
        self.clear_preview();
    }

    /// Update the arch preview at the given base center position.
    /// In single-click mode, places arch centered at base_center.
    /// In two-click mode, if start is set, calculates arch between start and target.
    pub fn update_preview(&mut self, target: Vector3<i32>) {
        use crate::gpu_resources::MAX_STENCIL_BLOCKS;

        if self.two_click_mode {
            if let Some(start) = self.start_position {
                // Two-click mode: calculate arch between start and target
                self.preview_end = Some(target);

                // Calculate width from distance between points
                let dx = (target.x - start.x).abs();
                let dz = (target.z - start.z).abs();

                // Determine orientation and width based on the axis with greater extent
                let (calculated_width, orientation) = if dx >= dz {
                    (dx.max(2), arch::ArchOrientation::FacingZ)
                } else {
                    (dz.max(2), arch::ArchOrientation::FacingX)
                };

                // Calculate center between start and target
                let center = Vector3::new(
                    (start.x + target.x) / 2,
                    start.y.min(target.y), // Use lower Y as base
                    (start.z + target.z) / 2,
                );

                let all_positions = arch::generate_arch_positions(
                    center,
                    calculated_width,
                    self.height,
                    self.thickness,
                    self.style,
                    orientation,
                    self.hollow,
                );

                // Track total count and truncation status
                self.total_blocks = all_positions.len();
                self.preview_truncated = all_positions.len() > MAX_STENCIL_BLOCKS;

                // Truncate for preview
                if all_positions.len() > MAX_STENCIL_BLOCKS {
                    self.preview_positions = all_positions[..MAX_STENCIL_BLOCKS].to_vec();
                } else {
                    self.preview_positions = all_positions;
                }
            } else {
                // No start set yet, just track target
                self.preview_center = Some(target);
                self.preview_positions.clear();
                self.total_blocks = 0;
            }
        } else {
            // Single-click mode: place arch centered at target
            self.preview_center = Some(target);

            let all_positions = arch::generate_arch_positions(
                target,
                self.width,
                self.height,
                self.thickness,
                self.style,
                self.orientation,
                self.hollow,
            );

            // Track total count and truncation status
            self.total_blocks = all_positions.len();
            self.preview_truncated = all_positions.len() > MAX_STENCIL_BLOCKS;

            // Truncate for preview
            if all_positions.len() > MAX_STENCIL_BLOCKS {
                self.preview_positions = all_positions[..MAX_STENCIL_BLOCKS].to_vec();
            } else {
                self.preview_positions = all_positions;
            }
        }
    }

    /// Get the calculated width for two-click mode.
    pub fn calculated_width(&self) -> Option<i32> {
        if self.two_click_mode {
            if let (Some(start), Some(end)) = (self.start_position, self.preview_end) {
                let dx = (end.x - start.x).abs();
                let dz = (end.z - start.z).abs();
                return Some(dx.max(dz).max(2));
            }
        }
        None
    }

    /// Get the calculated orientation for two-click mode.
    pub fn calculated_orientation(&self) -> Option<arch::ArchOrientation> {
        if self.two_click_mode {
            if let (Some(start), Some(end)) = (self.start_position, self.preview_end) {
                let dx = (end.x - start.x).abs();
                let dz = (end.z - start.z).abs();
                return Some(if dx >= dz {
                    arch::ArchOrientation::FacingZ
                } else {
                    arch::ArchOrientation::FacingX
                });
            }
        }
        None
    }
}

// ============================================================================
// ConeToolState
// ============================================================================

/// State for the cone/pyramid placement tool.
#[derive(Clone, Debug)]
pub struct ConeToolState {
    /// Whether the cone tool is currently active.
    pub active: bool,
    /// Base size (radius for cone, half-side for pyramid).
    pub base_size: i32,
    /// Height from base to apex.
    pub height: i32,
    /// Shape type (Cone or Pyramid).
    pub shape: cone::ConeShape,
    /// Whether to create a hollow shell.
    pub hollow: bool,
    /// Whether to invert (point down instead of up).
    pub inverted: bool,
    /// Cached preview positions for GPU upload.
    pub preview_positions: Vec<Vector3<i32>>,
    /// Current preview base center position.
    pub preview_center: Option<Vector3<i32>>,
    /// Total block count for the full shape.
    pub total_blocks: usize,
    /// Whether the preview was truncated due to exceeding buffer limit.
    pub preview_truncated: bool,
    /// Cached base_size for detecting changes.
    cached_base_size: i32,
    /// Cached height for detecting changes.
    cached_height: i32,
    /// Cached shape for detecting changes.
    cached_shape: cone::ConeShape,
    /// Cached hollow for detecting changes.
    cached_hollow: bool,
    /// Cached inverted for detecting changes.
    cached_inverted: bool,
}

impl Default for ConeToolState {
    fn default() -> Self {
        Self {
            active: false,
            base_size: 5,
            height: 8,
            shape: cone::ConeShape::Cone,
            hollow: false,
            inverted: false,
            preview_positions: Vec::new(),
            preview_center: None,
            total_blocks: 0,
            preview_truncated: false,
            cached_base_size: 5,
            cached_height: 8,
            cached_shape: cone::ConeShape::Cone,
            cached_hollow: false,
            cached_inverted: false,
        }
    }
}

impl ConeToolState {
    /// Clear the preview.
    pub fn clear_preview(&mut self) {
        self.preview_positions.clear();
        self.preview_center = None;
        self.total_blocks = 0;
        self.preview_truncated = false;
    }

    /// Deactivate the tool and reset state.
    #[allow(dead_code)]
    pub fn deactivate(&mut self) {
        self.active = false;
        self.clear_preview();
    }

    /// Check if settings have changed since last preview generation.
    fn settings_changed(&self) -> bool {
        self.base_size != self.cached_base_size
            || self.height != self.cached_height
            || self.shape != self.cached_shape
            || self.hollow != self.cached_hollow
            || self.inverted != self.cached_inverted
    }

    /// Update cached settings after regenerating preview.
    fn update_cache(&mut self) {
        self.cached_base_size = self.base_size;
        self.cached_height = self.height;
        self.cached_shape = self.shape;
        self.cached_hollow = self.hollow;
        self.cached_inverted = self.inverted;
    }

    /// Update the cone preview at the given base center position.
    pub fn update_preview(&mut self, base_center: Vector3<i32>) {
        use crate::gpu_resources::MAX_STENCIL_BLOCKS;

        // Check if regeneration needed
        let needs_regen = self.preview_center != Some(base_center) || self.settings_changed();

        if needs_regen {
            self.preview_center = Some(base_center);
            self.update_cache();

            let all_positions = cone::generate_cone_positions(
                base_center,
                self.base_size,
                self.height,
                self.shape,
                self.hollow,
                self.inverted,
            );

            // Track total count and truncation status
            self.total_blocks = all_positions.len();
            self.preview_truncated = all_positions.len() > MAX_STENCIL_BLOCKS;

            // Truncate for preview
            if all_positions.len() > MAX_STENCIL_BLOCKS {
                self.preview_positions = all_positions[..MAX_STENCIL_BLOCKS].to_vec();
            } else {
                self.preview_positions = all_positions;
            }
        }
    }
}

// ============================================================================
// CloneToolState
// ============================================================================

/// State for the clone/array placement tool.
#[derive(Clone, Debug)]
pub struct CloneToolState {
    /// Whether the clone tool is currently active.
    pub active: bool,
    /// Clone mode (Linear or Grid).
    pub mode: clone::CloneMode,
    /// Primary axis for linear mode.
    pub axis: clone::CloneAxis,
    /// Number of copies for linear mode (includes original).
    pub count: i32,
    /// Spacing between copies for linear mode.
    pub spacing: i32,
    /// Number of copies along X for grid mode.
    pub grid_count_x: i32,
    /// Number of copies along Z for grid mode.
    pub grid_count_z: i32,
    /// Spacing along X for grid mode.
    pub grid_spacing_x: i32,
    /// Spacing along Z for grid mode.
    pub grid_spacing_z: i32,
    /// Cached preview positions for GPU upload.
    pub preview_positions: Vec<Vector3<i32>>,
    /// Total block count for the clone operation.
    pub total_blocks: usize,
    /// Number of copies (origins) in the preview.
    pub copy_count: usize,
    /// Whether the preview was truncated due to exceeding buffer limit.
    pub preview_truncated: bool,
    /// Flag: user requested to execute the clone.
    pub execute_requested: bool,
}

impl Default for CloneToolState {
    fn default() -> Self {
        Self {
            active: false,
            mode: clone::CloneMode::Linear,
            axis: clone::CloneAxis::X,
            count: 3,
            spacing: 1,
            grid_count_x: 3,
            grid_count_z: 3,
            grid_spacing_x: 1,
            grid_spacing_z: 1,
            preview_positions: Vec::new(),
            total_blocks: 0,
            copy_count: 0,
            preview_truncated: false,
            execute_requested: false,
        }
    }
}

impl CloneToolState {
    /// Clear the preview.
    pub fn clear_preview(&mut self) {
        self.preview_positions.clear();
        self.total_blocks = 0;
        self.copy_count = 0;
        self.preview_truncated = false;
    }

    /// Deactivate the tool and reset state.
    #[allow(dead_code)]
    pub fn deactivate(&mut self) {
        self.active = false;
        self.clear_preview();
    }

    /// Update the clone preview based on the current selection.
    ///
    /// # Arguments
    /// * `selection` - Current template selection defining source region
    pub fn update_preview(
        &mut self,
        selection: &crate::templates::TemplateSelection,
        world: &crate::world::World,
    ) {
        use crate::gpu_resources::MAX_STENCIL_BLOCKS;

        if selection.pos1.is_none() || selection.pos2.is_none() {
            self.clear_preview();
            return;
        }

        let (min, max) = selection.bounds().unwrap();
        let selection_size = Vector3::new(
            (max.x - min.x + 1).abs(),
            (max.y - min.y + 1).abs(),
            (max.z - min.z + 1).abs(),
        );

        // Calculate clone origins
        let origins = clone::calculate_clone_origins(
            selection_size,
            self.mode,
            self.axis,
            self.count,
            self.spacing,
            self.grid_count_x,
            self.grid_count_z,
            self.grid_spacing_x,
            self.grid_spacing_z,
        );

        self.copy_count = origins.len();

        // Collect source positions from selection
        let source_positions: Vec<Vector3<i32>> = match selection.iter_positions() {
            Some(iter) => iter
                .filter(|pos| {
                    let block = world.get_block(*pos);
                    block != Some(crate::chunk::BlockType::Air)
                })
                .collect(),
            None => {
                self.clear_preview();
                return;
            }
        };

        if source_positions.is_empty() {
            self.clear_preview();
            return;
        }

        // Generate all cloned positions
        let all_positions = clone::generate_cloned_positions(&source_positions, &origins);

        // Track total count and truncation status
        self.total_blocks = all_positions.len();
        self.preview_truncated = all_positions.len() > MAX_STENCIL_BLOCKS;

        // Truncate for preview
        if all_positions.len() > MAX_STENCIL_BLOCKS {
            self.preview_positions = all_positions[..MAX_STENCIL_BLOCKS].to_vec();
        } else {
            self.preview_positions = all_positions;
        }
    }
}

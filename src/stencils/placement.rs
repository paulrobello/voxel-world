/// Placed stencil management with rotation support.
///
/// Stencils are holographic guides that persist in the world,
/// unlike templates which are placed as actual blocks.
use super::format::StencilFile;
use nalgebra::Vector3;
use serde::{Deserialize, Serialize};

/// Default stencil color (cyan).
#[allow(dead_code)]
pub const DEFAULT_STENCIL_COLOR: [f32; 3] = [0.0, 1.0, 1.0];

/// Default stencil opacity.
#[allow(dead_code)]
pub const DEFAULT_STENCIL_OPACITY: f32 = 0.5;

/// Serializable origin coordinates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedOrigin {
    /// X coordinate.
    pub x: i32,
    /// Y coordinate.
    pub y: i32,
    /// Z coordinate.
    pub z: i32,
}

impl From<Vector3<i32>> for SerializedOrigin {
    fn from(v: Vector3<i32>) -> Self {
        Self {
            x: v.x,
            y: v.y,
            z: v.z,
        }
    }
}

impl From<SerializedOrigin> for Vector3<i32> {
    fn from(s: SerializedOrigin) -> Self {
        Vector3::new(s.x, s.y, s.z)
    }
}

/// A stencil that has been placed in the world as a holographic guide.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct PlacedStencil {
    /// Unique identifier for this placed stencil.
    pub id: u64,
    /// The stencil data.
    pub stencil: StencilFile,
    /// World position (anchor point) - serialized as separate coords.
    #[serde(flatten)]
    origin_serialized: SerializedOrigin,
    /// Rotation (0-3 for 0°/90°/180°/270° around Y-axis).
    pub rotation: u8,
    /// RGB color for rendering.
    pub color: [f32; 3],
    /// Opacity (0.3-0.8).
    pub opacity: f32,
}

#[allow(dead_code)]
impl PlacedStencil {
    /// Gets the origin as a Vector3.
    pub fn origin(&self) -> Vector3<i32> {
        Vector3::new(
            self.origin_serialized.x,
            self.origin_serialized.y,
            self.origin_serialized.z,
        )
    }

    /// Sets the origin from a Vector3.
    pub fn set_origin(&mut self, origin: Vector3<i32>) {
        self.origin_serialized = SerializedOrigin::from(origin);
    }
}

#[allow(dead_code)]
impl PlacedStencil {
    /// Creates a new placed stencil at the specified position.
    pub fn new(id: u64, stencil: StencilFile, origin: Vector3<i32>) -> Self {
        Self {
            id,
            stencil,
            origin_serialized: SerializedOrigin::from(origin),
            rotation: 0,
            color: DEFAULT_STENCIL_COLOR,
            opacity: DEFAULT_STENCIL_OPACITY,
        }
    }

    /// Creates a placed stencil with custom color and opacity.
    pub fn with_color_and_opacity(
        id: u64,
        stencil: StencilFile,
        origin: Vector3<i32>,
        color: [f32; 3],
        opacity: f32,
    ) -> Self {
        Self {
            id,
            stencil,
            origin_serialized: SerializedOrigin::from(origin),
            rotation: 0,
            color,
            opacity: opacity.clamp(0.3, 0.8),
        }
    }

    /// Rotates the stencil 90° clockwise around Y-axis.
    pub fn rotate_90(&mut self) {
        self.rotation = (self.rotation + 1) % 4;
    }

    /// Rotates the stencil 90° counter-clockwise around Y-axis.
    pub fn rotate_90_ccw(&mut self) {
        self.rotation = if self.rotation == 0 {
            3
        } else {
            self.rotation - 1
        };
    }

    /// Sets a specific rotation (0-3).
    pub fn set_rotation(&mut self, rotation: u8) {
        self.rotation = rotation % 4;
    }

    /// Sets the stencil opacity (clamped to 0.3-0.8).
    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity.clamp(0.3, 0.8);
    }

    /// Gets the stencil dimensions after rotation (width, height, depth).
    pub fn get_rotated_dimensions(&self) -> (i32, i32, i32) {
        let w = self.stencil.width as i32;
        let h = self.stencil.height as i32;
        let d = self.stencil.depth as i32;

        match self.rotation {
            0 | 2 => (w, h, d), // 0° and 180°: dimensions unchanged
            1 | 3 => (d, h, w), // 90° and 270°: width and depth swapped
            _ => unreachable!(),
        }
    }

    /// Gets the bounding box in world coordinates (min, max).
    pub fn get_bounding_box(&self) -> (Vector3<i32>, Vector3<i32>) {
        let (width, height, depth) = self.get_rotated_dimensions();
        let origin = self.origin();
        let min = origin;
        let max = Vector3::new(
            origin.x + width - 1,
            origin.y + height - 1,
            origin.z + depth - 1,
        );
        (min, max)
    }

    /// Applies rotation transformation to stencil-relative coordinates.
    /// Returns world-relative offset from anchor position.
    fn apply_rotation(&self, x: u8, y: u8, z: u8) -> Vector3<i32> {
        let w = self.stencil.width as i32;
        let d = self.stencil.depth as i32;

        // Center of rotation (stencil center)
        let cx = w / 2;
        let cz = d / 2;

        // Position relative to center
        let rx = x as i32 - cx;
        let rz = z as i32 - cz;

        // Apply Y-axis rotation (clockwise when viewed from above)
        let (tx, tz) = match self.rotation {
            0 => (rx, rz),   // 0°
            1 => (rz, -rx),  // 90° clockwise
            2 => (-rx, -rz), // 180°
            3 => (-rz, rx),  // 270° clockwise
            _ => (rx, rz),   // Invalid, default to 0°
        };

        // Convert back to coordinates
        Vector3::new(tx + cx, y as i32, tz + cz)
    }

    /// Gets the world position for a stencil block after rotation.
    pub fn get_world_position(&self, x: u8, y: u8, z: u8) -> Vector3<i32> {
        let offset = self.apply_rotation(x, y, z);
        self.origin() + offset
    }

    /// Returns all world positions for this stencil (rotated and translated).
    pub fn get_world_positions(&self) -> Vec<Vector3<i32>> {
        self.stencil
            .positions
            .iter()
            .map(|p| self.get_world_position(p.x, p.y, p.z))
            .collect()
    }

    /// Gets positions up to a maximum count (for GPU upload).
    /// For large stencils, only returns surface positions.
    pub fn get_preview_positions(&self, max_positions: usize) -> Vec<Vector3<i32>> {
        let mut positions = Vec::new();

        // For large stencils, only show surface positions
        let is_large = self.stencil.position_count() > max_positions;

        if is_large {
            // Build a set of all positions for quick lookup
            use std::collections::HashSet;
            let mut pos_set: HashSet<(u8, u8, u8)> = HashSet::new();
            for pos in &self.stencil.positions {
                pos_set.insert((pos.x, pos.y, pos.z));
            }

            // Only include positions that have at least one air neighbor
            for pos in &self.stencil.positions {
                let x = pos.x as i32;
                let y = pos.y as i32;
                let z = pos.z as i32;

                // Check 6 neighbors
                let neighbors = [
                    (x + 1, y, z),
                    (x - 1, y, z),
                    (x, y + 1, z),
                    (x, y - 1, z),
                    (x, y, z + 1),
                    (x, y, z - 1),
                ];

                let has_air_neighbor = neighbors.iter().any(|(nx, ny, nz)| {
                    *nx < 0
                        || *ny < 0
                        || *nz < 0
                        || *nx >= self.stencil.width as i32
                        || *ny >= self.stencil.height as i32
                        || *nz >= self.stencil.depth as i32
                        || !pos_set.contains(&(*nx as u8, *ny as u8, *nz as u8))
                });

                if has_air_neighbor {
                    let world_pos = self.get_world_position(pos.x, pos.y, pos.z);
                    positions.push(world_pos);

                    if positions.len() >= max_positions {
                        break;
                    }
                }
            }
        } else {
            // Show all positions for small stencils
            for pos in &self.stencil.positions {
                let world_pos = self.get_world_position(pos.x, pos.y, pos.z);
                positions.push(world_pos);

                if positions.len() >= max_positions {
                    break;
                }
            }
        }

        positions
    }

    /// Returns an iterator over all world positions.
    pub fn iter_positions(&self) -> impl Iterator<Item = Vector3<i32>> + '_ {
        self.stencil
            .positions
            .iter()
            .map(move |p| self.get_world_position(p.x, p.y, p.z))
    }

    /// Checks if a world position is within this stencil.
    pub fn contains(&self, world_pos: Vector3<i32>) -> bool {
        // Check bounding box first
        let (min, max) = self.get_bounding_box();
        if world_pos.x < min.x
            || world_pos.x > max.x
            || world_pos.y < min.y
            || world_pos.y > max.y
            || world_pos.z < min.z
            || world_pos.z > max.z
        {
            return false;
        }

        // Check if the position is in the stencil
        self.iter_positions().any(|p| p == world_pos)
    }

    /// Updates position from a raycast hit.
    /// Centers the stencil horizontally on the hit position.
    pub fn update_position_from_raycast(&mut self, hit_pos: Vector3<i32>) {
        let (width, _, depth) = self.get_rotated_dimensions();
        self.origin_serialized.x = hit_pos.x - (width / 2);
        self.origin_serialized.y = hit_pos.y;
        self.origin_serialized.z = hit_pos.z - (depth / 2);
    }

    /// Moves the stencil by an offset.
    pub fn translate(&mut self, offset: Vector3<i32>) {
        self.origin_serialized.x += offset.x;
        self.origin_serialized.y += offset.y;
        self.origin_serialized.z += offset.z;
    }
}

/// Manages stencil placement mode before a stencil is committed to the world.
#[allow(dead_code)]
pub struct StencilPlacementMode {
    /// The stencil being placed.
    pub stencil: StencilFile,
    /// Current anchor position.
    pub position: Vector3<i32>,
    /// Current rotation (0-3).
    pub rotation: u8,
}

#[allow(dead_code)]
impl StencilPlacementMode {
    /// Creates a new placement mode for a stencil.
    pub fn new(stencil: StencilFile, position: Vector3<i32>) -> Self {
        Self {
            stencil,
            position,
            rotation: 0,
        }
    }

    /// Rotates 90° clockwise.
    pub fn rotate_90(&mut self) {
        self.rotation = (self.rotation + 1) % 4;
    }

    /// Rotates 90° counter-clockwise.
    pub fn rotate_90_ccw(&mut self) {
        self.rotation = if self.rotation == 0 {
            3
        } else {
            self.rotation - 1
        };
    }

    /// Gets rotated dimensions.
    pub fn get_rotated_dimensions(&self) -> (i32, i32, i32) {
        let w = self.stencil.width as i32;
        let h = self.stencil.height as i32;
        let d = self.stencil.depth as i32;

        match self.rotation {
            0 | 2 => (w, h, d),
            1 | 3 => (d, h, w),
            _ => unreachable!(),
        }
    }

    /// Updates position from raycast hit.
    pub fn update_position_from_raycast(&mut self, hit_pos: Vector3<i32>) {
        let (width, _, depth) = self.get_rotated_dimensions();
        self.position.x = hit_pos.x - (width / 2);
        self.position.y = hit_pos.y;
        self.position.z = hit_pos.z - (depth / 2);
    }

    /// Moves position by offset.
    pub fn translate(&mut self, offset: Vector3<i32>) {
        self.position += offset;
    }

    /// Gets preview positions for rendering (up to max_positions).
    pub fn get_preview_positions(&self, max_positions: usize) -> Vec<Vector3<i32>> {
        // Create a temporary placed stencil to reuse the logic
        let temp = PlacedStencil {
            id: 0,
            stencil: self.stencil.clone(),
            origin_serialized: SerializedOrigin::from(self.position),
            rotation: self.rotation,
            color: DEFAULT_STENCIL_COLOR,
            opacity: DEFAULT_STENCIL_OPACITY,
        };
        temp.get_preview_positions(max_positions)
    }

    /// Commits this placement to a PlacedStencil.
    pub fn commit(self, id: u64, color: [f32; 3], opacity: f32) -> PlacedStencil {
        PlacedStencil {
            id,
            stencil: self.stencil,
            origin_serialized: SerializedOrigin::from(self.position),
            rotation: self.rotation,
            color,
            opacity: opacity.clamp(0.3, 0.8),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stencils::format::StencilPosition;

    fn create_test_stencil() -> StencilFile {
        let mut stencil = StencilFile::new("test".to_string(), "author".to_string(), 3, 3, 3);
        stencil.positions.push(StencilPosition { x: 0, y: 0, z: 0 });
        stencil.positions.push(StencilPosition { x: 2, y: 0, z: 0 });
        stencil.positions.push(StencilPosition { x: 1, y: 1, z: 1 });
        stencil
    }

    #[test]
    fn test_rotation() {
        let stencil = create_test_stencil();
        let mut placed = PlacedStencil::new(1, stencil, Vector3::new(100, 64, 200));

        assert_eq!(placed.rotation, 0);
        placed.rotate_90();
        assert_eq!(placed.rotation, 1);
        placed.rotate_90();
        assert_eq!(placed.rotation, 2);
        placed.rotate_90();
        assert_eq!(placed.rotation, 3);
        placed.rotate_90();
        assert_eq!(placed.rotation, 0);
    }

    #[test]
    fn test_rotation_ccw() {
        let stencil = create_test_stencil();
        let mut placed = PlacedStencil::new(1, stencil, Vector3::new(100, 64, 200));

        placed.rotate_90_ccw();
        assert_eq!(placed.rotation, 3);
        placed.rotate_90_ccw();
        assert_eq!(placed.rotation, 2);
    }

    #[test]
    fn test_world_positions() {
        let stencil = create_test_stencil();
        let placed = PlacedStencil::new(1, stencil, Vector3::new(100, 64, 200));

        let positions = placed.get_world_positions();
        assert_eq!(positions.len(), 3);

        // All positions should be offset by the origin
        for pos in &positions {
            assert!(pos.x >= 100);
            assert!(pos.y >= 64);
            assert!(pos.z >= 200);
        }
    }

    #[test]
    fn test_opacity_clamping() {
        let stencil = create_test_stencil();
        let mut placed = PlacedStencil::new(1, stencil, Vector3::new(0, 0, 0));

        placed.set_opacity(0.1);
        assert_eq!(placed.opacity, 0.3);

        placed.set_opacity(0.5);
        assert_eq!(placed.opacity, 0.5);

        placed.set_opacity(0.9);
        assert_eq!(placed.opacity, 0.8);
    }

    #[test]
    fn test_bounding_box() {
        let stencil = create_test_stencil();
        let placed = PlacedStencil::new(1, stencil, Vector3::new(100, 64, 200));

        let (min, max) = placed.get_bounding_box();
        assert_eq!(min, Vector3::new(100, 64, 200));
        assert_eq!(max, Vector3::new(102, 66, 202)); // 3x3x3 stencil
    }

    #[test]
    fn test_rotated_dimensions() {
        let stencil = StencilFile::new("test".to_string(), "author".to_string(), 4, 2, 6);
        let mut placed = PlacedStencil::new(1, stencil, Vector3::new(0, 0, 0));

        assert_eq!(placed.get_rotated_dimensions(), (4, 2, 6));

        placed.set_rotation(1);
        assert_eq!(placed.get_rotated_dimensions(), (6, 2, 4)); // Width and depth swapped

        placed.set_rotation(2);
        assert_eq!(placed.get_rotated_dimensions(), (4, 2, 6));

        placed.set_rotation(3);
        assert_eq!(placed.get_rotated_dimensions(), (6, 2, 4));
    }

    #[test]
    fn test_translate() {
        let stencil = create_test_stencil();
        let mut placed = PlacedStencil::new(1, stencil, Vector3::new(100, 64, 200));

        placed.translate(Vector3::new(10, 5, -3));
        assert_eq!(placed.origin(), Vector3::new(110, 69, 197));
    }

    #[test]
    fn test_placement_mode() {
        let stencil = create_test_stencil();
        let mut mode = StencilPlacementMode::new(stencil, Vector3::new(0, 0, 0));

        mode.rotate_90();
        assert_eq!(mode.rotation, 1);

        mode.translate(Vector3::new(100, 64, 200));
        assert_eq!(mode.position, Vector3::new(100, 64, 200));

        let placed = mode.commit(42, [1.0, 0.5, 0.0], 0.6);
        assert_eq!(placed.id, 42);
        assert_eq!(placed.rotation, 1);
        assert_eq!(placed.origin(), Vector3::new(100, 64, 200));
        assert_eq!(placed.color, [1.0, 0.5, 0.0]);
        assert_eq!(placed.opacity, 0.6);
    }
}

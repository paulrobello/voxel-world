use super::format::StencilFile;
/// Stencil manager for tracking active stencils in the world.
use super::placement::{
    DEFAULT_STENCIL_COLOR, DEFAULT_STENCIL_OPACITY, PlacedStencil, StencilPlacementMode,
};
use nalgebra::Vector3;
use serde::{Deserialize, Serialize};

/// Render mode for stencils.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[allow(dead_code)]
pub enum StencilRenderMode {
    /// Render as wireframe edges only.
    #[default]
    Wireframe,
    /// Render as semi-transparent solid blocks.
    Solid,
}

#[allow(dead_code)]
impl StencilRenderMode {
    /// Toggles between wireframe and solid mode.
    pub fn toggle(&mut self) {
        *self = match self {
            StencilRenderMode::Wireframe => StencilRenderMode::Solid,
            StencilRenderMode::Solid => StencilRenderMode::Wireframe,
        };
    }

    /// Returns the render mode as an integer for shader use.
    pub fn as_i32(&self) -> i32 {
        match self {
            StencilRenderMode::Wireframe => 0,
            StencilRenderMode::Solid => 1,
        }
    }

    /// Returns a display name for the mode.
    pub fn display_name(&self) -> &'static str {
        match self {
            StencilRenderMode::Wireframe => "Wireframe",
            StencilRenderMode::Solid => "Solid",
        }
    }
}

/// Manages active stencils in the world.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct StencilManager {
    /// Currently active stencils.
    pub active_stencils: Vec<PlacedStencil>,
    /// Next available stencil ID.
    pub next_id: u64,
    /// Global opacity for new stencils.
    pub global_opacity: f32,
    /// Current render mode.
    pub render_mode: StencilRenderMode,
    /// Default color for new stencils.
    #[serde(default = "default_color")]
    pub default_color: [f32; 3],
}

#[allow(dead_code)]
fn default_color() -> [f32; 3] {
    DEFAULT_STENCIL_COLOR
}

impl Default for StencilManager {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl StencilManager {
    /// Creates a new empty stencil manager.
    pub fn new() -> Self {
        Self {
            active_stencils: Vec::new(),
            next_id: 1,
            global_opacity: DEFAULT_STENCIL_OPACITY,
            render_mode: StencilRenderMode::default(),
            default_color: DEFAULT_STENCIL_COLOR,
        }
    }

    /// Adds a stencil from placement mode.
    pub fn add_from_placement(&mut self, placement: StencilPlacementMode) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        let placed = placement.commit(id, self.default_color, self.global_opacity);
        self.active_stencils.push(placed);

        id
    }

    /// Adds a stencil at a specified position.
    pub fn add_stencil(&mut self, stencil: StencilFile, origin: Vector3<i32>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        let mut placed = PlacedStencil::new(id, stencil, origin);
        placed.color = self.default_color;
        placed.opacity = self.global_opacity;

        self.active_stencils.push(placed);
        id
    }

    /// Adds a pre-configured placed stencil (for loading from save).
    pub fn add_placed_stencil(&mut self, mut stencil: PlacedStencil) {
        // Ensure ID is unique
        if self.active_stencils.iter().any(|s| s.id == stencil.id) {
            stencil.id = self.next_id;
            self.next_id += 1;
        } else if stencil.id >= self.next_id {
            self.next_id = stencil.id + 1;
        }
        self.active_stencils.push(stencil);
    }

    /// Removes a stencil by ID.
    pub fn remove_stencil(&mut self, id: u64) -> Option<PlacedStencil> {
        if let Some(index) = self.active_stencils.iter().position(|s| s.id == id) {
            Some(self.active_stencils.remove(index))
        } else {
            None
        }
    }

    /// Gets a stencil by ID.
    pub fn get_stencil(&self, id: u64) -> Option<&PlacedStencil> {
        self.active_stencils.iter().find(|s| s.id == id)
    }

    /// Gets a mutable stencil by ID.
    pub fn get_stencil_mut(&mut self, id: u64) -> Option<&mut PlacedStencil> {
        self.active_stencils.iter_mut().find(|s| s.id == id)
    }

    /// Clears all active stencils.
    pub fn clear(&mut self) {
        self.active_stencils.clear();
    }

    /// Returns the number of active stencils.
    pub fn count(&self) -> usize {
        self.active_stencils.len()
    }

    /// Checks if there are any active stencils.
    pub fn is_empty(&self) -> bool {
        self.active_stencils.is_empty()
    }

    /// Sets opacity for a specific stencil.
    pub fn set_stencil_opacity(&mut self, id: u64, opacity: f32) {
        if let Some(stencil) = self.get_stencil_mut(id) {
            stencil.set_opacity(opacity);
        }
    }

    /// Sets the global opacity (affects new stencils).
    pub fn set_global_opacity(&mut self, opacity: f32) {
        self.global_opacity = opacity.clamp(0.3, 0.8);
    }

    /// Adjusts global opacity by a delta.
    pub fn adjust_global_opacity(&mut self, delta: f32) {
        self.set_global_opacity(self.global_opacity + delta);
    }

    /// Applies global opacity to all active stencils.
    pub fn apply_global_opacity_to_all(&mut self) {
        for stencil in &mut self.active_stencils {
            stencil.opacity = self.global_opacity;
        }
    }

    /// Toggles render mode between wireframe and solid.
    pub fn toggle_render_mode(&mut self) {
        self.render_mode.toggle();
    }

    /// Sets render mode.
    pub fn set_render_mode(&mut self, mode: StencilRenderMode) {
        self.render_mode = mode;
    }

    /// Gets all positions from all active stencils (up to max_positions).
    /// Each position includes the stencil ID for color lookup.
    pub fn get_all_positions(&self, max_positions: usize) -> Vec<(Vector3<i32>, u64)> {
        let mut positions = Vec::new();

        for stencil in &self.active_stencils {
            for pos in stencil.iter_positions() {
                positions.push((pos, stencil.id));

                if positions.len() >= max_positions {
                    return positions;
                }
            }
        }

        positions
    }

    /// Gets all positions with colors for GPU upload.
    /// Returns (position, color_index) where color_index maps to a stencil.
    pub fn get_gpu_positions(&self, max_positions: usize) -> Vec<(Vector3<i32>, usize)> {
        let mut positions = Vec::new();

        for (stencil_index, stencil) in self.active_stencils.iter().enumerate() {
            let preview_positions = stencil.get_preview_positions(max_positions - positions.len());
            for pos in preview_positions {
                positions.push((pos, stencil_index));

                if positions.len() >= max_positions {
                    return positions;
                }
            }
        }

        positions
    }

    /// Gets the list of active stencil IDs and names.
    pub fn list_active(&self) -> Vec<(u64, &str)> {
        self.active_stencils
            .iter()
            .map(|s| (s.id, s.stencil.name.as_str()))
            .collect()
    }

    /// Rotates a stencil by ID.
    pub fn rotate_stencil(&mut self, id: u64, clockwise: bool) {
        if let Some(stencil) = self.get_stencil_mut(id) {
            if clockwise {
                stencil.rotate_90();
            } else {
                stencil.rotate_90_ccw();
            }
        }
    }

    /// Translates a stencil by ID.
    pub fn translate_stencil(&mut self, id: u64, offset: Vector3<i32>) {
        if let Some(stencil) = self.get_stencil_mut(id) {
            stencil.translate(offset);
        }
    }

    /// Sets color for a specific stencil.
    pub fn set_stencil_color(&mut self, id: u64, color: [f32; 3]) {
        if let Some(stencil) = self.get_stencil_mut(id) {
            stencil.color = color;
        }
    }

    /// Gets the total number of positions across all stencils.
    pub fn total_position_count(&self) -> usize {
        self.active_stencils
            .iter()
            .map(|s| s.stencil.position_count())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stencils::format::StencilPosition;

    fn create_test_stencil(name: &str) -> StencilFile {
        let mut stencil = StencilFile::new(name.to_string(), "author".to_string(), 3, 3, 3);
        stencil.positions.push(StencilPosition { x: 0, y: 0, z: 0 });
        stencil.positions.push(StencilPosition { x: 1, y: 1, z: 1 });
        stencil
    }

    #[test]
    fn test_add_remove_stencil() {
        let mut manager = StencilManager::new();
        assert!(manager.is_empty());

        let id1 = manager.add_stencil(create_test_stencil("test1"), Vector3::new(0, 0, 0));
        assert_eq!(manager.count(), 1);
        assert_eq!(id1, 1);

        let id2 = manager.add_stencil(create_test_stencil("test2"), Vector3::new(10, 0, 10));
        assert_eq!(manager.count(), 2);
        assert_eq!(id2, 2);

        let removed = manager.remove_stencil(id1);
        assert!(removed.is_some());
        assert_eq!(manager.count(), 1);

        let remaining = manager.get_stencil(id2);
        assert!(remaining.is_some());
        assert_eq!(remaining.unwrap().stencil.name, "test2");
    }

    #[test]
    fn test_opacity() {
        let mut manager = StencilManager::new();

        // Test global opacity
        manager.set_global_opacity(0.6);
        assert_eq!(manager.global_opacity, 0.6);

        manager.set_global_opacity(0.1); // Should clamp to 0.3
        assert_eq!(manager.global_opacity, 0.3);

        manager.set_global_opacity(0.9); // Should clamp to 0.8
        assert_eq!(manager.global_opacity, 0.8);

        // Test adjustment
        manager.set_global_opacity(0.5);
        manager.adjust_global_opacity(0.1);
        assert_eq!(manager.global_opacity, 0.6);
    }

    #[test]
    fn test_render_mode() {
        let mut manager = StencilManager::new();
        assert_eq!(manager.render_mode, StencilRenderMode::Wireframe);

        manager.toggle_render_mode();
        assert_eq!(manager.render_mode, StencilRenderMode::Solid);

        manager.toggle_render_mode();
        assert_eq!(manager.render_mode, StencilRenderMode::Wireframe);
    }

    #[test]
    fn test_list_active() {
        let mut manager = StencilManager::new();

        manager.add_stencil(create_test_stencil("stencil_a"), Vector3::new(0, 0, 0));
        manager.add_stencil(create_test_stencil("stencil_b"), Vector3::new(10, 0, 10));

        let active = manager.list_active();
        assert_eq!(active.len(), 2);
        assert_eq!(active[0].1, "stencil_a");
        assert_eq!(active[1].1, "stencil_b");
    }

    #[test]
    fn test_clear() {
        let mut manager = StencilManager::new();

        manager.add_stencil(create_test_stencil("test1"), Vector3::new(0, 0, 0));
        manager.add_stencil(create_test_stencil("test2"), Vector3::new(10, 0, 10));
        assert_eq!(manager.count(), 2);

        manager.clear();
        assert!(manager.is_empty());
    }

    #[test]
    fn test_placement_mode() {
        let mut manager = StencilManager::new();

        let stencil = create_test_stencil("placement_test");
        let mut mode = StencilPlacementMode::new(stencil, Vector3::new(50, 64, 100));
        mode.rotate_90();

        let id = manager.add_from_placement(mode);
        assert_eq!(id, 1);

        let placed = manager.get_stencil(id).unwrap();
        assert_eq!(placed.rotation, 1);
        assert_eq!(placed.origin(), Vector3::new(50, 64, 100));
    }
}

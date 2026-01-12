//! Stencil state persistence for holographic building guides.
//!
//! Saves active stencil placements so they persist across world reloads.

#![allow(dead_code)] // Will be used when stencils are fully integrated

use crate::stencils::{PlacedStencil, StencilManager, StencilRenderMode};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

/// Serialized stencil state data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StencilState {
    /// List of active stencils in the world.
    #[serde(default)]
    pub active_stencils: Vec<PlacedStencil>,
    /// Next available stencil ID.
    #[serde(default = "default_next_id")]
    pub next_id: u64,
    /// Global opacity setting for new stencils.
    #[serde(default = "default_opacity")]
    pub global_opacity: f32,
    /// Render mode (wireframe or solid).
    #[serde(default)]
    pub render_mode: StencilRenderMode,
}

fn default_next_id() -> u64 {
    1
}

fn default_opacity() -> f32 {
    0.5
}

impl Default for StencilState {
    fn default() -> Self {
        Self {
            active_stencils: Vec::new(),
            next_id: default_next_id(),
            global_opacity: default_opacity(),
            render_mode: StencilRenderMode::default(),
        }
    }
}

impl StencilState {
    /// File name for stencil state data.
    pub const FILE_NAME: &'static str = "stencil_state.json";

    /// Creates a new empty stencil state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates stencil state from a StencilManager.
    pub fn from_manager(manager: &StencilManager) -> Self {
        Self {
            active_stencils: manager.active_stencils.clone(),
            next_id: manager.next_id,
            global_opacity: manager.global_opacity,
            render_mode: manager.render_mode,
        }
    }

    /// Applies this state to a StencilManager.
    pub fn apply_to_manager(&self, manager: &mut StencilManager) {
        manager.active_stencils = self.active_stencils.clone();
        manager.next_id = self.next_id;
        manager.global_opacity = self.global_opacity;
        manager.render_mode = self.render_mode;
    }

    /// Saves stencil state to a JSON file.
    pub fn save<P: AsRef<Path>>(&self, world_dir: P) -> Result<(), String> {
        let path = world_dir.as_ref().join(Self::FILE_NAME);
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize stencil state: {}", e))?;
        let mut file = File::create(&path).map_err(|e| e.to_string())?;
        file.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Loads stencil state from a JSON file.
    /// Returns empty state if file doesn't exist (backwards compatibility).
    pub fn load<P: AsRef<Path>>(world_dir: P) -> Self {
        let path = world_dir.as_ref().join(Self::FILE_NAME);
        if !path.exists() {
            return Self::new();
        }

        let result = (|| -> Result<Self, String> {
            let mut file = File::open(&path).map_err(|e| e.to_string())?;
            let mut json = String::new();
            file.read_to_string(&mut json).map_err(|e| e.to_string())?;
            serde_json::from_str(&json).map_err(|e| format!("Failed to parse stencil state: {}", e))
        })();

        match result {
            Ok(state) => state,
            Err(e) => {
                eprintln!("[Storage] Warning: Failed to load stencil state: {}", e);
                Self::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stencils::{StencilFile, StencilPosition};
    use nalgebra::Vector3;
    use std::env;
    use std::fs;

    #[test]
    fn test_stencil_state_round_trip() {
        // Create temp directory
        let temp_dir = env::temp_dir().join("voxel_world_stencil_state_test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Create a stencil state with some data
        let mut stencil = StencilFile::new("test".to_string(), "author".to_string(), 10, 10, 10);
        stencil.positions.push(StencilPosition { x: 0, y: 0, z: 0 });
        stencil.positions.push(StencilPosition { x: 1, y: 1, z: 1 });

        let placed = PlacedStencil::new(1, stencil, Vector3::new(100, 64, 200));

        let state = StencilState {
            active_stencils: vec![placed],
            next_id: 2,
            global_opacity: 0.7,
            render_mode: StencilRenderMode::Solid,
        };

        // Save and load
        state.save(&temp_dir).unwrap();
        let loaded = StencilState::load(&temp_dir);

        // Verify
        assert_eq!(loaded.active_stencils.len(), 1);
        assert_eq!(loaded.next_id, 2);
        assert!((loaded.global_opacity - 0.7).abs() < 0.01);
        assert_eq!(loaded.render_mode, StencilRenderMode::Solid);

        let loaded_stencil = &loaded.active_stencils[0];
        assert_eq!(loaded_stencil.id, 1);
        assert_eq!(loaded_stencil.stencil.name, "test");
        assert_eq!(loaded_stencil.origin(), Vector3::new(100, 64, 200));
        assert_eq!(loaded_stencil.stencil.positions.len(), 2);

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_missing_file_returns_default() {
        let temp_dir = env::temp_dir().join("voxel_world_stencil_missing_test");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Load from non-existent file
        let state = StencilState::load(&temp_dir);

        // Should return defaults
        assert!(state.active_stencils.is_empty());
        assert_eq!(state.next_id, 1);
        assert!((state.global_opacity - 0.5).abs() < 0.01);
        assert_eq!(state.render_mode, StencilRenderMode::Wireframe);

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }
}

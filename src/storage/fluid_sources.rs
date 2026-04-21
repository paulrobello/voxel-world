//! Fluid source persistence for water and lava simulation.
//!
//! Saves water and lava source positions (blocks with is_source=true)
//! so the simulation continues correctly after world reload.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

/// Serialized fluid source data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FluidSources {
    /// Water source positions [x, y, z].
    #[serde(default)]
    pub water: Vec<[i32; 3]>,
    /// Lava source positions [x, y, z].
    #[serde(default)]
    pub lava: Vec<[i32; 3]>,
}

impl FluidSources {
    /// File name for fluid sources data.
    pub const FILE_NAME: &'static str = "fluid_sources.json";

    /// Creates a new empty fluid sources struct.
    pub fn new() -> Self {
        Self::default()
    }

    /// Saves fluid sources to a JSON file.
    pub fn save<P: AsRef<Path>>(&self, world_dir: P) -> Result<(), String> {
        let path = world_dir.as_ref().join(Self::FILE_NAME);
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize fluid sources: {}", e))?;
        let mut file = File::create(&path).map_err(|e| e.to_string())?;
        file.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Loads fluid sources from a JSON file.
    /// Returns empty sources if file doesn't exist (backwards compatibility).
    pub fn load<P: AsRef<Path>>(world_dir: P) -> Self {
        let path = world_dir.as_ref().join(Self::FILE_NAME);
        if !path.exists() {
            return Self::new();
        }

        let result = (|| -> Result<Self, String> {
            let mut file = File::open(&path).map_err(|e| e.to_string())?;
            let mut json = String::new();
            file.read_to_string(&mut json).map_err(|e| e.to_string())?;
            serde_json::from_str(&json).map_err(|e| format!("Failed to parse fluid sources: {}", e))
        })();

        match result {
            Ok(sources) => sources,
            Err(e) => {
                log::warn!("[Storage] Warning: Failed to load fluid sources: {}", e);
                Self::new()
            }
        }
    }
}

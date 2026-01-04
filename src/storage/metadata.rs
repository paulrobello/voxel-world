use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

fn default_time() -> f32 {
    0.25
}

/// World-level metadata stored in level.dat.
/// Player-specific data (position, rotation) is stored in user_prefs.json instead.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldMetadata {
    pub seed: u32,
    pub spawn_pos: [f64; 3],
    pub version: u32,
    #[serde(default = "default_time")]
    pub time_of_day: f32,
    #[serde(default)]
    pub day_cycle_paused: bool,
}

impl WorldMetadata {
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize metadata: {}", e))?;
        let mut file = File::create(path).map_err(|e| e.to_string())?;
        file.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let mut file = File::open(path).map_err(|e| e.to_string())?;
        let mut json = String::new();
        file.read_to_string(&mut json).map_err(|e| e.to_string())?;

        // Use serde(default) on fields to handle legacy metadata files
        let metadata: WorldMetadata =
            serde_json::from_str(&json).map_err(|e| format!("Failed to parse metadata: {}", e))?;

        Ok(metadata)
    }
}

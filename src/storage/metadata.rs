use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerData {
    pub position: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
}

fn default_time() -> f32 {
    0.25
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldMetadata {
    pub seed: u32,
    pub spawn_pos: [f64; 3],
    pub version: u32,
    #[serde(default = "default_time")]
    pub time_of_day: f32,
    #[serde(default)]
    pub day_cycle_paused: bool,
    #[serde(default)]
    pub player: Option<PlayerData>,
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

        let metadata: WorldMetadata =
            serde_json::from_str(&json).map_err(|e| format!("Failed to parse metadata: {}", e))?;

        // Ensure legacy metadata (without these fields) gets defaults

        // Note: Serde's default behavior for missing fields would be error unless we use #[serde(default)]
        // but since we are modifying the struct, let's just make sure we handle it.
        // Actually, for a simple struct change like this, serde will fail if fields are missing
        // unless we make them Option or provide defaults.
        // Let's rely on serde(default) for new fields if possible, but we can't easily modify the struct definition retroactively.
        // A better approach for migration is to implement a manual default fallback or use Option.
        // I made player Option, but time_of_day and day_cycle_paused are primitive types.
        // I will trust the user to start a new world or handle the migration logic in main.rs if load fails?
        // No, load will fail on existing saves.
        // Let's implement a custom default for the struct or use #[serde(default)] on the new fields.

        Ok(metadata)
    }
}

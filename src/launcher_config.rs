use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Read;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LauncherConfig {
    pub last_world: Option<String>,
    pub recent_worlds: Vec<String>,
}

impl LauncherConfig {
    pub fn load() -> Self {
        let path = PathBuf::from("launcher.json");
        if !path.exists() {
            return Self::default();
        }

        if let Ok(mut file) = File::open(&path) {
            let mut json = String::new();
            if file.read_to_string(&mut json).is_ok() {
                if let Ok(config) = serde_json::from_str(&json) {
                    return config;
                }
            }
        }

        Self::default()
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write("launcher.json", json);
        }
    }

    pub fn update_last_world(&mut self, world_name: &str) {
        self.last_world = Some(world_name.to_string());

        // Remove if exists to move to top
        if let Some(pos) = self.recent_worlds.iter().position(|x| x == world_name) {
            self.recent_worlds.remove(pos);
        }
        // Add to front
        self.recent_worlds.insert(0, world_name.to_string());
        // Keep only last 10
        if self.recent_worlds.len() > 10 {
            self.recent_worlds.truncate(10);
        }

        self.save();
    }
}

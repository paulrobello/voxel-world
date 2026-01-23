//! User preferences persistence.
//!
//! Saves and loads user settings and hotbar configuration to a local JSON file.
//! Also stores per-world player data (position, rotation) for co-op/networked worlds.

use crate::chunk::BlockType;
use crate::config::Settings;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Default preferences file name.
const PREFS_FILE_NAME: &str = "user_prefs.json";

/// Global data directory (set once at startup).
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Sets the data directory for all game data (worlds, preferences, models).
/// Must be called once at startup before any data access.
pub fn set_data_dir(dir: &Path) {
    let _ = DATA_DIR.set(dir.to_path_buf());
}

/// Returns the data directory (defaults to current directory if not set).
pub fn get_data_dir() -> PathBuf {
    DATA_DIR
        .get()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Returns the path to the worlds directory.
pub fn worlds_dir() -> PathBuf {
    get_data_dir().join("worlds")
}

/// Returns the path to the user models directory.
pub fn user_models_dir() -> PathBuf {
    get_data_dir().join("user_models")
}

/// Returns the path to the user templates directory.
#[allow(dead_code)] // TODO: Remove once template system is integrated
pub fn user_templates_dir() -> PathBuf {
    get_data_dir().join("user_templates")
}

/// Returns the path to the user stencils directory.
pub fn user_stencils_dir() -> PathBuf {
    get_data_dir().join("user_stencils")
}

/// Returns the path to the profiles directory.
pub fn profiles_dir() -> PathBuf {
    get_data_dir().join("profiles")
}

/// Player-specific data for a world (position, rotation).
/// Stored per-user rather than per-world for co-op/networked support.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldPlayerData {
    /// Player position in world coordinates.
    pub position: [f64; 3],
    /// Horizontal rotation (yaw) in radians.
    pub yaw: f32,
    /// Vertical rotation (pitch) in radians.
    pub pitch: f32,
}

impl Default for WorldPlayerData {
    fn default() -> Self {
        Self {
            position: [0.0, 64.0, 0.0],
            yaw: 0.0,
            pitch: 0.0,
        }
    }
}

/// User preferences that are persisted to disk.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct UserPreferences {
    /// User settings (rendering, gameplay options).
    pub settings: Settings,

    /// Currently selected hotbar slot index (0-8).
    pub hotbar_index: usize,

    /// Block types in each hotbar slot.
    pub hotbar_blocks: [u8; 9],

    /// Model IDs for each hotbar slot (for Model blocks).
    pub hotbar_model_ids: [u8; 9],

    /// Tint indices for each hotbar slot (for TintedGlass blocks).
    pub hotbar_tint_indices: [u8; 9],

    /// Paint texture indices for each hotbar slot (for Painted blocks).
    pub hotbar_paint_textures: [u8; 9],

    /// Whether to show the minimap.
    pub show_minimap: bool,

    /// Currently selected picture ID for picture frames.
    pub selected_picture_id: Option<u32>,

    /// Last loaded world name.
    pub last_world: Option<String>,

    /// Recently played worlds (most recent first, max 10).
    #[serde(default)]
    pub recent_worlds: Vec<String>,

    /// Per-world player data (position, rotation).
    /// Key is the world name (folder name).
    #[serde(default)]
    pub world_player_data: HashMap<String, WorldPlayerData>,

    /// Last fly-mode setting (None = use defaults/CLI).
    #[serde(default)]
    pub last_fly_mode: Option<bool>,

    /// Console command history (most recent last, max 100 entries).
    #[serde(default)]
    pub console_history: Vec<String>,

    /// Saved positions per world.
    /// Key is the world name, value is a map of position name to coordinates.
    #[serde(default)]
    pub saved_positions: HashMap<String, HashMap<String, [f64; 3]>>,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            hotbar_index: 0,
            hotbar_blocks: [
                BlockType::Stone as u8,
                BlockType::Dirt as u8,
                BlockType::Grass as u8,
                BlockType::Planks as u8,
                BlockType::Cobblestone as u8,
                BlockType::Glass as u8,
                BlockType::Model as u8,
                BlockType::Model as u8,
                BlockType::Model as u8,
            ],
            hotbar_model_ids: [0, 0, 0, 0, 0, 0, 1, 4, 20],
            hotbar_tint_indices: [0; 9],
            hotbar_paint_textures: [BlockType::Stone as u8; 9],
            show_minimap: true,
            selected_picture_id: None,
            last_world: None,
            recent_worlds: Vec::new(),
            world_player_data: HashMap::new(),
            last_fly_mode: None,
            console_history: Vec::new(),
            saved_positions: HashMap::new(),
        }
    }
}

impl UserPreferences {
    /// Saves a named position for a specific world.
    pub fn save_position(&mut self, world_name: &str, name: &str, position: [f64; 3]) {
        self.saved_positions
            .entry(world_name.to_string())
            .or_default()
            .insert(name.to_string(), position);
    }

    /// Deletes a named position for a specific world.
    /// Returns true if the position existed and was deleted.
    pub fn delete_position(&mut self, world_name: &str, name: &str) -> bool {
        if let Some(world_positions) = self.saved_positions.get_mut(world_name) {
            world_positions.remove(name).is_some()
        } else {
            false
        }
    }

    /// Gets a named position for a specific world.
    pub fn get_position(&self, world_name: &str, name: &str) -> Option<[f64; 3]> {
        self.saved_positions
            .get(world_name)
            .and_then(|positions| positions.get(name).copied())
    }

    /// Gets all saved position names for a specific world.
    pub fn get_position_names(&self, world_name: &str) -> Vec<String> {
        self.saved_positions
            .get(world_name)
            .map(|positions| positions.keys().cloned().collect())
            .unwrap_or_default()
    }
}

impl UserPreferences {
    /// Gets player data for a specific world, or None if not found.
    pub fn get_player_data(&self, world_name: &str) -> Option<&WorldPlayerData> {
        self.world_player_data.get(world_name)
    }

    /// Sets player data for a specific world.
    pub fn set_player_data(&mut self, world_name: &str, data: WorldPlayerData) {
        self.world_player_data.insert(world_name.to_string(), data);
    }

    /// Updates the last played world and adds it to recent worlds list.
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
    }
}

impl UserPreferences {
    /// Returns the path to the preferences file.
    fn prefs_path() -> PathBuf {
        get_data_dir().join(PREFS_FILE_NAME)
    }

    /// Loads user preferences from the JSON file.
    /// Returns default preferences if the file doesn't exist or is invalid.
    pub fn load() -> Self {
        let path = Self::prefs_path();

        if !path.exists() {
            return Self::default();
        }

        match fs::read_to_string(&path) {
            Ok(contents) => match serde_json::from_str(&contents) {
                Ok(prefs) => {
                    println!("[Prefs] Loaded user preferences from {}", path.display());
                    prefs
                }
                Err(e) => {
                    eprintln!(
                        "[Prefs] Failed to parse {}: {}. Using defaults.",
                        path.display(),
                        e
                    );
                    Self::default()
                }
            },
            Err(e) => {
                eprintln!(
                    "[Prefs] Failed to read {}: {}. Using defaults.",
                    path.display(),
                    e
                );
                Self::default()
            }
        }
    }

    /// Saves user preferences to the JSON file.
    pub fn save(&self) {
        let path = Self::prefs_path();

        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = fs::write(&path, json) {
                    eprintln!("[Prefs] Failed to write {}: {}", path.display(), e);
                }
            }
            Err(e) => {
                eprintln!("[Prefs] Failed to serialize preferences: {}", e);
            }
        }
    }

    /// Converts hotbar_blocks from u8 array to BlockType array.
    pub fn get_hotbar_blocks(&self) -> [BlockType; 9] {
        let mut blocks = [BlockType::Air; 9];
        for (i, &b) in self.hotbar_blocks.iter().enumerate() {
            blocks[i] = BlockType::from(b);
        }
        blocks
    }

    /// Sets hotbar_blocks from BlockType array.
    pub fn set_hotbar_blocks(&mut self, blocks: &[BlockType; 9]) {
        for (i, block) in blocks.iter().enumerate() {
            self.hotbar_blocks[i] = *block as u8;
        }
    }
}

//! User preferences persistence.
//!
//! Saves and loads user settings and hotbar configuration to a local JSON file.

use crate::chunk::BlockType;
use crate::config::Settings;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Default preferences file name.
const PREFS_FILE_NAME: &str = "user_prefs.json";

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

    /// Whether to show the minimap.
    pub show_minimap: bool,

    /// Last loaded world name.
    pub last_world: Option<String>,
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
            show_minimap: true,
            last_world: None,
        }
    }
}

impl UserPreferences {
    /// Returns the path to the preferences file.
    fn prefs_path() -> PathBuf {
        // Store preferences in the current working directory
        PathBuf::from(PREFS_FILE_NAME)
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

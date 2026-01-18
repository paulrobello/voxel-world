//! Paint preset management for saving and loading favorite paint configurations.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use super::system::PaintConfig;
use crate::user_prefs::get_data_dir;

/// Maximum number of presets that can be stored.
pub const MAX_PRESETS: usize = 64;

/// A named paint preset containing one or more paint configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaintPreset {
    /// Unique name for the preset.
    pub name: String,
    /// Paint configurations in this preset (typically 1, but can be multiple for patterns).
    pub configs: Vec<PaintConfig>,
    /// Optional description.
    pub description: Option<String>,
    /// Whether this is a built-in preset (cannot be deleted).
    #[serde(default)]
    pub builtin: bool,
}

impl PaintPreset {
    /// Creates a new preset with a single paint config.
    pub fn new(name: impl Into<String>, config: PaintConfig) -> Self {
        Self {
            name: name.into(),
            configs: vec![config],
            description: None,
            builtin: false,
        }
    }

    /// Creates a new preset with multiple paint configs.
    pub fn with_configs(
        name: impl Into<String>,
        configs: Vec<PaintConfig>,
        description: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            configs,
            description,
            builtin: false,
        }
    }

    /// Returns the primary (first) paint config.
    pub fn primary_config(&self) -> Option<&PaintConfig> {
        self.configs.first()
    }
}

/// Library of paint presets with persistence.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PaintPresetLibrary {
    /// User-defined presets.
    presets: Vec<PaintPreset>,
    /// Index of the currently selected preset (if any).
    #[serde(skip)]
    selected: Option<usize>,
}

impl PaintPresetLibrary {
    /// Creates a new empty library.
    pub fn new() -> Self {
        Self {
            presets: Vec::new(),
            selected: None,
        }
    }

    /// Creates a library with default built-in presets.
    pub fn with_defaults() -> Self {
        let mut lib = Self::new();
        lib.add_builtin_presets();
        lib
    }

    /// Adds the built-in default presets.
    fn add_builtin_presets(&mut self) {
        use super::system::{BlendMode, HsvAdjustment};

        // Basic solid colors
        let builtins = [
            PaintPreset {
                name: "Stone Gray".to_string(),
                configs: vec![PaintConfig::simple(1, 13)], // Stone texture, light gray tint
                description: Some("Clean gray stone".to_string()),
                builtin: true,
            },
            PaintPreset {
                name: "Warm Wood".to_string(),
                configs: vec![PaintConfig::new(
                    4,  // Planks
                    17, // Peach
                    HsvAdjustment::new(0.0, 1.2, 1.1),
                    BlendMode::Multiply,
                )],
                description: Some("Warm wooden tone".to_string()),
                builtin: true,
            },
            PaintPreset {
                name: "Cool Brick".to_string(),
                configs: vec![PaintConfig::new(
                    12, // Brick
                    7,  // Sky blue
                    HsvAdjustment::new(-15.0, 0.8, 1.0),
                    BlendMode::Overlay,
                )],
                description: Some("Blue-tinted brick".to_string()),
                builtin: true,
            },
            PaintPreset {
                name: "Aged Stone".to_string(),
                configs: vec![PaintConfig::new(
                    14, // Cobblestone
                    15, // Brown
                    HsvAdjustment::new(0.0, 0.7, 0.85),
                    BlendMode::SoftLight,
                )],
                description: Some("Weathered, mossy stone".to_string()),
                builtin: true,
            },
            PaintPreset {
                name: "Vibrant Red".to_string(),
                configs: vec![PaintConfig::new(
                    1, // Stone
                    0, // Red
                    HsvAdjustment::new(0.0, 1.5, 1.2),
                    BlendMode::Screen,
                )],
                description: Some("Bright red glow".to_string()),
                builtin: true,
            },
            PaintPreset {
                name: "Pure Tint".to_string(),
                configs: vec![PaintConfig::new(
                    1, // Stone
                    8, // Blue
                    HsvAdjustment::default(),
                    BlendMode::ColorOnly,
                )],
                description: Some("Texture detail with pure color".to_string()),
                builtin: true,
            },
        ];

        for preset in builtins {
            self.presets.push(preset);
        }
    }

    /// Returns the number of presets.
    pub fn len(&self) -> usize {
        self.presets.len()
    }

    /// Returns true if the library is empty.
    pub fn is_empty(&self) -> bool {
        self.presets.is_empty()
    }

    /// Returns an iterator over all presets.
    pub fn iter(&self) -> impl Iterator<Item = &PaintPreset> {
        self.presets.iter()
    }

    /// Gets a preset by index.
    pub fn get(&self, index: usize) -> Option<&PaintPreset> {
        self.presets.get(index)
    }

    /// Gets a preset by name.
    pub fn get_by_name(&self, name: &str) -> Option<&PaintPreset> {
        self.presets.iter().find(|p| p.name == name)
    }

    /// Adds a new preset. Returns the index if successful.
    pub fn add(&mut self, preset: PaintPreset) -> Option<usize> {
        if self.presets.len() >= MAX_PRESETS {
            return None;
        }

        // Check for duplicate name
        if self.presets.iter().any(|p| p.name == preset.name) {
            return None;
        }

        let index = self.presets.len();
        self.presets.push(preset);
        Some(index)
    }

    /// Removes a preset by index. Returns the removed preset if successful.
    /// Built-in presets cannot be removed.
    pub fn remove(&mut self, index: usize) -> Option<PaintPreset> {
        if index >= self.presets.len() {
            return None;
        }

        if self.presets[index].builtin {
            return None;
        }

        // Adjust selected index if needed
        if let Some(sel) = self.selected {
            if sel == index {
                self.selected = None;
            } else if sel > index {
                self.selected = Some(sel - 1);
            }
        }

        Some(self.presets.remove(index))
    }

    /// Updates a preset at the given index.
    pub fn update(&mut self, index: usize, preset: PaintPreset) -> bool {
        if index >= self.presets.len() {
            return false;
        }

        // Cannot modify built-in presets
        if self.presets[index].builtin {
            return false;
        }

        self.presets[index] = preset;
        true
    }

    /// Gets the currently selected preset index.
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Sets the currently selected preset index.
    pub fn select(&mut self, index: Option<usize>) {
        self.selected = index.filter(|&i| i < self.presets.len());
    }

    /// Gets the currently selected preset.
    pub fn selected_preset(&self) -> Option<&PaintPreset> {
        self.selected.and_then(|i| self.presets.get(i))
    }

    /// Returns the default save path for presets.
    fn default_path() -> PathBuf {
        get_data_dir().join("paint_presets.json")
    }

    /// Loads presets from the default location.
    pub fn load() -> Self {
        Self::load_from(&Self::default_path())
    }

    /// Loads presets from a specific path.
    pub fn load_from(path: &PathBuf) -> Self {
        if let Ok(contents) = fs::read_to_string(path) {
            if let Ok(mut lib) = serde_json::from_str::<PaintPresetLibrary>(&contents) {
                // Ensure built-in presets exist
                lib.ensure_builtins();
                return lib;
            }
        }

        // Return defaults if loading fails
        Self::with_defaults()
    }

    /// Saves presets to the default location.
    pub fn save(&self) -> Result<(), String> {
        self.save_to(&Self::default_path())
    }

    /// Saves presets to a specific path.
    pub fn save_to(&self, path: &PathBuf) -> Result<(), String> {
        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
        }

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize presets: {}", e))?;

        fs::write(path, json).map_err(|e| format!("Failed to write presets file: {}", e))
    }

    /// Ensures all built-in presets are present.
    fn ensure_builtins(&mut self) {
        let defaults = Self::with_defaults();
        for builtin in defaults.presets {
            if !self.presets.iter().any(|p| p.name == builtin.name) {
                self.presets.insert(0, builtin);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preset_creation() {
        let config = PaintConfig::default();
        let preset = PaintPreset::new("Test", config);

        assert_eq!(preset.name, "Test");
        assert_eq!(preset.configs.len(), 1);
        assert!(!preset.builtin);
    }

    #[test]
    fn test_library_defaults() {
        let lib = PaintPresetLibrary::with_defaults();
        assert!(!lib.is_empty());

        // All defaults should be built-in
        for preset in lib.iter() {
            assert!(preset.builtin);
        }
    }

    #[test]
    fn test_library_add_remove() {
        let mut lib = PaintPresetLibrary::new();

        let preset = PaintPreset::new("Custom", PaintConfig::default());
        let index = lib.add(preset).unwrap();

        assert_eq!(lib.len(), 1);
        assert_eq!(lib.get(index).unwrap().name, "Custom");

        let removed = lib.remove(index).unwrap();
        assert_eq!(removed.name, "Custom");
        assert!(lib.is_empty());
    }

    #[test]
    fn test_library_duplicate_name() {
        let mut lib = PaintPresetLibrary::new();

        let preset1 = PaintPreset::new("Test", PaintConfig::default());
        let preset2 = PaintPreset::new("Test", PaintConfig::default());

        assert!(lib.add(preset1).is_some());
        assert!(lib.add(preset2).is_none()); // Duplicate name rejected
    }

    #[test]
    fn test_cannot_remove_builtin() {
        let mut lib = PaintPresetLibrary::with_defaults();
        let initial_len = lib.len();

        assert!(lib.remove(0).is_none()); // Built-in preset
        assert_eq!(lib.len(), initial_len);
    }
}

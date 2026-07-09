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

/// Returns the path to the worlds directory within the given data directory.
/// Prefer this variant in tests and library code to avoid depending on the global OnceLock.
pub fn worlds_dir_in(data_dir: &Path) -> PathBuf {
    data_dir.join("worlds")
}

/// Returns the path to the worlds directory using the global data directory.
pub fn worlds_dir() -> PathBuf {
    worlds_dir_in(&get_data_dir())
}

/// Returns the path to the user models directory within the given data directory.
/// Prefer this variant in tests and library code to avoid depending on the global OnceLock.
pub fn user_models_dir_in(data_dir: &Path) -> PathBuf {
    data_dir.join("user_models")
}

/// Returns the path to the user models directory using the global data directory.
pub fn user_models_dir() -> PathBuf {
    user_models_dir_in(&get_data_dir())
}

/// Returns the path to the user templates directory within the given data directory.
/// Prefer this variant in tests and library code to avoid depending on the global OnceLock.
pub fn user_templates_dir_in(data_dir: &Path) -> PathBuf {
    data_dir.join("user_templates")
}

/// Returns the path to the user templates directory using the global data directory.
#[allow(dead_code)] // TODO: Remove once template system is integrated
pub fn user_templates_dir() -> PathBuf {
    user_templates_dir_in(&get_data_dir())
}

/// Returns the path to the user stencils directory within the given data directory.
/// Prefer this variant in tests and library code to avoid depending on the global OnceLock.
pub fn user_stencils_dir_in(data_dir: &Path) -> PathBuf {
    data_dir.join("user_stencils")
}

/// Returns the path to the user stencils directory using the global data directory.
pub fn user_stencils_dir() -> PathBuf {
    user_stencils_dir_in(&get_data_dir())
}

/// Returns the path to the profiles directory within the given data directory.
/// Prefer this variant in tests and library code to avoid depending on the global OnceLock.
pub fn profiles_dir_in(data_dir: &Path) -> PathBuf {
    data_dir.join("profiles")
}

/// Returns the path to the profiles directory using the global data directory.
pub fn profiles_dir() -> PathBuf {
    profiles_dir_in(&get_data_dir())
}

/// FNV-1a 32-bit offset basis and prime (used for short server-address hashes).
const FNV1A_32_OFFSET_BASIS: u32 = 0x811c9dc5;
const FNV1A_32_PRIME: u32 = 0x0100_0193;

/// Computes the FNV-1a 32-bit hash of `s`. Deterministic and dependency-free.
fn fnv1a_32(s: &str) -> u32 {
    let mut hash = FNV1A_32_OFFSET_BASIS;
    for byte in s.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(FNV1A_32_PRIME);
    }
    hash
}

/// Replaces every character that is not filesystem-safe (`[A-Za-z0-9._-]`)
/// with `_`, and truncates the result so very long addresses (e.g. IPv6
/// literals) cannot produce oversized path components. The hash suffix
/// added by [`remote_cache_dir_in`] disambiguates any collisions the
/// sanitization could introduce.
fn sanitize_server_addr(addr: &str) -> String {
    const MAX_LEN: usize = 48;
    addr.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(MAX_LEN)
        .collect()
}

/// Returns the per-server cache directory a remote client uses for its own
/// local saves (STOR-005). Lives under the worlds directory so each remote
/// server's client-side edits stay isolated: connecting to server B never
/// overwrites server A's cached edits.
///
/// Format: `<worlds_dir>/remote__<sanitized_addr>__<fnv1a_hex>`. The raw
/// address is never used unsanitized (it contains `:` and possibly `/`),
/// and the short FNV-1a hash suffix disambiguates addresses that sanitize
/// to the same basename. Prefer [`remote_cache_dir_in`] in tests/library
/// code; [`remote_cache_dir`] reads the global data directory.
pub fn remote_cache_dir_in(worlds_dir: &Path, server_addr: &str) -> PathBuf {
    let sanitized = sanitize_server_addr(server_addr);
    let hash = fnv1a_32(server_addr);
    worlds_dir.join(format!("remote__{sanitized}__{hash:08x}"))
}

/// Returns the per-server cache directory using the global worlds directory.
#[allow(dead_code)] // Pair with `remote_cache_dir_in`; the `_in` form is preferred in callers.
pub fn remote_cache_dir(server_addr: &str) -> PathBuf {
    remote_cache_dir_in(&worlds_dir(), server_addr)
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

/// Default author name for templates and stencils.
fn default_author() -> String {
    "Player".to_string()
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

    /// Author name used when saving templates and stencils.
    /// Defaults to "Player" so existing preference files without this field load correctly.
    #[serde(default = "default_author")]
    pub author: String,
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
            author: default_author(),
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
    /// Returns the path to the preferences file within the given data directory.
    fn prefs_path_in(data_dir: &Path) -> PathBuf {
        data_dir.join(PREFS_FILE_NAME)
    }

    /// Loads user preferences from the JSON file in the given data directory.
    /// Returns default preferences if the file doesn't exist or is invalid.
    /// Use this variant in tests or when the data directory is not yet set globally.
    pub fn load_from(data_dir: &Path) -> Self {
        let path = Self::prefs_path_in(data_dir);

        if !path.exists() {
            return Self::default();
        }

        match fs::read_to_string(&path) {
            Ok(contents) => match serde_json::from_str(&contents) {
                Ok(prefs) => {
                    log::debug!("[Prefs] Loaded user preferences from {}", path.display());
                    prefs
                }
                Err(e) => {
                    log::warn!(
                        "[Prefs] Failed to parse {}: {}. Using defaults.",
                        path.display(),
                        e
                    );
                    Self::default()
                }
            },
            Err(e) => {
                log::warn!(
                    "[Prefs] Failed to read {}: {}. Using defaults.",
                    path.display(),
                    e
                );
                Self::default()
            }
        }
    }

    /// Loads user preferences from the JSON file using the global data directory.
    /// Returns default preferences if the file doesn't exist or is invalid.
    pub fn load() -> Self {
        Self::load_from(&get_data_dir())
    }

    /// Saves user preferences to the JSON file in the given data directory.
    /// Use this variant in tests or when the data directory is not yet set globally.
    pub fn save_to(&self, data_dir: &Path) {
        let path = Self::prefs_path_in(data_dir);

        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = fs::write(&path, json) {
                    log::warn!("[Prefs] Failed to write {}: {}", path.display(), e);
                }
            }
            Err(e) => {
                log::warn!("[Prefs] Failed to serialize preferences: {}", e);
            }
        }
    }

    /// Saves user preferences to the JSON file using the global data directory.
    pub fn save(&self) {
        self.save_to(&get_data_dir())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Test that `_in` directory helpers return paths relative to the supplied dir
    /// without touching the global OnceLock.
    #[test]
    fn dir_helpers_with_explicit_path() {
        let base = PathBuf::from("/tmp/test_data");
        assert_eq!(worlds_dir_in(&base), base.join("worlds"));
        assert_eq!(user_models_dir_in(&base), base.join("user_models"));
        assert_eq!(user_templates_dir_in(&base), base.join("user_templates"));
        assert_eq!(user_stencils_dir_in(&base), base.join("user_stencils"));
        assert_eq!(profiles_dir_in(&base), base.join("profiles"));
    }

    /// FNV-1a 32-bit reference vectors — guards against a broken hash table or
    /// accidental algorithm swap, since the hash suffix is the only collision
    /// defense between two sanitized addresses.
    #[test]
    fn fnv1a_32_reference_vectors() {
        assert_eq!(fnv1a_32(""), FNV1A_32_OFFSET_BASIS);
        // Known FNV-1a 32-bit values from the reference test suite.
        assert_eq!(fnv1a_32("a"), 0xe40c292c);
        assert_eq!(fnv1a_32("foobar"), 0xbf9cf968);
    }

    /// Sanitizer must keep filesystem-safe chars (`[A-Za-z0-9._-]`) and replace
    /// everything else (notably `:` from `host:port` and `[]/` from IPv6/path
    /// forms) with `_`.
    #[test]
    fn sanitize_server_addr_replaces_unsafe_chars() {
        assert_eq!(sanitize_server_addr("127.0.0.1:12345"), "127.0.0.1_12345");
        assert_eq!(
            sanitize_server_addr("example.com:25565"),
            "example.com_25565"
        );
        // IPv6 bracketed form — brackets and extra colons become `_`.
        // (`[` + `:` + `:` + `1` + `]` + `:` + `12345` → `___1__12345`.)
        assert_eq!(sanitize_server_addr("[::1]:12345"), "___1__12345");
        // Path separators are scrubbed.
        assert_eq!(sanitize_server_addr("a/b\\c"), "a_b_c");
        // Safe characters round-trip unchanged.
        assert_eq!(sanitize_server_addr("Host-1.example_2"), "Host-1.example_2");
    }

    /// `remote_cache_dir_in` is deterministic (same address → same dir),
    /// distinguishes different addresses, lives under the supplied worlds dir,
    /// and its basename matches the filesystem-safe pattern
    /// `remote__<safe>__<8-hex>` with no illegal characters.
    #[test]
    fn remote_cache_dir_is_deterministic_and_safe() {
        let base = PathBuf::from("/tmp/test_worlds");
        let a1 = remote_cache_dir_in(&base, "127.0.0.1:12345");
        let a2 = remote_cache_dir_in(&base, "127.0.0.1:12345");
        // Determinism: same address → identical path.
        assert_eq!(a1, a2);
        // Lives under the supplied worlds dir.
        assert!(a1.starts_with(&base));

        // Different addresses → different dirs (both sanitized form and hash).
        let b = remote_cache_dir_in(&base, "127.0.0.1:54321");
        assert_ne!(a1, b);
        let c = remote_cache_dir_in(&base, "example.com:25565");
        assert_ne!(a1, c);
        assert_ne!(b, c);

        // Basename matches `remote__<safe-chars>__<8 lowercase hex>`.
        // Asserting the pattern (rather than a hardcoded string) keeps the test
        // robust to FNV prime changes while still proving filesystem-safety.
        let basename = a1
            .file_name()
            .and_then(|s| s.to_str())
            .expect("basename must be valid UTF-8");
        let chars = basename.chars();
        assert!(basename.starts_with("remote__"));
        // Validate each char against the safe set, then check the hash tail.
        let tail = basename.trim_start_matches("remote__");
        let hash_idx = tail
            .rfind("__")
            .expect("basename must contain a __<hash> suffix");
        let (safe_part, hash_part) = (&tail[..hash_idx], &tail[hash_idx + 2..]);
        assert!(
            safe_part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_'),
            "sanitized portion must be filesystem-safe, got {safe_part}"
        );
        assert_eq!(
            hash_part.len(),
            8,
            "hash suffix must be 8 hex chars, got {hash_part}"
        );
        assert!(
            hash_part
                .chars()
                .all(|c: char| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "hash suffix must be lowercase hex, got {hash_part}"
        );
        let _ = chars;

        // No raw colon or slash anywhere in the basename (the STOR-005 hazard).
        assert!(!basename.contains(':'));
        assert!(!basename.contains('/'));
        assert!(!basename.contains('\\'));
    }

    /// Two addresses that sanitize to the same string still differ via the FNV
    /// hash suffix — this is the collision defense the hash provides.
    #[test]
    fn remote_cache_dir_hash_disambiguates_sanitization_collisions() {
        let base = PathBuf::from("/tmp/test_worlds");
        // `:` and `;` both sanitize to `_`, but the raw addresses differ.
        let x = remote_cache_dir_in(&base, "host:1234");
        let y = remote_cache_dir_in(&base, "host;1234");
        assert_ne!(
            x, y,
            "FNV hash suffix must distinguish addresses with the same sanitized form"
        );
    }

    /// Test that `load_from` returns defaults when the file does not exist.
    #[test]
    fn load_from_missing_file_returns_defaults() {
        let dir = PathBuf::from("/tmp/nonexistent_prefs_dir_abc123");
        let prefs = UserPreferences::load_from(&dir);
        assert_eq!(prefs.author, "Player");
        assert_eq!(prefs.hotbar_index, 0);
    }

    /// Test round-trip: save_to / load_from with a temp directory.
    #[test]
    fn save_to_and_load_from_roundtrip() {
        let dir = std::env::temp_dir().join("voxel_prefs_test_roundtrip");
        fs::create_dir_all(&dir).unwrap();

        let mut prefs = UserPreferences {
            author: "TestUser".to_string(),
            ..Default::default()
        };
        prefs.hotbar_index = 3;
        prefs.save_to(&dir);

        let loaded = UserPreferences::load_from(&dir);
        assert_eq!(loaded.author, "TestUser");
        assert_eq!(loaded.hotbar_index, 3);

        // Cleanup
        let _ = fs::remove_dir_all(&dir);
    }

    /// Test that loading a preferences JSON without an `author` field
    /// deserializes to the default value "Player" (serde default).
    #[test]
    fn load_from_json_without_author_uses_default() {
        let dir = std::env::temp_dir().join("voxel_prefs_test_no_author");
        fs::create_dir_all(&dir).unwrap();

        // Write a minimal prefs JSON with no `author` field
        let json = r#"{"hotbar_index": 5}"#;
        fs::write(dir.join(PREFS_FILE_NAME), json).unwrap();

        let prefs = UserPreferences::load_from(&dir);
        assert_eq!(
            prefs.author, "Player",
            "missing author field should default to 'Player'"
        );
        assert_eq!(prefs.hotbar_index, 5);

        // Cleanup
        let _ = fs::remove_dir_all(&dir);
    }

    /// Test that `author` field defaults to "Player" in Default impl.
    #[test]
    fn default_author_is_player() {
        let prefs = UserPreferences::default();
        assert_eq!(prefs.author, "Player");
    }

    /// Test that load_from and save_to with different directories don't interfere
    /// (parallel-safe because they don't touch the global OnceLock).
    #[test]
    fn parallel_load_from_different_dirs() {
        let dir_a = std::env::temp_dir().join("voxel_prefs_parallel_a");
        let dir_b = std::env::temp_dir().join("voxel_prefs_parallel_b");
        fs::create_dir_all(&dir_a).unwrap();
        fs::create_dir_all(&dir_b).unwrap();

        let prefs_a = UserPreferences {
            author: "Alice".to_string(),
            ..Default::default()
        };
        prefs_a.save_to(&dir_a);

        let prefs_b = UserPreferences {
            author: "Bob".to_string(),
            ..Default::default()
        };
        prefs_b.save_to(&dir_b);

        let loaded_a = UserPreferences::load_from(&dir_a);
        let loaded_b = UserPreferences::load_from(&dir_b);

        assert_eq!(loaded_a.author, "Alice");
        assert_eq!(loaded_b.author, "Bob");

        // Cleanup
        let _ = fs::remove_dir_all(&dir_a);
        let _ = fs::remove_dir_all(&dir_b);
    }
}

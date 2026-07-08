use crate::config::WorldGenType;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Default time of day for backwards compatibility (14:00 / 2pm).
fn default_time() -> f32 {
    14.0 / 24.0
}

/// World-level metadata stored in level.dat.
/// Player-specific data (position, rotation) is stored in user_prefs.json instead.
///
/// `version` tracks the on-disk format. Current writers emit `version = 2`;
/// `version = 1` files predate the `player_modified` flag and load with that
/// field defaulting to `false` via `#[serde(default)]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldMetadata {
    pub seed: u32,
    pub spawn_pos: [f64; 3],
    pub version: u32,
    #[serde(default = "default_time")]
    pub time_of_day: f32,
    #[serde(default)]
    pub day_cycle_paused: bool,
    /// World generation type (normal terrain or flat).
    /// Defaults to Normal for backwards compatibility with old worlds.
    #[serde(default)]
    pub world_gen: WorldGenType,
    /// Measurement marker positions for the rangefinder tool.
    #[serde(default)]
    pub measurement_markers: Vec<[i32; 3]>,
    /// True once this world has genuine local player edits (block placement,
    /// breaking, shape tools, console edits). Used to gate client-side saves so
    /// a cached/downloaded server world does not overwrite a different local
    /// world's data. Defaults to `false` so old `level.dat` files (version 1,
    /// no field) load cleanly as "not player-modified".
    #[serde(default)]
    pub player_modified: bool,
}

impl WorldMetadata {
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize metadata: {}", e))?;
        crate::storage::atomic::atomic_write_bytes(path.as_ref(), json.as_bytes())
            .map_err(|e| e.to_string())?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_metadata(player_modified: bool) -> WorldMetadata {
        WorldMetadata {
            seed: 42,
            spawn_pos: [1.5, 64.0, -2.25],
            version: 2,
            time_of_day: 0.3,
            day_cycle_paused: true,
            world_gen: WorldGenType::Normal,
            measurement_markers: vec![[10, 20, 30], [-7, 8, 9]],
            player_modified,
        }
    }

    /// `player_modified` survives a save/load round-trip for both true and false.
    #[test]
    fn player_modified_round_trips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("level.dat");

        for value in [true, false] {
            let saved = sample_metadata(value);
            saved.save(&path).expect("save");
            let loaded = WorldMetadata::load(&path).expect("load");
            assert_eq!(
                loaded.player_modified, value,
                "player_modified must round-trip through level.dat"
            );
            // Sanity: the other fields still round-trip too.
            assert_eq!(loaded.seed, 42);
            assert_eq!(loaded.measurement_markers, vec![[10, 20, 30], [-7, 8, 9]]);
        }
    }

    /// A fresh `WorldMetadata` (no field set) defaults `player_modified` to
    /// false. This is the value used for brand-new worlds.
    #[test]
    fn player_modified_defaults_false_on_fresh_metadata() {
        let fresh = WorldMetadata {
            seed: 1,
            spawn_pos: [0.0, 64.0, 0.0],
            version: 2,
            time_of_day: default_time(),
            day_cycle_paused: false,
            world_gen: WorldGenType::Normal,
            measurement_markers: Vec::new(),
            player_modified: false,
        };
        assert!(!fresh.player_modified);
    }

    /// STOR-004 backwards compatibility: a `level.dat` written by an older
    /// binary (version 1, no `player_modified` field) must load without error
    /// and yield `player_modified = false` via `#[serde(default)]`. This is
    /// the load path that protects existing worlds from a failed upgrade.
    #[test]
    fn legacy_level_dat_without_player_modified_loads_as_false() {
        // Hand-rolled JSON that mirrors the version-1 schema: every current
        // field except `player_modified`. This is exactly what an old binary
        // would have written.
        let legacy_json = r#"{
            "seed": 7,
            "spawn_pos": [0.0, 64.0, 0.0],
            "version": 1,
            "time_of_day": 0.5,
            "day_cycle_paused": false,
            "world_gen": "Normal",
            "measurement_markers": []
        }"#;

        let mut loaded: WorldMetadata =
            serde_json::from_str(legacy_json).expect("legacy metadata must parse");
        assert_eq!(loaded.seed, 7);
        assert_eq!(loaded.version, 1);
        assert!(
            !loaded.player_modified,
            "missing player_modified field must default to false, not fail"
        );

        // Round-trip: once re-saved by the new binary, the field is present
        // and the version bump is emitted.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("level.dat");
        loaded.player_modified = true;
        loaded.save(&path).expect("save");
        let reloaded = WorldMetadata::load(&path).expect("load");
        assert!(reloaded.player_modified);
    }
}

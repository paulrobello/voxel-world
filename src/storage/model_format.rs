//! Portable file format for sub-voxel models (.vxm).
//!
//! This module defines the VxmFile format for sharing models between worlds
//! and users, plus a LibraryManager for managing the user_models directory.

#![allow(dead_code)] // WIP: Editor integration pending

use crate::sub_voxel::{
    Color, LightBlocking, LightMode, ModelResolution, PALETTE_SIZE, SubVoxelModel,
};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Magic bytes for VXM format "VXM2"
const VXM_MAGIC: [u8; 4] = *b"VXM2";

/// Current version of the VXM format.
const VXM_VERSION: u16 = 2;

/// A portable file format for sub-voxel models (.vxm).
/// This allows models to be shared between worlds and users.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VxmFile {
    pub magic: [u8; 4],
    pub version: u16,
    pub resolution: u8, // 8, 16, or 32
    pub name: String,
    pub author: String,
    pub creation_date: u64,
    pub palette: Vec<u32>,          // RGBA8888 packed (32 entries)
    pub palette_emission: Vec<f32>, // Per-slot emission intensity (32 entries)
    pub voxels: Vec<u8>,            // resolution³ palette indices
    pub properties: ModelProps,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ModelProps {
    pub collision_mask: u64,
    pub is_transparent: bool, // Derived from LightBlocking != None
    pub light_emission: u32,  // RGBA packed, 0 = none (legacy, kept for compat)
    pub rotatable: bool,
    pub requires_ground_support: bool,
    // Light source properties
    pub is_light_source: bool,
    pub light_mode: u8,       // LightMode as u8
    pub light_radius: f32,    // Light radius in blocks
    pub light_intensity: f32, // Light intensity multiplier
}

impl VxmFile {
    /// Converts a runtime SubVoxelModel to a VxmFile.
    pub fn from_model(model: &SubVoxelModel, author: String) -> Self {
        let palette_packed: Vec<u32> = model
            .palette
            .iter()
            .map(|color| u32::from_le_bytes(color.to_array()))
            .collect();

        let emission_packed = if let Some(c) = model.emission {
            u32::from_le_bytes(c.to_array())
        } else {
            0
        };

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            magic: VXM_MAGIC,
            version: VXM_VERSION,
            resolution: model.resolution as u8,
            name: model.name.clone(),
            author,
            creation_date: timestamp,
            palette: palette_packed,
            palette_emission: model.palette_emission.to_vec(),
            voxels: model.voxels.clone(),
            properties: ModelProps {
                collision_mask: model.collision_mask,
                is_transparent: model.light_blocking != LightBlocking::Full,
                light_emission: emission_packed,
                rotatable: model.rotatable,
                requires_ground_support: model.requires_ground_support,
                is_light_source: model.is_light_source,
                light_mode: model.light_mode as u8,
                light_radius: model.light_radius,
                light_intensity: model.light_intensity,
            },
        }
    }

    /// Converts this VxmFile back to a runtime SubVoxelModel.
    /// Note: The ID is not set here (it's assigned by the registry).
    pub fn to_model(&self) -> SubVoxelModel {
        // Parse resolution from file
        let resolution = match self.resolution {
            8 => ModelResolution::Low,
            16 => ModelResolution::Medium,
            32 => ModelResolution::High,
            _ => ModelResolution::Medium, // Default fallback
        };

        // Load palette colors
        let mut palette = [Color::default(); PALETTE_SIZE];
        for (i, &packed) in self.palette.iter().take(PALETTE_SIZE).enumerate() {
            let [r, g, b, a] = packed.to_le_bytes();
            palette[i] = Color { r, g, b, a };
        }

        // Load palette emission values
        let mut palette_emission = [0.0f32; PALETTE_SIZE];
        for (i, &emission) in self.palette_emission.iter().take(PALETTE_SIZE).enumerate() {
            palette_emission[i] = emission;
        }

        // Voxels are stored as Vec, copy directly
        let voxels = self.voxels.clone();

        let emission = if self.properties.light_emission != 0 {
            let [r, g, b, a] = self.properties.light_emission.to_le_bytes();
            Some(Color { r, g, b, a })
        } else {
            None
        };

        let light_blocking = if self.properties.is_transparent {
            LightBlocking::Partial
        } else {
            LightBlocking::Full
        };

        // Parse light mode from u8
        let light_mode = match self.properties.light_mode {
            0 => LightMode::Steady,
            1 => LightMode::Pulse,
            2 => LightMode::Flicker,
            3 => LightMode::Candle,
            4 => LightMode::Strobe,
            5 => LightMode::Breathe,
            6 => LightMode::Sparkle,
            7 => LightMode::Wave,
            8 => LightMode::WarmUp,
            9 => LightMode::Arc,
            _ => LightMode::Steady,
        };

        SubVoxelModel {
            id: 0, // Assigned by registry
            name: self.name.clone(),
            resolution,
            voxels,
            palette,
            palette_emission,
            collision_mask: self.properties.collision_mask,
            light_blocking,
            rotatable: self.properties.rotatable,
            emission,
            requires_ground_support: self.properties.requires_ground_support,
            is_light_source: self.properties.is_light_source,
            light_mode,
            light_radius: self.properties.light_radius,
            light_intensity: self.properties.light_intensity,
            no_collision: false,
        }
    }
}

/// Helper to manage the user_models directory.
pub struct LibraryManager {
    pub root_path: PathBuf,
}

impl LibraryManager {
    pub fn new(root_path: impl Into<PathBuf>) -> Self {
        Self {
            root_path: root_path.into(),
        }
    }

    /// Ensures the library directory exists.
    pub fn init(&self) -> io::Result<()> {
        if !self.root_path.exists() {
            fs::create_dir_all(&self.root_path)?;
        }
        Ok(())
    }

    /// Saves a model to a .vxm file in the library.
    pub fn save_model(&self, model: &SubVoxelModel, author: &str) -> io::Result<()> {
        let vxm = VxmFile::from_model(model, author.to_string());

        // Sanitize filename
        let safe_name: String = model
            .name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();

        let path = self.root_path.join(format!("{}.vxm", safe_name));
        let file = File::create(path)?;
        let writer = BufWriter::new(file);

        bincode::serialize_into(writer, &vxm).map_err(io::Error::other)?;

        Ok(())
    }

    /// Loads a model from a .vxm file.
    pub fn load_model(&self, name: &str) -> io::Result<SubVoxelModel> {
        let path = self.root_path.join(format!("{}.vxm", name));
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        let vxm: VxmFile = bincode::deserialize_from(reader)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        if vxm.magic != VXM_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid magic bytes",
            ));
        }

        Ok(vxm.to_model())
    }

    /// Lists all available models in the library.
    pub fn list_models(&self) -> io::Result<Vec<String>> {
        let mut names = Vec::new();
        if self.root_path.exists() {
            for entry in fs::read_dir(&self.root_path)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "vxm") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        names.push(stem.to_string());
                    }
                }
            }
        }
        Ok(names)
    }

    /// Checks if a model with the given name exists in the library.
    pub fn model_exists(&self, name: &str) -> bool {
        // Sanitize the name the same way save_model does
        let safe_name: String = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let path = self.root_path.join(format!("{}.vxm", safe_name));
        path.exists()
    }

    /// Deletes a model from the library.
    ///
    /// # Arguments
    /// * `name` - The name of the model to delete (without .vxm extension)
    ///
    /// # Returns
    /// Ok(()) if deleted successfully, or an error if the file doesn't exist or deletion fails
    pub fn delete_model(&self, name: &str) -> io::Result<()> {
        // Sanitize filename the same way save_model does
        let safe_name: String = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();

        let path = self.root_path.join(format!("{}.vxm", safe_name));

        if !path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Model '{}' not found in library", name),
            ));
        }

        fs::remove_file(path)?;
        Ok(())
    }
}

/// Persisted model registry for a world.
/// Stores custom models (IDs >= first_custom_id) to models.dat.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorldModelStore {
    pub version: u16,
    /// The first model ID that is custom (built-ins have lower IDs).
    pub first_custom_id: u8,
    /// Custom models, stored in order. ID = first_custom_id + index.
    pub models: Vec<VxmFile>,
}

impl WorldModelStore {
    /// Creates an empty store with the given first custom ID.
    pub fn new(first_custom_id: u8) -> Self {
        Self {
            version: 1,
            first_custom_id,
            models: Vec::new(),
        }
    }

    /// Adds a model to the store. Returns the assigned ID.
    pub fn add_model(&mut self, model: &SubVoxelModel, author: &str) -> u8 {
        let id = self.first_custom_id + self.models.len() as u8;
        self.models
            .push(VxmFile::from_model(model, author.to_string()));
        id
    }

    /// Gets a model by ID. Returns None if ID is out of range.
    pub fn get_model(&self, id: u8) -> Option<SubVoxelModel> {
        if id < self.first_custom_id {
            return None;
        }
        let index = (id - self.first_custom_id) as usize;
        self.models.get(index).map(|vxm| vxm.to_model())
    }

    /// Saves the store to models.dat in the given world directory.
    pub fn save(&self, world_dir: &std::path::Path) -> io::Result<()> {
        let path = world_dir.join("models.dat");
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        bincode::serialize_into(writer, self).map_err(io::Error::other)?;
        Ok(())
    }

    /// Loads the store from models.dat in the given world directory.
    /// Returns None if the file doesn't exist.
    pub fn load(world_dir: &std::path::Path) -> io::Result<Option<Self>> {
        let path = world_dir.join("models.dat");
        if !path.exists() {
            return Ok(None);
        }
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let store: Self = bincode::deserialize_from(reader)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(Some(store))
    }

    /// Returns an iterator over all models with their IDs.
    pub fn iter(&self) -> impl Iterator<Item = (u8, SubVoxelModel)> + '_ {
        self.models.iter().enumerate().map(|(i, vxm)| {
            let id = self.first_custom_id + i as u8;
            (id, vxm.to_model())
        })
    }

    /// Returns the number of custom models.
    pub fn len(&self) -> usize {
        self.models.len()
    }

    /// Returns true if there are no custom models.
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_vxm_round_trip() {
        let mut model = SubVoxelModel::new("test_chair");
        model.set_voxel(0, 0, 0, 1);
        model.palette[1] = Color::rgb(255, 0, 0);
        model.rotatable = true;
        model.emission = Some(Color::rgb(0, 255, 0));
        model.compute_collision_mask();

        let vxm = VxmFile::from_model(&model, "tester".to_string());
        let restored = vxm.to_model();

        assert_eq!(restored.name, "test_chair");
        assert_eq!(restored.get_voxel(0, 0, 0), 1);
        assert_eq!(restored.palette[1].r, 255);
        assert!(restored.rotatable);
        assert_eq!(restored.emission.unwrap().g, 255);
        assert_eq!(restored.collision_mask, model.collision_mask);
    }

    #[test]
    fn test_library_manager() {
        let dir = tempdir().unwrap();
        let manager = LibraryManager::new(dir.path());
        manager.init().unwrap();

        let mut model = SubVoxelModel::new("my_table");
        model.set_voxel(2, 2, 2, 5);

        manager.save_model(&model, "Bob").unwrap();

        let listed = manager.list_models().unwrap();
        assert!(listed.contains(&"my_table".to_string()));

        let loaded = manager.load_model("my_table").unwrap();
        assert_eq!(loaded.name, "my_table");
        assert_eq!(loaded.get_voxel(2, 2, 2), 5);
    }

    #[test]
    fn test_world_model_store() {
        let dir = tempdir().unwrap();

        // Create store with first custom ID = 39 (after built-ins)
        let mut store = WorldModelStore::new(39);

        let mut model1 = SubVoxelModel::new("custom_chair");
        model1.set_voxel(0, 0, 0, 1);

        let mut model2 = SubVoxelModel::new("custom_lamp");
        model2.set_voxel(1, 1, 1, 2);

        let id1 = store.add_model(&model1, "Alice");
        let id2 = store.add_model(&model2, "Bob");

        assert_eq!(id1, 39);
        assert_eq!(id2, 40);
        assert_eq!(store.len(), 2);

        // Save and reload
        store.save(dir.path()).unwrap();
        let loaded = WorldModelStore::load(dir.path()).unwrap().unwrap();

        assert_eq!(loaded.first_custom_id, 39);
        assert_eq!(loaded.len(), 2);

        let m1 = loaded.get_model(39).unwrap();
        assert_eq!(m1.name, "custom_chair");
        assert_eq!(m1.get_voxel(0, 0, 0), 1);

        let m2 = loaded.get_model(40).unwrap();
        assert_eq!(m2.name, "custom_lamp");
        assert_eq!(m2.get_voxel(1, 1, 1), 2);

        // Out of range returns None
        assert!(loaded.get_model(38).is_none());
        assert!(loaded.get_model(41).is_none());
    }
}

//! Stencil system for holographic building guides.
//!
//! Stencils store only block positions (not types) and can be placed in the world
//! as persistent holographic guides for building. Multiple stencils can be active
//! simultaneously with adjustable opacity and render modes.

pub mod format;
pub mod placement;
pub mod rasterizer;
pub mod state;
pub mod ui;

// Re-exports for external use
// TODO: Remove allow once integrated with app
#[allow(unused_imports)]
pub use format::{StencilBuilder, StencilFile, StencilPosition};
#[allow(unused_imports)]
pub use placement::PlacedStencil;
#[allow(unused_imports)]
pub use state::{StencilManager, StencilRenderMode};
#[allow(unused_imports)]
pub use ui::{StencilBrowserAction, StencilUi, draw_stencil_browser};

use std::fs;
use std::io;
use std::path::PathBuf;

/// Manages the user_stencils directory for saving and loading stencils.
#[allow(dead_code)]
pub struct StencilLibrary {
    pub root_path: PathBuf,
}

#[allow(dead_code)]
impl StencilLibrary {
    /// Creates a new stencil library at the specified directory.
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

    /// Sanitizes a name for use as a filename.
    fn sanitize_name(name: &str) -> String {
        name.chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    /// Saves a stencil to a .vxs file in the library.
    pub fn save_stencil(&self, stencil: &StencilFile) -> io::Result<()> {
        let safe_name = Self::sanitize_name(&stencil.name);
        let path = self.root_path.join(format!("{}.vxs", safe_name));

        // Serialize and compress
        let bytes = stencil
            .to_bytes()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // Write to file
        fs::write(path, bytes)?;

        Ok(())
    }

    /// Loads a stencil from a .vxs file.
    pub fn load_stencil(&self, name: &str) -> io::Result<StencilFile> {
        let path = self.root_path.join(format!("{}.vxs", name));

        // Read compressed file
        let bytes = fs::read(path)?;

        // Decompress and deserialize
        StencilFile::from_bytes(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Lists all available stencils in the library.
    /// Returns a vector of stencil names (without .vxs extension).
    pub fn list_stencils(&self) -> io::Result<Vec<String>> {
        let mut names = Vec::new();

        if self.root_path.exists() {
            for entry in fs::read_dir(&self.root_path)? {
                let entry = entry?;
                let path = entry.path();

                if path.extension().is_some_and(|ext| ext == "vxs") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        names.push(stem.to_string());
                    }
                }
            }
        }

        names.sort();
        Ok(names)
    }

    /// Deletes a stencil from the library (including its thumbnail).
    pub fn delete_stencil(&self, name: &str) -> io::Result<()> {
        let path = self.root_path.join(format!("{}.vxs", name));

        if path.exists() {
            fs::remove_file(path)?;

            // Also delete thumbnail if it exists
            let thumbnail_path = self.get_thumbnail_path(name);
            if thumbnail_path.exists() {
                let _ = fs::remove_file(thumbnail_path); // Ignore errors for thumbnail
            }
        } else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Stencil '{}' not found", name),
            ));
        }

        Ok(())
    }

    /// Checks if a stencil exists in the library.
    pub fn stencil_exists(&self, name: &str) -> bool {
        let path = self.root_path.join(format!("{}.vxs", name));
        path.exists()
    }

    /// Gets information about a stencil without fully loading it.
    pub fn get_stencil_info(&self, name: &str) -> io::Result<StencilInfo> {
        let stencil = self.load_stencil(name)?;
        let thumbnail_path = self.get_thumbnail_path(name);

        Ok(StencilInfo {
            name: stencil.name.clone(),
            author: stencil.author.clone(),
            tags: stencil.tags.clone(),
            creation_date: stencil.creation_date,
            width: stencil.width,
            height: stencil.height,
            depth: stencil.depth,
            position_count: stencil.position_count(),
            volume: stencil.volume(),
            thumbnail_path: if thumbnail_path.exists() {
                Some(thumbnail_path)
            } else {
                None
            },
        })
    }

    /// Gets the path to a stencil's thumbnail (may not exist yet).
    pub fn get_thumbnail_path(&self, name: &str) -> PathBuf {
        let safe_name = Self::sanitize_name(name);
        self.root_path.join(format!("{}.png", safe_name))
    }

    /// Checks if a thumbnail exists for a stencil.
    pub fn has_thumbnail(&self, name: &str) -> bool {
        self.get_thumbnail_path(name).exists()
    }

    /// Gets the path to the stencil file.
    pub fn get_stencil_path(&self, name: &str) -> PathBuf {
        let safe_name = Self::sanitize_name(name);
        self.root_path.join(format!("{}.vxs", safe_name))
    }

    /// Regenerates the thumbnail for an existing stencil.
    pub fn regenerate_thumbnail(&self, name: &str) -> io::Result<()> {
        // Load the stencil
        let stencil = self.load_stencil(name)?;

        // Generate thumbnail
        let thumbnail_path = self.get_thumbnail_path(name);
        crate::stencils::rasterizer::generate_stencil_thumbnail(&stencil, &thumbnail_path)?;

        Ok(())
    }
}

/// Metadata about a stencil (without loading all position data).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StencilInfo {
    pub name: String,
    pub author: String,
    pub tags: Vec<String>,
    pub creation_date: u64,
    pub width: u8,
    pub height: u8,
    pub depth: u8,
    pub position_count: usize,
    pub volume: usize,
    pub thumbnail_path: Option<PathBuf>,
}

#[allow(dead_code)]
impl StencilInfo {
    /// Formats dimensions as a string (e.g., "16×32×16").
    pub fn dimensions_str(&self) -> String {
        format!("{}×{}×{}", self.width, self.height, self.depth)
    }

    /// Formats position count with thousands separator (e.g., "1,234").
    pub fn position_count_str(&self) -> String {
        format_number_with_commas(self.position_count)
    }

    /// Formats volume with thousands separator.
    pub fn volume_str(&self) -> String {
        format_number_with_commas(self.volume)
    }
}

/// Formats a number with thousands separators.
#[allow(dead_code)]
fn format_number_with_commas(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let mut count = 0;

    for c in s.chars().rev() {
        if count == 3 {
            result.push(',');
            count = 0;
        }
        result.push(c);
        count += 1;
    }

    result.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_stencil_library() {
        // Use a temporary directory for testing
        let temp_dir = env::temp_dir().join("voxel_world_stencil_test");
        let _ = fs::remove_dir_all(&temp_dir); // Clean up if exists

        let library = StencilLibrary::new(&temp_dir);
        library.init().unwrap();

        // Create and save a test stencil
        let mut stencil = StencilFile::new(
            "test_stencil".to_string(),
            "test_author".to_string(),
            10,
            10,
            10,
        );
        stencil.positions.push(StencilPosition { x: 0, y: 0, z: 0 });
        library.save_stencil(&stencil).unwrap();

        // List stencils
        let stencils = library.list_stencils().unwrap();
        assert_eq!(stencils.len(), 1);
        assert_eq!(stencils[0], "test_stencil");

        // Check existence
        assert!(library.stencil_exists("test_stencil"));
        assert!(!library.stencil_exists("nonexistent"));

        // Load stencil
        let loaded = library.load_stencil("test_stencil").unwrap();
        assert_eq!(loaded.name, "test_stencil");
        assert_eq!(loaded.author, "test_author");

        // Get info
        let info = library.get_stencil_info("test_stencil").unwrap();
        assert_eq!(info.width, 10);
        assert_eq!(info.height, 10);
        assert_eq!(info.depth, 10);
        assert_eq!(info.position_count, 1);

        // Delete stencil
        library.delete_stencil("test_stencil").unwrap();
        let stencils = library.list_stencils().unwrap();
        assert_eq!(stencils.len(), 0);

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_filename_sanitization() {
        let temp_dir = env::temp_dir().join("voxel_world_stencil_sanitize_test");
        let _ = fs::remove_dir_all(&temp_dir);

        let library = StencilLibrary::new(&temp_dir);
        library.init().unwrap();

        // Stencil name with special characters
        let mut stencil = StencilFile::new(
            "test/stencil:with*special?chars".to_string(),
            "author".to_string(),
            5,
            5,
            5,
        );
        stencil.positions.push(StencilPosition { x: 0, y: 0, z: 0 });

        library.save_stencil(&stencil).unwrap();

        // Should be saved as "test_stencil_with_special_chars.vxs"
        let stencils = library.list_stencils().unwrap();
        assert_eq!(stencils.len(), 1);
        assert_eq!(stencils[0], "test_stencil_with_special_chars");

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_format_number_with_commas() {
        assert_eq!(format_number_with_commas(0), "0");
        assert_eq!(format_number_with_commas(123), "123");
        assert_eq!(format_number_with_commas(1234), "1,234");
        assert_eq!(format_number_with_commas(1234567), "1,234,567");
    }
}

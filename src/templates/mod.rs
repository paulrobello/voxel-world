//! Template library system for saving and loading world region templates.

#![allow(dead_code)] // TODO: Remove once integrated

pub mod format;
pub mod placement;
pub mod rasterizer;
pub mod selection;
pub mod ui;

// Re-exports for external use (will be used when integrated)
#[allow(unused_imports)]
pub use format::{TemplateBuilder, VxtFile};
#[allow(unused_imports)]
pub use placement::{FrameDistributedPlacer, TemplatePlacement};
#[allow(unused_imports)]
pub use selection::TemplateSelection;
#[allow(unused_imports)]
pub use ui::{TemplateBrowserAction, TemplateUi, draw_save_template_dialog, draw_template_browser};

use std::fs;
use std::io;
use std::path::PathBuf;

/// Manages the user_templates directory for saving and loading templates.
pub struct TemplateLibrary {
    pub root_path: PathBuf,
}

impl TemplateLibrary {
    /// Creates a new template library at the specified directory.
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

    /// Saves a template to a .vxt file in the library.
    pub fn save_template(&self, template: &VxtFile) -> io::Result<()> {
        // Sanitize filename (alphanumeric, underscore, hyphen only)
        let safe_name: String = template
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

        let path = self.root_path.join(format!("{}.vxt", safe_name));

        // Serialize and compress
        let bytes = template
            .to_bytes()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // Write to file
        fs::write(path, bytes)?;

        Ok(())
    }

    /// Loads a template from a .vxt file.
    pub fn load_template(&self, name: &str) -> io::Result<VxtFile> {
        let path = self.root_path.join(format!("{}.vxt", name));

        // Read compressed file
        let bytes = fs::read(path)?;

        // Decompress and deserialize
        VxtFile::from_bytes(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Lists all available templates in the library.
    /// Returns a vector of template names (without .vxt extension).
    pub fn list_templates(&self) -> io::Result<Vec<String>> {
        let mut names = Vec::new();

        if self.root_path.exists() {
            for entry in fs::read_dir(&self.root_path)? {
                let entry = entry?;
                let path = entry.path();

                if path.extension().is_some_and(|ext| ext == "vxt") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        names.push(stem.to_string());
                    }
                }
            }
        }

        names.sort();
        Ok(names)
    }

    /// Deletes a template from the library (including its thumbnail).
    pub fn delete_template(&self, name: &str) -> io::Result<()> {
        let path = self.root_path.join(format!("{}.vxt", name));

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
                format!("Template '{}' not found", name),
            ));
        }

        Ok(())
    }

    /// Checks if a template exists in the library.
    pub fn template_exists(&self, name: &str) -> bool {
        let path = self.root_path.join(format!("{}.vxt", name));
        path.exists()
    }

    /// Gets information about a template without fully loading it.
    /// Returns (name, author, dimensions, block_count, tags).
    pub fn get_template_info(&self, name: &str) -> io::Result<TemplateInfo> {
        let template = self.load_template(name)?;
        let thumbnail_path = self.get_thumbnail_path(name);

        Ok(TemplateInfo {
            name: template.name.clone(),
            author: template.author.clone(),
            tags: template.tags.clone(),
            creation_date: template.creation_date,
            width: template.width,
            height: template.height,
            depth: template.depth,
            block_count: template.block_count(),
            volume: template.volume(),
            thumbnail_path: if thumbnail_path.exists() {
                Some(thumbnail_path)
            } else {
                None
            },
        })
    }

    /// Gets the path to a template's thumbnail (may not exist yet).
    pub fn get_thumbnail_path(&self, name: &str) -> PathBuf {
        // Sanitize filename
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

        self.root_path.join(format!("{}.png", safe_name))
    }

    /// Checks if a thumbnail exists for a template.
    pub fn has_thumbnail(&self, name: &str) -> bool {
        self.get_thumbnail_path(name).exists()
    }

    /// Gets the path to the template file.
    pub fn get_template_path(&self, name: &str) -> PathBuf {
        // Sanitize filename
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

        self.root_path.join(format!("{}.vxt", safe_name))
    }

    /// Regenerates the thumbnail for an existing template.
    pub fn regenerate_thumbnail(&self, name: &str) -> io::Result<()> {
        // Load the template
        let template = self.load_template(name)?;

        // Generate thumbnail
        let thumbnail_path = self.get_thumbnail_path(name);
        crate::templates::rasterizer::generate_template_thumbnail(&template, &thumbnail_path)?;

        Ok(())
    }
}

/// Metadata about a template (without loading all block data).
#[derive(Debug, Clone)]
pub struct TemplateInfo {
    pub name: String,
    pub author: String,
    pub tags: Vec<String>,
    pub creation_date: u64,
    pub width: u8,
    pub height: u8,
    pub depth: u8,
    pub block_count: usize,
    pub volume: usize,
    pub thumbnail_path: Option<PathBuf>,
}

impl TemplateInfo {
    /// Formats dimensions as a string (e.g., "16×32×16").
    pub fn dimensions_str(&self) -> String {
        format!("{}×{}×{}", self.width, self.height, self.depth)
    }

    /// Formats block count with thousands separator (e.g., "1,234").
    pub fn block_count_str(&self) -> String {
        format_number_with_commas(self.block_count)
    }

    /// Formats volume with thousands separator.
    pub fn volume_str(&self) -> String {
        format_number_with_commas(self.volume)
    }

    /// Formats creation date as a unix timestamp string.
    pub fn creation_date_str(&self) -> String {
        format!("{}", self.creation_date)
    }
}

/// Formats a number with thousands separators.
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
    fn test_template_library() {
        // Use a temporary directory for testing
        let temp_dir = env::temp_dir().join("voxel_world_template_test");
        let _ = fs::remove_dir_all(&temp_dir); // Clean up if exists

        let library = TemplateLibrary::new(&temp_dir);
        library.init().unwrap();

        // Create and save a test template
        let template = VxtFile::new(
            "test_template".to_string(),
            "test_author".to_string(),
            10,
            10,
            10,
        );
        library.save_template(&template).unwrap();

        // List templates
        let templates = library.list_templates().unwrap();
        assert_eq!(templates.len(), 1);
        assert_eq!(templates[0], "test_template");

        // Check existence
        assert!(library.template_exists("test_template"));
        assert!(!library.template_exists("nonexistent"));

        // Load template
        let loaded = library.load_template("test_template").unwrap();
        assert_eq!(loaded.name, "test_template");
        assert_eq!(loaded.author, "test_author");

        // Get info
        let info = library.get_template_info("test_template").unwrap();
        assert_eq!(info.width, 10);
        assert_eq!(info.height, 10);
        assert_eq!(info.depth, 10);

        // Delete template
        library.delete_template("test_template").unwrap();
        let templates = library.list_templates().unwrap();
        assert_eq!(templates.len(), 0);

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_filename_sanitization() {
        let temp_dir = env::temp_dir().join("voxel_world_template_sanitize_test");
        let _ = fs::remove_dir_all(&temp_dir);

        let library = TemplateLibrary::new(&temp_dir);
        library.init().unwrap();

        // Template name with special characters
        let template = VxtFile::new(
            "test/template:with*special?chars".to_string(),
            "author".to_string(),
            5,
            5,
            5,
        );

        library.save_template(&template).unwrap();

        // Should be saved as "test_template_with_special_chars.vxt"
        let templates = library.list_templates().unwrap();
        assert_eq!(templates.len(), 1);
        assert_eq!(templates[0], "test_template_with_special_chars");

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

    #[test]
    fn test_regenerate_thumbnail() {
        use crate::chunk::BlockType;
        use crate::templates::format::TemplateBuilder;
        use nalgebra::Vector3;

        // Use a temporary directory for testing
        let temp_dir = env::temp_dir().join("voxel_world_thumbnail_regenerate_test");
        let _ = fs::remove_dir_all(&temp_dir); // Clean up if exists

        let library = TemplateLibrary::new(&temp_dir);
        library.init().unwrap();

        // Create a template with some blocks
        let mut builder = TemplateBuilder::new(
            "test_thumb".to_string(),
            "test_author".to_string(),
            Vector3::new(0, 0, 0),
            10,
            10,
            10,
        );
        builder.add_block(Vector3::new(0, 0, 0), BlockType::Stone);
        builder.add_block(Vector3::new(1, 1, 1), BlockType::Dirt);
        builder.add_block(Vector3::new(2, 2, 2), BlockType::Grass);
        let template = builder.build();

        // Save template
        library.save_template(&template).unwrap();

        // Thumbnail should not exist yet (old templates before thumbnail feature)
        let thumb_path = library.get_thumbnail_path("test_thumb");
        if thumb_path.exists() {
            fs::remove_file(&thumb_path).unwrap();
        }
        assert!(!library.has_thumbnail("test_thumb"));

        // Regenerate thumbnail
        library.regenerate_thumbnail("test_thumb").unwrap();

        // Thumbnail should now exist
        assert!(library.has_thumbnail("test_thumb"));
        assert!(thumb_path.exists());

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }
}

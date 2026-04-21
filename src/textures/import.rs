//! Image import system for the texture generator.
//!
//! Provides functionality to load external images and resize/crop them
//! to 64x64 for use as custom textures.

use super::generator::TEXTURE_SIZE;
use image::{GenericImageView, RgbaImage, imageops::FilterType};
use std::path::PathBuf;

/// Resize mode for imported images.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResizeMode {
    /// Scale to fit within 64x64, preserving aspect ratio (may have transparent borders).
    #[default]
    Fit,
    /// Scale to fill 64x64, preserving aspect ratio (may crop edges).
    Fill,
    /// Stretch to exactly 64x64 (may distort).
    Stretch,
    /// Crop a 64x64 region from the source at the specified offset.
    Crop,
}

impl ResizeMode {
    /// Returns all available resize modes.
    pub const fn all() -> [ResizeMode; 4] {
        [
            ResizeMode::Fit,
            ResizeMode::Fill,
            ResizeMode::Stretch,
            ResizeMode::Crop,
        ]
    }

    /// Returns the display name for UI.
    pub const fn display_name(&self) -> &'static str {
        match *self {
            ResizeMode::Fit => "Fit",
            ResizeMode::Fill => "Fill",
            ResizeMode::Stretch => "Stretch",
            ResizeMode::Crop => "Crop",
        }
    }

    /// Returns a brief description.
    pub const fn description(&self) -> &'static str {
        match *self {
            ResizeMode::Fit => "Scale to fit, preserve aspect ratio",
            ResizeMode::Fill => "Scale to fill, crop excess",
            ResizeMode::Stretch => "Stretch to fill exactly",
            ResizeMode::Crop => "Crop a region from source",
        }
    }
}

/// Sample filter for image resizing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SampleFilter {
    /// Nearest neighbor (pixelated, fast).
    Nearest,
    /// Bilinear interpolation (smooth, default).
    #[default]
    Bilinear,
    /// Lanczos3 (high quality, slower).
    Lanczos,
}

impl SampleFilter {
    /// Returns all available filters.
    pub const fn all() -> [SampleFilter; 3] {
        [
            SampleFilter::Nearest,
            SampleFilter::Bilinear,
            SampleFilter::Lanczos,
        ]
    }

    /// Returns the display name for UI.
    pub const fn display_name(&self) -> &'static str {
        match *self {
            SampleFilter::Nearest => "Nearest",
            SampleFilter::Bilinear => "Bilinear",
            SampleFilter::Lanczos => "Lanczos",
        }
    }

    /// Returns a brief description.
    pub const fn description(&self) -> &'static str {
        match *self {
            SampleFilter::Nearest => "Pixelated, preserves sharp edges",
            SampleFilter::Bilinear => "Smooth, good balance",
            SampleFilter::Lanczos => "High quality, best for downscaling",
        }
    }

    /// Converts to image crate's FilterType.
    pub fn to_filter_type(self) -> FilterType {
        match self {
            SampleFilter::Nearest => FilterType::Nearest,
            SampleFilter::Bilinear => FilterType::Triangle,
            SampleFilter::Lanczos => FilterType::Lanczos3,
        }
    }
}

/// State for image import functionality.
#[derive(Debug, Clone)]
pub struct ImportState {
    /// Path to the source image file.
    pub path: PathBuf,
    /// Loaded source image.
    pub source_image: Option<RgbaImage>,
    /// Original dimensions of the source image.
    pub source_size: (u32, u32),
    /// Preview of the result (64x64 RGBA).
    pub preview_pixels: Vec<u8>,
    /// Current resize mode.
    pub resize_mode: ResizeMode,
    /// Current sample filter.
    pub sample_filter: SampleFilter,
    /// Crop offset (for Crop mode).
    pub crop_offset: (u32, u32),
    /// Error message if loading failed.
    pub error: Option<String>,
    /// Whether we're currently loading an image.
    pub loading: bool,
}

impl Default for ImportState {
    fn default() -> Self {
        Self::new()
    }
}

impl ImportState {
    /// Creates a new import state.
    pub fn new() -> Self {
        Self {
            path: PathBuf::new(),
            source_image: None,
            source_size: (0, 0),
            preview_pixels: vec![0u8; (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize],
            resize_mode: ResizeMode::default(),
            sample_filter: SampleFilter::default(),
            crop_offset: (0, 0),
            error: None,
            loading: false,
        }
    }

    /// Clears the current import state.
    pub fn clear(&mut self) {
        self.path = PathBuf::new();
        self.source_image = None;
        self.source_size = (0, 0);
        self.preview_pixels = vec![0u8; (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize];
        self.crop_offset = (0, 0);
        self.error = None;
        self.loading = false;
    }

    /// Loads an image from the given path.
    pub fn load_image(&mut self, path: PathBuf) -> bool {
        self.path = path.clone();
        self.error = None;
        self.loading = true;

        match image::open(&path) {
            Ok(img) => {
                self.source_size = img.dimensions();
                self.source_image = Some(img.to_rgba8());
                self.loading = false;
                self.crop_offset = (0, 0);
                self.update_preview();
                true
            }
            Err(e) => {
                self.error = Some(format!("Failed to load image: {}", e));
                self.source_image = None;
                self.source_size = (0, 0);
                self.loading = false;
                false
            }
        }
    }

    /// Returns true if an image is loaded.
    pub fn has_image(&self) -> bool {
        self.source_image.is_some()
    }

    /// Updates the preview based on current settings.
    pub fn update_preview(&mut self) {
        let Some(source) = &self.source_image else {
            return;
        };

        let result = self.process_image(source);

        // Copy to preview_pixels
        let raw = result.into_raw();
        if raw.len() == self.preview_pixels.len() {
            self.preview_pixels.copy_from_slice(&raw);
        }
    }

    /// Processes the source image with current settings.
    fn process_image(&self, source: &RgbaImage) -> RgbaImage {
        let (sw, sh) = source.dimensions();
        let target = TEXTURE_SIZE;

        match self.resize_mode {
            ResizeMode::Fit => {
                // Scale to fit, preserving aspect ratio
                let scale = (target as f32 / sw as f32).min(target as f32 / sh as f32);
                let new_w = (sw as f32 * scale) as u32;
                let new_h = (sh as f32 * scale) as u32;

                let resized = image::imageops::resize(
                    source,
                    new_w,
                    new_h,
                    self.sample_filter.to_filter_type(),
                );

                // Create target with transparent background
                let mut result = RgbaImage::new(target, target);

                // Center the resized image
                let offset_x = (target - new_w) / 2;
                let offset_y = (target - new_h) / 2;

                image::imageops::overlay(&mut result, &resized, offset_x.into(), offset_y.into());
                result
            }
            ResizeMode::Fill => {
                // Scale to fill, then crop center
                let scale = (target as f32 / sw as f32).max(target as f32 / sh as f32);
                let new_w = (sw as f32 * scale).ceil() as u32;
                let new_h = (sh as f32 * scale).ceil() as u32;

                let resized = image::imageops::resize(
                    source,
                    new_w,
                    new_h,
                    self.sample_filter.to_filter_type(),
                );

                // Crop center
                let offset_x = (new_w.saturating_sub(target)) / 2;
                let offset_y = (new_h.saturating_sub(target)) / 2;

                image::imageops::crop_imm(&resized, offset_x, offset_y, target, target).to_image()
            }
            ResizeMode::Stretch => {
                // Stretch to exact size
                image::imageops::resize(source, target, target, self.sample_filter.to_filter_type())
            }
            ResizeMode::Crop => {
                // Crop a region from source
                let (ox, oy) = self.crop_offset;
                let crop_x = ox.min(sw.saturating_sub(target));
                let crop_y = oy.min(sh.saturating_sub(target));
                let crop_w = target.min(sw - crop_x);
                let crop_h = target.min(sh - crop_y);

                let cropped = image::imageops::crop_imm(source, crop_x, crop_y, crop_w, crop_h);

                // If source is smaller than target, we need to pad
                if crop_w < target || crop_h < target {
                    let mut result = RgbaImage::new(target, target);
                    image::imageops::overlay(&mut result, &cropped.to_image(), 0, 0);
                    result
                } else {
                    cropped.to_image()
                }
            }
        }
    }

    /// Returns the maximum crop offset based on source image size.
    pub fn max_crop_offset(&self) -> (u32, u32) {
        let (sw, sh) = self.source_size;
        (
            sw.saturating_sub(TEXTURE_SIZE),
            sh.saturating_sub(TEXTURE_SIZE),
        )
    }

    /// Returns the processed pixel data (64x64 RGBA).
    pub fn get_result(&self) -> &[u8] {
        &self.preview_pixels
    }

    /// Returns the source image dimensions as a string.
    pub fn source_size_string(&self) -> String {
        if self.source_size.0 > 0 {
            format!("{}x{}", self.source_size.0, self.source_size.1)
        } else {
            "No image loaded".to_string()
        }
    }

    /// Returns the file name from the path.
    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or_default()
    }
}

/// Opens a native file dialog to select an image file.
///
/// Returns the selected path or None if cancelled.
pub fn open_image_dialog() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Select Image")
        .add_filter(
            "Images",
            &["png", "jpg", "jpeg", "gif", "bmp", "webp", "tiff", "tif"],
        )
        .add_filter("PNG", &["png"])
        .add_filter("JPEG", &["jpg", "jpeg"])
        .add_filter("All Files", &["*"])
        .pick_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_import_state_new() {
        let state = ImportState::new();
        assert!(!state.has_image());
        assert_eq!(
            state.preview_pixels.len(),
            (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize
        );
    }

    #[test]
    fn test_resize_modes() {
        for mode in ResizeMode::all() {
            assert!(!mode.display_name().is_empty());
            assert!(!mode.description().is_empty());
        }
    }

    #[test]
    fn test_sample_filters() {
        for filter in SampleFilter::all() {
            assert!(!filter.display_name().is_empty());
            assert!(!filter.description().is_empty());
            // Just verify conversion doesn't panic
            let _ = filter.to_filter_type();
        }
    }

    #[test]
    fn test_process_stretch() {
        // Create a simple 2x2 test image
        let mut source = RgbaImage::new(2, 2);
        source.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        source.put_pixel(1, 0, image::Rgba([0, 255, 0, 255]));
        source.put_pixel(0, 1, image::Rgba([0, 0, 255, 255]));
        source.put_pixel(1, 1, image::Rgba([255, 255, 0, 255]));

        let mut state = ImportState::new();
        state.resize_mode = ResizeMode::Stretch;
        state.sample_filter = SampleFilter::Nearest;
        state.source_image = Some(source);
        state.source_size = (2, 2);
        state.update_preview();

        // Verify result is 64x64
        assert_eq!(state.preview_pixels.len(), (64 * 64 * 4) as usize);

        // With nearest neighbor, top-left corner should still be red
        assert_eq!(state.preview_pixels[0], 255); // R
        assert_eq!(state.preview_pixels[1], 0); // G
        assert_eq!(state.preview_pixels[2], 0); // B
        assert_eq!(state.preview_pixels[3], 255); // A
    }

    #[test]
    fn test_max_crop_offset() {
        let mut state = ImportState::new();
        state.source_size = (128, 256);

        let (max_x, max_y) = state.max_crop_offset();
        assert_eq!(max_x, 128 - TEXTURE_SIZE);
        assert_eq!(max_y, 256 - TEXTURE_SIZE);
    }
}

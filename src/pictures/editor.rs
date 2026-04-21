//! Picture editor for creating and editing pictures.
//!
//! Provides drawing tools including pencil, eraser, fill, line, rectangle,
//! circle, and eyedropper. Supports undo/redo and a 32-color palette plus RGB.

use super::library::{MAX_PICTURE_SIZE, Picture};

/// Maximum undo history entries.
const MAX_UNDO_HISTORY: usize = 50;

/// Default palette colors (32 colors).
pub const DEFAULT_PALETTE: [[u8; 4]; 32] = [
    [0, 0, 0, 255],       // 0: Black
    [255, 255, 255, 255], // 1: White
    [255, 0, 0, 255],     // 2: Red
    [0, 255, 0, 255],     // 3: Green
    [0, 0, 255, 255],     // 4: Blue
    [255, 255, 0, 255],   // 5: Yellow
    [255, 0, 255, 255],   // 6: Magenta
    [0, 255, 255, 255],   // 7: Cyan
    [128, 128, 128, 255], // 8: Gray
    [192, 192, 192, 255], // 9: Light gray
    [128, 0, 0, 255],     // 10: Dark red
    [0, 128, 0, 255],     // 11: Dark green
    [0, 0, 128, 255],     // 12: Dark blue
    [128, 128, 0, 255],   // 13: Olive
    [128, 0, 128, 255],   // 14: Purple
    [0, 128, 128, 255],   // 15: Teal
    [255, 128, 0, 255],   // 16: Orange
    [255, 128, 128, 255], // 17: Light red
    [128, 255, 128, 255], // 18: Light green
    [128, 128, 255, 255], // 19: Light blue
    [255, 255, 128, 255], // 20: Light yellow
    [255, 128, 255, 255], // 21: Light magenta
    [128, 255, 255, 255], // 22: Light cyan
    [64, 64, 64, 255],    // 23: Dark gray
    [139, 69, 19, 255],   // 24: Brown
    [210, 180, 140, 255], // 25: Tan
    [255, 192, 203, 255], // 26: Pink
    [173, 216, 230, 255], // 27: Light blue
    [144, 238, 144, 255], // 28: Light green
    [255, 218, 185, 255], // 29: Peach
    [230, 230, 250, 255], // 30: Lavender
    [0, 0, 0, 0],         // 31: Transparent
];

/// Drawing tools available in the picture editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PictureEditorTool {
    /// Draw with the selected color.
    #[default]
    Pencil,
    /// Erase to transparent.
    Eraser,
    /// Flood fill connected same-color pixels.
    Fill,
    /// Pick color from canvas.
    Eyedropper,
    /// Draw straight line between two points.
    Line,
    /// Draw rectangle (outline or filled).
    Rectangle,
    /// Draw circle (outline or filled).
    Circle,
}

impl PictureEditorTool {
    /// Returns all available tools.
    pub const fn all() -> [Self; 7] {
        [
            Self::Pencil,
            Self::Eraser,
            Self::Fill,
            Self::Eyedropper,
            Self::Line,
            Self::Rectangle,
            Self::Circle,
        ]
    }

    /// Returns the display name of this tool.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Pencil => "Pencil",
            Self::Eraser => "Eraser",
            Self::Fill => "Fill",
            Self::Eyedropper => "Eyedropper",
            Self::Line => "Line",
            Self::Rectangle => "Rectangle",
            Self::Circle => "Circle",
        }
    }

    /// Returns the keyboard shortcut for this tool.
    pub const fn shortcut(&self) -> &'static str {
        match self {
            Self::Pencil => "P",
            Self::Eraser => "E",
            Self::Fill => "G",
            Self::Eyedropper => "I",
            Self::Line => "L",
            Self::Rectangle => "R",
            Self::Circle => "C",
        }
    }

    /// Returns true if this tool uses two-point input (click-drag).
    pub const fn is_two_point(&self) -> bool {
        matches!(self, Self::Line | Self::Rectangle | Self::Circle)
    }
}

/// Undo entry containing a snapshot of the picture pixels.
#[derive(Debug, Clone)]
struct UndoEntry {
    pixels: Vec<u8>,
}

/// State for the picture editor.
pub struct PictureEditor {
    /// Whether the editor is currently active.
    pub active: bool,

    /// The picture being edited.
    pub picture: Option<Picture>,

    /// Picture ID in library (if editing existing).
    pub picture_id: Option<u32>,

    /// Currently selected tool.
    pub tool: PictureEditorTool,

    /// Current drawing color.
    pub color: [u8; 4],

    /// Brush size in pixels (1-16).
    pub brush_size: u8,

    /// Whether shapes are filled (for Rectangle, Circle).
    pub fill_shape: bool,

    /// Zoom level (1.0 = 100%).
    pub zoom: f32,

    /// Pan offset in pixels.
    pub pan_offset: [f32; 2],

    /// Whether currently panning.
    pub is_panning: bool,

    /// First point for two-point tools (Line, Rectangle, Circle).
    pub first_point: Option<(i32, i32)>,

    /// Whether currently dragging.
    pub is_dragging: bool,

    /// Last mouse position during drag.
    pub last_drag_pos: Option<(i32, i32)>,

    /// Undo stack.
    undo_stack: Vec<UndoEntry>,

    /// Redo stack.
    redo_stack: Vec<UndoEntry>,

    /// Whether we're in the middle of a stroke (for batching undo).
    stroke_active: bool,

    /// Pending save after edit.
    pub needs_save: bool,

    /// Show grid overlay.
    pub show_grid: bool,

    /// Custom RGB color sliders.
    pub custom_rgb: [u8; 3],

    /// Selected palette index (0-31, or 32 for custom).
    pub selected_palette_idx: usize,

    /// Show new picture dialog.
    pub show_new_dialog: bool,

    /// New picture dimensions for dialog.
    pub new_width: u16,
    pub new_height: u16,
    pub new_name: String,

    /// Show import dialog.
    pub show_import_dialog: bool,

    /// Import source path.
    pub import_path: String,

    /// Imported image data for crop/scale.
    pub import_data: Option<ImportedImage>,

    /// Crop region for import.
    pub crop_rect: (u16, u16, u16, u16),
}

impl std::fmt::Debug for PictureEditor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PictureEditor")
            .field("active", &self.active)
            .field("picture_id", &self.picture_id)
            .field("tool", &self.tool)
            .field("color", &self.color)
            .field("brush_size", &self.brush_size)
            .field("zoom", &self.zoom)
            .finish_non_exhaustive()
    }
}

/// Imported image data before final crop/scale.
#[derive(Debug, Clone)]
pub struct ImportedImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Default for PictureEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl PictureEditor {
    /// Creates a new picture editor.
    pub fn new() -> Self {
        Self {
            active: false,
            picture: None,
            picture_id: None,
            tool: PictureEditorTool::Pencil,
            color: [0, 0, 0, 255],
            brush_size: 1,
            fill_shape: false,
            zoom: 4.0,
            pan_offset: [0.0, 0.0],
            is_panning: false,
            first_point: None,
            is_dragging: false,
            last_drag_pos: None,
            undo_stack: Vec::with_capacity(MAX_UNDO_HISTORY),
            redo_stack: Vec::with_capacity(MAX_UNDO_HISTORY / 2),
            stroke_active: false,
            needs_save: false,
            show_grid: true,
            custom_rgb: [128, 128, 128],
            selected_palette_idx: 0,
            show_new_dialog: false,
            new_width: 64,
            new_height: 64,
            new_name: String::from("New Picture"),
            show_import_dialog: false,
            import_path: String::new(),
            import_data: None,
            crop_rect: (0, 0, 256, 256),
        }
    }

    /// Opens the editor with a new blank picture.
    pub fn open_new(&mut self, name: &str, width: u16, height: u16) {
        self.picture = Some(Picture::new(name, width, height));
        self.picture_id = None;
        self.active = true;
        self.reset_view();
        self.clear_history();
        self.needs_save = false;
    }

    /// Opens the editor with an existing picture.
    pub fn open_existing(&mut self, id: u32, picture: Picture) {
        self.picture = Some(picture);
        self.picture_id = Some(id);
        self.active = true;
        self.reset_view();
        self.clear_history();
        self.needs_save = false;
    }

    /// Closes the editor.
    pub fn close(&mut self) {
        self.active = false;
        self.picture = None;
        self.picture_id = None;
        self.first_point = None;
        self.is_dragging = false;
        self.stroke_active = false;
    }

    /// Resets the view to center the picture.
    pub fn reset_view(&mut self) {
        self.zoom = 4.0;
        self.pan_offset = [0.0, 0.0];
    }

    /// Clears undo/redo history.
    fn clear_history(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// Saves current state to undo stack.
    fn save_undo(&mut self) {
        let Some(ref picture) = self.picture else {
            return;
        };

        // Clear redo stack when making new changes
        self.redo_stack.clear();

        self.undo_stack.push(UndoEntry {
            pixels: picture.pixels.clone(),
        });

        // Trim oldest entries if over limit
        if self.undo_stack.len() > MAX_UNDO_HISTORY {
            self.undo_stack.remove(0);
        }
    }

    /// Performs undo operation.
    pub fn undo(&mut self) -> bool {
        let Some(ref mut picture) = self.picture else {
            return false;
        };

        if let Some(entry) = self.undo_stack.pop() {
            // Save current state to redo
            self.redo_stack.push(UndoEntry {
                pixels: picture.pixels.clone(),
            });
            picture.pixels = entry.pixels;
            self.needs_save = true;
            true
        } else {
            false
        }
    }

    /// Performs redo operation.
    pub fn redo(&mut self) -> bool {
        let Some(ref mut picture) = self.picture else {
            return false;
        };

        if let Some(entry) = self.redo_stack.pop() {
            // Save current state to undo
            self.undo_stack.push(UndoEntry {
                pixels: picture.pixels.clone(),
            });
            picture.pixels = entry.pixels;
            self.needs_save = true;
            true
        } else {
            false
        }
    }

    /// Returns true if undo is available.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Returns true if redo is available.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Begins a drawing stroke.
    fn begin_stroke_if_needed(&mut self) {
        if !self.stroke_active {
            self.save_undo();
            self.stroke_active = true;
        }
    }

    /// Ends a drawing stroke.
    pub fn end_stroke(&mut self) {
        self.stroke_active = false;
    }

    /// Selects a color from the palette.
    pub fn select_palette_color(&mut self, idx: usize) {
        if idx < 32 {
            self.color = DEFAULT_PALETTE[idx];
            self.selected_palette_idx = idx;
        }
    }

    /// Sets a custom RGB color.
    pub fn set_custom_color(&mut self, r: u8, g: u8, b: u8) {
        self.custom_rgb = [r, g, b];
        self.color = [r, g, b, 255];
        self.selected_palette_idx = 32; // Custom
    }

    /// Applies the current tool at the given canvas coordinates.
    pub fn apply_tool(&mut self, x: i32, y: i32) {
        // Read values we need before borrowing picture
        let tool = self.tool;
        let color = self.color;
        let brush_size = self.brush_size;

        match tool {
            PictureEditorTool::Pencil => {
                self.begin_stroke_if_needed();
                if let Some(ref mut picture) = self.picture {
                    draw_brush(picture, x, y, color, brush_size);
                    self.needs_save = true;
                }
            }
            PictureEditorTool::Eraser => {
                self.begin_stroke_if_needed();
                if let Some(ref mut picture) = self.picture {
                    draw_brush(picture, x, y, [0, 0, 0, 0], brush_size);
                    self.needs_save = true;
                }
            }
            PictureEditorTool::Fill => {
                self.save_undo();
                if let Some(ref mut picture) = self.picture
                    && x >= 0
                    && x < picture.width as i32
                    && y >= 0
                    && y < picture.height as i32
                {
                    picture.flood_fill(x as u16, y as u16, color);
                    self.needs_save = true;
                }
            }
            PictureEditorTool::Eyedropper => {
                if let Some(ref picture) = self.picture
                    && let Some(pixel) = picture.get_pixel(x as u16, y as u16)
                {
                    self.color = pixel;
                    self.custom_rgb = [pixel[0], pixel[1], pixel[2]];
                    self.selected_palette_idx = 32; // Custom
                    // Switch back to pencil after picking
                    self.tool = PictureEditorTool::Pencil;
                }
            }
            PictureEditorTool::Line | PictureEditorTool::Rectangle | PictureEditorTool::Circle => {
                // Two-point tools are handled in click handlers
            }
        }
    }

    /// Handles mouse click at canvas coordinates.
    pub fn on_click(&mut self, x: i32, y: i32) {
        if self.tool.is_two_point() {
            if self.first_point.is_none() {
                // First click - store point
                self.first_point = Some((x, y));
            } else {
                // Second click - draw shape
                let (x0, y0) = self.first_point.take().unwrap();
                self.save_undo();
                self.draw_shape(x0, y0, x, y);
                self.needs_save = true;
            }
        } else {
            self.apply_tool(x, y);
        }
    }

    /// Handles mouse drag at canvas coordinates.
    pub fn on_drag(&mut self, x: i32, y: i32) {
        let tool = self.tool;
        let color = self.color;
        let brush_size = self.brush_size;

        match tool {
            PictureEditorTool::Pencil | PictureEditorTool::Eraser => {
                // Draw line from last position for smooth strokes
                if let Some((lx, ly)) = self.last_drag_pos {
                    self.begin_stroke_if_needed();

                    let draw_color = if tool == PictureEditorTool::Eraser {
                        [0, 0, 0, 0]
                    } else {
                        color
                    };

                    if let Some(ref mut picture) = self.picture {
                        // Bresenham line for smooth stroke
                        let dx = (x - lx).abs();
                        let dy = -(y - ly).abs();
                        let sx = if lx < x { 1 } else { -1 };
                        let sy = if ly < y { 1 } else { -1 };
                        let mut err = dx + dy;
                        let mut cx = lx;
                        let mut cy = ly;

                        loop {
                            draw_brush(picture, cx, cy, draw_color, brush_size);
                            if cx == x && cy == y {
                                break;
                            }
                            let e2 = 2 * err;
                            if e2 >= dy {
                                if cx == x {
                                    break;
                                }
                                err += dy;
                                cx += sx;
                            }
                            if e2 <= dx {
                                if cy == y {
                                    break;
                                }
                                err += dx;
                                cy += sy;
                            }
                        }
                        self.needs_save = true;
                    }
                }
                self.last_drag_pos = Some((x, y));
            }
            _ => {}
        }
    }

    /// Handles mouse release.
    pub fn on_release(&mut self) {
        self.end_stroke();
        self.last_drag_pos = None;
    }

    /// Draws a shape between two points.
    fn draw_shape(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        let tool = self.tool;
        let color = self.color;
        let fill_shape = self.fill_shape;

        let Some(ref mut picture) = self.picture else {
            return;
        };

        match tool {
            PictureEditorTool::Line => {
                picture.draw_line(x0, y0, x1, y1, color);
            }
            PictureEditorTool::Rectangle => {
                let min_x = x0.min(x1);
                let min_y = y0.min(y1);
                let w = (x1 - x0).abs();
                let h = (y1 - y0).abs();

                if fill_shape {
                    picture.fill_rect(
                        min_x.max(0) as u16,
                        min_y.max(0) as u16,
                        (w + 1) as u16,
                        (h + 1) as u16,
                        color,
                    );
                } else {
                    picture.draw_rect(min_x, min_y, w + 1, h + 1, color);
                }
            }
            PictureEditorTool::Circle => {
                let radius = (((x1 - x0).pow(2) + (y1 - y0).pow(2)) as f32).sqrt() as i32;
                if fill_shape {
                    picture.fill_circle(x0, y0, radius, color);
                } else {
                    picture.draw_circle(x0, y0, radius, color);
                }
            }
            _ => {}
        }
    }

    /// Cancels the current two-point operation.
    pub fn cancel_shape(&mut self) {
        self.first_point = None;
    }

    /// Converts screen coordinates to canvas coordinates.
    pub fn screen_to_canvas(
        &self,
        screen_x: f32,
        screen_y: f32,
        canvas_origin: [f32; 2],
    ) -> (i32, i32) {
        let x = ((screen_x - canvas_origin[0] - self.pan_offset[0]) / self.zoom).floor() as i32;
        let y = ((screen_y - canvas_origin[1] - self.pan_offset[1]) / self.zoom).floor() as i32;
        (x, y)
    }

    /// Zooms in.
    pub fn zoom_in(&mut self) {
        self.zoom = (self.zoom * 1.5).min(32.0);
    }

    /// Zooms out.
    pub fn zoom_out(&mut self) {
        self.zoom = (self.zoom / 1.5).max(0.5);
    }

    /// Sets zoom level directly.
    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(0.5, 32.0);
    }

    /// Handles zoom with mouse wheel.
    pub fn zoom_at(&mut self, delta: f32, screen_pos: [f32; 2], canvas_origin: [f32; 2]) {
        let old_zoom = self.zoom;
        if delta > 0.0 {
            self.zoom_in();
        } else {
            self.zoom_out();
        }

        // Adjust pan to zoom towards mouse position
        let zoom_ratio = self.zoom / old_zoom;
        let mouse_rel = [
            screen_pos[0] - canvas_origin[0] - self.pan_offset[0],
            screen_pos[1] - canvas_origin[1] - self.pan_offset[1],
        ];
        self.pan_offset[0] -= mouse_rel[0] * (zoom_ratio - 1.0);
        self.pan_offset[1] -= mouse_rel[1] * (zoom_ratio - 1.0);
    }

    /// Updates pan offset during drag.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.pan_offset[0] += dx;
        self.pan_offset[1] += dy;
    }

    /// Clears the picture to transparent.
    pub fn clear_picture(&mut self) {
        if self.picture.is_some() {
            self.save_undo();
            if let Some(ref mut picture) = self.picture {
                picture.clear();
            }
            self.needs_save = true;
        }
    }

    /// Returns the picture dimensions.
    pub fn dimensions(&self) -> Option<(u16, u16)> {
        self.picture.as_ref().map(|p| (p.width, p.height))
    }

    /// Gets the current picture's pixel data for rendering.
    pub fn get_pixels(&self) -> Option<&[u8]> {
        self.picture.as_ref().map(|p| p.pixels.as_slice())
    }

    /// Loads an image from file for import.
    pub fn load_import(&mut self, path: &str) -> Result<(), String> {
        let img = image::open(path).map_err(|e| format!("Failed to load image: {e}"))?;
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();

        self.import_data = Some(ImportedImage {
            width,
            height,
            pixels: rgba.into_raw(),
        });

        // Set crop to full image (clamped to max size)
        self.crop_rect = (
            0,
            0,
            width.min(MAX_PICTURE_SIZE as u32) as u16,
            height.min(MAX_PICTURE_SIZE as u32) as u16,
        );

        Ok(())
    }

    /// Completes the import with current crop settings.
    pub fn complete_import(&mut self, name: &str) -> Option<Picture> {
        let import = self.import_data.take()?;
        let (cx, cy, cw, ch) = self.crop_rect;

        // Validate crop region
        if cw == 0 || ch == 0 {
            return None;
        }

        // Create picture with cropped data
        let mut picture = Picture::new(name, cw, ch);

        for y in 0..ch {
            for x in 0..cw {
                let src_x = (cx + x) as u32;
                let src_y = (cy + y) as u32;

                if src_x < import.width && src_y < import.height {
                    let src_idx = (src_y * import.width + src_x) as usize * 4;
                    if src_idx + 4 <= import.pixels.len() {
                        let pixel = [
                            import.pixels[src_idx],
                            import.pixels[src_idx + 1],
                            import.pixels[src_idx + 2],
                            import.pixels[src_idx + 3],
                        ];
                        picture.set_pixel(x, y, pixel);
                    }
                }
            }
        }

        Some(picture)
    }

    /// Returns true if there's an active two-point operation preview.
    pub fn has_shape_preview(&self) -> bool {
        self.tool.is_two_point() && self.first_point.is_some()
    }

    /// Gets the first point for shape preview.
    pub fn get_first_point(&self) -> Option<(i32, i32)> {
        self.first_point
    }
}

/// Draws a brush stroke at the given position.
fn draw_brush(picture: &mut Picture, x: i32, y: i32, color: [u8; 4], brush_size: u8) {
    let half = brush_size as i32 / 2;
    for dy in -half..=(half + (brush_size as i32 % 2) - 1) {
        for dx in -half..=(half + (brush_size as i32 % 2) - 1) {
            let px = x + dx;
            let py = y + dy;
            if px >= 0 && px < picture.width as i32 && py >= 0 && py < picture.height as i32 {
                picture.set_pixel(px as u16, py as u16, color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_editor_new() {
        let editor = PictureEditor::new();
        assert!(!editor.active);
        assert_eq!(editor.tool, PictureEditorTool::Pencil);
    }

    #[test]
    fn test_editor_open_new() {
        let mut editor = PictureEditor::new();
        editor.open_new("test", 64, 64);
        assert!(editor.active);
        assert!(editor.picture.is_some());
        assert_eq!(editor.dimensions(), Some((64, 64)));
    }

    #[test]
    fn test_editor_undo_redo() {
        let mut editor = PictureEditor::new();
        editor.open_new("test", 16, 16);

        // Draw something
        editor.tool = PictureEditorTool::Fill;
        editor.color = [255, 0, 0, 255];
        editor.on_click(0, 0);

        assert!(editor.can_undo());
        assert!(!editor.can_redo());

        // Undo
        editor.undo();
        assert!(!editor.can_undo());
        assert!(editor.can_redo());

        // Redo
        editor.redo();
        assert!(editor.can_undo());
        assert!(!editor.can_redo());
    }

    #[test]
    fn test_screen_to_canvas() {
        let mut editor = PictureEditor::new();
        editor.zoom = 4.0;
        editor.pan_offset = [0.0, 0.0];

        let (x, y) = editor.screen_to_canvas(40.0, 40.0, [0.0, 0.0]);
        assert_eq!(x, 10);
        assert_eq!(y, 10);
    }

    #[test]
    fn test_tool_two_point() {
        assert!(PictureEditorTool::Line.is_two_point());
        assert!(PictureEditorTool::Rectangle.is_two_point());
        assert!(PictureEditorTool::Circle.is_two_point());
        assert!(!PictureEditorTool::Pencil.is_two_point());
    }
}

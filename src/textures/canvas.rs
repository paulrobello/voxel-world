//! Paint canvas system for the texture generator.
//!
//! Provides pixel-level painting tools, a 32-color indexed palette,
//! mirror modes for symmetric patterns, and undo/redo support.
//! Supports variable canvas sizes up to 128×128 pixels.

#![allow(dead_code)] // Public API methods may not all be used yet

use std::collections::VecDeque;

/// Maximum number of undo states to keep.
const MAX_UNDO_HISTORY: usize = 100;

/// Maximum canvas dimension (width or height).
const MAX_CANVAS_SIZE: u16 = 128;

/// Canvas dimensions for variable-size picture editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanvasSize {
    /// Canvas width in pixels (1-128).
    pub width: u16,
    /// Canvas height in pixels (1-128).
    pub height: u16,
}

impl CanvasSize {
    /// Creates a new canvas size with validation.
    ///
    /// # Panics
    /// Panics if width or height is 0 or exceeds 128.
    #[must_use]
    pub const fn new(width: u16, height: u16) -> Self {
        assert!(
            width > 0 && width <= MAX_CANVAS_SIZE,
            "Canvas width must be 1-128"
        );
        assert!(
            height > 0 && height <= MAX_CANVAS_SIZE,
            "Canvas height must be 1-128"
        );
        Self { width, height }
    }

    /// Returns the total number of pixels.
    #[must_use]
    pub const fn pixel_count(&self) -> usize {
        (self.width * self.height) as usize
    }

    /// Returns true if this is a square canvas.
    #[must_use]
    pub const fn is_square(&self) -> bool {
        self.width == self.height
    }

    /// Returns the aspect ratio (width / height).
    #[must_use]
    pub const fn aspect_ratio(&self) -> f32 {
        self.width as f32 / self.height as f32
    }

    /// Preset sizes for common use cases.
    /// Square sizes.
    pub const SQUARE_32X32: CanvasSize = CanvasSize {
        width: 32,
        height: 32,
    };
    pub const SQUARE_64X64: CanvasSize = CanvasSize {
        width: 64,
        height: 64,
    };
    pub const SQUARE_128X128: CanvasSize = CanvasSize {
        width: 128,
        height: 128,
    };

    /// Tall (portrait) sizes.
    pub const TALL_32X64: CanvasSize = CanvasSize {
        width: 32,
        height: 64,
    };
    pub const TALL_64X128: CanvasSize = CanvasSize {
        width: 64,
        height: 128,
    };

    /// Wide (landscape) sizes.
    pub const WIDE_64X32: CanvasSize = CanvasSize {
        width: 64,
        height: 32,
    };
    pub const WIDE_128X64: CanvasSize = CanvasSize {
        width: 128,
        height: 64,
    };

    /// Banner sizes.
    pub const BANNER_16X128: CanvasSize = CanvasSize {
        width: 16,
        height: 128,
    };

    /// Returns all preset sizes.
    #[must_use]
    pub const fn all_presets() -> [CanvasSize; 10] {
        [
            Self::SQUARE_32X32,
            Self::SQUARE_64X64,
            Self::SQUARE_128X128,
            Self::TALL_32X64,
            Self::TALL_64X128,
            Self::WIDE_64X32,
            Self::WIDE_128X64,
            Self::BANNER_16X128,
            CanvasSize {
                width: 32,
                height: 128,
            }, // Tall banner
            CanvasSize {
                width: 128,
                height: 32,
            }, // Wide banner
        ]
    }

    /// Returns a display name for this size.
    #[must_use]
    pub const fn display_name(&self) -> [u8; 7] {
        // Format: "WWxHH" as bytes (e.g., [b'6', b'4', b'x', b'6', b'4', 0, 0])
        // For simplicity, we'll use a runtime method instead
        // This is a placeholder - the actual implementation uses display_name()
        [0; 7]
    }

    /// Returns a human-readable size string.
    #[must_use]
    pub fn size_label(&self) -> String {
        format!("{}×{}", self.width, self.height)
    }
}

impl Default for CanvasSize {
    fn default() -> Self {
        Self::SQUARE_64X64
    }
}

/// Available paint tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum PaintTool {
    /// Single pixel drawing.
    #[default]
    Pencil = 0,
    /// Variable-size brush.
    Brush = 1,
    /// Erase to transparent.
    Eraser = 2,
    /// Flood fill connected area.
    Fill = 3,
    /// Pick color from canvas.
    Eyedropper = 4,
    /// Draw straight line.
    Line = 5,
    /// Draw rectangle.
    Rectangle = 6,
    /// Draw circle/ellipse.
    Circle = 7,
    /// Draw text.
    Text = 8,
}

impl PaintTool {
    /// Returns all available tools.
    pub const fn all() -> [PaintTool; 9] {
        [
            PaintTool::Pencil,
            PaintTool::Brush,
            PaintTool::Eraser,
            PaintTool::Fill,
            PaintTool::Eyedropper,
            PaintTool::Line,
            PaintTool::Rectangle,
            PaintTool::Circle,
            PaintTool::Text,
        ]
    }

    /// Returns the display name for UI.
    pub const fn display_name(&self) -> &'static str {
        match *self {
            PaintTool::Pencil => "Pencil",
            PaintTool::Brush => "Brush",
            PaintTool::Eraser => "Eraser",
            PaintTool::Fill => "Fill",
            PaintTool::Eyedropper => "Pick",
            PaintTool::Line => "Line",
            PaintTool::Rectangle => "Rect",
            PaintTool::Circle => "Circle",
            PaintTool::Text => "Text",
        }
    }

    /// Returns the icon character for the tool.
    pub const fn icon(&self) -> &'static str {
        match *self {
            PaintTool::Pencil => "✏",
            PaintTool::Brush => "🖌",
            PaintTool::Eraser => "🧹",
            PaintTool::Fill => "🪣",
            PaintTool::Eyedropper => "💉",
            PaintTool::Line => "╱",
            PaintTool::Rectangle => "▢",
            PaintTool::Circle => "○",
            PaintTool::Text => "T",
        }
    }

    /// Returns true if this tool uses shape preview during drag.
    pub const fn uses_preview(&self) -> bool {
        matches!(
            *self,
            PaintTool::Line | PaintTool::Rectangle | PaintTool::Circle
        )
    }
}

/// Shape rendering mode for rectangle and circle tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShapeMode {
    /// Draw filled shape.
    #[default]
    Filled,
    /// Draw outline only.
    Outline,
}

/// Manages undo/redo history for the canvas.
#[derive(Debug, Clone)]
pub struct UndoHistory {
    /// Stack of states you can undo to.
    undo_stack: VecDeque<Vec<u8>>,
    /// Stack of states you can redo to.
    redo_stack: Vec<Vec<u8>>,
}

impl Default for UndoHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl UndoHistory {
    /// Creates a new empty undo history.
    pub fn new() -> Self {
        Self {
            undo_stack: VecDeque::with_capacity(MAX_UNDO_HISTORY),
            redo_stack: Vec::with_capacity(MAX_UNDO_HISTORY / 2),
        }
    }

    /// Clears all history.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// Saves the current state before a modification.
    ///
    /// This clears the redo stack since we're branching from this point.
    pub fn save(&mut self, current_pixels: &[u8]) {
        // Clear redo stack - we're branching
        self.redo_stack.clear();

        // Add to undo stack
        self.undo_stack.push_back(current_pixels.to_vec());

        // Trim oldest states if we exceed max
        while self.undo_stack.len() > MAX_UNDO_HISTORY {
            self.undo_stack.pop_front();
        }
    }

    /// Performs an undo operation.
    ///
    /// Takes the current state and returns the previous state to restore.
    pub fn undo(&mut self, current_pixels: &[u8]) -> Option<Vec<u8>> {
        if let Some(previous) = self.undo_stack.pop_back() {
            self.redo_stack.push(current_pixels.to_vec());
            Some(previous)
        } else {
            None
        }
    }

    /// Performs a redo operation.
    ///
    /// Takes the current state and returns the next state to restore.
    pub fn redo(&mut self, current_pixels: &[u8]) -> Option<Vec<u8>> {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push_back(current_pixels.to_vec());
            Some(next)
        } else {
            None
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

    /// Returns the number of undo steps available.
    pub fn undo_count(&self) -> usize {
        self.undo_stack.len()
    }

    /// Returns the number of redo steps available.
    pub fn redo_count(&self) -> usize {
        self.redo_stack.len()
    }
}

const FONT_5X7: &[u8] = &[
    // Space (32)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // ! (33)
    0x04, 0x04, 0x04, 0x04, 0x04, 0x00, 0x04, // " (34)
    0x11, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00, // # (35)
    0x0A, 0x0A, 0x1F, 0x0A, 0x1F, 0x0A, 0x0A, // $ (36)
    0x04, 0x1F, 0x08, 0x04, 0x02, 0x1F, 0x04, // % (37)
    0x12, 0x15, 0x0A, 0x05, 0x0A, 0x15, 0x12, // & (38)
    0x06, 0x09, 0x04, 0x02, 0x11, 0x10, 0x0E, // ' (39)
    0x04, 0x04, 0x04, 0x00, 0x00, 0x00, 0x00, // ( (40)
    0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02, // ) (41)
    0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08, // * (42)
    0x04, 0x15, 0x1F, 0x15, 0x04, 0x00, 0x00, // + (43)
    0x04, 0x04, 0x1F, 0x04, 0x04, 0x00, 0x00, // , (44)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x10, // - (45)
    0x00, 0x00, 0x1F, 0x00, 0x00, 0x00, 0x00, // . (46)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, // / (47)
    0x01, 0x02, 0x04, 0x08, 0x10, 0x00, 0x00, // 0 (48)
    0x0E, 0x11, 0x13, 0x13, 0x13, 0x11, 0x0E, // 1 (49)
    0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E, // 2 (50)
    0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F, // 3 (51)
    0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E, // 4 (52)
    0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02, // 5 (53)
    0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E, // 6 (54)
    0x07, 0x08, 0x1E, 0x11, 0x11, 0x11, 0x0E, // 7 (55)
    0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10, // 8 (56)
    0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E, // 9 (57)
    0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C, // : (58)
    0x00, 0x04, 0x00, 0x00, 0x04, 0x00, 0x00, // ; (59)
    0x00, 0x04, 0x00, 0x00, 0x04, 0x08, 0x10, // < (60)
    0x01, 0x02, 0x04, 0x08, 0x04, 0x02, 0x01, // = (61)
    0x00, 0x1F, 0x00, 0x1F, 0x00, 0x00, 0x00, // > (62)
    0x10, 0x08, 0x04, 0x02, 0x04, 0x08, 0x10, // ? (63)
    0x0E, 0x11, 0x02, 0x04, 0x08, 0x00, 0x10, // @ (64)
    0x0E, 0x11, 0x19, 0x15, 0x16, 0x00, 0x0E, // A (65)
    0x04, 0x0A, 0x11, 0x1F, 0x11, 0x11, 0x11, // B (66)
    0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E, // C (67)
    0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E, // D (68)
    0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E, // E (69)
    0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F, // F (70)
    0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10, // G (71)
    0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0E, // H (72)
    0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11, // I (73)
    0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E, // J (74)
    0x02, 0x02, 0x02, 0x02, 0x11, 0x11, 0x0E, // K (75)
    0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11, // L (76)
    0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F, // M (77)
    0x11, 0x1B, 0x15, 0x11, 0x11, 0x11, 0x11, // N (78)
    0x13, 0x15, 0x19, 0x11, 0x11, 0x11, 0x11, // O (79)
    0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E, // P (80)
    0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10, // Q (81)
    0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0E, // R (82)
    0x1E, 0x11, 0x11, 0x1E, 0x12, 0x11, 0x11, // S (83)
    0x0E, 0x11, 0x10, 0x0E, 0x01, 0x11, 0x0E, // T (84)
    0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, // U (85)
    0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E, // V (86)
    0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04, // W (87)
    0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11, // X (88)
    0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11, // Y (89)
    0x11, 0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, // Z (90)
    0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F,
];

/// Returns the starting index in FONT_5X7 for a character, or None if not supported.
/// Supported characters: All printable ASCII from space (32) through Z (90)
fn get_char_index(ch: char) -> Option<usize> {
    let idx = match ch {
        ' ' => 0,
        '!' => 1,
        '"' => 2,
        '#' => 3,
        '$' => 4,
        '%' => 5,
        '&' => 6,
        '\'' => 7,
        '(' => 8,
        ')' => 9,
        '*' => 10,
        '+' => 11,
        ',' => 12,
        '-' => 13,
        '.' => 14,
        '/' => 15,
        '0'..='9' => 16 + (ch as usize) - ('0' as usize),
        ':' => 26,
        ';' => 27,
        '<' => 28,
        '=' => 29,
        '>' => 30,
        '?' => 31,
        '@' => 32,
        'A'..='Z' => 33 + (ch as usize) - ('A' as usize),
        _ => return None,
    };
    Some(idx)
}

/// Returns the 5x7 bitmap for a character as a slice of 7 bytes (one per row).
/// Each byte contains 5 bits (bit 0 = leftmost pixel, bit 4 = rightmost).
/// Returns None if the character is not supported.
fn get_char_bitmap(ch: char) -> Option<[u8; 7]> {
    let char_idx = get_char_index(ch)?;
    let start = char_idx * 7;
    let mut bitmap = [0u8; 7];
    if start + 7 <= FONT_5X7.len() {
        bitmap.copy_from_slice(&FONT_5X7[start..start + 7]);
    }
    Some(bitmap)
}

/// Default 32-color palette for texture painting.
pub const DEFAULT_PALETTE: [[u8; 4]; 32] = [
    [0, 0, 0, 0],         // 0: Transparent
    [255, 255, 255, 255], // 1: White
    [0, 0, 0, 255],       // 2: Black
    [128, 128, 128, 255], // 3: Gray
    [192, 192, 192, 255], // 4: Light Gray
    [64, 64, 64, 255],    // 5: Dark Gray
    [255, 0, 0, 255],     // 6: Red
    [0, 255, 0, 255],     // 7: Green
    [0, 0, 255, 255],     // 8: Blue
    [255, 255, 0, 255],   // 9: Yellow
    [255, 0, 255, 255],   // 10: Magenta
    [0, 255, 255, 255],   // 11: Cyan
    [255, 128, 0, 255],   // 12: Orange
    [128, 0, 255, 255],   // 13: Purple
    [255, 192, 203, 255], // 14: Pink
    [139, 69, 19, 255],   // 15: Brown
    [34, 139, 34, 255],   // 16: Forest Green
    [0, 128, 128, 255],   // 17: Teal
    [128, 0, 0, 255],     // 18: Maroon
    [0, 0, 128, 255],     // 19: Navy
    [128, 128, 0, 255],   // 20: Olive
    [136, 136, 136, 255], // 21: Stone
    [134, 96, 67, 255],   // 22: Dirt
    [156, 127, 90, 255],  // 23: Wood
    [85, 107, 47, 255],   // 24: Dark Olive
    [210, 180, 140, 255], // 25: Tan
    [244, 164, 96, 255],  // 26: Sandy Brown
    [188, 143, 143, 255], // 27: Rosy Brown
    [176, 224, 230, 255], // 28: Powder Blue
    [221, 160, 221, 255], // 29: Plum
    [245, 245, 220, 255], // 30: Beige
    [112, 128, 144, 255], // 31: Slate Gray
];

/// Canvas state for pixel painting.
#[derive(Debug, Clone)]
pub struct CanvasState {
    /// Canvas dimensions.
    pub size: CanvasSize,
    /// Pixel data (width × height × 4 bytes RGBA).
    pub pixels: Vec<u8>,
    /// 32-color indexed palette.
    pub palette: [[u8; 4]; 32],
    /// Currently selected palette index.
    pub selected_color: usize,
    /// Current painting tool.
    pub tool: PaintTool,
    /// Brush size (1-8 pixels).
    pub brush_size: u8,
    /// Shape mode (filled or outline).
    pub shape_mode: ShapeMode,
    /// Mirror vertically across horizontal axis (flips Y coordinate).
    pub mirror_x: bool,
    /// Mirror horizontally across vertical axis (flips X coordinate).
    pub mirror_y: bool,
    /// Starting position for shape drawing.
    pub shape_start: Option<(u32, u32)>,
    /// Preview pixels for shape tools (shown during drag).
    pub preview_pixels: Option<Vec<u8>>,
    /// Undo/redo history.
    pub history: UndoHistory,
    /// Zoom level (1, 2, 4, or 8).
    pub zoom: u8,
    /// Whether to show the grid overlay.
    pub show_grid: bool,
    /// Current hover position (if any).
    pub hover_pos: Option<(u32, u32)>,
    /// Whether canvas is dirty (needs GPU sync).
    pub dirty: bool,
    /// Text font size (1 = small 5x7, 2 = medium 10x14, 3 = large 15x21).
    pub text_font_size: u8,
    /// Text input buffer for text tool.
    pub text_input: String,
    /// Text cursor position (where next character will be placed).
    pub text_cursor: Option<(u32, u32)>,
    /// Whether text cursor is visible (for blinking effect).
    pub text_cursor_visible: bool,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self::new()
    }
}

impl CanvasState {
    /// Creates a new canvas state with default values.
    pub fn new() -> Self {
        Self::with_size(CanvasSize::default())
    }

    /// Creates a new canvas state with the specified dimensions.
    pub fn with_size(size: CanvasSize) -> Self {
        Self {
            size,
            pixels: vec![0u8; size.pixel_count() * 4],
            palette: DEFAULT_PALETTE,
            selected_color: 1, // White
            tool: PaintTool::default(),
            brush_size: 1,
            shape_mode: ShapeMode::default(),
            mirror_x: false,
            mirror_y: false,
            shape_start: None,
            preview_pixels: None,
            history: UndoHistory::new(),
            zoom: 2, // Default 2x zoom
            show_grid: true,
            hover_pos: None,
            dirty: false,
            text_font_size: 1,
            text_input: String::new(),
            text_cursor: None,
            text_cursor_visible: true,
        }
    }

    /// Returns the canvas width.
    #[must_use]
    pub const fn width(&self) -> u16 {
        self.size.width
    }

    /// Returns the canvas height.
    #[must_use]
    pub const fn height(&self) -> u16 {
        self.size.height
    }

    /// Returns the currently selected color as RGBA.
    pub fn selected_rgba(&self) -> [u8; 4] {
        self.palette[self.selected_color]
    }

    /// Returns the pixel color at the given position.
    pub fn get_pixel(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.size.width as u32 || y >= self.size.height as u32 {
            return [0, 0, 0, 0];
        }
        let idx = ((y * self.size.width as u32 + x) * 4) as usize;
        [
            self.pixels[idx],
            self.pixels[idx + 1],
            self.pixels[idx + 2],
            self.pixels[idx + 3],
        ]
    }

    /// Sets a pixel color at the given position.
    pub fn set_pixel(&mut self, x: u32, y: u32, rgba: [u8; 4]) {
        if x >= self.size.width as u32 || y >= self.size.height as u32 {
            return;
        }
        let idx = ((y * self.size.width as u32 + x) * 4) as usize;
        self.pixels[idx] = rgba[0];
        self.pixels[idx + 1] = rgba[1];
        self.pixels[idx + 2] = rgba[2];
        self.pixels[idx + 3] = rgba[3];
    }

    /// Saves the current state to undo history.
    pub fn save_state(&mut self) {
        self.history.save(&self.pixels);
    }

    /// Performs undo operation.
    pub fn undo(&mut self) -> bool {
        if let Some(previous) = self.history.undo(&self.pixels) {
            self.pixels = previous;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Performs redo operation.
    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.history.redo(&self.pixels) {
            self.pixels = next;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Clears the canvas to transparent.
    pub fn clear(&mut self) {
        self.save_state();
        self.pixels.fill(0);
        self.dirty = true;
    }

    /// Copies pixels from a texture's pixel data.
    ///
    /// If source size matches current canvas size, copies directly.
    /// If source is smaller, centers it. If source is larger, crops it.
    pub fn copy_from(&mut self, source_pixels: &[u8]) {
        let expected_size = self.size.pixel_count() * 4;
        if source_pixels.len() == expected_size {
            self.save_state();
            self.pixels.copy_from_slice(source_pixels);
            self.dirty = true;
        } else {
            // Size mismatch: resize the canvas to match the source
            // Calculate dimensions from source pixel count
            let source_len = source_pixels.len() / 4;
            // Try common dimensions
            let (new_width, new_height) = if source_len == 64 * 64 {
                (64, 64)
            } else if source_len == 128 * 128 {
                (128, 128)
            } else if source_len == 32 * 32 {
                (32, 32)
            } else if source_len == 64 * 128 {
                // Both 64×128 and 128×64 are 8192 pixels - default to 64×128 (tall)
                (64, 128)
            } else {
                // Try to find dimensions by factorization
                let mut w = (source_len as f32).sqrt() as u16;
                while w > 0 && source_len % w as usize != 0 {
                    w -= 1;
                }
                let h = (source_len / w as usize) as u16;
                (w, h)
            };

            self.save_state();
            self.size = CanvasSize::new(new_width, new_height);
            self.pixels = source_pixels.to_vec();
            self.dirty = true;
        }
    }

    /// Returns all positions affected by mirroring.
    fn get_mirrored_positions(&self, x: u32, y: u32) -> Vec<(u32, u32)> {
        let mut positions = vec![(x, y)];
        let max_x = self.size.width as u32 - 1;
        let max_y = self.size.height as u32 - 1;

        if self.mirror_x {
            let my = max_y - y;
            if !positions.contains(&(x, my)) {
                positions.push((x, my));
            }
        }

        if self.mirror_y {
            let mx = max_x - x;
            let len = positions.len();
            for i in 0..len {
                let (px, py) = positions[i];
                let mirrored = (max_x - px, py);
                if !positions.contains(&mirrored) {
                    positions.push(mirrored);
                }
            }
            if !positions.contains(&(mx, y)) {
                positions.push((mx, y));
            }
        }

        positions
    }

    /// Draws with the pencil tool (single pixel).
    pub fn draw_pencil(&mut self, x: u32, y: u32) {
        let rgba = self.selected_rgba();
        for (px, py) in self.get_mirrored_positions(x, y) {
            self.set_pixel(px, py, rgba);
        }
        self.dirty = true;
    }

    /// Draws with the brush tool (variable size).
    pub fn draw_brush(&mut self, cx: u32, cy: u32) {
        let rgba = self.selected_rgba();
        let radius = (self.brush_size as i32 - 1) / 2;

        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let dist_sq = dx * dx + dy * dy;
                let max_dist_sq = radius * radius + 1;
                if dist_sq <= max_dist_sq {
                    let x = (cx as i32 + dx).clamp(0, self.size.width as i32 - 1) as u32;
                    let y = (cy as i32 + dy).clamp(0, self.size.height as i32 - 1) as u32;
                    for (px, py) in self.get_mirrored_positions(x, y) {
                        self.set_pixel(px, py, rgba);
                    }
                }
            }
        }
        self.dirty = true;
    }

    /// Erases to transparent at the given position.
    pub fn erase(&mut self, cx: u32, cy: u32) {
        let radius = (self.brush_size as i32 - 1) / 2;

        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let dist_sq = dx * dx + dy * dy;
                let max_dist_sq = radius * radius + 1;
                if dist_sq <= max_dist_sq {
                    let x = (cx as i32 + dx).clamp(0, self.size.width as i32 - 1) as u32;
                    let y = (cy as i32 + dy).clamp(0, self.size.height as i32 - 1) as u32;
                    for (px, py) in self.get_mirrored_positions(x, y) {
                        self.set_pixel(px, py, [0, 0, 0, 0]);
                    }
                }
            }
        }
        self.dirty = true;
    }

    /// Performs flood fill starting at the given position.
    pub fn flood_fill(&mut self, start_x: u32, start_y: u32) {
        if start_x >= self.size.width as u32 || start_y >= self.size.height as u32 {
            return;
        }

        let target_color = self.get_pixel(start_x, start_y);
        let fill_color = self.selected_rgba();

        // Don't fill if target is the same as fill color
        if target_color == fill_color {
            return;
        }

        self.save_state();

        let mut stack = vec![(start_x, start_y)];
        let mut visited = std::collections::HashSet::new();

        while let Some((x, y)) = stack.pop() {
            if x >= self.size.width as u32 || y >= self.size.height as u32 {
                continue;
            }

            if visited.contains(&(x, y)) {
                continue;
            }

            let current = self.get_pixel(x, y);
            if current != target_color {
                continue;
            }

            visited.insert((x, y));

            // Apply fill with mirroring
            for (px, py) in self.get_mirrored_positions(x, y) {
                self.set_pixel(px, py, fill_color);
            }

            // Add neighbors
            if x > 0 {
                stack.push((x - 1, y));
            }
            if x < self.size.width as u32 - 1 {
                stack.push((x + 1, y));
            }
            if y > 0 {
                stack.push((x, y - 1));
            }
            if y < self.size.height as u32 - 1 {
                stack.push((x, y + 1));
            }
        }

        self.dirty = true;
    }

    /// Picks the color at the given position.
    pub fn eyedropper(&mut self, x: u32, y: u32) -> bool {
        if x >= self.size.width as u32 || y >= self.size.height as u32 {
            return false;
        }

        let color = self.get_pixel(x, y);

        // Find closest palette color
        let mut best_idx = 0;
        let mut best_dist = u32::MAX;

        for (idx, pal_color) in self.palette.iter().enumerate() {
            let dr = (color[0] as i32 - pal_color[0] as i32).unsigned_abs();
            let dg = (color[1] as i32 - pal_color[1] as i32).unsigned_abs();
            let db = (color[2] as i32 - pal_color[2] as i32).unsigned_abs();
            let da = (color[3] as i32 - pal_color[3] as i32).unsigned_abs();
            let dist = dr + dg + db + da;

            if dist < best_dist {
                best_dist = dist;
                best_idx = idx;
            }

            if dist == 0 {
                break;
            }
        }

        self.selected_color = best_idx;
        true
    }

    /// Draws a line using Bresenham's algorithm.
    pub fn draw_line(&mut self, x0: u32, y0: u32, x1: u32, y1: u32) {
        let rgba = self.selected_rgba();

        let dx = (x1 as i32 - x0 as i32).abs();
        let dy = -(y1 as i32 - y0 as i32).abs();
        let sx = if x0 < x1 { 1i32 } else { -1i32 };
        let sy = if y0 < y1 { 1i32 } else { -1i32 };
        let mut err = dx + dy;

        let mut x = x0 as i32;
        let mut y = y0 as i32;

        loop {
            if x >= 0 && x < self.size.width as i32 && y >= 0 && y < self.size.height as i32 {
                for (px, py) in self.get_mirrored_positions(x as u32, y as u32) {
                    self.set_pixel(px, py, rgba);
                }
            }

            if x == x1 as i32 && y == y1 as i32 {
                break;
            }

            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }

        self.dirty = true;
    }

    /// Draws a rectangle.
    pub fn draw_rectangle(&mut self, x0: u32, y0: u32, x1: u32, y1: u32) {
        let rgba = self.selected_rgba();
        let min_x = x0.min(x1);
        let max_x = x0.max(x1).min(self.size.width as u32 - 1);
        let min_y = y0.min(y1);
        let max_y = y0.max(y1).min(self.size.height as u32 - 1);

        match self.shape_mode {
            ShapeMode::Filled => {
                for y in min_y..=max_y {
                    for x in min_x..=max_x {
                        for (px, py) in self.get_mirrored_positions(x, y) {
                            self.set_pixel(px, py, rgba);
                        }
                    }
                }
            }
            ShapeMode::Outline => {
                // Top and bottom edges
                for x in min_x..=max_x {
                    for (px, py) in self.get_mirrored_positions(x, min_y) {
                        self.set_pixel(px, py, rgba);
                    }
                    for (px, py) in self.get_mirrored_positions(x, max_y) {
                        self.set_pixel(px, py, rgba);
                    }
                }
                // Left and right edges
                for y in min_y..=max_y {
                    for (px, py) in self.get_mirrored_positions(min_x, y) {
                        self.set_pixel(px, py, rgba);
                    }
                    for (px, py) in self.get_mirrored_positions(max_x, y) {
                        self.set_pixel(px, py, rgba);
                    }
                }
            }
        }

        self.dirty = true;
    }

    /// Draws a circle using midpoint algorithm.
    pub fn draw_circle(&mut self, x0: u32, y0: u32, x1: u32, y1: u32) {
        let rgba = self.selected_rgba();

        let cx = ((x0 + x1) / 2) as i32;
        let cy = ((y0 + y1) / 2) as i32;
        let rx = ((x0 as i32 - x1 as i32).abs() / 2).max(1);
        let ry = ((y0 as i32 - y1 as i32).abs() / 2).max(1);

        match self.shape_mode {
            ShapeMode::Filled => {
                // Fill ellipse using scanlines
                for dy in -ry..=ry {
                    let y = cy + dy;
                    if y < 0 || y >= self.size.height as i32 {
                        continue;
                    }

                    // Calculate x extent at this y
                    let t = dy as f32 / ry as f32;
                    let x_extent = ((1.0 - t * t).sqrt() * rx as f32) as i32;

                    for dx in -x_extent..=x_extent {
                        let x = cx + dx;
                        if x >= 0 && x < self.size.width as i32 {
                            for (px, py) in self.get_mirrored_positions(x as u32, y as u32) {
                                self.set_pixel(px, py, rgba);
                            }
                        }
                    }
                }
            }
            ShapeMode::Outline => {
                // Draw ellipse outline
                let samples = ((rx + ry) * 4).max(16);
                for i in 0..samples {
                    let theta = 2.0 * std::f32::consts::PI * (i as f32) / (samples as f32);
                    let x = cx + (rx as f32 * theta.cos()) as i32;
                    let y = cy + (ry as f32 * theta.sin()) as i32;

                    if x >= 0 && x < self.size.width as i32 && y >= 0 && y < self.size.height as i32
                    {
                        for (px, py) in self.get_mirrored_positions(x as u32, y as u32) {
                            self.set_pixel(px, py, rgba);
                        }
                    }
                }
            }
        }

        self.dirty = true;
    }

    /// Draws text at the specified position using the current color and font size.
    /// Text is drawn character by character, with spacing between characters.
    /// Returns the cursor position after the text (for continuing text).
    pub fn draw_text(&mut self, mut x: u32, mut y: u32, text: &str) -> (u32, u32) {
        let rgba = self.selected_rgba();
        let scale = self.text_font_size as u32;
        let base_char_width: u32 = 5;
        let base_char_height: u32 = 7;
        let char_width = base_char_width * scale;
        let char_height = base_char_height * scale;
        let char_spacing: u32 = scale;

        for ch in text.chars() {
            if let Some(bitmap) = get_char_bitmap(ch) {
                // Draw the scaled character bitmap
                // Font format: 7 bytes (one per row), 5 bits per byte
                // Bit 4 = leftmost pixel, Bit 0 = rightmost pixel
                for row in 0..base_char_height {
                    let row_byte = bitmap[row as usize];
                    for col in 0..base_char_width {
                        // Extract the bit for this column (0-4)
                        // col 0 (left) → bit 4, col 4 (right) → bit 0
                        let bit = (row_byte >> (4 - col)) & 1;

                        if bit != 0 {
                            // Scale the pixel: draw a scale×scale block for each font pixel
                            for sy in 0..scale {
                                for sx in 0..scale {
                                    let px = x + col * scale + sx;
                                    let py = y + row * scale + sy;

                                    // Only draw if within canvas bounds
                                    if px < self.size.width as u32 && py < self.size.height as u32 {
                                        for (mx, my) in self.get_mirrored_positions(px, py) {
                                            self.set_pixel(mx, my, rgba);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Move cursor for next character
                x += char_width + char_spacing;
            }
            // Handle newline
            if ch == '\n' {
                x = 0;
                y += char_height + 2 * scale; // Extra spacing between lines
            }
        }

        self.dirty = true;
        (x, y)
    }

    /// Draws the text cursor indicator at the current cursor position.
    /// Returns true if the cursor was drawn (within canvas bounds).
    pub fn draw_text_cursor(&mut self) -> bool {
        let Some((cx, cy)) = self.text_cursor else {
            return false;
        };

        let scale = self.text_font_size as u32;
        let cursor_width = 6 * scale; // Slightly wider than a character
        let cursor_height = 7 * scale;
        let cursor_color = [255, 255, 0, 255]; // Yellow for visibility

        // Only draw if cursor is visible
        if !self.text_cursor_visible {
            return false;
        }

        // Draw cursor outline (hollow rectangle)
        for i in 0..cursor_width {
            // Top edge
            if cx + i < self.size.width as u32 && cy < self.size.height as u32 {
                self.set_pixel(cx + i, cy, cursor_color);
            }
            // Bottom edge
            if cx + i < self.size.width as u32 && cy + cursor_height - 1 < self.size.height as u32 {
                self.set_pixel(cx + i, cy + cursor_height - 1, cursor_color);
            }
        }
        for i in 1..cursor_height - 1 {
            // Left edge
            if cx < self.size.width as u32 && cy + i < self.size.height as u32 {
                self.set_pixel(cx, cy + i, cursor_color);
            }
            // Right edge
            if cx + cursor_width - 1 < self.size.width as u32 && cy + i < self.size.height as u32 {
                self.set_pixel(cx + cursor_width - 1, cy + i, cursor_color);
            }
        }

        self.dirty = true;
        true
    }

    /// Toggles the text cursor visibility (for blinking effect).
    pub fn toggle_text_cursor(&mut self) {
        self.text_cursor_visible = !self.text_cursor_visible;
    }

    /// Generates a preview of a shape tool operation.
    pub fn generate_preview(&self, x0: u32, y0: u32, x1: u32, y1: u32) -> Vec<u8> {
        // Clone current pixels and draw the shape on them
        let mut preview = self.pixels.clone();
        let rgba = self.selected_rgba();

        // Helper to set pixel in preview
        let set_preview_pixel = |preview: &mut Vec<u8>, x: u32, y: u32, rgba: [u8; 4]| {
            if x < self.size.width as u32 && y < self.size.height as u32 {
                let idx = ((y * self.size.width as u32 + x) * 4) as usize;
                preview[idx] = rgba[0];
                preview[idx + 1] = rgba[1];
                preview[idx + 2] = rgba[2];
                preview[idx + 3] = rgba[3];
            }
        };

        match self.tool {
            PaintTool::Line => {
                // Bresenham line
                let dx = (x1 as i32 - x0 as i32).abs();
                let dy = -(y1 as i32 - y0 as i32).abs();
                let sx = if x0 < x1 { 1i32 } else { -1i32 };
                let sy = if y0 < y1 { 1i32 } else { -1i32 };
                let mut err = dx + dy;
                let mut x = x0 as i32;
                let mut y = y0 as i32;

                loop {
                    if x >= 0 && x < self.size.width as i32 && y >= 0 && y < self.size.height as i32
                    {
                        set_preview_pixel(&mut preview, x as u32, y as u32, rgba);
                        // Handle mirroring
                        if self.mirror_x {
                            set_preview_pixel(
                                &mut preview,
                                self.size.width as u32 - 1 - x as u32,
                                y as u32,
                                rgba,
                            );
                        }
                        if self.mirror_y {
                            set_preview_pixel(
                                &mut preview,
                                x as u32,
                                self.size.height as u32 - 1 - y as u32,
                                rgba,
                            );
                        }
                        if self.mirror_x && self.mirror_y {
                            set_preview_pixel(
                                &mut preview,
                                self.size.width as u32 - 1 - x as u32,
                                self.size.height as u32 - 1 - y as u32,
                                rgba,
                            );
                        }
                    }

                    if x == x1 as i32 && y == y1 as i32 {
                        break;
                    }

                    let e2 = 2 * err;
                    if e2 >= dy {
                        err += dy;
                        x += sx;
                    }
                    if e2 <= dx {
                        err += dx;
                        y += sy;
                    }
                }
            }
            PaintTool::Rectangle => {
                let min_x = x0.min(x1);
                let max_x = x0.max(x1).min(self.size.width as u32 - 1);
                let min_y = y0.min(y1);
                let max_y = y0.max(y1).min(self.size.height as u32 - 1);

                match self.shape_mode {
                    ShapeMode::Filled => {
                        for y in min_y..=max_y {
                            for x in min_x..=max_x {
                                set_preview_pixel(&mut preview, x, y, rgba);
                            }
                        }
                    }
                    ShapeMode::Outline => {
                        for x in min_x..=max_x {
                            set_preview_pixel(&mut preview, x, min_y, rgba);
                            set_preview_pixel(&mut preview, x, max_y, rgba);
                        }
                        for y in min_y..=max_y {
                            set_preview_pixel(&mut preview, min_x, y, rgba);
                            set_preview_pixel(&mut preview, max_x, y, rgba);
                        }
                    }
                }
            }
            PaintTool::Circle => {
                let cx = ((x0 + x1) / 2) as i32;
                let cy = ((y0 + y1) / 2) as i32;
                let rx = ((x0 as i32 - x1 as i32).abs() / 2).max(1);
                let ry = ((y0 as i32 - y1 as i32).abs() / 2).max(1);

                match self.shape_mode {
                    ShapeMode::Filled => {
                        for dy in -ry..=ry {
                            let y = cy + dy;
                            if y < 0 || y >= self.size.height as i32 {
                                continue;
                            }
                            let t = dy as f32 / ry as f32;
                            let x_extent = ((1.0 - t * t).sqrt() * rx as f32) as i32;
                            for dx in -x_extent..=x_extent {
                                let x = cx + dx;
                                if x >= 0 && x < self.size.width as i32 {
                                    set_preview_pixel(&mut preview, x as u32, y as u32, rgba);
                                }
                            }
                        }
                    }
                    ShapeMode::Outline => {
                        let samples = ((rx + ry) * 4).max(16);
                        for i in 0..samples {
                            let theta = 2.0 * std::f32::consts::PI * (i as f32) / (samples as f32);
                            let x = cx + (rx as f32 * theta.cos()) as i32;
                            let y = cy + (ry as f32 * theta.sin()) as i32;
                            if x >= 0
                                && x < self.size.width as i32
                                && y >= 0
                                && y < self.size.height as i32
                            {
                                set_preview_pixel(&mut preview, x as u32, y as u32, rgba);
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        preview
    }

    /// Generates a preview with text rendered at the cursor position.
    pub fn generate_text_preview(&self) -> Option<Vec<u8>> {
        let (cx, cy) = self.text_cursor?;
        if self.text_input.is_empty() {
            return None;
        }

        let mut preview = self.pixels.clone();
        let rgba = self.selected_rgba();
        let scale = self.text_font_size as u32;
        let base_char_width: u32 = 5;
        let base_char_height: u32 = 7;
        let char_width = base_char_width * scale;
        let char_spacing: u32 = scale;

        let mut x = cx;
        let y = cy;

        for ch in self.text_input.chars() {
            if let Some(bitmap) = get_char_bitmap(ch) {
                // Draw the scaled character bitmap to preview
                for row in 0..base_char_height {
                    let row_byte = bitmap[row as usize];
                    for col in 0..base_char_width {
                        let bit = (row_byte >> (4 - col)) & 1;
                        if bit != 0 {
                            for sy in 0..scale {
                                for sx in 0..scale {
                                    let px = x + col * scale + sx;
                                    let py = y + row * scale + sy;
                                    if px < self.size.width as u32 && py < self.size.height as u32 {
                                        let idx = ((py * self.size.width as u32 + px) * 4) as usize;
                                        preview[idx] = rgba[0];
                                        preview[idx + 1] = rgba[1];
                                        preview[idx + 2] = rgba[2];
                                        preview[idx + 3] = rgba[3];
                                    }
                                }
                            }
                        }
                    }
                }

                x += char_width + char_spacing;
            }
        }

        Some(preview)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canvas_new() {
        let canvas = CanvasState::new();
        assert_eq!(canvas.pixels.len(), canvas.size.pixel_count() * 4);
        assert_eq!(canvas.selected_color, 1);
        assert_eq!(canvas.tool, PaintTool::Pencil);
    }

    #[test]
    fn test_set_get_pixel() {
        let mut canvas = CanvasState::new();
        let color = [255, 128, 64, 255];
        canvas.set_pixel(10, 20, color);
        assert_eq!(canvas.get_pixel(10, 20), color);
    }

    #[test]
    fn test_undo_redo() {
        let mut canvas = CanvasState::new();

        // Draw something
        canvas.save_state();
        canvas.set_pixel(5, 5, [255, 0, 0, 255]);
        assert_eq!(canvas.get_pixel(5, 5), [255, 0, 0, 255]);

        // Undo
        assert!(canvas.undo());
        assert_eq!(canvas.get_pixel(5, 5), [0, 0, 0, 0]);

        // Redo
        assert!(canvas.redo());
        assert_eq!(canvas.get_pixel(5, 5), [255, 0, 0, 255]);
    }

    #[test]
    fn test_mirror_positions() {
        let mut canvas = CanvasState::new();
        canvas.mirror_x = true;
        canvas.mirror_y = true;

        let positions = canvas.get_mirrored_positions(10, 10);
        assert_eq!(positions.len(), 4);
        assert!(positions.contains(&(10, 10)));
        assert!(positions.contains(&(canvas.size.width as u32 - 1 - 10, 10)));
        assert!(positions.contains(&(10, canvas.size.height as u32 - 1 - 10)));
        assert!(positions.contains(&(
            canvas.size.width as u32 - 1 - 10,
            canvas.size.height as u32 - 1 - 10
        )));
    }

    #[test]
    fn test_flood_fill() {
        let mut canvas = CanvasState::new();

        // Fill the top-left corner with white
        canvas.selected_color = 1; // White
        canvas.flood_fill(0, 0);

        // The entire canvas should be white (since it was all transparent)
        assert_eq!(canvas.get_pixel(0, 0), [255, 255, 255, 255]);
        assert_eq!(canvas.get_pixel(32, 32), [255, 255, 255, 255]);
    }

    #[test]
    fn test_eyedropper() {
        let mut canvas = CanvasState::new();
        canvas.set_pixel(5, 5, [255, 0, 0, 255]); // Red

        canvas.eyedropper(5, 5);
        assert_eq!(canvas.selected_color, 6); // Red is at index 6
    }
}

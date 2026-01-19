//! Paint canvas system for the texture generator.
//!
//! Provides pixel-level painting tools, a 32-color indexed palette,
//! mirror modes for symmetric patterns, and undo/redo support.

#![allow(dead_code)] // Public API methods may not all be used yet

use super::generator::TEXTURE_SIZE;
use std::collections::VecDeque;

/// Maximum number of undo states to keep.
const MAX_UNDO_HISTORY: usize = 100;

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
}

impl PaintTool {
    /// Returns all available tools.
    pub const fn all() -> [PaintTool; 8] {
        [
            PaintTool::Pencil,
            PaintTool::Brush,
            PaintTool::Eraser,
            PaintTool::Fill,
            PaintTool::Eyedropper,
            PaintTool::Line,
            PaintTool::Rectangle,
            PaintTool::Circle,
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
    /// Pixel data (TEXTURE_SIZE × TEXTURE_SIZE × 4 bytes RGBA).
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
    /// Mirror across X axis.
    pub mirror_x: bool,
    /// Mirror across Y axis.
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
}

impl Default for CanvasState {
    fn default() -> Self {
        Self::new()
    }
}

impl CanvasState {
    /// Creates a new canvas state with default values.
    pub fn new() -> Self {
        Self {
            pixels: vec![0u8; (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize],
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
        }
    }

    /// Returns the currently selected color as RGBA.
    pub fn selected_rgba(&self) -> [u8; 4] {
        self.palette[self.selected_color]
    }

    /// Returns the pixel color at the given position.
    pub fn get_pixel(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= TEXTURE_SIZE || y >= TEXTURE_SIZE {
            return [0, 0, 0, 0];
        }
        let idx = ((y * TEXTURE_SIZE + x) * 4) as usize;
        [
            self.pixels[idx],
            self.pixels[idx + 1],
            self.pixels[idx + 2],
            self.pixels[idx + 3],
        ]
    }

    /// Sets a pixel color at the given position.
    pub fn set_pixel(&mut self, x: u32, y: u32, rgba: [u8; 4]) {
        if x >= TEXTURE_SIZE || y >= TEXTURE_SIZE {
            return;
        }
        let idx = ((y * TEXTURE_SIZE + x) * 4) as usize;
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
    pub fn copy_from(&mut self, source_pixels: &[u8]) {
        let expected_size = (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize;
        if source_pixels.len() == expected_size {
            self.save_state();
            self.pixels.copy_from_slice(source_pixels);
            self.dirty = true;
        }
    }

    /// Returns all positions affected by mirroring.
    fn get_mirrored_positions(&self, x: u32, y: u32) -> Vec<(u32, u32)> {
        let mut positions = vec![(x, y)];
        let max = TEXTURE_SIZE - 1;

        if self.mirror_x {
            let mx = max - x;
            if !positions.contains(&(mx, y)) {
                positions.push((mx, y));
            }
        }

        if self.mirror_y {
            let my = max - y;
            let len = positions.len();
            for i in 0..len {
                let (px, py) = positions[i];
                let mirrored = (px, max - py);
                if !positions.contains(&mirrored) {
                    positions.push(mirrored);
                }
            }
            if !positions.contains(&(x, my)) {
                positions.push((x, my));
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
                    let x = (cx as i32 + dx).clamp(0, TEXTURE_SIZE as i32 - 1) as u32;
                    let y = (cy as i32 + dy).clamp(0, TEXTURE_SIZE as i32 - 1) as u32;
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
                    let x = (cx as i32 + dx).clamp(0, TEXTURE_SIZE as i32 - 1) as u32;
                    let y = (cy as i32 + dy).clamp(0, TEXTURE_SIZE as i32 - 1) as u32;
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
        if start_x >= TEXTURE_SIZE || start_y >= TEXTURE_SIZE {
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
            if x >= TEXTURE_SIZE || y >= TEXTURE_SIZE {
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
            if x < TEXTURE_SIZE - 1 {
                stack.push((x + 1, y));
            }
            if y > 0 {
                stack.push((x, y - 1));
            }
            if y < TEXTURE_SIZE - 1 {
                stack.push((x, y + 1));
            }
        }

        self.dirty = true;
    }

    /// Picks the color at the given position.
    pub fn eyedropper(&mut self, x: u32, y: u32) -> bool {
        if x >= TEXTURE_SIZE || y >= TEXTURE_SIZE {
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
            if x >= 0 && x < TEXTURE_SIZE as i32 && y >= 0 && y < TEXTURE_SIZE as i32 {
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
        let max_x = x0.max(x1).min(TEXTURE_SIZE - 1);
        let min_y = y0.min(y1);
        let max_y = y0.max(y1).min(TEXTURE_SIZE - 1);

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
                    if y < 0 || y >= TEXTURE_SIZE as i32 {
                        continue;
                    }

                    // Calculate x extent at this y
                    let t = dy as f32 / ry as f32;
                    let x_extent = ((1.0 - t * t).sqrt() * rx as f32) as i32;

                    for dx in -x_extent..=x_extent {
                        let x = cx + dx;
                        if x >= 0 && x < TEXTURE_SIZE as i32 {
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

                    if x >= 0 && x < TEXTURE_SIZE as i32 && y >= 0 && y < TEXTURE_SIZE as i32 {
                        for (px, py) in self.get_mirrored_positions(x as u32, y as u32) {
                            self.set_pixel(px, py, rgba);
                        }
                    }
                }
            }
        }

        self.dirty = true;
    }

    /// Generates a preview of a shape tool operation.
    pub fn generate_preview(&self, x0: u32, y0: u32, x1: u32, y1: u32) -> Vec<u8> {
        // Clone current pixels and draw the shape on them
        let mut preview = self.pixels.clone();
        let rgba = self.selected_rgba();

        // Helper to set pixel in preview
        let set_preview_pixel = |preview: &mut Vec<u8>, x: u32, y: u32, rgba: [u8; 4]| {
            if x < TEXTURE_SIZE && y < TEXTURE_SIZE {
                let idx = ((y * TEXTURE_SIZE + x) * 4) as usize;
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
                    if x >= 0 && x < TEXTURE_SIZE as i32 && y >= 0 && y < TEXTURE_SIZE as i32 {
                        set_preview_pixel(&mut preview, x as u32, y as u32, rgba);
                        // Handle mirroring
                        if self.mirror_x {
                            set_preview_pixel(
                                &mut preview,
                                TEXTURE_SIZE - 1 - x as u32,
                                y as u32,
                                rgba,
                            );
                        }
                        if self.mirror_y {
                            set_preview_pixel(
                                &mut preview,
                                x as u32,
                                TEXTURE_SIZE - 1 - y as u32,
                                rgba,
                            );
                        }
                        if self.mirror_x && self.mirror_y {
                            set_preview_pixel(
                                &mut preview,
                                TEXTURE_SIZE - 1 - x as u32,
                                TEXTURE_SIZE - 1 - y as u32,
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
                let max_x = x0.max(x1).min(TEXTURE_SIZE - 1);
                let min_y = y0.min(y1);
                let max_y = y0.max(y1).min(TEXTURE_SIZE - 1);

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
                            if y < 0 || y >= TEXTURE_SIZE as i32 {
                                continue;
                            }
                            let t = dy as f32 / ry as f32;
                            let x_extent = ((1.0 - t * t).sqrt() * rx as f32) as i32;
                            for dx in -x_extent..=x_extent {
                                let x = cx + dx;
                                if x >= 0 && x < TEXTURE_SIZE as i32 {
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
                                && x < TEXTURE_SIZE as i32
                                && y >= 0
                                && y < TEXTURE_SIZE as i32
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canvas_new() {
        let canvas = CanvasState::new();
        assert_eq!(
            canvas.pixels.len(),
            (TEXTURE_SIZE * TEXTURE_SIZE * 4) as usize
        );
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
        assert!(positions.contains(&(TEXTURE_SIZE - 1 - 10, 10)));
        assert!(positions.contains(&(10, TEXTURE_SIZE - 1 - 10)));
        assert!(positions.contains(&(TEXTURE_SIZE - 1 - 10, TEXTURE_SIZE - 1 - 10)));
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

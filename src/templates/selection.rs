/// Two-corner region selection for templates.
use nalgebra::Vector3;

/// Manages selection of a rectangular region for template creation.
#[derive(Debug, Clone, Default)]
pub struct TemplateSelection {
    /// First corner position (world coordinates).
    pub pos1: Option<Vector3<i32>>,
    /// Second corner position (world coordinates).
    pub pos2: Option<Vector3<i32>>,
    /// Whether visual selection mode is active.
    pub visual_mode: bool,
}

impl TemplateSelection {
    /// Creates a new empty selection.
    pub fn new() -> Self {
        Self {
            pos1: None,
            pos2: None,
            visual_mode: false,
        }
    }

    /// Sets the first corner.
    pub fn set_pos1(&mut self, pos: Vector3<i32>) {
        self.pos1 = Some(pos);
    }

    /// Sets the second corner.
    pub fn set_pos2(&mut self, pos: Vector3<i32>) {
        self.pos2 = Some(pos);
    }

    /// Clears the selection.
    pub fn clear(&mut self) {
        self.pos1 = None;
        self.pos2 = None;
        self.visual_mode = false;
    }

    /// Checks if a complete selection exists (both corners set).
    pub fn is_complete(&self) -> bool {
        self.pos1.is_some() && self.pos2.is_some()
    }

    /// Returns the normalized bounding box (min, max) of the selection.
    /// Both corners must be set, otherwise returns None.
    pub fn bounds(&self) -> Option<(Vector3<i32>, Vector3<i32>)> {
        let pos1 = self.pos1?;
        let pos2 = self.pos2?;

        let min = Vector3::new(pos1.x.min(pos2.x), pos1.y.min(pos2.y), pos1.z.min(pos2.z));

        let max = Vector3::new(pos1.x.max(pos2.x), pos1.y.max(pos2.y), pos1.z.max(pos2.z));

        Some((min, max))
    }

    /// Returns the dimensions (width, height, depth) of the selection.
    /// Dimensions are inclusive (max - min + 1).
    pub fn dimensions(&self) -> Option<(i32, i32, i32)> {
        let (min, max) = self.bounds()?;
        let width = max.x - min.x + 1;
        let height = max.y - min.y + 1;
        let depth = max.z - min.z + 1;
        Some((width, height, depth))
    }

    /// Returns the total volume of the selection in blocks.
    pub fn volume(&self) -> Option<u64> {
        let (width, height, depth) = self.dimensions()?;
        Some(width as u64 * height as u64 * depth as u64)
    }

    /// Validates the selection size against template limits.
    /// Returns Ok if valid, or an error message if invalid.
    pub fn validate_size(&self) -> Result<(u8, u8, u8), String> {
        let (width, height, depth) = self
            .dimensions()
            .ok_or_else(|| "Selection incomplete".to_string())?;

        if width <= 0 || height <= 0 || depth <= 0 {
            return Err("Selection dimensions must be positive".to_string());
        }

        if width > 128 || height > 128 || depth > 128 {
            return Err(format!(
                "Selection too large ({}×{}×{}). Maximum is 128×128×128",
                width, height, depth
            ));
        }

        Ok((width as u8, height as u8, depth as u8))
    }

    /// Checks if a position is within the current selection bounds.
    pub fn contains(&self, pos: Vector3<i32>) -> bool {
        if let Some((min, max)) = self.bounds() {
            pos.x >= min.x
                && pos.x <= max.x
                && pos.y >= min.y
                && pos.y <= max.y
                && pos.z >= min.z
                && pos.z <= max.z
        } else {
            false
        }
    }

    /// Returns an iterator over all block positions in the selection.
    pub fn iter_positions(&self) -> Option<SelectionIterator> {
        let (min, max) = self.bounds()?;
        Some(SelectionIterator {
            min,
            max,
            current: min,
        })
    }

    /// Returns a formatted string describing the selection.
    pub fn format_info(&self) -> String {
        if let Some((min, max)) = self.bounds() {
            let (w, h, d) = self.dimensions().unwrap();
            let vol = self.volume().unwrap();
            format!(
                "From: ({}, {}, {}) To: ({}, {}, {})\nSize: {}×{}×{} ({} blocks)",
                min.x, min.y, min.z, max.x, max.y, max.z, w, h, d, vol
            )
        } else if let Some(pos1) = self.pos1 {
            format!("Pos1: ({}, {}, {})\nPos2: Not set", pos1.x, pos1.y, pos1.z)
        } else {
            "No selection".to_string()
        }
    }
}

/// Iterator over all block positions in a selection.
pub struct SelectionIterator {
    min: Vector3<i32>,
    max: Vector3<i32>,
    current: Vector3<i32>,
}

impl Iterator for SelectionIterator {
    type Item = Vector3<i32>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current.y > self.max.y {
            return None;
        }

        let result = self.current;

        // Advance to next position (x, then z, then y)
        self.current.x += 1;
        if self.current.x > self.max.x {
            self.current.x = self.min.x;
            self.current.z += 1;
            if self.current.z > self.max.z {
                self.current.z = self.min.z;
                self.current.y += 1;
            }
        }

        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection_bounds() {
        let mut sel = TemplateSelection::new();
        assert!(sel.bounds().is_none());

        sel.set_pos1(Vector3::new(100, 64, 200));
        assert!(sel.bounds().is_none()); // Still incomplete

        sel.set_pos2(Vector3::new(105, 70, 195));
        let (min, max) = sel.bounds().unwrap();

        assert_eq!(min, Vector3::new(100, 64, 195));
        assert_eq!(max, Vector3::new(105, 70, 200));
    }

    #[test]
    fn test_selection_dimensions() {
        let mut sel = TemplateSelection::new();
        sel.set_pos1(Vector3::new(0, 0, 0));
        sel.set_pos2(Vector3::new(9, 9, 9));

        let (w, h, d) = sel.dimensions().unwrap();
        assert_eq!((w, h, d), (10, 10, 10));

        let vol = sel.volume().unwrap();
        assert_eq!(vol, 1000);
    }

    #[test]
    fn test_selection_validation() {
        let mut sel = TemplateSelection::new();

        // Incomplete selection
        assert!(sel.validate_size().is_err());

        // Valid selection
        sel.set_pos1(Vector3::new(0, 0, 0));
        sel.set_pos2(Vector3::new(10, 10, 10));
        assert!(sel.validate_size().is_ok());

        // Oversized selection
        sel.set_pos2(Vector3::new(128, 10, 10));
        assert!(sel.validate_size().is_err());

        // Maximum valid size
        sel.set_pos2(Vector3::new(127, 127, 127));
        let (w, h, d) = sel.validate_size().unwrap();
        assert_eq!((w, h, d), (128, 128, 128));
    }

    #[test]
    fn test_selection_contains() {
        let mut sel = TemplateSelection::new();
        sel.set_pos1(Vector3::new(0, 0, 0));
        sel.set_pos2(Vector3::new(10, 10, 10));

        assert!(sel.contains(Vector3::new(5, 5, 5)));
        assert!(sel.contains(Vector3::new(0, 0, 0))); // Min inclusive
        assert!(sel.contains(Vector3::new(10, 10, 10))); // Max inclusive
        assert!(!sel.contains(Vector3::new(11, 5, 5)));
        assert!(!sel.contains(Vector3::new(-1, 5, 5)));
    }

    #[test]
    fn test_selection_iterator() {
        let mut sel = TemplateSelection::new();
        sel.set_pos1(Vector3::new(0, 0, 0));
        sel.set_pos2(Vector3::new(1, 1, 1));

        let positions: Vec<_> = sel.iter_positions().unwrap().collect();
        assert_eq!(positions.len(), 8); // 2×2×2

        // Verify order: x, then z, then y
        assert_eq!(positions[0], Vector3::new(0, 0, 0));
        assert_eq!(positions[1], Vector3::new(1, 0, 0));
        assert_eq!(positions[2], Vector3::new(0, 0, 1));
        assert_eq!(positions[3], Vector3::new(1, 0, 1));
        assert_eq!(positions[4], Vector3::new(0, 1, 0));
    }

    #[test]
    fn test_selection_clear() {
        let mut sel = TemplateSelection::new();
        sel.set_pos1(Vector3::new(0, 0, 0));
        sel.set_pos2(Vector3::new(10, 10, 10));

        assert!(sel.is_complete());

        sel.clear();
        assert!(!sel.is_complete());
        assert!(sel.pos1.is_none());
        assert!(sel.pos2.is_none());
    }
}

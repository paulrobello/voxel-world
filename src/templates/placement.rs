/// Template placement system with rotation and frame-distributed placement.
use super::format::VxtFile;
use nalgebra::Vector3;

/// Manages template placement with rotation and preview mode.
pub struct TemplatePlacement {
    /// The template being placed.
    pub template: VxtFile,
    /// Anchor position in world coordinates (typically the min corner).
    pub position: Vector3<i32>,
    /// Rotation (0-3 for 0°/90°/180°/270° around Y-axis).
    pub rotation: u8,
    /// Whether in preview mode (ghost blocks) or actively placing.
    pub preview_mode: bool,
    /// Placement progress for frame-distributed placement.
    pub placement_progress: usize,
    /// Total blocks to place (non-air).
    pub total_blocks: usize,
}

impl TemplatePlacement {
    /// Creates a new template placement at the specified position.
    pub fn new(template: VxtFile, position: Vector3<i32>) -> Self {
        let total_blocks = template.block_count();
        Self {
            template,
            position,
            rotation: 0,
            preview_mode: true,
            placement_progress: 0,
            total_blocks,
        }
    }

    /// Rotates the template 90° clockwise around Y-axis.
    pub fn rotate_90(&mut self) {
        self.rotation = (self.rotation + 1) % 4;
    }

    /// Rotates the template 90° counter-clockwise around Y-axis.
    pub fn rotate_90_ccw(&mut self) {
        self.rotation = if self.rotation == 0 {
            3
        } else {
            self.rotation - 1
        };
    }

    /// Sets a specific rotation (0-3).
    pub fn set_rotation(&mut self, rotation: u8) {
        self.rotation = rotation % 4;
    }

    /// Checks if placement is complete.
    pub fn is_complete(&self) -> bool {
        self.placement_progress >= self.total_blocks
    }

    /// Calculates completion percentage (0-100).
    pub fn completion_percentage(&self) -> u32 {
        if self.total_blocks == 0 {
            return 100;
        }
        ((self.placement_progress as f32 / self.total_blocks as f32) * 100.0) as u32
    }

    /// Applies rotation transformation to template-relative coordinates.
    /// Returns world-relative offset from anchor position.
    pub fn apply_rotation(&self, x: u8, y: u8, z: u8) -> Vector3<i32> {
        let w = self.template.width as i32;
        let d = self.template.depth as i32;

        // Center of rotation (template center)
        let cx = w / 2;
        let cz = d / 2;

        // Position relative to center
        let rx = x as i32 - cx;
        let rz = z as i32 - cz;

        // Apply Y-axis rotation
        let (tx, tz) = match self.rotation {
            0 => (rx, rz),   // 0°
            1 => (-rz, rx),  // 90°
            2 => (-rx, -rz), // 180°
            3 => (rz, -rx),  // 270°
            _ => (rx, rz),   // Invalid, default to 0°
        };

        // Convert back to world coordinates
        Vector3::new(tx + cx, y as i32, tz + cz)
    }

    /// Applies rotation to model block rotation value.
    /// Combines template rotation with model's own rotation.
    pub fn apply_model_rotation(&self, model_rotation: u8) -> u8 {
        (model_rotation + self.rotation) % 4
    }

    /// Gets the world position for a template block after rotation.
    pub fn get_world_position(&self, x: u8, y: u8, z: u8) -> Vector3<i32> {
        let offset = self.apply_rotation(x, y, z);
        self.position + offset
    }

    /// Returns an iterator over all blocks that need to be placed.
    /// Yields (world_pos, block_type) tuples.
    pub fn iter_blocks(&self) -> impl Iterator<Item = (Vector3<i32>, u8)> + '_ {
        self.template.blocks.iter().map(move |block| {
            let world_pos = self.get_world_position(block.x, block.y, block.z);
            (world_pos, block.block_type)
        })
    }

    /// Places a batch of blocks into the world.
    ///
    /// Returns true if placement is complete, false if more batches remain.
    /// This method is designed to be called once per frame for large templates.
    ///
    /// # Arguments
    /// * `world` - The world to place blocks into
    /// * `water_grid` - The water grid for placing water sources
    /// * `batch_size` - Number of blocks to place this frame
    pub fn place_batch(
        &mut self,
        world: &mut crate::world::World,
        water_grid: &mut crate::water::WaterGrid,
        batch_size: usize,
    ) -> bool {
        if self.is_complete() {
            return true;
        }

        let start = self.placement_progress;
        let end = (start + batch_size).min(self.total_blocks);

        // Place blocks in this batch
        for i in start..end {
            if i >= self.template.blocks.len() {
                break;
            }

            let block = &self.template.blocks[i];
            let world_pos = self.get_world_position(block.x, block.y, block.z);
            let block_type = crate::chunk::BlockType::from(block.block_type);

            // Place the block
            world.set_block(world_pos, block_type);

            // Place metadata based on block type
            match block_type {
                crate::chunk::BlockType::Model => {
                    // Find and place model metadata
                    if let Some(model_data) =
                        self.template.get_model_data(block.x, block.y, block.z)
                    {
                        let final_rotation = self.apply_model_rotation(model_data.rotation);
                        world.set_model_block(
                            world_pos,
                            model_data.model_id,
                            final_rotation,
                            model_data.waterlogged,
                        );
                    }
                }
                crate::chunk::BlockType::TintedGlass => {
                    // Find and place tint metadata
                    if let Some(tint_index) = self.template.get_tint_data(block.x, block.y, block.z)
                    {
                        world.set_tinted_glass_block(world_pos, tint_index);
                    }
                }
                crate::chunk::BlockType::Crystal => {
                    // Find and place crystal tint metadata
                    if let Some(tint_index) = self.template.get_tint_data(block.x, block.y, block.z)
                    {
                        world.set_crystal_block(world_pos, tint_index);
                    }
                }
                crate::chunk::BlockType::Painted => {
                    // Find and place paint metadata
                    if let Some((texture_idx, tint_idx)) =
                        self.template.get_paint_data(block.x, block.y, block.z)
                    {
                        world.set_painted_block(world_pos, texture_idx, tint_idx);
                    }
                }
                crate::chunk::BlockType::Water => {
                    // Find and place water metadata
                    if let Some((water_type, is_source)) =
                        self.template.get_water_data(block.x, block.y, block.z)
                    {
                        world.set_water_block(world_pos, water_type);

                        // Set water source if needed
                        if is_source {
                            water_grid.place_source(world_pos, water_type);
                        } else {
                            water_grid.set_water(world_pos, 1.0, false, water_type);
                        }
                    }
                }
                _ => {
                    // No metadata for other block types
                }
            }
        }

        self.placement_progress = end;
        self.is_complete()
    }
}

/// Frame-distributed template placer.
/// Places blocks in batches to prevent freezing on large templates.
pub struct FrameDistributedPlacer {
    /// Batch size (blocks per frame).
    pub batch_size: usize,
}

impl FrameDistributedPlacer {
    /// Creates a new placer with the default batch size (1000 blocks/frame).
    pub fn new() -> Self {
        Self { batch_size: 1000 }
    }

    /// Creates a placer with a custom batch size.
    pub fn with_batch_size(batch_size: usize) -> Self {
        Self { batch_size }
    }

    /// Checks if a template requires frame-distributed placement.
    /// Returns true if the template has more than batch_size blocks.
    pub fn requires_distribution(&self, template: &VxtFile) -> bool {
        template.block_count() > self.batch_size
    }

    /// Gets the current batch of blocks to place this frame.
    /// Returns a slice of block indices [start..end] for this batch.
    pub fn get_batch_range(&self, placement: &TemplatePlacement) -> (usize, usize) {
        let start = placement.placement_progress;
        let end = (start + self.batch_size).min(placement.total_blocks);
        (start, end)
    }
}

impl Default for FrameDistributedPlacer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::BlockType;
    use crate::templates::format::{TemplateBlock, VxtFile};

    fn create_test_template() -> VxtFile {
        let mut template = VxtFile::new("test".to_string(), "author".to_string(), 3, 3, 3);

        // Add a few blocks
        template.blocks.push(TemplateBlock {
            x: 0,
            y: 0,
            z: 0,
            block_type: BlockType::Stone as u8,
        });
        template.blocks.push(TemplateBlock {
            x: 2,
            y: 0,
            z: 0,
            block_type: BlockType::Dirt as u8,
        });
        template.blocks.push(TemplateBlock {
            x: 1,
            y: 1,
            z: 1,
            block_type: BlockType::Grass as u8,
        });

        template
    }

    #[test]
    fn test_rotation() {
        let template = create_test_template();
        let mut placement = TemplatePlacement::new(template, Vector3::new(100, 64, 200));

        // Test rotation cycling
        assert_eq!(placement.rotation, 0);
        placement.rotate_90();
        assert_eq!(placement.rotation, 1);
        placement.rotate_90();
        assert_eq!(placement.rotation, 2);
        placement.rotate_90();
        assert_eq!(placement.rotation, 3);
        placement.rotate_90();
        assert_eq!(placement.rotation, 0); // Wrap around
    }

    #[test]
    fn test_rotation_ccw() {
        let template = create_test_template();
        let mut placement = TemplatePlacement::new(template, Vector3::new(100, 64, 200));

        placement.rotate_90_ccw();
        assert_eq!(placement.rotation, 3);
        placement.rotate_90_ccw();
        assert_eq!(placement.rotation, 2);
    }

    #[test]
    fn test_apply_rotation_0() {
        let template = create_test_template();
        let placement = TemplatePlacement::new(template, Vector3::new(0, 0, 0));

        // At 0° rotation, positions should be unchanged (relative to center)
        let offset = placement.apply_rotation(1, 0, 1);
        assert_eq!(offset.x, 1); // Center
        assert_eq!(offset.z, 1); // Center
    }

    #[test]
    fn test_apply_rotation_90() {
        let template = create_test_template();
        let mut placement = TemplatePlacement::new(template, Vector3::new(0, 0, 0));
        placement.set_rotation(1); // 90°

        // 90° rotation: (x, z) -> (-z, x) relative to center
        let offset = placement.apply_rotation(0, 0, 0); // Top-left corner
        // This should rotate to a different corner
        // Center is (1, 1), corner (0, 0) is (-1, -1) relative to center
        // After 90°: (-z, x) = (1, -1) relative to center
        // Back to coords: (1+1, 1+(-1)) = (2, 0)
        assert_eq!(offset.x, 2);
        assert_eq!(offset.z, 0);
    }

    #[test]
    fn test_model_rotation_combination() {
        let template = create_test_template();
        let mut placement = TemplatePlacement::new(template, Vector3::new(0, 0, 0));

        // Model facing east (rotation 1) + template rotated 90° (rotation 1)
        // Should face south (rotation 2)
        placement.set_rotation(1);
        let combined = placement.apply_model_rotation(1);
        assert_eq!(combined, 2);

        // Model facing south (2) + template rotated 270° (3)
        // Should face west (1)
        placement.set_rotation(3);
        let combined = placement.apply_model_rotation(2);
        assert_eq!(combined, 1);
    }

    #[test]
    fn test_completion_tracking() {
        let template = create_test_template();
        let mut placement = TemplatePlacement::new(template, Vector3::new(0, 0, 0));

        assert_eq!(placement.total_blocks, 3);
        assert_eq!(placement.placement_progress, 0);
        assert!(!placement.is_complete());
        assert_eq!(placement.completion_percentage(), 0);

        placement.placement_progress = 1;
        assert_eq!(placement.completion_percentage(), 33);

        placement.placement_progress = 2;
        assert_eq!(placement.completion_percentage(), 66);

        placement.placement_progress = 3;
        assert!(placement.is_complete());
        assert_eq!(placement.completion_percentage(), 100);
    }

    #[test]
    fn test_frame_distributed_placer() {
        let placer = FrameDistributedPlacer::with_batch_size(2);

        let template = create_test_template(); // 3 blocks
        let placement = TemplatePlacement::new(template.clone(), Vector3::new(0, 0, 0));

        assert!(placer.requires_distribution(&template));

        // First batch: blocks 0-1
        let (start, end) = placer.get_batch_range(&placement);
        assert_eq!((start, end), (0, 2));

        // Second batch: blocks 2-2 (last one)
        let mut placement2 = placement;
        placement2.placement_progress = 2;
        let (start, end) = placer.get_batch_range(&placement2);
        assert_eq!((start, end), (2, 3));
    }

    #[test]
    fn test_iter_blocks() {
        let template = create_test_template();
        let placement = TemplatePlacement::new(template, Vector3::new(100, 64, 200));

        let blocks: Vec<_> = placement.iter_blocks().collect();
        assert_eq!(blocks.len(), 3);

        // First block should be Stone
        let (pos, block_type) = blocks[0];
        assert_eq!(block_type, BlockType::Stone as u8);
        // Position should be anchor + rotated offset
        assert!(pos.x >= 100);
        assert!(pos.y >= 64);
        assert!(pos.z >= 200);
    }
}

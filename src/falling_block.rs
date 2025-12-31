//! Falling block system for gravity-affected blocks.
//!
//! Handles falling sand, gravel, and other gravity-affected blocks.
//! When a block loses support, it converts to a falling entity,
//! simulates physics, and converts back to a static block on landing.

use bytemuck::{Pod, Zeroable};
use nalgebra::Vector3;

use crate::chunk::BlockType;

/// Maximum number of falling blocks that can exist at once.
pub const MAX_FALLING_BLOCKS: usize = 256;

/// Gravity acceleration in blocks per second squared.
const GRAVITY: f32 = 20.0;

/// A single falling block entity.
#[derive(Debug, Clone, Copy)]
pub struct FallingBlock {
    /// Position in world coordinates (center of block).
    pub position: Vector3<f32>,
    /// Velocity in blocks per second.
    pub velocity: Vector3<f32>,
    /// The type of block that is falling.
    pub block_type: BlockType,
    /// Time since block started falling (in seconds).
    pub age: f32,
}

impl FallingBlock {
    /// Creates a new falling block.
    ///
    /// Position should be the center of the block (e.g., grid position + 0.5).
    pub fn new(position: Vector3<f32>, block_type: BlockType) -> Self {
        Self {
            position,
            velocity: Vector3::zeros(),
            block_type,
            age: 0.0,
        }
    }

    /// Updates the falling block physics with world collision.
    ///
    /// `is_solid` should return true if the block at (x, y, z) is solid.
    /// Returns `Some(grid_position)` if the block has landed, `None` if still falling.
    pub fn update<F>(&mut self, delta_time: f32, is_solid: F) -> Option<Vector3<i32>>
    where
        F: Fn(i32, i32, i32) -> bool,
    {
        self.age += delta_time;

        // Apply gravity
        self.velocity.y -= GRAVITY * delta_time;

        // Calculate new position
        let new_pos = self.position + self.velocity * delta_time;

        // Check collision with ground (Y axis)
        // Check the block below the falling block's bottom edge
        let block_x = new_pos.x.floor() as i32;
        let block_y = (new_pos.y - 0.5).floor() as i32;
        let block_z = new_pos.z.floor() as i32;

        if is_solid(block_x, block_y, block_z) {
            // Land on the block above the solid one
            let land_pos = Vector3::new(block_x, block_y + 1, block_z);
            return Some(land_pos);
        }

        // No collision, update position
        self.position = new_pos;

        // Check if fallen too far (below world)
        if self.position.y < -64.0 {
            // Despawn by returning a position that will be ignored
            // (handled by caller checking bounds)
            return Some(Vector3::new(block_x, -100, block_z));
        }

        None
    }
}

/// GPU-compatible falling block data for shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GpuFallingBlock {
    /// Position XYZ + block type (as float)
    pub pos_type: [f32; 4],
    /// Velocity XYZ + age (for potential rotation animation)
    pub velocity_age: [f32; 4],
}

impl From<&FallingBlock> for GpuFallingBlock {
    fn from(fb: &FallingBlock) -> Self {
        Self {
            pos_type: [
                fb.position.x,
                fb.position.y,
                fb.position.z,
                fb.block_type as u8 as f32,
            ],
            velocity_age: [fb.velocity.x, fb.velocity.y, fb.velocity.z, fb.age],
        }
    }
}

/// Information about a block that has landed.
#[derive(Debug, Clone, Copy)]
pub struct LandedBlock {
    /// Grid position where the block landed.
    pub position: Vector3<i32>,
    /// The type of block that landed.
    pub block_type: BlockType,
}

/// Manages all active falling blocks.
pub struct FallingBlockSystem {
    blocks: Vec<FallingBlock>,
}

impl Default for FallingBlockSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl FallingBlockSystem {
    /// Creates a new empty falling block system.
    pub fn new() -> Self {
        Self {
            blocks: Vec::with_capacity(MAX_FALLING_BLOCKS),
        }
    }

    /// Spawns a new falling block at the given grid position.
    ///
    /// The position is converted to center coordinates (grid + 0.5).
    /// Returns false if at capacity.
    pub fn spawn(&mut self, grid_position: Vector3<i32>, block_type: BlockType) -> bool {
        if self.blocks.len() >= MAX_FALLING_BLOCKS {
            return false;
        }

        // Convert grid position to center of block
        let center = Vector3::new(
            grid_position.x as f32 + 0.5,
            grid_position.y as f32 + 0.5,
            grid_position.z as f32 + 0.5,
        );

        self.blocks.push(FallingBlock::new(center, block_type));
        true
    }

    /// Updates all falling blocks and returns blocks that have landed.
    ///
    /// `is_solid` should return true if the block at (x, y, z) is solid.
    /// Returns a vector of blocks that have landed and need to be placed in the world.
    pub fn update<F>(&mut self, delta_time: f32, is_solid: F) -> Vec<LandedBlock>
    where
        F: Fn(i32, i32, i32) -> bool + Copy,
    {
        let mut landed = Vec::new();

        self.blocks.retain_mut(|fb| {
            match fb.update(delta_time, is_solid) {
                Some(land_pos) => {
                    // Block landed - check if position is valid (not below world)
                    if land_pos.y >= 0 {
                        landed.push(LandedBlock {
                            position: land_pos,
                            block_type: fb.block_type,
                        });
                    }
                    false // Remove from falling blocks
                }
                None => true, // Still falling, keep it
            }
        });

        landed
    }

    /// Returns the number of active falling blocks.
    pub fn count(&self) -> usize {
        self.blocks.len()
    }

    /// Gets GPU-ready falling block data.
    pub fn gpu_data(&self) -> Vec<GpuFallingBlock> {
        self.blocks.iter().map(GpuFallingBlock::from).collect()
    }

    /// Clears all falling blocks.
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.blocks.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_falling_block_new() {
        let fb = FallingBlock::new(Vector3::new(5.5, 10.5, 3.5), BlockType::Sand);
        assert_eq!(fb.position, Vector3::new(5.5, 10.5, 3.5));
        assert_eq!(fb.velocity, Vector3::zeros());
        assert_eq!(fb.block_type, BlockType::Sand);
        assert_eq!(fb.age, 0.0);
    }

    #[test]
    fn test_falling_block_falls() {
        let mut fb = FallingBlock::new(Vector3::new(5.5, 10.5, 3.5), BlockType::Sand);

        // No solid blocks - should continue falling
        let result = fb.update(0.1, |_, _, _| false);
        assert!(result.is_none());
        assert!(fb.position.y < 10.5); // Should have fallen
        assert!(fb.velocity.y < 0.0); // Should have negative velocity
    }

    #[test]
    fn test_falling_block_lands() {
        let mut fb = FallingBlock::new(Vector3::new(5.5, 1.5, 3.5), BlockType::Sand);
        fb.velocity.y = -5.0; // Already falling

        // Solid block at y=0
        let result = fb.update(0.1, |_, y, _| y == 0);

        // Should land on top of the solid block (y=1)
        assert!(result.is_some());
        let land_pos = result.unwrap();
        assert_eq!(land_pos.y, 1);
    }

    #[test]
    fn test_system_spawn_and_update() {
        let mut system = FallingBlockSystem::new();

        // Spawn a falling block
        assert!(system.spawn(Vector3::new(5, 10, 3), BlockType::Sand));
        assert_eq!(system.count(), 1);

        // Update with no solid blocks
        let landed = system.update(0.016, |_, _, _| false);
        assert!(landed.is_empty());
        assert_eq!(system.count(), 1); // Still falling

        // Update with solid block below - will eventually land
        // Simulate many updates until it lands (use small time steps like 60 FPS)
        // With g=20 and fall distance of ~10 blocks, time to fall = sqrt(2*10/20) ≈ 1 second
        // 60 FPS * 2 seconds = 120 frames should be plenty
        for _ in 0..200 {
            let landed = system.update(0.016, |_, y, _| y == 0);
            if !landed.is_empty() {
                assert_eq!(landed[0].block_type, BlockType::Sand);
                assert_eq!(landed[0].position.y, 1);
                assert_eq!(system.count(), 0); // Removed after landing
                return;
            }
        }
        panic!("Block should have landed by now");
    }

    #[test]
    fn test_gpu_data() {
        let mut system = FallingBlockSystem::new();
        system.spawn(Vector3::new(5, 10, 3), BlockType::Sand);

        let gpu_data = system.gpu_data();
        assert_eq!(gpu_data.len(), 1);
        assert_eq!(gpu_data[0].pos_type[3], BlockType::Sand as u8 as f32);
    }

    #[test]
    fn test_max_capacity() {
        let mut system = FallingBlockSystem::new();

        // Fill to capacity
        for i in 0..MAX_FALLING_BLOCKS {
            assert!(system.spawn(Vector3::new(i as i32, 10, 0), BlockType::Sand));
        }

        // Should reject additional spawns
        assert!(!system.spawn(Vector3::new(0, 10, 0), BlockType::Gravel));
        assert_eq!(system.count(), MAX_FALLING_BLOCKS);
    }
}

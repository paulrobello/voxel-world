//! Stair shape calculation and automatic corner detection.

use super::World;
use crate::chunk::BlockType;
use crate::sub_voxel::{ModelRegistry, StairShape};
use nalgebra::Vector3;

impl World {
    /// Returns the facing direction vector for a stair rotation (0-3).
    #[inline]
    fn stair_facing(rotation: u8) -> Vector3<i32> {
        match rotation & 3 {
            // Base stairs model has low side toward -Z; rotate clockwise for subsequent states.
            0 => Vector3::new(0, 0, -1), // facing -Z (front)
            1 => Vector3::new(1, 0, 0),  // facing +X
            2 => Vector3::new(0, 0, 1),  // facing +Z
            _ => Vector3::new(-1, 0, 0), // facing -X
        }
    }

    #[inline]
    fn rotate_left(dir: Vector3<i32>) -> Vector3<i32> {
        // Left = up x dir
        Vector3::new(dir.z, 0, -dir.x)
    }

    #[inline]
    fn rotate_right(dir: Vector3<i32>) -> Vector3<i32> {
        // Right = dir x up
        Vector3::new(-dir.z, 0, dir.x)
    }

    /// Returns facing for neighboring stair if it matches the inverted flag.
    fn stair_neighbor_facing(&self, pos: Vector3<i32>, inverted: bool) -> Option<Vector3<i32>> {
        if let Some(BlockType::Model) = self.get_block(pos) {
            if let Some(data) = self.get_model_data(pos) {
                if ModelRegistry::is_stairs_model(data.model_id)
                    && ModelRegistry::is_stairs_inverted(data.model_id) == inverted
                {
                    return Some(Self::stair_facing(data.rotation));
                }
            }
        }
        None
    }

    /// Recomputes stair corner shape only for the given position.
    pub fn update_stair_shape_at(&mut self, pos: Vector3<i32>) {
        let Some(BlockType::Model) = self.get_block(pos) else {
            return;
        };
        let Some(data) = self.get_model_data(pos) else {
            return;
        };
        if !ModelRegistry::is_stairs_model(data.model_id) {
            return;
        }

        let inverted = ModelRegistry::is_stairs_inverted(data.model_id);
        let rotation = data.rotation & 3;
        let facing = Self::stair_facing(rotation);

        let left_dir = Self::rotate_left(facing);
        let right_dir = Self::rotate_right(facing);

        // Minecraft wiki stair corner logic:
        // - Inner corner: Our HALF-BLOCK (low/front) side adjacent to SIDE of another stair
        // - Outer corner: Our FULL-BLOCK (high/back) side adjacent to SIDE of another stair
        //
        // facing = direction of LOW side, so:
        //   front_pos (pos + facing) = toward our low side
        //   back_pos (pos - facing) = toward our high side
        let front_pos = pos + facing; // Low side direction - check for INNER corners
        let back_pos = pos - facing; // High side direction - check for OUTER corners

        let front_neighbor = self.stair_neighbor_facing(front_pos, inverted);
        let back_neighbor = self.stair_neighbor_facing(back_pos, inverted);

        let mut shape = StairShape::Straight;

        // 1. Outer Corner Check (Priority) - check FRONT neighbor (at our low side)
        // When neighbor is at our front and faces perpendicular, we get an outer corner
        // (single raised quadrant connecting to neighbor's high side)
        if let Some(ff) = front_neighbor {
            if ff == left_dir {
                shape = StairShape::OuterLeft;
            } else if ff == right_dir {
                shape = StairShape::OuterRight;
            }
        }

        // 2. Inner Corner Check - check BACK neighbor (at our high side)
        // When neighbor is at our back and faces perpendicular, we get an inner corner
        // (L-shaped top with pocket)
        if shape == StairShape::Straight {
            if let Some(bf) = back_neighbor {
                if bf == left_dir {
                    shape = StairShape::InnerRight;
                } else if bf == right_dir {
                    shape = StairShape::InnerLeft;
                }
            }
        }

        // 3. Check LEFT neighbor - stair to our left that's perpendicular
        if shape == StairShape::Straight {
            let left_pos = pos + left_dir;
            if let Some(lf) = self.stair_neighbor_facing(left_pos, inverted) {
                if lf == right_dir {
                    shape = StairShape::InnerRight;
                } else if lf == -left_dir {
                    shape = StairShape::OuterRight;
                }
            }
        }

        // 4. Check RIGHT neighbor - stair to our right that's perpendicular
        if shape == StairShape::Straight {
            let right_pos = pos + right_dir;
            if let Some(rf) = self.stair_neighbor_facing(right_pos, inverted) {
                if rf == left_dir {
                    shape = StairShape::InnerLeft;
                } else if rf == -right_dir {
                    shape = StairShape::OuterLeft;
                }
            }
        }

        // For inverted (ceiling) stairs, swap both Inner↔Outer AND Left↔Right
        // since geometry is flipped vertically, changing both relationships
        if inverted && shape != StairShape::Straight {
            shape = match shape {
                StairShape::InnerLeft => StairShape::OuterRight,
                StairShape::InnerRight => StairShape::OuterLeft,
                StairShape::OuterLeft => StairShape::InnerRight,
                StairShape::OuterRight => StairShape::InnerLeft,
                StairShape::Straight => StairShape::Straight,
            };
        }

        let target_model = ModelRegistry::stairs_model_id(shape, inverted);
        if target_model != data.model_id {
            println!(
                "Updating stair at ({}, {}, {}) from model_id {} to {} (shape: {:?})",
                pos.x, pos.y, pos.z, data.model_id, target_model, shape
            );
            self.set_model_block(pos, target_model, rotation, data.waterlogged);
        }
    }

    /// Recompute shapes for four horizontal neighbors.
    pub fn update_adjacent_stair_shapes(&mut self, center: Vector3<i32>) {
        let neighbors = [
            Vector3::new(1, 0, 0),
            Vector3::new(-1, 0, 0),
            Vector3::new(0, 0, 1),
            Vector3::new(0, 0, -1),
        ];
        for n in neighbors {
            self.update_stair_shape_at(center + n);
        }
    }

    /// Recompute shape for a newly placed stair (only the placed stair adapts, not neighbors).
    pub fn update_stair_and_neighbors(&mut self, pos: Vector3<i32>) {
        self.update_stair_shape_at(pos);
    }
}

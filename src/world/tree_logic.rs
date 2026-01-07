//! Tree detection and validation logic.

use super::World;
use crate::chunk::BlockType;
use nalgebra::Vector3;
use std::collections::{HashSet, VecDeque};

impl World {
    /// Finds all leaves connected to the starting leaf, and checks if any connect to a log.
    /// Returns (leaf_positions, has_log_connection).
    pub fn find_leaf_cluster_and_check_log(
        &self,
        start: Vector3<i32>,
    ) -> (Vec<(Vector3<i32>, BlockType)>, bool) {
        let mut leaves = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut found_log = false;

        // Verify starting block is leaves
        if let Some(block) = self.get_block(start) {
            if block != BlockType::Leaves {
                return (leaves, true); // Not leaves, assume connected
            }
        } else {
            return (leaves, true);
        }

        queue.push_back(start);
        visited.insert(start);

        // 26-directional for leaf-to-leaf, 6-directional for leaf-to-log check
        let mut neighbors_26 = Vec::with_capacity(26);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if dx != 0 || dy != 0 || dz != 0 {
                        neighbors_26.push(Vector3::new(dx, dy, dz));
                    }
                }
            }
        }

        while let Some(pos) = queue.pop_front() {
            if let Some(block) = self.get_block(pos) {
                if matches!(
                    block,
                    BlockType::Leaves | BlockType::PineLeaves | BlockType::WillowLeaves
                ) {
                    leaves.push((pos, block));

                    for offset in &neighbors_26 {
                        let neighbor = pos + offset;
                        let is_cardinal = (offset.x != 0) as i32
                            + (offset.y != 0) as i32
                            + (offset.z != 0) as i32
                            == 1;

                        if let Some(neighbor_block) = self.get_block(neighbor) {
                            // Check for log connection (orthogonal only)
                            if neighbor_block.is_log() && is_cardinal {
                                found_log = true;
                            }

                            // Add unvisited leaves to queue (any direction)
                            if matches!(
                                neighbor_block,
                                BlockType::Leaves | BlockType::PineLeaves | BlockType::WillowLeaves
                            ) && !visited.contains(&neighbor)
                            {
                                visited.insert(neighbor);
                                queue.push_back(neighbor);
                            }
                        }
                    }
                }
            }
        }

        (leaves, found_log)
    }

    /// Flood-fill to find all connected tree blocks (logs and leaves) starting from a log.
    /// Returns a vector of (position, block_type) for all connected blocks.
    pub fn find_connected_tree(&self, start: Vector3<i32>) -> Vec<(Vector3<i32>, BlockType)> {
        let mut tree_blocks = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        // Verify starting block is a log
        if let Some(block) = self.get_block(start) {
            if !block.is_log() {
                return tree_blocks;
            }
        } else {
            return tree_blocks;
        }

        queue.push_back(start);
        visited.insert(start);

        // 26-directional neighbors (including diagonals)
        let mut neighbors_26 = Vec::with_capacity(26);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if dx != 0 || dy != 0 || dz != 0 {
                        neighbors_26.push(Vector3::new(dx, dy, dz));
                    }
                }
            }
        }

        while let Some(pos) = queue.pop_front() {
            if let Some(block) = self.get_block(pos) {
                if block.is_tree_part() {
                    tree_blocks.push((pos, block));

                    // Connectivity rules to prevent merging separate trees:
                    // - Logs: only connect orthogonally (6-dir) to logs and leaves
                    // - Leaves: connect diagonally (26-dir) to OTHER leaves,
                    //           but only orthogonally (6-dir) to logs
                    for offset in &neighbors_26 {
                        let neighbor = pos + offset;
                        if !visited.contains(&neighbor) {
                            if let Some(neighbor_block) = self.get_block(neighbor) {
                                if !neighbor_block.is_tree_part() {
                                    continue;
                                }

                                // is_cardinal: exactly one axis is non-zero (orthogonal neighbor)
                                let is_cardinal = (offset.x != 0) as i32
                                    + (offset.y != 0) as i32
                                    + (offset.z != 0) as i32
                                    == 1;

                                let should_connect = if block.is_log() {
                                    // Logs only connect orthogonally (6-dir)
                                    is_cardinal
                                } else {
                                    // Leaves: connect to other leaves diagonally (26-dir),
                                    // but only connect to logs orthogonally (6-dir)
                                    if neighbor_block.is_log() {
                                        is_cardinal
                                    } else {
                                        true // leaf-to-leaf: any direction
                                    }
                                };

                                if should_connect {
                                    visited.insert(neighbor);
                                    queue.push_back(neighbor);
                                }
                            }
                        }
                    }
                }
            }
        }

        tree_blocks
    }

    /// Checks if any log in the tree has ground support.
    /// A log has ground support if the block below it is solid and NOT a log.
    pub fn tree_has_ground_support(&self, tree_blocks: &[(Vector3<i32>, BlockType)]) -> bool {
        for (pos, block) in tree_blocks {
            if block.is_log() {
                let below_pos = pos + Vector3::new(0, -1, 0);
                if let Some(below_block) = self.get_block(below_pos) {
                    // Supported if block below is solid and NOT part of the tree
                    // (leaves don't count as support either!)
                    if below_block.is_solid() && !below_block.is_tree_part() {
                        return true;
                    }
                }
            }
        }
        false
    }
}

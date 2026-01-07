//! Connection logic for fences, gates, and windows.

use super::World;
use crate::chunk::BlockType;
use crate::sub_voxel::ModelRegistry;
use nalgebra::Vector3;

impl World {
    /// Calculates fence connection bitmask based on neighboring fences/gates.
    /// Returns N=1, S=2, E=4, W=8 bitmask.
    /// Note: North is -Z, South is +Z (matching model definition)
    pub fn calculate_fence_connections(&self, pos: Vector3<i32>) -> u8 {
        let mut connections = 0u8;

        // Check north (-Z)
        if self.is_fence_connectable(pos + Vector3::new(0, 0, -1)) {
            connections |= 1;
        }
        // Check south (+Z)
        if self.is_fence_connectable(pos + Vector3::new(0, 0, 1)) {
            connections |= 2;
        }
        // Check east (+X)
        if self.is_fence_connectable(pos + Vector3::new(1, 0, 0)) {
            connections |= 4;
        }
        // Check west (-X)
        if self.is_fence_connectable(pos + Vector3::new(-1, 0, 0)) {
            connections |= 8;
        }

        connections
    }

    /// Calculates gate connection bitmask based on neighboring fences/gates.
    /// Returns W=1, E=2 bitmask (gates only connect east-west).
    pub fn calculate_gate_connections(&self, pos: Vector3<i32>) -> u8 {
        let mut connections = 0u8;

        // Check west (-X)
        if self.is_fence_connectable(pos + Vector3::new(-1, 0, 0)) {
            connections |= 1;
        }
        // Check east (+X)
        if self.is_fence_connectable(pos + Vector3::new(1, 0, 0)) {
            connections |= 2;
        }

        connections
    }

    /// Returns true if the block at pos can connect to fences/gates.
    pub fn is_fence_connectable(&self, pos: Vector3<i32>) -> bool {
        if let Some(block) = self.get_block(pos) {
            match block {
                BlockType::Model => {
                    // Check if it's a fence or gate model
                    if let Some(data) = self.get_model_data(pos) {
                        ModelRegistry::is_fence_or_gate(data.model_id)
                    } else {
                        false
                    }
                }
                // Solid blocks also connect to fences
                b if b.is_solid() => true,
                _ => false,
            }
        } else {
            false
        }
    }

    /// Updates fence/gate connections for a position and its neighbors.
    pub fn update_fence_connections(&mut self, center_pos: Vector3<i32>) {
        // Update neighbors in all 4 horizontal directions
        let neighbors = [
            Vector3::new(0, 0, 1),  // North
            Vector3::new(0, 0, -1), // South
            Vector3::new(1, 0, 0),  // East
            Vector3::new(-1, 0, 0), // West
        ];

        for offset in &neighbors {
            let neighbor_pos = center_pos + offset;
            if let Some(BlockType::Model) = self.get_block(neighbor_pos) {
                if let Some(data) = self.get_model_data(neighbor_pos) {
                    if ModelRegistry::is_fence_model(data.model_id) {
                        // Update fence connections
                        let connections = self.calculate_fence_connections(neighbor_pos);
                        let new_model_id = ModelRegistry::fence_model_id(connections);
                        if new_model_id != data.model_id {
                            // Force rotation 0 for fences as their orientation is in the model_id
                            self.set_model_block(neighbor_pos, new_model_id, 0, data.waterlogged);
                        }
                    } else if ModelRegistry::is_gate_model(data.model_id) {
                        // Update gate connections
                        let connections = self.calculate_gate_connections(neighbor_pos);
                        let is_open = ModelRegistry::is_gate_open_model(data.model_id);
                        let new_model_id = if is_open {
                            ModelRegistry::gate_open_model_id(connections)
                        } else {
                            ModelRegistry::gate_closed_model_id(connections)
                        };
                        if new_model_id != data.model_id {
                            self.set_model_block(
                                neighbor_pos,
                                new_model_id,
                                data.rotation,
                                data.waterlogged,
                            );
                        }
                    }
                }
            }
        }
    }

    /// Calculates window connection bitmask based on neighboring windows/solid blocks.
    /// Returns N=1, S=2, E=4, W=8 bitmask (same as fences).
    pub fn calculate_window_connections(&self, pos: Vector3<i32>) -> u8 {
        let mut connections = 0u8;

        // Check north (-Z)
        if self.is_window_connectable(pos + Vector3::new(0, 0, -1)) {
            connections |= 1;
        }
        // Check south (+Z)
        if self.is_window_connectable(pos + Vector3::new(0, 0, 1)) {
            connections |= 2;
        }
        // Check east (+X)
        if self.is_window_connectable(pos + Vector3::new(1, 0, 0)) {
            connections |= 4;
        }
        // Check west (-X)
        if self.is_window_connectable(pos + Vector3::new(-1, 0, 0)) {
            connections |= 8;
        }

        connections
    }

    /// Returns true if the block at pos can connect to windows.
    pub fn is_window_connectable(&self, pos: Vector3<i32>) -> bool {
        if let Some(block) = self.get_block(pos) {
            match block {
                BlockType::Model => {
                    // Check if it's a window model
                    if let Some(data) = self.get_model_data(pos) {
                        ModelRegistry::is_window_model(data.model_id)
                    } else {
                        false
                    }
                }
                // Solid blocks also connect to windows
                b if b.is_solid() => true,
                // Glass blocks connect too
                BlockType::Glass | BlockType::TintedGlass => true,
                _ => false,
            }
        } else {
            false
        }
    }

    /// Updates window connections for a position and its neighbors.
    pub fn update_window_connections(&mut self, center_pos: Vector3<i32>) {
        // Update neighbors in all 4 horizontal directions
        let neighbors = [
            Vector3::new(0, 0, 1),  // South
            Vector3::new(0, 0, -1), // North
            Vector3::new(1, 0, 0),  // East
            Vector3::new(-1, 0, 0), // West
        ];

        for offset in &neighbors {
            let neighbor_pos = center_pos + offset;
            if let Some(BlockType::Model) = self.get_block(neighbor_pos) {
                if let Some(data) = self.get_model_data(neighbor_pos) {
                    if ModelRegistry::is_window_model(data.model_id) {
                        // Update window connections
                        let connections = self.calculate_window_connections(neighbor_pos);
                        let new_model_id = ModelRegistry::window_model_id(connections);
                        if new_model_id != data.model_id {
                            // Force rotation 0 for windows as their orientation is in the model_id
                            self.set_model_block(neighbor_pos, new_model_id, 0, data.waterlogged);
                        }
                    }
                }
            }
        }
    }
}

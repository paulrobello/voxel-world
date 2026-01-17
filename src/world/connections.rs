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

    // ========================================================================
    // GLASS PANE CONNECTIONS
    // ========================================================================

    /// Returns true if the block at pos should cause a pane's frame edge to be hidden.
    /// Only other glass panes cause frame edges to hide (panes merge together).
    /// Solid blocks and glass blocks do NOT hide frame edges - the frame meets the block.
    pub fn is_pane_connectable(&self, pos: Vector3<i32>) -> bool {
        // Only glass pane models hide adjacent pane frame edges
        if let Some(BlockType::Model) = self.get_block(pos) {
            if let Some(data) = self.get_model_data(pos) {
                return ModelRegistry::is_glass_pane_model(data.model_id);
            }
        }
        false
    }

    /// Calculates horizontal glass pane connection bitmask.
    /// Returns N=1, S=2, E=4, W=8 bitmask (same as fences).
    pub fn calculate_horizontal_pane_connections(&self, pos: Vector3<i32>) -> u8 {
        let mut connections = 0u8;

        // Check north (-Z)
        if self.is_pane_connectable(pos + Vector3::new(0, 0, -1)) {
            connections |= 1;
        }
        // Check south (+Z)
        if self.is_pane_connectable(pos + Vector3::new(0, 0, 1)) {
            connections |= 2;
        }
        // Check east (+X)
        if self.is_pane_connectable(pos + Vector3::new(1, 0, 0)) {
            connections |= 4;
        }
        // Check west (-X)
        if self.is_pane_connectable(pos + Vector3::new(-1, 0, 0)) {
            connections |= 8;
        }

        connections
    }

    /// Calculates vertical glass pane connection bitmask based on rotation.
    /// Returns N=1 (+Y), S=2 (-Y), E=4, W=8 bitmask.
    ///
    /// For rotation 0 (XY plane facing Z): E/W check +X/-X
    /// For rotation 1 (YZ plane facing X): E/W check +Z/-Z
    pub fn calculate_vertical_pane_connections(&self, pos: Vector3<i32>, rotation: u8) -> u8 {
        let mut connections = 0u8;

        // Check up (+Y)
        if self.is_pane_connectable(pos + Vector3::new(0, 1, 0)) {
            connections |= 1;
        }
        // Check down (-Y)
        if self.is_pane_connectable(pos + Vector3::new(0, -1, 0)) {
            connections |= 2;
        }

        // Horizontal connections depend on rotation
        match rotation {
            0 | 2 => {
                // XY plane: check E (+X) and W (-X)
                if self.is_pane_connectable(pos + Vector3::new(1, 0, 0)) {
                    connections |= 4;
                }
                if self.is_pane_connectable(pos + Vector3::new(-1, 0, 0)) {
                    connections |= 8;
                }
            }
            1 | 3 => {
                // YZ plane: check E (+Z) and W (-Z)
                if self.is_pane_connectable(pos + Vector3::new(0, 0, 1)) {
                    connections |= 4;
                }
                if self.is_pane_connectable(pos + Vector3::new(0, 0, -1)) {
                    connections |= 8;
                }
            }
            _ => {}
        }

        connections
    }

    /// Updates horizontal glass pane connections for a position and its neighbors.
    pub fn update_horizontal_pane_connections(&mut self, center_pos: Vector3<i32>) {
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
                    if ModelRegistry::is_horizontal_glass_pane_model(data.model_id) {
                        let connections = self.calculate_horizontal_pane_connections(neighbor_pos);
                        let new_model_id =
                            ModelRegistry::horizontal_glass_pane_model_id(connections);
                        if new_model_id != data.model_id {
                            self.set_model_block(neighbor_pos, new_model_id, 0, data.waterlogged);
                        }
                    }
                }
            }
        }
    }

    /// Updates vertical glass pane connections for a position and its neighbors.
    pub fn update_vertical_pane_connections(&mut self, center_pos: Vector3<i32>) {
        // For vertical panes, we check up, down, and the 4 horizontal directions
        let neighbors = [
            Vector3::new(0, 1, 0),  // Up
            Vector3::new(0, -1, 0), // Down
            Vector3::new(1, 0, 0),  // East
            Vector3::new(-1, 0, 0), // West
            Vector3::new(0, 0, 1),  // South
            Vector3::new(0, 0, -1), // North
        ];

        for offset in &neighbors {
            let neighbor_pos = center_pos + offset;
            if let Some(BlockType::Model) = self.get_block(neighbor_pos) {
                if let Some(data) = self.get_model_data(neighbor_pos) {
                    if ModelRegistry::is_vertical_glass_pane_model(data.model_id) {
                        let connections =
                            self.calculate_vertical_pane_connections(neighbor_pos, data.rotation);
                        let new_model_id = ModelRegistry::vertical_glass_pane_model_id(connections);
                        if new_model_id != data.model_id {
                            // Preserve rotation
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

    /// Updates all glass pane connections (both horizontal and vertical).
    pub fn update_pane_connections(&mut self, center_pos: Vector3<i32>) {
        self.update_horizontal_pane_connections(center_pos);
        self.update_vertical_pane_connections(center_pos);
    }
}

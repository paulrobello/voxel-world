//! Connection logic for fences, gates, and windows.

use super::World;
use crate::chunk::BlockType;
use crate::sub_voxel::ModelRegistry;
use crate::sub_voxel::builtins::frames;
use nalgebra::Vector3;
use std::collections::VecDeque;

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

    // ========================================================================
    // PICTURE FRAME AUTO-SIZING
    // ========================================================================

    /// Returns the right-direction vector (width axis) for a frame based on facing.
    pub fn frame_right_vec(facing: u8) -> Vector3<i32> {
        match facing {
            0 => Vector3::new(1, 0, 0),  // +X
            1 => Vector3::new(0, 0, 1),  // +Z
            2 => Vector3::new(-1, 0, 0), // -X
            3 => Vector3::new(0, 0, -1), // -Z
            _ => Vector3::new(1, 0, 0),
        }
    }

    /// Updates metadata (offsets, size, facing) for the contiguous frame cluster
    /// containing `center_pos`. Cluster detection is limited to 3×3 blocks (MAX_FRAME_DIM).
    /// Returns the list of positions that were updated (for chunk dirty marking).
    pub fn update_frame_cluster(&mut self, center_pos: Vector3<i32>) -> Vec<Vector3<i32>> {
        let Some(BlockType::Model) = self.get_block(center_pos) else {
            return Vec::new();
        };

        let Some(data) = self.get_model_data(center_pos) else {
            return Vec::new();
        };

        if !ModelRegistry::is_frame_model(data.model_id) {
            return Vec::new();
        }

        let facing = frames::metadata::decode_facing(data.custom_data);
        let right = Self::frame_right_vec(facing);
        let up = Vector3::new(0, 1, 0);

        // Frames lie on a vertical plane: z-constant for facing X, x-constant for facing Z.
        let plane_coord = if facing % 2 == 0 {
            center_pos.z
        } else {
            center_pos.x
        };

        // BFS limited to MAX_FRAME_DIM in both axes.
        let mut visited = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(center_pos);

        while let Some(pos) = queue.pop_front() {
            if visited.contains(&pos) {
                continue;
            }
            // Plane check
            if (facing % 2 == 0 && pos.z != plane_coord)
                || (facing % 2 == 1 && pos.x != plane_coord)
            {
                continue;
            }
            // Size bounds (keep cluster within 3×3 around center)
            if (pos.x - center_pos.x).abs() >= frames::MAX_FRAME_DIM as i32
                || (pos.y - center_pos.y).abs() >= frames::MAX_FRAME_DIM as i32
                || (pos.z - center_pos.z).abs() >= frames::MAX_FRAME_DIM as i32
            {
                continue;
            }

            if let Some(BlockType::Model) = self.get_block(pos) {
                if let Some(md) = self.get_model_data(pos) {
                    if ModelRegistry::is_frame_model(md.model_id)
                        && frames::metadata::decode_facing(md.custom_data) == facing
                    {
                        visited.push(pos);
                        // Explore neighbors in frame plane (right/left/up/down)
                        queue.push_back(pos + right);
                        queue.push_back(pos - right);
                        queue.push_back(pos + up);
                        queue.push_back(pos - up);
                    }
                }
            }
        }

        if visited.is_empty() {
            return Vec::new();
        }

        // Compute bounds along right axis and vertical axis
        let right_axis_is_x = right.x != 0;
        let right_sign = if right_axis_is_x { right.x } else { right.z }; // ±1

        let mut min_right = i32::MAX;
        let mut max_right = i32::MIN;
        let mut min_y = i32::MAX;
        let mut max_y = i32::MIN;

        for pos in &visited {
            let rcoord = if right_axis_is_x { pos.x } else { pos.z };
            min_right = min_right.min(rcoord);
            max_right = max_right.max(rcoord);
            min_y = min_y.min(pos.y);
            max_y = max_y.max(pos.y);
        }

        let mut width = (max_right - min_right + 1) as u8;
        let mut height = (max_y - min_y + 1) as u8;
        width = width.min(frames::MAX_FRAME_DIM);
        height = height.min(frames::MAX_FRAME_DIM);

        let anchor_right = if right_sign >= 0 {
            min_right
        } else {
            max_right
        };

        // Use picture_id from first block (default 0).
        let picture_id = frames::metadata::decode_picture_id(data.custom_data);

        for pos in &visited {
            let rcoord = if right_axis_is_x { pos.x } else { pos.z };
            let offset_x = ((rcoord - anchor_right) * right_sign).max(0) as u8;
            let offset_y = (pos.y - min_y).max(0) as u8;

            if let Some(md) = self.get_model_data(*pos) {
                let waterlogged = md.waterlogged;
                // Edge mask: bit0=left, bit1=right, bit2=bottom, bit3=top.
                let mask_left = offset_x == 0;
                let mask_right = offset_x + 1 == width;
                let mask_bottom = offset_y == 0;
                let mask_top = offset_y + 1 == height;
                let edge_mask: u8 = (mask_left as u8)
                    | ((mask_right as u8) << 1)
                    | ((mask_bottom as u8) << 2)
                    | ((mask_top as u8) << 3);

                // Store facing in low bits. Model ID encodes edge mask: model_id = 160 + edge_mask.
                let rotation = facing & 0x03;
                let model_id = frames::edge_mask_to_frame_model_id(edge_mask);

                let custom =
                    frames::metadata::encode(picture_id, offset_x, offset_y, width, height, facing);

                self.set_model_block_with_data(
                    *pos,
                    model_id,
                    rotation,
                    waterlogged,
                    custom,
                );
            }
        }

        visited
    }

    /// Updates the frame cluster at `center_pos` and its immediate neighbors.
    pub fn update_adjacent_frame_clusters(&mut self, center_pos: Vector3<i32>) {
        let neighbors = [
            Vector3::new(0, 0, 0),
            Vector3::new(1, 0, 0),
            Vector3::new(-1, 0, 0),
            Vector3::new(0, 1, 0),
            Vector3::new(0, -1, 0),
            Vector3::new(0, 0, 1),
            Vector3::new(0, 0, -1),
        ];

        // Collect all positions that were updated across all cluster updates
        let mut all_updated_positions: std::collections::HashSet<Vector3<i32>> = std::collections::HashSet::new();

        for offset in neighbors {
            let pos = center_pos + offset;
            let updated = self.update_frame_cluster(pos);
            all_updated_positions.extend(updated);
        }

        // Mark chunks as dirty for ALL positions that were updated
        for pos in &all_updated_positions {
            let chunk_pos = Self::world_to_chunk(*pos);
            if let Some(chunk) = self.chunks.get_mut(&chunk_pos) {
                chunk.mark_dirty();
            }
        }
    }

    /// Recomputes all frame clusters in the world.
    /// This scans for all frame blocks and updates their cluster metadata.
    /// Useful for migrating worlds from before frame clustering was implemented.
    pub fn recompute_all_frame_clusters(&mut self) {
        use crate::sub_voxel::ModelRegistry;

        let mut frame_positions = Vec::new();

        // First pass: collect all frame positions
        let chunk_positions: Vec<_> = self.chunks.keys().copied().collect();
        for chunk_pos in chunk_positions {
            let chunk = match self.chunks.get(&chunk_pos) {
                Some(c) => c,
                None => continue,
            };

            // Get all model entries from this chunk
            for (&idx, _data) in chunk.model_entries() {
                let (lx, ly, lz) = crate::chunk::Chunk::index_to_coords(idx);
                let world_pos = crate::world::World::chunk_to_world(chunk_pos);

                // Add local offset
                let world_pos = world_pos + nalgebra::Vector3::new(lx as i32, ly as i32, lz as i32);

                // Check if this is a frame
                if let Some(model_data) = chunk.get_model_data(lx, ly, lz) {
                    if ModelRegistry::is_frame_model(model_data.model_id) {
                        frame_positions.push(world_pos);
                    }
                }
            }
        }

        // Second pass: update frame clusters and mark chunks dirty
        let mut all_updated_positions: std::collections::HashSet<Vector3<i32>> = std::collections::HashSet::new();
        for world_pos in frame_positions {
            let updated = self.update_frame_cluster(world_pos);
            all_updated_positions.extend(updated);
        }

        // Mark all affected chunks as dirty
        for pos in &all_updated_positions {
            let chunk_pos = Self::world_to_chunk(*pos);
            if let Some(chunk) = self.chunks.get_mut(&chunk_pos) {
                chunk.mark_dirty();
            }
        }
    }
}

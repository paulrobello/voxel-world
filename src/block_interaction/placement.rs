//! Block placement: `place_block_at` dispatcher and per-model-type placement
//! helpers (door, gate, stair, fence, window, glass pane, frame, etc.).

use super::should_place_inverted_stair;
use crate::block_interaction::BlockInteractionContext;
use crate::chunk::{BlockType, WaterType};
use crate::constants::TEXTURE_SIZE_Y;
use crate::placement::BlockPlacementParams;
use crate::sub_voxel::{FIRST_CUSTOM_MODEL_ID, ModelRegistry, StairShape};
use nalgebra::Vector3;

// ── Toggle / repaint helpers ──────────────────────────────────────────────────

impl<'a> BlockInteractionContext<'a> {
    pub fn toggle_gate_at(&mut self, pos: Vector3<i32>) -> bool {
        // Check if target is a Model block
        let Some(BlockType::Model) = self.sim.world.get_block(pos) else {
            return false;
        };

        // Get model data
        let Some(model_data) = self.sim.world.get_model_data(pos) else {
            return false;
        };

        let model_id = model_data.model_id;
        let rotation = model_data.rotation;

        // Check if it's a gate (closed: 20-23, open: 24-27)
        if !(20..=27).contains(&model_id) {
            return false;
        }

        // Calculate new model_id (toggle between closed and open)
        let new_model_id = if model_id < 24 {
            // Closed -> Open: add 4
            model_id + 4
        } else {
            // Open -> Closed: subtract 4
            model_id - 4
        };

        // Update the gate
        self.sim
            .world
            .set_model_block(pos, new_model_id, rotation, model_data.waterlogged);

        true
    }

    /// Toggles a door open/closed. Returns true if door was toggled.
    /// Handles both built-in doors and custom door pairs.
    pub fn toggle_door_at(&mut self, pos: Vector3<i32>) -> bool {
        // Check if target is a Model block
        let Some(BlockType::Model) = self.sim.world.get_block(pos) else {
            return false;
        };

        // Get model data
        let Some(model_data) = self.sim.world.get_model_data(pos) else {
            return false;
        };

        let model_id = model_data.model_id;
        let rotation = model_data.rotation;

        // Check if it's a built-in door
        if ModelRegistry::is_door_model(model_id) {
            // Find the other half of the door
            let other_pos = if ModelRegistry::is_door_upper(model_id) {
                pos + Vector3::new(0, -1, 0) // Upper -> Lower
            } else {
                pos + Vector3::new(0, 1, 0) // Lower -> Upper
            };

            // Get other half's data
            let other_model_data = self.sim.world.get_model_data(other_pos);

            // Toggle this half
            let new_model_id = ModelRegistry::door_toggled(model_id);
            self.sim
                .world
                .set_model_block(pos, new_model_id, rotation, model_data.waterlogged);

            // Toggle other half
            if let Some(other_data) = other_model_data
                && ModelRegistry::is_door_model(other_data.model_id)
            {
                let new_other_id = ModelRegistry::door_toggled(other_data.model_id);
                self.sim.world.set_model_block(
                    other_pos,
                    new_other_id,
                    other_data.rotation,
                    other_data.waterlogged,
                );
            }

            return true;
        }

        // Check if it's a custom door pair
        if self.sim.model_registry.is_custom_door_model(model_id) {
            // Find the other half of the door
            let is_upper = self.sim.model_registry.is_custom_door_upper(model_id);
            let other_pos = if is_upper {
                pos + Vector3::new(0, -1, 0) // Upper -> Lower
            } else {
                pos + Vector3::new(0, 1, 0) // Lower -> Upper
            };

            // Get other half's data
            let other_model_data = self.sim.world.get_model_data(other_pos);

            // Toggle this half
            let new_model_id = self.sim.model_registry.custom_door_toggled(model_id);
            self.sim
                .world
                .set_model_block(pos, new_model_id, rotation, model_data.waterlogged);

            // Toggle other half
            if let Some(other_data) = other_model_data
                && self
                    .sim
                    .model_registry
                    .is_custom_door_model(other_data.model_id)
            {
                let new_other_id = self
                    .sim
                    .model_registry
                    .custom_door_toggled(other_data.model_id);
                self.sim.world.set_model_block(
                    other_pos,
                    new_other_id,
                    other_data.rotation,
                    other_data.waterlogged,
                );
            }

            return true;
        }

        false
    }

    /// Toggles a trapdoor open/closed. Returns true if trapdoor was toggled.
    pub fn toggle_trapdoor_at(&mut self, pos: Vector3<i32>) -> bool {
        // Check if target is a Model block
        let Some(BlockType::Model) = self.sim.world.get_block(pos) else {
            return false;
        };

        // Get model data
        let Some(model_data) = self.sim.world.get_model_data(pos) else {
            return false;
        };

        let model_id = model_data.model_id;
        let rotation = model_data.rotation;

        // Check if it's a trapdoor
        if !ModelRegistry::is_trapdoor_model(model_id) {
            return false;
        }

        // Toggle the trapdoor
        let new_model_id = ModelRegistry::trapdoor_toggled(model_id);
        self.sim
            .world
            .set_model_block(pos, new_model_id, rotation, model_data.waterlogged);

        true
    }

    /// Repaints a Painted block with current hotbar texture/tint/blend. Returns true if repainted.
    pub fn repaint_painted_block_at(&mut self, pos: Vector3<i32>) -> bool {
        // Check if target is a Painted block
        let Some(BlockType::Painted) = self.sim.world.get_block(pos) else {
            return false;
        };

        // Get current hotbar paint settings
        let texture_idx = self.ui.hotbar.hotbar_paint_textures[self.ui.hotbar.hotbar_index];
        let tint_idx = self.ui.hotbar.hotbar_tint_indices[self.ui.hotbar.hotbar_index];
        let blend_mode = self.ui.paint_panel.current_config.blend_mode as u8;

        // Repaint the block with blend mode
        self.sim
            .world
            .set_painted_block_full(pos, texture_idx, tint_idx, blend_mode);

        true
    }

    /// Rotates a custom model 90 degrees around Y axis. Returns true if rotated.
    pub(super) fn rotate_custom_model_at(&mut self, pos: Vector3<i32>) -> bool {
        // Check if there's a model block at this position
        let Some(block) = self.sim.world.get_block(pos) else {
            return false;
        };

        if block != BlockType::Model {
            return false;
        }

        // Get model data
        let Some(model_data) = self.sim.world.get_model_data(pos) else {
            return false;
        };

        // Only rotate custom models (ID >= FIRST_CUSTOM_MODEL_ID)
        if model_data.model_id < FIRST_CUSTOM_MODEL_ID {
            return false;
        }

        // Rotate 90 degrees (increment rotation mod 4)
        let new_rotation = (model_data.rotation + 1) % 4;

        // Update the model with new rotation
        self.sim.world.set_model_block(
            pos,
            model_data.model_id,
            new_rotation,
            model_data.waterlogged,
        );

        true
    }

    pub(super) fn stack_model_at(&mut self, pos: Vector3<i32>) -> bool {
        // Check if there's a model block at this position
        let Some(block) = self.sim.world.get_block(pos) else {
            return false;
        };

        if block != BlockType::Model {
            return false;
        }

        // Get model data
        let Some(model_data) = self.sim.world.get_model_data(pos) else {
            return false;
        };

        // Only stack custom models (ID >= FIRST_CUSTOM_MODEL_ID)
        if model_data.model_id < FIRST_CUSTOM_MODEL_ID {
            return false;
        }

        // Check if we have a Model block selected
        if self.selected_block() != BlockType::Model {
            return false;
        }

        // Get the model ID from the hotbar selection
        let selected_model_id = self.ui.hotbar.hotbar_model_ids[self.ui.hotbar.hotbar_index];

        // Only stack if the selected model is a custom model
        if selected_model_id < FIRST_CUSTOM_MODEL_ID {
            return false;
        }

        // Calculate position above the clicked block
        let stack_pos = pos + Vector3::new(0, 1, 0);

        // Try to place the model on top
        self.place_block_at(stack_pos)
    }

    /// Get block placement parameters from the current hotbar selection.
    pub(crate) fn get_hotbar_placement_params(&self) -> BlockPlacementParams {
        let block_type = self.ui.hotbar.hotbar_blocks[self.ui.hotbar.hotbar_index];
        let tint_index = self.ui.hotbar.hotbar_tint_indices[self.ui.hotbar.hotbar_index];
        let paint_texture = self.ui.hotbar.hotbar_paint_textures[self.ui.hotbar.hotbar_index];
        BlockPlacementParams::new(block_type, tint_index, paint_texture)
    }
}

// ── place_block_at and per-type dispatchers ───────────────────────────────────

impl<'a> BlockInteractionContext<'a> {
    pub fn place_block_at(&mut self, place_pos: Vector3<i32>) -> bool {
        // Bounds check (Y only, X/Z are infinite)
        if place_pos.y < 0 || place_pos.y >= TEXTURE_SIZE_Y as i32 {
            return false;
        }

        if !self.player_aabb_allows_placement(place_pos) {
            return false;
        }

        let block_to_place = self.selected_block();
        let existing_block = self.sim.world.get_block(place_pos);
        let mut waterlogged = false;

        // Check if target position already has a block
        if let Some(existing) = existing_block {
            if existing == BlockType::Water {
                // If placing a model in water, it becomes waterlogged
                if block_to_place == BlockType::Model {
                    waterlogged = true;
                }
                // If placing solid block in water, water is removed (default behavior)
            } else if existing != BlockType::Air {
                return false; // Can't place on non-air (unless water)
            }
        }

        // Dispatch based on block type
        if block_to_place == BlockType::Model {
            self.place_model_block(place_pos, waterlogged)
        } else {
            self.place_simple_block(place_pos, block_to_place, waterlogged)
        }
    }

    /// Returns true if the AABB at `place_pos` does not overlap the player hitbox.
    fn player_aabb_allows_placement(&self, place_pos: Vector3<i32>) -> bool {
        use crate::player::{PLAYER_HALF_WIDTH, PLAYER_HEIGHT};

        let feet = self
            .sim
            .player
            .feet_pos(self.sim.world_extent, self.sim.texture_origin);
        let player_min = Vector3::new(
            feet.x - PLAYER_HALF_WIDTH,
            feet.y,
            feet.z - PLAYER_HALF_WIDTH,
        );
        let player_max = Vector3::new(
            feet.x + PLAYER_HALF_WIDTH,
            feet.y + PLAYER_HEIGHT,
            feet.z + PLAYER_HALF_WIDTH,
        );
        let block_min = place_pos.cast::<f64>();
        let block_max = block_min + Vector3::new(1.0, 1.0, 1.0);

        // Returns true when they do NOT overlap
        !(player_min.x < block_max.x
            && player_max.x > block_min.x
            && player_min.y < block_max.y
            && player_max.y > block_min.y
            && player_min.z < block_max.z
            && player_max.z > block_min.z)
    }

    // ── Model block placement ─────────────────────────────────────────────────

    fn place_model_block(&mut self, place_pos: Vector3<i32>, waterlogged: bool) -> bool {
        let base_model_id = self.ui.hotbar.hotbar_model_ids[self.ui.hotbar.hotbar_index];
        let mut rotation = 0u8;
        let mut custom_data: u32 = 0;

        // Determine final model_id based on type and connections
        let model_id = if ModelRegistry::is_fence_model(base_model_id)
            || (4..20).contains(&base_model_id)
        {
            self.place_fence(place_pos, waterlogged)
        } else if ModelRegistry::is_gate_model(base_model_id) || (20..28).contains(&base_model_id) {
            return self.place_gate(place_pos, waterlogged);
        } else if ModelRegistry::is_ladder_model(base_model_id) {
            return self.place_ladder(place_pos, base_model_id, waterlogged);
        } else if ModelRegistry::is_stairs_model(base_model_id) {
            let (id, rot) = self.resolve_stair_placement(base_model_id);
            rotation = rot;
            id
        } else if ModelRegistry::is_door_model(base_model_id) {
            return self.place_door(place_pos, base_model_id, waterlogged);
        } else if ModelRegistry::is_trapdoor_model(base_model_id) {
            let (id, rot) = self.resolve_trapdoor_placement(base_model_id, waterlogged);
            rotation = rot;
            // place_trapdoor returns early after setting model
            self.sim
                .world
                .set_model_block(place_pos, id, rotation, waterlogged);
            return true;
        } else if ModelRegistry::is_window_model(base_model_id) {
            return self.place_window(place_pos, waterlogged);
        } else if ModelRegistry::is_horizontal_glass_pane_model(base_model_id) {
            return self.place_horizontal_pane(place_pos, waterlogged);
        } else if ModelRegistry::is_vertical_glass_pane_model(base_model_id) {
            return self.place_vertical_pane(place_pos, waterlogged);
        } else if ModelRegistry::is_frame_model(base_model_id) {
            let (id, rot, data) = self.resolve_frame_placement(place_pos, base_model_id);
            rotation = rot;
            custom_data = data;
            id
        } else if base_model_id >= FIRST_CUSTOM_MODEL_ID {
            let yaw = self.sim.player.camera.rotation.y as f32;
            let rot = (yaw / std::f32::consts::FRAC_PI_2).round() as i32;
            rotation = rot.rem_euclid(4) as u8;
            base_model_id
        } else {
            base_model_id
        };

        // Use extended setter if custom_data is present (frames), otherwise regular.
        if custom_data != 0 {
            self.sim.world.set_model_block_with_data(
                place_pos,
                model_id,
                rotation,
                waterlogged,
                custom_data,
            );
        } else {
            self.sim
                .world
                .set_model_block(place_pos, model_id, rotation, waterlogged);
        }

        if ModelRegistry::is_fence_or_gate(model_id) {
            self.sim.world.update_fence_connections(place_pos);
        } else if ModelRegistry::is_stairs_model(model_id) {
            // Update placed stair and neighbors to form corners
            self.sim.world.update_stair_and_neighbors(place_pos);
        } else if ModelRegistry::is_frame_model(model_id) {
            self.sim.world.update_adjacent_frame_clusters(place_pos);
        }

        // Post-placement effects shared with the simple block path
        self.apply_post_placement_effects(place_pos, BlockType::Model, waterlogged);
        true
    }

    fn place_fence(&mut self, place_pos: Vector3<i32>, _waterlogged: bool) -> u8 {
        let connections = self.sim.world.calculate_fence_connections(place_pos);
        ModelRegistry::fence_model_id(connections)
    }

    fn place_gate(&mut self, place_pos: Vector3<i32>, waterlogged: bool) -> bool {
        let has_west = self
            .sim
            .world
            .is_fence_connectable(place_pos + Vector3::new(-1, 0, 0));
        let has_east = self
            .sim
            .world
            .is_fence_connectable(place_pos + Vector3::new(1, 0, 0));
        let has_north = self
            .sim
            .world
            .is_fence_connectable(place_pos + Vector3::new(0, 0, -1));
        let has_south = self
            .sim
            .world
            .is_fence_connectable(place_pos + Vector3::new(0, 0, 1));

        let player_pos = self
            .sim
            .player
            .feet_pos(self.sim.world_extent, self.sim.texture_origin);
        let gate_center = place_pos.cast::<f64>() + Vector3::new(0.5, 0.0, 0.5);
        let to_player = player_pos - gate_center;

        let (connections, rotation) = if (has_north || has_south) && !has_west && !has_east {
            let mut conn = 0u8;
            if has_north {
                conn |= 1;
            }
            if has_south {
                conn |= 2;
            }
            let rot = if to_player.x < 0.0 { 1u8 } else { 3u8 };
            (conn, rot)
        } else {
            let connections = self.sim.world.calculate_gate_connections(place_pos);
            let rot = if to_player.z < 0.0 { 0u8 } else { 2u8 };
            (connections, rot)
        };

        self.sim.world.set_model_block(
            place_pos,
            ModelRegistry::gate_closed_model_id(connections),
            rotation,
            waterlogged,
        );

        self.sim.world.update_fence_connections(place_pos);
        true
    }

    fn place_ladder(
        &mut self,
        place_pos: Vector3<i32>,
        base_model_id: u8,
        waterlogged: bool,
    ) -> bool {
        let player_pos = self
            .sim
            .player
            .feet_pos(self.sim.world_extent, self.sim.texture_origin);
        let ladder_center = place_pos.cast::<f64>() + Vector3::new(0.5, 0.0, 0.5);
        let to_player = player_pos - ladder_center;

        let rotation = if to_player.x.abs() > to_player.z.abs() {
            if to_player.x > 0.0 { 3 } else { 1 }
        } else if to_player.z > 0.0 {
            2
        } else {
            0
        };

        self.sim
            .world
            .set_model_block(place_pos, base_model_id, rotation, waterlogged);
        true
    }

    fn resolve_stair_placement(&mut self, base_model_id: u8) -> (u8, u8) {
        let yaw = self.sim.player.camera.rotation.y as f32;
        let rot = (yaw / std::f32::consts::FRAC_PI_2).round() as i32;
        let rotation = ((rot + 2).rem_euclid(4)) as u8;

        let mut inverted = ModelRegistry::is_stairs_inverted(base_model_id);
        if !inverted && let Some(hit) = self.ui.placement.current_hit {
            let local_y = if hit.normal.y == 0 {
                let origin = self
                    .sim
                    .player
                    .camera_world_pos(self.sim.world_extent, self.sim.texture_origin)
                    .cast::<f32>();
                let direction = self.sim.player.camera_direction().cast::<f32>();
                let hit_point = origin + direction * hit.distance;
                hit_point.y - hit_point.y.floor()
            } else {
                0.0
            };
            inverted = should_place_inverted_stair(hit.normal.y, local_y);
        }

        let shape = ModelRegistry::stairs_shape(base_model_id).unwrap_or(StairShape::Straight);
        let id = ModelRegistry::stairs_model_id(shape, inverted);
        (id, rotation)
    }

    fn place_door(
        &mut self,
        place_pos: Vector3<i32>,
        base_model_id: u8,
        waterlogged: bool,
    ) -> bool {
        let upper_pos = place_pos + Vector3::new(0, 1, 0);

        // Check upper position is valid and empty
        if upper_pos.y >= TEXTURE_SIZE_Y as i32 {
            return false;
        }
        let upper_block = self.sim.world.get_block(upper_pos);
        if let Some(b) = upper_block
            && b != BlockType::Air
            && b != BlockType::Water
        {
            return false;
        }

        let yaw = self.sim.player.camera.rotation.y as f32;
        let rot = (yaw / std::f32::consts::FRAC_PI_2).round() as i32;
        let mut base_rotation = rot.rem_euclid(4) as u8;

        if let Some(hit) = self.ui.placement.current_hit {
            let player_pos = self
                .sim
                .player
                .camera_world_pos(self.sim.world_extent, self.sim.texture_origin)
                .cast::<f32>();
            let direction = self.sim.player.camera_direction().cast::<f32>();
            let hit_point = player_pos + direction * hit.distance;
            let local_hit = hit_point - place_pos.cast::<f32>();

            let place_at_far_edge = match (hit.normal.x, hit.normal.y, hit.normal.z) {
                (_, 1, _) | (_, -1, _) => match base_rotation {
                    0 => local_hit.z > 0.5,
                    1 => local_hit.x < 0.5,
                    2 => local_hit.z < 0.5,
                    _ => local_hit.x > 0.5,
                },
                (1, _, _) | (-1, _, _) => match base_rotation {
                    0 => local_hit.z > 0.5,
                    2 => local_hit.z < 0.5,
                    _ => false,
                },
                (_, _, 1) | (_, _, -1) => match base_rotation {
                    1 => local_hit.x < 0.5,
                    3 => local_hit.x > 0.5,
                    _ => false,
                },
                _ => false,
            };

            if place_at_far_edge {
                base_rotation = (base_rotation + 2) % 4;
            }
        }

        let rotation = base_rotation;

        let (left_offset, right_offset) = match rotation {
            0 => (Vector3::new(-1, 0, 0), Vector3::new(1, 0, 0)),
            1 => (Vector3::new(0, 0, -1), Vector3::new(0, 0, 1)),
            2 => (Vector3::new(1, 0, 0), Vector3::new(-1, 0, 0)),
            _ => (Vector3::new(0, 0, 1), Vector3::new(0, 0, -1)),
        };

        let left_solid = self
            .sim
            .world
            .get_block(place_pos + left_offset)
            .map(|b| b.is_solid())
            .unwrap_or(false);
        let right_solid = self
            .sim
            .world
            .get_block(place_pos + right_offset)
            .map(|b| b.is_solid())
            .unwrap_or(false);

        let hinge_left = !right_solid || left_solid;
        let door_base = ModelRegistry::door_type_base(base_model_id).unwrap_or(39);

        let lower_model =
            ModelRegistry::door_model_id_with_base(door_base, false, hinge_left, false);
        self.sim
            .world
            .set_model_block(place_pos, lower_model, rotation, waterlogged);

        let upper_model =
            ModelRegistry::door_model_id_with_base(door_base, true, hinge_left, false);
        let upper_waterlogged = upper_block == Some(BlockType::Water);
        self.sim
            .world
            .set_model_block(upper_pos, upper_model, rotation, upper_waterlogged);

        self.ui.placement.gate_needs_reclick = true;
        true
    }

    fn resolve_trapdoor_placement(&mut self, _base_model_id: u8, _waterlogged: bool) -> (u8, u8) {
        let yaw = self.sim.player.camera.rotation.y as f32;
        let rot = (yaw / std::f32::consts::FRAC_PI_2).round() as i32;
        let rotation = rot.rem_euclid(4) as u8;

        let is_ceiling = if let Some(hit) = self.ui.placement.current_hit {
            if hit.normal.y < 0 {
                true
            } else if hit.normal.y == 0 {
                let origin = self
                    .sim
                    .player
                    .camera_world_pos(self.sim.world_extent, self.sim.texture_origin)
                    .cast::<f32>();
                let direction = self.sim.player.camera_direction().cast::<f32>();
                let hit_point = origin + direction * hit.distance;
                let local_y = hit_point.y - hit_point.y.floor();
                local_y >= 0.5
            } else {
                false
            }
        } else {
            false
        };

        let model_id = ModelRegistry::trapdoor_model_id(is_ceiling, false);
        (model_id, rotation)
    }

    fn place_window(&mut self, place_pos: Vector3<i32>, waterlogged: bool) -> bool {
        let connections = self.sim.world.calculate_window_connections(place_pos);
        let model_id = ModelRegistry::window_model_id(connections);
        self.sim
            .world
            .set_model_block(place_pos, model_id, 0, waterlogged);
        self.sim.world.update_window_connections(place_pos);
        true
    }

    fn place_horizontal_pane(&mut self, place_pos: Vector3<i32>, waterlogged: bool) -> bool {
        let connections = self
            .sim
            .world
            .calculate_horizontal_pane_connections(place_pos);
        let model_id = ModelRegistry::horizontal_glass_pane_model_id(connections);
        self.sim
            .world
            .set_model_block(place_pos, model_id, 0, waterlogged);
        self.sim.world.update_horizontal_pane_connections(place_pos);
        true
    }

    fn place_vertical_pane(&mut self, place_pos: Vector3<i32>, waterlogged: bool) -> bool {
        let rotation = if let Some(hit) = self.ui.placement.current_hit {
            match (hit.normal.x, hit.normal.z) {
                (1, 0) | (-1, 0) => 1u8,
                (0, 1) | (0, -1) => 0u8,
                _ => {
                    let yaw = self.sim.player.camera.rotation.y as f32;
                    let rot = (yaw / std::f32::consts::FRAC_PI_2).round() as i32;
                    (rot.rem_euclid(2)) as u8
                }
            }
        } else {
            0u8
        };
        let connections = self
            .sim
            .world
            .calculate_vertical_pane_connections(place_pos, rotation);
        let model_id = ModelRegistry::vertical_glass_pane_model_id(connections);
        self.sim
            .world
            .set_model_block(place_pos, model_id, rotation, waterlogged);
        self.sim.world.update_vertical_pane_connections(place_pos);
        true
    }

    fn resolve_frame_placement(
        &mut self,
        place_pos: Vector3<i32>,
        _base_model_id: u8,
    ) -> (u8, u8, u32) {
        use crate::sub_voxel::builtins::frames;

        log::warn!("[DEBUG] Placing frame model, place_pos={:?}", place_pos);

        // Derive facing from hit normal for stable orientation per wall.
        // BUT: if placing adjacent to an existing frame, use that frame's facing.
        let mut facing = if let Some(hit) = self.ui.placement.current_hit {
            let n = hit.normal;
            if n.z < 0 {
                0
            } else if n.z > 0 {
                2
            } else if n.x < 0 {
                3
            } else if n.x > 0 {
                1
            } else {
                0
            }
        } else {
            0
        };

        // Check if we're placing adjacent to an existing frame - if so, use its facing
        let neighbors = [
            place_pos + nalgebra::Vector3::new(1, 0, 0),
            place_pos + nalgebra::Vector3::new(-1, 0, 0),
            place_pos + nalgebra::Vector3::new(0, 1, 0),
            place_pos + nalgebra::Vector3::new(0, -1, 0),
            place_pos + nalgebra::Vector3::new(0, 0, 1),
            place_pos + nalgebra::Vector3::new(0, 0, -1),
        ];

        for neighbor_pos in neighbors {
            if let Some(BlockType::Model) = self.sim.world.get_block(neighbor_pos)
                && let Some(md) = self.sim.world.get_model_data(neighbor_pos)
                && ModelRegistry::is_frame_model(md.model_id)
            {
                let neighbor_facing = frames::metadata::decode_facing(md.custom_data);
                facing = neighbor_facing;
                break;
            }
        }

        let right = crate::world::World::frame_right_vec(facing);
        let up = nalgebra::Vector3::new(0, 1, 0);

        let mut max_w = 1u8;
        let mut max_h = 1u8;
        for dx in 0..frames::MAX_FRAME_DIM {
            for dy in 0..frames::MAX_FRAME_DIM {
                let check_pos = place_pos + right * dx as i32 + up * dy as i32;
                if let Some(BlockType::Model) = self.sim.world.get_block(check_pos)
                    && let Some(md) = self.sim.world.get_model_data(check_pos)
                    && ModelRegistry::is_frame_model(md.model_id)
                {
                    max_w = max_w.max(dx + 1);
                    max_h = max_h.max(dy + 1);
                }
            }
        }

        let picture_id = self.ui.picture_state.selected_picture_id.unwrap_or(0);
        let custom_data = frames::metadata::encode(picture_id, 0, 0, max_w, max_h, facing);

        let rotation = facing & 0x03;
        let edge_mask: u8 = 0x0F;
        let model_id = frames::edge_mask_to_frame_model_id(edge_mask);

        log::warn!(
            "[FRAME PLACE] Wall: facing={}, rotation={}, model_id={}",
            facing,
            rotation,
            model_id
        );

        (model_id, rotation, custom_data)
    }

    // ── Simple block placement ────────────────────────────────────────────────

    fn place_simple_block(
        &mut self,
        place_pos: Vector3<i32>,
        block_to_place: BlockType,
        waterlogged: bool,
    ) -> bool {
        if block_to_place == BlockType::TintedGlass {
            let tint_index = self.ui.hotbar.hotbar_tint_indices[self.ui.hotbar.hotbar_index];
            self.sim.world.set_tinted_glass_block(place_pos, tint_index);
        } else if block_to_place == BlockType::Crystal {
            let tint_index = self.ui.hotbar.hotbar_tint_indices[self.ui.hotbar.hotbar_index];
            self.sim.world.set_crystal_block(place_pos, tint_index);
        } else if block_to_place == BlockType::Painted {
            let texture_idx = self.ui.hotbar.hotbar_paint_textures[self.ui.hotbar.hotbar_index];
            let tint_idx = self.ui.hotbar.hotbar_tint_indices[self.ui.hotbar.hotbar_index];
            let blend_mode = self.ui.paint_panel.current_config.blend_mode as u8;
            self.sim
                .world
                .set_painted_block_full(place_pos, texture_idx, tint_idx, blend_mode);
        } else {
            self.sim.world.set_block(place_pos, block_to_place);

            if block_to_place.is_solid() {
                self.sim.world.update_fence_connections(place_pos);
                self.sim.world.update_pane_connections(place_pos);
            }
        }

        self.apply_post_placement_effects(place_pos, block_to_place, waterlogged);
        true
    }

    /// Shared post-placement effects: minimap invalidation, water/lava grid
    /// update, and multiplayer sync. Called after any block voxel write succeeds.
    fn apply_post_placement_effects(
        &mut self,
        place_pos: Vector3<i32>,
        block_to_place: BlockType,
        waterlogged: bool,
    ) {
        self.sim
            .world
            .invalidate_minimap_cache(place_pos.x, place_pos.z);

        // Handle water/lava grid updates
        if block_to_place == BlockType::Water {
            let water_type =
                WaterType::from_u8(self.ui.hotbar.hotbar_tint_indices[self.ui.hotbar.hotbar_index]);
            self.sim.water_grid.place_source(place_pos, water_type);
            self.sim.world.set_water_block(place_pos, water_type);
            self.sync_water_source([place_pos.x, place_pos.y, place_pos.z], water_type);
        } else if waterlogged {
            // Ensure water grid knows about waterlogged block (if not already there)
            if !self.sim.water_grid.has_water(place_pos) {
                let water_type = self
                    .sim
                    .world
                    .get_water_type(place_pos)
                    .unwrap_or(WaterType::Ocean);
                self.sim.water_grid.place_source(place_pos, water_type);
                self.sync_water_source([place_pos.x, place_pos.y, place_pos.z], water_type);
            }
        } else {
            self.sim.water_grid.on_block_placed(place_pos);
        }

        if block_to_place == BlockType::Lava {
            self.sim.lava_grid.place_source(place_pos);
        } else {
            self.sim.lava_grid.on_block_placed(place_pos);
        }

        // Sync block placement to server in multiplayer mode
        if self.is_multiplayer() {
            let block_data = crate::net::protocol::BlockData {
                block_type: block_to_place,
                model_data: if block_to_place == BlockType::Model {
                    self.sim.world.get_model_data(place_pos)
                } else {
                    None
                },
                paint_data: if block_to_place == BlockType::Painted {
                    self.sim.world.get_paint_data(place_pos)
                } else {
                    None
                },
                tint_index: if block_to_place == BlockType::TintedGlass
                    || block_to_place == BlockType::Crystal
                {
                    Some(self.ui.hotbar.hotbar_tint_indices[self.ui.hotbar.hotbar_index])
                } else {
                    None
                },
                water_type: if block_to_place == BlockType::Water {
                    self.sim.world.get_water_type(place_pos)
                } else {
                    None
                },
            };
            self.sync_block_placement([place_pos.x, place_pos.y, place_pos.z], block_data);
        }
    }
}

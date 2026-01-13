use crate::App;
use crate::block_update::BlockUpdateType;
use crate::chunk::{BlockType, WaterType};
use crate::constants::TEXTURE_SIZE_Y;
use crate::player::{PLAYER_HALF_WIDTH, PLAYER_HEIGHT};
use crate::raycast::{MAX_RAYCAST_DISTANCE, get_place_position, raycast};
use crate::sub_voxel::{FIRST_CUSTOM_MODEL_ID, ModelRegistry, StairShape};
use nalgebra::Vector3;
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

impl App {
    pub fn update_raycast(&mut self) {
        // Camera uses normalized texture-relative coords (0-1), raycast needs world coords
        let origin = self
            .sim
            .player
            .camera_world_pos(self.sim.world_extent, self.sim.texture_origin)
            .cast::<f32>();
        let direction = self.sim.player.camera_direction().cast::<f32>();

        self.ui.current_hit = raycast(
            &self.sim.world,
            &self.sim.model_registry,
            origin,
            direction,
            MAX_RAYCAST_DISTANCE,
        );
    }

    pub fn update_block_breaking(&mut self, delta_time: f32, holding_break: bool) -> bool {
        // Decrement cooldown timer
        if self.ui.break_cooldown > 0.0 {
            self.ui.break_cooldown -= delta_time;
            if self.ui.break_cooldown < 0.0 {
                self.ui.break_cooldown = 0.0;
            }
        }

        // Get the block we're looking at
        let target_block = self.ui.current_hit.as_ref().map(|hit| hit.block_pos);

        // If not holding break button or not looking at anything, reset
        if !holding_break || target_block.is_none() {
            self.ui.breaking_block = None;
            self.ui.break_progress = 0.0;
            return false;
        }

        // Don't start breaking if on cooldown (instant break mode)
        if self.ui.settings.instant_break && self.ui.break_cooldown > 0.0 {
            return false;
        }

        let target = target_block.unwrap();

        // Get the block type to determine break time
        let block_type = self.sim.world.get_block(target).unwrap_or(BlockType::Air);
        let break_time = block_type.break_time();

        // Can't break air or water
        if break_time <= 0.0 {
            self.ui.breaking_block = None;
            self.ui.break_progress = 0.0;
            return false;
        }

        // If we're looking at a different block, reset progress
        if self.ui.breaking_block != Some(target) {
            self.ui.breaking_block = Some(target);
            self.ui.break_progress = 0.0;
        }

        // Increment break progress (instant if enabled)
        if self.ui.settings.instant_break {
            self.ui.break_progress = 1.0;
        } else {
            self.ui.break_progress += delta_time / break_time;
        }

        // Check if block is fully broken
        if self.ui.break_progress >= 1.0 {
            // Bounds check (Y only - X/Z are infinite)
            if target.y >= 0 && target.y < TEXTURE_SIZE_Y as i32 {
                // Get block color for particles before breaking
                if let Some(block_type) = self.sim.world.get_block(target) {
                    let color = block_type.color();
                    let mut particle_color = nalgebra::Vector3::new(color[0], color[1], color[2]);

                    // Apply tint color for TintedGlass and Crystal blocks
                    if block_type == BlockType::TintedGlass || block_type == BlockType::Crystal {
                        if let Some(tint_index) = self.sim.world.get_tint_index(target) {
                            let tint = crate::chunk::tint_color(tint_index);
                            particle_color = nalgebra::Vector3::new(tint[0], tint[1], tint[2]);
                        }
                    } else if block_type == BlockType::Painted {
                        if let Some(paint) = self.sim.world.get_paint_data(target) {
                            let tint = crate::chunk::tint_color(paint.tint_idx);
                            particle_color = nalgebra::Vector3::new(tint[0], tint[1], tint[2]);
                        }
                    }

                    self.sim
                        .particles
                        .spawn_block_break(target.cast::<f32>(), particle_color);
                }

                let is_waterlogged = if block_type == BlockType::Model {
                    self.sim
                        .world
                        .get_model_data(target)
                        .map(|d| d.waterlogged)
                        .unwrap_or(false)
                } else {
                    false
                };

                // Check if breaking a door - need to break both halves
                let mut other_door_half: Option<Vector3<i32>> = None;
                if block_type == BlockType::Model {
                    if let Some(model_data) = self.sim.world.get_model_data(target) {
                        if ModelRegistry::is_door_model(model_data.model_id) {
                            other_door_half = if ModelRegistry::is_door_upper(model_data.model_id) {
                                Some(target + Vector3::new(0, -1, 0))
                            } else {
                                Some(target + Vector3::new(0, 1, 0))
                            };
                        }
                    }
                }

                if is_waterlogged {
                    self.sim.world.set_block(target, BlockType::Water);
                } else {
                    self.sim.world.set_block(target, BlockType::Air);
                }
                self.sim.world.invalidate_minimap_cache(target.x, target.z);

                // Break other door half if present
                if let Some(other_pos) = other_door_half {
                    if other_pos.y >= 0 && other_pos.y < TEXTURE_SIZE_Y as i32 {
                        if let Some(BlockType::Model) = self.sim.world.get_block(other_pos) {
                            if let Some(other_data) = self.sim.world.get_model_data(other_pos) {
                                if ModelRegistry::is_door_model(other_data.model_id) {
                                    if other_data.waterlogged {
                                        self.sim.world.set_block(other_pos, BlockType::Water);
                                    } else {
                                        self.sim.world.set_block(other_pos, BlockType::Air);
                                    }
                                    self.sim
                                        .world
                                        .invalidate_minimap_cache(other_pos.x, other_pos.z);
                                }
                            }
                        }
                    }
                }

                // Update neighboring fence/gate connections
                self.sim.world.update_fence_connections(target);
                // Update neighboring window connections
                self.sim.world.update_window_connections(target);
                // Update neighboring stair shapes (stair neighbors may straighten)
                self.sim.world.update_adjacent_stair_shapes(target);
                // Update neighboring stair corner shapes
                self.sim.world.update_adjacent_stair_shapes(target);

                // Notify water grid that a block was removed (may trigger flow)
                self.sim.water_grid.on_block_removed(target);

                // Check if any adjacent terrain water should start flowing
                self.sim
                    .water_grid
                    .activate_adjacent_terrain_water(&self.sim.world, target);

                // Notify lava grid that a block was removed (may trigger flow)
                self.sim.lava_grid.on_block_removed(target);

                // Check if any adjacent terrain lava should start flowing
                self.sim
                    .lava_grid
                    .activate_adjacent_terrain_lava(&self.sim.world, target);

                // Queue physics checks (frame-distributed to prevent FPS spikes)
                let player_pos = self
                    .sim
                    .player
                    .feet_pos(self.sim.world_extent, self.sim.texture_origin)
                    .cast::<f32>();

                // Queue gravity check for block above
                self.sim.block_updates.enqueue(
                    target + Vector3::new(0, 1, 0),
                    BlockUpdateType::Gravity,
                    player_pos,
                );

                // Queue ground support check for model block above (fences, torches, gates)
                self.sim.block_updates.enqueue(
                    target + Vector3::new(0, 1, 0),
                    BlockUpdateType::ModelGroundSupport,
                    player_pos,
                );

                // Queue tree support checks for all nearby logs
                if block_type.is_log() {
                    self.sim.block_updates.enqueue_neighbors(
                        target,
                        BlockUpdateType::TreeSupport,
                        player_pos,
                    );
                }
                self.sim.block_updates.enqueue_radius(
                    target,
                    3,
                    BlockUpdateType::TreeSupport,
                    player_pos,
                );

                // Queue orphaned leaves checks
                self.sim.block_updates.enqueue_radius(
                    target,
                    4,
                    BlockUpdateType::OrphanedLeaves,
                    player_pos,
                );
            }

            // Reset for next block
            self.ui.breaking_block = None;
            self.ui.break_progress = 0.0;

            // Set cooldown for instant break mode
            if self.ui.settings.instant_break {
                self.ui.break_cooldown = self.ui.settings.break_cooldown_duration;
            }

            return true;
        }

        false
    }

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

        // Check if it's a door
        if !ModelRegistry::is_door_model(model_id) {
            return false;
        }

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
        if let Some(other_data) = other_model_data {
            if ModelRegistry::is_door_model(other_data.model_id) {
                let new_other_id = ModelRegistry::door_toggled(other_data.model_id);
                self.sim.world.set_model_block(
                    other_pos,
                    new_other_id,
                    other_data.rotation,
                    other_data.waterlogged,
                );
            }
        }

        true
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

    /// Repaints a Painted block with current hotbar texture/tint. Returns true if repainted.
    pub fn repaint_painted_block_at(&mut self, pos: Vector3<i32>) -> bool {
        // Check if target is a Painted block
        let Some(BlockType::Painted) = self.sim.world.get_block(pos) else {
            return false;
        };

        // Get current hotbar paint settings
        let texture_idx = self.ui.hotbar_paint_textures[self.ui.hotbar_index];
        let tint_idx = self.ui.hotbar_tint_indices[self.ui.hotbar_index];

        // Repaint the block
        self.sim.world.set_painted_block(pos, texture_idx, tint_idx);

        true
    }

    /// Rotates a custom model 90 degrees around Y axis. Returns true if rotated.
    fn rotate_custom_model_at(&mut self, pos: Vector3<i32>) -> bool {
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

    fn stack_model_at(&mut self, pos: Vector3<i32>) -> bool {
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
        let selected_model_id = self.ui.hotbar_model_ids[self.ui.hotbar_index];

        // Only stack if the selected model is a custom model
        if selected_model_id < FIRST_CUSTOM_MODEL_ID {
            return false;
        }

        // Calculate position above the clicked block
        let stack_pos = pos + Vector3::new(0, 1, 0);

        // Try to place the model on top
        self.place_block_at(stack_pos)
    }

    pub fn update_block_placing(&mut self, delta_time: f32) {
        // Skip block placement if in template/stencil placement mode, selection mode, rangefinder, flood fill, or shape tools
        if self.ui.active_placement.is_some()
            || self.ui.active_stencil_placement.is_some()
            || self.ui.template_selection.visual_mode
            || self.ui.rangefinder_active
            || self.ui.flood_fill_active
            || self.ui.sphere_tool.active
            || self.ui.cube_tool.active
            || self.ui.bridge_tool.active
        {
            return;
        }

        // Decrease cooldown
        if self.ui.place_cooldown > 0.0 {
            self.ui.place_cooldown -= delta_time;
        }

        let holding_place = self.input.mouse_held(MouseButton::Right);

        if !holding_place {
            // Reset all line building state when mouse released
            self.ui.last_place_pos = None;
            self.ui.line_start_pos = None;
            self.ui.line_locked_axis = None;
            self.ui.place_needs_reclick = false; // Allow block placement on next click
            self.ui.model_needs_reclick = false; // Allow model placement on next click
            self.ui.gate_needs_reclick = false; // Allow gate toggle on next click
            self.ui.custom_rotate_needs_reclick = false; // Allow custom model rotation on next click
            return;
        }

        // If instant_place is disabled, require mouse release between placements
        if self.ui.place_needs_reclick {
            return;
        }

        // If we just toggled a gate this hold, require a release before placing or toggling again.
        if self.ui.gate_needs_reclick {
            return;
        }

        // If we just rotated a custom model, require a release before rotating again.
        if self.ui.custom_rotate_needs_reclick {
            return;
        }

        if let Some(hit) = self.ui.current_hit {
            // Check shift state early for custom model interactions
            let shift_held =
                self.input.key_held(KeyCode::ShiftLeft) || self.input.key_held(KeyCode::ShiftRight);

            // Priority 1a: Rotate existing custom model (Shift+Right-Click)
            if shift_held && self.rotate_custom_model_at(hit.block_pos) {
                self.ui.custom_rotate_needs_reclick = true;
                return;
            }

            // Priority 1b: Stack model on top of existing custom model (Right-Click without Shift)
            // Check model_needs_reclick to prevent double-placement
            if !shift_held && !self.ui.model_needs_reclick && self.stack_model_at(hit.block_pos) {
                // Don't set place_needs_reclick - let place_block_at handle model_needs_reclick
                return;
            }

            // Priority 2: Toggle existing door
            if !self.ui.gate_needs_reclick && self.toggle_door_at(hit.block_pos) {
                self.ui.gate_needs_reclick = true;
                return;
            }

            // Priority 3: Toggle existing trapdoor
            if !self.ui.gate_needs_reclick && self.toggle_trapdoor_at(hit.block_pos) {
                self.ui.gate_needs_reclick = true;
                return;
            }

            // Priority 4: Toggle existing gate
            if !self.ui.gate_needs_reclick && self.toggle_gate_at(hit.block_pos) {
                self.ui.gate_needs_reclick = true;
                return;
            }

            // Priority 5: Repaint existing Painted block (Shift+Right-Click while holding Painted)
            if self.selected_block() == BlockType::Painted
                && shift_held
                && !self.ui.gate_needs_reclick
                && self.repaint_painted_block_at(hit.block_pos)
            {
                self.ui.gate_needs_reclick = true;
                return;
            }

            // Priority 6: Place new block
            let place_pos = get_place_position(&hit);

            // Handle model blocks - require re-click to place multiple
            if self.selected_block() == BlockType::Model && self.ui.model_needs_reclick {
                return;
            }

            // Handle line building (lock to axis)
            let mut constrained_pos = place_pos;
            if let Some(start) = self.ui.line_start_pos {
                if let Some(axis) = self.ui.line_locked_axis {
                    // Lock to the established axis
                    match axis {
                        0 => {
                            constrained_pos.y = start.y;
                            constrained_pos.z = start.z;
                        }
                        1 => {
                            constrained_pos.x = start.x;
                            constrained_pos.z = start.z;
                        }
                        2 => {
                            constrained_pos.x = start.x;
                            constrained_pos.y = start.y;
                        }
                        _ => {}
                    }
                } else {
                    // Try to establish an axis
                    let diff = place_pos - start;
                    if diff.x.abs() > 0 {
                        self.ui.line_locked_axis = Some(0);
                        constrained_pos.y = start.y;
                        constrained_pos.z = start.z;
                    } else if diff.y.abs() > 0 {
                        self.ui.line_locked_axis = Some(1);
                        constrained_pos.x = start.x;
                        constrained_pos.z = start.z;
                    } else if diff.z.abs() > 0 {
                        self.ui.line_locked_axis = Some(2);
                        constrained_pos.x = start.x;
                        constrained_pos.y = start.y;
                    }
                }
            } else {
                // First block in a potential line
                self.ui.line_start_pos = Some(place_pos);
            }

            // Continuous placing logic
            let can_place_new =
                self.ui.last_place_pos != Some(constrained_pos) && self.ui.place_cooldown <= 0.0;

            if can_place_new && self.place_block_at(constrained_pos) {
                // Only advance state when we actually placed a block
                self.ui.last_place_pos = Some(constrained_pos);
                self.ui.place_cooldown = self.ui.settings.place_cooldown_duration;

                // Require re-click if instant_place is disabled
                if !self.ui.settings.instant_place {
                    self.ui.place_needs_reclick = true;
                }
                // Model blocks require re-click, except fences which can be placed rapidly
                if self.selected_block() == BlockType::Model {
                    let model_id = self.ui.hotbar_model_ids[self.ui.hotbar_index];
                    if !ModelRegistry::is_fence_model(model_id) {
                        self.ui.model_needs_reclick = true;
                    }
                }
            }
        }
    }

    pub fn place_block_at(&mut self, place_pos: Vector3<i32>) -> bool {
        // Bounds check (Y only, X/Z are infinite)
        if place_pos.y < 0 || place_pos.y >= TEXTURE_SIZE_Y as i32 {
            return false;
        }

        // Check if block would overlap with player hitbox (AABB collision)
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

        // AABB overlap check
        let overlaps = player_min.x < block_max.x
            && player_max.x > block_min.x
            && player_min.y < block_max.y
            && player_max.y > block_min.y
            && player_min.z < block_max.z
            && player_max.z > block_min.z;

        if overlaps {
            return false; // Can't place block inside player
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

        // Handle model blocks specially - set both block type and metadata
        if block_to_place == BlockType::Model {
            let base_model_id = self.ui.hotbar_model_ids[self.ui.hotbar_index];
            let mut rotation = 0u8;

            // Determine final model_id based on type and connections
            let model_id = if ModelRegistry::is_fence_model(base_model_id)
                || (4..20).contains(&base_model_id)
            {
                // Fence: calculate connections and get correct variant
                let connections = self.sim.world.calculate_fence_connections(place_pos);
                ModelRegistry::fence_model_id(connections)
            } else if ModelRegistry::is_gate_model(base_model_id)
                || (20..28).contains(&base_model_id)
            {
                // Gate: auto-detect orientation based on neighboring fences
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

                // Calculate player position relative to gate for open direction
                let player_pos = self
                    .sim
                    .player
                    .feet_pos(self.sim.world_extent, self.sim.texture_origin);
                let gate_center = place_pos.cast::<f64>() + Vector3::new(0.5, 0.0, 0.5);
                let to_player = player_pos - gate_center;

                let (connections, rotation) = if (has_north || has_south) && !has_west && !has_east
                {
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
                return true;
            } else if ModelRegistry::is_ladder_model(base_model_id) {
                let player_pos = self
                    .sim
                    .player
                    .feet_pos(self.sim.world_extent, self.sim.texture_origin);
                let ladder_center = place_pos.cast::<f64>() + Vector3::new(0.5, 0.0, 0.5);
                let to_player = player_pos - ladder_center;

                let rotation = if to_player.x.abs() > to_player.z.abs() {
                    // Player is more to E/W
                    if to_player.x > 0.0 { 3 } else { 1 } // Face +X or -X
                } else if to_player.z > 0.0 {
                    2
                } else {
                    0
                };

                self.sim
                    .world
                    .set_model_block(place_pos, base_model_id, rotation, waterlogged);
                return true;
            } else if ModelRegistry::is_stairs_model(base_model_id) {
                // Stairs: determine rotation from player yaw
                // Step (low side) faces toward player
                let yaw = self.sim.player.camera.rotation.y as f32;
                let rot = (yaw / std::f32::consts::FRAC_PI_2).round() as i32;
                rotation = ((rot + 2).rem_euclid(4)) as u8;

                // Determine if inverted (ceiling) placement
                let mut inverted = ModelRegistry::is_stairs_inverted(base_model_id);
                if !inverted {
                    if let Some(hit) = self.ui.current_hit {
                        // Compute local Y for wall placement detection
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
                }

                let shape =
                    ModelRegistry::stairs_shape(base_model_id).unwrap_or(StairShape::Straight);
                ModelRegistry::stairs_model_id(shape, inverted)
            } else if ModelRegistry::is_door_model(base_model_id) {
                // Doors: two-block placement
                let upper_pos = place_pos + Vector3::new(0, 1, 0);

                // Check upper position is valid and empty
                if upper_pos.y >= TEXTURE_SIZE_Y as i32 {
                    return false; // No room for upper half
                }
                let upper_block = self.sim.world.get_block(upper_pos);
                if let Some(b) = upper_block {
                    if b != BlockType::Air && b != BlockType::Water {
                        return false; // Upper position blocked
                    }
                }

                // Determine rotation from player yaw (door faces player)
                let yaw = self.sim.player.camera.rotation.y as f32;
                let rot = (yaw / std::f32::consts::FRAC_PI_2).round() as i32;
                let mut base_rotation = rot.rem_euclid(4) as u8;

                // Determine if click was on near or far side of block
                // Calculate exact hit point
                if let Some(hit) = self.ui.current_hit {
                    let player_pos = self
                        .sim
                        .player
                        .camera_world_pos(self.sim.world_extent, self.sim.texture_origin)
                        .cast::<f32>();
                    let direction = self.sim.player.camera_direction().cast::<f32>();
                    let hit_point = player_pos + direction * hit.distance;
                    let local_hit = hit_point - place_pos.cast::<f32>();

                    // Determine if click was on far side based on which face was clicked
                    let place_at_far_edge = match (hit.normal.x, hit.normal.y, hit.normal.z) {
                        (_, 1, _) => {
                            // Clicking on top face (Y+), check depth along player's facing direction
                            match base_rotation {
                                0 => local_hit.z > 0.5, // Facing +Z: far if z > 0.5
                                1 => local_hit.x < 0.5, // Facing -X: far if x < 0.5
                                2 => local_hit.z < 0.5, // Facing -Z: far if z < 0.5
                                _ => local_hit.x > 0.5, // Facing +X: far if x > 0.5
                            }
                        }
                        (_, -1, _) => {
                            // Clicking on bottom face (Y-), check depth along player's facing direction
                            match base_rotation {
                                0 => local_hit.z > 0.5, // Facing +Z: far if z > 0.5
                                1 => local_hit.x < 0.5, // Facing -X: far if x < 0.5
                                2 => local_hit.z < 0.5, // Facing -Z: far if z < 0.5
                                _ => local_hit.x > 0.5, // Facing +X: far if x > 0.5
                            }
                        }
                        (1, _, _) | (-1, _, _) => {
                            // Clicking on X-facing wall
                            match base_rotation {
                                0 => local_hit.z > 0.5, // Facing +Z: far if z > 0.5
                                2 => local_hit.z < 0.5, // Facing -Z: far if z < 0.5
                                _ => false,             // Perpendicular to wall, no depth
                            }
                        }
                        (_, _, 1) | (_, _, -1) => {
                            // Clicking on Z-facing wall
                            match base_rotation {
                                1 => local_hit.x < 0.5, // Facing -X: far if x < 0.5
                                3 => local_hit.x > 0.5, // Facing +X: far if x > 0.5
                                _ => false,             // Perpendicular to wall, no depth
                            }
                        }
                        _ => false,
                    };

                    // If placing at far edge, rotate door 180° so it's at back of block
                    if place_at_far_edge {
                        base_rotation = (base_rotation + 2) % 4;
                    }
                }

                rotation = base_rotation;

                // Determine hinge side based on adjacent blocks
                // Check blocks to the left and right of the door (based on rotation)
                let (left_offset, right_offset) = match rotation {
                    0 => (Vector3::new(-1, 0, 0), Vector3::new(1, 0, 0)), // Facing +Z
                    1 => (Vector3::new(0, 0, -1), Vector3::new(0, 0, 1)), // Facing -X
                    2 => (Vector3::new(1, 0, 0), Vector3::new(-1, 0, 0)), // Facing -Z
                    _ => (Vector3::new(0, 0, 1), Vector3::new(0, 0, -1)), // Facing +X
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

                // Prefer hinge on solid side, default to left
                let hinge_left = !right_solid || left_solid;

                // Determine door type from selected model
                let door_base = ModelRegistry::door_type_base(base_model_id).unwrap_or(39);

                // Place lower half
                let lower_model =
                    ModelRegistry::door_model_id_with_base(door_base, false, hinge_left, false);
                self.sim
                    .world
                    .set_model_block(place_pos, lower_model, rotation, waterlogged);

                // Place upper half
                let upper_model =
                    ModelRegistry::door_model_id_with_base(door_base, true, hinge_left, false);
                let upper_waterlogged = upper_block == Some(BlockType::Water);
                self.sim
                    .world
                    .set_model_block(upper_pos, upper_model, rotation, upper_waterlogged);

                // Prevent door from being toggled on same click
                self.ui.gate_needs_reclick = true;

                return true;
            } else if ModelRegistry::is_trapdoor_model(base_model_id) {
                // Trapdoors: determine ceiling vs floor placement
                let yaw = self.sim.player.camera.rotation.y as f32;
                let rot = (yaw / std::f32::consts::FRAC_PI_2).round() as i32;
                rotation = rot.rem_euclid(4) as u8;

                let is_ceiling = if let Some(hit) = self.ui.current_hit {
                    // Ceiling if clicking bottom face or upper half of side
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
                self.sim
                    .world
                    .set_model_block(place_pos, model_id, rotation, waterlogged);
                return true;
            } else if ModelRegistry::is_window_model(base_model_id) {
                // Windows: calculate connections like fences
                let connections = self.sim.world.calculate_window_connections(place_pos);
                let model_id = ModelRegistry::window_model_id(connections);
                self.sim
                    .world
                    .set_model_block(place_pos, model_id, 0, waterlogged);
                self.sim.world.update_window_connections(place_pos);
                return true;
            } else if base_model_id >= FIRST_CUSTOM_MODEL_ID {
                // Custom models: auto-rotate to face player
                let yaw = self.sim.player.camera.rotation.y as f32;
                let rot = (yaw / std::f32::consts::FRAC_PI_2).round() as i32;
                rotation = rot.rem_euclid(4) as u8;
                base_model_id
            } else {
                base_model_id
            };

            self.sim
                .world
                .set_model_block(place_pos, model_id, rotation, waterlogged);

            if ModelRegistry::is_fence_or_gate(model_id) {
                self.sim.world.update_fence_connections(place_pos);
            } else if ModelRegistry::is_stairs_model(model_id) {
                // Update placed stair and neighbors to form corners
                self.sim.world.update_stair_and_neighbors(place_pos);
            }
        } else if block_to_place == BlockType::TintedGlass {
            // TintedGlass needs the tint_index from the hotbar
            let tint_index = self.ui.hotbar_tint_indices[self.ui.hotbar_index];
            self.sim.world.set_tinted_glass_block(place_pos, tint_index);
        } else if block_to_place == BlockType::Crystal {
            // Crystal needs the tint_index from the hotbar for color
            let tint_index = self.ui.hotbar_tint_indices[self.ui.hotbar_index];
            self.sim.world.set_crystal_block(place_pos, tint_index);
        } else if block_to_place == BlockType::Painted {
            let texture_idx = self.ui.hotbar_paint_textures[self.ui.hotbar_index];
            let tint_idx = self.ui.hotbar_tint_indices[self.ui.hotbar_index];
            self.sim
                .world
                .set_painted_block(place_pos, texture_idx, tint_idx);
        } else {
            self.sim.world.set_block(place_pos, block_to_place);

            if block_to_place.is_solid() {
                self.sim.world.update_fence_connections(place_pos);
            }
        }
        self.sim
            .world
            .invalidate_minimap_cache(place_pos.x, place_pos.z);

        if block_to_place == BlockType::Water {
            let water_type = WaterType::from_u8(self.ui.hotbar_tint_indices[self.ui.hotbar_index]);
            self.sim.water_grid.place_source(place_pos, water_type);
            self.sim.world.set_water_block(place_pos, water_type);
        } else if waterlogged {
            // Ensure water grid knows about waterlogged block (if not already there)
            if !self.sim.water_grid.has_water(place_pos) {
                // Determine existing water type if possible, otherwise default
                let water_type = self
                    .sim
                    .world
                    .get_water_type(place_pos)
                    .unwrap_or(WaterType::Ocean);
                self.sim.water_grid.place_source(place_pos, water_type);
            }
        } else {
            self.sim.water_grid.on_block_placed(place_pos);
        }

        // Handle lava placement
        if block_to_place == BlockType::Lava {
            self.sim.lava_grid.place_source(place_pos);
        } else {
            self.sim.lava_grid.on_block_placed(place_pos);
        }

        true
    }

    pub fn process_landed_blocks(&mut self, mut landed: Vec<crate::falling_block::LandedBlock>) {
        landed.sort_by_key(|lb| lb.position.y);

        for lb in landed {
            if lb.position.y >= 0 && lb.position.y < TEXTURE_SIZE_Y as i32 {
                let mut place_y = lb.position.y;
                while place_y < TEXTURE_SIZE_Y as i32 {
                    let check_pos = Vector3::new(lb.position.x, place_y, lb.position.z);
                    if let Some(existing) = self.sim.world.get_block(check_pos) {
                        if existing == BlockType::Air {
                            break;
                        }
                    }
                    place_y += 1;
                }

                if place_y < TEXTURE_SIZE_Y as i32 {
                    let final_pos = Vector3::new(lb.position.x, place_y, lb.position.z);
                    self.sim.world.set_block(final_pos, lb.block_type);
                    self.sim
                        .world
                        .invalidate_minimap_cache(final_pos.x, final_pos.z);

                    let player_pos = self
                        .sim
                        .player
                        .feet_pos(self.sim.world_extent, self.sim.texture_origin)
                        .cast::<f32>();

                    // Queue gravity check for block above (in case there's more falling blocks)
                    self.sim.block_updates.enqueue(
                        final_pos + Vector3::new(0, 1, 0),
                        BlockUpdateType::Gravity,
                        player_pos,
                    );
                }
            }
        }
    }

    /// Place a sphere using the current sphere tool settings and hotbar selection.
    pub fn place_sphere(&mut self) {
        let sphere = &self.ui.sphere_tool;
        if !sphere.active || sphere.preview_center.is_none() {
            return;
        }

        // Get block type and metadata from hotbar
        let block_type = self.ui.hotbar_blocks[self.ui.hotbar_index];
        let hotbar_idx = self.ui.hotbar_index;
        let tint_index = self.ui.hotbar_tint_indices[hotbar_idx];
        let paint_texture = self.ui.hotbar_paint_textures[hotbar_idx];

        // Regenerate full positions (preview may be truncated)
        let center = sphere.preview_center.unwrap();
        let positions = crate::shape_tools::sphere::generate_sphere_positions(
            center,
            sphere.radius,
            sphere.hollow,
            sphere.dome,
        );

        // Place blocks
        let mut placed_count = 0;
        for pos in &positions {
            // Skip if out of Y bounds (X/Z are infinite)
            if pos.y < 0 || pos.y >= TEXTURE_SIZE_Y as i32 {
                continue;
            }

            match block_type {
                BlockType::TintedGlass => {
                    self.sim.world.set_tinted_glass_block(*pos, tint_index);
                }
                BlockType::Crystal => {
                    self.sim.world.set_crystal_block(*pos, tint_index);
                }
                BlockType::Painted => {
                    self.sim
                        .world
                        .set_painted_block(*pos, paint_texture, tint_index);
                }
                BlockType::Water => {
                    let water_type = WaterType::from_u8(tint_index);
                    self.sim.water_grid.place_source(*pos, water_type);
                    self.sim.world.set_water_block(*pos, water_type);
                }
                BlockType::Lava => {
                    self.sim.lava_grid.place_source(*pos);
                    self.sim.world.set_block(*pos, BlockType::Lava);
                }
                BlockType::Model | BlockType::Air => {
                    // Skip model and air blocks - don't make sense for sphere fill
                    continue;
                }
                _ => {
                    self.sim.world.set_block(*pos, block_type);
                }
            }
            placed_count += 1;
        }

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        println!(
            "Placed {} sphere ({} blocks, radius {})",
            if self.ui.sphere_tool.hollow {
                "hollow"
            } else {
                "solid"
            },
            placed_count,
            self.ui.sphere_tool.radius
        );

        // Don't deactivate tool - allow placing multiple spheres
    }

    /// Place a cube using the current cube tool settings and hotbar selection.
    pub fn place_cube(&mut self) {
        let cube = &self.ui.cube_tool;
        if !cube.active || cube.preview_center.is_none() {
            return;
        }

        // Get block type and metadata from hotbar
        let block_type = self.ui.hotbar_blocks[self.ui.hotbar_index];
        let hotbar_idx = self.ui.hotbar_index;
        let tint_index = self.ui.hotbar_tint_indices[hotbar_idx];
        let paint_texture = self.ui.hotbar_paint_textures[hotbar_idx];

        // Regenerate full positions (preview may be truncated)
        let center = cube.preview_center.unwrap();
        let positions = crate::shape_tools::cube::generate_cube_positions(
            center,
            cube.size_x,
            cube.size_y,
            cube.size_z,
            cube.hollow,
            cube.dome,
        );

        // Place blocks
        let mut placed_count = 0;
        for pos in &positions {
            // Skip if out of Y bounds (X/Z are infinite)
            if pos.y < 0 || pos.y >= TEXTURE_SIZE_Y as i32 {
                continue;
            }

            match block_type {
                BlockType::TintedGlass => {
                    self.sim.world.set_tinted_glass_block(*pos, tint_index);
                }
                BlockType::Crystal => {
                    self.sim.world.set_crystal_block(*pos, tint_index);
                }
                BlockType::Painted => {
                    self.sim
                        .world
                        .set_painted_block(*pos, paint_texture, tint_index);
                }
                BlockType::Water => {
                    let water_type = WaterType::from_u8(tint_index);
                    self.sim.water_grid.place_source(*pos, water_type);
                    self.sim.world.set_water_block(*pos, water_type);
                }
                BlockType::Lava => {
                    self.sim.lava_grid.place_source(*pos);
                    self.sim.world.set_block(*pos, BlockType::Lava);
                }
                BlockType::Model | BlockType::Air => {
                    // Skip model and air blocks - don't make sense for cube fill
                    continue;
                }
                _ => {
                    self.sim.world.set_block(*pos, block_type);
                }
            }
            placed_count += 1;
        }

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        let width = self.ui.cube_tool.size_x * 2 + 1;
        let height = self.ui.cube_tool.size_y * 2 + 1;
        let depth = self.ui.cube_tool.size_z * 2 + 1;
        println!(
            "Placed {} cube ({} blocks, {}x{}x{})",
            if self.ui.cube_tool.hollow {
                "hollow"
            } else {
                "solid"
            },
            placed_count,
            width,
            height,
            depth
        );

        // Don't deactivate tool - allow placing multiple cubes
    }

    /// Place a bridge (line) using the current bridge tool settings and hotbar selection.
    pub fn place_bridge(&mut self) {
        let bridge = &self.ui.bridge_tool;
        if !bridge.active || bridge.start_position.is_none() || bridge.preview_end.is_none() {
            return;
        }

        // Get block type and metadata from hotbar
        let block_type = self.ui.hotbar_blocks[self.ui.hotbar_index];
        let hotbar_idx = self.ui.hotbar_index;
        let tint_index = self.ui.hotbar_tint_indices[hotbar_idx];
        let paint_texture = self.ui.hotbar_paint_textures[hotbar_idx];

        // Generate line positions
        let start = bridge.start_position.unwrap();
        let end = bridge.preview_end.unwrap();
        let positions = crate::shape_tools::bridge::generate_line_positions(start, end);

        // Place blocks
        let mut placed_count = 0;
        for pos in &positions {
            // Skip if out of Y bounds (X/Z are infinite)
            if pos.y < 0 || pos.y >= TEXTURE_SIZE_Y as i32 {
                continue;
            }

            match block_type {
                BlockType::TintedGlass => {
                    self.sim.world.set_tinted_glass_block(*pos, tint_index);
                }
                BlockType::Crystal => {
                    self.sim.world.set_crystal_block(*pos, tint_index);
                }
                BlockType::Painted => {
                    self.sim
                        .world
                        .set_painted_block(*pos, paint_texture, tint_index);
                }
                BlockType::Water => {
                    let water_type = WaterType::from_u8(tint_index);
                    self.sim.water_grid.place_source(*pos, water_type);
                    self.sim.world.set_water_block(*pos, water_type);
                }
                BlockType::Lava => {
                    self.sim.lava_grid.place_source(*pos);
                    self.sim.world.set_block(*pos, BlockType::Lava);
                }
                BlockType::Model | BlockType::Air => {
                    // Skip model and air blocks - don't make sense for bridge
                    continue;
                }
                _ => {
                    self.sim.world.set_block(*pos, block_type);
                }
            }
            placed_count += 1;
        }

        // Invalidate minimap cache for affected area
        self.sim.world.invalidate_minimap_cache(start.x, start.z);
        self.sim.world.invalidate_minimap_cache(end.x, end.z);

        println!(
            "Placed bridge ({} blocks, from ({},{},{}) to ({},{},{}))",
            placed_count, start.x, start.y, start.z, end.x, end.y, end.z
        );

        // Don't deactivate tool - allow placing multiple bridges
    }

    /// Place a cylinder using the current cylinder tool settings and hotbar selection.
    pub fn place_cylinder(&mut self) {
        let cylinder = &self.ui.cylinder_tool;
        if !cylinder.active || cylinder.preview_center.is_none() {
            return;
        }

        // Get block type and metadata from hotbar
        let block_type = self.ui.hotbar_blocks[self.ui.hotbar_index];
        let hotbar_idx = self.ui.hotbar_index;
        let tint_index = self.ui.hotbar_tint_indices[hotbar_idx];
        let paint_texture = self.ui.hotbar_paint_textures[hotbar_idx];

        // Regenerate full positions (preview may be truncated)
        let center = cylinder.preview_center.unwrap();
        let positions = crate::shape_tools::cylinder::generate_cylinder_positions(
            center,
            cylinder.radius,
            cylinder.height,
            cylinder.hollow,
            cylinder.axis,
        );

        // Place blocks
        let mut placed_count = 0;
        for pos in &positions {
            // Skip if out of Y bounds (X/Z are infinite)
            if pos.y < 0 || pos.y >= TEXTURE_SIZE_Y as i32 {
                continue;
            }

            match block_type {
                BlockType::TintedGlass => {
                    self.sim.world.set_tinted_glass_block(*pos, tint_index);
                }
                BlockType::Crystal => {
                    self.sim.world.set_crystal_block(*pos, tint_index);
                }
                BlockType::Painted => {
                    self.sim
                        .world
                        .set_painted_block(*pos, paint_texture, tint_index);
                }
                BlockType::Water => {
                    let water_type = WaterType::from_u8(tint_index);
                    self.sim.water_grid.place_source(*pos, water_type);
                    self.sim.world.set_water_block(*pos, water_type);
                }
                BlockType::Lava => {
                    self.sim.lava_grid.place_source(*pos);
                    self.sim.world.set_block(*pos, BlockType::Lava);
                }
                BlockType::Model | BlockType::Air => {
                    // Skip model and air blocks - don't make sense for cylinder fill
                    continue;
                }
                _ => {
                    self.sim.world.set_block(*pos, block_type);
                }
            }
            placed_count += 1;
        }

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        let axis_name = match self.ui.cylinder_tool.axis {
            crate::shape_tools::cylinder::CylinderAxis::Y => "vertical",
            crate::shape_tools::cylinder::CylinderAxis::X => "X-axis",
            crate::shape_tools::cylinder::CylinderAxis::Z => "Z-axis",
        };
        println!(
            "Placed {} {} cylinder ({} blocks, radius {}, height {})",
            if self.ui.cylinder_tool.hollow {
                "hollow"
            } else {
                "solid"
            },
            axis_name,
            placed_count,
            self.ui.cylinder_tool.radius,
            self.ui.cylinder_tool.height
        );

        // Don't deactivate tool - allow placing multiple cylinders
    }

    /// Place a wall using the current wall tool settings and hotbar selection.
    pub fn place_wall(&mut self) {
        let wall = &self.ui.wall_tool;
        if !wall.active || wall.start_position.is_none() || wall.preview_end.is_none() {
            return;
        }

        // Get block type and metadata from hotbar
        let block_type = self.ui.hotbar_blocks[self.ui.hotbar_index];
        let hotbar_idx = self.ui.hotbar_index;
        let tint_index = self.ui.hotbar_tint_indices[hotbar_idx];
        let paint_texture = self.ui.hotbar_paint_textures[hotbar_idx];

        // Regenerate full positions (preview may be truncated)
        let start = wall.start_position.unwrap();
        let end = wall.preview_end.unwrap();
        let positions = crate::shape_tools::wall::generate_wall_positions(
            start,
            end,
            wall.thickness,
            wall.effective_manual_height(),
        );

        // Place blocks
        let mut placed_count = 0;
        for pos in &positions {
            // Skip if out of Y bounds (X/Z are infinite)
            if pos.y < 0 || pos.y >= TEXTURE_SIZE_Y as i32 {
                continue;
            }

            match block_type {
                BlockType::TintedGlass => {
                    self.sim.world.set_tinted_glass_block(*pos, tint_index);
                }
                BlockType::Crystal => {
                    self.sim.world.set_crystal_block(*pos, tint_index);
                }
                BlockType::Painted => {
                    self.sim
                        .world
                        .set_painted_block(*pos, paint_texture, tint_index);
                }
                BlockType::Water => {
                    let water_type = WaterType::from_u8(tint_index);
                    self.sim.water_grid.place_source(*pos, water_type);
                    self.sim.world.set_water_block(*pos, water_type);
                }
                BlockType::Lava => {
                    self.sim.lava_grid.place_source(*pos);
                    self.sim.world.set_block(*pos, BlockType::Lava);
                }
                BlockType::Model | BlockType::Air => {
                    // Skip model and air blocks - don't make sense for wall fill
                    continue;
                }
                _ => {
                    self.sim.world.set_block(*pos, block_type);
                }
            }
            placed_count += 1;
        }

        // Invalidate minimap cache for affected area
        self.sim.world.invalidate_minimap_cache(start.x, start.z);
        self.sim.world.invalidate_minimap_cache(end.x, end.z);

        let (length, height, thickness) = crate::shape_tools::wall::calculate_dimensions(
            start,
            end,
            self.ui.wall_tool.thickness,
            self.ui.wall_tool.effective_manual_height(),
        );
        println!(
            "Placed wall ({} blocks, {}L × {}H × {}T)",
            placed_count, length, height, thickness
        );

        // Don't deactivate tool - allow placing multiple walls
    }

    /// Place a floor/platform between two corners using the hotbar block.
    pub fn place_floor(&mut self) {
        let floor = &self.ui.floor_tool;
        if !floor.active || floor.start_position.is_none() || floor.preview_end.is_none() {
            return;
        }

        // Get block type and metadata from hotbar
        let block_type = self.ui.hotbar_blocks[self.ui.hotbar_index];
        let hotbar_idx = self.ui.hotbar_index;
        let tint_index = self.ui.hotbar_tint_indices[hotbar_idx];
        let paint_texture = self.ui.hotbar_paint_textures[hotbar_idx];

        // Regenerate full positions (preview may be truncated)
        let start = floor.start_position.unwrap();
        let end = floor.preview_end.unwrap();
        let positions = crate::shape_tools::floor::generate_floor_positions(
            start,
            end,
            floor.thickness,
            floor.direction,
        );

        // Place blocks
        let mut placed_count = 0;
        for pos in &positions {
            // Skip if out of Y bounds (X/Z are infinite)
            if pos.y < 0 || pos.y >= TEXTURE_SIZE_Y as i32 {
                continue;
            }

            match block_type {
                BlockType::TintedGlass => {
                    self.sim.world.set_tinted_glass_block(*pos, tint_index);
                }
                BlockType::Crystal => {
                    self.sim.world.set_crystal_block(*pos, tint_index);
                }
                BlockType::Painted => {
                    self.sim
                        .world
                        .set_painted_block(*pos, paint_texture, tint_index);
                }
                BlockType::Water => {
                    let water_type = WaterType::from_u8(tint_index);
                    self.sim.water_grid.place_source(*pos, water_type);
                    self.sim.world.set_water_block(*pos, water_type);
                }
                BlockType::Lava => {
                    self.sim.lava_grid.place_source(*pos);
                    self.sim.world.set_block(*pos, BlockType::Lava);
                }
                BlockType::Model | BlockType::Air => {
                    // Skip model and air blocks - don't make sense for floor fill
                    continue;
                }
                _ => {
                    self.sim.world.set_block(*pos, block_type);
                }
            }
            placed_count += 1;
        }

        // Invalidate minimap cache for affected area
        self.sim.world.invalidate_minimap_cache(start.x, start.z);
        self.sim.world.invalidate_minimap_cache(end.x, end.z);

        let (length, width, thickness) = crate::shape_tools::floor::calculate_dimensions(
            start,
            end,
            self.ui.floor_tool.thickness,
        );
        println!(
            "Placed floor ({} blocks, {}L × {}W × {}T)",
            placed_count, length, width, thickness
        );

        // Don't deactivate tool - allow placing multiple floors
    }

    /// Execute block replacement within the current selection.
    pub fn execute_replace(&mut self) {
        let replace = &self.ui.replace_tool;
        if !replace.active {
            return;
        }

        let selection = &self.ui.template_selection;
        if selection.pos1.is_none() || selection.pos2.is_none() {
            return;
        }

        let (min, max) = selection.bounds().unwrap();
        let source_id = replace.source_identity();
        let target_block = replace.target_block;
        let target_tint = replace.target_tint;
        let target_texture = replace.target_texture;

        let mut replaced_count = 0;

        for x in min.x..=max.x {
            for y in min.y..=max.y {
                for z in min.z..=max.z {
                    let pos = nalgebra::Vector3::new(x, y, z);

                    // Skip if out of Y bounds
                    if y < 0 || y >= TEXTURE_SIZE_Y as i32 {
                        continue;
                    }

                    if source_id.matches(&self.sim.world, pos) {
                        // Replace the block
                        match target_block {
                            BlockType::TintedGlass => {
                                self.sim.world.set_tinted_glass_block(pos, target_tint);
                            }
                            BlockType::Crystal => {
                                self.sim.world.set_crystal_block(pos, target_tint);
                            }
                            BlockType::Painted => {
                                self.sim
                                    .world
                                    .set_painted_block(pos, target_texture, target_tint);
                            }
                            BlockType::Water => {
                                let water_type = WaterType::from_u8(target_tint);
                                self.sim.water_grid.place_source(pos, water_type);
                                self.sim.world.set_water_block(pos, water_type);
                            }
                            BlockType::Lava => {
                                self.sim.lava_grid.place_source(pos);
                                self.sim.world.set_block(pos, BlockType::Lava);
                            }
                            BlockType::Air => {
                                // Removing blocks - need to handle water/lava
                                let old_block = self.sim.world.get_block(pos);
                                if old_block == Some(BlockType::Water) {
                                    self.sim.water_grid.remove_source(pos);
                                } else if old_block == Some(BlockType::Lava) {
                                    self.sim.lava_grid.remove_source(pos);
                                }
                                self.sim.world.set_block(pos, BlockType::Air);
                            }
                            BlockType::Model => {
                                // Skip model blocks - not supported for replacement
                                continue;
                            }
                            _ => {
                                self.sim.world.set_block(pos, target_block);
                            }
                        }
                        replaced_count += 1;
                    }
                }
            }
        }

        // Invalidate minimap cache for affected area
        self.sim.world.invalidate_minimap_cache(min.x, min.z);
        self.sim.world.invalidate_minimap_cache(max.x, max.z);

        println!(
            "Replaced {} blocks: {:?} -> {:?}",
            replaced_count, self.ui.replace_tool.source_block, target_block
        );

        // Clear preview after replacement
        self.ui.replace_tool.clear_preview();
    }
}

/// Determines if a stair should be placed inverted based on hit information.
///
/// # Arguments
/// * `hit_normal_y` - The Y component of the hit normal (-1, 0, or 1)
/// * `local_y` - The fractional Y position within the hit block (0.0 to 1.0)
///
/// # Returns
/// `true` if the stair should be inverted (ceiling placement)
pub fn should_place_inverted_stair(hit_normal_y: i32, local_y: f32) -> bool {
    if hit_normal_y < 0 {
        // Clicking on bottom face of block above -> inverted
        true
    } else if hit_normal_y == 0 {
        // Clicking on side face: upper half -> inverted
        local_y >= 0.5
    } else {
        // Clicking on top face -> normal (not inverted)
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wall_placement_lower_half() {
        // Clicking lower half of wall (Y < 0.5) -> normal stair
        assert!(!should_place_inverted_stair(0, 0.0));
        assert!(!should_place_inverted_stair(0, 0.25));
        assert!(!should_place_inverted_stair(0, 0.49));
    }

    #[test]
    fn test_wall_placement_upper_half() {
        // Clicking upper half of wall (Y >= 0.5) -> inverted stair
        assert!(should_place_inverted_stair(0, 0.5));
        assert!(should_place_inverted_stair(0, 0.75));
        assert!(should_place_inverted_stair(0, 0.99));
    }

    #[test]
    fn test_floor_placement() {
        // Clicking on top face of block (normal_y = 1) -> normal stair
        assert!(!should_place_inverted_stair(1, 0.0));
        assert!(!should_place_inverted_stair(1, 0.5));
        assert!(!should_place_inverted_stair(1, 1.0));
    }

    #[test]
    fn test_ceiling_placement() {
        // Clicking on bottom face of block (normal_y = -1) -> inverted stair
        assert!(should_place_inverted_stair(-1, 0.0));
        assert!(should_place_inverted_stair(-1, 0.5));
        assert!(should_place_inverted_stair(-1, 1.0));
    }

    #[test]
    fn test_wall_placement_boundary() {
        // Test exact boundary at 0.5
        assert!(should_place_inverted_stair(0, 0.5));
        // Just below boundary
        assert!(!should_place_inverted_stair(0, 0.4999));
    }
}

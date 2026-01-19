use crate::App;
use crate::block_update::BlockUpdateType;
use crate::chunk::{BlockType, WaterType};
use crate::constants::TEXTURE_SIZE_Y;
use crate::placement::{BlockPlacementParams, place_blocks_at_positions};
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
                // Check if breaking a frame - need to break all frame blocks
                let mut frame_blocks: Vec<Vector3<i32>> = Vec::new();
                if block_type == BlockType::Model {
                    if let Some(model_data) = self.sim.world.get_model_data(target) {
                        if ModelRegistry::is_door_model(model_data.model_id) {
                            other_door_half = if ModelRegistry::is_door_upper(model_data.model_id) {
                                Some(target + Vector3::new(0, -1, 0))
                            } else {
                                Some(target + Vector3::new(0, 1, 0))
                            };
                        } else if ModelRegistry::is_frame_model(model_data.model_id) {
                            // Get all blocks in this frame (excluding target)
                            frame_blocks = ModelRegistry::frame_block_positions(
                                target,
                                model_data.model_id,
                                model_data.custom_data,
                            )
                            .into_iter()
                            .filter(|&p| p != target)
                            .collect();
                        }
                    }
                }

                if is_waterlogged {
                    self.sim.world.set_block(target, BlockType::Water);
                } else {
                    self.sim.world.set_block(target, BlockType::Air);
                }
                self.sim.world.invalidate_minimap_cache(target.x, target.z);

                // Break mirrored positions if mirror tool is active
                if self.ui.mirror_tool.active && self.ui.mirror_tool.plane_set {
                    let mirrored_positions = self.ui.mirror_tool.mirror_position(target);
                    for mirrored_pos in mirrored_positions.into_iter().skip(1) {
                        // Skip if out of bounds
                        if mirrored_pos.y < 0 || mirrored_pos.y >= TEXTURE_SIZE_Y as i32 {
                            continue;
                        }
                        // Get the block at mirrored position
                        if let Some(mirrored_block) = self.sim.world.get_block(mirrored_pos) {
                            if mirrored_block.break_time() > 0.0 {
                                // Check if mirrored block is waterlogged model
                                let mirrored_waterlogged = if mirrored_block == BlockType::Model {
                                    self.sim
                                        .world
                                        .get_model_data(mirrored_pos)
                                        .map(|d| d.waterlogged)
                                        .unwrap_or(false)
                                } else {
                                    false
                                };

                                // Spawn particles for mirrored break
                                let color = mirrored_block.color();
                                let particle_color =
                                    nalgebra::Vector3::new(color[0], color[1], color[2]);
                                self.sim
                                    .particles
                                    .spawn_block_break(mirrored_pos.cast::<f32>(), particle_color);

                                // Break the mirrored block
                                if mirrored_waterlogged {
                                    self.sim.world.set_block(mirrored_pos, BlockType::Water);
                                } else {
                                    self.sim.world.set_block(mirrored_pos, BlockType::Air);
                                }
                                self.sim
                                    .world
                                    .invalidate_minimap_cache(mirrored_pos.x, mirrored_pos.z);

                                // Update connections for mirrored position
                                self.sim.world.update_fence_connections(mirrored_pos);
                                self.sim.world.update_window_connections(mirrored_pos);
                                self.sim.world.update_pane_connections(mirrored_pos);
                                self.sim.world.update_adjacent_stair_shapes(mirrored_pos);

                                // Notify water/lava grids
                                self.sim.water_grid.on_block_removed(mirrored_pos);
                                self.sim
                                    .water_grid
                                    .activate_adjacent_terrain_water(&self.sim.world, mirrored_pos);
                                self.sim.lava_grid.on_block_removed(mirrored_pos);
                                self.sim
                                    .lava_grid
                                    .activate_adjacent_terrain_lava(&self.sim.world, mirrored_pos);
                            }
                        }
                    }
                }

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

                // Break other frame blocks if present
                for frame_pos in frame_blocks {
                    if frame_pos.y >= 0 && frame_pos.y < TEXTURE_SIZE_Y as i32 {
                        if let Some(BlockType::Model) = self.sim.world.get_block(frame_pos) {
                            if let Some(frame_data) = self.sim.world.get_model_data(frame_pos) {
                                if ModelRegistry::is_frame_model(frame_data.model_id) {
                                    // Spawn particles for frame block break
                                    let frame_color = BlockType::Model.color();
                                    let particle_color = nalgebra::Vector3::new(
                                        frame_color[0],
                                        frame_color[1],
                                        frame_color[2],
                                    );
                                    self.sim
                                        .particles
                                        .spawn_block_break(frame_pos.cast::<f32>(), particle_color);

                                    if frame_data.waterlogged {
                                        self.sim.world.set_block(frame_pos, BlockType::Water);
                                    } else {
                                        self.sim.world.set_block(frame_pos, BlockType::Air);
                                    }
                                    self.sim
                                        .world
                                        .invalidate_minimap_cache(frame_pos.x, frame_pos.z);
                                }
                            }
                        }
                    }
                }

                // Update neighboring fence/gate connections
                self.sim.world.update_fence_connections(target);
                // Update neighboring window connections
                self.sim.world.update_window_connections(target);
                // Update neighboring glass pane connections
                self.sim.world.update_pane_connections(target);
                // Update neighboring stair shapes (stair neighbors may straighten)
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
            if let Some(other_data) = other_model_data {
                if self
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
        let texture_idx = self.ui.hotbar_paint_textures[self.ui.hotbar_index];
        let tint_idx = self.ui.hotbar_tint_indices[self.ui.hotbar_index];
        let blend_mode = self.ui.paint_panel.current_config.blend_mode as u8;

        // Repaint the block with blend mode
        self.sim
            .world
            .set_painted_block_full(pos, texture_idx, tint_idx, blend_mode);

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
        // Note: mirror_tool is NOT in this list - it allows normal placement but mirrors the result
        if self.ui.active_placement.is_some()
            || self.ui.active_stencil_placement.is_some()
            || self.ui.template_selection.visual_mode
            || self.ui.rangefinder_active
            || self.ui.flood_fill_active
            || self.ui.sphere_tool.active
            || self.ui.cube_tool.active
            || self.ui.bridge_tool.active
            || self.ui.cylinder_tool.active
            || self.ui.wall_tool.active
            || self.ui.floor_tool.active
            || self.ui.replace_tool.active
            || self.ui.circle_tool.active
            || self.ui.stairs_tool.active
            || self.ui.arch_tool.active
            || self.ui.cone_tool.active
            || self.ui.clone_tool.active
            // Phase 18 tools
            || self.ui.torus_tool.active
            || self.ui.helix_tool.active
            || self.ui.polygon_tool.active
            || self.ui.bezier_tool.active
            || self.ui.pattern_fill.active
            || self.ui.scatter_tool.active
            || self.ui.hollow_tool.active
            || self.ui.terrain_brush.active
        {
            return;
        }

        // Skip block placement if mirror tool is active but plane not set yet
        // (right-click should set the plane, not place a block)
        if self.ui.mirror_tool.active && !self.ui.mirror_tool.plane_set {
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
                // Place at mirrored positions if mirror tool is active
                if self.ui.mirror_tool.active && self.ui.mirror_tool.plane_set {
                    let mirrored_positions = self.ui.mirror_tool.mirror_position(constrained_pos);
                    // Skip first position (it's the original we already placed)
                    for mirrored_pos in mirrored_positions.into_iter().skip(1) {
                        self.place_block_at(mirrored_pos);
                    }
                }

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
            } else if ModelRegistry::is_horizontal_glass_pane_model(base_model_id) {
                // Horizontal glass pane: calculate connections
                let connections = self
                    .sim
                    .world
                    .calculate_horizontal_pane_connections(place_pos);
                let model_id = ModelRegistry::horizontal_glass_pane_model_id(connections);
                self.sim
                    .world
                    .set_model_block(place_pos, model_id, 0, waterlogged);
                self.sim.world.update_horizontal_pane_connections(place_pos);
                return true;
            } else if ModelRegistry::is_vertical_glass_pane_model(base_model_id) {
                // Vertical glass pane: determine rotation from player view or clicked face
                let rotation = if let Some(hit) = self.ui.current_hit {
                    // Determine orientation based on which face was clicked
                    match (hit.normal.x, hit.normal.z) {
                        (1, 0) | (-1, 0) => 1u8, // Clicked X face -> YZ plane
                        (0, 1) | (0, -1) => 0u8, // Clicked Z face -> XY plane
                        _ => {
                            // Top/bottom face: use player yaw
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

    /// Get block placement parameters from the current hotbar selection.
    fn get_hotbar_placement_params(&self) -> BlockPlacementParams {
        let block_type = self.ui.hotbar_blocks[self.ui.hotbar_index];
        let tint_index = self.ui.hotbar_tint_indices[self.ui.hotbar_index];
        let paint_texture = self.ui.hotbar_paint_textures[self.ui.hotbar_index];
        BlockPlacementParams::new(block_type, tint_index, paint_texture)
    }

    /// Place a sphere using the current sphere tool settings and hotbar selection.
    pub fn place_sphere(&mut self) {
        let sphere = &self.ui.sphere_tool;
        if !sphere.active || sphere.preview_center.is_none() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Regenerate full positions (preview may be truncated)
        let center = sphere.preview_center.unwrap();
        let radius = sphere.radius;
        let hollow = sphere.hollow;
        let dome = sphere.dome;
        let positions =
            crate::shape_tools::sphere::generate_sphere_positions(center, radius, hollow, dome);

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        println!(
            "Placed {} sphere ({} blocks, radius {})",
            if hollow { "hollow" } else { "solid" },
            placed_count,
            radius
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
        let params = self.get_hotbar_placement_params();

        // Regenerate full positions (preview may be truncated)
        let center = cube.preview_center.unwrap();
        let size_x = cube.size_x;
        let size_y = cube.size_y;
        let size_z = cube.size_z;
        let hollow = cube.hollow;
        let dome = cube.dome;
        let positions = crate::shape_tools::cube::generate_cube_positions(
            center, size_x, size_y, size_z, hollow, dome,
        );

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        let width = size_x * 2 + 1;
        let height = size_y * 2 + 1;
        let depth = size_z * 2 + 1;
        println!(
            "Placed {} cube ({} blocks, {}x{}x{})",
            if hollow { "hollow" } else { "solid" },
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
        let params = self.get_hotbar_placement_params();

        // Generate line positions
        let start = bridge.start_position.unwrap();
        let end = bridge.preview_end.unwrap();
        let positions = crate::shape_tools::bridge::generate_line_positions(start, end);

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

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
        let params = self.get_hotbar_placement_params();

        // Regenerate full positions (preview may be truncated)
        let center = cylinder.preview_center.unwrap();
        let radius = cylinder.radius;
        let height = cylinder.height;
        let hollow = cylinder.hollow;
        let axis = cylinder.axis;
        let positions = crate::shape_tools::cylinder::generate_cylinder_positions(
            center, radius, height, hollow, axis,
        );

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        let axis_name = match axis {
            crate::shape_tools::cylinder::CylinderAxis::Y => "vertical",
            crate::shape_tools::cylinder::CylinderAxis::X => "X-axis",
            crate::shape_tools::cylinder::CylinderAxis::Z => "Z-axis",
        };
        println!(
            "Placed {} {} cylinder ({} blocks, radius {}, height {})",
            if hollow { "hollow" } else { "solid" },
            axis_name,
            placed_count,
            radius,
            height
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
        let params = self.get_hotbar_placement_params();

        // Regenerate full positions (preview may be truncated)
        let start = wall.start_position.unwrap();
        let end = wall.preview_end.unwrap();
        let thickness = wall.thickness;
        let manual_height = wall.effective_manual_height();
        let positions =
            crate::shape_tools::wall::generate_wall_positions(start, end, thickness, manual_height);

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        self.sim.world.invalidate_minimap_cache(start.x, start.z);
        self.sim.world.invalidate_minimap_cache(end.x, end.z);

        let (length, height, thick) =
            crate::shape_tools::wall::calculate_dimensions(start, end, thickness, manual_height);
        println!(
            "Placed wall ({} blocks, {}L × {}H × {}T)",
            placed_count, length, height, thick
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
        let params = self.get_hotbar_placement_params();

        // Regenerate full positions (preview may be truncated)
        let start = floor.start_position.unwrap();
        let end = floor.preview_end.unwrap();
        let thickness = floor.thickness;
        let direction = floor.direction;
        let positions =
            crate::shape_tools::floor::generate_floor_positions(start, end, thickness, direction);

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        self.sim.world.invalidate_minimap_cache(start.x, start.z);
        self.sim.world.invalidate_minimap_cache(end.x, end.z);

        let (length, width, thick) =
            crate::shape_tools::floor::calculate_dimensions(start, end, thickness);
        println!(
            "Placed floor ({} blocks, {}L × {}W × {}T)",
            placed_count, length, width, thick
        );

        // Don't deactivate tool - allow placing multiple floors
    }

    /// Place a circle or ellipse using the hotbar block.
    pub fn place_circle(&mut self) {
        let circle = &self.ui.circle_tool;
        if !circle.active || circle.preview_positions.is_empty() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Regenerate full positions (preview may be truncated)
        // Apply placement mode adjustment to get the actual center
        let raw_center = circle.preview_center.unwrap();
        let center = circle.adjust_center_for_placement(raw_center);
        let radius_a = circle.radius_a;
        let radius_b = circle.effective_radius_b();
        let plane = circle.plane;
        let filled = circle.filled;
        let ellipse_mode = circle.ellipse_mode;
        let positions = crate::shape_tools::circle::generate_circle_positions(
            center, radius_a, radius_b, plane, filled,
        );

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        self.sim.world.invalidate_minimap_cache(center.x, center.z);

        let radius_desc = if ellipse_mode {
            format!("{}×{}", radius_a, radius_b)
        } else {
            format!("{}", radius_a)
        };
        let fill_desc = if filled { "filled" } else { "outline" };
        println!(
            "Placed {} circle ({} blocks, radius {})",
            fill_desc, placed_count, radius_desc
        );

        // Don't deactivate tool - allow placing multiple circles
    }

    /// Place stairs using the current stairs tool settings and hotbar selection.
    pub fn place_stairs(&mut self) {
        let stairs = &self.ui.stairs_tool;
        if !stairs.active || stairs.start_pos.is_none() || stairs.preview_positions.is_empty() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Use preview positions (already generated with current target)
        let positions = self.ui.stairs_tool.preview_positions.clone();
        let step_count = self.ui.stairs_tool.step_count;
        let width = self.ui.stairs_tool.width;

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }
        if let Some(last_pos) = positions.last() {
            self.sim
                .world
                .invalidate_minimap_cache(last_pos.x, last_pos.z);
        }

        println!(
            "Placed stairs ({} blocks, {} steps × {} wide)",
            placed_count, step_count, width
        );

        // Don't deactivate tool - allow placing multiple staircases
    }

    /// Place an arch using the current arch tool settings and hotbar selection.
    pub fn place_arch(&mut self) {
        let arch = &self.ui.arch_tool;
        if !arch.active || arch.preview_positions.is_empty() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Use preview positions (already generated with current settings)
        let positions = self.ui.arch_tool.preview_positions.clone();
        let width = self.ui.arch_tool.width;
        let height = self.ui.arch_tool.height;
        let style_name = self.ui.arch_tool.style.name();

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        println!(
            "Placed {} arch ({} blocks, {}W × {}H)",
            style_name, placed_count, width, height
        );

        // Don't deactivate tool - allow placing multiple arches
    }

    /// Place a cone or pyramid at the preview position.
    pub fn place_cone(&mut self) {
        let cone = &self.ui.cone_tool;
        if !cone.active || cone.preview_positions.is_empty() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Use preview positions (already generated with current settings)
        let positions = self.ui.cone_tool.preview_positions.clone();
        let shape_name = self.ui.cone_tool.shape.name();
        let base_size = self.ui.cone_tool.base_size;
        let height = self.ui.cone_tool.height;

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        println!(
            "Placed {} ({} blocks, base {} × height {})",
            shape_name, placed_count, base_size, height
        );

        // Don't deactivate tool - allow placing multiple cones
    }

    /// Place a torus (ring/donut) at the preview position.
    pub fn place_torus(&mut self) {
        let torus = &self.ui.torus_tool;
        if !torus.active || torus.preview_positions.is_empty() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Use preview positions (already generated with current settings)
        let positions = self.ui.torus_tool.preview_positions.clone();
        let major_radius = self.ui.torus_tool.major_radius;
        let minor_radius = self.ui.torus_tool.minor_radius;
        let plane_name = self.ui.torus_tool.plane.name();

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        println!(
            "Placed torus ({} blocks, R={}/{}, plane={})",
            placed_count, major_radius, minor_radius, plane_name
        );

        // Don't deactivate tool - allow placing multiple tori
    }

    /// Place a helix (spiral) at the preview position.
    pub fn place_helix(&mut self) {
        let helix = &self.ui.helix_tool;
        if !helix.active || helix.preview_positions.is_empty() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Use preview positions (already generated with current settings)
        let positions = self.ui.helix_tool.preview_positions.clone();
        let radius = self.ui.helix_tool.radius;
        let height = self.ui.helix_tool.height;
        let turns = self.ui.helix_tool.turns;

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        println!(
            "Placed helix ({} blocks, R={}, H={}, {:.1} turns)",
            placed_count, radius, height, turns
        );

        // Don't deactivate tool - allow placing multiple helixes
    }

    /// Place a polygon/prism at the preview position.
    pub fn place_polygon(&mut self) {
        let polygon = &self.ui.polygon_tool;
        if !polygon.active || polygon.preview_positions.is_empty() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Use preview positions (already generated with current settings)
        let positions = self.ui.polygon_tool.preview_positions.clone();
        let sides = self.ui.polygon_tool.sides;
        let radius = self.ui.polygon_tool.radius;
        let height = self.ui.polygon_tool.height;

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        let shape_name = self.ui.polygon_tool.polygon_name();
        println!(
            "Placed {} ({} blocks, {} sides, R={}, H={})",
            shape_name, placed_count, sides, radius, height
        );

        // Don't deactivate tool - allow placing multiple polygons
    }

    /// Place bezier curve at the preview positions.
    pub fn place_bezier(&mut self) {
        let bezier = &self.ui.bezier_tool;
        if !bezier.active || bezier.preview_positions.is_empty() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Use preview positions (already generated with current settings)
        let positions = self.ui.bezier_tool.preview_positions.clone();
        let num_points = self.ui.bezier_tool.control_points.len();
        let tube_radius = self.ui.bezier_tool.tube_radius;

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        let curve_type = if num_points == 3 {
            "quadratic"
        } else {
            "cubic"
        };
        println!(
            "Placed {} Bezier curve ({} blocks, {} control points, tube R={})",
            curve_type, placed_count, num_points, tube_radius
        );

        // Clear control points for next curve, keep tool active
        self.ui.bezier_tool.clear();
    }

    /// Apply pattern fill to the selection.
    ///
    /// Uses hotbar slot 0 for Block A and slot 1 for Block B.
    pub fn apply_pattern_fill(&mut self) {
        let pattern = &self.ui.pattern_fill;
        if !pattern.active || pattern.preview_a.is_empty() {
            return;
        }

        // Get block types from hotbar slots 0 and 1
        let block_a = self.ui.hotbar_blocks[0];
        let tint_a = self.ui.hotbar_tint_indices[0];
        let paint_tex_a = self.ui.hotbar_paint_textures[0];

        let block_b = self.ui.hotbar_blocks[1];
        let tint_b = self.ui.hotbar_tint_indices[1];
        let paint_tex_b = self.ui.hotbar_paint_textures[1];

        // Create params for each block type
        let params_a = BlockPlacementParams::new(block_a, tint_a, paint_tex_a);
        let params_b = BlockPlacementParams::new(block_b, tint_b, paint_tex_b);

        // Clone positions before placement
        let positions_a = self.ui.pattern_fill.preview_a.clone();
        let positions_b = self.ui.pattern_fill.preview_b.clone();
        let pattern_type = self.ui.pattern_fill.pattern_type;

        // Place Block A positions
        let placed_a = place_blocks_at_positions(
            &positions_a,
            params_a,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Place Block B positions
        let placed_b = place_blocks_at_positions(
            &positions_b,
            params_b,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions_a.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        println!(
            "Applied {} pattern ({} + {} = {} blocks)",
            pattern_type.name(),
            placed_a,
            placed_b,
            placed_a + placed_b
        );

        // Don't deactivate tool - allow applying multiple patterns
    }

    /// Apply scatter brush placement at the given center position.
    ///
    /// Places blocks in a circular brush area with configurable density and height variation.
    /// Supports both regular blocks and model blocks.
    pub fn apply_scatter(&mut self, center: nalgebra::Vector3<i32>) {
        let scatter = &self.ui.scatter_tool;
        if !scatter.active {
            return;
        }

        // Get block type and model info from hotbar
        let block_type = self.ui.hotbar_blocks[self.ui.hotbar_index];
        let model_id = self.ui.hotbar_model_ids[self.ui.hotbar_index];
        let params = self.get_hotbar_placement_params();

        // Generate scatter positions
        let positions = if scatter.surface_only {
            // For surface mode, generate positions at the center Y
            crate::shape_tools::scatter::generate_scatter_positions(
                center,
                scatter.radius,
                scatter.density,
                scatter.seed(),
            )
        } else {
            // With height variation
            crate::shape_tools::scatter::generate_scatter_positions_with_height(
                center,
                scatter.radius,
                scatter.density,
                scatter.height_variation,
                scatter.seed(),
            )
        };

        // For surface-only mode, find actual surface positions
        let final_positions: Vec<_> = if self.ui.scatter_tool.surface_only {
            positions
                .into_iter()
                .filter_map(|pos| {
                    // Raycast downward to find surface
                    for dy in 0..20 {
                        let check_pos = nalgebra::Vector3::new(pos.x, pos.y - dy, pos.z);
                        if let Some(block) = self.sim.world.get_block(check_pos) {
                            let is_air = block == BlockType::Air;
                            let is_fluid = block == BlockType::Water || block == BlockType::Lava;
                            if !is_air && !is_fluid {
                                // Found solid block, place one above it
                                let place_pos = nalgebra::Vector3::new(
                                    check_pos.x,
                                    check_pos.y + 1,
                                    check_pos.z,
                                );
                                // Only place if position is air
                                if let Some(above) = self.sim.world.get_block(place_pos) {
                                    if above == BlockType::Air {
                                        return Some(place_pos);
                                    }
                                }
                            }
                        }
                    }
                    None
                })
                .collect()
        } else {
            positions
        };

        if final_positions.is_empty() {
            return;
        }

        // Place blocks - handle models specially
        let placed_count = if block_type == BlockType::Model && model_id > 0 {
            // Place model blocks
            let mut count = 0;
            for pos in &final_positions {
                if pos.y >= 0 && pos.y < crate::constants::TEXTURE_SIZE_Y as i32 {
                    self.sim.world.set_model_block(*pos, model_id, 0, false);
                    count += 1;
                }
            }
            count
        } else {
            // Place regular blocks using shared helper
            place_blocks_at_positions(
                &final_positions,
                params,
                &mut self.sim.world,
                &mut self.sim.water_grid,
                &mut self.sim.lava_grid,
            )
        };

        // Invalidate minimap cache
        if let Some(first_pos) = final_positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        if placed_count > 0 {
            if block_type == BlockType::Model {
                println!(
                    "Scattered {} models (R={}, D={}%)",
                    placed_count, self.ui.scatter_tool.radius, self.ui.scatter_tool.density
                );
            } else {
                println!(
                    "Scattered {} blocks (R={}, D={}%)",
                    placed_count, self.ui.scatter_tool.radius, self.ui.scatter_tool.density
                );
            }
        }
    }

    /// Apply hollow operation to remove interior blocks from selection.
    ///
    /// Removes all blocks in the interior of the selection, leaving a shell
    /// with the configured wall thickness.
    pub fn apply_hollow(&mut self) {
        let hollow = &self.ui.hollow_tool;
        if !hollow.active || hollow.preview_positions.is_empty() {
            return;
        }

        // Clone positions before mutating world
        let positions = self.ui.hollow_tool.preview_positions.clone();
        let thickness = self.ui.hollow_tool.thickness;

        let mut removed = 0;
        for pos in &positions {
            if let Some(block) = self.sim.world.get_block(*pos) {
                // Only remove non-air, non-fluid blocks
                if block != BlockType::Air && block != BlockType::Water && block != BlockType::Lava
                {
                    // Clear water/lava cells if present
                    self.sim.water_grid.remove_water(*pos, 999.0);
                    self.sim.lava_grid.remove_lava(*pos, 999.0);

                    // Remove the block (set to air)
                    self.sim.world.set_block(*pos, BlockType::Air);
                    removed += 1;
                }
            }
        }

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        println!(
            "Hollowed {} interior blocks (thickness={})",
            removed, thickness
        );

        // Don't deactivate tool - allow applying to other selections
    }

    /// Apply terrain brush at the given center position.
    ///
    /// Modifies terrain based on brush mode (raise, lower, smooth, flatten).
    pub fn apply_terrain_brush(&mut self, center: nalgebra::Vector3<i32>) {
        use crate::shape_tools::terrain_brush::{
            TerrainBrushMode, calculate_flatten_positions, calculate_lower_positions,
            calculate_raise_positions, calculate_smooth_positions,
        };

        let brush = &self.ui.terrain_brush;
        if !brush.active {
            return;
        }

        let radius = brush.radius;
        let strength = brush.strength;
        let mode = brush.mode;
        let shape = brush.shape;
        let target_y = brush.target_y;

        // Gather terrain heights within brush radius
        let mut heights = Vec::new();
        let r2 = (radius * radius) as f32;
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                let include = match shape {
                    crate::shape_tools::terrain_brush::BrushShape::Circle => {
                        (dx * dx + dz * dz) as f32 <= r2
                    }
                    crate::shape_tools::terrain_brush::BrushShape::Square => true,
                };
                if include {
                    let x = center.x + dx;
                    let z = center.z + dz;
                    // Find terrain height by scanning down
                    if let Some(height) = self.find_terrain_height_at(x, z, center.y + 20) {
                        heights.push((x, z, height));
                    }
                }
            }
        }

        if heights.is_empty() {
            return;
        }

        // Get block type from hotbar
        let hotbar_block = self.ui.hotbar_blocks[self.ui.hotbar_index];
        let block_type = if hotbar_block == BlockType::Air {
            BlockType::Dirt // Default to dirt if air selected
        } else {
            hotbar_block
        };

        match mode {
            TerrainBrushMode::Raise => {
                let positions =
                    calculate_raise_positions(center, radius, strength, shape, &heights);
                for pos in positions {
                    if let Some(existing) = self.sim.world.get_block(pos) {
                        if existing == BlockType::Air
                            || existing == BlockType::Water
                            || existing == BlockType::Lava
                        {
                            self.sim.world.set_block(pos, block_type);
                        }
                    }
                }
            }
            TerrainBrushMode::Lower => {
                let positions =
                    calculate_lower_positions(center, radius, strength, shape, &heights);
                for pos in positions {
                    if let Some(existing) = self.sim.world.get_block(pos) {
                        if existing != BlockType::Air && existing != BlockType::Bedrock {
                            // Clear water/lava cells if present
                            self.sim.water_grid.remove_water(pos, 999.0);
                            self.sim.lava_grid.remove_lava(pos, 999.0);
                            self.sim.world.set_block(pos, BlockType::Air);
                        }
                    }
                }
            }
            TerrainBrushMode::Smooth => {
                let (to_add, to_remove) =
                    calculate_smooth_positions(center, radius, shape, &heights);
                // Remove blocks first
                for pos in to_remove {
                    if let Some(existing) = self.sim.world.get_block(pos) {
                        if existing != BlockType::Air && existing != BlockType::Bedrock {
                            self.sim.water_grid.remove_water(pos, 999.0);
                            self.sim.lava_grid.remove_lava(pos, 999.0);
                            self.sim.world.set_block(pos, BlockType::Air);
                        }
                    }
                }
                // Then add blocks
                for pos in to_add {
                    if let Some(existing) = self.sim.world.get_block(pos) {
                        if existing == BlockType::Air {
                            self.sim.world.set_block(pos, block_type);
                        }
                    }
                }
            }
            TerrainBrushMode::Flatten => {
                let (to_add, to_remove) =
                    calculate_flatten_positions(center, radius, target_y, shape, &heights);
                // Remove blocks first
                for pos in to_remove {
                    if let Some(existing) = self.sim.world.get_block(pos) {
                        if existing != BlockType::Air && existing != BlockType::Bedrock {
                            self.sim.water_grid.remove_water(pos, 999.0);
                            self.sim.lava_grid.remove_lava(pos, 999.0);
                            self.sim.world.set_block(pos, BlockType::Air);
                        }
                    }
                }
                // Then add blocks
                for pos in to_add {
                    if let Some(existing) = self.sim.world.get_block(pos) {
                        if existing == BlockType::Air {
                            self.sim.world.set_block(pos, block_type);
                        }
                    }
                }
            }
        }

        // Invalidate minimap cache for affected area
        self.sim.world.invalidate_minimap_cache(center.x, center.z);
    }

    /// Find the terrain height at a given XZ position.
    fn find_terrain_height_at(&self, x: i32, z: i32, max_y: i32) -> Option<i32> {
        for y in (0..=max_y).rev() {
            if let Some(block) = self.sim.world.get_block(nalgebra::Vector3::new(x, y, z)) {
                if block != BlockType::Air && block != BlockType::Water && block != BlockType::Lava
                {
                    return Some(y);
                }
            }
        }
        None
    }

    /// Execute clone operation: copy blocks from selection to cloned positions.
    pub fn execute_clone(&mut self) {
        let clone_tool = &self.ui.clone_tool;
        if !clone_tool.active {
            return;
        }

        let selection = &self.ui.template_selection;
        if selection.pos1.is_none() || selection.pos2.is_none() {
            return;
        }

        let (min, max) = selection.bounds().unwrap();
        let selection_size = Vector3::new(
            (max.x - min.x + 1).abs(),
            (max.y - min.y + 1).abs(),
            (max.z - min.z + 1).abs(),
        );

        // Calculate clone origins
        let origins = crate::shape_tools::clone::calculate_clone_origins(
            selection_size,
            clone_tool.mode,
            clone_tool.axis,
            clone_tool.count,
            clone_tool.spacing,
            clone_tool.grid_count_x,
            clone_tool.grid_count_z,
            clone_tool.grid_spacing_x,
            clone_tool.grid_spacing_z,
            clone_tool.grid_count_y,
            clone_tool.grid_spacing_y,
        );

        // Skip the first origin (it's the original at 0,0,0)
        let clone_origins: Vec<_> = origins.into_iter().skip(1).collect();
        if clone_origins.is_empty() {
            println!("Clone: No copies to make (count=1)");
            return;
        }

        // Collect source blocks with their types and metadata
        // (position, block_type, tint_index, paint_data)
        #[allow(clippy::type_complexity)]
        let mut source_blocks: Vec<(
            Vector3<i32>,
            BlockType,
            Option<u8>,
            Option<crate::chunk::BlockPaintData>,
        )> = Vec::new();
        if let Some(iter) = selection.iter_positions() {
            for pos in iter {
                let block = self.sim.world.get_block(pos);
                if let Some(block_type) = block {
                    if block_type == BlockType::Air {
                        continue;
                    }
                    let tint = self.sim.world.get_tint_index(pos);
                    let paint = self.sim.world.get_paint_data(pos);
                    source_blocks.push((pos, block_type, tint, paint));
                }
            }
        }

        if source_blocks.is_empty() {
            println!("Clone: No blocks in selection to clone");
            return;
        }

        // Place cloned blocks at each origin offset
        let mut placed_count = 0;
        for origin in &clone_origins {
            for (source_pos, block_type, tint, paint) in &source_blocks {
                let target_pos = source_pos + origin;

                // Skip if out of Y bounds
                if target_pos.y < 0 || target_pos.y >= TEXTURE_SIZE_Y as i32 {
                    continue;
                }

                match *block_type {
                    BlockType::TintedGlass => {
                        let tint_idx: u8 = tint.unwrap_or(0);
                        self.sim.world.set_tinted_glass_block(target_pos, tint_idx);
                    }
                    BlockType::Crystal => {
                        let tint_idx: u8 = tint.unwrap_or(0);
                        self.sim.world.set_crystal_block(target_pos, tint_idx);
                    }
                    BlockType::Painted => {
                        if let Some(p) = paint {
                            self.sim.world.set_painted_block_full(
                                target_pos,
                                p.texture_idx,
                                p.tint_idx,
                                p.blend_mode,
                            );
                        } else {
                            self.sim.world.set_painted_block(target_pos, 0, 0);
                        }
                    }
                    BlockType::Water => {
                        let water_type = self
                            .sim
                            .world
                            .get_water_type(*source_pos)
                            .unwrap_or(WaterType::Ocean);
                        self.sim.water_grid.place_source(target_pos, water_type);
                        self.sim.world.set_water_block(target_pos, water_type);
                    }
                    BlockType::Lava => {
                        self.sim.lava_grid.place_source(target_pos);
                        self.sim.world.set_block(target_pos, BlockType::Lava);
                    }
                    BlockType::Model => {
                        // Clone model blocks with their metadata
                        if let Some(model_data) = self.sim.world.get_model_data(*source_pos) {
                            self.sim.world.set_model_block(
                                target_pos,
                                model_data.model_id,
                                model_data.rotation,
                                model_data.waterlogged,
                            );
                        }
                    }
                    BlockType::Air => {
                        // Skip air blocks
                        continue;
                    }
                    _ => {
                        self.sim.world.set_block(target_pos, *block_type);
                    }
                }
                placed_count += 1;
            }
        }

        // Invalidate minimap cache for affected area
        self.sim.world.invalidate_minimap_cache(min.x, min.z);
        self.sim.world.invalidate_minimap_cache(max.x, max.z);
        // Also invalidate cache for cloned regions
        for origin in &clone_origins {
            self.sim
                .world
                .invalidate_minimap_cache(min.x + origin.x, min.z + origin.z);
            self.sim
                .world
                .invalidate_minimap_cache(max.x + origin.x, max.z + origin.z);
        }

        let mode_name = self.ui.clone_tool.mode.name();
        println!(
            "Cloned {} blocks in {} mode ({} copies)",
            placed_count,
            mode_name,
            clone_origins.len()
        );

        // Clear preview after cloning
        self.ui.clone_tool.clear_preview();
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
                                let blend_mode =
                                    self.ui.paint_panel.current_config.blend_mode as u8;
                                self.sim.world.set_painted_block_full(
                                    pos,
                                    target_texture,
                                    target_tint,
                                    blend_mode,
                                );
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

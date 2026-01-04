use crate::App;
use crate::block_update::BlockUpdateType;
use crate::chunk::BlockType;
use crate::constants::TEXTURE_SIZE_Y;
use crate::player::{PLAYER_HALF_WIDTH, PLAYER_HEIGHT};
use crate::raycast::{MAX_RAYCAST_DISTANCE, get_place_position, raycast};
use crate::sub_voxel::{FIRST_CUSTOM_MODEL_ID, ModelRegistry, StairShape};
use nalgebra::Vector3;
use winit::event::MouseButton;

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

                    // Apply tint color for TintedGlass blocks
                    if block_type == BlockType::TintedGlass {
                        if let Some(tint_index) = self.sim.world.get_tint_index(target) {
                            let tint = crate::chunk::tint_color(tint_index);
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

                if is_waterlogged {
                    self.sim.world.set_block(target, BlockType::Water);
                } else {
                    self.sim.world.set_block(target, BlockType::Air);
                }
                self.sim.world.invalidate_minimap_cache(target.x, target.z);

                // Update neighboring fence/gate connections
                self.sim.world.update_fence_connections(target);
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

    pub fn update_block_placing(&mut self, delta_time: f32) {
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
            // Priority 1: Rotate existing custom model
            if self.rotate_custom_model_at(hit.block_pos) {
                self.ui.custom_rotate_needs_reclick = true;
                return;
            }

            // Priority 2: Toggle existing gate
            if !self.ui.gate_needs_reclick && self.toggle_gate_at(hit.block_pos) {
                self.ui.gate_needs_reclick = true;
                return;
            }

            // Priority 3: Place new block
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
            self.sim.water_grid.place_source(place_pos);
        } else if waterlogged {
            // Ensure water grid knows about waterlogged block (if not already there)
            if !self.sim.water_grid.has_water(place_pos) {
                self.sim.water_grid.place_source(place_pos);
            }
        } else {
            self.sim.water_grid.on_block_placed(place_pos);
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
                    self.sim.block_updates.enqueue(
                        final_pos + Vector3::new(0, 1, 0),
                        BlockUpdateType::Gravity,
                        player_pos,
                    );
                }
            }
        }
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

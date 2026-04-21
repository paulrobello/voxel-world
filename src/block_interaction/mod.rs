//! Block interaction system: placing, breaking, hotbar management, and palette UI.
//!
//! Handles all player-initiated block operations, including raycast targeting,
//! block placement and breaking with cooldown timers, hotbar slot selection,
//! line-locked building, and the block palette overlay. Also contains block
//! physics triggers (water source placement, falling block checks) and
//! multiplayer sync hooks.
//!
//! # Architecture
//!
//! All block interaction logic lives on [`BlockInteractionContext`], a short-lived
//! struct that borrows only the [`App`] fields it needs. The `impl App` block at the
//! bottom of this file contains thin one-liner delegates that construct the context
//! and forward the call, preserving the existing public API without requiring callers
//! to change.
//!
//! This breaks the God-Object borrow on `&mut App`: code that only needs `sim` and
//! `ui` no longer has to borrow the graphics pipeline or input system.
//!
//! ## Module layout
//!
//! | Submodule       | Responsibilities |
//! |-----------------|-----------------|
//! | `mod.rs`        | `BlockInteractionContext` struct, multiplayer sync helpers, raycast update, block breaking, block placing |
//! | `placement`     | `place_block_at` dispatcher, per-model-type helpers, toggle/repaint helpers |
//! | `shape_tools`   | All shape tool methods (sphere, cube, cylinder, wall, floor, …) |
//! | `physics`       | `process_landed_blocks`, `find_terrain_height_at` |

mod physics;
mod placement;
mod shape_tools;

use crate::app_state::{InputState, MultiplayerState, UiState, WorldSim};
use crate::block_update::BlockUpdateType;
use crate::chunk::BlockType;
use crate::constants::TEXTURE_SIZE_Y;
use crate::raycast::{MAX_RAYCAST_DISTANCE, get_place_position, raycast};
use crate::sub_voxel::ModelRegistry;
use nalgebra::Vector3;
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

/// Borrows only the [`App`] fields required by the block-interaction subsystem.
///
/// Construct via [`BlockInteractionContext::from_app`] or by calling any of the
/// thin delegate methods on `App` (which do so automatically).
pub(crate) struct BlockInteractionContext<'a> {
    pub sim: &'a mut WorldSim,
    pub ui: &'a mut UiState,
    pub input: &'a InputState,
    pub multiplayer: &'a mut MultiplayerState,
}

impl<'a> BlockInteractionContext<'a> {
    // ── Multiplayer sync helpers (mirrors the thin methods in app/core.rs) ──────

    pub(super) fn is_multiplayer(&self) -> bool {
        self.multiplayer.mode != crate::config::GameMode::SinglePlayer
    }

    pub(super) fn is_connected_to_server(&self) -> bool {
        self.multiplayer.is_connected()
    }

    pub(super) fn sync_block_placement(
        &mut self,
        position: [i32; 3],
        block: crate::net::protocol::BlockData,
    ) {
        if self.is_connected_to_server() {
            log::debug!("[Client] Syncing block placement at {:?}", position);
            self.multiplayer.send_place_block(position, block);
        }
    }

    pub(super) fn sync_block_break(&mut self, position: [i32; 3]) {
        if self.is_connected_to_server() {
            log::debug!("[Client] Syncing block break at {:?}", position);
            self.multiplayer.send_break_block(position);
        }
    }

    pub(super) fn sync_door_toggle(
        &mut self,
        lower_pos: [i32; 3],
        lower_block: crate::net::protocol::BlockData,
        upper_pos: [i32; 3],
        upper_block: crate::net::protocol::BlockData,
    ) {
        if self.is_connected_to_server() {
            log::debug!("[Client] Syncing door toggle at {:?}", lower_pos);
            self.multiplayer
                .send_toggle_door(lower_pos, lower_block, upper_pos, upper_block);
        }
    }

    pub(super) fn sync_water_source(
        &mut self,
        position: [i32; 3],
        water_type: crate::chunk::WaterType,
    ) {
        if self.multiplayer.is_host() {
            log::debug!("[Host] Broadcasting water source at {:?}", position);
            self.multiplayer
                .broadcast_water_source(position, water_type);
        }
    }

    /// Returns the currently selected block from the hotbar.
    pub(super) fn selected_block(&self) -> BlockType {
        self.ui.hotbar.hotbar_blocks[self.ui.hotbar.hotbar_index]
    }

    // ── Block interaction methods ────────────────────────────────────────────────
    pub fn update_raycast(&mut self) {
        // Camera uses normalized texture-relative coords (0-1), raycast needs world coords
        let origin = self
            .sim
            .player
            .camera_world_pos(self.sim.world_extent, self.sim.texture_origin)
            .cast::<f32>();
        let direction = self.sim.player.camera_direction().cast::<f32>();

        self.ui.placement.current_hit = raycast(
            &self.sim.world,
            &self.sim.model_registry,
            origin,
            direction,
            MAX_RAYCAST_DISTANCE,
        );
    }

    pub fn update_block_breaking(&mut self, delta_time: f32, holding_break: bool) -> bool {
        // Decrement cooldown timer
        if self.ui.placement.break_cooldown > 0.0 {
            self.ui.placement.break_cooldown -= delta_time;
            if self.ui.placement.break_cooldown < 0.0 {
                self.ui.placement.break_cooldown = 0.0;
            }
        }

        // Get the block we're looking at
        let target_block = self
            .ui
            .placement
            .current_hit
            .as_ref()
            .map(|hit| hit.block_pos);

        // If not holding break button or not looking at anything, reset
        if !holding_break || target_block.is_none() {
            self.ui.placement.breaking_block = None;
            self.ui.placement.break_progress = 0.0;
            return false;
        }

        // Don't start breaking if on cooldown (instant break mode)
        if self.ui.settings.instant_break && self.ui.placement.break_cooldown > 0.0 {
            return false;
        }

        let target = target_block.unwrap();

        // Get the block type to determine break time
        let block_type = self.sim.world.get_block(target).unwrap_or(BlockType::Air);
        let break_time = block_type.break_time();

        // Can't break air or water
        if break_time <= 0.0 {
            self.ui.placement.breaking_block = None;
            self.ui.placement.break_progress = 0.0;
            return false;
        }

        // If we're looking at a different block, reset progress
        if self.ui.placement.breaking_block != Some(target) {
            self.ui.placement.breaking_block = Some(target);
            self.ui.placement.break_progress = 0.0;
        }

        // Increment break progress (instant if enabled)
        if self.ui.settings.instant_break {
            self.ui.placement.break_progress = 1.0;
        } else {
            self.ui.placement.break_progress += delta_time / break_time;
        }

        // Check if block is fully broken
        if self.ui.placement.break_progress >= 1.0 {
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
                    } else if block_type == BlockType::Painted
                        && let Some(paint) = self.sim.world.get_paint_data(target)
                    {
                        let tint = crate::chunk::tint_color(paint.tint_idx);
                        particle_color = nalgebra::Vector3::new(tint[0], tint[1], tint[2]);
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
                if block_type == BlockType::Model
                    && let Some(model_data) = self.sim.world.get_model_data(target)
                {
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

                if is_waterlogged {
                    self.sim.world.set_block(target, BlockType::Water);
                } else {
                    self.sim.world.set_block(target, BlockType::Air);
                }
                self.sim.world.invalidate_minimap_cache(target.x, target.z);

                // Sync block break to server in multiplayer mode
                self.sync_block_break([target.x, target.y, target.z]);

                // Break mirrored positions if mirror tool is active
                if self.ui.mirror_tool.active && self.ui.mirror_tool.plane_set {
                    let mirrored_positions = self.ui.mirror_tool.mirror_position(target);
                    for mirrored_pos in mirrored_positions.into_iter().skip(1) {
                        // Skip if out of bounds
                        if mirrored_pos.y < 0 || mirrored_pos.y >= TEXTURE_SIZE_Y as i32 {
                            continue;
                        }
                        // Get the block at mirrored position
                        if let Some(mirrored_block) = self.sim.world.get_block(mirrored_pos)
                            && mirrored_block.break_time() > 0.0
                        {
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

                // Break other door half if present
                if let Some(other_pos) = other_door_half
                    && other_pos.y >= 0
                    && other_pos.y < TEXTURE_SIZE_Y as i32
                    && let Some(BlockType::Model) = self.sim.world.get_block(other_pos)
                    && let Some(other_data) = self.sim.world.get_model_data(other_pos)
                    && ModelRegistry::is_door_model(other_data.model_id)
                {
                    if other_data.waterlogged {
                        self.sim.world.set_block(other_pos, BlockType::Water);
                    } else {
                        self.sim.world.set_block(other_pos, BlockType::Air);
                    }
                    self.sim
                        .world
                        .invalidate_minimap_cache(other_pos.x, other_pos.z);
                }

                // Break other frame blocks if present
                for frame_pos in frame_blocks {
                    if frame_pos.y >= 0
                        && frame_pos.y < TEXTURE_SIZE_Y as i32
                        && let Some(BlockType::Model) = self.sim.world.get_block(frame_pos)
                        && let Some(frame_data) = self.sim.world.get_model_data(frame_pos)
                        && ModelRegistry::is_frame_model(frame_data.model_id)
                    {
                        // Spawn particles for frame block break
                        let frame_color = BlockType::Model.color();
                        let particle_color =
                            nalgebra::Vector3::new(frame_color[0], frame_color[1], frame_color[2]);
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
            self.ui.placement.breaking_block = None;
            self.ui.placement.break_progress = 0.0;

            // Set cooldown for instant break mode
            if self.ui.settings.instant_break {
                self.ui.placement.break_cooldown = self.ui.settings.break_cooldown_duration;
            }

            return true;
        }

        false
    }

    pub fn update_block_placing(&mut self, delta_time: f32) {
        // Skip block placement if in template/stencil placement mode, selection mode, rangefinder, flood fill, or shape tools
        // Note: mirror_tool is NOT in this list - it allows normal placement but mirrors the result
        if self.ui.active_placement.is_some()
            || self.ui.active_stencil_placement.is_some()
            || self.ui.template_selection.visual_mode
            || self.ui.placement.rangefinder_active
            || self.ui.placement.flood_fill_active
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
        if self.ui.placement.place_cooldown > 0.0 {
            self.ui.placement.place_cooldown -= delta_time;
        }

        let holding_place = self.input.mouse_held(MouseButton::Right);

        if !holding_place {
            // Reset all line building state when mouse released
            self.ui.placement.last_place_pos = None;
            self.ui.placement.line_start_pos = None;
            self.ui.placement.line_locked_axis = None;
            self.ui.placement.place_needs_reclick = false; // Allow block placement on next click
            self.ui.placement.model_needs_reclick = false; // Allow model placement on next click
            self.ui.placement.gate_needs_reclick = false; // Allow gate toggle on next click
            self.ui.placement.custom_rotate_needs_reclick = false; // Allow custom model rotation on next click
            return;
        }

        // If instant_place is disabled, require mouse release between placements
        if self.ui.placement.place_needs_reclick {
            return;
        }

        // If we just toggled a gate this hold, require a release before placing or toggling again.
        if self.ui.placement.gate_needs_reclick {
            return;
        }

        // If we just rotated a custom model, require a release before rotating again.
        if self.ui.placement.custom_rotate_needs_reclick {
            return;
        }

        if let Some(hit) = self.ui.placement.current_hit {
            // Check shift state early for custom model interactions
            let shift_held =
                self.input.key_held(KeyCode::ShiftLeft) || self.input.key_held(KeyCode::ShiftRight);

            // Priority 1a: Rotate existing custom model (Shift+Right-Click)
            if shift_held && self.rotate_custom_model_at(hit.block_pos) {
                self.ui.placement.custom_rotate_needs_reclick = true;
                return;
            }

            // Priority 1b: Stack model on top of existing custom model (Right-Click without Shift)
            // Check model_needs_reclick to prevent double-placement
            if !shift_held
                && !self.ui.placement.model_needs_reclick
                && self.stack_model_at(hit.block_pos)
            {
                // Don't set place_needs_reclick - let place_block_at handle model_needs_reclick
                return;
            }

            // Priority 2: Toggle existing door
            if !self.ui.placement.gate_needs_reclick && self.toggle_door_at(hit.block_pos) {
                self.ui.placement.gate_needs_reclick = true;
                // Sync door toggle to server in multiplayer mode
                // Read the new door state and send it to the server
                if self.is_connected_to_server() {
                    // Determine lower and upper positions
                    let model_data = self.sim.world.get_model_data(hit.block_pos);
                    let is_upper = model_data
                        .as_ref()
                        .map(|d| crate::sub_voxel::ModelRegistry::is_door_upper(d.model_id))
                        .unwrap_or(false);
                    let (lower_pos, upper_pos) = if is_upper {
                        (
                            hit.block_pos + nalgebra::Vector3::new(0, -1, 0),
                            hit.block_pos,
                        )
                    } else {
                        (
                            hit.block_pos,
                            hit.block_pos + nalgebra::Vector3::new(0, 1, 0),
                        )
                    };

                    // Get the new block data for both halves
                    let lower_model_data = self.sim.world.get_model_data(lower_pos);
                    let upper_model_data = self.sim.world.get_model_data(upper_pos);

                    let lower_block = crate::net::protocol::BlockData {
                        block_type: crate::chunk::BlockType::Model,
                        model_data: lower_model_data,
                        paint_data: None,
                        tint_index: None,
                        water_type: None,
                    };
                    let upper_block = crate::net::protocol::BlockData {
                        block_type: crate::chunk::BlockType::Model,
                        model_data: upper_model_data,
                        paint_data: None,
                        tint_index: None,
                        water_type: None,
                    };

                    self.sync_door_toggle(
                        [lower_pos.x, lower_pos.y, lower_pos.z],
                        lower_block,
                        [upper_pos.x, upper_pos.y, upper_pos.z],
                        upper_block,
                    );
                }
                return;
            }

            // Priority 3: Toggle existing trapdoor
            if !self.ui.placement.gate_needs_reclick && self.toggle_trapdoor_at(hit.block_pos) {
                self.ui.placement.gate_needs_reclick = true;
                return;
            }

            // Priority 4: Toggle existing gate
            if !self.ui.placement.gate_needs_reclick && self.toggle_gate_at(hit.block_pos) {
                self.ui.placement.gate_needs_reclick = true;
                return;
            }

            // Priority 5: Repaint existing Painted block (Shift+Right-Click while holding Painted)
            if self.selected_block() == BlockType::Painted
                && shift_held
                && !self.ui.placement.gate_needs_reclick
                && self.repaint_painted_block_at(hit.block_pos)
            {
                self.ui.placement.gate_needs_reclick = true;
                return;
            }

            // Priority 6: Place new block
            let place_pos = get_place_position(&hit);

            // Handle model blocks - require re-click to place multiple
            if self.selected_block() == BlockType::Model && self.ui.placement.model_needs_reclick {
                return;
            }

            // Handle line building (lock to axis)
            let mut constrained_pos = place_pos;
            if let Some(start) = self.ui.placement.line_start_pos {
                if let Some(axis) = self.ui.placement.line_locked_axis {
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
                        self.ui.placement.line_locked_axis = Some(0);
                        constrained_pos.y = start.y;
                        constrained_pos.z = start.z;
                    } else if diff.y.abs() > 0 {
                        self.ui.placement.line_locked_axis = Some(1);
                        constrained_pos.x = start.x;
                        constrained_pos.z = start.z;
                    } else if diff.z.abs() > 0 {
                        self.ui.placement.line_locked_axis = Some(2);
                        constrained_pos.x = start.x;
                        constrained_pos.y = start.y;
                    }
                }
            } else {
                // First block in a potential line
                self.ui.placement.line_start_pos = Some(place_pos);
            }

            // Continuous placing logic
            let can_place_new = self.ui.placement.last_place_pos != Some(constrained_pos)
                && self.ui.placement.place_cooldown <= 0.0;

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
                self.ui.placement.last_place_pos = Some(constrained_pos);
                self.ui.placement.place_cooldown = self.ui.settings.place_cooldown_duration;

                // Require re-click if instant_place is disabled
                if !self.ui.settings.instant_place {
                    self.ui.placement.place_needs_reclick = true;
                }
                // Model blocks require re-click, except fences which can be placed rapidly
                if self.selected_block() == BlockType::Model {
                    let model_id = self.ui.hotbar.hotbar_model_ids[self.ui.hotbar.hotbar_index];
                    if !ModelRegistry::is_fence_model(model_id) {
                        self.ui.placement.model_needs_reclick = true;
                    }
                }
            }
        }
    }
}

// ── Thin `impl App` delegates ─────────────────────────────────────────────────
//
// Each method constructs a [`BlockInteractionContext`] from the relevant `App`
// fields and forwards the call.  Callers in `input.rs`, `update.rs`, etc. are
// unchanged.

impl crate::App {
    #[inline]
    pub fn update_raycast(&mut self) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .update_raycast();
    }

    #[inline]
    pub fn update_block_breaking(&mut self, delta_time: f32, holding_break: bool) -> bool {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .update_block_breaking(delta_time, holding_break)
    }

    #[inline]
    pub fn update_block_placing(&mut self, delta_time: f32) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .update_block_placing(delta_time);
    }

    #[inline]
    pub fn process_landed_blocks(&mut self, landed: Vec<crate::falling_block::LandedBlock>) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .process_landed_blocks(landed);
    }

    #[inline]
    pub fn place_sphere(&mut self) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .place_sphere();
    }

    #[inline]
    pub fn place_cube(&mut self) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .place_cube();
    }

    #[inline]
    pub fn place_bridge(&mut self) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .place_bridge();
    }

    #[inline]
    pub fn place_cylinder(&mut self) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .place_cylinder();
    }

    #[inline]
    pub fn place_wall(&mut self) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .place_wall();
    }

    #[inline]
    pub fn place_floor(&mut self) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .place_floor();
    }

    #[inline]
    pub fn place_circle(&mut self) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .place_circle();
    }

    #[inline]
    pub fn place_stairs(&mut self) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .place_stairs();
    }

    #[inline]
    pub fn place_arch(&mut self) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .place_arch();
    }

    #[inline]
    pub fn place_cone(&mut self) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .place_cone();
    }

    #[inline]
    pub fn place_torus(&mut self) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .place_torus();
    }

    #[inline]
    pub fn place_helix(&mut self) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .place_helix();
    }

    #[inline]
    pub fn place_polygon(&mut self) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .place_polygon();
    }

    #[inline]
    pub fn place_bezier(&mut self) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .place_bezier();
    }

    #[inline]
    pub fn apply_pattern_fill(&mut self) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .apply_pattern_fill();
    }

    #[inline]
    pub fn apply_scatter(&mut self, center: nalgebra::Vector3<i32>) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .apply_scatter(center);
    }

    #[inline]
    pub fn apply_hollow(&mut self) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .apply_hollow();
    }

    #[inline]
    pub fn apply_terrain_brush(&mut self, center: nalgebra::Vector3<i32>) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .apply_terrain_brush(center);
    }

    #[inline]
    pub fn execute_clone(&mut self) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .execute_clone();
    }

    #[inline]
    pub fn execute_replace(&mut self) {
        BlockInteractionContext {
            sim: &mut self.sim,
            ui: &mut self.ui,
            input: &self.input,
            multiplayer: &mut self.multiplayer,
        }
        .execute_replace();
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

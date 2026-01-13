use super::App;
use crate::chunk::BlockType;
use crate::raycast::get_place_position;
use nalgebra::Vector3;
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

const ATLAS_TILE_COUNT: u8 = 19;

impl App {
    /// Handle focus/unfocus toggles. Returns true if we should early-return from update.
    pub fn handle_focus_toggles(&mut self) -> bool {
        // Close palette with Escape (restores focus if it was focused before opening)
        if self.input.key_pressed(KeyCode::Escape) && self.ui.palette_open {
            self.ui.palette_open = false;
            self.ui.dragging_item = None;
            // Only restore focus if no other cursor-needing panels are still open
            let other_panel_open = self.ui.editor.active || self.ui.console.active;
            if !other_panel_open && self.ui.palette_previously_focused {
                self.input.focused = true;
                self.input.pending_grab = Some(true);
                self.input.skip_input_frame = true;
                self.ui.palette_previously_focused = false;
            }
            return true;
        }

        // Close editor with Escape (restores focus if it was focused before opening)
        if self.input.key_pressed(KeyCode::Escape) && self.ui.editor.active {
            self.ui.editor.active = false;
            // Only restore focus if no other cursor-needing panels are still open
            let other_panel_open = self.ui.palette_open || self.ui.console.active;
            if !other_panel_open && self.ui.editor_previously_focused {
                self.input.focused = true;
                self.input.pending_grab = Some(true);
                self.input.skip_input_frame = true;
                self.ui.editor_previously_focused = false;
            }
            println!("Model editor: OFF");
            return true;
        }

        // Close console with Escape (restores focus if it was focused before opening)
        if self.input.key_pressed(KeyCode::Escape) && self.ui.console.active {
            self.ui.console.close();
            // Only restore focus if no other cursor-needing panels are still open
            let other_panel_open = self.ui.palette_open || self.ui.editor.active;
            if !other_panel_open && self.ui.console_previously_focused {
                self.input.focused = true;
                self.input.pending_grab = Some(true);
                self.input.skip_input_frame = true;
                self.ui.console_previously_focused = false;
            }
            return true;
        }

        // Cancel template placement
        if self.input.key_pressed(KeyCode::Escape) && self.ui.active_placement.is_some() {
            if let Some(ref placement) = self.ui.active_placement {
                println!("Cancelled template placement: {}", placement.template.name);
            }
            self.ui.active_placement = None;
            return true;
        }

        // Cancel stencil placement
        if self.input.key_pressed(KeyCode::Escape) && self.ui.active_stencil_placement.is_some() {
            if let Some(ref placement) = self.ui.active_stencil_placement {
                println!("Cancelled stencil placement: {}", placement.stencil.name);
            }
            self.ui.active_stencil_placement = None;
            return true;
        }

        // Cancel flood fill mode
        if self.input.key_pressed(KeyCode::Escape) && self.ui.flood_fill_active {
            println!("Flood Fill Mode: OFF");
            self.ui.flood_fill_active = false;
            return true;
        }

        // Cancel sphere tool
        if self.input.key_pressed(KeyCode::Escape) && self.ui.sphere_tool.active {
            println!("Sphere Tool: OFF");
            self.ui.sphere_tool.deactivate();
            return true;
        }

        // Cancel cube tool
        if self.input.key_pressed(KeyCode::Escape) && self.ui.cube_tool.active {
            println!("Cube Tool: OFF");
            self.ui.cube_tool.deactivate();
            return true;
        }

        // Cancel bridge tool
        if self.input.key_pressed(KeyCode::Escape) && self.ui.bridge_tool.active {
            println!("Bridge Tool: OFF");
            self.ui.bridge_tool.deactivate();
            return true;
        }

        // Handle escape to unfocus
        if self.input.key_pressed(KeyCode::Escape) && self.input.focused {
            self.input.focused = false;
            self.input.pending_grab = Some(false);
            println!("Unfocused - cursor will be released");
        }

        // Handle focus toggling - click to focus (don't process this click for gameplay)
        // Only allow focusing if no panels are open that need the cursor
        // Note: tools_palette is excluded - it's a passive overlay that doesn't need cursor
        let panel_open = self.ui.palette_open
            || self.ui.editor.active
            || self.ui.console.active
            || self.ui.template_ui.browser_open
            || self.ui.stencil_ui.browser_open;

        if !self.input.focused && self.input.mouse_pressed(MouseButton::Left) && !panel_open {
            println!("Focus click...");
            self.input.focused = true;
            self.input.pending_grab = Some(true);
            // Skip block breaking until mouse is released to avoid breaking on focus click
            self.ui.skip_break_until_release = true;
            // Skip input processing on next frame to prevent stale key presses
            self.input.skip_input_frame = true;
            println!("Focus complete - cursor will be grabbed");
            return true; // skip rest of update for this frame
        }
        false
    }

    /// Handles movement, toggles, and block placing when focused.
    pub fn handle_focused_controls(&mut self, delta_time: f64) {
        if !self.input.focused {
            return;
        }

        // Skip input processing on first frame after focus regain to prevent stale key presses
        if self.input.skip_input_frame {
            self.input.skip_input_frame = false;
            // Still update physics with no input changes to maintain smooth movement
            self.sim.player.update_physics(
                delta_time,
                &self.sim.world,
                self.sim.world_extent,
                self.sim.texture_origin,
                &self.input,
                &self.sim.model_registry,
                self.args.verbose,
                self.ui.settings.collision_enabled_fly,
            );
            return;
        }

        // Update player physics (movement, gravity, collisions)
        self.sim.player.update_physics(
            delta_time,
            &self.sim.world,
            self.sim.world_extent,
            self.sim.texture_origin,
            &self.input,
            &self.sim.model_registry,
            self.args.verbose,
            self.ui.settings.collision_enabled_fly,
        );

        // Mouse look
        let sens = 0.002 * (self.sim.player.camera.fov.to_radians() * 0.5).tan();

        let (dx, dy) = self.input.mouse_diff();
        // rotation.y = yaw (horizontal), rotation.x = pitch (vertical)
        self.sim.player.camera.rotation.y -= dx as f64 * sens;
        self.sim.player.camera.rotation.x -= dy as f64 * sens;
        self.sim.player.camera.rotation.x = self
            .sim
            .player
            .camera
            .rotation
            .x
            .clamp(-std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2);
        self.sim.player.camera.rotation.y = self
            .sim
            .player
            .camera
            .rotation
            .y
            .rem_euclid(std::f64::consts::TAU);

        // Scroll wheel to cycle through hotbar slots
        let ds = self.input.scroll_diff();
        if ds.1.abs() > 0.1 {
            self.ui.hotbar_index = if ds.1 > 0.0 {
                (self.ui.hotbar_index + self.ui.hotbar_blocks.len() - 1)
                    % self.ui.hotbar_blocks.len()
            } else {
                (self.ui.hotbar_index + 1) % self.ui.hotbar_blocks.len()
            };
        }

        // Number keys 1-9 to select hotbar slot
        if self.input.key_pressed(KeyCode::Digit1) {
            self.ui.hotbar_index = 0;
        }
        if self.input.key_pressed(KeyCode::Digit2) {
            self.ui.hotbar_index = 1;
        }
        if self.input.key_pressed(KeyCode::Digit3) {
            self.ui.hotbar_index = 2;
        }
        if self.input.key_pressed(KeyCode::Digit4) {
            self.ui.hotbar_index = 3;
        }
        if self.input.key_pressed(KeyCode::Digit5) {
            self.ui.hotbar_index = 4;
        }
        if self.input.key_pressed(KeyCode::Digit6) {
            self.ui.hotbar_index = 5;
        }
        if self.input.key_pressed(KeyCode::Digit7) {
            self.ui.hotbar_index = 6;
        }
        if self.input.key_pressed(KeyCode::Digit8) {
            self.ui.hotbar_index = 7;
        }
        if self.input.key_pressed(KeyCode::Digit9) {
            self.ui.hotbar_index = 8;
        }

        // Toggle fly mode (F key)
        if self.input.key_pressed(KeyCode::KeyF) {
            let new_mode = !self.sim.player.fly_mode;
            self.sim.player.fly_mode = new_mode;
            if !new_mode {
                // Dropping out of fly: clear any overlap and reset vertical velocity.
                self.resolve_player_overlap();
                self.sim.player.velocity.y = 0.0;
                self.sim.player.on_ground = false;
            } else {
                // Entering fly: zero velocity to avoid lingering gravity impulses.
                self.sim.player.velocity = Vector3::zeros();
            }
            println!("Fly mode: {}", if new_mode { "ON" } else { "OFF" });
        }

        // Toggle sprint mode (Left Control)
        if self.input.key_pressed(KeyCode::ControlLeft) {
            self.sim.player.sprint_mode = !self.sim.player.sprint_mode;
            println!(
                "Sprint mode: {}",
                if self.sim.player.sprint_mode {
                    "ON"
                } else {
                    "OFF"
                }
            );
        }

        // Toggle player torch light (J key)
        if self.input.key_pressed(KeyCode::KeyJ) {
            self.sim.player.light_enabled = !self.sim.player.light_enabled;
            if self.sim.player.light_enabled {
                if self.ui.settings.enable_point_lights {
                    println!("Torch light: ON");
                } else {
                    println!(
                        "Torch light: ON (but Point Lights are disabled in settings - press Esc to enable)"
                    );
                }
            } else {
                println!("Torch light: OFF");
            }
        }

        // Toggle chunk boundary debug (B key)
        if self.input.key_pressed(KeyCode::KeyB) {
            self.ui.settings.show_chunk_boundaries = !self.ui.settings.show_chunk_boundaries;
            println!(
                "Chunk boundaries: {}",
                if self.ui.settings.show_chunk_boundaries {
                    "ON"
                } else {
                    "OFF"
                }
            );
        }

        // Toggle debug cutaway mode (C key) - hides chunk in front of player for cave exploration
        if self.input.key_pressed(KeyCode::KeyC) {
            self.ui.settings.debug_cutaway_enabled = !self.ui.settings.debug_cutaway_enabled;
            println!(
                "Debug cutaway: {}",
                if self.ui.settings.debug_cutaway_enabled {
                    "ON (chunk in front hidden)"
                } else {
                    "OFF"
                }
            );
        }

        // Toggle minimap (M key)
        if self.input.key_pressed(KeyCode::KeyM) {
            self.ui.show_minimap = !self.ui.show_minimap;
            println!(
                "Minimap: {}",
                if self.ui.show_minimap { "ON" } else { "OFF" }
            );
        }

        // Toggle laser rangefinder mode (G key)
        if self.input.key_pressed(KeyCode::KeyG) {
            self.ui.rangefinder_active = !self.ui.rangefinder_active;
            println!(
                "Rangefinder: {}",
                if self.ui.rangefinder_active {
                    "ON"
                } else {
                    "OFF"
                }
            );
        }

        // Toggle water/lava source debug markers (K key)
        if self.input.key_pressed(KeyCode::KeyK) {
            self.ui.settings.show_water_sources = !self.ui.settings.show_water_sources;
            if self.ui.settings.show_water_sources {
                // Count true sources (cells with is_source == true)
                let water_sources = self
                    .sim
                    .water_grid
                    .iter()
                    .filter(|(_, c)| c.is_source)
                    .count();
                let lava_sources = self
                    .sim
                    .lava_grid
                    .iter()
                    .filter(|(_, c)| c.is_source)
                    .count();

                println!(
                    "Water/Lava sources: ON ({} water sources, {} lava sources)",
                    water_sources, lava_sources
                );
            } else {
                println!("Water/Lava sources: OFF");
            }
        }

        // Toggle template selection mode (V key)
        if self.input.key_pressed(KeyCode::KeyV) {
            self.ui.template_selection.visual_mode = !self.ui.template_selection.visual_mode;
            if self.ui.template_selection.visual_mode {
                println!("Template selection mode: ON (Left-click for pos1, Right-click for pos2)");
                // Clear existing selection when entering mode
                self.ui.template_selection.pos1 = None;
                self.ui.template_selection.pos2 = None;
            } else {
                println!("Template selection mode: OFF");
            }
        }

        // Block placing - continuous when holding right mouse button
        self.update_block_placing(delta_time as f32);
    }

    /// Processes block breaking and middle-click pick interactions.
    pub fn handle_block_interactions(&mut self, delta_time: f32) {
        // Update raycast for block selection
        self.update_raycast();

        // Handle selection marker placement (takes priority in selection mode)
        if self.ui.template_selection.visual_mode && self.input.focused {
            if let Some(hit) = self.ui.current_hit {
                // Use placement position (adjacent to hit face) instead of block position
                let pos = get_place_position(&hit);

                // Left-click sets pos1 (green marker)
                if self.input.mouse_pressed(MouseButton::Left) {
                    self.ui.template_selection.set_pos1(pos);
                    println!("Selection pos1 set to ({}, {}, {})", pos.x, pos.y, pos.z);
                    if let Some((min, max)) = self.ui.template_selection.bounds() {
                        if let Some((w, h, d)) = self.ui.template_selection.dimensions() {
                            println!(
                                "Selection: {}×{}×{} (from ({},{},{}) to ({},{},{}))",
                                w, h, d, min.x, min.y, min.z, max.x, max.y, max.z
                            );
                        }
                    }
                    // Skip normal block breaking when in selection mode
                    self.ui.skip_break_until_release = true;
                }

                // Right-click sets pos2 (blue marker)
                if self.input.mouse_pressed(MouseButton::Right) {
                    self.ui.template_selection.set_pos2(pos);
                    println!("Selection pos2 set to ({}, {}, {})", pos.x, pos.y, pos.z);
                    if let Some((min, max)) = self.ui.template_selection.bounds() {
                        if let Some((w, h, d)) = self.ui.template_selection.dimensions() {
                            println!(
                                "Selection: {}×{}×{} (from ({},{},{}) to ({},{},{}))",
                                w, h, d, min.x, min.y, min.z, max.x, max.y, max.z
                            );
                        }
                    }
                    // Skip normal block placing when in selection mode
                    self.ui.place_needs_reclick = true;
                }
            }

            // Skip remaining interactions when in selection mode
            if self.input.mouse_pressed(MouseButton::Left)
                || self.input.mouse_pressed(MouseButton::Right)
            {
                return;
            }
        }

        // Handle measurement marker placement (when rangefinder is active)
        if self.ui.rangefinder_active
            && self.input.focused
            && !self.ui.template_selection.visual_mode
        {
            // Left-click adds a marker at the hit block position
            if self.input.mouse_pressed(MouseButton::Left) {
                if let Some(hit) = self.ui.current_hit {
                    // Use the actual block position (not placement position)
                    let pos = hit.block_pos;

                    // Check if marker already exists at this position
                    if !self.ui.measurement_markers.contains(&pos) {
                        if self.ui.measurement_markers.len() < 4 {
                            self.ui.measurement_markers.push(pos);
                            println!(
                                "Measurement marker {} placed at ({}, {}, {})",
                                self.ui.measurement_markers.len(),
                                pos.x,
                                pos.y,
                                pos.z
                            );

                            // Show distance to previous marker
                            if self.ui.measurement_markers.len() >= 2 {
                                let prev = self.ui.measurement_markers
                                    [self.ui.measurement_markers.len() - 2];
                                let dist = ((pos.x - prev.x).pow(2)
                                    + (pos.y - prev.y).pow(2)
                                    + (pos.z - prev.z).pow(2))
                                    as f32;
                                let dist = dist.sqrt();
                                println!(
                                    "  Distance from marker {}: {:.1} blocks",
                                    self.ui.measurement_markers.len() - 1,
                                    dist
                                );
                            }
                        } else {
                            println!(
                                "Maximum 4 measurement markers reached. Right-click to remove."
                            );
                        }
                    }
                    // Skip normal block breaking
                    self.ui.skip_break_until_release = true;
                }
            }

            // Right-click removes the last marker
            if self.input.mouse_pressed(MouseButton::Right) {
                if let Some(removed) = self.ui.measurement_markers.pop() {
                    println!(
                        "Removed measurement marker at ({}, {}, {}). {} markers remaining.",
                        removed.x,
                        removed.y,
                        removed.z,
                        self.ui.measurement_markers.len()
                    );
                }
                // Skip normal block placing
                self.ui.place_needs_reclick = true;
            }

            // Skip remaining interactions when in rangefinder mode with click
            if self.input.mouse_pressed(MouseButton::Left)
                || self.input.mouse_pressed(MouseButton::Right)
            {
                return;
            }
        }

        // Update template placement position from raycast
        if let Some(ref mut placement) = self.ui.active_placement {
            if let Some(hit) = self.ui.current_hit {
                let place_pos = get_place_position(&hit);
                placement.update_position_from_raycast(place_pos);
            }
        }

        // Update stencil placement position from raycast
        if let Some(ref mut placement) = self.ui.active_stencil_placement {
            if let Some(hit) = self.ui.current_hit {
                let place_pos = get_place_position(&hit);
                placement.update_position_from_raycast(place_pos);
            }
        }

        // Update sphere tool preview from raycast
        if self.ui.sphere_tool.active {
            if let Some(hit) = self.ui.current_hit {
                // For Base mode, use hit block position (sphere sits ON the block)
                // For Center mode, use placement position (where block would be placed)
                let target = if self.ui.sphere_tool.placement_mode
                    == crate::shape_tools::PlacementMode::Base
                {
                    // Base mode: sphere bottom rests on top of hit block
                    // So target is one above hit block (first air block)
                    hit.block_pos + Vector3::new(0, 1, 0)
                } else {
                    get_place_position(&hit)
                };
                self.ui.sphere_tool.update_preview(target);
            } else {
                self.ui.sphere_tool.clear_preview();
            }
        }

        // Update cube tool preview from raycast
        if self.ui.cube_tool.active {
            if let Some(hit) = self.ui.current_hit {
                // For Base mode, use hit block position (cube sits ON the block)
                // For Center mode, use placement position (where block would be placed)
                let target = if self.ui.cube_tool.placement_mode
                    == crate::shape_tools::PlacementMode::Base
                {
                    // Base mode: cube bottom rests on top of hit block
                    // So target is one above hit block (first air block)
                    hit.block_pos + Vector3::new(0, 1, 0)
                } else {
                    get_place_position(&hit)
                };
                self.ui.cube_tool.update_preview(target);
            } else {
                self.ui.cube_tool.clear_preview();
            }
        }

        // Update cylinder tool preview from raycast
        if self.ui.cylinder_tool.active {
            if let Some(hit) = self.ui.current_hit {
                // For Base mode, use hit block position (cylinder sits ON the block)
                // For Center mode, use placement position (where block would be placed)
                let target = if self.ui.cylinder_tool.placement_mode
                    == crate::shape_tools::PlacementMode::Base
                {
                    // Base mode: cylinder bottom rests on top of hit block
                    // So target is one above hit block (first air block)
                    hit.block_pos + Vector3::new(0, 1, 0)
                } else {
                    get_place_position(&hit)
                };
                self.ui.cylinder_tool.update_preview(target);
            } else {
                self.ui.cylinder_tool.clear_preview();
            }
        }

        // Update bridge tool preview from raycast (only when start position is set)
        if self.ui.bridge_tool.active && self.ui.bridge_tool.start_position.is_some() {
            if let Some(hit) = self.ui.current_hit {
                let target = get_place_position(&hit);
                self.ui.bridge_tool.update_preview(target);
            } else {
                self.ui.bridge_tool.clear_preview();
            }
        }

        // Update wall tool preview from raycast (only when start position is set)
        if self.ui.wall_tool.active && self.ui.wall_tool.start_position.is_some() {
            if let Some(hit) = self.ui.current_hit {
                let target = get_place_position(&hit);
                self.ui.wall_tool.update_preview(target);
            } else {
                self.ui.wall_tool.clear_preview();
            }
        }

        // Update floor tool preview from raycast (only when start position is set)
        if self.ui.floor_tool.active && self.ui.floor_tool.start_position.is_some() {
            if let Some(hit) = self.ui.current_hit {
                let target = get_place_position(&hit);
                self.ui.floor_tool.update_preview(target);
            } else {
                self.ui.floor_tool.clear_preview();
            }
        }

        // Update circle tool preview from raycast
        if self.ui.circle_tool.active {
            if let Some(hit) = self.ui.current_hit {
                let target = get_place_position(&hit);
                self.ui.circle_tool.update_preview(target);
            } else {
                self.ui.circle_tool.clear_preview();
            }
        }

        // Update stairs tool preview from raycast (only when start position is set)
        if self.ui.stairs_tool.active && self.ui.stairs_tool.start_pos.is_some() {
            if let Some(hit) = self.ui.current_hit {
                let target = get_place_position(&hit);
                self.ui.stairs_tool.update_preview(target);
            } else {
                self.ui.stairs_tool.clear_preview();
            }
        }

        // Handle replace tool preview and execution requests
        if self.ui.replace_tool.active {
            if self.ui.replace_tool.preview_requested {
                self.ui.replace_tool.preview_requested = false;
                self.ui
                    .replace_tool
                    .update_preview(&self.sim.world, &self.ui.template_selection);
            }
            if self.ui.replace_tool.execute_requested {
                self.ui.replace_tool.execute_requested = false;
                self.execute_replace();
            }
        }

        // Handle template placement with right-click
        // Check if mouse was released (clear reclick flag)
        if self.ui.place_needs_reclick && !self.input.mouse_held(MouseButton::Right) {
            self.ui.place_needs_reclick = false;
        }

        if self.input.focused
            && self.ui.active_placement.is_some()
            && self.input.mouse_pressed(MouseButton::Right)
            && !self.ui.place_needs_reclick
        {
            if let Some(ref mut placement) = self.ui.active_placement {
                // Place blocks in batches until complete
                const BATCH_SIZE: usize = 1000;
                while !placement.place_batch(
                    &mut self.sim.world,
                    &mut self.sim.water_grid,
                    BATCH_SIZE,
                ) {
                    // Continue until complete
                }

                println!(
                    "Placed template '{}' ({} blocks) at ({}, {}, {})",
                    placement.template.name,
                    placement.template.block_count(),
                    placement.position.x,
                    placement.position.y,
                    placement.position.z
                );
            }

            // Clear active placement and require mouse release before next action
            self.ui.active_placement = None;
            self.ui.place_needs_reclick = true;
            return; // Skip block breaking
        }

        // Handle stencil placement with right-click
        if self.input.focused
            && self.ui.active_stencil_placement.is_some()
            && self.input.mouse_pressed(MouseButton::Right)
            && !self.ui.place_needs_reclick
        {
            if let Some(placement) = self.ui.active_stencil_placement.take() {
                let stencil_name = placement.stencil.name.clone();
                let position_count = placement.stencil.position_count();
                let pos = placement.position;

                // Add stencil to manager with current position and rotation
                let placed = placement.commit(
                    0, // ID will be assigned by manager
                    self.ui.stencil_manager.default_color,
                    self.ui.stencil_manager.global_opacity,
                );
                self.ui.stencil_manager.active_stencils.push(placed);

                // Update next_id
                if let Some(last) = self.ui.stencil_manager.active_stencils.last() {
                    self.ui.stencil_manager.next_id = last.id + 1;
                }

                println!(
                    "Placed stencil '{}' ({} positions) at ({}, {}, {})",
                    stencil_name, position_count, pos.x, pos.y, pos.z
                );
            }

            self.ui.place_needs_reclick = true;
            return; // Skip block breaking
        }

        // Handle sphere placement with right-click
        if self.input.focused
            && self.ui.sphere_tool.active
            && !self.ui.sphere_tool.preview_positions.is_empty()
            && self.input.mouse_pressed(MouseButton::Right)
            && !self.ui.place_needs_reclick
        {
            self.place_sphere();
            self.ui.place_needs_reclick = true;
            return; // Skip block placement
        }

        // Handle cube placement with right-click
        if self.input.focused
            && self.ui.cube_tool.active
            && !self.ui.cube_tool.preview_positions.is_empty()
            && self.input.mouse_pressed(MouseButton::Right)
            && !self.ui.place_needs_reclick
        {
            self.place_cube();
            self.ui.place_needs_reclick = true;
            return; // Skip block placement
        }

        // Handle cylinder placement with right-click
        if self.input.focused
            && self.ui.cylinder_tool.active
            && !self.ui.cylinder_tool.preview_positions.is_empty()
            && self.input.mouse_pressed(MouseButton::Right)
            && !self.ui.place_needs_reclick
        {
            self.place_cylinder();
            self.ui.place_needs_reclick = true;
            return; // Skip block placement
        }

        // Handle bridge tool with right-click (two-click workflow)
        if self.input.focused
            && self.ui.bridge_tool.active
            && self.input.mouse_pressed(MouseButton::Right)
            && !self.ui.place_needs_reclick
        {
            if let Some(hit) = self.ui.current_hit {
                let target = get_place_position(&hit);

                if self.ui.bridge_tool.start_position.is_some() {
                    // Second click - place the bridge and clear start
                    self.place_bridge();
                    self.ui.bridge_tool.cancel(); // Clear start position for next bridge
                } else {
                    // First click - set start position
                    self.ui.bridge_tool.start_position = Some(target);
                    println!("Bridge start: ({}, {}, {})", target.x, target.y, target.z);
                }
                self.ui.place_needs_reclick = true;
                return; // Skip block placement
            }
        }

        // Handle wall tool with right-click (two-click workflow)
        if self.input.focused
            && self.ui.wall_tool.active
            && self.input.mouse_pressed(MouseButton::Right)
            && !self.ui.place_needs_reclick
        {
            if let Some(hit) = self.ui.current_hit {
                let target = get_place_position(&hit);

                if self.ui.wall_tool.start_position.is_some() {
                    // Second click - place the wall and clear start
                    self.place_wall();
                    self.ui.wall_tool.cancel(); // Clear start position for next wall
                } else {
                    // First click - set start position
                    self.ui.wall_tool.start_position = Some(target);
                    println!("Wall start: ({}, {}, {})", target.x, target.y, target.z);
                }
                self.ui.place_needs_reclick = true;
                return; // Skip block placement
            }
        }

        // Handle floor tool with right-click (two-click workflow)
        if self.input.focused
            && self.ui.floor_tool.active
            && self.input.mouse_pressed(MouseButton::Right)
            && !self.ui.place_needs_reclick
        {
            if let Some(hit) = self.ui.current_hit {
                let target = get_place_position(&hit);

                if self.ui.floor_tool.start_position.is_some() {
                    // Second click - place the floor and clear start
                    self.place_floor();
                    self.ui.floor_tool.cancel(); // Clear start position for next floor
                } else {
                    // First click - set start position
                    self.ui.floor_tool.start_position = Some(target);
                    println!("Floor start: ({}, {}, {})", target.x, target.y, target.z);
                }
                self.ui.place_needs_reclick = true;
                return; // Skip block placement
            }
        }

        // Handle circle placement with right-click
        if self.input.focused
            && self.ui.circle_tool.active
            && !self.ui.circle_tool.preview_positions.is_empty()
            && self.input.mouse_pressed(MouseButton::Right)
            && !self.ui.place_needs_reclick
        {
            self.place_circle();
            self.ui.place_needs_reclick = true;
            return; // Skip block placement
        }

        // Handle stairs tool with right-click (two-click workflow)
        if self.input.focused
            && self.ui.stairs_tool.active
            && self.input.mouse_pressed(MouseButton::Right)
            && !self.ui.place_needs_reclick
        {
            if let Some(hit) = self.ui.current_hit {
                let target = get_place_position(&hit);

                if self.ui.stairs_tool.start_pos.is_some() {
                    // Second click - place the stairs and clear start
                    self.place_stairs();
                    self.ui.stairs_tool.reset(); // Clear start position for next staircase
                } else {
                    // First click - set start position
                    self.ui.stairs_tool.start_pos = Some(target);
                    println!("Stairs start: ({}, {}, {})", target.x, target.y, target.z);
                }
                self.ui.place_needs_reclick = true;
                return; // Skip block placement
            }
        }

        // Handle mirror tool axis cycling with Tab key
        if self.input.focused && self.ui.mirror_tool.active && self.input.key_pressed(KeyCode::Tab)
        {
            self.ui.mirror_tool.cycle_axis();
            println!("Mirror axis: {}", self.ui.mirror_tool.axis.name());
        }

        // Handle mirror tool plane setting with right-click
        if self.input.focused
            && self.ui.mirror_tool.active
            && self.input.mouse_pressed(MouseButton::Right)
            && !self.ui.place_needs_reclick
        {
            if let Some(hit) = self.ui.current_hit {
                let target = get_place_position(&hit);
                self.ui.mirror_tool.set_plane(target);
                println!(
                    "Mirror plane set at ({}, {}, {})",
                    target.x, target.y, target.z
                );
            }
            self.ui.place_needs_reclick = true;
            return; // Skip block placement
        }

        // Handle flood fill with right-click
        if self.input.focused
            && self.ui.flood_fill_active
            && self.input.mouse_pressed(MouseButton::Right)
            && !self.ui.place_needs_reclick
        {
            if let Some(hit) = self.ui.current_hit {
                let target_block = self.ui.hotbar_blocks[self.ui.hotbar_index];
                let start_pos = hit.block_pos;

                // Call the flood fill function
                let player_pos = Vector3::new(
                    self.sim.player.camera.position.x.floor() as i32,
                    self.sim.player.camera.position.y.floor() as i32,
                    self.sim.player.camera.position.z.floor() as i32,
                );
                // Get block name as lowercase string (e.g., "Stone" -> "stone")
                let block_name = format!("{:?}", target_block).to_lowercase();
                let args = [block_name.as_str()];

                match crate::console::commands::floodfill(
                    &args,
                    &mut self.sim.world,
                    player_pos,
                    Some(start_pos),
                    true, // confirmed - skip threshold check for interactive mode
                ) {
                    crate::console::CommandResult::Success(msg) => {
                        println!("{}", msg);
                    }
                    crate::console::CommandResult::Error(msg) => {
                        println!("Flood fill error: {}", msg);
                    }
                    _ => {}
                }
            }

            self.ui.place_needs_reclick = true;
            return; // Skip block placement
        }

        // Block breaking (hold to break) - must be after raycast update
        // Skip if in template or stencil placement mode
        if self.input.focused
            && self.ui.active_placement.is_none()
            && self.ui.active_stencil_placement.is_none()
        {
            let holding_break = self.input.mouse_held(MouseButton::Left);

            // Clear skip flag when mouse is released
            if self.ui.skip_break_until_release && !holding_break {
                self.ui.skip_break_until_release = false;
            }

            // Skip block breaking until mouse is released after focusing
            if !self.ui.skip_break_until_release {
                self.update_block_breaking(delta_time, holding_break);
            }
        } else {
            // Reset breaking if unfocused
            self.ui.breaking_block = None;
            self.ui.break_progress = 0.0;
        }

        // Middle-click block picker: pick block type under cursor
        if self.input.focused && self.input.mouse_pressed(MouseButton::Middle) {
            if let Some(hit) = self.ui.current_hit {
                if let Some(block_type) = self.sim.world.get_block(hit.block_pos) {
                    if block_type != BlockType::Air {
                        // Capture metadata for special blocks before potential slot change
                        let mut picked_model_id = 0u8;
                        let mut picked_tint = 0u8;
                        let mut picked_paint_texture = 0u8;
                        if block_type == BlockType::Model {
                            if let Some(data) = self.sim.world.get_model_data(hit.block_pos) {
                                picked_model_id = data.model_id;
                            }
                        } else if block_type == BlockType::TintedGlass {
                            picked_tint = self.sim.world.get_tint_index(hit.block_pos).unwrap_or(0);
                        } else if block_type == BlockType::Painted {
                            if let Some(data) = self.sim.world.get_paint_data(hit.block_pos) {
                                picked_tint = data.tint_idx;
                                picked_paint_texture = data.texture_idx;
                            }
                        }

                        // Check if block type is already in hotbar
                        if let Some(idx) =
                            self.ui.hotbar_blocks.iter().position(|&b| b == block_type)
                        {
                            // Switch to that slot
                            self.ui.hotbar_index = idx;
                            println!("Picked {:?} (slot {})", block_type, idx + 1);
                        } else {
                            // Replace current slot with the picked block
                            self.ui.hotbar_blocks[self.ui.hotbar_index] = block_type;
                            println!(
                                "Replaced slot {} with {:?}",
                                self.ui.hotbar_index + 1,
                                block_type
                            );
                        }

                        // Update metadata arrays based on picked block
                        let idx = self.ui.hotbar_index;
                        match block_type {
                            BlockType::Model => {
                                self.ui.hotbar_model_ids[idx] = picked_model_id;
                            }
                            BlockType::TintedGlass => {
                                self.ui.hotbar_tint_indices[idx] = picked_tint;
                                self.ui.hotbar_model_ids[idx] = 0;
                                self.ui.hotbar_paint_textures[idx] = 0;
                            }
                            BlockType::Painted => {
                                self.ui.hotbar_tint_indices[idx] = picked_tint;
                                self.ui.hotbar_paint_textures[idx] = picked_paint_texture;
                                self.ui.hotbar_model_ids[idx] = 0;
                            }
                            _ => {
                                self.ui.hotbar_model_ids[idx] = 0;
                                self.ui.hotbar_tint_indices[idx] = 0;
                                self.ui.hotbar_paint_textures[idx] = 0;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Hotkeys that should work even when gameplay focus is released.
    pub fn handle_global_shortcuts(&mut self) {
        // Don't process shortcuts if console is active (it captures text input)
        if self.ui.console.active {
            return;
        }

        if self.input.key_pressed(KeyCode::KeyE) {
            self.toggle_palette_panel();
        }

        // Toggle model editor (N key)
        if self.input.key_pressed(KeyCode::KeyN) {
            self.toggle_editor_panel();
        }

        // Toggle tools palette (T key)
        if self.input.key_pressed(KeyCode::KeyT) {
            self.toggle_tools_palette();
        }

        // Toggle template browser (L key for Library)
        if self.input.key_pressed(KeyCode::KeyL) {
            self.toggle_template_browser();
        }

        // Toggle stencil browser (K key)
        if self.input.key_pressed(KeyCode::KeyK) {
            self.toggle_stencil_browser();
        }

        // Stencil opacity adjustment ([ and ] keys)
        if self.input.key_pressed(KeyCode::BracketLeft) {
            self.ui.stencil_manager.adjust_global_opacity(-0.1);
            println!(
                "Stencil opacity: {:.0}%",
                self.ui.stencil_manager.global_opacity * 100.0
            );
        }
        if self.input.key_pressed(KeyCode::BracketRight) {
            self.ui.stencil_manager.adjust_global_opacity(0.1);
            println!(
                "Stencil opacity: {:.0}%",
                self.ui.stencil_manager.global_opacity * 100.0
            );
        }

        // Toggle stencil render mode (\ key)
        if self.input.key_pressed(KeyCode::Backslash) {
            self.ui.stencil_manager.toggle_render_mode();
            println!("Stencil mode: {:?}", self.ui.stencil_manager.render_mode);
        }

        // Handle deferred cursor grab request (from template loading)
        if self.ui.request_cursor_grab {
            self.ui.request_cursor_grab = false;
            self.input.focused = true;
            self.input.pending_grab = Some(true);
            self.input.skip_input_frame = true;
        }

        // Rotate template placement (R key)
        if self.input.key_pressed(KeyCode::KeyR) {
            if let Some(ref mut placement) = self.ui.active_placement {
                placement.rotate_90();
                println!("Rotated template to {}°", placement.rotation * 90);
            }
            // Rotate stencil placement (R key)
            if let Some(ref mut placement) = self.ui.active_stencil_placement {
                placement.rotate_90();
                println!("Rotated stencil to {}°", placement.rotation * 90);
            }
        }

        // Repaint painted block under cursor with current hotbar texture/tint (P key)
        if self.input.key_pressed(KeyCode::KeyP) {
            if let Some(hit) = self.ui.current_hit {
                if let Some(BlockType::Painted) = self.sim.world.get_block(hit.block_pos) {
                    let tex = self.ui.hotbar_paint_textures[self.ui.hotbar_index];
                    let tint = self.ui.hotbar_tint_indices[self.ui.hotbar_index];
                    self.sim.world.set_painted_block(hit.block_pos, tex, tint);
                    self.sim
                        .world
                        .invalidate_minimap_cache(hit.block_pos.x, hit.block_pos.z);
                    println!(
                        "Repainted block at {:?} -> tex {}, tint {}",
                        hit.block_pos, tex, tint
                    );
                }
            }
        }

        // Cycle paint texture on selected Painted hotbar slot
        if self.input.key_pressed(KeyCode::BracketRight)
            && self.ui.hotbar_blocks[self.ui.hotbar_index] == BlockType::Painted
        {
            let tex = (self.ui.hotbar_paint_textures[self.ui.hotbar_index] + 1) % ATLAS_TILE_COUNT;
            self.ui.hotbar_paint_textures[self.ui.hotbar_index] = tex;
            println!("Paint texture -> {}", tex);
        }
        if self.input.key_pressed(KeyCode::BracketLeft)
            && self.ui.hotbar_blocks[self.ui.hotbar_index] == BlockType::Painted
        {
            let tex = self.ui.hotbar_paint_textures[self.ui.hotbar_index];
            let tex = if tex == 0 {
                ATLAS_TILE_COUNT - 1
            } else {
                tex - 1
            };
            self.ui.hotbar_paint_textures[self.ui.hotbar_index] = tex;
            println!("Paint texture -> {}", tex);
        }

        // Cycle tint for Painted or TintedGlass hotbar slot
        if self.input.key_pressed(KeyCode::Period)
            && matches!(
                self.ui.hotbar_blocks[self.ui.hotbar_index],
                BlockType::Painted | BlockType::TintedGlass
            )
        {
            self.ui.hotbar_tint_indices[self.ui.hotbar_index] =
                (self.ui.hotbar_tint_indices[self.ui.hotbar_index] + 1) & 0x1F;
            println!(
                "Tint -> {}",
                self.ui.hotbar_tint_indices[self.ui.hotbar_index]
            );
        }
        if self.input.key_pressed(KeyCode::Comma)
            && matches!(
                self.ui.hotbar_blocks[self.ui.hotbar_index],
                BlockType::Painted | BlockType::TintedGlass
            )
        {
            let tint = self.ui.hotbar_tint_indices[self.ui.hotbar_index];
            self.ui.hotbar_tint_indices[self.ui.hotbar_index] = tint.wrapping_sub(1) & 0x1F;
            println!(
                "Tint -> {}",
                self.ui.hotbar_tint_indices[self.ui.hotbar_index]
            );
        }

        // Toggle console (/ key)
        if self.input.key_pressed(KeyCode::Slash) {
            self.toggle_console();
        }

        // Editor undo/redo shortcuts (Cmd+Z/Ctrl+Z and Cmd+Shift+Z/Ctrl+Shift+Z)
        if self.ui.editor.active {
            let cmd_or_ctrl_held = self.input.key_held(KeyCode::SuperLeft)
                || self.input.key_held(KeyCode::SuperRight)
                || self.input.key_held(KeyCode::ControlLeft)
                || self.input.key_held(KeyCode::ControlRight);
            let shift_held =
                self.input.key_held(KeyCode::ShiftLeft) || self.input.key_held(KeyCode::ShiftRight);

            if cmd_or_ctrl_held && self.input.key_pressed(KeyCode::KeyZ) {
                if shift_held {
                    // Cmd/Ctrl+Shift+Z = Redo
                    self.ui.editor.redo();
                } else {
                    // Cmd/Ctrl+Z = Undo
                    self.ui.editor.undo();
                }
            }
        }

        // Allow scrolling hotbar while palette is open (focus may be released)
        if self.ui.palette_open {
            let ds = self.input.scroll_diff();
            if ds.1.abs() > 0.1 {
                let len = self.ui.hotbar_blocks.len();
                self.ui.hotbar_index = if ds.1 > 0.0 {
                    (self.ui.hotbar_index + len - 1) % len
                } else {
                    (self.ui.hotbar_index + 1) % len
                };
            }
        }
    }

    /// Toggles the model editor on/off.
    fn toggle_editor_panel(&mut self) {
        self.ui.editor.toggle();
        if self.ui.editor.active {
            // Opening editor: release cursor, store previous focus
            self.ui.editor_previously_focused = self.input.focused;
            self.input.focused = false;
            self.input.pending_grab = Some(false);

            // Save the target position for placing the model when done
            if let Some(hit) = &self.ui.current_hit {
                let place_pos = get_place_position(hit);
                self.ui.editor.set_target_pos(place_pos);
            }
            println!("Model editor: ON");
        } else {
            // Closing editor: restore focus if we were focused before and no other panel is open
            let other_panel_open = self.ui.palette_open || self.ui.console.active;
            if !other_panel_open && self.ui.editor_previously_focused {
                self.input.focused = true;
                self.input.pending_grab = Some(true);
                self.input.skip_input_frame = true;
                self.ui.editor_previously_focused = false;
            }
            println!("Model editor: OFF");
        }
    }

    /// Toggles the command console on/off.
    fn toggle_console(&mut self) {
        self.ui.console.toggle();
        if self.ui.console.active {
            // Opening console: release cursor, store previous focus
            self.ui.console_previously_focused = self.input.focused;
            self.input.focused = false;
            self.input.pending_grab = Some(false);
        } else {
            // Closing console: restore focus if we were focused before and no other panel is open
            let other_panel_open = self.ui.palette_open || self.ui.editor.active;
            if !other_panel_open && self.ui.console_previously_focused {
                self.input.focused = true;
                self.input.pending_grab = Some(true);
                self.input.skip_input_frame = true;
                self.ui.console_previously_focused = false;
            }
        }
    }

    /// Toggles the template browser on/off.
    fn toggle_template_browser(&mut self) {
        self.ui.template_ui.toggle_browser();
        if self.ui.template_ui.browser_open {
            // Opening template browser: release cursor, store previous focus
            self.ui.template_previously_focused = self.input.focused;
            self.input.focused = false;
            self.input.pending_grab = Some(false);
            println!("Template browser: ON");
        } else {
            // Closing template browser: restore focus if we were focused before and no other panel is open
            let other_panel_open =
                self.ui.palette_open || self.ui.editor.active || self.ui.console.active;
            if !other_panel_open && self.ui.template_previously_focused {
                self.input.focused = true;
                self.input.pending_grab = Some(true);
                self.input.skip_input_frame = true;
                self.ui.template_previously_focused = false;
            }
            println!("Template browser: OFF");
        }
    }

    /// Toggles the stencil browser on/off.
    fn toggle_stencil_browser(&mut self) {
        self.ui.stencil_ui.toggle_browser();
        if self.ui.stencil_ui.browser_open {
            // Opening stencil browser: release cursor, store previous focus
            self.ui.stencil_previously_focused = self.input.focused;
            self.input.focused = false;
            self.input.pending_grab = Some(false);
            println!("Stencil browser: ON");
        } else {
            // Closing stencil browser: restore focus if we were focused before and no other panel is open
            let other_panel_open =
                self.ui.palette_open || self.ui.editor.active || self.ui.console.active;
            if !other_panel_open && self.ui.stencil_previously_focused {
                self.input.focused = true;
                self.input.pending_grab = Some(true);
                self.input.skip_input_frame = true;
                self.ui.stencil_previously_focused = false;
            }
            println!("Stencil browser: OFF");
        }
    }
}

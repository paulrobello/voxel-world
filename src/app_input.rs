use crate::App;
use crate::chunk::BlockType;
use crate::raycast::get_place_position;
use nalgebra::Vector3;
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

impl App {
    /// Handle focus/unfocus toggles. Returns true if we should early-return from update.
    pub fn handle_focus_toggles(&mut self) -> bool {
        // Close palette with Escape (restores focus if it was focused before opening)
        if self.input.key_pressed(KeyCode::Escape) && self.ui.palette_open {
            self.ui.palette_open = false;
            self.ui.dragging_item = None;
            if self.ui.palette_previously_focused {
                self.input.focused = true;
                self.input.pending_grab = Some(true);
                self.ui.palette_previously_focused = false;
            }
            return true;
        }

        // Close editor with Escape (restores focus if it was focused before opening)
        if self.input.key_pressed(KeyCode::Escape) && self.ui.editor.active {
            self.ui.editor.active = false;
            if self.ui.editor_previously_focused {
                self.input.focused = true;
                self.input.pending_grab = Some(true);
                self.ui.editor_previously_focused = false;
            }
            println!("Model editor: OFF");
            return true;
        }

        // Handle escape to unfocus
        if self.input.key_pressed(KeyCode::Escape) && self.input.focused {
            self.input.focused = false;
            self.input.pending_grab = Some(false);
            println!("Unfocused - cursor will be released");
        }

        // Handle focus toggling - click to focus (don't process this click for gameplay)
        if !self.input.focused && self.input.mouse_pressed(MouseButton::Left) {
            println!("Focus click...");
            self.input.focused = true;
            self.input.pending_grab = Some(true);
            // Skip block breaking until mouse is released to avoid breaking on focus click
            self.ui.skip_break_until_release = true;
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

        // Update player physics (movement, gravity, collisions)
        self.sim.player.update_physics(
            delta_time,
            &self.sim.world,
            self.sim.world_extent,
            self.sim.texture_origin,
            &self.input,
            &self.sim.model_registry,
            self.args.verbose,
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

        // Toggle minimap (M key)
        if self.input.key_pressed(KeyCode::KeyM) {
            self.ui.show_minimap = !self.ui.show_minimap;
            println!(
                "Minimap: {}",
                if self.ui.show_minimap { "ON" } else { "OFF" }
            );
        }

        // Block placing - continuous when holding right mouse button
        self.update_block_placing(delta_time as f32);
    }

    /// Processes block breaking and middle-click pick interactions.
    pub fn handle_block_interactions(&mut self, delta_time: f32) {
        // Update raycast for block selection
        self.update_raycast();

        // Block breaking (hold to break) - must be after raycast update
        if self.input.focused {
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
                    }
                }
            }
        }
    }

    /// Hotkeys that should work even when gameplay focus is released.
    pub fn handle_global_shortcuts(&mut self) {
        if self.input.key_pressed(KeyCode::KeyE) {
            self.toggle_palette_panel();
        }

        // Toggle model editor (N key)
        if self.input.key_pressed(KeyCode::KeyN) {
            self.toggle_editor_panel();
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
            // Closing editor: restore focus if we were focused before
            if self.ui.editor_previously_focused {
                self.input.focused = true;
                self.input.pending_grab = Some(true);
                self.ui.editor_previously_focused = false;
            }
            println!("Model editor: OFF");
        }
    }
}

//! Core App struct definition and basic methods

use crate::app_state::{Graphics, InputState, MultiplayerState, UiState, WorldSim};
use crate::chunk::BlockType;
use crate::config::Args;
use crate::user_prefs::UserPreferences;
use std::time::Instant;

pub struct App {
    pub args: Args,
    pub start_time: Instant,
    pub graphics: Graphics,
    pub sim: WorldSim,
    pub ui: UiState,
    pub input: InputState,
    pub prefs: UserPreferences,
    /// Multiplayer state (server/client management).
    pub multiplayer: MultiplayerState,
}

impl App {
    /// Returns true if we're in multiplayer mode (host or client).
    pub fn is_multiplayer(&self) -> bool {
        self.multiplayer.mode != crate::config::GameMode::SinglePlayer
    }

    /// Returns true if connected to a server (as host's local client or as remote client).
    pub fn is_connected_to_server(&self) -> bool {
        self.multiplayer.is_connected()
    }

    /// Syncs a block placement to the server (if in multiplayer mode).
    pub fn sync_block_placement(
        &mut self,
        position: [i32; 3],
        block: crate::net::protocol::BlockData,
    ) {
        if self.is_connected_to_server() {
            self.multiplayer.send_place_block(position, block);
        }
    }

    /// Syncs a block break to the server (if in multiplayer mode).
    pub fn sync_block_break(&mut self, position: [i32; 3]) {
        if self.is_connected_to_server() {
            self.multiplayer.send_break_block(position);
        }
    }

    /// Applies pending block changes from the server to the world.
    /// Call this from the game loop to apply remote block changes.
    pub fn apply_remote_block_changes(&mut self) {
        if !self.multiplayer.has_pending_block_changes() {
            return;
        }

        let changes = self.multiplayer.take_pending_block_changes();
        for change in changes {
            let pos =
                nalgebra::Vector3::new(change.position[0], change.position[1], change.position[2]);

            // Apply block type and metadata based on type
            let block_type = change.block.block_type;

            match block_type {
                BlockType::Model => {
                    if let Some(model_data) = &change.block.model_data {
                        self.sim.world.set_model_block_with_data(
                            pos,
                            model_data.model_id,
                            model_data.rotation,
                            model_data.waterlogged,
                            model_data.custom_data,
                        );
                    }
                }
                BlockType::TintedGlass => {
                    let tint_index = change.block.tint_index.unwrap_or(0);
                    self.sim.world.set_tinted_glass_block(pos, tint_index);
                }
                BlockType::Crystal => {
                    let tint_index = change.block.tint_index.unwrap_or(0);
                    self.sim.world.set_crystal_block(pos, tint_index);
                }
                BlockType::Painted => {
                    if let Some(paint_data) = &change.block.paint_data {
                        self.sim.world.set_painted_block_full(
                            pos,
                            paint_data.texture_idx,
                            paint_data.tint_idx,
                            paint_data.blend_mode,
                        );
                    }
                }
                BlockType::Water => {
                    let water_type = change
                        .block
                        .water_type
                        .unwrap_or(crate::chunk::WaterType::Ocean);
                    self.sim.world.set_water_block(pos, water_type);
                }
                _ => {
                    // Standard block types
                    self.sim.world.set_block(pos, block_type);
                }
            }

            // Invalidate minimap cache
            self.sim.world.invalidate_minimap_cache(pos.x, pos.z);
        }
    }

    /// Returns the currently selected block from the hotbar.
    pub fn selected_block(&self) -> BlockType {
        self.ui.hotbar_blocks[self.ui.hotbar_index]
    }

    /// Move the player upward in small steps until no collision, to safely exit fly mode.
    pub fn resolve_player_overlap(&mut self) {
        let mut feet = self
            .sim
            .player
            .feet_pos(self.sim.world_extent, self.sim.texture_origin);
        for _ in 0..12 {
            if !self.sim.player.check_collision(
                feet,
                &self.sim.world,
                &self.sim.model_registry,
                true,
            ) {
                break;
            }
            feet.y += 0.25;
        }
        self.sim
            .player
            .set_feet_pos(feet, self.sim.world_extent, self.sim.texture_origin);
    }

    pub fn toggle_palette_panel(&mut self) {
        self.ui.palette_open = !self.ui.palette_open;
        if self.ui.palette_open {
            self.ui.palette_previously_focused = self.input.focused;
            self.input.focused = false;
            self.input.pending_grab = Some(false);
            self.ui.dragging_item = None;
        } else {
            // Closing palette: restore focus if we were focused before and no other panel is open
            let other_panel_open = self.ui.editor.active
                || self.ui.console.active
                || self.ui.texture_generator.open
                || self.ui.paint_panel.open;
            if !other_panel_open && self.ui.palette_previously_focused {
                self.input.focused = true;
                self.input.pending_grab = Some(true);
                self.input.skip_input_frame = true;
                self.ui.palette_previously_focused = false;
            }
        }
    }

    /// Toggles the texture generator panel on/off.
    /// When opened, releases the cursor so user can interact with UI.
    pub fn toggle_texture_generator(&mut self) {
        self.ui.texture_generator.open = !self.ui.texture_generator.open;
        if self.ui.texture_generator.open {
            // Release cursor when opening
            if self.input.focused {
                self.input.focused = false;
                self.input.pending_grab = Some(false);
            }
        } else {
            // Restore focus when closing
            self.input.focused = true;
            self.input.pending_grab = Some(true);
            self.input.skip_input_frame = true;
        }
    }

    /// Toggles the paint panel on/off.
    /// When opened, releases the cursor so user can interact with UI.
    pub fn toggle_paint_panel(&mut self) {
        self.ui.paint_panel.open = !self.ui.paint_panel.open;
        if self.ui.paint_panel.open {
            // Release cursor when opening
            if self.input.focused {
                self.input.focused = false;
                self.input.pending_grab = Some(false);
            }
        } else {
            // Restore focus when closing
            self.input.focused = true;
            self.input.pending_grab = Some(true);
            self.input.skip_input_frame = true;
        }
    }

    /// Toggles the tools palette on/off.
    /// When opened, releases the cursor so user can interact with UI.
    pub fn toggle_tools_palette(&mut self) {
        self.ui.tools_palette.toggle();

        if self.ui.tools_palette.open {
            // Release cursor when opening tools palette
            if self.input.focused {
                self.ui.tools_palette.previously_focused = true;
                self.input.focused = false;
                self.input.pending_grab = Some(false);
            }
        } else {
            // Restore focus when closing if we had it before
            if self.ui.tools_palette.previously_focused {
                self.input.focused = true;
                self.input.pending_grab = Some(true);
                self.input.skip_input_frame = true;
                self.ui.tools_palette.previously_focused = false;
            }
        }

        println!(
            "Tools palette: {}",
            if self.ui.tools_palette.open {
                "ON"
            } else {
                "OFF"
            }
        );
    }

    /// Uploads the custom texture library to the GPU.
    /// Call this after generating or modifying custom textures.
    pub fn sync_custom_textures(&self) {
        // Upload each texture slot individually for efficiency
        for texture in self.ui.texture_library.iter() {
            if !texture.pixels.is_empty() {
                crate::gpu_resources::update_custom_texture_slot(
                    self.graphics.memory_allocator.clone(),
                    self.graphics.command_buffer_allocator.clone(),
                    &self.graphics.queue,
                    &self.graphics.custom_texture_atlas,
                    texture.id as u32,
                    &texture.pixels,
                );
            }
        }
    }

    /// Saves user preferences to disk.
    pub fn save_preferences(&mut self) {
        self.prefs.settings = self.ui.settings.clone();
        self.prefs.hotbar_index = self.ui.hotbar_index;
        self.prefs.set_hotbar_blocks(&self.ui.hotbar_blocks);
        self.prefs.hotbar_model_ids = self.ui.hotbar_model_ids;
        self.prefs.hotbar_tint_indices = self.ui.hotbar_tint_indices;
        self.prefs.hotbar_paint_textures = self.ui.hotbar_paint_textures;
        self.prefs.show_minimap = self.ui.show_minimap;
        self.prefs.console_history = self.ui.console.get_history();
        self.prefs.last_fly_mode = Some(self.sim.player.fly_mode);

        // Save player position for the current world
        let player_pos = self
            .sim
            .player
            .feet_pos(self.sim.world_extent, self.sim.texture_origin);
        let yaw = self.sim.player.camera.rotation.y as f32;
        let pitch = self.sim.player.camera.rotation.x as f32;
        self.prefs.set_player_data(
            &self.sim.world_name,
            crate::user_prefs::WorldPlayerData {
                position: [player_pos.x, player_pos.y, player_pos.z],
                yaw,
                pitch,
            },
        );

        self.prefs.save();
    }

    /// Requests chunks from the server when in multiplayer client mode.
    /// This should be called every frame to maintain chunk loading around the player.
    pub fn request_network_chunks(&mut self) {
        // Get player position and look direction for chunk prioritization
        let player_world_pos = self
            .sim
            .player
            .feet_pos(self.sim.world_extent, self.sim.texture_origin);

        let yaw = self.sim.player.camera.rotation.y as f32;
        let look_dir = [yaw.sin(), 0.0, -yaw.cos()]; // XZ direction player is looking

        // Update chunk sync with player state and get chunks to cancel
        let _to_cancel = self.multiplayer.chunk_sync.update_player_state(
            [
                player_world_pos.x as f32,
                player_world_pos.y as f32,
                player_world_pos.z as f32,
            ],
            look_dir,
        );

        // TODO: Send cancellation to server when implemented

        // Calculate player's chunk position
        let player_chunk = self
            .sim
            .player
            .get_chunk_pos(self.sim.world_extent, self.sim.texture_origin);

        // Request chunks around the player
        if let Some(request) = self.multiplayer.chunk_sync.request_chunks_around(
            [player_chunk.x, player_chunk.y, player_chunk.z],
            self.sim.view_distance,
        ) {
            // Send chunk request to server
            if let Some(ref mut client) = self.multiplayer.client {
                client.send_chunk_request(request.positions);
            }
        }
    }

    /// Applies pending network chunks to the world.
    /// Call this from the chunk loading system to apply chunks received from server.
    pub fn apply_network_chunks(&mut self) -> Vec<(nalgebra::Vector3<i32>, crate::chunk::Chunk)> {
        if !self.multiplayer.has_pending_chunks() {
            return Vec::new();
        }

        self.multiplayer.take_pending_chunks()
    }
}

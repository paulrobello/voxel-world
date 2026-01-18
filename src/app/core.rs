//! Core App struct definition and basic methods

use crate::app_state::{Graphics, InputState, UiState, WorldSim};
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
}

impl App {
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
            let other_panel_open = self.ui.editor.active || self.ui.console.active;
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
}

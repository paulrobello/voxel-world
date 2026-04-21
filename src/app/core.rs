//! Core App struct definition and basic methods

use crate::app_state::{Graphics, InputState, MultiplayerState, UiState, WorldSim};
use crate::chunk::BlockType;
use crate::config::Args;
use crate::user_prefs::UserPreferences;
use std::time::Instant;
use vulkano::sync::GpuFuture;

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
    /// Fence from the previous frame — waited on at the start of the next frame
    /// to enable CPU/GPU pipelining (CPU prepares frame N+1 while GPU renders frame N).
    pub previous_frame_fence: Option<Box<dyn GpuFuture>>,
}

impl App {
    /// Returns true if we're in multiplayer mode (host or client).
    pub fn is_multiplayer(&self) -> bool {
        self.multiplayer.mode != crate::config::GameMode::SinglePlayer
    }

    /// Returns the currently selected block from the hotbar.
    pub fn selected_block(&self) -> BlockType {
        self.ui.hotbar.hotbar_blocks[self.ui.hotbar.hotbar_index]
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
        self.ui.palette_ui.palette_open = !self.ui.palette_ui.palette_open;
        if self.ui.palette_ui.palette_open {
            self.ui.palette_ui.palette_previously_focused = self.input.focused;
            self.input.focused = false;
            self.input.pending_grab = Some(false);
            self.ui.palette_ui.dragging_item = None;
        } else {
            // Closing palette: restore focus if we were focused before and no other panel is open
            let other_panel_open = self.ui.editor.active
                || self.ui.console.active
                || self.ui.texture_generator.open
                || self.ui.paint_panel.open;
            if !other_panel_open && self.ui.palette_ui.palette_previously_focused {
                self.input.focused = true;
                self.input.pending_grab = Some(true);
                self.input.skip_input_frame = true;
                self.ui.palette_ui.palette_previously_focused = false;
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

        log::debug!(
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
    ///
    /// All dirty slots are recorded into one command buffer and submitted with
    /// a single fence, replacing the previous per-slot block-and-wait pattern.
    pub fn sync_custom_textures(&self) {
        let slots: Vec<(u32, &[u8])> = self
            .ui
            .texture_library
            .iter()
            .filter(|t| !t.pixels.is_empty())
            .map(|t| (t.id as u32, t.pixels.as_slice()))
            .collect();

        if slots.is_empty() {
            return;
        }

        crate::gpu_resources::batch_update_custom_texture_slots(
            self.graphics.memory_allocator.clone(),
            self.graphics.command_buffer_allocator.clone(),
            &self.graphics.queue,
            &self.graphics.custom_texture_atlas,
            slots,
        );
    }

    /// Encodes 64x64 RGBA pixel data as a PNG file.
    /// Used for uploading textures to the server.
    pub fn encode_texture_as_png(&self, pixels: &[u8]) -> Vec<u8> {
        use std::io::Cursor;
        // pixels should be 64x64x4 = 16384 bytes
        let img = image::RgbaImage::from_raw(64, 64, pixels.to_vec())
            .expect("Failed to create image from pixels");
        let mut cursor = Cursor::new(Vec::new());
        img.write_to(&mut cursor, image::ImageFormat::Png)
            .expect("Failed to encode PNG");
        cursor.into_inner()
    }

    /// Saves user preferences to disk.
    pub fn save_preferences(&mut self) {
        self.prefs.settings = self.ui.settings.clone();
        self.prefs.hotbar_index = self.ui.hotbar.hotbar_index;
        self.prefs.set_hotbar_blocks(&self.ui.hotbar.hotbar_blocks);
        self.prefs.hotbar_model_ids = self.ui.hotbar.hotbar_model_ids;
        self.prefs.hotbar_tint_indices = self.ui.hotbar.hotbar_tint_indices;
        self.prefs.hotbar_paint_textures = self.ui.hotbar.hotbar_paint_textures;
        self.prefs.show_minimap = self.ui.minimap_ui.show_minimap;
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

    /// Initializes the multiplayer texture array when connecting to a server.
    /// This creates the GPU texture array with the specified number of slots.
    pub fn init_multiplayer_textures(&mut self, max_slots: u32) {
        if self.graphics.multiplayer_texture_array.is_some() {
            return; // Already initialized
        }

        let (image, view, sampler) = crate::gpu_resources::create_multiplayer_texture_array(
            self.graphics.memory_allocator.clone(),
            max_slots,
        );

        self.graphics.multiplayer_texture_array = Some(image);
        self.graphics.multiplayer_texture_array_view = Some(view);
        self.graphics.multiplayer_texture_sampler = Some(sampler);
        self.graphics.multiplayer_texture_count = max_slots;

        log::debug!(
            "[Multiplayer] Initialized texture array with {} slots",
            max_slots
        );
    }

    /// Uploads any pending custom textures from the multiplayer cache to the GPU.
    /// Call this after multiplayer.update() to sync received textures to the GPU.
    pub fn upload_multiplayer_textures(&mut self) {
        // Check if we need to initialize the GPU texture array
        if let Some(max_slots) = self.multiplayer.take_pending_gpu_texture_init() {
            self.init_multiplayer_textures(max_slots as u32);
        }

        // Check if we have any textures to upload
        let texture_cache = self.multiplayer.texture_cache();
        let new_textures = texture_cache.get_new_textures();

        if new_textures.is_empty() {
            return;
        }

        // Ensure texture array is initialized
        if self.graphics.multiplayer_texture_array.is_none() {
            // Initialize with default max slots
            self.init_multiplayer_textures(32);
        }

        let texture_array = match &self.graphics.multiplayer_texture_array {
            Some(arr) => arr.clone(),
            None => return,
        };

        // Upload each new texture
        for (slot, data) in &new_textures {
            match crate::gpu_resources::update_multiplayer_texture_slot(
                self.graphics.memory_allocator.clone(),
                self.graphics.command_buffer_allocator.clone(),
                &self.graphics.queue,
                &texture_array,
                *slot as u32,
                data,
            ) {
                Ok(()) => {
                    log::debug!("[Multiplayer] Uploaded custom texture to slot {}", slot);
                }
                Err(e) => {
                    log::warn!(
                        "[Multiplayer] Failed to upload texture slot {}: {}",
                        slot,
                        e
                    );
                }
            }
        }

        // Mark textures as uploaded
        self.multiplayer
            .texture_cache_mut()
            .mark_uploaded(&new_textures);
    }
}

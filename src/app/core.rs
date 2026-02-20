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
            println!("[Client] Syncing block placement at {:?}", position);
            self.multiplayer.send_place_block(position, block);
        }
    }

    /// Syncs a block break to the server (if in multiplayer mode).
    pub fn sync_block_break(&mut self, position: [i32; 3]) {
        if self.is_connected_to_server() {
            println!("[Client] Syncing block break at {:?}", position);
            self.multiplayer.send_break_block(position);
        }
    }

    /// Syncs a water source placement to all clients (if hosting).
    /// This is server-authoritative: the host broadcasts to all clients.
    pub fn sync_water_source(&mut self, position: [i32; 3], water_type: crate::chunk::WaterType) {
        if self.multiplayer.is_host() {
            println!("[Host] Broadcasting water source at {:?}", position);
            self.multiplayer
                .broadcast_water_source(position, water_type);
        }
    }

    /// Applies pending water updates from the server to the local simulation.
    /// Call this from the game loop to apply remote water changes.
    pub fn apply_remote_water_updates(&mut self) {
        if !self.multiplayer.has_pending_water_updates() {
            return;
        }

        let updates = self.multiplayer.take_pending_water_updates();
        for update in updates {
            let pos =
                nalgebra::Vector3::new(update.position[0], update.position[1], update.position[2]);

            if update.mass <= 0.0 {
                // Remove water
                self.sim.water_grid.remove_water(pos, 1.0);
                self.sim.world.set_block(pos, BlockType::Air);
            } else if update.is_source {
                // Add/update water source
                self.sim.water_grid.place_source(pos, update.water_type);
                self.sim.world.set_water_block(pos, update.water_type);
            } else {
                // Add/update non-source water
                self.sim
                    .water_grid
                    .set_water(pos, update.mass, false, update.water_type);
                self.sim.world.set_water_block(pos, update.water_type);
            }
        }
    }

    /// Applies pending lava updates from the server to the local simulation.
    /// Call this from the game loop to apply remote lava changes.
    pub fn apply_remote_lava_updates(&mut self) {
        if !self.multiplayer.has_pending_lava_updates() {
            return;
        }

        let updates = self.multiplayer.take_pending_lava_updates();
        for update in updates {
            let pos =
                nalgebra::Vector3::new(update.position[0], update.position[1], update.position[2]);

            if update.mass <= 0.0 {
                // Remove lava
                self.sim.lava_grid.set_lava(pos, 0.0, false);
                self.sim.world.set_block(pos, BlockType::Air);
            } else if update.is_source {
                // Add/update lava source
                self.sim.lava_grid.place_source(pos);
                self.sim.world.set_block(pos, BlockType::Lava);
            } else {
                // Add/update non-source lava
                self.sim.lava_grid.set_lava(pos, update.mass, false);
                self.sim.world.set_block(pos, BlockType::Lava);
            }
        }
    }

    /// Applies pending block changes from the server to the world.
    /// Call this from the game loop to apply remote block changes.
    pub fn apply_remote_block_changes(&mut self) {
        if !self.multiplayer.has_pending_block_changes() {
            return;
        }

        use crate::block_update::BlockUpdateType;

        let changes = self.multiplayer.take_pending_block_changes();
        println!("[Client] Applying {} remote block change(s)", changes.len());

        let player_pos = self
            .sim
            .player
            .feet_pos(self.sim.world_extent, self.sim.texture_origin)
            .cast::<f32>();

        for change in changes {
            let pos =
                nalgebra::Vector3::new(change.position[0], change.position[1], change.position[2]);

            println!(
                "[Client] Applying remote block change at {:?}: {:?}",
                pos, change.block.block_type
            );

            // Get the previous block type before changing (for physics triggers)
            let prev_block_type = self.sim.world.get_block(pos);

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
                    // Add water source to grid for simulation
                    self.sim.water_grid.place_source(pos, water_type);
                    self.sim.world.set_water_block(pos, water_type);
                }
                _ => {
                    // Standard block types
                    self.sim.world.set_block(pos, block_type);
                }
            }

            // Invalidate minimap cache
            self.sim.world.invalidate_minimap_cache(pos.x, pos.z);

            // Trigger physics checks if block was removed (changed to Air or Water)
            // This ensures tree physics sync across all clients
            if block_type == BlockType::Air || block_type == BlockType::Water {
                // Queue gravity check for block above
                self.sim.block_updates.enqueue(
                    pos + nalgebra::Vector3::new(0, 1, 0),
                    BlockUpdateType::Gravity,
                    player_pos,
                );

                // Queue ground support check for model block above
                self.sim.block_updates.enqueue(
                    pos + nalgebra::Vector3::new(0, 1, 0),
                    BlockUpdateType::ModelGroundSupport,
                    player_pos,
                );

                // If the removed block was a log, queue tree support checks
                if let Some(prev) = prev_block_type {
                    if prev.is_log() {
                        self.sim.block_updates.enqueue_neighbors(
                            pos,
                            BlockUpdateType::TreeSupport,
                            player_pos,
                        );
                    }

                    // If the removed block was a Model (torch/fence/gate), spawn break particles
                    // This ensures clients see particle effects for ground support breaks
                    if prev == BlockType::Model {
                        // Use the same brown color as process_model_ground_support_update
                        let particle_color = nalgebra::Vector3::new(0.5, 0.35, 0.2);
                        self.sim
                            .particles
                            .spawn_block_break(pos.cast::<f32>(), particle_color);
                    }
                }

                // Always queue tree support checks in radius (tree might have lost support)
                self.sim.block_updates.enqueue_radius(
                    pos,
                    3,
                    BlockUpdateType::TreeSupport,
                    player_pos,
                );

                // Queue orphaned leaves checks
                self.sim.block_updates.enqueue_radius(
                    pos,
                    4,
                    BlockUpdateType::OrphanedLeaves,
                    player_pos,
                );
            }
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
                println!(
                    "[Client] Requesting {} chunks around player at chunk {:?}",
                    request.positions.len(),
                    player_chunk
                );
                client.send_chunk_request(request.positions);
                // Flush immediately to ensure request is sent
                client.flush_packets();
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

    /// Returns positions of chunks that should be generated locally from seed.
    /// These are unmodified chunks that the server told us to generate locally.
    pub fn take_pending_local_chunks(&mut self) -> Vec<[i32; 3]> {
        self.multiplayer.take_pending_local_chunks()
    }

    /// Returns true if there are local chunks to generate.
    pub fn has_pending_local_chunks(&self) -> bool {
        self.multiplayer.has_pending_local_chunks()
    }

    /// Fulfills chunk requests from clients (server-side, when hosting).
    /// Call this from the game loop after multiplayer.update() to process
    /// pending chunk requests and send chunk data to clients.
    pub fn fulfill_chunk_requests(&mut self) {
        if !self.multiplayer.has_pending_chunk_requests() {
            return;
        }

        let requests = self.multiplayer.take_pending_chunk_requests();
        println!("[Server] Fulfilling {} chunk request(s)", requests.len());
        for (client_id, positions) in requests {
            println!(
                "[Server] Client {} requested {} chunk(s)",
                client_id,
                positions.len()
            );
            for chunk_pos in positions {
                // Convert to World's chunk coordinate format
                let pos = nalgebra::Vector3::new(chunk_pos[0], chunk_pos[1], chunk_pos[2]);

                // Check if chunk exists in the world
                if let Some(chunk) = self.sim.world.get_chunk(pos) {
                    // Send the chunk to the client
                    self.multiplayer
                        .send_chunk_to_client(client_id, chunk_pos, chunk);
                    println!(
                        "[Server] Sent chunk {:?} to client {} (dirty={})",
                        chunk_pos, client_id, chunk.persistence_dirty
                    );
                } else {
                    println!("[Server] Chunk {:?} not found, skipping", chunk_pos);
                }
                // If chunk doesn't exist, we skip it (client will re-request later)
            }
        }
    }

    /// Processes pending model uploads from clients (server-side, when hosting).
    /// Registers models in the server's registry, saves them, and broadcasts to all clients.
    pub fn process_model_uploads(&mut self) {
        if !self.multiplayer.has_pending_model_uploads() {
            return;
        }

        let uploads = self.multiplayer.take_pending_model_uploads();
        println!("[Server] Processing {} model upload(s)", uploads.len());

        for (_client_id, upload) in uploads {
            use crate::storage::model_format::VxmFile;
            use lz4_flex::decompress_size_prepended;

            // Decompress the model data
            let decompressed = match decompress_size_prepended(&upload.model_data) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!(
                        "[Server] Failed to decompress model '{}': {:?}",
                        upload.name, e
                    );
                    continue;
                }
            };

            // Deserialize the VxmFile
            let vxm: VxmFile =
                match bincode::serde::decode_from_slice(&decompressed, bincode::config::legacy()) {
                    Ok((vxm, _)) => vxm,
                    Err(e) => {
                        eprintln!(
                            "[Server] Failed to deserialize model '{}': {:?}",
                            upload.name, e
                        );
                        continue;
                    }
                };

            // Convert to SubVoxelModel and register
            let model = vxm.to_model();
            let model_id = self.sim.model_registry.register(model.clone());

            println!(
                "[Server] Registered model '{}' as ID {} from client",
                upload.name, model_id
            );

            // Save to disk
            let library_path = crate::user_prefs::user_models_dir();
            if let Err(e) = crate::storage::model_format::LibraryManager::new(&library_path)
                .save_model(&model, &upload.author)
            {
                eprintln!("[Server] Failed to save model '{}': {}", upload.name, e);
            }

            // Broadcast to all clients
            if let Some(ref mut server) = self.multiplayer.server {
                server.broadcast_model_added(
                    model_id,
                    upload.name.clone(),
                    upload.author.clone(),
                    upload.model_data.clone(),
                );
            }
        }
    }

    /// Processes pending texture uploads from clients (server-side, when hosting).
    /// Registers textures in the server's texture manager, saves them, and broadcasts to all clients.
    pub fn process_texture_uploads(&mut self) {
        if !self.multiplayer.has_pending_texture_uploads() {
            return;
        }

        let uploads = self.multiplayer.take_pending_texture_uploads();
        println!("[Server] Processing {} texture upload(s)", uploads.len());

        for (_client_id, upload) in uploads {
            // Use the server's TextureSlotManager if available
            if let Some(ref mut manager) = self.multiplayer.server {
                // Add texture to the manager (which saves to disk)
                let slot = match manager.add_texture(&upload.name, &upload.png_data) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("[Server] Failed to add texture '{}': {}", upload.name, e);
                        continue;
                    }
                };

                println!(
                    "[Server] Added texture '{}' to slot {} from client",
                    upload.name, slot
                );

                // Broadcast to all clients
                manager.broadcast_texture_added(slot, upload.name.clone(), upload.png_data.clone());
            }
        }
    }

    /// Processes pending picture uploads from clients (server-side, when hosting).
    /// Registers pictures in the server's picture manager, saves them, and broadcasts to all clients.
    pub fn process_picture_uploads(&mut self) {
        if !self.multiplayer.has_pending_picture_uploads() {
            return;
        }

        let uploads = self.multiplayer.take_pending_picture_uploads();
        println!("[Server] Processing {} picture upload(s)", uploads.len());

        for (_client_id, upload) in uploads {
            // Use the server's GameServer if available
            if let Some(ref mut server) = self.multiplayer.server {
                // Add picture to the manager (which saves to disk)
                let picture_id = match server.add_picture(&upload.name, &upload.png_data) {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("[Server] Failed to add picture '{}': {}", upload.name, e);
                        continue;
                    }
                };

                println!(
                    "[Server] Added picture '{}' with ID {} from client",
                    upload.name, picture_id
                );

                // Broadcast to all clients
                server.broadcast_picture_added(picture_id, upload.name.clone());
            }
        }
    }

    /// Registers pending models received from server (client-side).
    /// Call this from the game loop after multiplayer.update() to register
    /// models that were broadcast by the server.
    pub fn register_pending_models(&mut self) {
        if !self.multiplayer.has_pending_models() {
            return;
        }

        let models = self.multiplayer.take_pending_models();
        println!("[Client] Registering {} model(s)", models.len());

        for model_added in models {
            use crate::storage::model_format::VxmFile;
            use lz4_flex::decompress_size_prepended;

            // Decompress the model data
            let decompressed = match decompress_size_prepended(&model_added.model_data) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!(
                        "[Client] Failed to decompress model '{}': {:?}",
                        model_added.name, e
                    );
                    continue;
                }
            };

            // Deserialize the VxmFile
            let vxm: VxmFile =
                match bincode::serde::decode_from_slice(&decompressed, bincode::config::legacy()) {
                    Ok((vxm, _)) => vxm,
                    Err(e) => {
                        eprintln!(
                            "[Client] Failed to deserialize model '{}': {:?}",
                            model_added.name, e
                        );
                        continue;
                    }
                };

            // Convert to SubVoxelModel
            let mut model = vxm.to_model();
            model.id = model_added.model_id;

            // Register in local registry
            let registered_id = self.sim.model_registry.register(model.clone());

            println!(
                "[Client] Registered model '{}' as ID {} (server ID {})",
                model_added.name, registered_id, model_added.model_id
            );

            // Generate sprite for the model
            let sprites_dir = std::path::Path::new("textures/rendered");
            if std::fs::create_dir_all(sprites_dir).is_ok() {
                let sprite_path = sprites_dir.join(format!("model_{}.png", registered_id));
                if crate::editor::rasterizer::generate_model_sprite(&model, &sprite_path).is_ok() {
                    println!("[Client] Generated sprite for model {}", registered_id);
                }
            }
        }
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

        println!(
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
                    println!("[Multiplayer] Uploaded custom texture to slot {}", slot);
                }
                Err(e) => {
                    eprintln!(
                        "[Multiplayer] Failed to upload texture slot {}: {}",
                        slot, e
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

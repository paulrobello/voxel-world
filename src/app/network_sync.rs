//! Network synchronisation context for the App.
//!
//! All multiplayer state-application logic lives on [`NetworkSyncContext`], a
//! short-lived struct that borrows only the [`App`] fields it needs.  The
//! `impl App` delegates at the bottom forward public calls unchanged, so no
//! call-site in `update.rs` or `world_streaming.rs` needs to change.
//!
//! # Fields used
//! | Method group           | sim | multiplayer | ui |
//! |------------------------|:---:|:-----------:|:--:|
//! | apply_remote_*         |  ✓  |      ✓      |    |
//! | request/apply_chunks   |  ✓  |      ✓      |    |
//! | fulfill_chunk_requests |  ✓  |      ✓      |    |
//! | process_*_uploads      |  ✓  |      ✓      |    |
//! | register_pending_*     |  ✓  |      ✓      |  ✓ |
//! | remove_pending_*       |     |      ✓      |    |

use crate::app_state::{MultiplayerState, UiState, WorldSim};
use crate::chunk::BlockType;

/// Borrows the [`App`] fields required by the network-synchronisation subsystem.
pub(crate) struct NetworkSyncContext<'a> {
    pub sim: &'a mut WorldSim,
    pub multiplayer: &'a mut MultiplayerState,
    pub ui: &'a mut UiState,
}

impl<'a> NetworkSyncContext<'a> {
    /// Applies pending water updates from the server to the local simulation.
    pub fn apply_remote_water_updates(&mut self) {
        if !self.multiplayer.has_pending_water_updates() {
            return;
        }

        let updates = self.multiplayer.take_pending_water_updates();
        for update in updates {
            let pos =
                nalgebra::Vector3::new(update.position[0], update.position[1], update.position[2]);

            if update.mass <= 0.0 {
                self.sim.water_grid.remove_water(pos, 1.0);
                self.sim.world.set_block(pos, BlockType::Air);
            } else if update.is_source {
                self.sim.water_grid.place_source(pos, update.water_type);
                self.sim.world.set_water_block(pos, update.water_type);
            } else {
                self.sim
                    .water_grid
                    .set_water(pos, update.mass, false, update.water_type);
                self.sim.world.set_water_block(pos, update.water_type);
            }
        }
    }

    /// Applies pending lava updates from the server to the local simulation.
    pub fn apply_remote_lava_updates(&mut self) {
        if !self.multiplayer.has_pending_lava_updates() {
            return;
        }

        let updates = self.multiplayer.take_pending_lava_updates();
        for update in updates {
            let pos =
                nalgebra::Vector3::new(update.position[0], update.position[1], update.position[2]);

            if update.mass <= 0.0 {
                self.sim.lava_grid.set_lava(pos, 0.0, false);
                self.sim.world.set_block(pos, BlockType::Air);
            } else if update.is_source {
                self.sim.lava_grid.place_source(pos);
                self.sim.world.set_block(pos, BlockType::Lava);
            } else {
                self.sim.lava_grid.set_lava(pos, update.mass, false);
                self.sim.world.set_block(pos, BlockType::Lava);
            }
        }
    }

    /// Applies a capped batch of pending block changes from the server to the
    /// world (H13 client-side). Limits work per frame so a large `BlocksChanged`
    /// doesn't stall the render loop.
    pub fn apply_remote_block_changes(&mut self) {
        if !self.multiplayer.has_pending_block_changes() {
            return;
        }

        use crate::block_update::BlockUpdateType;

        /// Client-side budget per frame — mirrors the host-side BULK_OPS_PER_TICK.
        const CLIENT_BLOCK_CHANGES_PER_FRAME: usize = 1000;

        let changes = self
            .multiplayer
            .take_pending_block_changes_budgeted(CLIENT_BLOCK_CHANGES_PER_FRAME);
        log::debug!("[Client] Applying {} remote block change(s)", changes.len());

        let player_pos = self
            .sim
            .player
            .feet_pos(self.sim.world_extent, self.sim.texture_origin)
            .cast::<f32>();

        for change in changes {
            let pos =
                nalgebra::Vector3::new(change.position[0], change.position[1], change.position[2]);

            log::debug!(
                "[Client] Applying remote block change at {:?}: {:?}",
                pos,
                change.block.block_type
            );

            let prev_block_type = self.sim.world.get_block(pos);
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
                        if crate::sub_voxel::ModelRegistry::is_frame_model(model_data.model_id) {
                            self.sim.world.update_adjacent_frame_clusters(pos);
                        }
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
                    self.sim.water_grid.place_source(pos, water_type);
                    self.sim.world.set_water_block(pos, water_type);
                }
                _ => {
                    self.sim.world.set_block(pos, block_type);
                }
            }

            self.sim.world.invalidate_minimap_cache(pos.x, pos.z);

            if block_type == BlockType::Air || block_type == BlockType::Water {
                self.sim.block_updates.enqueue(
                    pos + nalgebra::Vector3::new(0, 1, 0),
                    BlockUpdateType::Gravity,
                    player_pos,
                );
                self.sim.block_updates.enqueue(
                    pos + nalgebra::Vector3::new(0, 1, 0),
                    BlockUpdateType::ModelGroundSupport,
                    player_pos,
                );

                if let Some(prev) = prev_block_type {
                    if prev.is_log() {
                        self.sim.block_updates.enqueue_neighbors(
                            pos,
                            BlockUpdateType::TreeSupport,
                            player_pos,
                        );
                    }
                    if prev == BlockType::Model {
                        let particle_color = nalgebra::Vector3::new(0.5, 0.35, 0.2);
                        self.sim
                            .particles
                            .spawn_block_break(pos.cast::<f32>(), particle_color);
                    }
                }

                self.sim.block_updates.enqueue_radius(
                    pos,
                    3,
                    BlockUpdateType::TreeSupport,
                    player_pos,
                );
                self.sim.block_updates.enqueue_radius(
                    pos,
                    4,
                    BlockUpdateType::OrphanedLeaves,
                    player_pos,
                );
            }
        }
    }

    /// Applies pending frame picture set updates from the server.
    pub fn apply_remote_frame_picture_sets(&mut self) {
        if !self.multiplayer.has_pending_frame_picture_sets() {
            return;
        }

        use crate::sub_voxel::builtins::frames;

        let updates = self.multiplayer.take_pending_frame_picture_sets();
        log::debug!(
            "[Client] Applying {} frame picture set update(s)",
            updates.len()
        );

        for update in updates {
            let pos =
                nalgebra::Vector3::new(update.position[0], update.position[1], update.position[2]);

            if let Some(model_data) = self.sim.world.get_model_data(pos)
                && crate::sub_voxel::ModelRegistry::is_frame_model(model_data.model_id)
            {
                let facing = frames::metadata::decode_facing(model_data.custom_data);
                let offset_x = frames::metadata::decode_offset_x(model_data.custom_data);
                let offset_y = frames::metadata::decode_offset_y(model_data.custom_data);
                let width = frames::metadata::decode_width(model_data.custom_data);
                let height = frames::metadata::decode_height(model_data.custom_data);

                let picture_id = update.picture_id.unwrap_or(0) as u32;
                let new_custom_data =
                    frames::metadata::encode(picture_id, offset_x, offset_y, width, height, facing);

                self.sim.world.set_model_block_with_data(
                    pos,
                    model_data.model_id,
                    model_data.rotation,
                    model_data.waterlogged,
                    new_custom_data,
                );

                log::debug!(
                    "[Client] Updated frame at {:?} with picture_id={}",
                    pos,
                    picture_id
                );
            }
        }
    }

    /// Requests chunks from the server when in multiplayer client mode.
    pub fn request_network_chunks(&mut self) {
        let player_world_pos = self
            .sim
            .player
            .feet_pos(self.sim.world_extent, self.sim.texture_origin);

        let yaw = self.sim.player.camera.rotation.y as f32;
        let look_dir = [yaw.sin(), 0.0, -yaw.cos()];

        let _to_cancel = self.multiplayer.chunk_sync.update_player_state(
            [
                player_world_pos.x as f32,
                player_world_pos.y as f32,
                player_world_pos.z as f32,
            ],
            look_dir,
        );

        let player_chunk = self
            .sim
            .player
            .get_chunk_pos(self.sim.world_extent, self.sim.texture_origin);

        if let Some(request) = self.multiplayer.chunk_sync.request_chunks_around(
            [player_chunk.x, player_chunk.y, player_chunk.z],
            self.sim.view_distance,
        ) && let Some(ref mut client) = self.multiplayer.client
        {
            log::debug!(
                "[Client] Requesting {} chunks around player at chunk {:?}",
                request.positions.len(),
                player_chunk
            );
            client.send_chunk_request(request.positions);
            client.flush_packets();
        }
    }

    /// Applies pending network chunks; returns them for the chunk loading system.
    pub fn apply_network_chunks(&mut self) -> Vec<(nalgebra::Vector3<i32>, crate::chunk::Chunk)> {
        if !self.multiplayer.has_pending_chunks() {
            return Vec::new();
        }
        self.multiplayer.take_pending_chunks()
    }

    /// Returns positions of chunks that should be generated locally from seed.
    pub fn take_pending_local_chunks(&mut self) -> Vec<[i32; 3]> {
        self.multiplayer.take_pending_local_chunks()
    }

    /// Fulfills chunk requests from clients (server-side, when hosting).
    pub fn fulfill_chunk_requests(&mut self) {
        if !self.multiplayer.has_pending_chunk_requests() {
            return;
        }

        let requests = self.multiplayer.take_pending_chunk_requests();
        log::debug!("[Server] Fulfilling {} chunk request(s)", requests.len());
        for (client_id, positions) in requests {
            log::debug!(
                "[Server] Client {} requested {} chunk(s)",
                client_id,
                positions.len()
            );
            for chunk_pos in positions {
                let pos = nalgebra::Vector3::new(chunk_pos[0], chunk_pos[1], chunk_pos[2]);
                if let Some(chunk) = self.sim.world.get_chunk(pos) {
                    self.multiplayer
                        .send_chunk_to_client(client_id, chunk_pos, chunk);
                    log::debug!(
                        "[Server] Sent chunk {:?} to client {} (dirty={})",
                        chunk_pos,
                        client_id,
                        chunk.persistence_dirty
                    );
                } else {
                    log::debug!("[Server] Chunk {:?} not found, skipping", chunk_pos);
                }
            }
        }
    }

    /// Applies a capped batch of pending server-side BulkOperation placements
    /// each tick (H13). Drains up to `BULK_OPS_PER_TICK` blocks from the
    /// multiplayer pending queue, applies them to the host world, and
    /// broadcasts them as a single `BlocksChanged` so remote clients catch up
    /// in lockstep. Replace entries whose `from_filter` doesn't match the live
    /// world block are silently skipped.
    pub fn fulfill_bulk_ops(&mut self) {
        use crate::net::protocol::{BlockData, BlocksChanged};
        use nalgebra::Vector3;

        /// Host-side budget per tick. 1000 blocks/tick matches the existing
        /// client-side frame-distributed cadence called out in plan.md.
        const BULK_OPS_PER_TICK: usize = 1000;

        let batch = self.multiplayer.take_bulk_block_batch(BULK_OPS_PER_TICK);
        if batch.is_empty() {
            return;
        }

        let mut applied: Vec<([i32; 3], BlockData)> = Vec::with_capacity(batch.len());
        for (pos, block, from_filter) in batch {
            let wp = Vector3::new(pos[0], pos[1], pos[2]);
            if let Some(from_type) = from_filter {
                let current = self.sim.world.get_block(wp);
                if current != Some(from_type) {
                    continue;
                }
            }
            self.sim.world.set_block(wp, block.block_type);
            applied.push((pos, block));
        }

        if !applied.is_empty()
            && let Some(ref mut server) = self.multiplayer.server
        {
            let msg = BlocksChanged { changes: applied };
            server.broadcast_block_changes(msg);
        }
    }

    /// Processes pending model uploads from clients (server-side, when hosting).
    pub fn process_model_uploads(&mut self) {
        if !self.multiplayer.has_pending_model_uploads() {
            return;
        }

        let uploads = self.multiplayer.take_pending_model_uploads();
        log::debug!("[Server] Processing {} model upload(s)", uploads.len());

        for (_client_id, upload) in uploads {
            use crate::storage::model_format::VxmFile;
            use lz4_flex::decompress_size_prepended;

            const MAX_COMPRESSED_BYTES: usize = 5 * 1024 * 1024;
            if upload.model_data.len() > MAX_COMPRESSED_BYTES {
                log::warn!(
                    "[Server] Rejected model '{}': compressed size {} exceeds {} byte cap",
                    upload.name,
                    upload.model_data.len(),
                    MAX_COMPRESSED_BYTES
                );
                continue;
            }

            const MAX_DECOMPRESSED_BYTES: usize = 50 * 1024 * 1024;
            if upload.model_data.len() < 4 {
                log::warn!(
                    "[Server] Rejected model '{}': payload too short to contain size header",
                    upload.name
                );
                continue;
            }
            let declared_size = u32::from_le_bytes([
                upload.model_data[0],
                upload.model_data[1],
                upload.model_data[2],
                upload.model_data[3],
            ]) as usize;

            if declared_size > MAX_DECOMPRESSED_BYTES {
                log::warn!(
                    "[Server] Rejected model '{}': declared decompressed size {} exceeds {} byte cap",
                    upload.name,
                    declared_size,
                    MAX_DECOMPRESSED_BYTES
                );
                continue;
            }

            let decompressed = match decompress_size_prepended(&upload.model_data) {
                Ok(d) => d,
                Err(e) => {
                    log::warn!(
                        "[Server] Failed to decompress model '{}': {:?}",
                        upload.name,
                        e
                    );
                    continue;
                }
            };

            let vxm: VxmFile =
                match bincode::serde::decode_from_slice(&decompressed, bincode::config::legacy()) {
                    Ok((vxm, _)) => vxm,
                    Err(e) => {
                        log::warn!(
                            "[Server] Failed to deserialize model '{}': {:?}",
                            upload.name,
                            e
                        );
                        continue;
                    }
                };

            let model = vxm.to_model();
            let model_id = match self.sim.model_registry.register(model.clone()) {
                Some(id) => id,
                None => {
                    log::warn!(
                        "[Server] Cannot register model '{}' from client: registry full",
                        upload.name
                    );
                    continue;
                }
            };

            log::debug!(
                "[Server] Registered model '{}' as ID {} from client",
                upload.name,
                model_id
            );

            let library_path = crate::user_prefs::user_models_dir();
            if let Err(e) = crate::storage::model_format::LibraryManager::new(&library_path)
                .save_model(&model, &upload.author)
            {
                log::warn!("[Server] Failed to save model '{}': {}", upload.name, e);
            }

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
    pub fn process_texture_uploads(&mut self) {
        if !self.multiplayer.has_pending_texture_uploads() {
            return;
        }

        let uploads = self.multiplayer.take_pending_texture_uploads();
        log::debug!("[Server] Processing {} texture upload(s)", uploads.len());

        for (_client_id, upload) in uploads {
            if let Some(ref mut manager) = self.multiplayer.server {
                let slot = match manager.add_texture(&upload.name, &upload.png_data) {
                    Ok(s) => s,
                    Err(e) => {
                        log::warn!("[Server] Failed to add texture '{}': {}", upload.name, e);
                        continue;
                    }
                };

                log::debug!(
                    "[Server] Added texture '{}' to slot {} from client",
                    upload.name,
                    slot
                );

                manager.broadcast_texture_added(slot, upload.name.clone(), upload.png_data.clone());
            }
        }
    }

    /// Processes pending picture uploads from clients (server-side, when hosting).
    pub fn process_picture_uploads(&mut self) {
        if !self.multiplayer.has_pending_picture_uploads() {
            return;
        }

        let uploads = self.multiplayer.take_pending_picture_uploads();
        log::debug!("[Server] Processing {} picture upload(s)", uploads.len());

        for (_client_id, upload) in uploads {
            if let Some(ref mut server) = self.multiplayer.server {
                let picture_id = match server.add_picture(&upload.name, &upload.png_data) {
                    Ok(id) => id,
                    Err(e) => {
                        log::warn!("[Server] Failed to add picture '{}': {}", upload.name, e);
                        continue;
                    }
                };

                log::debug!(
                    "[Server] Added picture '{}' with ID {} from client",
                    upload.name,
                    picture_id
                );

                server.broadcast_picture_added(picture_id, upload.name.clone());
            }
        }
    }

    /// Registers pending models received from server (client-side).
    pub fn register_pending_models(&mut self) {
        if !self.multiplayer.has_pending_models() {
            return;
        }

        let models = self.multiplayer.take_pending_models();
        log::debug!("[Client] Registering {} model(s)", models.len());

        for model_added in models {
            use crate::storage::model_format::VxmFile;
            use lz4_flex::decompress_size_prepended;

            let decompressed = match decompress_size_prepended(&model_added.model_data) {
                Ok(d) => d,
                Err(e) => {
                    log::warn!(
                        "[Client] Failed to decompress model '{}': {:?}",
                        model_added.name,
                        e
                    );
                    continue;
                }
            };

            let vxm: VxmFile =
                match bincode::serde::decode_from_slice(&decompressed, bincode::config::legacy()) {
                    Ok((vxm, _)) => vxm,
                    Err(e) => {
                        log::warn!(
                            "[Client] Failed to deserialize model '{}': {:?}",
                            model_added.name,
                            e
                        );
                        continue;
                    }
                };

            let mut model = vxm.to_model();
            model.id = model_added.model_id;

            let registered_id = match self.sim.model_registry.register(model.clone()) {
                Some(id) => id,
                None => {
                    log::warn!(
                        "[Client] Cannot register model '{}': registry full",
                        model_added.name
                    );
                    continue;
                }
            };

            log::debug!(
                "[Client] Registered model '{}' as ID {} (server ID {})",
                model_added.name,
                registered_id,
                model_added.model_id
            );

            let sprites_dir = std::path::Path::new("textures/rendered");
            if std::fs::create_dir_all(sprites_dir).is_ok() {
                let sprite_path = sprites_dir.join(format!("model_{}.png", registered_id));
                if crate::editor::rasterizer::generate_model_sprite(&model, &sprite_path).is_ok() {
                    log::debug!("[Client] Generated sprite for model {}", registered_id);
                }
            }
        }
    }

    /// Registers pending templates received from server (client-side).
    pub fn register_pending_templates(&mut self) {
        if !self.multiplayer.has_pending_template_loads() {
            return;
        }

        let templates = self.multiplayer.take_pending_template_loads();
        log::debug!("[Client] Registering {} template(s)", templates.len());

        for template_loaded in templates {
            use lz4_flex::decompress_size_prepended;

            let decompressed = match decompress_size_prepended(&template_loaded.template_data) {
                Ok(d) => d,
                Err(e) => {
                    log::warn!(
                        "[Client] Failed to decompress template '{}': {:?}",
                        template_loaded.name,
                        e
                    );
                    continue;
                }
            };

            let vxt: crate::templates::VxtFile =
                match crate::templates::VxtFile::from_bytes(&decompressed) {
                    Ok(v) => v,
                    Err(e) => {
                        log::warn!(
                            "[Client] Failed to deserialize template '{}': {}",
                            template_loaded.name,
                            e
                        );
                        continue;
                    }
                };

            if let Err(e) = self.ui.template_library.save_template(&vxt) {
                log::warn!(
                    "[Client] Failed to save template '{}' to library: {}",
                    template_loaded.name,
                    e
                );
                continue;
            }

            log::debug!(
                "[Client] Registered template '{}' (server ID {})",
                template_loaded.name,
                template_loaded.template_id
            );
        }
    }

    /// Removes pending templates received from server (client-side).
    pub fn remove_pending_templates(&mut self) {
        if !self.multiplayer.has_pending_template_removals() {
            return;
        }

        let removals = self.multiplayer.take_pending_template_removals();
        log::debug!("[Client] Processing {} template removal(s)", removals.len());

        for removed in removals {
            log::debug!(
                "[Client] Template removal notification received (server ID {})",
                removed.template_id
            );
        }
    }
}

// ── Thin `impl App` delegates ─────────────────────────────────────────────────

impl crate::app::core::App {
    #[inline]
    pub fn apply_remote_water_updates(&mut self) {
        NetworkSyncContext {
            sim: &mut self.sim,
            multiplayer: &mut self.multiplayer,
            ui: &mut self.ui,
        }
        .apply_remote_water_updates();
    }

    #[inline]
    pub fn apply_remote_lava_updates(&mut self) {
        NetworkSyncContext {
            sim: &mut self.sim,
            multiplayer: &mut self.multiplayer,
            ui: &mut self.ui,
        }
        .apply_remote_lava_updates();
    }

    #[inline]
    pub fn apply_remote_block_changes(&mut self) {
        NetworkSyncContext {
            sim: &mut self.sim,
            multiplayer: &mut self.multiplayer,
            ui: &mut self.ui,
        }
        .apply_remote_block_changes();
    }

    #[inline]
    pub fn apply_remote_frame_picture_sets(&mut self) {
        NetworkSyncContext {
            sim: &mut self.sim,
            multiplayer: &mut self.multiplayer,
            ui: &mut self.ui,
        }
        .apply_remote_frame_picture_sets();
    }

    #[inline]
    pub fn request_network_chunks(&mut self) {
        NetworkSyncContext {
            sim: &mut self.sim,
            multiplayer: &mut self.multiplayer,
            ui: &mut self.ui,
        }
        .request_network_chunks();
    }

    #[inline]
    pub fn apply_network_chunks(&mut self) -> Vec<(nalgebra::Vector3<i32>, crate::chunk::Chunk)> {
        NetworkSyncContext {
            sim: &mut self.sim,
            multiplayer: &mut self.multiplayer,
            ui: &mut self.ui,
        }
        .apply_network_chunks()
    }

    #[inline]
    pub fn take_pending_local_chunks(&mut self) -> Vec<[i32; 3]> {
        NetworkSyncContext {
            sim: &mut self.sim,
            multiplayer: &mut self.multiplayer,
            ui: &mut self.ui,
        }
        .take_pending_local_chunks()
    }

    #[inline]
    pub fn has_pending_local_chunks(&self) -> bool {
        self.multiplayer.has_pending_local_chunks()
    }

    #[inline]
    pub fn fulfill_chunk_requests(&mut self) {
        NetworkSyncContext {
            sim: &mut self.sim,
            multiplayer: &mut self.multiplayer,
            ui: &mut self.ui,
        }
        .fulfill_chunk_requests();
    }

    #[inline]
    pub fn fulfill_bulk_ops(&mut self) {
        NetworkSyncContext {
            sim: &mut self.sim,
            multiplayer: &mut self.multiplayer,
            ui: &mut self.ui,
        }
        .fulfill_bulk_ops();
    }

    #[inline]
    pub fn process_model_uploads(&mut self) {
        NetworkSyncContext {
            sim: &mut self.sim,
            multiplayer: &mut self.multiplayer,
            ui: &mut self.ui,
        }
        .process_model_uploads();
    }

    #[inline]
    pub fn process_texture_uploads(&mut self) {
        NetworkSyncContext {
            sim: &mut self.sim,
            multiplayer: &mut self.multiplayer,
            ui: &mut self.ui,
        }
        .process_texture_uploads();
    }

    #[inline]
    pub fn process_picture_uploads(&mut self) {
        NetworkSyncContext {
            sim: &mut self.sim,
            multiplayer: &mut self.multiplayer,
            ui: &mut self.ui,
        }
        .process_picture_uploads();
    }

    #[inline]
    pub fn register_pending_models(&mut self) {
        NetworkSyncContext {
            sim: &mut self.sim,
            multiplayer: &mut self.multiplayer,
            ui: &mut self.ui,
        }
        .register_pending_models();
    }

    #[inline]
    pub fn register_pending_templates(&mut self) {
        NetworkSyncContext {
            sim: &mut self.sim,
            multiplayer: &mut self.multiplayer,
            ui: &mut self.ui,
        }
        .register_pending_templates();
    }

    #[inline]
    pub fn remove_pending_templates(&mut self) {
        NetworkSyncContext {
            sim: &mut self.sim,
            multiplayer: &mut self.multiplayer,
            ui: &mut self.ui,
        }
        .remove_pending_templates();
    }
}

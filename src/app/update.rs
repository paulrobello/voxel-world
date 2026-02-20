//! App update loop

use super::App;
use crate::app::stats::print_stats;
use crate::app_state::AutoProfileFeature;
use crate::constants::{DAY_CYCLE_DURATION, TEXTURE_SIZE_Y};
use crate::gpu_resources::upload_model_registry;
use nalgebra::Vector3;
use std::time::{Duration, Instant};
use winit::event_loop::ActiveEventLoop;

impl App {
    /// Checks if texture origin needs to shift and handles re-upload if necessary.
    /// Returns true if a shift occurred.
    pub fn update(&mut self, event_loop: &ActiveEventLoop) {
        self.ui.total_frames += 1;
        let now = Instant::now();

        // Check for screenshot delay
        if let Some(delay) = self.args.screenshot_delay {
            if !self.ui.screenshot_taken {
                let elapsed = now.duration_since(self.start_time).as_secs_f64();
                if elapsed >= delay {
                    // Mark that we need to take a screenshot on the next render
                    // The actual screenshot will be taken in render()
                    println!(
                        "[SCREENSHOT] Taking screenshot after {:.1}s (saving to voxel-world_screen_shot.png)",
                        elapsed
                    );
                }
            }
        }

        // Check for exit delay
        if let Some(delay) = self.args.exit_delay {
            let elapsed = now.duration_since(self.start_time).as_secs_f64();
            if elapsed >= delay {
                println!("[EXIT] Exiting after {:.1}s delay", elapsed);
                self.sim
                    .save_all(&self.ui.measurement_markers, &self.ui.stencil_manager);
                self.save_preferences();
                event_loop.exit();
                return;
            }
        }

        // Check for benchmark duration (auto-exit for profiling runs)
        if let Some(duration) = self.args.benchmark_duration {
            let elapsed = now.duration_since(self.start_time).as_secs_f64();
            if elapsed >= duration {
                println!("[BENCHMARK] Complete after {:.1}s", duration);
                self.sim
                    .save_all(&self.ui.measurement_markers, &self.ui.stencil_manager);
                self.save_preferences();
                event_loop.exit();
                return;
            }
        }

        if now.duration_since(self.ui.last_second) > Duration::from_secs(1) {
            self.ui.fps = self.ui.frames_since_last_second;
            self.ui.frames_since_last_second = 0;
            self.ui.last_second = now;

            // Update smoothed FPS (exponential moving average, alpha=0.15 for stability)
            let alpha = 0.15;
            self.ui.smoothed_fps =
                self.ui.smoothed_fps * (1.0 - alpha) + self.ui.fps as f32 * alpha;

            // Dynamic render scale adjustment
            if self.ui.settings.dynamic_render_scale {
                let target = self.ui.settings.dynamic_render_scale_target_fps;
                let min_scale = self.ui.settings.dynamic_render_scale_min;
                let max_scale = self.ui.settings.dynamic_render_scale_max;
                let current_scale = self.ui.settings.render_scale;

                // Use deadband of ±5% of target FPS to prevent oscillation
                let deadband = target * 0.05;
                let fps_error = self.ui.smoothed_fps - target;

                // Only adjust if outside the deadband
                if fps_error.abs() > deadband {
                    // Adjustment rate: larger adjustments when further from target
                    // Base rate of 0.02 per second, scaled by error magnitude
                    let adjustment_rate = 0.02 * (fps_error.abs() / target).min(0.5);

                    let new_scale = if fps_error < -deadband {
                        // FPS too low, decrease render scale
                        (current_scale - adjustment_rate).max(min_scale)
                    } else {
                        // FPS too high, increase render scale
                        (current_scale + adjustment_rate).min(max_scale)
                    };

                    if (new_scale - current_scale).abs() > 0.001 {
                        self.ui.settings.render_scale = new_scale;
                        self.ui.pending_scale_change = true;
                    }
                }
            }

            print_stats(&mut self.ui, &mut self.sim, self.args.verbose);

            // Auto-profile state machine: toggle features, then fly
            if self.ui.auto_profile_enabled {
                // Phase duration is dynamic: 5s for toggle tests, 30s for flying
                let phase_duration =
                    Duration::from_secs(self.ui.auto_profile_feature.duration_secs());
                let phase_elapsed = now.duration_since(self.ui.auto_profile_phase_start);

                if phase_elapsed >= phase_duration {
                    self.ui.auto_profile_phase_start = now;

                    match self.ui.auto_profile_feature {
                        AutoProfileFeature::Baseline => {
                            // Move to first feature test (AO OFF)
                            self.ui.auto_profile_feature = AutoProfileFeature::AO;
                            self.ui.auto_profile_feature_off = true;
                            self.ui.settings.enable_ao = false;
                            println!("[AUTO-PROFILE] Testing AO: OFF");
                        }
                        AutoProfileFeature::Flying => {
                            // Flying phase complete, move to Done
                            self.ui.auto_profile_feature = AutoProfileFeature::Done;
                            self.sim.player.auto_fly_enabled = false;
                            println!("[AUTO-PROFILE] Flying phase complete.");
                        }
                        AutoProfileFeature::Done => {
                            // Exit the application
                            println!("[AUTO-PROFILE] Complete! Exiting...");
                            self.sim
                                .save_all(&self.ui.measurement_markers, &self.ui.stencil_manager);
                            self.save_preferences();
                            std::process::exit(0);
                        }
                        _ => {
                            if self.ui.auto_profile_feature_off {
                                // Feature was OFF/MIN, turn it back ON/MAX
                                self.ui.auto_profile_feature_off = false;
                                match self.ui.auto_profile_feature {
                                    AutoProfileFeature::AO => self.ui.settings.enable_ao = true,
                                    AutoProfileFeature::Shadows => {
                                        self.ui.settings.enable_shadows = true
                                    }
                                    AutoProfileFeature::ModelShadows => {
                                        self.ui.settings.enable_model_shadows = true
                                    }
                                    AutoProfileFeature::PointLights => {
                                        self.ui.settings.enable_point_lights = true
                                    }
                                    AutoProfileFeature::LightCullRadius => {
                                        self.ui.settings.light_cull_radius = 128.0; // MAX
                                    }
                                    AutoProfileFeature::MaxActiveLights => {
                                        self.ui.settings.max_active_lights = 256; // MAX
                                    }
                                    AutoProfileFeature::Minimap => self.ui.show_minimap = true,
                                    AutoProfileFeature::MinimapSkipDecorative => {
                                        self.ui.minimap.skip_decorative = true;
                                        self.ui.minimap_cached_image = None; // Clear cache
                                        self.sim.world.clear_minimap_cache();
                                    }
                                    AutoProfileFeature::HideGroundCover => {
                                        self.ui.settings.hide_ground_cover = true
                                    }
                                    _ => {}
                                }
                                let state_name = match self.ui.auto_profile_feature {
                                    AutoProfileFeature::LightCullRadius => "MAX (128)",
                                    AutoProfileFeature::MaxActiveLights => "MAX (256)",
                                    _ => "ON",
                                };
                                println!(
                                    "[AUTO-PROFILE] Testing {}: {}",
                                    self.ui.auto_profile_feature.name(),
                                    state_name
                                );
                            } else {
                                // Feature was ON/MAX, move to next feature (OFF/MIN or Flying)
                                self.ui.auto_profile_feature = self.ui.auto_profile_feature.next();
                                self.ui.auto_profile_feature_off = true;
                                match self.ui.auto_profile_feature {
                                    AutoProfileFeature::AO => self.ui.settings.enable_ao = false,
                                    AutoProfileFeature::Shadows => {
                                        self.ui.settings.enable_shadows = false
                                    }
                                    AutoProfileFeature::ModelShadows => {
                                        self.ui.settings.enable_model_shadows = false
                                    }
                                    AutoProfileFeature::PointLights => {
                                        self.ui.settings.enable_point_lights = false
                                    }
                                    AutoProfileFeature::LightCullRadius => {
                                        self.ui.settings.light_cull_radius = 16.0; // MIN
                                    }
                                    AutoProfileFeature::MaxActiveLights => {
                                        self.ui.settings.max_active_lights = 8; // MIN
                                    }
                                    AutoProfileFeature::Minimap => self.ui.show_minimap = false,
                                    AutoProfileFeature::MinimapSkipDecorative => {
                                        self.ui.minimap.skip_decorative = false;
                                        self.ui.minimap_cached_image = None; // Clear cache
                                        self.sim.world.clear_minimap_cache();
                                    }
                                    AutoProfileFeature::HideGroundCover => {
                                        self.ui.settings.hide_ground_cover = false
                                    }
                                    AutoProfileFeature::Flying => {
                                        // Start auto-fly for streaming test
                                        self.sim.player.auto_fly_enabled = true;
                                        self.sim.player.fly_mode = true;
                                        self.sim.player.auto_fly_time = 0.0;
                                        // Set camera to face +X direction (yaw = -π/2)
                                        // Angle down 30° to see terrain ahead
                                        self.sim.player.camera.rotation.y =
                                            -std::f64::consts::FRAC_PI_2;
                                        self.sim.player.camera.rotation.x =
                                            -std::f64::consts::FRAC_PI_6;
                                        println!(
                                            "[AUTO-PROFILE] Starting Flying phase ({}s, straight +X)",
                                            self.ui.auto_profile_feature.duration_secs()
                                        );
                                        self.ui.auto_profile_feature_off = false; // No OFF phase
                                    }
                                    AutoProfileFeature::Done => {
                                        // Should not reach here (Flying handles transition)
                                        self.ui.auto_profile_feature_off = false;
                                    }
                                    _ => {}
                                }
                                // Print message for toggle features (not Flying/Done)
                                if self.ui.auto_profile_feature != AutoProfileFeature::Flying
                                    && self.ui.auto_profile_feature != AutoProfileFeature::Done
                                {
                                    let state_name = match self.ui.auto_profile_feature {
                                        AutoProfileFeature::LightCullRadius => "MIN (16)",
                                        AutoProfileFeature::MaxActiveLights => "MIN (8)",
                                        _ => "OFF",
                                    };
                                    println!(
                                        "[AUTO-PROFILE] Testing {}: {}",
                                        self.ui.auto_profile_feature.name(),
                                        state_name
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        self.ui.frames_since_last_second += 1;

        // Debug interval output
        if self.args.debug_interval > 0
            && self.ui.total_frames % self.args.debug_interval as u64 == 0
        {
            let player_pos = self
                .sim
                .player
                .feet_pos(self.sim.world_extent, self.sim.texture_origin);
            let player_chunk = self
                .sim
                .player
                .get_chunk_pos(self.sim.world_extent, self.sim.texture_origin);
            println!(
                "[DEBUG Frame {}] Pos: ({:.2}, {:.2}, {:.2}) Chunk: ({}, {}, {}) TexOrigin: ({}, {}, {}) Velocity: ({:.2}, {:.2}, {:.2})",
                self.ui.total_frames,
                player_pos.x,
                player_pos.y,
                player_pos.z,
                player_chunk.x,
                player_chunk.y,
                player_chunk.z,
                self.sim.texture_origin.x,
                self.sim.texture_origin.y,
                self.sim.texture_origin.z,
                self.sim.player.velocity.x,
                self.sim.player.velocity.y,
                self.sim.player.velocity.z
            );
        }

        // Always update chunks and upload to GPU, even before delta_time is available
        // This ensures initial chunks are uploaded on the first frame
        let t0 = Instant::now();
        let (loaded_positions, loaded, _unloaded) = self.update_chunk_loading();
        self.sim.profiler.chunk_loading_us += t0.elapsed().as_micros() as u64;

        // Invalidate minimap cache when new chunks are loaded so it refreshes
        if loaded > 0 {
            self.ui.minimap_cached_image = None;
        }

        // Mark loaded chunks as complete in multiplayer chunk sync
        // This ensures chunks from ChunkGenerateLocal are properly tracked
        for pos in &loaded_positions {
            self.multiplayer
                .chunk_sync
                .try_complete_local_generation([pos.x, pos.y, pos.z]);
        }

        let t1 = Instant::now();
        self.upload_world_to_gpu();
        self.sim.profiler.gpu_upload_us += t1.elapsed().as_micros() as u64;

        // Refresh model GPU data if models were updated (e.g., after editing in model editor)
        if self.sim.model_registry.is_gpu_dirty() {
            upload_model_registry(
                self.graphics.memory_allocator.clone(),
                self.graphics.command_buffer_allocator.clone(),
                &self.graphics.queue,
                &self.sim.model_registry,
                &self.graphics.model_atlas_8,
                &self.graphics.model_atlas_16,
                &self.graphics.model_atlas_32,
                &self.graphics.model_palettes,
                &self.graphics.model_palette_emission,
                &self.graphics.model_properties_buffer,
            );
            self.sim.model_registry.clear_gpu_dirty();
        }

        self.sim.auto_save(&self.ui.measurement_markers);

        // Amortized metadata refresh runs once per frame.
        self.update_metadata_buffers();

        let Some(delta_time) = self.input.delta_time().as_ref().map(Duration::as_secs_f64) else {
            return;
        };

        // Update multiplayer networking (process server/client updates)
        if self.multiplayer.mode != crate::config::GameMode::SinglePlayer {
            self.multiplayer.update(Duration::from_secs_f64(delta_time));

            // Upload any received custom textures to the GPU
            self.upload_multiplayer_textures();

            // Interpolate remote player positions for smooth rendering
            self.multiplayer.update_remote_players();

            // Update host player position on server (so it's broadcast to clients)
            // IMPORTANT: Use world coordinates, not normalized texture coordinates
            if self.multiplayer.mode == crate::config::GameMode::Host {
                let world_extent = self.sim.world_extent;
                let texture_origin = self.sim.texture_origin;
                let player_world_pos = self.sim.player.feet_pos(world_extent, texture_origin);
                let player_yaw = self.sim.player.camera.rotation.y as f32;
                let player_pitch = self.sim.player.camera.rotation.x as f32;
                self.multiplayer.update_host_position(
                    [
                        player_world_pos.x as f32,
                        player_world_pos.y as f32,
                        player_world_pos.z as f32,
                    ],
                    [0.0, 0.0, 0.0], // TODO: get actual velocity
                    player_yaw,
                    player_pitch,
                );
            }

            // Send client position to server (both pure clients and host's local client)
            // This is separate from host position update above - that updates the server's
            // knowledge of the host player, this sends our position as a client
            if self.multiplayer.mode == crate::config::GameMode::Client
                || self.multiplayer.mode == crate::config::GameMode::Host
            {
                let world_extent = self.sim.world_extent;
                let texture_origin = self.sim.texture_origin;
                let player_world_pos = self.sim.player.feet_pos(world_extent, texture_origin);
                let player_yaw = self.sim.player.camera.rotation.y as f32;
                let player_pitch = self.sim.player.camera.rotation.x as f32;

                // Send input ~20 times per second (every 3 frames at 60fps)
                static INPUT_COUNTER: std::sync::atomic::AtomicU64 =
                    std::sync::atomic::AtomicU64::new(0);
                let count = INPUT_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if count % 3 == 0 {
                    self.multiplayer.send_input(
                        [
                            player_world_pos.x as f32,
                            player_world_pos.y as f32,
                            player_world_pos.z as f32,
                        ],
                        [0.0, 0.0, 0.0], // TODO: get actual velocity
                        player_yaw,
                        player_pitch,
                        crate::net::protocol::InputActions::new(0),
                    );
                }
            }

            // Check if we received the server's world seed (on ConnectionAccepted)
            if self.multiplayer.has_pending_server_seed() {
                if let Some((seed, world_gen_byte)) = self.multiplayer.take_pending_server_seed() {
                    let world_gen = match world_gen_byte {
                        1 => crate::config::WorldGenType::Flat,
                        2 => crate::config::WorldGenType::Benchmark,
                        _ => crate::config::WorldGenType::Normal,
                    };
                    println!(
                        "[Client] Applying server's world seed: {} (gen: {:?})",
                        seed, world_gen
                    );

                    // Clear the chunk sync state so we request fresh chunks
                    self.multiplayer.chunk_sync.clear_received();

                    // Update world seed and clear local world
                    self.sim.set_world_seed(seed, world_gen);

                    println!("[Client] World reset complete, ready to load server's world");
                }
            }

            // Apply any remote block changes received from server
            self.apply_remote_block_changes();

            // Apply any remote water updates received from server
            self.apply_remote_water_updates();

            // Apply any remote lava updates received from server
            self.apply_remote_lava_updates();

            // Request chunks from server when in client mode (or as host's local client)
            // Both pure clients AND hosts need to request chunks (host has a local client)
            if self.multiplayer.mode == crate::config::GameMode::Client
                || self.multiplayer.mode == crate::config::GameMode::Host
            {
                self.request_network_chunks();

                // Handle chunks that should be generated locally (bandwidth optimization)
                // Server sends ChunkGenerateLocal for unmodified chunks
                if self.has_pending_local_chunks() {
                    let local_positions = self.take_pending_local_chunks();
                    let positions: Vec<nalgebra::Vector3<i32>> = local_positions
                        .iter()
                        .map(|p| nalgebra::Vector3::new(p[0], p[1], p[2]))
                        .collect();
                    // Request from chunk_loader - it will generate using same seed as server
                    let _ = self.sim.chunk_loader.request_chunks(&positions);
                }

                // Apply chunks received from server (full chunk data)
                if self.multiplayer.has_pending_chunks() {
                    let chunks = self.apply_network_chunks();
                    for (pos, chunk) in chunks {
                        // Insert the received chunk into the world
                        self.sim.world.insert_chunk(pos, chunk);
                        println!("[Client] Applied received chunk at {:?}", pos);
                    }
                }
            }

            // Fulfill chunk requests from clients when hosting
            if self.multiplayer.is_hosting() {
                self.fulfill_chunk_requests();
                // Process model uploads from clients
                self.process_model_uploads();
                // Process texture uploads from clients
                self.process_texture_uploads();
            }

            // Register models received from server (all connected clients)
            self.register_pending_models();
        }

        if self.input.close_requested() {
            println!("[Storage] Saving world before exit...");
            self.sim
                .save_all(&self.ui.measurement_markers, &self.ui.stencil_manager);
            println!("[Prefs] Saving user preferences...");
            self.save_preferences();
            event_loop.exit();
            return;
        }

        if self.handle_focus_toggles() {
            return;
        }

        self.handle_global_shortcuts();

        // Sync custom textures to GPU if needed
        if self.ui.texture_generator.needs_gpu_sync {
            self.sync_custom_textures();
            self.ui.texture_generator.needs_gpu_sync = false;
        }

        // Upload texture to server if needed (multiplayer sync)
        if let Some(slot) = self.ui.texture_generator.pending_multiplayer_upload.take() {
            // Only upload if connected as client (not host)
            if self.multiplayer.is_connected() && !self.multiplayer.is_host() {
                // Get the texture from the library
                if let Some(texture) = self.ui.texture_library.get(slot) {
                    // Convert pixels to PNG
                    if !texture.pixels.is_empty() {
                        let png_data = self.encode_texture_as_png(&texture.pixels);
                        self.multiplayer
                            .send_upload_texture(texture.name.clone(), png_data);
                        println!(
                            "[Texture] Uploading texture '{}' (slot {}) to server",
                            texture.name, slot
                        );
                    }
                }
            }
        }

        // Restore focus if palette was closed externally and no other panel is open
        let other_panel_open = self.ui.editor.active
            || self.ui.console.active
            || self.ui.texture_generator.open
            || self.ui.paint_panel.open;
        if !self.ui.palette_open
            && self.ui.palette_previously_focused
            && !self.input.focused
            && !other_panel_open
        {
            self.input.focused = true;
            self.input.pending_grab = Some(true);
            self.input.skip_input_frame = true;
            self.ui.palette_previously_focused = false;
        }
        if !self.ui.palette_open {
            self.ui.dragging_item = None;
        }

        // Apply pending day cycle pause state from server (client-side)
        if let Some(pause) = self.multiplayer.take_pending_day_cycle_pause() {
            self.sim.day_cycle_paused = pause.paused;
            self.sim.time_of_day = pause.time_of_day;
            println!(
                "[Client] Applied day cycle sync: {} at time {:.3}",
                if pause.paused { "PAUSED" } else { "RUNNING" },
                pause.time_of_day
            );
        }

        // Apply pending time of day update from server (client-side)
        if let Some(time) = self.multiplayer.take_pending_time_update() {
            self.sim.time_of_day = time;
        }

        // Update day/night cycle
        if !self.sim.day_cycle_paused {
            self.sim.time_of_day += delta_time as f32 / DAY_CYCLE_DURATION;
            self.sim.time_of_day = self.sim.time_of_day.rem_euclid(1.0);
        }

        // Update animation time (always advances for water waves, etc.)
        self.sim.animation_time += delta_time as f32;

        // Update particle system with world collision
        // Note: X and Z can be any value in an infinite world, only Y has bounds
        let world = &self.sim.world;
        self.sim.particles.update(delta_time as f32, |x, y, z| {
            // Y bounds check only (X and Z are infinite)
            if y < 0 || y >= TEXTURE_SIZE_Y as i32 {
                return false;
            }
            // Check if block is solid - world.get_block handles infinite X/Z
            world
                .get_block(Vector3::new(x, y, z))
                .is_some_and(|b| b.is_solid())
        });

        // Update falling blocks with world collision
        // Server-authoritative: Only process physics on server (host) or in single-player
        // Pure clients only spawn/render falling blocks based on network messages
        if !self.multiplayer.is_client() {
            // Note: X and Z can be any value in an infinite world, only Y has bounds
            let landed = self
                .sim
                .falling_blocks
                .update(delta_time as f32, |x, y, z| {
                    // Y bounds check only (X and Z are infinite)
                    if y < 0 || y >= TEXTURE_SIZE_Y as i32 {
                        return false;
                    }
                    // Check if block is solid - world.get_block handles infinite X/Z
                    world
                        .get_block(Vector3::new(x, y, z))
                        .is_some_and(|b| b.is_solid())
                });

            // Process any blocks that have landed
            if !landed.is_empty() {
                // When hosting, broadcast landings to all clients
                if self.multiplayer.is_host() {
                    for lb in &landed {
                        self.multiplayer.broadcast_falling_block_land(
                            0, // entity_id = 0 for legacy sync without tracking
                            [lb.position.x, lb.position.y, lb.position.z],
                            lb.block_type,
                        );
                    }
                }
                self.process_landed_blocks(landed);
            }
        }

        // Process incoming falling block messages from server (client-side)
        if self.multiplayer.has_pending_falling_block_spawns() {
            for spawn in self.multiplayer.take_pending_falling_block_spawns() {
                // Spawn falling block from network message
                let grid_pos = Vector3::new(
                    spawn.position[0].floor() as i32,
                    spawn.position[1].floor() as i32,
                    spawn.position[2].floor() as i32,
                );
                self.sim.falling_blocks.spawn(grid_pos, spawn.block_type);
            }
        }

        // Process incoming landing messages from server (client-side)
        if self.multiplayer.has_pending_falling_block_lands() {
            let mut all_landed = Vec::new();
            for land in self.multiplayer.take_pending_falling_block_lands() {
                all_landed.push(crate::falling_block::LandedBlock {
                    position: Vector3::new(land.position[0], land.position[1], land.position[2]),
                    block_type: land.block_type,
                });
            }
            if !all_landed.is_empty() {
                self.process_landed_blocks(all_landed);
            }
        }

        // Process queued block physics updates (frame-distributed to prevent FPS spikes)
        // Server-authoritative: Only process physics on server (host) or in single-player
        // Pure clients receive physics results from server via network messages
        if !self.multiplayer.is_client() {
            let player_pos_f32 = self
                .sim
                .player
                .feet_pos(self.sim.world_extent, self.sim.texture_origin)
                .cast::<f32>();
            let spawn_events = self.sim.block_updates.process_updates(
                &mut self.sim.world,
                &mut self.sim.falling_blocks,
                &mut self.sim.particles,
                &self.sim.model_registry,
                player_pos_f32,
            );

            // Broadcast falling block spawns to all clients when hosting
            if self.multiplayer.is_host() && !spawn_events.is_empty() {
                for event in spawn_events {
                    let position = [
                        event.position.x as f32 + 0.5,
                        event.position.y as f32 + 0.5,
                        event.position.z as f32 + 0.5,
                    ];
                    self.multiplayer
                        .broadcast_falling_block_spawn(position, event.block_type);
                }
            }
        }

        // Process water flow simulation (frame-distributed)
        // Server-authoritative: Only process simulation on server (host) or in single-player
        // Pure clients receive water/lava state from server via network messages
        if self.ui.settings.water_simulation_enabled && !self.multiplayer.is_client() {
            let player_pos_f32 = self
                .sim
                .player
                .feet_pos(self.sim.world_extent, self.sim.texture_origin)
                .cast::<f32>();
            let water_updates = self.sim.water_grid.process_simulation(
                &mut self.sim.world,
                &mut self.sim.lava_grid,
                player_pos_f32,
            );

            // Broadcast water updates to all clients when hosting
            if self.multiplayer.is_host() && !water_updates.is_empty() {
                self.multiplayer.broadcast_water_cell_updates(water_updates);
            }

            // Process lava flow simulation (uses same enabled flag as water)
            let lava_updates = self.sim.lava_grid.process_simulation(
                &mut self.sim.world,
                &mut self.sim.water_grid,
                player_pos_f32,
            );

            // Broadcast lava updates to all clients when hosting
            if self.multiplayer.is_host() && !lava_updates.is_empty() {
                self.multiplayer.broadcast_lava_cell_updates(lava_updates);
            }
        }

        self.handle_focused_controls(delta_time);
        self.handle_block_interactions(delta_time as f32);
    }
}

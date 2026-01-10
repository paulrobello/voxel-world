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
                        "[SCREENSHOT] Taking screenshot after {:.1}s (saving to voxel_world_screen_shot.png)",
                        elapsed
                    );
                }
            }
        }

        if now.duration_since(self.ui.last_second) > Duration::from_secs(1) {
            self.ui.fps = self.ui.frames_since_last_second;
            self.ui.frames_since_last_second = 0;
            self.ui.last_second = now;

            print_stats(&mut self.ui, &mut self.sim, self.args.verbose);

            // Auto-profile state machine: toggle features every 5 seconds
            if self.ui.auto_profile_enabled {
                const PHASE_DURATION: Duration = Duration::from_secs(5);
                let phase_elapsed = now.duration_since(self.ui.auto_profile_phase_start);

                if phase_elapsed >= PHASE_DURATION {
                    self.ui.auto_profile_phase_start = now;

                    match self.ui.auto_profile_feature {
                        AutoProfileFeature::Baseline => {
                            // Move to first feature test (AO OFF)
                            self.ui.auto_profile_feature = AutoProfileFeature::AO;
                            self.ui.auto_profile_feature_off = true;
                            self.ui.settings.enable_ao = false;
                            println!("[AUTO-PROFILE] Testing AO: OFF");
                        }
                        AutoProfileFeature::Done => {
                            // Exit the application
                            println!("[AUTO-PROFILE] Complete! Exiting...");
                            std::process::exit(0);
                        }
                        _ => {
                            if self.ui.auto_profile_feature_off {
                                // Feature was OFF, turn it back ON
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
                                println!(
                                    "[AUTO-PROFILE] Testing {}: ON",
                                    self.ui.auto_profile_feature.name()
                                );
                            } else {
                                // Feature was ON, move to next feature (OFF)
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
                                    AutoProfileFeature::Minimap => self.ui.show_minimap = false,
                                    AutoProfileFeature::MinimapSkipDecorative => {
                                        self.ui.minimap.skip_decorative = false;
                                        self.ui.minimap_cached_image = None; // Clear cache
                                        self.sim.world.clear_minimap_cache();
                                    }
                                    AutoProfileFeature::HideGroundCover => {
                                        self.ui.settings.hide_ground_cover = false
                                    }
                                    AutoProfileFeature::Done => {
                                        println!(
                                            "[AUTO-PROFILE] All features tested. Final 5s baseline..."
                                        );
                                        self.ui.auto_profile_feature_off = false;
                                    }
                                    _ => {}
                                }
                                if self.ui.auto_profile_feature != AutoProfileFeature::Done {
                                    println!(
                                        "[AUTO-PROFILE] Testing {}: OFF",
                                        self.ui.auto_profile_feature.name()
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
        self.update_chunk_loading();
        self.sim.profiler.chunk_loading_us += t0.elapsed().as_micros() as u64;

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

        self.sim.auto_save();

        // Amortized metadata refresh runs once per frame.
        self.update_metadata_buffers();

        let Some(delta_time) = self.input.delta_time().as_ref().map(Duration::as_secs_f64) else {
            return;
        };

        if self.input.close_requested() {
            println!("[Storage] Saving world before exit...");
            self.sim.save_all();
            println!("[Prefs] Saving user preferences...");
            self.save_preferences();
            event_loop.exit();
            return;
        }

        if self.handle_focus_toggles() {
            return;
        }

        self.handle_global_shortcuts();

        // Restore focus if palette was closed externally and no other panel is open
        let other_panel_open = self.ui.editor.active || self.ui.console.active;
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
            self.process_landed_blocks(landed);
        }

        // Process queued block physics updates (frame-distributed to prevent FPS spikes)
        let player_pos_f32 = self
            .sim
            .player
            .feet_pos(self.sim.world_extent, self.sim.texture_origin)
            .cast::<f32>();
        self.sim.block_updates.process_updates(
            &mut self.sim.world,
            &mut self.sim.falling_blocks,
            &mut self.sim.particles,
            &self.sim.model_registry,
            player_pos_f32,
        );

        // Process water flow simulation (frame-distributed)
        if self.ui.settings.water_simulation_enabled {
            let player_pos_f32 = self
                .sim
                .player
                .feet_pos(self.sim.world_extent, self.sim.texture_origin)
                .cast::<f32>();
            self.sim.water_grid.process_simulation(
                &mut self.sim.world,
                &mut self.sim.lava_grid,
                player_pos_f32,
            );

            // Process lava flow simulation (uses same enabled flag as water)
            self.sim.lava_grid.process_simulation(
                &mut self.sim.world,
                &mut self.sim.water_grid,
                player_pos_f32,
            );
        }

        // Update water visual smoothing every frame (even if simulation disabled)
        // This makes water level transitions smooth instead of jumpy
        let _visual_changes = self.sim.water_grid.update_visuals(delta_time as f32);

        self.handle_focused_controls(delta_time);
        self.handle_block_interactions(delta_time as f32);
    }
}

//! Rendering logic for the application.

use crate::app::core::App;
use crate::app::hud::render_hud;
use crate::app::minimap::prepare_minimap_image;
use crate::constants::TEXTURE_SIZE_Y;
use crate::gpu_resources::{
    self, PushConstants, get_distance_image_and_set, get_images_and_sets, save_screenshot,
};
use crate::player::HEAD_BOB_AMPLITUDE;
use crate::raycast::get_place_position;
use crate::remote_player::{GpuRemotePlayer, MAX_REMOTE_PLAYERS};
use nalgebra::Vector3;
use std::time::Instant;
use vulkano::{
    Validated, VulkanError,
    command_buffer::{
        AutoCommandBufferBuilder, BlitImageInfo, ClearColorImageInfo, CommandBufferUsage,
    },
    image::{
        sampler::Filter,
        view::{ImageView, ImageViewCreateInfo},
    },
    pipeline::{Pipeline, PipelineBindPoint},
    swapchain::{SwapchainCreateInfo, SwapchainPresentInfo, acquire_next_image},
    sync::GpuFuture,
};
use winit::event_loop::ActiveEventLoop;

/// Converts a world-space block position to texture-relative coordinates.
///
/// The GPU shader operates in texture space (origin at `tex_origin`), so all
/// world positions must be offset before being uploaded in push constants or
/// GPU buffers.
#[inline]
fn world_to_tex(world_pos: Vector3<i32>, tex_origin: Vector3<i32>) -> (i32, i32, i32) {
    (
        world_pos.x - tex_origin.x,
        world_pos.y - tex_origin.y,
        world_pos.z - tex_origin.z,
    )
}

/// Inputs to [`App::build_push_constants`]. Grouped into a struct so the
/// call site isn't a wall of 19 positional arguments — the field names
/// make it obvious which value feeds which push-constant slot.
struct PushConstantInputs {
    pixel_to_ray: nalgebra::Matrix4<f64>,
    light_count: u32,
    water_source_count: u32,
    template_block_count: u32,
    stencil_block_count: u32,
    stencil_opacity: f32,
    stencil_render_mode: u32,
    break_x: i32,
    break_y: i32,
    break_z: i32,
    preview_x: i32,
    preview_y: i32,
    preview_z: i32,
    preview_type: u32,
    target_x: i32,
    target_y: i32,
    target_z: i32,
    particle_count: u32,
    remote_player_count: u32,
    tex_origin: Vector3<i32>,
}

impl App {
    /// Main render entry point — orchestrates the full frame pipeline.
    pub(super) fn render(
        &mut self,
        _event_loop: &ActiveEventLoop,
    ) -> Result<(), crate::gpu_error::GpuError> {
        let t_render_start = Instant::now();

        self.graphics.render_pipeline.maybe_reload();
        self.graphics.resample_pipeline.maybe_reload();

        // --- Pre-pass data collection (before rcx borrow) ---
        let gpu_lights = self.collect_lights();
        let light_count = gpu_lights.len() as u32;

        let player_world_pos = self
            .sim
            .player
            .feet_pos(self.sim.world_extent, self.sim.texture_origin);
        let selected_block = self.selected_block();
        let camera_yaw = self.sim.player.camera.rotation.y as f32;
        let minimap_image =
            prepare_minimap_image(&mut self.ui, &mut self.sim, player_world_pos, camera_yaw);
        let remote_player_data: Vec<([f32; 3], u64)> = if self.is_multiplayer() {
            self.multiplayer.get_remote_player_positions()
        } else {
            Vec::new()
        };

        // --- Window size check ---
        {
            let rcx = self.graphics.rcx.as_mut().unwrap();
            if self.input.window_resized().is_some() {
                rcx.recreate_swapchain = true;
            }
            let window_size = rcx.window.inner_size();
            if window_size.width == 0 || window_size.height == 0 {
                return Ok(());
            }
        }

        // --- Swapchain management ---
        let window_size = self.graphics.rcx.as_ref().unwrap().window.inner_size();
        self.maybe_recreate_swapchain(window_size);

        let (image_index, suboptimal, acquire_future) = {
            let rcx = self.graphics.rcx.as_mut().unwrap();
            match acquire_next_image(rcx.swapchain.clone(), None).map_err(Validated::unwrap) {
                Ok(r) => r,
                Err(VulkanError::OutOfDate) => {
                    rcx.recreate_swapchain = true;
                    return Ok(());
                }
                Err(e) => panic!("failed to acquire next image: {e}"),
            }
        };

        if suboptimal {
            self.graphics.rcx.as_mut().unwrap().recreate_swapchain = true;
        }

        // Get atlas texture id before borrowing gui
        let _atlas_texture_id = self.graphics.rcx.as_ref().unwrap().atlas_texture_id;

        // Get camera data for HUD rendering
        let camera_pitch = self.sim.player.camera.rotation.x as f32;
        let camera_position = self.sim.player.camera.position;
        let camera_fov = self.sim.player.camera.fov;
        let screen_extent = self.sim.player.camera.extent;

        // --- HUD render ---
        let scale_changed_from_ui = {
            let rcx = self.graphics.rcx.as_mut().unwrap();
            render_hud(
                rcx,
                &mut self.ui,
                &mut self.sim,
                &mut self.prefs,
                selected_block,
                minimap_image,
                camera_yaw,
                camera_pitch,
                camera_position,
                camera_fov,
                screen_extent,
                player_world_pos,
                &mut self.multiplayer,
                self.args.seed.unwrap_or(314159),
                self.args.world_gen,
            )
        };
        let scale_changed_from_dynamic = self.ui.frame.pending_scale_change;
        self.ui.frame.pending_scale_change = false;

        if scale_changed_from_ui || scale_changed_from_dynamic {
            let window_extent: [u32; 2] = self
                .graphics
                .rcx
                .as_ref()
                .unwrap()
                .window
                .inner_size()
                .into();
            let render_extent = [
                (window_extent[0] as f32 * self.ui.settings.render_scale) as u32,
                (window_extent[1] as f32 * self.ui.settings.render_scale) as u32,
            ];
            let (ri, rs, resi, ress) = get_images_and_sets(
                self.graphics.memory_allocator.clone(),
                self.graphics.descriptor_set_allocator.clone(),
                &self.graphics.render_pipeline,
                &self.graphics.resample_pipeline,
                render_extent,
                window_extent,
                None, // Multiplayer texture array will be wired in Task 12
            );
            let (di, ds) = get_distance_image_and_set(
                self.graphics.memory_allocator.clone(),
                self.graphics.descriptor_set_allocator.clone(),
                &self.graphics.render_pipeline,
                render_extent,
            );
            let rcx = self.graphics.rcx.as_mut().unwrap();
            rcx.render_image = ri;
            rcx.render_set = rs;
            rcx.resample_image = resi;
            rcx.resample_set = ress;
            rcx.distance_image = di;
            rcx.distance_set = ds;
        }

        // --- Console command processing ---
        self.process_console_commands();

        // --- Multiplayer stencil sync ---
        self.process_multiplayer_stencils();

        let tex_origin = self.sim.texture_origin;
        let render_extent = self.graphics.rcx.as_ref().unwrap().render_image.extent();
        let resample_extent = self.graphics.rcx.as_ref().unwrap().resample_image.extent();
        self.sim.player.camera.extent = [render_extent[0] as f64, render_extent[1] as f64];

        // --- GPU buffer updates ---
        let water_source_count = self.collect_water_sources();
        let template_block_count = self.populate_template_buffer(tex_origin);
        let (stencil_block_count, stencil_opacity, stencil_render_mode) =
            self.populate_stencil_buffer(tex_origin);
        let (break_x, break_y, break_z) = self
            .ui
            .placement
            .breaking_block
            .map(|p| world_to_tex(p, tex_origin))
            .unwrap_or((-1, -1, -1));
        let (preview_x, preview_y, preview_z, preview_type) =
            self.compute_preview_block(tex_origin, player_world_pos);
        let (target_x, target_y, target_z) = self.compute_target_block(tex_origin);
        let particle_count = self.update_particle_buffer(tex_origin);
        self.update_falling_block_buffer(tex_origin);
        self.update_light_buffer(&gpu_lights);
        let remote_player_count = self.update_remote_player_buffer(&remote_player_data, tex_origin);

        // --- Push constants ---
        let pixel_to_ray = self.build_pixel_to_ray();
        let push_constants = self.build_push_constants(PushConstantInputs {
            pixel_to_ray,
            light_count,
            water_source_count,
            template_block_count,
            stencil_block_count,
            stencil_opacity,
            stencil_render_mode,
            break_x,
            break_y,
            break_z,
            preview_x,
            preview_y,
            preview_z,
            preview_type,
            target_x,
            target_y,
            target_z,
            particle_count,
            remote_player_count,
            tex_origin,
        });

        // --- Picture uploads ---
        self.upload_pending_pictures();

        // Reclaim staging buffers of atlas uploads whose GPU fences have
        // signaled. Cheap when idle; keeps memory bounded when many uploads
        // were submitted in a short window (e.g. post-world-load picture
        // batch).
        crate::gpu_resources::poll_atlas_upload_ring();

        // --- Command buffer recording and frame submission ---
        self.record_and_submit_frame(
            image_index,
            acquire_future,
            push_constants,
            render_extent,
            resample_extent,
        );

        // --- Screenshot ---
        self.handle_screenshot(image_index);

        // --- Frame stats ---
        self.sim.profiler.render_us += t_render_start.elapsed().as_micros() as u64;
        self.sim.profiler.sample_count += 1;

        Ok(())
    }

    /// Collects active point lights from the world for GPU upload.
    ///
    /// Gathers torch lights near the player, prioritised by camera direction, and
    /// returns the collected GpuLight vec ready for buffer upload.
    fn collect_lights(&self) -> Vec<gpu_resources::GpuLight> {
        let player_world_pos = self
            .sim
            .player
            .camera_world_pos(self.sim.world_extent, self.sim.texture_origin);
        let camera_dir = self.sim.player.camera_direction();
        let camera_dir_f32 = Vector3::new(
            camera_dir.x as f32,
            camera_dir.y as f32,
            camera_dir.z as f32,
        );
        self.sim.world.collect_torch_lights(
            self.sim.player.light_enabled,
            player_world_pos,
            camera_dir_f32,
            self.sim.texture_origin,
            &self.sim.model_registry,
            self.sim.world_extent,
            self.sim.animation_time,
            self.ui.settings.light_cull_radius,
            self.ui.settings.max_active_lights as usize,
        )
    }

    /// Collects water and lava source positions for debug visualisation and uploads
    /// them to the GPU water-source buffer.
    ///
    /// Returns the number of sources uploaded (0 when the debug overlay is disabled).
    fn collect_water_sources(&mut self) -> u32 {
        if !self.ui.settings.show_water_sources {
            return 0;
        }

        let tex_origin = self.sim.texture_origin;
        let mut sources = Vec::with_capacity(gpu_resources::MAX_WATER_SOURCES);

        for (pos, cell) in self.sim.water_grid.iter() {
            if cell.is_source && sources.len() < gpu_resources::MAX_WATER_SOURCES {
                sources.push(gpu_resources::GpuWaterSource {
                    position: [
                        (pos.x - tex_origin.x) as f32,
                        (pos.y - tex_origin.y) as f32,
                        (pos.z - tex_origin.z) as f32,
                        0.0, // 0 = water
                    ],
                });
            }
        }

        for (pos, cell) in self.sim.lava_grid.iter() {
            if cell.is_source && sources.len() < gpu_resources::MAX_WATER_SOURCES {
                sources.push(gpu_resources::GpuWaterSource {
                    position: [
                        (pos.x - tex_origin.x) as f32,
                        (pos.y - tex_origin.y) as f32,
                        (pos.z - tex_origin.z) as f32,
                        1.0, // 1 = lava
                    ],
                });
            }
        }

        let count = sources.len().min(gpu_resources::MAX_WATER_SOURCES);
        {
            let mut write = self.graphics.water_source_buffer.write().unwrap();
            for (i, src) in sources.iter().take(count).enumerate() {
                write[i] = *src;
            }
        }
        count as u32
    }

    /// Recreates the swapchain and render images if the window was resized or the
    /// swapchain was marked out-of-date.
    fn maybe_recreate_swapchain(&mut self, window_size: winit::dpi::PhysicalSize<u32>) {
        {
            let rcx = self.graphics.rcx.as_ref().unwrap();
            if !rcx.recreate_swapchain {
                return;
            }
        }

        let (new_swapchain, images) = {
            let rcx = self.graphics.rcx.as_ref().unwrap();
            rcx.swapchain
                .recreate(SwapchainCreateInfo {
                    image_extent: window_size.into(),
                    ..rcx.swapchain.create_info()
                })
                .unwrap()
        };

        let window_extent: [u32; 2] = window_size.into();
        let render_extent = [
            (window_extent[0] as f32 * self.ui.settings.render_scale) as u32,
            (window_extent[1] as f32 * self.ui.settings.render_scale) as u32,
        ];

        let new_image_views: Vec<_> = images
            .iter()
            .map(|i| ImageView::new(i.clone(), ImageViewCreateInfo::from_image(i)).unwrap())
            .collect();

        let (ri, rs, resi, ress) = get_images_and_sets(
            self.graphics.memory_allocator.clone(),
            self.graphics.descriptor_set_allocator.clone(),
            &self.graphics.render_pipeline,
            &self.graphics.resample_pipeline,
            render_extent,
            window_extent,
            None, // Multiplayer texture array will be wired in Task 12
        );
        let (di, ds) = get_distance_image_and_set(
            self.graphics.memory_allocator.clone(),
            self.graphics.descriptor_set_allocator.clone(),
            &self.graphics.render_pipeline,
            render_extent,
        );

        let rcx = self.graphics.rcx.as_mut().unwrap();
        rcx.swapchain = new_swapchain;
        rcx.image_views = new_image_views;
        rcx.render_image = ri;
        rcx.render_set = rs;
        rcx.resample_image = resi;
        rcx.resample_set = ress;
        rcx.distance_image = di;
        rcx.distance_set = ds;
        rcx.recreate_swapchain = false;
        self.ui.frame.window_size = window_extent;
    }

    /// Processes all pending console commands that were queued during the previous
    /// simulation tick (teleports, saved positions, water debug, stencil ops, etc.).
    fn process_console_commands(&mut self) {
        // Handle pending teleport from console
        if let Some(tp) = self.ui.console.pending_teleport.take() {
            let feet_pos = Vector3::new(tp.x, tp.y, tp.z);
            self.sim
                .player
                .set_feet_pos(feet_pos, self.sim.world_extent, self.sim.texture_origin);
            // Reset velocity to prevent continued movement
            self.sim.player.velocity = Vector3::zeros();
        }

        // Handle pending spawn position from console
        if let Some(pos) = self.ui.console.pending_set_spawn_position.take() {
            let spawn_pos = Vector3::new(pos[0], pos[1], pos[2]);
            self.sim.player.set_spawn_pos(spawn_pos);
            // Broadcast to all clients in multiplayer
            self.multiplayer.broadcast_spawn_position([
                pos[0] as f32,
                pos[1] as f32,
                pos[2] as f32,
            ]);
        }

        // Handle pending biome debug toggle
        if let Some(enabled) = self.ui.console.pending_biome_debug.take() {
            self.ui.settings.show_biome_debug = enabled;
        }

        // Handle pending water profiling toggle
        if let Some(enabled) = self.ui.console.pending_water_profile.take() {
            self.sim.water_grid.set_profiling(enabled);
        }

        // Handle pending force water active from console
        if self.ui.console.pending_force_water_active {
            let count = self.sim.water_grid.force_all_active();
            self.ui
                .console
                .success(format!("Forced {} water cells active", count));
            self.ui.console.pending_force_water_active = false;
        }

        // Handle pending water analyze from console
        if self.ui.console.pending_water_analyze {
            let player_block = self
                .sim
                .player
                .feet_pos(self.sim.world_extent, self.sim.texture_origin)
                .map(|c| c.floor() as i32);
            let world = &self.sim.world;
            let is_solid =
                |pos: Vector3<i32>| world.get_block(pos).map(|b| b.is_solid()).unwrap_or(true);
            let is_out_of_bounds = |pos: Vector3<i32>| world.get_block(pos).is_none();
            let has_world_water = |pos: Vector3<i32>| {
                world
                    .get_block(pos)
                    .map(|b| matches!(b, crate::chunk::BlockType::Water))
                    .unwrap_or(false)
            };

            let analysis = self.sim.water_grid.debug_flow_analysis(
                player_block,
                is_solid,
                is_out_of_bounds,
                &has_world_water,
            );
            for line in analysis.lines() {
                self.ui.console.info(line);
            }
            self.ui.console.pending_water_analyze = false;
        }

        // Handle pending save position from console
        if let Some(name) = self.ui.console.pending_save_position.take() {
            let player_pos = self
                .sim
                .player
                .feet_pos(self.sim.world_extent, self.sim.texture_origin);
            let position = [player_pos.x, player_pos.y, player_pos.z];
            self.prefs
                .save_position(&self.sim.world_name, &name, position);
            self.prefs.save();
            self.ui.console.success(format!(
                "Saved position '{}' at ({:.1}, {:.1}, {:.1})",
                name, player_pos.x, player_pos.y, player_pos.z
            ));
        }

        // Handle pending delete position from console
        if let Some(name) = self.ui.console.pending_delete_position.take() {
            if self.prefs.delete_position(&self.sim.world_name, &name) {
                self.prefs.save();
                self.ui
                    .console
                    .success(format!("Deleted position '{}'", name));
            } else {
                self.ui
                    .console
                    .error(format!("Position '{}' not found", name));
            }
        }

        // Handle pending list positions from console
        if self.ui.console.pending_list_positions {
            let names = self.prefs.get_position_names(&self.sim.world_name);
            if names.is_empty() {
                self.ui.console.info("No saved positions for this world.");
            } else {
                self.ui.console.info("Saved positions:");
                for name in &names {
                    if let Some(pos) = self.prefs.get_position(&self.sim.world_name, name) {
                        self.ui.console.info(format!(
                            "  {} - ({:.1}, {:.1}, {:.1})",
                            name, pos[0], pos[1], pos[2]
                        ));
                    }
                }
                self.ui
                    .console
                    .info("Use 'tp <x> <y> <z>' to teleport to a saved position.");
            }
            self.ui.console.pending_list_positions = false;
        }

        // Handle pending set picture selection from console
        if let Some(id) = self.ui.console.pending_set_picture.take() {
            self.ui.picture_state.selected_picture_id = if id == 0 { None } else { Some(id) };
            self.prefs.selected_picture_id = self.ui.picture_state.selected_picture_id;
            self.prefs.save();
        }

        // Handle pending stencil clear from console
        if self.ui.console.pending_stencil_clear {
            let stencil_ids: Vec<u64> = self
                .ui
                .stencil_manager
                .active_stencils
                .iter()
                .map(|s| s.id)
                .collect();
            self.ui.stencil_manager.clear();
            self.ui.console.pending_stencil_clear = false;
            // Broadcast removal of all stencils if hosting
            for id in stencil_ids {
                self.multiplayer.broadcast_stencil_removed(id);
            }
        }

        // Handle pending stencil removal from console
        if let Some(id) = self.ui.console.pending_stencil_remove.take() {
            self.ui.stencil_manager.remove_stencil(id);
            // Broadcast removal to all clients if hosting
            self.multiplayer.broadcast_stencil_removed(id);
        }
    }

    /// Applies pending stencil events received from the multiplayer server
    /// (loads, transform updates, and removals).
    fn process_multiplayer_stencils(&mut self) {
        // Process pending stencil loads from multiplayer (received StencilLoaded messages)
        for stencil_loaded in self.multiplayer.take_pending_stencil_loads() {
            // Deserialize the stencil data
            match crate::stencils::StencilFile::from_bytes(&stencil_loaded.stencil_data) {
                Ok(stencil) => {
                    // Create a PlacedStencil with the server's stencil_id
                    let mut placed = crate::stencils::PlacedStencil::new(
                        stencil_loaded.stencil_id,
                        stencil,
                        nalgebra::Vector3::new(0, 0, 0), // Initial position - will be updated via StencilTransformUpdate
                    );
                    // Apply default color/opacity
                    placed.color = self.ui.stencil_manager.default_color;
                    placed.opacity = self.ui.stencil_manager.global_opacity;
                    // Add to manager (this preserves the server's ID)
                    self.ui.stencil_manager.add_placed_stencil(placed);
                    log::debug!(
                        "[Multiplayer] Applied StencilLoaded: id={} name='{}'",
                        stencil_loaded.stencil_id,
                        stencil_loaded.name
                    );
                }
                Err(e) => {
                    log::warn!(
                        "[Multiplayer] Failed to deserialize stencil '{}': {}",
                        stencil_loaded.name,
                        e
                    );
                }
            }
        }

        // Process pending stencil transform updates from multiplayer
        for transform in self.multiplayer.take_pending_stencil_transforms() {
            // Find the stencil by ID and update its position and rotation
            // Note: Stencil IDs from server may not match local IDs, so we match by index
            // For now, we use the stencil_id directly as stored in the PlacedStencil.id field
            if let Some(stencil) = self
                .ui
                .stencil_manager
                .get_stencil_mut(transform.stencil_id)
            {
                stencil.set_origin(nalgebra::Vector3::new(
                    transform.position[0],
                    transform.position[1],
                    transform.position[2],
                ));
                stencil.set_rotation(transform.rotation);
                log::debug!(
                    "[Multiplayer] Applied StencilTransformUpdate: id={} pos={:?} rot={}",
                    transform.stencil_id,
                    transform.position,
                    transform.rotation
                );
            } else {
                // Stencil not found locally - might need to request it from server
                log::warn!(
                    "[Multiplayer] Stencil {} not found for transform update",
                    transform.stencil_id
                );
            }
        }

        // Process pending stencil removals from multiplayer
        for removed in self.multiplayer.take_pending_stencil_removals() {
            if self
                .ui
                .stencil_manager
                .remove_stencil(removed.stencil_id)
                .is_some()
            {
                log::debug!(
                    "[Multiplayer] Applied StencilRemoved: id={}",
                    removed.stencil_id
                );
            } else {
                log::warn!(
                    "[Multiplayer] Stencil {} not found for removal",
                    removed.stencil_id
                );
            }
        }
    }

    /// Uploads active template block positions to the GPU template buffer.
    ///
    /// Returns the number of blocks written (0 when no template placement is active).
    fn populate_template_buffer(&mut self, tex_origin: Vector3<i32>) -> u32 {
        if let Some(ref placement) = self.ui.active_placement {
            let block_positions = placement.get_preview_blocks(gpu_resources::MAX_TEMPLATE_BLOCKS);
            let count = block_positions.len();
            let mut write = self.graphics.template_block_buffer.write().unwrap();
            for (i, pos) in block_positions.iter().enumerate() {
                let tex_pos = world_to_tex(*pos, tex_origin);
                write[i] = gpu_resources::GpuTemplateBlock {
                    position: [tex_pos.0 as f32, tex_pos.1 as f32, tex_pos.2 as f32, 0.0],
                };
            }
            count as u32
        } else {
            0
        }
    }

    /// Writes all active tool-preview and stencil block positions to the GPU stencil buffer.
    ///
    /// Returns `(block_count, global_opacity, render_mode)`.
    fn populate_stencil_buffer(&mut self, tex_origin: Vector3<i32>) -> (u32, f32, u32) {
        let mut total_blocks = 0usize;
        let mut write = self.graphics.stencil_block_buffer.write().unwrap();

        /// Append one stencil block entry to the buffer if capacity permits.
        macro_rules! push_block {
            ($world_pos:expr, $color_id:expr) => {
                if total_blocks < gpu_resources::MAX_STENCIL_BLOCKS {
                    let tp = world_to_tex($world_pos, tex_origin);
                    write[total_blocks] = gpu_resources::GpuStencilBlock {
                        position: [tp.0 as f32, tp.1 as f32, tp.2 as f32, $color_id as f32],
                    };
                    total_blocks += 1;
                }
            };
        }

        // Add blocks from active stencils
        for stencil in &self.ui.stencil_manager.active_stencils {
            let color_id = stencil.id as u32 % 8; // Cycle through 8 colors
            for world_pos in stencil.iter_positions() {
                if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                    break;
                }
                push_block!(world_pos, color_id);
            }
        }

        // Add blocks from stencil placement preview
        if let Some(ref placement) = self.ui.active_stencil_placement {
            let preview_color_id = 7u32; // Use distinct color for preview
            for world_pos in placement.get_preview_positions(gpu_resources::MAX_STENCIL_BLOCKS) {
                push_block!(world_pos, preview_color_id);
            }
        }

        // Add blocks from sphere tool preview (cyan)
        if self.ui.sphere_tool.active {
            for &world_pos in &self.ui.sphere_tool.preview_positions {
                push_block!(world_pos, 0u32);
            }
        }

        // Add blocks from cube tool preview (cyan)
        if self.ui.cube_tool.active {
            for &world_pos in &self.ui.cube_tool.preview_positions {
                push_block!(world_pos, 0u32);
            }
        }

        // Add blocks from bridge tool: start position marker (magenta), preview line (cyan)
        if self.ui.bridge_tool.active {
            if let Some(start) = self.ui.bridge_tool.start_position {
                push_block!(start, 2u32); // Magenta for start marker
            }
            for &world_pos in &self.ui.bridge_tool.preview_positions {
                push_block!(world_pos, 0u32); // Cyan for bridge preview
            }
        }

        // Add blocks from cylinder tool preview (cyan)
        if self.ui.cylinder_tool.active {
            for &world_pos in &self.ui.cylinder_tool.preview_positions {
                push_block!(world_pos, 0u32);
            }
        }

        // Add blocks from wall tool preview (cyan)
        if self.ui.wall_tool.active {
            for &world_pos in &self.ui.wall_tool.preview_positions {
                push_block!(world_pos, 0u32);
            }
        }

        // Add blocks from floor tool preview (cyan)
        if self.ui.floor_tool.active {
            for &world_pos in &self.ui.floor_tool.preview_positions {
                push_block!(world_pos, 0u32);
            }
        }

        // Add blocks from replace tool preview (yellow — distinct from placement)
        if self.ui.replace_tool.active {
            for &world_pos in &self.ui.replace_tool.preview_positions {
                push_block!(world_pos, 3u32);
            }
        }

        // Add blocks from circle tool preview (cyan)
        if self.ui.circle_tool.active {
            for &world_pos in &self.ui.circle_tool.preview_positions {
                push_block!(world_pos, 0u32);
            }
        }

        // Add blocks from stairs tool: start marker (magenta if no preview yet), preview (green)
        if self.ui.stairs_tool.active {
            if let Some(start) = self.ui.stairs_tool.start_pos
                && self.ui.stairs_tool.preview_positions.is_empty()
            {
                push_block!(start, 2u32); // Magenta for start marker
            }
            for &world_pos in &self.ui.stairs_tool.preview_positions {
                push_block!(world_pos, 1u32); // Green for stairs preview
            }
        }

        // Add blocks from arch tool preview (cyan)
        if self.ui.arch_tool.active {
            for &world_pos in &self.ui.arch_tool.preview_positions {
                push_block!(world_pos, 0u32);
            }
        }

        // Add blocks from cone tool preview (cyan)
        if self.ui.cone_tool.active {
            for &world_pos in &self.ui.cone_tool.preview_positions {
                push_block!(world_pos, 0u32);
            }
        }

        // Add blocks from torus tool preview (cyan)
        if self.ui.torus_tool.active {
            for &world_pos in &self.ui.torus_tool.preview_positions {
                push_block!(world_pos, 0u32);
            }
        }

        // Add blocks from helix tool preview (cyan)
        if self.ui.helix_tool.active {
            for &world_pos in &self.ui.helix_tool.preview_positions {
                push_block!(world_pos, 0u32);
            }
        }

        // Add blocks from polygon tool preview (cyan)
        if self.ui.polygon_tool.active {
            for &world_pos in &self.ui.polygon_tool.preview_positions {
                push_block!(world_pos, 0u32);
            }
        }

        // Add blocks from bezier tool: curve preview (cyan), control point markers (magenta)
        if self.ui.bezier_tool.active {
            for &world_pos in &self.ui.bezier_tool.preview_positions {
                push_block!(world_pos, 0u32); // Cyan for the curve
            }
            for &world_pos in &self.ui.bezier_tool.control_point_markers {
                push_block!(world_pos, 3u32); // Magenta for control point markers
            }
        }

        // Add blocks from clone tool preview (green — distinguishes from original)
        if self.ui.clone_tool.active {
            for &world_pos in &self.ui.clone_tool.preview_positions {
                push_block!(world_pos, 1u32);
            }
        }

        // Add blocks from pattern fill tool preview (Block A = cyan, Block B = green)
        if self.ui.pattern_fill.active {
            for &world_pos in &self.ui.pattern_fill.preview_a {
                push_block!(world_pos, 0u32);
            }
            for &world_pos in &self.ui.pattern_fill.preview_b {
                push_block!(world_pos, 1u32);
            }
        }

        // Add blocks from hollow tool preview (interior blocks to remove — red/orange)
        if self.ui.hollow_tool.active {
            for &world_pos in &self.ui.hollow_tool.preview_positions {
                push_block!(world_pos, 3u32);
            }
        }

        // Add terrain brush preview footprint (cyan)
        if self.ui.terrain_brush.active {
            for &world_pos in &self.ui.terrain_brush.preview_positions {
                push_block!(world_pos, 0u32);
            }
        }

        // Add mirror plane visualisation (magenta)
        if self.ui.mirror_tool.active
            && self.ui.mirror_tool.plane_set
            && self.ui.mirror_tool.show_plane
        {
            for world_pos in self.collect_mirror_plane_positions() {
                push_block!(world_pos, 2u32); // Magenta for mirror plane
            }
        }

        (
            total_blocks as u32,
            self.ui.stencil_manager.global_opacity,
            self.ui.stencil_manager.render_mode.as_i32() as u32,
        )
    }

    /// Returns the world-space positions of all blocks in the mirror-plane visualisation.
    ///
    /// The caller is responsible for assigning colour IDs and writing to the GPU buffer.
    fn collect_mirror_plane_positions(&self) -> Vec<Vector3<i32>> {
        use crate::shape_tools::mirror::MirrorAxis;

        let plane_pos = self.ui.mirror_tool.plane_position;
        let plane_size = 16i32; // Size of plane visualization
        // Each plane has (2*plane_size+1) * (plane_size+1) positions
        let capacity = ((2 * plane_size + 1) * (plane_size + 1)) as usize;
        let mut positions = Vec::with_capacity(capacity);

        // Generate plane grid based on axis
        match self.ui.mirror_tool.axis {
            MirrorAxis::X => {
                // Plane perpendicular to Z at z = plane_z
                for dx in -plane_size..=plane_size {
                    for dy in 0..=plane_size {
                        positions.push(Vector3::new(
                            plane_pos.x + dx,
                            plane_pos.y + dy,
                            plane_pos.z,
                        ));
                    }
                }
            }
            MirrorAxis::Z => {
                // Plane perpendicular to X at x = plane_x
                for dz in -plane_size..=plane_size {
                    for dy in 0..=plane_size {
                        positions.push(Vector3::new(
                            plane_pos.x,
                            plane_pos.y + dy,
                            plane_pos.z + dz,
                        ));
                    }
                }
            }
            MirrorAxis::Both => {
                // Show both planes (cross pattern)
                // X-axis plane (perpendicular to Z)
                for dx in -plane_size..=plane_size {
                    for dy in 0..=plane_size {
                        positions.push(Vector3::new(
                            plane_pos.x + dx,
                            plane_pos.y + dy,
                            plane_pos.z,
                        ));
                    }
                }
                // Z-axis plane (perpendicular to X)
                for dz in -plane_size..=plane_size {
                    // Skip center column (already rendered by X plane)
                    if dz == 0 {
                        continue;
                    }
                    for dy in 0..=plane_size {
                        positions.push(Vector3::new(
                            plane_pos.x,
                            plane_pos.y + dy,
                            plane_pos.z + dz,
                        ));
                    }
                }
            }
        }

        positions
    }

    /// Computes the pixel-to-ray matrix for this frame, scaled to texture space
    /// and with head-bob applied.
    fn build_pixel_to_ray(&self) -> nalgebra::Matrix4<f64> {
        let pixel_to_ray = self.sim.player.camera.pixel_to_ray_matrix();

        // Scale only the position (column 4), not the direction (3x3 rotation part)
        // This prevents ray distortion from non-uniform world dimensions
        let mut pixel_to_ray_scaled = pixel_to_ray;
        // Camera position is normalized (0-1), scale to texture size
        // Ray marching happens in texture space (0 to textureSize)
        pixel_to_ray_scaled.m14 *= self.sim.world_extent[0] as f64;
        pixel_to_ray_scaled.m24 *= self.sim.world_extent[1] as f64;
        pixel_to_ray_scaled.m34 *= self.sim.world_extent[2] as f64;

        // Apply head bob offset to camera Y position for rendering
        let head_bob_offset = (self.sim.player.head_bob_timer * std::f64::consts::TAU).sin()
            * HEAD_BOB_AMPLITUDE
            * self.sim.player.head_bob_intensity;
        pixel_to_ray_scaled.m24 += head_bob_offset;

        pixel_to_ray_scaled
    }

    /// Computes the block-preview position and type for the current frame.
    ///
    /// Returns `(x, y, z, block_type)` in texture coordinates; all `-1`/`0` when
    /// the preview is disabled or out-of-bounds.
    fn compute_preview_block(
        &self,
        tex_origin: Vector3<i32>,
        player_world_pos: Vector3<f64>,
    ) -> (i32, i32, i32, u32) {
        // Use player_world_pos computed earlier (before rcx borrow)
        if !self.ui.settings.show_block_preview {
            return (-1, -1, -1, 0); // Preview disabled
        }

        let selected_block_id = self.selected_block() as u32;
        self.ui
            .placement
            .current_hit
            .as_ref()
            .map(|hit| {
                let place_pos = get_place_position(hit);
                let block_center = place_pos.cast::<f64>() + Vector3::new(0.5, 0.5, 0.5);
                // Y bounds only (X/Z are infinite)
                let in_bounds = place_pos.y >= 0 && place_pos.y < TEXTURE_SIZE_Y as i32;
                let not_in_player = (player_world_pos - block_center).norm() > 1.5;
                if in_bounds && not_in_player {
                    let tex_pos = world_to_tex(place_pos, tex_origin);
                    (tex_pos.0, tex_pos.1, tex_pos.2, selected_block_id)
                } else {
                    (-1, -1, -1, 0)
                }
            })
            .unwrap_or((-1, -1, -1, 0))
    }

    /// Returns the target (looked-at) block position in texture coordinates, or
    /// `(-1,-1,-1)` when the outline overlay is disabled.
    fn compute_target_block(&self, tex_origin: Vector3<i32>) -> (i32, i32, i32) {
        // Only send if outline is enabled
        if !self.ui.settings.show_target_outline {
            return (-1, -1, -1); // Outline disabled
        }
        self.ui
            .placement
            .current_hit
            .as_ref()
            .map(|hit| world_to_tex(hit.block_pos, tex_origin))
            .unwrap_or((-1, -1, -1))
    }

    /// Uploads particle positions (converted to texture space) to the GPU buffer.
    ///
    /// Returns the number of active particles.
    fn update_particle_buffer(&mut self, tex_origin: Vector3<i32>) -> u32 {
        // Update particle buffer (convert world coords to texture coords)
        let gpu_particles = self.sim.particles.gpu_data();
        let particle_count = gpu_particles.len() as u32;
        {
            let mut write = self.graphics.particle_buffer.write().unwrap();
            for (i, p) in gpu_particles.iter().enumerate() {
                let mut converted = *p;
                // Convert world position to texture position
                converted.pos_size[0] -= tex_origin.x as f32;
                converted.pos_size[1] -= tex_origin.y as f32;
                converted.pos_size[2] -= tex_origin.z as f32;
                write[i] = converted;
            }
        }
        particle_count
    }

    /// Uploads falling-block positions (converted to texture space) to the GPU buffer.
    fn update_falling_block_buffer(&mut self, tex_origin: Vector3<i32>) {
        // Update falling block buffer (convert world coords to texture coords)
        let gpu_falling_blocks = self.sim.falling_blocks.gpu_data();
        {
            let mut write = self.graphics.falling_block_buffer.write().unwrap();
            for (i, fb) in gpu_falling_blocks.iter().enumerate() {
                let mut converted = *fb;
                // Convert world position to texture position
                converted.pos_type[0] -= tex_origin.x as f32;
                converted.pos_type[1] -= tex_origin.y as f32;
                converted.pos_type[2] -= tex_origin.z as f32;
                write[i] = converted;
            }
        }
    }

    /// Copies the pre-collected GPU light slice into the light GPU buffer.
    fn update_light_buffer(&mut self, gpu_lights: &[gpu_resources::GpuLight]) {
        // Update light buffer with torch positions (collected earlier)
        let mut write = self.graphics.light_buffer.write().unwrap();
        for (i, l) in gpu_lights.iter().enumerate() {
            write[i] = *l;
        }
    }

    /// Uploads remote-player positions (converted to texture space) to the GPU buffer.
    ///
    /// Returns the number of remote players written.
    fn update_remote_player_buffer(
        &mut self,
        remote_player_data: &[([f32; 3], u64)],
        tex_origin: Vector3<i32>,
    ) -> u32 {
        // Update remote player buffer for multiplayer
        if remote_player_data.is_empty() {
            return 0;
        }
        let mut write = self.graphics.remote_player_buffer.write().unwrap();
        for (i, (pos, player_id)) in remote_player_data.iter().enumerate() {
            if i >= MAX_REMOTE_PLAYERS {
                break;
            }
            // Assign color based on player_id (same logic as minimap):
            // Host (player_id 0) always gets red (index 0)
            // Other players get colors 1-7 based on their player_id hash
            let color_index = if *player_id == 0 {
                0 // Host is always red
            } else {
                ((player_id.wrapping_mul(0x5851F42E4C957F2D) % 7) + 1) as u32
            };

            // Convert world position to texture position
            let gpu_player = GpuRemotePlayer::new(
                [
                    pos[0] - tex_origin.x as f32,
                    pos[1] - tex_origin.y as f32,
                    pos[2] - tex_origin.z as f32,
                ],
                color_index,
                1.8, // 2-block tall player
            );
            write[i] = gpu_player;
        }
        remote_player_data.len().min(MAX_REMOTE_PLAYERS) as u32
    }

    /// Assembles the `PushConstants` struct that is pushed to every compute shader
    /// invocation for this frame.
    ///
    /// All positional inputs are already in texture-relative coordinates.
    #[allow(clippy::too_many_arguments)]
    fn build_push_constants(&self, inputs: PushConstantInputs) -> PushConstants {
        let PushConstantInputs {
            pixel_to_ray,
            light_count,
            water_source_count,
            template_block_count,
            stencil_block_count,
            stencil_opacity,
            stencil_render_mode,
            break_x,
            break_y,
            break_z,
            preview_x,
            preview_y,
            preview_z,
            preview_type,
            target_x,
            target_y,
            target_z,
            particle_count,
            remote_player_count,
            tex_origin,
        } = inputs;
        let world_to_tex_fn = |p: Vector3<i32>| world_to_tex(p, tex_origin);

        // Compute template bounding box in texture coordinates
        let (tmpl_min_x, tmpl_min_y, tmpl_min_z, tmpl_max_x, tmpl_max_y, tmpl_max_z) =
            if let Some(ref placement) = self.ui.active_placement {
                let (min, max) = placement.get_bounding_box();
                let tmin = world_to_tex_fn(min);
                let tmax = world_to_tex_fn(max);
                (tmin.0, tmin.1, tmin.2, tmax.0, tmax.1, tmax.2)
            } else {
                (-1, -1, -1, -1, -1, -1)
            };

        // Compute cutaway chunk coordinates
        let (
            cutaway_chunk_x,
            cutaway_chunk_y,
            cutaway_chunk_z,
            cutaway_player_chunk_x,
            cutaway_player_chunk_z,
        ) = self.compute_cutaway_coords(tex_origin);

        PushConstants {
            pixel_to_ray: pixel_to_ray.cast(),
            texture_size_x: self.sim.world_extent[0],
            texture_size_y: self.sim.world_extent[1],
            texture_size_z: self.sim.world_extent[2],
            render_mode: self.sim.render_mode as u32,
            show_chunk_boundaries: self.ui.settings.show_chunk_boundaries as u32,
            player_in_water: self.sim.player.in_water as u32,
            time_of_day: self.sim.time_of_day,
            animation_time: self.sim.animation_time,
            cloud_speed: self.sim.atmosphere.cloud_speed,
            cloud_coverage: self.sim.atmosphere.cloud_coverage,
            cloud_color_r: self.sim.atmosphere.cloud_color[0],
            cloud_color_g: self.sim.atmosphere.cloud_color[1],
            cloud_color_b: self.sim.atmosphere.cloud_color[2],
            clouds_enabled: self.sim.atmosphere.clouds_enabled as u32,
            break_block_x: break_x,
            break_block_y: break_y,
            break_block_z: break_z,
            break_progress: self.ui.placement.break_progress,
            particle_count,
            preview_block_x: preview_x,
            preview_block_y: preview_y,
            preview_block_z: preview_z,
            preview_block_type: preview_type,
            light_count,
            ambient_light: self.sim.atmosphere.ambient_light,
            fog_density: self.sim.atmosphere.fog_density,
            fog_start: self.sim.atmosphere.fog_start,
            fog_overlay_scale: self.sim.atmosphere.fog_overlay_scale,
            target_block_x: target_x,
            target_block_y: target_y,
            target_block_z: target_z,
            max_ray_steps: self.ui.settings.max_ray_steps,
            shadow_max_steps: self.ui.settings.shadow_max_steps,
            texture_origin_x: tex_origin.x,
            texture_origin_y: tex_origin.y,
            texture_origin_z: tex_origin.z,
            enable_ao: if self.ui.settings.enable_ao { 1 } else { 0 },
            enable_shadows: if self.ui.settings.enable_shadows {
                1
            } else {
                0
            },
            enable_model_shadows: if self.ui.settings.enable_model_shadows {
                1
            } else {
                0
            },
            enable_point_lights: if self.ui.settings.enable_point_lights {
                1
            } else {
                0
            },
            enable_tinted_shadows: if self.ui.settings.enable_tinted_shadows {
                1
            } else {
                0
            },
            transparent_background: 0,
            pass_mode: 0, // Will be set per-pass
            lod_ao_distance: self.ui.settings.lod_ao_distance,
            lod_shadow_distance: self.ui.settings.lod_shadow_distance,
            lod_point_light_distance: self.ui.settings.lod_point_light_distance,
            lod_model_distance: self.ui.settings.lod_model_distance,
            falling_block_count: self.sim.falling_blocks.count() as u32,
            show_water_sources: self.ui.settings.show_water_sources as u32,
            water_source_count,
            template_block_count,
            template_preview_min_x: tmpl_min_x,
            template_preview_min_y: tmpl_min_y,
            template_preview_min_z: tmpl_min_z,
            template_preview_max_x: tmpl_max_x,
            template_preview_max_y: tmpl_max_y,
            template_preview_max_z: tmpl_max_z,
            _padding: [0; 12],
            camera_pos: {
                let cam = self
                    .sim
                    .player
                    .camera_world_pos(self.sim.world_extent, self.sim.texture_origin);
                [cam.x as f32, cam.y as f32, cam.z as f32, 0.0]
            },
            selection_pos1_x: self
                .ui
                .template_selection
                .pos1
                .map(|p| world_to_tex_fn(p).0)
                .unwrap_or(-1),
            selection_pos1_y: self
                .ui
                .template_selection
                .pos1
                .map(|p| world_to_tex_fn(p).1)
                .unwrap_or(-1),
            selection_pos1_z: self
                .ui
                .template_selection
                .pos1
                .map(|p| world_to_tex_fn(p).2)
                .unwrap_or(-1),
            selection_pos2_x: self
                .ui
                .template_selection
                .pos2
                .map(|p| world_to_tex_fn(p).0)
                .unwrap_or(-1),
            selection_pos2_y: self
                .ui
                .template_selection
                .pos2
                .map(|p| world_to_tex_fn(p).1)
                .unwrap_or(-1),
            selection_pos2_z: self
                .ui
                .template_selection
                .pos2
                .map(|p| world_to_tex_fn(p).2)
                .unwrap_or(-1),
            hide_ground_cover: if self.ui.settings.hide_ground_cover {
                1
            } else {
                0
            },
            cutaway_enabled: if self.ui.settings.debug_cutaway_enabled {
                1
            } else {
                0
            },
            cutaway_chunk_x,
            cutaway_chunk_y,
            cutaway_chunk_z,
            cutaway_player_chunk_x,
            cutaway_player_chunk_z,
            // Measurement markers
            measurement_marker_count: self.ui.placement.measurement_markers.len().min(4) as u32,
            measurement_marker_0_x: self
                .ui
                .placement
                .measurement_markers
                .first()
                .map(|p| p.x - tex_origin.x)
                .unwrap_or(-10000),
            measurement_marker_0_y: self
                .ui
                .placement
                .measurement_markers
                .first()
                .map(|p| p.y - tex_origin.y)
                .unwrap_or(-10000),
            measurement_marker_0_z: self
                .ui
                .placement
                .measurement_markers
                .first()
                .map(|p| p.z - tex_origin.z)
                .unwrap_or(-10000),
            measurement_marker_1_x: self
                .ui
                .placement
                .measurement_markers
                .get(1)
                .map(|p| p.x - tex_origin.x)
                .unwrap_or(-10000),
            measurement_marker_1_y: self
                .ui
                .placement
                .measurement_markers
                .get(1)
                .map(|p| p.y - tex_origin.y)
                .unwrap_or(-10000),
            measurement_marker_1_z: self
                .ui
                .placement
                .measurement_markers
                .get(1)
                .map(|p| p.z - tex_origin.z)
                .unwrap_or(-10000),
            measurement_marker_2_x: self
                .ui
                .placement
                .measurement_markers
                .get(2)
                .map(|p| p.x - tex_origin.x)
                .unwrap_or(-10000),
            measurement_marker_2_y: self
                .ui
                .placement
                .measurement_markers
                .get(2)
                .map(|p| p.y - tex_origin.y)
                .unwrap_or(-10000),
            measurement_marker_2_z: self
                .ui
                .placement
                .measurement_markers
                .get(2)
                .map(|p| p.z - tex_origin.z)
                .unwrap_or(-10000),
            measurement_marker_3_x: self
                .ui
                .placement
                .measurement_markers
                .get(3)
                .map(|p| p.x - tex_origin.x)
                .unwrap_or(-10000),
            measurement_marker_3_y: self
                .ui
                .placement
                .measurement_markers
                .get(3)
                .map(|p| p.y - tex_origin.y)
                .unwrap_or(-10000),
            measurement_marker_3_z: self
                .ui
                .placement
                .measurement_markers
                .get(3)
                .map(|p| p.z - tex_origin.z)
                .unwrap_or(-10000),
            // Stencil rendering
            stencil_block_count,
            stencil_opacity,
            stencil_render_mode,
            // Measurement laser color
            laser_color_r: self.ui.tools_palette.settings.measurement.laser_color[0],
            laser_color_g: self.ui.tools_palette.settings.measurement.laser_color[1],
            laser_color_b: self.ui.tools_palette.settings.measurement.laser_color[2],
            sky_zenith_r: self.sim.atmosphere.sky_color_zenith[0],
            sky_zenith_g: self.sim.atmosphere.sky_color_zenith[1],
            sky_zenith_b: self.sim.atmosphere.sky_color_zenith[2],
            sky_horizon_r: self.sim.atmosphere.sky_color_horizon[0],
            sky_horizon_g: self.sim.atmosphere.sky_color_horizon[1],
            sky_horizon_b: self.sim.atmosphere.sky_color_horizon[2],
            selected_picture_id: self.ui.picture_state.selected_picture_id.unwrap_or(0),
            remote_player_count,
            custom_texture_count: self.graphics.multiplayer_texture_count,
            mushroom_pulse: 0.95 + 0.05 * (self.sim.animation_time * 1.5).sin(),
            lava_time_phase: self.sim.animation_time * 2.0,
        }
    }

    /// Computes texture-relative cutaway chunk coordinates for the debug cutaway feature.
    ///
    /// Returns `(chunk_x, chunk_y, chunk_z, player_chunk_x, player_chunk_z)`.
    /// All values are `-1000` (shader sentinel for "disabled") when the feature is off.
    fn compute_cutaway_coords(&self, tex_origin: Vector3<i32>) -> (i32, i32, i32, i32, i32) {
        if !self.ui.settings.debug_cutaway_enabled {
            return (-1000, -1000, -1000, -1000, -1000);
        }

        let cam_pos = self
            .sim
            .player
            .camera_world_pos(self.sim.world_extent, self.sim.texture_origin);
        let cam_dir = self.sim.player.camera_direction();

        let player_chunk_x = (cam_pos.x / 32.0).floor() as i32;
        let player_chunk_z = (cam_pos.z / 32.0).floor() as i32;

        // Find chunk one ahead in facing direction (use horizontal component only)
        let facing_chunk_offset_x = if cam_dir.x.abs() > cam_dir.z.abs() {
            if cam_dir.x > 0.0 { 1 } else { -1 }
        } else {
            0
        };
        let facing_chunk_offset_z = if cam_dir.z.abs() >= cam_dir.x.abs() {
            if cam_dir.z > 0.0 { 1 } else { -1 }
        } else {
            0
        };

        // Calculate the cutaway chunk's texture-relative position (in blocks)
        let cutaway_world_x = (player_chunk_x + facing_chunk_offset_x) * 32;
        let cutaway_world_z = (player_chunk_z + facing_chunk_offset_z) * 32;

        let chunk_x = cutaway_world_x - tex_origin.x;
        let chunk_y = 0; // Use full Y range of chunks (0 to world height); shader checks all Y levels
        let chunk_z = cutaway_world_z - tex_origin.z;
        let player_chunk_world_x = player_chunk_x * 32 - tex_origin.x;
        let player_chunk_world_z = player_chunk_z * 32 - tex_origin.z;

        (
            chunk_x,
            chunk_y,
            chunk_z,
            player_chunk_world_x,
            player_chunk_world_z,
        )
    }

    /// Handles batch and incremental picture uploads to the GPU atlas.
    ///
    /// Uploads all pictures on first render after world load, then handles
    /// individual pending uploads queued by player actions.
    fn upload_pending_pictures(&mut self) {
        // Batch upload all pictures on first render after world load
        if self.ui.picture_state.pictures_need_upload {
            log::debug!(
                "[Render] Batch uploading {} pictures to GPU atlas...",
                self.sim.picture_library.len()
            );
            let mut uploaded_count = 0;
            for picture in self.sim.picture_library.iter() {
                let success = crate::gpu_resources::upload_picture_to_atlas(
                    self.graphics.memory_allocator.clone(),
                    self.graphics.command_buffer_allocator.clone(),
                    &self.graphics.queue,
                    &self.graphics.picture_atlas,
                    &self.sim.picture_library,
                    picture.id,
                );
                if success {
                    uploaded_count += 1;
                }
            }
            self.ui.picture_state.pictures_need_upload = false;
            log::debug!("[Render] Uploaded {} pictures to GPU atlas", uploaded_count);
        }

        // Upload pending picture to GPU atlas if needed
        if let Some(picture_id) = self.ui.picture_state.pending_picture_upload.take() {
            log::debug!(
                "[Render] Uploading picture {} to GPU atlas (selected_picture_id: {:?})",
                picture_id,
                self.ui.picture_state.selected_picture_id
            );
            let success = crate::gpu_resources::upload_picture_to_atlas(
                self.graphics.memory_allocator.clone(),
                self.graphics.command_buffer_allocator.clone(),
                &self.graphics.queue,
                &self.graphics.picture_atlas,
                &self.sim.picture_library,
                picture_id,
            );
            log::debug!("[Render] Upload result: {}", success);
        }
    }

    /// Records the primary compute pass, the resample pass, the blit to the swapchain
    /// image, and submits the frame for presentation.
    ///
    /// Blocks until the frame fence is signalled.
    fn record_and_submit_frame(
        &mut self,
        image_index: u32,
        acquire_future: vulkano::swapchain::SwapchainAcquireFuture,
        push_constants: PushConstants,
        render_extent: [u32; 3],
        resample_extent: [u32; 3],
    ) {
        let mut builder = AutoCommandBufferBuilder::primary(
            self.graphics.command_buffer_allocator.clone(),
            self.graphics.queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )
        .unwrap();

        builder
            .clear_color_image(ClearColorImageInfo::image(
                self.graphics.rcx.as_ref().unwrap().render_image.clone(),
            ))
            .unwrap();

        // Single-pass rendering with empty chunk skip optimization
        // (Two-pass beam optimization was tested but added overhead without benefit
        // since empty chunk skip already makes rays very fast)

        builder
            .bind_pipeline_compute(self.graphics.render_pipeline.clone())
            .unwrap()
            .push_constants(
                self.graphics.render_pipeline.layout().clone(),
                0,
                push_constants,
            )
            .unwrap()
            .bind_descriptor_sets(
                PipelineBindPoint::Compute,
                self.graphics.render_pipeline.layout().clone(),
                0,
                vec![
                    self.graphics.rcx.as_ref().unwrap().render_set.clone(),
                    self.graphics.voxel_set.clone(),
                    self.graphics.texture_set.clone(),
                    self.graphics.particle_set.clone(),
                    self.graphics.light_set.clone(),
                    self.graphics.chunk_metadata_set.clone(),
                    self.graphics.rcx.as_ref().unwrap().distance_set.clone(),
                    self.graphics.brick_and_model_set.clone(), // Combined set 7: brick + model resources
                ],
            )
            .unwrap();

        // SAFETY: The pipeline and descriptor sets were bound immediately before this
        // dispatch call; the workgroup dimensions are derived from the swapchain extent
        // and cannot exceed hardware limits.  All buffer/image resources referenced by
        // the descriptor sets remain valid for the lifetime of the command buffer.
        unsafe {
            builder
                .dispatch([
                    render_extent[0].div_ceil(8),
                    render_extent[1].div_ceil(8),
                    1,
                ])
                .unwrap();
        }

        builder
            .bind_pipeline_compute(self.graphics.resample_pipeline.clone())
            .unwrap()
            .bind_descriptor_sets(
                PipelineBindPoint::Compute,
                self.graphics.resample_pipeline.layout().clone(),
                0,
                vec![self.graphics.rcx.as_ref().unwrap().resample_set.clone()],
            )
            .unwrap();

        // SAFETY: Same invariants as the primary dispatch above; the resample pipeline
        // and its descriptor set are fully bound and all referenced resources are live.
        unsafe {
            builder
                .dispatch([
                    resample_extent[0].div_ceil(8),
                    resample_extent[1].div_ceil(8),
                    1,
                ])
                .unwrap();
        }

        let mut info = BlitImageInfo::images(
            self.graphics.rcx.as_ref().unwrap().resample_image.clone(),
            self.graphics.rcx.as_ref().unwrap().image_views[image_index as usize]
                .image()
                .clone(),
        );
        info.filter = Filter::Nearest;
        builder.blit_image(info).unwrap();

        let command_buffer = builder.build().unwrap();

        let render_future = acquire_future
            .then_execute(self.graphics.queue.clone(), command_buffer)
            .unwrap();

        // Clone the image view before the mutable borrow for gui.draw_on_image
        let swapchain_image_view =
            self.graphics.rcx.as_ref().unwrap().image_views[image_index as usize].clone();
        let swapchain = self.graphics.rcx.as_ref().unwrap().swapchain.clone();

        let gui_future = self
            .graphics
            .rcx
            .as_mut()
            .unwrap()
            .gui
            .draw_on_image(render_future, swapchain_image_view);

        let frame_fence = gui_future
            .then_swapchain_present(
                self.graphics.queue.clone(),
                SwapchainPresentInfo::swapchain_image_index(swapchain, image_index),
            )
            .then_signal_fence_and_flush()
            .unwrap();

        // Store fence for pipelined waiting at the start of the next frame.
        // This allows the CPU to prepare the next frame while the GPU is still rendering.
        self.previous_frame_fence = Some(frame_fence.boxed());
    }

    /// Takes a screenshot if the configured delay has elapsed and one has not
    /// already been captured this session.
    fn handle_screenshot(&mut self, image_index: u32) {
        // Check if we need to take a screenshot
        let Some(delay) = self.args.screenshot_delay else {
            return;
        };
        if self.ui.frame.screenshot_taken {
            return;
        }

        let elapsed = Instant::now().duration_since(self.start_time).as_secs_f64();
        if elapsed < delay {
            return;
        }

        let image_view =
            self.graphics.rcx.as_ref().unwrap().image_views[image_index as usize].clone();

        // Take screenshot outside rcx borrow scope
        save_screenshot(
            &self.graphics.device,
            &self.graphics.queue,
            &self.graphics.memory_allocator,
            &self.graphics.command_buffer_allocator,
            &image_view,
            "voxel-world_screen_shot.png",
        );
        self.ui.frame.screenshot_taken = true;
    }
}

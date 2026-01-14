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

impl App {
    pub(super) fn render(&mut self, _event_loop: &ActiveEventLoop) {
        let t_render_start = Instant::now();
        self.graphics.render_pipeline.maybe_reload();
        self.graphics.resample_pipeline.maybe_reload();

        // Collect data before borrowing rcx (avoids borrow checker issues)
        // Convert player position from normalized to world coordinates for light collection
        let player_world_pos = self
            .sim
            .player
            .camera_world_pos(self.sim.world_extent, self.sim.texture_origin);
        // Get camera direction for frustum-aware light prioritization
        let camera_dir = self.sim.player.camera_direction();
        let camera_dir_f32 = Vector3::new(
            camera_dir.x as f32,
            camera_dir.y as f32,
            camera_dir.z as f32,
        );
        let gpu_lights = self.sim.world.collect_torch_lights(
            self.sim.player.light_enabled,
            player_world_pos,
            camera_dir_f32,
            self.sim.texture_origin,
            &self.sim.model_registry,
            self.sim.world_extent,
            self.sim.animation_time,
            self.ui.settings.light_cull_radius,
            self.ui.settings.max_active_lights as usize,
        );
        let light_count = gpu_lights.len() as u32;

        // Collect water/lava sources for debug visualization (only true sources with is_source flag)
        let water_source_count = if self.ui.settings.show_water_sources {
            let tex_origin = self.sim.texture_origin;
            let mut sources = Vec::new();

            // Collect water sources from grid (only cells with is_source == true)
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

            // Collect lava sources from grid (only cells with is_source == true)
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

            // Upload to GPU buffer
            let count = sources.len().min(gpu_resources::MAX_WATER_SOURCES);
            {
                let mut write = self.graphics.water_source_buffer.write().unwrap();
                for (i, src) in sources.iter().take(count).enumerate() {
                    write[i] = *src;
                }
            }
            count as u32
        } else {
            0
        };

        let player_world_pos = self
            .sim
            .player
            .feet_pos(self.sim.world_extent, self.sim.texture_origin);
        let selected_block = self.selected_block();

        // Pre-generate minimap image if showing (before entering gui closure)
        // Throttle updates based on position change and rotation change
        let camera_yaw = self.sim.player.camera.rotation.y as f32;
        let minimap_image =
            prepare_minimap_image(&mut self.ui, &mut self.sim, player_world_pos, camera_yaw);

        let rcx = self.graphics.rcx.as_mut().unwrap();

        if self.input.window_resized().is_some() {
            rcx.recreate_swapchain = true;
        }

        let window_size = rcx.window.inner_size();

        if window_size.width == 0 || window_size.height == 0 {
            return;
        }

        if rcx.recreate_swapchain {
            let images;
            (rcx.swapchain, images) = rcx
                .swapchain
                .recreate(SwapchainCreateInfo {
                    image_extent: window_size.into(),
                    ..rcx.swapchain.create_info()
                })
                .unwrap();

            rcx.image_views = images
                .iter()
                .map(|i| ImageView::new(i.clone(), ImageViewCreateInfo::from_image(i)).unwrap())
                .collect();

            let window_extent: [u32; 2] = window_size.into();
            self.ui.window_size = window_extent;
            let render_extent = [
                (window_extent[0] as f32 * self.ui.settings.render_scale) as u32,
                (window_extent[1] as f32 * self.ui.settings.render_scale) as u32,
            ];
            (
                rcx.render_image,
                rcx.render_set,
                rcx.resample_image,
                rcx.resample_set,
            ) = get_images_and_sets(
                self.graphics.memory_allocator.clone(),
                self.graphics.descriptor_set_allocator.clone(),
                &self.graphics.render_pipeline,
                &self.graphics.resample_pipeline,
                render_extent,
                window_extent,
            );

            // Recreate distance buffer for two-pass beam optimization
            (rcx.distance_image, rcx.distance_set) = get_distance_image_and_set(
                self.graphics.memory_allocator.clone(),
                self.graphics.descriptor_set_allocator.clone(),
                &self.graphics.render_pipeline,
                render_extent,
            );

            rcx.recreate_swapchain = false;
        }

        let (image_index, suboptimal, acquire_future) =
            match acquire_next_image(rcx.swapchain.clone(), None).map_err(Validated::unwrap) {
                Ok(r) => r,
                Err(VulkanError::OutOfDate) => {
                    rcx.recreate_swapchain = true;
                    return;
                }
                Err(e) => panic!("failed to acquire next image: {e}"),
            };

        if suboptimal {
            rcx.recreate_swapchain = true;
        }

        // Get atlas texture id before borrowing gui
        let _atlas_texture_id = rcx.atlas_texture_id;

        // Check if scale changed from dynamic render scale or UI
        let scale_changed_from_ui = render_hud(
            rcx,
            &mut self.ui,
            &mut self.sim,
            selected_block,
            minimap_image,
            camera_yaw,
            player_world_pos,
        );
        let scale_changed_from_dynamic = self.ui.pending_scale_change;
        self.ui.pending_scale_change = false;

        if scale_changed_from_ui || scale_changed_from_dynamic {
            let window_extent: [u32; 2] = rcx.window.inner_size().into();
            let render_extent = [
                (window_extent[0] as f32 * self.ui.settings.render_scale) as u32,
                (window_extent[1] as f32 * self.ui.settings.render_scale) as u32,
            ];
            (
                rcx.render_image,
                rcx.render_set,
                rcx.resample_image,
                rcx.resample_set,
            ) = get_images_and_sets(
                self.graphics.memory_allocator.clone(),
                self.graphics.descriptor_set_allocator.clone(),
                &self.graphics.render_pipeline,
                &self.graphics.resample_pipeline,
                render_extent,
                window_extent,
            );
            // Recreate distance buffer for two-pass beam optimization
            (rcx.distance_image, rcx.distance_set) = get_distance_image_and_set(
                self.graphics.memory_allocator.clone(),
                self.graphics.descriptor_set_allocator.clone(),
                &self.graphics.render_pipeline,
                render_extent,
            );
        }

        // Handle pending teleport from console
        if let Some(tp) = self.ui.console.pending_teleport.take() {
            let feet_pos = Vector3::new(tp.x, tp.y, tp.z);
            self.sim
                .player
                .set_feet_pos(feet_pos, self.sim.world_extent, self.sim.texture_origin);
            // Reset velocity to prevent continued movement
            self.sim.player.velocity = Vector3::zeros();
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

        let render_extent = rcx.render_image.extent();
        let resample_extent = rcx.resample_image.extent();
        self.sim.player.camera.extent = [render_extent[0] as f64, render_extent[1] as f64];

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

        let pixel_to_ray = pixel_to_ray_scaled;

        // Convert world coordinates to texture coordinates for shader
        // Shader works in texture space, so we subtract texture_origin
        let tex_origin = self.sim.texture_origin;
        let world_to_tex = |world_pos: Vector3<i32>| -> (i32, i32, i32) {
            (
                world_pos.x - tex_origin.x,
                world_pos.y - tex_origin.y,
                world_pos.z - tex_origin.z,
            )
        };

        // Populate template block buffer if a template is being placed
        let template_block_count = if let Some(ref placement) = self.ui.active_placement {
            let block_positions = placement.get_preview_blocks(gpu_resources::MAX_TEMPLATE_BLOCKS);
            let count = block_positions.len();

            let mut write = self.graphics.template_block_buffer.write().unwrap();
            for (i, pos) in block_positions.iter().enumerate() {
                // Convert world coordinates to texture coordinates
                let tex_pos = world_to_tex(*pos);
                write[i] = gpu_resources::GpuTemplateBlock {
                    position: [tex_pos.0 as f32, tex_pos.1 as f32, tex_pos.2 as f32, 0.0],
                };
            }
            count as u32
        } else {
            0
        };

        // Populate stencil block buffer from all active stencils and placement preview
        let (stencil_block_count, stencil_opacity, stencil_render_mode) = {
            let mut total_blocks = 0usize;
            let mut write = self.graphics.stencil_block_buffer.write().unwrap();

            // Add blocks from active stencils
            for stencil in &self.ui.stencil_manager.active_stencils {
                let color_id = stencil.id as u32 % 8; // Cycle through 8 colors
                for world_pos in stencil.iter_positions() {
                    if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                        break;
                    }
                    let tex_pos = world_to_tex(world_pos);
                    write[total_blocks] = gpu_resources::GpuStencilBlock {
                        position: [
                            tex_pos.0 as f32,
                            tex_pos.1 as f32,
                            tex_pos.2 as f32,
                            color_id as f32,
                        ],
                    };
                    total_blocks += 1;
                }
            }

            // Add blocks from stencil placement preview
            if let Some(ref placement) = self.ui.active_stencil_placement {
                let preview_color_id = 7u32; // Use distinct color for preview
                for world_pos in placement.get_preview_positions(gpu_resources::MAX_STENCIL_BLOCKS)
                {
                    if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                        break;
                    }
                    let tex_pos = world_to_tex(world_pos);
                    write[total_blocks] = gpu_resources::GpuStencilBlock {
                        position: [
                            tex_pos.0 as f32,
                            tex_pos.1 as f32,
                            tex_pos.2 as f32,
                            preview_color_id as f32,
                        ],
                    };
                    total_blocks += 1;
                }
            }

            // Add blocks from sphere tool preview
            if self.ui.sphere_tool.active {
                let preview_color_id = 0u32; // Cyan for sphere preview
                for world_pos in &self.ui.sphere_tool.preview_positions {
                    if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                        break;
                    }
                    let tex_pos = world_to_tex(*world_pos);
                    write[total_blocks] = gpu_resources::GpuStencilBlock {
                        position: [
                            tex_pos.0 as f32,
                            tex_pos.1 as f32,
                            tex_pos.2 as f32,
                            preview_color_id as f32,
                        ],
                    };
                    total_blocks += 1;
                }
            }

            // Add blocks from cube tool preview
            if self.ui.cube_tool.active {
                let preview_color_id = 0u32; // Cyan for cube preview
                for world_pos in &self.ui.cube_tool.preview_positions {
                    if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                        break;
                    }
                    let tex_pos = world_to_tex(*world_pos);
                    write[total_blocks] = gpu_resources::GpuStencilBlock {
                        position: [
                            tex_pos.0 as f32,
                            tex_pos.1 as f32,
                            tex_pos.2 as f32,
                            preview_color_id as f32,
                        ],
                    };
                    total_blocks += 1;
                }
            }

            // Add blocks from bridge tool preview
            if self.ui.bridge_tool.active {
                // First, show start position marker (magenta) if set but no preview yet
                if let Some(start) = self.ui.bridge_tool.start_position {
                    // Use magenta (color_id 2) for start marker to stand out
                    let start_color_id = 2u32;
                    if total_blocks < gpu_resources::MAX_STENCIL_BLOCKS {
                        let tex_pos = world_to_tex(start);
                        write[total_blocks] = gpu_resources::GpuStencilBlock {
                            position: [
                                tex_pos.0 as f32,
                                tex_pos.1 as f32,
                                tex_pos.2 as f32,
                                start_color_id as f32,
                            ],
                        };
                        total_blocks += 1;
                    }
                }

                // Then show preview line (cyan)
                let preview_color_id = 0u32; // Cyan for bridge preview
                for world_pos in &self.ui.bridge_tool.preview_positions {
                    if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                        break;
                    }
                    let tex_pos = world_to_tex(*world_pos);
                    write[total_blocks] = gpu_resources::GpuStencilBlock {
                        position: [
                            tex_pos.0 as f32,
                            tex_pos.1 as f32,
                            tex_pos.2 as f32,
                            preview_color_id as f32,
                        ],
                    };
                    total_blocks += 1;
                }
            }

            // Add blocks from cylinder tool preview
            if self.ui.cylinder_tool.active {
                let preview_color_id = 0u32; // Cyan for cylinder preview
                for world_pos in &self.ui.cylinder_tool.preview_positions {
                    if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                        break;
                    }
                    let tex_pos = world_to_tex(*world_pos);
                    write[total_blocks] = gpu_resources::GpuStencilBlock {
                        position: [
                            tex_pos.0 as f32,
                            tex_pos.1 as f32,
                            tex_pos.2 as f32,
                            preview_color_id as f32,
                        ],
                    };
                    total_blocks += 1;
                }
            }

            // Add blocks from wall tool preview
            if self.ui.wall_tool.active {
                let preview_color_id = 0u32; // Cyan for wall preview
                for world_pos in &self.ui.wall_tool.preview_positions {
                    if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                        break;
                    }
                    let tex_pos = world_to_tex(*world_pos);
                    write[total_blocks] = gpu_resources::GpuStencilBlock {
                        position: [
                            tex_pos.0 as f32,
                            tex_pos.1 as f32,
                            tex_pos.2 as f32,
                            preview_color_id as f32,
                        ],
                    };
                    total_blocks += 1;
                }
            }

            // Add blocks from floor tool preview
            if self.ui.floor_tool.active {
                let preview_color_id = 0u32; // Cyan for floor preview
                for world_pos in &self.ui.floor_tool.preview_positions {
                    if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                        break;
                    }
                    let tex_pos = world_to_tex(*world_pos);
                    write[total_blocks] = gpu_resources::GpuStencilBlock {
                        position: [
                            tex_pos.0 as f32,
                            tex_pos.1 as f32,
                            tex_pos.2 as f32,
                            preview_color_id as f32,
                        ],
                    };
                    total_blocks += 1;
                }
            }

            // Add blocks from replace tool preview (highlight blocks to be replaced)
            if self.ui.replace_tool.active {
                let preview_color_id = 3u32; // Yellow for replace preview (different from placement)
                for world_pos in &self.ui.replace_tool.preview_positions {
                    if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                        break;
                    }
                    let tex_pos = world_to_tex(*world_pos);
                    write[total_blocks] = gpu_resources::GpuStencilBlock {
                        position: [
                            tex_pos.0 as f32,
                            tex_pos.1 as f32,
                            tex_pos.2 as f32,
                            preview_color_id as f32,
                        ],
                    };
                    total_blocks += 1;
                }
            }

            // Add blocks from circle tool preview
            if self.ui.circle_tool.active {
                let preview_color_id = 0u32; // Cyan for circle preview
                for world_pos in &self.ui.circle_tool.preview_positions {
                    if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                        break;
                    }
                    let tex_pos = world_to_tex(*world_pos);
                    write[total_blocks] = gpu_resources::GpuStencilBlock {
                        position: [
                            tex_pos.0 as f32,
                            tex_pos.1 as f32,
                            tex_pos.2 as f32,
                            preview_color_id as f32,
                        ],
                    };
                    total_blocks += 1;
                }
            }

            // Add blocks from stairs tool preview
            if self.ui.stairs_tool.active {
                // Show start marker (magenta) if set but preview is empty
                if let Some(start) = self.ui.stairs_tool.start_pos {
                    if self.ui.stairs_tool.preview_positions.is_empty() {
                        let start_color_id = 2u32; // Magenta for start marker
                        if total_blocks < gpu_resources::MAX_STENCIL_BLOCKS {
                            let tex_pos = world_to_tex(start);
                            write[total_blocks] = gpu_resources::GpuStencilBlock {
                                position: [
                                    tex_pos.0 as f32,
                                    tex_pos.1 as f32,
                                    tex_pos.2 as f32,
                                    start_color_id as f32,
                                ],
                            };
                            total_blocks += 1;
                        }
                    }
                }

                // Render preview positions
                let preview_color_id = 1u32; // Green for stairs preview
                for world_pos in &self.ui.stairs_tool.preview_positions {
                    if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                        break;
                    }
                    let tex_pos = world_to_tex(*world_pos);
                    write[total_blocks] = gpu_resources::GpuStencilBlock {
                        position: [
                            tex_pos.0 as f32,
                            tex_pos.1 as f32,
                            tex_pos.2 as f32,
                            preview_color_id as f32,
                        ],
                    };
                    total_blocks += 1;
                }
            }

            // Add blocks from arch tool preview
            if self.ui.arch_tool.active {
                let preview_color_id = 0u32; // Cyan for arch preview
                for world_pos in &self.ui.arch_tool.preview_positions {
                    if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                        break;
                    }
                    let tex_pos = world_to_tex(*world_pos);
                    write[total_blocks] = gpu_resources::GpuStencilBlock {
                        position: [
                            tex_pos.0 as f32,
                            tex_pos.1 as f32,
                            tex_pos.2 as f32,
                            preview_color_id as f32,
                        ],
                    };
                    total_blocks += 1;
                }
            }

            // Add blocks from cone tool preview
            if self.ui.cone_tool.active {
                let preview_color_id = 0u32; // Cyan for cone preview
                for world_pos in &self.ui.cone_tool.preview_positions {
                    if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                        break;
                    }
                    let tex_pos = world_to_tex(*world_pos);
                    write[total_blocks] = gpu_resources::GpuStencilBlock {
                        position: [
                            tex_pos.0 as f32,
                            tex_pos.1 as f32,
                            tex_pos.2 as f32,
                            preview_color_id as f32,
                        ],
                    };
                    total_blocks += 1;
                }
            }

            // Add blocks from torus tool preview
            if self.ui.torus_tool.active {
                let preview_color_id = 0u32; // Cyan for torus preview
                for world_pos in &self.ui.torus_tool.preview_positions {
                    if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                        break;
                    }
                    let tex_pos = world_to_tex(*world_pos);
                    write[total_blocks] = gpu_resources::GpuStencilBlock {
                        position: [
                            tex_pos.0 as f32,
                            tex_pos.1 as f32,
                            tex_pos.2 as f32,
                            preview_color_id as f32,
                        ],
                    };
                    total_blocks += 1;
                }
            }

            // Add blocks from helix tool preview
            if self.ui.helix_tool.active {
                let preview_color_id = 0u32; // Cyan for helix preview
                for world_pos in &self.ui.helix_tool.preview_positions {
                    if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                        break;
                    }
                    let tex_pos = world_to_tex(*world_pos);
                    write[total_blocks] = gpu_resources::GpuStencilBlock {
                        position: [
                            tex_pos.0 as f32,
                            tex_pos.1 as f32,
                            tex_pos.2 as f32,
                            preview_color_id as f32,
                        ],
                    };
                    total_blocks += 1;
                }
            }

            // Add blocks from polygon tool preview
            if self.ui.polygon_tool.active {
                let preview_color_id = 0u32; // Cyan for polygon preview
                for world_pos in &self.ui.polygon_tool.preview_positions {
                    if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                        break;
                    }
                    let tex_pos = world_to_tex(*world_pos);
                    write[total_blocks] = gpu_resources::GpuStencilBlock {
                        position: [
                            tex_pos.0 as f32,
                            tex_pos.1 as f32,
                            tex_pos.2 as f32,
                            preview_color_id as f32,
                        ],
                    };
                    total_blocks += 1;
                }
            }

            // Add blocks from bezier tool preview
            if self.ui.bezier_tool.active {
                let preview_color_id = 0u32; // Cyan for bezier preview
                for world_pos in &self.ui.bezier_tool.preview_positions {
                    if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                        break;
                    }
                    let tex_pos = world_to_tex(*world_pos);
                    write[total_blocks] = gpu_resources::GpuStencilBlock {
                        position: [
                            tex_pos.0 as f32,
                            tex_pos.1 as f32,
                            tex_pos.2 as f32,
                            preview_color_id as f32,
                        ],
                    };
                    total_blocks += 1;
                }
            }

            // Add blocks from clone tool preview
            if self.ui.clone_tool.active {
                let preview_color_id = 1u32; // Green for clone preview (distinguishes from original)
                for world_pos in &self.ui.clone_tool.preview_positions {
                    if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                        break;
                    }
                    let tex_pos = world_to_tex(*world_pos);
                    write[total_blocks] = gpu_resources::GpuStencilBlock {
                        position: [
                            tex_pos.0 as f32,
                            tex_pos.1 as f32,
                            tex_pos.2 as f32,
                            preview_color_id as f32,
                        ],
                    };
                    total_blocks += 1;
                }
            }

            // Add blocks from pattern fill tool preview
            if self.ui.pattern_fill.active {
                // Block A uses cyan (0), Block B uses green (1)
                for world_pos in &self.ui.pattern_fill.preview_a {
                    if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                        break;
                    }
                    let tex_pos = world_to_tex(*world_pos);
                    write[total_blocks] = gpu_resources::GpuStencilBlock {
                        position: [
                            tex_pos.0 as f32,
                            tex_pos.1 as f32,
                            tex_pos.2 as f32,
                            0.0, // Cyan for Block A
                        ],
                    };
                    total_blocks += 1;
                }
                for world_pos in &self.ui.pattern_fill.preview_b {
                    if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                        break;
                    }
                    let tex_pos = world_to_tex(*world_pos);
                    write[total_blocks] = gpu_resources::GpuStencilBlock {
                        position: [
                            tex_pos.0 as f32,
                            tex_pos.1 as f32,
                            tex_pos.2 as f32,
                            1.0, // Green for Block B
                        ],
                    };
                    total_blocks += 1;
                }
            }

            // Add blocks from hollow tool preview (interior blocks to remove)
            if self.ui.hollow_tool.active {
                for world_pos in &self.ui.hollow_tool.preview_positions {
                    if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                        break;
                    }
                    let tex_pos = world_to_tex(*world_pos);
                    write[total_blocks] = gpu_resources::GpuStencilBlock {
                        position: [
                            tex_pos.0 as f32,
                            tex_pos.1 as f32,
                            tex_pos.2 as f32,
                            3.0, // Red/orange for blocks to be removed
                        ],
                    };
                    total_blocks += 1;
                }
            }

            // Add mirror plane visualization
            if self.ui.mirror_tool.active
                && self.ui.mirror_tool.plane_set
                && self.ui.mirror_tool.show_plane
            {
                use crate::shape_tools::mirror::MirrorAxis;

                let plane_pos = self.ui.mirror_tool.plane_position;
                let plane_color_id = 2u32; // Magenta for mirror plane
                let plane_size = 16i32; // Size of plane visualization

                // Generate plane grid based on axis
                match self.ui.mirror_tool.axis {
                    MirrorAxis::X => {
                        // Plane perpendicular to Z at z = plane_z
                        for dx in -plane_size..=plane_size {
                            for dy in 0..=plane_size {
                                if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                                    break;
                                }
                                let world_pos =
                                    Vector3::new(plane_pos.x + dx, plane_pos.y + dy, plane_pos.z);
                                let tex_pos = world_to_tex(world_pos);
                                write[total_blocks] = gpu_resources::GpuStencilBlock {
                                    position: [
                                        tex_pos.0 as f32,
                                        tex_pos.1 as f32,
                                        tex_pos.2 as f32,
                                        plane_color_id as f32,
                                    ],
                                };
                                total_blocks += 1;
                            }
                        }
                    }
                    MirrorAxis::Z => {
                        // Plane perpendicular to X at x = plane_x
                        for dz in -plane_size..=plane_size {
                            for dy in 0..=plane_size {
                                if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                                    break;
                                }
                                let world_pos =
                                    Vector3::new(plane_pos.x, plane_pos.y + dy, plane_pos.z + dz);
                                let tex_pos = world_to_tex(world_pos);
                                write[total_blocks] = gpu_resources::GpuStencilBlock {
                                    position: [
                                        tex_pos.0 as f32,
                                        tex_pos.1 as f32,
                                        tex_pos.2 as f32,
                                        plane_color_id as f32,
                                    ],
                                };
                                total_blocks += 1;
                            }
                        }
                    }
                    MirrorAxis::Both => {
                        // Show both planes (cross pattern)
                        // X-axis plane (perpendicular to Z)
                        for dx in -plane_size..=plane_size {
                            for dy in 0..=plane_size {
                                if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                                    break;
                                }
                                let world_pos =
                                    Vector3::new(plane_pos.x + dx, plane_pos.y + dy, plane_pos.z);
                                let tex_pos = world_to_tex(world_pos);
                                write[total_blocks] = gpu_resources::GpuStencilBlock {
                                    position: [
                                        tex_pos.0 as f32,
                                        tex_pos.1 as f32,
                                        tex_pos.2 as f32,
                                        plane_color_id as f32,
                                    ],
                                };
                                total_blocks += 1;
                            }
                        }
                        // Z-axis plane (perpendicular to X)
                        for dz in -plane_size..=plane_size {
                            for dy in 0..=plane_size {
                                if total_blocks >= gpu_resources::MAX_STENCIL_BLOCKS {
                                    break;
                                }
                                // Skip center column (already rendered by X plane)
                                if dz == 0 {
                                    continue;
                                }
                                let world_pos =
                                    Vector3::new(plane_pos.x, plane_pos.y + dy, plane_pos.z + dz);
                                let tex_pos = world_to_tex(world_pos);
                                write[total_blocks] = gpu_resources::GpuStencilBlock {
                                    position: [
                                        tex_pos.0 as f32,
                                        tex_pos.1 as f32,
                                        tex_pos.2 as f32,
                                        plane_color_id as f32,
                                    ],
                                };
                                total_blocks += 1;
                            }
                        }
                    }
                }
            }

            (
                total_blocks as u32,
                self.ui.stencil_manager.global_opacity,
                self.ui.stencil_manager.render_mode.as_i32() as u32,
            )
        };

        let (break_x, break_y, break_z) = self
            .ui
            .breaking_block
            .map(&world_to_tex)
            .unwrap_or((-1, -1, -1));

        // Calculate preview block position (where block would be placed)
        let selected_block_id = selected_block as u32;
        // Use player_world_pos computed earlier (before rcx borrow)
        let (preview_x, preview_y, preview_z, preview_type) = if self.ui.settings.show_block_preview
        {
            self.ui
                .current_hit
                .as_ref()
                .map(|hit| {
                    let place_pos = get_place_position(hit);
                    // Only show preview if position is in bounds and not inside player
                    let block_center = place_pos.cast::<f64>() + Vector3::new(0.5, 0.5, 0.5);
                    // Y bounds only (X/Z are infinite)
                    let in_bounds = place_pos.y >= 0 && place_pos.y < TEXTURE_SIZE_Y as i32;
                    let not_in_player = (player_world_pos - block_center).norm() > 1.5;
                    if in_bounds && not_in_player {
                        let tex_pos = world_to_tex(place_pos);
                        (tex_pos.0, tex_pos.1, tex_pos.2, selected_block_id)
                    } else {
                        (-1, -1, -1, 0)
                    }
                })
                .unwrap_or((-1, -1, -1, 0))
        } else {
            (-1, -1, -1, 0) // Preview disabled
        };

        // Target block (block player is looking at) - convert to texture coords
        // Only send if outline is enabled
        let (target_x, target_y, target_z) = if self.ui.settings.show_target_outline {
            self.ui
                .current_hit
                .as_ref()
                .map(|hit| world_to_tex(hit.block_pos))
                .unwrap_or((-1, -1, -1))
        } else {
            (-1, -1, -1) // Outline disabled
        };

        // Update particle buffer (convert world coords to texture coords)
        let gpu_particles = self.sim.particles.gpu_data();
        let particle_count = gpu_particles.len() as u32;
        {
            let tex_origin = self.sim.texture_origin;
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

        // Update falling block buffer (convert world coords to texture coords)
        let gpu_falling_blocks = self.sim.falling_blocks.gpu_data();
        {
            let tex_origin = self.sim.texture_origin;
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

        // Update light buffer with torch positions (collected earlier)
        {
            let mut write = self.graphics.light_buffer.write().unwrap();
            for (i, l) in gpu_lights.iter().enumerate() {
                write[i] = *l;
            }
        }

        let push_constants = PushConstants {
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
            break_progress: self.ui.break_progress,
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
            texture_origin_x: self.sim.texture_origin.x,
            texture_origin_y: self.sim.texture_origin.y,
            texture_origin_z: self.sim.texture_origin.z,
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
            template_preview_min_x: {
                if let Some(ref placement) = self.ui.active_placement {
                    let (min, _) = placement.get_bounding_box();
                    world_to_tex(min).0
                } else {
                    -1
                }
            },
            template_preview_min_y: {
                if let Some(ref placement) = self.ui.active_placement {
                    let (min, _) = placement.get_bounding_box();
                    world_to_tex(min).1
                } else {
                    -1
                }
            },
            template_preview_min_z: {
                if let Some(ref placement) = self.ui.active_placement {
                    let (min, _) = placement.get_bounding_box();
                    world_to_tex(min).2
                } else {
                    -1
                }
            },
            template_preview_max_x: {
                if let Some(ref placement) = self.ui.active_placement {
                    let (_, max) = placement.get_bounding_box();
                    world_to_tex(max).0
                } else {
                    -1
                }
            },
            template_preview_max_y: {
                if let Some(ref placement) = self.ui.active_placement {
                    let (_, max) = placement.get_bounding_box();
                    world_to_tex(max).1
                } else {
                    -1
                }
            },
            template_preview_max_z: {
                if let Some(ref placement) = self.ui.active_placement {
                    let (_, max) = placement.get_bounding_box();
                    world_to_tex(max).2
                } else {
                    -1
                }
            },
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
                .map(|p| world_to_tex(p).0)
                .unwrap_or(-1),
            selection_pos1_y: self
                .ui
                .template_selection
                .pos1
                .map(|p| world_to_tex(p).1)
                .unwrap_or(-1),
            selection_pos1_z: self
                .ui
                .template_selection
                .pos1
                .map(|p| world_to_tex(p).2)
                .unwrap_or(-1),
            selection_pos2_x: self
                .ui
                .template_selection
                .pos2
                .map(|p| world_to_tex(p).0)
                .unwrap_or(-1),
            selection_pos2_y: self
                .ui
                .template_selection
                .pos2
                .map(|p| world_to_tex(p).1)
                .unwrap_or(-1),
            selection_pos2_z: self
                .ui
                .template_selection
                .pos2
                .map(|p| world_to_tex(p).2)
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
            cutaway_chunk_x: {
                if self.ui.settings.debug_cutaway_enabled {
                    // Get camera position and direction
                    let cam_pos = self
                        .sim
                        .player
                        .camera_world_pos(self.sim.world_extent, self.sim.texture_origin);
                    let cam_dir = self.sim.player.camera_direction();

                    // Find player's current chunk
                    let player_chunk_x = (cam_pos.x / 32.0).floor() as i32;

                    // Find chunk one ahead in facing direction (use horizontal component only)
                    let facing_chunk_offset_x = if cam_dir.x.abs() > cam_dir.z.abs() {
                        if cam_dir.x > 0.0 { 1 } else { -1 }
                    } else {
                        0
                    };

                    // Calculate the cutaway chunk's texture-relative position (in blocks)
                    let cutaway_world_x = (player_chunk_x + facing_chunk_offset_x) * 32;
                    cutaway_world_x - self.sim.texture_origin.x
                } else {
                    -1000 // Far outside any valid chunk
                }
            },
            cutaway_chunk_y: {
                if self.ui.settings.debug_cutaway_enabled {
                    // Use full Y range of chunks (0 to world height)
                    0 // We'll check all Y levels in the shader
                } else {
                    -1000
                }
            },
            cutaway_chunk_z: {
                if self.ui.settings.debug_cutaway_enabled {
                    let cam_pos = self
                        .sim
                        .player
                        .camera_world_pos(self.sim.world_extent, self.sim.texture_origin);
                    let cam_dir = self.sim.player.camera_direction();

                    let player_chunk_z = (cam_pos.z / 32.0).floor() as i32;

                    let facing_chunk_offset_z = if cam_dir.z.abs() >= cam_dir.x.abs() {
                        if cam_dir.z > 0.0 { 1 } else { -1 }
                    } else {
                        0
                    };

                    let cutaway_world_z = (player_chunk_z + facing_chunk_offset_z) * 32;
                    cutaway_world_z - self.sim.texture_origin.z
                } else {
                    -1000
                }
            },
            cutaway_player_chunk_x: {
                if self.ui.settings.debug_cutaway_enabled {
                    let cam_pos = self
                        .sim
                        .player
                        .camera_world_pos(self.sim.world_extent, self.sim.texture_origin);
                    let player_chunk_x = (cam_pos.x / 32.0).floor() as i32;
                    let player_chunk_world_x = player_chunk_x * 32;
                    player_chunk_world_x - self.sim.texture_origin.x
                } else {
                    -1000
                }
            },
            cutaway_player_chunk_z: {
                if self.ui.settings.debug_cutaway_enabled {
                    let cam_pos = self
                        .sim
                        .player
                        .camera_world_pos(self.sim.world_extent, self.sim.texture_origin);
                    let player_chunk_z = (cam_pos.z / 32.0).floor() as i32;
                    let player_chunk_world_z = player_chunk_z * 32;
                    player_chunk_world_z - self.sim.texture_origin.z
                } else {
                    -1000
                }
            },
            // Measurement markers
            measurement_marker_count: self.ui.measurement_markers.len().min(4) as u32,
            measurement_marker_0_x: self
                .ui
                .measurement_markers
                .first()
                .map(|p| p.x - self.sim.texture_origin.x)
                .unwrap_or(-10000),
            measurement_marker_0_y: self
                .ui
                .measurement_markers
                .first()
                .map(|p| p.y - self.sim.texture_origin.y)
                .unwrap_or(-10000),
            measurement_marker_0_z: self
                .ui
                .measurement_markers
                .first()
                .map(|p| p.z - self.sim.texture_origin.z)
                .unwrap_or(-10000),
            measurement_marker_1_x: self
                .ui
                .measurement_markers
                .get(1)
                .map(|p| p.x - self.sim.texture_origin.x)
                .unwrap_or(-10000),
            measurement_marker_1_y: self
                .ui
                .measurement_markers
                .get(1)
                .map(|p| p.y - self.sim.texture_origin.y)
                .unwrap_or(-10000),
            measurement_marker_1_z: self
                .ui
                .measurement_markers
                .get(1)
                .map(|p| p.z - self.sim.texture_origin.z)
                .unwrap_or(-10000),
            measurement_marker_2_x: self
                .ui
                .measurement_markers
                .get(2)
                .map(|p| p.x - self.sim.texture_origin.x)
                .unwrap_or(-10000),
            measurement_marker_2_y: self
                .ui
                .measurement_markers
                .get(2)
                .map(|p| p.y - self.sim.texture_origin.y)
                .unwrap_or(-10000),
            measurement_marker_2_z: self
                .ui
                .measurement_markers
                .get(2)
                .map(|p| p.z - self.sim.texture_origin.z)
                .unwrap_or(-10000),
            measurement_marker_3_x: self
                .ui
                .measurement_markers
                .get(3)
                .map(|p| p.x - self.sim.texture_origin.x)
                .unwrap_or(-10000),
            measurement_marker_3_y: self
                .ui
                .measurement_markers
                .get(3)
                .map(|p| p.y - self.sim.texture_origin.y)
                .unwrap_or(-10000),
            measurement_marker_3_z: self
                .ui
                .measurement_markers
                .get(3)
                .map(|p| p.z - self.sim.texture_origin.z)
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
        };

        let mut builder = AutoCommandBufferBuilder::primary(
            self.graphics.command_buffer_allocator.clone(),
            self.graphics.queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )
        .unwrap();

        builder
            .clear_color_image(ClearColorImageInfo::image(rcx.render_image.clone()))
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
                    rcx.render_set.clone(),
                    self.graphics.voxel_set.clone(),
                    self.graphics.texture_set.clone(),
                    self.graphics.particle_set.clone(),
                    self.graphics.light_set.clone(),
                    self.graphics.chunk_metadata_set.clone(),
                    rcx.distance_set.clone(),
                    self.graphics.brick_and_model_set.clone(), // Combined set 7: brick + model resources
                ],
            )
            .unwrap();
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
                vec![rcx.resample_set.clone()],
            )
            .unwrap();

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
            rcx.resample_image.clone(),
            rcx.image_views[image_index as usize].image().clone(),
        );
        info.filter = Filter::Nearest;
        builder.blit_image(info).unwrap();

        let command_buffer = builder.build().unwrap();

        let render_future = acquire_future
            .then_execute(self.graphics.queue.clone(), command_buffer)
            .unwrap();

        let gui_future = rcx
            .gui
            .draw_on_image(render_future, rcx.image_views[image_index as usize].clone());

        gui_future
            .then_swapchain_present(
                self.graphics.queue.clone(),
                SwapchainPresentInfo::swapchain_image_index(rcx.swapchain.clone(), image_index),
            )
            .then_signal_fence_and_flush()
            .unwrap()
            .wait(None)
            .unwrap();

        // Check if we need to take a screenshot (do this before the borrow is released)
        let needs_screenshot = if let Some(delay) = self.args.screenshot_delay {
            if !self.ui.screenshot_taken {
                let elapsed = Instant::now().duration_since(self.start_time).as_secs_f64();
                if elapsed >= delay {
                    Some(rcx.image_views[image_index as usize].clone())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // Take screenshot if delay has elapsed (outside rcx borrow scope)
        if let Some(image_view) = needs_screenshot {
            save_screenshot(
                &self.graphics.device,
                &self.graphics.queue,
                &self.graphics.memory_allocator,
                &self.graphics.command_buffer_allocator,
                &image_view,
                "voxel_world_screen_shot.png",
            );
            self.ui.screenshot_taken = true;
        }

        // Record render time and increment sample count
        self.sim.profiler.render_us += t_render_start.elapsed().as_micros() as u64;
        self.sim.profiler.sample_count += 1;
    }
}

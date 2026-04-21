//! Shape tool execution: sphere, cube, cylinder, wall, floor, circle, stairs,
//! arch, cone, torus, helix, polygon, bezier, bridge, pattern fill, scatter,
//! hollow, terrain brush, clone, and replace.

use crate::block_interaction::BlockInteractionContext;
use crate::chunk::{BlockType, WaterType};
use crate::constants::TEXTURE_SIZE_Y;
use crate::placement::{BlockPlacementParams, place_blocks_at_positions};
use nalgebra::Vector3;

impl<'a> BlockInteractionContext<'a> {
    /// Place a sphere using the current sphere tool settings and hotbar selection.
    pub fn place_sphere(&mut self) {
        let sphere = &self.ui.sphere_tool;
        if !sphere.active || sphere.preview_center.is_none() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Regenerate full positions (preview may be truncated)
        let center = sphere.preview_center.unwrap();
        let radius = sphere.radius;
        let hollow = sphere.hollow;
        let dome = sphere.dome;
        let positions =
            crate::shape_tools::sphere::generate_sphere_positions(center, radius, hollow, dome);

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        log::debug!(
            "Placed {} sphere ({} blocks, radius {})",
            if hollow { "hollow" } else { "solid" },
            placed_count,
            radius
        );

        // Don't deactivate tool - allow placing multiple spheres
    }

    /// Place a cube using the current cube tool settings and hotbar selection.
    pub fn place_cube(&mut self) {
        let cube = &self.ui.cube_tool;
        if !cube.active || cube.preview_center.is_none() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Regenerate full positions (preview may be truncated)
        let center = cube.preview_center.unwrap();
        let size_x = cube.size_x;
        let size_y = cube.size_y;
        let size_z = cube.size_z;
        let hollow = cube.hollow;
        let dome = cube.dome;
        let positions = crate::shape_tools::cube::generate_cube_positions(
            center, size_x, size_y, size_z, hollow, dome,
        );

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        let width = size_x * 2 + 1;
        let height = size_y * 2 + 1;
        let depth = size_z * 2 + 1;
        log::debug!(
            "Placed {} cube ({} blocks, {}x{}x{})",
            if hollow { "hollow" } else { "solid" },
            placed_count,
            width,
            height,
            depth
        );

        // Don't deactivate tool - allow placing multiple cubes
    }

    /// Place a bridge (line) using the current bridge tool settings and hotbar selection.
    pub fn place_bridge(&mut self) {
        let bridge = &self.ui.bridge_tool;
        if !bridge.active || bridge.start_position.is_none() || bridge.preview_end.is_none() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Generate line positions
        let start = bridge.start_position.unwrap();
        let end = bridge.preview_end.unwrap();
        let positions = crate::shape_tools::bridge::generate_line_positions(start, end);

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        self.sim.world.invalidate_minimap_cache(start.x, start.z);
        self.sim.world.invalidate_minimap_cache(end.x, end.z);

        log::debug!(
            "Placed bridge ({} blocks, from ({},{},{}) to ({},{},{}))",
            placed_count,
            start.x,
            start.y,
            start.z,
            end.x,
            end.y,
            end.z
        );

        // Don't deactivate tool - allow placing multiple bridges
    }

    /// Place a cylinder using the current cylinder tool settings and hotbar selection.
    pub fn place_cylinder(&mut self) {
        let cylinder = &self.ui.cylinder_tool;
        if !cylinder.active || cylinder.preview_center.is_none() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Regenerate full positions (preview may be truncated)
        let center = cylinder.preview_center.unwrap();
        let radius = cylinder.radius;
        let height = cylinder.height;
        let hollow = cylinder.hollow;
        let axis = cylinder.axis;
        let positions = crate::shape_tools::cylinder::generate_cylinder_positions(
            center, radius, height, hollow, axis,
        );

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        let axis_name = match axis {
            crate::shape_tools::cylinder::CylinderAxis::Y => "vertical",
            crate::shape_tools::cylinder::CylinderAxis::X => "X-axis",
            crate::shape_tools::cylinder::CylinderAxis::Z => "Z-axis",
        };
        log::debug!(
            "Placed {} {} cylinder ({} blocks, radius {}, height {})",
            if hollow { "hollow" } else { "solid" },
            axis_name,
            placed_count,
            radius,
            height
        );

        // Don't deactivate tool - allow placing multiple cylinders
    }

    /// Place a wall using the current wall tool settings and hotbar selection.
    pub fn place_wall(&mut self) {
        let wall = &self.ui.wall_tool;
        if !wall.active || wall.start_position.is_none() || wall.preview_end.is_none() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Regenerate full positions (preview may be truncated)
        let start = wall.start_position.unwrap();
        let end = wall.preview_end.unwrap();
        let thickness = wall.thickness;
        let manual_height = wall.effective_manual_height();
        let positions =
            crate::shape_tools::wall::generate_wall_positions(start, end, thickness, manual_height);

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        self.sim.world.invalidate_minimap_cache(start.x, start.z);
        self.sim.world.invalidate_minimap_cache(end.x, end.z);

        let (length, height, thick) =
            crate::shape_tools::wall::calculate_dimensions(start, end, thickness, manual_height);
        log::debug!(
            "Placed wall ({} blocks, {}L × {}H × {}T)",
            placed_count,
            length,
            height,
            thick
        );

        // Don't deactivate tool - allow placing multiple walls
    }

    /// Place a floor/platform between two corners using the hotbar block.
    pub fn place_floor(&mut self) {
        let floor = &self.ui.floor_tool;
        if !floor.active || floor.start_position.is_none() || floor.preview_end.is_none() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Regenerate full positions (preview may be truncated)
        let start = floor.start_position.unwrap();
        let end = floor.preview_end.unwrap();
        let thickness = floor.thickness;
        let direction = floor.direction;
        let positions =
            crate::shape_tools::floor::generate_floor_positions(start, end, thickness, direction);

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        self.sim.world.invalidate_minimap_cache(start.x, start.z);
        self.sim.world.invalidate_minimap_cache(end.x, end.z);

        let (length, width, thick) =
            crate::shape_tools::floor::calculate_dimensions(start, end, thickness);
        log::debug!(
            "Placed floor ({} blocks, {}L × {}W × {}T)",
            placed_count,
            length,
            width,
            thick
        );

        // Don't deactivate tool - allow placing multiple floors
    }

    /// Place a circle or ellipse using the hotbar block.
    pub fn place_circle(&mut self) {
        let circle = &self.ui.circle_tool;
        if !circle.active || circle.preview_positions.is_empty() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Regenerate full positions (preview may be truncated)
        // Apply placement mode adjustment to get the actual center
        let raw_center = circle.preview_center.unwrap();
        let center = circle.adjust_center_for_placement(raw_center);
        let radius_a = circle.radius_a;
        let radius_b = circle.effective_radius_b();
        let plane = circle.plane;
        let filled = circle.filled;
        let ellipse_mode = circle.ellipse_mode;
        let positions = crate::shape_tools::circle::generate_circle_positions(
            center, radius_a, radius_b, plane, filled,
        );

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        self.sim.world.invalidate_minimap_cache(center.x, center.z);

        let radius_desc = if ellipse_mode {
            format!("{}×{}", radius_a, radius_b)
        } else {
            format!("{}", radius_a)
        };
        let fill_desc = if filled { "filled" } else { "outline" };
        log::debug!(
            "Placed {} circle ({} blocks, radius {})",
            fill_desc,
            placed_count,
            radius_desc
        );

        // Don't deactivate tool - allow placing multiple circles
    }

    /// Place stairs using the current stairs tool settings and hotbar selection.
    pub fn place_stairs(&mut self) {
        let stairs = &self.ui.stairs_tool;
        if !stairs.active || stairs.start_pos.is_none() || stairs.preview_positions.is_empty() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Use preview positions (already generated with current target)
        let positions = self.ui.stairs_tool.preview_positions.clone();
        let step_count = self.ui.stairs_tool.step_count;
        let width = self.ui.stairs_tool.width;

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }
        if let Some(last_pos) = positions.last() {
            self.sim
                .world
                .invalidate_minimap_cache(last_pos.x, last_pos.z);
        }

        log::debug!(
            "Placed stairs ({} blocks, {} steps × {} wide)",
            placed_count,
            step_count,
            width
        );

        // Don't deactivate tool - allow placing multiple staircases
    }

    /// Place an arch using the current arch tool settings and hotbar selection.
    pub fn place_arch(&mut self) {
        let arch = &self.ui.arch_tool;
        if !arch.active || arch.preview_positions.is_empty() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Use preview positions (already generated with current settings)
        let positions = self.ui.arch_tool.preview_positions.clone();
        let width = self.ui.arch_tool.width;
        let height = self.ui.arch_tool.height;
        let style_name = self.ui.arch_tool.style.name();

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        log::debug!(
            "Placed {} arch ({} blocks, {}W × {}H)",
            style_name,
            placed_count,
            width,
            height
        );

        // Don't deactivate tool - allow placing multiple arches
    }

    /// Place a cone or pyramid at the preview position.
    pub fn place_cone(&mut self) {
        let cone = &self.ui.cone_tool;
        if !cone.active || cone.preview_positions.is_empty() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Use preview positions (already generated with current settings)
        let positions = self.ui.cone_tool.preview_positions.clone();
        let shape_name = self.ui.cone_tool.shape.name();
        let base_size = self.ui.cone_tool.base_size;
        let height = self.ui.cone_tool.height;

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        log::debug!(
            "Placed {} ({} blocks, base {} × height {})",
            shape_name,
            placed_count,
            base_size,
            height
        );

        // Don't deactivate tool - allow placing multiple cones
    }

    /// Place a torus (ring/donut) at the preview position.
    pub fn place_torus(&mut self) {
        let torus = &self.ui.torus_tool;
        if !torus.active || torus.preview_positions.is_empty() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Use preview positions (already generated with current settings)
        let positions = self.ui.torus_tool.preview_positions.clone();
        let major_radius = self.ui.torus_tool.major_radius;
        let minor_radius = self.ui.torus_tool.minor_radius;
        let plane_name = self.ui.torus_tool.plane.name();

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        log::debug!(
            "Placed torus ({} blocks, R={}/{}, plane={})",
            placed_count,
            major_radius,
            minor_radius,
            plane_name
        );

        // Don't deactivate tool - allow placing multiple tori
    }

    /// Place a helix (spiral) at the preview position.
    pub fn place_helix(&mut self) {
        let helix = &self.ui.helix_tool;
        if !helix.active || helix.preview_positions.is_empty() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Use preview positions (already generated with current settings)
        let positions = self.ui.helix_tool.preview_positions.clone();
        let radius = self.ui.helix_tool.radius;
        let height = self.ui.helix_tool.height;
        let turns = self.ui.helix_tool.turns;

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        log::debug!(
            "Placed helix ({} blocks, R={}, H={}, {:.1} turns)",
            placed_count,
            radius,
            height,
            turns
        );

        // Don't deactivate tool - allow placing multiple helixes
    }

    /// Place a polygon/prism at the preview position.
    pub fn place_polygon(&mut self) {
        let polygon = &self.ui.polygon_tool;
        if !polygon.active || polygon.preview_positions.is_empty() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Use preview positions (already generated with current settings)
        let positions = self.ui.polygon_tool.preview_positions.clone();
        let sides = self.ui.polygon_tool.sides;
        let radius = self.ui.polygon_tool.radius;
        let height = self.ui.polygon_tool.height;

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        let shape_name = self.ui.polygon_tool.polygon_name();
        log::debug!(
            "Placed {} ({} blocks, {} sides, R={}, H={})",
            shape_name,
            placed_count,
            sides,
            radius,
            height
        );

        // Don't deactivate tool - allow placing multiple polygons
    }

    /// Place bezier curve at the preview positions.
    pub fn place_bezier(&mut self) {
        let bezier = &self.ui.bezier_tool;
        if !bezier.active || bezier.preview_positions.is_empty() {
            return;
        }

        // Get block type and metadata from hotbar
        let params = self.get_hotbar_placement_params();

        // Use preview positions (already generated with current settings)
        let positions = self.ui.bezier_tool.preview_positions.clone();
        let num_points = self.ui.bezier_tool.control_points.len();
        let tube_radius = self.ui.bezier_tool.tube_radius;

        // Place blocks using shared helper
        let placed_count = place_blocks_at_positions(
            &positions,
            params,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        let curve_type = if num_points == 3 {
            "quadratic"
        } else {
            "cubic"
        };
        log::debug!(
            "Placed {} Bezier curve ({} blocks, {} control points, tube R={})",
            curve_type,
            placed_count,
            num_points,
            tube_radius
        );

        // Clear control points for next curve, keep tool active
        self.ui.bezier_tool.clear();
    }

    /// Apply pattern fill to the selection.
    ///
    /// Uses hotbar slot 0 for Block A and slot 1 for Block B.
    pub fn apply_pattern_fill(&mut self) {
        let pattern = &self.ui.pattern_fill;
        if !pattern.active || pattern.preview_a.is_empty() {
            return;
        }

        // Get block types from hotbar slots 0 and 1
        let block_a = self.ui.hotbar.hotbar_blocks[0];
        let tint_a = self.ui.hotbar.hotbar_tint_indices[0];
        let paint_tex_a = self.ui.hotbar.hotbar_paint_textures[0];

        let block_b = self.ui.hotbar.hotbar_blocks[1];
        let tint_b = self.ui.hotbar.hotbar_tint_indices[1];
        let paint_tex_b = self.ui.hotbar.hotbar_paint_textures[1];

        // Create params for each block type
        let params_a = BlockPlacementParams::new(block_a, tint_a, paint_tex_a);
        let params_b = BlockPlacementParams::new(block_b, tint_b, paint_tex_b);

        // Clone positions before placement
        let positions_a = self.ui.pattern_fill.preview_a.clone();
        let positions_b = self.ui.pattern_fill.preview_b.clone();
        let pattern_type = self.ui.pattern_fill.pattern_type;

        // Place Block A positions
        let placed_a = place_blocks_at_positions(
            &positions_a,
            params_a,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Place Block B positions
        let placed_b = place_blocks_at_positions(
            &positions_b,
            params_b,
            &mut self.sim.world,
            &mut self.sim.water_grid,
            &mut self.sim.lava_grid,
        );

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions_a.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        log::debug!(
            "Applied {} pattern ({} + {} = {} blocks)",
            pattern_type.name(),
            placed_a,
            placed_b,
            placed_a + placed_b
        );

        // Don't deactivate tool - allow applying multiple patterns
    }

    /// Apply scatter brush placement at the given center position.
    ///
    /// Places blocks in a circular brush area with configurable density and height variation.
    /// Supports both regular blocks and model blocks.
    pub fn apply_scatter(&mut self, center: nalgebra::Vector3<i32>) {
        let scatter = &self.ui.scatter_tool;
        if !scatter.active {
            return;
        }

        // Get block type and model info from hotbar
        let block_type = self.ui.hotbar.hotbar_blocks[self.ui.hotbar.hotbar_index];
        let model_id = self.ui.hotbar.hotbar_model_ids[self.ui.hotbar.hotbar_index];
        let params = self.get_hotbar_placement_params();

        // Generate scatter positions
        let positions = if scatter.surface_only {
            // For surface mode, generate positions at the center Y
            crate::shape_tools::scatter::generate_scatter_positions(
                center,
                scatter.radius,
                scatter.density,
                scatter.seed(),
            )
        } else {
            // With height variation
            crate::shape_tools::scatter::generate_scatter_positions_with_height(
                center,
                scatter.radius,
                scatter.density,
                scatter.height_variation,
                scatter.seed(),
            )
        };

        // For surface-only mode, find actual surface positions
        let final_positions: Vec<_> = if self.ui.scatter_tool.surface_only {
            positions
                .into_iter()
                .filter_map(|pos| {
                    // Raycast downward to find surface
                    for dy in 0..20 {
                        let check_pos = nalgebra::Vector3::new(pos.x, pos.y - dy, pos.z);
                        if let Some(block) = self.sim.world.get_block(check_pos) {
                            let is_air = block == BlockType::Air;
                            let is_fluid = block == BlockType::Water || block == BlockType::Lava;
                            if !is_air && !is_fluid {
                                // Found solid block, place one above it
                                let place_pos = nalgebra::Vector3::new(
                                    check_pos.x,
                                    check_pos.y + 1,
                                    check_pos.z,
                                );
                                // Only place if position is air
                                if let Some(above) = self.sim.world.get_block(place_pos)
                                    && above == BlockType::Air
                                {
                                    return Some(place_pos);
                                }
                            }
                        }
                    }
                    None
                })
                .collect()
        } else {
            positions
        };

        if final_positions.is_empty() {
            return;
        }

        // Place blocks - handle models specially
        let placed_count = if block_type == BlockType::Model && model_id > 0 {
            // Place model blocks
            let mut count = 0;
            for pos in &final_positions {
                if pos.y >= 0 && pos.y < crate::constants::TEXTURE_SIZE_Y as i32 {
                    self.sim.world.set_model_block(*pos, model_id, 0, false);
                    count += 1;
                }
            }
            count
        } else {
            // Place regular blocks using shared helper
            place_blocks_at_positions(
                &final_positions,
                params,
                &mut self.sim.world,
                &mut self.sim.water_grid,
                &mut self.sim.lava_grid,
            )
        };

        // Invalidate minimap cache
        if let Some(first_pos) = final_positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        if placed_count > 0 {
            if block_type == BlockType::Model {
                log::debug!(
                    "Scattered {} models (R={}, D={}%)",
                    placed_count,
                    self.ui.scatter_tool.radius,
                    self.ui.scatter_tool.density
                );
            } else {
                log::debug!(
                    "Scattered {} blocks (R={}, D={}%)",
                    placed_count,
                    self.ui.scatter_tool.radius,
                    self.ui.scatter_tool.density
                );
            }
        }
    }

    /// Apply hollow operation to remove interior blocks from selection.
    ///
    /// Removes all blocks in the interior of the selection, leaving a shell
    /// with the configured wall thickness.
    pub fn apply_hollow(&mut self) {
        let hollow = &self.ui.hollow_tool;
        if !hollow.active || hollow.preview_positions.is_empty() {
            return;
        }

        // Clone positions before mutating world
        let positions = self.ui.hollow_tool.preview_positions.clone();
        let thickness = self.ui.hollow_tool.thickness;

        let mut removed = 0;
        for pos in &positions {
            if let Some(block) = self.sim.world.get_block(*pos) {
                // Only remove non-air, non-fluid blocks
                if block != BlockType::Air && block != BlockType::Water && block != BlockType::Lava
                {
                    // Clear water/lava cells if present
                    self.sim.water_grid.remove_water(*pos, 999.0);
                    self.sim.lava_grid.remove_lava(*pos, 999.0);

                    // Remove the block (set to air)
                    self.sim.world.set_block(*pos, BlockType::Air);
                    removed += 1;
                }
            }
        }

        // Invalidate minimap cache for affected area
        if let Some(first_pos) = positions.first() {
            self.sim
                .world
                .invalidate_minimap_cache(first_pos.x, first_pos.z);
        }

        log::debug!(
            "Hollowed {} interior blocks (thickness={})",
            removed,
            thickness
        );

        // Don't deactivate tool - allow applying to other selections
    }

    /// Apply terrain brush at the given center position.
    ///
    /// Modifies terrain based on brush mode (raise, lower, smooth, flatten).
    pub fn apply_terrain_brush(&mut self, center: nalgebra::Vector3<i32>) {
        use crate::shape_tools::terrain_brush::{
            TerrainBrushMode, calculate_flatten_positions, calculate_lower_positions,
            calculate_raise_positions, calculate_smooth_positions,
        };

        let brush = &self.ui.terrain_brush;
        if !brush.active {
            return;
        }

        let radius = brush.radius;
        let strength = brush.strength;
        let mode = brush.mode;
        let shape = brush.shape;
        let target_y = brush.target_y;

        // Gather terrain heights within brush radius
        let mut heights = Vec::new();
        let r2 = (radius * radius) as f32;
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                let include = match shape {
                    crate::shape_tools::terrain_brush::BrushShape::Circle => {
                        (dx * dx + dz * dz) as f32 <= r2
                    }
                    crate::shape_tools::terrain_brush::BrushShape::Square => true,
                };
                if include {
                    let x = center.x + dx;
                    let z = center.z + dz;
                    // Find terrain height by scanning down
                    if let Some(height) = self.find_terrain_height_at(x, z, center.y + 20) {
                        heights.push((x, z, height));
                    }
                }
            }
        }

        if heights.is_empty() {
            return;
        }

        // Get block type from hotbar
        let hotbar_block = self.ui.hotbar.hotbar_blocks[self.ui.hotbar.hotbar_index];
        let block_type = if hotbar_block == BlockType::Air {
            BlockType::Dirt // Default to dirt if air selected
        } else {
            hotbar_block
        };

        match mode {
            TerrainBrushMode::Raise => {
                let positions =
                    calculate_raise_positions(center, radius, strength, shape, &heights);
                for pos in positions {
                    if let Some(existing) = self.sim.world.get_block(pos)
                        && (existing == BlockType::Air
                            || existing == BlockType::Water
                            || existing == BlockType::Lava)
                    {
                        self.sim.world.set_block(pos, block_type);
                    }
                }
            }
            TerrainBrushMode::Lower => {
                let positions =
                    calculate_lower_positions(center, radius, strength, shape, &heights);
                for pos in positions {
                    if let Some(existing) = self.sim.world.get_block(pos)
                        && existing != BlockType::Air
                        && existing != BlockType::Bedrock
                    {
                        // Clear water/lava cells if present
                        self.sim.water_grid.remove_water(pos, 999.0);
                        self.sim.lava_grid.remove_lava(pos, 999.0);
                        self.sim.world.set_block(pos, BlockType::Air);
                    }
                }
            }
            TerrainBrushMode::Smooth => {
                let (to_add, to_remove) =
                    calculate_smooth_positions(center, radius, shape, &heights);
                // Remove blocks first
                for pos in to_remove {
                    if let Some(existing) = self.sim.world.get_block(pos)
                        && existing != BlockType::Air
                        && existing != BlockType::Bedrock
                    {
                        self.sim.water_grid.remove_water(pos, 999.0);
                        self.sim.lava_grid.remove_lava(pos, 999.0);
                        self.sim.world.set_block(pos, BlockType::Air);
                    }
                }
                // Then add blocks
                for pos in to_add {
                    if let Some(existing) = self.sim.world.get_block(pos)
                        && existing == BlockType::Air
                    {
                        self.sim.world.set_block(pos, block_type);
                    }
                }
            }
            TerrainBrushMode::Flatten => {
                let (to_add, to_remove) =
                    calculate_flatten_positions(center, radius, target_y, shape, &heights);
                // Remove blocks first
                for pos in to_remove {
                    if let Some(existing) = self.sim.world.get_block(pos)
                        && existing != BlockType::Air
                        && existing != BlockType::Bedrock
                    {
                        self.sim.water_grid.remove_water(pos, 999.0);
                        self.sim.lava_grid.remove_lava(pos, 999.0);
                        self.sim.world.set_block(pos, BlockType::Air);
                    }
                }
                // Then add blocks
                for pos in to_add {
                    if let Some(existing) = self.sim.world.get_block(pos)
                        && existing == BlockType::Air
                    {
                        self.sim.world.set_block(pos, block_type);
                    }
                }
            }
        }

        // Invalidate minimap cache for affected area
        self.sim.world.invalidate_minimap_cache(center.x, center.z);
    }

    /// Execute clone operation: copy blocks from selection to cloned positions.
    pub fn execute_clone(&mut self) {
        let clone_tool = &self.ui.clone_tool;
        if !clone_tool.active {
            return;
        }

        let selection = &self.ui.template_selection;
        if selection.pos1.is_none() || selection.pos2.is_none() {
            return;
        }

        let (min, max) = selection.bounds().unwrap();
        let selection_size = Vector3::new(
            (max.x - min.x + 1).abs(),
            (max.y - min.y + 1).abs(),
            (max.z - min.z + 1).abs(),
        );

        // Calculate clone origins
        let origins = crate::shape_tools::clone::calculate_clone_origins(
            selection_size,
            clone_tool.mode,
            clone_tool.axis,
            clone_tool.count,
            clone_tool.spacing,
            clone_tool.grid_count_x,
            clone_tool.grid_count_z,
            clone_tool.grid_spacing_x,
            clone_tool.grid_spacing_z,
            clone_tool.grid_count_y,
            clone_tool.grid_spacing_y,
        );

        // Skip the first origin (it's the original at 0,0,0)
        let clone_origins: Vec<_> = origins.into_iter().skip(1).collect();
        if clone_origins.is_empty() {
            log::debug!("Clone: No copies to make (count=1)");
            return;
        }

        // Collect source blocks with their types and metadata
        // (position, block_type, tint_index, paint_data)
        #[allow(clippy::type_complexity)]
        let mut source_blocks: Vec<(
            Vector3<i32>,
            BlockType,
            Option<u8>,
            Option<crate::chunk::BlockPaintData>,
        )> = Vec::new();
        if let Some(iter) = selection.iter_positions() {
            for pos in iter {
                let block = self.sim.world.get_block(pos);
                if let Some(block_type) = block {
                    if block_type == BlockType::Air {
                        continue;
                    }
                    let tint = self.sim.world.get_tint_index(pos);
                    let paint = self.sim.world.get_paint_data(pos);
                    source_blocks.push((pos, block_type, tint, paint));
                }
            }
        }

        if source_blocks.is_empty() {
            log::debug!("Clone: No blocks in selection to clone");
            return;
        }

        // Place cloned blocks at each origin offset
        let mut placed_count = 0;
        for origin in &clone_origins {
            for (source_pos, block_type, tint, paint) in &source_blocks {
                let target_pos = source_pos + origin;

                // Skip if out of Y bounds
                if target_pos.y < 0 || target_pos.y >= TEXTURE_SIZE_Y as i32 {
                    continue;
                }

                match *block_type {
                    BlockType::TintedGlass => {
                        let tint_idx: u8 = tint.unwrap_or(0);
                        self.sim.world.set_tinted_glass_block(target_pos, tint_idx);
                    }
                    BlockType::Crystal => {
                        let tint_idx: u8 = tint.unwrap_or(0);
                        self.sim.world.set_crystal_block(target_pos, tint_idx);
                    }
                    BlockType::Painted => {
                        if let Some(p) = paint {
                            self.sim.world.set_painted_block_full(
                                target_pos,
                                p.texture_idx,
                                p.tint_idx,
                                p.blend_mode,
                            );
                        } else {
                            self.sim.world.set_painted_block(target_pos, 0, 0);
                        }
                    }
                    BlockType::Water => {
                        let water_type = self
                            .sim
                            .world
                            .get_water_type(*source_pos)
                            .unwrap_or(WaterType::Ocean);
                        self.sim.water_grid.place_source(target_pos, water_type);
                        self.sim.world.set_water_block(target_pos, water_type);
                    }
                    BlockType::Lava => {
                        self.sim.lava_grid.place_source(target_pos);
                        self.sim.world.set_block(target_pos, BlockType::Lava);
                    }
                    BlockType::Model => {
                        // Clone model blocks with their metadata
                        if let Some(model_data) = self.sim.world.get_model_data(*source_pos) {
                            self.sim.world.set_model_block(
                                target_pos,
                                model_data.model_id,
                                model_data.rotation,
                                model_data.waterlogged,
                            );
                        }
                    }
                    BlockType::Air => {
                        // Skip air blocks
                        continue;
                    }
                    _ => {
                        self.sim.world.set_block(target_pos, *block_type);
                    }
                }
                placed_count += 1;
            }
        }

        // Invalidate minimap cache for affected area
        self.sim.world.invalidate_minimap_cache(min.x, min.z);
        self.sim.world.invalidate_minimap_cache(max.x, max.z);
        // Also invalidate cache for cloned regions
        for origin in &clone_origins {
            self.sim
                .world
                .invalidate_minimap_cache(min.x + origin.x, min.z + origin.z);
            self.sim
                .world
                .invalidate_minimap_cache(max.x + origin.x, max.z + origin.z);
        }

        let mode_name = self.ui.clone_tool.mode.name();
        log::debug!(
            "Cloned {} blocks in {} mode ({} copies)",
            placed_count,
            mode_name,
            clone_origins.len()
        );

        // Clear preview after cloning
        self.ui.clone_tool.clear_preview();
    }

    /// Execute block replacement within the current selection.
    pub fn execute_replace(&mut self) {
        let replace = &self.ui.replace_tool;
        if !replace.active {
            return;
        }

        let selection = &self.ui.template_selection;
        if selection.pos1.is_none() || selection.pos2.is_none() {
            return;
        }

        let (min, max) = selection.bounds().unwrap();
        let source_id = replace.source_identity();
        let target_block = replace.target_block;
        let target_tint = replace.target_tint;
        let target_texture = replace.target_texture;

        let mut replaced_count = 0;

        for x in min.x..=max.x {
            for y in min.y..=max.y {
                for z in min.z..=max.z {
                    let pos = nalgebra::Vector3::new(x, y, z);

                    // Skip if out of Y bounds
                    if y < 0 || y >= TEXTURE_SIZE_Y as i32 {
                        continue;
                    }

                    if source_id.matches(&self.sim.world, pos) {
                        // Replace the block
                        match target_block {
                            BlockType::TintedGlass => {
                                self.sim.world.set_tinted_glass_block(pos, target_tint);
                            }
                            BlockType::Crystal => {
                                self.sim.world.set_crystal_block(pos, target_tint);
                            }
                            BlockType::Painted => {
                                let blend_mode =
                                    self.ui.paint_panel.current_config.blend_mode as u8;
                                self.sim.world.set_painted_block_full(
                                    pos,
                                    target_texture,
                                    target_tint,
                                    blend_mode,
                                );
                            }
                            BlockType::Water => {
                                let water_type = WaterType::from_u8(target_tint);
                                self.sim.water_grid.place_source(pos, water_type);
                                self.sim.world.set_water_block(pos, water_type);
                            }
                            BlockType::Lava => {
                                self.sim.lava_grid.place_source(pos);
                                self.sim.world.set_block(pos, BlockType::Lava);
                            }
                            BlockType::Air => {
                                // Removing blocks - need to handle water/lava
                                let old_block = self.sim.world.get_block(pos);
                                if old_block == Some(BlockType::Water) {
                                    self.sim.water_grid.remove_source(pos);
                                } else if old_block == Some(BlockType::Lava) {
                                    self.sim.lava_grid.remove_source(pos);
                                }
                                self.sim.world.set_block(pos, BlockType::Air);
                            }
                            BlockType::Model => {
                                // Skip model blocks - not supported for replacement
                                continue;
                            }
                            _ => {
                                self.sim.world.set_block(pos, target_block);
                            }
                        }
                        replaced_count += 1;
                    }
                }
            }
        }

        // Invalidate minimap cache for affected area
        self.sim.world.invalidate_minimap_cache(min.x, min.z);
        self.sim.world.invalidate_minimap_cache(max.x, max.z);

        log::debug!(
            "Replaced {} blocks: {:?} -> {:?}",
            replaced_count,
            self.ui.replace_tool.source_block,
            target_block
        );

        // Clear preview after replacement
        self.ui.replace_tool.clear_preview();
    }
}

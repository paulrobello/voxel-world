use crate::app_state::{UiState, WorldSim};
use crate::chunk::{BlockType, CHUNK_SIZE};
use crate::constants::WORLD_CHUNKS_Y;
use crate::hud::Minimap;
use crate::terrain_gen::TerrainGenerator;
use crate::world::World;
use egui_winit_vulkano::egui;
use nalgebra::{Vector3, vector};
use std::time::Instant;

pub fn prepare_minimap_image(
    ui: &mut UiState,
    sim: &mut WorldSim,
    player_world_pos: Vector3<f64>,
    camera_yaw: f32,
) -> Option<egui::ColorImage> {
    if !ui.minimap_ui.show_minimap {
        return None;
    }

    let current_pos = Vector3::new(
        player_world_pos.x.floor() as i32,
        player_world_pos.y.floor() as i32,
        player_world_pos.z.floor() as i32,
    );
    // Check if player moved at least 1 block
    let moved = (current_pos.x - ui.minimap_ui.minimap_last_pos.x).abs() >= 1
        || (current_pos.z - ui.minimap_ui.minimap_last_pos.z).abs() >= 1;
    // Check if player rotated significantly (5 degrees) - only matters when rotate mode is on
    let yaw_changed =
        ui.minimap_ui.minimap.rotate && (camera_yaw - ui.minimap_ui.minimap_last_yaw).abs() > 0.087; // ~5 degrees
    // Check if enough time has passed (0.1 seconds for rotation, 0.5 for position)
    let time_elapsed = ui.minimap_ui.minimap_last_update.elapsed().as_secs_f32();
    let time_ok = if ui.minimap_ui.minimap.rotate {
        time_elapsed >= 0.1
    } else {
        time_elapsed >= 0.5
    };

    if ((moved || yaw_changed) && time_ok) || ui.minimap_ui.minimap_cached_image.is_none() {
        // Update last position/time/yaw and regenerate
        ui.minimap_ui.minimap_last_pos = current_pos;
        ui.minimap_ui.minimap_last_update = Instant::now();
        ui.minimap_ui.minimap_last_yaw = camera_yaw;
        let image = generate_minimap_image(
            &mut sim.world,
            player_world_pos,
            camera_yaw,
            &ui.minimap_ui.minimap,
            &sim.terrain_generator,
        );
        ui.minimap_ui.minimap_cached_image = Some(image.clone());
        Some(image)
    } else {
        // Use cached image
        ui.minimap_ui.minimap_cached_image.clone()
    }
}

/// Build the minimap pixel image by sampling the world's surface heights.
///
/// This is a view-layer concern (it produces display pixels), so it lives in the
/// app module rather than on `World` — keeping the `world` domain module free of
/// `egui` and HUD imports (audit ARC-M06).
fn generate_minimap_image(
    world: &mut World,
    player_pos: Vector3<f64>,
    yaw: f32,
    minimap: &Minimap,
    terrain: &TerrainGenerator,
) -> egui::ColorImage {
    let display_size = minimap.size as usize;
    let center_x = player_pos.x as f32;
    let center_z = player_pos.z as f32;

    // Base sample radius adjusted by zoom (higher zoom = larger area = zoomed out)
    // When rotating, multiply by sqrt(2) ≈ 1.42 to fill corners
    let base_radius = (display_size as f32 / 2.0) * minimap.zoom;
    let sample_radius = if minimap.rotate {
        base_radius * 1.42
    } else {
        base_radius
    };

    let mut pixels = vec![egui::Color32::BLACK; display_size * display_size];

    // Precompute rotation (rotate world coords to align with player facing direction)
    let (sin_yaw, cos_yaw) = if minimap.rotate {
        (yaw.sin(), yaw.cos())
    } else {
        (0.0, 1.0) // No rotation
    };

    let half = display_size as f32 / 2.0;

    for dz in 0..display_size {
        for dx in 0..display_size {
            // Screen-space offset from center (-half to +half)
            let sx = dx as f32 - half;
            let sz = dz as f32 - half;

            // Scale to sample radius
            let scale = sample_radius / half;
            let scaled_x = sx * scale;
            let scaled_z = sz * scale;

            // Apply rotation to get world-space offset
            let world_offset_x = scaled_x * cos_yaw + scaled_z * sin_yaw;
            let world_offset_z = -scaled_x * sin_yaw + scaled_z * cos_yaw;

            let world_x = (center_x + world_offset_x).floor() as i32;
            let world_z = (center_z + world_offset_z).floor() as i32;

            // Find surface block (top-down) with caching. `.copied()` releases the
            // mutable cache borrow before the scan below re-borrows `world`.
            let cached = world
                .minimap_height_cache_mut()
                .get(&(world_x, world_z))
                .copied();
            let (block_type, height) = if let Some(c) = cached {
                c
            } else {
                let mut res = (BlockType::Air, 0);

                // Optimization: Scan chunk-by-chunk from top to bottom
                // Skip empty chunks entirely (32 blocks at once)
                'chunk_scan: for chunk_y in (0..WORLD_CHUNKS_Y).rev() {
                    let chunk_pos = vector![
                        world_x.div_euclid(CHUNK_SIZE as i32),
                        chunk_y,
                        world_z.div_euclid(CHUNK_SIZE as i32)
                    ];

                    // Check if chunk exists and is not empty
                    if let Some(chunk) = world.get_chunk(chunk_pos) {
                        // Skip entire chunk if it's all air
                        if chunk.is_empty() {
                            continue;
                        }

                        // Scan blocks within this chunk (top to bottom)
                        for local_y in (0..CHUNK_SIZE).rev() {
                            let y = chunk_y * CHUNK_SIZE as i32 + local_y as i32;
                            if let Some(block) = world.get_block(Vector3::new(world_x, y, world_z))
                                && block != BlockType::Air
                            {
                                // Skip ground clutter (flowers, grass, torches) if enabled
                                // Note: Leaves are intentionally NOT skipped - trees are important landmarks
                                if minimap.skip_decorative && block == BlockType::Model {
                                    continue; // Keep scanning down past ground clutter
                                }
                                res = (block, y);
                                break 'chunk_scan;
                            }
                        }
                    }
                }

                world
                    .minimap_height_cache_mut()
                    .insert((world_x, world_z), res);
                res
            };

            // Get biome info for noise maps
            let biome_info = Some(terrain.get_biome_info(world_x, world_z));

            // Calculate color based on mode
            let color = minimap.get_color(block_type, height, biome_info);

            pixels[dz * display_size + dx] = color;
        }
    }

    egui::ColorImage {
        size: [display_size, display_size],
        pixels,
    }
}

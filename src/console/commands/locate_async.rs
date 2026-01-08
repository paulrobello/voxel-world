//! Frame-distributed locate search updates.

use crate::cave_gen::CaveGenerator;
use crate::chunk::BlockType;
use crate::console::{CommandResult, LocateSearchType, PendingLocateSearch};
use crate::terrain_gen::TerrainGenerator;
use crate::world::World;
use nalgebra::Vector3;
use std::collections::{HashSet, VecDeque};

/// Update a pending locate search for one frame.
/// Returns Some(CommandResult) if search completes, None if still searching.
pub fn update_locate_search(
    search: &mut PendingLocateSearch,
    world: &World,
    terrain: &TerrainGenerator,
    cave_gen: &CaveGenerator,
) -> Option<CommandResult> {
    let mut positions_this_frame = 0;

    // Continue searching based on type
    match &search.search_type {
        LocateSearchType::Biome(target_biome) => {
            update_biome_search(search, *target_biome, terrain, &mut positions_this_frame)
        }
        LocateSearchType::Block(target_block) => update_block_search(
            search,
            *target_block,
            world,
            terrain,
            cave_gen,
            &mut positions_this_frame,
        ),
        LocateSearchType::Cave(min_size) => {
            update_cave_search(search, *min_size, world, &mut positions_this_frame)
        }
    }
}

/// Update biome search for one frame
fn update_biome_search(
    search: &mut PendingLocateSearch,
    target_biome: crate::terrain_gen::BiomeType,
    terrain: &TerrainGenerator,
    positions_this_frame: &mut usize,
) -> Option<CommandResult> {
    let start_x = search.player_pos.x;
    let start_z = search.player_pos.z;
    let step = search.step;
    let step_usize = step as usize;

    // Search in spiral pattern
    while search.current_radius <= search.max_range {
        let radius = search.current_radius;

        // Safety check: if radius exceeds max_range, stop
        if radius > search.max_range {
            break;
        }

        // Generate positions for this radius
        let positions = [
            (-radius..=radius)
                .step_by(step_usize)
                .map(|x| (start_x + x, start_z - radius))
                .collect::<Vec<_>>(),
            (-radius..=radius)
                .step_by(step_usize)
                .map(|z| (start_x + radius, start_z + z))
                .collect::<Vec<_>>(),
            (-radius..=radius)
                .step_by(step_usize)
                .map(|x| (start_x - x, start_z + radius))
                .collect::<Vec<_>>(),
            (-radius..=radius)
                .step_by(step_usize)
                .map(|z| (start_x - radius, start_z - z))
                .collect::<Vec<_>>(),
        ]
        .concat();

        // Check positions for this frame
        for (x, z) in positions {
            if *positions_this_frame >= search.positions_per_frame {
                return None; // Continue next frame
            }

            let biome = terrain.get_biome(x, z);
            search.positions_checked += 1;
            *positions_this_frame += 1;

            if biome == target_biome {
                let dx = x - start_x;
                let dz = z - start_z;
                let distance = (dx * dx + dz * dz).abs();

                if distance < search.min_distance {
                    search.min_distance = distance;
                    let y = terrain.get_height(x, z);
                    search.best_match = Some((Vector3::new(x, y, z), 0));
                }
            }
        }

        // If we found something, return it
        if search.best_match.is_some() {
            let (pos, _) = search.best_match.unwrap();
            let distance = ((search.min_distance as f64).sqrt()) as i32;
            let dx = pos.x - start_x;
            let dz = pos.z - start_z;

            let direction = if dx.abs() > dz.abs() {
                if dx > 0 { "east" } else { "west" }
            } else if dz > 0 {
                "south"
            } else {
                "north"
            };

            return Some(if search.teleport_on_find {
                CommandResult::Teleport {
                    x: pos.x as f64 + 0.5,
                    y: pos.y as f64,
                    z: pos.z as f64 + 0.5,
                }
            } else {
                CommandResult::LocateBiome {
                    biome_name: format!("{:?}", target_biome),
                    x: pos.x,
                    y: pos.y,
                    z: pos.z,
                    distance,
                    direction: direction.to_string(),
                }
            });
        }

        // Move to next radius
        search.current_radius += step;
    }

    // Search complete, not found
    Some(CommandResult::Error(format!(
        "Could not find {:?} biome within {} blocks (checked {} positions)",
        target_biome, search.max_range, search.positions_checked
    )))
}

/// Update block search for one frame
fn update_block_search(
    search: &mut PendingLocateSearch,
    target_block: BlockType,
    world: &World,
    terrain: &TerrainGenerator,
    cave_gen: &CaveGenerator,
    positions_this_frame: &mut usize,
) -> Option<CommandResult> {
    let start_x = search.player_pos.x;
    let start_y = search.player_pos.y;
    let start_z = search.player_pos.z;
    let step = search.step;
    let step_usize = step as usize;

    // 3D spiral search (horizontal spiral at each Y level)
    // For lava specifically, focus on Y: 5-30 range in mountains
    let mut y_levels_skipped = 0;
    while search.y_offset < 256 {
        // Alternate between below and above player
        let y = start_y + (search.y_offset * search.y_dir);

        // Skip Y levels outside valid range
        let should_skip = if target_block == BlockType::Lava {
            !(5..=30).contains(&y)
        } else {
            !(0..512).contains(&y)
        };

        if should_skip {
            // Move to next Y level
            if search.y_dir == -1 {
                search.y_dir = 1;
            } else {
                search.y_dir = -1;
                search.y_offset += 8;
                search.current_radius = step; // Reset radius for next Y level
            }
            y_levels_skipped += 1;
            // Yield after skipping 20 Y levels to prevent tight loop
            if y_levels_skipped >= 20 {
                return None;
            }
            continue;
        }

        // Search this Y level in spiral pattern
        while search.current_radius <= search.max_range {
            let radius = search.current_radius;

            let positions = [
                (-radius..=radius)
                    .step_by(step_usize)
                    .map(|x| Vector3::new(start_x + x, y, start_z - radius))
                    .collect::<Vec<_>>(),
                (-radius..=radius)
                    .step_by(step_usize)
                    .map(|z| Vector3::new(start_x + radius, y, start_z + z))
                    .collect::<Vec<_>>(),
                (-radius..=radius)
                    .step_by(step_usize)
                    .map(|x| Vector3::new(start_x - x, y, start_z + radius))
                    .collect::<Vec<_>>(),
                (-radius..=radius)
                    .step_by(step_usize)
                    .map(|z| Vector3::new(start_x - radius, y, start_z - z))
                    .collect::<Vec<_>>(),
            ]
            .concat();

            // Check positions for this frame
            for pos in positions {
                if *positions_this_frame >= search.positions_per_frame {
                    return None; // Continue next frame
                }

                search.positions_checked += 1;
                *positions_this_frame += 1;

                // Early termination: if we've checked 50k+ positions for lava without finding mountains, give up
                if target_block == BlockType::Lava
                    && search.positions_checked > 50000
                    && search.relevant_biomes_found == 0
                {
                    return Some(CommandResult::Error(
                        "No mountain biomes found within search range. Lava only spawns in mountain caves."
                            .to_string(),
                    ));
                }

                // For lava, use terrain generator to predict spawns (doesn't require loaded chunks)
                if target_block == BlockType::Lava {
                    // Check if this would be a lava spawn using terrain generation
                    let biome = terrain.get_biome(pos.x, pos.z);

                    // Only mountains have lava lakes
                    if !matches!(biome, crate::terrain_gen::BiomeType::Mountains) {
                        continue;
                    }

                    // Track that we found a mountain biome
                    search.relevant_biomes_found += 1;

                    // Check if there's a cave here
                    let surface_height = terrain.get_height(pos.x, pos.z);
                    if !cave_gen.is_cave(pos.x, pos.y, pos.z, surface_height, biome) {
                        continue;
                    }

                    // Check if lava would spawn here
                    if !cave_gen.should_spawn_lava(biome, pos.y) {
                        continue;
                    }

                    // Found a lava spawn location!
                    let dx = pos.x - start_x;
                    let dy = pos.y - start_y;
                    let dz = pos.z - start_z;
                    let distance = dx * dx + dy * dy + dz * dz;

                    if distance < search.min_distance {
                        search.min_distance = distance;
                        search.best_match = Some((pos, 0));
                    }
                } else {
                    // For other blocks, use world.get_block (requires loaded chunks)
                    if let Some(block) = world.get_block(pos) {
                        if block == target_block {
                            let dx = pos.x - start_x;
                            let dy = pos.y - start_y;
                            let dz = pos.z - start_z;
                            let distance = dx * dx + dy * dy + dz * dz;

                            if distance < search.min_distance {
                                search.min_distance = distance;
                                search.best_match = Some((pos, 0));
                            }
                        }
                    }
                }
            }

            // If we found something, return it
            if search.best_match.is_some() {
                let (pos, _) = search.best_match.unwrap();
                let distance = ((search.min_distance as f64).sqrt()) as i32;
                let dx = pos.x - start_x;
                let dz = pos.z - start_z;

                let direction = if dx.abs() > dz.abs() {
                    if dx > 0 { "east" } else { "west" }
                } else if dz > 0 {
                    "south"
                } else {
                    "north"
                };

                return Some(if search.teleport_on_find {
                    CommandResult::Teleport {
                        x: pos.x as f64 + 0.5,
                        y: pos.y as f64,
                        z: pos.z as f64 + 0.5,
                    }
                } else {
                    CommandResult::LocateBiome {
                        biome_name: format!("{:?}", target_block),
                        x: pos.x,
                        y: pos.y,
                        z: pos.z,
                        distance,
                        direction: direction.to_string(),
                    }
                });
            }

            search.current_radius += step;
        }

        // Move to next Y level
        if search.y_dir == -1 {
            search.y_dir = 1;
        } else {
            search.y_dir = -1;
            search.y_offset += 8;
            search.current_radius = step; // Reset radius for next Y level
        }
    }

    // Search complete, not found
    if target_block == BlockType::Lava {
        Some(CommandResult::Error(format!(
            "Could not find lava within {} blocks (checked {} positions, {} mountain biomes)",
            search.max_range, search.positions_checked, search.relevant_biomes_found
        )))
    } else {
        Some(CommandResult::Error(format!(
            "Could not find {:?} block within {} blocks (checked {} positions)",
            target_block, search.max_range, search.positions_checked
        )))
    }
}

/// Update cave search for one frame
fn update_cave_search(
    search: &mut PendingLocateSearch,
    min_size: usize,
    world: &World,
    positions_this_frame: &mut usize,
) -> Option<CommandResult> {
    let start_x = search.player_pos.x;
    let start_y = search.player_pos.y;
    let start_z = search.player_pos.z;
    let step = search.step;
    let step_usize = step as usize;

    // Search underground primarily
    while search.y_offset < 256 {
        let y = start_y - search.y_offset; // Search downward

        if !(10..500).contains(&y) {
            search.y_offset += 8;
            search.current_radius = step; // Reset radius for next Y level
            continue;
        }

        // Search this Y level in spiral pattern
        while search.current_radius <= search.max_range {
            let radius = search.current_radius;

            let positions = [
                (-radius..=radius)
                    .step_by(step_usize)
                    .map(|x| Vector3::new(start_x + x, y, start_z - radius))
                    .collect::<Vec<_>>(),
                (-radius..=radius)
                    .step_by(step_usize)
                    .map(|z| Vector3::new(start_x + radius, y, start_z + z))
                    .collect::<Vec<_>>(),
                (-radius..=radius)
                    .step_by(step_usize)
                    .map(|x| Vector3::new(start_x - x, y, start_z + radius))
                    .collect::<Vec<_>>(),
                (-radius..=radius)
                    .step_by(step_usize)
                    .map(|z| Vector3::new(start_x - radius, y, start_z - z))
                    .collect::<Vec<_>>(),
            ]
            .concat();

            // Check positions for this frame
            for pos in positions {
                if *positions_this_frame >= search.positions_per_frame {
                    return None; // Continue next frame
                }

                if let Some(block) = world.get_block(pos) {
                    search.positions_checked += 1;
                    *positions_this_frame += 1;

                    if block == BlockType::Air {
                        // Found air, measure the cave size
                        let cave_size = measure_cave_size(world, pos, min_size * 2);

                        if cave_size >= min_size {
                            let dx = pos.x - start_x;
                            let dy = pos.y - start_y;
                            let dz = pos.z - start_z;
                            let distance = dx * dx + dy * dy + dz * dz;

                            if distance < search.min_distance {
                                search.min_distance = distance;
                                search.best_match = Some((pos, cave_size));
                            }
                        }
                    }
                }
            }

            // If we found something, return it
            if search.best_match.is_some() {
                let (pos, cave_size) = search.best_match.unwrap();
                let distance = ((search.min_distance as f64).sqrt()) as i32;
                let dx = pos.x - start_x;
                let dz = pos.z - start_z;

                let direction = if dx.abs() > dz.abs() {
                    if dx > 0 { "east" } else { "west" }
                } else if dz > 0 {
                    "south"
                } else {
                    "north"
                };

                return Some(if search.teleport_on_find {
                    CommandResult::Teleport {
                        x: pos.x as f64 + 0.5,
                        y: pos.y as f64,
                        z: pos.z as f64 + 0.5,
                    }
                } else {
                    CommandResult::LocateBiome {
                        biome_name: format!("Cave ({} blocks)", cave_size),
                        x: pos.x,
                        y: pos.y,
                        z: pos.z,
                        distance,
                        direction: direction.to_string(),
                    }
                });
            }

            search.current_radius += step;
        }

        // Move to next Y level
        search.y_offset += 8;
        search.current_radius = step; // Reset radius for next Y level
    }

    // Search complete, not found
    Some(CommandResult::Error(format!(
        "Could not find cave (min {} blocks) within {} blocks (checked {} positions)",
        min_size, search.max_range, search.positions_checked
    )))
}

/// Measure the size of a cave using flood-fill (limited version for async)
fn measure_cave_size(world: &World, start: Vector3<i32>, max_check: usize) -> usize {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(start);

    while let Some(pos) = queue.pop_front() {
        if visited.len() >= max_check {
            return visited.len(); // Early exit if large enough
        }

        if visited.contains(&pos) {
            continue;
        }

        // Check if this position is air
        match world.get_block(pos) {
            Some(BlockType::Air) => {
                visited.insert(pos);

                // Check 6 neighbors
                for offset in [
                    Vector3::new(1, 0, 0),
                    Vector3::new(-1, 0, 0),
                    Vector3::new(0, 1, 0),
                    Vector3::new(0, -1, 0),
                    Vector3::new(0, 0, 1),
                    Vector3::new(0, 0, -1),
                ] {
                    queue.push_back(pos + offset);
                }
            }
            _ => continue,
        }
    }

    visited.len()
}

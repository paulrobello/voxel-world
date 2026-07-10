//! Frame-distributed locate search updates.

use crate::cave_gen::CaveGenerator;
use crate::chunk::BlockType;
use crate::console::{CommandResult, LocateSearchType, PendingLocateSearch};
use crate::terrain_gen::TerrainGenerator;
use crate::world::World;
use nalgebra::Vector3;
use std::collections::{HashSet, VecDeque};

/// CON-M03(a): emit a throttled warning when a locate search encounters unloaded
/// chunks, so the user/developer is told the search is limited to already-loaded
/// terrain instead of silently truncating.
///
/// `unloaded_this_frame` is how many unloaded-chunk positions were hit this frame;
/// `positions_advanced` is how many positions the per-frame counter consumed this
/// frame (used to recover the `positions_checked` value at the start of the frame).
/// Throttled to one warning per `UNLOADED_WARN_INTERVAL` positions so a long search
/// emits only a handful of warnings instead of one per frame.
fn warn_if_unloaded_limited(
    search: &PendingLocateSearch,
    unloaded_this_frame: u32,
    positions_advanced: usize,
    kind: &str,
) {
    const UNLOADED_WARN_INTERVAL: usize = 8192;
    if unloaded_this_frame == 0 {
        return;
    }
    let prev_bucket =
        search.positions_checked.saturating_sub(positions_advanced) / UNLOADED_WARN_INTERVAL;
    let curr_bucket = search.positions_checked / UNLOADED_WARN_INTERVAL;
    if curr_bucket != prev_bucket {
        log::warn!(
            "{kind} locate search hit {unloaded} unloaded chunk position(s) near checked \
             position #{pos}; results are limited to already-loaded chunks",
            unloaded = unloaded_this_frame,
            pos = search.positions_checked,
        );
    }
}

/// CON-M03(b): whether lava lakes can spawn in `biome` under current world-gen rules.
///
/// This isolates the world-gen knowledge that this locate heuristic restricts lava
/// to mountain biomes, which otherwise lives inlined at the call site. The
/// authoritative source is `CaveGenerator::get_cave_fill` / `should_spawn_lava`; this
/// helper is a deliberately conservative predicate so search semantics are unchanged.
///
/// TODO(world-gen-api): replace this with a public predicate on the world-gen side
/// (e.g. `CaveGenerator::is_lava_biome`) when one is exposed. Editing cave_gen /
/// terrain_gen is out of scope for this single-file change; tracked as the deferred
/// remainder of CON-M03(b).
fn is_lava_spawn_biome(biome: crate::terrain_gen::BiomeType) -> bool {
    matches!(biome, crate::terrain_gen::BiomeType::Mountains)
}

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
        LocateSearchType::River => update_river_search(search, terrain, &mut positions_this_frame),
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
                // CRITICAL FIX: Move to next radius before yielding
                // Otherwise we'll re-check the same radius forever
                search.current_radius += step;
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
        if let Some((pos, _)) = search.best_match {
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
    // CON-M03(a): count unloaded-chunk positions hit this frame so we can warn.
    let mut unloaded_this_frame: u32 = 0;
    while search.y_offset < 256 {
        // CRITICAL: Check termination FIRST, before skipping Y levels
        // If we've exceeded max range, stop searching regardless of Y level
        if search.current_radius > search.max_range {
            break;
        }

        // Alternate between below and above player
        let y = start_y + (search.y_offset * search.y_dir);

        // Skip Y levels outside valid range
        let should_skip = if target_block == BlockType::Lava {
            !(2..=99).contains(&y)
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
                // Don't reset radius - let it accumulate to properly limit search distance
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
                    // CRITICAL: Increment radius before yielding, otherwise we re-check same radius forever
                    search.current_radius += step;
                    // CON-M03(a): surface that block locate only sees loaded chunks.
                    warn_if_unloaded_limited(
                        search,
                        unloaded_this_frame,
                        *positions_this_frame,
                        "block",
                    );
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

                    // CON-M03(b): route through a named helper instead of inlining the
                    // biome gate; see `is_lava_spawn_biome` for the world-gen-api TODO.
                    if !is_lava_spawn_biome(biome) {
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
                    if !cave_gen.should_spawn_lava(pos.x, biome, pos.y, pos.z) {
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
                    // For other blocks, use world.get_block (requires loaded chunks).
                    // CON-M03(a): track unloaded positions so the search can surface
                    // that it is limited to loaded chunks; `match` avoids a second
                    // `world.get_block` call just to detect `None`.
                    match world.get_block(pos) {
                        Some(block) if block == target_block => {
                            let dx = pos.x - start_x;
                            let dy = pos.y - start_y;
                            let dz = pos.z - start_z;
                            let distance = dx * dx + dy * dy + dz * dz;

                            if distance < search.min_distance {
                                search.min_distance = distance;
                                search.best_match = Some((pos, 0));
                            }
                        }
                        None => {
                            unloaded_this_frame += 1;
                        }
                        _ => {}
                    }
                }
            }

            // If we found something, return it
            if let Some((pos, _)) = search.best_match {
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
            // Don't reset radius - let it accumulate to properly limit search distance
        }
    }

    // Search complete, not found
    if target_block == BlockType::Lava {
        Some(CommandResult::Error(format!(
            "Could not find lava within {} blocks (checked {} positions, {} mountain biomes)",
            search.max_range, search.positions_checked, search.relevant_biomes_found
        )))
    } else {
        // CON-M03(a): non-lava block locate only sees loaded chunks; tell the user
        // rather than silently truncating the search.
        Some(CommandResult::Error(format!(
            "Could not find {:?} block within {} blocks (checked {} positions; \
             search limited to loaded chunks near you)",
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
    // CON-M03(a): count unloaded-chunk positions hit this frame so we can warn.
    let mut unloaded_this_frame: u32 = 0;

    // Search underground primarily
    while search.y_offset < 256 {
        // CRITICAL: Check termination FIRST, before skipping Y levels
        if search.current_radius > search.max_range {
            break;
        }

        let y = start_y - search.y_offset; // Search downward

        if !(10..500).contains(&y) {
            search.y_offset += 8;
            // Don't reset radius - let it accumulate to properly limit search distance
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
                    // CRITICAL: Increment radius before yielding, otherwise we re-check same radius forever
                    search.current_radius += step;
                    // CON-M03(a): surface that cave locate only sees loaded chunks.
                    warn_if_unloaded_limited(
                        search,
                        unloaded_this_frame,
                        *positions_this_frame,
                        "cave",
                    );
                    return None; // Continue next frame
                }

                // CON-M02: every position examined counts against the frame budget,
                // including unloaded-chunk positions. Previously the budget was only
                // consumed inside the `Some(block)` branch, so a long run of unloaded
                // chunks bypassed the per-frame cap and stalled the main thread.
                search.positions_checked += 1;
                *positions_this_frame += 1;

                if let Some(block) = world.get_block(pos) {
                    if block == BlockType::Air {
                        // Verify this is actually an underground cave, not open sky
                        // Check if there's solid terrain above (within 64 blocks)
                        let mut has_ceiling = false;
                        for check_y in (pos.y + 1)..=(pos.y + 64).min(500) {
                            if let Some(above_block) =
                                world.get_block(Vector3::new(pos.x, check_y, pos.z))
                                && above_block != BlockType::Air
                                && above_block != BlockType::Water
                                && above_block != BlockType::Lava
                            {
                                has_ceiling = true;
                                break;
                            }
                        }

                        // Only consider this a cave if there's solid terrain above
                        if has_ceiling {
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
                } else {
                    // CON-M03(a): chunk not loaded — position is unsearchable.
                    unloaded_this_frame += 1;
                }
            }

            // If we found something, return it
            if let Some((pos, cave_size)) = search.best_match {
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

                // For cave teleport, find surface above cave and place player there
                // This prevents teleporting into solid rock or floating in air
                let surface_y = if search.teleport_on_find {
                    // Search upward from cave to find surface (first solid block, then first air above it)
                    let mut y = pos.y;

                    // First, go up until we hit solid ground (exit the cave)
                    // Limit to reasonable height to avoid going out of bounds
                    while y < 480 {
                        if let Some(block) = world.get_block(Vector3::new(pos.x, y, pos.z)) {
                            if block != crate::chunk::BlockType::Air
                                && block != crate::chunk::BlockType::Water
                                && block != crate::chunk::BlockType::Lava
                            {
                                // Hit solid terrain, now find air above it
                                break;
                            }
                        } else {
                            // Block not loaded, stop here
                            break;
                        }
                        y += 1;
                    }

                    // Now find the first air block above the solid terrain (the surface)
                    while y < 480 {
                        if let Some(block) = world.get_block(Vector3::new(pos.x, y, pos.z)) {
                            if block == crate::chunk::BlockType::Air {
                                break;
                            }
                        } else {
                            // Block not loaded, stop here
                            break;
                        }
                        y += 1;
                    }

                    // Ensure we're within valid world bounds (Y: 0-511)
                    y.clamp(10, 480)
                } else {
                    pos.y
                };

                return Some(if search.teleport_on_find {
                    CommandResult::Teleport {
                        x: pos.x as f64 + 0.5,
                        y: surface_y as f64,
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
        // Don't reset radius - let it accumulate to properly limit search distance
    }

    // Search complete, not found.
    // CON-M03(a): cave locate only sees loaded chunks; tell the user instead of
    // silently truncating the search.
    Some(CommandResult::Error(format!(
        "Could not find cave (min {} blocks) within {} blocks (checked {} positions; \
         search limited to loaded chunks near you)",
        min_size, search.max_range, search.positions_checked
    )))
}

/// Update river search for one frame.
/// Uses terrain generator's river detection to find rivers without needing loaded chunks.
fn update_river_search(
    search: &mut PendingLocateSearch,
    terrain: &TerrainGenerator,
    positions_this_frame: &mut usize,
) -> Option<CommandResult> {
    let start_x = search.player_pos.x;
    let start_z = search.player_pos.z;
    let step = search.step;
    let step_usize = step as usize;

    // Search in spiral pattern (2D surface search)
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
                // Move to next radius before yielding
                search.current_radius += step;
                return None; // Continue next frame
            }

            search.positions_checked += 1;
            *positions_this_frame += 1;

            // Get terrain info to check for river
            let height = terrain.get_height(x, z);
            let biome = terrain.get_biome(x, z);

            // Use river generator to check if this is a river location
            if let Some(river_info) = terrain.river_generator().get_river_at(x, z, height, biome) {
                let dx = x - start_x;
                let dz = z - start_z;
                let distance = (dx * dx + dz * dz).abs();

                if distance < search.min_distance {
                    search.min_distance = distance;
                    // Store river type in the second element (reusing cave size field)
                    let river_type_id = match river_info.river_type {
                        crate::world_gen::rivers::RiverType::MainRiver => 1,
                        crate::world_gen::rivers::RiverType::Tributary => 2,
                        crate::world_gen::rivers::RiverType::MountainStream => 3,
                    };
                    search.best_match = Some((Vector3::new(x, height, z), river_type_id));
                }
            }
        }

        // If we found something, return it
        if let Some((pos, river_type_id)) = search.best_match {
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

            let river_type_name = match river_type_id {
                1 => "Main River",
                2 => "Tributary",
                3 => "Mountain Stream",
                _ => "River",
            };

            return Some(if search.teleport_on_find {
                CommandResult::Teleport {
                    x: pos.x as f64 + 0.5,
                    y: pos.y as f64 + 1.0, // Teleport above water
                    z: pos.z as f64 + 0.5,
                }
            } else {
                CommandResult::LocateBiome {
                    biome_name: river_type_name.to_string(),
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
        "Could not find river within {} blocks (checked {} positions)",
        search.max_range, search.positions_checked
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

#[cfg(test)]
mod tests {
    use super::update_cave_search;
    use crate::console::{LocateSearchType, PendingLocateSearch};
    use crate::world::World;
    use nalgebra::Vector3;

    fn cave_search(positions_per_frame: usize) -> PendingLocateSearch {
        PendingLocateSearch {
            search_type: LocateSearchType::Cave(5),
            player_pos: Vector3::new(0, 100, 0),
            max_range: 2000,
            current_radius: 0,
            step: 1,
            y_offset: 0,
            y_dir: -1,
            best_match: None,
            min_distance: i32::MAX,
            positions_checked: 0,
            positions_per_frame,
            relevant_biomes_found: 0,
            teleport_on_find: false,
        }
    }

    /// CON-M02: an all-unloaded world must not let the cave search bypass the
    /// per-frame budget. Before the fix, `world.get_block()` returning `None`
    /// skipped the budget counter, so the entire spiral ran in one frame and
    /// returned a "not found" error — a main-thread stall.
    #[test]
    fn cave_search_yields_on_budget_over_unloaded_chunks() {
        let mut search = cave_search(5);
        let world = World::new(); // empty world: every block is unloaded
        let mut positions_this_frame = 0usize;

        let result = update_cave_search(&mut search, 5, &world, &mut positions_this_frame);

        // The search must yield (return None) when its budget is exhausted, not
        // complete the whole spiral in a single frame.
        assert!(
            result.is_none(),
            "cave search should yield when budget exhausted, not complete in one frame"
        );
        // The budget counter must advance even though every chunk is unloaded.
        assert!(
            positions_this_frame > 0,
            "budget counter should advance over unloaded chunks"
        );
        // And it must stay at or below the per-frame cap, not the whole search space.
        assert!(
            positions_this_frame <= search.positions_per_frame,
            "budget counter ({}) should not exceed the per-frame cap ({})",
            positions_this_frame,
            search.positions_per_frame
        );
    }
}

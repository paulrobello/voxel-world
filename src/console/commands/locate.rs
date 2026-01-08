//! Locate command implementation.
//!
//! Finds the nearest occurrence of a biome, block type, or cave.

use crate::chunk::BlockType;
use crate::console::CommandResult;
use crate::terrain_gen::{BiomeType, TerrainGenerator};
use crate::world::World;
use nalgebra::Vector3;
use std::collections::{HashSet, VecDeque};

/// Execute the locate command.
///
/// Syntax:
/// - locate <biome> [range]
/// - locate <block> [range]
/// - locate cave [min_size] [range]
pub fn locate(
    args: &[&str],
    player_pos: Vector3<i32>,
    terrain: &TerrainGenerator,
    world: &World,
) -> CommandResult {
    if args.is_empty() {
        return CommandResult::Error(
            "Usage: locate <biome|block|cave> [range]\n\
             Biomes: grassland, mountains, desert, swamp, snow\n\
             Blocks: stone, dirt, water, lava, etc.\n\
             Cave: locate cave [min_size] [range]"
                .to_string(),
        );
    }

    let search_term = args[0].to_lowercase();

    // Try to parse as biome first
    if let Some(biome) = parse_biome(&search_term) {
        let range = match parse_range(args.get(1)) {
            Ok(r) => r,
            Err(e) => return e,
        };
        return locate_biome(biome, player_pos, terrain, range);
    }

    // Special handling for cave search
    if search_term == "cave" || search_term == "caves" {
        let min_size = if args.len() > 1 {
            match args[1].parse::<usize>() {
                Ok(s) if s > 0 && s <= 100000 => s,
                Ok(_) => {
                    return CommandResult::Error(
                        "Cave size must be between 1 and 100,000 blocks".to_string(),
                    );
                }
                Err(_) => 50, // Default minimum size
            }
        } else {
            50 // Default minimum cave size
        };

        let range = match parse_range(args.get(2)) {
            Ok(r) => r,
            Err(e) => return e,
        };
        return locate_cave(player_pos, world, min_size, range);
    }

    // Try to parse as block type
    match BlockType::from_name(&search_term) {
        Some(block_type) => {
            let range = match parse_range(args.get(1)) {
                Ok(r) => r,
                Err(e) => return e,
            };
            locate_block(block_type, player_pos, world, range)
        }
        None => CommandResult::Error(format!(
            "Unknown biome or block type: '{}'\n\
             Valid biomes: grassland, mountains, desert, swamp, snow\n\
             Valid blocks: {}\n\
             Or use: locate cave [min_size] [range]",
            search_term,
            BlockType::all_block_names().join(", ")
        )),
    }
}

/// Parse a biome name
fn parse_biome(name: &str) -> Option<BiomeType> {
    match name {
        "grassland" | "grass" => Some(BiomeType::Grassland),
        "mountains" | "mountain" | "mount" => Some(BiomeType::Mountains),
        "desert" => Some(BiomeType::Desert),
        "swamp" => Some(BiomeType::Swamp),
        "snow" | "tundra" | "ice" => Some(BiomeType::Snow),
        _ => None,
    }
}

/// Parse range argument
#[allow(clippy::result_large_err)]
fn parse_range(arg: Option<&&str>) -> Result<i32, CommandResult> {
    match arg {
        Some(s) => match s.parse::<i32>() {
            Ok(r) if r > 0 && r <= 16384 => Ok(r),
            Ok(_) => Err(CommandResult::Error(
                "Range must be between 1 and 16384 blocks".to_string(),
            )),
            Err(_) => Err(CommandResult::Error(format!("Invalid range: '{}'", s))),
        },
        None => Ok(2048), // Default range
    }
}

/// Locate a biome
fn locate_biome(
    target_biome: BiomeType,
    player_pos: Vector3<i32>,
    terrain: &TerrainGenerator,
    max_range: i32,
) -> CommandResult {
    let step = 16i32;
    let step_usize = step as usize;
    let mut found_pos: Option<(i32, i32)> = None;
    let mut min_distance = i32::MAX;

    let start_x = player_pos.x;
    let start_z = player_pos.z;

    // Spiral search pattern
    for radius in (step..=max_range).step_by(step_usize) {
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

        for (x, z) in positions {
            let biome = terrain.get_biome(x, z);
            if biome == target_biome {
                let dx = x - start_x;
                let dz = z - start_z;
                let distance = (dx * dx + dz * dz).abs();

                if distance < min_distance {
                    min_distance = distance;
                    found_pos = Some((x, z));
                }
            }
        }

        if found_pos.is_some() {
            break;
        }
    }

    match found_pos {
        Some((x, z)) => {
            let distance = ((min_distance as f64).sqrt()) as i32;
            let dx = x - start_x;
            let dz = z - start_z;

            let direction = if dx.abs() > dz.abs() {
                if dx > 0 { "east" } else { "west" }
            } else if dz > 0 {
                "south"
            } else {
                "north"
            };

            let y = terrain.get_height(x, z);

            CommandResult::LocateBiome {
                biome_name: format!("{:?}", target_biome),
                x,
                y,
                z,
                distance,
                direction: direction.to_string(),
            }
        }
        None => CommandResult::Error(format!(
            "Could not find {:?} biome within {} blocks",
            target_biome, max_range
        )),
    }
}

/// Locate a specific block type
fn locate_block(
    target_block: BlockType,
    player_pos: Vector3<i32>,
    world: &World,
    max_range: i32,
) -> CommandResult {
    let step = 4i32; // Check every 4 blocks for better accuracy
    let step_usize = step as usize;
    let mut found_pos: Option<Vector3<i32>> = None;
    let mut min_distance = i32::MAX;

    let start_x = player_pos.x;
    let start_y = player_pos.y;
    let start_z = player_pos.z;

    // 3D spiral search (horizontal spiral at each Y level)
    for y_offset in (0..128).step_by(8) {
        // Check levels below and above
        for &y_dir in &[-1, 1] {
            let y = start_y + (y_offset * y_dir);
            if !(0..512).contains(&y) {
                continue;
            }

            for radius in (step..=max_range).step_by(step_usize) {
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

                for pos in positions {
                    if let Some(block) = world.get_block(pos) {
                        if block == target_block {
                            let dx = pos.x - start_x;
                            let dy = pos.y - start_y;
                            let dz = pos.z - start_z;
                            let distance = dx * dx + dy * dy + dz * dz;

                            if distance < min_distance {
                                min_distance = distance;
                                found_pos = Some(pos);
                            }
                        }
                    }
                }

                if found_pos.is_some() {
                    break;
                }
            }

            if found_pos.is_some() {
                break;
            }
        }

        if found_pos.is_some() {
            break;
        }
    }

    match found_pos {
        Some(pos) => {
            let distance = ((min_distance as f64).sqrt()) as i32;
            let dx = pos.x - start_x;
            let dz = pos.z - start_z;

            let direction = if dx.abs() > dz.abs() {
                if dx > 0 { "east" } else { "west" }
            } else if dz > 0 {
                "south"
            } else {
                "north"
            };

            CommandResult::LocateBiome {
                biome_name: format!("{:?}", target_block),
                x: pos.x,
                y: pos.y,
                z: pos.z,
                distance,
                direction: direction.to_string(),
            }
        }
        None => CommandResult::Error(format!(
            "Could not find {:?} block within {} blocks",
            target_block, max_range
        )),
    }
}

/// Locate a cave (air pocket of minimum size)
fn locate_cave(
    player_pos: Vector3<i32>,
    world: &World,
    min_size: usize,
    max_range: i32,
) -> CommandResult {
    let step = 8i32; // Check every 8 blocks for caves
    let step_usize = step as usize;
    let mut found_pos: Option<Vector3<i32>> = None;
    let mut found_size = 0;
    let mut min_distance = i32::MAX;

    let start_x = player_pos.x;
    let start_y = player_pos.y;
    let start_z = player_pos.z;

    // Search underground primarily
    for y_offset in (8..256).step_by(8) {
        let y = start_y - y_offset; // Search downward
        if !(10..500).contains(&y) {
            continue;
        }

        for radius in (step..=max_range).step_by(step_usize) {
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

            for pos in positions {
                if let Some(block) = world.get_block(pos) {
                    if block == BlockType::Air {
                        // Found air, measure the cave size
                        let cave_size = measure_cave_size(world, pos, min_size * 2);

                        if cave_size >= min_size {
                            let dx = pos.x - start_x;
                            let dy = pos.y - start_y;
                            let dz = pos.z - start_z;
                            let distance = dx * dx + dy * dy + dz * dz;

                            if distance < min_distance {
                                min_distance = distance;
                                found_pos = Some(pos);
                                found_size = cave_size;
                            }
                        }
                    }
                }
            }

            if found_pos.is_some() {
                break;
            }
        }

        if found_pos.is_some() {
            break;
        }
    }

    match found_pos {
        Some(pos) => {
            let distance = ((min_distance as f64).sqrt()) as i32;
            let dx = pos.x - start_x;
            let dz = pos.z - start_z;

            let direction = if dx.abs() > dz.abs() {
                if dx > 0 { "east" } else { "west" }
            } else if dz > 0 {
                "south"
            } else {
                "north"
            };

            CommandResult::LocateBiome {
                biome_name: format!("Cave ({} blocks)", found_size),
                x: pos.x,
                y: pos.y,
                z: pos.z,
                distance,
                direction: direction.to_string(),
            }
        }
        None => CommandResult::Error(format!(
            "Could not find cave (min {} blocks) within {} blocks range",
            min_size, max_range
        )),
    }
}

/// Measure the size of a cave using flood-fill
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

//! Locate command implementation.
//!
//! Finds the nearest occurrence of a biome and reports its location.

use crate::console::CommandResult;
use crate::terrain_gen::{BiomeType, TerrainGenerator};
use nalgebra::Vector3;

/// Execute the locate command.
///
/// Syntax: locate <biome> [range]
/// Biomes: grassland, mountains, desert, swamp, snow
pub fn locate(
    args: &[&str],
    player_pos: Vector3<i32>,
    terrain: &TerrainGenerator,
) -> CommandResult {
    if args.is_empty() {
        return CommandResult::Error(
            "Usage: locate <biome> [range]\nBiomes: grassland, mountains, desert, swamp, snow"
                .to_string(),
        );
    }

    // Parse biome type
    let target_biome = match args[0].to_lowercase().as_str() {
        "grassland" | "grass" => BiomeType::Grassland,
        "mountains" | "mountain" | "mount" => BiomeType::Mountains,
        "desert" => BiomeType::Desert,
        "swamp" => BiomeType::Swamp,
        "snow" | "tundra" | "ice" => BiomeType::Snow,
        other => {
            return CommandResult::Error(format!(
                "Unknown biome: '{}'\nValid biomes: grassland, mountains, desert, swamp, snow",
                other
            ));
        }
    };

    // Parse search range (default 2048 blocks)
    let max_range = if args.len() > 1 {
        match args[1].parse::<i32>() {
            Ok(r) if r > 0 && r <= 16384 => r,
            Ok(_) => {
                return CommandResult::Error(
                    "Range must be between 1 and 16384 blocks".to_string(),
                );
            }
            Err(_) => return CommandResult::Error(format!("Invalid range: '{}'", args[1])),
        }
    } else {
        2048
    };

    // Search in expanding square spiral
    let step = 16i32; // Check every 16 blocks for speed
    let step_usize = step as usize;
    let mut found_pos: Option<(i32, i32)> = None;
    let mut min_distance = i32::MAX;

    // Start from player position
    let start_x = player_pos.x;
    let start_z = player_pos.z;

    // Spiral search pattern
    for radius in (step..=max_range).step_by(step_usize) {
        // Check the four sides of the square at this radius
        let positions = [
            // Top edge (left to right)
            (-radius..=radius)
                .step_by(step_usize)
                .map(|x| (start_x + x, start_z - radius))
                .collect::<Vec<_>>(),
            // Right edge (top to bottom)
            (-radius..=radius)
                .step_by(step_usize)
                .map(|z| (start_x + radius, start_z + z))
                .collect::<Vec<_>>(),
            // Bottom edge (right to left)
            (-radius..=radius)
                .step_by(step_usize)
                .map(|x| (start_x - x, start_z + radius))
                .collect::<Vec<_>>(),
            // Left edge (bottom to top)
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

        // If we found something, we can stop (since we're spiraling outward)
        if found_pos.is_some() {
            break;
        }
    }

    match found_pos {
        Some((x, z)) => {
            let distance = ((min_distance as f64).sqrt()) as i32;
            let dx = x - start_x;
            let dz = z - start_z;

            // Calculate direction
            let direction = if dx.abs() > dz.abs() {
                if dx > 0 { "east" } else { "west" }
            } else if dz > 0 {
                "south"
            } else {
                "north"
            };

            // Get terrain height at that location
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

//! Sphere command implementation.
//!
//! Creates a sphere of blocks at the specified center with given radius.

use crate::chunk::BlockType;
use crate::console::{
    CommandResult, parse_coordinate, validate_y_bounds, volume_confirm_threshold,
};
use crate::world::World;
use nalgebra::Vector3;

/// Execute the sphere command.
///
/// Syntax: sphere <block> <cx> <cy> <cz> <radius> [hollow] [dome]
pub fn sphere(
    args: &[&str],
    world: &mut World,
    player_pos: Vector3<i32>,
    confirmed: bool,
) -> CommandResult {
    // Parse arguments
    if args.len() < 5 {
        return CommandResult::Error(
            "Usage: sphere <block> <cx> <cy> <cz> <radius> [hollow] [dome]".to_string(),
        );
    }

    // Parse block name
    let block_name = args[0].to_lowercase();
    let block = match BlockType::from_name(&block_name) {
        Some(b) => b,
        None => {
            return CommandResult::Error(format!(
                "Unknown block type: '{}'. Valid types: {}",
                block_name,
                BlockType::all_block_names().join(", ")
            ));
        }
    };

    // Parse center coordinates
    let cx = match parse_coordinate(args[1], player_pos.x) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let cy = match parse_coordinate(args[2], player_pos.y) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };
    let cz = match parse_coordinate(args[3], player_pos.z) {
        Ok(v) => v,
        Err(e) => return CommandResult::Error(e),
    };

    // Parse radius
    let radius: i32 = match args[4].parse() {
        Ok(r) if r > 0 => r,
        Ok(_) => return CommandResult::Error("Radius must be positive".to_string()),
        Err(_) => return CommandResult::Error(format!("Invalid radius: '{}'", args[4])),
    };

    // Check for hollow and dome flags (can appear in any order after radius)
    let mut hollow = false;
    let mut dome = false;
    for arg in args.iter().skip(5) {
        match arg.to_lowercase().as_str() {
            "hollow" => hollow = true,
            "dome" => dome = true,
            _ => {}
        }
    }

    // Validate Y bounds for sphere extent
    let min_y = if dome { cy } else { cy - radius };
    let max_y = cy + radius;
    if let Some(error) = validate_y_bounds(min_y) {
        return CommandResult::Error(error);
    }
    if let Some(error) = validate_y_bounds(max_y) {
        return CommandResult::Error(error);
    }

    // Estimate volume for confirmation
    let radius_f = radius as f64;
    let mut estimated_volume = if hollow {
        // Hollow sphere shell: outer - inner (with inner radius = r-1)
        let outer_vol = (4.0 / 3.0) * std::f64::consts::PI * radius_f.powi(3);
        let inner_radius = (radius - 1).max(0) as f64;
        let inner_vol = (4.0 / 3.0) * std::f64::consts::PI * inner_radius.powi(3);
        (outer_vol - inner_vol) as u64
    } else {
        // Solid sphere volume
        ((4.0 / 3.0) * std::f64::consts::PI * radius_f.powi(3)) as u64
    };

    // Dome is roughly half the volume
    if dome {
        estimated_volume /= 2;
    }

    // Check volume threshold
    if !confirmed && estimated_volume > volume_confirm_threshold() {
        let original_cmd = args.join(" ");
        return CommandResult::NeedsConfirmation {
            message: format!(
                "This will modify approximately {} blocks. Are you sure?",
                estimated_volume
            ),
            command: format!("sphere {}", original_cmd),
        };
    }

    // Execute the sphere generation
    let radius_sq = (radius * radius) as i64;
    let inner_radius_sq = if hollow && radius > 1 {
        ((radius - 1) * (radius - 1)) as i64
    } else {
        -1 // No inner cutout
    };

    let mut count = 0u64;
    let mut changed_blocks = Vec::new();

    // For dome mode, start at center.y instead of center.y - radius
    let y_start = if dome { cy } else { cy - radius };

    for x in (cx - radius)..=(cx + radius) {
        for y in y_start..=(cy + radius) {
            for z in (cz - radius)..=(cz + radius) {
                let dx = (x - cx) as i64;
                let dy = (y - cy) as i64;
                let dz = (z - cz) as i64;
                let dist_sq = dx * dx + dy * dy + dz * dz;

                // Check if within sphere
                if dist_sq <= radius_sq {
                    let pos = Vector3::new(x, y, z);
                    if hollow {
                        // For hollow sphere, only place on the shell
                        if dist_sq > inner_radius_sq {
                            world.set_block(pos, block);
                            changed_blocks.push((pos, block));
                            count += 1;
                        } else {
                            // Clear interior
                            world.set_block(pos, BlockType::Air);
                            changed_blocks.push((pos, BlockType::Air));
                        }
                    } else {
                        world.set_block(pos, block);
                        changed_blocks.push((pos, block));
                        count += 1;
                    }
                }
            }
        }
    }

    let hollow_str = if hollow { " hollow" } else { "" };
    let dome_str = if dome { " dome" } else { "" };
    CommandResult::success_with_blocks(
        format!(
            "Created{}{} sphere of {} blocks with {:?}",
            hollow_str, dome_str, count, block
        ),
        changed_blocks,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_sphere_volume_estimate() {
        // Solid sphere with radius 5: ~523 blocks
        let radius = 5.0_f64;
        let volume = (4.0 / 3.0) * std::f64::consts::PI * radius.powi(3);
        assert!((volume - 523.6).abs() < 1.0);
    }

    #[test]
    fn test_hollow_sphere_shell_volume() {
        // Hollow sphere shell: outer(r=5) - inner(r=4)
        let outer = (4.0 / 3.0) * std::f64::consts::PI * 5.0_f64.powi(3);
        let inner = (4.0 / 3.0) * std::f64::consts::PI * 4.0_f64.powi(3);
        let shell = outer - inner;
        // Shell should be around 255 blocks
        assert!((shell - 255.5).abs() < 1.0);
    }

    #[test]
    fn test_distance_squared() {
        // Point at (3, 4, 0) from origin has distance 5, dist_sq = 25
        let dx: i64 = 3;
        let dy: i64 = 4;
        let dz: i64 = 0;
        let dist_sq = dx * dx + dy * dy + dz * dz;
        assert_eq!(dist_sq, 25);
    }
}

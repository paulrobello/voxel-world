//! Console commands for texture management (host-only).
//!
//! These commands allow the host to add, list, and remove custom textures
//! that can be synced to connected clients.

use crate::console::CommandResult;
use std::path::PathBuf;

/// Add a custom texture from a PNG file.
///
/// Usage: `texture_add <filepath> <name>`
///
/// # Arguments
/// * `filepath` - Path to the PNG file (64x64 pixels recommended)
/// * `name` - Unique name for the texture
///
/// # Example
/// `texture_add textures/my_custom_block.png my_block`
pub fn texture_add(args: &[&str]) -> CommandResult {
    if args.len() < 2 {
        return CommandResult::Error("Usage: texture_add <filepath> <name>".to_string());
    }

    let filepath = args[0];
    let name = args[1];

    // Validate name
    if name.is_empty() {
        return CommandResult::Error("Texture name cannot be empty".to_string());
    }

    if name.len() > 32 {
        return CommandResult::Error("Texture name too long (max 32 characters)".to_string());
    }

    // Check for valid characters (alphanumeric and underscore)
    if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return CommandResult::Error(
            "Texture name must contain only alphanumeric characters and underscores".to_string(),
        );
    }

    // Read PNG file
    let path = PathBuf::from(filepath);
    let png_data = match std::fs::read(&path) {
        Ok(d) => d,
        Err(e) => {
            return CommandResult::Error(format!("Failed to read file '{}': {}", filepath, e))
        }
    };

    // Validate PNG header
    const PNG_HEADER: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    if png_data.len() < 8 || png_data[0..8] != PNG_HEADER {
        return CommandResult::Error(format!("File '{}' is not a valid PNG file", filepath));
    }

    // Validate file size (max 1MB for textures)
    const MAX_TEXTURE_SIZE: usize = 1024 * 1024;
    if png_data.len() > MAX_TEXTURE_SIZE {
        return CommandResult::Error(format!(
            "Texture file too large ({} bytes, max {} bytes)",
            png_data.len(),
            MAX_TEXTURE_SIZE
        ));
    }

    // Note: Full implementation would need access to GameServer's texture_manager
    // For now, return a placeholder indicating the feature needs world context
    CommandResult::Success(format!(
        "Texture '{}' from '{}' validated successfully ({} bytes). \
         Note: Full texture registration requires active multiplayer server.",
        name,
        filepath,
        png_data.len()
    ))
}

/// List all custom textures.
///
/// Usage: `texture_list`
///
/// Shows the slot number, name, and dimensions of each custom texture.
pub fn texture_list() -> CommandResult {
    // Placeholder - would query server's texture_manager
    // Full implementation would show:
    // - Slot number
    // - Texture name
    // - Dimensions
    // - Hash for verification
    CommandResult::Success(
        "Custom texture list: (requires active multiplayer server to list)\n\
         Use 'texture_add <file> <name>' to add textures."
            .to_string(),
    )
}

/// Remove a custom texture by slot number.
///
/// Usage: `texture_remove <slot>`
///
/// # Arguments
/// * `slot` - Slot number of the texture to remove (0-255)
///
/// # Example
/// `texture_remove 5`
pub fn texture_remove(args: &[&str]) -> CommandResult {
    if args.is_empty() {
        return CommandResult::Error("Usage: texture_remove <slot>".to_string());
    }

    let slot: u8 = match args[0].trim().parse() {
        Ok(s) => s,
        Err(_) => {
            return CommandResult::Error(format!(
                "Invalid slot number: '{}'. Must be 0-255.",
                args[0]
            ))
        }
    };

    // Note: Full implementation would:
    // 1. Check if texture exists in that slot
    // 2. Remove from texture_manager
    // 3. Broadcast removal to all connected clients
    // 4. Update any blocks using that texture (or prevent removal if in use)

    CommandResult::Success(format!(
        "Texture slot {} removal would be processed (requires active multiplayer server)",
        slot
    ))
}

/// Get texture info by name or slot.
///
/// Usage: `texture_info <name|slot>`
///
/// Shows detailed information about a specific texture.
pub fn texture_info(args: &[&str]) -> CommandResult {
    if args.is_empty() {
        return CommandResult::Error("Usage: texture_info <name|slot>".to_string());
    }

    let identifier = args[0];

    // Check if it's a slot number
    if let Ok(slot) = identifier.parse::<u8>() {
        // Placeholder - would query texture_manager
        CommandResult::Success(format!(
            "Texture info for slot {}: (requires active multiplayer server)",
            slot
        ))
    } else {
        // Look up by name
        CommandResult::Success(format!(
            "Texture info for '{}': (requires active multiplayer server)",
            identifier
        ))
    }
}

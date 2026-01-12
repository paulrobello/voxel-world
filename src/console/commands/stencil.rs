//! Stencil command implementation for saving/loading/managing stencils.

use crate::console::CommandResult;
use crate::stencils::{StencilFile, StencilLibrary};
use crate::templates::TemplateSelection;
use crate::world::World;

/// Handles /stencil command with subcommands: create, load, list, delete, active, clear, opacity, mode
pub fn stencil(
    args: &[&str],
    selection: &TemplateSelection,
    world: &World,
    library: &StencilLibrary,
    confirmed: bool,
) -> CommandResult {
    if args.is_empty() {
        return CommandResult::Error(
            "Usage: /stencil create|load|list|delete|active|clear|opacity|mode <args>".to_string(),
        );
    }

    let subcommand = args[0];

    match subcommand {
        "create" => stencil_create(args, selection, world, library, confirmed),
        "load" => stencil_load(args, library),
        "list" => stencil_list(library),
        "delete" => stencil_delete(args, library, confirmed),
        "active" => stencil_active(),
        "clear" => stencil_clear(),
        "opacity" => stencil_opacity(args),
        "mode" => stencil_mode(args),
        "remove" => stencil_remove(args),
        _ => CommandResult::Error(format!(
            "Unknown subcommand '{}'. Use: create, load, list, delete, active, clear, opacity, mode, remove",
            subcommand
        )),
    }
}

/// Creates a stencil from the current selection.
fn stencil_create(
    args: &[&str],
    selection: &TemplateSelection,
    world: &World,
    library: &StencilLibrary,
    confirmed: bool,
) -> CommandResult {
    if args.len() < 2 {
        return CommandResult::Error("Usage: /stencil create <name> [tags...]".to_string());
    }

    let name = args[1].to_string();

    // Validate selection
    let (min, max) = match selection.bounds() {
        Some(bounds) => bounds,
        None => {
            return CommandResult::Error(
                "No selection! Use /select pos1 and /select pos2 first".to_string(),
            );
        }
    };

    // Validate size
    if let Err(e) = selection.validate_size() {
        return CommandResult::Error(format!("Invalid selection: {}", e));
    }

    // Parse tags (remaining args)
    let tags: Vec<String> = args[2..].iter().map(|s| s.to_string()).collect();

    // Check if stencil exists (skip if already confirmed)
    if !confirmed && library.stencil_exists(&name) {
        return CommandResult::NeedsConfirmation {
            message: format!("Stencil '{}' already exists. Overwrite? (yes/no)", name),
            command: format!("stencil create {} {}", name, tags.join(" ")),
        };
    }

    // Create stencil from world region
    let author = "Player".to_string(); // TODO: Get from user prefs
    let stencil = match StencilFile::from_world_region(world, name.clone(), author, min, max) {
        Ok(s) => s,
        Err(e) => return CommandResult::Error(format!("Failed to create stencil: {}", e)),
    };

    // Add tags
    let mut stencil = stencil;
    stencil.tags = tags;

    let position_count = stencil.position_count();

    // Save to library
    match library.save_stencil(&stencil) {
        Ok(_) => {
            // Generate thumbnail
            let thumbnail_path = library.get_thumbnail_path(&name);
            if let Err(e) =
                crate::stencils::rasterizer::generate_stencil_thumbnail(&stencil, &thumbnail_path)
            {
                eprintln!("[Stencil] Warning: Failed to generate thumbnail: {}", e);
            }

            let (w, h, d) = selection.dimensions().unwrap();
            CommandResult::Success(format!(
                "Saved stencil '{}' ({}×{}×{}, {} positions)",
                name, w, h, d, position_count
            ))
        }
        Err(e) => CommandResult::Error(format!("Failed to save stencil: {}", e)),
    }
}

/// Loads a stencil for placement.
fn stencil_load(args: &[&str], library: &StencilLibrary) -> CommandResult {
    if args.len() < 2 {
        return CommandResult::Error("Usage: /stencil load <name>".to_string());
    }

    let name = args[1];

    match library.load_stencil(name) {
        Ok(stencil) => CommandResult::LoadStencil(stencil),
        Err(e) => CommandResult::Error(format!("Failed to load stencil '{}': {}", name, e)),
    }
}

/// Lists all available stencils.
fn stencil_list(library: &StencilLibrary) -> CommandResult {
    match library.list_stencils() {
        Ok(stencils) => {
            if stencils.is_empty() {
                CommandResult::Success("No stencils found".to_string())
            } else {
                let mut output = format!("Stencils ({}):\n", stencils.len());
                for name in stencils {
                    output.push_str(&format!("  - {}\n", name));
                }
                CommandResult::Success(output)
            }
        }
        Err(e) => CommandResult::Error(format!("Failed to list stencils: {}", e)),
    }
}

/// Deletes a stencil.
fn stencil_delete(args: &[&str], library: &StencilLibrary, confirmed: bool) -> CommandResult {
    if args.len() < 2 {
        return CommandResult::Error("Usage: /stencil delete <name>".to_string());
    }

    let name = args[1];

    if !library.stencil_exists(name) {
        return CommandResult::Error(format!("Stencil '{}' not found", name));
    }

    // Request confirmation if not already confirmed
    if !confirmed {
        return CommandResult::NeedsConfirmation {
            message: format!("Delete stencil '{}'? This cannot be undone. (yes/no)", name),
            command: format!("stencil delete {}", name),
        };
    }

    // Confirmed - perform deletion
    match library.delete_stencil(name) {
        Ok(_) => CommandResult::Success(format!("Deleted stencil '{}'", name)),
        Err(e) => CommandResult::Error(format!("Failed to delete stencil: {}", e)),
    }
}

/// Shows active stencils with their IDs.
fn stencil_active() -> CommandResult {
    // This needs access to StencilManager which is in UI state.
    // Return a special result that the console handler will fill in.
    CommandResult::ListActiveStencils
}

/// Clears all active stencils.
fn stencil_clear() -> CommandResult {
    CommandResult::ClearStencils
}

/// Sets global stencil opacity.
fn stencil_opacity(args: &[&str]) -> CommandResult {
    if args.len() < 2 {
        return CommandResult::Error("Usage: /stencil opacity <0.3-0.8>".to_string());
    }

    let value: f32 = match args[1].parse() {
        Ok(v) => v,
        Err(_) => return CommandResult::Error("Invalid opacity value. Use 0.3 to 0.8".to_string()),
    };

    if !(0.3..=0.8).contains(&value) {
        return CommandResult::Error("Opacity must be between 0.3 and 0.8".to_string());
    }

    CommandResult::SetStencilOpacity(value)
}

/// Sets stencil render mode.
fn stencil_mode(args: &[&str]) -> CommandResult {
    if args.len() < 2 {
        return CommandResult::Error("Usage: /stencil mode <wireframe|solid>".to_string());
    }

    let mode = args[1].to_lowercase();
    match mode.as_str() {
        "wireframe" | "wire" => CommandResult::SetStencilRenderMode(0),
        "solid" => CommandResult::SetStencilRenderMode(1),
        _ => CommandResult::Error("Mode must be 'wireframe' or 'solid'".to_string()),
    }
}

/// Removes a specific active stencil by ID.
fn stencil_remove(args: &[&str]) -> CommandResult {
    if args.len() < 2 {
        return CommandResult::Error("Usage: /stencil remove <id>".to_string());
    }

    let id: u64 = match args[1].parse() {
        Ok(v) => v,
        Err(_) => return CommandResult::Error("Invalid stencil ID. Use a number.".to_string()),
    };

    CommandResult::RemoveStencil(id)
}

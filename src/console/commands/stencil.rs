//! Stencil command implementation for saving/loading/managing stencils.

use crate::console::CommandResult;
use crate::stencils::{StencilFile, StencilLibrary};
use crate::templates::{TemplateLibrary, TemplateSelection};
use crate::world::World;

/// Handles /stencil command with subcommands: create, load, list, delete, active, clear, opacity, mode, from-template
pub fn stencil(
    args: &[&str],
    selection: &TemplateSelection,
    world: &World,
    library: &StencilLibrary,
    template_library: &TemplateLibrary,
    confirmed: bool,
    author: &str,
) -> CommandResult {
    if args.is_empty() {
        return CommandResult::Error(
            "Usage: /stencil create|load|list|delete|active|clear|opacity|mode|from-template <args>".to_string(),
        );
    }

    let subcommand = args[0];

    match subcommand {
        "create" => stencil_create(args, selection, world, library, confirmed, author),
        "load" => stencil_load(args, library),
        "list" => stencil_list(library),
        "delete" => stencil_delete(args, library, confirmed),
        "active" => stencil_active(),
        "clear" => stencil_clear(),
        "opacity" => stencil_opacity(args),
        "mode" => stencil_mode(args),
        "remove" => stencil_remove(args),
        "from-template" => stencil_from_template(args, library, template_library, confirmed),
        _ => CommandResult::Error(format!(
            "Unknown subcommand '{}'. Use: create, load, list, delete, active, clear, opacity, mode, remove, from-template",
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
    author: &str,
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
    let stencil =
        match StencilFile::from_world_region(world, name.clone(), author.to_string(), min, max) {
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
                log::warn!("[Stencil] Warning: Failed to generate thumbnail: {}", e);
            }

            let (w, h, d) = selection.dimensions().unwrap();
            CommandResult::success(format!(
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
                CommandResult::success("No stencils found")
            } else {
                let mut output = format!("Stencils ({}):\n", stencils.len());
                for name in stencils {
                    output.push_str(&format!("  - {}\n", name));
                }
                CommandResult::success(output)
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
        Ok(_) => CommandResult::success(format!("Deleted stencil '{}'", name)),
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

/// Creates a stencil from an existing template.
fn stencil_from_template(
    args: &[&str],
    stencil_library: &StencilLibrary,
    template_library: &crate::templates::TemplateLibrary,
    confirmed: bool,
) -> CommandResult {
    if args.len() < 2 {
        return CommandResult::Error(
            "Usage: /stencil from-template <template-name> [stencil-name]".to_string(),
        );
    }

    let template_name = args[1];
    let stencil_name = if args.len() >= 3 {
        Some(args[2].to_string())
    } else {
        None // Will use template name with "-stencil" suffix
    };

    // Load the template
    let template = match template_library.load_template(template_name) {
        Ok(t) => t,
        Err(e) => {
            return CommandResult::Error(format!(
                "Failed to load template '{}': {}",
                template_name, e
            ));
        }
    };

    // Determine final stencil name
    let final_name = stencil_name
        .clone()
        .unwrap_or_else(|| format!("{}-stencil", template.name));

    // Check if stencil exists (skip if already confirmed)
    if !confirmed && stencil_library.stencil_exists(&final_name) {
        return CommandResult::NeedsConfirmation {
            message: format!(
                "Stencil '{}' already exists. Overwrite? (yes/no)",
                final_name
            ),
            command: if args.len() >= 3 {
                format!("stencil from-template {} {}", template_name, args[2])
            } else {
                format!("stencil from-template {}", template_name)
            },
        };
    }

    // Convert template to stencil
    let stencil = StencilFile::from_template(&template, stencil_name);
    let position_count = stencil.position_count();

    // Save to library
    match stencil_library.save_stencil(&stencil) {
        Ok(_) => {
            // Generate thumbnail
            let thumbnail_path = stencil_library.get_thumbnail_path(&final_name);
            if let Err(e) =
                crate::stencils::rasterizer::generate_stencil_thumbnail(&stencil, &thumbnail_path)
            {
                log::warn!("[Stencil] Warning: Failed to generate thumbnail: {}", e);
            }

            CommandResult::success(format!(
                "Created stencil '{}' from template '{}' ({}×{}×{}, {} positions)",
                final_name,
                template.name,
                stencil.width,
                stencil.height,
                stencil.depth,
                position_count
            ))
        }
        Err(e) => CommandResult::Error(format!("Failed to save stencil: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    //! Tests for the /stencil subcommands whose effects are wired through
    //! typed `CommandResult` variants (CON-003): `opacity`, `mode`, and
    //! `active`. These subcommands early-return without touching `World`
    //! or the libraries, so stubs are sufficient.
    use super::*;
    use crate::console::CommandResult;
    use crate::templates::TemplateSelection;
    use crate::world::World;

    fn libs() -> (StencilLibrary, TemplateLibrary) {
        // Path is only stored; no I/O happens until save. Unique per-process
        // so parallel test runs never collide.
        let dir =
            std::env::temp_dir().join(format!("voxel-world-stencil-test-{}", std::process::id()));
        (StencilLibrary::new(dir.clone()), TemplateLibrary::new(dir))
    }

    fn run(args: &[&str]) -> CommandResult {
        let (stencil_lib, template_lib) = libs();
        stencil(
            args,
            &TemplateSelection::new(),
            &World::new(),
            &stencil_lib,
            &template_lib,
            false,
            "tester",
        )
    }

    #[test]
    fn test_stencil_opacity_valid_value() {
        // CON-003: /stencil opacity must emit SetStencilOpacity so the
        // render.rs drain can apply it to StencilManager.global_opacity.
        match run(&["opacity", "0.5"]) {
            CommandResult::SetStencilOpacity(v) => assert_eq!(v, 0.5),
            _ => panic!("expected SetStencilOpacity(0.5)"),
        }
    }

    #[test]
    fn test_stencil_opacity_below_range_errors() {
        assert!(matches!(run(&["opacity", "0.2"]), CommandResult::Error(_)));
    }

    #[test]
    fn test_stencil_opacity_above_range_errors() {
        assert!(matches!(run(&["opacity", "0.9"]), CommandResult::Error(_)));
    }

    #[test]
    fn test_stencil_opacity_non_numeric_errors() {
        assert!(matches!(run(&["opacity", "foo"]), CommandResult::Error(_)));
    }

    #[test]
    fn test_stencil_opacity_missing_arg_errors() {
        assert!(matches!(run(&["opacity"]), CommandResult::Error(_)));
    }

    #[test]
    fn test_stencil_mode_wireframe_emits_zero() {
        // CON-003: 0 = wireframe, drained into StencilRenderMode::Wireframe.
        match run(&["mode", "wireframe"]) {
            CommandResult::SetStencilRenderMode(m) => assert_eq!(m, 0),
            _ => panic!("expected SetStencilRenderMode(0)"),
        }
    }

    #[test]
    fn test_stencil_mode_wire_alias() {
        match run(&["mode", "wire"]) {
            CommandResult::SetStencilRenderMode(m) => assert_eq!(m, 0),
            _ => panic!("expected SetStencilRenderMode(0) for 'wire'"),
        }
    }

    #[test]
    fn test_stencil_mode_solid_emits_one() {
        match run(&["mode", "solid"]) {
            CommandResult::SetStencilRenderMode(m) => assert_eq!(m, 1),
            _ => panic!("expected SetStencilRenderMode(1)"),
        }
    }

    #[test]
    fn test_stencil_mode_invalid_errors() {
        assert!(matches!(
            run(&["mode", "frobnicate"]),
            CommandResult::Error(_)
        ));
    }

    #[test]
    fn test_stencil_mode_missing_arg_errors() {
        assert!(matches!(run(&["mode"]), CommandResult::Error(_)));
    }

    #[test]
    fn test_stencil_active_emits_list_marker() {
        // CON-003: /stencil active must emit ListActiveStencils so the
        // render.rs drain can read StencilManager.list_active() and print it.
        assert!(matches!(
            run(&["active"]),
            CommandResult::ListActiveStencils
        ));
    }

    #[test]
    fn test_stencil_no_subcommand_errors() {
        // Bare `/stencil` should guide the user, not silently no-op.
        assert!(matches!(run(&[]), CommandResult::Error(_)));
    }

    #[test]
    fn test_stencil_unknown_subcommand_errors() {
        assert!(matches!(run(&["frobnicate"]), CommandResult::Error(_)));
    }
}

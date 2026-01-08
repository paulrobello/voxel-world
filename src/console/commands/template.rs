//! Template command implementation for saving/loading/managing templates.

use crate::console::CommandResult;
use crate::templates::{TemplateLibrary, TemplateSelection, VxtFile};
use crate::water::WaterGrid;
use crate::world::World;

/// Handles /template command with subcommands: save, load, list, delete, info
#[allow(dead_code)] // TODO: Remove once integrated with main.rs
pub fn template(
    args: &[&str],
    selection: &TemplateSelection,
    world: &World,
    water_grid: &WaterGrid,
    library: &TemplateLibrary,
    confirmed: bool,
) -> CommandResult {
    if args.is_empty() {
        return CommandResult::Error(
            "Usage: /template save|load|list|delete|info <name> [tags...]".to_string(),
        );
    }

    let subcommand = args[0];

    match subcommand {
        "save" => template_save(args, selection, world, water_grid, library, confirmed),
        "load" => template_load(args, library),
        "list" => template_list(library),
        "delete" => template_delete(args, library, confirmed),
        "info" => template_info(args, library),
        _ => CommandResult::Error(format!(
            "Unknown subcommand '{}'. Use: save, load, list, delete, or info",
            subcommand
        )),
    }
}

/// Saves the current selection as a template
fn template_save(
    args: &[&str],
    selection: &TemplateSelection,
    world: &World,
    water_grid: &WaterGrid,
    library: &TemplateLibrary,
    confirmed: bool,
) -> CommandResult {
    if args.len() < 2 {
        return CommandResult::Error("Usage: /template save <name> [tags...]".to_string());
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

    // Check if template exists (skip if already confirmed)
    if !confirmed && library.template_exists(&name) {
        return CommandResult::NeedsConfirmation {
            message: format!("Template '{}' already exists. Overwrite? (yes/no)", name),
            command: format!("template save {} {}", name, tags.join(" ")),
        };
    }

    // Create template from world region
    let author = "Player".to_string(); // TODO: Get from user prefs
    let template =
        match VxtFile::from_world_region(world, water_grid, name.clone(), author, min, max) {
            Ok(t) => t,
            Err(e) => return CommandResult::Error(format!("Failed to create template: {}", e)),
        };

    // Add tags
    let mut template = template;
    template.tags = tags;

    // Save to library
    match library.save_template(&template) {
        Ok(_) => {
            // Generate thumbnail
            let thumbnail_path = library.get_thumbnail_path(&name);
            if let Err(e) = crate::templates::rasterizer::generate_template_thumbnail(
                &template,
                &thumbnail_path,
            ) {
                eprintln!("[Template] Warning: Failed to generate thumbnail: {}", e);
                // Don't fail the save operation if thumbnail generation fails
            }

            let (w, h, d) = selection.dimensions().unwrap();
            CommandResult::Success(format!(
                "Saved template '{}' ({}×{}×{}, {} blocks)",
                name,
                w,
                h,
                d,
                template.block_count()
            ))
        }
        Err(e) => CommandResult::Error(format!("Failed to save template: {}", e)),
    }
}

/// Loads a template for placement
fn template_load(args: &[&str], library: &TemplateLibrary) -> CommandResult {
    if args.len() < 2 {
        return CommandResult::Error("Usage: /template load <name>".to_string());
    }

    let name = args[1];

    match library.load_template(name) {
        Ok(template) => CommandResult::LoadTemplate(template),
        Err(e) => CommandResult::Error(format!("Failed to load template '{}': {}", name, e)),
    }
}

/// Lists all available templates
fn template_list(library: &TemplateLibrary) -> CommandResult {
    match library.list_templates() {
        Ok(templates) => {
            if templates.is_empty() {
                CommandResult::Success("No templates found".to_string())
            } else {
                let mut output = format!("Templates ({}):\n", templates.len());
                for name in templates {
                    output.push_str(&format!("  - {}\n", name));
                }
                CommandResult::Success(output)
            }
        }
        Err(e) => CommandResult::Error(format!("Failed to list templates: {}", e)),
    }
}

/// Deletes a template
fn template_delete(args: &[&str], library: &TemplateLibrary, confirmed: bool) -> CommandResult {
    if args.len() < 2 {
        return CommandResult::Error("Usage: /template delete <name>".to_string());
    }

    let name = args[1];

    if !library.template_exists(name) {
        return CommandResult::Error(format!("Template '{}' not found", name));
    }

    // Request confirmation if not already confirmed
    if !confirmed {
        return CommandResult::NeedsConfirmation {
            message: format!(
                "Delete template '{}'? This cannot be undone. (yes/no)",
                name
            ),
            command: format!("template delete {}", name),
        };
    }

    // Confirmed - perform deletion
    match library.delete_template(name) {
        Ok(_) => CommandResult::Success(format!("Deleted template '{}'", name)),
        Err(e) => CommandResult::Error(format!("Failed to delete template: {}", e)),
    }
}

/// Shows template information
fn template_info(args: &[&str], library: &TemplateLibrary) -> CommandResult {
    if args.len() < 2 {
        return CommandResult::Error("Usage: /template info <name>".to_string());
    }

    let name = args[1];

    match library.get_template_info(name) {
        Ok(info) => {
            let mut output = format!("Template: {}\n", info.name);
            output.push_str(&format!("Author: {}\n", info.author));
            output.push_str(&format!(
                "Dimensions: {} ({})\n",
                info.dimensions_str(),
                info.volume_str()
            ));
            output.push_str(&format!("Blocks: {}\n", info.block_count_str()));
            output.push_str(&format!("Created: {}\n", info.creation_date_str()));
            if !info.tags.is_empty() {
                output.push_str(&format!("Tags: {}\n", info.tags.join(", ")));
            }
            CommandResult::Success(output)
        }
        Err(e) => CommandResult::Error(format!("Failed to get template info: {}", e)),
    }
}

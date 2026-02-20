//! Picture frame console commands.

use crate::console::CommandResult;
use crate::pictures::PictureLibrary;

/// Handles picture frame commands: list, set <id>, clear, debug
pub fn picture(args: &[&str], picture_library: &PictureLibrary) -> CommandResult {
    if args.is_empty() {
        return CommandResult::Error(
            "Usage: /frame picture list|set|clear|debug [args]".to_string(),
        );
    }

    let subcommand = args[0].to_lowercase();

    match subcommand.as_str() {
        "list" => picture_list(picture_library),
        "set" => {
            if args.len() < 2 {
                return CommandResult::Error("Usage: /frame picture set <id>".to_string());
            }
            let id_str = args[1];
            match id_str.parse::<u32>() {
                Ok(id) => picture_set(id, picture_library),
                Err(_) => CommandResult::Error(format!("Invalid picture ID: {}", id_str)),
            }
        }
        "clear" => picture_clear(),
        "debug" => picture_debug(picture_library),
        _ => CommandResult::Error(format!(
            "Unknown subcommand '{}'. Use: list, set, clear, debug",
            subcommand
        )),
    }
}

/// List all pictures in the library.
fn picture_list(picture_library: &PictureLibrary) -> CommandResult {
    let mut pictures: Vec<_> = picture_library.iter().collect();
    pictures.sort_by_key(|p| p.id);

    if pictures.is_empty() {
        return CommandResult::success(
            "No pictures saved. Create one in the texture editor (P key)",
        );
    }

    let mut output = String::from("Available pictures:\n");
    for pic in &pictures {
        output.push_str(&format!(
            "  [{}] {} ({}×{}){}\n",
            pic.id,
            pic.name,
            pic.width,
            pic.height,
            if pic.width == pic.height {
                match pic.width {
                    64 => " → fits 1×1 frame",
                    128 => " → fits 1×1, use 2× for 2×2, 3×3 for 3×3 clusters",
                    256 => " → use 2×2 or 3×3 for cluster",
                    384 => " → fits 3×3 cluster",
                    _ => "",
                }
            } else {
                " (non-square)"
            }
        ));
    }

    output.push_str("\nCluster picture size guide:\n");
    output.push_str("  1×1 frame: 128×128 picture\n");
    output.push_str("  2×2 frames: 256×256 picture (each frame shows 128×128 quadrant)\n");
    output.push_str("  3×3 frames: 384×384 picture (each frame shows 128×128 region)\n");

    CommandResult::success(output)
}

/// Set the selected picture for frame placement.
fn picture_set(id: u32, picture_library: &PictureLibrary) -> CommandResult {
    if let Some(picture) = picture_library.get(id) {
        CommandResult::SetPictureSelection {
            id,
            name: picture.name.clone(),
        }
    } else {
        CommandResult::Error(format!(
            "Picture ID {} not found. Use '/frame picture list' to see available pictures.",
            id
        ))
    }
}

/// Clear the selected picture (frames will be empty).
fn picture_clear() -> CommandResult {
    CommandResult::SetPictureSelection {
        id: 0,
        name: "None".to_string(),
    }
}

/// Show cluster picture size guide.
fn picture_debug(picture_library: &PictureLibrary) -> CommandResult {
    let _ = picture_library; // Currently unused, but kept for future expansion
    let output = "Cluster picture size guide:\n\
  1×1 frame: 128×128 picture\n\
  2×2 frames: 256×256 picture (each frame shows 128×128 quadrant)\n\
  3×3 frames: 384×384 picture (each frame shows 128×128 region)\n\
\n\
Note: Texture editor creates 64×64 pictures. For larger clusters,\n\
import external images or create larger pictures with other tools.";

    CommandResult::success(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_picture_list_empty() {
        let library = PictureLibrary::new();
        let result = picture_list(&library);
        assert!(matches!(result, CommandResult::Success { .. }));
    }

    #[test]
    fn test_picture_clear_command() {
        let result = picture_clear();
        assert!(matches!(
            result,
            CommandResult::SetPictureSelection { id: 0, name } if name == "None"
        ));
    }

    #[test]
    fn test_picture_invalid_subcommand() {
        let library = PictureLibrary::new();
        let result = picture(&["invalid"], &library);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    #[test]
    fn test_picture_set_missing_id() {
        let library = PictureLibrary::new();
        let result = picture(&["set"], &library);
        assert!(matches!(result, CommandResult::Error(_)));
    }

    #[test]
    fn test_picture_set_invalid_id() {
        let library = PictureLibrary::new();
        let result = picture(&["set", "abc"], &library);
        assert!(matches!(result, CommandResult::Error(_)));
    }
}

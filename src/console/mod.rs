//! In-game command console for world editing operations.
//!
//! Toggle the console with `/` key. Supports commands like:
//! - `fill <block> <x1> <y1> <z1> <x2> <y2> <z2> [hollow]`
//! - `help` - List available commands

pub mod commands;

use crate::chunk::CHUNK_SIZE;
use crate::constants::WORLD_CHUNKS_Y;
use crate::world::World;
use egui_winit_vulkano::egui;
use nalgebra::Vector3;

/// Maximum number of output lines to keep in history.
const MAX_OUTPUT_LINES: usize = 100;

/// Volume threshold requiring confirmation before execution.
const VOLUME_CONFIRM_THRESHOLD: u64 = 100_000;

/// Maximum Y coordinate (world height).
const MAX_Y: i32 = WORLD_CHUNKS_Y * CHUNK_SIZE as i32 - 1;

/// Entry type for console output with color coding.
#[derive(Clone)]
pub enum ConsoleEntry {
    /// Informational message (gray).
    Info(String),
    /// Success message (green).
    Success(String),
    /// Error message (red).
    Error(String),
    /// Warning message (yellow).
    Warning(String),
}

impl ConsoleEntry {
    /// Returns the text content of this entry.
    pub fn text(&self) -> &str {
        match self {
            ConsoleEntry::Info(s)
            | ConsoleEntry::Success(s)
            | ConsoleEntry::Error(s)
            | ConsoleEntry::Warning(s) => s,
        }
    }

    /// Returns the color for this entry type.
    pub fn color(&self) -> egui::Color32 {
        match self {
            ConsoleEntry::Info(_) => egui::Color32::from_gray(180),
            ConsoleEntry::Success(_) => egui::Color32::from_rgb(100, 255, 100),
            ConsoleEntry::Error(_) => egui::Color32::from_rgb(255, 100, 100),
            ConsoleEntry::Warning(_) => egui::Color32::from_rgb(255, 200, 100),
        }
    }
}

/// Result of command execution.
#[allow(clippy::result_large_err)]
pub enum CommandResult {
    /// Command executed successfully.
    Success(String),
    /// Command failed with error message.
    Error(String),
    /// Command needs user confirmation before execution.
    NeedsConfirmation { message: String, command: String },
    /// Teleport player to coordinates.
    Teleport { x: f64, y: f64, z: f64 },
    /// Request water/lava debug info output (caller has access to grids).
    FluidDebug,
    /// Force all water cells to become active (for debugging stuck water).
    ForceWaterActive,
    /// Analyze water flow at player position.
    WaterAnalyze,
    /// Enable/disable biome debug visualization.
    SetBiomeDebug(bool),
    /// Load template for placement.
    LoadTemplate(crate::templates::VxtFile),
    /// Biome location found.
    LocateBiome {
        biome_name: String,
        x: i32,
        y: i32,
        z: i32,
        distance: i32,
        direction: String,
    },
    /// Start an asynchronous locate search.
    StartLocateSearch(PendingLocateSearch),
}

/// Pending command awaiting confirmation.
#[derive(Clone)]
pub struct PendingCommand {
    /// The original command string.
    pub command: String,
}

/// Pending teleport coordinates.
#[derive(Clone, Copy)]
pub struct PendingTeleport {
    /// Target X coordinate.
    pub x: f64,
    /// Target Y coordinate.
    pub y: f64,
    /// Target Z coordinate.
    pub z: f64,
}

/// Type of search being performed.
#[derive(Clone, Debug)]
pub enum LocateSearchType {
    /// Searching for a biome.
    Biome(crate::terrain_gen::BiomeType),
    /// Searching for a block type.
    Block(crate::chunk::BlockType),
    /// Searching for a cave of minimum size.
    Cave(usize),
}

/// Pending locate search state for frame-distributed searching.
#[derive(Clone)]
pub struct PendingLocateSearch {
    /// Type of search.
    pub search_type: LocateSearchType,
    /// Player position when search started.
    pub player_pos: Vector3<i32>,
    /// Maximum search range.
    pub max_range: i32,
    /// Current radius being searched.
    pub current_radius: i32,
    /// Step size for spiral search.
    pub step: i32,
    /// Current Y offset (for 3D searches).
    pub y_offset: i32,
    /// Current Y direction (-1 or 1).
    pub y_dir: i32,
    /// Best match found so far.
    pub best_match: Option<(Vector3<i32>, usize)>, // (position, size for caves)
    /// Minimum distance found.
    pub min_distance: i32,
    /// Total positions checked.
    pub positions_checked: usize,
    /// Positions to check per frame.
    pub positions_per_frame: usize,
    /// For lava/block searches: count of relevant biomes found.
    pub relevant_biomes_found: usize,
    /// Whether to teleport to found location.
    pub teleport_on_find: bool,
}

/// Parameter type for command autocomplete.
#[derive(Clone, Debug, PartialEq)]
pub enum ParamType {
    /// Block type name (completable).
    Block,
    /// X coordinate (relative ~ supported).
    CoordX,
    /// Y coordinate (relative ~ supported).
    CoordY,
    /// Z coordinate (relative ~ supported).
    CoordZ,
    /// Flag (like "hollow", "on", "off").
    Flag(&'static [&'static str]),
    /// Free text (no completion).
    Text,
}

/// Command signature for autocomplete.
#[derive(Clone)]
pub struct CommandSignature {
    /// Command name.
    pub name: &'static str,
    /// Command aliases.
    pub aliases: &'static [&'static str],
    /// Parameter types.
    pub params: &'static [ParamType],
}

/// Console state for the in-game command system.
#[derive(Default)]
pub struct ConsoleState {
    /// Whether the console is currently visible.
    pub active: bool,
    /// Current input text.
    pub input: String,
    /// Command history (most recent last).
    pub history: Vec<String>,
    /// Current position in history navigation (None = not navigating).
    history_index: Option<usize>,
    /// Saved input when navigating history.
    saved_input: String,
    /// Output log entries.
    pub output: Vec<ConsoleEntry>,
    /// Command pending confirmation.
    pub pending_confirm: Option<PendingCommand>,
    /// Whether the text input should request focus.
    pub request_focus: bool,
    /// Pending teleport to be handled by game loop.
    pub pending_teleport: Option<PendingTeleport>,
    /// Pending fluid debug output request.
    pub pending_fluid_debug: bool,
    /// Pending force water active request.
    pub pending_force_water_active: bool,
    /// Pending water analyze request.
    pub pending_water_analyze: bool,
    /// Pending biome debug toggle (Some(true/false) if changed).
    pub pending_biome_debug: Option<bool>,
    /// Pending template to be loaded for placement.
    pub pending_template_load: Option<crate::templates::VxtFile>,
    /// Pending locate search being processed frame by frame.
    pub pending_locate_search: Option<PendingLocateSearch>,
    /// Current autocomplete suggestions.
    pub suggestions: Vec<String>,
    /// Currently selected suggestion index.
    pub suggestion_index: usize,
    /// Whether to move cursor to end of input on next frame.
    pub move_cursor_to_end: bool,
}

/// Maximum number of command history entries to persist.
const MAX_HISTORY_ENTRIES: usize = 100;

impl ConsoleState {
    /// Creates a new console state.
    pub fn new() -> Self {
        Self {
            active: false,
            input: String::new(),
            history: Vec::new(),
            history_index: None,
            saved_input: String::new(),
            output: Vec::new(),
            pending_confirm: None,
            request_focus: false,
            pending_teleport: None,
            pending_fluid_debug: false,
            pending_force_water_active: false,
            pending_water_analyze: false,
            pending_biome_debug: None,
            pending_template_load: None,
            pending_locate_search: None,
            suggestions: Vec::new(),
            suggestion_index: 0,
            move_cursor_to_end: false,
        }
    }

    /// Get all available command signatures for autocomplete.
    pub fn command_signatures() -> Vec<CommandSignature> {
        const HOLLOW_FLAG: &[&str] = &["hollow"];
        const BIOME_FLAGS: &[&str] = &["on", "off", "true", "false"];
        const ROTATION_FLAGS: &[&str] = &["rotate_90", "rotate_180", "rotate_270"];
        const LOCATE_TARGETS: &[&str] = &[
            "grassland",
            "mountains",
            "desert",
            "swamp",
            "snow",
            "cave",
            "lava",
            "water",
            "stone",
            "dirt",
            "grass",
            "sand",
            "gravel",
            "iron",
        ];
        const LOCATE_FLAGS: &[&str] = &["tp"];

        vec![
            CommandSignature {
                name: "help",
                aliases: &["?"],
                params: &[],
            },
            CommandSignature {
                name: "fill",
                aliases: &[],
                params: &[
                    ParamType::Block,
                    ParamType::CoordX,
                    ParamType::CoordY,
                    ParamType::CoordZ,
                    ParamType::CoordX,
                    ParamType::CoordY,
                    ParamType::CoordZ,
                    ParamType::Flag(HOLLOW_FLAG),
                ],
            },
            CommandSignature {
                name: "sphere",
                aliases: &[],
                params: &[
                    ParamType::Block,
                    ParamType::CoordX,
                    ParamType::CoordY,
                    ParamType::CoordZ,
                    ParamType::Text, // radius
                    ParamType::Flag(HOLLOW_FLAG),
                ],
            },
            CommandSignature {
                name: "boxme",
                aliases: &[],
                params: &[ParamType::Block, ParamType::Flag(HOLLOW_FLAG)],
            },
            CommandSignature {
                name: "copy",
                aliases: &[],
                params: &[
                    ParamType::CoordX,
                    ParamType::CoordY,
                    ParamType::CoordZ,
                    ParamType::CoordX,
                    ParamType::CoordY,
                    ParamType::CoordZ,
                    ParamType::CoordX,
                    ParamType::CoordY,
                    ParamType::CoordZ,
                    ParamType::Flag(ROTATION_FLAGS),
                ],
            },
            CommandSignature {
                name: "tp",
                aliases: &["teleport"],
                params: &[ParamType::CoordX, ParamType::CoordY, ParamType::CoordZ],
            },
            CommandSignature {
                name: "locate",
                aliases: &[],
                params: &[
                    ParamType::Flag(LOCATE_TARGETS), // Biomes, common blocks, and "cave"
                    ParamType::Text,                 // Range or size
                    ParamType::Flag(LOCATE_FLAGS),   // tp flag
                ],
            },
            CommandSignature {
                name: "cancel",
                aliases: &["cancellocate"],
                params: &[],
            },
            CommandSignature {
                name: "waterdebug",
                aliases: &["wd"],
                params: &[],
            },
            CommandSignature {
                name: "waterforce",
                aliases: &["wf"],
                params: &[],
            },
            CommandSignature {
                name: "wateranalyze",
                aliases: &["wa"],
                params: &[],
            },
            CommandSignature {
                name: "biome_debug",
                aliases: &["bd"],
                params: &[ParamType::Flag(BIOME_FLAGS)],
            },
            CommandSignature {
                name: "select",
                aliases: &[],
                params: &[
                    ParamType::Text, // subcommand: pos1, pos2, clear
                    ParamType::CoordX,
                    ParamType::CoordY,
                    ParamType::CoordZ,
                ],
            },
            CommandSignature {
                name: "template",
                aliases: &[],
                params: &[
                    ParamType::Text, // subcommand: save, load, list, delete, info
                    ParamType::Text, // name
                    ParamType::Text, // tags (variadic)
                ],
            },
            CommandSignature {
                name: "clear",
                aliases: &[],
                params: &[],
            },
        ]
    }

    /// Update autocomplete suggestions based on current input.
    pub fn update_autocomplete(&mut self) {
        use crate::chunk::BlockType;

        self.suggestions.clear();
        self.suggestion_index = 0;

        let input = self.input.trim();
        if input.is_empty() {
            return;
        }

        let parts: Vec<&str> = input.split_whitespace().collect();
        let signatures = Self::command_signatures();

        // If we only have one word, complete command names
        if parts.len() == 1 {
            let partial = parts[0].to_lowercase();
            for sig in &signatures {
                if sig.name.starts_with(&partial) {
                    self.suggestions.push(sig.name.to_string());
                }
                for alias in sig.aliases {
                    if alias.starts_with(&partial) {
                        self.suggestions.push(alias.to_string());
                    }
                }
            }
            self.suggestions.sort();
            self.suggestions.dedup();
            return;
        }

        // Find matching command signature
        let cmd = parts[0].to_lowercase();
        let param_index = parts.len() - 2; // -1 for command, -1 for 0-based index

        for sig in &signatures {
            let matches_cmd = sig.name == cmd || sig.aliases.contains(&cmd.as_str());
            if !matches_cmd {
                continue;
            }

            // Get the parameter type we're completing
            if param_index >= sig.params.len() {
                continue;
            }

            let param_type = &sig.params[param_index];
            let partial = parts.last().unwrap_or(&"").to_lowercase();

            match param_type {
                ParamType::Block => {
                    // Complete block names
                    for name in BlockType::all_block_names() {
                        if name.starts_with(&partial) {
                            self.suggestions.push(name.to_string());
                        }
                    }
                }
                ParamType::Flag(flags) => {
                    // Complete flag names
                    for flag in *flags {
                        if flag.starts_with(&partial) {
                            self.suggestions.push(flag.to_string());
                        }
                    }
                }
                ParamType::CoordX | ParamType::CoordY | ParamType::CoordZ => {
                    // Suggest relative coordinate syntax
                    if partial.is_empty() || partial.starts_with('~') {
                        self.suggestions.push("~".to_string());
                        if partial.is_empty() {
                            self.suggestions.push("0".to_string());
                        }
                    }
                }
                ParamType::Text => {
                    // No completion for free text
                }
            }

            break;
        }

        self.suggestions.sort();
        self.suggestions.dedup();
    }

    /// Apply the currently selected suggestion.
    pub fn apply_suggestion(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }

        let suggestion = &self.suggestions[self.suggestion_index];
        let parts: Vec<&str> = self.input.split_whitespace().collect();

        if parts.len() == 1 {
            // Completing command name
            self.input = format!("{} ", suggestion);
        } else {
            // Completing parameter
            let mut new_parts = parts[..parts.len() - 1].to_vec();
            new_parts.push(suggestion);
            self.input = new_parts.join(" ") + " ";
        }

        self.suggestions.clear();
        self.suggestion_index = 0;
        self.move_cursor_to_end = true;
    }

    /// Cycle to next suggestion.
    pub fn next_suggestion(&mut self) {
        if !self.suggestions.is_empty() {
            self.suggestion_index = (self.suggestion_index + 1) % self.suggestions.len();
        }
    }

    /// Cycle to previous suggestion.
    pub fn prev_suggestion(&mut self) {
        if !self.suggestions.is_empty() {
            if self.suggestion_index == 0 {
                self.suggestion_index = self.suggestions.len() - 1;
            } else {
                self.suggestion_index -= 1;
            }
        }
    }

    /// Get ghost text placeholder for current input.
    pub fn get_ghost_text(&self) -> String {
        // Don't trim - we need to preserve trailing spaces to know if user finished typing a word
        let input = &self.input;
        if input.is_empty() {
            return String::new();
        }

        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            return String::new();
        }

        let signatures = Self::command_signatures();

        // Find matching command signature
        let cmd = parts[0].to_lowercase();
        for sig in &signatures {
            let matches_cmd = sig.name == cmd || sig.aliases.contains(&cmd.as_str());
            if !matches_cmd {
                continue;
            }

            // Check if user has finished typing current word (ends with space)
            let ends_with_space = input.ends_with(' ');

            // Calculate parameter index
            // If "fill " (ends with space), we're starting parameter 0
            // If "fill" (no space), we're still typing command, no ghost text
            // If "fill stone" (no trailing space), we're typing parameter 0, no ghost text yet
            // If "fill stone " (trailing space), we're starting parameter 1
            let param_start = if ends_with_space {
                parts.len() - 1
            } else {
                // Still typing current word, no ghost text
                return String::new();
            };

            if param_start >= sig.params.len() {
                // No more parameters to show
                return String::new();
            }

            let mut ghost_parts = Vec::new();
            for (i, param) in sig.params[param_start..].iter().enumerate() {
                let label = match param {
                    ParamType::Block => "<block>",
                    ParamType::CoordX => "<x>",
                    ParamType::CoordY => "<y>",
                    ParamType::CoordZ => "<z>",
                    ParamType::Flag(flags) => {
                        if flags.len() == 1 {
                            flags[0]
                        } else if i == 0 {
                            // If this is the next param, show first option
                            flags[0]
                        } else {
                            "<flag>"
                        }
                    }
                    ParamType::Text => "<value>",
                };
                ghost_parts.push(label);
            }

            return ghost_parts.join(" ");
        }

        String::new()
    }

    /// Creates a console state with pre-loaded command history.
    pub fn with_history(history: Vec<String>) -> Self {
        let mut state = Self::new();
        // Take only the last MAX_HISTORY_ENTRIES
        state.history = if history.len() > MAX_HISTORY_ENTRIES {
            history[history.len() - MAX_HISTORY_ENTRIES..].to_vec()
        } else {
            history
        };
        state
    }

    /// Returns the command history for persistence.
    /// Limits to MAX_HISTORY_ENTRIES.
    pub fn get_history(&self) -> Vec<String> {
        if self.history.len() > MAX_HISTORY_ENTRIES {
            self.history[self.history.len() - MAX_HISTORY_ENTRIES..].to_vec()
        } else {
            self.history.clone()
        }
    }

    /// Toggles console visibility.
    pub fn toggle(&mut self) {
        self.active = !self.active;
        if self.active {
            self.request_focus = true;
        }
    }

    /// Closes the console.
    pub fn close(&mut self) {
        self.active = false;
        self.pending_confirm = None;
    }

    /// Adds an output entry to the console.
    pub fn add_output(&mut self, entry: ConsoleEntry) {
        self.output.push(entry);
        // Trim old entries if needed
        while self.output.len() > MAX_OUTPUT_LINES {
            self.output.remove(0);
        }
    }

    /// Adds an info message to output.
    pub fn info(&mut self, msg: impl Into<String>) {
        self.add_output(ConsoleEntry::Info(msg.into()));
    }

    /// Adds a success message to output.
    pub fn success(&mut self, msg: impl Into<String>) {
        self.add_output(ConsoleEntry::Success(msg.into()));
    }

    /// Adds an error message to output.
    pub fn error(&mut self, msg: impl Into<String>) {
        self.add_output(ConsoleEntry::Error(msg.into()));
    }

    /// Adds a warning message to output.
    pub fn warning(&mut self, msg: impl Into<String>) {
        self.add_output(ConsoleEntry::Warning(msg.into()));
    }

    /// Outputs fluid debug information.
    /// Called by the HUD when pending_fluid_debug is set.
    pub fn output_fluid_debug(
        &mut self,
        water_cells: usize,
        water_active: usize,
        lava_cells: usize,
        lava_active: usize,
    ) {
        self.info("=== Fluid Simulation Debug ===");
        self.info(format!(
            "Water: {} cells, {} active",
            water_cells, water_active
        ));
        self.info(format!(
            "Lava: {} cells, {} active",
            lava_cells, lava_active
        ));
        if water_active == 0 && water_cells > 0 {
            self.warning("Water cells exist but none are active - water is stable/stuck");
        }
        if lava_active == 0 && lava_cells > 0 {
            self.warning("Lava cells exist but none are active - lava is stable/stuck");
        }
        self.pending_fluid_debug = false;
    }

    /// Navigate up in command history.
    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        match self.history_index {
            None => {
                // Start navigating from the end
                self.saved_input = self.input.clone();
                self.history_index = Some(self.history.len() - 1);
                self.input = self.history[self.history.len() - 1].clone();
            }
            Some(idx) if idx > 0 => {
                self.history_index = Some(idx - 1);
                self.input = self.history[idx - 1].clone();
            }
            _ => {}
        }
    }

    /// Navigate down in command history.
    pub fn history_down(&mut self) {
        if let Some(idx) = self.history_index {
            if idx + 1 < self.history.len() {
                self.history_index = Some(idx + 1);
                self.input = self.history[idx + 1].clone();
            } else {
                // Back to original input
                self.history_index = None;
                self.input = self.saved_input.clone();
            }
        }
    }

    /// Submit the current input for execution.
    #[allow(clippy::too_many_arguments)]
    pub fn submit(
        &mut self,
        world: &mut World,
        player_pos: Vector3<i32>,
        template_selection: &mut crate::templates::TemplateSelection,
        template_library: &crate::templates::TemplateLibrary,
        water_grid: &crate::water::WaterGrid,
        terrain_generator: &crate::terrain_gen::TerrainGenerator,
    ) {
        let input = self.input.trim().to_string();
        if input.is_empty() {
            return;
        }

        // Add to history (avoid duplicates at end)
        if self.history.last() != Some(&input) {
            self.history.push(input.clone());
        }

        // Reset history navigation
        self.history_index = None;
        self.saved_input.clear();

        // Clear input
        self.input.clear();

        // Execute command
        self.execute(
            &input,
            world,
            player_pos,
            template_selection,
            template_library,
            water_grid,
            terrain_generator,
        );
    }

    /// Execute a command string.
    #[allow(clippy::too_many_arguments)]
    fn execute(
        &mut self,
        input: &str,
        world: &mut World,
        player_pos: Vector3<i32>,
        template_selection: &mut crate::templates::TemplateSelection,
        template_library: &crate::templates::TemplateLibrary,
        water_grid: &crate::water::WaterGrid,
        terrain_generator: &crate::terrain_gen::TerrainGenerator,
    ) {
        // Echo the command
        self.info(format!("> {}", input));

        // Handle confirmation response
        if let Some(pending) = self.pending_confirm.take() {
            let response = input.to_lowercase();
            if response == "y" || response == "yes" {
                // Re-execute the original command with confirmation bypass
                self.execute_confirmed(
                    &pending.command,
                    world,
                    player_pos,
                    template_selection,
                    template_library,
                    water_grid,
                    terrain_generator,
                );
            } else {
                self.info("Command cancelled.");
            }
            return;
        }

        // Parse and execute command
        let result = self.parse_and_execute(
            input,
            world,
            player_pos,
            template_selection,
            template_library,
            water_grid,
            terrain_generator,
            false,
        );
        self.handle_result(result);
    }

    /// Execute a confirmed command (bypass volume check).
    #[allow(clippy::too_many_arguments)]
    fn execute_confirmed(
        &mut self,
        input: &str,
        world: &mut World,
        player_pos: Vector3<i32>,
        template_selection: &mut crate::templates::TemplateSelection,
        template_library: &crate::templates::TemplateLibrary,
        water_grid: &crate::water::WaterGrid,
        terrain_generator: &crate::terrain_gen::TerrainGenerator,
    ) {
        let result = self.parse_and_execute(
            input,
            world,
            player_pos,
            template_selection,
            template_library,
            water_grid,
            terrain_generator,
            true,
        );
        self.handle_result(result);
    }

    /// Handle command result.
    pub fn handle_result(&mut self, result: CommandResult) {
        match result {
            CommandResult::Success(msg) => self.success(msg),
            CommandResult::Error(msg) => self.error(msg),
            CommandResult::NeedsConfirmation { message, command } => {
                self.warning(&message);
                self.info("Type 'y' or 'yes' to confirm, anything else to cancel.");
                self.pending_confirm = Some(PendingCommand { command });
            }
            CommandResult::Teleport { x, y, z } => {
                self.success(format!("Teleporting to ({:.1}, {:.1}, {:.1})", x, y, z));
                self.pending_teleport = Some(PendingTeleport { x, y, z });
            }
            CommandResult::FluidDebug => {
                // Signal that caller should output fluid debug info
                self.pending_fluid_debug = true;
            }
            CommandResult::ForceWaterActive => {
                // Signal that caller should force all water cells active
                self.pending_force_water_active = true;
            }
            CommandResult::WaterAnalyze => {
                // Signal that caller should analyze water at player position
                self.pending_water_analyze = true;
            }
            CommandResult::SetBiomeDebug(enabled) => {
                self.success(format!(
                    "Biome debug visualization: {}",
                    if enabled { "ON" } else { "OFF" }
                ));
                self.pending_biome_debug = Some(enabled);
            }
            CommandResult::LoadTemplate(template) => {
                self.success(format!(
                    "Loaded template '{}' ({}×{}×{}, {} blocks). Use R to rotate, Enter to place",
                    template.name,
                    template.width,
                    template.height,
                    template.depth,
                    template.block_count()
                ));
                self.pending_template_load = Some(template);
            }
            CommandResult::LocateBiome {
                biome_name,
                x,
                y,
                z,
                distance,
                direction,
            } => {
                self.success(format!(
                    "{} biome found {} blocks {} at ({}, {}, {})",
                    biome_name, distance, direction, x, y, z
                ));
                self.info(format!("Use 'tp {} {} {}' to teleport there", x, y, z));
            }
            CommandResult::StartLocateSearch(search) => {
                let search_name = match &search.search_type {
                    LocateSearchType::Biome(biome) => format!("{:?} biome", biome),
                    LocateSearchType::Block(block) => format!("{:?} block", block),
                    LocateSearchType::Cave(size) => format!("cave (min {} blocks)", size),
                };
                self.info(format!("Searching for {}...", search_name));
                self.pending_locate_search = Some(search);
            }
        }
    }

    /// Parse and execute a command.
    #[allow(clippy::too_many_arguments)]
    fn parse_and_execute(
        &mut self,
        input: &str,
        world: &mut World,
        player_pos: Vector3<i32>,
        template_selection: &mut crate::templates::TemplateSelection,
        template_library: &crate::templates::TemplateLibrary,
        water_grid: &crate::water::WaterGrid,
        terrain_generator: &crate::terrain_gen::TerrainGenerator,
        confirmed: bool,
    ) -> CommandResult {
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            return CommandResult::Error("Empty command".to_string());
        }

        let cmd = parts[0].to_lowercase();
        let args = &parts[1..];

        match cmd.as_str() {
            "help" | "?" => commands::help(),
            "fill" => commands::fill(args, world, player_pos, confirmed),
            "sphere" => commands::sphere(args, world, player_pos, confirmed),
            "boxme" => commands::boxme(args, world, player_pos, confirmed),
            "copy" => commands::copy(args, world, player_pos, confirmed),
            "tp" | "teleport" => commands::tp(args, player_pos),
            "locate" => commands::locate(args, player_pos, terrain_generator, world),
            "cancel" | "cancellocate" => {
                if self.pending_locate_search.is_some() {
                    self.pending_locate_search = None;
                    CommandResult::Success("Locate search cancelled.".to_string())
                } else {
                    CommandResult::Error("No active locate search to cancel.".to_string())
                }
            }
            "waterdebug" | "wd" => CommandResult::FluidDebug,
            "waterforce" | "wf" => CommandResult::ForceWaterActive,
            "wateranalyze" | "wa" => CommandResult::WaterAnalyze,
            "biome_debug" | "bd" => {
                if let Some(arg) = args.first() {
                    match arg.to_lowercase().as_str() {
                        "on" | "true" | "1" => CommandResult::SetBiomeDebug(true),
                        "off" | "false" | "0" => CommandResult::SetBiomeDebug(false),
                        _ => CommandResult::Error("Usage: biome_debug [on|off]".to_string()),
                    }
                } else {
                    // Toggle if no argument (requires knowing current state, which we don't. So default to ON or error?
                    // Better to require argument or assume toggle based on UI state (can't access UI here).
                    // Let's assume ON if typing it without args is common debug behavior, or Error.
                    // Actually, let's just make it ON.
                    CommandResult::SetBiomeDebug(true)
                }
            }
            "select" => {
                // Convert player_pos from i32 to f64 for the select command
                // Note: player_pos.y is already +1 from feet position
                let player_pos_f64 = Vector3::new(
                    player_pos.x as f64 + 0.5,
                    player_pos.y as f64,
                    player_pos.z as f64 + 0.5,
                );
                commands::select(args, player_pos_f64, template_selection)
            }
            "template" => commands::template(
                args,
                template_selection,
                world,
                water_grid,
                template_library,
                confirmed,
            ),
            "clear" => {
                self.output.clear();
                CommandResult::Success("Console cleared.".to_string())
            }
            _ => CommandResult::Error(format!(
                "Unknown command: '{}'. Type 'help' for commands.",
                cmd
            )),
        }
    }
}

/// Parse a coordinate value that may be relative (~).
/// `~` means player position, `~5` means player + 5, `~-5` means player - 5.
pub fn parse_coordinate(s: &str, player_coord: i32) -> Result<i32, String> {
    let s = s.trim();
    if let Some(offset_str) = s.strip_prefix('~') {
        if offset_str.is_empty() {
            Ok(player_coord)
        } else {
            offset_str
                .parse::<i32>()
                .map(|offset| player_coord + offset)
                .map_err(|_| format!("Invalid relative coordinate: '{}'", s))
        }
    } else {
        s.parse::<i32>()
            .map_err(|_| format!("Invalid coordinate: '{}'", s))
    }
}

/// Volume threshold for confirmation.
pub fn volume_confirm_threshold() -> u64 {
    VOLUME_CONFIRM_THRESHOLD
}

/// Validate Y coordinate is within world bounds.
/// Returns an error message if out of bounds, None if valid.
pub fn validate_y_bounds(y: i32) -> Option<String> {
    if y < 0 {
        Some(format!("Y coordinate {} is below world (min: 0)", y))
    } else if y > MAX_Y {
        Some(format!(
            "Y coordinate {} is above world (max: {})",
            y, MAX_Y
        ))
    } else {
        None
    }
}

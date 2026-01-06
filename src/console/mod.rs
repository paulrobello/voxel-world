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
        }
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
    pub fn submit(&mut self, world: &mut World, player_pos: Vector3<i32>) {
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
        self.execute(&input, world, player_pos);
    }

    /// Execute a command string.
    fn execute(&mut self, input: &str, world: &mut World, player_pos: Vector3<i32>) {
        // Echo the command
        self.info(format!("> {}", input));

        // Handle confirmation response
        if let Some(pending) = self.pending_confirm.take() {
            let response = input.to_lowercase();
            if response == "y" || response == "yes" {
                // Re-execute the original command with confirmation bypass
                self.execute_confirmed(&pending.command, world, player_pos);
            } else {
                self.info("Command cancelled.");
            }
            return;
        }

        // Parse and execute command
        let result = self.parse_and_execute(input, world, player_pos, false);
        self.handle_result(result);
    }

    /// Execute a confirmed command (bypass volume check).
    fn execute_confirmed(&mut self, input: &str, world: &mut World, player_pos: Vector3<i32>) {
        let result = self.parse_and_execute(input, world, player_pos, true);
        self.handle_result(result);
    }

    /// Handle command result.
    fn handle_result(&mut self, result: CommandResult) {
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
        }
    }

    /// Parse and execute a command.
    fn parse_and_execute(
        &mut self,
        input: &str,
        world: &mut World,
        player_pos: Vector3<i32>,
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
            "tp" | "teleport" => commands::tp(args, player_pos),
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

//! Water flow simulation using cellular automata.
//!
//! This module implements a mass-based water system where each cell can hold
//! a variable amount of water (0.0 to 1.0+). Water flows using the W-Shadow
//! algorithm: down first, then horizontally to equalize, then up under pressure.
//!
//! Water is stored in a sparse HashMap to minimize memory usage since most
//! blocks don't contain water.
//!
//! ## Performance Optimizations
//!
//! - **Y-Layer Bucket Sort**: O(n) bucket distribution instead of O(n log n) sort
//! - **Cached Neighbor Masses**: Pre-fetch all 6 neighbor masses once per cell
//! - **Lazy Pruning**: Only prune when active set exceeds threshold
//! - **Reusable Buffers**: Avoid per-tick allocations for working vectors

#![allow(dead_code)]

use crate::chunk::WaterType;
use crate::constants::ORTHO_DIRS;
use nalgebra::Vector3;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Maximum water mass a cell can hold before it's considered "full".
/// Values above this indicate pressure (compressed water).
pub const MAX_MASS: f32 = 1.0;

/// Maximum compression (water can hold up to MAX_MASS + MAX_COMPRESS).
pub const MAX_COMPRESS: f32 = 0.02;

/// Minimum water mass to track (below this, water evaporates).
pub const MIN_MASS: f32 = 0.001;

/// Minimum flow amount (don't bother flowing tiny amounts).
/// Set to MIN_MASS to ensure any measurable water can flow and drain.
pub const MIN_FLOW: f32 = MIN_MASS;

/// Flow damping factor to prevent oscillation (0.0 to 1.0).
/// Lower values = more damping = slower but more stable flow.
pub const FLOW_DAMPING: f32 = 0.8;

/// Maximum water updates per frame (performance limit).
pub const DEFAULT_WATER_UPDATES_PER_FRAME: usize = 1024;

/// Default simulation radius in blocks (water outside this range is dormant).
/// This ensures water is only simulated in loaded chunks near the player.
pub const DEFAULT_SIMULATION_RADIUS: f32 = 64.0;

/// Default tick interval in milliseconds.
/// Lower = faster simulation, higher = slower/smoother.
/// 50ms = 20 ticks/second (Minecraft-like)
pub const DEFAULT_TICK_INTERVAL_MS: u64 = 50;

/// Threshold for lazy pruning - only prune when active set exceeds this size.
/// This amortizes the cost of pruning instead of doing it every tick.
const PRUNE_THRESHOLD: usize = 2048;

/// Number of Y-layer buckets for bucket sort optimization.
/// Covers Y coordinates 0-511 (world height).
const Y_BUCKET_COUNT: usize = 512;

/// Checks if a block position is within a squared radius of a player position.
#[inline]
fn is_within_radius_sq(pos: &Vector3<i32>, player_pos: &Vector3<f32>, radius_sq: f32) -> bool {
    let dx = pos.x as f32 - player_pos.x;
    let dy = pos.y as f32 - player_pos.y;
    let dz = pos.z as f32 - player_pos.z;
    dx * dx + dy * dy + dz * dz <= radius_sq
}

/// Profiling statistics for water simulation performance.
#[derive(Debug, Clone, Default)]
pub struct WaterSimStats {
    /// Time spent in the last tick
    pub last_tick_duration: Duration,
    /// Number of cells processed in last tick
    pub last_cells_processed: usize,
    /// Number of cells that flowed in last tick
    pub last_cells_flowed: usize,
    /// Number of cells deactivated in last tick
    pub last_cells_deactivated: usize,
    /// Time spent sorting/bucketing
    pub last_sort_duration: Duration,
    /// Time spent calculating flow
    pub last_flow_duration: Duration,
    /// Time spent applying changes
    pub last_apply_duration: Duration,
    /// Whether profiling is enabled
    pub profiling_enabled: bool,
}

impl WaterSimStats {
    /// Returns average microseconds per cell processed
    pub fn us_per_cell(&self) -> f64 {
        if self.last_cells_processed == 0 {
            0.0
        } else {
            self.last_tick_duration.as_secs_f64() * 1_000_000.0 / self.last_cells_processed as f64
        }
    }
}

/// Cached neighbor masses for a cell to avoid repeated HashMap lookups.
/// All masses include pending changes from earlier in the tick.
#[derive(Debug, Clone, Copy, Default)]
struct NeighborMasses {
    below: f32,
    above: f32,
    pos_x: f32,
    neg_x: f32,
    pos_z: f32,
    neg_z: f32,
    /// Whether below position is out of bounds (drains to void)
    below_void: bool,
    /// Solid state for each neighbor (true = blocked)
    below_solid: bool,
    above_solid: bool,
    pos_x_solid: bool,
    neg_x_solid: bool,
    pos_z_solid: bool,
    neg_z_solid: bool,
}

/// A single water cell with mass and properties.
#[derive(Debug, Clone, Copy)]
pub struct WaterCell {
    /// Amount of water in this cell (0.0 = empty, 1.0 = full block).
    /// Can exceed 1.0 for pressurized water (underwater columns).
    pub mass: f32,

    /// Visual mass for smooth rendering. Lerps toward actual mass each frame.
    /// This prevents strobing/flickering at water edges.
    pub display_mass: f32,

    /// If true, this cell generates infinite water (ocean surface, springs).
    /// Source cells always maintain mass = MAX_MASS.
    pub is_source: bool,

    /// Ticks since last flow activity. Used to deactivate stable water.
    pub stable_ticks: u8,

    /// Type of water (Ocean, Lake, River, Swamp, Spring).
    /// Determines color, flow rate, and other properties.
    pub water_type: WaterType,
}

impl Default for WaterCell {
    fn default() -> Self {
        Self {
            mass: 0.0,
            display_mass: 0.0,
            is_source: false,
            stable_ticks: 0,
            water_type: WaterType::Ocean,
        }
    }
}

impl WaterCell {
    /// Creates a new water cell with the given mass and type.
    pub fn new(mass: f32, water_type: WaterType) -> Self {
        Self {
            mass,
            display_mass: mass,
            is_source: false,
            stable_ticks: 0,
            water_type,
        }
    }

    /// Creates a source cell (infinite water).
    pub fn source(water_type: WaterType) -> Self {
        Self {
            mass: MAX_MASS,
            display_mass: MAX_MASS,
            is_source: true,
            stable_ticks: 0,
            water_type,
        }
    }

    /// Returns true if this cell has significant water.
    #[inline]
    pub fn has_water(&self) -> bool {
        self.mass > MIN_MASS
    }

    /// Returns true if this cell is full (can't accept more water without pressure).
    #[inline]
    pub fn is_full(&self) -> bool {
        self.mass >= MAX_MASS
    }

    /// Returns true if this cell is stable (hasn't flowed recently).
    #[inline]
    pub fn is_stable(&self) -> bool {
        self.stable_ticks >= 10
    }

    /// Returns the visual water height (0.0 to 1.0) for rendering.
    /// Uses display_mass for smooth transitions.
    #[inline]
    pub fn visual_height(&self) -> f32 {
        self.display_mass.clamp(0.0, 1.0)
    }

    /// Updates the display mass toward the actual mass for smooth visuals.
    /// Call this every frame with delta_time in seconds.
    #[inline]
    pub fn update_display(&mut self, delta_time: f32) {
        // Lerp speed - higher = faster catch-up (10.0 = ~100ms to reach target)
        const LERP_SPEED: f32 = 10.0;
        let t = (LERP_SPEED * delta_time).min(1.0);
        self.display_mass = self.display_mass + (self.mass - self.display_mass) * t;
    }
}

/// Flow result from calculating water movement.
#[derive(Debug, Clone, Default)]
pub struct FlowResult {
    /// Water flowing down (-Y).
    pub down: f32,
    /// Water flowing up (+Y) - only under pressure.
    pub up: f32,
    /// Water flowing in +X direction.
    pub pos_x: f32,
    /// Water flowing in -X direction.
    pub neg_x: f32,
    /// Water flowing in +Z direction.
    pub pos_z: f32,
    /// Water flowing in -Z direction.
    pub neg_z: f32,
}

impl FlowResult {
    /// Returns true if any water is flowing.
    pub fn has_flow(&self) -> bool {
        self.down > MIN_FLOW
            || self.up > MIN_FLOW
            || self.pos_x > MIN_FLOW
            || self.neg_x > MIN_FLOW
            || self.pos_z > MIN_FLOW
            || self.neg_z > MIN_FLOW
    }

    /// Returns the total amount of water flowing out.
    pub fn total_outflow(&self) -> f32 {
        self.down + self.up + self.pos_x + self.neg_x + self.pos_z + self.neg_z
    }
}

/// Water cell update for multiplayer synchronization.
/// Contains all data needed to sync a single water cell state.
#[derive(Debug, Clone, Copy)]
pub struct WaterCellSyncUpdate {
    /// World position of the water cell.
    pub position: Vector3<i32>,
    /// Water mass (0.0 to 1.0+ for pressurized water).
    /// Mass <= 0 indicates the cell was removed.
    pub mass: f32,
    /// Whether this is an infinite water source.
    pub is_source: bool,
    /// Type of water (determines color and flow behavior).
    pub water_type: WaterType,
}

/// Sparse storage for water cells using HashMap.
///
/// Only cells with water are stored, minimizing memory for worlds
/// where most blocks are dry.
pub struct WaterGrid {
    /// Water cells indexed by world position.
    cells: HashMap<Vector3<i32>, WaterCell>,

    /// Set of positions with active (potentially flowing) water.
    /// Water that hasn't moved for several ticks is removed from this set.
    active: HashSet<Vector3<i32>>,

    /// Pending water changes (double-buffer for simulation).
    /// Key: position, Value: (mass delta, water type).
    /// If type is Ocean (default), it means "keep existing type or inherit".
    /// If delta is negative, type is ignored.
    pending_changes: HashMap<Vector3<i32>, (f32, WaterType)>,

    /// Positions that need to be checked next tick.
    dirty_positions: HashSet<Vector3<i32>>,

    /// Maximum updates per frame.
    pub max_updates_per_frame: usize,

    /// Simulation radius in blocks. Water outside this radius from the player is dormant.
    /// This ensures water is only simulated in loaded chunks near the player.
    pub simulation_radius: f32,

    /// Tick interval in milliseconds. Controls simulation speed.
    pub tick_interval_ms: u64,

    /// Last time a simulation tick was run.
    last_tick: Instant,

    // === Performance Optimization Fields ===
    /// Reusable buffer for changed positions (avoids per-tick allocation)
    changed_positions_buffer: Vec<Vector3<i32>>,

    /// Reusable buffer for cells to deactivate
    deactivate_buffer: Vec<Vector3<i32>>,

    /// Reusable Y-layer buckets to avoid per-tick allocations
    y_buckets: [Vec<Vector3<i32>>; Y_BUCKET_COUNT],

    /// Last player position for lazy pruning decision
    last_prune_player_pos: Option<Vector3<f32>>,

    /// Tick counter for periodic operations
    tick_counter: u64,

    /// Performance profiling statistics
    pub stats: WaterSimStats,

    /// Buffer for collecting sync updates (reused to avoid allocations)
    sync_updates_buffer: Vec<WaterCellSyncUpdate>,

    /// Reusable buffer for water-lava adjacency checks (avoids per-tick allocation)
    lava_check_buffer: HashSet<Vector3<i32>>,
}

impl Default for WaterGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl WaterGrid {
    /// Creates a new empty water grid.
    pub fn new() -> Self {
        Self {
            cells: HashMap::with_capacity(4096),
            active: HashSet::with_capacity(1024),
            pending_changes: HashMap::with_capacity(256),
            dirty_positions: HashSet::with_capacity(256),
            max_updates_per_frame: DEFAULT_WATER_UPDATES_PER_FRAME,
            simulation_radius: DEFAULT_SIMULATION_RADIUS,
            tick_interval_ms: DEFAULT_TICK_INTERVAL_MS,
            last_tick: Instant::now(),
            // Performance optimization fields
            changed_positions_buffer: Vec::with_capacity(2048),
            deactivate_buffer: Vec::with_capacity(256),
            last_prune_player_pos: None,
            tick_counter: 0,
            stats: WaterSimStats::default(),
            y_buckets: std::array::from_fn(|_| Vec::new()),
            sync_updates_buffer: Vec::with_capacity(256),
            lava_check_buffer: HashSet::with_capacity(256),
        }
    }

    /// Enables or disables performance profiling.
    pub fn set_profiling(&mut self, enabled: bool) {
        self.stats.profiling_enabled = enabled;
    }

    /// Returns true if enough time has passed for a simulation tick.
    /// Call this before process_simulation to throttle updates.
    pub fn should_tick(&self) -> bool {
        self.last_tick.elapsed().as_millis() >= self.tick_interval_ms as u128
    }

    /// Marks that a tick just occurred. Called automatically by process_simulation.
    fn mark_tick(&mut self) {
        self.last_tick = Instant::now();
    }

    /// Updates display masses for all water cells for smooth visual transitions.
    /// Call this every frame with delta_time in seconds.
    /// Returns positions where display mass changed significantly (for GPU update).
    pub fn update_visuals(&mut self, delta_time: f32) -> Vec<Vector3<i32>> {
        let mut changed = Vec::new();
        for &pos in &self.active {
            if let Some(cell) = self.cells.get_mut(&pos) {
                let old_display = cell.display_mass;
                cell.update_display(delta_time);
                // Only mark as changed if the visual height changed noticeably
                if (cell.display_mass - old_display).abs() > 0.001 {
                    changed.push(pos);
                }
            }
        }
        changed
    }

    /// Gets the water mass at a position (0.0 if no water).
    #[inline]
    pub fn get_mass(&self, pos: Vector3<i32>) -> f32 {
        self.cells.get(&pos).map(|c| c.mass).unwrap_or(0.0)
    }

    /// Gets the effective water mass including pending changes from this tick.
    /// This allows cells processed later in a tick to see flow from earlier cells.
    ///
    /// The `has_world_water` closure should return true if the world has a Water
    /// block at the position (even if there's no grid cell). This ensures Water
    /// blocks placed by terrain/fill commands are treated as full water.
    #[inline]
    fn get_effective_mass<W>(&self, pos: Vector3<i32>, has_world_water: &W) -> f32
    where
        W: Fn(Vector3<i32>) -> bool,
    {
        let base = self.cells.get(&pos).map(|c| c.mass).unwrap_or_else(|| {
            // No grid cell - check if world has water block
            if has_world_water(pos) { MAX_MASS } else { 0.0 }
        });
        let (pending, _) = self
            .pending_changes
            .get(&pos)
            .copied()
            .unwrap_or((0.0, WaterType::Ocean));
        (base + pending).max(0.0)
    }

    /// Caches all neighbor masses for a position in a single pass.
    /// This reduces HashMap lookups from 6+ per cell to a single batch.
    #[inline]
    fn cache_neighbor_masses<F, B, W>(
        &self,
        pos: Vector3<i32>,
        is_solid: &F,
        is_out_of_bounds: &B,
        has_world_water: &W,
    ) -> NeighborMasses
    where
        F: Fn(Vector3<i32>) -> bool,
        B: Fn(Vector3<i32>) -> bool,
        W: Fn(Vector3<i32>) -> bool,
    {
        let below = pos + Vector3::new(0, -1, 0);
        let above = pos + Vector3::new(0, 1, 0);
        let pos_x = pos + Vector3::new(1, 0, 0);
        let neg_x = pos + Vector3::new(-1, 0, 0);
        let pos_z = pos + Vector3::new(0, 0, 1);
        let neg_z = pos + Vector3::new(0, 0, -1);

        let below_void = is_out_of_bounds(below);
        let below_solid = !below_void && is_solid(below);
        let above_solid = is_solid(above);
        let pos_x_solid = is_solid(pos_x);
        let neg_x_solid = is_solid(neg_x);
        let pos_z_solid = is_solid(pos_z);
        let neg_z_solid = is_solid(neg_z);

        NeighborMasses {
            below: if below_void || below_solid {
                0.0
            } else {
                self.get_effective_mass(below, has_world_water)
            },
            above: if above_solid {
                0.0
            } else {
                self.get_effective_mass(above, has_world_water)
            },
            pos_x: if pos_x_solid {
                0.0
            } else {
                self.get_effective_mass(pos_x, has_world_water)
            },
            neg_x: if neg_x_solid {
                0.0
            } else {
                self.get_effective_mass(neg_x, has_world_water)
            },
            pos_z: if pos_z_solid {
                0.0
            } else {
                self.get_effective_mass(pos_z, has_world_water)
            },
            neg_z: if neg_z_solid {
                0.0
            } else {
                self.get_effective_mass(neg_z, has_world_water)
            },
            below_void,
            below_solid,
            above_solid,
            pos_x_solid,
            neg_x_solid,
            pos_z_solid,
            neg_z_solid,
        }
    }

    /// Gets a water cell at a position (None if no water).
    #[inline]
    pub fn get_cell(&self, pos: Vector3<i32>) -> Option<&WaterCell> {
        self.cells.get(&pos)
    }

    /// Returns true if there's significant water at this position.
    #[inline]
    pub fn has_water(&self, pos: Vector3<i32>) -> bool {
        self.cells.get(&pos).map(|c| c.has_water()).unwrap_or(false)
    }

    /// Returns true if the cell at this position is a source.
    #[inline]
    pub fn is_source(&self, pos: Vector3<i32>) -> bool {
        self.cells.get(&pos).map(|c| c.is_source).unwrap_or(false)
    }

    /// Sets water at a position. If mass <= MIN_MASS, removes the cell.
    pub fn set_water(
        &mut self,
        pos: Vector3<i32>,
        mass: f32,
        is_source: bool,
        water_type: WaterType,
    ) {
        if mass <= MIN_MASS && !is_source {
            self.cells.remove(&pos);
            self.active.remove(&pos);
        } else {
            let cell = self.cells.entry(pos).or_default();
            cell.mass = if is_source { MAX_MASS } else { mass };
            cell.is_source = is_source;
            cell.stable_ticks = 0;
            cell.water_type = water_type;
            self.active.insert(pos);
        }
    }

    /// Adds water at a position (creates cell if needed).
    pub fn add_water(&mut self, pos: Vector3<i32>, amount: f32, water_type: WaterType) {
        if amount <= 0.0 {
            return;
        }
        let cell = self.cells.entry(pos).or_default();
        cell.mass += amount;
        cell.stable_ticks = 0;
        // Inherit type if adding to empty cell, otherwise existing type dominates
        if cell.mass <= amount {
            cell.water_type = water_type;
        }
        self.active.insert(pos);
    }

    /// Removes water from a position. Returns the amount actually removed.
    pub fn remove_water(&mut self, pos: Vector3<i32>, amount: f32) -> f32 {
        if let Some(cell) = self.cells.get_mut(&pos) {
            if cell.is_source {
                return 0.0; // Can't remove from source
            }
            let removed = amount.min(cell.mass);
            cell.mass -= removed;
            cell.stable_ticks = 0;
            self.active.insert(pos);
            if cell.mass <= MIN_MASS {
                self.cells.remove(&pos);
                self.active.remove(&pos);
            }
            removed
        } else {
            0.0
        }
    }

    /// Places a source block at a position (infinite water).
    pub fn place_source(&mut self, pos: Vector3<i32>, water_type: WaterType) {
        self.set_water(pos, MAX_MASS, true, water_type);
        // Activate neighbors for flow
        self.activate_neighbors(pos);
    }

    /// Removes a source block, leaving normal water.
    pub fn remove_source(&mut self, pos: Vector3<i32>) {
        if let Some(cell) = self.cells.get_mut(&pos) {
            cell.is_source = false;
            cell.stable_ticks = 0;
            self.active.insert(pos);
        }
    }

    /// Marks a position and its neighbors as needing update (deduped via set).
    pub fn activate_neighbors(&mut self, pos: Vector3<i32>) {
        if self.dirty_positions.insert(pos) {
            // only iterate neighbors when pos was newly inserted
            for (dx, dy, dz) in ORTHO_DIRS {
                self.dirty_positions.insert(pos + Vector3::new(dx, dy, dz));
            }
        }
    }

    /// Called when a solid block is placed - removes water at that position.
    pub fn on_block_placed(&mut self, pos: Vector3<i32>) {
        self.cells.remove(&pos);
        self.active.remove(&pos);
        self.activate_neighbors(pos);
    }

    /// Called when a solid block is removed - water may flow into this space.
    pub fn on_block_removed(&mut self, pos: Vector3<i32>) {
        self.activate_neighbors(pos);
    }

    /// Calculates how water should flow from a position.
    ///
    /// Uses W-Shadow algorithm:
    /// 1. Flow down (gravity) - fill below up to MAX_MASS + MAX_COMPRESS
    /// 2. Flow horizontal - equalize with neighbors
    /// 3. Flow up (pressure) - only if mass > MAX_MASS
    ///
    /// The `is_out_of_bounds` closure should return true for positions outside the world.
    /// Water flowing out of bounds is destroyed (drains into void).
    ///
    /// The `has_world_water` closure should return true if the world has a Water block
    /// at the position, even if there's no grid cell. This is critical for proper flow
    /// when water was placed via terrain generation or fill commands.
    pub fn calculate_flow<F, B, W>(
        &self,
        pos: Vector3<i32>,
        is_solid: F,
        is_out_of_bounds: B,
        has_world_water: &W,
    ) -> FlowResult
    where
        F: Fn(Vector3<i32>) -> bool,
        B: Fn(Vector3<i32>) -> bool,
        W: Fn(Vector3<i32>) -> bool,
    {
        let mut result = FlowResult::default();

        let cell = match self.cells.get(&pos) {
            Some(c) if c.has_water() => c,
            _ => return result,
        };

        let mass = cell.mass;
        let mut remaining = mass;

        // Positions
        let below = pos + Vector3::new(0, -1, 0);
        let above = pos + Vector3::new(0, 1, 0);
        let neighbor_positions = [
            pos + Vector3::new(1, 0, 0),
            pos + Vector3::new(-1, 0, 0),
            pos + Vector3::new(0, 0, 1),
            pos + Vector3::new(0, 0, -1),
        ];
        let mut neighbor_mass = [0.0f32; 4];
        let mut neighbor_open = [false; 4];

        // 1. Flow DOWN (gravity) - highest priority
        // Special case: if below is out of bounds, water drains into void
        if is_out_of_bounds(below) {
            // Drain all water into the void - no damping needed
            result.down = remaining;
            remaining = 0.0;
        } else if !is_solid(below) {
            // Use effective mass to see pending changes from earlier this tick
            let below_mass = self.get_effective_mass(below, has_world_water);
            let space_below = (MAX_MASS + MAX_COMPRESS) - below_mass;
            if space_below > MIN_MASS {
                // Gravity always wins - water falls without damping when there's space
                // Only apply damping when filling into existing water (to prevent oscillation)
                let flow = if below_mass < MIN_MASS {
                    // Falling into air/empty - transfer all mass that fits
                    remaining.min(space_below)
                } else {
                    // Filling into existing water - apply damping
                    remaining.min(space_below) * FLOW_DAMPING
                };
                if flow > MIN_MASS {
                    result.down = flow;
                    remaining -= flow;
                }
            }
        }

        // 2. Flow HORIZONTAL (equalization)
        if remaining > MIN_FLOW {
            let flow_refs: [&mut f32; 4] = [
                &mut result.pos_x,
                &mut result.neg_x,
                &mut result.pos_z,
                &mut result.neg_z,
            ];

            let mut lower_count = 0;
            let mut total_mass = remaining;

            for i in 0..4 {
                let neighbor_pos = neighbor_positions[i];
                if !is_solid(neighbor_pos) {
                    let m = self.get_effective_mass(neighbor_pos, has_world_water);
                    if m < remaining {
                        neighbor_mass[i] = m;
                        neighbor_open[i] = true;
                        total_mass += m;
                        lower_count += 1;
                    }
                }
            }

            if lower_count > 0 {
                let avg_mass = total_mass / (lower_count + 1) as f32;

                // Adjust flow rate based on water type
                let flow_rate = match cell.water_type {
                    WaterType::River => FLOW_DAMPING * 1.5,
                    WaterType::Swamp => FLOW_DAMPING * 0.3,
                    WaterType::Lake => FLOW_DAMPING * 0.7,
                    _ => FLOW_DAMPING,
                }
                .min(1.0);

                for i in 0..4 {
                    if neighbor_open[i]
                        && neighbor_mass[i] < remaining
                        && neighbor_mass[i] < avg_mass
                    {
                        let mut flow = (avg_mass - neighbor_mass[i]) * flow_rate;

                        // Keep a minimum trickle to avoid stuck thin layers
                        if flow < MIN_FLOW && remaining > MIN_FLOW * 2.0 {
                            flow = MIN_FLOW;
                        }

                        if flow >= MIN_FLOW && remaining > flow {
                            *flow_refs[i] = flow;
                            remaining -= flow;
                        }
                    }
                }
            }
        }

        // 3. Flow UP - only under pressure (mass > MAX_MASS)
        // Water only rises when compressed beyond its normal capacity.
        // This prevents water from "climbing" out of containers and creating
        // circulation loops where water exits, falls, rises back up, and re-enters.
        if !is_solid(above) && remaining > MAX_MASS {
            let above_mass = self.get_effective_mass(above, has_world_water);
            let excess = remaining - MAX_MASS;
            let space_above = MAX_MASS - above_mass;
            if space_above > MIN_FLOW {
                let flow = excess.min(space_above) * FLOW_DAMPING;
                if flow > MIN_FLOW {
                    result.up = flow;
                }
            }
        }

        result
    }

    /// Optimized flow calculation using pre-cached neighbor masses.
    /// Avoids repeated HashMap lookups by using NeighborMasses computed once per cell.
    #[inline]
    fn calculate_flow_cached(&self, pos: Vector3<i32>, neighbors: &NeighborMasses) -> FlowResult {
        let mut result = FlowResult::default();

        let cell = match self.cells.get(&pos) {
            Some(c) if c.has_water() => c,
            _ => return result,
        };

        let mass = cell.mass;
        let mut remaining = mass;

        // 1. Flow DOWN (gravity) - highest priority
        if neighbors.below_void {
            // Drain all water into the void
            result.down = remaining;
            remaining = 0.0;
        } else if !neighbors.below_solid {
            let space_below = (MAX_MASS + MAX_COMPRESS) - neighbors.below;
            if space_below > MIN_MASS {
                let flow = if neighbors.below < MIN_MASS {
                    remaining.min(space_below)
                } else {
                    remaining.min(space_below) * FLOW_DAMPING
                };
                if flow > MIN_MASS {
                    result.down = flow;
                    remaining -= flow;
                }
            }
        }

        // 2. Flow HORIZONTAL (equalization)
        if remaining > MIN_FLOW {
            // Collect neighbors that can accept water (using cached masses)
            let horizontal = [
                (!neighbors.pos_x_solid, neighbors.pos_x, &mut result.pos_x),
                (!neighbors.neg_x_solid, neighbors.neg_x, &mut result.neg_x),
                (!neighbors.pos_z_solid, neighbors.pos_z, &mut result.pos_z),
                (!neighbors.neg_z_solid, neighbors.neg_z, &mut result.neg_z),
            ];

            // Count valid neighbors and calculate total mass for averaging
            let mut total_mass = remaining;
            let mut neighbor_count = 1; // Start with 1 for self
            let mut lower_count = 0;

            for (can_flow, neighbor_mass, _) in &horizontal {
                if *can_flow && *neighbor_mass < remaining {
                    total_mass += *neighbor_mass;
                    neighbor_count += 1;
                    lower_count += 1;
                }
            }

            if lower_count > 0 {
                let avg_mass = total_mass / neighbor_count as f32;

                // Adjust flow rate based on water type
                let flow_rate = match cell.water_type {
                    WaterType::River => FLOW_DAMPING * 1.5,
                    WaterType::Swamp => FLOW_DAMPING * 0.3,
                    WaterType::Lake => FLOW_DAMPING * 0.7,
                    _ => FLOW_DAMPING,
                }
                .min(1.0);

                // Flow to neighbors below average
                for (can_flow, neighbor_mass, flow_ref) in horizontal {
                    if can_flow && neighbor_mass < remaining && neighbor_mass < avg_mass {
                        let mut flow = (avg_mass - neighbor_mass) * flow_rate;

                        if flow < MIN_FLOW && remaining > MIN_FLOW * 2.0 {
                            flow = MIN_FLOW;
                        }

                        if flow >= MIN_FLOW && remaining > flow {
                            *flow_ref = flow;
                            remaining -= flow;
                        }
                    }
                }
            }
        }

        // 3. Flow UP - only under pressure (mass > MAX_MASS)
        if !neighbors.above_solid && remaining > MAX_MASS {
            let excess = remaining - MAX_MASS;
            let space_above = MAX_MASS - neighbors.above;
            if space_above > MIN_FLOW {
                let flow = excess.min(space_above) * FLOW_DAMPING;
                if flow > MIN_FLOW {
                    result.up = flow;
                }
            }
        }

        result
    }

    /// Applies pending changes from the flow simulation.
    fn apply_pending_changes(&mut self) {
        for (pos, (delta, water_type)) in self.pending_changes.drain() {
            if let Some(cell) = self.cells.get_mut(&pos) {
                if !cell.is_source {
                    cell.mass = (cell.mass + delta).max(0.0);
                    cell.stable_ticks = 0;
                    if cell.mass <= MIN_MASS {
                        self.cells.remove(&pos);
                        self.active.remove(&pos);
                        continue;
                    }
                    // Inherit type if influx is significant
                    if delta > 0.0 && delta > cell.mass * 0.5 {
                        cell.water_type = water_type;
                    }
                }
                self.active.insert(pos);
            } else if delta > MIN_MASS {
                // Create new water cell
                self.cells.insert(
                    pos,
                    WaterCell {
                        mass: delta,
                        display_mass: delta,
                        is_source: false,
                        stable_ticks: 0,
                        water_type,
                    },
                );
                self.active.insert(pos);
            }
        }
    }

    /// Performs one tick of water simulation.
    ///
    /// Returns a tuple of:
    /// - List of positions that changed (for GPU upload)
    /// - List of water cell updates for multiplayer synchronization
    ///
    /// ## Performance Optimizations
    /// - Y-layer bucket sort: O(n) instead of O(n log n)
    /// - Cached neighbor masses: Reduced HashMap lookups
    /// - Lazy pruning: Only when active set exceeds threshold
    /// - Reusable buffers: No per-tick allocations
    ///
    /// # Arguments
    /// * `is_solid` - Returns true if a block at position is solid (water can't flow there)
    /// * `is_out_of_bounds` - Returns true if position is outside the world (water drains into void)
    /// * `has_world_water` - Returns true if world has a Water block at position (even without grid cell)
    /// * `player_pos` - Player position for prioritizing nearby water updates
    pub fn tick<F, B, W>(
        &mut self,
        is_solid: F,
        is_out_of_bounds: B,
        has_world_water: W,
        player_pos: Vector3<f32>,
    ) -> (Vec<Vector3<i32>>, Vec<WaterCellSyncUpdate>)
    where
        F: Fn(Vector3<i32>) -> bool,
        B: Fn(Vector3<i32>) -> bool,
        W: Fn(Vector3<i32>) -> bool,
    {
        let tick_start = if self.stats.profiling_enabled {
            Some(Instant::now())
        } else {
            None
        };

        self.tick_counter = self.tick_counter.wrapping_add(1);

        // Reuse buffer instead of allocating
        self.changed_positions_buffer.clear();

        // Add dirty positions to active set
        let dirty: Vec<_> = self.dirty_positions.drain().collect();
        for pos in dirty {
            if self.has_water(pos) {
                self.active.insert(pos);
            }
        }

        // OPTIMIZATION: Lazy pruning - only prune when active set exceeds threshold
        // This amortizes the O(n) cost instead of paying it every tick
        if self.active.len() > PRUNE_THRESHOLD {
            self.prune_far_sets(player_pos);
        }

        let sort_start = if self.stats.profiling_enabled {
            Some(Instant::now())
        } else {
            None
        };

        // OPTIMIZATION: Y-layer bucket sort (O(n) instead of O(n log n))
        // Bottom-first processing is CRITICAL for draining: lower cells must
        // flow out first so their pending_changes create space that upper cells
        // can see via get_effective_mass() and flow into during the same tick.
        let radius_sq = self.simulation_radius * self.simulation_radius;

        // Reuse Y-layer buckets (indices 0-511)
        for bucket in self.y_buckets.iter_mut() {
            bucket.clear();
        }

        // Distribute active cells into Y buckets (O(n))
        for &pos in &self.active {
            let dx = pos.x as f32 - player_pos.x;
            let dy = pos.y as f32 - player_pos.y;
            let dz = pos.z as f32 - player_pos.z;
            let dist_sq = dx * dx + dy * dy + dz * dz;

            if dist_sq <= radius_sq {
                // Clamp Y to valid bucket range (0-511)
                let y_index = (pos.y.max(0) as usize).min(Y_BUCKET_COUNT - 1);
                self.y_buckets[y_index].push(pos);
            }
        }

        if let Some(start) = sort_start {
            self.stats.last_sort_duration = start.elapsed();
        }

        let flow_start = if self.stats.profiling_enabled {
            Some(Instant::now())
        } else {
            None
        };

        // Process cells from lowest Y to highest (bucket sort order)
        let mut process_count = 0;
        let mut cells_flowed = 0;

        // Reuse deactivate buffer
        self.deactivate_buffer.clear();

        // Evaporation constants
        const EVAPORATION_THRESHOLD: f32 = 0.3;
        const EVAPORATION_RATE: f32 = 0.005;
        const VERY_THIN_THRESHOLD: f32 = 0.1;

        'outer: for bucket in &self.y_buckets {
            for &pos in bucket {
                if process_count >= self.max_updates_per_frame {
                    break 'outer;
                }
                process_count += 1;

                // OPTIMIZATION: Cache all neighbor masses in a single batch lookup
                let neighbors =
                    self.cache_neighbor_masses(pos, &is_solid, &is_out_of_bounds, &has_world_water);

                // Use cached flow calculation
                let flow = self.calculate_flow_cached(pos, &neighbors);

                // Get water type to propagate
                let water_type = self
                    .cells
                    .get(&pos)
                    .map(|c| c.water_type)
                    .unwrap_or(WaterType::Ocean);

                if flow.has_flow() {
                    cells_flowed += 1;

                    // Record outflow from this cell
                    let total_out = flow.total_outflow();
                    let entry = self.pending_changes.entry(pos).or_insert((0.0, water_type));
                    entry.0 -= total_out;
                    self.changed_positions_buffer.push(pos);

                    // Record inflow to neighbors (but NOT to out-of-bounds - water drains into void)
                    let below = pos + Vector3::new(0, -1, 0);
                    if flow.down > MIN_FLOW && !neighbors.below_void {
                        let entry = self
                            .pending_changes
                            .entry(below)
                            .or_insert((0.0, water_type));
                        entry.0 += flow.down;
                        entry.1 = water_type;
                        self.changed_positions_buffer.push(below);
                    }

                    if flow.up > MIN_FLOW {
                        let above = pos + Vector3::new(0, 1, 0);
                        let entry = self
                            .pending_changes
                            .entry(above)
                            .or_insert((0.0, water_type));
                        entry.0 += flow.up;
                        entry.1 = water_type;
                        self.changed_positions_buffer.push(above);
                    }
                    if flow.pos_x > MIN_FLOW {
                        let neighbor = pos + Vector3::new(1, 0, 0);
                        let entry = self
                            .pending_changes
                            .entry(neighbor)
                            .or_insert((0.0, water_type));
                        entry.0 += flow.pos_x;
                        entry.1 = water_type;
                        self.changed_positions_buffer.push(neighbor);
                    }
                    if flow.neg_x > MIN_FLOW {
                        let neighbor = pos + Vector3::new(-1, 0, 0);
                        let entry = self
                            .pending_changes
                            .entry(neighbor)
                            .or_insert((0.0, water_type));
                        entry.0 += flow.neg_x;
                        entry.1 = water_type;
                        self.changed_positions_buffer.push(neighbor);
                    }
                    if flow.pos_z > MIN_FLOW {
                        let neighbor = pos + Vector3::new(0, 0, 1);
                        let entry = self
                            .pending_changes
                            .entry(neighbor)
                            .or_insert((0.0, water_type));
                        entry.0 += flow.pos_z;
                        entry.1 = water_type;
                        self.changed_positions_buffer.push(neighbor);
                    }
                    if flow.neg_z > MIN_FLOW {
                        let neighbor = pos + Vector3::new(0, 0, -1);
                        let entry = self
                            .pending_changes
                            .entry(neighbor)
                            .or_insert((0.0, water_type));
                        entry.0 += flow.neg_z;
                        entry.1 = water_type;
                        self.changed_positions_buffer.push(neighbor);
                    }

                    // Reset stability counter
                    if let Some(cell) = self.cells.get_mut(&pos) {
                        cell.stable_ticks = 0;

                        // Even while flowing, very thin water evaporates to break deadlocks
                        if !cell.is_source && cell.mass < VERY_THIN_THRESHOLD {
                            cell.mass -= EVAPORATION_RATE;
                            if cell.mass <= MIN_MASS {
                                self.cells.remove(&pos);
                                self.active.remove(&pos);
                                self.changed_positions_buffer.push(pos);
                                continue;
                            }
                        }
                    }

                    // Wake up neighbors for chain draining
                    for (dx, dy, dz) in ORTHO_DIRS {
                        self.dirty_positions.insert(pos + Vector3::new(dx, dy, dz));
                    }
                } else {
                    // No flow - increment stability counter and apply evaporation
                    if let Some(cell) = self.cells.get_mut(&pos) {
                        cell.stable_ticks = cell.stable_ticks.saturating_add(1);

                        let is_evaporating = !cell.is_source
                            && cell.mass < EVAPORATION_THRESHOLD
                            && cell.stable_ticks > 5;

                        if is_evaporating {
                            cell.mass -= EVAPORATION_RATE;
                            if cell.mass <= MIN_MASS {
                                self.cells.remove(&pos);
                                self.active.remove(&pos);
                                self.changed_positions_buffer.push(pos);
                                continue;
                            }
                            self.changed_positions_buffer.push(pos);
                        }

                        if cell.is_stable() && !is_evaporating {
                            self.deactivate_buffer.push(pos);
                        }
                    }
                }
            }
        }

        if let Some(start) = flow_start {
            self.stats.last_flow_duration = start.elapsed();
        }

        let apply_start = if self.stats.profiling_enabled {
            Some(Instant::now())
        } else {
            None
        };

        // Apply all pending changes
        self.apply_pending_changes();

        // Deactivate stable cells
        let deactivate_count = self.deactivate_buffer.len();
        for pos in self.deactivate_buffer.drain(..) {
            self.active.remove(&pos);
        }

        // Deduplicate changed positions
        self.changed_positions_buffer
            .sort_by_key(|a| (a.x, a.y, a.z));
        self.changed_positions_buffer.dedup();

        if let Some(start) = apply_start {
            self.stats.last_apply_duration = start.elapsed();
        }

        // Update profiling stats
        if let Some(start) = tick_start {
            self.stats.last_tick_duration = start.elapsed();
            self.stats.last_cells_processed = process_count;
            self.stats.last_cells_flowed = cells_flowed;
            self.stats.last_cells_deactivated = deactivate_count;
        }

        // Collect sync updates for multiplayer before moving the buffer
        // This iterates over changed positions and creates sync updates with current cell state
        self.sync_updates_buffer.clear();
        for &pos in &self.changed_positions_buffer {
            if let Some(cell) = self.cells.get(&pos) {
                self.sync_updates_buffer.push(WaterCellSyncUpdate {
                    position: pos,
                    mass: cell.mass,
                    is_source: cell.is_source,
                    water_type: cell.water_type,
                });
            } else {
                // Cell was removed - send update with mass 0 to indicate removal
                self.sync_updates_buffer.push(WaterCellSyncUpdate {
                    position: pos,
                    mass: 0.0,
                    is_source: false,
                    water_type: WaterType::Ocean,
                });
            }
        }

        // Move out the buffer to avoid cloning; recreate an empty buffer with same capacity
        let changed_out = std::mem::take(&mut self.changed_positions_buffer);
        let cap = changed_out.capacity().max(256);
        self.changed_positions_buffer = Vec::with_capacity(cap);

        let sync_out = std::mem::take(&mut self.sync_updates_buffer);
        let sync_cap = sync_out.capacity().max(64);
        self.sync_updates_buffer = Vec::with_capacity(sync_cap);

        (changed_out, sync_out)
    }

    /// Returns the number of water cells.
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Returns the number of active (potentially flowing) cells.
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Returns the number of dirty positions waiting to be processed.
    pub fn dirty_count(&self) -> usize {
        self.dirty_positions.len()
    }

    /// Forces ALL water cells to become active (for debugging stuck water).
    /// Returns the number of cells activated.
    pub fn force_all_active(&mut self) -> usize {
        let count = self.cells.len();
        for pos in self.cells.keys().cloned().collect::<Vec<_>>() {
            self.active.insert(pos);
            // Also reset stable ticks so they'll try to flow
            if let Some(cell) = self.cells.get_mut(&pos) {
                cell.stable_ticks = 0;
            }
        }
        count
    }

    /// Debug: Get detailed info about water cells near a position.
    /// Returns a vector of (position, mass, is_active, stable_ticks) for cells within radius.
    pub fn debug_sample_cells(
        &self,
        center: Vector3<i32>,
        radius: i32,
    ) -> Vec<(Vector3<i32>, f32, bool, u8)> {
        let mut samples = Vec::new();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                for dz in -radius..=radius {
                    let pos = center + Vector3::new(dx, dy, dz);
                    if let Some(cell) = self.cells.get(&pos) {
                        samples.push((
                            pos,
                            cell.mass,
                            self.active.contains(&pos),
                            cell.stable_ticks,
                        ));
                    }
                }
            }
        }
        // Sort by Y descending (top to bottom) for easier reading
        samples.sort_by_key(|b| std::cmp::Reverse(b.0.y));
        samples
    }

    /// Debug: Analyze why a specific cell isn't flowing.
    /// Returns detailed diagnostic info.
    pub fn debug_flow_analysis<F, B, W>(
        &self,
        pos: Vector3<i32>,
        is_solid: F,
        is_out_of_bounds: B,
        has_world_water: &W,
    ) -> String
    where
        F: Fn(Vector3<i32>) -> bool,
        B: Fn(Vector3<i32>) -> bool,
        W: Fn(Vector3<i32>) -> bool,
    {
        let mut info = format!("=== Flow Analysis for {:?} ===\n", pos);

        // Check if cell exists
        match self.cells.get(&pos) {
            None => {
                info.push_str("No water cell at this position\n");
                if has_world_water(pos) {
                    info.push_str("BUT world has Water block here!\n");
                }
                return info;
            }
            Some(cell) => {
                info.push_str(&format!(
                    "Cell: mass={:.3}, source={}, stable_ticks={}\n",
                    cell.mass, cell.is_source, cell.stable_ticks
                ));
                info.push_str(&format!("Active: {}\n", self.active.contains(&pos)));
            }
        }

        let below = pos + Vector3::new(0, -1, 0);
        let above = pos + Vector3::new(0, 1, 0);

        // Check below
        info.push_str(&format!("\nBELOW {:?}:\n", below));
        info.push_str(&format!(
            "  is_out_of_bounds: {}\n",
            is_out_of_bounds(below)
        ));
        info.push_str(&format!("  is_solid: {}\n", is_solid(below)));
        info.push_str(&format!("  has_world_water: {}\n", has_world_water(below)));
        let below_mass = self.get_effective_mass(below, has_world_water);
        info.push_str(&format!("  effective_mass: {:.3}\n", below_mass));
        let space = (MAX_MASS + MAX_COMPRESS) - below_mass;
        info.push_str(&format!("  space_available: {:.3}\n", space));
        if let Some(cell) = self.cells.get(&below) {
            info.push_str(&format!("  cell_mass: {:.3}\n", cell.mass));
        } else {
            info.push_str("  no cell exists\n");
        }
        if let Some((pending, _)) = self.pending_changes.get(&below) {
            info.push_str(&format!("  pending_changes: {:.3}\n", pending));
        }

        // Check above
        info.push_str(&format!("\nABOVE {:?}:\n", above));
        info.push_str(&format!("  is_solid: {}\n", is_solid(above)));
        info.push_str(&format!(
            "  effective_mass: {:.3}\n",
            self.get_effective_mass(above, has_world_water)
        ));

        // Check horizontal neighbors
        for (name, offset) in [
            ("+X", Vector3::new(1, 0, 0)),
            ("-X", Vector3::new(-1, 0, 0)),
            ("+Z", Vector3::new(0, 0, 1)),
            ("-Z", Vector3::new(0, 0, -1)),
        ] {
            let neighbor = pos + offset;
            info.push_str(&format!("\n{} {:?}:\n", name, neighbor));
            info.push_str(&format!("  is_solid: {}\n", is_solid(neighbor)));
            info.push_str(&format!(
                "  effective_mass: {:.3}\n",
                self.get_effective_mass(neighbor, has_world_water)
            ));
        }

        // Calculate what flow WOULD happen
        let flow = self.calculate_flow(pos, &is_solid, is_out_of_bounds, has_world_water);
        info.push_str(&format!(
            "\nCalculated flow: down={:.3}, up={:.3}, +x={:.3}, -x={:.3}, +z={:.3}, -z={:.3}\n",
            flow.down, flow.up, flow.pos_x, flow.neg_x, flow.pos_z, flow.neg_z
        ));
        info.push_str(&format!("has_flow: {}\n", flow.has_flow()));

        // Global stats
        info.push_str(&format!(
            "\nGlobal: {} cells, {} active\n",
            self.cells.len(),
            self.active.len()
        ));

        // Mass distribution of nearby cells (within 5 blocks)
        let mut nearby_masses: Vec<f32> = Vec::new();
        let mut nearby_with_air_neighbor = 0;
        for (cell_pos, cell) in &self.cells {
            let dx = (cell_pos.x - pos.x).abs();
            let dy = (cell_pos.y - pos.y).abs();
            let dz = (cell_pos.z - pos.z).abs();
            if dx <= 5 && dy <= 5 && dz <= 5 {
                nearby_masses.push(cell.mass);
                // Check if this cell has any non-solid, non-water neighbor
                for (ddx, ddy, ddz) in ORTHO_DIRS {
                    let neighbor = *cell_pos + Vector3::new(ddx, ddy, ddz);
                    if !is_solid(neighbor)
                        && self.get_effective_mass(neighbor, has_world_water) < MIN_FLOW
                    {
                        nearby_with_air_neighbor += 1;
                        break;
                    }
                }
            }
        }
        if !nearby_masses.is_empty() {
            nearby_masses.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let min = nearby_masses.first().unwrap();
            let max = nearby_masses.last().unwrap();
            let sum: f32 = nearby_masses.iter().sum();
            let avg = sum / nearby_masses.len() as f32;
            info.push_str(&format!(
                "Nearby (5 block radius): {} cells, mass min={:.3} max={:.3} avg={:.3}\n",
                nearby_masses.len(),
                min,
                max,
                avg
            ));
            info.push_str(&format!(
                "Nearby cells with air neighbor: {}\n",
                nearby_with_air_neighbor
            ));
        }

        info
    }

    /// Prunes active and dirty sets to a maximum radius from player to avoid unbounded growth.
    fn prune_far_sets(&mut self, player_pos: Vector3<f32>) {
        let radius_sq = self.simulation_radius * self.simulation_radius;
        self.active
            .retain(|p| is_within_radius_sq(p, &player_pos, radius_sq));
        self.dirty_positions
            .retain(|p| is_within_radius_sq(p, &player_pos, radius_sq));
    }

    /// Returns an iterator over all water cells.
    pub fn iter(&self) -> impl Iterator<Item = (&Vector3<i32>, &WaterCell)> {
        self.cells.iter()
    }

    /// Clears all water.
    pub fn clear(&mut self) {
        self.cells.clear();
        self.active.clear();
        self.pending_changes.clear();
        self.dirty_positions.clear();
        self.changed_positions_buffer.clear();
        self.deactivate_buffer.clear();
        self.sync_updates_buffer.clear();
        self.lava_check_buffer.clear();
        self.last_prune_player_pos = None;
        self.tick_counter = 0;
        self.stats = WaterSimStats::default();
        for bucket in self.y_buckets.iter_mut() {
            bucket.clear();
        }
    }

    /// Processes water flow simulation.
    /// Uses tick_interval_ms to throttle simulation speed.
    ///
    /// Returns a list of water cell updates for multiplayer synchronization.
    /// When running as server, these should be broadcast to all connected clients.
    pub fn process_simulation(
        &mut self,
        world: &mut crate::world::World,
        lava_grid: &mut crate::lava::LavaGrid,
        player_pos: Vector3<f32>,
    ) -> Vec<WaterCellSyncUpdate> {
        use crate::chunk::BlockType;
        use crate::constants::TEXTURE_SIZE_Y;

        let texture_height = TEXTURE_SIZE_Y as i32;

        // NOTE: We no longer auto-sync terrain Water blocks into the simulation grid.
        // Terrain water (lakes, oceans) stays STATIC and doesn't simulate unless
        // explicitly activated via activate_adjacent_terrain_water() when a player
        // breaks a block next to water. This prevents cascade activation through
        // entire lake/cave systems (which caused 40k+ active cells and performance issues).
        //
        // Player-placed water sources go through place_source() which adds them properly.
        // Terrain water acts as infinite static water - it renders but doesn't flow
        // until disturbed.
        //
        // We DO still use dirty_positions to wake up EXISTING water cells in the grid.

        // Throttle simulation ticks based on tick_interval_ms
        if !self.should_tick() {
            return Vec::new();
        }
        self.mark_tick();

        // Create a closure that checks if a block is solid
        // Also returns true for unloaded chunks (blocks water flow until chunk loads)
        let is_solid = |pos: Vector3<i32>| -> bool {
            if pos.y < 0 || pos.y >= texture_height {
                return true;
            }
            world.get_block(pos).map(|b| b.is_solid()).unwrap_or(true)
        };

        let is_out_of_bounds = |pos: Vector3<i32>| -> bool { pos.y < 0 };

        // Check if world has a Water block (even without grid cell).
        // This is critical for proper flow calculation - Water blocks placed via
        // terrain generation or fill commands should be treated as full water.
        let has_world_water = |pos: Vector3<i32>| -> bool {
            if pos.y < 0 || pos.y >= texture_height {
                return false;
            }
            matches!(world.get_block(pos), Some(BlockType::Water))
        };

        // Run water simulation tick
        let (changed_positions, sync_updates) =
            self.tick(is_solid, is_out_of_bounds, has_world_water, player_pos);

        // Update world blocks and GPU for changed water cells
        for &pos in &changed_positions {
            if pos.y < 0 || pos.y >= texture_height {
                continue;
            }

            let has_water = self.has_water(pos);
            let current_block = world.get_block(pos);
            let water_type = self
                .cells
                .get(&pos)
                .map(|c| c.water_type)
                .unwrap_or(WaterType::Ocean);

            match (current_block, has_water) {
                (Some(BlockType::Air), true) => {
                    world.set_water_block(pos, water_type);
                    world.invalidate_minimap_cache(pos.x, pos.z);
                }
                (Some(BlockType::Water), false) => {
                    world.set_block(pos, BlockType::Air);
                    world.invalidate_minimap_cache(pos.x, pos.z);
                }
                (Some(BlockType::Lava), true) => {
                    // Water flows into lava - creates cobblestone
                    self.cells.remove(&pos);
                    self.active.remove(&pos);
                    lava_grid.on_block_placed(pos);
                    world.set_block(pos, BlockType::Cobblestone);
                    world.invalidate_minimap_cache(pos.x, pos.z);
                }
                (Some(BlockType::Model), true) => {
                    // Set waterlogged = true
                    if let Some(mut data) = world.get_model_data(pos)
                        && !data.waterlogged
                    {
                        data.waterlogged = true;
                        world.set_model_block(pos, data.model_id, data.rotation, true);
                    }
                }
                (Some(BlockType::Model), false) => {
                    // Set waterlogged = false
                    if let Some(mut data) = world.get_model_data(pos)
                        && data.waterlogged
                    {
                        data.waterlogged = false;
                        world.set_model_block(pos, data.model_id, data.rotation, false);
                    }
                }
                _ => {}
            }
        }

        // Check for water-lava adjacency and create cobblestone
        // This handles cases where water is adjacent to lava but not flowing into it
        if !changed_positions.is_empty() {
            // Check only around cells that changed this tick (and their neighbors)
            self.lava_check_buffer.clear();
            for &pos in &changed_positions {
                if pos.y >= 0 && pos.y < texture_height {
                    self.lava_check_buffer.insert(pos);
                    for (dx, dy, dz) in ORTHO_DIRS {
                        self.lava_check_buffer
                            .insert(pos + Vector3::new(dx, dy, dz));
                    }
                }
            }

            for &pos in &self.lava_check_buffer {
                if pos.y < 0 || pos.y >= texture_height {
                    continue;
                }
                // Check adjacent positions for lava
                let neighbors = [
                    pos + Vector3::new(1, 0, 0),
                    pos + Vector3::new(-1, 0, 0),
                    pos + Vector3::new(0, 1, 0),
                    pos + Vector3::new(0, -1, 0),
                    pos + Vector3::new(0, 0, 1),
                    pos + Vector3::new(0, 0, -1),
                ];

                for neighbor in neighbors {
                    let neighbor_is_lava = lava_grid.has_lava(neighbor)
                        || world
                            .get_block(neighbor)
                            .map(|b| b == BlockType::Lava)
                            .unwrap_or(false);

                    if neighbor_is_lava && self.has_water(pos) {
                        lava_grid.set_lava(neighbor, 0.0, false); // Removes the lava cell
                        world.set_block(neighbor, BlockType::Cobblestone);
                        world.invalidate_minimap_cache(neighbor.x, neighbor.z);
                        break;
                    }
                }
            }
        }

        // Visual smoothing: run once per simulation tick instead of every frame
        let _ = self.update_visuals(self.tick_interval_ms as f32 / 1000.0);

        sync_updates
    }

    /// Checks adjacent blocks for terrain water and adds them to the water grid.
    pub fn activate_adjacent_terrain_water(
        &mut self,
        world: &crate::world::World,
        pos: Vector3<i32>,
    ) {
        use crate::chunk::BlockType;
        use crate::constants::TEXTURE_SIZE_Y;

        let directions = [
            Vector3::new(1, 0, 0),
            Vector3::new(-1, 0, 0),
            Vector3::new(0, 1, 0),
            Vector3::new(0, -1, 0),
            Vector3::new(0, 0, 1),
            Vector3::new(0, 0, -1),
        ];

        for dir in directions {
            let neighbor = pos + dir;
            if neighbor.y < 0 || neighbor.y >= TEXTURE_SIZE_Y as i32 {
                continue;
            }

            if let Some(BlockType::Water) = world.get_block(neighbor) {
                if !self.has_water(neighbor) {
                    let water_type = world.get_water_type(neighbor).unwrap_or(WaterType::Ocean);
                    self.place_source(neighbor, water_type);
                } else {
                    self.activate_neighbors(neighbor);
                }
            }
        }
    }

    /// Returns all source positions for serialization.
    pub fn get_source_positions(&self) -> Vec<[i32; 3]> {
        self.cells
            .iter()
            .filter(|(_, cell)| cell.is_source)
            .map(|(pos, _)| [pos.x, pos.y, pos.z])
            .collect()
    }

    /// Loads sources from serialized positions.
    /// This also sets BlockType::Water in the world for each source.
    pub fn load_sources(&mut self, positions: &[[i32; 3]], world: &mut crate::world::World) {
        for [x, y, z] in positions {
            let pos = Vector3::new(*x, *y, *z);
            let water_type = world.get_water_type(pos).unwrap_or(WaterType::Ocean);
            self.place_source(pos, water_type);
            world.set_water_block(pos, water_type);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn never_solid(_: Vector3<i32>) -> bool {
        false
    }

    fn floor_solid(pos: Vector3<i32>) -> bool {
        pos.y < 0 // Floor at y=0
    }

    fn never_out_of_bounds(_: Vector3<i32>) -> bool {
        false
    }

    fn void_below_zero(pos: Vector3<i32>) -> bool {
        pos.y < 0 // Void below y=0
    }

    fn no_world_water(_: Vector3<i32>) -> bool {
        false // No water blocks in world (tests only use grid cells)
    }

    #[test]
    fn test_water_cell_creation() {
        let cell = WaterCell::new(0.5, WaterType::Ocean);
        assert_eq!(cell.mass, 0.5);
        assert!(!cell.is_source);
        assert!(cell.has_water());
        assert!(!cell.is_full());
    }

    #[test]
    fn test_source_cell() {
        let cell = WaterCell::source(WaterType::Ocean);
        assert_eq!(cell.mass, MAX_MASS);
        assert!(cell.is_source);
        assert!(cell.is_full());
    }

    #[test]
    fn test_water_grid_basic() {
        let mut grid = WaterGrid::new();
        let pos = Vector3::new(0, 0, 0);

        assert!(!grid.has_water(pos));
        assert_eq!(grid.get_mass(pos), 0.0);

        grid.set_water(pos, 0.5, false, WaterType::Ocean);
        assert!(grid.has_water(pos));
        assert_eq!(grid.get_mass(pos), 0.5);

        grid.set_water(pos, 0.0, false, WaterType::Ocean);
        assert!(!grid.has_water(pos));
    }

    #[test]
    fn test_prune_far_sets() {
        let mut grid = WaterGrid::new();
        grid.simulation_radius = 4.0;

        // Far entry
        grid.cells.insert(
            Vector3::new(10, 0, 0),
            WaterCell {
                mass: 1.0,
                display_mass: 1.0,
                is_source: false,
                stable_ticks: 0,
                water_type: WaterType::Ocean,
            },
        );
        grid.active.insert(Vector3::new(10, 0, 0));
        grid.dirty_positions.insert(Vector3::new(10, 0, 0));

        // Near entry
        grid.cells.insert(
            Vector3::new(1, 0, 0),
            WaterCell {
                mass: 1.0,
                display_mass: 1.0,
                is_source: false,
                stable_ticks: 0,
                water_type: WaterType::Ocean,
            },
        );
        grid.active.insert(Vector3::new(1, 0, 0));
        grid.dirty_positions.insert(Vector3::new(1, 0, 0));

        grid.prune_far_sets(Vector3::new(0.0, 0.0, 0.0));

        assert!(grid.active.contains(&Vector3::new(1, 0, 0)));
        assert!(grid.dirty_positions.contains(&Vector3::new(1, 0, 0)));
        assert!(!grid.active.contains(&Vector3::new(10, 0, 0)));
        assert!(!grid.dirty_positions.contains(&Vector3::new(10, 0, 0)));
    }

    #[test]
    fn test_water_flow_down() {
        let mut grid = WaterGrid::new();
        let pos = Vector3::new(0, 5, 0);

        grid.set_water(pos, 1.0, false, WaterType::Ocean);

        let flow = grid.calculate_flow(pos, floor_solid, never_out_of_bounds, &no_world_water);
        assert!(flow.down > 0.0, "Water should flow down");
        // Note: upward equalization flow can happen when above is empty
        // (this is needed for draining rooms with side exits)
        assert!(
            flow.down > flow.up,
            "Downward flow should be greater than upward equalization"
        );
    }

    #[test]
    fn test_water_flow_horizontal() {
        let mut grid = WaterGrid::new();
        let pos = Vector3::new(0, 0, 0);
        let _neighbor = Vector3::new(1, 0, 0);

        // Water at pos, empty at neighbor, floor below both
        grid.set_water(pos, 0.8, false, WaterType::Ocean);

        let flow = grid.calculate_flow(pos, floor_solid, never_out_of_bounds, &no_world_water);
        // Should flow down AND horizontal
        assert!(
            flow.pos_x > 0.0 || flow.neg_x > 0.0 || flow.pos_z > 0.0 || flow.neg_z > 0.0,
            "Water should flow horizontally to equalize"
        );
    }

    #[test]
    fn test_source_maintains_mass() {
        let mut grid = WaterGrid::new();
        let pos = Vector3::new(0, 5, 0);

        grid.place_source(pos, WaterType::Ocean);
        assert!(grid.is_source(pos));
        assert_eq!(grid.get_mass(pos), MAX_MASS);

        // Remove water shouldn't affect source
        let removed = grid.remove_water(pos, 0.5);
        assert_eq!(removed, 0.0);
        assert_eq!(grid.get_mass(pos), MAX_MASS);
    }

    #[test]
    fn test_tick_simulation() {
        let mut grid = WaterGrid::new();
        let pos = Vector3::new(0, 5, 0);
        let player_pos = Vector3::new(0.0, 0.0, 0.0);

        grid.set_water(pos, 1.0, false, WaterType::Ocean);

        // Run a tick
        let (changed, _sync_updates) =
            grid.tick(floor_solid, never_out_of_bounds, no_world_water, player_pos);

        // Water should have flowed
        assert!(!changed.is_empty(), "Water should have moved");

        // Original position should have less water
        assert!(
            grid.get_mass(pos) < 1.0,
            "Water should have flowed out of original position"
        );

        // Below should have some water now
        let below = pos + Vector3::new(0, -1, 0);
        assert!(
            grid.get_mass(below) > 0.0,
            "Water should have flowed to below"
        );
    }

    #[test]
    fn test_water_drains_into_void() {
        let mut grid = WaterGrid::new();
        let pos = Vector3::new(0, 0, 0); // At y=0, above void
        let player_pos = Vector3::new(0.0, 0.0, 0.0);

        grid.set_water(pos, 1.0, false, WaterType::Ocean);

        // Run a tick with void below y=0
        let _changed = grid.tick(never_solid, void_below_zero, no_world_water, player_pos);

        // Water should have drained (flowed into void)
        assert!(
            grid.get_mass(pos) < 1.0,
            "Water should have drained into void"
        );

        // Below (y=-1) should NOT have water (it's in the void)
        let below = pos + Vector3::new(0, -1, 0);
        assert_eq!(grid.get_mass(below), 0.0, "Water should not exist in void");
    }

    #[test]
    fn test_water_evaporates() {
        let mut grid = WaterGrid::new();
        let pos = Vector3::new(0, 0, 0);

        grid.set_water(pos, MIN_MASS / 2.0, false, WaterType::Ocean);
        assert!(!grid.has_water(pos), "Tiny amounts should evaporate");
    }
}

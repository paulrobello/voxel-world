//! Water flow simulation using cellular automata.
//!
//! This module implements a mass-based water system where each cell can hold
//! a variable amount of water (0.0 to 1.0+). Water flows using the W-Shadow
//! algorithm: down first, then horizontally to equalize, then up under pressure.
//!
//! Water is stored in a sparse HashMap to minimize memory usage since most
//! blocks don't contain water.

#![allow(dead_code)]

use crate::constants::ORTHO_DIRS;
use nalgebra::Vector3;
use std::collections::{HashMap, HashSet};

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

/// A single water cell with mass and properties.
#[derive(Debug, Clone, Copy)]
pub struct WaterCell {
    /// Amount of water in this cell (0.0 = empty, 1.0 = full block).
    /// Can exceed 1.0 for pressurized water (underwater columns).
    pub mass: f32,

    /// If true, this cell generates infinite water (ocean surface, springs).
    /// Source cells always maintain mass = MAX_MASS.
    pub is_source: bool,

    /// Ticks since last flow activity. Used to deactivate stable water.
    pub stable_ticks: u8,
}

impl Default for WaterCell {
    fn default() -> Self {
        Self {
            mass: 0.0,
            is_source: false,
            stable_ticks: 0,
        }
    }
}

impl WaterCell {
    /// Creates a new water cell with the given mass.
    pub fn new(mass: f32) -> Self {
        Self {
            mass,
            is_source: false,
            stable_ticks: 0,
        }
    }

    /// Creates a source cell (infinite water).
    pub fn source() -> Self {
        Self {
            mass: MAX_MASS,
            is_source: true,
            stable_ticks: 0,
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
    #[inline]
    pub fn visual_height(&self) -> f32 {
        self.mass.clamp(0.0, 1.0)
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
    /// Key: position, Value: mass delta (positive = add, negative = remove).
    pending_changes: HashMap<Vector3<i32>, f32>,

    /// Positions that need to be checked next tick.
    dirty_positions: HashSet<Vector3<i32>>,

    /// Maximum updates per frame.
    pub max_updates_per_frame: usize,

    /// Simulation radius in blocks. Water outside this radius from the player is dormant.
    /// This ensures water is only simulated in loaded chunks near the player.
    pub simulation_radius: f32,
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
        }
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
        let pending = self.pending_changes.get(&pos).copied().unwrap_or(0.0);
        (base + pending).max(0.0)
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
    pub fn set_water(&mut self, pos: Vector3<i32>, mass: f32, is_source: bool) {
        if mass <= MIN_MASS && !is_source {
            self.cells.remove(&pos);
            self.active.remove(&pos);
        } else {
            let cell = self.cells.entry(pos).or_default();
            cell.mass = if is_source { MAX_MASS } else { mass };
            cell.is_source = is_source;
            cell.stable_ticks = 0;
            self.active.insert(pos);
        }
    }

    /// Adds water at a position (creates cell if needed).
    pub fn add_water(&mut self, pos: Vector3<i32>, amount: f32) {
        if amount <= 0.0 {
            return;
        }
        let cell = self.cells.entry(pos).or_default();
        cell.mass += amount;
        cell.stable_ticks = 0;
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
    pub fn place_source(&mut self, pos: Vector3<i32>) {
        self.set_water(pos, MAX_MASS, true);
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
        let neighbors = [
            (pos + Vector3::new(1, 0, 0), &mut result.pos_x),
            (pos + Vector3::new(-1, 0, 0), &mut result.neg_x),
            (pos + Vector3::new(0, 0, 1), &mut result.pos_z),
            (pos + Vector3::new(0, 0, -1), &mut result.neg_z),
        ];

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
            // Find neighbors that can accept water
            // Use effective mass to see pending changes from earlier this tick
            let mut lower_neighbors: Vec<(Vector3<i32>, f32, &mut f32)> = Vec::new();

            for (neighbor_pos, flow_ref) in neighbors {
                if !is_solid(neighbor_pos) {
                    let neighbor_mass = self.get_effective_mass(neighbor_pos, has_world_water);
                    if neighbor_mass < remaining {
                        lower_neighbors.push((neighbor_pos, neighbor_mass, flow_ref));
                    }
                }
            }

            if !lower_neighbors.is_empty() {
                // Calculate target level (equalize water across cells)
                let total_mass: f32 =
                    remaining + lower_neighbors.iter().map(|(_, m, _)| *m).sum::<f32>();
                let avg_mass = total_mass / (lower_neighbors.len() + 1) as f32;

                // Flow to neighbors below average
                for (_, neighbor_mass, flow_ref) in lower_neighbors {
                    if neighbor_mass < avg_mass {
                        let mut flow = (avg_mass - neighbor_mass) * FLOW_DAMPING;

                        // CRITICAL FIX: Ensure minimum flow to maintain gradient propagation.
                        // When equalization produces tiny flow values (< MIN_FLOW), we still
                        // need to flow SOMETHING to prevent water from getting "stuck".
                        // This is especially important for drain scenarios where mass
                        // differences become very small but water should still drain.
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

    /// Applies pending changes from the flow simulation.
    fn apply_pending_changes(&mut self) {
        for (pos, delta) in self.pending_changes.drain() {
            if let Some(cell) = self.cells.get_mut(&pos) {
                if !cell.is_source {
                    cell.mass = (cell.mass + delta).max(0.0);
                    cell.stable_ticks = 0;
                    if cell.mass <= MIN_MASS {
                        self.cells.remove(&pos);
                        self.active.remove(&pos);
                        continue;
                    }
                }
                self.active.insert(pos);
            } else if delta > MIN_MASS {
                // Create new water cell
                self.cells.insert(
                    pos,
                    WaterCell {
                        mass: delta,
                        is_source: false,
                        stable_ticks: 0,
                    },
                );
                self.active.insert(pos);
            }
        }
    }

    /// Performs one tick of water simulation.
    ///
    /// Returns a list of positions that changed (for GPU upload).
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
    ) -> Vec<Vector3<i32>>
    where
        F: Fn(Vector3<i32>) -> bool,
        B: Fn(Vector3<i32>) -> bool,
        W: Fn(Vector3<i32>) -> bool,
    {
        let mut changed_positions = Vec::new();

        // Add dirty positions to active set
        let dirty: Vec<_> = self.dirty_positions.drain().collect();
        for pos in dirty {
            if self.has_water(pos) {
                self.active.insert(pos);
            }
        }

        // Prune far-away tracked cells to keep sets bounded
        self.prune_far_sets(player_pos);

        // Filter and sort active cells by distance to player
        // Only simulate water within simulation_radius (ensures chunks are loaded)
        let radius_sq = self.simulation_radius * self.simulation_radius;
        let mut active_list: Vec<_> = self
            .active
            .iter()
            .copied()
            .filter_map(|pos| {
                let dx = pos.x as f32 - player_pos.x;
                let dy = pos.y as f32 - player_pos.y;
                let dz = pos.z as f32 - player_pos.z;
                let dist_sq = dx * dx + dy * dy + dz * dz;
                if dist_sq <= radius_sq {
                    Some((pos, dist_sq))
                } else {
                    None
                }
            })
            .collect();

        // Sort primarily by Y coordinate (lowest first), then by distance.
        // Bottom-first processing is CRITICAL for draining: lower cells must
        // flow out first so their pending_changes create space that upper cells
        // can see via get_effective_mass() and flow into during the same tick.
        // Distance is secondary tiebreaker to prioritize water near the player.
        active_list.sort_by(|(pos_a, dist_a), (pos_b, dist_b)| {
            pos_a.y.cmp(&pos_b.y).then_with(|| {
                dist_a
                    .partial_cmp(dist_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        });

        // Extract just the positions
        let active_list: Vec<_> = active_list.into_iter().map(|(pos, _)| pos).collect();

        // Process up to max_updates_per_frame cells
        let process_count = active_list.len().min(self.max_updates_per_frame);
        let mut deactivate = Vec::new();

        for &pos in active_list.iter().take(process_count) {
            let flow = self.calculate_flow(pos, &is_solid, &is_out_of_bounds, &has_world_water);

            // Evaporation constants
            const EVAPORATION_THRESHOLD: f32 = 0.3;
            const EVAPORATION_RATE: f32 = 0.005;
            // Very thin water evaporates even while flowing to break circulation deadlocks
            // (e.g., water trapped in 1x1 pits that keeps pushing tiny amounts up/down)
            const VERY_THIN_THRESHOLD: f32 = 0.1;

            if flow.has_flow() {
                // Record outflow from this cell
                let total_out = flow.total_outflow();
                *self.pending_changes.entry(pos).or_insert(0.0) -= total_out;
                changed_positions.push(pos);

                // Record inflow to neighbors (but NOT to out-of-bounds - water drains into void)
                let below = pos + Vector3::new(0, -1, 0);
                if flow.down > MIN_FLOW && !is_out_of_bounds(below) {
                    *self.pending_changes.entry(below).or_insert(0.0) += flow.down;
                    changed_positions.push(below);
                }
                // Note: Water flowing down into void is just removed, not added anywhere

                if flow.up > MIN_FLOW {
                    let above = pos + Vector3::new(0, 1, 0);
                    if !is_out_of_bounds(above) {
                        *self.pending_changes.entry(above).or_insert(0.0) += flow.up;
                        changed_positions.push(above);
                    }
                }
                if flow.pos_x > MIN_FLOW {
                    let neighbor = pos + Vector3::new(1, 0, 0);
                    if !is_out_of_bounds(neighbor) {
                        *self.pending_changes.entry(neighbor).or_insert(0.0) += flow.pos_x;
                        changed_positions.push(neighbor);
                    }
                }
                if flow.neg_x > MIN_FLOW {
                    let neighbor = pos + Vector3::new(-1, 0, 0);
                    if !is_out_of_bounds(neighbor) {
                        *self.pending_changes.entry(neighbor).or_insert(0.0) += flow.neg_x;
                        changed_positions.push(neighbor);
                    }
                }
                if flow.pos_z > MIN_FLOW {
                    let neighbor = pos + Vector3::new(0, 0, 1);
                    if !is_out_of_bounds(neighbor) {
                        *self.pending_changes.entry(neighbor).or_insert(0.0) += flow.pos_z;
                        changed_positions.push(neighbor);
                    }
                }
                if flow.neg_z > MIN_FLOW {
                    let neighbor = pos + Vector3::new(0, 0, -1);
                    if !is_out_of_bounds(neighbor) {
                        *self.pending_changes.entry(neighbor).or_insert(0.0) += flow.neg_z;
                        changed_positions.push(neighbor);
                    }
                }

                // Reset stability counter
                if let Some(cell) = self.cells.get_mut(&pos) {
                    cell.stable_ticks = 0;

                    // Even while flowing, very thin water evaporates to break deadlocks
                    // This handles circulation loops where tiny amounts bounce back and forth
                    if !cell.is_source && cell.mass < VERY_THIN_THRESHOLD {
                        cell.mass -= EVAPORATION_RATE;
                        if cell.mass <= MIN_MASS {
                            self.cells.remove(&pos);
                            self.active.remove(&pos);
                            changed_positions.push(pos);
                            continue;
                        }
                    }
                }

                // When water flows OUT of this cell, wake up all neighbors
                // so they can flow into this now-emptier cell (chain draining).
                // Insert directly to dirty_positions (not via activate_neighbors
                // which would cause exponential growth by also re-inserting pos).
                for (dx, dy, dz) in ORTHO_DIRS {
                    self.dirty_positions.insert(pos + Vector3::new(dx, dy, dz));
                }
            } else {
                // No flow - increment stability counter and apply evaporation
                if let Some(cell) = self.cells.get_mut(&pos) {
                    cell.stable_ticks = cell.stable_ticks.saturating_add(1);

                    // Thin water evaporates slowly when stable (simulates absorption/evaporation)
                    // This cleans up residual puddles that can't drain
                    let is_evaporating = !cell.is_source
                        && cell.mass < EVAPORATION_THRESHOLD
                        && cell.stable_ticks > 5;

                    if is_evaporating {
                        cell.mass -= EVAPORATION_RATE;
                        if cell.mass <= MIN_MASS {
                            self.cells.remove(&pos);
                            self.active.remove(&pos);
                            changed_positions.push(pos);
                            continue;
                        }
                        changed_positions.push(pos);
                    }

                    // Don't deactivate cells that are still evaporating
                    if cell.is_stable() && !is_evaporating {
                        deactivate.push(pos);
                    }
                }
            }
        }

        // Apply all pending changes
        self.apply_pending_changes();

        // Deactivate stable cells
        for pos in deactivate {
            self.active.remove(&pos);
        }

        // Deduplicate changed positions (sort by x, y, z then dedup)
        changed_positions.sort_by(|a, b| (a.x, a.y, a.z).cmp(&(b.x, b.y, b.z)));
        changed_positions.dedup();

        changed_positions
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
        samples.sort_by(|a, b| b.0.y.cmp(&a.0.y));
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
        if let Some(&pending) = self.pending_changes.get(&below) {
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
        self.active.retain(|p| {
            let dx = p.x as f32 - player_pos.x;
            let dy = p.y as f32 - player_pos.y;
            let dz = p.z as f32 - player_pos.z;
            dx * dx + dy * dy + dz * dz <= radius_sq
        });
        self.dirty_positions.retain(|p| {
            let dx = p.x as f32 - player_pos.x;
            let dy = p.y as f32 - player_pos.y;
            let dz = p.z as f32 - player_pos.z;
            dx * dx + dy * dy + dz * dz <= radius_sq
        });
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
    }

    /// Processes water flow simulation.
    pub fn process_simulation(
        &mut self,
        world: &mut crate::world::World,
        lava_grid: &mut crate::lava::LavaGrid,
        player_pos: Vector3<f32>,
    ) {
        use crate::chunk::BlockType;
        use crate::constants::TEXTURE_SIZE_Y;

        let texture_height = TEXTURE_SIZE_Y as i32;

        // Sync dirty positions: if world has Water block but grid has no cell, create one.
        // This ensures water blocks placed by terrain or other systems can drain.
        let dirty_to_check: Vec<_> = self.dirty_positions.iter().copied().collect();
        for pos in dirty_to_check {
            if pos.y >= 0 && pos.y < texture_height {
                if let std::collections::hash_map::Entry::Vacant(e) = self.cells.entry(pos) {
                    if let Some(BlockType::Water) = world.get_block(pos) {
                        e.insert(WaterCell::new(MAX_MASS));
                        self.active.insert(pos);
                    }
                }
            }
        }

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
        let changed_positions = self.tick(is_solid, is_out_of_bounds, has_world_water, player_pos);

        // Update world blocks and GPU for changed water cells
        for pos in changed_positions {
            if pos.y < 0 || pos.y >= texture_height {
                continue;
            }

            let has_water = self.has_water(pos);
            let current_block = world.get_block(pos);

            match (current_block, has_water) {
                (Some(BlockType::Air), true) => {
                    world.set_block(pos, BlockType::Water);
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
                    if let Some(mut data) = world.get_model_data(pos) {
                        if !data.waterlogged {
                            data.waterlogged = true;
                            world.set_model_block(pos, data.model_id, data.rotation, true);
                        }
                    }
                }
                (Some(BlockType::Model), false) => {
                    // Set waterlogged = false
                    if let Some(mut data) = world.get_model_data(pos) {
                        if data.waterlogged {
                            data.waterlogged = false;
                            world.set_model_block(pos, data.model_id, data.rotation, false);
                        }
                    }
                }
                _ => {}
            }
        }

        // Check for water-lava adjacency and create cobblestone
        // This handles cases where water is adjacent to lava but not flowing into it
        let water_positions: Vec<_> = self.cells.keys().copied().collect();
        for pos in water_positions {
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
                // Check if neighbor has lava (either in world or lava grid)
                let neighbor_is_lava = lava_grid.has_lava(neighbor)
                    || world
                        .get_block(neighbor)
                        .map(|b| b == BlockType::Lava)
                        .unwrap_or(false);

                if neighbor_is_lava && self.has_water(pos) {
                    // Water touching lava - convert lava to cobblestone
                    lava_grid.set_lava(neighbor, 0.0, false); // Removes the lava cell
                    world.set_block(neighbor, BlockType::Cobblestone);
                    world.invalidate_minimap_cache(neighbor.x, neighbor.z);
                    break; // Only convert one lava per tick per water cell
                }
            }
        }
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
                    self.place_source(neighbor);
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
            self.place_source(pos);
            world.set_block(pos, crate::chunk::BlockType::Water);
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
        let cell = WaterCell::new(0.5);
        assert_eq!(cell.mass, 0.5);
        assert!(!cell.is_source);
        assert!(cell.has_water());
        assert!(!cell.is_full());
    }

    #[test]
    fn test_source_cell() {
        let cell = WaterCell::source();
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

        grid.set_water(pos, 0.5, false);
        assert!(grid.has_water(pos));
        assert_eq!(grid.get_mass(pos), 0.5);

        grid.set_water(pos, 0.0, false);
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
                is_source: false,
                stable_ticks: 0,
            },
        );
        grid.active.insert(Vector3::new(10, 0, 0));
        grid.dirty_positions.insert(Vector3::new(10, 0, 0));

        // Near entry
        grid.cells.insert(
            Vector3::new(1, 0, 0),
            WaterCell {
                mass: 1.0,
                is_source: false,
                stable_ticks: 0,
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

        grid.set_water(pos, 1.0, false);

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
        grid.set_water(pos, 0.8, false);

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

        grid.place_source(pos);
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

        grid.set_water(pos, 1.0, false);

        // Run a tick
        let changed = grid.tick(floor_solid, never_out_of_bounds, no_world_water, player_pos);

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

        grid.set_water(pos, 1.0, false);

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

        grid.set_water(pos, MIN_MASS / 2.0, false);
        assert!(!grid.has_water(pos), "Tiny amounts should evaporate");
    }
}

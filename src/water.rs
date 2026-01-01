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
pub const MIN_FLOW: f32 = 0.01;

/// Flow damping factor to prevent oscillation (0.0 to 1.0).
/// Lower values = more damping = slower but more stable flow.
pub const FLOW_DAMPING: f32 = 0.5;

/// Maximum water updates per frame (performance limit).
pub const DEFAULT_WATER_UPDATES_PER_FRAME: usize = 64;

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
    pub fn calculate_flow<F, B>(
        &self,
        pos: Vector3<i32>,
        is_solid: F,
        is_out_of_bounds: B,
    ) -> FlowResult
    where
        F: Fn(Vector3<i32>) -> bool,
        B: Fn(Vector3<i32>) -> bool,
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
            // Drain all water into the void
            result.down = remaining * FLOW_DAMPING;
            remaining -= result.down;
        } else if !is_solid(below) {
            let below_mass = self.get_mass(below);
            let space_below = (MAX_MASS + MAX_COMPRESS) - below_mass;
            if space_below > MIN_FLOW {
                // Flow as much as possible down
                let flow = remaining.min(space_below) * FLOW_DAMPING;
                if flow > MIN_FLOW {
                    result.down = flow;
                    remaining -= flow;
                }
            }
        }

        // 2. Flow HORIZONTAL (equalization)
        if remaining > MIN_FLOW {
            // Find neighbors that can accept water
            let mut lower_neighbors: Vec<(Vector3<i32>, f32, &mut f32)> = Vec::new();

            for (neighbor_pos, flow_ref) in neighbors {
                if !is_solid(neighbor_pos) {
                    let neighbor_mass = self.get_mass(neighbor_pos);
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
                        let flow = (avg_mass - neighbor_mass) * FLOW_DAMPING;
                        if flow > MIN_FLOW && remaining > flow {
                            *flow_ref = flow;
                            remaining -= flow;
                        }
                    }
                }
            }
        }

        // 3. Flow UP (pressure) - only if we have excess water
        if remaining > MAX_MASS && !is_solid(above) {
            let above_mass = self.get_mass(above);
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
    /// * `player_pos` - Player position for prioritizing nearby water updates
    pub fn tick<F, B>(
        &mut self,
        is_solid: F,
        is_out_of_bounds: B,
        player_pos: Vector3<f32>,
    ) -> Vec<Vector3<i32>>
    where
        F: Fn(Vector3<i32>) -> bool,
        B: Fn(Vector3<i32>) -> bool,
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

        // Sort by distance (closer = higher priority)
        active_list
            .sort_by(|(_, da), (_, db)| da.partial_cmp(db).unwrap_or(std::cmp::Ordering::Equal));

        // Extract just the positions
        let active_list: Vec<_> = active_list.into_iter().map(|(pos, _)| pos).collect();

        // Process up to max_updates_per_frame cells
        let process_count = active_list.len().min(self.max_updates_per_frame);
        let mut deactivate = Vec::new();

        for &pos in active_list.iter().take(process_count) {
            let flow = self.calculate_flow(pos, &is_solid, &is_out_of_bounds);

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
                }
            } else {
                // No flow - increment stability counter
                if let Some(cell) = self.cells.get_mut(&pos) {
                    cell.stable_ticks = cell.stable_ticks.saturating_add(1);
                    if cell.is_stable() {
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
        player_pos: Vector3<f32>,
    ) {
        use crate::chunk::BlockType;
        use crate::constants::TEXTURE_SIZE_Y;

        let texture_height = TEXTURE_SIZE_Y as i32;

        // Create a closure that checks if a block is solid
        // Also returns true for unloaded chunks (blocks water flow until chunk loads)
        let is_solid = |pos: Vector3<i32>| -> bool {
            if pos.y < 0 || pos.y >= texture_height {
                return true;
            }
            world.get_block(pos).map(|b| b.is_solid()).unwrap_or(true)
        };

        let is_out_of_bounds = |pos: Vector3<i32>| -> bool { pos.y < 0 };

        // Run water simulation tick
        let changed_positions = self.tick(is_solid, is_out_of_bounds, player_pos);

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
                _ => {}
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

        let flow = grid.calculate_flow(pos, floor_solid, never_out_of_bounds);
        assert!(flow.down > 0.0, "Water should flow down");
        assert!(flow.up == 0.0, "Water should not flow up without pressure");
    }

    #[test]
    fn test_water_flow_horizontal() {
        let mut grid = WaterGrid::new();
        let pos = Vector3::new(0, 0, 0);
        let _neighbor = Vector3::new(1, 0, 0);

        // Water at pos, empty at neighbor, floor below both
        grid.set_water(pos, 0.8, false);

        let flow = grid.calculate_flow(pos, floor_solid, never_out_of_bounds);
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
        let changed = grid.tick(floor_solid, never_out_of_bounds, player_pos);

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
        let _changed = grid.tick(never_solid, void_below_zero, player_pos);

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

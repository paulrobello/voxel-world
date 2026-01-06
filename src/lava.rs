//! Lava flow simulation using cellular automata.
//!
//! Similar to water flow but with key differences:
//! - Slower flow (lava is viscous)
//! - No upward pressure flow
//! - Interacts with water to create cobblestone

#![allow(dead_code)]

use crate::chunk::BlockType;
use crate::constants::ORTHO_DIRS;
use nalgebra::Vector3;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

/// Maximum lava mass a cell can hold.
pub const MAX_MASS: f32 = 1.0;

/// Minimum lava mass to track (below this, lava solidifies/disappears).
pub const MIN_MASS: f32 = 0.01;

/// Minimum flow amount.
pub const MIN_FLOW: f32 = 0.02;

/// Flow damping factor - lower = slower, more viscous flow.
pub const FLOW_DAMPING: f32 = 0.25;

/// Maximum lava updates per frame (lower than water for performance).
pub const DEFAULT_LAVA_UPDATES_PER_FRAME: usize = 128;

/// Default simulation radius in blocks.
pub const DEFAULT_SIMULATION_RADIUS: f32 = 48.0;

/// Default tick interval in milliseconds.
/// Lava is slower than water, so use a longer interval.
/// 100ms = 10 ticks/second
pub const DEFAULT_TICK_INTERVAL_MS: u64 = 100;

/// A single lava cell with mass and properties.
#[derive(Debug, Clone, Copy)]
pub struct LavaCell {
    /// Amount of lava in this cell (0.0 = empty, 1.0 = full block).
    pub mass: f32,

    /// If true, this cell generates infinite lava (lava lakes, volcanoes).
    pub is_source: bool,

    /// Ticks since last flow activity.
    pub stable_ticks: u8,
}

impl Default for LavaCell {
    fn default() -> Self {
        Self {
            mass: 0.0,
            is_source: false,
            stable_ticks: 0,
        }
    }
}

impl LavaCell {
    /// Creates a new lava cell with the given mass.
    pub fn new(mass: f32) -> Self {
        Self {
            mass,
            is_source: false,
            stable_ticks: 0,
        }
    }

    /// Creates a source cell (infinite lava).
    pub fn source() -> Self {
        Self {
            mass: MAX_MASS,
            is_source: true,
            stable_ticks: 0,
        }
    }

    /// Returns true if this cell has significant lava.
    #[inline]
    pub fn has_lava(&self) -> bool {
        self.mass > MIN_MASS
    }

    /// Returns true if this cell is full.
    #[inline]
    pub fn is_full(&self) -> bool {
        self.mass >= MAX_MASS
    }

    /// Returns true if this cell is stable (hasn't flowed recently).
    #[inline]
    pub fn is_stable(&self) -> bool {
        self.stable_ticks >= 15 // Lava stabilizes slower than water
    }
}

/// Flow result from calculating lava movement.
#[derive(Debug, Clone, Default)]
pub struct FlowResult {
    pub down: f32,
    pub pos_x: f32,
    pub neg_x: f32,
    pub pos_z: f32,
    pub neg_z: f32,
    // No upward flow for lava
}

impl FlowResult {
    pub fn has_flow(&self) -> bool {
        self.down > MIN_FLOW
            || self.pos_x > MIN_FLOW
            || self.neg_x > MIN_FLOW
            || self.pos_z > MIN_FLOW
            || self.neg_z > MIN_FLOW
    }

    pub fn total_outflow(&self) -> f32 {
        self.down + self.pos_x + self.neg_x + self.pos_z + self.neg_z
    }
}

/// Sparse storage for lava cells using HashMap.
pub struct LavaGrid {
    cells: HashMap<Vector3<i32>, LavaCell>,
    active: HashSet<Vector3<i32>>,
    pending_changes: HashMap<Vector3<i32>, f32>,
    dirty_positions: HashSet<Vector3<i32>>,
    pub max_updates_per_frame: usize,
    pub simulation_radius: f32,
    /// Tick interval in milliseconds. Controls simulation speed.
    pub tick_interval_ms: u64,
    /// Last time a simulation tick was run.
    last_tick: Instant,
}

impl Default for LavaGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl LavaGrid {
    pub fn new() -> Self {
        Self {
            cells: HashMap::with_capacity(1024),
            active: HashSet::with_capacity(256),
            pending_changes: HashMap::with_capacity(128),
            dirty_positions: HashSet::with_capacity(128),
            max_updates_per_frame: DEFAULT_LAVA_UPDATES_PER_FRAME,
            simulation_radius: DEFAULT_SIMULATION_RADIUS,
            tick_interval_ms: DEFAULT_TICK_INTERVAL_MS,
            last_tick: Instant::now(),
        }
    }

    /// Returns true if enough time has passed for a simulation tick.
    pub fn should_tick(&self) -> bool {
        self.last_tick.elapsed().as_millis() >= self.tick_interval_ms as u128
    }

    /// Marks that a tick just occurred.
    fn mark_tick(&mut self) {
        self.last_tick = Instant::now();
    }

    #[inline]
    pub fn get_mass(&self, pos: Vector3<i32>) -> f32 {
        self.cells.get(&pos).map(|c| c.mass).unwrap_or(0.0)
    }

    /// Gets the effective lava mass including pending changes from this tick.
    ///
    /// The `has_world_lava` closure should return true if the world has a Lava
    /// block at the position (even if there's no grid cell). This ensures Lava
    /// blocks placed by terrain/fill commands are treated as full lava.
    #[inline]
    fn get_effective_mass<W>(&self, pos: Vector3<i32>, has_world_lava: &W) -> f32
    where
        W: Fn(Vector3<i32>) -> bool,
    {
        let base = self.cells.get(&pos).map(|c| c.mass).unwrap_or_else(|| {
            // No grid cell - check if world has lava block
            if has_world_lava(pos) { MAX_MASS } else { 0.0 }
        });
        let pending = self.pending_changes.get(&pos).copied().unwrap_or(0.0);
        (base + pending).max(0.0)
    }

    #[inline]
    pub fn has_lava(&self, pos: Vector3<i32>) -> bool {
        self.cells.get(&pos).map(|c| c.has_lava()).unwrap_or(false)
    }

    #[inline]
    pub fn is_source(&self, pos: Vector3<i32>) -> bool {
        self.cells.get(&pos).map(|c| c.is_source).unwrap_or(false)
    }

    pub fn set_lava(&mut self, pos: Vector3<i32>, mass: f32, is_source: bool) {
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

    pub fn add_lava(&mut self, pos: Vector3<i32>, amount: f32) {
        if amount <= 0.0 {
            return;
        }
        let cell = self.cells.entry(pos).or_default();
        cell.mass += amount;
        cell.stable_ticks = 0;
        self.active.insert(pos);
    }

    pub fn remove_lava(&mut self, pos: Vector3<i32>, amount: f32) -> f32 {
        if let Some(cell) = self.cells.get_mut(&pos) {
            if cell.is_source {
                return 0.0;
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

    pub fn place_source(&mut self, pos: Vector3<i32>) {
        self.set_lava(pos, MAX_MASS, true);
        self.activate_neighbors(pos);
    }

    pub fn remove_source(&mut self, pos: Vector3<i32>) {
        if let Some(cell) = self.cells.get_mut(&pos) {
            cell.is_source = false;
            cell.stable_ticks = 0;
            self.active.insert(pos);
        }
    }

    pub fn activate_neighbors(&mut self, pos: Vector3<i32>) {
        if self.dirty_positions.insert(pos) {
            for (dx, dy, dz) in ORTHO_DIRS {
                self.dirty_positions.insert(pos + Vector3::new(dx, dy, dz));
            }
        }
    }

    pub fn on_block_placed(&mut self, pos: Vector3<i32>) {
        self.cells.remove(&pos);
        self.active.remove(&pos);
        self.activate_neighbors(pos);
    }

    pub fn on_block_removed(&mut self, pos: Vector3<i32>) {
        self.activate_neighbors(pos);
    }

    /// Calculates lava flow - similar to water but no upward pressure flow.
    ///
    /// The `has_world_lava` closure should return true if the world has a Lava block
    /// at the position, even if there's no grid cell. This is critical for proper flow
    /// when lava was placed via terrain generation or fill commands.
    pub fn calculate_flow<F, B, W>(
        &self,
        pos: Vector3<i32>,
        is_solid: F,
        is_out_of_bounds: B,
        has_world_lava: &W,
    ) -> FlowResult
    where
        F: Fn(Vector3<i32>) -> bool,
        B: Fn(Vector3<i32>) -> bool,
        W: Fn(Vector3<i32>) -> bool,
    {
        let mut result = FlowResult::default();

        let cell = match self.cells.get(&pos) {
            Some(c) if c.has_lava() => c,
            _ => return result,
        };

        let mass = cell.mass;
        let mut remaining = mass;

        let below = pos + Vector3::new(0, -1, 0);
        let neighbors = [
            (pos + Vector3::new(1, 0, 0), &mut result.pos_x),
            (pos + Vector3::new(-1, 0, 0), &mut result.neg_x),
            (pos + Vector3::new(0, 0, 1), &mut result.pos_z),
            (pos + Vector3::new(0, 0, -1), &mut result.neg_z),
        ];

        // 1. Flow DOWN (gravity) - highest priority
        if is_out_of_bounds(below) {
            result.down = remaining * FLOW_DAMPING;
            remaining -= result.down;
        } else if !is_solid(below) {
            // Use effective mass to see pending changes from earlier this tick
            let below_mass = self.get_effective_mass(below, has_world_lava);
            let space_below = MAX_MASS - below_mass;
            if space_below > MIN_FLOW {
                let flow = remaining.min(space_below) * FLOW_DAMPING;
                if flow > MIN_FLOW {
                    result.down = flow;
                    remaining -= flow;
                }
            }
        }

        // 2. Flow HORIZONTAL (equalization) - only if supported by solid below
        // Lava spreads more slowly horizontally
        if remaining > MIN_FLOW && is_solid(below) {
            // Use effective mass to see pending changes from earlier this tick
            let mut lower_neighbors: Vec<(Vector3<i32>, f32, &mut f32)> = Vec::new();

            for (neighbor_pos, flow_ref) in neighbors {
                if !is_solid(neighbor_pos) {
                    let neighbor_mass = self.get_effective_mass(neighbor_pos, has_world_lava);
                    if neighbor_mass < remaining {
                        lower_neighbors.push((neighbor_pos, neighbor_mass, flow_ref));
                    }
                }
            }

            if !lower_neighbors.is_empty() {
                let total_mass: f32 =
                    remaining + lower_neighbors.iter().map(|(_, m, _)| *m).sum::<f32>();
                let avg_mass = total_mass / (lower_neighbors.len() + 1) as f32;

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

        // No upward flow for lava (unlike water)

        result
    }

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
                self.cells.insert(
                    pos,
                    LavaCell {
                        mass: delta,
                        is_source: false,
                        stable_ticks: 0,
                    },
                );
                self.active.insert(pos);
            }
        }
    }

    /// Performs one tick of lava simulation.
    /// Returns (changed_positions, water_lava_contacts) for block updates.
    ///
    /// # Arguments
    /// * `has_world_lava` - Returns true if world has a Lava block at position (even without grid cell)
    pub fn tick<F, B, Wtr, Lva>(
        &mut self,
        is_solid: F,
        is_out_of_bounds: B,
        has_water: Wtr,
        has_world_lava: Lva,
        player_pos: Vector3<f32>,
    ) -> (Vec<Vector3<i32>>, Vec<Vector3<i32>>)
    where
        F: Fn(Vector3<i32>) -> bool,
        B: Fn(Vector3<i32>) -> bool,
        Wtr: Fn(Vector3<i32>) -> bool,
        Lva: Fn(Vector3<i32>) -> bool,
    {
        let mut changed_positions = Vec::new();
        let mut water_contacts = Vec::new();

        // Add dirty positions to active set
        let dirty: Vec<_> = self.dirty_positions.drain().collect();
        for pos in dirty {
            if self.has_lava(pos) {
                self.active.insert(pos);
            }
        }

        // Prune far-away tracked cells
        self.prune_far_sets(player_pos);

        // Filter and sort active cells by distance to player
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
        // Distance is secondary tiebreaker to prioritize lava near the player.
        active_list.sort_by(|(pos_a, dist_a), (pos_b, dist_b)| {
            pos_a.y.cmp(&pos_b.y).then_with(|| {
                dist_a
                    .partial_cmp(dist_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        });

        let active_list: Vec<_> = active_list.into_iter().map(|(pos, _)| pos).collect();

        let process_count = active_list.len().min(self.max_updates_per_frame);
        let mut deactivate = Vec::new();

        for &pos in active_list.iter().take(process_count) {
            // Check for water contact at this position or neighbors
            let check_positions = [
                pos,
                pos + Vector3::new(1, 0, 0),
                pos + Vector3::new(-1, 0, 0),
                pos + Vector3::new(0, 1, 0),
                pos + Vector3::new(0, -1, 0),
                pos + Vector3::new(0, 0, 1),
                pos + Vector3::new(0, 0, -1),
            ];

            for check_pos in check_positions {
                if has_water(check_pos) && self.has_lava(pos) {
                    water_contacts.push(pos);
                }
            }

            let flow = self.calculate_flow(pos, &is_solid, &is_out_of_bounds, &has_world_lava);

            if flow.has_flow() {
                let total_out = flow.total_outflow();
                *self.pending_changes.entry(pos).or_insert(0.0) -= total_out;
                changed_positions.push(pos);

                let below = pos + Vector3::new(0, -1, 0);
                if flow.down > MIN_FLOW && !is_out_of_bounds(below) {
                    *self.pending_changes.entry(below).or_insert(0.0) += flow.down;
                    changed_positions.push(below);
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

                if let Some(cell) = self.cells.get_mut(&pos) {
                    cell.stable_ticks = 0;
                }

                // When lava flows OUT of this cell, wake up all neighbors
                // so they can flow into this now-emptier cell (chain draining).
                // Insert directly to dirty_positions (not via activate_neighbors
                // which would cause exponential growth by also re-inserting pos).
                for (dx, dy, dz) in ORTHO_DIRS {
                    self.dirty_positions.insert(pos + Vector3::new(dx, dy, dz));
                }
            } else if let Some(cell) = self.cells.get_mut(&pos) {
                cell.stable_ticks = cell.stable_ticks.saturating_add(1);
                if cell.is_stable() {
                    deactivate.push(pos);
                }
            }
        }

        self.apply_pending_changes();

        for pos in deactivate {
            self.active.remove(&pos);
        }

        changed_positions.sort_by(|a, b| (a.x, a.y, a.z).cmp(&(b.x, b.y, b.z)));
        changed_positions.dedup();
        water_contacts.sort_by(|a, b| (a.x, a.y, a.z).cmp(&(b.x, b.y, b.z)));
        water_contacts.dedup();

        (changed_positions, water_contacts)
    }

    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Forces ALL lava cells to become active (for debugging stuck lava).
    pub fn force_all_active(&mut self) -> usize {
        let count = self.cells.len();
        for pos in self.cells.keys().cloned().collect::<Vec<_>>() {
            self.active.insert(pos);
            if let Some(cell) = self.cells.get_mut(&pos) {
                cell.stable_ticks = 0;
            }
        }
        count
    }

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

    pub fn iter(&self) -> impl Iterator<Item = (&Vector3<i32>, &LavaCell)> {
        self.cells.iter()
    }

    pub fn clear(&mut self) {
        self.cells.clear();
        self.active.clear();
        self.pending_changes.clear();
        self.dirty_positions.clear();
    }

    /// Processes lava flow simulation.
    /// Uses tick_interval_ms to throttle simulation speed.
    pub fn process_simulation(
        &mut self,
        world: &mut crate::world::World,
        water_grid: &mut crate::water::WaterGrid,
        player_pos: Vector3<f32>,
    ) {
        use crate::constants::TEXTURE_SIZE_Y;

        let texture_height = TEXTURE_SIZE_Y as i32;

        // Sync dirty positions: if world has Lava block but grid has no cell, create one.
        // Always run this (not throttled) so block placement is immediately responsive.
        let dirty_to_check: Vec<_> = self.dirty_positions.iter().copied().collect();
        for pos in dirty_to_check {
            if pos.y >= 0 && pos.y < texture_height {
                if let std::collections::hash_map::Entry::Vacant(e) = self.cells.entry(pos) {
                    if let Some(BlockType::Lava) = world.get_block(pos) {
                        e.insert(LavaCell::new(MAX_MASS));
                        self.active.insert(pos);
                    }
                }
            }
        }

        // Throttle simulation ticks based on tick_interval_ms
        if !self.should_tick() {
            return;
        }
        self.mark_tick();

        let is_solid = |pos: Vector3<i32>| -> bool {
            if pos.y < 0 || pos.y >= texture_height {
                return true;
            }
            world
                .get_block(pos)
                .map(|b| b.is_solid() && b != BlockType::Lava)
                .unwrap_or(true)
        };

        let is_out_of_bounds = |pos: Vector3<i32>| -> bool { pos.y < 0 };

        let has_water = |pos: Vector3<i32>| -> bool {
            // Check both the world block and water grid
            // Water might exist in the grid before being rendered as a block
            if water_grid.has_water(pos) {
                return true;
            }
            if let Some(block) = world.get_block(pos) {
                block == BlockType::Water
            } else {
                false
            }
        };

        // Check if world has a Lava block (even without grid cell).
        // This is critical for proper flow calculation - Lava blocks placed via
        // terrain generation or fill commands should be treated as full lava.
        let has_world_lava = |pos: Vector3<i32>| -> bool {
            if pos.y < 0 || pos.y >= texture_height {
                return false;
            }
            matches!(world.get_block(pos), Some(BlockType::Lava))
        };

        let (changed_positions, water_contacts) = self.tick(
            is_solid,
            is_out_of_bounds,
            has_water,
            has_world_lava,
            player_pos,
        );

        // Handle water-lava interactions first (create cobblestone)
        for pos in water_contacts {
            // Lava touching water creates cobblestone
            self.cells.remove(&pos);
            self.active.remove(&pos);
            water_grid.on_block_placed(pos);
            world.set_block(pos, BlockType::Cobblestone);
            world.invalidate_minimap_cache(pos.x, pos.z);
        }

        // Update world blocks for changed lava cells
        for pos in changed_positions {
            if pos.y < 0 || pos.y >= texture_height {
                continue;
            }

            let has_lava = self.has_lava(pos);
            let current_block = world.get_block(pos);

            match (current_block, has_lava) {
                (Some(BlockType::Air), true) => {
                    world.set_block(pos, BlockType::Lava);
                    world.invalidate_minimap_cache(pos.x, pos.z);
                }
                (Some(BlockType::Lava), false) => {
                    world.set_block(pos, BlockType::Air);
                    world.invalidate_minimap_cache(pos.x, pos.z);
                }
                (Some(BlockType::Water), true) => {
                    // Lava flows into water - creates cobblestone
                    self.cells.remove(&pos);
                    self.active.remove(&pos);
                    water_grid.on_block_placed(pos);
                    world.set_block(pos, BlockType::Cobblestone);
                    world.invalidate_minimap_cache(pos.x, pos.z);
                }
                _ => {}
            }
        }
    }

    /// Activates adjacent terrain lava for simulation.
    pub fn activate_adjacent_terrain_lava(
        &mut self,
        world: &crate::world::World,
        pos: Vector3<i32>,
    ) {
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

            if let Some(BlockType::Lava) = world.get_block(neighbor) {
                if !self.has_lava(neighbor) {
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
    /// This also sets BlockType::Lava in the world for each source.
    pub fn load_sources(&mut self, positions: &[[i32; 3]], world: &mut crate::world::World) {
        for [x, y, z] in positions {
            let pos = Vector3::new(*x, *y, *z);
            self.place_source(pos);
            world.set_block(pos, crate::chunk::BlockType::Lava);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn floor_solid(pos: Vector3<i32>) -> bool {
        pos.y < 0
    }

    fn never_out_of_bounds(_: Vector3<i32>) -> bool {
        false
    }

    fn no_water(_: Vector3<i32>) -> bool {
        false
    }

    fn no_world_lava(_: Vector3<i32>) -> bool {
        false // No lava blocks in world (tests only use grid cells)
    }

    #[test]
    fn test_lava_cell_creation() {
        let cell = LavaCell::new(0.5);
        assert_eq!(cell.mass, 0.5);
        assert!(!cell.is_source);
        assert!(cell.has_lava());
    }

    #[test]
    fn test_lava_flow_down() {
        let mut grid = LavaGrid::new();
        let pos = Vector3::new(0, 5, 0);

        grid.set_lava(pos, 1.0, false);

        let flow = grid.calculate_flow(pos, floor_solid, never_out_of_bounds, &no_world_lava);
        assert!(flow.down > 0.0, "Lava should flow down");
    }

    #[test]
    fn test_lava_tick() {
        let mut grid = LavaGrid::new();
        let pos = Vector3::new(0, 5, 0);
        let player_pos = Vector3::new(0.0, 0.0, 0.0);

        grid.set_lava(pos, 1.0, false);

        let (changed, _water_contacts) = grid.tick(
            floor_solid,
            never_out_of_bounds,
            no_water,
            no_world_lava,
            player_pos,
        );

        assert!(!changed.is_empty(), "Lava should have moved");
        assert!(
            grid.get_mass(pos) < 1.0,
            "Lava should have flowed out of original position"
        );
    }
}

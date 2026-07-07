//! Falling block system for gravity-affected blocks.
//!
//! Handles falling sand, gravel, and other gravity-affected blocks.
//! When a block loses support, it converts to a falling entity,
//! simulates physics, and converts back to a static block on landing.

use bytemuck::{Pod, Zeroable};
use nalgebra::Vector3;

use crate::chunk::BlockType;

/// Maximum number of falling blocks that can exist at once.
pub const MAX_FALLING_BLOCKS: usize = 256;

/// Gravity acceleration in blocks per second squared.
///
/// Exposed publicly so client-side prediction in `net::falling_block_sync`
/// reads the same constant the server simulation does — if we ever tune
/// gravity we can't have the two drift silently.
pub const GRAVITY: f32 = 20.0;

/// Maximum dt (seconds) fed into a single falling-block physics step.
///
/// Frame stalls (chunk generation, GPU uploads, window resize) can produce
/// multi-hundred-millisecond frames. Feeding that directly to the sim lets a
/// falling block skip the cell it should have landed on (PHY-004 tunneling).
/// The update loop clamps the raw frame dt to this before integrating, giving
/// a 20 fps simulation floor regardless of render hitches.
pub const MAX_PHYSICS_DT: f32 = 0.05; // 50 ms

/// Hard cap on how many cells a falling block may descend in one step.
///
/// `FallingBlock::update` samples a single cell at the block's new bottom
/// edge for collision, so the per-step fall must stay at 1 cell or a thin
/// floor between the old and new positions can be skipped. The terminal
/// velocity below is derived from this: at the max substep (`MAX_PHYSICS_DT`)
/// a block at terminal velocity moves exactly this many cells.
pub const MAX_FALL_CELLS_PER_STEP: i32 = 1;

/// Terminal downward velocity in cells per second.
///
/// `MAX_FALL_CELLS_PER_STEP / MAX_PHYSICS_DT` = 1 / 0.05 = 20 cells/s. Even a
/// pathological dt cannot push a block further than `MAX_FALL_CELLS_PER_STEP`
/// per step because downward velocity is clamped to this before integration.
pub const MAX_FALL_VELOCITY: f32 = MAX_FALL_CELLS_PER_STEP as f32 / MAX_PHYSICS_DT;

/// Decomposes a raw frame dt into physics substeps (PHY-004).
///
/// Clamps the total to `MAX_PHYSICS_DT` so a frame stall doesn't simulate
/// hundreds of milliseconds of physics at once, then splits the clamped total
/// into `N` equal substeps each `≤ MAX_PHYSICS_DT` that sum exactly to the
/// clamped total (no simulated time lost to substep rounding). Returns the
/// substeps and the clamped total so callers can log how much was dropped.
///
/// With the clamp in place `N` is normally 1; the loop generalizes so that
/// raising `MAX_PHYSICS_DT` or calling with a larger dt still produces
/// correctly-sized substeps instead of a single oversized step.
pub fn physics_substeps(raw_dt: f32) -> (Vec<f32>, f32) {
    let clamped = raw_dt.clamp(0.0, MAX_PHYSICS_DT);
    if clamped <= 0.0 {
        return (Vec::new(), 0.0);
    }
    let n = ((clamped / MAX_PHYSICS_DT).ceil() as usize).max(1);
    let sub = clamped / n as f32;
    (vec![sub; n], clamped)
}

/// A single falling block entity.
#[derive(Debug, Clone, Copy)]
pub struct FallingBlock {
    /// Unique entity ID for network sync.
    pub entity_id: u32,
    /// Position in world coordinates (center of block).
    pub position: Vector3<f32>,
    /// Velocity in blocks per second.
    pub velocity: Vector3<f32>,
    /// The type of block that is falling.
    pub block_type: BlockType,
    /// Time since block started falling (in seconds).
    pub age: f32,
}

impl FallingBlock {
    /// Creates a new falling block with the given entity ID.
    ///
    /// Position should be the center of the block (e.g., grid position + 0.5).
    pub fn new(entity_id: u32, position: Vector3<f32>, block_type: BlockType) -> Self {
        Self {
            entity_id,
            position,
            velocity: Vector3::zeros(),
            block_type,
            age: 0.0,
        }
    }

    /// Updates the falling block physics with world collision.
    ///
    /// `is_solid` should return true if the block at (x, y, z) is solid.
    /// Returns `Some(grid_position)` if the block has landed, `None` if still falling.
    pub fn update<F>(&mut self, delta_time: f32, is_solid: F) -> Option<Vector3<i32>>
    where
        F: Fn(i32, i32, i32) -> bool,
    {
        self.age += delta_time;

        // Apply gravity
        self.velocity.y -= GRAVITY * delta_time;

        // Terminal velocity backstop (PHY-004): cap downward speed so the
        // single-cell collision check below can never skip a floor cell, even
        // if delta_time is larger than expected. At MAX_PHYSICS_DT this limits
        // the per-step fall to MAX_FALL_CELLS_PER_STEP.
        if self.velocity.y < -MAX_FALL_VELOCITY {
            self.velocity.y = -MAX_FALL_VELOCITY;
        }

        // Calculate new position
        let new_pos = self.position + self.velocity * delta_time;

        // Check collision with ground (Y axis)
        // Check the block below the falling block's bottom edge
        let block_x = new_pos.x.floor() as i32;
        let block_y = (new_pos.y - 0.5).floor() as i32;
        let block_z = new_pos.z.floor() as i32;

        if is_solid(block_x, block_y, block_z) {
            // Land on the block above the solid one
            let land_pos = Vector3::new(block_x, block_y + 1, block_z);
            return Some(land_pos);
        }

        // No collision, update position
        self.position = new_pos;

        // Check if fallen too far (below world)
        if self.position.y < -64.0 {
            // Despawn by returning a position that will be ignored
            // (handled by caller checking bounds)
            return Some(Vector3::new(block_x, -100, block_z));
        }

        None
    }
}

/// GPU-compatible falling block data for shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GpuFallingBlock {
    /// Position XYZ + block type (as float)
    pub pos_type: [f32; 4],
    /// Velocity XYZ + age (for potential rotation animation)
    pub velocity_age: [f32; 4],
}

impl From<&FallingBlock> for GpuFallingBlock {
    fn from(fb: &FallingBlock) -> Self {
        Self {
            pos_type: [
                fb.position.x,
                fb.position.y,
                fb.position.z,
                fb.block_type as u8 as f32,
            ],
            velocity_age: [fb.velocity.x, fb.velocity.y, fb.velocity.z, fb.age],
        }
    }
}

/// Information about a block that has landed.
#[derive(Debug, Clone, Copy)]
pub struct LandedBlock {
    /// Unique entity ID for network sync.
    pub entity_id: u32,
    /// Grid position where the block landed.
    pub position: Vector3<i32>,
    /// The type of block that landed.
    pub block_type: BlockType,
}

/// Manages all active falling blocks.
pub struct FallingBlockSystem {
    blocks: Vec<FallingBlock>,
    /// Next entity ID to assign (wraps around at u32::MAX).
    next_entity_id: u32,
}

impl Default for FallingBlockSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl FallingBlockSystem {
    /// Creates a new empty falling block system.
    pub fn new() -> Self {
        Self {
            blocks: Vec::with_capacity(MAX_FALLING_BLOCKS),
            next_entity_id: 1, // Start at 1, 0 reserved for "no ID"
        }
    }

    /// Spawns a new falling block at the given grid position.
    ///
    /// The position is converted to center coordinates (grid + 0.5).
    /// Returns the assigned entity ID, or None if at capacity.
    pub fn spawn(&mut self, grid_position: Vector3<i32>, block_type: BlockType) -> Option<u32> {
        if self.blocks.len() >= MAX_FALLING_BLOCKS {
            return None;
        }

        // PHY-002: the local counter is the sole allocator for locally-spawned
        // blocks. `advance_counter_past` preserves the 0-reserved and wrap
        // invariants.
        let entity_id = self.next_entity_id;
        self.advance_counter_past(entity_id);

        // Convert grid position to center of block
        let center = Vector3::new(
            grid_position.x as f32 + 0.5,
            grid_position.y as f32 + 0.5,
            grid_position.z as f32 + 0.5,
        );

        // Defensive guard: the monotonic counter should make a live collision
        // impossible in normal operation (only reachable via a u32 wrap past
        // MAX). Replace the stale entry instead of producing a duplicate.
        let existing = self.blocks.iter().position(|fb| fb.entity_id == entity_id);
        debug_assert!(
            existing.is_none(),
            "spawn allocated entity_id {} that was already live",
            entity_id
        );
        if let Some(idx) = existing {
            log::warn!(
                "[FallingBlockSystem] entity_id {} collision on local spawn; replacing stale entry",
                entity_id
            );
            self.blocks[idx] = FallingBlock::new(entity_id, center, block_type);
        } else {
            self.blocks
                .push(FallingBlock::new(entity_id, center, block_type));
        }
        Some(entity_id)
    }

    /// Spawns a falling block with a server-supplied entity ID (network sync).
    ///
    /// PHY-002: the server's ID is authoritative, so we adopt it — but we must
    /// also ensure future local `spawn` calls never reuse it. We advance the
    /// local counter past `entity_id`, keeping the locally-allocated range
    /// disjoint from every network-adopted ID.
    ///
    /// If a block with this ID is already live (legitimate under re-broadcasts
    /// or retries), the stale entry is replaced rather than appending a
    /// duplicate — preserving the "one live block per ID" invariant that
    /// `remove_by_id` relies on.
    pub fn spawn_with_id(
        &mut self,
        entity_id: u32,
        grid_position: Vector3<i32>,
        block_type: BlockType,
    ) -> bool {
        if self.blocks.len() >= MAX_FALLING_BLOCKS {
            return false;
        }

        // Adopt the server ID and move the local counter past it so future
        // local spawns cannot collide with this network-spawned block.
        self.advance_counter_past(entity_id);

        // Convert grid position to center of block
        let center = Vector3::new(
            grid_position.x as f32 + 0.5,
            grid_position.y as f32 + 0.5,
            grid_position.z as f32 + 0.5,
        );

        if let Some(idx) = self.blocks.iter().position(|fb| fb.entity_id == entity_id) {
            log::warn!(
                "[FallingBlockSystem] spawn_with_id for already-live entity_id {}; replacing stale entry",
                entity_id
            );
            self.blocks[idx] = FallingBlock::new(entity_id, center, block_type);
        } else {
            self.blocks
                .push(FallingBlock::new(entity_id, center, block_type));
        }
        true
    }

    /// PHY-002: advances `next_entity_id` past `id` so no future local `spawn`
    /// ever reuses `id`. Preserves the "0 is reserved" invariant and the u32
    /// wrap-around behavior (after `u32::MAX` the counter wraps to 1, not 0).
    fn advance_counter_past(&mut self, id: u32) {
        let after = id.wrapping_add(1);
        let after = if after == 0 { 1 } else { after };
        self.next_entity_id = self.next_entity_id.max(after);
    }

    /// Removes a falling block by entity ID (for network sync).
    ///
    /// Used by clients when receiving landing messages from server.
    /// Returns true if a block was removed.
    pub fn remove_by_id(&mut self, entity_id: u32) -> bool {
        let len_before = self.blocks.len();
        self.blocks.retain(|fb| fb.entity_id != entity_id);
        self.blocks.len() < len_before
    }

    /// Updates all falling blocks and returns blocks that have landed.
    ///
    /// `is_solid` should return true if the block at (x, y, z) is solid.
    /// Returns a vector of blocks that have landed and need to be placed in the world.
    pub fn update<F>(&mut self, delta_time: f32, is_solid: F) -> Vec<LandedBlock>
    where
        F: Fn(i32, i32, i32) -> bool + Copy,
    {
        let mut landed = Vec::new();

        self.blocks.retain_mut(|fb| {
            match fb.update(delta_time, is_solid) {
                Some(land_pos) => {
                    // Block landed - check if position is valid (not below world)
                    if land_pos.y >= 0 {
                        landed.push(LandedBlock {
                            entity_id: fb.entity_id,
                            position: land_pos,
                            block_type: fb.block_type,
                        });
                    }
                    false // Remove from falling blocks
                }
                None => true, // Still falling, keep it
            }
        });

        landed
    }

    /// Returns the number of active falling blocks.
    pub fn count(&self) -> usize {
        self.blocks.len()
    }

    /// Gets GPU-ready falling block data.
    pub fn gpu_data(&self) -> Vec<GpuFallingBlock> {
        self.blocks.iter().map(GpuFallingBlock::from).collect()
    }

    /// Clears all falling blocks.
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.blocks.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_falling_block_new() {
        let fb = FallingBlock::new(1, Vector3::new(5.5, 10.5, 3.5), BlockType::Sand);
        assert_eq!(fb.entity_id, 1);
        assert_eq!(fb.position, Vector3::new(5.5, 10.5, 3.5));
        assert_eq!(fb.velocity, Vector3::zeros());
        assert_eq!(fb.block_type, BlockType::Sand);
        assert_eq!(fb.age, 0.0);
    }

    #[test]
    fn test_falling_block_falls() {
        let mut fb = FallingBlock::new(1, Vector3::new(5.5, 10.5, 3.5), BlockType::Sand);

        // No solid blocks - should continue falling
        let result = fb.update(0.1, |_, _, _| false);
        assert!(result.is_none());
        assert!(fb.position.y < 10.5); // Should have fallen
        assert!(fb.velocity.y < 0.0); // Should have negative velocity
    }

    #[test]
    fn test_falling_block_lands() {
        let mut fb = FallingBlock::new(1, Vector3::new(5.5, 1.5, 3.5), BlockType::Sand);
        fb.velocity.y = -5.0; // Already falling

        // Solid block at y=0
        let result = fb.update(0.1, |_, y, _| y == 0);

        // Should land on top of the solid block (y=1)
        assert!(result.is_some());
        let land_pos = result.unwrap();
        assert_eq!(land_pos.y, 1);
    }

    #[test]
    fn test_system_spawn_and_update() {
        let mut system = FallingBlockSystem::new();

        // Spawn a falling block
        let entity_id = system.spawn(Vector3::new(5, 10, 3), BlockType::Sand);
        assert!(entity_id.is_some());
        assert_eq!(system.count(), 1);

        // Update with no solid blocks
        let landed = system.update(0.016, |_, _, _| false);
        assert!(landed.is_empty());
        assert_eq!(system.count(), 1); // Still falling

        // Update with solid block below - will eventually land
        // Simulate many updates until it lands (use small time steps like 60 FPS)
        // With g=20 and fall distance of ~10 blocks, time to fall = sqrt(2*10/20) ≈ 1 second
        // 60 FPS * 2 seconds = 120 frames should be plenty
        for _ in 0..200 {
            let landed = system.update(0.016, |_, y, _| y == 0);
            if !landed.is_empty() {
                assert_eq!(landed[0].block_type, BlockType::Sand);
                assert_eq!(landed[0].position.y, 1);
                assert_eq!(landed[0].entity_id, entity_id.unwrap());
                assert_eq!(system.count(), 0); // Removed after landing
                return;
            }
        }
        panic!("Block should have landed by now");
    }

    #[test]
    fn test_gpu_data() {
        let mut system = FallingBlockSystem::new();
        system.spawn(Vector3::new(5, 10, 3), BlockType::Sand);

        let gpu_data = system.gpu_data();
        assert_eq!(gpu_data.len(), 1);
        assert_eq!(gpu_data[0].pos_type[3], BlockType::Sand as u8 as f32);
    }

    #[test]
    fn test_max_capacity() {
        let mut system = FallingBlockSystem::new();

        // Fill to capacity
        for i in 0..MAX_FALLING_BLOCKS {
            assert!(
                system
                    .spawn(Vector3::new(i as i32, 10, 0), BlockType::Sand)
                    .is_some()
            );
        }

        // Should reject additional spawns
        assert!(
            system
                .spawn(Vector3::new(0, 10, 0), BlockType::Gravel)
                .is_none()
        );
        assert_eq!(system.count(), MAX_FALLING_BLOCKS);
    }

    #[test]
    fn test_entity_id_generation() {
        let mut system = FallingBlockSystem::new();

        let id1 = system.spawn(Vector3::new(0, 0, 0), BlockType::Sand);
        let id2 = system.spawn(Vector3::new(1, 0, 0), BlockType::Sand);
        let id3 = system.spawn(Vector3::new(2, 0, 0), BlockType::Sand);

        assert!(id1.is_some());
        assert!(id2.is_some());
        assert!(id3.is_some());
        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_spawn_with_id() {
        let mut system = FallingBlockSystem::new();

        // Spawn with specific ID (for network sync)
        assert!(system.spawn_with_id(999, Vector3::new(5, 10, 3), BlockType::Sand));
        assert_eq!(system.count(), 1);
    }

    #[test]
    fn test_remove_by_id() {
        let mut system = FallingBlockSystem::new();

        let id = system
            .spawn(Vector3::new(5, 10, 3), BlockType::Sand)
            .unwrap();
        assert_eq!(system.count(), 1);

        // Remove by ID
        assert!(system.remove_by_id(id));
        assert_eq!(system.count(), 0);

        // Removing non-existent ID should return false
        assert!(!system.remove_by_id(999));
    }

    #[test]
    fn test_physics_substeps_clamps_large_dt() {
        // PHY-004: a 500 ms frame stall is clamped to MAX_PHYSICS_DT (50 ms)
        // and the substeps each stay ≤ MAX_PHYSICS_DT while summing exactly
        // to the clamped total (no simulated time lost to rounding).
        let (substeps, clamped) = physics_substeps(0.500);
        assert!(
            (clamped - MAX_PHYSICS_DT).abs() < 1e-6,
            "raw dt clamped to MAX_PHYSICS_DT, got {clamped}"
        );
        assert!(substeps.iter().all(|&s| s <= MAX_PHYSICS_DT + 1e-6));
        let sum: f32 = substeps.iter().sum();
        assert!(
            (sum - clamped).abs() < 1e-6,
            "substeps sum to clamped total"
        );
        assert!(!substeps.is_empty());
    }

    #[test]
    fn test_physics_substeps_preserves_small_dt() {
        // A normal 60 fps frame dt passes through as a single substep.
        let (substeps, clamped) = physics_substeps(0.016);
        assert!((clamped - 0.016).abs() < 1e-6);
        assert_eq!(substeps.len(), 1);
        assert!((substeps[0] - 0.016).abs() < 1e-6);
    }

    #[test]
    fn test_terminal_velocity_caps_per_step_fall() {
        // PHY-004 backstop: even with a pathological downward velocity and a
        // max-sized substep, a single update() call cannot move the block
        // more than MAX_FALL_CELLS_PER_STEP cells downward.
        let mut fb = FallingBlock::new(1, Vector3::new(0.5, 100.5, 0.5), BlockType::Sand);
        fb.velocity.y = -1000.0; // far beyond terminal velocity
        let before = fb.position.y;
        fb.update(MAX_PHYSICS_DT, |_, _, _| false); // open space, no landing
        let fallen = before - fb.position.y;
        assert!(
            fallen <= MAX_FALL_CELLS_PER_STEP as f32 + 1e-5,
            "fell {fallen} cells in one step, cap is {MAX_FALL_CELLS_PER_STEP}"
        );
    }

    #[test]
    fn test_no_tunnel_through_floor_with_large_dt() {
        // PHY-004: a fast-falling block given a pathological dt must still
        // land ON a solid floor instead of skipping past it.
        //
        // Without the terminal-velocity cap, velocity=-1000 with dt=0.05
        // integrates to a ~50-cell step; the single-cell collision check at
        // the new bottom edge reads a cell far below y=0 and misses the floor
        // entirely (tunnel). With MAX_FALL_VELOCITY the step is clamped to
        // MAX_FALL_CELLS_PER_STEP so the probe always samples the cell
        // directly below the block and the floor is detected.
        let mut fb = FallingBlock::new(1, Vector3::new(0.5, 1.5, 0.5), BlockType::Sand);
        fb.velocity.y = -1000.0;
        let result = fb.update(0.05, |_, y, _| y == 0); // solid floor at y=0
        assert!(
            result.is_some(),
            "must detect the floor instead of tunneling through it"
        );
        let land_pos = result.unwrap();
        assert_eq!(
            land_pos.y, 1,
            "lands ON top of the floor (y=0), not below it"
        );
    }

    #[test]
    fn test_local_spawn_ids_are_strictly_increasing_and_unique() {
        // PHY-002: local spawn() is the authoritative allocator and must never
        // reuse or backtrack on an ID.
        let mut system = FallingBlockSystem::new();
        let mut prev = 0u32;
        let mut seen = std::collections::HashSet::new();
        for _ in 0..64 {
            let id = system
                .spawn(Vector3::new(0, 0, 0), BlockType::Sand)
                .expect("spawn succeeds under MAX_FALLING_BLOCKS");
            assert!(id > prev, "id {id} not strictly greater than prev {prev}");
            assert!(seen.insert(id), "id {id} was allocated twice");
            prev = id;
        }
    }

    #[test]
    fn test_spawn_with_id_advances_counter_past_server_id() {
        // PHY-002: adopting a server-supplied ID must advance the local counter
        // past it so the next local spawn can never collide.
        let mut system = FallingBlockSystem::new();
        let server_id = 999u32;
        assert!(system.spawn_with_id(server_id, Vector3::new(5, 10, 3), BlockType::Sand));

        let next_local = system
            .spawn(Vector3::new(6, 10, 3), BlockType::Sand)
            .unwrap();
        assert!(
            next_local > server_id,
            "local id {next_local} must be > server_id {server_id} to avoid collision"
        );

        // Subsequent local spawns keep strictly increasing.
        let next_local2 = system
            .spawn(Vector3::new(7, 10, 3), BlockType::Sand)
            .unwrap();
        assert!(next_local2 > next_local);
    }

    #[test]
    fn test_spawn_with_id_replaces_stale_live_block() {
        // PHY-002: a duplicate spawn_with_id (re-broadcast / retry) must not
        // leave two live blocks sharing an ID. The stale entry is replaced.
        let mut system = FallingBlockSystem::new();
        let id = 42u32;
        assert!(system.spawn_with_id(id, Vector3::new(1, 5, 1), BlockType::Sand));
        assert_eq!(system.count(), 1);

        // Same ID again — must replace, not append.
        assert!(system.spawn_with_id(id, Vector3::new(2, 6, 2), BlockType::Gravel));
        assert_eq!(system.count(), 1);

        // Exactly one block live for this ID.
        assert_eq!(system.gpu_data().len(), 1);
    }

    #[test]
    fn test_remove_by_id_targets_correct_block_after_network_spawn() {
        // PHY-002: after a server-ID spawn followed by local spawns, a
        // remove_by_id for the server ID must remove exactly the right block
        // and leave the others intact — this is the invariant that was broken
        // by the old collision-prone allocator.
        let mut system = FallingBlockSystem::new();
        let server_id = 500u32;
        assert!(system.spawn_with_id(server_id, Vector3::new(0, 0, 0), BlockType::Sand));
        let local1 = system
            .spawn(Vector3::new(1, 0, 0), BlockType::Sand)
            .unwrap();
        let local2 = system
            .spawn(Vector3::new(2, 0, 0), BlockType::Sand)
            .unwrap();
        assert_ne!(local1, server_id);
        assert_ne!(local2, server_id);
        assert_ne!(local1, local2);
        assert_eq!(system.count(), 3);

        // Remove the server-spawned block by ID; locals are unaffected.
        assert!(system.remove_by_id(server_id));
        assert_eq!(system.count(), 2);
        assert!(system.remove_by_id(local1));
        assert!(system.remove_by_id(local2));
        assert_eq!(system.count(), 0);
        // Server ID already gone — second removal must report no-op.
        assert!(!system.remove_by_id(server_id));
    }
}

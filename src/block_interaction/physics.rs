//! Physics helpers: landed-block processing and terrain height queries.

use crate::block_interaction::BlockInteractionContext;
use crate::block_update::BlockUpdateType;
use crate::chunk::BlockType;
use crate::constants::TEXTURE_SIZE_Y;
use crate::falling_block::LandedBlock;
use crate::world::World;
use nalgebra::Vector3;

impl<'a> BlockInteractionContext<'a> {
    pub fn process_landed_blocks(&mut self, mut landed: Vec<LandedBlock>) {
        landed.sort_by_key(|lb| lb.position.y);

        // PHY-M06: the player's feet position is constant for the whole batch;
        // hoist it out of the loop so both the AABB guard (inside
        // `place_landed_block`) and the cascade priority see the same value.
        let player_feet = self
            .sim
            .player
            .feet_pos(self.sim.world_extent, self.sim.texture_origin);
        let player_pos = player_feet.cast::<f32>();

        for lb in landed {
            let Some(final_pos) = place_landed_block(&mut self.sim.world, &lb, player_feet) else {
                continue;
            };

            // Queue gravity check for block above (in case there's more falling blocks)
            self.sim.block_updates.enqueue(
                final_pos + Vector3::new(0, 1, 0),
                BlockUpdateType::Gravity,
                player_pos,
            );
        }
    }
}

/// Places a single landed block at the first restable cell at/above
/// `lb.position` in the same column, returning the final placement position.
///
/// Idempotent: if a prior call already placed this landing (the block now sits
/// at the landing origin), this is a no-op and returns `None`. This is the
/// defense-in-depth against the host double-processing its own loopback landing
/// broadcast — the caller-side `is_client()` guard in `update.rs` is the primary
/// fix, but this keeps `process_landed_blocks` safe even if that guard is ever
/// bypassed.
///
/// PHY-M06(a) — water alignment: the landing physics (`FallingBlock::update`
/// with the `blocks_movement` solidity predicate used in `update.rs`) rests a
/// falling block on the first cell ABOVE a movement-blocking block. That rest
/// cell may be Air OR Water (both are passable, so the entity descends through
/// them and comes to rest on the floor below). The placement scan therefore
/// accepts any non-`blocks_movement` cell, matching the physics — previously it
/// only accepted Air, so an underwater landing was wrongly pushed up to the
/// water surface.
///
/// PHY-M06(b) — player AABB: a landed block is never placed inside the player's
/// own hitbox. A falling block can come to rest exactly where the player is
/// standing; placing it there would suffocate/trap the player. If the final
/// cell overlaps the player AABB the placement is skipped (returns `None`).
///
/// Extracted from `process_landed_blocks` so the idempotency rule, the
/// upward-scan placement, and the AABB guard are unit-testable without the
/// full sim/GPU stack.
///
/// Returns `None` when the landing origin is out of Y bounds, the landing was
/// already applied, the column above is entirely full, or the final cell
/// overlaps the player AABB.
fn place_landed_block(
    world: &mut World,
    lb: &LandedBlock,
    player_feet: Vector3<f64>,
) -> Option<Vector3<i32>> {
    if !(lb.position.y >= 0 && lb.position.y < TEXTURE_SIZE_Y as i32) {
        return None;
    }

    // Idempotency: a prior call already placed this landing at the origin cell.
    // `LandedBlock.position` is the first restable cell above the impact floor
    // (see `FallingBlock::update`), so the block is placed exactly at
    // `lb.position` on the first call — if the same block type is already
    // there, skip.
    if world
        .get_block(lb.position)
        .is_some_and(|b| b == lb.block_type)
    {
        return None;
    }

    let mut place_y = lb.position.y;
    while place_y < TEXTURE_SIZE_Y as i32 {
        let check_pos = Vector3::new(lb.position.x, place_y, lb.position.z);
        // PHY-M06(a): accept any cell the physics would let the block rest in,
        // i.e. anything that does NOT block movement (Air, Water, …). The old
        // `== Air` test disagreed with physics at water boundaries.
        if world
            .get_block(check_pos)
            .is_some_and(|existing| !existing.blocks_movement())
        {
            break;
        }
        place_y += 1;
    }

    if place_y >= TEXTURE_SIZE_Y as i32 {
        return None;
    }

    let final_pos = Vector3::new(lb.position.x, place_y, lb.position.z);

    // PHY-M06(b): never place a landed block on top of the player. Mirror the
    // overlap test from `player_aabb_allows_placement` (placement.rs) and skip
    // if the final cell intersects the player hitbox.
    use crate::player::{PLAYER_HALF_WIDTH, PLAYER_HEIGHT};
    let player_min = Vector3::new(
        player_feet.x - PLAYER_HALF_WIDTH,
        player_feet.y,
        player_feet.z - PLAYER_HALF_WIDTH,
    );
    let player_max = Vector3::new(
        player_feet.x + PLAYER_HALF_WIDTH,
        player_feet.y + PLAYER_HEIGHT,
        player_feet.z + PLAYER_HALF_WIDTH,
    );
    let block_min = final_pos.cast::<f64>();
    let block_max = block_min + Vector3::new(1.0, 1.0, 1.0);
    let overlaps_player = player_min.x < block_max.x
        && player_max.x > block_min.x
        && player_min.y < block_max.y
        && player_max.y > block_min.y
        && player_min.z < block_max.z
        && player_max.z > block_min.z;
    if overlaps_player {
        return None;
    }

    world.set_block(final_pos, lb.block_type);
    world.invalidate_minimap_cache(final_pos.x, final_pos.z);
    Some(final_pos)
}

impl<'a> BlockInteractionContext<'a> {
    /// Find the terrain height at a given XZ position.
    pub(super) fn find_terrain_height_at(&self, x: i32, z: i32, max_y: i32) -> Option<i32> {
        for y in (0..=max_y).rev() {
            if let Some(block) = self.sim.world.get_block(nalgebra::Vector3::new(x, y, z))
                && block != BlockType::Air
                && block != BlockType::Water
                && block != BlockType::Lava
            {
                return Some(y);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Repeating the same landing must place exactly one block: the first call
    /// places it at the landing origin, the second (duplicate) call is a no-op.
    /// This is the core PHY-001 acceptance — the host consuming its own loopback
    /// landing broadcast must not stack a duplicate on top.
    #[test]
    fn place_landed_block_is_idempotent_on_duplicate_landing() {
        let mut world = World::new();
        let pos = Vector3::new(3, 20, 7);
        // Solid floor one cell below loads the chunk so get_block resolves to
        // Some(Air) for the rest cell above (mirrors real landing geometry).
        world.set_block(Vector3::new(pos.x, pos.y - 1, pos.z), BlockType::Stone);
        let lb = LandedBlock {
            entity_id: 1,
            position: pos,
            block_type: BlockType::Sand,
        };
        // Player far away so the PHY-M06(b) AABB guard never trips here.
        let far_player = Vector3::new(1000.0, 1000.0, 1000.0);

        // First call places at the landing origin (it is air).
        let first = place_landed_block(&mut world, &lb, far_player);
        assert_eq!(first, Some(pos));
        assert_eq!(world.get_block(pos), Some(BlockType::Sand));

        // Duplicate call must short-circuit and NOT stack a second block above.
        let second = place_landed_block(&mut world, &lb, far_player);
        assert_eq!(second, None);
        assert_eq!(world.get_block(pos), Some(BlockType::Sand));
        assert_eq!(
            world.get_block(Vector3::new(pos.x, pos.y + 1, pos.z)),
            Some(BlockType::Air),
            "duplicate landing must not place a stacked block"
        );

        // Sanity: the column has exactly one Sand cell.
        let sand_count = (0..TEXTURE_SIZE_Y as i32)
            .filter(|&y| world.get_block(Vector3::new(pos.x, y, pos.z)) == Some(BlockType::Sand))
            .count();
        assert_eq!(sand_count, 1);
    }

    /// A different landing (distinct position) must still place normally after a
    /// duplicate skip — the idempotency check must not over-aggressively suppress
    /// unrelated landings.
    #[test]
    fn place_landed_block_still_places_distinct_landings() {
        let mut world = World::new();
        let sand_pos = Vector3::new(0, 10, 0);
        let dirt_pos = Vector3::new(5, 12, 9);
        // Floors below each landing load their chunks.
        world.set_block(
            Vector3::new(sand_pos.x, sand_pos.y - 1, sand_pos.z),
            BlockType::Stone,
        );
        world.set_block(
            Vector3::new(dirt_pos.x, dirt_pos.y - 1, dirt_pos.z),
            BlockType::Stone,
        );

        let sand_lb = LandedBlock {
            entity_id: 1,
            position: sand_pos,
            block_type: BlockType::Sand,
        };
        let dirt_lb = LandedBlock {
            entity_id: 2,
            position: dirt_pos,
            block_type: BlockType::Dirt,
        };
        let far_player = Vector3::new(1000.0, 1000.0, 1000.0);

        assert_eq!(
            place_landed_block(&mut world, &sand_lb, far_player),
            Some(sand_pos)
        );
        // Duplicate of sand is skipped.
        assert_eq!(place_landed_block(&mut world, &sand_lb, far_player), None);
        // A distinct landing places normally.
        assert_eq!(
            place_landed_block(&mut world, &dirt_lb, far_player),
            Some(dirt_pos)
        );

        assert_eq!(world.get_block(sand_pos), Some(BlockType::Sand));
        assert_eq!(world.get_block(dirt_pos), Some(BlockType::Dirt));
    }

    /// Two sand blocks coming to rest one cell apart (the normal stacking case —
    /// the physics system gives each its own rest cell) must both place. This
    /// guards against the idempotency check accidentally suppressing legitimate
    /// adjacent stacking within a batch.
    #[test]
    fn place_landed_block_stacks_adjacent_landings() {
        let mut world = World::new();
        let lower = Vector3::new(2, 8, 4);
        let upper = Vector3::new(2, 9, 4);
        // Floor below the lower rest cell loads the chunk for the whole column.
        world.set_block(
            Vector3::new(lower.x, lower.y - 1, lower.z),
            BlockType::Stone,
        );
        let lb_lower = LandedBlock {
            entity_id: 10,
            position: lower,
            block_type: BlockType::Sand,
        };
        let lb_upper = LandedBlock {
            entity_id: 11,
            position: upper,
            block_type: BlockType::Sand,
        };
        let far_player = Vector3::new(1000.0, 1000.0, 1000.0);

        assert_eq!(
            place_landed_block(&mut world, &lb_lower, far_player),
            Some(lower)
        );
        assert_eq!(
            place_landed_block(&mut world, &lb_upper, far_player),
            Some(upper)
        );

        assert_eq!(world.get_block(lower), Some(BlockType::Sand));
        assert_eq!(world.get_block(upper), Some(BlockType::Sand));
    }

    /// PHY-M06(a): a landing whose rest cell is Water must be placed at that
    /// underwater cell, matching the physics (the falling entity descends
    /// through non-`blocks_movement` water and rests on the floor beneath).
    /// The old `== Air` scan wrongly pushed it up to the surface Air cell.
    #[test]
    fn place_landed_block_rests_underwater_matching_physics() {
        let mut world = World::new();
        // Column: Stone floor at y=18, Water at y=19 (rest cell), Air above.
        let floor = Vector3::new(4, 18, 4);
        let water_rest = Vector3::new(4, 19, 4);
        world.set_block(floor, BlockType::Stone);
        world.set_block(water_rest, BlockType::Water);
        let far_player = Vector3::new(1000.0, 1000.0, 1000.0);

        let lb = LandedBlock {
            entity_id: 1,
            position: water_rest,
            block_type: BlockType::Sand,
        };

        // Must place at the Water rest cell (displacing water), NOT scan upward.
        assert_eq!(
            place_landed_block(&mut world, &lb, far_player),
            Some(water_rest),
            "PHY-M06(a): underwater landing must place at the rest cell, not the surface"
        );
        assert_eq!(world.get_block(water_rest), Some(BlockType::Sand));
        assert_eq!(
            world.get_block(Vector3::new(water_rest.x, water_rest.y + 1, water_rest.z)),
            Some(BlockType::Air),
            "must not place a spurious surface block"
        );
    }

    /// PHY-M06(b): a landed block whose final cell overlaps the player AABB
    /// must be skipped — never place a block inside the player.
    #[test]
    fn place_landed_block_skips_when_player_standing_on_rest_cell() {
        use crate::player::{PLAYER_HALF_WIDTH, PLAYER_HEIGHT};
        let mut world = World::new();
        let floor = Vector3::new(7, 20, 7);
        let rest = Vector3::new(7, 21, 7);
        world.set_block(floor, BlockType::Stone);

        // Stand the player directly on the rest cell: feet on top of `floor`
        // means the player body occupies the rest cell.
        let player_feet = Vector3::new(rest.x as f64 + 0.5, rest.y as f64, rest.z as f64 + 0.5);
        // Sanity: the player AABB must actually overlap the rest cell here.
        let pmin = Vector3::new(
            player_feet.x - PLAYER_HALF_WIDTH,
            player_feet.y,
            player_feet.z - PLAYER_HALF_WIDTH,
        );
        let pmax = Vector3::new(
            player_feet.x + PLAYER_HALF_WIDTH,
            player_feet.y + PLAYER_HEIGHT,
            player_feet.z + PLAYER_HALF_WIDTH,
        );
        let bmin = rest.cast::<f64>();
        let bmax = bmin + Vector3::new(1.0, 1.0, 1.0);
        assert!(
            pmin.x < bmax.x
                && pmax.x > bmin.x
                && pmin.y < bmax.y
                && pmax.y > bmin.y
                && pmin.z < bmax.z
                && pmax.z > bmin.z,
            "test setup: player AABB must overlap rest cell"
        );

        let lb = LandedBlock {
            entity_id: 1,
            position: rest,
            block_type: BlockType::Sand,
        };

        assert_eq!(
            place_landed_block(&mut world, &lb, player_feet),
            None,
            "PHY-M06(b): must not place a block inside the player AABB"
        );
        assert_eq!(
            world.get_block(rest),
            Some(BlockType::Air),
            "no block placed inside the player"
        );
    }
}

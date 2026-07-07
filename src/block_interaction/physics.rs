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

        for lb in landed {
            let Some(final_pos) = place_landed_block(&mut self.sim.world, &lb) else {
                continue;
            };

            let player_pos = self
                .sim
                .player
                .feet_pos(self.sim.world_extent, self.sim.texture_origin)
                .cast::<f32>();

            // Queue gravity check for block above (in case there's more falling blocks)
            self.sim.block_updates.enqueue(
                final_pos + Vector3::new(0, 1, 0),
                BlockUpdateType::Gravity,
                player_pos,
            );
        }
    }
}

/// Places a single landed block at the first air cell at/above `lb.position` in
/// the same column, returning the final placement position.
///
/// Idempotent: if a prior call already placed this landing (the block now sits
/// at the landing origin), this is a no-op and returns `None`. This is the
/// defense-in-depth against the host double-processing its own loopback landing
/// broadcast — the caller-side `is_client()` guard in `update.rs` is the primary
/// fix, but this keeps `process_landed_blocks` safe even if that guard is ever
/// bypassed.
///
/// Extracted from `process_landed_blocks` so the idempotency rule and the
/// upward-scan placement are unit-testable without the full sim/GPU stack.
///
/// Returns `None` when the landing origin is out of Y bounds, the landing was
/// already applied, or the column above is entirely full.
fn place_landed_block(world: &mut World, lb: &LandedBlock) -> Option<Vector3<i32>> {
    if !(lb.position.y >= 0 && lb.position.y < TEXTURE_SIZE_Y as i32) {
        return None;
    }

    // Idempotency: a prior call already placed this landing at the origin cell.
    // `LandedBlock.position` is the first air cell above the impact floor (see
    // `FallingBlock::update`), so the block is placed exactly at `lb.position`
    // on the first call — if the same block type is already there, skip.
    if world
        .get_block(lb.position)
        .is_some_and(|b| b == lb.block_type)
    {
        return None;
    }

    let mut place_y = lb.position.y;
    while place_y < TEXTURE_SIZE_Y as i32 {
        let check_pos = Vector3::new(lb.position.x, place_y, lb.position.z);
        if world
            .get_block(check_pos)
            .is_some_and(|existing| existing == BlockType::Air)
        {
            break;
        }
        place_y += 1;
    }

    if place_y >= TEXTURE_SIZE_Y as i32 {
        return None;
    }

    let final_pos = Vector3::new(lb.position.x, place_y, lb.position.z);
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

        // First call places at the landing origin (it is air).
        let first = place_landed_block(&mut world, &lb);
        assert_eq!(first, Some(pos));
        assert_eq!(world.get_block(pos), Some(BlockType::Sand));

        // Duplicate call must short-circuit and NOT stack a second block above.
        let second = place_landed_block(&mut world, &lb);
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

        assert_eq!(place_landed_block(&mut world, &sand_lb), Some(sand_pos));
        // Duplicate of sand is skipped.
        assert_eq!(place_landed_block(&mut world, &sand_lb), None);
        // A distinct landing places normally.
        assert_eq!(place_landed_block(&mut world, &dirt_lb), Some(dirt_pos));

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

        assert_eq!(place_landed_block(&mut world, &lb_lower), Some(lower));
        assert_eq!(place_landed_block(&mut world, &lb_upper), Some(upper));

        assert_eq!(world.get_block(lower), Some(BlockType::Sand));
        assert_eq!(world.get_block(upper), Some(BlockType::Sand));
    }
}

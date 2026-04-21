//! Physics helpers: landed-block processing and terrain height queries.

use crate::block_interaction::BlockInteractionContext;
use crate::block_update::BlockUpdateType;
use crate::chunk::BlockType;
use crate::constants::TEXTURE_SIZE_Y;
use nalgebra::Vector3;

impl<'a> BlockInteractionContext<'a> {
    pub fn process_landed_blocks(&mut self, mut landed: Vec<crate::falling_block::LandedBlock>) {
        landed.sort_by_key(|lb| lb.position.y);

        for lb in landed {
            if lb.position.y >= 0 && lb.position.y < TEXTURE_SIZE_Y as i32 {
                let mut place_y = lb.position.y;
                while place_y < TEXTURE_SIZE_Y as i32 {
                    let check_pos = Vector3::new(lb.position.x, place_y, lb.position.z);
                    if let Some(existing) = self.sim.world.get_block(check_pos)
                        && existing == BlockType::Air
                    {
                        break;
                    }
                    place_y += 1;
                }

                if place_y < TEXTURE_SIZE_Y as i32 {
                    let final_pos = Vector3::new(lb.position.x, place_y, lb.position.z);
                    self.sim.world.set_block(final_pos, lb.block_type);
                    self.sim
                        .world
                        .invalidate_minimap_cache(final_pos.x, final_pos.z);

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
    }

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

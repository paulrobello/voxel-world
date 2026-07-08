//! Fluid ↔ block reactions shared by the water and lava simulations.
//!
//! Both sims detect water+lava contact independently (water scans changed cells
//! for adjacent lava; lava scans active cells for adjacent water), but the
//! actual reaction — consume both fluids, leave cobblestone — is identical.
//! [`form_cobblestone`] is that single reaction, called from both
//! `WaterGrid::process_simulation` and `LavaGrid::process_simulation` so the
//! conversion can't drift between them (FLU-001).

use crate::chunk::BlockType;
use crate::lava::LavaGrid;
use crate::water::WaterGrid;
use crate::world::World;
use nalgebra::Vector3;

/// Convert a position into cobblestone, consuming any water and lava cell there
/// and waking both fluids' neighbors (a solid block now occupies `pos`).
///
/// Used when water and lava occupy the same position after a flow step. The
/// "lava adjacent to water" case in `WaterGrid::process_simulation` removes only
/// the lava cell at a neighboring position, so it does not go through here.
pub(crate) fn form_cobblestone(
    world: &mut World,
    water_grid: &mut WaterGrid,
    lava_grid: &mut LavaGrid,
    pos: Vector3<i32>,
) {
    // `on_block_placed` removes the fluid cell at `pos` and re-activates its
    // neighbors so they re-evaluate flow around the new solid block.
    water_grid.on_block_placed(pos);
    lava_grid.on_block_placed(pos);
    world.set_block(pos, BlockType::Cobblestone);
    world.invalidate_minimap_cache(pos.x, pos.z);
}

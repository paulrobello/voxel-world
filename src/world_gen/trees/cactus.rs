//! Cactus generation for desert biomes.

use crate::chunk::{BlockType, CHUNK_SIZE, Chunk};
use crate::world_gen::utils::OverflowBlock;

/// Generate a cactus.
#[allow(clippy::too_many_arguments)]
pub fn generate_cactus(
    chunk: &mut Chunk,
    x: i32,
    y: i32,
    z: i32,
    hash: i32,
    _chunk_world_x: i32,
    _chunk_world_y: i32,
    _chunk_world_z: i32,
    _overflow_blocks: &mut Vec<OverflowBlock>,
) {
    let height = 3 + (hash % 3);

    // Main column (trunk)
    for dy in 1..=height {
        let block_y = y + dy;
        if x >= 0
            && x < CHUNK_SIZE as i32
            && block_y >= 0
            && block_y < CHUNK_SIZE as i32
            && z >= 0
            && z < CHUNK_SIZE as i32
            && chunk
                .get_block(x as usize, block_y as usize, z as usize)
                .is_transparent()
        {
            chunk.set_block(x as usize, block_y as usize, z as usize, BlockType::Cactus);
        }
    }

    // Add branches for taller cacti (height >= 4)
    if height >= 4 {
        let branch_dir = hash % 4;
        let branch_height = y + 2 + (hash % 2);

        let (dx, dz) = match branch_dir {
            0 => (0, -1),
            1 => (0, 1),
            2 => (1, 0),
            _ => (-1, 0),
        };

        // Place branch (1-2 blocks long)
        let branch_len = 1 + ((hash / 7) % 2);
        for i in 1..=branch_len {
            let branch_x = x + dx * i;
            let branch_z = z + dz * i;
            if branch_x >= 0
                && branch_x < CHUNK_SIZE as i32
                && branch_height >= 0
                && branch_height < CHUNK_SIZE as i32
                && branch_z >= 0
                && branch_z < CHUNK_SIZE as i32
                && chunk
                    .get_block(branch_x as usize, branch_height as usize, branch_z as usize)
                    .is_transparent()
            {
                chunk.set_block(
                    branch_x as usize,
                    branch_height as usize,
                    branch_z as usize,
                    BlockType::Cactus,
                );
            }
        }

        // Add vertical growth on branch tip (0-1 blocks)
        if (hash / 13) % 2 == 0 {
            let tip_x = x + dx * branch_len;
            let tip_y = branch_height + 1;
            let tip_z = z + dz * branch_len;
            if tip_x >= 0
                && tip_x < CHUNK_SIZE as i32
                && tip_y >= 0
                && tip_y < CHUNK_SIZE as i32
                && tip_z >= 0
                && tip_z < CHUNK_SIZE as i32
                && chunk
                    .get_block(tip_x as usize, tip_y as usize, tip_z as usize)
                    .is_transparent()
            {
                chunk.set_block(
                    tip_x as usize,
                    tip_y as usize,
                    tip_z as usize,
                    BlockType::Cactus,
                );
            }
        }

        // Optionally add a second branch on the opposite side for very tall cacti
        if height >= 5 && (hash / 11) % 2 == 0 {
            let branch2_height = branch_height + 1;
            let (dx2, dz2) = match (branch_dir + 2) % 4 {
                0 => (0, -1),
                1 => (0, 1),
                2 => (1, 0),
                _ => (-1, 0),
            };

            let branch2_x = x + dx2;
            let branch2_z = z + dz2;
            if branch2_x >= 0
                && branch2_x < CHUNK_SIZE as i32
                && branch2_height >= 0
                && branch2_height < CHUNK_SIZE as i32
                && branch2_z >= 0
                && branch2_z < CHUNK_SIZE as i32
                && chunk
                    .get_block(
                        branch2_x as usize,
                        branch2_height as usize,
                        branch2_z as usize,
                    )
                    .is_transparent()
            {
                chunk.set_block(
                    branch2_x as usize,
                    branch2_height as usize,
                    branch2_z as usize,
                    BlockType::Cactus,
                );
            }
        }
    }
}

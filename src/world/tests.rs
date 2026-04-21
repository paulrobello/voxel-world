//! Tests for world module.

use super::*;
use nalgebra::vector;

#[test]
fn test_world_to_chunk() {
    assert_eq!(World::world_to_chunk(vector![0, 0, 0]), vector![0, 0, 0]);
    assert_eq!(World::world_to_chunk(vector![31, 31, 31]), vector![0, 0, 0]);
    assert_eq!(World::world_to_chunk(vector![32, 0, 0]), vector![1, 0, 0]);
    assert_eq!(World::world_to_chunk(vector![-1, 0, 0]), vector![-1, 0, 0]);
    assert_eq!(World::world_to_chunk(vector![-32, 0, 0]), vector![-1, 0, 0]);
    assert_eq!(World::world_to_chunk(vector![-33, 0, 0]), vector![-2, 0, 0]);
}

#[test]
fn test_world_to_local() {
    assert_eq!(World::world_to_local(vector![0, 0, 0]), (0, 0, 0));
    assert_eq!(World::world_to_local(vector![5, 10, 15]), (5, 10, 15));
    assert_eq!(World::world_to_local(vector![32, 0, 0]), (0, 0, 0));
    assert_eq!(World::world_to_local(vector![-1, 0, 0]), (31, 0, 0));
}

#[test]
fn test_set_get_block() {
    use crate::chunk::BlockType;
    let mut world = World::new();

    world.set_block(vector![10, 20, 30], BlockType::Stone);
    assert_eq!(world.get_block(vector![10, 20, 30]), Some(BlockType::Stone));
    assert_eq!(world.get_block(vector![0, 0, 0]), Some(BlockType::Air));

    // Test negative coordinates
    world.set_block(vector![-5, -10, -15], BlockType::Dirt);
    assert_eq!(
        world.get_block(vector![-5, -10, -15]),
        Some(BlockType::Dirt)
    );
}

#[test]
fn test_dirty_chunks() {
    use crate::chunk::BlockType;
    let mut world = World::new();

    world.set_block(vector![0, 0, 0], BlockType::Stone);
    world.set_block(vector![32, 0, 0], BlockType::Dirt);
    // Setting the same block again should not duplicate entries
    world.set_block(vector![0, 0, 0], BlockType::Stone);

    let dirty = world.drain_dirty_chunks();
    assert_eq!(dirty.len(), 2);
    assert!(world.dirty_chunks().is_empty());
}

#[test]
fn test_remove_dirty_positions() {
    use crate::chunk::BlockType;
    let mut world = World::new();
    let pos_a = vector![0, 0, 0];
    let pos_b = vector![32, 0, 0];
    let chunk_b = World::world_to_chunk(pos_b);

    world.set_block(pos_a, BlockType::Stone);
    world.set_block(pos_b, BlockType::Dirt);
    assert_eq!(world.dirty_chunks().len(), 2);

    // Remove one entry
    let chunk_a = World::world_to_chunk(pos_a);
    world.remove_dirty_positions(&[chunk_a]);
    let mut remaining: Vec<_> = world
        .dirty_chunks()
        .iter()
        .map(|v| (v.x, v.y, v.z))
        .collect();
    remaining.sort();
    assert_eq!(remaining, vec![(chunk_b.x, chunk_b.y, chunk_b.z)]);

    // Removing again is a no-op
    world.remove_dirty_positions(&[chunk_a]);
    let mut remaining: Vec<_> = world
        .dirty_chunks()
        .iter()
        .map(|v| (v.x, v.y, v.z))
        .collect();
    remaining.sort();
    assert_eq!(remaining, vec![(chunk_b.x, chunk_b.y, chunk_b.z)]);

    // Remove remaining
    world.remove_dirty_positions(&[chunk_b]);
    let remaining: Vec<_> = world
        .dirty_chunks()
        .iter()
        .map(|v| (v.x, v.y, v.z))
        .collect();
    assert!(
        remaining.is_empty(),
        "dirty_chunks should be empty, found: {:?}",
        remaining
    );
}

#[test]
fn test_stair_shapes_front_back_neighbors() {
    use crate::sub_voxel::{ModelRegistry, StairShape};
    let mut world = World::new();

    // Stair corner detection logic:
    // - Outer corner (single raised quadrant): Neighbor at our LOW/front side, facing perpendicular
    // - Inner corner (L-shaped top with pocket): Neighbor at our HIGH/back side, facing perpendicular
    //
    // Rotation 0: facing -Z, left_dir = -X, right_dir = +X
    // Rotation 1: facing +X, left_dir = -Z, right_dir = +Z
    // Rotation 2: facing +Z, left_dir = +X, right_dir = -X
    // Rotation 3: facing -X, left_dir = +Z, right_dir = -Z

    let straight_id = ModelRegistry::stairs_model_id(StairShape::Straight, false);

    // Case 1: INNER corner - neighbor at our HIGH/back side, neighbor faces our left
    // Our stair at (0,0,0): rotation 0 → facing (-Z), left_dir = (-X)
    // Neighbor at (0,0,1): rotation 3 → facing (-X) = our left_dir
    // back_neighbor == left_dir → InnerRight
    world.set_model_block(vector![0, 0, 0], straight_id, 0, false);
    world.set_model_block(vector![0, 0, 1], straight_id, 3, false);

    world.update_stair_shape_at(vector![0, 0, 0]);

    let data = world.get_model_data(vector![0, 0, 0]).unwrap();
    let expected_id = ModelRegistry::stairs_model_id(StairShape::InnerRight, false);
    assert_eq!(
        data.model_id, expected_id,
        "Back neighbor facing left → InnerRight"
    );

    // Case 2: OUTER corner - neighbor at our LOW/front side, neighbor faces our left
    // Our stair at (10,0,0): rotation 0 → facing (-Z), left_dir = (-X)
    // Neighbor at (10,0,-1): rotation 3 → facing (-X) = our left_dir
    // front_neighbor == left_dir → OuterLeft
    world.set_model_block(vector![10, 0, 0], straight_id, 0, false);
    world.set_model_block(vector![10, 0, -1], straight_id, 3, false);

    world.update_stair_shape_at(vector![10, 0, 0]);

    let data = world.get_model_data(vector![10, 0, 0]).unwrap();
    let expected_id = ModelRegistry::stairs_model_id(StairShape::OuterLeft, false);
    assert_eq!(
        data.model_id, expected_id,
        "Front neighbor facing left → OuterLeft"
    );

    // Case 3: OUTER corner - neighbor at our LOW/front side, neighbor faces our right
    // Our stair at (20,0,0): rotation 0 → facing (-Z), right_dir = (+X)
    // Neighbor at (20,0,-1): rotation 1 → facing (+X) = our right_dir
    // front_neighbor == right_dir → OuterRight
    world.set_model_block(vector![20, 0, 0], straight_id, 0, false);
    world.set_model_block(vector![20, 0, -1], straight_id, 1, false);

    world.update_stair_shape_at(vector![20, 0, 0]);

    let data = world.get_model_data(vector![20, 0, 0]).unwrap();
    let expected_id = ModelRegistry::stairs_model_id(StairShape::OuterRight, false);
    assert_eq!(
        data.model_id, expected_id,
        "Front neighbor facing right → OuterRight"
    );

    // Case 4: INNER corner - neighbor at our HIGH/back side, neighbor faces our right
    // Our stair at (30,0,0): rotation 0 → facing (-Z), right_dir = (+X)
    // Neighbor at (30,0,1): rotation 1 → facing (+X) = our right_dir
    // back_neighbor == right_dir → InnerLeft
    world.set_model_block(vector![30, 0, 0], straight_id, 0, false);
    world.set_model_block(vector![30, 0, 1], straight_id, 1, false);

    world.update_stair_shape_at(vector![30, 0, 0]);

    let data = world.get_model_data(vector![30, 0, 0]).unwrap();
    let expected_id = ModelRegistry::stairs_model_id(StairShape::InnerLeft, false);
    assert_eq!(
        data.model_id, expected_id,
        "Back neighbor facing right → InnerLeft"
    );
}

#[test]
fn test_stair_shapes_left_right_neighbors() {
    use crate::sub_voxel::{ModelRegistry, StairShape};
    let mut world = World::new();

    let straight_id = ModelRegistry::stairs_model_id(StairShape::Straight, false);

    // Case 1: Left neighbor - neighbor faces away (our right_dir) → InnerRight
    // Our stair at (0,0,0): rotation 0 → facing (-Z), left_dir = (-X), right_dir = (+X)
    // Left neighbor at (-1,0,0): rotation 1 → facing (+X) = our right_dir
    world.set_model_block(vector![0, 0, 0], straight_id, 0, false);
    world.set_model_block(vector![-1, 0, 0], straight_id, 1, false);

    world.update_stair_shape_at(vector![0, 0, 0]);

    let data = world.get_model_data(vector![0, 0, 0]).unwrap();
    let expected_id = ModelRegistry::stairs_model_id(StairShape::InnerRight, false);
    assert_eq!(
        data.model_id, expected_id,
        "Left neighbor facing away (right_dir) → InnerRight"
    );

    // Case 2: Left neighbor facing our left_dir (parallel, no corner)
    // For rotation 0: left_dir = -X, neighbor facing -X = rotation 3
    world.set_model_block(vector![20, 0, 0], straight_id, 0, false);
    world.set_model_block(vector![19, 0, 0], straight_id, 3, false);

    world.update_stair_shape_at(vector![20, 0, 0]);

    let data = world.get_model_data(vector![20, 0, 0]).unwrap();
    assert_eq!(
        data.model_id, straight_id,
        "Left neighbor facing same direction as our left_dir → stays Straight"
    );

    // Case 3: Right neighbor - neighbor faces away (our left_dir) → InnerLeft
    // Our stair at (30,0,0): rotation 0 → facing (-Z), left_dir = (-X), right_dir = (+X)
    // Right neighbor at (31,0,0): rotation 3 → facing (-X) = our left_dir
    world.set_model_block(vector![30, 0, 0], straight_id, 0, false);
    world.set_model_block(vector![31, 0, 0], straight_id, 3, false);

    world.update_stair_shape_at(vector![30, 0, 0]);

    let data = world.get_model_data(vector![30, 0, 0]).unwrap();
    let expected_id = ModelRegistry::stairs_model_id(StairShape::InnerLeft, false);
    assert_eq!(
        data.model_id, expected_id,
        "Right neighbor facing away (left_dir) → InnerLeft"
    );

    // Case 4: Right neighbor facing our right_dir (parallel, no corner)
    // For rotation 0: right_dir = +X, neighbor facing +X = rotation 1
    world.set_model_block(vector![40, 0, 0], straight_id, 0, false);
    world.set_model_block(vector![41, 0, 0], straight_id, 1, false);

    world.update_stair_shape_at(vector![40, 0, 0]);

    let data = world.get_model_data(vector![40, 0, 0]).unwrap();
    assert_eq!(
        data.model_id, straight_id,
        "Right neighbor facing same direction as our right_dir → stays Straight"
    );
}

#[test]
fn test_stair_shapes_parallel_neighbors_stay_straight() {
    use crate::sub_voxel::{ModelRegistry, StairShape};
    let mut world = World::new();

    let straight_id = ModelRegistry::stairs_model_id(StairShape::Straight, false);

    // Two stairs side by side facing the same direction should both stay straight
    // Our stair at (0,0,0): rotation 0 (facing -Z)
    // Neighbor at (1,0,0): rotation 0 (facing -Z) - parallel, not perpendicular
    world.set_model_block(vector![0, 0, 0], straight_id, 0, false);
    world.set_model_block(vector![1, 0, 0], straight_id, 0, false);

    world.update_stair_shape_at(vector![0, 0, 0]);

    let data = world.get_model_data(vector![0, 0, 0]).unwrap();
    assert_eq!(
        data.model_id, straight_id,
        "Parallel neighbors should stay straight"
    );

    // Also test the other neighbor
    world.update_stair_shape_at(vector![1, 0, 0]);
    let data = world.get_model_data(vector![1, 0, 0]).unwrap();
    assert_eq!(
        data.model_id, straight_id,
        "Parallel neighbors should stay straight"
    );

    // Test a row of 3 stairs all facing same direction
    world.set_model_block(vector![10, 0, 0], straight_id, 0, false);
    world.set_model_block(vector![11, 0, 0], straight_id, 0, false);
    world.set_model_block(vector![12, 0, 0], straight_id, 0, false);

    world.update_stair_shape_at(vector![11, 0, 0]); // Middle one
    let data = world.get_model_data(vector![11, 0, 0]).unwrap();
    assert_eq!(
        data.model_id, straight_id,
        "Middle of row should stay straight"
    );
}

#[test]
fn test_stair_shapes_different_rotations() {
    use crate::sub_voxel::{ModelRegistry, StairShape};
    let mut world = World::new();

    let straight_id = ModelRegistry::stairs_model_id(StairShape::Straight, false);

    // Test with rotation 2 (facing +Z)
    // Our stair at (0,0,0): rotation 2 → facing (+Z), left_dir = (+X), right_dir = (-X)
    // Neighbor at (0,0,-1): this is at our HIGH/back side (opposite of +Z)
    // Neighbor with rotation 1 → facing (+X) = our left_dir
    // back_neighbor == left_dir → InnerRight
    world.set_model_block(vector![0, 0, 0], straight_id, 2, false);
    world.set_model_block(vector![0, 0, -1], straight_id, 1, false);

    world.update_stair_shape_at(vector![0, 0, 0]);

    let data = world.get_model_data(vector![0, 0, 0]).unwrap();
    let expected_id = ModelRegistry::stairs_model_id(StairShape::InnerRight, false);
    assert_eq!(
        data.model_id, expected_id,
        "Rotation 2 inner corner should work"
    );

    // Test with rotation 1 (facing +X)
    // Our stair at (10,0,0): rotation 1 → facing (+X), left_dir = (-Z), right_dir = (+Z)
    // Neighbor at (9,0,0): this is at our HIGH/back side (opposite of +X)
    // Neighbor with rotation 0 → facing (-Z) = our left_dir
    // back_neighbor == left_dir → InnerRight
    world.set_model_block(vector![10, 0, 0], straight_id, 1, false);
    world.set_model_block(vector![9, 0, 0], straight_id, 0, false);

    world.update_stair_shape_at(vector![10, 0, 0]);

    let data = world.get_model_data(vector![10, 0, 0]).unwrap();
    let expected_id = ModelRegistry::stairs_model_id(StairShape::InnerRight, false);
    assert_eq!(
        data.model_id, expected_id,
        "Rotation 1 inner corner should work"
    );
}

#[test]
fn test_stair_inner_priority_over_outer() {
    use crate::sub_voxel::{ModelRegistry, StairShape};
    let mut world = World::new();

    let straight_id = ModelRegistry::stairs_model_id(StairShape::Straight, false);

    // When a stair has both front and back neighbors that could trigger corners,
    // outer (front) takes priority over inner (back)
    // Our stair at (0,0,0): rotation 0 → facing (-Z)
    // Front neighbor at (0,0,-1): rotation 3 → facing (-X) = left_dir → OuterLeft
    // Back neighbor at (0,0,1): rotation 1 → facing (+X) = right_dir → would be InnerLeft
    // Outer should win (front is checked first)
    world.set_model_block(vector![0, 0, 0], straight_id, 0, false);
    world.set_model_block(vector![0, 0, -1], straight_id, 3, false); // Front - triggers outer
    world.set_model_block(vector![0, 0, 1], straight_id, 1, false); // Back - would trigger inner

    world.update_stair_shape_at(vector![0, 0, 0]);

    let data = world.get_model_data(vector![0, 0, 0]).unwrap();
    let expected_id = ModelRegistry::stairs_model_id(StairShape::OuterLeft, false);
    assert_eq!(
        data.model_id, expected_id,
        "Outer corner takes priority over inner"
    );
}

#[test]
fn test_stair_no_corner_with_opposite_facing() {
    use crate::sub_voxel::{ModelRegistry, StairShape};
    let mut world = World::new();

    let straight_id = ModelRegistry::stairs_model_id(StairShape::Straight, false);

    // Stairs facing opposite directions (180° apart) should not form corners
    // Our stair at (0,0,0): rotation 0 (facing -Z)
    // Neighbor at (0,0,1): rotation 2 (facing +Z) - directly opposite, not perpendicular
    world.set_model_block(vector![0, 0, 0], straight_id, 0, false);
    world.set_model_block(vector![0, 0, 1], straight_id, 2, false);

    world.update_stair_shape_at(vector![0, 0, 0]);

    let data = world.get_model_data(vector![0, 0, 0]).unwrap();
    assert_eq!(
        data.model_id, straight_id,
        "Opposite facing neighbors stay straight"
    );

    // Same test for front neighbor with opposite facing
    world.set_model_block(vector![10, 0, 0], straight_id, 0, false);
    world.set_model_block(vector![10, 0, -1], straight_id, 2, false); // Facing toward us

    world.update_stair_shape_at(vector![10, 0, 0]);

    let data = world.get_model_data(vector![10, 0, 0]).unwrap();
    assert_eq!(
        data.model_id, straight_id,
        "Opposite facing front neighbor stays straight"
    );
}

#[test]
fn test_stair_shapes_inverted_stairs() {
    use crate::sub_voxel::{ModelRegistry, StairShape};
    let mut world = World::new();

    let straight_inverted = ModelRegistry::stairs_model_id(StairShape::Straight, true);

    // Inverted stairs should also form corners with other inverted stairs
    // For inverted stairs, both Inner↔Outer AND Left↔Right are flipped
    // Case 1: Back neighbor → OuterLeft (inverted) - would be InnerRight if not inverted
    world.set_model_block(vector![0, 0, 0], straight_inverted, 0, false);
    world.set_model_block(vector![0, 0, 1], straight_inverted, 3, false);

    world.update_stair_shape_at(vector![0, 0, 0]);

    let data = world.get_model_data(vector![0, 0, 0]).unwrap();
    let expected_id = ModelRegistry::stairs_model_id(StairShape::OuterLeft, true);
    assert_eq!(
        data.model_id, expected_id,
        "Inverted: back neighbor facing left → OuterLeft (flipped from InnerRight)"
    );

    // Case 2: Front neighbor → InnerRight (inverted) - would be OuterLeft if not inverted
    world.set_model_block(vector![10, 0, 0], straight_inverted, 0, false);
    world.set_model_block(vector![10, 0, -1], straight_inverted, 3, false);

    world.update_stair_shape_at(vector![10, 0, 0]);

    let data = world.get_model_data(vector![10, 0, 0]).unwrap();
    let expected_id = ModelRegistry::stairs_model_id(StairShape::InnerRight, true);
    assert_eq!(
        data.model_id, expected_id,
        "Inverted: front neighbor facing left → InnerRight (flipped from OuterLeft)"
    );

    // Case 3: Inverted stair should NOT form corner with non-inverted stair
    let straight_normal = ModelRegistry::stairs_model_id(StairShape::Straight, false);
    world.set_model_block(vector![20, 0, 0], straight_inverted, 0, false);
    world.set_model_block(vector![20, 0, 1], straight_normal, 3, false); // Non-inverted neighbor

    world.update_stair_shape_at(vector![20, 0, 0]);

    let data = world.get_model_data(vector![20, 0, 0]).unwrap();
    assert_eq!(
        data.model_id, straight_inverted,
        "Inverted stair should not corner with non-inverted neighbor"
    );

    // Case 4: Non-inverted stair should NOT form corner with inverted stair
    world.set_model_block(vector![30, 0, 0], straight_normal, 0, false);
    world.set_model_block(vector![30, 0, 1], straight_inverted, 3, false); // Inverted neighbor

    world.update_stair_shape_at(vector![30, 0, 0]);

    let data = world.get_model_data(vector![30, 0, 0]).unwrap();
    assert_eq!(
        data.model_id, straight_normal,
        "Non-inverted stair should not corner with inverted neighbor"
    );
}

#[test]
fn test_stair_shapes_all_rotations_outer_corners() {
    use crate::sub_voxel::{ModelRegistry, StairShape};
    let mut world = World::new();

    let straight_id = ModelRegistry::stairs_model_id(StairShape::Straight, false);

    // Test outer corners for all 4 rotations with front neighbors
    // Rotation 0: facing -Z, front at -Z, left = -X, right = +X
    // Rotation 1: facing +X, front at +X, left = -Z, right = +Z
    // Rotation 2: facing +Z, front at +Z, left = +X, right = -X
    // Rotation 3: facing -X, front at -X, left = +Z, right = -Z

    struct TestCase {
        rotation: u8,
        front_offset: [i32; 3],
        neighbor_rot_for_left: u8, // neighbor rotation to face our left_dir
        neighbor_rot_for_right: u8, // neighbor rotation to face our right_dir
    }

    let cases = [
        TestCase {
            rotation: 0,
            front_offset: [0, 0, -1],
            neighbor_rot_for_left: 3,  // faces -X
            neighbor_rot_for_right: 1, // faces +X
        },
        TestCase {
            rotation: 1,
            front_offset: [1, 0, 0],
            neighbor_rot_for_left: 0,  // faces -Z
            neighbor_rot_for_right: 2, // faces +Z
        },
        TestCase {
            rotation: 2,
            front_offset: [0, 0, 1],
            neighbor_rot_for_left: 1,  // faces +X
            neighbor_rot_for_right: 3, // faces -X
        },
        TestCase {
            rotation: 3,
            front_offset: [-1, 0, 0],
            neighbor_rot_for_left: 2,  // faces +Z
            neighbor_rot_for_right: 0, // faces -Z
        },
    ];

    for (i, case) in cases.iter().enumerate() {
        let base_x = (i as i32) * 20;

        // Test OuterLeft: front neighbor faces our left_dir
        let pos = vector![base_x, 0, 0];
        let front_pos = vector![
            base_x + case.front_offset[0],
            case.front_offset[1],
            case.front_offset[2]
        ];

        world.set_model_block(pos, straight_id, case.rotation, false);
        world.set_model_block(front_pos, straight_id, case.neighbor_rot_for_left, false);

        world.update_stair_shape_at(pos);

        let data = world.get_model_data(pos).unwrap();
        let expected_id = ModelRegistry::stairs_model_id(StairShape::OuterLeft, false);
        assert_eq!(
            data.model_id, expected_id,
            "Rotation {}: front neighbor facing left → OuterLeft",
            case.rotation
        );

        // Test OuterRight: front neighbor faces our right_dir
        let pos = vector![base_x + 10, 0, 0];
        let front_pos = vector![
            base_x + 10 + case.front_offset[0],
            case.front_offset[1],
            case.front_offset[2]
        ];

        world.set_model_block(pos, straight_id, case.rotation, false);
        world.set_model_block(front_pos, straight_id, case.neighbor_rot_for_right, false);

        world.update_stair_shape_at(pos);

        let data = world.get_model_data(pos).unwrap();
        let expected_id = ModelRegistry::stairs_model_id(StairShape::OuterRight, false);
        assert_eq!(
            data.model_id, expected_id,
            "Rotation {}: front neighbor facing right → OuterRight",
            case.rotation
        );
    }
}

#[test]
fn test_stair_shapes_all_rotations_inner_corners() {
    use crate::sub_voxel::{ModelRegistry, StairShape};
    let mut world = World::new();

    let straight_id = ModelRegistry::stairs_model_id(StairShape::Straight, false);

    // Test inner corners for all 4 rotations with back neighbors
    struct TestCase {
        rotation: u8,
        back_offset: [i32; 3],
        neighbor_rot_for_left: u8,
        neighbor_rot_for_right: u8,
    }

    let cases = [
        TestCase {
            rotation: 0,
            back_offset: [0, 0, 1],
            neighbor_rot_for_left: 3,
            neighbor_rot_for_right: 1,
        },
        TestCase {
            rotation: 1,
            back_offset: [-1, 0, 0],
            neighbor_rot_for_left: 0,
            neighbor_rot_for_right: 2,
        },
        TestCase {
            rotation: 2,
            back_offset: [0, 0, -1],
            neighbor_rot_for_left: 1,
            neighbor_rot_for_right: 3,
        },
        TestCase {
            rotation: 3,
            back_offset: [1, 0, 0],
            neighbor_rot_for_left: 2,
            neighbor_rot_for_right: 0,
        },
    ];

    for (i, case) in cases.iter().enumerate() {
        let base_x = (i as i32) * 20;

        // Test InnerRight: back neighbor faces our left_dir
        let pos = vector![base_x, 0, 0];
        let back_pos = vector![
            base_x + case.back_offset[0],
            case.back_offset[1],
            case.back_offset[2]
        ];

        world.set_model_block(pos, straight_id, case.rotation, false);
        world.set_model_block(back_pos, straight_id, case.neighbor_rot_for_left, false);

        world.update_stair_shape_at(pos);

        let data = world.get_model_data(pos).unwrap();
        let expected_id = ModelRegistry::stairs_model_id(StairShape::InnerRight, false);
        assert_eq!(
            data.model_id, expected_id,
            "Rotation {}: back neighbor facing left → InnerRight",
            case.rotation
        );

        // Test InnerLeft: back neighbor faces our right_dir
        let pos = vector![base_x + 10, 0, 0];
        let back_pos = vector![
            base_x + 10 + case.back_offset[0],
            case.back_offset[1],
            case.back_offset[2]
        ];

        world.set_model_block(pos, straight_id, case.rotation, false);
        world.set_model_block(back_pos, straight_id, case.neighbor_rot_for_right, false);

        world.update_stair_shape_at(pos);

        let data = world.get_model_data(pos).unwrap();
        let expected_id = ModelRegistry::stairs_model_id(StairShape::InnerLeft, false);
        assert_eq!(
            data.model_id, expected_id,
            "Rotation {}: back neighbor facing right → InnerLeft",
            case.rotation
        );
    }
}

#[test]
fn test_stair_neighbor_removal_resets_shape() {
    use crate::chunk::BlockType;
    use crate::sub_voxel::{ModelRegistry, StairShape};
    let mut world = World::new();

    let straight_id = ModelRegistry::stairs_model_id(StairShape::Straight, false);

    // Place two stairs that form a corner
    world.set_model_block(vector![0, 0, 0], straight_id, 0, false);
    world.set_model_block(vector![0, 0, -1], straight_id, 3, false);

    world.update_stair_shape_at(vector![0, 0, 0]);

    let data = world.get_model_data(vector![0, 0, 0]).unwrap();
    let outer_left = ModelRegistry::stairs_model_id(StairShape::OuterLeft, false);
    assert_eq!(data.model_id, outer_left, "Should form OuterLeft corner");

    // Remove the neighbor
    world.set_block(vector![0, 0, -1], BlockType::Air);

    // Update the stair shape
    world.update_stair_shape_at(vector![0, 0, 0]);

    let data = world.get_model_data(vector![0, 0, 0]).unwrap();
    assert_eq!(
        data.model_id, straight_id,
        "Should reset to Straight after neighbor removal"
    );
}

/// Test ceiling (inverted) stairs form correct outer corners at all rotations.
/// For inverted stairs, the shape mapping is flipped: InnerLeft↔OuterRight, InnerRight↔OuterLeft
#[test]
fn test_ceiling_stair_shapes_all_rotations_outer_corners() {
    use crate::sub_voxel::{ModelRegistry, StairShape};
    let mut world = World::new();

    let straight_inv = ModelRegistry::stairs_model_id(StairShape::Straight, true);

    // Test configurations: (main_rotation, neighbor_rotation, neighbor_offset, expected_shape)
    // For inverted stairs, OuterLeft/OuterRight are produced where floor stairs would get InnerRight/InnerLeft
    let test_cases = [
        // Rotation 0: faces -Z. Front neighbor at -Z facing perpendicular creates outer corner
        // Floor: front neighbor facing left (rot 3) → OuterLeft
        // Ceiling: flipped → InnerRight
        (
            0u8,
            3u8,
            vector![0i32, 0, -1],
            StairShape::InnerRight,
            "rot0 front-left",
        ),
        (
            0,
            1,
            vector![0, 0, -1],
            StairShape::InnerLeft,
            "rot0 front-right",
        ),
        // Rotation 1: faces +X
        (
            1,
            0,
            vector![1, 0, 0],
            StairShape::InnerRight,
            "rot1 front-left",
        ),
        (
            1,
            2,
            vector![1, 0, 0],
            StairShape::InnerLeft,
            "rot1 front-right",
        ),
        // Rotation 2: faces +Z
        (
            2,
            1,
            vector![0, 0, 1],
            StairShape::InnerRight,
            "rot2 front-left",
        ),
        (
            2,
            3,
            vector![0, 0, 1],
            StairShape::InnerLeft,
            "rot2 front-right",
        ),
        // Rotation 3: faces -X
        (
            3,
            2,
            vector![-1, 0, 0],
            StairShape::InnerRight,
            "rot3 front-left",
        ),
        (
            3,
            0,
            vector![-1, 0, 0],
            StairShape::InnerLeft,
            "rot3 front-right",
        ),
    ];

    for (i, (main_rot, neighbor_rot, offset, expected_shape, desc)) in test_cases.iter().enumerate()
    {
        let base = vector![i as i32 * 10, 0, 0];
        let neighbor_pos = base + offset;

        world.set_model_block(base, straight_inv, *main_rot, false);
        world.set_model_block(neighbor_pos, straight_inv, *neighbor_rot, false);

        world.update_stair_shape_at(base);

        let data = world.get_model_data(base).unwrap();
        let expected_id = ModelRegistry::stairs_model_id(*expected_shape, true);
        assert_eq!(
            data.model_id, expected_id,
            "Ceiling outer corner {}: expected {:?}",
            desc, expected_shape
        );
    }
}

/// Test ceiling (inverted) stairs form correct inner corners at all rotations.
#[test]
fn test_ceiling_stair_shapes_all_rotations_inner_corners() {
    use crate::sub_voxel::{ModelRegistry, StairShape};
    let mut world = World::new();

    let straight_inv = ModelRegistry::stairs_model_id(StairShape::Straight, true);

    // Test configurations for inner corners (back neighbor creates inner corner)
    // For inverted stairs, InnerLeft/InnerRight are produced where floor stairs would get OuterRight/OuterLeft
    let test_cases = [
        // Rotation 0: faces -Z. Back neighbor at +Z facing perpendicular creates inner corner
        // Floor: back neighbor facing left (rot 3) → InnerRight
        // Ceiling: flipped → OuterLeft
        (
            0u8,
            3u8,
            vector![0i32, 0, 1],
            StairShape::OuterLeft,
            "rot0 back-left",
        ),
        (
            0,
            1,
            vector![0, 0, 1],
            StairShape::OuterRight,
            "rot0 back-right",
        ),
        // Rotation 1: faces +X
        (
            1,
            0,
            vector![-1, 0, 0],
            StairShape::OuterLeft,
            "rot1 back-left",
        ),
        (
            1,
            2,
            vector![-1, 0, 0],
            StairShape::OuterRight,
            "rot1 back-right",
        ),
        // Rotation 2: faces +Z
        (
            2,
            1,
            vector![0, 0, -1],
            StairShape::OuterLeft,
            "rot2 back-left",
        ),
        (
            2,
            3,
            vector![0, 0, -1],
            StairShape::OuterRight,
            "rot2 back-right",
        ),
        // Rotation 3: faces -X
        (
            3,
            2,
            vector![1, 0, 0],
            StairShape::OuterLeft,
            "rot3 back-left",
        ),
        (
            3,
            0,
            vector![1, 0, 0],
            StairShape::OuterRight,
            "rot3 back-right",
        ),
    ];

    for (i, (main_rot, neighbor_rot, offset, expected_shape, desc)) in test_cases.iter().enumerate()
    {
        let base = vector![i as i32 * 10, 0, 0];
        let neighbor_pos = base + offset;

        world.set_model_block(base, straight_inv, *main_rot, false);
        world.set_model_block(neighbor_pos, straight_inv, *neighbor_rot, false);

        world.update_stair_shape_at(base);

        let data = world.get_model_data(base).unwrap();
        let expected_id = ModelRegistry::stairs_model_id(*expected_shape, true);
        assert_eq!(
            data.model_id, expected_id,
            "Ceiling inner corner {}: expected {:?}",
            desc, expected_shape
        );
    }
}

/// Test that floor and ceiling stairs don't form corners with each other
#[test]
fn test_floor_ceiling_stairs_dont_mix() {
    use crate::sub_voxel::{ModelRegistry, StairShape};
    let mut world = World::new();

    let straight_floor = ModelRegistry::stairs_model_id(StairShape::Straight, false);
    let straight_ceiling = ModelRegistry::stairs_model_id(StairShape::Straight, true);

    // Place floor stair with ceiling neighbor that would form corner if same type
    world.set_model_block(vector![0, 0, 0], straight_floor, 0, false);
    world.set_model_block(vector![0, 0, -1], straight_ceiling, 3, false);

    world.update_stair_shape_at(vector![0, 0, 0]);

    let data = world.get_model_data(vector![0, 0, 0]).unwrap();
    assert_eq!(
        data.model_id, straight_floor,
        "Floor stair should stay straight when neighbor is ceiling stair"
    );

    // Place ceiling stair with floor neighbor
    world.set_model_block(vector![10, 0, 0], straight_ceiling, 0, false);
    world.set_model_block(vector![10, 0, -1], straight_floor, 3, false);

    world.update_stair_shape_at(vector![10, 0, 0]);

    let data = world.get_model_data(vector![10, 0, 0]).unwrap();
    assert_eq!(
        data.model_id, straight_ceiling,
        "Ceiling stair should stay straight when neighbor is floor stair"
    );
}

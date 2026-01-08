use crate::camera::Camera;
use crate::chunk::{BlockType, CHUNK_SIZE};
use crate::config::INITIAL_WINDOW_RESOLUTION;
use crate::constants::*;
use crate::sub_voxel::ModelRegistry;
use crate::world::World;
use nalgebra::{Vector3, vector};
use winit::keyboard::KeyCode;
use winit_input_helper::WinitInputHelper;

// Player physics constants (in world/voxel units, where 1 unit = 1 block)
/// Gravity acceleration in blocks per second squared
pub const GRAVITY: f64 = 20.0;
/// Jump velocity in blocks per second
pub const JUMP_VELOCITY: f64 = 8.0;
/// Player movement speed in blocks per second
pub const MOVE_SPEED: f64 = 5.0;
/// Player hitbox half-width (X and Z)
pub const PLAYER_HALF_WIDTH: f64 = 0.3;
/// Player height (from feet to camera)
pub const PLAYER_HEIGHT: f64 = 1.6;
/// Player eye height from feet
pub const PLAYER_EYE_HEIGHT: f64 = 1.6;

// Swimming physics constants
pub const WATER_GRAVITY: f64 = 4.0;
pub const WATER_BUOYANCY: f64 = 2.0;
pub const SWIM_SPEED: f64 = 3.0;
pub const SWIM_UP_SPEED: f64 = 4.0;
pub const SWIM_DOWN_SPEED: f64 = 3.0;
pub const WATER_DRAG: f64 = 0.85;

// Ladder/climbing constants
pub const CLIMB_UP_SPEED: f64 = 4.0;
pub const CLIMB_DOWN_SPEED: f64 = 3.0;
pub const CLIMB_HORIZ_SPEED: f64 = 2.0;

/// Head bob amplitude (in blocks)
pub const HEAD_BOB_AMPLITUDE: f64 = 0.04;
/// Head bob frequency (cycles per block walked)
pub const HEAD_BOB_FREQUENCY: f64 = 0.8;

pub struct Player {
    pub camera: Camera,
    pub velocity: Vector3<f64>,
    pub on_ground: bool,
    pub head_bob_timer: f64,
    pub head_bob_intensity: f64,
    pub in_water: bool,
    pub fly_mode: bool,
    pub sprint_mode: bool,
    pub auto_jump: bool,
    pub light_enabled: bool,
    /// Spawn position in world coordinates (for respawning when falling out of world)
    spawn_pos: Vector3<f64>,
}

impl Player {
    pub fn new(
        spawn_pos: Vector3<f64>,
        texture_origin: Vector3<i32>,
        world_extent: [u32; 3],
        fly_mode: bool,
    ) -> Self {
        // Convert spawn position to texture-relative normalized camera coordinates
        let texture_relative_pos = spawn_pos - texture_origin.cast::<f64>();
        let camera_pos = Vector3::new(
            texture_relative_pos.x / world_extent[0] as f64,
            (texture_relative_pos.y + PLAYER_EYE_HEIGHT) / world_extent[1] as f64,
            texture_relative_pos.z / world_extent[2] as f64,
        );

        let mut camera = Camera::new(
            camera_pos,
            Vector3::zeros(),
            INITIAL_WINDOW_RESOLUTION.into(),
            70.0,
        );
        camera.look_at(Vector3::new(0.5, 0.25, 0.75));

        Self {
            camera,
            velocity: Vector3::zeros(),
            on_ground: false,
            head_bob_timer: 0.0,
            head_bob_intensity: 0.0,
            in_water: false,
            fly_mode,
            sprint_mode: false,
            auto_jump: true,
            light_enabled: false,
            spawn_pos,
        }
    }

    pub fn camera_direction(&self) -> Vector3<f64> {
        -self.camera.rotation_matrix().column(2).xyz()
    }

    /// World-space position of the camera (not feet), derived from normalized texture coords.
    pub fn camera_world_pos(
        &self,
        world_extent: [u32; 3],
        texture_origin: Vector3<i32>,
    ) -> Vector3<f64> {
        let scale = Vector3::new(
            world_extent[0] as f64,
            world_extent[1] as f64,
            world_extent[2] as f64,
        );
        let texture_pos = self.camera.position.component_mul(&scale);
        Vector3::new(
            texture_pos.x + texture_origin.x as f64,
            texture_pos.y + texture_origin.y as f64,
            texture_pos.z + texture_origin.z as f64,
        )
    }

    pub fn feet_pos(&self, world_extent: [u32; 3], texture_origin: Vector3<i32>) -> Vector3<f64> {
        let world_cam = self.camera_world_pos(world_extent, texture_origin);
        Vector3::new(world_cam.x, world_cam.y - PLAYER_EYE_HEIGHT, world_cam.z)
    }

    pub fn set_feet_pos(
        &mut self,
        feet_pos: Vector3<f64>,
        world_extent: [u32; 3],
        texture_origin: Vector3<i32>,
    ) {
        let scale = Vector3::new(
            world_extent[0] as f64,
            world_extent[1] as f64,
            world_extent[2] as f64,
        );
        let texture_pos = Vector3::new(
            feet_pos.x - texture_origin.x as f64,
            feet_pos.y - texture_origin.y as f64,
            feet_pos.z - texture_origin.z as f64,
        );
        self.camera.position = Vector3::new(
            texture_pos.x / scale.x,
            (texture_pos.y + PLAYER_EYE_HEIGHT) / scale.y,
            texture_pos.z / scale.z,
        );
    }

    pub fn get_chunk_pos(
        &self,
        world_extent: [u32; 3],
        texture_origin: Vector3<i32>,
    ) -> Vector3<i32> {
        let feet = self.feet_pos(world_extent, texture_origin);
        vector![
            (feet.x.floor() as i32).div_euclid(CHUNK_SIZE as i32),
            (feet.y.floor() as i32).div_euclid(CHUNK_SIZE as i32),
            (feet.z.floor() as i32).div_euclid(CHUNK_SIZE as i32)
        ]
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_physics(
        &mut self,
        delta_time: f64,
        world: &World,
        world_extent: [u32; 3],
        texture_origin: Vector3<i32>,
        input: &WinitInputHelper,
        model_registry: &ModelRegistry,
        verbose: bool,
    ) {
        let mut feet = self.feet_pos(world_extent, texture_origin);

        let head_in_water = self.check_in_water(feet, world);
        let touching_water = self.check_touching_water(feet, world);
        let touching_ladder = self.check_touching_ladder(feet, world, model_registry, verbose);
        self.in_water = head_in_water;

        // Get movement input
        let t = |k: KeyCode| input.key_held(k) as u8 as f64;
        let forward = t(KeyCode::KeyW) - t(KeyCode::KeyS);
        let right = t(KeyCode::KeyD) - t(KeyCode::KeyA);

        let yaw = self.camera.rotation.y;
        let move_dir = Vector3::new(
            -forward * yaw.sin() + right * yaw.cos(),
            0.0,
            -forward * yaw.cos() - right * yaw.sin(),
        );

        // Fly mode should ignore medium (water/ladder) speed modifiers
        let base_speed = if self.fly_mode {
            MOVE_SPEED * 2.0
        } else if touching_water {
            SWIM_SPEED
        } else if touching_ladder {
            CLIMB_HORIZ_SPEED
        } else {
            MOVE_SPEED
        };
        let current_speed = if self.sprint_mode {
            base_speed * 2.0
        } else {
            base_speed
        };

        let move_len = move_dir.magnitude();
        if move_len > 0.001 {
            let normalized = move_dir / move_len;
            self.velocity.x = normalized.x * current_speed;
            self.velocity.z = normalized.z * current_speed;
        } else {
            self.velocity.x = 0.0;
            self.velocity.z = 0.0;
        }

        if self.fly_mode {
            let shift_held = (input.key_held(KeyCode::ShiftLeft)
                || input.key_held(KeyCode::ShiftRight)) as i32 as f64;
            let up = t(KeyCode::Space) - shift_held;
            self.velocity.y = up * current_speed;

            feet.x += self.velocity.x * delta_time;
            feet.y += self.velocity.y * delta_time;
            feet.z += self.velocity.z * delta_time;
            feet.y = feet.y.clamp(0.5, TEXTURE_SIZE_Y as f64 - 0.5);
        } else if touching_water {
            self.velocity.y -= WATER_GRAVITY * delta_time;
            self.velocity.y += WATER_BUOYANCY * delta_time;
            let drag = WATER_DRAG.powf(delta_time);
            self.velocity.y *= drag;

            if input.key_held(KeyCode::Space) {
                self.velocity.y = SWIM_UP_SPEED;
            } else if input.key_held(KeyCode::ShiftLeft) || input.key_held(KeyCode::ShiftRight) {
                self.velocity.y = -SWIM_DOWN_SPEED;
            }

            let horiz_check_y = feet.y + 0.01;
            let new_x = feet.x + self.velocity.x * delta_time;
            if !self.check_collision(
                Vector3::new(new_x, horiz_check_y, feet.z),
                world,
                model_registry,
            ) {
                feet.x = new_x;
            } else {
                self.velocity.x = 0.0;
            }

            let new_z = feet.z + self.velocity.z * delta_time;
            if !self.check_collision(
                Vector3::new(feet.x, horiz_check_y, new_z),
                world,
                model_registry,
            ) {
                feet.z = new_z;
            } else {
                self.velocity.z = 0.0;
            }

            let new_y = feet.y + self.velocity.y * delta_time;
            if !self.check_collision(Vector3::new(feet.x, new_y, feet.z), world, model_registry) {
                feet.y = new_y;
            } else {
                self.velocity.y = 0.0;
            }
            self.on_ground = false;
        } else if touching_ladder
            && (input.key_held(KeyCode::Space)
                || input.key_held(KeyCode::ShiftLeft)
                || input.key_held(KeyCode::ShiftRight))
        {
            if input.key_held(KeyCode::Space) {
                self.velocity.y = CLIMB_UP_SPEED;
            } else {
                self.velocity.y = -CLIMB_DOWN_SPEED;
            }

            let horiz_check_y = feet.y + 0.01;
            let new_x = feet.x + self.velocity.x * delta_time;
            if !self.check_collision_ex(
                Vector3::new(new_x, horiz_check_y, feet.z),
                true,
                world,
                model_registry,
            ) {
                feet.x = new_x;
            } else {
                self.velocity.x = 0.0;
            }

            let new_z = feet.z + self.velocity.z * delta_time;
            if !self.check_collision_ex(
                Vector3::new(feet.x, horiz_check_y, new_z),
                true,
                world,
                model_registry,
            ) {
                feet.z = new_z;
            } else {
                self.velocity.z = 0.0;
            }

            let new_y = feet.y + self.velocity.y * delta_time;
            if !self.check_collision_ex(
                Vector3::new(feet.x, new_y, feet.z),
                true,
                world,
                model_registry,
            ) {
                feet.y = new_y;
            } else {
                self.velocity.y = 0.0;
            }
            self.on_ground = false;
        } else {
            self.velocity.y -= GRAVITY * delta_time;
            if self.on_ground && input.key_pressed(KeyCode::Space) {
                self.velocity.y = JUMP_VELOCITY;
                self.on_ground = false;
            }

            let horiz_check_y = feet.y + 0.01;
            let mut should_auto_jump = false;

            let new_x = feet.x + self.velocity.x * delta_time;
            if !self.check_collision(
                Vector3::new(new_x, horiz_check_y, feet.z),
                world,
                model_registry,
            ) {
                feet.x = new_x;
            } else {
                if self.auto_jump
                    && self.on_ground
                    && self.velocity.x.abs() > 0.1
                    && !self.check_collision(
                        Vector3::new(new_x, feet.y + 1.01, feet.z),
                        world,
                        model_registry,
                    )
                {
                    should_auto_jump = true;
                }
                self.velocity.x = 0.0;
            }

            let new_z = feet.z + self.velocity.z * delta_time;
            if !self.check_collision(
                Vector3::new(feet.x, horiz_check_y, new_z),
                world,
                model_registry,
            ) {
                feet.z = new_z;
            } else {
                if self.auto_jump
                    && self.on_ground
                    && self.velocity.z.abs() > 0.1
                    && !self.check_collision(
                        Vector3::new(feet.x, feet.y + 1.01, new_z),
                        world,
                        model_registry,
                    )
                {
                    should_auto_jump = true;
                }
                self.velocity.z = 0.0;
            }

            if should_auto_jump {
                self.velocity.y = JUMP_VELOCITY;
                self.on_ground = false;
            }

            let new_y = feet.y + self.velocity.y * delta_time;
            if !self.check_collision(Vector3::new(feet.x, new_y, feet.z), world, model_registry) {
                feet.y = new_y;
                self.on_ground = false;
            } else {
                if self.velocity.y < 0.0 {
                    feet.y = (feet.y + self.velocity.y * delta_time).floor() + 1.0;
                    self.on_ground = true;
                }
                self.velocity.y = 0.0;
            }
        }

        if feet.y < -10.0 {
            feet = self.get_spawn_pos(world);
            self.velocity = Vector3::zeros();
            self.on_ground = false;
        }

        // Push player up if stuck inside a solid block (but not when in water,
        // as water physics handles collision differently and this would cause
        // the player to be shoved to the surface when entering water).
        if self.check_collision(feet, world, model_registry) && !self.fly_mode && !touching_water {
            for offset in 1..10 {
                let test_pos = Vector3::new(feet.x, feet.y + offset as f64, feet.z);
                if !self.check_collision(test_pos, world, model_registry) {
                    feet = test_pos;
                    self.velocity.y = 0.0;
                    break;
                }
            }
        }

        let horizontal_speed = (self.velocity.x.powi(2) + self.velocity.z.powi(2)).sqrt();
        let is_walking = self.on_ground && horizontal_speed > 0.5 && !self.fly_mode;

        if is_walking {
            self.head_bob_timer += horizontal_speed * delta_time * HEAD_BOB_FREQUENCY;
            self.head_bob_intensity += (1.0 - self.head_bob_intensity) * delta_time * 8.0;
        } else {
            self.head_bob_intensity *= 0.9_f64.powf(delta_time * 60.0);
        }
        self.head_bob_intensity = self.head_bob_intensity.clamp(0.0, 1.0);

        // Clamp fly-mode vertical movement to world bounds (Y is bounded; X/Z are effectively infinite)
        if self.fly_mode {
            let min_y = 0.0;
            let max_y = world_extent[1] as f64 - PLAYER_HEIGHT;
            let clamped_y = feet.y.clamp(min_y, max_y);
            if clamped_y != feet.y {
                feet.y = clamped_y;
                self.velocity.y = 0.0;
            }
        }

        self.set_feet_pos(feet, world_extent, texture_origin);
    }

    pub fn get_spawn_pos(&self, _world: &World) -> Vector3<f64> {
        self.spawn_pos
    }

    pub fn check_collision(
        &self,
        feet_pos: Vector3<f64>,
        world: &World,
        model_registry: &ModelRegistry,
    ) -> bool {
        self.check_collision_ex(feet_pos, false, world, model_registry)
    }

    pub fn check_collision_ex(
        &self,
        feet_pos: Vector3<f64>,
        skip_ladders: bool,
        world: &World,
        model_registry: &ModelRegistry,
    ) -> bool {
        let min_x = (feet_pos.x - PLAYER_HALF_WIDTH).floor() as i32;
        let max_x = (feet_pos.x + PLAYER_HALF_WIDTH).floor() as i32;
        let min_y = feet_pos.y.floor() as i32;
        let max_y = (feet_pos.y + PLAYER_HEIGHT).floor() as i32;
        let min_z = (feet_pos.z - PLAYER_HALF_WIDTH).floor() as i32;
        let max_z = (feet_pos.z + PLAYER_HALF_WIDTH).floor() as i32;

        let player_min = Vector3::new(
            feet_pos.x - PLAYER_HALF_WIDTH,
            feet_pos.y,
            feet_pos.z - PLAYER_HALF_WIDTH,
        );
        let player_max = Vector3::new(
            feet_pos.x + PLAYER_HALF_WIDTH,
            feet_pos.y + PLAYER_HEIGHT,
            feet_pos.z + PLAYER_HALF_WIDTH,
        );

        for bx in min_x..=max_x {
            for by in min_y..=max_y {
                for bz in min_z..=max_z {
                    let world_pos = Vector3::new(bx, by, bz);
                    if let Some(block_type) = world.get_block(world_pos) {
                        if block_type.is_solid() {
                            let block_min = Vector3::new(bx as f64, by as f64, bz as f64);
                            let block_max = block_min + Vector3::new(1.0, 1.0, 1.0);
                            if player_min.x < block_max.x
                                && player_max.x > block_min.x
                                && player_min.y < block_max.y
                                && player_max.y > block_min.y
                                && player_min.z < block_max.z
                                && player_max.z > block_min.z
                            {
                                return true;
                            }
                        } else if block_type == BlockType::Model {
                            if skip_ladders {
                                if let Some(model_data) = world.get_model_data(world_pos) {
                                    if ModelRegistry::is_ladder_model(model_data.model_id) {
                                        continue;
                                    }
                                }
                            }
                            if self.check_model_collision(
                                world_pos,
                                &player_min,
                                &player_max,
                                world,
                                model_registry,
                            ) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    pub fn check_model_collision(
        &self,
        block_pos: Vector3<i32>,
        player_min: &Vector3<f64>,
        player_max: &Vector3<f64>,
        world: &World,
        model_registry: &ModelRegistry,
    ) -> bool {
        let chunk_pos = World::world_to_chunk(block_pos);
        let (lx, ly, lz) = World::world_to_local(block_pos);
        let chunk = match world.get_chunk(chunk_pos) {
            Some(c) => c,
            None => return false,
        };
        let model_data = match chunk.get_model_data(lx, ly, lz) {
            Some(d) => d,
            None => return false,
        };
        let model = match model_registry.get(model_data.model_id) {
            Some(m) => m,
            None => return false,
        };

        let block_min = Vector3::new(block_pos.x as f64, block_pos.y as f64, block_pos.z as f64);
        let block_max = block_min + Vector3::new(1.0, 1.0, 1.0);

        if player_min.x >= block_max.x
            || player_max.x <= block_min.x
            || player_min.y >= block_max.y
            || player_max.y <= block_min.y
            || player_min.z >= block_max.z
            || player_max.z <= block_min.z
        {
            return false;
        }

        let overlap_min_x = (player_min.x - block_min.x).max(0.0);
        let overlap_max_x = (player_max.x - block_min.x).min(1.0);
        let overlap_min_y = (player_min.y - block_min.y).max(0.0);
        let overlap_max_y = (player_max.y - block_min.y).min(1.0);
        let overlap_min_z = (player_min.z - block_min.z).max(0.0);
        let overlap_max_z = (player_max.z - block_min.z).min(1.0);

        let rotate_point = |x: f64, z: f64| -> (f64, f64) {
            match model_data.rotation {
                0 => (x, z),
                1 => (1.0 - z, x),
                2 => (1.0 - x, 1.0 - z),
                3 => (z, 1.0 - x),
                _ => (x, z),
            }
        };

        let step = 0.25;
        let mut local_y = overlap_min_y;
        while local_y < overlap_max_y {
            let mut local_z = overlap_min_z;
            while local_z < overlap_max_z {
                let mut local_x = overlap_min_x;
                while local_x < overlap_max_x {
                    let (rot_x, rot_z) = rotate_point(local_x, local_z);
                    if model.point_collides(rot_x as f32, local_y as f32, rot_z as f32) {
                        return true;
                    }
                    local_x += step;
                }
                local_z += step;
            }
            local_y += step;
        }
        let (rot_max_x, rot_max_z) = rotate_point(overlap_max_x - 0.001, overlap_max_z - 0.001);
        if model.point_collides(
            rot_max_x as f32,
            (overlap_max_y - 0.001) as f32,
            rot_max_z as f32,
        ) {
            return true;
        }
        false
    }

    fn check_in_water(&self, feet_pos: Vector3<f64>, world: &World) -> bool {
        let head_y = feet_pos.y + PLAYER_EYE_HEIGHT;
        let head_x = feet_pos.x.floor() as i32;
        let head_y_block = head_y.floor() as i32;
        let head_z = feet_pos.z.floor() as i32;
        world.get_block(Vector3::new(head_x, head_y_block, head_z)) == Some(BlockType::Water)
    }

    fn check_touching_water(&self, feet_pos: Vector3<f64>, world: &World) -> bool {
        let min_x = (feet_pos.x - PLAYER_HALF_WIDTH).floor() as i32;
        let max_x = (feet_pos.x + PLAYER_HALF_WIDTH).floor() as i32;
        let min_y = (feet_pos.y - 0.1).floor() as i32;
        let max_y = (feet_pos.y + PLAYER_HEIGHT).floor() as i32;
        let min_z = (feet_pos.z - PLAYER_HALF_WIDTH).floor() as i32;
        let max_z = (feet_pos.z + PLAYER_HALF_WIDTH).floor() as i32;

        for bx in min_x..=max_x {
            for by in min_y..=max_y {
                for bz in min_z..=max_z {
                    if world.get_block(Vector3::new(bx, by, bz)) == Some(BlockType::Water) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn check_touching_ladder(
        &self,
        feet_pos: Vector3<f64>,
        world: &World,
        _model_registry: &ModelRegistry,
        _verbose: bool,
    ) -> bool {
        let min_x = (feet_pos.x - PLAYER_HALF_WIDTH).floor() as i32;
        let max_x = (feet_pos.x + PLAYER_HALF_WIDTH).floor() as i32;
        let min_y = (feet_pos.y - 0.1).floor() as i32;
        let max_y = (feet_pos.y + PLAYER_HEIGHT).floor() as i32;
        let min_z = (feet_pos.z - PLAYER_HALF_WIDTH).floor() as i32;
        let max_z = (feet_pos.z + PLAYER_HALF_WIDTH).floor() as i32;

        for bx in min_x..=max_x {
            for by in min_y..=max_y {
                for bz in min_z..=max_z {
                    let pos = Vector3::new(bx, by, bz);
                    if let Some(BlockType::Model) = world.get_block(pos) {
                        if let Some(model_data) = world.get_model_data(pos) {
                            if ModelRegistry::is_ladder_model(model_data.model_id) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }
}

//! Particle system for visual effects.
//!
//! Handles particle spawning, physics simulation, and data preparation for GPU rendering.

#![allow(dead_code)]

use bytemuck::{Pod, Zeroable};
use nalgebra::Vector3;

/// Maximum number of particles that can exist at once.
pub const MAX_PARTICLES: usize = 1024;

/// Time in seconds before particles start fading if they haven't landed.
const FADE_DELAY: f32 = 2.0;

/// Duration of the fade-out effect in seconds.
const FADE_DURATION: f32 = 0.5;

/// A single particle with physics properties.
#[derive(Debug, Clone, Copy)]
pub struct Particle {
    /// Position in world coordinates.
    pub position: Vector3<f32>,
    /// Velocity in blocks per second.
    pub velocity: Vector3<f32>,
    /// Color (RGB, 0-1).
    pub color: Vector3<f32>,
    /// Time since particle was spawned (in seconds).
    pub age: f32,
    /// Time when fading started (None = not fading yet).
    pub fade_start_age: Option<f32>,
    /// Size of particle (in blocks).
    pub size: f32,
    /// Gravity multiplier (1.0 = normal, 0.0 = no gravity).
    pub gravity: f32,
    /// Whether particle is resting on ground.
    pub on_ground: bool,
}

impl Particle {
    /// Creates a new particle.
    pub fn new(
        position: Vector3<f32>,
        velocity: Vector3<f32>,
        color: Vector3<f32>,
        size: f32,
        gravity: f32,
    ) -> Self {
        Self {
            position,
            velocity,
            color,
            age: 0.0,
            fade_start_age: None,
            size,
            gravity,
            on_ground: false,
        }
    }

    /// Updates the particle physics with world collision.
    /// `is_solid` should return true if the block at (x, y, z) is solid.
    /// Returns true if particle is still alive.
    pub fn update<F>(&mut self, delta_time: f32, is_solid: F) -> bool
    where
        F: Fn(i32, i32, i32) -> bool,
    {
        // Increment age
        self.age += delta_time;

        if !self.on_ground {
            // Apply gravity
            self.velocity.y -= 20.0 * self.gravity * delta_time;

            // Air resistance
            self.velocity *= 0.98_f32.powf(delta_time * 60.0);

            // Calculate new position
            let new_pos = self.position + self.velocity * delta_time;

            // Check collision with ground (Y axis)
            let block_x = new_pos.x.floor() as i32;
            let block_y = (new_pos.y - self.size * 0.5).floor() as i32;
            let block_z = new_pos.z.floor() as i32;

            if is_solid(block_x, block_y, block_z) {
                // Land on ground - start fading
                self.on_ground = true;
                self.position.x = new_pos.x;
                self.position.y = (block_y + 1) as f32 + self.size * 0.5;
                self.position.z = new_pos.z;
                self.velocity = Vector3::zeros();

                // Start fading when landing
                if self.fade_start_age.is_none() {
                    self.fade_start_age = Some(self.age);
                }
            } else {
                // Check X collision
                let check_x = if self.velocity.x > 0.0 {
                    (new_pos.x + self.size * 0.5).floor() as i32
                } else {
                    (new_pos.x - self.size * 0.5).floor() as i32
                };
                if is_solid(check_x, self.position.y.floor() as i32, block_z) {
                    self.velocity.x *= -0.5; // Bounce
                } else {
                    self.position.x = new_pos.x;
                }

                // Check Z collision
                let check_z = if self.velocity.z > 0.0 {
                    (new_pos.z + self.size * 0.5).floor() as i32
                } else {
                    (new_pos.z - self.size * 0.5).floor() as i32
                };
                if is_solid(block_x, self.position.y.floor() as i32, check_z) {
                    self.velocity.z *= -0.5; // Bounce
                } else {
                    self.position.z = new_pos.z;
                }

                // Update Y if not blocked
                self.position.y = new_pos.y;
            }
        }

        // Start fading after FADE_DELAY if not already fading
        if self.fade_start_age.is_none() && self.age >= FADE_DELAY {
            self.fade_start_age = Some(self.age);
        }

        // Check if fade is complete
        if let Some(fade_start) = self.fade_start_age {
            let fade_progress = self.age - fade_start;
            if fade_progress >= FADE_DURATION {
                return false; // Particle is dead
            }
        }

        true
    }

    /// Gets the alpha value (1.0 until fading starts, then fades to 0).
    pub fn alpha(&self) -> f32 {
        match self.fade_start_age {
            None => 1.0, // Fully opaque until fading starts
            Some(fade_start) => {
                let fade_progress = (self.age - fade_start) / FADE_DURATION;
                (1.0 - fade_progress).clamp(0.0, 1.0)
            }
        }
    }
}

/// GPU-compatible particle data for shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GpuParticle {
    /// Position XYZ + size W
    pub pos_size: [f32; 4],
    /// Color RGB + alpha W
    pub color_alpha: [f32; 4],
}

impl From<&Particle> for GpuParticle {
    fn from(p: &Particle) -> Self {
        Self {
            pos_size: [p.position.x, p.position.y, p.position.z, p.size],
            color_alpha: [p.color.x, p.color.y, p.color.z, p.alpha()],
        }
    }
}

/// Manages all active particles.
pub struct ParticleSystem {
    particles: Vec<Particle>,
}

impl Default for ParticleSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl ParticleSystem {
    /// Creates a new empty particle system.
    pub fn new() -> Self {
        Self {
            particles: Vec::with_capacity(MAX_PARTICLES),
        }
    }

    /// Spawns a new particle. Returns false if at capacity.
    pub fn spawn(&mut self, particle: Particle) -> bool {
        if self.particles.len() >= MAX_PARTICLES {
            return false;
        }
        self.particles.push(particle);
        true
    }

    /// Updates all particles and removes dead ones.
    /// `is_solid` should return true if the block at (x, y, z) is solid.
    pub fn update<F>(&mut self, delta_time: f32, is_solid: F)
    where
        F: Fn(i32, i32, i32) -> bool + Copy,
    {
        self.particles
            .retain_mut(|p| p.update(delta_time, is_solid));
    }

    /// Returns the number of active particles.
    pub fn count(&self) -> usize {
        self.particles.len()
    }

    /// Gets GPU-ready particle data.
    pub fn gpu_data(&self) -> Vec<GpuParticle> {
        self.particles.iter().map(GpuParticle::from).collect()
    }

    /// Spawns block break particles at a position with a given color.
    pub fn spawn_block_break(&mut self, position: Vector3<f32>, color: Vector3<f32>) {
        use std::f32::consts::PI;

        // Spawn 20-36 particles
        let count = 20 + (rand_simple(position.x * 7.1 + position.z * 3.3) * 16.0) as usize;

        for i in 0..count {
            // Use well-separated seeds for each random value to avoid correlation
            let base = position.x * 127.1 + position.y * 311.7 + position.z * 74.7;
            let idx = i as f32;

            // Spread particles throughout the block volume
            // Use different prime multipliers for each axis
            let offset = Vector3::new(
                rand_simple(base + idx * 17.0) - 0.5,
                rand_simple(base + idx * 31.0 + 100.0) - 0.5,
                rand_simple(base + idx * 47.0 + 200.0) - 0.5,
            );

            // Randomize velocity direction in 3D (spherical coordinates)
            let theta = rand_simple(base + idx * 59.0 + 300.0) * 2.0 * PI; // Horizontal angle
            let phi = rand_simple(base + idx * 67.0 + 400.0) * PI * 0.4; // Vertical angle (bias upward)

            // Vary speed more dramatically
            let speed = rand_simple(base + idx * 79.0 + 500.0) * 4.0 + 1.5;

            // Convert spherical to cartesian, biased upward
            let up_bias = 0.6 + rand_simple(base + idx * 83.0 + 600.0) * 0.8;
            let velocity = Vector3::new(
                phi.sin() * theta.cos() * speed,
                phi.cos() * speed * up_bias + 1.5, // Always some upward component
                phi.sin() * theta.sin() * speed,
            );

            // Color variation
            let color_var = 0.7 + rand_simple(base + idx * 97.0 + 700.0) * 0.5;
            let varied_color = color * color_var;

            // Vary particle size (0.04 to 0.11 blocks)
            let size = 0.04 + rand_simple(base + idx * 103.0 + 800.0) * 0.07;

            let particle = Particle::new(
                position + Vector3::new(0.5, 0.5, 0.5) + offset * 0.8, // Wider spread
                velocity,
                varied_color,
                size,
                1.0, // Normal gravity
            );

            self.spawn(particle);
        }
    }

    /// Spawns water splash particles.
    pub fn spawn_water_splash(&mut self, position: Vector3<f32>) {
        use std::f32::consts::PI;

        let water_color = Vector3::new(0.3, 0.5, 0.8);

        // Spawn 6-10 splash particles
        let count = 6 + (rand_simple(position.x + position.z) * 4.0) as usize;

        for i in 0..count {
            let seed = position.x * 17.3 + position.y * 5.7 + position.z * 13.9 + i as f32;

            let angle = rand_simple(seed) * 2.0 * PI;
            let up_speed = rand_simple(seed + 1.0) * 4.0 + 3.0;
            let horiz_speed = rand_simple(seed + 2.0) * 1.5 + 0.5;

            let velocity = Vector3::new(
                angle.cos() * horiz_speed,
                up_speed,
                angle.sin() * horiz_speed,
            );

            let particle = Particle::new(
                position,
                velocity,
                water_color,
                0.08 + rand_simple(seed + 3.0) * 0.08,
                0.8, // Slightly less gravity for water
            );

            self.spawn(particle);
        }
    }

    /// Spawns dust/walking particles.
    pub fn spawn_dust(&mut self, position: Vector3<f32>, color: Vector3<f32>) {
        let seed = position.x * 11.1 + position.z * 7.7;

        // Small upward puff
        let velocity = Vector3::new(
            (rand_simple(seed) - 0.5) * 0.5,
            rand_simple(seed + 1.0) * 0.8 + 0.2,
            (rand_simple(seed + 2.0) - 0.5) * 0.5,
        );

        // Desaturate color for dust
        let dust_color = color * 0.6 + Vector3::new(0.2, 0.2, 0.2);

        let particle = Particle::new(
            position,
            velocity,
            dust_color,
            0.05 + rand_simple(seed + 3.0) * 0.05,
            0.3, // Light gravity for dust
        );

        self.spawn(particle);
    }
}

/// Simple deterministic random function.
fn rand_simple(seed: f32) -> f32 {
    let x = (seed * 12.9898 + 78.233).sin() * 43_758.547;
    x.fract()
}

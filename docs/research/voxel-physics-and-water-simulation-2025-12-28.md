# Voxel Game Physics and Water Flow Algorithms

**Research Date**: 2025-12-28
**Last Updated**: 2025-12-28
**Primary Focus**: Falling block physics, cellular automata water simulation, block update propagation, and GPU-based approaches for real-time voxel engines

Comprehensive research on physics systems for voxel-based games, covering gravity-affected blocks, fluid simulation algorithms, and efficient update propagation in chunk-based worlds.

## Table of Contents
- [Overview](#overview)
- [Falling Block Physics](#falling-block-physics)
  - [Minecraft's Gravity System](#minecrafts-gravity-system)
  - [Realistic Block Physics](#realistic-block-physics)
  - [Implementation Approaches](#implementation-approaches)
- [Water Flow Simulation](#water-flow-simulation)
  - [Cellular Automata (CA) Method](#cellular-automata-ca-method)
  - [Algorithm Details](#algorithm-details)
  - [GPU vs CPU Implementation](#gpu-vs-cpu-implementation)
- [Block Update Propagation](#block-update-propagation)
  - [Chunk-Based Updates](#chunk-based-updates)
  - [Dirty Region Tracking](#dirty-region-tracking)
  - [Update Queue Systems](#update-queue-systems)
- [Advanced Techniques](#advanced-techniques)
  - [Smooth Particle Hydrodynamics (SPH)](#smooth-particle-hydrodynamics-sph)
  - [Procedural Destruction](#procedural-destruction)
  - [Tree Physics](#tree-physics)
- [Performance Considerations](#performance-considerations)
- [Implementation Examples](#implementation-examples)
- [Further Reading](#further-reading)

## Overview

Voxel game physics presents unique challenges compared to traditional 3D game physics. The discrete grid-based nature of voxels requires specialized algorithms for simulating natural phenomena like gravity, water flow, and structural integrity. This document surveys proven approaches used in successful voxel games and academic research.

### Key Design Considerations

1. **Real-time Performance**: Physics must run at interactive frame rates (30-60+ FPS)
2. **Chunk-Based Architecture**: Most voxel engines use 16³ or 32³ chunks for memory and rendering efficiency
3. **Update Locality**: Changes should propagate efficiently without scanning the entire world
4. **Determinism**: Multiplayer games require reproducible physics across clients
5. **Scalability**: Systems must handle worlds with millions or billions of blocks

## Falling Block Physics

### Minecraft's Gravity System

Minecraft implements a simple but effective gravity system for specific block types (sand, gravel, concrete powder, etc.).

**Core Mechanism:**
- Certain blocks are flagged as gravity-affected in their block definition
- When a gravity block loses support from below, it converts to a "falling block entity"
- The entity falls with standard gravity physics (acceleration)
- Upon landing on a solid block, it converts back to a placed block
- If it lands on a non-solid block (like torches, flowers), it drops as an item

**Block Update Triggering:**
```
Player places/breaks block adjacent to gravity block
    ↓
Block receives update notification
    ↓
Check: Is there a solid block below?
    ↓ No
Convert to falling entity
    ↓
Apply gravity physics (entity system)
    ↓
Collision with ground
    ↓
Convert back to block or drop as item
```

**Suspended Block Behavior:**
- During world generation, gravity blocks may spawn "suspended" without updating
- Placing/breaking adjacent blocks triggers update cascade
- This creates the familiar "chain reaction" of falling sand/gravel

**Source**: [Minecraft Wiki - Falling Block](https://minecraft.fandom.com/wiki/Falling_Block)

### Realistic Block Physics

More advanced implementations treat blocks as having physical properties: mass, strength, and structural support requirements.

**MCGravity Plugin Architecture:**

The MCGravity plugin demonstrates a sophisticated block support system:

1. **Strength System**: Each block type has a strength value defining how many blocks it can support
   - Obsidian: strength = 15
   - Wood planks: strength = 6
   - Dirt: strength = 3

2. **Connection Graph**:
   - Blocks are "rooted" if they connect to other supported blocks within their strength radius
   - A support requires 2+ connections to the main structure
   - Connected block count × 0.25 = effective support strength (configurable)

3. **Update Algorithm**:
   ```
   When block is broken/placed:
       1. Check all blocks within update radius
       2. For each affected block:
           a. Calculate connections to rooted blocks
           b. Sum connection strengths
           c. If total strength < required: mark for falling
       3. Convert marked blocks to falling entities
   ```

**Benefits:**
- Realistic structural collapse behavior
- Player-built structures need proper engineering
- Creates interesting gameplay (bridges can collapse, towers need strong bases)

**Performance Costs:**
- Graph traversal on every block update
- Configurable update radius to balance realism vs performance

**Source**: [MCGravity Plugin - CurseForge](https://www.curseforge.com/minecraft/bukkit-plugins/mcgravity)

### Implementation Approaches

**1. Event-Driven Updates (Minecraft style)**
```rust
// Pseudocode for simple gravity
fn on_block_update(world: &mut World, pos: BlockPos) {
    let block = world.get_block(pos);

    if block.is_gravity_affected() {
        let below = pos.offset(0, -1, 0);

        if !world.get_block(below).is_solid() {
            // Remove block and spawn entity
            world.set_block(pos, AIR);
            world.spawn_falling_entity(pos, block.block_type);
        }
    }
}
```

**2. Tick-Based Scanning (Less Common)**
- Maintain list of all gravity-affected blocks
- Each physics tick, check if they should fall
- High overhead for large worlds

**3. Chunk Dirty Flagging**
- Mark chunks as "needs physics update" when modified
- Process only dirty chunks during physics tick
- Efficient for sparse updates

## Water Flow Simulation

### Cellular Automata (CA) Method

The most common approach for voxel water is cellular automata, where each cell stores a continuous water mass value and applies local rules for flow.

**Why CA for Voxels?**
- Fast: Simple arithmetic, no complex PDE solvers
- Local: Only checks immediate neighbors
- Grid-aligned: Perfect fit for voxel architecture
- Predictable: Easy to tune for desired behavior

**Limitations:**
- Not physically accurate (creates "hill" when falling into basin)
- Can be slow to settle to equilibrium
- May have artifacts like oscillation

### Algorithm Details

The definitive CA water algorithm comes from W-Shadow's tutorial, used in many indie voxel games:

**Data Structure:**
```c
// Per-voxel data
float mass[MAP_WIDTH][MAP_HEIGHT];  // 0.0 to ~1.02
int blocks[MAP_WIDTH][MAP_HEIGHT];   // AIR, GROUND, or WATER

// Constants
const float MaxMass = 1.0;       // Normal mass of full water cell
const float MaxCompress = 0.02;  // Extra water cells can hold under pressure
const float MinMass = 0.0001;    // Threshold for "empty"
const float MinFlow = 0.01;      // Minimum flow to bother with
const float MaxSpeed = 1.0;      // Max units of water moved per tick
```

**Core Flow Rules (applied in order):**

1. **Downward Flow** (Gravity)
   ```c
   // Calculate stable state between current cell and cell below
   float get_stable_state_b(float total_mass) {
       if (total_mass <= 1) {
           return 1;
       } else if (total_mass < 2*MaxMass + MaxCompress) {
           return (MaxMass*MaxMass + total_mass*MaxCompress) /
                  (MaxMass + MaxCompress);
       } else {
           return (total_mass + MaxCompress) / 2;
       }
   }

   flow = get_stable_state_b(remaining_mass + mass_below) - mass_below;
   flow = min(flow, remaining_mass);  // Don't flow more than we have
   ```

2. **Horizontal Flow** (Equalization)
   ```c
   // Left neighbor
   flow = (mass[current] - mass[left]) / 4;
   flow *= 0.5;  // Damping for smoother flow
   flow = clamp(flow, 0, remaining_mass);

   // Right neighbor (same logic)
   ```

3. **Upward Flow** (Pressure Relief)
   ```c
   // Only if compressed beyond normal capacity
   flow = remaining_mass - get_stable_state_b(remaining_mass + mass_above);
   flow *= 0.5;  // Damping
   flow = clamp(flow, 0, remaining_mass);
   ```

**Two-Buffer Update:**
```c
// CRITICAL: Use separate read/write buffers
float new_mass[MAP_WIDTH][MAP_HEIGHT];

// Copy current state
memcpy(new_mass, mass, sizeof(mass));

// Apply rules to all cells, writing to new_mass
for each cell:
    apply_flow_rules(cell, mass /* read */, new_mass /* write */);

// Swap buffers
memcpy(mass, new_mass, sizeof(mass));
```

**Why Two Buffers?**
- Prevents order-dependent artifacts
- Water would flow at different speeds depending on iteration order
- Ensures symmetrical behavior

**Iteration Rate Trade-off:**
```c
// Faster settling, higher CPU cost
for (int i = 0; i < 5; i++) {
    simulate_step();
}
```

**Sources**:
- [W-Shadow - Simple Fluid Simulation](https://w-shadow.com/blog/2009/09/01/simple-fluid-simulation/)
- [jgallant - 2D Liquid Simulator](http://www.jgallant.com/2d-liquid-simulator-with-cellular-automaton-in-unity/)

### GPU vs CPU Implementation

**CPU Implementation (DwarfCorp, most indie games):**

Advantages:
- Simpler to debug
- Easier integration with game logic
- Direct memory access
- No GPU sync overhead

Disadvantages:
- Limited parallelism
- Slower for large water volumes
- Can cause frame spikes

**GPU Implementation (Compute Shaders):**

Advantages:
- Massive parallelism (thousands of cells/frame)
- Offloads work from main thread
- Can integrate with rendering pipeline
- Explosions/instant spread nearly free

Disadvantages:
- More complex debugging (render debug textures)
- Requires careful memory management
- GPU/CPU data transfer can be bottleneck
- Harder to implement complex game rules

**GPU Cellular Automata Pattern:**
```glsl
// GLSL Compute Shader Pseudocode
#version 450

layout(local_size_x = 8, local_size_y = 8, local_size_z = 8) in;

layout(binding = 0, r32f) uniform image3D waterMassRead;
layout(binding = 1, r32f) uniform image3D waterMassWrite;

void main() {
    ivec3 pos = ivec3(gl_GlobalInvocationID.xyz);

    float mass = imageLoad(waterMassRead, pos).r;
    float remaining = mass;

    // Flow down
    if (pos.y > 0) {
        float mass_below = imageLoad(waterMassRead, pos + ivec3(0,-1,0)).r;
        float flow = calculate_flow(remaining, mass_below);
        imageStore(waterMassWrite, pos + ivec3(0,-1,0), vec4(mass_below + flow));
        remaining -= flow;
    }

    // Flow horizontally (left/right/forward/back)
    // ... similar logic

    imageStore(waterMassWrite, pos, vec4(remaining));
}
```

**Dispatch:**
```rust
// Rust + Vulkan example
let dispatch_x = (WORLD_SIZE_X + 7) / 8;
let dispatch_y = (WORLD_SIZE_Y + 7) / 8;
let dispatch_z = (WORLD_SIZE_Z + 7) / 8;

cmd_buffer.dispatch(dispatch_x, dispatch_y, dispatch_z);
```

**Noita's Decision**: The creator of Noita (famous falling sand game) chose **CPU** despite the GPU option being viable, citing:
- Development complexity
- Easier debugging
- Tight integration with game rules (7-year dev cycle)

**Sources**:
- [GPU Falling Sand - meatbatgames](https://meatbatgames.com/blog/falling-sand-gpu/) (URL fetch returned empty, but referenced in search)
- [GitHub - cellular-automata-fluid-simulation](https://github.com/0x7b1/cellular-automata-fluid-simulation)

## Block Update Propagation

Efficient block update propagation is critical for responsive physics without scanning the entire world.

### Chunk-Based Updates

**Standard Chunk Sizes:**
- 16×16×16: Minecraft's choice
- 32×32×32: Common for compute-heavy engines
- Larger = fewer chunks, slower rebuilds
- Smaller = more chunks, more overhead

**Chunk Dirty Flagging:**
```rust
struct Chunk {
    blocks: [BlockType; 32*32*32],
    is_dirty: bool,  // Needs mesh rebuild
    needs_physics_update: bool,  // Contains blocks that may update
}

fn on_block_changed(world: &mut World, pos: BlockPos) {
    let chunk_pos = pos.to_chunk_pos();
    world.get_chunk_mut(chunk_pos).is_dirty = true;

    // If on chunk boundary, mark neighbors too
    if pos.x % CHUNK_SIZE == 0 {
        world.get_chunk_mut(chunk_pos.offset(-1,0,0)).is_dirty = true;
    }
    // ... similar for all 6 faces
}
```

### Dirty Region Tracking

Instead of marking entire chunks, track sub-regions:

```rust
struct Chunk {
    dirty_min: IVec3,  // Min corner of dirty region
    dirty_max: IVec3,  // Max corner of dirty region
}

fn mark_block_dirty(chunk: &mut Chunk, local_pos: IVec3) {
    chunk.dirty_min = chunk.dirty_min.min(local_pos);
    chunk.dirty_max = chunk.dirty_max.max(local_pos);
}

fn rebuild_chunk_partial(chunk: &Chunk) {
    for x in chunk.dirty_min.x..=chunk.dirty_max.x {
        for y in chunk.dirty_min.y..=chunk.dirty_max.y {
            for z in chunk.dirty_min.z..=chunk.dirty_max.z {
                // Only rebuild this sub-region
            }
        }
    }
}
```

**Benefits:**
- Smaller rebuilds when only a few blocks change
- Especially effective for sparse updates

### Update Queue Systems

The **voxelman** (open-source voxel engine) demonstrates a sophisticated multi-queue system:

**Three-Queue Architecture:**

1. **changedChunks** - Chunks with pending physics changes
   ```rust
   // On block change
   changedChunks.insert(chunk_pos);
   ```

2. **chunksToMesh** - Chunks ready for mesh rebuild
   ```rust
   // After applying physics
   if chunk.isDirty && all_neighbors_updated {
       chunksToMesh.push(chunk_pos);
   }
   ```

3. **dirtyChunks** - Chunks currently being meshed (worker thread)
   ```rust
   // Move from chunksToMesh to dirtyChunks when worker starts
   // Remove from dirtyChunks when mesh ready
   ```

**Update Flow:**
```
Block Changed
    ↓
Add to changedChunks
    ↓
Apply Physics Updates (applyChunkChanges)
    ↓
Calculate Affected Neighbors
    ↓
Add to chunksToMesh (if neighbors ready)
    ↓
Send to Mesh Worker Thread
    ↓
Mark as dirtyChunks
    ↓
Mesh Complete → Update Render Data
```

**Critical Rule**: Only mesh chunks whose neighbors have no unapplied changes. This prevents mesh seams and artifacts.

**Sources**:
- [voxelman - Handling chunk updates](https://github.com/MrSmith33/voxelman/wiki/%5BOld%5D-Handling-chunk-updates-in-client.-Remeshing)
- [Let's Make a Voxel Engine - Chunk Management](https://sites.google.com/site/letsmakeavoxelengine/home/chunk-management)

## Advanced Techniques

### Smooth Particle Hydrodynamics (SPH)

SPH is a meshless, particle-based fluid simulation method used in high-end graphics and engineering simulations.

**Core Concept:**
- Fluid represented as particles carrying properties (density, velocity, pressure)
- Each particle influences neighbors within a "smoothing kernel" radius
- Properties interpolated using kernel functions (Gaussian, cubic spline, etc.)

**Equations (Simplified):**
```
Density: ρ_i = Σ(m_j * W(r_i - r_j, h))
Pressure Force: F_i = -Σ(m_j * (P_i + P_j)/(2*ρ_j) * ∇W(r_i - r_j, h))
Viscosity: F_i = μ * Σ(m_j * (v_j - v_i)/ρ_j * ∇²W(r_i - r_j, h))
```

Where:
- W = smoothing kernel function
- h = smoothing radius
- m = particle mass
- ρ = density
- P = pressure
- v = velocity

**GPU Implementation:**
- CUDA/OpenCL for neighbor search (typically grid-based spatial hashing)
- Parallel force calculation
- Integration (position/velocity update)

**Voxel Hybrid Approach:**
- SPH particles for fluid simulation
- Rasterize particles to voxel grid for rendering
- Voxel grid provides collision geometry
- Best of both: realistic physics + voxel rendering

**Performance:**
- 10,000+ particles for real-time (GPU)
- Neighbor search is bottleneck (O(n²) naive, O(n log n) with spatial hashing)

**Not Recommended For Most Voxel Games:**
- Extreme complexity compared to CA
- Hard to integrate with discrete block manipulation
- Better suited for realistic water in open environments

**Sources**:
- [Smooth Particle Hydrodynamics Overview](https://link.springer.com/article/10.1007/s11831-010-9040-7)
- [SPH for Interactive Applications](https://link.springer.com/article/10.1007/s00371-010-0439-9)

### Procedural Destruction

Physics-based voxel destruction requires structural integrity checking.

**Key Techniques:**

1. **Flood Fill from Anchors**
   ```rust
   // After explosion, find all disconnected groups
   let groups = flood_fill_from_ground(affected_region);

   for group in groups {
       if !group.is_connected_to_ground {
           convert_to_physics_objects(group);
       }
   }
   ```

2. **Constraint Graph** (TeaR Engine approach)
   - Each voxel group tracks connections to neighbors
   - Breaking voxels severs constraints
   - Physics islands spawn when constraints insufficient

3. **Rigid Body Grouping**
   - Disconnected voxel clusters become rigid bodies
   - Use convex hull or compound collision shapes
   - Standard physics engine (Bullet, Rapier, PhysX) handles motion

**Sources**:
- [Voxel Destruction Physics - Roblox](https://www.roblox.com/games/11594344738/Voxel-Destruction-Physics)
- [TeaR Engine - Developer Forum](https://devforum.roblox.com/t/tear-engine-a-destruction-physics-engine-020/2041924)

### Tree Physics

**Lithosphere Approach** (Marching Cubes + UE4 Physics):

Trees fall when ground beneath is removed:

```cpp
// Pseudocode
void OnVoxelRemoved(Vec3 pos) {
    // Check if any trees are above this position
    for (Tree tree : trees_in_region) {
        if (!tree.IsGrounded()) {
            // Convert tree to rigid body
            tree.ConvertToPhysicsActor();
            tree.ApplyGravity();
        }
    }
}

bool Tree::IsGrounded() {
    // Flood fill down from trunk base
    // Check if any path reaches solid ground
    return FloodFillToGround(trunk_base);
}
```

**Challenges:**
- Voxel granularity vs smooth tree movement
- Performance for large forests
- Believable breaking points (trunk should split realistically)

**Sources**:
- [Lithosphere - Voxel Terrain + Falling Trees](https://www.moddb.com/games/lithosphere/news/voxel-terrain-falling-trees-test)

## Performance Considerations

### CPU vs GPU Trade-offs

| Aspect | CPU | GPU |
|--------|-----|-----|
| Development Time | Fast | Slow |
| Debugging | Easy (prints, debugger) | Hard (render debug textures) |
| Integration | Direct | Sync overhead |
| Scalability | Linear | Massive parallel |
| Best For | Complex rules, sparse updates | Simple rules, dense updates |

### Chunk Size Impact

**Larger Chunks (32³, 64³):**
- ✅ Fewer draw calls
- ✅ Less chunk management overhead
- ❌ Slower mesh rebuilds
- ❌ Larger memory allocations

**Smaller Chunks (8³, 16³):**
- ✅ Faster rebuilds
- ✅ Finer-grained culling
- ❌ More chunks to manage
- ❌ More draw calls (without instancing)

**Recommendation**: 32×32×32 for compute-heavy engines, 16×16×16 for CPU-based

### Update Frequency

**Strategies:**

1. **Fixed Substep**
   ```rust
   const PHYSICS_HZ: f32 = 30.0;
   const SUBSTEPS: usize = 5;

   for _ in 0..SUBSTEPS {
       update_water_physics(dt / SUBSTEPS as f32);
   }
   ```

2. **Adaptive Iteration**
   ```rust
   // Run until stable or timeout
   for i in 0..max_iterations {
       let changes = update_physics();
       if changes < threshold { break; }
   }
   ```

3. **Distance-Based LOD**
   ```rust
   // Update nearby chunks more frequently
   let update_freq = match distance_to_player {
       0..=32 => EveryFrame,
       33..=128 => Every2Frames,
       _ => Every10Frames,
   };
   ```

### Memory Optimization

**Sparse Voxel Storage:**
```rust
// Don't allocate chunks with no water
if chunk.water_count == 0 {
    chunk.water_mass = None;  // Free memory
}

// Lazy allocation
fn get_water_mass_mut(&mut self) -> &mut [f32; CHUNK_SIZE_CUBED] {
    self.water_mass.get_or_insert_with(|| Box::new([0.0; CHUNK_SIZE_CUBED]))
}
```

**Quantization:**
```rust
// Store as u8 instead of f32 (4× memory savings)
// Range 0-255 mapped to 0.0-1.02
fn encode_mass(mass: f32) -> u8 {
    (mass * 250.0).clamp(0.0, 255.0) as u8
}

fn decode_mass(encoded: u8) -> f32 {
    encoded as f32 / 250.0
}
```

## Implementation Examples

### Simple Gravity Block System (Rust)

```rust
#[derive(Clone, Copy, PartialEq)]
enum BlockType {
    Air,
    Stone,
    Sand,     // Gravity-affected
    Gravel,   // Gravity-affected
}

impl BlockType {
    fn is_gravity_affected(&self) -> bool {
        matches!(self, BlockType::Sand | BlockType::Gravel)
    }

    fn is_solid(&self) -> bool {
        !matches!(self, BlockType::Air)
    }
}

struct World {
    blocks: HashMap<IVec3, BlockType>,
    falling_blocks: Vec<FallingBlock>,
}

struct FallingBlock {
    position: Vec3,
    velocity: Vec3,
    block_type: BlockType,
}

impl World {
    fn update_physics(&mut self, dt: f32) {
        // Update falling blocks
        for block in &mut self.falling_blocks {
            block.velocity.y -= 9.8 * dt;  // Gravity
            block.position += block.velocity * dt;

            // Check collision with ground
            let grid_pos = block.position.floor().as_ivec3();
            let below = grid_pos + IVec3::new(0, -1, 0);

            if self.get_block(below).is_solid() {
                // Landed
                self.set_block(grid_pos, block.block_type);
                block.mark_for_removal();
            }
        }

        self.falling_blocks.retain(|b| !b.is_marked_for_removal());
    }

    fn on_block_changed(&mut self, pos: IVec3) {
        // Check blocks above for gravity
        for dy in 1..=16 {
            let above = pos + IVec3::new(0, dy, 0);
            let block = self.get_block(above);

            if !block.is_gravity_affected() {
                break;  // Hit non-gravity block or air
            }

            // Check if it should fall
            let below = above + IVec3::new(0, -1, 0);
            if !self.get_block(below).is_solid() {
                self.set_block(above, BlockType::Air);
                self.falling_blocks.push(FallingBlock {
                    position: above.as_vec3(),
                    velocity: Vec3::ZERO,
                    block_type: block,
                });
            }
        }
    }
}
```

### Cellular Automata Water (Simplified)

```rust
struct WaterSimulator {
    mass: Vec<f32>,  // Flat array for cache efficiency
    new_mass: Vec<f32>,
    blocks: Vec<BlockType>,
    width: usize,
    height: usize,
    depth: usize,
}

const MAX_MASS: f32 = 1.0;
const MAX_COMPRESS: f32 = 0.02;
const MIN_FLOW: f32 = 0.01;

impl WaterSimulator {
    fn index(&self, x: usize, y: usize, z: usize) -> usize {
        x + y * self.width + z * self.width * self.height
    }

    fn simulate_step(&mut self) {
        // Copy current to new
        self.new_mass.copy_from_slice(&self.mass);

        for x in 0..self.width {
            for y in 0..self.height {
                for z in 0..self.depth {
                    let idx = self.index(x, y, z);

                    if self.blocks[idx] == BlockType::Air && self.mass[idx] < MIN_FLOW {
                        continue;  // Skip empty cells
                    }

                    let mut remaining = self.mass[idx];

                    // Flow down
                    if y > 0 {
                        let below_idx = self.index(x, y-1, z);
                        let flow = self.calculate_vertical_flow(remaining, self.mass[below_idx]);
                        self.new_mass[below_idx] += flow;
                        remaining -= flow;
                    }

                    // Flow horizontally (simplified - just left/right)
                    if x > 0 {
                        let left_idx = self.index(x-1, y, z);
                        let flow = ((remaining - self.mass[left_idx]) / 4.0).max(0.0);
                        self.new_mass[left_idx] += flow;
                        remaining -= flow;
                    }

                    self.new_mass[idx] = remaining;
                }
            }
        }

        // Swap buffers
        std::mem::swap(&mut self.mass, &mut self.new_mass);
    }

    fn calculate_vertical_flow(&self, source: f32, dest: f32) -> f32 {
        let sum = source + dest;

        let stable_dest = if sum <= MAX_MASS {
            MAX_MASS
        } else if sum < 2.0 * MAX_MASS + MAX_COMPRESS {
            (MAX_MASS * MAX_MASS + sum * MAX_COMPRESS) / (MAX_MASS + MAX_COMPRESS)
        } else {
            (sum + MAX_COMPRESS) / 2.0
        };

        (stable_dest - dest).max(0.0).min(source)
    }
}
```

### DDA Ray Marching for Block Picking

```rust
// Based on Amanatides & Woo algorithm
pub fn raycast_block(
    world: &World,
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
) -> Option<(IVec3, IVec3)> {  // (hit_pos, face_normal)
    let mut pos = origin.floor().as_ivec3();

    let step = IVec3::new(
        if direction.x > 0.0 { 1 } else { -1 },
        if direction.y > 0.0 { 1 } else { -1 },
        if direction.z > 0.0 { 1 } else { -1 },
    );

    // Distance to next voxel boundary
    let t_delta = Vec3::new(
        1.0 / direction.x.abs(),
        1.0 / direction.y.abs(),
        1.0 / direction.z.abs(),
    );

    // Initial t_max values
    let mut t_max = Vec3::new(
        if direction.x > 0.0 {
            (pos.x as f32 + 1.0 - origin.x) / direction.x
        } else {
            (origin.x - pos.x as f32) / -direction.x
        },
        // ... similar for y, z
    );

    let mut t = 0.0;
    let mut normal = IVec3::ZERO;

    while t < max_distance {
        if world.get_block(pos).is_solid() {
            return Some((pos, normal));
        }

        // Step to next voxel
        if t_max.x < t_max.y {
            if t_max.x < t_max.z {
                pos.x += step.x;
                t = t_max.x;
                t_max.x += t_delta.x;
                normal = IVec3::new(-step.x, 0, 0);
            } else {
                pos.z += step.z;
                t = t_max.z;
                t_max.z += t_delta.z;
                normal = IVec3::new(0, 0, -step.z);
            }
        } else {
            if t_max.y < t_max.z {
                pos.y += step.y;
                t = t_max.y;
                t_max.y += t_delta.y;
                normal = IVec3::new(0, -step.y, 0);
            } else {
                pos.z += step.z;
                t = t_max.z;
                t_max.z += t_delta.z;
                normal = IVec3::new(0, 0, -step.z);
            }
        }
    }

    None
}
```

## Key Takeaways

1. **Falling Blocks**: Event-driven updates triggered by block changes are most efficient. Store gravity-affected blocks in entity system once falling.

2. **Water Simulation**: Cellular automata with mass-based flow is the industry standard for voxel games. GPU implementation offers massive speedup but significant complexity.

3. **Block Updates**: Use chunk dirty flagging with multi-queue systems. Only process chunks whose neighbors are stable.

4. **Chunk Size**: 32×32×32 is optimal for compute shader engines, 16×16×16 for CPU.

5. **Performance**: Distance-based LOD, sparse storage, and quantization are essential for large worlds.

6. **SPH**: Only consider for high-fidelity water in limited spaces; CA is better for gameplay-focused voxel games.

7. **Destruction**: Use flood-fill from anchors to detect disconnected structures.

## Further Reading

### Core Algorithms
- [A Fast Voxel Traversal Algorithm for Ray Tracing (1987)](http://www.cse.yorku.ca/~amana/research/grid.pdf) - Original DDA paper by Amanatides & Woo
- [W-Shadow - Simple Fluid Simulation](https://w-shadow.com/blog/2009/09/01/simple-fluid-simulation/) - Definitive CA water tutorial
- [jgallant - 2D Liquid Simulator with Cellular Automaton](http://www.jgallant.com/2d-liquid-simulator-with-cellular-automaton-in-unity/) - Unity implementation with source code

### Voxel Engine Optimization
- [Vercidium - Optimised CPU Ray Marching](https://vercidium.com/blog/optimised-voxel-raymarching/) - Chunk-aware DDA optimization
- [Vercidium - Voxel World Optimisations](https://vercidium.com/blog/voxel-world-optimisations/) - Mesh generation optimization
- [Let's Make a Voxel Engine - Chunk Management](https://sites.google.com/site/letsmakeavoxelengine/home/chunk-management) - Comprehensive chunk system design
- [voxelman Wiki - Chunk Updates](https://github.com/MrSmith33/voxelman/wiki/%5BOld%5D-Handling-chunk-updates-in-client.-Remeshing) - Multi-queue update system

### Academic Resources
- [Integrating Real-Time Fluid Simulation with a Voxel Engine](https://link.springer.com/article/10.1007/s40869-016-0020-5) - Academic paper on GPU fluid simulation
- [Cellular Automata Fluid Physics For Voxel Engines](https://studenttheses.uu.nl/handle/20.500.12932/42762) - Master's thesis on advanced CA methods
- [Smooth Particle Hydrodynamics - Overview](https://link.springer.com/article/10.1007/s11831-010-9040-7) - Comprehensive SPH review

### Implementations
- [GitHub - Vercidium/voxel-ray-marching](https://github.com/Vercidium/voxel-ray-marching) - Optimized C# ray marching
- [GitHub - jongallant/LiquidSimulator](https://github.com/jongallant/LiquidSimulator) - Unity CA water with source
- [GitHub - 0x7b1/cellular-automata-fluid-simulation](https://github.com/0x7b1/cellular-automata-fluid-simulation) - GPU compute shader CA
- [Voxel.Wiki - 3D Raycasting](https://voxel.wiki/wiki/raycasting/) - Collection of DDA implementations

### Game Development
- [Minecraft Wiki - Falling Block](https://minecraft.fandom.com/wiki/Falling_Block) - Minecraft's gravity system
- [How Water Works In DwarfCorp](https://www.gamedeveloper.com/programming/how-water-works-in-dwarfcorp) - CA water in commercial game
- [Lithosphere - Voxel Terrain + Falling Trees](https://www.moddb.com/games/lithosphere/news/voxel-terrain-falling-trees-test) - Tree physics demo

### Tools & Plugins
- [MCGravity Plugin](https://www.curseforge.com/minecraft/bukkit-plugins/mcgravity) - Realistic block support system
- [Realistic Block Physics Mod](https://www.curseforge.com/minecraft/mc-mods/realistic-block-physics) - Mass-based structural physics

## Notes

**Performance Benchmarks** (from research):
- W-Shadow's CA: ~1ms for 256×256 grid on 2009 hardware
- Vercidium's DDA: 0.48ms chunk initialization (32³)
- GPU CA: ~0.1ms for 128³ volume on modern GPU

**Common Pitfalls:**
- Single-buffer updates create directional bias in water flow
- Forgetting to check chunk boundaries causes visual seams
- Not clamping flow values can create negative mass
- Iterating too few times creates slow, unrealistic water

**Multiplayer Considerations:**
- Water simulation must be deterministic for client prediction
- Use fixed timestep for physics
- Quantize mass values to u8/u16 for network sync
- Consider server-authoritative with client-side prediction

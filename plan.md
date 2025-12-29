# Voxel Engine Enhancement Plan

## Overview

Transform the current fixed-size voxel engine into an infinite world system with physics and water simulation, similar to Minecraft.

---

## Phase 1: Infinite Chunk Streaming

### Goal
Remove world size limitations and enable procedural generation of unlimited terrain.

### Current Limitations
- World bounds hardcoded: 16×4×16 chunks (512×128×512 blocks)
- Single monolithic 3D texture (33MB fixed)
- Terrain generated only within bounds
- No chunk unloading from GPU memory

### Implementation Tasks

#### 1.1 Remove World Bounds
- [ ] Remove `WORLD_CHUNKS_X/Y/Z` constants from main.rs
- [ ] Remove `WORLD_SIZE_X/Y/Z` constants
- [ ] Update shader to handle unbounded coordinates
- [ ] Change coordinate system to support negative chunk positions

#### 1.2 Virtual Texture System
- [ ] Replace single 3D texture with chunk-indexed system
- [ ] Create indirection table (chunk position → texture slot)
- [ ] Implement texture slot allocation/deallocation
- [ ] Update shader to use indirection lookup

#### 1.3 Dynamic Chunk Management
- [ ] Modify `World` to support unlimited chunks
- [ ] Implement chunk loading priority queue (distance-based)
- [ ] Add chunk unloading when beyond unload distance
- [ ] Track GPU memory usage, evict least-recently-used chunks

#### 1.4 Async Terrain Generation
- [ ] Move terrain generation to background thread(s)
- [ ] Use channel to send generated chunks to main thread
- [ ] Implement chunk request queue
- [ ] Add loading placeholder (or skip rendering unloaded chunks)

#### 1.5 Shader Updates
- [ ] Update `traverse.comp` for unbounded world
- [ ] Implement chunk-aware ray marching
- [ ] Add empty chunk early-out optimization
- [ ] Handle chunk boundaries in ray traversal

### Technical Approach

**Chunk Texture Pool:**
```
- Allocate pool of 3D texture slots (e.g., 512 slots × 32³ each)
- Indirection buffer: maps chunk coords → slot index (or -1 if not loaded)
- When chunk loads: allocate slot, upload data, update indirection
- When chunk unloads: mark slot as free, update indirection to -1
```

**Shader Indirection:**
```glsl
// Instead of: blockType = texture(blockData, worldPos)
// Use:
ivec3 chunkPos = worldPos / 32;
int slotIndex = indirectionBuffer[hash(chunkPos)];
if (slotIndex < 0) return AIR; // Chunk not loaded
ivec3 localPos = worldPos % 32;
blockType = textureSlots[slotIndex][localPos];
```

---

## Phase 2: Block Physics System

### Goal
Implement gravity-affected blocks and structural integrity for tree chopping.

### Features
- Falling blocks (sand, gravel, logs, leaves)
- Tree chopping: break trunk → tree falls
- Block support system

### Implementation Tasks

#### 2.1 Falling Entity System
- [ ] Create `FallingBlock` entity struct (position, velocity, block_type)
- [ ] Add entity list to game state
- [ ] Implement gravity physics for entities
- [ ] Convert entity back to block on landing
- [ ] Render falling blocks as particles or mini-voxels

#### 2.2 Block Support Detection
- [ ] Define which blocks are gravity-affected (sand, gravel, logs, leaves)
- [ ] On block break: check if neighbors lose support
- [ ] Propagate support check through connected blocks
- [ ] Convert unsupported blocks to falling entities

#### 2.3 Tree Chopping
- [ ] Detect when log block is broken
- [ ] Flood-fill to find connected logs and leaves
- [ ] Check if any logs still connect to ground
- [ ] If not: convert entire tree to falling entities
- [ ] Add satisfying "tree fall" animation

#### 2.4 Block Update Queue
- [ ] Create block update queue (position + update type)
- [ ] Process N updates per frame (prevent lag spikes)
- [ ] Prioritize updates near player
- [ ] Cascade updates to neighbors when needed

### Technical Approach

**Support Algorithm:**
```rust
fn check_support(world: &World, pos: BlockPos) -> bool {
    let block = world.get_block(pos);
    match block {
        BlockType::Sand | BlockType::Gravel => {
            // Supported only if solid block below
            world.get_block(pos.down()).is_solid()
        }
        BlockType::Log => {
            // Supported if connected to ground via other logs
            flood_fill_to_ground(world, pos, BlockType::Log)
        }
        BlockType::Leaves => {
            // Supported if within 4 blocks of a log
            find_nearby_log(world, pos, 4)
        }
        _ => true // Most blocks don't need support
    }
}
```

---

## Phase 3: Water Flow Simulation

### Goal
Implement cellular automata water that flows, fills basins, and responds to terrain changes.

### Features
- Water flows downhill and spreads horizontally
- Water has "mass" (depth/pressure)
- Source blocks generate infinite water
- Water interacts with player (swimming already exists)

### Implementation Tasks

#### 3.1 Water Mass System
- [ ] Change water from boolean to float (mass per cell)
- [ ] Store water mass in separate data structure (not block type)
- [ ] Render water level based on mass (partial blocks)
- [ ] Update shader to render variable water heights

#### 3.2 Cellular Automata Flow
- [ ] Implement W-Shadow algorithm for flow
- [ ] Double-buffer system (read from A, write to B, swap)
- [ ] Flow priority: down > horizontal > up (pressure)
- [ ] Add flow damping to prevent oscillation

#### 3.3 Water Update System
- [ ] Track "active" water cells (not static/full)
- [ ] Update only active cells each frame
- [ ] Deactivate cells when flow stabilizes
- [ ] Reactivate neighbors when terrain changes

#### 3.4 Source Blocks
- [ ] Mark certain water blocks as "source" (infinite)
- [ ] Sources always have mass = 1.0
- [ ] Player-placed water creates source
- [ ] Oceans/lakes use source blocks at surface

#### 3.5 GPU Water Rendering
- [ ] Update shader for variable water heights
- [ ] Smooth water surface between cells
- [ ] Keep existing wave animation
- [ ] Add flow direction to caustics

### Technical Approach

**W-Shadow Flow Algorithm:**
```rust
const MAX_MASS: f32 = 1.0;
const MAX_COMPRESS: f32 = 0.02;
const MIN_FLOW: f32 = 0.01;

fn calculate_flow(world: &WaterGrid, pos: BlockPos) -> FlowResult {
    let mass = world.get_mass(pos);
    let mut remaining = mass;
    let mut flows = FlowResult::default();

    // Flow down (gravity)
    let below = world.get_mass(pos.down());
    if below < MAX_MASS + MAX_COMPRESS {
        let flow = min(remaining, (MAX_MASS + MAX_COMPRESS) - below);
        flows.down = flow;
        remaining -= flow;
    }

    // Flow horizontal (equal distribution)
    if remaining > MIN_FLOW {
        let neighbors = [pos.north(), pos.south(), pos.east(), pos.west()];
        let lower_neighbors: Vec<_> = neighbors.iter()
            .filter(|n| world.get_mass(**n) < remaining)
            .collect();

        if !lower_neighbors.is_empty() {
            let avg = remaining / (lower_neighbors.len() + 1) as f32;
            for n in lower_neighbors {
                let neighbor_mass = world.get_mass(*n);
                if neighbor_mass < avg {
                    let flow = (avg - neighbor_mass) * 0.5; // Damping
                    flows.set(*n - pos, flow);
                    remaining -= flow;
                }
            }
        }
    }

    flows
}
```

---

## Phase 4: Performance Optimizations

### Goal
Maintain 60+ FPS with infinite world, physics, and water.

### Implementation Tasks

#### 4.1 Sparse Voxel Octree (Optional)
- [ ] Implement SVO structure for empty space skipping
- [ ] Build SVO per chunk on generation
- [ ] Modify ray marcher to use SVO for large skips
- [ ] Benchmark: only implement if needed

#### 4.2 Level of Detail
- [ ] Reduce detail for distant chunks
- [ ] Skip AO/shadows beyond threshold (already done)
- [ ] Consider mesh-based rendering for very distant terrain
- [ ] Implement Transvoxel for LOD transitions (if mesh-based)

#### 4.3 Multithreading
- [ ] Terrain generation on thread pool
- [ ] Physics updates on separate thread
- [ ] Water simulation on separate thread (or GPU)
- [ ] Careful synchronization with main thread

#### 4.4 GPU Compute for Water
- [ ] Move water CA to compute shader
- [ ] Ping-pong between two 3D textures
- [ ] Only update chunks with active water
- [ ] Significantly faster than CPU for large water bodies

---

## Implementation Order

1. **Phase 1.1-1.2**: Remove bounds, virtual textures (foundation)
2. **Phase 1.3-1.4**: Dynamic chunks, async generation (playable infinite world)
3. **Phase 1.5**: Shader updates (complete infinite world)
4. **Phase 2.1-2.2**: Falling blocks, support detection
5. **Phase 2.3**: Tree chopping
6. **Phase 3.1-3.2**: Water mass and flow
7. **Phase 3.3-3.5**: Water polish
8. **Phase 4**: Optimize as needed

---

## Success Criteria

- [ ] World generates infinitely in all horizontal directions
- [ ] Steady 60+ FPS while moving through world
- [ ] Trees fall when trunk is broken
- [ ] Water flows naturally and fills basins
- [ ] No memory leaks (chunks properly unload)
- [ ] Smooth chunk loading (no stutter)

---

## Research References

- `docs/research/infinite-voxel-world-optimization-2025-12-28.md`
- `docs/research/voxel-physics-and-water-simulation-2025-12-28.md`
- Amanatides & Woo DDA Algorithm (1987)
- W-Shadow Cellular Automata Water
- NVIDIA Sparse Voxel Octrees (Laine & Karras, 2010)

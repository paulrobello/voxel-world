# Voxel Physics Algorithms - Quick Reference

**Research Date**: 2025-12-28

Quick lookup guide for implementing physics in voxel engines. For detailed explanations, see [voxel-physics-and-water-simulation-2025-12-28.md](./voxel-physics-and-water-simulation-2025-12-28.md).

## Algorithm Selection Matrix

| Feature | Best Algorithm | Complexity | Performance | Realism |
|---------|---------------|------------|-------------|---------|
| Falling sand/gravel | Event-driven updates | Low | Excellent | Good |
| Water flow | Cellular Automata | Medium | Good (CPU) / Excellent (GPU) | Fair |
| Realistic fluids | Smooth Particle Hydrodynamics | Very High | Poor (CPU) / Good (GPU) | Excellent |
| Structural collapse | Flood-fill from anchors | Medium | Good | Fair |
| Tree falling | Grounding check + rigid body | Low | Excellent | Good |

## Cellular Automata Water - Constants

```c
const float MaxMass = 1.0;       // Normal mass of full water cell
const float MaxCompress = 0.02;  // Extra water under pressure (1.02 total)
const float MinMass = 0.0001;    // Threshold for "empty"
const float MinFlow = 0.01;      // Don't bother flowing less than this
const float MaxSpeed = 1.0;      // Max units of water moved per update
```

## CA Water - Core Equation

```c
// Calculate how much water bottom cell should hold
float get_stable_state_b(float total_mass) {
    if (total_mass <= 1) {
        return 1;
    } else if (total_mass < 2*MaxMass + MaxCompress) {
        return (MaxMass*MaxMass + total_mass*MaxCompress) / (MaxMass + MaxCompress);
    } else {
        return (total_mass + MaxCompress) / 2;
    }
}
```

## Update Order (Critical!)

Always apply in this order:

1. **Down** (gravity)
2. **Left/Right/Forward/Back** (equalization)
3. **Up** (pressure relief)

Use **two buffers** to prevent order-dependent artifacts!

## DDA Ray Marching - Core Loop

```rust
while t < max_distance {
    if is_solid(current_pos) {
        return hit(current_pos, normal);
    }

    // Step to closest voxel boundary
    if t_max.x < t_max.y && t_max.x < t_max.z {
        pos.x += step.x;
        t = t_max.x;
        t_max.x += t_delta.x;
        normal.x = -step.x;
    } else if t_max.y < t_max.z {
        pos.y += step.y;
        // ... etc
    }
}
```

## Chunk Size Recommendations

| Use Case | Size | Rationale |
|----------|------|-----------|
| CPU-based engine | 16³ | Fast rebuilds, lower latency |
| GPU compute shaders | 32³ | Better GPU utilization |
| Sparse worlds | 32³-64³ | Fewer empty chunks |
| Dense, detailed worlds | 16³ | Finer-grained updates |

## Performance Optimization Checklist

- [ ] Use dirty region tracking (not full chunk rebuilds)
- [ ] Implement chunk LOD (update nearby chunks more frequently)
- [ ] Quantize water mass to u8/u16 (4× memory savings)
- [ ] Lazy-allocate water data (only if chunk has water)
- [ ] Process chunks only when neighbors are stable
- [ ] Use fixed timestep for deterministic physics
- [ ] Consider GPU compute for >10,000 active water cells

## Falling Block Pattern

```rust
fn on_block_changed(pos) {
    // Check blocks above
    for dy in 1..scan_height {
        let above = pos.offset(0, dy, 0);
        if is_gravity_block(above) && !is_solid(above.below()) {
            remove_block(above);
            spawn_falling_entity(above);
        }
    }
}

fn update_falling_entities(dt) {
    for entity in falling_entities {
        entity.velocity.y -= 9.8 * dt;
        entity.position += entity.velocity * dt;

        if is_solid(entity.position.below()) {
            place_block(entity.position.floor());
            remove_entity(entity);
        }
    }
}
```

## Multi-Queue Chunk Update System

```
Block Changed → changedChunks queue
    ↓
Apply physics updates
    ↓
Calculate affected neighbors
    ↓
chunksToMesh queue (only if neighbors ready!)
    ↓
Send to worker thread
    ↓
dirtyChunks queue (being meshed)
    ↓
Mesh complete → upload to GPU
```

**Critical**: Never mesh a chunk if neighbors have pending updates!

## GPU Compute Shader Dispatch

```glsl
// Work group size
layout(local_size_x = 8, local_size_y = 8, local_size_z = 8) in;

// Dispatch
dispatch_x = (world_size_x + 7) / 8;
dispatch_y = (world_size_y + 7) / 8;
dispatch_z = (world_size_z + 7) / 8;
```

**Memory**: Use double-buffered 3D textures (ping-pong between read/write)

## Water Mass Quantization

```rust
// Encode f32 (0.0-1.02) to u8 (0-255)
fn encode(mass: f32) -> u8 {
    (mass * 250.0).clamp(0.0, 255.0) as u8
}

fn decode(val: u8) -> f32 {
    val as f32 / 250.0
}
```

Saves 75% memory! (~1 error in mass value)

## Common Bugs and Fixes

| Bug | Cause | Fix |
|-----|-------|-----|
| Water flows faster in one direction | Single buffer update | Use two buffers, swap after update |
| Water creates "stairs" pattern | Integer truncation | Use floating point for t_max in DDA |
| Chunk seams visible | Meshing before neighbors update | Check neighbor stability before meshing |
| Water oscillates forever | No damping | Multiply flow by 0.5 (damping factor) |
| Negative water mass | Flow > remaining_mass | Clamp flow to min(calculated, remaining) |

## Benchmark Targets (Modern Hardware)

| Operation | Target Time | Notes |
|-----------|-------------|-------|
| Chunk mesh rebuild (32³) | <1ms | CPU, single thread |
| Water update (32³ chunk) | <0.5ms | CPU, 5 substeps |
| Water update (128³ volume) | <0.2ms | GPU compute shader |
| DDA raycast (100 blocks) | <0.01ms | Chunk-aware optimization |
| Block update propagation | <0.1ms | Per chunk with dirty regions |

## Essential Code Snippets

### Chunk-Aware Block Access

```rust
// Fast: Use bit operations
let chunk_x = x >> 5;  // Divide by 32
let local_x = x & 0x1f;  // Modulo 32

// Slow: Don't do this every frame
let chunk_x = x / CHUNK_SIZE;
let local_x = x % CHUNK_SIZE;
```

### Boundary Dirty Marking

```rust
fn mark_chunk_dirty(world: &mut World, pos: IVec3) {
    let chunk_pos = pos.to_chunk();
    world.mark_dirty(chunk_pos);

    // If on boundary, mark neighbors
    let local = pos.to_local();
    if local.x == 0 { world.mark_dirty(chunk_pos.offset(-1, 0, 0)); }
    if local.x == 31 { world.mark_dirty(chunk_pos.offset(1, 0, 0)); }
    if local.y == 0 { world.mark_dirty(chunk_pos.offset(0, -1, 0)); }
    if local.y == 31 { world.mark_dirty(chunk_pos.offset(0, 1, 0)); }
    if local.z == 0 { world.mark_dirty(chunk_pos.offset(0, 0, -1)); }
    if local.z == 31 { world.mark_dirty(chunk_pos.offset(0, 0, 1)); }
}
```

### Sparse Chunk Allocation

```rust
struct Chunk {
    blocks: Box<[BlockType; 32*32*32]>,  // Always allocated
    water: Option<Box<[f32; 32*32*32]>>,  // Only if needed
}

impl Chunk {
    fn add_water(&mut self, local_pos: IVec3, mass: f32) {
        let water = self.water.get_or_insert_with(|| {
            Box::new([0.0; 32*32*32])
        });
        water[Self::index(local_pos)] = mass;
    }
}
```

## When to Use Each Approach

### CPU Cellular Automata
✅ Indie games, small-medium worlds
✅ Complex game rules tied to water
✅ Easier debugging
✅ <100k active water cells

### GPU Cellular Automata
✅ Large open worlds
✅ Simple, uniform water behavior
✅ >100k active water cells
✅ Team has graphics programming experience

### Smooth Particle Hydrodynamics
✅ High-fidelity simulation required
✅ Limited simulation volume
✅ Academic/research project
✅ Graphics showcase
❌ Most gameplay-focused voxel games

## References

- **Detailed Guide**: [voxel-physics-and-water-simulation-2025-12-28.md](./voxel-physics-and-water-simulation-2025-12-28.md)
- **Original CA Tutorial**: [W-Shadow](https://w-shadow.com/blog/2009/09/01/simple-fluid-simulation/)
- **DDA Paper**: [Amanatides & Woo (1987)](http://www.cse.yorku.ca/~amana/research/grid.pdf)
- **Chunk Optimization**: [Vercidium Blog](https://vercidium.com/blog/optimised-voxel-raymarching/)

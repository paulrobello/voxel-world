# Voxel Optimization Quick Reference

**Quick lookup guide for infinite voxel world techniques**

## Algorithm Comparison

| Technique | Memory Savings | Speed Impact | Complexity | Use Case |
|-----------|----------------|--------------|------------|----------|
| **Sparse Voxel Octree** | 10-50x | Fast | Medium | Empty space skipping |
| **DAG Compression** | 100-1000x | Very Fast | High | Repetitive structures |
| **Chunk Streaming** | Constant | None (async) | Medium | Infinite worlds |
| **LOD (Distance)** | 4-16x | 2-4x faster | Low | Far terrain |
| **Transvoxel** | None | Slight | High | LOD transitions |
| **POP Buffers** | None | Fast | Low | Implicit LOD |
| **Sparse Textures** | OS-managed | None | Low | If HW supported |

## Implementation Priority for Infinite Worlds

### Phase 1: Essential (Do First) ✅
```
1. Chunk System (32³ blocks)
   - HashMap<ChunkPos, Chunk>
   - Load/unload based on distance
   - Async generation on worker threads

2. Distance-based LOD
   - LOD = log2(distance / bias)
   - Store multiple resolutions in octree
   - Simple distance culling
```

### Phase 2: Performance (Next) ⚡
```
3. Sparse Voxel Octree
   - Hierarchical empty space skipping
   - GPU-friendly traversal
   - Contour information for detail

4. GPU Optimization
   - Frustum culling
   - Early ray termination
   - Shared memory in compute shader
```

### Phase 3: Advanced (Optional) 🚀
```
5. DAG Compression
   - Only if memory is bottleneck
   - Complex implementation
   - Massive savings for repetitive worlds

6. Advanced LOD
   - Transvoxel for seamless transitions
   - Geomorphing to prevent popping
   - Beam optimization for coherent rays
```

## Code Snippets

### Chunk Management
```cpp
// Core chunk system
struct ChunkPos { int x, y, z; };
HashMap<ChunkPos, Chunk*> loadedChunks;

void update(vec3 playerPos) {
    ChunkPos player = worldToChunk(playerPos);

    // Load nearby chunks
    for (int r = 0; r <= RENDER_DIST; r++) {
        for (each chunk at distance r) {
            if (!loaded) loadQueue.push(chunk, -r);
        }
    }

    // Unload distant chunks
    for (auto& [pos, chunk] : loadedChunks) {
        if (distance(pos, player) > UNLOAD_DIST) {
            saveAndUnload(chunk);
        }
    }
}
```

### LOD Selection
```cpp
// Distance-based LOD
int selectLOD(vec3 chunkCenter, vec3 camera) {
    float dist = length(chunkCenter - camera);
    return clamp(int(log2(dist / 32.0)), 0, MAX_LOD);
}
```

### SVO Traversal
```glsl
// GPU octree ray marching
bool traverseOctree(Ray ray) {
    uint node = 0;  // Root
    int level = 0;

    while (level < MAX_DEPTH) {
        uint octant = getOctant(ray.pos, level);

        if (!(nodes[node].mask & (1 << octant))) {
            // Empty - skip to next node
            ray.pos = nextBoundary(ray.pos, ray.dir);
            continue;
        }

        // Descend
        node = nodes[node].children[octant];
        level++;
    }
}
```

### Empty Space Skipping
```glsl
// Skip empty chunks
if (chunks[chunkID].isEmpty) {
    vec3 exit = rayExitPoint(ray, chunkBounds);
    ray.pos = exit + ray.dir * EPSILON;
    continue;
}
```

## Key Papers to Read

1. **"Efficient Sparse Voxel Octrees"** - Laine & Karras (2010)
   - Foundation for GPU voxel rendering
   - Must-read for any voxel engine

2. **"High Resolution Sparse Voxel DAGs"** - Kämpe et al. (2013)
   - Memory compression via deduplication
   - 100x+ savings demonstrated

3. **"Transvoxel Algorithm"** - Lengyel (2010)
   - Seamless LOD transitions
   - Industry-standard approach

## Common Mistakes to Avoid

❌ **Don't**: Upload entire world to GPU
✅ **Do**: Stream chunks as needed

❌ **Don't**: Render all chunks at full resolution
✅ **Do**: Use distance-based LOD

❌ **Don't**: Block main thread on chunk generation
✅ **Do**: Generate chunks asynchronously

❌ **Don't**: Store chunk positions as floating point
✅ **Do**: Use integer chunk coordinates

❌ **Don't**: Rebuild entire chunk for one block change
✅ **Do**: Mark dirty and batch updates

❌ **Don't**: Call noise functions redundantly in generation passes
✅ **Do**: Cache column data (height, biome) once and reuse

❌ **Don't**: Call expensive 3D functions unconditionally for all blocks
✅ **Do**: Early exit on block type checks, only compute when needed

## Terrain Generation Optimization

### Column Data Caching (30x Speedup)
```rust
// BEFORE: Redundant noise lookups (170-220ms per chunk)
fn generate_vegetation(chunk: &mut Chunk, terrain: &TerrainGenerator) {
    for lx in 0..32 {
        for lz in 0..32 {
            let height = terrain.get_height(x, z);  // Expensive!
            let biome = terrain.get_biome(x, z);    // Expensive!
            // ... place vegetation
        }
    }
}

// AFTER: Use pre-computed cache (7ms per chunk)
fn generate_vegetation(chunk: &mut Chunk, cache: &ColumnDataCache) {
    for lx in 0..32 {
        for lz in 0..32 {
            let col = cache.get_local(lx, lz);  // Free lookup!
            // ... place vegetation using col.height, col.biome
        }
    }
}
```

### Conditional 3D Biome Lookup
```rust
// BEFORE: 32,768 expensive biome lookups per chunk
for ly in 0..32 {
    let biome = terrain.get_biome_3d(x, y, z);  // Called for EVERY block!
    // ... maybe place decoration
}

// AFTER: ~100-500 lookups only when needed
for ly in 0..32 {
    let block = chunk.get_block(lx, ly, lz);
    if block != BlockType::Stone { continue; }  // Early exit!

    if chunk.get_block(lx, ly - 1, lz) == BlockType::Air {
        // Only NOW compute expensive biome
        let biome = terrain.get_biome_3d(x, y, z);
        // ... place ceiling decoration
    }
}
```

**Result**: Chunk generation reduced from 170-220ms to 7ms (30x faster)

## Performance Targets

### Render Distance Scaling
```
Distance   Chunks    Memory    Frame Time
--------   ------    ------    ----------
8 chunks   ~400      ~50MB     ~5ms
16 chunks  ~1600     ~200MB    ~10ms
32 chunks  ~6400     ~800MB    ~20ms
```

### LOD Impact
```
Technique          Memory    GPU Time
---------------    ------    --------
No LOD             100%      100%
2-level LOD        40%       50%
4-level LOD        25%       30%
8-level LOD        15%       20%
```

## Resources

- **Docs**: `/Users/probello/Repos/voxel_world/docs/research/infinite-voxel-world-optimization-2025-12-28.md`
- **GitHub**: [AdamYuan/SparseVoxelOctree](https://github.com/AdamYuan/SparseVoxelOctree)
- **Transvoxel**: [transvoxel.org](https://transvoxel.org/)
- **0fps Blog**: [LOD for Blocky Voxels](https://0fps.net/2018/03/03/a-level-of-detail-method-for-blocky-voxels/)

## Decision Tree

```
Need infinite world?
├─ Yes → Implement chunk system first
└─ No  → Current approach is fine

Memory constrained?
├─ Yes → Add LOD, consider DAG compression
└─ No  → LOD still recommended for performance

Rendering too slow?
├─ Yes → Check: LOD? Frustum culling? Empty space skip?
└─ No  → Focus on features, optimize later

Dynamic world (player modifies)?
├─ Yes → Simple array/SVO, avoid complex compression
└─ No  → DAG compression very effective
```

## Immediate Next Steps for Your Project

1. **Split world into chunks** (32³ each)
   - File: `src/chunk.rs` (already exists!)
   - Integrate with `src/world.rs`

2. **Add chunk loading/unloading**
   - Distance-based management
   - Async generation thread

3. **Update shader** to handle multiple chunks
   - Chunk ID in ray traversal
   - Empty chunk skipping

4. **Add simple LOD**
   - Generate 3-4 LOD levels per chunk
   - Distance-based selection

5. **Procedural generation**
   - Replace hardcoded world
   - Noise-based terrain

This gets you to infinite worlds with good performance!

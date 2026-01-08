# Cross-Chunk Terrain Generation Refactor

## Overview

This refactor enables terrain generation features (trees, caves, decorations) to span chunk boundaries, eliminating artificial constraints and creating more natural, sprawling structures.

## Core Infrastructure

### ✅ Completed

#### 1. Data Structures (src/terrain_gen.rs)
```rust
/// Represents a block that should be placed outside the current chunk
#[derive(Clone, Debug)]
pub struct OverflowBlock {
    pub world_pos: Vector3<i32>,
    pub block_type: BlockType,
}

/// Result of chunk terrain generation including overflow blocks
pub struct ChunkGenerationResult {
    pub chunk: Chunk,
    pub overflow_blocks: Vec<OverflowBlock>,
}
```

#### 2. Main Generation Functions
- ✅ `generate_chunk_terrain()` - Returns `ChunkGenerationResult` instead of `Chunk`
- ✅ `generate_normal_chunk()` - Collects overflow blocks, returns `ChunkGenerationResult`
- ✅ `generate_flat_chunk()` - Returns `ChunkGenerationResult` (no overflow for flat worlds)
- ✅ `generate_trees()` - Signature updated to accept `overflow_blocks` parameter
- ✅ `generate_ground_cover()` - Signature updated to accept `overflow_blocks` parameter
- ✅ `generate_cave_decorations()` - Signature updated to accept `overflow_blocks` parameter

#### 3. Helper Functions
- ✅ `set_block_safe()` - Updated to accept chunk world coordinates and overflow_blocks
  - Blocks within chunk bounds: placed directly
  - Blocks outside chunk bounds: added to overflow_blocks vector
- ✅ `set_painted_block_safe()` - Documented as not supporting overflow (silently skips out-of-bounds)
- ✅ `get_block_safe()` - Already supports safe bounds checking (no changes needed)

### ❌ Remaining Work

#### 1. Application Layer Integration

**File: `src/app/init.rs` (or wherever chunk generation is called)**

Current code expects:
```rust
move |pos| generate_chunk_terrain(&terrain, pos, world_gen)
```

Needs to be updated to:
```rust
move |pos| {
    let result = generate_chunk_terrain(&terrain, pos, world_gen);
    // Apply overflow blocks to neighboring chunks
    for overflow in result.overflow_blocks {
        apply_overflow_block(world, overflow);
    }
    result.chunk
}
```

**Required:**
- Find all callers of `generate_chunk_terrain()`
- Update to handle `ChunkGenerationResult`
- Implement overflow block application logic (world.set_block at world_pos)
- Determine when to apply overflow (immediately vs. during neighbor generation)

#### 2. Overflow Block Application Strategy

**Option A: Immediate Application**
- Apply overflow blocks to existing neighboring chunks immediately
- Pros: Simple, works with any chunk generation order
- Cons: Requires chunks to be mutable during generation, may cause race conditions

**Option B: Deferred Application**
- Store overflow blocks with the chunk
- Apply when neighboring chunks are generated
- Pros: Clean separation, works with parallel generation
- Cons: Need to track pending overflow blocks, more complex

**Recommended: Option B with Chunk Metadata**
```rust
pub struct Chunk {
    // ... existing fields
    pub pending_overflow: Vec<OverflowBlock>, // Blocks to apply when neighbors load
}
```

## Tree Generation

### ✅ Completed
- ✅ Removed trunk section >= 5 guard (was preventing branches on short sections)
- ✅ Increased branch counts and lengths
- ✅ Added vertical support columns from branch tips
- ✅ Added horizontal cross-bracing between aligned supports
- ✅ Ellipsoid/spherical canopy shapes instead of stacked discs
- ✅ 10% random gaps in foliage for natural variation

### ❌ Remaining Work

#### Function Signature Updates
All tree generation functions need to accept:
- `chunk_world_x: i32`
- `chunk_world_y: i32`
- `chunk_world_z: i32`
- `overflow_blocks: &mut Vec<OverflowBlock>`

**Functions requiring updates (~15 total):**

1. **Oak Trees**
   - `fn generate_oak()` - Dispatcher function
   - `fn generate_normal_oak()` - Small oak tree generation
   - `fn generate_giant_oak()` - Multi-deck oak with branches
   - `fn generate_oak_canopy()` - Ellipsoid canopy generation

2. **Pine Trees**
   - `fn generate_pine()` - Dispatcher function
   - `fn generate_normal_pine()` - Small pine tree generation
   - `fn generate_giant_pine()` - Multi-deck pine with branches
   - `fn generate_pine_cone()` - Conical canopy generation

3. **Willow Trees**
   - `fn generate_willow()` - Willow tree with drooping branches

4. **Cactus** (Desert biome)
   - `fn generate_cactus()` - Multi-armed cactus generation

#### set_block_safe() Call Updates (~20+ calls)

Every call to `set_block_safe()` needs to be updated from:
```rust
set_block_safe(chunk, x, y, z, BlockType::Log);
```

To:
```rust
set_block_safe(chunk, x, y, z, BlockType::Log,
               chunk_world_x, chunk_world_y, chunk_world_z, overflow_blocks);
```

**Locations:**
- Trunk placement loops in all tree types
- Branch placement loops (horizontal branches)
- Vertical support column loops
- Cross-bracing loops
- Canopy generation loops
- Cactus arm placement

#### Remove Boundary Guards

Current tree placement uses boundary guards:
```rust
for lx in (6..CHUNK_SIZE - 6).step_by(4) {
    for lz in (6..CHUNK_SIZE - 6).step_by(4) {
```

**Change to:**
```rust
for lx in (0..CHUNK_SIZE).step_by(4) {
    for lz in (0..CHUNK_SIZE).step_by(4) {
```

This allows trees to spawn at chunk edges and overflow into neighbors.

**Files to update:**
- `src/terrain_gen.rs` - Tree generation loops

## Ground Cover

### Current State
Ground cover (grass tufts, flowers, mushrooms, lily pads) is limited to single blocks and rarely needs cross-chunk support.

### ❌ Potential Enhancements

While not critical, ground cover could benefit from:

1. **Multi-block Features**
   - Large mushroom clusters that span multiple blocks
   - Dense grass/flower patches
   - Fallen logs (multi-block horizontal structures)

2. **Implementation**
   - Currently uses `set_model_block_safe()` for vegetation models
   - Would need similar overflow support if multi-block features are added

**Priority: Low** - Single block features work fine within chunk boundaries.

## Cave System

### Current State
Caves use noise-based carving during terrain generation. They already span chunks naturally because:
- Cave detection uses `terrain.cave_generator.is_cave(world_x, world_y, world_z)`
- World coordinates are used, not local chunk coordinates
- Each chunk independently queries the same noise field

### ✅ Already Working Across Chunks
Caves don't need refactoring because they're generated using continuous noise functions that are consistent across chunk boundaries.

### ❌ Potential Enhancements

#### 1. Cave Decorations (Stalactites/Stalagmites)

**Current:**
- Signature updated to accept overflow_blocks
- Still uses `set_block_safe()` with old signature

**Needed:**
- Update all `set_block_safe()` calls in `generate_cave_decorations()`
- Pass chunk world coordinates and overflow_blocks
- Enable stalactites/stalagmites to extend across chunk boundaries

**Locations in generate_cave_decorations():**
- Stalactite placement loops (hanging from ceiling)
- Stalagmite placement loops (growing from floor)
- Ice formations in frozen caves

#### 2. Cave Features That Could Benefit

**Natural Cave Pillars:**
- Thick support columns connecting floor to ceiling
- Currently limited to single chunk height
- Could span multiple vertical chunks with overflow

**Underground Lakes:**
- Already work across chunks (water placement uses world coordinates)
- No changes needed

**Lava Pools:**
- Already work across chunks
- No changes needed

**Ore Veins:**
- Not yet implemented
- When added, should use overflow for cross-chunk veins

## Biome-Specific Features

### Desert (Cactus)
- ❌ `generate_cactus()` needs signature updates
- ❌ All `set_painted_block_safe()` calls need updates
- Note: Painted blocks don't currently support overflow

### Swamp (Willow Trees, Lily Pads)
- ❌ `generate_willow()` needs signature updates
- ❌ set_block_safe() calls need updates
- ✅ Lily pads are single blocks, work fine in current system

### Mountains (Pine Trees, Ice)
- ❌ Pine tree functions need signature updates
- ❌ set_block_safe() calls need updates

### Grassland (Oak Trees, Flowers)
- ❌ Oak tree functions need signature updates (most complex due to giant oaks)
- ❌ set_block_safe() calls need updates
- ✅ Flowers are single blocks, work fine

### Snow (Snow-covered variants)
- Uses existing tree types with snow blocks
- Same updates as base tree types

## Migration Strategy

### Phase 1: Complete Core Infrastructure ✅ DONE
- ✅ Add OverflowBlock and ChunkGenerationResult structures
- ✅ Update main generation functions to return results
- ✅ Update set_block_safe() to support overflow

### Phase 2: Application Layer Integration ✅ COMPLETED (Partial)
1. ✅ Found all `generate_chunk_terrain()` callers (3 locations)
2. ✅ Updated `src/world_init/generation.rs` (2 locations) to handle `ChunkGenerationResult`
3. ✅ Implemented overflow block application in synchronous generation paths
4. ⚠️ **Limitation**: `src/app/init.rs` ChunkLoader currently discards overflow blocks
   - ChunkLoader uses async threading and is complex to update
   - Overflow blocks work in initial world generation but not dynamic chunk loading
   - **TODO**: Update ChunkLoader to support overflow in future enhancement

### Phase 3: Tree Generation (High Priority) ❌ TODO
1. Update all tree function signatures (systematic, ~15 functions)
2. Update all set_block_safe() calls (~20+ calls)
3. Remove boundary guards from tree placement loops
4. Test trees spanning chunk boundaries

### Phase 4: Cave Decorations (Medium Priority) ❌ TODO
1. Update set_block_safe() calls in generate_cave_decorations()
2. Test stalactites/stalagmites spanning chunks

### Phase 5: Future Enhancements (Low Priority) ❌ TODO
1. Multi-block ground cover features
2. Cross-vertical-chunk cave pillars
3. Ore vein systems

## Testing Checklist

### Basic Functionality
- [ ] Chunks still generate without overflow (flat world)
- [ ] Normal world generates with overflow blocks
- [ ] Overflow blocks are collected correctly
- [ ] World coordinates in overflow blocks are correct

### Tree Generation
- [ ] Trees spawn at chunk edges
- [ ] Tree trunks span chunk boundaries
- [ ] Branches extend into neighboring chunks
- [ ] Vertical supports cross chunk boundaries
- [ ] Cross-bracing connects across boundaries
- [ ] Canopy foliage spans multiple chunks
- [ ] No duplicate blocks at boundaries
- [ ] No missing blocks at boundaries

### Cave Decorations
- [ ] Stalactites extend across boundaries
- [ ] Stalagmites grow across boundaries
- [ ] No floating disconnected formations

### Performance
- [ ] Overflow block count is reasonable (< 1000 per chunk)
- [ ] Chunk generation time doesn't significantly increase
- [ ] Memory usage is acceptable

### Edge Cases
- [ ] Trees at world boundaries (no overflow outside world)
- [ ] Very tall trees crossing multiple vertical chunks
- [ ] Dense forests with overlapping overflow regions
- [ ] Chunk regeneration doesn't duplicate overflow structures

## Known Limitations

### Painted Blocks
Painted blocks (cactus, sandstone, mud) don't support overflow yet. They are silently skipped when out of bounds. This is acceptable because:
- Cacti are relatively small (3-6 blocks tall)
- Most painted blocks are terrain/ground features
- Can be added later if needed

### Model Blocks
Vegetation models (grass tufts, flowers) don't have overflow support. Not needed because:
- All are single-block placements
- Multi-block model structures would need new system

### Chunk Loading Order
Overflow blocks must be applied carefully based on chunk loading order:
- If neighbor exists: apply immediately
- If neighbor doesn't exist: store for later application
- Need to prevent duplicates if chunk is unloaded/reloaded

## Implementation Notes

### Thread Safety
If chunks are generated in parallel:
- Overflow blocks should be stored with source chunk
- Application should happen in controlled phase
- Consider using message passing for cross-chunk updates

### Persistence
Overflow blocks should not be saved separately:
- Final block placement is what persists
- Overflow is only relevant during initial generation
- Regenerated chunks should produce same overflow

### Debugging
Add debug logging for:
- Number of overflow blocks generated per chunk
- Overflow block world positions
- Application of overflow to neighbors
- Any skipped overflow (out of world bounds)

## Code Snippets

### Applying Overflow Blocks (Pseudocode)
```rust
fn apply_overflow_blocks(world: &mut World, overflow_blocks: Vec<OverflowBlock>) {
    for overflow in overflow_blocks {
        let chunk_pos = world.world_to_chunk(overflow.world_pos);

        // Check if target chunk exists
        if let Some(chunk) = world.get_chunk_mut(chunk_pos) {
            let local = world.world_to_local(overflow.world_pos);

            // Only place if target block is air or transparent
            if chunk.get_block(local.x, local.y, local.z).is_air() ||
               chunk.get_block(local.x, local.y, local.z).is_transparent() {
                chunk.set_block(local.x, local.y, local.z, overflow.block_type);
            }
        } else {
            // Store overflow for when neighbor loads
            world.pending_overflow.entry(chunk_pos)
                .or_insert_with(Vec::new)
                .push(overflow);
        }
    }
}
```

### Example Tree Function Update
```rust
// BEFORE
fn generate_giant_oak(chunk: &mut Chunk, x: i32, y: i32, z: i32, hash: i32) {
    for dy in 1..height {
        set_block_safe(chunk, x, y + dy, z, BlockType::Log);
    }
}

// AFTER
fn generate_giant_oak(
    chunk: &mut Chunk,
    x: i32,
    y: i32,
    z: i32,
    hash: i32,
    chunk_world_x: i32,
    chunk_world_y: i32,
    chunk_world_z: i32,
    overflow_blocks: &mut Vec<OverflowBlock>,
) {
    for dy in 1..height {
        set_block_safe(
            chunk,
            x,
            y + dy,
            z,
            BlockType::Log,
            chunk_world_x,
            chunk_world_y,
            chunk_world_z,
            overflow_blocks,
        );
    }
}
```

## Benefits

### Immediate Benefits
- **Natural tree placement**: Trees can spawn at chunk edges without being cut off
- **Larger structures**: Giant oaks with 10-block branches can sprawl freely
- **Better aesthetics**: No visible chunk boundaries in forests
- **Rainforest potential**: Dense multi-chunk canopies become possible

### Future Benefits
- **Underground structures**: Dungeons, mines, caverns can span chunks
- **Villages**: Buildings can cross chunk boundaries naturally
- **Roads/Paths**: Can be continuous across chunks
- **Water features**: Rivers, lakes can have features on both sides of boundaries

## Related Files

- `src/terrain_gen.rs` - Main terrain generation (needs most updates)
- `src/app/init.rs` - Chunk generation caller (needs result handling)
- `src/world.rs` - World management (may need overflow application)
- `src/chunk.rs` - Chunk structure (may need pending_overflow field)

## Estimated Effort

- **Phase 2** (Application Layer): 2-3 hours (complex, need to understand world management)
- **Phase 3** (Tree Generation): 3-4 hours (mechanical but extensive)
- **Phase 4** (Cave Decorations): 1 hour (small scope)
- **Testing**: 2-3 hours (thorough testing of edge cases)

**Total: ~10 hours of development work**

## Questions to Resolve

1. **Overflow Application Timing**: When should overflow blocks be applied?
   - During chunk generation?
   - After all chunks in view are generated?
   - On-demand when neighbor loads?

2. **Duplicate Prevention**: How to prevent the same overflow block being applied multiple times?
   - Track applied overflow blocks?
   - Check target block before applying?

3. **Persistence**: Should pending overflow be saved to disk?
   - Probably no - regenerate on chunk load
   - But need consistent generation for same seed

4. **World Boundaries**: What happens to overflow beyond world edges?
   - Simply discard?
   - Clamp to world boundary?

5. **Vertical Chunks**: Do we need special handling for very tall trees crossing Y chunk boundaries?
   - Currently world is 16x4x16 chunks = 512x128x512 blocks
   - Max tree height ~30 blocks, so usually fits in 1 vertical chunk
   - May need consideration for future height increases

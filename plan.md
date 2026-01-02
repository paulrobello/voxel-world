# Voxel Engine Enhancement Plan

## Overview

Transform the current fixed-size voxel engine into a **sandboxed building game** with sub-voxel models, physics, persistence, multiplayer, and scriptable AI. This is NOT a survival/crafting game—it's focused on creative building with rich object variety.

### Vision Statement
- **Builder-first**: No crafting, no enemies—pure creative expression
- **Sub-voxel detail**: 8³ voxels per block for furniture, fences, decorations
- **In-game editor**: Create custom models without external tools
- **Living worlds**: Animals and critters with scriptable AI
- **Multiplayer**: Dedicated server for collaborative building
- **Performance**: Maintain 90+ FPS with all features enabled

---

## Phase 1: Infinite Chunk Streaming ✅ COMPLETE

### Goal
Remove world size limitations and enable procedural generation of unlimited terrain.

### Completed Tasks
- [x] Remove world bounds constraints
- [x] Sliding window texture system with auto-shifting origin
- [x] HashMap-based unlimited chunk storage
- [x] Chunk loading priority queue (distance-based)
- [x] Async 4-thread terrain generation
- [x] Shader updates for unbounded world coordinates

---

## Phase 2: Block Physics System ✅ COMPLETE

### Goal
Implement gravity-affected blocks and structural integrity for tree chopping.

### Completed Tasks
- [x] FallingBlock entity system with gravity physics
- [x] Block support detection (sand, gravel fall when unsupported)
- [x] Tree chopping with flood-fill support detection
- [x] Frame-distributed BlockUpdateQueue (configurable 16-128 updates/frame)
- [x] Priority queue processes nearby blocks first

---

## Phase 3: World Persistence

### Goal
Save and load worlds using a region-based file format similar to Minecraft's Anvil format.

### Why Region-Based?
- **Efficient I/O**: Only load/save chunks that changed
- **Scalability**: Unlimited world size without single-file bloat
- **Concurrent access**: Multiple readers, single writer per region
- **Network-friendly**: Easy to sync deltas between server/client

### Implementation Tasks

#### 3.1 Region File Format
- [ ] Define region size: 32×32 chunks (1024×128×1024 blocks per region file)
- [ ] Design file header: version, chunk offset table, compression flags
- [ ] Implement chunk serialization: block data + metadata + sub-voxel models
- [ ] Use zstd compression for chunk data (fast decompress, good ratio)
- [ ] Store region files as `world/r.{rx}.{rz}.vxr`

#### 3.2 Chunk Serialization
- [ ] Serialize BlockType array (32³ = 32KB uncompressed)
- [ ] Serialize sub-voxel model references (Phase 5 integration)
- [ ] Serialize entity data within chunk bounds
- [ ] Add chunk-level metadata (biome, generation flags, timestamps)
- [ ] Implement dirty chunk tracking for incremental saves

#### 3.3 World Save/Load System
- [ ] Create `WorldSave` struct managing region files
- [ ] Implement lazy region loading (load on first chunk access)
- [ ] Auto-save dirty regions every N seconds (configurable, default 30s)
- [ ] Add save-on-exit with progress indicator
- [ ] Implement world metadata file (seed, spawn point, game settings)

#### 3.4 Migration and Versioning
- [ ] Version header in all save files
- [ ] Forward-compatible chunk format (unknown fields ignored)
- [ ] Migration path for format upgrades
- [ ] Backup creation before migration

### Technical Approach

**Region File Structure:**
```
Header (512 bytes):
  - Magic: "VXRG" (4 bytes)
  - Version: u16
  - Chunk count: u16
  - Offset table: [1024 × (offset: u32, size: u32)]

Chunk Data (variable):
  - Compressed block data
  - Sub-voxel model references
  - Entity snapshots
```

**Performance Considerations:**
- Memory-mapped I/O for large regions
- Background thread for save operations
- Chunk dirty flags to minimize writes
- LRU cache for recently accessed regions

---

## Phase 4: Sub-Voxel Model System

### Goal
Support detailed 8³ voxel models within standard block space for furniture, fences, decorations.

### Why 8³?
- **512 sub-voxels per block**: Rich detail without excessive memory
- **GPU-friendly**: Fits in 512 bytes (1 byte per sub-voxel)
- **Ray march compatible**: Can extend existing DDA algorithm
- **Artist-friendly**: Intuitive 8×8×8 canvas

### Implementation Tasks

#### 4.1 Sub-Voxel Data Structure
- [x] Define `SubVoxelModel` struct: 8³ voxel grid + 16-color palette (1 byte indices)
- [x] Palette-based coloring: per-model RGBA palette packing
- [x] Model registry: HashMap<ModelId, SubVoxelModel> with name lookups
- [x] Built-in models: torch, slabs, fences/gates (connection variants), stairs, ladder (chair/table/picture_frame TBD)
- [x] Model LOD: full detail within LOD radius, culled beyond

#### 4.2 Block Metadata System
- [x] Extend block storage: BlockType::Model + optional metadata
- [x] Metadata types: `model_id` + `rotation` (custom data deferred)
- [x] Efficient storage: sparse per-chunk map + cached RG8 buffer
- [x] GPU upload strategy: separate model metadata texture + atlas packing

#### 4.3 Shader Integration
- [x] Sub-voxel ray marching for hit tests and shading
- [x] Model voxel + palette atlases uploaded to GPU
- [x] LOD switching based on distance for render and shadows
- [ ] Ambient occlusion for sub-voxel geometry (still block-level AO)
- [x] Shadow casting from sub-voxel shapes (fine marcher + mask fallback)

#### 4.4 Collision Detection
- [x] Sub-voxel collision masks/AABB for player collision
- [x] Per-model collision masks (non-solid voxels supported)
- [ ] Physics integration: falling sub-voxel objects
- [x] Raycasting through sub-voxel geometry

### Technical Approach

**Memory Layout:**
```rust
struct SubVoxelModel {
    voxels: [u8; 512],     // 8³ palette indices (0 = air)
    palette: [Color; 16],  // 16-color RGBA palette
    collision_mask: u64,   // 64-bit simplified collision
    flags: ModelFlags,     // Transparent, emissive, etc.
}

struct BlockMetadata {
    rotation: u8,   // 0-3 rotation states (Y axis)
    model_id: u8,   // Reference to model registry
    custom: u8,     // Reserved for future block-specific data
}
```

**GPU Data:**
- 3D texture for sub-voxel models (8×8×8 per slice)
- Model metadata texture (RG8) stores model_id + rotation per block
- Palette texture: model_id → 16 colors

---

## Phase 5: In-Game Model Editor

### Goal
Create and edit sub-voxel models without leaving the game.

### Features
- **Voxel canvas**: 8×8×8 editable grid
- **Tools**: Draw, erase, fill, copy, mirror
- **Palette editor**: Pick colors, import from image
- **Preview**: See model in-world before saving
- **Library**: Browse and search saved models

### Implementation Tasks

#### 5.1 Editor UI Framework
- [ ] Modal editor overlay (Esc to open/close)
- [ ] 3D voxel canvas with rotation controls
- [ ] Tool palette: pencil, eraser, bucket fill, eyedropper
- [ ] Color palette with HSV picker
- [ ] Undo/redo stack (50 states)

#### 5.2 Editing Tools
- [ ] Single voxel place/remove
- [ ] Line drawing (shift-click)
- [ ] Box fill (drag selection)
- [ ] Mirror mode (X, Y, Z axis)
- [ ] Copy/paste regions
- [ ] Import PNG as palette/texture

#### 5.3 Model Management
- [ ] Save model to library (name, tags, thumbnail)
- [ ] Load model from library
- [ ] Export/import model files (.vxm format)
- [ ] Share models in multiplayer (sync to server)

#### 5.4 In-World Placement
- [ ] Ghost preview when placing custom model
- [ ] Rotation controls (R key cycles orientations)
- [ ] Snap-to-grid for precise placement
- [ ] Replace existing model blocks

---

## Phase 6: Entity System

### Goal
Unified entity framework for animals, items, and physics objects.

### Why Dedicated Entity System?
- **Decoupled from blocks**: Entities move freely, not grid-locked
- **Component-based**: Flexible composition of behaviors
- **Network-ready**: Entity state syncs naturally
- **AI-compatible**: Entities can have behaviors/scripts

### Implementation Tasks

#### 6.1 Entity Core
- [ ] Entity struct: position, velocity, rotation, AABB
- [ ] EntityId: u64 unique identifier
- [ ] EntityManager: spawn, despawn, query by type/region
- [ ] Chunk association: entities belong to chunks for streaming

#### 6.2 Entity Types
- [ ] ItemEntity: dropped items with physics
- [ ] AnimalEntity: critters with AI (Phase 8)
- [ ] ModelEntity: placed sub-voxel objects with physics
- [ ] FallingBlockEntity: refactor existing falling blocks

#### 6.3 Physics Integration
- [ ] Gravity and collision for all entities
- [ ] Entity-entity collision detection
- [ ] Entity-block collision (including sub-voxels)
- [ ] Sleeping/waking for performance

#### 6.4 Rendering
- [ ] Entity rendering pass (after blocks, before UI)
- [ ] Billboard sprites for simple entities
- [ ] Sub-voxel model rendering for complex entities
- [ ] Shadow casting from entities

### Technical Approach

**Component Structure:**
```rust
struct Entity {
    id: EntityId,
    position: Vector3<f32>,
    velocity: Vector3<f32>,
    rotation: Quaternion<f32>,
    aabb: AABB,
    entity_type: EntityType,
    components: ComponentSet,
}

enum EntityType {
    Item { item_id: u16, count: u8 },
    Animal { species: AnimalSpecies, ai_state: AiState },
    Model { model_id: u16, physics: bool },
    FallingBlock { block_type: BlockType },
}
```

---

## Phase 7: Multiplayer Networking

### Goal
Dedicated server architecture for collaborative building.

### Why Dedicated Server?
- **Authority**: Server is source of truth, prevents cheating
- **Scalability**: Server handles heavy lifting, clients stay light
- **Persistence**: Server manages world saves
- **Flexibility**: Clients can be web, mobile, desktop

### Implementation Tasks

#### 7.1 Network Protocol
- [ ] Define message types: connect, disconnect, chunk_data, block_change, entity_update
- [ ] Binary serialization with versioning (bincode or custom)
- [ ] UDP for position updates, TCP for reliable events
- [ ] Message compression for chunk data

#### 7.2 Server Architecture
- [ ] Standalone server binary (`voxel-server`)
- [ ] Accept client connections (tokio async runtime)
- [ ] Tick-based game loop (20 ticks/second)
- [ ] Broadcast entity/block updates to nearby clients
- [ ] Region-based interest management

#### 7.3 Client Integration
- [ ] Connect/disconnect flow with authentication
- [ ] Request chunks from server (not local generation)
- [ ] Send input/actions to server
- [ ] Interpolate remote player positions
- [ ] Handle packet loss gracefully

#### 7.4 Synchronization
- [ ] Server authoritative block changes
- [ ] Client-side prediction for local player
- [ ] Rollback on server correction
- [ ] Entity state interpolation
- [ ] Chunk streaming priority by player positions

#### 7.5 Security
- [ ] Rate limiting for actions
- [ ] Validation of all client requests
- [ ] Anti-speedhack checks
- [ ] Permission system (build zones, admin commands)

### Technical Approach

**Message Types:**
```rust
enum ServerMessage {
    Welcome { player_id: u64, spawn: Vector3<f32> },
    ChunkData { pos: ChunkPos, data: CompressedChunk },
    BlockChange { pos: BlockPos, block: BlockType },
    EntitySpawn { entity: EntitySnapshot },
    EntityUpdate { id: EntityId, pos: Vector3<f32>, vel: Vector3<f32> },
    EntityDespawn { id: EntityId },
    PlayerJoin { id: u64, name: String },
    PlayerLeave { id: u64 },
}

enum ClientMessage {
    RequestChunk { pos: ChunkPos },
    PlaceBlock { pos: BlockPos, block: BlockType },
    BreakBlock { pos: BlockPos },
    PlayerMove { pos: Vector3<f32>, look: Vector2<f32> },
    ChatMessage { text: String },
}
```

**Network Architecture:**
```
┌─────────────┐     TCP/UDP      ┌─────────────┐
│   Client    │◄────────────────►│   Server    │
│  (Vulkan)   │                  │  (Headless) │
└─────────────┘                  └─────────────┘
      │                                │
      │ Local rendering                │ World state
      │ Input capture                  │ Physics tick
      │ Prediction                     │ AI tick
      │                                │ Persistence
```

---

## Phase 8: AI and Scripting

### Goal
Scriptable animal behaviors via TypeScript/Python SDK compiled to WASM.

### Why WASM?
- **Sandboxed**: Scripts can't crash the game or access system
- **Fast**: Near-native performance after JIT
- **Portable**: Same scripts work on server and client
- **Familiar**: Developers use TypeScript/Python, not Rust

### Implementation Tasks

#### 8.1 WASM Runtime Integration
- [ ] Integrate wasmtime or wasmer runtime
- [ ] Define host functions: get_block, set_entity_velocity, pathfind, etc.
- [ ] Memory limits per script (prevent DoS)
- [ ] Script loading from world save or server

#### 8.2 TypeScript SDK
- [ ] Create `@voxel/sdk` npm package
- [ ] Type definitions for all host functions
- [ ] Entity behavior base class
- [ ] Compile to WASM via AssemblyScript
- [ ] Hot reload scripts during development

#### 8.3 Python SDK (Alternative)
- [ ] Create `voxel-sdk` PyPI package
- [ ] Compile to WASM via py2wasm or RustPython
- [ ] Same API surface as TypeScript SDK
- [ ] Jupyter notebook integration for testing

#### 8.4 Animal AI Framework
- [ ] Behavior tree primitives: sequence, selector, parallel
- [ ] Built-in behaviors: wander, flee, follow, idle
- [ ] Pathfinding API (A* through blocks and entities)
- [ ] Perception: sight range, hearing, smell
- [ ] Needs system: hunger, rest, social

#### 8.5 Script Examples
- [ ] Chicken: wander, peck ground, flee from player
- [ ] Dog: follow player, sit on command
- [ ] Bird: fly between trees, sing at dawn
- [ ] Fish: swim in schools, avoid shore

### Technical Approach

**TypeScript SDK Example:**
```typescript
import { Entity, Vector3, World } from '@voxel/sdk';

export class Chicken extends Entity {
  private wanderTarget: Vector3 | null = null;
  private fleeFrom: Entity | null = null;

  onTick(world: World, dt: number) {
    // Check for threats
    const nearbyPlayers = world.getEntitiesInRadius(this.position, 8, 'player');
    if (nearbyPlayers.length > 0) {
      this.fleeFrom = nearbyPlayers[0];
      this.wanderTarget = null;
    }

    if (this.fleeFrom) {
      // Run away!
      const dir = this.position.sub(this.fleeFrom.position).normalize();
      this.velocity = dir.mul(4); // 4 blocks/sec flee speed

      if (this.position.distanceTo(this.fleeFrom.position) > 16) {
        this.fleeFrom = null; // Safe now
      }
    } else {
      // Peaceful wandering
      if (!this.wanderTarget || this.position.distanceTo(this.wanderTarget) < 0.5) {
        this.wanderTarget = this.position.add(Vector3.random().mul(8));
      }
      const dir = this.wanderTarget.sub(this.position).normalize();
      this.velocity = dir.mul(1.5); // Slow wander
    }
  }
}
```

**Host Function Bindings:**
```rust
// Rust side - exposed to WASM
#[wasm_bindable]
fn get_block(x: i32, y: i32, z: i32) -> u8;

#[wasm_bindable]
fn set_velocity(entity_id: u64, vx: f32, vy: f32, vz: f32);

#[wasm_bindable]
fn pathfind(from_x: f32, from_y: f32, from_z: f32,
            to_x: f32, to_y: f32, to_z: f32) -> PathResult;

#[wasm_bindable]
fn get_entities_in_radius(x: f32, y: f32, z: f32, radius: f32,
                          type_filter: &str) -> Vec<EntityId>;
```

---

## Phase 9: Water Flow Simulation ✅ COMPLETE

### Goal
Implement cellular automata water that flows, fills basins, and responds to terrain changes.

### Completed Tasks
- [x] Mass-based water system with sparse HashMap storage
- [x] W-Shadow cellular automata algorithm (down > horizontal > up priority)
- [x] Double-buffer pending changes system
- [x] Active cell tracking with stability detection
- [x] Frame-distributed updates (64 cells/frame default)
- [x] Simulation radius limiting (64 blocks from player)
- [x] Source blocks for infinite water (player-placed or terrain)
- [x] Boundary handling: drains at y<0, blocks at unloaded chunks
- [x] Integration with block placement/removal events
- [x] Terrain water activation when adjacent blocks broken

### Deferred Tasks (Future Phases)
- [ ] Update shader for variable water heights (visual enhancement)
- [ ] Interaction with sub-voxel models (requires Phase 4)
- [ ] Network sync for multiplayer (requires Phase 7)
- [ ] Save/load water state in regions (requires Phase 3)

### Technical Details
- Storage: `WaterGrid` with `HashMap<Vector3<i32>, WaterCell>`
- Cell properties: mass (0.0-1.0+), is_source, stable_ticks
- Flow constants: `MIN_MASS=0.001`, `FLOW_DAMPING=0.5`
- Spread distance: ~7-10 blocks before water thins and evaporates
- See `src/water.rs` for full implementation

---

## Phase 10: Performance Optimization

### Goal
Maintain 90+ FPS with all features enabled.

### Implementation Tasks

#### 10.1 Sub-Voxel LOD
- [ ] Full 8³ detail within 32 blocks
- [ ] Simplified 4³ or billboard 32-64 blocks
- [ ] Block-level only beyond 64 blocks
- [ ] Smooth LOD transitions

#### 10.2 Entity Culling
- [ ] Frustum culling for entities
- [ ] Distance-based update rates
- [ ] Sleep distant entities (no physics tick)
- [ ] Entity pooling to reduce allocations

#### 10.3 Network Optimization
- [ ] Delta compression for chunk updates
- [ ] Entity state interpolation (reduce update rate)
- [ ] Priority queue for network messages
- [ ] Bandwidth throttling per client

#### 10.4 GPU Optimization
- [ ] Texture streaming for sub-voxel models
- [ ] Compute shader for water simulation
- [ ] Indirect draw for entity batching
- [ ] Async compute for physics

---

## Implementation Order

### Foundation (Do First)
1. **Phase 3**: World Persistence - Required for testing everything else
2. **Phase 4**: Sub-Voxel Models - Core feature, affects rendering architecture

### Core Features (Build On Foundation)
3. **Phase 5**: In-Game Editor - Makes sub-voxel models usable
4. **Phase 6**: Entity System - Required for animals and physics objects
5. **Phase 9**: Water Flow ✅ - Core simulation complete, sub-voxel interaction deferred

### Multiplayer (After Single-Player Works)
6. **Phase 7**: Networking - Build on solid single-player foundation

### Polish (Final Phase)
7. **Phase 8**: AI/Scripting - Animals need stable entity system
8. **Phase 10**: Optimization - Profile with full feature set

---

## Success Criteria

### Completed
- [x] World generates infinitely in all horizontal directions
- [x] Steady 90+ FPS while moving through world
- [x] Trees fall when trunk is broken
- [x] Frame-distributed physics prevents lag spikes
- [x] Water flows naturally using W-Shadow cellular automata
- [x] Water spreads ~7-10 blocks from source before evaporating

### Phase 3: Persistence
- [ ] World saves to region files on exit
- [ ] World loads from region files on start
- [ ] Incremental saves don't cause stutter

### Phase 4: Sub-Voxels
- [ ] Place and break 8³ models (torch, fence, chair)
- [ ] Sub-voxel collision works correctly
- [ ] No FPS drop with 100+ sub-voxel models visible

### Phase 5: Editor
- [ ] Create custom model in under 2 minutes
- [ ] Save model to library
- [ ] Place custom model in world

### Phase 6: Entities
- [ ] Animals spawn and move in world
- [ ] Dropped items have physics
- [ ] Entity persistence across save/load

### Phase 7: Multiplayer
- [ ] Two players in same world simultaneously
- [ ] Block changes sync within 100ms
- [ ] Player positions interpolate smoothly

### Phase 8: Scripting
- [ ] Chicken AI runs from TypeScript
- [ ] Hot reload script without restart
- [ ] Scripts can't crash game

### Phase 9: Water ✅ COMPLETE
- [x] Water flows naturally and fills basins
- [ ] Water flows around sub-voxel models (deferred to Phase 4)
- [ ] Water state persists and syncs (deferred to Phases 3/7)

### Phase 10: Performance
- [ ] 90+ FPS with 1000 entities
- [ ] 60+ FPS in multiplayer with 4 players
- [ ] Sub-100ms world load time

---

## Current Work (2026-01-01)
- Sub-voxel models (ladders/fences/gates) rendering & shadows:
  - Fence self-shadow artifacts resolved; connected fence shadows align without gaps.
  - Remaining issues: missing top faces on tall posts; model normals still unstable in some views.
  - Shader changes: shadow ray partial fallback (mask/AABB), sub-voxel normal calc and inverse rotation.
- UX: default target block outline set **off** (toggled in UI).

## In Progress
- Stabilize sub-voxel face rendering (missing top faces) and normals for models while keeping correct shadows.

## Done Recently
- Forced shadow rays to skip the originating model voxel to eliminate fence self-shadow artifacts.
- Restored model shadows without block-sized fallback.
- Fixed single-post shadow self-intersection.
- Added partial-model collision-mask shadow fallback.
- Defaulted target outline off.

---

## Research References

- `docs/research/infinite-voxel-world-optimization-2025-12-28.md`
- `docs/research/voxel-physics-and-water-simulation-2025-12-28.md`
- Amanatides & Woo DDA Algorithm (1987)
- W-Shadow Cellular Automata Water
- NVIDIA Sparse Voxel Octrees (Laine & Karras, 2010)
- AssemblyScript for TypeScript→WASM compilation
- Minecraft Anvil region file format
- ECS architecture (specs, legion, bevy_ecs)

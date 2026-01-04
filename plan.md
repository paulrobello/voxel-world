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

## Phase 3: World Persistence ✅ COMPLETE

### Goal
Save and load worlds using a region-based file format similar to Minecraft's Anvil format.

### Why Region-Based?
- **Efficient I/O**: Only load/save chunks that changed
- **Scalability**: Unlimited world size without single-file bloat
- **Concurrent access**: Multiple readers, single writer per region
- **Network-friendly**: Easy to sync deltas between server/client

### Implementation Tasks

#### 3.1 Region File Format
- [x] Define region size: 32×32 chunks (1024×128×1024 blocks per region file)
- [x] Design file header: version, chunk offset table, compression flags
- [x] Implement chunk serialization: block data + metadata + sub-voxel models
- [x] Use zstd compression for chunk data (fast decompress, good ratio)
- [x] Store region files as `world/r.{rx}.{rz}.vxr`

#### 3.2 Chunk Serialization
- [x] Serialize BlockType array (32³ = 32KB uncompressed)
- [x] Serialize sub-voxel model references (Phase 4 integration)
- [x] Serialize entity data within chunk bounds
- [x] Add chunk-level metadata (biome, generation flags, timestamps)
- [x] Implement dirty chunk tracking for incremental saves

#### 3.3 World Save/Load System
- [x] Create `WorldSave` struct managing region files
- [x] Implement lazy region loading (load on first chunk access)
- [x] Auto-save dirty regions every N seconds (configurable, default 30s)
- [x] Add save-on-exit with progress indicator
- [x] Implement world metadata file (seed, spawn point, game settings)

#### 3.4 Migration and Versioning
- [x] Version header in all save files
- [x] Forward-compatible chunk format (unknown fields ignored)
- [x] Migration path for format upgrades
- [x] Backup creation before migration

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

## Phase 4: Sub-Voxel Model System ✅ COMPLETE

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
- [x] Translucency support: palette alpha < 1.0 renders as translucent with colored shadows

#### 4.2 Block Metadata System
- [x] Extend block storage: BlockType::Model + optional metadata
- [x] Metadata types: `model_id` + `rotation` (custom data deferred)
- [x] Efficient storage: sparse per-chunk map + cached RG8 buffer
- [x] GPU upload strategy: separate model metadata texture + atlas packing

#### 4.3 Shader Integration
 - [x] Sub-voxel ray marching for hit tests and shading
 - [x] Model voxel + palette atlases uploaded to GPU
 - [x] LOD switching based on distance for render and shadows
 - [x] Shadow casting from sub-voxel shapes (fine marcher + mask fallback)
 - [x] Shadow receiving tuned for slabs/models (fine march + offset, no skip)

#### 4.4 Collision Detection
 - [x] Sub-voxel collision masks/AABB for player collision
 - [x] Per-model collision masks (non-solid voxels supported)
 - [x] Raycasting through sub-voxel geometry

#### 4.5 Stairs Auto-Joining (Minecraft-style)
- [x] Add stair block metadata for shape/orientation (straight/inner/outer corners)
- [x] Neighbor-aware stair model selection to auto-form corners
- [x] Update sub-voxel meshes for stair variants
- [x] Ensure collision shapes match visual stair variants

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

## Phase 5: In-Game Model Editor ✅ COMPLETE

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
- [x] Modal editor overlay (N to open, Esc to close)
- [x] 3D voxel canvas with orbit rotation controls
- [x] Tool palette: pencil, eraser, bucket fill, eyedropper
- [x] Color palette with RGBA picker
- [x] Undo/redo stack (50 states)

#### 5.2 Editing Tools
- [x] Single voxel place/remove
- [x] Rotate model 90° around Y axis
- [x] Mirror mode (X, Y, Z axis) with visual plane indicators
- [ ] Line drawing (shift-click) - *deferred*
- [ ] Box fill (drag selection) - *deferred*

#### 5.3 Model Management
- [x] Save model to library (name, author, thumbnail)
- [x] Load model from library
- [x] Export/import model files (.vxm format)
- [x] Overwrite confirmation for existing models
- [x] Runtime sprite generation for palette icons
- [x] GPU buffer refresh when models updated
- [ ] Share models in multiplayer (sync to server) - *requires Phase 7*

#### 5.4 In-World Placement
- [x] Custom models appear in E-key palette
- [x] Auto-rotation to face player when placing
- [x] Right-click to rotate placed custom models
- [x] Snap-to-grid placement (standard block grid)

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
- [ ] Sync sub-voxel metadata (model_id, rotation) in block_change events
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
- [x] Implement "Waterlogging" mechanics (water flows OUT of models but doesn't wash them away)
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
1. **Phase 3**: World Persistence ✅ - Required for testing everything else
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
- [x] World saves to region files on exit
- [x] World loads from region files on start
- [x] Incremental saves don't cause stutter

### Phase 4: Sub-Voxels ✅ COMPLETE
- [x] Place and break 8³ models (torch, fence, chair)
- [x] Sub-voxel collision works correctly
- [x] No FPS drop with 100+ sub-voxel models visible

### Phase 5: Editor ✅ COMPLETE
- [x] Create custom model in under 2 minutes
- [x] Save model to library
- [x] Place custom model in world

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
- [x] Waterlogged models support (fences, stairs, slabs)
- [ ] Water state persists and syncs (deferred to Phases 3/7)

### Phase 10: Performance
- [ ] 90+ FPS with 1000 entities
- [ ] 60+ FPS in multiplayer with 4 players
- [ ] Sub-100ms world load time

---

## Sprite Icon Generation ✅ COMPLETE (2026-01-03)
- [x] Added `--generate-sprites` CLI flag and `make sprite-gen` target to render hotbar/palette icons.
- [x] GPU icon pass renders one block/model in 3/4 view with AO/shadows and saves to `textures/rendered/`.
- [x] Transparent backgrounds via chroma-key sky; auto-generated `missing.png` placeholder.
- [x] HUD loads generated sprites when present, with fallback to placeholder.

---

## Current Work (2026-01-04)
- All planned phases complete. Looking for new features to implement.

## Done Recently
- **Mirror Mode for Model Editor** (2026-01-04):
  - X/Y/Z axis toggle buttons in editor UI
  - Multiple axes can be enabled simultaneously (2x/4x/8x placements)
  - Mirrored place and erase operations
  - Single undo entry for all mirrored voxels
  - Wireframe plane indicators showing active mirror axes
  - 8 unit tests covering mirror functionality
- **Phase 4: Sub-Voxel Model System** (2026-01-04): Marked complete - all sub-voxel features implemented
- **Phase 5: Editor Undo/Redo** (2026-01-04): 50-state undo/redo stack for voxel editor
- **Tinted Glass & Sub-voxel Translucency** (2026-01-03):
  - `TintedGlass` block type with 32-color tint palette stored in metadata
  - Tinted shadows: light passing through tinted glass gets colored
  - Settings toggle for tinted shadows
  - Sub-voxel translucency: palette colors with alpha < 1.0 render as translucent
  - Internal face artifact elimination for smooth translucent volumes
  - Colored shadows from translucent sub-voxels
- **Phase 5: In-Game Model Editor** - Complete implementation:
  - `.vxm` file format for portable model storage
  - `LibraryManager` for saving/loading models from `user_models/` directory
  - `EditorState` with scratch pad, palette, orbit camera
  - Isometric 3D viewport with software rasterizer and z-buffer
  - Tools: Pencil, Eraser, Eyedropper, Fill, Rotate (90° Y-axis)
  - 16-color palette with RGBA color picker
  - Library browser with Load functionality and scrollbar
  - Save to Library with overwrite confirmation
  - Runtime sprite generation for HUD icons
  - GPU buffer refresh when models are edited
  - Custom models appear in E-key palette
  - Auto-rotation to face player when placing custom models
  - Right-click to rotate placed custom models
  - Name input limited to 32 characters
- Sub-voxel models (ladders/fences/gates) rendering & shadows complete.
- Waterlogging: complete support for models coexisting with water sources and flow.
- UX: default target block outline set **off** (toggled in UI).

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
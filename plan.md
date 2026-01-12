# Voxel World - Building Game Plan

## Mission Statement

**Voxel World** is a **creative building-focused multiplayer game** (1-4 players) emphasizing collaborative construction, not survival or crafting. Players explore diverse biomes and use powerful building tools to create structures ranging from simple shelters to complex architectural works. The game prioritizes:

- **Pure Creative Expression**: No mining progression, no crafting recipes, no health/danger mechanics
- **Rich Building Tools**: Templates, measurement guides, stencils, flood fill for efficient construction
- **Diverse Biomes**: Procedurally generated worlds with grasslands, mountains, deserts, swamps, snow, and caves
- **Sub-Voxel Detail**: Multi-resolution voxel models (8³/16³/32³) for furniture, decorations, and architectural elements
- **Collaborative Building**: Multiplayer support for shared creative projects (deferred until single-player complete)
- **Performance First**: 90+ FPS on mid-range hardware with optional graphics features

---

## Core Goals

### Building Experience
- **Template Library**: Save and reuse world regions with full metadata (doors, colors, water)
- **Measurement Tools**: Persistent holographic rulers with dimension displays and laser rangefinders
- **Stencil System**: Create holographic building guides from existing structures
- **Flood Fill**: Smart block replacement respecting type boundaries and painted block properties
- **Intuitive UI**: Toolbar-based tool palette (T key) with visual feedback

### World Diversity
- **5 Primary Biomes**: Grassland, Mountains, Desert, Swamp, Snow
- **Rare/Exotic Variants**: Crystal caves, mushroom forests, lava fields
- **Natural Transitions**: Elevation, temperature, and rainfall noise maps create smooth biome blending
- **Cave Networks**: Multi-depth cave systems with water/lava lakes, stalactites, stalagmites
- **Debug Visualization**: In-game worldgen map viewing for tweaking terrain

### Visual Richness
- **Glowing Blocks**: Lava and luminescent blocks with optional real-time lighting
- **Water Varieties**: Ocean, lake, swamp water with distinct colors and flow rates
- **Biome-Specific Assets**: Unique trees, ground cover, and block types per biome
- **Sub-Voxel Models**: Multi-resolution models (8³/16³/32³) for doors, furniture, decorations with native GPU rendering
- **Painted Blocks**: 19 textures × 32 tints = 608 customizable block variants

### Multiplayer (Deferred)
- **1-4 Players**: Small cooperative sessions, not MMO-scale
- **Free-For-All Building**: No permissions or zones, full creative freedom
- **Template Sharing**: Sync custom templates between players
- **Real-Time Sync**: Block changes, water flow, and tool visualizations

---

## Architecture Overview

### Coordinate Systems
- **World**: Global block positions (i32), infinite horizontal bounds
- **Chunk**: 32³ blocks, organized in unlimited HashMap
- **Region**: 32×32 chunks saved to disk (1024×128×1024 blocks per file)
- **Sub-Voxel**: 8³, 16³, or 32³ voxels per block for models (per-model resolution)

### Core Systems
- **Vulkan Compute Shader Rendering**: GPU ray marching for blocks and sub-voxels
- **Async Chunk Generation**: 4-thread terrain generation with priority queue
- **Sparse Water Grid**: HashMap-based cellular automata (W-Shadow algorithm)
- **Block Metadata**: Per-block data for models, rotation, painted colors, water properties
- **Region-Based Persistence**: Incremental saves, zstd compression

### Key Files
- `main.rs` - Vulkan setup, render loop, input, physics, HUD
- `chunk.rs` - BlockType enum, chunk storage (32³), metadata
- `world.rs` - Multi-chunk management, terrain generation, biomes
- `shaders/traverse.comp` - GPU ray marching, lighting, AO, sub-voxels
- `water.rs` - Cellular automata water simulation
- `console/` - Command system for world editing
- `editor/` - In-game sub-voxel model editor

---

## Completed Phases

### Phase 1: Infinite Chunk Streaming ✅
- Sliding window texture system with auto-shifting origin
- HashMap-based unlimited chunk storage
- Async 4-thread terrain generation
- Distance-based chunk loading priority

### Phase 2: Block Physics System ✅
- FallingBlock entity system with gravity
- Tree chopping with flood-fill support detection
- Frame-distributed BlockUpdateQueue (16-128 updates/frame)

### Phase 3: World Persistence ✅
- Region file format (32×32 chunks per file)
- Chunk serialization with zstd compression
- Auto-save every 30s, dirty chunk tracking
- Migration and versioning support

### Phase 4: Sub-Voxel Model System ✅
- Multi-resolution voxel models: 8³, 16³, and 32³ with 32-color palettes
- Three-tier GPU atlas system for native resolution rendering (no downsampling)
- GPU ray marching for sub-voxel rendering with dynamic resolution
- Collision detection and shadow casting
- Translucency support with colored shadows
- Model rotation and LOD system

### Phase 5: In-Game Model Editor ✅
- Modal editor (N key) with 3D canvas supporting 8³, 16³, and 32³ resolutions
- Tools: pencil, eraser, fill, eyedropper, rotate, mirror, cube, sphere
- Scroll to zoom, adjustable shape sizes (1-32 voxels for 32³ models)
- Library management with save/load/overwrite
- Runtime sprite generation for HUD (auto-scaled for model size)
- Custom models placeable in world at native resolution

### Phase 6: Interactive Block Types ✅
- 5 door variants (40 models total: upper/lower × hinge × open/closed)
- Trapdoor system (floor/ceiling, open/closed)
- Window blocks with fence-like connections
- State persistence and GPU metadata sync

### Phase 10: Water Flow Simulation ✅
- Mass-based cellular automata (W-Shadow algorithm)
- Source blocks for infinite water
- Waterlogging support for models
- Frame-distributed updates (64 cells/frame)
- Simulation radius limiting (64 blocks)

### Phase 12: Command Console System ✅ (Partial)
- Console framework (/ key to open)
- Commands: `fill`, `sphere`, `tp`, `help`, `clear`
- Relative coordinates (`~` syntax)
- Volume confirmation for large operations

### Phase 14: Enhanced Water System ✅
- Ocean, Lake, River, Swamp, Spring water types
- Visual differentiation (color, fog, murky swamp water)
- Physical differentiation (River flows fast, Swamp flows slow)
- Persistence and GPU metadata integration
- Palette support for placing specific water types

### Phase 15: Biome Generation System (Core) ✅
- Noise maps for Elevation, Temperature, and Rainfall
- 5 Primary Biomes: Grassland, Mountains, Desert, Swamp, Snow
- Biome-specific height curves and surface/subsurface blocks
- Biome-specific vegetation: Oak, Pine, Willow, Cactus
- Ground cover: Tall Grass, Flowers, Mushrooms, Lily Pads
- Seamless transitions between climates

### Additional Completed Features
- **New Biome Textures**: Seamless textures for Cactus, Mud, Sandstone, and Ice
- **no_collision Models**: Walk-through support for grass, flowers, and mushrooms
- **Paintable Blocks**: 19 textures × 32 tints with per-block metadata
- **Sprite Icon Generation**: GPU-rendered hotbar/palette icons
- **Tinted Glass**: Colored shadows through translucent blocks
- **Model Editor Mirror Mode**: Multi-axis symmetry with visual guides
- **Undo/Redo**: 50-state history for model editor
- **Multi-Resolution Sub-Voxels**: Support for 8³, 16³, and 32³ models with native GPU rendering
- **Crystal Blocks**: Sub-voxel crystal models with 32 tint colors and point light emission

---

## Active Development Phases

### Phase 15.4: Cave Biome Integration ✅

**Goal**: Extend biome diversity into the underground cave networks.

**Status**: COMPLETE

#### 15.4 Cave Biome Rules
- [x] Caves inherit surface biome properties (biome-specific density multipliers)
- [x] Ice caves: ice stalactites and stalagmites in snow biomes
- [x] Desert caves: dry (no water), fewer caves (0.6x density)
- [x] Swamp caves: heavily flooded (water up to sea_level+5)
- [x] Mountain caves: deep networks (1.5x density), lava lakes at low depths (<20)
- [x] Stalactites/Stalagmites: 4 new sub-voxel models (stone and ice variants), ~15% spawn rate

#### 15.5 Debug Visualization ✅
- [x] Console command: `/biome_debug [on|off]`
- [x] Overlay HUD: show current elevation, temperature, rainfall values
- [x] Minimap mode: color-coded biome map (red=desert, green=grassland, etc.)
- [ ] Noise map export: save elevation/temp/rainfall as PNG for external editing
- [ ] Hot-reload biome rules without restarting

---

### Phase 16: Building Tools System

**Goal**: Implement template library, measurement tools, stencils, and flood fill for efficient building.

**Priority**: HIGH (Development Priority #4)

#### 16.1 Template Library

**16.1.1 Template Data Structure**
- [x] `VxtFile` struct: block data, metadata (doors, painted blocks, water), dimensions, rotation
- [x] File format: `.vxt` (Voxel Template), compressed with zstd
- [x] Metadata: name, author, tags, creation date
- [x] Storage: `user_templates/` directory
- [x] Thumbnail generation (CPU-based software rasterizer, 64×64 PNG)

**16.1.2 Template Selection & Copy**
- [x] Selection mode: V key toggle, left-click pos1, right-click pos2 (green/blue markers + yellow wireframe)
- [x] Visual HUD overlay showing selection status and dimensions
- [x] Console command: `/copy <x1> <y1> <z1> <x2> <y2> <z2> <dx> <dy> <dz> [rotate_90|rotate_180|rotate_270]`
- [x] Include all block types, metadata, water states, sub-voxel models
- [x] Rotation support (90°, 180°, 270° around Y-axis)
- [x] Volume confirmation for large operations (>volume threshold)
- [x] Template save/load system (.vxt file format with zstd compression)
- [x] Console commands: `/template save <name> [tags]`, `/template load <name>`, `/template list`, `/template delete <name>`, `/template info <name>`
- [x] Maximum size: 128×128×128 blocks (enforced with warning)

**16.1.3 Template Placement**
- [x] Rotation controls: 0°, 90°, 180°, 270° around Y-axis (R key to rotate)
- [x] Placement mode: ghost preview follows cursor, Enter to place
- [x] Frame-distributed placement: 1000 blocks/frame for large templates
- [x] Confirmation prompt before placement
- [x] Browse library UI in template browser (L key)
- [x] Preview thumbnails: 64×64 isometric view with adaptive cell sizing
- [ ] No undo (by design - templates are large operations)

**16.1.4 Template Library UI**
- [x] In-game browser (keybind: L for Library)
- [x] Save dialog with name and tags input (auto-focus on name field)
- [x] Template list with Load/Delete/Regenerate Thumbnail actions
- [x] Current selection display (dimensions, block count)
- [x] Selection mode toggle and status
- [x] Search/filter: by name, tags, dimensions (real-time filtering)
- [x] Runtime thumbnail generation (CPU-based software rasterizer)
- [x] Thumbnail display: 64×64 isometric preview next to each template
- [x] Regenerate thumbnail button: recreate thumbnails for existing templates
- [x] Thumbnail caching: automatically refreshes on save/regenerate
- [ ] Import/Export UI (files can be manually shared via user_templates/)

**Technical Approach:**
```rust
struct Template {
    name: String,
    author: String,
    tags: Vec<String>,
    dimensions: (u32, u32, u32),
    blocks: Vec<BlockType>,          // Flattened 3D array
    metadata: Vec<Option<BlockMetadata>>,
    water_cells: HashMap<Vector3<i32>, WaterCell>,
    thumbnail: Option<Vec<u8>>,      // PNG data
}

impl Template {
    fn save(&self, path: &Path) -> Result<()> {
        let data = bincode::serialize(self)?;
        let compressed = zstd::encode_all(&data[..], 3)?;
        std::fs::write(path, compressed)?;
        Ok(())
    }

    fn load(path: &Path) -> Result<Self> {
        let compressed = std::fs::read(path)?;
        let data = zstd::decode_all(&compressed[..])?;
        Ok(bincode::deserialize(&data)?)
    }

    fn place(&self, world: &mut World, origin: Vector3<i32>, rotation: u8) {
        // Frame-distributed placement for large templates
        let batch_size = 1000; // blocks per frame
        for chunk in self.blocks.chunks(batch_size) {
            // Place chunk of blocks, yield to next frame
        }
    }
}
```

#### 16.2 Measurement Tool

**16.2.1 Measurement Block Placement**
- [x] Place mode: left-click to place holographic marker block (when rangefinder active)
- [x] Remove mode: right-click to remove last marker
- [x] Hologram appearance: glowing colored cubes with pulsing animation (shader-rendered)
- [x] Maximum measurement blocks: 4 markers (push constant limit), color-coded (Cyan/Magenta/Yellow/Orange)

**16.2.2 Laser Rangefinder Mode**
- [x] Toggle mode: G key toggle (M key is used by minimap)
- [x] Laser beam: decorative red brackets around crosshair when targeting
- [x] HUD display: distance in blocks (e.g., "📏 12.5 blocks") below crosshair
- [ ] Locked measurement: place block while in rangefinder mode → laser stays, updates if hit block changes

**16.2.3 Dimension Display**
- [x] When 2+ measurement blocks exist: white connecting lines between consecutive markers
- [x] HUD panel shows distances between consecutive markers
- [x] Text labels: HUD overlay with axis breakdowns (X:n, Y:n, Z:n)
- [x] Total distance shown when 3+ markers present

**16.2.4 Measurement Block Persistence**
- [x] Measurement blocks persist in world (saved in metadata, loaded on startup)
- [x] Console command: `/measure clear` removes all measurement markers

**Technical Approach:**
```rust
enum MeasurementMode {
    Placed,            // Static marker
    Rangefinder {      // Dynamic laser
        target: Vector3<i32>,
        distance: f32,
    },
}

struct MeasurementBlock {
    position: Vector3<i32>,
    mode: MeasurementMode,
    color: [f32; 3],   // User-configurable
}

impl MeasurementBlock {
    fn render(&self, renderer: &mut Renderer) {
        match self.mode {
            MeasurementMode::Placed => {
                // Render holographic cube
                renderer.draw_wireframe_cube(self.position, [0.0, 1.0, 1.0, 0.5]);
            }
            MeasurementMode::Rangefinder { target, distance } => {
                // Render laser beam
                renderer.draw_line(self.position, target, [1.0, 0.0, 0.0, 1.0]);
                renderer.draw_text_3d(
                    target,
                    &format!("{:.1} blocks", distance),
                    [1.0, 1.0, 1.0, 1.0]
                );
            }
        }
    }
}
```

#### 16.3 Stencil System

**16.3.1 Stencil Creation**
- [ ] Select region (same UI as template copy)
- [ ] Console command: `/stencil create <name>` after selecting
- [ ] Extract block shapes (ignore specific types), create holographic guide
- [ ] Save to stencil library: `user_stencils/` directory

**16.3.2 Stencil Placement**
- [ ] Browse stencil library (similar to template browser)
- [ ] Placement mode: ghost blocks follow cursor, adjustable opacity (0.3-0.8)
- [ ] Rotation: 0°, 90°, 180°, 270° around Y-axis
- [ ] Click to place → stencil stays in world until dismissed

**16.3.3 Stencil Rendering**
- [ ] Holographic blocks: single color (configurable: default cyan)
- [ ] Adjustable opacity: HUD slider or keybind ([/] keys)
- [ ] Render mode: wireframe or semi-transparent solid
- [ ] Z-fighting prevention: slight depth offset

**16.3.4 Stencil Persistence**
- [ ] Stencils persist in world save (separate from block data)
- [ ] Dismissing: console command `/stencil remove <id>` or UI button
- [ ] Multiple stencils: can have several active simultaneously
- [ ] Stencil IDs: auto-generated, shown in UI list

**Technical Approach:**
```rust
struct Stencil {
    name: String,
    positions: Vec<Vector3<i32>>,  // Relative to origin
    color: [f32; 4],               // RGBA
    opacity: f32,
}

struct PlacedStencil {
    id: u64,
    stencil: Stencil,
    origin: Vector3<i32>,
    rotation: u8,
}

impl PlacedStencil {
    fn render(&self, renderer: &mut Renderer) {
        for &offset in &self.stencil.positions {
            let rotated = rotate_offset(offset, self.rotation);
            let world_pos = self.origin + rotated;

            let mut color = self.stencil.color;
            color[3] = self.stencil.opacity;

            renderer.draw_holographic_block(world_pos, color);
        }
    }
}
```

#### 16.4 Flood Fill Tool

**16.4.1 Flood Fill Logic**
- [ ] Start block: raycast to determine clicked block type
- [ ] Fill rule: only replace blocks of same type as start block
- [ ] Painted blocks: only fill if texture AND tint match
- [ ] Model blocks: do not fill (prevents accidental overwrite)
- [ ] Water blocks: treat as separate type (don't mix with solid blocks)

**16.4.2 Safety Limits**
- [ ] Pre-scan flood fill region, count affected blocks
- [ ] If count > 10,000: show confirmation dialog with exact count
- [ ] Maximum fill size: 1,000,000 blocks (hard limit with error message)
- [ ] Frame-distributed filling: spread over multiple frames (like template placement)

**16.4.3 Flood Fill UI**
- [ ] Hotbar item: `FloodFillTool`
- [ ] Select replacement block from palette (right-click to choose)
- [ ] HUD display: "Fill: [source] → [target]"
- [ ] Click block to execute fill

**16.4.4 Console Command**
- [ ] `/fill_flood <target_block> [x] [y] [z]`
- [ ] If coordinates omitted: use raycast hit point
- [ ] Confirmation prompt if >10,000 blocks affected

**Technical Approach:**
```rust
struct FloodFillTool {
    target_block: BlockType,
}

impl FloodFillTool {
    fn execute(&self, world: &mut World, start_pos: Vector3<i32>) -> Result<u32> {
        let source_block = world.get_block(start_pos);

        // Pre-scan to count affected blocks
        let affected = self.scan_region(world, start_pos, source_block);

        if affected.len() > 10_000 {
            // Require user confirmation
            confirm_dialog(&format!("Fill {} blocks?", affected.len()))?;
        }

        if affected.len() > 1_000_000 {
            return Err("Fill region too large (max 1M blocks)".into());
        }

        // Frame-distributed fill
        for chunk in affected.chunks(1000) {
            for &pos in chunk {
                world.set_block(pos, self.target_block);
            }
            yield_frame(); // Prevent freeze
        }

        Ok(affected.len())
    }

    fn scan_region(&self, world: &World, start: Vector3<i32>, source: BlockType)
        -> Vec<Vector3<i32>>
    {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(start);

        while let Some(pos) = queue.pop_front() {
            if visited.contains(&pos) || visited.len() > 1_000_000 {
                continue;
            }

            let block = world.get_block(pos);
            if !self.matches_source(block, source, pos, world) {
                continue;
            }

            visited.insert(pos);

            // Check 6 neighbors
            for offset in NEIGHBOR_OFFSETS {
                queue.push_back(pos + offset);
            }
        }

        visited.into_iter().collect()
    }

    fn matches_source(&self, block: BlockType, source: BlockType,
                      pos: Vector3<i32>, world: &World) -> bool
    {
        if block != source {
            return false;
        }

        // For painted blocks, check texture AND tint
        if block == BlockType::Painted {
            let source_paint = world.get_paint_data(start_pos);
            let current_paint = world.get_paint_data(pos);
            return source_paint == current_paint;
        }

        true
    }
}
```

#### 16.5 Tools Palette UI

**16.5.1 UI Layout**
- [ ] Keybind: T key toggles tools window
- [ ] Window position: right side of screen (configurable)
- [ ] Toolbar: vertical icon list with labels
- [ ] Mouse-over: tooltip with tool description and hotkey

**16.5.2 Tool Icons**
- [ ] Template: blueprint icon
- [ ] Measurement: ruler icon
- [ ] Stencil: ghost block icon
- [ ] Flood Fill: paint bucket icon
- [ ] Active tool: highlighted border

**16.5.3 Multi-Tool Support**
- [ ] Some tools can be active simultaneously:
  - Measurement blocks persist while using other tools
  - Stencils persist while building
  - Template placement is exclusive (blocks other tools)
- [ ] Tool state: independent activation/deactivation

**16.5.4 Tool Settings Panel**
- [ ] Expandable panel below tool icons
- [ ] Settings per tool:
  - Measurement: dimension interval, laser color
  - Stencil: opacity slider, color picker
  - Flood Fill: preview mode (show affected blocks before filling)

---

## Deferred Phases

### Phase 7: Entity System (Low Priority)
- Deferred until multiplayer or advanced features needed
- Falling blocks already implemented (Phase 2)
- Dropped items not critical for building-focused game
- Animals/critters: nice-to-have, not core experience

### Phase 8: Multiplayer Networking (Deferred)
- **Status**: Deferred until single-player features complete
- **Scope**: 1-4 players, cooperative building
- **Free-for-all**: No permissions or build zones
- **Sync Requirements**: Blocks, water, templates, tool visualizations
- **Architecture**: Dedicated server, UDP for positions, TCP for block changes

### Phase 9: AI and Scripting (Not Applicable)
- No animals or AI-driven entities in current vision
- Removed from roadmap unless future feature request

### Phase 11: Performance Optimization (Ongoing)
- Maintain 90+ FPS target throughout development
- Optional graphics features for lower-end hardware:
  - Dynamic lighting (glowing blocks)
  - Water reflections
  - Sub-voxel LOD
  - Shadow quality settings

---

## Development Roadmap

### Immediate Next Steps (Priority Order)

1. ~~**Phase 13: Glowing Blocks** ✅ COMPLETE~~

2. **Phase 14: Enhanced Water** (Priority #1)
   - Extend water metadata (type, color, flow rate)
   - Implement swamp water with murky color
   - Integrate water types into terrain generation
   - Shader updates for colored water

3. **Phase 15: Biome Generation** (Priority #2)
   - Implement elevation, temperature, rainfall noise maps
   - Define 5 primary biomes with classification rules
   - Add biome-specific blocks (mud, cactus, willow trees)
   - Cave biome integration (stalactites, glowing mushrooms)
   - Debug visualization overlay

4. **Phase 16: Building Tools** (Priority #3)
   - Template library (copy/paste with rotation)
   - Measurement tool (blocks + laser rangefinder)
   - Stencil system (holographic guides)
   - Flood fill tool (safe mass replacement)
   - Tools palette UI (T key)

### Future Work (After Core Features)

5. **Performance Optimization Pass**
   - Profile with all features enabled
   - Optimize glowing block rendering
   - Water simulation performance tuning
   - Template placement frame distribution

6. **Polish & UX Improvements**
   - Tutorial system for new players
   - Improved hotbar/palette organization
   - Customizable keybinds
   - Settings menu overhaul

7. **Multiplayer Networking** (Phase 8)
   - Only start after single-player is feature-complete
   - Dedicated server architecture
   - Block change synchronization
   - Template sharing between players

---

## Success Criteria

### Completed ✅
- [x] World generates infinitely in all horizontal directions
- [x] Steady 90+ FPS while moving through world
- [x] Trees fall when trunk is broken
- [x] Water flows naturally using W-Shadow cellular automata
- [x] Sub-voxel models render with shadows and translucency
- [x] In-game model editor with full toolset
- [x] Doors and interactive blocks with state persistence
- [x] Painted blocks with 608 texture/tint combinations
- [x] Command console with world editing commands

### Phase 13: Glowing Blocks ✅
- [x] Lava blocks glow orange/red with optional light emission
- [x] GlowStone illuminates area (when point lights enabled)
- [x] Settings toggle for point lights works correctly
- [x] Crystal blocks with 32 tint colors and tinted point light emission
- [x] Performance maintained with multiple light sources

### Phase 14: Enhanced Water ✅
- [x] Swamp water renders murky green-brown
- [x] River water flows 1.5x faster than ocean water
- [x] Water type persists across save/load
- [x] Underwater fog color matches water type
- [x] Biomes generate correct water types (swamp → swamp water)

### Phase 15: Biome Generation ✅
- [x] 5 distinct biomes generate with natural transitions
- [x] Temperature decreases visibly with elevation (snow on mountain peaks)
- [x] Rainfall affects vegetation density (sparse grass in dry areas)
- [ ] Debug overlay shows elevation/temp/rainfall values
- [ ] Rare biomes spawn (<5% of world)
- [x] Caves have biome-specific features (partially: sandstone in desert, etc.)

### Phase 16: Building Tools
- [ ] Copy 64×64×64 region to template library in <5 seconds
- [ ] Place template with rotation preview, confirm placement
- [ ] Measurement blocks show distance accurately (±0.1 blocks)
- [ ] Laser rangefinder updates in real-time (<16ms latency)
- [ ] Stencil opacity adjustable from 0.3 to 0.8
- [ ] Flood fill 50,000 blocks without freezing (frame-distributed)
- [ ] Tools palette (T key) responsive, <50ms to open/close

### Performance Targets
- [ ] 90+ FPS with all features enabled (mid-range GPU: RTX 3060 / RX 6600)
- [ ] 60+ FPS on lower-end hardware (GTX 1660 / RX 580) with optional features disabled
- [ ] <200ms world load time from save file
- [ ] <100ms chunk generation time (async, doesn't block render)

---

## Technical Debt & Future Considerations

### Known Limitations
- **Template placement**: No undo (by design), but adds friction to workflow
- **Water simulation**: CPU-bound, could move to GPU compute shader (Phase 11)
- **Biome transitions**: Fixed blend distance, could be dynamic based on terrain features
- **Tool state**: Not networked yet (required for multiplayer)

### Research & Exploration
- **Terrain sculpting**: Smooth terrain modification (curved hills, not just blocks)
- **World height expansion**: 256 → 384 or 512 blocks (requires chunk format change)
- **Advanced water**: Currents, waterfalls with particle effects
- **Procedural structures**: Villages, ruins, dungeons (building templates as procedural content)

---

## Build Commands

```bash
make build          # Build release (default)
make run            # Build and run release
make run-debug      # Build and run debug with RUST_BACKTRACE=1
make test           # Run tests
make fmt            # Format code
make lint           # Run clippy linter
make checkall       # Format, lint, and test (run after making changes)
```

## CLI Options

```bash
make run ARGS="--seed 42"           # Custom terrain seed (-S)
make run ARGS="--fly-mode"          # Start in fly mode (-f)
make run ARGS="--spawn-x 100 --spawn-z 200"  # Custom spawn (-x, -z)
make run ARGS="--time-of-day 0.5"   # Pause at noon (-t)
make run ARGS="--view-distance 8"   # Increase view distance (-v)
make run ARGS="--render-mode depth" # Start in depth mode (-r)
make run ARGS="--verbose"           # Debug output
```

## Development Workflow

### ⚠️ PRIORITY ONE: Commit After Every Batch of Work

**CRITICAL**: To enable rollback and prevent loss of working states:

1. After completing a logical batch of changes, run `make checkall`
2. Fix any errors or warnings found
3. **Commit immediately** with a descriptive message
4. Do NOT accumulate multiple unrelated changes before committing

```bash
make checkall                    # Must pass before committing
git add -A
git commit -m "type: description"
```

---

## Current Work (2026-01-11)

**Status**: Phase 16.2 (Measurement Tool) - COMPLETE

**Completed Features:**
- **Laser Rangefinder** (16.2.2):
  - G key toggles rangefinder mode on/off
  - Real-time distance display below crosshair ("📏 X.X blocks")
  - Red corner brackets around crosshair when targeting a block
  - Shows "--.- blocks" when not targeting anything
- **Measurement Markers** (16.2.1 + 16.2.3):
  - Left-click places holographic marker blocks (up to 4, color-coded)
  - Right-click removes the last marker (stack-based)
  - Shader-rendered glowing cubes with pulsing animation
  - White connecting lines between consecutive markers
  - HUD panel shows distances with axis breakdowns (X:n, Y:n, Z:n)
  - Total distance when 3+ markers present
- **Measurement Persistence** (16.2.4):
  - Markers saved in world metadata, loaded on startup
  - `/measure clear` command to remove all markers
- **Visual Selection System**:
  - V key toggles selection mode (with HUD indicator)
  - Left-click places green pos1 marker, right-click places blue pos2 marker
  - Yellow wireframe box shows selection bounds in real-time
  - Selection markers use placement position (adjacent to hit face)
  - Top-center HUD overlay shows selection status and dimensions
  - Shader-rendered markers and wireframe (overlays.glsl)
- **Copy Command** (src/console/commands/copy.rs):
  - Syntax: `/copy <x1> <y1> <z1> <x2> <y2> <z2> <dx> <dy> <dz> [rotate_90|rotate_180|rotate_270]`
  - Preserves all block metadata (models, tinted glass, painted blocks, water types)
  - Y-axis rotation support (90°, 180°, 270°)
  - Volume confirmation for large operations
  - Relative coordinate support with `~`
- **Template Save/Load System** (src/templates/):
  - .vxt file format with zstd compression
  - Console commands: `/template save/load/list/delete/info`
  - Sparse block storage (only non-air blocks)
  - Full metadata preservation (models, tint, paint, water sources)
  - Template library manager with user_templates/ directory
- **Template Placement**:
  - Ghost preview follows cursor (wireframe visualization)
  - R key to rotate template (90° increments)
  - Enter to confirm placement
  - Frame-distributed placement (1000 blocks/frame)
  - Arrow keys to move placement position
- **Template Browser UI** (L key):
  - Save dialog with name and tags
  - Template list with Load/Delete actions
  - Current selection display (dimensions, volume)
  - Selection mode status and controls
- **Console Y Coordinate Fix**:
  - `~ ~ ~` now refers to one block above the ground (feet_pos + 1)
  - All console commands use consistent player position

**Testing Guide:**
```bash
# Visual selection and save
V                           # Enter selection mode
# Click to set pos1 and pos2
L                           # Open template browser
# Click "Save as Template..."
# Enter name and tags, click Save

# Or use console commands
/select pos1 ~ ~ ~
/select pos2 ~10 ~5 ~10
/template save my_house building

# Load and place template
/template load my_house     # Enters placement mode
R                           # Rotate
Arrow keys                  # Move position
Enter                       # Confirm placement

# Copy region directly
/copy ~ ~ ~ ~10 ~5 ~10 ~20 ~ ~ rotate_90
```

**Recently Added** (2026-01-07):
- Search/filter functionality (by name, tags, dimensions)
- Runtime thumbnail generation (CPU-based software rasterizer)
  - Auto-generates 64×64 PNG thumbnails when saving templates
  - Isometric 3/4 view matching custom model sprites
  - Metadata-aware colors (TintedGlass, Painted, Water types)
  - Per-face shading (top 100%, bottom 40%, sides 60-85%)
  - Adaptive cell sizing for templates 1×1×1 to 128×128×128
- Thumbnail display in template browser with caching
- Regenerate thumbnail button for existing/corrupted thumbnails
- Auto-focus on name field in save dialog
- Delete operation removes both .vxt and .png files

**Optional Enhancements** (future work):
1. Import/Export UI (files can be manually shared via user_templates/)
2. Template categories/folders for better organization

---

## Done Recently

- **Phase 16.2.1 + 16.2.3: Measurement Markers & Dimension Display** (2026-01-11): ✅ COMPLETE
  - Left-click to place up to 4 holographic marker cubes (color-coded: Cyan/Magenta/Yellow/Orange)
  - Right-click to remove last marker
  - Shader-rendered glowing cubes with pulsing animation (overlays.glsl)
  - White connecting lines between consecutive markers
  - HUD panel with distances and axis breakdowns (X:n, Y:n, Z:n)
  - Total distance calculation when 3+ markers present
  - Push constants for marker data (12 i32 fields + count)
- **Phase 16.2.2: Laser Rangefinder Mode** (2026-01-11): ✅ COMPLETE
  - G key toggles rangefinder mode on/off
  - Real-time distance display below crosshair (e.g., "📏 12.5 blocks")
  - Decorative red corner brackets around crosshair when targeting
  - Dark HUD overlay with red border for visibility
- **Phase 16.2.4: Measurement Persistence** (2026-01-11): ✅ COMPLETE
  - Markers saved in world metadata, loaded on startup
  - `/measure clear` console command
- **Multi-Resolution Sub-Voxel System** (2026-01-09): ✅ COMPLETE
  - Three-tier GPU atlas system for native 8³, 16³, and 32³ model rendering
  - Separate texture atlases (128×8×128, 256×16×256, 512×32×512) with zero voxel loss
  - Dynamic shader resolution: all functions query model_properties for actual resolution
  - Shadow quality upgrade: 96-step limit for accurate 32³ model shadows
  - Built-in models use 8³ for performance, custom models support all resolutions
  - Player physics fixes: fly mode collision detection and walk mode collision always-on
  - Editor UX improvements: bridge tool first-point indicator, right-click cancellation
  - Model placement fixes: prevented double-placement of custom models
- **Phase 16.1: Template Library** (2026-01-07): ✅ FEATURE COMPLETE
  - Visual selection system with V key toggle and shader-rendered markers
  - Copy command with rotation and full metadata preservation
  - Template save/load system (.vxt file format with zstd compression)
  - Template placement with ghost preview and frame-distributed loading
  - Template browser UI (L key) with save/load/delete actions
  - Search/filter by name, tags, and dimensions (real-time)
  - Runtime thumbnail generation with CPU-based software rasterizer
    - 64×64 PNG thumbnails with isometric 3/4 view
    - Auto-generated on save, regenerate button for existing templates
    - Metadata-aware colors and adaptive cell sizing
    - Thumbnail display with caching in template browser
  - Auto-focus on save dialog name field
  - Console Y coordinate fix (~ ~ ~ = feet_pos + 1)
- **Phase 15.4: Cave Biome Integration** (2026-01-07): ✅ COMPLETE
  - Cave generation module (src/cave_gen.rs) with biome-aware logic
  - 4 new cave decoration models: stalactites/stalagmites (stone and ice variants)
  - Biome-specific cave density, water rules, and decorations
  - Mountain lava lakes with depth-based spawn probability
- **Phase 15: Biome Generation System** (2026-01-06): ✅ CORE COMPLETE
  - Noise maps, biome rules, vegetation, and height curves implemented
- **Phase 14: Enhanced Water System** (2026-01-06): ✅ COMPLETE
  - Ocean, Lake, River, Swamp, Spring water types
  - Visual differentiation (color/fog) and physical differentiation (flow rate)
  - Palette integration: Place specific water types from the "Blocks" tab
- **Phase 13: Advanced Lighting System** (2026-01-05): ✅ COMPLETE
  - Lava, GlowStone, GlowMushroom, Crystal blocks with emission
  - Point light system with tinted colors for crystals
  - Settings toggles for point lights and LOD distance
- **Sub-Voxel Resolution Upgrades** (2026-01-05 → 2026-01-09): ✅ COMPLETE
  - Initial upgrade: Doubled model resolution from 8³ to 16³
  - Final upgrade: Added 32³ support with multi-tier GPU atlas system
  - Updated all model-related constants and shader code for dynamic resolution
- **Model Editor Enhancements** (2026-01-05): ✅ COMPLETE
  - Cube and sphere placement tools with adjustable size (1-32 for 32³ models)
  - Scroll to zoom functionality
  - Dynamic viewport scaling for 8³, 16³, and 32³ models
  - Fixed axis labels and depth testing
  - Fixed sprite generation scaling for all resolutions
- **Paintable Blocks Feature** (2026-01-05): ✅ COMPLETE
- **Phase 6: Interactive Block Types** (2026-01-04): ✅ COMPLETE
- **Sphere Console Command** (2026-01-04): ✅ COMPLETE
- **Phase 12: Command Console System** (2026-01-04): ✅ PARTIAL
- **Mirror Mode for Model Editor** (2026-01-04): ✅ COMPLETE
- **Phase 4: Sub-Voxel Model System** (2026-01-04): ✅ COMPLETE
- **Phase 5: In-Game Model Editor** (2026-01-03): ✅ COMPLETE
- **Tinted Glass & Sub-voxel Translucency** (2026-01-03): ✅ COMPLETE

---

*Last Updated: 2026-01-11*
*Plan Version: 2.3 - Building Tools (Measurement)*

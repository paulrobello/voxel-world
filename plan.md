# Voxel World - Building Game Plan

## Mission Statement

**Voxel World** is a **creative building-focused multiplayer game** (1-4 players) emphasizing collaborative construction, not survival or crafting. Players explore diverse biomes and use powerful building tools to create structures ranging from simple shelters to complex architectural works. The game prioritizes:

- **Pure Creative Expression**: No mining progression, no crafting recipes, no health/danger mechanics
- **Rich Building Tools**: Templates, measurement guides, stencils, flood fill for efficient construction
- **Diverse Biomes**: Procedurally generated worlds with grasslands, mountains, deserts, swamps, snow, and caves
- **Sub-Voxel Detail**: 16³ voxel models for furniture, decorations, and architectural elements
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
- **Sub-Voxel Models**: Detailed 16³ models for doors, furniture, decorations
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
- **Sub-Voxel**: 16³ voxels per block for models

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
- 16³ voxel models with 16-color palettes (upgraded from 8³)
- GPU ray marching for sub-voxel rendering
- Collision detection and shadow casting
- Translucency support with colored shadows
- Model rotation and LOD system

### Phase 5: In-Game Model Editor ✅
- Modal editor (N key) with 3D canvas (600×600 viewport for 16³ models)
- Tools: pencil, eraser, fill, eyedropper, rotate, mirror, cube, sphere
- Scroll to zoom, adjustable shape sizes (1-16 voxels)
- Library management with save/load/overwrite
- Runtime sprite generation for HUD (auto-scaled for model size)
- Custom models placeable in world

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

### Additional Completed Features
- **Paintable Blocks**: 19 textures × 32 tints with per-block metadata
- **Sprite Icon Generation**: GPU-rendered hotbar/palette icons
- **Tinted Glass**: Colored shadows through translucent blocks
- **Model Editor Mirror Mode**: Multi-axis symmetry with visual guides
- **Undo/Redo**: 50-state history for model editor
- **Sub-Voxel 16³ Upgrade**: Doubled model resolution from 8³ to 16³
- **Crystal Blocks**: Sub-voxel crystal models with 32 tint colors and point light emission

---

## Active Development Phases

### Phase 13: Advanced Lighting System ✅

**Goal**: Add glowing blocks with optional real-time light emission for dynamic lighting.

**Status**: COMPLETE

#### 13.1 Glowing Block Types ✅
- [x] `Lava` block: glowing orange/red, decorative (no damage)
- [x] `GlowStone` block: bright white/yellow light source
- [x] `GlowMushroom` block: soft blue/green glow for caves
- [x] `Crystal` block: colored glowing crystals with 32 tint variants
- [x] Block property: `emission_color` (RGB) + `emission_strength` (0.0-1.0)

#### 13.2 Light Emission System ✅
- [x] Settings toggle: "Enable Point Lights" in settings
- [x] Point light sources with radius falloff (quadratic attenuation)
- [x] Real-time point lights with configurable LOD distance
- [x] Maximum light sources: 256 lights per frame
- [x] Light culling: distance-based, only nearby chunks processed

#### 13.3 Visual Glow ✅
- [x] Emissive material rendering in shader
- [x] Shader: detect emissive blocks, add emission color to final output
- [x] Crystal blocks: tint-based emission colors (32 color palette)
- [x] Crystal point lights emit tinted colors matching block appearance

#### 13.4 Performance Considerations ✅
- [x] GPU light buffer upload (structured buffer of active lights)
- [x] LOD: configurable point light distance in settings
- [x] Separate enable toggles for shadows, AO, and point lights

**Technical Approach:**
```rust
// Block property extension
impl BlockType {
    fn emission_color(&self) -> Option<[f32; 3]> {
        match self {
            BlockType::Lava => Some([1.0, 0.3, 0.0]), // Orange
            BlockType::GlowStone => Some([1.0, 1.0, 0.8]), // Warm white
            BlockType::GlowMushroom => Some([0.2, 0.8, 1.0]), // Cyan
            _ => None,
        }
    }

    fn emission_strength(&self) -> f32 {
        match self {
            BlockType::Lava => 0.8,
            BlockType::GlowStone => 1.0,
            BlockType::GlowMushroom => 0.5,
            _ => 0.0,
        }
    }
}
```

**Shader Integration:**
```glsl
// In traverse.comp
struct Light {
    vec3 position;
    vec3 color;
    float radius;
};

layout(set = 0, binding = N) buffer LightBuffer {
    Light lights[];
};

vec3 calculateLighting(vec3 worldPos, vec3 normal, vec3 baseColor) {
    vec3 lighting = ambientOcclusion(worldPos, normal); // Base AO

    if (settings.dynamicLighting) {
        for (int i = 0; i < lightCount; i++) {
            Light light = lights[i];
            vec3 toLight = light.position - worldPos;
            float dist = length(toLight);

            if (dist < light.radius) {
                float attenuation = 1.0 - (dist / light.radius);
                attenuation *= attenuation; // Quadratic falloff
                lighting += light.color * attenuation;
            }
        }
    }

    return baseColor * lighting;
}
```

---

### Phase 14: Enhanced Water System

**Goal**: Extend water system with colors, types, and configurable flow rates.

**Priority**: HIGH (Development Priority #2)

#### 14.1 Water Metadata Extension ✅
- [x] Add `WaterType` enum: `Ocean`, `Lake`, `River`, `Swamp`, `Spring`
- [x] Per-cell metadata: `water_type`, `color_tint`, `flow_rate_multiplier`
- [x] Extend `WaterCell` struct with new fields
- [x] GPU upload: separate water metadata texture (reused modelMetadata)

#### 14.2 Water Types ✅
- [x] **Ocean**: Deep blue, standard flow rate
- [x] **Lake**: Clear blue-green, slower flow (0.7x)
- [x] **River**: Fast flow (1.5x), lighter blue
- [x] **Swamp**: Murky green-brown, very slow (0.3x), static in some areas
- [x] **Spring**: Crystal clear, source blocks only

#### 14.3 Water Color Rendering ✅
- [x] Shader: sample water type → color lookup table
- [x] Tint underwater fog based on water type
- [x] Surface reflections: sky color mixed with water tint
- [x] Swamp water: reduce transparency (murkier)

#### 14.4 Flow Rate Implementation ✅
- [x] Modify W-Shadow algorithm with `flow_rate_multiplier`
- [x] Swamp water: increase stability threshold (less spreading)
- [x] River water: decrease damping (faster flow)
- [x] Visualize flow direction with animated water surface (optional)

#### 14.5 Terrain Integration (In Progress)
- [ ] Desert biome: no water or dry riverbeds only
- [ ] Swamp biome: generate swamp water lakes
- [ ] Mountain biome: spring sources, fast rivers
- [ ] Grassland/forest: lakes and slow streams
- [ ] Snow biome: frozen water blocks (ice)

**Technical Approach:**
```rust
#[derive(Clone, Copy, Debug)]
enum WaterType {
    Ocean,
    Lake,
    River,
    Swamp,
    Spring,
}

impl WaterType {
    fn color_tint(&self) -> [f32; 3] {
        match self {
            WaterType::Ocean => [0.0, 0.3, 0.8],      // Deep blue
            WaterType::Lake => [0.1, 0.5, 0.7],       // Blue-green
            WaterType::River => [0.3, 0.6, 0.9],      // Light blue
            WaterType::Swamp => [0.3, 0.4, 0.2],      // Murky green
            WaterType::Spring => [0.4, 0.7, 1.0],     // Crystal clear
        }
    }

    fn flow_rate_multiplier(&self) -> f32 {
        match self {
            WaterType::Ocean => 1.0,
            WaterType::Lake => 0.7,
            WaterType::River => 1.5,
            WaterType::Swamp => 0.3,
            WaterType::Spring => 1.0,
        }
    }

    fn transparency(&self) -> f32 {
        match self {
            WaterType::Swamp => 0.4,  // Murky
            _ => 0.8,                  // Clear
        }
    }
}

struct WaterCell {
    mass: f32,
    is_source: bool,
    stable_ticks: u32,
    water_type: WaterType,  // NEW
}
```

---

### Phase 15: Biome Generation System

**Goal**: Implement elevation, temperature, and rainfall-based biome generation with smooth transitions.

**Priority**: HIGH (Development Priority #3)

#### 15.1 Noise Map Generation
- [ ] **Elevation Map**: Perlin/Simplex noise for terrain height (0-255 blocks)
- [ ] **Temperature Map**: Separate noise, decreases with elevation
- [ ] **Rainfall Map**: Independent noise for precipitation
- [ ] Configurable octaves, scale, and seed for each map
- [ ] Store maps in world save metadata for consistency

#### 15.2 Biome Classification Rules
- [ ] **Grassland**: Mid elevation (40-80), mid temp (0.4-0.7), mid-high rainfall (>0.5)
- [ ] **Mountains**: High elevation (>100), low temp (<0.3), any rainfall
- [ ] **Desert**: Low-mid elevation (<70), high temp (>0.7), low rainfall (<0.3)
- [ ] **Swamp**: Low elevation (<50), mid-high temp (>0.5), high rainfall (>0.7)
- [ ] **Snow/Tundra**: Any elevation with low temp (<0.2) OR high elevation (>120)
- [ ] **Rare/Exotic**: Special seeds or elevation+temp+rainfall combinations

#### 15.3 Biome-Specific Features

**Grassland:**
- [ ] Block types: Grass, Dirt, Stone
- [ ] Trees: Oak (dense), occasional flowers
- [ ] Ground cover: Tall grass patches (5-10% density)

**Mountains:**
- [ ] Block types: Stone, Gravel, Snow (peaks), exposed Bedrock (cliffs)
- [ ] Trees: Pine/Spruce (sparse, lower elevations only)
- [ ] Ground cover: Rocky, sparse grass

**Desert:**
- [ ] Block types: Sand, Sandstone, Gravel (dry riverbeds)
- [ ] Trees: Cactus blocks (vertical pillars), dead trees (rare)
- [ ] Ground cover: None, occasional red sand patches

**Swamp:**
- [ ] Block types: Dirt, mud blocks (new), swamp water
- [ ] Trees: Willow (drooping leaves), cypress (thick trunks)
- [ ] Ground cover: Lily pads (sub-voxel models), algae, mushrooms

**Snow/Tundra:**
- [ ] Block types: Snow, Ice, Stone
- [ ] Trees: Dead trees, sparse pine
- [ ] Ground cover: Snow layers (variable depth)

#### 15.4 Cave Biome Integration
- [ ] Caves inherit surface biome properties (temperature affects ice caves)
- [ ] Ice caves: frozen water, stalactites made of ice
- [ ] Desert caves: sandstone walls, dry (no water lakes)
- [ ] Swamp caves: flooded, glowing mushrooms
- [ ] Mountain caves: deep networks, lava lakes at low depths (<20)
- [ ] Stalactites/Stalagmites: new sub-voxel models, connect over time

#### 15.5 Debug Visualization
- [ ] Console command: `/biome_debug [on|off]`
- [ ] Overlay HUD: show current elevation, temperature, rainfall values
- [ ] Minimap mode: color-coded biome map (red=desert, green=grassland, etc.)
- [ ] Noise map export: save elevation/temp/rainfall as PNG for external editing
- [ ] Hot-reload biome rules without restarting

#### 15.6 Terrain Height Generation
- [ ] Use elevation map directly for Y-coordinate
- [ ] Add secondary noise for local variation (hills, valleys)
- [ ] Clamp to world height (0-255)
- [ ] Smooth transitions between biomes (interpolate height at boundaries)

**Technical Approach:**
```rust
struct BiomeGenerator {
    elevation_noise: FastNoise,
    temperature_noise: FastNoise,
    rainfall_noise: FastNoise,
    seed: u64,
}

impl BiomeGenerator {
    fn get_biome(&self, x: i32, z: i32) -> BiomeType {
        let elevation = self.elevation_noise.get_2d(x, z); // 0.0-1.0
        let temp = self.temperature_noise.get_2d(x, z);
        let rainfall = self.rainfall_noise.get_2d(x, z);

        // Adjust temperature by elevation (lapse rate)
        let adjusted_temp = temp - (elevation * 0.6);

        // Classify biome based on conditions
        if adjusted_temp < 0.2 || elevation > 0.8 {
            BiomeType::Snow
        } else if elevation > 0.6 {
            BiomeType::Mountains
        } else if temp > 0.7 && rainfall < 0.3 {
            BiomeType::Desert
        } else if elevation < 0.4 && rainfall > 0.7 {
            BiomeType::Swamp
        } else {
            BiomeType::Grassland
        }
    }

    fn get_height(&self, x: i32, z: i32) -> u8 {
        let base_height = self.elevation_noise.get_2d(x, z); // 0.0-1.0
        let detail = self.detail_noise.get_2d(x, z) * 0.2;   // Local variation
        ((base_height + detail) * 200.0 + 20.0) as u8        // Range: 20-220
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum BiomeType {
    Grassland,
    Mountains,
    Desert,
    Swamp,
    Snow,
    // Rare variants
    CrystalCave,
    MushroomForest,
    LavaField,
}
```

---

### Phase 16: Building Tools System

**Goal**: Implement template library, measurement tools, stencils, and flood fill for efficient building.

**Priority**: HIGH (Development Priority #4)

#### 16.1 Template Library

**16.1.1 Template Data Structure**
- [ ] `Template` struct: block data, metadata (doors, painted blocks, water), dimensions, rotation
- [ ] File format: `.vxt` (Voxel Template), compressed with zstd
- [ ] Metadata: name, author, tags, creation date, thumbnail
- [ ] Storage: `user_templates/` directory

**16.1.2 Template Selection & Copy**
- [ ] Selection mode: left-click first corner, right-click second corner (wireframe preview)
- [ ] Console command: `/template copy <name>` after selecting region
- [ ] Include all block types, metadata, water states, sub-voxel models
- [ ] Maximum size: 128×128×128 blocks (enforced with warning)

**16.1.3 Template Placement**
- [ ] Browse library UI (similar to model editor library)
- [ ] Preview thumbnails: isometric view, dimensions shown
- [ ] Rotation controls: 0°, 90°, 180°, 270° around Y-axis
- [ ] Placement mode: ghost preview follows cursor, click to place
- [ ] Frame-distributed placement: spread over multiple frames if >10,000 blocks
- [ ] No undo (too large), but confirmation prompt before placement

**16.1.4 Template Library UI**
- [ ] In-game browser (keybind: L for Library)
- [ ] Search/filter: by name, tags, size, date
- [ ] Preview pane: 3D isometric view, block count, dimensions
- [ ] Actions: Load, Delete, Export, Import (share files)

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
- [ ] New item in hotbar: `MeasurementTool`
- [ ] Place mode: click to place holographic marker block
- [ ] Hologram appearance: semi-transparent cube with glowing edges
- [ ] Maximum measurement blocks: unlimited (user manages cleanup)

**16.2.2 Laser Rangefinder Mode**
- [ ] Toggle mode: M key (or right-click with tool equipped)
- [ ] Laser beam: visible colored line from player to block hit point
- [ ] HUD display: distance in blocks (e.g., "Distance: 42 blocks")
- [ ] Locked measurement: place block while in rangefinder mode → laser stays, updates if hit block changes

**16.2.3 Dimension Display**
- [ ] When 2+ measurement blocks exist: draw wireframe connecting them
- [ ] Display dimensions at configurable intervals (default: every 8 blocks)
- [ ] Text labels: in-world 3D text or HUD overlay (user preference)
- [ ] Format: "X: 24, Y: 12, Z: 16" along each axis

**16.2.4 Measurement Block Persistence**
- [ ] Measurement blocks persist in world (saved with chunks)
- [ ] Breaking: punch like normal block (creative mode-style)
- [ ] Console command: `/measure clear` removes all measurement blocks in loaded chunks

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

### Phase 14: Enhanced Water
- [ ] Swamp water renders murky green-brown
- [ ] River water flows 1.5x faster than ocean water
- [ ] Water type persists across save/load
- [ ] Underwater fog color matches water type
- [ ] Biomes generate correct water types (swamp → swamp water)

### Phase 15: Biome Generation
- [ ] 5 distinct biomes generate with natural transitions
- [ ] Temperature decreases visibly with elevation (snow on mountain peaks)
- [ ] Rainfall affects vegetation density (sparse grass in dry areas)
- [ ] Debug overlay shows elevation/temp/rainfall values
- [ ] Rare biomes spawn (<5% of world)
- [ ] Caves have biome-specific features (ice stalactites in snow caves)

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

## Current Work (2026-01-06)

**Status**: Phase 14 (Enhanced Water System) Core Features COMPLETE. Moving to biome integration.

**Recent Work:**
- Implemented `WaterType` enum and metadata storage in chunks
- Added GPU support for per-voxel water types (color, clarity, fog)
- Implemented variable flow rates for River (fast), Swamp (slow), Lake (medium)
- Updated shaders to render murky swamp water and clear springs

**Next Actions:**
1. Begin Phase 15: Biome Generation System
2. Implement noise maps for elevation, temperature, rainfall
3. Integrate biome-specific water types (Swamp biome -> Swamp water)

---

## Done Recently

- **Phase 14: Enhanced Water System** (2026-01-06): ✅ CORE COMPLETE
  - Ocean, Lake, River, Swamp, Spring water types
  - Visual differentiation (color/fog) and physical differentiation (flow rate)
- **Phase 13: Advanced Lighting System** (2026-01-05): ✅ COMPLETE
  - Lava, GlowStone, GlowMushroom, Crystal blocks with emission
  - Point light system with tinted colors for crystals
  - Settings toggles for point lights and LOD distance
- **Sub-Voxel 16³ Upgrade** (2026-01-05): ✅ COMPLETE
  - Doubled model resolution from 8³ to 16³
  - Updated all model-related constants and atlas sizing
- **Model Editor Enhancements** (2026-01-05): ✅ COMPLETE
  - Cube and sphere placement tools with adjustable size (1-16)
  - Scroll to zoom functionality
  - Fixed viewport size for 16³ models (600×600)
  - Fixed axis labels and depth testing
  - Fixed sprite generation scaling
- **Paintable Blocks Feature** (2026-01-05): ✅ COMPLETE
- **Phase 6: Interactive Block Types** (2026-01-04): ✅ COMPLETE
- **Sphere Console Command** (2026-01-04): ✅ COMPLETE
- **Phase 12: Command Console System** (2026-01-04): ✅ PARTIAL
- **Mirror Mode for Model Editor** (2026-01-04): ✅ COMPLETE
- **Phase 4: Sub-Voxel Model System** (2026-01-04): ✅ COMPLETE
- **Phase 5: In-Game Model Editor** (2026-01-03): ✅ COMPLETE
- **Tinted Glass & Sub-voxel Translucency** (2026-01-03): ✅ COMPLETE

---

*Last Updated: 2026-01-05*
*Plan Version: 2.0 - Building-Focused Game*

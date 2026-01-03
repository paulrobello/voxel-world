# Refactor & Logic Review (2026-01-03)

## Workflow (must follow each batch)
- [x] Run `make checkall`.
- [x] Fix any issues it reports.
- [ ] Have the user verify nothing is broken.
- [x] Update this checklist with outcomes/notes.
- [ ] Commit all work before moving to the next item.

## Waterlogging Implementation Plan ✅ COMPLETE

### 1. Data Structure Updates (`src/chunk.rs`)
- [x] Add `waterlogged: bool` field to `BlockModelData` struct.
- [x] Update `Chunk::set_model_block` and `Chunk::set_model_data` to accept/preserve this flag.
- [x] Update `Chunk::model_metadata_bytes` to pack `waterlogged` status into the Green channel (bit 2, mask 0x04) alongside rotation.

### 2. Water Simulation (`src/water.rs`)
- [x] Update `process_simulation` to handle `BlockType::Model`:
    - If water flows into a model block, set `waterlogged = true` instead of replacing with `BlockType::Water`.
    - If water drains from a model block, set `waterlogged = false`.
    - Ensure `WaterGrid` correctly tracks water mass at these coordinates.
- [x] Update `calculate_flow` to treat waterlogged models as valid flow sources/destinations.
- [x] (Optional) Check `ModelRegistry` to ensure only valid models (fences, slabs) are waterloggable. (Note: Currently all models allow waterlogging for simplicity, matching Minecraft's latest direction).

### 3. Interaction & Logic (`src/block_interaction.rs`)
- [x] Update `handle_block_placement` (or equivalent):
    - If placing Water bucket on a Model block: Set `waterlogged = true` + add to `WaterGrid`.
    - If placing Model block in Water: Set `waterlogged = true` + preserve `WaterGrid` entry.
- [x] Update `break_block`:
    - If breaking a waterlogged model, replace with `BlockType::Water` (preserve water) instead of `BlockType::Air`.

### 4. Rendering (`shaders/traverse.comp`, `shaders/models.glsl`)
- [x] Update `readModelMetadata` helper (or call site) to extract waterlogged bit.
- [x] In `traverse.comp`:
    - If `blockType == BLOCK_MODEL` and `waterlogged`:
        - Trigger water entry logic (`inWater = true`, `waterEntryT`, etc.) if not already in water.
        - Ensure water fog/tint is applied if the ray ends inside the block or passes through without hitting the model.
    - Handle surface hit:
        - If ray hits the water surface *before* the model geometry (e.g. water level is full, model is partial), render water surface?
        - *Simplification:* Render model geometry. If ray misses model, it hits "water" (pass through).
        - Use `waterSurfaceHit` logic combined with model marching.

### 5. Verification
- [x] Test placing fences in water.
- [x] Test pouring water on fences.
- [x] Test breaking waterlogged fences.
- [x] Verify water flow visualization and physics around these blocks.
# Refactor Opportunities

> Keep this report updated as you address items: note status, decisions, and follow-ups so the list stays current.
> After each batch of work: run `make checkall`, resolve any issues it finds, then commit so we can easily roll back and pinpoint regressions.

Last batch: `make checkall` (2026-01-01) — pass.

1. Chunk upload path duplication (high)
   - Locations: `src/world_streaming.rs` lines ~164-212, 314-367, 370-416 all build `(pos, block_data, model_metadata)` vectors, call `upload_chunks_batched`, then mark chunks clean and refresh metadata.
   - Impact: ~120 duplicated lines; risk of behavior drift between paths (e.g., metadata timing, profiling, dirty-marking). Harder to tweak upload logic or add instrumentation once.
   - Direction: Extract a shared helper (e.g., `upload_chunks_with_metadata(&[(Vector3<i32>, &[u8], &[u8])])` or a small `ChunkUploadBatch` owned by `World`) that handles upload, dirty marking, metadata updates, and profiling. Reuse across initial loads, dirty uploads, and unload clears.
   - Status: addressed in `src/world_streaming.rs` via `upload_owned_chunks`, `upload_chunk_refs`, `mark_chunks_clean`, and `update_metadata_buffers`; duplicated upload/metadata blocks replaced. Keep an eye out for future call sites to route through helpers.

2. Descriptor-set and buffer boilerplate (high)
   - Locations: `src/gpu_resources.rs` helpers `get_images_and_sets`, `get_distance_image_and_set`, `get_particle_and_falling_block_set`, `get_light_set`, `get_chunk_metadata_set`, `get_brick_and_model_set`.
   - Impact: Repeated `DescriptorSet::new` and buffer-construction patterns make set-index/layout changes error-prone and keep the file at 1.3k LOC.
   - Direction: Introduce small helpers (e.g., `make_set(pipeline, set_idx, writes)`, `make_storage_buffer<T>(alloc, len)`) to centralize layout lookup and buffer creation, trimming duplication and aligning future descriptor changes.
   - Status: addressed — added `make_set` and `make_storage_buffer` helpers and rewired the noted call sites to use them.

3. HUD render parameter bloat (medium)
   - Location: `HUDRenderer::render` signature (`src/hud_render.rs:15-47`) takes ~30 params; overlay panels duplicate `Area` + `Frame` styling (`52-104`, `106-119`).
   - Impact: Hard to evolve HUD; every new control extends an already unwieldy call site; repeated style code invites divergence.
   - Direction: Wrap inputs in a `HudInputs` struct and split overlays into helpers (e.g., `draw_stats_overlay`, `draw_position_overlay`) sharing style constants.
   - Status: addressed — added `HudInputs` struct plus overlay helpers; render call site updated to pass the struct.

4. Repeated neighbor lists and Y-bounds checks (medium)
   - Locations: identical Y guards in `block_update.rs` at 242, 272, 299, 327; orthogonal neighbor sets duplicated in `block_update::enqueue_neighbors` and `water::activate_neighbors` (`water.rs:273-281`).
   - Impact: Divergent bounds rules or neighbor orders risk subtle bugs; extra code noise.
   - Direction: Add an `ORTHO_DIRS` constant (e.g., in `constants.rs`) and a `WorldBounds::in_y_range(y)` helper; reuse in block updates and water flow.
   - Status: addressed — `constants::ORTHO_DIRS` and `utils::y_in_bounds` added; `block_update` and `water` now reuse them.

5. `main.rs` monolith (medium)
   - Location: `src/main.rs` `App` struct (~160 fields, lines 268-433) and `App::new` (441+).
   - Impact: Hard to navigate and extend; responsibilities for CLI, GPU, world gen, HUD, physics all interleaved; inhibits testing and reset/restart flows.
   - Direction: Split into nested structs (`Graphics`, `WorldSim`, `UiState`, `InputState`) and module-specific `init_*` builders; move constants to modules where used.
   - Status: open.

6. Camera/world conversion duplication (low)
   - Locations: `block_interaction::update_raycast` (`block_interaction.rs:12-35`) and `Player::feet_pos` (`player.rs:97-108`) both scale camera coords by `world_extent` and add `texture_origin`.
   - Impact: If texture-origin math changes, risk inconsistency; minor code bloat.
   - Direction: Add a shared helper (e.g., `Player::camera_world_pos(world_extent, texture_origin)`) or a free utility to centralize conversion.
   - Status: addressed — added `Player::camera_world_pos` and reused in `block_interaction`.

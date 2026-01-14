# Chunk streaming optimization plan (2026-01-14)

- [x] Remove per-frame block clone in metadata refresh by borrowing slices or using scratch buffers in `src/world_streaming.rs` (ChunkWork).
- [x] Avoid Vec allocations on uploads: reuse chunk-owned metadata buffer / pooled scratch for block + model metadata in `src/world_streaming.rs` and `src/gpu_resources.rs`; expose borrow helpers from `src/chunk.rs` if needed.
- [x] Clamp preload requests to a limited Y band around the player (e.g., ±1–2 chunks) while keeping full Y for visible range (`src/world/storage.rs`).
- [x] Unload farthest chunks first when over cap to reduce reload thrash (`src/world_streaming.rs` unload step).
- [x] Drop overflow blocks for out-of-bounds completed chunks after origin shifts (`src/world_streaming.rs` completion loop).
- [x] Add preload backoff when queue is near full (not just full) using queue length (`src/world_streaming.rs`; may expose helper in `src/chunk_loader.rs`).
- [x] Optional profiling log: per-second CSV of queue/FPS/worker stats gated by `--profile` (`src/app/stats.rs` or new helper).
- [x] Prioritized reupload + nearest-first dirty scheduling after origin shift (validated in flight).
- [x] Validate visual stability post-shift (holes/flash); tune per-frame budgets if needed.

## 2026-01-14 follow-up tasks

- [x] Run `cargo check` to surface warnings from new ChunkStats fields and budget helpers.
- [x] Wire origin shift debug info into on-screen stats overlay (B key) so origin offset/shift count visible.
- [x] Ensure ChunkStats origin fields are consumed in stats logging/CSV to avoid dead code warnings.
- [x] Investigate/resolve remaining outer-edge chunk flash (validated: no flashing at high-speed flight).
- [x] Visualize origin-shift window in chunk boundary overlay (magenta threshold lines).
- [x] Persist fly mode in user preferences (defaults to CLI flag, then saved value, else flat-world default).
- [x] Add HUD/CSV telemetry for upload/reupload/metadata budgets and pending queues to aid validation.
- [x] Raise default metadata budget to match upload/reupload (256) to reduce edge flashes after shifts.

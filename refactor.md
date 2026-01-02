# Refactor & Logic Review (2026-01-01)

## Workflow (must follow each batch)
- Run `make checkall`.
- Fix any issues it reports.
- Have the user verify nothing is broken.
- Update this checklist with outcomes/notes.
- Commit all work before moving to the next item.

## Open Findings
- [x] Rust perf: `World::collect_torch_lights` (`src/world.rs`) still walks every voxel of every loaded chunk each frame. Add a chunk-level iterator over `model_data` (or a cached emissive list) and skip chunks where `cached_is_empty` so we only touch blocks that can emit light before hitting `MAX_LIGHTS`. _Done: iterates only model metadata + optional light blocks, skips empty chunks._
- [x] Rust perf: Chunk uploads rebuild `Vec<u8>` buffers via `Chunk::to_block_data()`/`to_model_metadata()` for every dirty chunk in `upload_world_to_gpu` / `upload_all_dirty_chunks` / `check_and_shift_texture_origin` (`src/world_streaming.rs`). Expose a zero-copy `block_bytes()` slice (repr u8) and reuse a per-chunk metadata scratch buffer so uploads can copy directly into the pooled staging buffers without per-upload heap allocations. _Done: chunk exposes zero-copy slices and world streaming reuses them; model metadata cached per chunk._
- [x] Rust perf: `clear_voxel_texture` (`src/world_streaming.rs`) allocates a full `TEXTURE_SIZE_X*TEXTURE_SIZE_Y*TEXTURE_SIZE_Z` zero vector and staging buffer on each origin shift. Swap to `clear_color_image`/`vkCmdClearColorImage` on the 3D texture or keep a persistent zeroed GPU buffer to copy from to avoid large host allocations and transfers. _Done: uses `clear_color_image` for voxel + model metadata textures._
- [x] Rust perf: `update_chunk_metadata` and `update_brick_metadata` (`src/gpu_resources.rs`) allocate fresh `Vec<u32>` every call when streaming chunks. Keep reusable scratch buffers (thread-local or on `App`) and `Vec::clear()` between fills to cut allocator churn during frequent load/unload cycles. _Done: thread-local scratch buffers reused per call._
- [x] DX: `make run -- --args` failed because Makefile didn't forward CLI args. Run with `make run ARGS="..."`; docs updated to match.
- [ ] CPU perf: Chunk/metadata work still ~6 ms/frame at ~1.1–1.7k loaded chunks (profile.csv). Amortize chunk metadata refresh across frames (e.g., slice X/Z ring per frame) to cap per-frame time.
- [ ] CPU perf: Offload chunk metadata + brick SVT generation to a rayon pool; parallelize over loaded chunks instead of single-thread scans.
- [ ] CPU perf: Skip metadata recompute when texture origin is unchanged and no chunks were loaded/unloaded/dirty (early return flag in `update_metadata_buffers`).
- [ ] CPU perf: Consider coarser LOD masks for far chunks to reduce metadata iteration (e.g., mark whole Y stack as empty/solid when all four Y levels match).

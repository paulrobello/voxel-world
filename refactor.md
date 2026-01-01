# Refactor & Logic Review (2026-01-01)

## Workflow (must follow each batch)
- Run `make checkall`.
- Fix any issues it reports.
- Have the user verify nothing is broken.
- Update this checklist with outcomes/notes.
- Commit all work before moving to the next item.

## Open Findings
1) **Texture-origin shift leaves stale metadata**  
   - Location: `src/world_streaming.rs` shift path (lines ~20-93) and metadata refresh (lines ~247-249).  
   - Issue: When the texture origin shifts, voxel/model images are reuploaded but chunk/brick metadata buffers are not rebuilt unless chunks are loaded/unloaded the same frame. Rays can skip or mis-classify chunks/bricks until a later refresh.  
   - Direction: After a shift/reupload, force `update_metadata_buffers()` (or gate on `shifted` flag) to keep skip data aligned.

2) **Duplicate uploads for freshly generated chunks**  
   - Location: `src/world.rs` `insert_chunk` always pushes to `dirty_chunks`; `src/world_streaming.rs` `update_chunk_loading` uploads those chunks immediately and marks them clean. `upload_world_to_gpu` then drains `dirty_chunks` and reuploads the same chunks.  
   - Impact: Wasted GPU bandwidth and inflated profiler counters each time a chunk generation completes.  
   - Direction: Clear or skip these positions after the immediate upload (e.g., drain from dirty queue or track “already uploaded this frame”).

3) **Outdated comment about sub-voxel integration**  
   - Location: Top of `src/sub_voxel.rs`.  
   - Issue: Comment says the module is “under construction” though it’s already used by rendering/interactions.  
   - Direction: Update wording to reflect current integration to avoid future confusion.

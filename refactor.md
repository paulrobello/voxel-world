# Refactor & Logic Review (2026-01-01)

## Workflow (must follow each batch)
- Run `make checkall`.
- Fix any issues it reports.
- Have the user verify nothing is broken.
- Update this checklist with outcomes/notes.
- Commit all work before moving to the next item.

## Open Findings
_(None right now)_ – keep adding here as new issues are found.

## Resolved (2026-01-01)
- **Texture-origin shift metadata gap** — `check_and_shift_texture_origin` now forces `update_metadata_buffers()` after reuploading chunks so skip buffers align immediately.  
- **Duplicate uploads after generation** — Newly uploaded chunks are removed from `dirty_chunks` to avoid a second upload in `upload_world_to_gpu`; added `World::remove_dirty_positions` plus regression test.  
- **Sub-voxel comment accuracy** — Updated module docs to reflect current integration.

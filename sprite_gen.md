# Sprite Icon Generation - Implementation Plan

Goal: add a built‑in, scriptable way for the engine to render block and model sprites (hotbar + palette) with transparent backgrounds in a consistent three‑quarter view, save them under `textures/rendered/`, and fall back to a placeholder when a sprite is missing. Also add a CLI flag to generate the sprites and exit.

## Outputs & Conventions
- Rendered PNGs at 64×64 (configurable constant) with premultiplied alpha preserved.
- Naming: `textures/rendered/block_<block_name>.png` for `BlockType` variants; `textures/rendered/model_<model_id>.png` for sub‑voxel models. Placeholder `textures/rendered/missing.png`.
- Three‑quarter view: camera at (~45° yaw, ~30° pitch) looking toward origin; a slight offset + orthographic projection to avoid perspective distortion on icons.

## Integration Surface
1) **CLI flag**: `--generate-sprites` (short `-g?` unused) triggers sprite generation then clean exit before starting the interactive loop. Flag lives in `Args` (config.rs) and is handled early in `main.rs`.
2) **Sprite loader**: HUD rendering (hotbar + palette) tries to load a per-item icon from `textures/rendered/` (new egui texture) and falls back to atlas UV if missing. Placeholder texture is used when lookup fails.
3) **Generator module**: new Rust module (likely `src/sprite_gen.rs`) encapsulating rendering pipeline setup and save routines; callable both from CLI flag and future tooling.

## Rendering Approach
- Reuse existing Vulkan context setup (device/queue/memory allocators) but create a minimal offscreen render target (RGBA8). No swapchain needed; use a single storage image bound to the existing compute render pipeline.
- Build a tiny “icon scene”: a 3×3×3 world with the target block/model placed at the center. Upload to voxel texture + model buffers using existing helpers (`create_empty_voxel_texture`, `get_brick_and_model_set`, etc.).
- Camera & matrices: construct `pixelToRay` for an orthographic frustum sized to fit a block at the origin; yaw 45°, pitch -30°; position pulled back along the diagonal so the whole block fits.
- Lighting: reuse default lighting constants (ambient + sun) and disable fog/overlays for clean icons. Optionally force AO on, shadows off for crisp silhouettes.
- Background: clear to transparent and ensure shader writes alpha=1 for hit surfaces, 0 for empty; discard sky/fog contribution when no hit.

## Placeholder & Missing Handling
- Generate `textures/rendered/missing.png` (magenta/black checker + transparent holes) if absent.
- Lookup helper returns placeholder when sprite file is missing or fails to load.

## File/Code Changes (planned)
1) `src/config.rs`: add flag `--generate-sprites`.
2) `src/main.rs`: early branch to call generator and exit; wire HUD to optional sprite atlas (egui texture) and per-item UVs from generated PNGs.
3) `src/gpu_resources.rs`: expose minimal helpers to build offscreen render image + save buffer without swapchain; possibly refactor `save_screenshot` to accept raw image data.
4) `src/sprite_gen.rs` (new): orchestrates per-block/model rendering loop, naming, camera setup, directory creation, placeholder generation, saving PNGs.
5) `textures/rendered/` (new dir): populated on generation; add `.gitkeep` optionally.
6) Tests/validation: lightweight smoke test that generator runs and produces `missing.png` + at least one block icon when Vulkan is available (behind `cfg(test)` skip on CI if no GPU).

## Generation Workflow
1) Parse args; if `--generate-sprites` set, build Vulkan context (no window), load built-in model registry.
2) For each `BlockType` (excluding Air) and each registered model id: create scene → render icon → save PNG.
3) Write placeholder first; on errors, log and continue.
4) Exit process with code 0 after generation.

## Risks & Mitigations
- Headless Vulkan availability: detect failure early, print clear error, return non-zero.
- Alpha correctness: ensure render pipeline preserves alpha; clear target to transparent and bypass sky/fog when no hit.
- Performance: small render targets + shared buffers; generation expected sub-second per sprite.

## Stretch (if time)
- Configurable size/angle via env or flags.
- Parallel generation using rayon if GPU/driver allows concurrent queues.

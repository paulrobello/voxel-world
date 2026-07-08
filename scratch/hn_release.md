# Hacker News Release Post

## Title (80 char max)

Show HN: Voxel World – Vulkan ray-marched voxel sandbox in Rust

## Text (for "Show HN" text field)

I've been working on a voxel sandbox game that renders everything through Vulkan compute shaders -- no traditional vertex/fragment pipeline at all. The entire scene is ray marched through a 3D texture. Written in Rust.

Some of the more interesting technical bits:

The renderer keeps a 512x512x512 block volume resident as a 3D texture (16^3 chunks, each 32^3 blocks). Infinite terrain on X/Z works by swapping chunks in and out around the player position. A Sparse Voxel Tree with 64-bit brick masks and per-brick distance fields handles empty space skipping, which makes a huge difference in practice since most of the world is air.

There's a sub-voxel model system where individual blocks can contain 8^3, 16^3, or 32^3 resolution models with their own 32-color palettes and per-voxel emission. This is how torches, fences, doors, stairs, glass panes etc work -- they're all models rendered inside a single block voxel. There are 175 built-in ones and an in-game editor for custom models.

Terrain generation uses 5D climate noise (temperature, humidity, continentalness, erosion, weirdness) driving 17 biome types, 4 cave types, 9 tree species. Water and lava use cellular automata with gravity and spread. Sand and gravel fall. Trees topple when you break the trunk.

Multiplayer uses encrypted UDP via renet, supports up to 4 players with full world sync. Had to build epoch-aware chunk deduplication to avoid re-sending unchanged chunks and LZ4 compression to keep bandwidth sane.

Other stuff: 47 block types, 608 painted variants (any of 19 textures in any of 32 tints), ambient occlusion, directional shadow rays, point lights with 10 animation modes, day/night cycle, animated clouds/stars/water, 20+ building tools, hot-reload GLSL shaders (edit while running, changes apply instantly).

Runs on Linux, macOS, Windows. Quality presets from "potato" up to "ultra".

MIT licensed: https://github.com/paulrobello/voxel-world

Build from source with `git clone && make run`.

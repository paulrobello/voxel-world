# Reddit Release Post

## Title

I built a Vulkan ray-marched voxel sandbox in Rust because I got tired of switching between Minecraft and external tools just to make custom blocks

## Body

My daughter and I have spent countless hours in Minecraft creative mode. Over time we kept reaching for external apps to design custom blocks, models, and textures. It worked, but the context switching killed the flow. At some point I thought -- why isn't all of this just... in the game? An ultimate creative mode where you never have to leave to make something new.

So I built Voxel World.

It's a GPU-accelerated voxel sandbox written in Rust that renders entirely through Vulkan compute shaders. No vertex/fragment pipeline -- everything is ray marched through a 3D texture. I went this route because I wanted to see how far you could push pure compute-based voxel rendering and honestly because it was a fun engineering challenge.

What started as a rendering experiment turned into a pretty full-featured creative sandbox:

**World building tools** -- 20+ tools for cube, sphere, torus, arch, bridge, bezier curves, helix, stairs, terrain brushes, clone stamp, and more. All the stuff we wished Minecraft had built in.

**In-game model editor** -- Sub-voxel models at 8^3, 16^3, or 32^3 resolution with 32-color palettes and per-voxel emission. 175 built-in models (torches, fences, doors, glass panes, etc.) and a full editor for making your own with pencil, fill, mirror, undo/redo. This was the big one for us -- being able to design a model and place it without alt-tabbing.

**Procedural texture generator** -- Design custom block textures in-game with real-time pattern preview. No more exporting to an image editor and hoping the tiling works.

The world itself is procedurally generated with 17 biomes, 4 cave types, 9 tree species, water/lava simulation, and falling block physics. 47 block types with 608 painted variants (any of 19 textures in any of 32 color tints). Day/night cycle, shadow rays, ambient occlusion, animated clouds, stars, water, point lights with animation modes. Quality presets scale from potato to ultra depending on your hardware.

Multiplayer is still very work in progress but getting better. Encrypted UDP, up to 4 players, full world sync. The networking stack has been the hardest part to get right -- epoch-aware chunk dedup, LZ4 compression, handling the host running both server and client. It works but I wouldn't call it battle-tested yet.

Runs on Linux, macOS, and Windows. MIT licensed, fully open source.

Repo: https://github.com/paulrobello/voxel-world

Build from source: `git clone https://github.com/paulrobello/voxel-world.git && cd voxel-world && make run`

If you have any questions about the rendering pipeline, the sub-voxel model system, or the chunk streaming architecture I'm happy to dig into the details. This has been a wild project to work on and I've learned a ton building it.

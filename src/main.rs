//! Voxel Game Engine
//!
//! A Minecraft-like voxel game with GPU ray-marching rendering.

#[cfg(target_os = "macos")]
mod macos_cursor {
    use core_graphics::display::CGAssociateMouseAndMouseCursorPosition;
    use objc2_app_kit::NSCursor;

    /// Grab cursor and hide it using native macOS APIs.
    /// This avoids winit's set_cursor_visible which crashes with SIGBUS.
    pub fn grab_and_hide() {
        unsafe {
            // Disconnect mouse movement from cursor position (0 = false)
            CGAssociateMouseAndMouseCursorPosition(0);
            // Hide the cursor
            NSCursor::hide();
        }
    }

    /// Release cursor and show it using native macOS APIs.
    pub fn release_and_show() {
        unsafe {
            // Reconnect mouse movement to cursor position (1 = true)
            CGAssociateMouseAndMouseCursorPosition(1);
            // Show the cursor
            NSCursor::unhide();
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod macos_cursor {
    pub fn grab_and_hide() {}
    pub fn release_and_show() {}
}

mod app;
mod app_state;
mod atmosphere;
mod block_interaction;
mod block_update;
mod camera;
mod cave_gen;
mod chunk;
mod chunk_loader;
mod config;
mod console;
mod constants;
mod editor;
mod falling_block;
mod gpu_resources;
mod hot_reload;
mod hud;
mod lava;
mod particles;
mod placement;
mod player;
mod raycast;
mod render_mode;
mod shape_tools;
mod sprite_gen;
mod stencils;
mod storage;
mod sub_voxel;
mod svt;
mod templates;
mod terrain_gen;
mod ui;
mod user_prefs;
mod utils;
mod vulkan_context;
mod water;
mod world;
mod world_gen;
mod world_init;
mod world_streaming;

use app::App;
use winit::event_loop::EventLoop;

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new(&event_loop);

    // Print auto-profile info if enabled
    if app.ui.auto_profile_enabled {
        println!("[AUTO-PROFILE] Starting automated feature profiling");
        println!("[AUTO-PROFILE] Sequence: 5s baseline → for each feature: 5s OFF, 5s ON → exit");
        println!("[AUTO-PROFILE] Features: AO, Shadows, ModelShadows, PointLights, Minimap");
        println!("[AUTO-PROFILE] Total duration: ~55 seconds");
    }

    // Upload all initial chunks to GPU before starting the game
    app.upload_all_dirty_chunks();

    event_loop.run_app(&mut app).unwrap();
}

//! Event handling implementation for the application.

use crate::app::core::App;
use crate::config::INITIAL_WINDOW_RESOLUTION;
use crate::gpu_resources::{
    self, get_distance_image_and_set, get_images_and_sets, get_swapchain_images, load_icon,
};
use egui_winit_vulkano::{Gui, GuiConfig};
use std::sync::Arc;
use vulkano::{
    image::{
        sampler::{Filter, SamplerAddressMode, SamplerCreateInfo},
        view::{ImageView, ImageViewCreateInfo},
    },
    swapchain::Surface,
};
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, WindowEvent},
    event_loop::ActiveEventLoop,
    window::{Window, WindowId},
};

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_inner_size(INITIAL_WINDOW_RESOLUTION)
                        .with_window_icon(Some(load_icon(include_bytes!("../../assets/icon.png"))))
                        .with_title("Voxel World"),
                )
                .unwrap(),
        );
        let surface = Surface::from_window(self.graphics.instance.clone(), window.clone()).unwrap();

        let (swapchain, images) = get_swapchain_images(&self.graphics.device, &surface, &window);
        let image_views = images
            .iter()
            .map(|i| ImageView::new(i.clone(), ImageViewCreateInfo::from_image(i)).unwrap())
            .collect::<Vec<_>>();

        let window_extent: [u32; 2] = window.inner_size().into();
        let render_extent = [
            (window_extent[0] as f32 * self.ui.settings.render_scale) as u32,
            (window_extent[1] as f32 * self.ui.settings.render_scale) as u32,
        ];
        let (render_image, render_set, resample_image, resample_set) = get_images_and_sets(
            self.graphics.memory_allocator.clone(),
            self.graphics.descriptor_set_allocator.clone(),
            &self.graphics.render_pipeline,
            &self.graphics.resample_pipeline,
            render_extent,
            window_extent,
            None, // Multiplayer texture array will be wired in Task 12
        );

        // Create distance buffer for two-pass beam optimization
        let (distance_image, distance_set) = get_distance_image_and_set(
            self.graphics.memory_allocator.clone(),
            self.graphics.descriptor_set_allocator.clone(),
            &self.graphics.render_pipeline,
            render_extent,
        );

        let mut gui = Gui::new(
            event_loop,
            surface,
            self.graphics.queue.clone(),
            swapchain.image_format(),
            GuiConfig {
                is_overlay: true,
                ..Default::default()
            },
        );

        // Register the texture atlas with egui for HUD display
        let atlas_texture_id = gui.register_user_image_view(
            self.graphics.texture_atlas_view.clone(),
            SamplerCreateInfo {
                mag_filter: Filter::Nearest,
                min_filter: Filter::Nearest,
                address_mode: [SamplerAddressMode::ClampToEdge; 3],
                ..Default::default()
            },
        );
        let sprite_icons = gpu_resources::load_sprite_icons(&mut gui);

        let recreate_swapchain = false;

        self.graphics.rcx = Some(gpu_resources::RenderContext {
            window,
            swapchain,
            image_views,

            render_image,
            render_set,
            resample_image,
            resample_set,

            distance_image,
            distance_set,

            gui,
            atlas_texture_id,
            sprite_icons,

            picture_atlas: self.graphics.picture_atlas.clone(),
            picture_atlas_view: self.graphics.picture_atlas_view.clone(),

            recreate_swapchain,
        });
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, _cause: winit::event::StartCause) {
        self.input.step();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if !self.graphics.rcx.as_mut().unwrap().gui.update(&event) {
            self.input.process_window_event(&event);
        }

        if event == WindowEvent::RedrawRequested {
            self.render(event_loop);
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        self.input.process_device_event(&event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.input.end_step();
        self.update(event_loop);

        // Apply deferred cursor grab/release using native macOS APIs
        // winit's set_cursor_grab and set_cursor_visible cause SIGBUS on macOS
        if let Some(grab) = self.input.pending_grab.take() {
            if grab {
                crate::macos_cursor::grab_and_hide();
                println!("Cursor grabbed and hidden (native macOS API)");
            } else {
                crate::macos_cursor::release_and_show();
                println!("Cursor released and shown (native macOS API)");
            }
        }

        let rcx = self.graphics.rcx.as_mut().unwrap();
        rcx.window.request_redraw();
    }
}

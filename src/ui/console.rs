//! Console UI rendering.

use super::FluidStats;
use crate::console::ConsoleState;
use egui_winit_vulkano::egui;
use nalgebra::Vector3;

pub struct ConsoleUI;

impl ConsoleUI {
    /// Draw the command console UI.
    pub fn draw_console(
        ctx: &egui::Context,
        console: &mut ConsoleState,
        world: &mut crate::world::World,
        player_world_pos: Vector3<f64>,
        fluid_stats: FluidStats,
    ) {
        if !console.active {
            return;
        }

        let screen_rect = ctx.screen_rect();
        let console_height = screen_rect.height() * 0.6;
        let console_width = screen_rect.width().min(800.0);

        // Position at bottom center of screen
        let console_pos = egui::pos2(
            (screen_rect.width() - console_width) / 2.0,
            screen_rect.height() - console_height - 10.0,
        );

        egui::Window::new("Console")
            .fixed_pos(console_pos)
            .fixed_size([console_width, console_height])
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(egui::Color32::from_rgba_unmultiplied(20, 20, 20, 230)),
            )
            .show(ctx, |ui| {
                // Output history with scroll - use full width
                let output_height = console_height - 40.0;
                let available_width = ui.available_width();
                egui::ScrollArea::vertical()
                    .max_height(output_height)
                    .stick_to_bottom(true)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_min_width(available_width);
                        for entry in &console.output {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(entry.text())
                                        .color(entry.color())
                                        .monospace(),
                                )
                                .wrap_mode(egui::TextWrapMode::Wrap),
                            );
                        }
                    });

                ui.separator();

                // Input field
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(">")
                            .monospace()
                            .color(egui::Color32::from_rgb(100, 255, 100)),
                    );

                    let response = ui.add(
                        egui::TextEdit::singleline(&mut console.input)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(console_width - 30.0)
                            .frame(false)
                            .hint_text("Type a command... (help for list)"),
                    );

                    // Request focus if needed
                    if console.request_focus {
                        response.request_focus();
                        console.request_focus = false;
                    }

                    // Handle keyboard input
                    if response.lost_focus() {
                        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            // Submit command
                            let player_pos = Vector3::new(
                                player_world_pos.x.floor() as i32,
                                player_world_pos.y.floor() as i32,
                                player_world_pos.z.floor() as i32,
                            );
                            console.submit(world, player_pos);
                            // Handle pending fluid debug output
                            if console.pending_fluid_debug {
                                console.output_fluid_debug(
                                    fluid_stats.water_cells,
                                    fluid_stats.water_active,
                                    fluid_stats.lava_cells,
                                    fluid_stats.lava_active,
                                );
                            }
                            // Re-focus the input
                            console.request_focus = true;
                        } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            // Close console
                            console.close();
                        }
                    }

                    // History navigation (check while focused)
                    if response.has_focus() {
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                            console.history_up();
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                            console.history_down();
                        }
                    }
                });
            });
    }
}

//! Console UI rendering.

use super::FluidStats;
use crate::console::ConsoleState;
use crate::templates::TemplatePlacement;
use egui_winit_vulkano::egui;
use nalgebra::Vector3;

pub struct ConsoleUI;

impl ConsoleUI {
    /// Draw the command console UI.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_console(
        ctx: &egui::Context,
        console: &mut ConsoleState,
        world: &mut crate::world::World,
        player_world_pos: Vector3<f64>,
        fluid_stats: FluidStats,
        template_selection: &mut crate::templates::TemplateSelection,
        template_library: &crate::templates::TemplateLibrary,
        water_grid: &crate::water::WaterGrid,
        active_placement: &mut Option<TemplatePlacement>,
        terrain_generator: &crate::terrain_gen::TerrainGenerator,
    ) {
        if !console.active {
            // Still update pending searches even when console is closed
            if let Some(mut search) = console.pending_locate_search.take() {
                if let Some(result) = crate::console::commands::update_locate_search(
                    &mut search,
                    world,
                    terrain_generator,
                ) {
                    // Search completed, handle result
                    console.handle_result(result);
                } else {
                    // Still searching, keep it for next frame
                    // Show progress update every 1000 positions
                    if search.positions_checked % 1000 == 0 {
                        let search_name = match &search.search_type {
                            crate::console::LocateSearchType::Biome(biome) => {
                                format!("{:?} biome", biome)
                            }
                            crate::console::LocateSearchType::Block(block) => {
                                format!("{:?} block", block)
                            }
                            crate::console::LocateSearchType::Cave(size) => {
                                format!("cave (min {} blocks)", size)
                            }
                        };
                        console.info(format!(
                            "Searching for {}... ({} positions checked)",
                            search_name, search.positions_checked
                        ));
                    }
                    console.pending_locate_search = Some(search);
                }
            }
            return;
        }

        // Update pending locate search if active
        if let Some(mut search) = console.pending_locate_search.take() {
            if let Some(result) = crate::console::commands::update_locate_search(
                &mut search,
                world,
                terrain_generator,
            ) {
                // Search completed, handle result
                console.handle_result(result);
            } else {
                // Still searching, keep it for next frame
                // Show progress update every 1000 positions
                if search.positions_checked % 1000 == 0 {
                    let search_name = match &search.search_type {
                        crate::console::LocateSearchType::Biome(biome) => {
                            format!("{:?} biome", biome)
                        }
                        crate::console::LocateSearchType::Block(block) => {
                            format!("{:?} block", block)
                        }
                        crate::console::LocateSearchType::Cave(size) => {
                            format!("cave (min {} blocks)", size)
                        }
                    };
                    console.info(format!(
                        "Searching for {}... ({} positions checked)",
                        search_name, search.positions_checked
                    ));
                }
                console.pending_locate_search = Some(search);
            }
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

                // Check for Tab key BEFORE creating input to consume it early
                let tab_pressed = ctx.input(|i| i.key_pressed(egui::Key::Tab));
                let shift_held = ctx.input(|i| i.modifiers.shift);

                // Input field with ghost text overlay
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(">")
                            .monospace()
                            .color(egui::Color32::from_rgb(100, 255, 100)),
                    );

                    // Input field
                    let input_width = console_width - 30.0;
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut console.input)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(input_width)
                            .frame(false)
                            .hint_text("Type a command... (help for list)")
                            .lock_focus(true),
                    );

                    // Move cursor to end if requested
                    if console.move_cursor_to_end {
                        if let Some(mut state) = egui::TextEdit::load_state(ctx, response.id) {
                            let cursor_pos = console.input.len();
                            let ccursor = egui::text::CCursor::new(cursor_pos);
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::one(ccursor)));
                            state.store(ctx, response.id);
                        }
                        console.move_cursor_to_end = false;
                    }

                    // Draw ghost text overlay if we have one and no suggestions popup
                    if console.suggestions.is_empty() && !console.input.is_empty() {
                        let ghost_text = console.get_ghost_text();
                        if !ghost_text.is_empty() {
                            let input_rect = response.rect;
                            let font_id = egui::FontId::monospace(13.0);

                            // Calculate width of current input text
                            let input_text_width = ui.fonts(|f| {
                                f.layout_no_wrap(
                                    console.input.clone(),
                                    font_id.clone(),
                                    egui::Color32::WHITE,
                                )
                                .size()
                                .x
                            });

                            let ghost_start_pos = egui::pos2(
                                input_rect.min.x + input_text_width + 4.0,
                                input_rect.min.y + 2.0,
                            );

                            ui.painter().text(
                                ghost_start_pos,
                                egui::Align2::LEFT_TOP,
                                format!(" {}", ghost_text),
                                font_id,
                                egui::Color32::from_rgba_unmultiplied(100, 100, 100, 180),
                            );
                        }
                    }

                    // Request focus if needed
                    if console.request_focus {
                        response.request_focus();
                        console.request_focus = false;
                    }

                    // Handle keyboard input
                    if response.lost_focus() {
                        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            // Submit command
                            // Note: Y is +1 so ~ ~ ~ is one block above the ground you're standing on
                            let player_pos = Vector3::new(
                                player_world_pos.x.floor() as i32,
                                player_world_pos.y.floor() as i32 + 1,
                                player_world_pos.z.floor() as i32,
                            );
                            console.submit(
                                world,
                                player_pos,
                                template_selection,
                                template_library,
                                water_grid,
                                terrain_generator,
                            );

                            // Handle pending template load
                            if let Some(template) = console.pending_template_load.take() {
                                let placement_pos = Vector3::new(
                                    player_world_pos.x.floor() as i32,
                                    (player_world_pos.y - 1.0).floor() as i32,
                                    player_world_pos.z.floor() as i32,
                                );
                                *active_placement =
                                    Some(TemplatePlacement::new(template, placement_pos));
                            }

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

                    // Handle autocomplete and history navigation (check while focused)
                    if response.has_focus() {
                        // Handle Tab for autocomplete
                        if tab_pressed {
                            if console.suggestions.is_empty() {
                                // Generate suggestions
                                console.update_autocomplete();
                            } else {
                                // Cycle through suggestions
                                if shift_held {
                                    console.prev_suggestion();
                                } else {
                                    console.next_suggestion();
                                }
                            }
                            // Apply if we have exactly one suggestion or user is cycling
                            if console.suggestions.len() == 1 || console.suggestion_index > 0 {
                                console.apply_suggestion();
                            }
                        }

                        // Update autocomplete on text change
                        if response.changed() {
                            console.update_autocomplete();
                        }

                        // History navigation
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                            console.history_up();
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                            console.history_down();
                        }
                    }
                });

                // Show suggestions popup if available
                if !console.suggestions.is_empty() {
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new("Suggestions:")
                                .color(egui::Color32::from_gray(180))
                                .monospace(),
                        );
                        for (idx, suggestion) in console.suggestions.iter().enumerate() {
                            let is_selected = idx == console.suggestion_index;
                            let color = if is_selected {
                                egui::Color32::from_rgb(100, 255, 100)
                            } else {
                                egui::Color32::from_gray(200)
                            };
                            let text = if is_selected {
                                format!("[{}]", suggestion)
                            } else {
                                suggestion.clone()
                            };
                            ui.label(egui::RichText::new(text).color(color).monospace());
                        }
                    });
                }
            });
    }
}

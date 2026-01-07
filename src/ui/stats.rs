//! Performance stats and debug overlays.

use super::FluidStats;
use super::helpers::HudHelpers;
use crate::terrain_gen::TerrainGenerator;
use crate::utils::ChunkStats;
use egui_winit_vulkano::egui;
use nalgebra::Vector3;

pub struct StatsUI;

impl StatsUI {
    pub fn draw_stats_overlay(
        ctx: &egui::Context,
        fps: u32,
        chunk_stats: &ChunkStats,
        fluid_stats: FluidStats,
    ) {
        egui::Area::new(egui::Id::new("fps_overlay"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 10.0))
            .show(ctx, |ui| {
                HudHelpers::overlay_frame().show(ui, |ui| {
                    ui.set_min_width(150.0);
                    ui.label(
                        egui::RichText::new(format!("FPS: {}", fps))
                            .color(egui::Color32::WHITE)
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new(format!("Chunks: {}", chunk_stats.loaded_count))
                            .color(egui::Color32::LIGHT_GRAY)
                            .small(),
                    );
                    if chunk_stats.dirty_count > 0 {
                        ui.label(
                            egui::RichText::new(format!("Dirty: {}", chunk_stats.dirty_count))
                                .color(egui::Color32::YELLOW)
                                .small(),
                        );
                    }
                    if chunk_stats.in_flight_count > 0 {
                        ui.label(
                            egui::RichText::new(format!(
                                "Generating: {}",
                                chunk_stats.in_flight_count
                            ))
                            .color(egui::Color32::LIGHT_GREEN)
                            .small(),
                        );
                    }
                    ui.label(
                        egui::RichText::new(format!("GPU: {:.1} MB", chunk_stats.memory_mb))
                            .color(egui::Color32::LIGHT_GRAY)
                            .small(),
                    );
                    // Show fluid stats when there are active fluid cells
                    if fluid_stats.water_cells > 0 || fluid_stats.lava_cells > 0 {
                        ui.separator();
                        if fluid_stats.water_cells > 0 {
                            let water_color = egui::Color32::from_rgb(64, 164, 223);
                            ui.label(
                                egui::RichText::new(format!(
                                    "Water: {} ({} active)",
                                    fluid_stats.water_cells, fluid_stats.water_active
                                ))
                                .color(water_color)
                                .small(),
                            );
                        }
                        if fluid_stats.lava_cells > 0 {
                            let lava_color = egui::Color32::from_rgb(255, 100, 50);
                            ui.label(
                                egui::RichText::new(format!(
                                    "Lava: {} ({} active)",
                                    fluid_stats.lava_cells, fluid_stats.lava_active
                                ))
                                .color(lava_color)
                                .small(),
                            );
                        }
                    }
                });
            });
    }

    pub fn draw_position_overlay(ctx: &egui::Context, player_world_pos: Vector3<f64>) {
        egui::Area::new(egui::Id::new("position_overlay"))
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 10.0))
            .show(ctx, |ui| {
                HudHelpers::overlay_frame()
                    .inner_margin(egui::Margin::symmetric(12, 6))
                    .show(ui, |ui| {
                        let pos_text = format!(
                            "Pos: {:.1}, {:.1}, {:.1}",
                            player_world_pos.x, player_world_pos.y, player_world_pos.z
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(pos_text)
                                    .color(egui::Color32::WHITE)
                                    .strong()
                                    .monospace(),
                            )
                            .wrap_mode(egui::TextWrapMode::Extend),
                        );
                    });
            });
    }

    pub fn draw_biome_debug_overlay(
        ctx: &egui::Context,
        terrain_generator: &TerrainGenerator,
        player_world_pos: Vector3<f64>,
    ) {
        let x = player_world_pos.x.floor() as i32;
        let z = player_world_pos.z.floor() as i32;
        let info = terrain_generator.get_biome_info(x, z);

        egui::Area::new(egui::Id::new("biome_debug_overlay"))
            .anchor(egui::Align2::RIGHT_CENTER, egui::vec2(-10.0, 0.0))
            .show(ctx, |ui| {
                HudHelpers::overlay_frame().show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("Biome Debug")
                            .strong()
                            .color(egui::Color32::WHITE),
                    );
                    ui.separator();
                    ui.label(format!("Biome: {:?}", info.biome));
                    ui.label(format!("Elevation: {:.3}", info.elevation));
                    ui.label(format!("Temperature: {:.3}", info.temperature));
                    ui.label(format!("Rainfall: {:.3}", info.rainfall));
                });
            });
    }
}

use crate::{UiState, WorldSim};
use std::fs::OpenOptions;
use std::io::Write;

pub fn print_stats(ui: &mut UiState, sim: &mut WorldSim, verbose: bool) {
    let player_pos = sim.player.feet_pos(sim.world_extent, sim.texture_origin);
    let player_chunk = sim
        .player
        .get_chunk_pos(sim.world_extent, sim.texture_origin);
    let frame_time_ms = if ui.fps > 0 {
        1000.0 / ui.fps as f32
    } else {
        0.0
    };

    let render_res = [
        (ui.window_size[0] as f32 * ui.settings.render_scale) as u32,
        (ui.window_size[1] as f32 * ui.settings.render_scale) as u32,
    ];
    if verbose {
        println!(
            "[STATS] FPS: {} ({:.1}ms) | Win: {}x{} Render: {}x{} | Chunks: {} | Dirty: {} | Gen: {} | Pos: ({:.1}, {:.1}, {:.1}) | Chunk: ({}, {}, {}) | TexOrigin: ({}, {})",
            ui.fps,
            frame_time_ms,
            ui.window_size[0],
            ui.window_size[1],
            render_res[0],
            render_res[1],
            sim.chunk_stats.loaded_count,
            sim.chunk_stats.dirty_count,
            sim.chunk_loader.in_flight_count(),
            player_pos.x,
            player_pos.y,
            player_pos.z,
            player_chunk.x,
            player_chunk.y,
            player_chunk.z,
            sim.texture_origin.x,
            sim.texture_origin.z,
        );
    } else {
        println!(
            "[STATS] FPS: {} ({:.1}ms) | Win: {}x{} Render: {}x{} | Chunks: {} | Gen: {} | Pos: ({:.1}, {:.1}, {:.1})",
            ui.fps,
            frame_time_ms,
            ui.window_size[0],
            ui.window_size[1],
            render_res[0],
            render_res[1],
            sim.chunk_stats.loaded_count,
            sim.chunk_loader.in_flight_count(),
            player_pos.x,
            player_pos.y,
            player_pos.z,
        );
    }

    // Persist CSV sample if requested
    if let Some(path) = &ui.profile_log_path {
        let n = sim.profiler.sample_count as f64;
        let (chunkload_ms, upload_ms, metadata_ms, render_ms, chunks_uploaded) = if n > 0.0 {
            (
                sim.profiler.chunk_loading_us as f64 / 1000.0 / n,
                sim.profiler.gpu_upload_us as f64 / 1000.0 / n,
                sim.profiler.metadata_update_us as f64 / 1000.0 / n,
                sim.profiler.render_us as f64 / 1000.0 / n,
                sim.profiler.chunks_uploaded,
            )
        } else {
            (0.0, 0.0, 0.0, 0.0, 0)
        };

        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            if !ui.profile_log_header_written {
                // Only write the header if the file is empty
                if file.metadata().map(|m| m.len()).unwrap_or(1) == 0 {
                    let _ = writeln!(
                        file,
                        "time_s,fps,frame_ms,win_w,win_h,render_w,render_h,chunks_loaded,chunks_dirty,chunks_inflight,pos_x,pos_y,pos_z,chunk_x,chunk_y,chunk_z,tex_x,tex_z,chunkload_ms,upload_ms,chunks_uploaded,metadata_ms,render_ms"
                    );
                }
                ui.profile_log_header_written = true;
            }

            let elapsed = ui.start_time.elapsed().as_secs_f64();
            let _ = writeln!(
                file,
                "{:.3},{},{:.3},{},{},{},{},{},{},{:.1},{:.1},{:.1},{},{},{},{},{},{:.3},{:.3},{},{:.3},{:.3},{:.3}",
                elapsed,
                ui.fps,
                frame_time_ms,
                ui.window_size[0],
                ui.window_size[1],
                render_res[0],
                render_res[1],
                sim.chunk_stats.loaded_count,
                sim.chunk_stats.dirty_count,
                sim.chunk_loader.in_flight_count(),
                player_pos.x,
                player_pos.y,
                player_pos.z,
                player_chunk.x,
                player_chunk.y,
                player_chunk.z,
                sim.texture_origin.x,
                sim.texture_origin.z,
                chunkload_ms,
                upload_ms,
                chunks_uploaded,
                metadata_ms,
                render_ms
            );
        }
    }

    sim.profiler.print_stats();
    sim.profiler.reset();
}

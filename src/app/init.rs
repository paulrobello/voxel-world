//! App initialization

use super::App;
use crate::app_state::{AutoProfileFeature, Graphics, InputState, PaletteTab, UiState, WorldSim};
use crate::atmosphere;
use crate::block_update::BlockUpdateQueue;
use crate::chunk::CHUNK_SIZE;
use crate::chunk_loader::ChunkLoader;
use crate::config::{Args, INITIAL_WINDOW_RESOLUTION};
use crate::console::ConsoleState;
use crate::constants::{
    DEFAULT_TIME_OF_DAY, LOAD_DISTANCE, LOADED_CHUNKS_X, LOADED_CHUNKS_Z, TEXTURE_SIZE_X,
    TEXTURE_SIZE_Y, TEXTURE_SIZE_Z, UNLOAD_DISTANCE, VIEW_DISTANCE,
};
use crate::editor::EditorState;
use crate::falling_block::FallingBlockSystem;
use crate::gpu_resources::{
    create_empty_voxel_texture, get_brick_and_model_set, get_chunk_metadata_set, get_light_set,
    get_particle_and_falling_block_set, load_texture_atlas,
};
use crate::hot_reload::HotReloadComputePipeline;
use crate::hud::Minimap;
use crate::lava::LavaGrid;
use crate::particles::ParticleSystem;
use crate::player::{PLAYER_EYE_HEIGHT, Player};
use crate::render_mode::RenderMode;
use crate::sprite_gen;
use crate::storage;
use crate::sub_voxel::ModelRegistry;
use crate::templates::{TemplateLibrary, TemplateSelection, TemplateUi};
use crate::terrain_gen::{TerrainGenerator, generate_chunk_terrain};
use crate::user_prefs::{
    UserPreferences, profiles_dir, set_data_dir, user_models_dir, user_stencils_dir,
    user_templates_dir, worlds_dir,
};
use crate::utils::{ChunkStats, Profiler};
use crate::vulkan_context::VulkanContext;
use crate::water::WaterGrid;
use crate::world_init::{create_initial_world_with_seed, find_ground_level, find_safe_spawn};
use crate::world_streaming::MetadataState;
use clap::Parser;
use nalgebra::{Vector3, vector};
use std::path::PathBuf;
use std::process;
use std::sync::Arc;
use std::time::Instant;
use winit::event_loop::EventLoop;
use winit_input_helper::WinitInputHelper;

impl App {
    pub fn new(event_loop: &EventLoop<()>) -> Self {
        // Parse command line arguments
        let args = Args::parse();

        if args.verbose {
            println!("CLI Args: {:?}", args);
        }

        if args.generate_sprites {
            match sprite_gen::run(&args, event_loop) {
                Ok(()) => process::exit(0),
                Err(e) => {
                    eprintln!("[sprites] failed: {e}");
                    process::exit(1);
                }
            }
        }

        // Set data directory if specified (must happen before any data access)
        if let Some(ref data_dir) = args.data_dir {
            let path = PathBuf::from(data_dir);
            if !path.exists() {
                std::fs::create_dir_all(&path).expect("Failed to create data directory");
            }
            set_data_dir(&path);
            println!("[Launcher] Using data directory: {}", path.display());
        }

        // Load user preferences from disk (needed for world selection and player data)
        let mut prefs = UserPreferences::load();

        // Determine world name
        let world_name = args
            .world
            .clone()
            .or(prefs.last_world.clone())
            .unwrap_or_else(|| "default".to_string());

        println!("[Launcher] Loading world: '{}'", world_name);
        prefs.update_last_world(&world_name);

        let worlds_directory = worlds_dir();
        let world_dir = worlds_directory.join(&world_name);

        // Migration: If 'world' exists but 'worlds/default' doesn't, migrate it
        if PathBuf::from("world").exists() && !world_dir.exists() && world_name == "default" {
            println!("[Launcher] Migrating legacy world to 'worlds/default'...");
            if !worlds_directory.exists() {
                std::fs::create_dir_all(&worlds_directory)
                    .expect("Failed to create worlds directory");
            }
            std::fs::rename("world", &world_dir).expect("Failed to migrate legacy world");
        }

        if !world_dir.exists() {
            std::fs::create_dir_all(&world_dir).expect("Failed to create world directory");
        }

        let metadata_path = world_dir.join("level.dat");
        let mut seed = args.seed.unwrap_or(98765);
        let mut initial_time_of_day = DEFAULT_TIME_OF_DAY;
        let mut initial_day_paused = true; // Default
        let mut world_gen = args.world_gen; // Default to CLI arg
        let mut initial_measurement_markers: Vec<Vector3<i32>> = Vec::new();

        if metadata_path.exists() {
            if let Ok(meta) = storage::metadata::WorldMetadata::load(&metadata_path) {
                println!(
                    "[Storage] Loaded world metadata. Seed: {}, WorldGen: {:?}",
                    meta.seed, meta.world_gen
                );
                seed = meta.seed;
                initial_time_of_day = meta.time_of_day;
                initial_day_paused = meta.day_cycle_paused;
                // Use persisted world_gen, not CLI arg (existing world takes precedence)
                world_gen = meta.world_gen;
                // Load measurement markers
                initial_measurement_markers = meta
                    .measurement_markers
                    .iter()
                    .map(|&[x, y, z]| Vector3::new(x, y, z))
                    .collect();
                if !initial_measurement_markers.is_empty() {
                    println!(
                        "[Storage] Loaded {} measurement markers",
                        initial_measurement_markers.len()
                    );
                }
            }
        } else {
            let meta = storage::metadata::WorldMetadata {
                seed,
                spawn_pos: [0.0, 64.0, 0.0], // Initial guess, will be updated
                version: 1,
                time_of_day: DEFAULT_TIME_OF_DAY,
                day_cycle_paused: true,
                world_gen,
                measurement_markers: Vec::new(),
            };
            let _ = meta.save(&metadata_path);
            println!(
                "[Storage] Created new world metadata. Seed: {}, WorldGen: {:?}",
                seed, world_gen
            );
        }

        // Load player data from user preferences (per-world)
        let initial_player_data = prefs.get_player_data(&world_name).cloned();

        let view_distance = args.view_distance.unwrap_or(VIEW_DISTANCE);
        let load_distance = LOAD_DISTANCE;
        let unload_distance = UNLOAD_DISTANCE;

        let vk = VulkanContext::new(event_loop);

        let shaders_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("shaders");
        let render_pipeline =
            HotReloadComputePipeline::new(vk.device.clone(), &shaders_dir.join("traverse.comp"));
        let resample_pipeline =
            HotReloadComputePipeline::new(vk.device.clone(), &shaders_dir.join("resample.comp"));

        // Calculate spawn chunk based on CLI args (or default to origin)
        let spawn_block_x = args.spawn_x.unwrap_or(0);
        let spawn_block_z = args.spawn_z.unwrap_or(0);

        // Texture origin: the world position that maps to texture coordinate (0,0,0)
        // For infinite worlds, center the texture on the spawn chunk
        let spawn_chunk_x = spawn_block_x.div_euclid(CHUNK_SIZE as i32);
        let spawn_chunk_z = spawn_block_z.div_euclid(CHUNK_SIZE as i32);

        let texture_origin = Vector3::new(
            (spawn_chunk_x - LOADED_CHUNKS_X / 2) * CHUNK_SIZE as i32,
            0, // Y always starts at 0
            (spawn_chunk_z - LOADED_CHUNKS_Z / 2) * CHUNK_SIZE as i32,
        );

        // Initialize world
        let spawn_chunk = vector![spawn_chunk_x, 0, spawn_chunk_z];

        let storage = Arc::new(storage::worker::StorageSystem::new(world_dir.clone()));

        // Create world with only chunks near spawn loaded, checking storage first
        let world = create_initial_world_with_seed(spawn_chunk, seed, world_gen, Some(&storage));

        // Texture dimensions (not world bounds - world is infinite)
        let world_extent = [
            TEXTURE_SIZE_X as u32,
            TEXTURE_SIZE_Y as u32,
            TEXTURE_SIZE_Z as u32,
        ];

        // Create empty GPU texture (chunks will be uploaded by update_chunk_loading)
        let (voxel_set, voxel_image) = create_empty_voxel_texture(
            vk.memory_allocator.clone(),
            vk.command_buffer_allocator.clone(),
            vk.descriptor_set_allocator.clone(),
            &render_pipeline,
            &vk.queue,
            world_extent,
        );

        // Load texture atlas
        let texture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("textures")
            .join("texture_atlas.png");
        let (texture_set, _sampler, texture_atlas_view) = load_texture_atlas(
            vk.memory_allocator.clone(),
            vk.command_buffer_allocator.clone(),
            vk.descriptor_set_allocator.clone(),
            &render_pipeline,
            &vk.queue,
            &texture_path,
        );

        // Create particle, falling block, water source, template block, and stencil block buffers (share set 3)
        let (
            particle_buffer,
            falling_block_buffer,
            water_source_buffer,
            template_block_buffer,
            stencil_block_buffer,
            particle_set,
        ) = get_particle_and_falling_block_set(
            vk.memory_allocator.clone(),
            vk.descriptor_set_allocator.clone(),
            &render_pipeline,
        );

        // Create light buffer and descriptor set
        let (light_buffer, light_set) = get_light_set(
            vk.memory_allocator.clone(),
            vk.descriptor_set_allocator.clone(),
            &render_pipeline,
        );

        // Create chunk metadata buffer and descriptor set
        let (chunk_metadata_buffer, chunk_metadata_set) = get_chunk_metadata_set(
            vk.memory_allocator.clone(),
            vk.descriptor_set_allocator.clone(),
            &render_pipeline,
        );

        // Create model registry with built-in models and load library models
        let mut model_registry = ModelRegistry::new();
        let library_path = user_models_dir();
        match model_registry.load_library_models(&library_path) {
            Ok(count) if count > 0 => {
                println!("Loaded {} custom models from library", count);
            }
            Err(e) => {
                eprintln!("Warning: Failed to load library models: {}", e);
            }
            _ => {}
        }

        // Create combined brick metadata and model resources (set 7)
        let (
            brick_mask_buffer,
            brick_dist_buffer,
            model_atlas_8,
            model_atlas_16,
            model_atlas_32,
            model_palettes,
            model_palette_emission,
            model_metadata,
            model_properties_buffer,
            brick_and_model_set,
        ) = get_brick_and_model_set(
            vk.memory_allocator.clone(),
            vk.command_buffer_allocator.clone(),
            vk.descriptor_set_allocator.clone(),
            &render_pipeline,
            &vk.queue,
            world_extent,
            &model_registry,
        );

        let input = WinitInputHelper::new();

        // Spawn at safe location (avoiding water/rivers)
        let spawn_pos = if let Some(ref player_data) = initial_player_data {
            Vector3::new(
                player_data.position[0],
                player_data.position[1] + PLAYER_EYE_HEIGHT,
                player_data.position[2],
            )
        } else {
            // If explicit spawn coords provided, use them; otherwise find safe spawn
            let (safe_x, safe_z) = if args.spawn_x.is_some() || args.spawn_z.is_some() {
                (spawn_block_x, spawn_block_z)
            } else {
                // Search for safe spawn (dry land) within 64 blocks of origin
                let (x, z) = find_safe_spawn(&world, spawn_block_x, spawn_block_z, 64);
                if x != spawn_block_x || z != spawn_block_z {
                    println!(
                        "[SPAWN] Found safe spawn at ({}, {}) instead of origin",
                        x, z
                    );
                }
                (x, z)
            };
            let spawn_y = find_ground_level(&world, safe_x, safe_z);
            Vector3::new(safe_x as f64, spawn_y as f64 + 1.0, safe_z as f64)
        };

        let mut player = Player::new(spawn_pos, texture_origin, world_extent, args.fly_mode);
        player.auto_jump = true;

        // Restore rotation if available
        if let Some(ref p) = initial_player_data {
            player.camera.rotation.y = p.yaw as f64;
            player.camera.rotation.x = p.pitch as f64;
        }

        println!(
            "Voxel Game started! Click to focus, then use WASD to move, mouse to look, left/right click to edit blocks."
        );

        let graphics = Graphics {
            instance: vk.instance,
            device: vk.device,
            queue: vk.queue,
            memory_allocator: vk.memory_allocator,
            descriptor_set_allocator: vk.descriptor_set_allocator,
            command_buffer_allocator: vk.command_buffer_allocator,
            render_pipeline,
            resample_pipeline,
            voxel_set,
            texture_set,
            texture_atlas_view,
            particle_buffer,
            particle_set,
            light_buffer,
            light_set,
            chunk_metadata_buffer,
            chunk_metadata_set,
            brick_mask_buffer,
            brick_dist_buffer,
            brick_and_model_set,
            falling_block_buffer,
            water_source_buffer,
            template_block_buffer,
            stencil_block_buffer,
            voxel_image,
            model_atlas_8,
            model_atlas_16,
            model_atlas_32,
            model_palettes,
            model_palette_emission,
            model_metadata,
            model_properties_buffer,
            rcx: None,
        };

        let terrain_generator = TerrainGenerator::new(seed);

        let mut sim = WorldSim {
            world,
            model_registry,
            terrain_generator: terrain_generator.clone(),
            player,
            world_extent,
            texture_origin,
            last_player_chunk: spawn_chunk,
            chunk_stats: ChunkStats::default(),
            chunk_loader: {
                let terrain = terrain_generator.clone();
                ChunkLoader::new(
                    move |pos| {
                        // Generate chunk with overflow blocks for cross-chunk structures
                        generate_chunk_terrain(&terrain, pos, world_gen)
                    },
                    Some(world_dir.clone()),
                )
            },
            storage,
            particles: ParticleSystem::new(),
            falling_blocks: FallingBlockSystem::new(),
            block_updates: BlockUpdateQueue::new(32),
            water_grid: WaterGrid::new(),
            lava_grid: LavaGrid::new(),
            time_of_day: if args.time_of_day.is_some() {
                args.time_of_day.map(|t| t as f32).unwrap()
            } else {
                initial_time_of_day
            },
            day_cycle_paused: initial_day_paused,
            atmosphere: atmosphere::AtmosphereSettings::default(),
            animation_time: 0.0,
            render_mode: match args.render_mode.as_deref() {
                Some("normal") => RenderMode::Normal,
                Some("coord") => RenderMode::Coord,
                Some("steps") => RenderMode::Steps,
                Some("uv") => RenderMode::UV,
                Some("depth") => RenderMode::Depth,
                _ => RenderMode::Textured,
            },
            view_distance,
            load_distance,
            unload_distance,
            profiler: Profiler::default(),
            metadata_state: MetadataState::new(texture_origin),
            last_save: Instant::now(),
            world_dir: world_dir.clone(),
            world_name: world_name.clone(),
            seed,
            world_gen,
        };

        // Load fluid sources (water/lava sources that were saved)
        let fluid_sources = storage::fluid_sources::FluidSources::load(&world_dir);
        if !fluid_sources.water.is_empty() || !fluid_sources.lava.is_empty() {
            println!(
                "[Storage] Loaded {} fluid sources ({} water, {} lava)",
                fluid_sources.water.len() + fluid_sources.lava.len(),
                fluid_sources.water.len(),
                fluid_sources.lava.len()
            );
            // Load water sources into grid
            sim.water_grid
                .load_sources(&fluid_sources.water, &mut sim.world);
            // Load lava sources into grid
            sim.lava_grid
                .load_sources(&fluid_sources.lava, &mut sim.world);
        }

        // Load stencil state (active stencils in world)
        let stencil_state = storage::stencil_state::StencilState::load(&world_dir);
        let mut stencil_manager = crate::stencils::StencilManager::new();
        stencil_state.apply_to_manager(&mut stencil_manager);
        if !stencil_manager.active_stencils.is_empty() {
            println!(
                "[Storage] Loaded {} active stencils",
                stencil_manager.active_stencils.len()
            );
        }

        let start_time = Instant::now();

        let ui = UiState {
            settings: {
                let mut s = prefs.settings.clone();
                // Apply CLI overrides
                if args.show_chunk_boundaries {
                    s.show_chunk_boundaries = true;
                }
                s
            },
            window_size: INITIAL_WINDOW_RESOLUTION.into(),
            start_time,
            profile_log_path: if args.profile || args.auto_profile {
                // Create profiles directory and generate timestamped filename
                let profiles_directory = profiles_dir();
                std::fs::create_dir_all(&profiles_directory).ok();
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                // Convert to readable format: YYYYMMDD_HHMMSS
                let secs_per_day = 86400u64;
                let secs_per_hour = 3600u64;
                let secs_per_min = 60u64;
                let days_since_epoch = timestamp / secs_per_day;
                let time_of_day = timestamp % secs_per_day;
                let hours = time_of_day / secs_per_hour;
                let mins = (time_of_day % secs_per_hour) / secs_per_min;
                let secs = time_of_day % secs_per_min;
                // Approximate year/month/day (good enough for filenames)
                let years = days_since_epoch / 365;
                let year = 1970 + years;
                let day_of_year = days_since_epoch % 365;
                let month = day_of_year / 30 + 1;
                let day = day_of_year % 30 + 1;
                let filename = format!(
                    "profile_{:04}{:02}{:02}_{:02}{:02}{:02}.csv",
                    year, month, day, hours, mins, secs
                );
                Some(
                    profiles_directory
                        .join(filename)
                        .to_string_lossy()
                        .into_owned(),
                )
            } else {
                None
            },
            profile_log_header_written: false,
            auto_profile_enabled: args.auto_profile,
            auto_profile_feature: AutoProfileFeature::Baseline,
            auto_profile_feature_off: false,
            auto_profile_phase_start: start_time,
            show_minimap: prefs.show_minimap,
            minimap: Minimap::new(),
            minimap_cached_image: None,
            minimap_last_pos: Vector3::new(i32::MAX, 0, i32::MAX),
            minimap_last_update: Instant::now(),
            minimap_last_yaw: f32::MAX,
            palette_open: false,
            palette_tab: PaletteTab::default(),
            palette_previously_focused: false,
            palette_search: String::new(),
            dragging_item: None,
            hotbar_index: prefs.hotbar_index,
            hotbar_blocks: prefs.get_hotbar_blocks(),
            hotbar_model_ids: prefs.hotbar_model_ids,
            hotbar_tint_indices: prefs.hotbar_tint_indices,
            hotbar_paint_textures: prefs.hotbar_paint_textures,
            current_hit: None,
            breaking_block: None,
            break_progress: 0.0,
            break_cooldown: 0.0,
            skip_break_until_release: false,
            last_place_pos: None,
            place_cooldown: 0.0,
            place_needs_reclick: false,
            model_needs_reclick: false,
            gate_needs_reclick: false,
            custom_rotate_needs_reclick: false,
            line_start_pos: None,
            line_locked_axis: None,
            last_second: Instant::now(),
            frames_since_last_second: 0,
            fps: 0,
            total_frames: 0,
            screenshot_taken: false,
            editor: EditorState::new(),
            editor_previously_focused: false,
            console: ConsoleState::with_history(prefs.console_history.clone()),
            console_previously_focused: false,
            template_ui: TemplateUi::new(),
            template_selection: TemplateSelection::new(),
            template_library: {
                let lib = TemplateLibrary::new(user_templates_dir());
                if let Err(e) = lib.init() {
                    eprintln!("Failed to initialize template library: {}", e);
                }
                lib
            },
            stencil_library: {
                let lib = crate::stencils::StencilLibrary::new(user_stencils_dir());
                if let Err(e) = lib.init() {
                    eprintln!("Failed to initialize stencil library: {}", e);
                }
                lib
            },
            stencil_manager,
            stencil_ui: crate::stencils::StencilUi::new(),
            stencil_previously_focused: false,
            active_stencil_placement: None,
            active_placement: None,
            template_previously_focused: false,
            request_cursor_grab: false,
            rangefinder_active: false,
            flood_fill_active: false,
            measurement_markers: initial_measurement_markers,
            tools_palette: crate::ui::tools::ToolsPaletteState::default(),
            sphere_tool: crate::shape_tools::SphereToolState::default(),
            cube_tool: crate::shape_tools::CubeToolState::default(),
            bridge_tool: crate::shape_tools::BridgeToolState::default(),
            cylinder_tool: crate::shape_tools::CylinderToolState::default(),
            wall_tool: crate::shape_tools::WallToolState::default(),
            floor_tool: crate::shape_tools::FloorToolState::default(),
            replace_tool: crate::shape_tools::ReplaceToolState::default(),
            circle_tool: crate::shape_tools::CircleToolState::default(),
        };

        let input = InputState {
            helper: input,
            focused: false,
            pending_grab: None,
            skip_input_frame: false,
        };

        App {
            args,
            start_time,
            graphics,
            sim,
            ui,
            input,
            prefs,
        }
    }
}

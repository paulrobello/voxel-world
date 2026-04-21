use std::{
    fs::File,
    io::Read,
    ops::Deref,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use vulkano::{
    device::{Device, DeviceOwned},
    pipeline::{
        ComputePipeline, Pipeline, PipelineLayout, PipelineShaderStageCreateInfo,
        compute::ComputePipelineCreateInfo, layout::PipelineDescriptorSetLayoutCreateInfo,
    },
    shader::{ShaderModule, ShaderModuleCreateInfo},
};

/// Maximum include nesting depth to prevent stack overflow from circular includes.
const MAX_INCLUDE_DEPTH: usize = 16;

// ARC-018: Why we use a hand-rolled #include preprocessor instead of
// shaderc's `CompileOptions::set_include_callback`:
//
// 1. The shaderc include callback does not expose shared state between
//    individual include resolutions, so implementing diamond-include support
//    (allowing the same file at two different depths) while still detecting
//    true cycles would require a mutex-wrapped external visited set — more
//    complexity than the current recursive approach.
//
// 2. We call `compile_into_spirv` with the *already-expanded* source text and
//    a single logical file name; shaderc then provides clean column/line error
//    messages that reference the original entry-point path rather than the
//    expanded include tree.
//
// 3. The cycle detection added in SEC-014 (depth limit + canonicalized visited
//    set tracking the current stack, not all-time includes) is already correct
//    and well-tested.  Migrating to the callback would not improve correctness.
fn preprocess_shader(path: &Path) -> String {
    preprocess_shader_inner(path, &mut std::collections::HashSet::new(), 0)
}

fn preprocess_shader_inner(
    path: &Path,
    visited: &mut std::collections::HashSet<PathBuf>,
    depth: usize,
) -> String {
    if depth > MAX_INCLUDE_DEPTH {
        log::warn!(
            "[hot_reload] include depth exceeded {} levels at {:?}; stopping recursion",
            MAX_INCLUDE_DEPTH,
            path
        );
        return String::new();
    }

    // Canonicalize to catch symlink cycles; fall back to the original path on error.
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !visited.insert(canonical.clone()) {
        log::warn!(
            "[hot_reload] circular #include detected for {:?}; skipping",
            path
        );
        return String::new();
    }

    let mut f = File::open(path).unwrap();
    let mut source_text = String::new();
    f.read_to_string(&mut source_text).unwrap();

    let mut result = String::new();
    for line in source_text.lines() {
        if line.trim().starts_with("#include \"") {
            let include_path_str = line
                .trim()
                .strip_prefix("#include \"")
                .unwrap()
                .strip_suffix("\"")
                .unwrap();
            let mut include_path = path.parent().unwrap().to_path_buf();
            include_path.push(include_path_str);
            result.push_str(&preprocess_shader_inner(&include_path, visited, depth + 1));
            result.push('\n');
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }

    // Remove this path from the visited set so sibling includes at the same
    // depth are still processed (visited tracks the current include *stack*,
    // not every file ever included).
    visited.remove(&canonical);

    result
}

fn compile_to_spirv(
    path: &Path,
    kind: shaderc::ShaderKind,
    entry_point_name: &str,
) -> Result<shaderc::CompilationArtifact, shaderc::Error> {
    let source_text = preprocess_shader(path);

    let compiler = shaderc::Compiler::new().unwrap();
    let mut options = shaderc::CompileOptions::new().unwrap();
    options.set_optimization_level(shaderc::OptimizationLevel::Performance);
    options.add_macro_definition("EP", Some(entry_point_name));
    compiler.compile_into_spirv(
        &source_text,
        kind,
        path.to_str().unwrap(),
        entry_point_name,
        Some(&options),
    )
}

fn get_pipeline(shader_module: Arc<ShaderModule>) -> Arc<ComputePipeline> {
    let device = shader_module.device().clone();
    let entry_point = shader_module.entry_point("main").unwrap();

    let stage = PipelineShaderStageCreateInfo::new(entry_point);

    let layout = PipelineLayout::new(
        device.clone(),
        PipelineDescriptorSetLayoutCreateInfo::from_stages([&stage])
            .into_pipeline_layout_create_info(device.clone())
            .unwrap(),
    )
    .unwrap();

    ComputePipeline::new(
        device.clone(),
        None,
        ComputePipelineCreateInfo::stage_layout(stage, layout),
    )
    .unwrap()
}

pub struct HotReloadComputePipeline {
    pipeline: Arc<ComputePipeline>,
    reload: Arc<AtomicBool>,
    path: PathBuf,
    _watcher: RecommendedWatcher,
}

impl Deref for HotReloadComputePipeline {
    type Target = Arc<ComputePipeline>;

    fn deref(&self) -> &Self::Target {
        &self.pipeline
    }
}

impl HotReloadComputePipeline {
    pub fn new(device: Arc<Device>, path: &Path) -> Self {
        let reload = Arc::<AtomicBool>::default();
        let cloned_reload = reload.clone();
        let mut watcher =
            notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res
                    && event.kind.is_modify()
                {
                    cloned_reload.store(true, Ordering::Relaxed);
                }
            })
            .unwrap();

        // Watch the main shader file
        watcher.watch(path, RecursiveMode::NonRecursive).unwrap();

        // Also watch all .glsl files in the same directory (for #include dependencies)
        if let Some(shader_dir) = path.parent()
            && let Ok(entries) = std::fs::read_dir(shader_dir)
        {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.extension().map(|e| e == "glsl").unwrap_or(false) {
                    let _ = watcher.watch(&entry_path, RecursiveMode::NonRecursive);
                }
            }
        }

        let artifact = compile_to_spirv(path, shaderc::ShaderKind::Compute, "main").unwrap();

        let shader_module = unsafe {
            ShaderModule::new(device, ShaderModuleCreateInfo::new(artifact.as_binary())).unwrap()
        };

        let pipeline = get_pipeline(shader_module);

        Self {
            pipeline,
            reload,
            path: path.to_path_buf(),
            _watcher: watcher,
        }
    }

    pub fn maybe_reload(&mut self) {
        if self.reload.swap(false, Ordering::Relaxed) {
            let artifact = match compile_to_spirv(&self.path, shaderc::ShaderKind::Compute, "main")
            {
                Ok(artifact) => artifact,
                Err(e) => {
                    eprint!("{}", e);
                    return;
                }
            };

            let shader_module = unsafe {
                ShaderModule::new(
                    self.pipeline.device().clone(),
                    ShaderModuleCreateInfo::new(artifact.as_binary()),
                )
                .unwrap()
            };

            let new_pipeline = get_pipeline(shader_module);

            let num_sets = self.pipeline.layout().set_layouts().len() as u32;
            let new_num_sets = new_pipeline.layout().set_layouts().len() as u32;
            if num_sets != new_num_sets
                || !new_pipeline
                    .layout()
                    .is_compatible_with(self.pipeline.layout(), num_sets)
            {
                log::warn!(
                    "{} layout is not compatible with pipeline.",
                    self.path.display()
                );
                return;
            }

            self.pipeline = new_pipeline;
            log::warn!("{} successfully reloaded", self.path.display());
        }
    }
}

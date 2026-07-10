//! GPU point-light gathering has moved to [`crate::gpu::collect_torch_lights`].
//!
//! This module previously held render-concern logic (building `GpuLight`s,
//! frustum sorting, and light animation) as `impl World` methods, despite the
//! `World` type living in `world::storage` and the work being pure GPU-light
//! preparation rather than light *propagation* (the world does not propagate
//! light). Those free functions now live next to the `GpuLight` definition in
//! `crate::gpu::lighting`, which is the cleaner home and is reachable from the
//! renderer without a re-export.

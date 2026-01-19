//! Enhanced paint system with HSV adjustment, blend modes, and presets.
//!
//! This module provides:
//! - HSV color adjustments (hue shift, saturation, value)
//! - Multiple blend modes (Multiply, Overlay, SoftLight, Screen, ColorOnly)
//! - Paint presets for saving and loading favorite configurations
//!
//! HSV adjustments are stored per-preset (not per-block) to keep block metadata minimal.
//! The shader applies HSV and blend mode transformations at render time.

// TODO: Fully integrate paint panel UI with the palette window
#![allow(dead_code)]

pub mod presets;
pub mod system;

pub use presets::{PaintPreset, PaintPresetLibrary};
pub use system::{BlendMode, HsvAdjustment, PaintConfig, apply_blend_mode, apply_hsv_adjustment};

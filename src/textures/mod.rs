//! In-game procedural texture generation system.
//!
//! Provides a library of procedural patterns that can be combined with
//! colors to create custom textures for painted blocks. Up to 16 custom
//! textures can be stored and used in-game.
//!
//! Also includes a paint canvas system for pixel-level editing and
//! image import functionality.
#![allow(unused_imports)] // Public API re-exports for future use

pub mod canvas;
pub mod generator;
pub mod import;
pub mod patterns;

pub use canvas::{CanvasState, DEFAULT_PALETTE, PaintTool, ShapeMode};
pub use generator::{
    CUSTOM_TEXTURE_FLAG, CustomTexture, MAX_CUSTOM_TEXTURES, TEXTURE_SIZE, TextureColor,
    TextureLibrary, is_custom_texture, slot_to_texture_index, texture_index_to_slot,
};
pub use import::{ImportState, ResizeMode, SampleFilter, open_image_dialog};
pub use patterns::TexturePattern;

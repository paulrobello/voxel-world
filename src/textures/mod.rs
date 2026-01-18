//! In-game procedural texture generation system.
//!
//! Provides a library of procedural patterns that can be combined with
//! colors to create custom textures for painted blocks. Up to 16 custom
//! textures can be stored and used in-game.
#![allow(unused_imports)] // Public API re-exports for future use

pub mod generator;
pub mod patterns;

pub use generator::{
    CUSTOM_TEXTURE_FLAG, CustomTexture, MAX_CUSTOM_TEXTURES, TEXTURE_SIZE, TextureColor,
    TextureLibrary, is_custom_texture, slot_to_texture_index, texture_index_to_slot,
};
pub use patterns::TexturePattern;

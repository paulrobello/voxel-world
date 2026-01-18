//! In-game procedural texture generation system.
//!
//! Provides a library of procedural patterns that can be combined with
//! colors to create custom textures for painted blocks. Up to 16 custom
//! textures can be stored and used in-game.

pub mod generator;
pub mod patterns;

pub use generator::{CustomTexture, TextureColor, TextureLibrary};
pub use patterns::TexturePattern;

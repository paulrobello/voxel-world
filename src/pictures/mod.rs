//! Picture frame system for in-game artwork.
//!
//! This module provides:
//! - Picture library: Global storage for user-created pictures (shared across worlds)
//! - Picture editor: In-game drawing tools for creating pixel art
//! - Image import: Load and crop/scale external images
//! - Picture atlas: GPU texture atlas for frame rendering
//! - Frame models: 9 frame sizes from 1×1 to 3×3 blocks

// WIP: Full integration pending - re-exports will be used when UI is implemented
#![allow(dead_code)]
#![allow(unused_imports)]

pub mod atlas;
pub mod editor;
pub mod library;

pub use atlas::PictureAtlas;
pub use editor::{PictureEditor, PictureEditorTool};
pub use library::{Picture, PictureLibrary};

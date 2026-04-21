//! Application module containing all app logic and state management.

// Core app structure
mod core;
pub use core::App;

// Initialization
mod init;

// Game loop
mod update;

// Rendering
mod render;

// Event handling
mod event_handler;

// Input handling
pub mod input;

// UI components
pub mod hud;
pub mod minimap;
pub mod stats;

// Helpers
mod helpers;

// Network synchronisation context (extracted from core.rs God Object)
pub(crate) mod network_sync;

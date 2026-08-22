#![cfg_attr(not(feature = "sim"), no_std)]

pub mod assets;
pub mod behavior;
pub mod collision;
pub mod dirty;
pub mod game;
pub mod hardware;
pub mod layer;
pub mod stickman;

#[cfg(feature = "device")]
pub mod app;

/// Display dimensions (landscape: 536×240)
pub const DISPLAY_WIDTH: u32 = 536;
pub const DISPLAY_HEIGHT: u32 = 240;

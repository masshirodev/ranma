//! ranma: a tiling window manager for the terminal.
//!
//! The binary in `main.rs` is a thin CLI over this crate, so everything here is
//! testable without a terminal attached.

pub mod action;
pub mod app;
pub mod bar;
pub mod config;
pub mod hostcolors;
pub mod input;
pub mod ipc;
pub mod keys;
pub mod layout;
pub mod pane;
pub mod picker;
pub mod render;
pub mod theme;
pub mod toast;
pub mod workspace;

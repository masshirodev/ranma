//! ranma: a tiling window manager for the terminal.
//!
//! The binary in `main.rs` is a thin CLI over this crate, so everything here is
//! testable without a terminal attached.

pub mod action;
pub mod app;
pub mod bar;
pub mod client;
pub mod config;
pub mod hints;
pub mod hostcolors;
pub mod input;
pub mod ipc;
pub mod keys;
pub mod layout;
pub mod pane;
pub mod picker;
pub mod proto;
pub mod render;
pub mod sysstat;
pub mod theme;
pub mod tmux;
pub mod toast;
pub mod update;
pub mod workspace;

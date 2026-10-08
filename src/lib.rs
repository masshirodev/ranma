//! ranma: a tiling window manager for the terminal.
//!
//! The binary in `main.rs` is a thin CLI over this crate, so everything here is
//! testable without a terminal attached.

pub mod action;
pub mod app;
pub mod bar;
pub mod chrome;
pub mod client;
pub mod config;
pub mod devtools;
pub mod hints;
pub mod hostcolors;
pub mod input;
pub mod ipc;
pub mod jobs;
pub mod keys;
pub mod layout;
pub mod layouts;
pub mod luapane;
pub mod luaui;
pub mod nestbar;
pub mod osc;
pub mod pane;
pub mod panetext;
pub mod paste;
pub mod picker;
pub mod proto;
pub mod pty;
pub mod render;
pub mod restore;
pub mod snapshot;
pub mod splash;
pub mod store;
pub mod sysstat;
pub mod theme;
pub mod tmux;
pub mod toast;
pub mod toolbar;
pub mod update;
pub mod whichkey;
pub mod winch;
pub mod workspace;

//! What ranma asks the operating system that POSIX does not answer: another
//! process's program, command line and directory, the foreground of a
//! terminal, CPU and memory, the running binary, and a descriptor that turns
//! readable when a child exits. Nothing outside this module reads `/proc` or
//! opens a pidfd, so a second OS is a second file here (ROADMAP, "macOS").

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

use std::path::PathBuf;

/// The longest path a Unix socket can be bound to, without its NUL:
/// `sun_path` is 108 bytes on Linux and 104 on macOS.
#[cfg(target_os = "linux")]
pub const SOCKET_PATH_MAX: usize = 107;
#[cfg(target_os = "macos")]
pub const SOCKET_PATH_MAX: usize = 103;

/// `$XDG_<VAR>` when it is an absolute path, else `~/<fallback>`: the XDG
/// rule on every OS. On Linux it is what `dirs` answers; on macOS it keeps
/// `~/.config/ranma` where a dotfiles repository expects it, rather than
/// `~/Library/Application Support`.
fn xdg(var: &str, fallback: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| dirs::home_dir().map(|h| h.join(fallback)))
}

/// `~/.config`.
pub fn config_home() -> Option<PathBuf> {
    xdg("XDG_CONFIG_HOME", ".config")
}

/// `~/.cache`: logs, the update check.
pub fn cache_home() -> Option<PathBuf> {
    xdg("XDG_CACHE_HOME", ".cache")
}

/// `~/.local/share`: the update clone, hako's clones.
pub fn data_home() -> Option<PathBuf> {
    xdg("XDG_DATA_HOME", ".local/share")
}

/// `~/.local/state`: saved layouts, pane logs.
pub fn state_home() -> Option<PathBuf> {
    xdg("XDG_STATE_HOME", ".local/state")
}

/// `sh -c` that opens `$1` (a link) with the desktop's handler, detached,
/// and returns at once.
#[cfg(target_os = "linux")]
pub const OPEN_URL_SH: &str = "setsid xdg-open \"$1\" >/dev/null 2>&1 </dev/null &";
#[cfg(target_os = "macos")]
pub const OPEN_URL_SH: &str = "open \"$1\" >/dev/null 2>&1 </dev/null";

/// The name of [`OPEN_URL_SH`]'s program, for saying it failed.
#[cfg(target_os = "linux")]
pub const OPEN_URL_PROGRAM: &str = "xdg-open";
#[cfg(target_os = "macos")]
pub const OPEN_URL_PROGRAM: &str = "open";

//! What ranma asks the operating system that POSIX does not answer: another
//! process's program, command line and directory, the foreground of a
//! terminal, CPU and memory, the running binary, and a descriptor that turns
//! readable when a child exits. Nothing outside this module reads `/proc` or
//! opens a pidfd, so a second OS is a second file here (ROADMAP, "macOS").

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

//! Linux: `/proc` and `pidfd_open`.

use std::io;
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::path::{Path, PathBuf};

use crate::sysstat::{self, CpuSample, Mem};

/// Whether a process with this pid exists.
pub fn pid_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

/// The program a process runs, by name (`nvim`, `zsh`).
pub fn program_name(pid: u32) -> Option<String> {
    let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    Some(comm.trim().to_string()).filter(|c| !c.is_empty())
}

/// A process's command line, as words. Empty when it has none to read (a
/// zombie, a kernel thread).
pub fn command_line(pid: u32) -> Option<Vec<String>> {
    let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    Some(
        raw.split(|b| *b == 0)
            .filter(|a| !a.is_empty())
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect(),
    )
}

/// The directory a process is in now.
pub fn cwd(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
}

/// The foreground process group of the terminal whose session leader is
/// `pid` (field 8 of its `stat`), when there is one.
pub fn foreground_group(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    parse_tpgid(&stat)
}

/// The command name is in parentheses and may hold spaces or parentheses of
/// its own: the fields start after the last `)`.
fn parse_tpgid(stat: &str) -> Option<u32> {
    let after = &stat[stat.rfind(')')? + 1..];
    let t = after.split_whitespace().nth(5)?.parse::<i32>().ok()?;
    (t > 0).then_some(t as u32)
}

/// The kernel's release string, which is how WSL is recognised without its
/// variables.
pub fn kernel_release() -> Option<String> {
    std::fs::read_to_string("/proc/sys/kernel/osrelease").ok()
}

/// Nothing to keep: `/proc/self/exe` opens the running binary whatever
/// happened to its file.
pub fn keep_running_binary() {}

/// A path that opens the running binary even after the file it was started
/// from has been replaced.
pub fn running_binary() -> PathBuf {
    PathBuf::from("/proc/self/exe")
}

pub fn read_cpu() -> Option<CpuSample> {
    sysstat::parse_cpu(&std::fs::read_to_string("/proc/stat").ok()?)
}

pub fn read_mem() -> Option<Mem> {
    sysstat::parse_mem(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

/// A descriptor that turns readable once the child `pid` has exited.
pub fn child_exit_fd(pid: u32) -> io::Result<OwnedFd> {
    // SAFETY: a plain syscall; the result is checked.
    let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0) };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: pidfd_open returned a new descriptor.
    Ok(unsafe { OwnedFd::from_raw_fd(raw as RawFd) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_foreground_group_is_read_after_the_last_parenthesis() {
        let stat = "123 (a (b) c) S 1 123 123 34816 4567 4194560 0";
        assert_eq!(parse_tpgid(stat), Some(4567));
        assert_eq!(parse_tpgid("123 (zsh) S 1 123 123 0 -1 0"), None);
        assert_eq!(parse_tpgid("garbage"), None);
    }

    #[test]
    fn this_process_reads_as_itself() {
        let me = std::process::id();
        assert!(pid_alive(me));
        assert!(program_name(me).is_some());
        assert!(!command_line(me).unwrap().is_empty());
        assert_eq!(cwd(me), std::env::current_dir().ok());
        assert!(running_binary().exists());
    }
}

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

/// A descriptor that turns readable once the child `pid` has exited: a pidfd,
/// or on a kernel older than 5.3, which has no `pidfd_open` (Android tablets
/// ship 4.14), [`exit_pipe`].
pub fn child_exit_fd(pid: u32) -> io::Result<OwnedFd> {
    // SAFETY: a plain syscall; the result is checked.
    let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0) };
    if raw < 0 {
        let err = io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::ENOSYS) {
            return exit_pipe(pid);
        }
        return Err(err);
    }
    // SAFETY: pidfd_open returned a new descriptor.
    Ok(unsafe { OwnedFd::from_raw_fd(raw as RawFd) })
}

/// The read end of a pipe whose write end a thread closes once `pid` has
/// exited, so it turns readable (at end of file) for good, as a pidfd does.
/// The thread leaves the child unreaped: the caller's `waitpid` still owns its
/// status.
fn exit_pipe(pid: u32) -> io::Result<OwnedFd> {
    let mut fds = [0; 2];
    // SAFETY: pipe2 fills two descriptors.
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: both are new and owned from here on.
    let (read, write) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    std::thread::Builder::new()
        .name(format!("exit-{pid}"))
        .spawn(move || {
            wait_without_reaping(pid);
            drop(write);
        })?;
    Ok(read)
}

/// Block until `pid` has exited. `WNOWAIT` leaves it a zombie for its reaper.
/// A pid that is not this process's child (`ECHILD`) is watched in `/proc`
/// instead.
fn wait_without_reaping(pid: u32) {
    loop {
        // SAFETY: siginfo_t is plain data, filled by waitid.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        // SAFETY: waitid writes only into `info`.
        let r = unsafe {
            libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOWAIT,
            )
        };
        if r == 0 {
            return;
        }
        if io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
            break;
        }
    }
    while pid_alive(pid) {
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
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

    /// Whether `fd` turns readable within `ms`. A child's exit interrupts
    /// `poll` with `EINTR` when another test has a SIGCHLD handler installed
    /// (alacritty's), so an interrupted wait goes on for what is left.
    fn readable_within(fd: &OwnedFd, ms: u64) -> bool {
        use std::os::fd::AsRawFd;
        use std::time::{Duration, Instant};
        let end = Instant::now() + Duration::from_millis(ms);
        loop {
            let left = end.saturating_duration_since(Instant::now()).as_millis() as i32;
            let mut p = libc::pollfd {
                fd: fd.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: one pollfd, a timeout in milliseconds.
            let n = unsafe { libc::poll(&mut p, 1, left) };
            if n >= 0 || io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
                return n == 1;
            }
        }
    }

    /// The pipe a kernel without `pidfd_open` gets: quiet while the child
    /// runs, readable once it exits, and the child still there to reap.
    #[test]
    fn the_exit_pipe_turns_readable_and_leaves_the_child_to_reap() {
        let mut child = std::process::Command::new("sleep")
            .arg("0.3")
            .spawn()
            .unwrap();
        let fd = exit_pipe(child.id()).unwrap();
        assert!(!readable_within(&fd, 50), "readable while the child runs");
        assert!(readable_within(&fd, 5000), "no exit seen in 5 s");
        assert!(child.wait().unwrap().success(), "the thread reaped it");
    }

    #[test]
    fn a_child_s_exit_makes_its_descriptor_readable() {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let fd = child_exit_fd(child.id()).unwrap();
        assert!(readable_within(&fd, 5000), "no exit seen in 5 s");
        child.wait().unwrap();
    }
}

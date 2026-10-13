//! macOS: libproc's `proc_pidinfo` and `proc_name`, `sysctl`, the mach host
//! statistics, and kqueue. Written to the same questions as `linux.rs`, so
//! nothing outside this module knows which one it is talking to.
//!
//! The mach calls are marked deprecated in libc in favour of the `mach2`
//! crate; three calls do not earn a dependency, so they are allowed here.

use std::ffi::CStr;
use std::io;
use std::mem::{MaybeUninit, size_of};
use std::os::fd::{FromRawFd, OwnedFd};
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::sysstat::{CpuSample, Mem};

/// Whether a process with this pid exists (one we may not signal still does).
pub fn pid_alive(pid: u32) -> bool {
    // SAFETY: signal 0 checks for the process and sends nothing.
    let r = unsafe { libc::kill(pid as libc::pid_t, 0) };
    r == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// The program a process runs, by name (`nvim`, `zsh`).
pub fn program_name(pid: u32) -> Option<String> {
    let mut buf = [0u8; 2 * libc::MAXCOMLEN + 1];
    // SAFETY: proc_name writes at most `buf.len()` bytes and returns how many.
    let n = unsafe {
        libc::proc_name(
            pid as libc::c_int,
            buf.as_mut_ptr().cast(),
            buf.len() as u32,
        )
    };
    if n <= 0 {
        return None;
    }
    Some(String::from_utf8_lossy(&buf[..n as usize]).into_owned()).filter(|c| !c.is_empty())
}

/// A process's command line, as words, from `KERN_PROCARGS2`: argc, the
/// executable's path, NUL padding, then argc NUL-terminated words (and the
/// environment after them, which is left alone).
pub fn command_line(pid: u32) -> Option<Vec<String>> {
    let mut argmax: libc::c_int = 0;
    let mut size = size_of::<libc::c_int>();
    let mut mib = [libc::CTL_KERN, libc::KERN_ARGMAX];
    // SAFETY: sysctl into a c_int of the size given.
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            2,
            (&mut argmax as *mut libc::c_int).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return None;
    }
    let mut buf = vec![0u8; argmax as usize];
    let mut size = buf.len();
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as libc::c_int];
    // SAFETY: sysctl into `buf`, which is `size` bytes.
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            buf.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return None;
    }
    parse_procargs(&buf[..size])
}

fn parse_procargs(buf: &[u8]) -> Option<Vec<String>> {
    let argc = i32::from_ne_bytes(buf.get(..4)?.try_into().ok()?);
    let rest = &buf[4..];
    // The executable's path, then the NULs that pad it.
    let after_path = rest.iter().position(|b| *b == 0)?;
    let start = after_path + rest[after_path..].iter().position(|b| *b != 0)?;
    Some(
        rest[start..]
            .split(|b| *b == 0)
            .take(argc.max(0) as usize)
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect(),
    )
}

/// The directory a process is in now.
pub fn cwd(pid: u32) -> Option<PathBuf> {
    let mut info = MaybeUninit::<libc::proc_vnodepathinfo>::zeroed();
    let size = size_of::<libc::proc_vnodepathinfo>() as libc::c_int;
    // SAFETY: proc_pidinfo fills at most `size` bytes of `info`.
    let n = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if n != size {
        return None;
    }
    // SAFETY: filled whole, as checked; zeroed before, so the path ends.
    let info = unsafe { info.assume_init() };
    // libc spells the path as 32 rows of 32 so old compilers take it.
    let path: &[libc::c_char] = info.pvi_cdir.vip_path.as_flattened();
    // SAFETY: a NUL-terminated string inside `path`, which was zeroed.
    let s = unsafe { CStr::from_ptr(path.as_ptr()) };
    Some(PathBuf::from(s.to_string_lossy().into_owned())).filter(|p| p.is_absolute())
}

/// The foreground process group of the terminal whose session leader is
/// `pid` (`e_tpgid` of its BSD info), when there is one.
pub fn foreground_group(pid: u32) -> Option<u32> {
    let mut info = MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = size_of::<libc::proc_bsdinfo>() as libc::c_int;
    // SAFETY: proc_pidinfo fills at most `size` bytes of `info`.
    let n = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if n != size {
        return None;
    }
    // SAFETY: filled whole, as checked.
    let t = unsafe { info.assume_init() }.e_tpgid;
    (t > 0).then_some(t)
}

/// Only Linux's kernel can say it is WSL.
pub fn kernel_release() -> Option<String> {
    None
}

static KEPT: OnceLock<PathBuf> = OnceLock::new();

/// Keep a link to the running binary, for [`running_binary`]. macOS has no
/// `/proc/self/exe` to open the file a process started from once
/// `install.sh` has replaced it, so a hard link (a copy across volumes) is
/// made at start. One a server long gone left is cleared on the way.
pub fn keep_running_binary() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let dir = crate::ipc::server_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if let Some(pid) = name
            .strip_prefix("exe-")
            .and_then(|p| p.parse::<u32>().ok())
            && !pid_alive(pid)
        {
            let _ = std::fs::remove_file(e.path());
        }
    }
    let kept = dir.join(format!("exe-{}", std::process::id()));
    let _ = std::fs::remove_file(&kept);
    if std::fs::hard_link(&exe, &kept).is_ok() || std::fs::copy(&exe, &kept).is_ok() {
        let _ = KEPT.set(kept);
    }
}

/// A path that opens the running binary even after the file it was started
/// from has been replaced: the link [`keep_running_binary`] made.
pub fn running_binary() -> PathBuf {
    KEPT.get()
        .cloned()
        .or_else(|| std::env::current_exe().ok())
        .unwrap_or_default()
}

#[allow(deprecated)]
fn host_statistics<T>(flavor: libc::c_int, count: u32) -> Option<T> {
    let mut out = MaybeUninit::<T>::zeroed();
    let mut count = count;
    // SAFETY: the flavor's struct and its count in integer_t units, as
    // mach's headers pair them.
    let r = unsafe {
        libc::host_statistics64(
            libc::mach_host_self(),
            flavor,
            out.as_mut_ptr().cast(),
            &mut count,
        )
    };
    // SAFETY: KERN_SUCCESS means it was filled.
    (r == libc::KERN_SUCCESS).then(|| unsafe { out.assume_init() })
}

/// Ticks of every CPU together: user, system, idle, nice.
pub fn read_cpu() -> Option<CpuSample> {
    let info: libc::host_cpu_load_info =
        host_statistics(libc::HOST_CPU_LOAD_INFO, libc::HOST_CPU_LOAD_INFO_COUNT)?;
    let t = info.cpu_ticks.map(u64::from);
    Some(CpuSample::from_ticks(
        t[libc::CPU_STATE_IDLE as usize],
        t.iter().sum(),
    ))
}

/// The machine's memory (`hw.memsize`), and what is free, inactive or
/// speculative as what could be had: what Activity Monitor leaves out of
/// "used".
#[allow(deprecated)]
pub fn read_mem() -> Option<Mem> {
    let mut total: u64 = 0;
    let mut size = size_of::<u64>();
    // SAFETY: sysctlbyname into a u64 of the size given.
    if unsafe {
        libc::sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&mut total as *mut u64).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return None;
    }
    let vm: libc::vm_statistics64 =
        host_statistics(libc::HOST_VM_INFO64, libc::HOST_VM_INFO64_COUNT)?;
    // SAFETY: a static the system sets before main.
    let page = unsafe { libc::vm_page_size } as u64;
    let pages =
        u64::from(vm.free_count) + u64::from(vm.inactive_count) + u64::from(vm.speculative_count);
    Some(Mem {
        total_kib: total / 1024,
        available_kib: pages * page / 1024,
    })
}

/// A kqueue that turns readable once the child `pid` has exited, as Linux's
/// pidfd does. A child that has already exited cannot be registered, and
/// gets a pipe with no writer instead, which reads as ready at once.
pub fn child_exit_fd(pid: u32) -> io::Result<OwnedFd> {
    // SAFETY: kqueue returns a new descriptor or -1.
    let kq = unsafe { libc::kqueue() };
    if kq < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: kqueue returned a new descriptor.
    let kq = unsafe { OwnedFd::from_raw_fd(kq) };
    use std::os::fd::AsRawFd;
    let _ = crate::pty::set_cloexec(kq.as_raw_fd(), true);
    let ev = libc::kevent {
        ident: pid as libc::uintptr_t,
        filter: libc::EVFILT_PROC,
        flags: libc::EV_ADD,
        fflags: libc::NOTE_EXIT,
        data: 0,
        udata: std::ptr::null_mut(),
    };
    // SAFETY: one change, no events asked back.
    let r = unsafe {
        libc::kevent(
            kq.as_raw_fd(),
            &ev,
            1,
            std::ptr::null_mut(),
            0,
            std::ptr::null(),
        )
    };
    if r == 0 {
        return Ok(kq);
    }
    let err = io::Error::last_os_error();
    if err.raw_os_error() != Some(libc::ESRCH) {
        return Err(err);
    }
    let mut fds = [0; 2];
    // SAFETY: pipe fills two descriptors.
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: both are new; the write end is closed at once.
    unsafe {
        libc::close(fds[1]);
        let _ = crate::pty::set_cloexec(fds[0], true);
        Ok(OwnedFd::from_raw_fd(fds[0]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procargs_skip_the_path_and_its_padding() {
        let mut buf = 2i32.to_ne_bytes().to_vec();
        buf.extend_from_slice(b"/usr/bin/ssh\0\0\0\0ssh\0vps\0HOME=/x\0");
        assert_eq!(parse_procargs(&buf), Some(vec!["ssh".into(), "vps".into()]));
        assert_eq!(parse_procargs(b"\x01"), None);
    }

    #[test]
    fn this_process_reads_as_itself() {
        let me = std::process::id();
        assert!(pid_alive(me));
        assert!(program_name(me).is_some());
        assert!(!command_line(me).unwrap().is_empty());
        assert_eq!(cwd(me), std::env::current_dir().ok());
        assert!(read_cpu().is_some());
        let m = read_mem().unwrap();
        assert!(m.total_kib > 0 && m.available_kib <= m.total_kib);
    }

    #[test]
    fn a_child_s_exit_makes_the_queue_readable() {
        let child = std::process::Command::new("true").spawn().unwrap();
        let fd = child_exit_fd(child.id()).unwrap();
        use std::os::fd::AsRawFd;
        let mut p = libc::pollfd {
            fd: fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one pollfd, a timeout in milliseconds.
        let n = unsafe { libc::poll(&mut p, 1, 5000) };
        assert_eq!(n, 1, "no exit seen in 5 s");
        let mut child = child;
        child.wait().unwrap();
    }
}

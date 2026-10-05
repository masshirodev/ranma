//! A PTY whose child is already running: what a pane is after a server takes
//! a new build (DESIGN.md, "Upgrading a server in place"). alacritty's own
//! PTY type can only start a child; this one adopts one, from the master's
//! descriptor and the child's pid, which survive the server's `execve`.
//!
//! It is polled the way alacritty's is, under the two keys its event loop
//! waits on: the master for reading and writing, and a pidfd that turns
//! readable when the child exits.

use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::sync::Arc;

use alacritty_terminal::event::{OnResize, WindowSize};
use alacritty_terminal::tty::{ChildEvent, EventedPty, EventedReadWrite};
use polling::{Event, PollMode, Poller};

/// alacritty's event loop keys: `tty::PTY_READ_WRITE_TOKEN` and
/// `PTY_CHILD_EVENT_TOKEN`, which it keeps private. Its loop matches these
/// numbers, so an adopted PTY must register under them.
const READ_WRITE: usize = 0;
const CHILD: usize = 1;

pub struct AdoptedPty {
    reader: File,
    writer: File,
    pid: libc::pid_t,
    /// Readable once the child has exited.
    pidfd: OwnedFd,
    exited: bool,
}

impl AdoptedPty {
    /// Take over the PTY whose master is `fd`, running `pid`. The descriptor
    /// is owned from here on.
    ///
    /// # Safety
    /// `fd` must be an open PTY master nothing else will close.
    pub unsafe fn adopt(fd: RawFd, pid: u32) -> io::Result<AdoptedPty> {
        // The master was kept open across the exec; from here on a program
        // this ranma starts must not inherit it, or a pane closed later would
        // never hang up its shell.
        set_cloexec(fd, true)?;
        set_nonblocking(fd)?;
        // SAFETY: the caller hands the descriptor over.
        let reader = unsafe { File::from_raw_fd(fd) };
        let writer = reader.try_clone()?;
        // SAFETY: a plain syscall; the result is checked.
        let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0) };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: pidfd_open returned a new descriptor.
        let pidfd = unsafe { OwnedFd::from_raw_fd(raw as RawFd) };
        Ok(AdoptedPty {
            reader,
            writer,
            pid: pid as libc::pid_t,
            pidfd,
            exited: false,
        })
    }
}

/// Mark a descriptor close-on-exec or not: kept across a server's exec, and
/// closed across any other.
pub fn set_cloexec(fd: RawFd, on: bool) -> io::Result<()> {
    // SAFETY: fcntl on a descriptor the caller holds.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    let flags = if on {
        flags | libc::FD_CLOEXEC
    } else {
        flags & !libc::FD_CLOEXEC
    };
    // SAFETY: as above.
    if unsafe { libc::fcntl(fd, libc::F_SETFD, flags) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn set_nonblocking(fd: RawFd) -> io::Result<()> {
    // SAFETY: fcntl on a descriptor the caller holds.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// How long a PTY stays a row short before [`redraw`] puts its size back:
/// long enough for the program to have read the first change (ssh sends it
/// on at once), short enough not to be seen.
pub const REDRAW_NUDGE: std::time::Duration = std::time::Duration::from_millis(150);

/// Ask the programs in a PTY's foreground to draw again, as a resize would:
/// a server that took a new build does this for panes whose program drew on
/// the alternate screen. A bare SIGWINCH at the same size is not enough,
/// since programs that compare the size skip it, OpenSSH among them (it sends
/// a window change on only when the size differs), so a ranma or nvim across
/// ssh never heard it and its pane stayed blank. So the PTY really changes
/// size, a row shorter, and is put back after [`REDRAW_NUDGE`]: the kernel
/// signals both. The size is put back only if nothing resized the pane in
/// between.
pub fn redraw(fd: RawFd) {
    // SAFETY: plain ioctls and fcntl on a descriptor this server owns; the
    // duplicate is the thread's own, so the pane closing meanwhile cannot
    // leave it naming another file.
    unsafe {
        let mut was: libc::winsize = std::mem::zeroed();
        if libc::ioctl(fd, libc::TIOCGWINSZ, &mut was) != 0 || was.ws_row == 0 {
            signal_foreground(fd);
            return;
        }
        let dup = libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0);
        if dup < 0 {
            signal_foreground(fd);
            return;
        }
        let mut short = was;
        short.ws_row = if was.ws_row > 1 {
            was.ws_row - 1
        } else {
            was.ws_row + 1
        };
        libc::ioctl(dup, libc::TIOCSWINSZ, &short);
        let _ = std::thread::Builder::new()
            .name("redraw".into())
            .spawn(move || {
                std::thread::sleep(REDRAW_NUDGE);
                let mut now: libc::winsize = std::mem::zeroed();
                if libc::ioctl(dup, libc::TIOCGWINSZ, &mut now) == 0
                    && (now.ws_row, now.ws_col) == (short.ws_row, short.ws_col)
                {
                    libc::ioctl(dup, libc::TIOCSWINSZ, &was);
                }
                libc::close(dup);
            });
    }
}

/// SIGWINCH to a PTY's foreground process group, when its size cannot be read.
fn signal_foreground(fd: RawFd) {
    // SAFETY: tcgetpgrp and kill are plain syscalls; failures are ignored.
    unsafe {
        let pgrp = libc::tcgetpgrp(fd);
        if pgrp > 0 {
            libc::kill(-pgrp, libc::SIGWINCH);
        }
    }
}

impl EventedReadWrite for AdoptedPty {
    type Reader = File;
    type Writer = File;

    unsafe fn register(
        &mut self,
        poll: &Arc<Poller>,
        mut interest: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        interest.key = READ_WRITE;
        // SAFETY: both descriptors live as long as `self`, which the event
        // loop deregisters before dropping.
        unsafe {
            poll.add_with_mode(&self.reader, interest, mode)?;
            poll.add_with_mode(&self.pidfd, Event::readable(CHILD), PollMode::Level)
        }
    }

    fn reregister(
        &mut self,
        poll: &Arc<Poller>,
        mut interest: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        interest.key = READ_WRITE;
        poll.modify_with_mode(&self.reader, interest, mode)?;
        poll.modify_with_mode(&self.pidfd, Event::readable(CHILD), PollMode::Level)
    }

    fn deregister(&mut self, poll: &Arc<Poller>) -> io::Result<()> {
        poll.delete(&self.reader)?;
        poll.delete(&self.pidfd)
    }

    fn reader(&mut self) -> &mut File {
        &mut self.reader
    }

    fn writer(&mut self) -> &mut File {
        &mut self.writer
    }
}

impl EventedPty for AdoptedPty {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        if self.exited {
            return None;
        }
        let mut status = 0;
        // SAFETY: waitpid on our own child, not blocking.
        let r = unsafe { libc::waitpid(self.pid, &mut status, libc::WNOHANG) };
        if r == self.pid {
            self.exited = true;
            use std::os::unix::process::ExitStatusExt;
            return Some(ChildEvent::Exited(Some(
                std::process::ExitStatus::from_raw(status),
            )));
        }
        None
    }
}

impl OnResize for AdoptedPty {
    fn on_resize(&mut self, size: WindowSize) {
        let win = libc::winsize {
            ws_row: size.num_lines,
            ws_col: size.num_cols,
            ws_xpixel: size.num_cols.saturating_mul(size.cell_width),
            ws_ypixel: size.num_lines.saturating_mul(size.cell_height),
        };
        // SAFETY: TIOCSWINSZ on the master with a valid winsize.
        unsafe {
            libc::ioctl(self.reader.as_raw_fd(), libc::TIOCSWINSZ, &win);
        }
    }
}

impl Drop for AdoptedPty {
    /// As alacritty's does: hang up the child and reap it, so closing a pane
    /// ends its shell and the jobs in it.
    fn drop(&mut self) {
        if self.exited {
            return;
        }
        // SAFETY: signals and waitpid on our own child.
        unsafe {
            libc::kill(self.pid, libc::SIGHUP);
            let mut status = 0;
            libc::waitpid(self.pid, &mut status, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pty(rows: u16, cols: u16) -> (RawFd, RawFd) {
        let (mut master, mut slave) = (0, 0);
        let ws = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: openpty fills the two descriptors; the null names are allowed.
        let r = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                &ws,
            )
        };
        assert_eq!(r, 0);
        (master, slave)
    }

    fn size(fd: RawFd) -> (u16, u16) {
        // SAFETY: a plain ioctl into a local.
        unsafe {
            let mut ws: libc::winsize = std::mem::zeroed();
            libc::ioctl(fd, libc::TIOCGWINSZ, &mut ws);
            (ws.ws_row, ws.ws_col)
        }
    }

    /// A redraw is a real size change and back, which a program that skips
    /// a same-size SIGWINCH (ssh) still hears.
    #[test]
    fn a_redraw_changes_the_size_and_puts_it_back() {
        let (m, s) = pty(24, 80);
        redraw(m);
        assert_eq!(size(s), (23, 80), "a row short first");
        std::thread::sleep(REDRAW_NUDGE * 3);
        assert_eq!(size(s), (24, 80), "and back");
        // SAFETY: closing the descriptors this test opened.
        unsafe {
            libc::close(m);
            libc::close(s);
        }
    }

    /// A pane resized while it is a row short keeps its new size.
    #[test]
    fn a_resize_meanwhile_is_not_undone() {
        let (m, s) = pty(24, 80);
        redraw(m);
        let ws = libc::winsize {
            ws_row: 40,
            ws_col: 120,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: a plain ioctl on the descriptor this test opened.
        unsafe { libc::ioctl(m, libc::TIOCSWINSZ, &ws) };
        std::thread::sleep(REDRAW_NUDGE * 3);
        assert_eq!(size(s), (40, 120));
        // SAFETY: closing the descriptors this test opened.
        unsafe {
            libc::close(m);
            libc::close(s);
        }
    }
}

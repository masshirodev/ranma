//! Which socket a Windows program is started through, under WSL.
//!
//! A Linux process runs a `.exe` by asking the socket `$WSL_INTEROP` names,
//! `/run/WSL/<pid>_interop`, where `<pid>` is the `/init` relay of one
//! terminal window's WSL session. The socket goes when that window closes. A
//! server outlives the terminal that started it, so the variable it inherited
//! soon names nothing, and every `.exe` the server or its panes start (the
//! clipboard's `powershell.exe` and `clip.exe`, `code`, `explorer.exe`) waits
//! ten seconds and fails with `UtilAcceptVsock: accept4 failed 110`.
//!
//! So each spawn is given a socket that works now: the one of the terminal
//! that last attached (open, by being attached), else the server's own if it
//! is still there, else the newest session's. A client reached over SSH has
//! none to give, which is why the newest one is looked for at all. The boot
//! sessions' `1_interop` and `2_interop` never go, but on WSL 2 they fail the
//! same way, and a connect succeeds on them all the same; only running a
//! program tells, at ten seconds a try, so they are never picked.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::SystemTime;

pub const VAR: &str = "WSL_INTEROP";
const DIR: &str = "/run/WSL";

/// The `WSL_INTEROP` of the client that attached last.
static ATTACHED: Mutex<Option<PathBuf>> = Mutex::new(None);

/// A client attached, carrying its terminal's `WSL_INTEROP` (`proto::Hello`).
/// One without (an older build, or over SSH) leaves the last one standing.
pub fn attached(var: Option<&str>) {
    if let Some(v) = var.filter(|v| !v.is_empty()) {
        *ATTACHED.lock().unwrap_or_else(|e| e.into_inner()) = Some(PathBuf::from(v));
    }
}

/// The socket to start a Windows program through now; `None` off WSL, or when
/// no terminal window is open to reach Windows by.
pub fn live() -> Option<PathBuf> {
    let attached = ATTACHED.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let own = std::env::var_os(VAR).map(PathBuf::from);
    let entries = std::fs::read_dir(DIR).into_iter().flatten().flatten();
    let found = sessions(
        entries.filter_map(|e| {
            use std::os::unix::fs::FileTypeExt;
            // Not followed: `1_interop` is a link to `2_interop`.
            let m = e.path().symlink_metadata().ok()?;
            Some((e.path(), m.file_type().is_socket(), m.modified().ok()?))
        }),
        crate::os::pid_alive,
    );
    choose(attached.as_deref(), own.as_deref(), &found)
}

/// Give `cmd` a working `WSL_INTEROP`, when there is one; otherwise it keeps
/// what it would have inherited.
pub fn apply(cmd: &mut Command) {
    if let Some(s) = live() {
        cmd.env(VAR, s);
    }
}

/// A session's socket, alive when looked at.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Session {
    path: PathBuf,
    modified: SystemTime,
}

/// The sessions among a directory's entries (path, is a socket, modified):
/// sockets named `<pid>_interop` whose relay is `alive`, the boot sessions
/// left out.
pub(crate) fn sessions(
    entries: impl Iterator<Item = (PathBuf, bool, SystemTime)>,
    alive: impl Fn(u32) -> bool,
) -> Vec<Session> {
    entries
        .filter(|(_, socket, _)| *socket)
        .filter(|(path, _, _)| {
            path.file_name()
                .and_then(|n| n.to_str()?.strip_suffix("_interop")?.parse::<u32>().ok())
                .is_some_and(|pid| pid > 2 && alive(pid))
        })
        .map(|(path, _, modified)| Session { path, modified })
        .collect()
}

/// The attached client's socket, else the server's own, if either is among
/// the live `found`; else the newest of them.
pub(crate) fn choose(
    attached: Option<&Path>,
    own: Option<&Path>,
    found: &[Session],
) -> Option<PathBuf> {
    let live = |p: &Path| found.iter().any(|s| s.path == p);
    attached
        .filter(|p| live(p))
        .or(own.filter(|p| live(p)))
        .map(Path::to_path_buf)
        .or_else(|| {
            found
                .iter()
                .max_by_key(|s| s.modified)
                .map(|s| s.path.clone())
        })
}

/// What WSL prints when the socket a `.exe` was started through is gone.
pub fn unreachable(stderr: &str) -> bool {
    stderr.contains("UtilAcceptVsock")
}

/// Said instead of WSL's own words, which name no cause.
pub const UNREACHABLE: &str = "cannot reach Windows: no WSL terminal window is open \
     (the one this server started from has closed); open one and try again";

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn entry(name: &str, socket: bool, secs: u64) -> (PathBuf, bool, SystemTime) {
        (Path::new(DIR).join(name), socket, at(secs))
    }

    fn p(name: &str) -> PathBuf {
        Path::new(DIR).join(name)
    }

    /// What `/run/WSL` held on the machine this was found on: the boot
    /// sessions, which fail, and one open WezTerm window.
    fn found() -> Vec<Session> {
        sessions(
            [
                entry("1_interop", false, 10),
                entry("2_interop", true, 10),
                entry("378034_interop", true, 50),
                entry("384208_interop", true, 60),
                entry("notes", false, 70),
                entry("x_interop", true, 70),
            ]
            .into_iter(),
            |pid| pid != 384208,
        )
    }

    #[test]
    fn the_boot_sessions_a_dead_relay_and_other_files_are_no_session() {
        assert_eq!(
            found(),
            vec![Session {
                path: p("378034_interop"),
                modified: at(50)
            }]
        );
    }

    #[test]
    fn the_attached_terminal_comes_first_then_the_servers_own_then_the_newest() {
        let found = sessions(
            [
                entry("100_interop", true, 10),
                entry("200_interop", true, 30),
                entry("300_interop", true, 20),
            ]
            .into_iter(),
            |_| true,
        );
        let (a, b) = (p("100_interop"), p("300_interop"));
        assert_eq!(choose(Some(&a), Some(&b), &found), Some(a.clone()));
        assert_eq!(choose(None, Some(&b), &found), Some(b.clone()));
        assert_eq!(choose(None, None, &found), Some(p("200_interop")));
    }

    /// The bug: a server started from a Windows Terminal window since closed.
    #[test]
    fn a_closed_windows_socket_is_passed_over() {
        let gone = p("7104_interop");
        assert_eq!(
            choose(Some(&gone), Some(&gone), &found()),
            Some(p("378034_interop"))
        );
        assert_eq!(
            choose(None, Some(&p("2_interop")), &found()),
            Some(p("378034_interop")),
            "the boot session is never kept, even as the server's own"
        );
    }

    #[test]
    fn with_no_window_open_there_is_nothing_to_give() {
        assert_eq!(choose(Some(&p("378034_interop")), None, &[]), None);
    }

    #[test]
    fn wsls_own_complaint_is_recognised() {
        assert!(unreachable(
            "<3>WSL (404874 - ) ERROR: UtilAcceptVsock:273: accept4 failed 110\n"
        ));
        assert!(!unreachable("Access is denied."));
    }

    #[test]
    fn a_client_without_one_keeps_the_last() {
        attached(Some("/run/WSL/5_interop"));
        attached(None);
        attached(Some(""));
        assert_eq!(
            ATTACHED.lock().unwrap().clone(),
            Some(PathBuf::from("/run/WSL/5_interop"))
        );
    }
}

//! The ranma you start in a terminal: a client of a ranma server.
//!
//! It picks a server — the one you name, else the most recently used one that
//! no terminal is showing, else a new one — starts it if it has to, and becomes a
//! pipe: keys, mouse and resizes to the server, the server's bytes to the
//! terminal. It keeps no state of its own, so there is nothing to fall out of step
//! (see DESIGN.md, "A daemon, and a client that holds nothing"). Closing the
//! terminal ends the client, never the server.

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use crossterm::cursor::SetCursorStyle;
use crossterm::event::{DisableBracketedPaste, DisableFocusChange, DisableMouseCapture};
use crossterm::execute;

use crate::ipc;
use crate::proto::{self, Hello, Status, ToClient, ToServer};

/// Every server that answers, pruning sockets whose server is gone.
pub fn servers() -> Vec<Status> {
    let Ok(entries) = std::fs::read_dir(ipc::server_dir()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|x| x.to_str()) != Some("sock") {
            continue;
        }
        match ipc::status(&path) {
            Ok(st) => out.push(st),
            // Nobody listens: a crashed server's leftover.
            Err(_) if UnixStream::connect(&path).is_err() => {
                let _ = std::fs::remove_file(&path);
            }
            Err(_) => {}
        }
    }
    out.sort_by_key(|s| natural(&s.name));
    out
}

/// Numbers before names, numbers in order: 2 before 10.
fn natural(name: &str) -> (u64, String) {
    (name.parse().unwrap_or(u64::MAX), name.to_string())
}

/// Which server to attach to: the one named, else the most recently active
/// one nobody is attached to, else a new one with the lowest free number.
pub fn choose(servers: &[Status], name: Option<&str>) -> (String, bool) {
    if let Some(n) = name {
        return (n.to_string(), !servers.iter().any(|s| s.name == n));
    }
    if let Some(s) = servers
        .iter()
        .filter(|s| !s.attached)
        .max_by_key(|s| s.last_active)
    {
        return (s.name.clone(), false);
    }
    let name = (1..)
        .map(|i: u32| i.to_string())
        .find(|n| !servers.iter().any(|s| &s.name == n))
        .expect("some number is free");
    (name, true)
}

fn log_path(name: &str) -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("ranma").join(format!("server-{name}.log")))
}

/// Start `ranma server --name NAME` in a session of its own, so it outlives this
/// terminal, and wait until it listens.
fn start_server(name: &str) -> Result<()> {
    let exe = std::env::current_exe().context("finding the ranma binary")?;
    let log = log_path(name);
    let stderr = match &log {
        Some(p) => {
            if let Some(d) = p.parent() {
                std::fs::create_dir_all(d)?;
            }
            Stdio::from(std::fs::File::create(p)?)
        }
        None => Stdio::null(),
    };
    let mut cmd = Command::new(exe);
    cmd.args(["server", "--name", name])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr);
    // SAFETY: setsid in the child, before exec: no allocation, no locks.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = cmd.spawn().context("starting a ranma server")?;
    let sock = ipc::server_socket(name);
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if UnixStream::connect(&sock).is_ok() {
            return Ok(());
        }
        if let Ok(Some(status)) = child.try_wait() {
            let tail = log
                .as_ref()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .unwrap_or_default();
            bail!("the ranma server exited ({status}):\n{}", tail.trim());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    bail!("the ranma server did not start listening within 5 s")
}

/// `ranma` / `ranma attach NAME`: attach this terminal to a server.
pub fn run(name: Option<&str>) -> Result<ExitCode> {
    let all = servers();
    let (name, new) = choose(&all, name);
    let sock = ipc::server_socket(&name);
    // Attaching a server to itself, inside its own pane, would feed its output
    // back into itself.
    if std::env::var_os(ipc::ENV).is_some_and(|s| s == sock.as_os_str()) {
        bail!("this shell runs inside ranma server {name} already");
    }
    if new {
        start_server(&name)?;
    }
    let mut stream =
        UnixStream::connect(&sock).with_context(|| format!("connecting to {}", sock.display()))?;

    // The terminal is ours from here: raw mode, the alternate screen, and a
    // panic hook that gives it back.
    drop(ratatui::try_init().context("setting up the terminal")?);
    let mut out = std::io::stdout();
    let _ = out.write_all(b"\x1b[22;0t");
    let result = attach(&mut stream, &name);
    let _ = out.write_all(b"\x1b[23;0t");
    let _ = execute!(
        out,
        DisableMouseCapture,
        DisableBracketedPaste,
        DisableFocusChange,
        SetCursorStyle::DefaultUserShape
    );
    let _ = out.flush();
    ratatui::restore();
    match result {
        Ok(End::Detached(why)) => {
            println!("[ranma {name}: {why}]");
            Ok(ExitCode::SUCCESS)
        }
        Ok(End::Exited) => Ok(ExitCode::SUCCESS),
        Ok(End::Lost) => {
            let log = log_path(&name)
                .map(|p| format!("; its log: {}", p.display()))
                .unwrap_or_default();
            bail!("lost the connection to ranma server {name}{log}")
        }
        Err(e) => Err(e),
    }
}

enum End {
    Detached(String),
    Exited,
    Lost,
}

fn attach(stream: &mut UnixStream, name: &str) -> Result<End> {
    // Before any input thread: the host's replies are read off the terminal
    // here, with any keys typed meanwhile kept for the shell.
    let (colors, typed_early) = crate::hostcolors::query(Duration::from_millis(300));
    let (cols, rows) = crossterm::terminal::size()?;
    stream.write_all(b"attach\n")?;
    proto::send_to_server(
        stream,
        &ToServer::Hello(Hello {
            build: crate::update::BUILD_SHA.to_string(),
            cols,
            rows,
            colors,
            typed_early,
        }),
    )?;

    let mut writer = stream.try_clone()?;
    std::thread::Builder::new()
        .name("client-input".into())
        .spawn(move || {
            while let Ok(ev) = crossterm::event::read() {
                if proto::send_to_server(&mut writer, &ToServer::Event(ev)).is_err() {
                    return;
                }
            }
        })?;

    let mut out = std::io::stdout();
    loop {
        match proto::read_to_client(stream) {
            Ok(Some(ToClient::Output(bytes))) => {
                out.write_all(&bytes)?;
                out.flush()?;
            }
            Ok(Some(ToClient::Detached(why))) => return Ok(End::Detached(why)),
            Ok(Some(ToClient::Exited(_))) => return Ok(End::Exited),
            Ok(None) | Err(_) => {
                let _ = name;
                return Ok(End::Lost);
            }
        }
    }
}

/// `ranma ls`: the servers, which ones a terminal shows, and what they hold.
pub fn list() -> ExitCode {
    let all = servers();
    if all.is_empty() {
        println!("no ranma servers running");
        return ExitCode::SUCCESS;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    for s in all {
        let ago = now.saturating_sub(s.last_active);
        let ago = match ago {
            0..=59 => format!("{ago}s ago"),
            60..=3599 => format!("{}m ago", ago / 60),
            3600..=86399 => format!("{}h ago", ago / 3600),
            _ => format!("{}d ago", ago / 86400),
        };
        let mine = std::env::var_os(ipc::ENV)
            .is_some_and(|p| p == ipc::server_socket(&s.name).as_os_str());
        println!(
            "{:<4} {:<9} {:>3} pane{} · sessions: {} · active {ago}{}{}",
            s.name,
            if s.attached { "attached" } else { "detached" },
            s.panes,
            if s.panes == 1 { " " } else { "s" },
            s.sessions.join(", "),
            if s.build != crate::update::BUILD_SHA {
                " · other build"
            } else {
                ""
            },
            if mine { " · (this one)" } else { "" },
        );
    }
    ExitCode::SUCCESS
}

/// `ranma kill NAME`: quit a server and everything in it, without asking.
pub fn kill(name: &str) -> Result<()> {
    let sock = ipc::server_socket(name);
    ipc::send_to(&sock, "action\nquit now")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(name: &str, attached: bool, last_active: u64) -> Status {
        Status {
            name: name.into(),
            attached,
            panes: 1,
            sessions: vec!["main".into()],
            last_active,
            build: String::new(),
        }
    }

    #[test]
    fn choosing_a_server() {
        // None running: a new one, numbered 1.
        assert_eq!(choose(&[], None), ("1".into(), true));
        // The most recently active detached one.
        let all = [st("1", false, 10), st("2", false, 30), st("3", true, 99)];
        assert_eq!(choose(&all, None), ("2".into(), false));
        // All attached (two monitors): a new one, the lowest free number.
        let all = [st("1", true, 1), st("3", true, 1)];
        assert_eq!(choose(&all, None), ("2".into(), true));
        // Named: that one, attached or not, or a new one by that name.
        assert_eq!(choose(&all, Some("3")), ("3".into(), false));
        assert_eq!(choose(&all, Some("work")), ("work".into(), true));
    }

    #[test]
    fn numbers_sort_naturally() {
        let mut v = vec!["10", "2", "work", "1"];
        v.sort_by_key(|n| natural(n));
        assert_eq!(v, ["1", "2", "10", "work"]);
    }
}

//! The ranma you start in a terminal: a client of a ranma server.
//!
//! It picks a server — the one you name, else the most recently used one that
//! no terminal is showing, else a new one — starts it if it has to, and becomes a
//! pipe: keys, mouse and resizes to the server, the server's bytes to the
//! terminal. It keeps no state of its own, so there is nothing to fall out of step
//! (see DESIGN.md, "A daemon, and a client that holds nothing"). Closing the
//! terminal ends the client, never the server. A server can pass the client on
//! to another (the server switcher): it reconnects there, keeping the terminal.

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use crossterm::cursor::SetCursorStyle;
use crossterm::event::{DisableBracketedPaste, DisableFocusChange, DisableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{DisableLineWrap, EnableLineWrap};

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

/// `ranma` / `ranma attach [--steal] NAME`: attach this terminal to a server.
/// Attached by name, it shares the server with any terminal already showing
/// it, unless `steal` sends those away.
pub fn run(name: Option<&str>, steal: bool, mobile: bool) -> Result<ExitCode> {
    let mobile = mobile || mobile_env();
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
    let stream =
        UnixStream::connect(&sock).with_context(|| format!("connecting to {}", sock.display()))?;

    // The terminal is ours from here: raw mode, the alternate screen, and a
    // panic hook that gives it back.
    drop(ratatui::try_init().context("setting up the terminal")?);
    let mut out = std::io::stdout();
    let _ = out.write_all(b"\x1b[22;0t");
    // No autowrap: a terminal smaller than the one driving is sent frames at
    // the driver's size, and with wrapping on, every row too long for it ran
    // onto the next and scrolled the screen. Off, they are cut at its edge.
    let _ = execute!(out, DisableLineWrap);
    let result = attach(stream, &name, steal, mobile);
    crate::input::pop_keyboard_flags();
    let _ = out.write_all(b"\x1b[23;0t");
    let _ = execute!(
        out,
        EnableLineWrap,
        DisableMouseCapture,
        DisableBracketedPaste,
        DisableFocusChange,
        SetCursorStyle::DefaultUserShape
    );
    let _ = out.flush();
    ratatui::restore();
    match result {
        Ok((End::Detached(why), name)) => {
            println!("[ranma {name}: {why}]");
            Ok(ExitCode::SUCCESS)
        }
        Ok((End::Exited, _)) => Ok(ExitCode::SUCCESS),
        Ok((End::Lost, name)) => {
            let log = log_path(&name)
                .map(|p| format!("; its log: {}", p.display()))
                .unwrap_or_default();
            bail!("lost the connection to ranma server {name}{log}")
        }
        Err(e) => Err(e),
    }
}

/// The terminal's cell in pixels, from its window size, if it reports one.
pub fn cell_px() -> Option<(u16, u16)> {
    let w = crossterm::terminal::window_size().ok()?;
    if w.width == 0 || w.height == 0 || w.columns == 0 || w.rows == 0 {
        return None;
    }
    Some((w.width / w.columns, w.height / w.rows))
}

/// `RANMA_MOBILE` set to anything but empty or `0`: this terminal is a phone or
/// a tablet (DESIGN.md, "A mobile view").
pub fn mobile_env() -> bool {
    std::env::var("RANMA_MOBILE").is_ok_and(|v| !v.is_empty() && v != "0")
}

enum End {
    Detached(String),
    Exited,
    Lost,
}

fn attach(mut stream: UnixStream, name: &str, steal: bool, mobile: bool) -> Result<(End, String)> {
    // Before any input thread: the host's replies are read off the terminal
    // here, with any keys typed meanwhile kept for the shell. The colours are
    // asked once; a switch to another server reuses them.
    let crate::hostcolors::Replies {
        colors,
        typed_early,
        outer,
        outer_colors,
        graphics,
        kitty_keys,
    } = crate::hostcolors::query_all(Duration::from_millis(300));
    if kitty_keys {
        crate::input::push_keyboard_flags();
    }
    let inside = std::env::var(ipc::ENV).ok();
    // Only the first server is stolen: a switch later joins whoever is there.
    let greet = |stream: &mut UnixStream, typed_early: Vec<u8>, steal: bool| -> Result<()> {
        let (cols, rows) = crossterm::terminal::size()?;
        stream.write_all(b"attach\n")?;
        proto::send_to_server(
            stream,
            &ToServer::Hello(Hello {
                build: crate::update::BUILD_SHA.to_string(),
                cols,
                rows,
                colors: colors.clone(),
                typed_early,
                inside: inside.clone(),
                remote: crate::pane::over_ssh(),
                outer,
                outer_colors: outer_colors.clone(),
                steal,
                mobile,
                graphics,
                cell_px: crate::client::cell_px(),
            }),
        )?;
        Ok(())
    };
    greet(&mut stream, typed_early, steal)?;

    // The input thread writes to whichever server the client is on now: a
    // switch swaps the stream under it, so keys follow the terminal.
    let writer = Arc::new(Mutex::new(stream.try_clone()?));
    let input = writer.clone();
    std::thread::Builder::new()
        .name("client-input".into())
        .spawn(move || {
            while let Ok(ev) = crossterm::event::read() {
                if crate::winch::from_crossterm(&ev) {
                    continue;
                }
                // A server that has gone is noticed by the reading side, which
                // ends the client; a key lost mid-switch is not worth more.
                let mut w = input.lock().expect("writer lock");
                let _ = proto::send_to_server(&mut *w, &ToServer::Event(ev));
            }
        })?;
    let resizes = writer.clone();
    crate::winch::spawn(crossterm::terminal::size, move |cols, rows| {
        let mut w = resizes.lock().expect("writer lock");
        let _ = proto::send_to_server(
            &mut *w,
            &ToServer::Event(crossterm::event::Event::Resize(cols, rows)),
        );
        true
    })?;

    let mut name = name.to_string();
    let mut out = std::io::stdout();
    loop {
        match proto::read_to_client(&mut stream) {
            Ok(Some(ToClient::Output(bytes))) => {
                out.write_all(&bytes)?;
                out.flush()?;
            }
            Ok(Some(ToClient::Detached(why))) => return Ok((End::Detached(why), name)),
            Ok(Some(ToClient::Exited(_))) => return Ok((End::Exited, name)),
            Ok(Some(ToClient::Switch(next))) => {
                let sock = ipc::server_socket(&next);
                // The server checks this too, from the hello; this is the
                // client refusing to be fed into itself whatever a server says.
                if inside.as_deref() == sock.to_str() {
                    let why = format!("server {next} is the one this terminal runs inside");
                    return Ok((End::Detached(why), name));
                }
                let mut next_stream = match UnixStream::connect(&sock) {
                    Ok(s) => s,
                    Err(e) => {
                        let why = format!("left for server {next}, which did not answer ({e})");
                        return Ok((End::Detached(why), name));
                    }
                };
                greet(&mut next_stream, Vec::new(), false)?;
                *writer.lock().expect("writer lock") = next_stream.try_clone()?;
                // Dropping the old stream is what tells the old server we left.
                stream = next_stream;
                name = next;
            }
            Ok(None) | Err(_) => return Ok((End::Lost, name)),
        }
    }
}

/// `46m ago`: how long since a server last had input, as `ls` and the server
/// switcher say it.
pub fn ago(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs}s ago"),
        60..=3599 => format!("{}m ago", secs / 60),
        3600..=86399 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86400),
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
        let ago = ago(now.saturating_sub(s.last_active));
        let mine = std::env::var_os(ipc::ENV)
            .is_some_and(|p| p == ipc::server_socket(&s.name).as_os_str());
        println!(
            "{:<4} {:<9} {:>3} pane{} · sessions: {} · active {ago}{}{}",
            s.name,
            match s.clients {
                0 | 1 if s.attached => "attached".to_string(),
                0 | 1 => "detached".to_string(),
                n => format!("attached×{n}"),
            },
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
    ipc::send_to(&sock, "action\nquit now").map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(name: &str, attached: bool, last_active: u64) -> Status {
        Status {
            name: name.into(),
            attached,
            clients: usize::from(attached),
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

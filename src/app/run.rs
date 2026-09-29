//! The event loop, in its two forms.
//!
//! **Server** (`ranma server`, what `ranma` starts): the window manager draws
//! into a buffer that goes to whichever client is attached, and keeps running
//! with none. Closing the terminal, or losing the SSH connection, only detaches.
//! **Standalone** (`ranma --standalone`): the same loop drawing to its own
//! terminal, ending with it; for tests and for when a server is not wanted.
//!
//! Everything but where the bytes go and who the input comes from is shared,
//! so the two cannot drift apart.

use std::io::{Stdout, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use crossterm::cursor::SetCursorStyle;
use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, Event,
};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect as RRect;
use ratatui::{Terminal, TerminalOptions, Viewport};

use super::{App, FRAME, cursor_style, spawn_input_thread, watch_config};
use crate::config::Config;
use crate::pane::AppEvent;
use crate::proto::{self, ToClient};
use crate::render::{self, CursorState};

/// Where a frame's bytes go: the terminal ranma runs in, or a buffer the
/// server sends to its client after each round of the loop.
enum Sink {
    Stdout(Stdout),
    /// Shared with the loop, which takes what was written after each round
    /// (ratatui keeps its backend's writer to itself).
    Buffer(Arc<Mutex<Vec<u8>>>),
}

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Sink::Stdout(s) => s.write(buf),
            Sink::Buffer(b) => b.lock().expect("buffer lock").write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Sink::Stdout(s) => s.flush(),
            Sink::Buffer(_) => Ok(()),
        }
    }
}

type Term = Terminal<CrosstermBackend<Sink>>;

fn terminal(sink: Sink, cols: u16, rows: u16) -> Result<Term> {
    // A fixed viewport sized by hand: a server has no terminal of its own to
    // ask, and the standalone loop resizes on the same events.
    Ok(Terminal::with_options(
        CrosstermBackend::new(sink),
        TerminalOptions {
            viewport: Viewport::Fixed(RRect::new(0, 0, cols.max(1), rows.max(1))),
        },
    )?)
}

/// Start over at `cols` x `rows`: a new terminal with no previous frame, and
/// the screen cleared, so the next draw paints everything. Used on attach and on
/// every resize instead of ratatui's resize/clear, which ask the backend for the
/// terminal's size — and a server has no terminal to ask.
fn restart(
    term: &mut Term,
    buffer: Option<&Arc<Mutex<Vec<u8>>>>,
    cols: u16,
    rows: u16,
) -> Result<()> {
    let sink = match buffer {
        Some(b) => Sink::Buffer(b.clone()),
        None => Sink::Stdout(std::io::stdout()),
    };
    *term = terminal(sink, cols, rows)?;
    term.backend_mut().write_all(b"\x1b[H\x1b[2J")?;
    Ok(())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The attached client of a server.
struct Client {
    id: u64,
    writer: UnixStream,
    /// Its build: only a client that knows `ToClient::Switch` is sent one.
    build: String,
    /// What it said when it attached: handed over with it on an upgrade.
    hello: proto::Hello,
}

impl Client {
    fn send(&mut self, m: &ToClient) -> bool {
        proto::send_to_client(&mut self.writer, m).is_ok()
    }
}

/// Standalone: ranma in this terminal, ending with it.
pub fn run(config: Config) -> Result<()> {
    let (tx, rx) = mpsc::channel();
    // Raw mode, the alternate screen, and a panic hook that puts the terminal
    // back before the panic message prints.
    drop(ratatui::try_init().context("setting up the terminal")?);
    let (cols, rows) = crossterm::terminal::size()?;
    let mut term = terminal(Sink::Stdout(std::io::stdout()), cols, rows)?;
    let result = (|| -> Result<()> {
        execute!(term.backend_mut(), EnableBracketedPaste, EnableFocusChange)?;
        // Keep the host's title to give back on exit (xterm's title stack).
        term.backend_mut().write_all(b"\x1b[22;0t")?;
        // Before the input thread exists: the replies are read straight off the
        // terminal here, and none may be left for crossterm to take for keys.
        let (host_colors, typed_early, outer) =
            crate::hostcolors::query_all(Duration::from_millis(300));
        let mut app = App::new(config, tx.clone(), cols, rows);
        app.host_colors = host_colors;
        app.set_outer(outer);
        app.client_remote = crate::pane::over_ssh();
        let _ipc = listen(&mut app, &tx, None);
        app.open_pane(None).context("starting the first pane")?;
        if !typed_early.is_empty()
            && let Some(p) = app.focused_pane()
        {
            p.write(typed_early);
        }
        app.after_event();
        spawn_input_thread(tx.clone());
        start_background(&app, &tx);
        let _watcher = watch_config(tx);
        event_loop(&mut app, &rx, &mut term, None, None, None)
    })();
    let out = term.backend_mut();
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
    result
}

/// Server: ranma with no terminal of its own, serving clients on the socket
/// named `name`, until its last pane closes or it is quit.
pub fn run_server(config: Config, name: &str) -> Result<()> {
    remember_exe();
    let (tx, rx) = mpsc::channel();
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let mut term = terminal(Sink::Buffer(buffer.clone()), 80, 24)?;
    let mut app = App::new(config, tx.clone(), 80, 24);
    let path = crate::ipc::server_socket(name);
    let _ipc = listen(&mut app, &tx, Some(path));
    if _ipc.is_none() {
        anyhow::bail!("could not listen; see the status above");
    }
    start_background(&app, &tx);
    let _watcher = watch_config(tx);
    event_loop(&mut app, &rx, &mut term, Some(name), Some(&buffer), None)
}

/// The binary a server was started from. After `install.sh` replaces it, this
/// path holds the new build, while `/proc/self/exe` still reads the old one.
static SERVER_EXE: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

fn remember_exe() {
    if let Ok(p) = std::env::current_exe() {
        // A path that was already replaced reads "… (deleted)": not ours.
        let p = std::path::PathBuf::from(
            p.to_string_lossy()
                .trim_end_matches(" (deleted)")
                .to_string(),
        );
        let _ = SERVER_EXE.set(p);
    }
}

/// A server that took a new build: everything the old one handed over,
/// taken back. If it cannot be, the old build (kept aside by the old server)
/// is exec'd with the same handover, so no shell is lost to a bad build.
pub fn run_server_resume(
    config: Config,
    name: &str,
    file: &std::path::Path,
    fallback: Option<&std::path::Path>,
) -> Result<()> {
    remember_exe();
    let fail = |e: anyhow::Error, h: Option<&super::upgrade::Handover>| -> anyhow::Error {
        eprintln!("ranma: taking over from the old build failed: {e:#}");
        if let Some(prev) = fallback {
            // Keep every descriptor for the old build, as the old server did.
            if let Some(h) = h {
                keep_across_exec(h);
            }
            eprintln!("ranma: going back to the old build ({})", prev.display());
            use std::os::unix::process::CommandExt;
            let err = std::process::Command::new(prev)
                .args(["server", "--name", name, "--resume"])
                .arg(file)
                .exec();
            eprintln!("ranma: could not go back: {err}");
        }
        e
    };
    let h = match super::upgrade::check(file) {
        Ok(h) => h,
        Err(e) => return Err(fail(e, None)),
    };
    let (tx, rx) = mpsc::channel();
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let mut term = terminal(Sink::Buffer(buffer.clone()), h.cols, h.rows)?;
    let alt: Vec<std::os::fd::RawFd> = h
        .state
        .panes
        .iter()
        .filter(|p| p.was_full_screen())
        .map(|p| p.fd)
        .collect();
    // Kept aside in case: the descriptors must stay open for a fallback.
    let listener_fd = h.listener_fd;
    let client_h = h.client.as_ref().map(|c| (c.fd, c.hello.clone()));
    let (cols, rows) = (h.cols, h.rows);
    // SAFETY: the descriptors in the handover are the ones the old server
    // kept open across its exec for exactly this.
    let mut app = match unsafe { App::take_over(config, tx.clone(), h.state, cols, rows) } {
        Ok(app) => app,
        Err(e) => {
            // The handover was consumed; read it again for the fallback's sake.
            let again = super::upgrade::read(file).ok();
            return Err(fail(e, again.as_ref()));
        }
    };
    let path = crate::ipc::server_socket(name);
    // SAFETY: as above.
    let _ipc = match unsafe { crate::ipc::adopt_listener(tx.clone(), listener_fd, path) } {
        Ok(l) => Some(l),
        Err(e) => {
            app.status = Some(format!("ranma notify unavailable: {e:#}"));
            None
        }
    };
    let mut client = None;
    if let Some((fd, hello)) = client_h {
        use std::os::fd::FromRawFd;
        let _ = crate::pty::set_cloexec(fd, true);
        // SAFETY: as above.
        let stream = unsafe { UnixStream::from_raw_fd(fd) };
        if let Some((id, writer)) = crate::ipc::resume_client(stream, &tx) {
            restart(&mut term, Some(&buffer), hello.cols, hello.rows)?;
            app.host_colors = hello.colors.clone();
            app.client_inside = hello.inside.clone();
            app.client_remote = hello.remote;
            app.set_outer(hello.outer);
            client = Some(Client {
                id,
                writer,
                build: hello.build.clone(),
                hello,
            });
        }
    }
    // What a full-screen program had drawn was not handed over: it draws again.
    for fd in alt {
        crate::pty::redraw(fd);
    }
    let _ = std::fs::remove_file(file);
    if let Some(prev) = fallback {
        let _ = std::fs::remove_file(prev);
    }
    app.toast(
        format!("ranma upgraded to {}", crate::update::BUILD_SHA),
        crate::toast::Level::Normal,
        None,
    );
    app.after_event();
    start_background(&app, &tx);
    let _watcher = watch_config(tx);
    event_loop(&mut app, &rx, &mut term, Some(name), Some(&buffer), client)
}

/// Clear close-on-exec on everything a new build takes over: the PTY masters,
/// the listening socket, the client's connection.
fn keep_across_exec(h: &super::upgrade::Handover) {
    for p in &h.state.panes {
        let _ = crate::pty::set_cloexec(p.fd, false);
    }
    let _ = crate::pty::set_cloexec(h.listener_fd, false);
    if let Some(c) = &h.client {
        let _ = crate::pty::set_cloexec(c.fd, false);
    }
}

/// Take the new build in place (DESIGN.md, "Upgrading a server in place").
/// Returns only if it did not happen, with why; the server then goes on as
/// before, only full-screen programs asked to draw again.
fn upgrade(
    app: &mut App,
    client: Option<&Client>,
    name: &str,
    cols: u16,
    rows: u16,
    reply: &Sender<std::result::Result<String, String>>,
    written: &Receiver<()>,
) -> std::result::Result<(), String> {
    use std::os::fd::AsRawFd;
    let exe = SERVER_EXE
        .get()
        .cloned()
        .ok_or("the binary this server started from is unknown")?;
    if !exe.is_file() {
        return Err(format!("{} is not there any more", exe.display()));
    }
    let dir = crate::ipc::server_dir();
    let file = dir.join(format!("handover-{name}.json"));
    let prev = dir.join(format!("previous-{name}"));
    let listener_fd = crate::ipc::listener_fd().ok_or("this server has no socket")?;

    super::upgrade::hold_output();
    let state = app.hand_over();
    let alt: Vec<_> = state
        .panes
        .iter()
        .filter(|p| p.was_full_screen())
        .map(|p| p.fd)
        .collect();
    let give_up = |why: String| {
        super::upgrade::release_output();
        for fd in &alt {
            crate::pty::redraw(*fd);
        }
        let _ = std::fs::remove_file(&file);
        why
    };
    let h = super::upgrade::Handover {
        version: super::upgrade::VERSION,
        name: name.to_string(),
        listener_fd,
        client: client.map(|c| super::upgrade::ClientHandover {
            fd: c.writer.as_raw_fd(),
            hello: c.hello.clone(),
        }),
        cols,
        rows,
        state,
    };
    super::upgrade::write(&file, &h).map_err(|e| give_up(format!("{e:#}")))?;
    // The new build reads it first, in a process of its own: if it cannot,
    // nothing is exec'd and nothing is lost.
    let check = std::process::Command::new(&exe)
        .arg("--check-handover")
        .arg(&file)
        .output()
        .map_err(|e| give_up(format!("running the new build: {e}")))?;
    if !check.status.success() {
        let why = String::from_utf8_lossy(&check.stderr).trim().to_string();
        return Err(give_up(format!(
            "the new build refused the handover: {why}"
        )));
    }
    // The old build, kept to go back to if the new one fails anyway.
    std::fs::copy("/proc/self/exe", &prev)
        .map_err(|e| give_up(format!("keeping the old build: {e}")))?;
    let _ = reply.send(Ok(format!(
        "server {name} is taking the new build
"
    )));
    let _ = written.recv_timeout(Duration::from_secs(2));
    keep_across_exec(&h);
    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new(&exe)
        .args(["server", "--name", name, "--resume"])
        .arg(&file)
        .arg("--fallback")
        .arg(&prev)
        .exec();
    // Still here: the exec failed. Everything goes back to how it was.
    for p in &h.state.panes {
        let _ = crate::pty::set_cloexec(p.fd, true);
    }
    let _ = crate::pty::set_cloexec(listener_fd, true);
    if let Some(c) = &h.client {
        let _ = crate::pty::set_cloexec(c.fd, true);
    }
    let _ = std::fs::remove_file(&prev);
    Err(give_up(format!("exec {}: {err}", exe.display())))
}

fn listen(
    app: &mut App,
    tx: &Sender<AppEvent>,
    path: Option<std::path::PathBuf>,
) -> Option<crate::ipc::Listening> {
    let res = match path {
        Some(p) => crate::ipc::listen_at(tx.clone(), p),
        None => crate::ipc::listen(tx.clone()),
    };
    match res {
        Ok(l) => Some(l),
        // Not fatal standalone: only `ranma notify` and friends are lost.
        Err(e) => {
            app.status = Some(format!("ranma notify unavailable: {e:#}"));
            eprintln!("ranma: {e:#}");
            None
        }
    }
}

fn start_background(app: &App, tx: &Sender<AppEvent>) {
    if app.config.settings.updates != crate::config::UpdateMode::Off {
        let hours = app.config.settings.update_check_hours;
        crate::update::spawn_checker(tx.clone(), Duration::from_secs_f64(hours * 3600.0));
    }
}

fn event_loop(
    app: &mut App,
    rx: &Receiver<AppEvent>,
    term: &mut Term,
    server: Option<&str>,
    buffer: Option<&Arc<Mutex<Vec<u8>>>>,
    resumed: Option<Client>,
) -> Result<()> {
    let mut client: Option<Client> = resumed;
    let mut last_active = now_secs();
    let mut last_draw = Instant::now() - FRAME;
    let mut last_cursor: Option<CursorState> = None;
    let mut mouse = false;

    loop {
        // Idle means blocked here: no timeout unless a frame is owed or a timer
        // (a bar module, a pending reload) is due. Zero frames, zero wakeups.
        let can_draw = server.is_none() || client.is_some();
        let frame_due = (app.dirty && can_draw).then(|| last_draw + FRAME);
        let deadline = [frame_due, app.next_deadline()].into_iter().flatten().min();
        let first = match deadline {
            Some(d) => match rx.recv_timeout(d.saturating_duration_since(Instant::now())) {
                Ok(ev) => Some(ev),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            },
            None => match rx.recv() {
                Ok(ev) => Some(ev),
                Err(_) => break,
            },
        };
        let events = first
            .into_iter()
            .chain(std::iter::from_fn(|| rx.try_recv().ok()));
        for ev in events.collect::<Vec<_>>() {
            match ev {
                AppEvent::Attach { id, writer, hello } => {
                    if let Some(mut old) = client.take() {
                        old.send(&ToClient::Detached("taken over by another terminal".into()));
                    }
                    client = Some(Client {
                        id,
                        writer,
                        build: hello.build.clone(),
                        hello: (*hello).clone(),
                    });
                    last_active = now_secs();
                    restart(term, buffer, hello.cols, hello.rows)?;
                    attached(app, &hello)?;
                    // Modes and cursor are re-sent to the new terminal below.
                    mouse = !app.wants_mouse();
                    last_cursor = None;
                    execute!(term.backend_mut(), EnableBracketedPaste, EnableFocusChange)?;
                }
                AppEvent::ClientInput(id, ev) => {
                    if client.as_ref().is_some_and(|c| c.id == id) {
                        last_active = now_secs();
                        input(app, term, buffer, ev)?;
                    }
                }
                AppEvent::ClientGone(id) => {
                    if client.as_ref().is_some_and(|c| c.id == id) {
                        client = None;
                    }
                }
                AppEvent::Status(reply) => {
                    let _ = reply.send(proto::Status {
                        name: server.unwrap_or("standalone").to_string(),
                        attached: client.is_some(),
                        panes: app.panes.len(),
                        sessions: app.session_list().into_iter().map(|s| s.1).collect(),
                        last_active,
                        build: crate::update::BUILD_SHA.to_string(),
                    });
                }
                AppEvent::Upgrade { reply, written } => {
                    let result = match server {
                        None => Err("a standalone ranma cannot take a new build in place".into()),
                        Some(name) => {
                            let size = term.get_frame().area();
                            upgrade(
                                app,
                                client.as_ref(),
                                name,
                                size.width,
                                size.height,
                                &reply,
                                &written,
                            )
                        }
                    };
                    if let Err(why) = result {
                        app.toast(format!("upgrade: {why}"), crate::toast::Level::Urgent, None);
                        let _ = reply.send(Err(why));
                    }
                }
                AppEvent::Input(ev) => input(app, term, buffer, ev)?,
                other => app.handle(other),
            }
        }
        app.run_timers(Instant::now());
        app.after_event();
        app.report_outward();
        if !app.host_out.is_empty() {
            let out = term.backend_mut();
            for bytes in app.host_out.drain(..) {
                out.write_all(&bytes)?;
            }
            out.flush()?;
        }
        if app.quit {
            break;
        }
        if std::mem::take(&mut app.detach_requested) {
            match client.take() {
                Some(mut c) => {
                    c.send(&ToClient::Detached("detached".into()));
                }
                None => {
                    app.status = Some("nothing to detach from (ranma --standalone)".into());
                }
            }
        }
        if let Some(to) = app.switch_requested.take() {
            match refuse_switch(&to, client.as_ref(), server, app.client_inside.as_deref()) {
                Some(why) => app.status = Some(why),
                None => {
                    if let Some(mut c) = client.take() {
                        c.send(&ToClient::Switch(to));
                    }
                }
            }
        }
        if app.wants_mouse() != mouse {
            mouse = app.wants_mouse();
            if mouse {
                execute!(term.backend_mut(), EnableMouseCapture)?;
            } else {
                execute!(term.backend_mut(), DisableMouseCapture)?;
            }
        }
        let can_draw = server.is_none() || client.is_some();
        if app.dirty && can_draw && last_draw.elapsed() >= FRAME {
            app.begin_frame();
            let mut cursor = None;
            term.draw(|f| cursor = render::draw(f, app))?;
            if cursor != last_cursor {
                if let Some(c) = cursor {
                    execute!(term.backend_mut(), cursor_style(c))?;
                }
                last_cursor = cursor;
            }
            last_draw = Instant::now();
        }
        // A server sends what the round wrote; with no client it goes nowhere.
        if let Some(buf) = buffer {
            let bytes = std::mem::take(&mut *buf.lock().expect("buffer lock"));
            if !bytes.is_empty()
                && let Some(c) = client.as_mut()
                && !c.send(&ToClient::Output(bytes))
            {
                client = None;
            }
        }
    }
    if let Some(mut c) = client.take() {
        c.send(&ToClient::Exited("ranma exited".into()));
    }
    close_all(app);
    Ok(())
}

/// Hang up every pane's program and wait (briefly) until they have gone, so
/// quitting ranma takes their shells, and those shells' jobs, with it.
fn close_all(app: &mut App) {
    let handles: Vec<_> = app.panes.drain().filter_map(|(_, p)| p.close()).collect();
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::spawn(move || {
        for h in handles {
            let _ = h.join();
        }
        let _ = done_tx.send(());
    });
    // A program that ignores its hangup does not get to keep ranma alive.
    let _ = done_rx.recv_timeout(Duration::from_secs(3));
}

/// Why this terminal cannot move to server `to`, or `None` when it can.
fn refuse_switch(
    to: &str,
    client: Option<&Client>,
    server: Option<&str>,
    inside: Option<&str>,
) -> Option<String> {
    let Some(me) = server else {
        return Some("ranma --standalone has no other servers to move to".into());
    };
    let Some(c) = client else {
        return Some("no terminal is attached to move".into());
    };
    let sock = crate::ipc::server_socket(to);
    if to == me {
        return Some(format!("this terminal is on server {to} already"));
    }
    if c.build != crate::update::BUILD_SHA {
        return Some(format!(
            "this terminal runs an older ranma that cannot switch: detach (leader d) \
             and run `ranma attach {to}`"
        ));
    }
    if inside == sock.to_str() {
        return Some(format!("this terminal runs inside server {to}"));
    }
    if UnixStream::connect(&sock).is_err() {
        return Some(format!("no ranma server named {to} (ranma ls lists them)"));
    }
    None
}

/// A client attached (the terminal was already restarted at its size): take its
/// colours and early keys, and draw everything again.
fn attached(app: &mut App, hello: &proto::Hello) -> Result<()> {
    app.host_colors = hello.colors.clone();
    app.client_inside = hello.inside.clone();
    app.client_remote = hello.remote;
    app.set_outer(hello.outer);
    app.handle(AppEvent::Input(Event::Resize(hello.cols, hello.rows)));
    // The first client of a new server: its first pane opens now, at the size
    // the client's terminal gives it.
    if app.panes.is_empty() {
        app.open_pane(None).context("starting the first pane")?;
    }
    if !hello.typed_early.is_empty()
        && let Some(p) = app.focused_pane()
    {
        p.write(hello.typed_early.clone());
    }
    if hello.build != crate::update::BUILD_SHA {
        app.toast(
            "this ranma server runs a different build than the ranma you started; \
             quit it (leader Delete) and start again to switch",
            crate::toast::Level::Normal,
            Some(Duration::from_secs(15)),
        );
    }
    // The terminal's title is the new client's now: say who we are again (and
    // where, if this client came over SSH), even with nothing else changed.
    app.host_title.clear();
    app.announce();
    app.dirty = true;
    app.after_event();
    Ok(())
}

fn input(
    app: &mut App,
    term: &mut Term,
    buffer: Option<&Arc<Mutex<Vec<u8>>>>,
    ev: Event,
) -> Result<()> {
    if let Event::Resize(w, h) = ev {
        restart(term, buffer, w, h)?;
    }
    app.handle(AppEvent::Input(ev));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(build: &str) -> Client {
        let (writer, _) = UnixStream::pair().unwrap();
        Client {
            id: 1,
            writer,
            build: build.into(),
            hello: proto::Hello {
                build: build.into(),
                cols: 80,
                rows: 24,
                colors: Default::default(),
                typed_early: Vec::new(),
                inside: None,
                remote: false,
                outer: None,
            },
        }
    }

    #[test]
    fn a_switch_is_refused_with_the_reason() {
        let here = client(crate::update::BUILD_SHA);
        let why = |to, c: Option<&Client>, server, inside| {
            refuse_switch(to, c, server, inside).unwrap_or_default()
        };
        assert!(why("2", Some(&here), None, None).contains("standalone"));
        assert!(why("2", None, Some("1"), None).contains("no terminal"));
        assert!(why("1", Some(&here), Some("1"), None).contains("already"));
        assert!(why("2", Some(&client("older")), Some("1"), None).contains("ranma attach 2"));
        let inside = crate::ipc::server_socket("2");
        assert!(why("2", Some(&here), Some("1"), inside.to_str()).contains("inside"));
        // Nobody listens on a name no server has.
        assert!(
            why("no-such-server-here", Some(&here), Some("1"), None).contains("no ranma server")
        );
    }
}

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
        let (host_colors, typed_early) = crate::hostcolors::query(Duration::from_millis(300));
        let mut app = App::new(config, tx.clone(), cols, rows);
        app.host_colors = host_colors;
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
        event_loop(&mut app, &rx, &mut term, None, None)
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
    event_loop(&mut app, &rx, &mut term, Some(name), Some(&buffer))
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
) -> Result<()> {
    let mut client: Option<Client> = None;
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
                    client = Some(Client { id, writer });
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
                AppEvent::Input(ev) => input(app, term, buffer, ev)?,
                other => app.handle(other),
            }
        }
        app.run_timers(Instant::now());
        app.after_event();
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

/// A client attached (the terminal was already restarted at its size): take its
/// colours and early keys, and draw everything again.
fn attached(app: &mut App, hello: &proto::Hello) -> Result<()> {
    app.host_colors = hello.colors.clone();
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
    // The terminal's title is the new client's now: say who we are again.
    app.host_title.clear();
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

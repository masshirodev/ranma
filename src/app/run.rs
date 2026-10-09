//! The event loop, in its two forms.
//!
//! **Server** (`ranma server`, what `ranma` starts): the window manager draws
//! into a buffer that goes to every client attached, and keeps running with
//! none. Closing the terminal, or losing the SSH connection, only detaches.
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
    EnableFocusChange, EnableMouseCapture, Event, MouseEventKind,
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

/// How long a write to a client may block before that client is dropped. A
/// terminal that stopped reading (a tablet asleep behind an SSH connection)
/// must not freeze the screen of every other terminal on the server.
const CLIENT_WRITE_TIMEOUT: Duration = Duration::from_secs(1);

/// A client attached to a server.
struct Client {
    id: u64,
    writer: UnixStream,
    /// Its build: only a client that knows `ToClient::Switch` is sent one.
    build: String,
    /// What it said when it attached, its size kept current: handed over with
    /// it on an upgrade.
    hello: proto::Hello,
    /// The press that made it drive is being swallowed, with the drag and
    /// release that follow it (see [`Clients::arrive`]).
    swallowing: bool,
}

impl Client {
    fn new(id: u64, writer: UnixStream, hello: proto::Hello) -> Client {
        let _ = writer.set_write_timeout(Some(CLIENT_WRITE_TIMEOUT));
        Client {
            id,
            writer,
            build: hello.build.clone(),
            hello,
            swallowing: false,
        }
    }

    fn send(&mut self, m: &ToClient) -> bool {
        proto::send_to_client(&mut self.writer, m).is_ok()
    }

    /// Close its connection outright, since its reader thread holds a clone of
    /// it: after a write that timed out, the stream is mid-frame and useless.
    fn hang_up(&self) {
        let _ = self.writer.shutdown(std::net::Shutdown::Both);
    }
}

/// The clients of a server, most recently active first (DESIGN.md, "Several
/// terminals on one server"). The front one **drives**: the screen has its
/// size, its colours and its title. Every client is sent the same bytes.
#[derive(Default)]
struct Clients {
    list: Vec<Client>,
    /// The client the screen was last set up for.
    driving: Option<u64>,
}

impl Clients {
    fn position(&self, id: u64) -> Option<usize> {
        self.list.iter().position(|c| c.id == id)
    }

    fn get(&self, id: u64) -> Option<&Client> {
        self.list.iter().find(|c| c.id == id)
    }

    fn remove(&mut self, id: u64) -> Option<Client> {
        let at = self.position(id)?;
        Some(self.list.remove(at))
    }

    /// Input from the client at `at`: it is the most recently active now.
    fn touch(&mut self, at: usize) {
        let c = self.list.remove(at);
        self.list.insert(0, c);
    }

    /// Input from the client at `at`: someone at a terminal that is not driving
    /// makes it drive. Returns whether the event goes on to the app. A click
    /// that takes the drive does not: its position was read off the screen as
    /// it was before the resize, so it would land on whatever is there after
    /// it. The drag and release that follow it are swallowed with it. A key or
    /// a paste has no position and goes through.
    fn arrive(&mut self, at: usize, ev: &Event) -> bool {
        if let Event::Mouse(m) = ev
            && self.list[at].swallowing
        {
            if matches!(m.kind, MouseEventKind::Up(_)) {
                self.list[at].swallowing = false;
            }
            return false;
        }
        if at == 0 {
            return true;
        }
        if !is_presence(ev) {
            // The pointer passing over a terminal that is not driving: its
            // position is on a screen of another size, so it hovers nothing.
            // A release still ends a drag that began there before it lost the
            // screen, so that drag is not left held.
            return !matches!(ev, Event::Mouse(m) if !matches!(m.kind, MouseEventKind::Up(_)));
        }
        self.touch(at);
        match ev {
            Event::Mouse(m) => {
                self.list[0].swallowing = matches!(m.kind, MouseEventKind::Down(_));
                false
            }
            _ => true,
        }
    }

    /// The front client's hello when the screen is not set up for it yet:
    /// after it typed, or after the one driving left.
    fn new_driver(&mut self) -> Option<proto::Hello> {
        let front = self.list.first().map(|c| c.id);
        if front == self.driving {
            return None;
        }
        self.driving = front;
        self.list.first().map(|c| c.hello.clone())
    }

    /// Where the screen is drawn, and so the size every client is sent.
    fn size(&self) -> Option<(u16, u16)> {
        self.list.first().map(|c| (c.hello.cols, c.hello.rows))
    }
}

/// Input that means someone is at that terminal, and so moves the screen to
/// it. Not a resize, and not a focus report: every terminal answers the
/// focus-reporting mode with one as it is (re)enabled on each attach. Of the
/// mouse, only a press or the wheel: with motion reporting on, the pointer
/// crossing a terminal on its way elsewhere (a desk seen through VNC) would
/// otherwise take the screen from the one being typed in.
fn is_presence(ev: &Event) -> bool {
    match ev {
        Event::Key(_) | Event::Paste(_) => true,
        Event::Mouse(m) => !matches!(
            m.kind,
            MouseEventKind::Moved | MouseEventKind::Drag(_) | MouseEventKind::Up(_)
        ),
        _ => false,
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
        let replies = crate::hostcolors::query_all(Duration::from_millis(300));
        let typed_early = replies.typed_early;
        let mut app = App::new(config, tx.clone(), cols, rows);
        app.host_colors = replies.colors;
        app.set_outer(replies.outer);
        app.set_outer_colors(replies.outer_colors);
        app.host_graphics = replies.graphics;
        if replies.kitty_keys {
            crate::input::push_keyboard_flags();
        }
        let _ipc = listen(&mut app, &tx, None);
        app.driven_by(crate::client::mobile_env(), crate::pane::over_ssh());
        app.open_pane(None).context("starting the first pane")?;
        if !typed_early.is_empty()
            && let Some(p) = app.focused_pane()
        {
            p.write(typed_early);
        }
        app.after_event();
        spawn_input_thread(tx.clone());
        start_background(&app, &tx);
        app.watcher = watch_config(tx);
        event_loop(&mut app, &rx, &mut term, None, None, Vec::new())
    })();
    crate::input::pop_keyboard_flags();
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
    app.watcher = watch_config(tx);
    // A server started, not upgraded: it offers what the last one of its
    // name left (after a reboot, say).
    app.start_snapshots(name, true);
    let result = event_loop(
        &mut app,
        &rx,
        &mut term,
        Some(name),
        Some(&buffer),
        Vec::new(),
    );
    app.write_snapshot();
    result
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
    let clients_h: Vec<_> = h
        .client
        .iter()
        .chain(&h.others)
        .map(|c| (c.fd, c.hello.clone()))
        .collect();
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
    // The one that drove first: the event loop sets the screen up for it.
    let mut clients = Vec::new();
    for (fd, hello) in clients_h {
        use std::os::fd::FromRawFd;
        let _ = crate::pty::set_cloexec(fd, true);
        // SAFETY: as above.
        let stream = unsafe { UnixStream::from_raw_fd(fd) };
        if let Some((id, writer)) = crate::ipc::resume_client(stream, &tx) {
            clients.push(Client::new(id, writer, hello));
        }
    }
    // The screen's size is the frame the handover was taken at. The hello of the
    // one driving says the size it attached with, which an older build never
    // kept current: drawn at that, a resize since left every border doubled.
    if let Some(c) = clients.first_mut() {
        c.hello.cols = cols;
        c.hello.rows = rows;
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
    app.watcher = watch_config(tx);
    // Taking a new build keeps the snapshots going and asks nothing.
    app.start_snapshots(name, false);
    let result = event_loop(&mut app, &rx, &mut term, Some(name), Some(&buffer), clients);
    app.write_snapshot();
    result
}

/// Clear close-on-exec on everything a new build takes over: the PTY masters,
/// the listening socket, every client's connection.
fn keep_across_exec(h: &super::upgrade::Handover) {
    for p in &h.state.panes {
        let _ = crate::pty::set_cloexec(p.fd, false);
    }
    let _ = crate::pty::set_cloexec(h.listener_fd, false);
    for c in h.client.iter().chain(&h.others) {
        let _ = crate::pty::set_cloexec(c.fd, false);
    }
}

/// Take the new build in place (DESIGN.md, "Upgrading a server in place").
/// Returns only if it did not happen, with why; the server then goes on as
/// before, only full-screen programs asked to draw again.
fn upgrade(
    app: &mut App,
    clients: &[Client],
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
        client: clients.first().map(|c| super::upgrade::ClientHandover {
            fd: c.writer.as_raw_fd(),
            hello: c.hello.clone(),
        }),
        others: clients
            .iter()
            .skip(1)
            .map(|c| super::upgrade::ClientHandover {
                fd: c.writer.as_raw_fd(),
                hello: c.hello.clone(),
            })
            .collect(),
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
    for c in h.client.iter().chain(&h.others) {
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
    resumed: Vec<Client>,
) -> Result<()> {
    let mut clients = Clients {
        list: resumed,
        driving: None,
    };
    if let Some(hello) = clients.new_driver() {
        drive(app, term, buffer, &hello)?;
    }
    let mut last_active = now_secs();
    let mut last_draw = Instant::now() - FRAME;
    let mut last_cursor: Option<CursorState> = None;
    let mut mouse = false;

    loop {
        // Idle means blocked here: no timeout unless a frame is owed or a timer
        // (a bar module, a pending reload) is due. Zero frames, zero wakeups.
        let can_draw = server.is_none() || !clients.list.is_empty();
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
                    if hello.steal {
                        for mut old in clients.list.drain(..) {
                            old.send(&ToClient::Detached("taken over by another terminal".into()));
                        }
                    }
                    let joining = !clients.list.is_empty();
                    let mut client = Client::new(id, writer, (*hello).clone());
                    // Its terminal has none of the images the panes show.
                    if hello.graphics && !app.graphics.is_empty() {
                        client.send(&ToClient::Output(app.graphics.replay().concat()));
                    }
                    clients.list.push(client);
                    last_active = now_secs();
                    if joining {
                        // A peek moves nothing: the screen keeps the size of the
                        // terminal that drives it, drawn whole again for everyone.
                        repaint(app, term, buffer, &clients)?;
                        let n = clients.list.len();
                        app.toast(
                            format!(
                                "{n} terminals show this server; it takes the size of the one last typed in"
                            ),
                            crate::toast::Level::Normal,
                            Some(Duration::from_secs(6)),
                        );
                    } else if let Some(hello) = clients.new_driver() {
                        drive(app, term, buffer, &hello)?;
                    }
                    attached(app, &hello)?;
                    // Modes and cursor are re-sent to the new terminal below.
                    mouse = !app.wants_mouse();
                    last_cursor = None;
                    execute!(term.backend_mut(), EnableBracketedPaste, EnableFocusChange)?;
                }
                AppEvent::ClientInput(id, ev) => {
                    let Some(at) = clients.position(id) else {
                        continue;
                    };
                    last_active = now_secs();
                    match ev {
                        Event::Resize(w, h) => {
                            clients.list[at].hello.cols = w;
                            clients.list[at].hello.rows = h;
                            if at == 0 {
                                input(app, term, buffer, ev)?;
                            } else {
                                // Its terminal redrew what it had: send it the
                                // screen again, at the size it already had.
                                repaint(app, term, buffer, &clients)?;
                            }
                        }
                        // Only the terminal the screen follows is focused, as far
                        // as the programs in it can tell.
                        Event::FocusGained | Event::FocusLost if at != 0 => {}
                        _ => {
                            let pass = clients.arrive(at, &ev);
                            if let Some(hello) = clients.new_driver() {
                                drive(app, term, buffer, &hello)?;
                            }
                            if pass {
                                input(app, term, buffer, ev)?;
                            }
                        }
                    }
                    requests(app, &mut clients, Some(id), server);
                }
                AppEvent::ClientGone(id) => {
                    clients.remove(id);
                }
                AppEvent::Status(reply) => {
                    let _ = reply.send(proto::Status {
                        name: server.unwrap_or("standalone").to_string(),
                        attached: !clients.list.is_empty(),
                        clients: clients.list.len(),
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
                                &clients.list,
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
        // Images go only to terminals that can show them: to one that
        // cannot, they would be bytes on screen.
        if server.is_some() {
            app.host_graphics = clients.list.iter().any(|c| c.hello.graphics);
        }
        if !app.graphics_out.is_empty() {
            let bytes = app.graphics_out.concat();
            app.graphics_out.clear();
            if server.is_some() {
                let out = ToClient::Output(bytes);
                for c in clients.list.iter_mut().filter(|c| c.hello.graphics) {
                    c.send(&out);
                }
            } else if app.host_graphics {
                let out = term.backend_mut();
                out.write_all(&bytes)?;
                out.flush()?;
            }
        }
        if app.quit {
            break;
        }
        // Asked for by something other than a terminal (`ranma action`, a Lua
        // hook): it is the terminal the screen follows that goes.
        let front = clients.list.first().map(|c| c.id);
        requests(app, &mut clients, front, server);
        // The one driving left: the screen moves to the next most recent.
        if let Some(hello) = clients.new_driver() {
            drive(app, term, buffer, &hello)?;
        }
        if app.wants_mouse() != mouse {
            mouse = app.wants_mouse();
            if mouse {
                execute!(term.backend_mut(), EnableMouseCapture)?;
            } else {
                execute!(term.backend_mut(), DisableMouseCapture)?;
            }
        }
        let can_draw = server.is_none() || !clients.list.is_empty();
        if app.dirty && can_draw && last_draw.elapsed() >= FRAME {
            app.begin_frame();
            let mut cursor = None;
            term.draw(|f| cursor = render::draw(f, app))?;
            app.screen_drawn_now();
            if cursor != last_cursor {
                if let Some(c) = cursor {
                    execute!(term.backend_mut(), cursor_style(c))?;
                }
                last_cursor = cursor;
            }
            last_draw = Instant::now();
        }
        // A server sends what the round wrote, the same bytes to every client;
        // with none it goes nowhere.
        if let Some(buf) = buffer {
            let bytes = std::mem::take(&mut *buf.lock().expect("buffer lock"));
            if !bytes.is_empty() {
                let out = ToClient::Output(bytes);
                clients.list.retain_mut(|c| {
                    let ok = c.send(&out);
                    if !ok {
                        c.hang_up();
                    }
                    ok
                });
            }
        }
    }
    for mut c in clients.list.drain(..) {
        c.send(&ToClient::Exited("ranma exited".into()));
    }
    close_all(app);
    Ok(())
}

/// What the input of client `id` asked for (`detach`, `attach NAME`), done to
/// that client only, not to every terminal showing the server.
fn requests(app: &mut App, clients: &mut Clients, id: Option<u64>, server: Option<&str>) {
    if std::mem::take(&mut app.detach_requested) {
        match id.and_then(|id| clients.remove(id)) {
            Some(mut c) => {
                c.send(&ToClient::Detached("detached".into()));
            }
            None => {
                app.status = Some("nothing to detach from (ranma --standalone)".into());
            }
        }
    }
    if let Some(to) = app.switch_requested.take() {
        let c = id.and_then(|id| clients.get(id));
        let inside = c.and_then(|c| c.hello.inside.clone());
        match refuse_switch(&to, c, server, inside.as_deref()) {
            Some(why) => app.status = Some(why),
            None => {
                if let Some(mut c) = id.and_then(|id| clients.remove(id)) {
                    c.send(&ToClient::Switch(to));
                }
            }
        }
    }
}

/// The screen follows this client now: its size, colours and title.
fn drive(
    app: &mut App,
    term: &mut Term,
    buffer: Option<&Arc<Mutex<Vec<u8>>>>,
    hello: &proto::Hello,
) -> Result<()> {
    restart(term, buffer, hello.cols, hello.rows)?;
    app.host_colors = hello.colors.clone();
    app.client_inside = hello.inside.clone();
    app.set_outer(hello.outer);
    app.set_outer_colors(hello.outer_colors.clone());
    app.handle(AppEvent::Input(Event::Resize(hello.cols, hello.rows)));
    app.driven_by(hello.mobile, hello.remote);
    // The title is this terminal's now: say who we are again (and where, if it
    // came over SSH), even with nothing else changed.
    app.host_title.clear();
    app.announce();
    app.dirty = true;
    Ok(())
}

/// Draw everything again, at the size the screen already has: every client
/// is sent the whole screen.
fn repaint(
    app: &mut App,
    term: &mut Term,
    buffer: Option<&Arc<Mutex<Vec<u8>>>>,
    clients: &Clients,
) -> Result<()> {
    if let Some((cols, rows)) = clients.size() {
        restart(term, buffer, cols, rows)?;
    }
    app.host_title.clear();
    app.announce();
    app.dirty = true;
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

/// A client attached (the screen already set up, for it or for the one
/// driving): its early keys, and the first pane of a new server.
fn attached(app: &mut App, hello: &proto::Hello) -> Result<()> {
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
        Client::new(
            1,
            writer,
            proto::Hello {
                build: build.into(),
                cols: 80,
                rows: 24,
                colors: Default::default(),
                typed_early: Vec::new(),
                inside: None,
                remote: false,
                outer: None,
                outer_colors: None,
                steal: false,
                mobile: false,
                graphics: false,
            },
        )
    }

    fn sized(id: u64, cols: u16) -> Client {
        let mut c = client(crate::update::BUILD_SHA);
        c.id = id;
        c.hello.cols = cols;
        c
    }

    #[test]
    fn the_screen_follows_the_terminal_last_typed_in() {
        let mut cs = Clients::default();
        cs.list.push(sized(1, 200));
        assert_eq!(
            cs.new_driver().map(|h| h.cols),
            Some(200),
            "the first one drives"
        );
        assert_eq!(cs.new_driver(), None, "and is set up once");
        // A second terminal joins: a peek changes nothing.
        cs.list.push(sized(2, 90));
        assert_eq!(cs.new_driver(), None);
        assert_eq!(cs.size(), Some((200, 24)));
        // It types: the screen is its size now.
        cs.touch(cs.position(2).unwrap());
        assert_eq!(cs.new_driver().map(|h| h.cols), Some(90));
        // It leaves: back to the one before.
        cs.remove(2);
        assert_eq!(cs.new_driver().map(|h| h.cols), Some(200));
        cs.remove(1);
        assert_eq!(cs.new_driver(), None, "nobody left to drive");
        assert_eq!(cs.size(), None);
    }

    #[test]
    fn only_someone_at_the_terminal_takes_the_screen() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let key = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        assert!(is_presence(&key));
        assert!(is_presence(&Event::Paste("x".into())));
        // Answered by the terminal itself on every attach, or by the window
        // manager around it: not a person.
        assert!(!is_presence(&Event::FocusGained));
        assert!(!is_presence(&Event::Resize(80, 24)));
        let moved = Event::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Moved,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        assert!(!is_presence(&moved), "a pointer passing over");
    }

    #[test]
    fn the_click_that_takes_the_screen_does_nothing_else() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent};
        let mouse = |kind| {
            Event::Mouse(MouseEvent {
                kind,
                column: 10,
                row: 5,
                modifiers: KeyModifiers::NONE,
            })
        };
        let down = mouse(MouseEventKind::Down(MouseButton::Left));
        let drag = mouse(MouseEventKind::Drag(MouseButton::Left));
        let up = mouse(MouseEventKind::Up(MouseButton::Left));
        let mut cs = Clients::default();
        cs.list.push(sized(1, 200));
        cs.list.push(sized(2, 52));
        cs.new_driver();
        // The driver's clicks go through.
        assert!(cs.arrive(0, &down));
        assert!(cs.arrive(0, &up));
        // The phone taps: it drives now, and the tap, read off the desk's
        // screen, lands nowhere, drag and release included.
        assert!(!cs.arrive(1, &down));
        assert_eq!(cs.new_driver().map(|h| h.cols), Some(52));
        assert!(!cs.arrive(0, &drag));
        assert!(!cs.arrive(0, &up));
        // The next tap is an ordinary one.
        assert!(cs.arrive(0, &down));
        assert!(cs.arrive(0, &up));
        // A key that takes the screen back is typed: it has no position.
        let key = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        assert!(cs.arrive(1, &key));
        assert_eq!(cs.new_driver().map(|h| h.cols), Some(200));
        // A wheel from the one not driving takes it, and scrolls nothing.
        assert!(!cs.arrive(1, &mouse(MouseEventKind::ScrollUp)));
        assert!(
            cs.arrive(0, &mouse(MouseEventKind::ScrollUp)),
            "no release to wait for"
        );
    }

    #[test]
    fn a_pointer_passing_over_does_not_take_the_screen() {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent};
        let mouse = |kind| {
            Event::Mouse(MouseEvent {
                kind,
                column: 140,
                row: 38,
                modifiers: KeyModifiers::NONE,
            })
        };
        let mut cs = Clients::default();
        cs.list.push(sized(1, 120));
        cs.list.push(sized(2, 150));
        cs.new_driver();
        // The desk's pointer moves or drags with nobody pressing there: the
        // screen stays, and positions read off the desk's size reach nothing.
        for kind in [
            MouseEventKind::Moved,
            MouseEventKind::Drag(MouseButton::Left),
        ] {
            assert!(!cs.arrive(1, &mouse(kind)), "{kind:?} went through");
            assert_eq!(cs.new_driver(), None, "{kind:?} took the screen");
        }
        // A release takes nothing either, but goes through: it may end a drag
        // begun there while that terminal still drove.
        assert!(cs.arrive(1, &mouse(MouseEventKind::Up(MouseButton::Left))));
        assert_eq!(cs.new_driver(), None);
        // The driver's own motion still hovers.
        assert!(cs.arrive(0, &mouse(MouseEventKind::Moved)));
        // A press is someone there.
        assert!(!cs.arrive(1, &mouse(MouseEventKind::Down(MouseButton::Left))));
        assert_eq!(cs.new_driver().map(|h| h.cols), Some(150));
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

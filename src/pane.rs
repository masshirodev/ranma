//! A pane: one PTY, one child process, one terminal emulator.
//!
//! alacritty_terminal does the hard parts. Its event loop owns the PTY on a thread
//! of its own, reads in bounded chunks, parses into the `Term` under a fair lock,
//! and reports back through [`Proxy`]. The UI thread only ever locks the `Term` to
//! read it for a frame, or to resize it.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

use alacritty_terminal::event::{Event as TermEvent, EventListener, OnResize, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{self, Term, TermMode};
use alacritty_terminal::tty;
use anyhow::{Context, Result};

use crate::input::PaneModes;
use crate::layout::PaneId;

/// What the UI thread is woken with.
#[derive(Debug)]
pub enum AppEvent {
    Input(crossterm::event::Event),
    InputClosed,
    Pane(PaneId, TermEvent),
    /// An exec bar module finished. `generation` ties it to the config that
    /// started it, so a result from before a reload is dropped.
    Module {
        name: String,
        generation: u64,
        text: Result<String, String>,
    },
    /// A process `ranma.spawn` started printed lines or ended (`jobs`).
    Job {
        id: u64,
        event: crate::jobs::JobEvent,
    },
    /// Something in the config directory changed on disk.
    ConfigChanged,
    /// A toast from `ranma notify` (see `ipc`).
    Toast {
        text: String,
        level: crate::toast::Level,
        timeout: Option<std::time::Duration>,
    },
    /// An action from `ranma action`.
    Action(crate::action::Action),
    /// A request answered with data (`ranma open`, `panes`, `send`, `capture`,
    /// `wait`), and where the answer goes.
    Query(
        crate::ipc::Query,
        Sender<std::result::Result<String, String>>,
    ),
    /// The source has commits this binary lacks (see `update`).
    UpdateAvailable(crate::update::Behind),
    /// A client attached (see `proto`): where to send its output, and its hello.
    Attach {
        id: u64,
        writer: std::os::unix::net::UnixStream,
        hello: Box<crate::proto::Hello>,
    },
    /// Input from the client with this id.
    ClientInput(u64, crossterm::event::Event),
    /// The client with this id disconnected.
    ClientGone(u64),
    /// Someone asks how this server is (`ranma ls`, a client choosing a server).
    Status(Sender<crate::proto::Status>),
    /// Every server's status, gathered off the UI thread for the server switcher.
    Servers(Vec<crate::proto::Status>),
    /// A paste's upload finished (see `paste`): the path to type, or why not.
    Pasted {
        id: u64,
        result: std::result::Result<String, String>,
    },
    /// A shell mark or a notification a pane's program sent (see `osc`).
    Mark(PaneId, crate::osc::Mark),
    /// `ranma upgrade`: take the new build in place (see `app::upgrade`).
    /// The answer goes to `reply`; `written` says it has reached the asker,
    /// which must happen before the exec closes that connection.
    Upgrade {
        reply: Sender<std::result::Result<String, String>>,
        written: std::sync::mpsc::Receiver<()>,
    },
}

/// Forwards a pane's terminal events to the UI thread.
#[derive(Clone)]
pub struct Proxy {
    id: PaneId,
    tx: Sender<AppEvent>,
    /// Set when a wakeup is queued and not yet drawn. A pane printing as fast as it
    /// can produces a wakeup per read; without this each one would be a message.
    wakeup_pending: Arc<AtomicBool>,
    /// Cleared when the pane's process is replaced (`respawn`): the old event
    /// loop's last events, its exit above all, must not reach the new one.
    live: Arc<AtomicBool>,
}

impl Proxy {
    /// One whose events go nowhere, for a terminal made in a test.
    #[cfg(test)]
    pub fn for_test(id: PaneId) -> Proxy {
        Proxy {
            id,
            tx: std::sync::mpsc::channel().0,
            wakeup_pending: Arc::default(),
            live: Arc::new(AtomicBool::new(true)),
        }
    }

    /// A mark from the PTY reader, unless the pane's process was replaced.
    fn mark(&self, m: crate::osc::Mark) {
        if self.live.load(Ordering::Acquire) {
            let _ = self.tx.send(AppEvent::Mark(self.id, m));
        }
    }
}

/// The PTY as alacritty_terminal's event loop drives it, with a look at what
/// it reads on the way: the OSC sequences the emulator drops (see `osc`).
struct ScanPty<P> {
    inner: P,
    reader: ScanReader,
}

/// Where `pipe_pane` sends what a pane's program writes, while it does.
pub type Tee = Arc<std::sync::Mutex<Option<std::sync::mpsc::SyncSender<Vec<u8>>>>>;

struct ScanReader {
    tee: Tee,
    /// The PTY's own descriptor, duplicated: the event loop polls the
    /// original and reads through this one, which is the same open file.
    file: std::fs::File,
    scanner: crate::osc::Scanner,
    proxy: Proxy,
    /// Turns direct image placements into placeholder cells (`graphics`).
    placer: crate::graphics::Placer,
    /// Placeholder text to hand the emulator before anything else, and the
    /// output read after the placement, not yet looked at.
    out: Vec<u8>,
    raw: Vec<u8>,
}

/// While set, no pane reads its PTY: a server handing over to a new build
/// holds what programs print in the kernel's buffer, for the new process to
/// read (see `upgrade`).
pub static HOLD_OUTPUT: AtomicBool = AtomicBool::new(false);

impl ScanReader {
    /// `pipe_pane`'s copy. Never waits: a sink behind by a full queue loses
    /// this read, and one that is gone stops being sent to.
    fn tee(&self, bytes: &[u8]) {
        if !bytes.is_empty()
            && let Ok(mut tee) = self.tee.lock()
            && let Some(tx) = tee.as_ref()
            && let Err(std::sync::mpsc::TrySendError::Disconnected(_)) = tx.try_send(bytes.to_vec())
        {
            *tee = None;
        }
    }
}

impl std::io::Read for ScanReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if HOLD_OUTPUT.load(Ordering::Acquire) {
            return Err(std::io::ErrorKind::WouldBlock.into());
        }
        // An image's placeholders go first, then what the program wrote
        // after placing it (see below).
        if !self.out.is_empty() {
            let k = self.out.len().min(buf.len());
            buf[..k].copy_from_slice(&self.out[..k]);
            self.out.drain(..k);
            return Ok(k);
        }
        let mut n = if self.raw.is_empty() {
            let n = self.file.read(buf)?;
            self.tee(&buf[..n]);
            n
        } else {
            let k = self.raw.len().min(buf.len());
            buf[..k].copy_from_slice(&self.raw[..k]);
            self.raw.drain(..k);
            k
        };
        let now = std::time::Instant::now();
        let mut at = 0;
        while at < n {
            let (marks, used) = self.scanner.feed_until_graphics(&buf[at..n], now);
            at += used;
            let mut placed = None;
            for m in marks {
                match m {
                    crate::osc::Mark::Graphics(body) => {
                        let (body, text) = self.placer.place(&body);
                        self.proxy.mark(crate::osc::Mark::Graphics(body));
                        placed = text;
                    }
                    m => self.proxy.mark(m),
                }
            }
            // A direct placement: the placeholder cells go into the output
            // right after it, so they land at the cursor the program placed
            // the image at, before whatever it writes next. In the buffer
            // when they fit (the event loop's is large); otherwise the rest
            // waits for the next read, which the loop makes at once unless
            // it has just read its fill.
            if let Some(text) = placed {
                if n + text.len() <= buf.len() {
                    buf.copy_within(at..n, at + text.len());
                    buf[at..at + text.len()].copy_from_slice(&text);
                    at += text.len();
                    n += text.len();
                } else {
                    self.out = text;
                    let mut rest = buf[at..n].to_vec();
                    rest.append(&mut self.raw);
                    self.raw = rest;
                    return Ok(at);
                }
            }
        }
        Ok(n)
    }
}

impl<P: tty::EventedReadWrite<Writer = std::fs::File>> tty::EventedReadWrite for ScanPty<P> {
    type Reader = ScanReader;
    type Writer = std::fs::File;

    unsafe fn register(
        &mut self,
        poll: &Arc<polling::Poller>,
        interest: polling::Event,
        mode: polling::PollMode,
    ) -> std::io::Result<()> {
        // SAFETY: the caller's promise, passed on; the PTY lives in `self`.
        unsafe { self.inner.register(poll, interest, mode) }
    }
    fn reregister(
        &mut self,
        poll: &Arc<polling::Poller>,
        interest: polling::Event,
        mode: polling::PollMode,
    ) -> std::io::Result<()> {
        self.inner.reregister(poll, interest, mode)
    }
    fn deregister(&mut self, poll: &Arc<polling::Poller>) -> std::io::Result<()> {
        self.inner.deregister(poll)
    }
    fn reader(&mut self) -> &mut ScanReader {
        &mut self.reader
    }
    fn writer(&mut self) -> &mut std::fs::File {
        self.inner.writer()
    }
}

impl<P: OnResize> OnResize for ScanPty<P> {
    fn on_resize(&mut self, size: WindowSize) {
        self.inner.on_resize(size)
    }
}

impl<P: tty::EventedPty<Writer = std::fs::File>> tty::EventedPty for ScanPty<P> {
    fn next_child_event(&mut self) -> Option<tty::ChildEvent> {
        self.inner.next_child_event()
    }
}

impl EventListener for Proxy {
    fn send_event(&self, event: TermEvent) {
        if !self.live.load(Ordering::Acquire) {
            return;
        }
        if matches!(event, TermEvent::Wakeup) && self.wakeup_pending.swap(true, Ordering::AcqRel) {
            return;
        }
        // The UI thread may be gone during shutdown; nothing to do about it then.
        let _ = self.tx.send(AppEvent::Pane(self.id, event));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows as usize
    }
    fn screen_lines(&self) -> usize {
        self.rows as usize
    }
    fn columns(&self) -> usize {
        self.cols as usize
    }
}

impl Size {
    fn window(self) -> WindowSize {
        WindowSize {
            num_lines: self.rows,
            num_cols: self.cols,
            // Pixel sizes are unknown inside a terminal; programs that ask get zero,
            // which is what tmux reports too.
            cell_width: 0,
            cell_height: 0,
        }
    }
}

pub struct Pane {
    pub id: PaneId,
    pub term: Arc<FairMutex<Term<Proxy>>>,
    sender: EventLoopSender,
    wakeup_pending: Arc<AtomicBool>,
    live: Arc<AtomicBool>,
    pub size: Size,
    /// What the program last set (OSC 0/2). Window rules match this.
    pub title: String,
    /// A name given with rename_pane; shown instead of the title while set.
    pub name: Option<String>,
    /// The pane's own child (the shell, or the `exec` command).
    pub pid: u32,
    /// The command line it was started with (`None`: the shell) and where:
    /// what `respawn_pane` runs again once the program is gone.
    pub command: Option<String>,
    pub start_cwd: Option<std::path::PathBuf>,
    /// A descriptor of the PTY master, open for as long as the pane: what a
    /// server keeps across its exec to take a new build.
    pub master_fd: std::os::fd::RawFd,
    /// `pipe_pane`'s sink, shared with the PTY reader.
    tee: Tee,
    /// The PTY's I/O thread; it drops the PTY (hanging up the child) as it ends.
    io_thread: Option<std::thread::JoinHandle<()>>,
}

/// What every pane is made of, however its PTY came to be.
struct Parts {
    id: PaneId,
    term: Arc<FairMutex<Term<Proxy>>>,
    proxy: Proxy,
    wakeup_pending: Arc<AtomicBool>,
    live: Arc<AtomicBool>,
    size: Size,
    pid: u32,
}

pub struct SpawnOptions<'a> {
    pub shell: Option<&'a str>,
    pub command: Option<&'a str>,
    pub scrollback_lines: usize,
    /// Where the child starts; ranma's own directory when `None`.
    pub cwd: Option<std::path::PathBuf>,
    /// Variables for the child on top of ranma's own environment (the tmux
    /// shim passes its caller's). ranma's own (`TERM`, `RANMA_*`) still win,
    /// and a `TMUX` given here gets this pane as its `TMUX_PANE`.
    pub env: &'a [(String, String)],
}

impl Pane {
    pub fn spawn(
        id: PaneId,
        size: Size,
        opts: &SpawnOptions,
        tx: Sender<AppEvent>,
    ) -> Result<Pane> {
        // A zero-sized PTY confuses programs (and some refuse to start).
        let size = Size {
            cols: size.cols.max(2),
            rows: size.rows.max(1),
        };
        let wakeup_pending = Arc::new(AtomicBool::new(false));
        let live = Arc::new(AtomicBool::new(true));
        let proxy = Proxy {
            id,
            tx,
            wakeup_pending: wakeup_pending.clone(),
            live: live.clone(),
        };

        let config = term::Config {
            scrolling_history: opts.scrollback_lines,
            kitty_keyboard: true,
            ..Default::default()
        };
        let term = Arc::new(FairMutex::new(Term::new(config, &size, proxy.clone())));

        let shell = opts
            .shell
            .map(str::to_string)
            .or_else(|| std::env::var("SHELL").ok().filter(|s| !s.is_empty()))
            .unwrap_or_else(|| "/bin/sh".into());
        let program = match opts.command {
            // `exec` runs through the shell so the command line means what it would
            // at a prompt: quoting, globs, pipes.
            Some(cmd) => tty::Shell::new(shell, vec!["-c".into(), cmd.into()]),
            None => tty::Shell::new(shell, vec![]),
        };

        let mut env: HashMap<String, String> = opts.env.iter().cloned().collect();
        if env.contains_key("TMUX") {
            env.insert("TMUX_PANE".into(), format!("%{id}"));
        }
        // ranma emulates what alacritty_terminal emulates, so its terminfo is the
        // accurate one when installed; xterm-256color is the safe fallback.
        let term_name = if terminfo_exists("alacritty") {
            "alacritty"
        } else {
            "xterm-256color"
        };
        env.insert("TERM".into(), term_name.into());
        env.insert("COLORTERM".into(), "truecolor".into());
        env.insert("RANMA".into(), std::process::id().to_string());
        env.insert("RANMA_PANE".into(), id.to_string());
        // How `ranma notify` in this pane finds this ranma.
        if let Some(sock) = crate::ipc::socket_path() {
            env.insert(crate::ipc::ENV.into(), sock.display().to_string());
        }

        let start_cwd = opts.cwd.clone().or_else(|| std::env::current_dir().ok());
        let pty_opts = tty::Options {
            shell: Some(program),
            working_directory: start_cwd.clone(),
            drain_on_exit: false,
            env,
        };
        let pty = tty::new(&pty_opts, size.window(), id).context("opening a PTY")?;
        let pid = pty.child().id();
        let master = pty.file().try_clone().context("duplicating the PTY")?;
        let mut pane = Pane::start(
            Parts {
                id,
                term,
                proxy,
                wakeup_pending,
                live,
                size,
                pid,
            },
            master,
            pty,
        )?;
        pane.command = opts.command.map(str::to_string);
        pane.start_cwd = start_cwd;
        Ok(pane)
    }

    /// A pane around a child that is already running, after a server took a
    /// new build (DESIGN.md, "Upgrading a server in place"): `fd` is the PTY
    /// master kept across the exec, `snapshot` what the pane showed.
    ///
    /// # Safety
    /// `fd` must be the open PTY master of `pid`, owned by nothing else.
    pub unsafe fn adopt(
        id: PaneId,
        size: Size,
        fd: std::os::fd::RawFd,
        pid: u32,
        snapshot: &crate::snapshot::Snapshot,
        scrollback_lines: usize,
        tx: Sender<AppEvent>,
    ) -> Result<Pane> {
        let wakeup_pending = Arc::new(AtomicBool::new(false));
        let live = Arc::new(AtomicBool::new(true));
        let proxy = Proxy {
            id,
            tx,
            wakeup_pending: wakeup_pending.clone(),
            live: live.clone(),
        };
        let config = term::Config {
            scrolling_history: scrollback_lines,
            kitty_keyboard: true,
            ..Default::default()
        };
        let mut t = Term::new(config, &size, proxy.clone());
        crate::snapshot::restore(&mut t, snapshot);
        let term = Arc::new(FairMutex::new(t));
        // SAFETY: the caller hands the master over.
        let mut pty =
            unsafe { crate::pty::AdoptedPty::adopt(fd, pid) }.context("adopting a PTY")?;
        let master = {
            use tty::EventedReadWrite;
            pty.reader().try_clone().context("duplicating the PTY")?
        };
        Pane::start(
            Parts {
                id,
                term,
                proxy,
                wakeup_pending,
                live,
                size,
                pid,
            },
            master,
            pty,
        )
    }

    /// The rest of a pane, whichever way its PTY came: the scanner in front of
    /// it, and alacritty's event loop on a thread of its own.
    fn start<P>(parts: Parts, master: std::fs::File, pty: P) -> Result<Pane>
    where
        P: tty::EventedPty<Writer = std::fs::File> + OnResize + Send + 'static,
    {
        use std::os::fd::AsRawFd;
        let Parts {
            id,
            term,
            proxy,
            wakeup_pending,
            live,
            size,
            pid,
        } = parts;
        let master_fd = master.as_raw_fd();
        let tee: Tee = Arc::default();
        let pty = ScanPty {
            reader: ScanReader {
                tee: tee.clone(),
                placer: Default::default(),
                out: Vec::new(),
                raw: Vec::new(),
                file: master,
                scanner: Default::default(),
                proxy: proxy.clone(),
            },
            inner: pty,
        };
        // Drain on exit: one more read, without blocking, of what the program
        // printed as it ended, so a pane that stays (`remain_on_exit`) shows
        // its last lines.
        let event_loop = EventLoop::new(term.clone(), proxy, pty, true, false)
            .context("starting the PTY event loop")?;
        let sender = event_loop.channel();
        // The thread ends on Msg::Shutdown or when the child exits, and drops
        // the PTY as it goes, which hangs up the child and waits for it. Its
        // handle is kept so quitting ranma can wait for that (see `close`).
        let handle = event_loop.spawn();
        let io_thread = std::thread::Builder::new()
            .name("pty-join".into())
            .spawn(move || {
                let _ = handle.join();
            })
            .ok();

        Ok(Pane {
            id,
            term,
            sender,
            wakeup_pending,
            live,
            size,
            title: String::new(),
            name: None,
            pid,
            command: None,
            start_cwd: None,
            tee,
            master_fd,
            io_thread,
        })
    }

    /// What to call the pane on screen: its name if it has one, else its title
    /// (without the marker a ranma inside it puts there).
    pub fn label(&self) -> &str {
        self.name
            .as_deref()
            .unwrap_or_else(|| strip_nested_marker(&self.title))
    }

    /// The title as its program meant it, marker removed. Window rules match this.
    pub fn clean_title(&self) -> &str {
        strip_nested_marker(&self.title)
    }

    /// A ranma is running in this pane (it announces itself in its title).
    pub fn hosts_ranma(&self) -> bool {
        self.title.starts_with(NESTED_MARKER)
    }

    /// The host a ranma in this pane says it runs on (it or one inside it).
    pub fn inner_host(&self) -> Option<&str> {
        marker_host(&self.title)
    }

    /// The host the `ssh` in this pane's foreground connects to, if one runs
    /// there: what the title names when no ranma on the other side does.
    pub fn ssh_host(&self) -> Option<String> {
        foreground_ssh_host(self.pid)
    }

    /// The whole command line of that `ssh`, for a paste to upload over.
    pub fn ssh_argv(&self) -> Option<Vec<String>> {
        foreground_ssh_argv(self.pid)
    }

    /// The ranma in this pane is engaged: in WM mode, or with an engaged ranma
    /// of its own. The outer leader goes on down to it instead of stopping here.
    pub fn inner_engaged(&self) -> bool {
        self.title.starts_with(NESTED_MARKER_ENGAGED)
    }

    /// The program in the pane's foreground (see [`foreground_program`]).
    pub fn program(&self) -> Option<String> {
        foreground_program(self.pid)
    }

    /// What a workspace is called after this pane when it has no name of its
    /// own (see [`workspace_label`]).
    pub fn workspace_label(&self) -> Option<String> {
        workspace_label(self.program(), || self.ssh_host(), self.inner_host())
    }

    /// The command line in the pane's foreground, as words, unless that is
    /// a shell named `shell` (the pane's own at its prompt, or one started in
    /// it): what a saved layout types back in.
    pub fn foreground_command(&self, shell: &str) -> Option<Vec<String>> {
        let fg = foreground_pid(self.pid);
        let comm = std::fs::read_to_string(format!("/proc/{fg}/comm")).ok()?;
        let shell = std::path::Path::new(shell)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(shell);
        if comm.trim() == shell {
            return None;
        }
        let raw = std::fs::read(format!("/proc/{fg}/cmdline")).ok()?;
        let argv: Vec<String> = raw
            .split(|b| *b == 0)
            .filter(|a| !a.is_empty())
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect();
        (!argv.is_empty()).then_some(argv)
    }

    /// The directory the pane's child is in now: where `cd` last took the shell.
    /// Read from /proc, so it follows the shell without any shell integration.
    pub fn cwd(&self) -> Option<std::path::PathBuf> {
        std::fs::read_link(format!("/proc/{}/cwd", self.pid))
            .ok()
            .filter(|p| p.is_dir())
    }

    pub fn write(&self, bytes: impl Into<Vec<u8>>) {
        let _ = self.sender.send(Msg::Input(bytes.into().into()));
    }

    pub fn resize(&mut self, size: Size) {
        let size = Size {
            cols: size.cols.max(2),
            rows: size.rows.max(1),
        };
        if size == self.size {
            return;
        }
        self.size = size;
        self.term.lock().resize(size);
        let _ = self.sender.send(Msg::Resize(size.window()));
    }

    pub fn modes(&self) -> PaneModes {
        let mode = *self.term.lock().mode();
        PaneModes {
            app_cursor: mode.contains(TermMode::APP_CURSOR),
            bracketed_paste: mode.contains(TermMode::BRACKETED_PASTE),
            focus_events: mode.contains(TermMode::FOCUS_IN_OUT),
            mouse_click: mode.contains(TermMode::MOUSE_REPORT_CLICK),
            mouse_drag: mode.contains(TermMode::MOUSE_DRAG),
            mouse_motion: mode.contains(TermMode::MOUSE_MOTION),
            mouse_sgr: mode.contains(TermMode::SGR_MOUSE),
            alt_screen: mode.contains(TermMode::ALT_SCREEN),
            alternate_scroll: mode.contains(TermMode::ALTERNATE_SCROLL),
            kitty: kitty_flags(mode),
        }
    }

    /// Scroll the view through scrollback; positive is up (older output).
    pub fn scroll(&self, lines: i32) {
        self.term
            .lock()
            .scroll_display(alacritty_terminal::grid::Scroll::Delta(lines));
    }

    /// Back to the live screen, if scrolled back. Typing does this, as in any
    /// terminal: input goes where the output is.
    pub fn scroll_to_bottom(&self) {
        let mut term = self.term.lock();
        if term.grid().display_offset() != 0 {
            term.scroll_display(alacritty_terminal::grid::Scroll::Bottom);
        }
    }

    /// Called when a frame including this pane is about to be drawn: the next
    /// output wakes the UI again.
    /// Send what the program writes to `sink` from now on, or stop (`None`).
    pub fn pipe(&self, sink: Option<std::sync::mpsc::SyncSender<Vec<u8>>>) {
        if let Ok(mut t) = self.tee.lock() {
            *t = sink;
        }
    }

    /// Where `pipe_pane` sends, for a pane taking this one's place.
    pub fn pipe_sender(&self) -> Option<std::sync::mpsc::SyncSender<Vec<u8>>> {
        self.tee.lock().ok().and_then(|t| t.clone())
    }

    /// Whether `pipe_pane` is still sending somewhere: false once the sink
    /// went away by itself (a command that ended).
    pub fn piping(&self) -> bool {
        self.tee.lock().is_ok_and(|t| t.is_some())
    }

    pub fn drawn(&self) {
        self.wakeup_pending.store(false, Ordering::Release);
    }

    /// Stop this pane's events reaching the window manager: its process is
    /// being replaced, and whatever the old one still says is about the past.
    pub fn retire(&self) {
        self.live.store(false, Ordering::Release);
    }

    pub fn shutdown(&self) {
        let _ = self.sender.send(Msg::Shutdown);
    }

    /// Shut the pane down and hand back a handle that finishes once its child
    /// has been hung up and has exited. Quitting ranma waits on these: exiting
    /// first would kill the I/O threads before they hang up the shells, and a
    /// shell never hung up leaves its background jobs running.
    pub fn close(mut self) -> Option<std::thread::JoinHandle<()>> {
        self.shutdown();
        self.io_thread.take()
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// How a ranma marks the title of the terminal it runs in, so a ranma around it
/// can tell: titles are one of the few things that pass through a terminal
/// emulator's parser, and over SSH.
pub const NESTED_MARKER: &str = "⧉ ranma";
/// The same, for a ranma that is engaged (see `Pane::inner_engaged`).
pub const NESTED_MARKER_ENGAGED: &str = "⧉ ranma+";

/// A marked title in parts: the host it names, if any, and the title after the
/// mark. `⧉ ranma@vps · nvim` is `(Some("vps"), "nvim")`; an unmarked title is
/// `None`.
fn split_marker(title: &str) -> Option<(Option<&str>, &str)> {
    let rest = title
        .strip_prefix(NESTED_MARKER_ENGAGED)
        .or_else(|| title.strip_prefix(NESTED_MARKER))?;
    let (host, rest) = match rest.strip_prefix('@') {
        Some(r) => match r.split_once(' ') {
            Some((h, r)) => (Some(h), r),
            None => (Some(r), ""),
        },
        None => (None, rest),
    };
    let label = rest.strip_prefix(" · ").unwrap_or(rest.trim_start());
    let label = label.strip_prefix("· ").unwrap_or(label);
    Some((host.filter(|h| !h.is_empty()), label))
}

/// A reply the terminal emulator makes to a program's query, as ranma sends
/// it. alacritty_terminal answers DA1 with `ESC [ ? 6 c` (a VT102), which
/// lists no extensions; ranma passes OSC 52 copies on to the host, so it says
/// so, as a VT220 with ANSI colour (22) and the clipboard (52). nvim turns
/// its OSC 52 clipboard on only when DA1 lists 52, and over SSH that is the
/// only clipboard it has.
pub fn pty_reply(reply: String) -> String {
    if reply == "\x1b[?6c" {
        "\x1b[?62;22;52c".to_string()
    } else {
        reply
    }
}

/// A title without the nested-ranma marker: `⧉ ranma · nvim` is `nvim`, and so
/// is `⧉ ranma@vps · nvim`.
pub fn strip_nested_marker(title: &str) -> &str {
    split_marker(title).map_or(title, |(_, label)| label)
}

/// The host a marked title names: the ranma in a pane runs there (or a ranma
/// inside it does), which is the host this ranma's own title should carry.
pub fn marker_host(title: &str) -> Option<&str> {
    // One host, never a chain: a mark only ever carries the innermost. Should a
    // title arrive as `@a@b` anyway, the last one is the innermost, and taking
    // it alone keeps every ranma further out from growing the chain.
    split_marker(title)
        .and_then(|(host, _)| host)
        .and_then(|h| h.rsplit('@').next())
        .filter(|h| !h.is_empty())
}

/// The title a ranma gives its terminal: its mark, carrying one host (the one a
/// ranma in the focused pane names, else `own`), then the focused title.
pub fn own_title(engaged: bool, inner: Option<&str>, own: Option<&str>, label: &str) -> String {
    let m = marker(engaged, inner.or(own));
    if label.is_empty() {
        m
    } else {
        format!("{m} · {label}")
    }
}

/// The mark for this ranma's own title, with the host when there is one.
pub fn marker(engaged: bool, host: Option<&str>) -> String {
    let m = if engaged {
        NESTED_MARKER_ENGAGED
    } else {
        NESTED_MARKER
    };
    match host {
        Some(h) => format!("{m}@{h}"),
        None => m.to_string(),
    }
}

/// This machine's name, short (`vps`, not `vps.example.com`), asked once.
pub fn hostname() -> &'static str {
    static HOST: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HOST.get_or_init(|| {
        let mut buf = [0u8; 256];
        // SAFETY: the buffer and its length are ours; gethostname NUL-terminates
        // within it, or we take up to the first NUL anyway.
        let ok = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } == 0;
        let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
        let name = if ok {
            String::from_utf8_lossy(&buf[..end]).into_owned()
        } else {
            String::new()
        };
        let short = name.split('.').next().unwrap_or("");
        // Titles and the mark are split on spaces: a host never has one, but a
        // mark must not be broken by a strange one either.
        let short: String = short
            .chars()
            .filter(|c| !c.is_whitespace() && !c.is_control())
            .collect();
        if short.is_empty() {
            "localhost".into()
        } else {
            short
        }
    })
}

/// Whether this process was reached over SSH (as sshd tells its sessions).
pub fn over_ssh() -> bool {
    ["SSH_CONNECTION", "SSH_TTY", "SSH_CLIENT"]
        .iter()
        .any(|v| std::env::var_os(v).is_some_and(|s| !s.is_empty()))
}

/// The leader of the foreground process group of the terminal whose session
/// leader is `pid` (field 8 of its /proc `stat`), or `pid` itself.
fn foreground_pid(pid: u32) -> u32 {
    let tpgid = std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|stat| {
            // The command name is in parentheses and may hold spaces or
            // parentheses of its own: the fields start after the last `)`.
            let after = &stat[stat.rfind(')')? + 1..];
            after.split_whitespace().nth(5)?.parse::<i32>().ok()
        });
    match tpgid {
        Some(t) if t > 0 && std::path::Path::new(&format!("/proc/{t}")).exists() => t as u32,
        _ => pid,
    }
}

/// The program in the foreground of a terminal whose session leader is `pid`:
/// `nvim` while the shell runs it, the shell itself at its prompt.
pub fn foreground_program(pid: u32) -> Option<String> {
    let comm = std::fs::read_to_string(format!("/proc/{}/comm", foreground_pid(pid))).ok()?;
    Some(comm.trim().to_string()).filter(|c| !c.is_empty())
}

/// A workspace's automatic name: the program in its focused pane, except that
/// a connection is named for where it goes. `ssh vps` is `vps`, which is what
/// was typed and so what is recognised; with no plain `ssh` to read (mosh, a
/// wrapper), the host a ranma on the far side reports stands in. A ranma
/// running here keeps its own name: its workspaces are its own, not a host's.
pub fn workspace_label(
    program: Option<String>,
    ssh_host: impl FnOnce() -> Option<String>,
    inner_host: Option<&str>,
) -> Option<String> {
    match program.as_deref() {
        Some("ssh") => ssh_host()
            .or_else(|| inner_host.map(str::to_string))
            .or(program),
        Some("ranma") | None => program,
        Some(_) => inner_host.map(str::to_string).or(program),
    }
}

/// Where the `ssh` in the foreground of that terminal went, if one is there:
/// `vps` for `ssh -p 22 me@vps htop`.
pub fn foreground_ssh_host(pid: u32) -> Option<String> {
    let args = foreground_ssh_argv(pid)?;
    ssh_destination(args.iter().skip(1).map(String::as_str))
}

/// The argv of the `ssh` in the foreground of that terminal, if one is there.
/// The kitty keyboard flags a terminal mode holds, as the protocol numbers them.
pub fn kitty_flags(mode: TermMode) -> u8 {
    [
        TermMode::DISAMBIGUATE_ESC_CODES,
        TermMode::REPORT_EVENT_TYPES,
        TermMode::REPORT_ALTERNATE_KEYS,
        TermMode::REPORT_ALL_KEYS_AS_ESC,
        TermMode::REPORT_ASSOCIATED_TEXT,
    ]
    .iter()
    .enumerate()
    .filter(|(_, m)| mode.contains(**m))
    .fold(0, |acc, (i, _)| acc | 1 << i)
}

pub fn foreground_ssh_argv(pid: u32) -> Option<Vec<String>> {
    let fg = foreground_pid(pid);
    if std::fs::read_to_string(format!("/proc/{fg}/comm"))
        .ok()?
        .trim()
        != "ssh"
    {
        return None;
    }
    let raw = std::fs::read(format!("/proc/{fg}/cmdline")).ok()?;
    Some(
        raw.split(|b| *b == 0)
            .filter(|a| !a.is_empty())
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect(),
    )
}

/// The host an ssh command line connects to: its first argument that is not
/// an option (nor an option's value), without `ssh://`, the user or the port.
pub fn ssh_destination<'a>(args: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let args: Vec<&str> = args.into_iter().collect();
    host_of(args[ssh_destination_at(&args)?])
}

/// Where the destination is among ssh's arguments (its own name left out).
pub fn ssh_destination_at(args: &[&str]) -> Option<usize> {
    // ssh's options that take a value (ssh(1), SYNOPSIS).
    const WITH_VALUE: &str = "BbcDEeFIiJLlmOoPpQRSWw";
    let mut i = 0;
    while i < args.len() {
        let a = args[i];
        if a == "--" {
            return (i + 1 < args.len()).then_some(i + 1);
        }
        let Some(flags) = a.strip_prefix('-').filter(|f| !f.is_empty()) else {
            return Some(i);
        };
        for (j, c) in flags.char_indices() {
            if WITH_VALUE.contains(c) {
                // `-p22` carries it; `-p 22` takes the next argument.
                if j + c.len_utf8() == flags.len() {
                    i += 1;
                }
                break;
            }
        }
        i += 1;
    }
    None
}

fn host_of(dest: &str) -> Option<String> {
    let d = dest.strip_prefix("ssh://").unwrap_or(dest);
    let d = d.rsplit_once('@').map_or(d, |(_, h)| h);
    // A port (ssh://host:22) or an IPv6 address in brackets.
    let d = match d.strip_prefix('[') {
        Some(r) => r.split(']').next().unwrap_or(r),
        None => d.split(':').next().unwrap_or(d),
    };
    let d: String = d
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control() && *c != '@')
        .collect();
    Some(d).filter(|d| !d.is_empty())
}

fn terminfo_exists(name: &str) -> bool {
    let first = &name[..1];
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".terminfo"));
    }
    if let Ok(v) = std::env::var("TERMINFO") {
        dirs.push(v.into());
    }
    if let Ok(v) = std::env::var("TERMINFO_DIRS") {
        dirs.extend(v.split(':').filter(|s| !s.is_empty()).map(Into::into));
    }
    dirs.extend(
        [
            "/etc/terminfo",
            "/lib/terminfo",
            "/usr/share/terminfo",
            "/usr/lib/terminfo",
        ]
        .map(Into::into),
    );
    let hex = format!("{:x}", first.as_bytes()[0]);
    dirs.iter()
        .any(|d| d.join(first).join(name).exists() || d.join(&hex).join(name).exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_attributes_say_the_clipboard_is_passed_on() {
        assert_eq!(pty_reply("\x1b[?6c".into()), "\x1b[?62;22;52c");
        // Other replies (the cursor's position, say) go as they are.
        assert_eq!(pty_reply("\x1b[3;1R".into()), "\x1b[3;1R");
    }

    #[test]
    fn a_connection_is_named_for_where_it_goes() {
        let p = |s: &str| Some(s.to_string());
        let host = || p("vps");
        let none = || None;
        // `ssh vps` is `vps`, whatever the far side calls itself.
        assert_eq!(workspace_label(p("ssh"), host, Some("server")), p("vps"));
        // An ssh whose destination could not be read: the far ranma's host,
        // else still `ssh`.
        assert_eq!(workspace_label(p("ssh"), none, Some("server")), p("server"));
        assert_eq!(workspace_label(p("ssh"), none, None), p("ssh"));
        // mosh and the like carry no destination we read; the ranma there says.
        assert_eq!(
            workspace_label(p("mosh-client"), none, Some("vps")),
            p("vps")
        );
        // Everything else keeps its program, and a ranma here stays `ranma`.
        assert_eq!(workspace_label(p("nvim"), none, None), p("nvim"));
        assert_eq!(workspace_label(p("ranma"), host, Some("vps")), p("ranma"));
        assert_eq!(workspace_label(None, host, Some("vps")), None);
    }

    #[test]
    fn the_marker_is_not_part_of_the_title() {
        assert_eq!(strip_nested_marker("⧉ ranma · nvim"), "nvim");
        assert_eq!(strip_nested_marker("⧉ ranma+ · nvim"), "nvim");
        assert_eq!(strip_nested_marker("⧉ ranma"), "");
        assert_eq!(strip_nested_marker("plain title"), "plain title");
        assert_eq!(strip_nested_marker("⧉ ranma@vps · nvim"), "nvim");
        assert_eq!(strip_nested_marker("⧉ ranma+@vps · a · b"), "a · b");
        assert_eq!(strip_nested_marker("⧉ ranma@vps"), "");
    }

    #[test]
    fn the_mark_carries_a_host_through() {
        assert_eq!(marker_host("⧉ ranma@vps · nvim"), Some("vps"));
        assert_eq!(marker_host("⧉ ranma+@vps"), Some("vps"));
        assert_eq!(marker_host("⧉ ranma · nvim"), None);
        assert_eq!(marker_host("user@vps: ~"), None);
        assert_eq!(marker(false, Some("vps")), "⧉ ranma@vps");
        assert_eq!(marker(true, None), NESTED_MARKER_ENGAGED);
        // A marked title with a host still reads as a ranma, engaged or not.
        let engaged = format!("{} · x", marker(true, Some("vps")));
        assert!(engaged.starts_with(NESTED_MARKER_ENGAGED));
        assert!(!marker(false, Some("vps")).starts_with(NESTED_MARKER_ENGAGED));
    }

    /// What a ranma running at `own` (reached over SSH, so naming itself) says,
    /// given what the ranma in its focused pane says.
    fn level(own: &str, inner_title: &str) -> String {
        own_title(
            false,
            marker_host(inner_title),
            Some(own),
            strip_nested_marker(inner_title),
        )
    }

    #[test]
    fn layered_ssh_names_only_the_innermost_host() {
        // desk -> ssh a -> ranma -> ssh b -> ranma -> ssh c -> ranma running nvim.
        let c = own_title(false, None, Some("c"), "nvim");
        assert_eq!(c, "⧉ ranma@c · nvim");
        let b = level("b", &c);
        assert_eq!(b, "⧉ ranma@c · nvim");
        let a = level("a", &b);
        assert_eq!(a, "⧉ ranma@c · nvim");
        // The desk names no host of its own; it still passes c out, once.
        let desk = own_title(false, marker_host(&a), None, strip_nested_marker(&a));
        assert_eq!(desk, "⧉ ranma@c · nvim");
        // A malformed chain does not grow further out.
        assert_eq!(marker_host("⧉ ranma@a@b · x"), Some("b"));
        assert_eq!(level("z", "⧉ ranma@a@b · x"), "⧉ ranma@b · x");
    }

    #[test]
    fn the_destination_of_an_ssh_command_line() {
        let d = |line: &str| ssh_destination(line.split_whitespace());
        assert_eq!(d("vps").as_deref(), Some("vps"));
        assert_eq!(d("me@vps htop").as_deref(), Some("vps"));
        assert_eq!(d("-p 2222 -A vps").as_deref(), Some("vps"));
        assert_eq!(d("-p2222 -tt vps").as_deref(), Some("vps"));
        assert_eq!(
            d("-J jump -l me box.example.com").as_deref(),
            Some("box.example.com")
        );
        assert_eq!(
            d("-o ProxyJump=j -vX ssh://me@vps:22").as_deref(),
            Some("vps")
        );
        assert_eq!(d("-4 -- vps ls").as_deref(), Some("vps"));
        assert_eq!(d("me@[::1]").as_deref(), Some("::1"));
        assert_eq!(d("-V"), None);
    }

    #[test]
    fn the_foreground_program_of_this_process() {
        let me = foreground_program(std::process::id()).unwrap();
        assert!(!me.is_empty());
        assert!(!hostname().is_empty() && !hostname().contains('.'));
    }
}

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

use alacritty_terminal::event::{Event as TermEvent, EventListener, WindowSize};
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
    /// A pane from `ranma open`.
    Open(crate::ipc::OpenSpec),
    /// The source has commits this binary lacks (see `update`).
    UpdateAvailable(crate::update::Behind),
}

/// Forwards a pane's terminal events to the UI thread.
#[derive(Clone)]
pub struct Proxy {
    id: PaneId,
    tx: Sender<AppEvent>,
    /// Set when a wakeup is queued and not yet drawn. A pane printing as fast as it
    /// can produces a wakeup per read; without this each one would be a message.
    wakeup_pending: Arc<AtomicBool>,
}

impl EventListener for Proxy {
    fn send_event(&self, event: TermEvent) {
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
    pub size: Size,
    /// What the program last set (OSC 0/2). Window rules match this.
    pub title: String,
    /// A name given with rename_pane; shown instead of the title while set.
    pub name: Option<String>,
    /// The pane's own child (the shell, or the `exec` command).
    pub pid: u32,
}

pub struct SpawnOptions<'a> {
    pub shell: Option<&'a str>,
    pub command: Option<&'a str>,
    pub scrollback_lines: usize,
    /// Where the child starts; ranma's own directory when `None`.
    pub cwd: Option<std::path::PathBuf>,
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
        let proxy = Proxy {
            id,
            tx,
            wakeup_pending: wakeup_pending.clone(),
        };

        let config = term::Config {
            scrolling_history: opts.scrollback_lines,
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

        let mut env = HashMap::new();
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

        let pty_opts = tty::Options {
            shell: Some(program),
            working_directory: opts.cwd.clone().or_else(|| std::env::current_dir().ok()),
            drain_on_exit: false,
            env,
        };
        let pty = tty::new(&pty_opts, size.window(), id).context("opening a PTY")?;
        let pid = pty.child().id();
        let event_loop = EventLoop::new(term.clone(), proxy, pty, false, false)
            .context("starting the PTY event loop")?;
        let sender = event_loop.channel();
        // The thread ends on Msg::Shutdown or when the child exits; dropping its
        // PTY then sends the child SIGHUP. Nothing needs to join it.
        let _ = event_loop.spawn();

        Ok(Pane {
            id,
            term,
            sender,
            wakeup_pending,
            size,
            title: String::new(),
            name: None,
            pid,
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

    /// The ranma in this pane is engaged: in WM mode, or with an engaged ranma
    /// of its own. The outer leader goes on down to it instead of stopping here.
    pub fn inner_engaged(&self) -> bool {
        self.title.starts_with(NESTED_MARKER_ENGAGED)
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
    pub fn drawn(&self) {
        self.wakeup_pending.store(false, Ordering::Release);
    }

    pub fn shutdown(&self) {
        let _ = self.sender.send(Msg::Shutdown);
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

/// A title without the nested-ranma marker: `⧉ ranma · nvim` is `nvim`.
pub fn strip_nested_marker(title: &str) -> &str {
    match title
        .strip_prefix(NESTED_MARKER_ENGAGED)
        .or_else(|| title.strip_prefix(NESTED_MARKER))
    {
        Some(rest) => rest.strip_prefix(" · ").unwrap_or(rest.trim_start()),
        None => title,
    }
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
    use super::strip_nested_marker;

    #[test]
    fn the_marker_is_not_part_of_the_title() {
        assert_eq!(strip_nested_marker("⧉ ranma · nvim"), "nvim");
        assert_eq!(strip_nested_marker("⧉ ranma+ · nvim"), "nvim");
        assert_eq!(strip_nested_marker("⧉ ranma"), "");
        assert_eq!(strip_nested_marker("plain title"), "plain title");
    }
}

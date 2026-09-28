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
    pub title: String,
}

pub struct SpawnOptions<'a> {
    pub shell: Option<&'a str>,
    pub command: Option<&'a str>,
    pub scrollback_lines: usize,
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

        let pty_opts = tty::Options {
            shell: Some(program),
            working_directory: std::env::current_dir().ok(),
            drain_on_exit: false,
            env,
        };
        let pty = tty::new(&pty_opts, size.window(), id).context("opening a PTY")?;
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
        })
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
        }
    }

    /// Called when a frame including this pane has been drawn.
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

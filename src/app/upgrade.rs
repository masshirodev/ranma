//! Upgrading a server in place (DESIGN.md): the state a server hands to the
//! new build it exec's into, taking it out of a running server, and building a
//! running server back from it. The exec itself, and keeping the descriptors
//! open across it, are the event loop's (`run`).

use std::collections::{BTreeMap, HashMap};
use std::os::fd::RawFd;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::App;
use super::session::Session;
use crate::config::Config;
use crate::layout::PaneId;
use crate::nestbar::Report;
use crate::pane::{AppEvent, HOLD_OUTPUT, Pane, Size};
use crate::snapshot::Snapshot;
use crate::workspace::Workspace;

/// The handover's format. A build that does not know it refuses it at the
/// dry run, before the old server exec's anything.
pub const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
pub struct Handover {
    pub version: u32,
    pub name: String,
    /// The listening socket, kept open across the exec.
    pub listener_fd: RawFd,
    /// The client whose size the screen has (see DESIGN.md, "Several terminals
    /// on one server").
    pub client: Option<ClientHandover>,
    /// The other clients, most recently active first. Defaulted, so a handover
    /// from a build with one client still reads.
    #[serde(default)]
    pub others: Vec<ClientHandover>,
    pub cols: u16,
    pub rows: u16,
    pub state: State,
}

/// An attached client: its connection, kept open, and what it said when it
/// attached (its size as it is now).
#[derive(Serialize, Deserialize)]
pub struct ClientHandover {
    pub fd: RawFd,
    pub hello: crate::proto::Hello,
}

#[derive(Serialize, Deserialize)]
pub struct State {
    pub next_id: PaneId,
    pub panes: Vec<PaneHandover>,
    pub workspaces: BTreeMap<u8, Workspace>,
    pub current: u8,
    pub sessions: Vec<Session>,
    pub active_session: usize,
    pub scratch: Workspace,
    pub scratch_shown: bool,
    pub synced: Vec<PaneId>,
    pub return_focus: Vec<(PaneId, PaneId)>,
    pub reports: Vec<(PaneId, Report)>,
    pub last_focused: Option<PaneId>,
}

#[derive(Serialize, Deserialize)]
pub struct PaneHandover {
    pub id: PaneId,
    /// Its PTY master, kept open across the exec.
    pub fd: RawFd,
    pub pid: u32,
    pub cols: u16,
    pub rows: u16,
    pub title: String,
    pub name: Option<String>,
    pub snapshot: Snapshot,
}

impl PaneHandover {
    /// Its program drew on the alternate screen, which is not handed over: it
    /// is asked to draw again.
    pub fn was_full_screen(&self) -> bool {
        self.snapshot.modes.contains("\x1b[?1049h")
    }
}

/// Stop every pane reading its PTY, and wait for reads already under way to
/// be parsed: from here, what programs print waits in the kernel.
pub fn hold_output() {
    HOLD_OUTPUT.store(true, Ordering::Release);
    std::thread::sleep(std::time::Duration::from_millis(50));
}

pub fn release_output() {
    HOLD_OUTPUT.store(false, Ordering::Release);
}

impl App {
    /// Everything the new build needs, panes as text. Output must be held
    /// (`hold_output`) first. The server can go on after it: only a program on
    /// the alternate screen needs to draw again (see `snapshot::take`).
    pub(crate) fn hand_over(&mut self) -> State {
        self.abandon_paste();
        let mut ids: Vec<PaneId> = self.panes.keys().copied().collect();
        ids.sort_unstable();
        let panes = ids
            .into_iter()
            .filter_map(|id| {
                let p = self.panes.get(&id)?;
                let snapshot = crate::snapshot::take(&mut p.term.lock());
                Some(PaneHandover {
                    id,
                    fd: p.master_fd,
                    pid: p.pid,
                    cols: p.size.cols,
                    rows: p.size.rows,
                    title: p.title.clone(),
                    name: p.name.clone(),
                    snapshot,
                })
            })
            .collect();
        State {
            next_id: self.next_id,
            panes,
            workspaces: self.workspaces.clone(),
            current: self.current,
            sessions: self.sessions.clone(),
            active_session: self.active_session,
            scratch: self.scratch.clone(),
            scratch_shown: self.scratch_shown,
            synced: self.synced.iter().copied().collect(),
            return_focus: self.return_focus.iter().map(|(a, b)| (*a, *b)).collect(),
            reports: self.reports.iter().map(|(a, b)| (*a, b.clone())).collect(),
            last_focused: self.last_focused,
        }
    }

    /// A running server again, from what the old build handed over: every
    /// pane adopted from its PTY, its screen put back.
    ///
    /// If a pane cannot be adopted, nothing adopted so far is dropped (that
    /// would hang up its shell): they are forgotten, and the error goes back
    /// for the caller to fall back to the old build with.
    ///
    /// # Safety
    /// The handover's descriptors must be the ones kept across the exec,
    /// owned by nothing else.
    pub(crate) unsafe fn take_over(
        config: Config,
        tx: Sender<AppEvent>,
        state: State,
        cols: u16,
        rows: u16,
    ) -> Result<App> {
        let mut app = App::new(config, tx.clone(), cols, rows);
        let scrollback = app.config.settings.scrollback_lines;
        let mut adopted: HashMap<PaneId, Pane> = HashMap::new();
        for p in &state.panes {
            let size = Size {
                cols: p.cols,
                rows: p.rows,
            };
            // SAFETY: the caller vouches for the descriptors.
            match unsafe {
                Pane::adopt(p.id, size, p.fd, p.pid, &p.snapshot, scrollback, tx.clone())
            } {
                Ok(mut pane) => {
                    pane.title = p.title.clone();
                    pane.name = p.name.clone();
                    adopted.insert(p.id, pane);
                }
                Err(e) => {
                    for (_, pane) in adopted.drain() {
                        std::mem::forget(pane);
                    }
                    return Err(e.context(format!("adopting pane {}", p.id)));
                }
            }
        }
        app.panes = adopted;
        app.next_id = state.next_id;
        app.workspaces = state.workspaces;
        app.current = state.current;
        app.sessions = state.sessions;
        app.active_session = state.active_session;
        app.scratch = state.scratch;
        app.scratch_shown = state.scratch_shown;
        app.synced = state.synced.into_iter().collect();
        app.return_focus = state.return_focus.into_iter().collect();
        app.reports = state.reports.into_iter().collect();
        app.last_focused = state.last_focused;
        app.relayout();
        Ok(app)
    }
}

/// Read a handover and check it: the format, and every pane's screen fed to
/// an emulator that is thrown away. Nothing is adopted, so it is safe to run
/// in a process of its own before the exec (`ranma --check-handover`).
pub fn check(path: &Path) -> Result<Handover> {
    let h = read(path)?;
    for p in &h.state.panes {
        let size = Size {
            cols: p.cols.max(2),
            rows: p.rows.max(1),
        };
        let mut t = alacritty_terminal::Term::new(
            alacritty_terminal::term::Config::default(),
            &size,
            alacritty_terminal::event::VoidListener,
        );
        crate::snapshot::restore(&mut t, &p.snapshot);
    }
    if h.state.active_session >= h.state.sessions.len() {
        bail!("the shown session is not one of the sessions");
    }
    Ok(h)
}

pub fn read(path: &Path) -> Result<Handover> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let v: serde_json::Value = serde_json::from_str(&text).context("the handover is not JSON")?;
    let version = v.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
    if version != VERSION as u64 {
        bail!("handover format {version}, and this build reads {VERSION}");
    }
    serde_json::from_value(v).context("reading the handover")
}

pub fn write(path: &Path, h: &Handover) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("writing {}", path.display()))?;
    f.write_all(serde_json::to_string(h)?.as_bytes())?;
    Ok(())
}

/// `ranma --check-handover FILE`: `check`, as the new build's answer to the
/// old server asking whether it can take over.
pub fn check_handover(path: &Path) -> Result<()> {
    check(path).map(|_| ())
}

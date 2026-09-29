//! The window manager: state, input routing, actions, and the event loop.
//!
//! One thread owns all of this. PTY threads, the input thread, exec modules and
//! the config watcher only send it messages, so nothing here is shared and
//! nothing needs a lock except each pane's `Term`, which alacritty_terminal's own
//! thread also writes.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use alacritty_terminal::event::Event as TermEvent;
use alacritty_terminal::vte::ansi::CursorShape;
use anyhow::Result;
use crossterm::cursor::SetCursorStyle;
use crossterm::event::{Event, KeyEvent, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};
use mlua::{Function, Lua, Table, Value};

use crate::action::{Action, Dir, WorkspaceTarget};
use crate::bar::{self, Click, Piece, Segment, Style};
use crate::config::{self, BindAction, Config, Event as HookEvent, Layout, ModuleKind};
use crate::input;
use crate::layout::{self, PaneId, Placement, Rect, Split, TabBar};
use crate::pane::{AppEvent, Pane, Size, SpawnOptions};
use crate::render::CursorState;
use crate::theme::{BarPosition, BorderStyle};
use crate::workspace::Workspace;

mod copy;
mod drag;
mod rules;
mod run;
mod session;
mod switch;

pub use copy::CopyState;
pub use drag::drop_half;
pub use run::{run, run_server};
use session::Session;
pub use switch::PickerLayout;

/// The frame cap. Output arriving faster than this is coalesced: the pane is drawn
/// at its latest state once per interval, not once per read.
const FRAME: Duration = Duration::from_micros(8_333);
/// Editors save in several steps (write, rename, chmod); one reload for all of them.
const RELOAD_DEBOUNCE: Duration = Duration::from_millis(150);
/// Hooks that run actions that fire hooks: stop before it becomes a loop.
const MAX_LUA_DEPTH: u8 = 4;
/// The workspace number the scratchpad reports in hook payloads and state.
const SCRATCHPAD: u8 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Keys go to the focused pane; only the leader is looked at.
    Normal,
    /// Keys are looked up in the bind table.
    Wm,
    /// Keys move a cursor through the focused pane's scrollback (see `copy`).
    Copy,
}

/// Where one pane is drawn this frame.
#[derive(Debug, Clone, Copy)]
pub struct PaneView {
    pub id: PaneId,
    /// Including the border.
    pub outer: Rect,
    /// The pane's own cells: its PTY size.
    pub inner: Rect,
    pub focused: bool,
    pub floating: bool,
}

/// Everything a frame needs to know about where things go.
#[derive(Debug, Default)]
pub struct Frame {
    /// In drawing order: tiles, then floats bottom to top, then the scratchpad.
    pub views: Vec<PaneView>,
    /// Panes not shown (background tabs, a fullscreen workspace's others) with the
    /// inner size they would have, so showing them does not resize the program.
    pub hidden: Vec<(PaneId, Rect)>,
    pub tab_bars: Vec<TabBar>,
    /// Where the scratchpad is drawn, when it is shown.
    pub overlay: Option<Rect>,
}

#[derive(Debug, Clone, Copy)]
enum Drag {
    Move {
        id: PaneId,
        dx: u16,
        dy: u16,
    },
    Resize {
        id: PaneId,
        start: Rect,
        x: u16,
        y: u16,
    },
    /// A tile border between two panes; `last` is the pointer's position along
    /// the axis at the previous event.
    Edge {
        id: PaneId,
        split: Split,
        last: u16,
    },
    /// A tile picked up by its title bar, to drop beside another.
    Tile {
        id: PaneId,
    },
    /// Selecting text in a pane with the mouse (see `drag`, "Selecting").
    Select {
        id: PaneId,
    },
}

/// The parts of state hooks and state-driven modules react to.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Observed {
    session: String,
    /// Whether this ranma is engaged (see `engaged`); announced to a ranma around it.
    engaged: bool,
    focus: Option<PaneId>,
    workspace: u8,
    mode_wm: bool,
    title: String,
    panes: usize,
}

pub struct App {
    pub config: Config,
    pub panes: HashMap<PaneId, Pane>,
    /// The shown session's workspaces (see `session` for how sessions swap).
    workspaces: BTreeMap<u8, Workspace>,
    current: u8,
    sessions: Vec<Session>,
    active_session: usize,
    scratch: Workspace,
    scratch_shown: bool,
    pub mode: Mode,
    /// A one-line message for the bar; cleared by the next key.
    pub status: Option<String>,
    screen: Rect,
    next_id: PaneId,
    tx: Sender<AppEvent>,
    dirty: bool,
    quit: bool,
    visible: HashSet<PaneId>,
    drag: Option<Drag>,
    /// What the last completed event left behind, for change detection.
    observed: Observed,
    lua_depth: u8,

    /// Lua and exec module output, keyed by module name.
    module_values: HashMap<String, Segment>,
    module_due: HashMap<String, Instant>,
    module_running: HashSet<String>,
    /// Bumped on config reload so results of the old config's execs are dropped.
    module_generation: u64,
    reload_at: Option<Instant>,

    picker: Option<crate::picker::Picker>,
    copy: Option<CopyState>,
    /// Bytes for the host terminal itself (OSC 52 clipboard writes), written by
    /// the event loop after the event that queued them.
    host_out: Vec<Vec<u8>>,
    /// (pane, rule index) pairs already applied, so title rules fire once.
    rules_applied: HashSet<(PaneId, usize)>,
    /// The last pane that had focus anywhere. A new session or an empty
    /// scratchpad has no focused pane of its own, and still starts where you were.
    last_focused: Option<PaneId>,
    /// The host terminal's colours, asked once at startup (see `hostcolors`).
    pub host_colors: crate::hostcolors::HostColors,
    pub toasts: crate::toast::Toasts,
    /// Where a tile being dragged would land (see `drag`).
    drop_preview: Option<(PaneId, Dir)>,
    /// The source has commits this binary lacks (see `update`).
    update_available: Option<crate::update::Behind>,
    /// The pane holding a mouse selection, cleared by the next click or key.
    selection_pane: Option<PaneId>,
    /// The last left click, for double and triple clicks: when, where, how many.
    last_click: Option<(Instant, u16, u16, u8)>,
    /// The title last given to the host terminal (see `announce`).
    host_title: String,
    /// The detach action ran; the event loop sends the client away.
    detach_requested: bool,
}

impl App {
    pub fn new(config: Config, tx: Sender<AppEvent>, cols: u16, rows: u16) -> App {
        let mut workspaces = BTreeMap::new();
        workspaces.insert(1, Workspace::default());
        let mut app = App {
            config,
            panes: HashMap::new(),
            workspaces,
            current: 1,
            sessions: vec![Session {
                name: "main".into(),
                workspaces: BTreeMap::new(),
                current: 1,
            }],
            active_session: 0,
            scratch: Workspace::default(),
            scratch_shown: false,
            mode: Mode::Normal,
            status: None,
            screen: Rect::new(0, 0, cols, rows),
            next_id: 1,
            tx,
            dirty: true,
            quit: false,
            visible: HashSet::new(),
            drag: None,
            observed: Observed {
                session: "main".into(),
                workspace: 1,
                ..Default::default()
            },
            lua_depth: 0,
            module_values: HashMap::new(),
            module_due: HashMap::new(),
            module_running: HashSet::new(),
            module_generation: 0,
            reload_at: None,
            picker: None,
            copy: None,
            host_out: Vec::new(),
            rules_applied: HashSet::new(),
            last_focused: None,
            host_colors: Default::default(),
            toasts: Default::default(),
            drop_preview: None,
            update_available: None,
            selection_pane: None,
            last_click: None,
            host_title: String::new(),
            detach_requested: false,
        };
        app.schedule_modules(Instant::now());
        app
    }

    // ---- where things are ------------------------------------------------------

    /// The workspace keys and actions apply to: the scratchpad while it is shown.
    fn active(&self) -> &Workspace {
        if self.scratch_shown {
            &self.scratch
        } else {
            self.workspaces
                .get(&self.current)
                .expect("current workspace exists")
        }
    }

    fn active_mut(&mut self) -> &mut Workspace {
        if self.scratch_shown {
            &mut self.scratch
        } else {
            self.workspaces
                .get_mut(&self.current)
                .expect("current workspace exists")
        }
    }

    pub fn focused(&self) -> Option<PaneId> {
        self.active().focused
    }

    /// The workspace holding a pane, and its number (the scratchpad's is 0).
    fn locate(&self, id: PaneId) -> Option<u8> {
        if self.scratch.contains(id) {
            return Some(SCRATCHPAD);
        }
        self.workspaces
            .iter()
            .find(|(_, ws)| ws.contains(id))
            .map(|(n, _)| *n)
    }

    fn ws_mut(&mut self, n: u8) -> &mut Workspace {
        if n == SCRATCHPAD {
            &mut self.scratch
        } else {
            self.workspaces.entry(n).or_default()
        }
    }

    pub fn current_workspace(&self) -> u8 {
        self.current
    }

    pub fn scratch_state(&self) -> (bool, bool) {
        (!self.scratch.is_empty(), self.scratch_shown)
    }

    /// Workspaces to list in the bar: (number, is current, has panes, urgent).
    /// (number, is current, has panes, urgent, name) for each listed workspace.
    pub fn workspace_list(&self) -> Vec<(u8, bool, bool, bool, Option<String>)> {
        let mut nums: Vec<u8> = self
            .workspaces
            .iter()
            .filter(|(n, ws)| **n == self.current || !ws.is_empty() || ws.name.is_some())
            .map(|(n, _)| *n)
            .collect();
        if self.config.workspaces_show_all {
            nums.extend(1..=10);
            nums.sort_unstable();
            nums.dedup();
        }
        nums.into_iter()
            .map(|n| {
                let ws = self.workspaces.get(&n);
                (
                    n,
                    n == self.current,
                    ws.is_some_and(|w| !w.is_empty()),
                    ws.is_some_and(|w| w.urgent),
                    ws.and_then(|w| w.name.clone()),
                )
            })
            .collect()
    }

    // ---- geometry ----------------------------------------------------------

    pub fn bar_rect(&self) -> Option<Rect> {
        let s = self.screen;
        match self.config.theme.bar.position {
            BarPosition::Hidden => None,
            _ if s.h == 0 => None,
            BarPosition::Top => Some(Rect::new(s.x, s.y, s.w, 1)),
            BarPosition::Bottom => Some(Rect::new(s.x, s.bottom() - 1, s.w, 1)),
        }
    }

    /// The area panes are laid out in: the screen minus the bar and outer gaps.
    fn workspace_area(&self) -> Rect {
        let s = self.screen;
        let a = match self.bar_rect() {
            None => s,
            Some(_) if self.config.theme.bar.position == BarPosition::Top => {
                Rect::new(s.x, s.y + 1, s.w, s.h.saturating_sub(1))
            }
            Some(_) => Rect::new(s.x, s.y, s.w, s.h.saturating_sub(1)),
        };
        let g = &self.config.theme.gaps;
        a.inset(g.outer_horizontal, g.outer_vertical)
    }

    fn scratch_area(&self) -> Rect {
        self.workspace_area().centered(80, 80)
    }

    fn border(&self) -> u16 {
        match self.config.theme.border.style {
            BorderStyle::None => 0,
            _ => 1,
        }
    }

    pub fn frame(&self) -> Frame {
        let mut f = Frame::default();
        let area = self.workspace_area();
        let gap = self.config.theme.gaps.inner;
        let b = self.border();
        let focused = self.focused();
        let view = |id, outer: Rect, floating| PaneView {
            id,
            outer,
            inner: outer.inset(b, b),
            focused: Some(id) == focused,
            floating,
        };

        if let Some(ws) = self.workspaces.get(&self.current) {
            let full = ws.focused.filter(|f| ws.fullscreen && ws.contains(*f));
            let lay = ws.tree.layout_full(area, gap);
            match full {
                Some(fid) => {
                    f.views.push(view(fid, area, false));
                    for (id, r) in lay.visible.iter().chain(&lay.hidden) {
                        if *id != fid {
                            f.hidden.push((*id, r.inset(b, b)));
                        }
                    }
                    for (id, r) in &ws.floating {
                        if *id != fid {
                            f.hidden.push((*id, r.clamp_into(area).inset(b, b)));
                        }
                    }
                }
                None => {
                    for (id, r) in lay.visible {
                        f.views.push(view(id, r, false));
                    }
                    f.hidden
                        .extend(lay.hidden.iter().map(|(id, r)| (*id, r.inset(b, b))));
                    f.tab_bars = lay.tab_bars;
                    for (id, r) in &ws.floating {
                        f.views.push(view(*id, r.clamp_into(area), true));
                    }
                }
            }
        }

        // The scratchpad is a layer of free floats over whatever is shown.
        if self.scratch_shown {
            for (id, r) in &self.scratch.floating {
                f.views.push(view(*id, r.clamp_into(area), true));
            }
        }
        f
    }

    /// Rects of the panes focus can move between: the active layer only.
    fn focus_rects(&self) -> Vec<(PaneId, Rect)> {
        let active = self.active();
        self.frame()
            .views
            .into_iter()
            .filter(|v| active.contains(v.id))
            .map(|v| (v.id, v.outer))
            .collect()
    }

    /// Push the current layout down to every PTY and note what is visible.
    fn relayout(&mut self) {
        let frame = self.frame();
        let sizes: Vec<(PaneId, Rect)> = frame
            .views
            .iter()
            .map(|v| (v.id, v.inner))
            .chain(frame.hidden.iter().copied())
            .collect();
        for (id, inner) in sizes {
            // Panes being resized by the mouse keep their PTY size until the
            // button is released: one resize, not one per mouse event.
            match self.drag {
                Some(Drag::Resize { id: d, .. }) if d == id => continue,
                Some(Drag::Edge { .. }) => continue,
                _ => {}
            }
            if let Some(p) = self.panes.get_mut(&id) {
                p.resize(Size {
                    cols: inner.w,
                    rows: inner.h,
                });
            }
        }
        self.visible = frame.views.iter().map(|v| v.id).collect();
        self.dirty = true;
    }

    pub fn fullscreen(&self) -> bool {
        !self.scratch_shown && self.active().fullscreen
    }

    pub fn focused_title(&self) -> Option<&str> {
        self.focused()
            .and_then(|f| self.panes.get(&f))
            .map(|p| p.label())
    }

    // ---- panes ---------------------------------------------------------------

    pub fn open_pane(&mut self, command: Option<&str>) -> Result<()> {
        self.open_pane_at(command, None, None).map(|_| ())
    }

    /// Open a pane; with `side`, on that side of the focused tile rather than
    /// where the layout would put it.
    /// With `cwd`, it starts there instead of in the focused pane's directory.
    /// Returns the new pane.
    pub fn open_pane_at(
        &mut self,
        command: Option<&str>,
        side: Option<Dir>,
        cwd: Option<std::path::PathBuf>,
    ) -> Result<PaneId> {
        let spawn_cwd = cwd.or_else(|| {
            self.focused()
                .or(self.last_focused)
                .and_then(|f| self.panes.get(&f))
                .and_then(|p| p.cwd())
        });
        let id = self.next_id;
        self.next_id += 1;
        let focused = self.focused().filter(|f| self.active().tree.contains(*f));
        let focused_rect = focused.and_then(|f| {
            self.frame()
                .views
                .into_iter()
                .find(|v| v.id == f)
                .map(|v| v.outer)
        });
        let placement = match self.config.settings.layout {
            Layout::Dwindle => Placement::Dwindle,
            // i3's default for a fresh split is side by side.
            Layout::Manual => Placement::Manual(Split::Horizontal),
        };
        let area = self.workspace_area();
        let in_scratch = self.scratch_shown;
        let ws = self.active_mut();
        match (side, focused) {
            // Everything in the scratchpad floats: a new pane joins the pile.
            _ if in_scratch => {
                let r = ws.cascade(area, 80, 80);
                ws.floating.push((id, r));
            }
            (Some(dir), Some(f)) => {
                ws.tree.insert_beside(id, f, dir);
            }
            _ => ws.tree.insert(id, focused, focused_rect, placement),
        }
        ws.fullscreen = false;

        // Spawn at the size the layout gives it, so the program starts at its real
        // size instead of getting a resize immediately after starting.
        let size = self.frame().views.into_iter().find(|v| v.id == id).map_or(
            Size { cols: 80, rows: 24 },
            |v| Size {
                cols: v.inner.w,
                rows: v.inner.h,
            },
        );
        // Where the focused pane's shell is now, as captured before this pane
        // took focus: a new pane continues where you were working.
        let s = &self.config.settings;
        let opts = SpawnOptions {
            shell: s.shell.as_deref(),
            command,
            scrollback_lines: s.scrollback_lines,
            cwd: spawn_cwd,
        };
        match Pane::spawn(id, size, &opts, self.tx.clone()) {
            Ok(pane) => {
                self.panes.insert(id, pane);
                self.focus(id);
                self.relayout();
                if let Some(cmd) = command {
                    self.apply_command_rules(id, cmd);
                }
                let ws = if self.scratch_shown {
                    SCRATCHPAD
                } else {
                    self.current
                };
                self.emit(HookEvent::PaneOpen, |t| {
                    t.set("pane", id)?;
                    t.set("workspace", ws)
                });
                Ok(id)
            }
            Err(e) => {
                self.active_mut().tree.remove(id);
                self.relayout();
                Err(e)
            }
        }
    }

    /// Where focus goes when `id` leaves a visible workspace: the neighbour the
    /// eye lands on (left, up, right, down), else any pane left there.
    fn successor(&self, ws_num: u8, id: PaneId) -> Option<PaneId> {
        let ws = if ws_num == SCRATCHPAD {
            &self.scratch
        } else {
            self.workspaces.get(&ws_num)?
        };
        let rects: Vec<(PaneId, Rect)> = self
            .frame()
            .views
            .into_iter()
            .filter(|v| ws.contains(v.id))
            .map(|v| (v.id, v.outer))
            .collect();
        [Dir::Left, Dir::Up, Dir::Right, Dir::Down]
            .into_iter()
            .find_map(|d| layout::neighbour(&rects, id, d))
            .or_else(|| ws.panes().into_iter().find(|p| *p != id))
    }

    /// Remove a pane from whichever workspace holds it and fix that one's focus.
    /// Returns the workspace it was in and its float rect if it floated.
    fn detach(&mut self, id: PaneId) -> Option<(u8, Option<Rect>)> {
        let n = self.locate(id)?;
        let next = self.successor(n, id);
        let ws = self.ws_mut(n);
        let was = ws.take(id)?;
        if ws.focused == Some(id) {
            ws.focused = next.filter(|p| *p != id);
        }
        ws.fullscreen &= ws.focused.is_some();
        Some((n, was))
    }

    /// Drop empty workspaces other than the current one, and hide an empty scratchpad.
    fn tidy(&mut self) {
        let cur = self.current;
        self.workspaces
            .retain(|n, ws| *n == cur || !ws.is_empty() || ws.name.is_some());
        if self.scratch.is_empty() {
            self.scratch_shown = false;
        }
    }

    fn close_pane(&mut self, id: PaneId) {
        let n = match self.detach(id) {
            Some((n, _)) => n,
            None if self.detach_hidden(id) => 0,
            None => return,
        };
        if self.copy.as_ref().is_some_and(|c| c.pane == id) {
            self.copy = None;
            if self.mode == Mode::Copy {
                self.mode = Mode::Normal;
            }
        }
        self.panes.remove(&id);
        self.rules_applied.retain(|(p, _)| *p != id);
        if self.panes.is_empty() {
            self.quit = true;
            return;
        }
        self.tidy();
        self.drop_empty_sessions();
        self.relayout();
        self.emit(HookEvent::PaneClose, |t| {
            t.set("pane", id)?;
            t.set("workspace", n)
        });
    }

    /// Focus a pane in the active workspace: reveal its tab, raise it if it floats.
    fn focus(&mut self, id: PaneId) {
        let ws = self.active_mut();
        if !ws.contains(id) {
            return;
        }
        ws.focused = Some(id);
        ws.tree.reveal(id);
        ws.raise(id);
        self.dirty = true;
    }

    fn focused_pane(&self) -> Option<&Pane> {
        self.focused().and_then(|f| self.panes.get(&f))
    }

    fn switch_workspace(&mut self, target: u8) {
        self.scratch_shown = false;
        if target == self.current {
            return;
        }
        self.current = target;
        let ws = self.workspaces.entry(target).or_default();
        ws.urgent = false;
        self.tidy();
        self.relayout();
    }

    fn resolve(&self, t: &WorkspaceTarget) -> u8 {
        match t {
            WorkspaceTarget::Index(n) => *n,
            // Relative moves stay among 1-10, wrapping, like a row of keys.
            WorkspaceTarget::Next => {
                if self.current >= 10 {
                    1
                } else {
                    self.current + 1
                }
            }
            WorkspaceTarget::Prev => {
                if self.current <= 1 {
                    10
                } else {
                    self.current - 1
                }
            }
            WorkspaceTarget::Empty => (1..=99u8)
                .find(|n| self.workspaces.get(n).is_none_or(|w| w.is_empty()))
                .unwrap_or(self.current),
        }
    }

    /// Move the focused pane to workspace `target` (0 is the scratchpad).
    fn move_focused_to(&mut self, target: u8, follow: bool) {
        if let Some(id) = self.focused() {
            self.move_pane_to(id, target, follow);
        }
    }

    /// Move a pane of the shown session to workspace `target` (0 is the scratchpad).
    fn move_pane_to(&mut self, id: PaneId, target: u8, follow: bool) {
        if self.locate(id).is_none_or(|from| from == target) {
            return;
        }
        let Some((_, float)) = self.detach(id) else {
            return;
        };
        let area = self.workspace_area();
        let gap = self.config.theme.gaps.inner;
        let ws = self.ws_mut(target);
        match float {
            // A float stays where it was, wherever it goes.
            Some(r) => ws.floating.push((id, r)),
            // Everything in the scratchpad floats: a tile sent there joins the pile.
            None if target == SCRATCHPAD => {
                let r = ws.cascade(area, 80, 80);
                ws.floating.push((id, r));
            }
            None => {
                let anchor = ws.focused.filter(|f| ws.tree.contains(*f));
                let rect = anchor.and_then(|a| {
                    ws.tree
                        .layout(area, gap)
                        .into_iter()
                        .find(|(p, _)| *p == a)
                        .map(|(_, r)| r)
                });
                ws.tree.insert(id, anchor, rect, Placement::Dwindle);
            }
        }
        ws.focused = Some(id);
        ws.fullscreen = false;
        if follow && target != SCRATCHPAD {
            self.switch_workspace(target);
        }
        self.tidy();
        self.relayout();
    }

    fn toggle_floating(&mut self) {
        if self.scratch_shown {
            self.status = Some("scratchpad panes always float".into());
            return;
        }
        if let Some(id) = self.focused() {
            self.toggle_floating_pane(id);
        }
    }

    // ---- events --------------------------------------------------------------

    pub fn handle(&mut self, ev: AppEvent) {
        match ev {
            AppEvent::Input(ev) => self.handle_input(ev),
            AppEvent::InputClosed => self.quit = true,
            AppEvent::Pane(id, ev) => self.handle_pane_event(id, ev),
            AppEvent::Module {
                name,
                generation,
                text,
            } => {
                self.module_running.remove(&name);
                if generation == self.module_generation {
                    let seg = match text {
                        Ok(line) => {
                            let fmt = match self.config.modules.get(&name).map(|m| &m.kind) {
                                Some(ModuleKind::Exec { format, .. }) => format.clone(),
                                _ => None,
                            };
                            vec![Piece::new(
                                bar::format_output(fmt.as_deref(), &line),
                                Style::Normal,
                            )]
                        }
                        Err(e) => vec![Piece::new(format!("{name}: {e}"), Style::Urgent)],
                    };
                    self.set_module_value(name, seg);
                }
            }
            AppEvent::Toast {
                text,
                level,
                timeout,
            } => self.toast(text, level, timeout),
            AppEvent::Action(a) => self.run_action(a),
            AppEvent::Open(spec) => self.open_spec(spec),
            AppEvent::UpdateAvailable(b) => self.update_found(b),
            // The event loop (run.rs) deals with clients itself.
            AppEvent::Attach { .. }
            | AppEvent::ClientInput(..)
            | AppEvent::ClientGone(_)
            | AppEvent::Status(_) => {}
            AppEvent::ConfigChanged => self.reload_at = Some(Instant::now() + RELOAD_DEBOUNCE),
        }
        self.after_event();
    }

    /// Fire hooks for whatever the event changed, and refresh state-driven modules.
    fn after_event(&mut self) {
        // Copy mode belongs to one pane; it ends when that pane is no longer the
        // focused one (switched away by a click, a hook, a workspace change).
        if self
            .copy
            .as_ref()
            .is_some_and(|c| Some(c.pane) != self.focused())
        {
            self.exit_copy_mode();
        }
        let now = Observed {
            engaged: self.engaged(),
            session: self.session_name().to_string(),
            focus: self.focused(),
            workspace: self.current,
            mode_wm: self.mode == Mode::Wm,
            title: self.focused_title().unwrap_or("").to_string(),
            panes: self.panes.len(),
        };
        if now == self.observed {
            return;
        }
        self.announce();
        let before = std::mem::replace(&mut self.observed, now.clone());
        if now.focus.is_some() {
            self.last_focused = now.focus;
        }

        if now.focus != before.focus {
            // Programs that asked for focus events get them, as in any terminal.
            for (id, gained) in [(before.focus, false), (now.focus, true)] {
                if let Some(p) = id.and_then(|i| self.panes.get(&i))
                    && let Some(b) = input::encode_focus(gained, p.modes())
                {
                    p.write(b);
                }
            }
            self.emit(HookEvent::FocusChange, |t| {
                t.set("pane", now.focus)?;
                t.set("previous", before.focus)
            });
        }
        if now.session != before.session {
            self.emit_session_switch(now.session.clone(), before.session.clone());
        }
        if now.workspace != before.workspace {
            self.emit(HookEvent::WorkspaceChange, |t| {
                t.set("workspace", now.workspace)?;
                t.set("previous", before.workspace)
            });
        }
        if now.mode_wm != before.mode_wm {
            self.emit(HookEvent::ModeChange, |t| {
                t.set("mode", if now.mode_wm { "wm" } else { "normal" })
            });
        }
        self.render_state_modules();
        self.dirty = true;
    }

    fn handle_pane_event(&mut self, id: PaneId, ev: TermEvent) {
        let visible = self.visible.contains(&id);
        let Some(pane) = self.panes.get_mut(&id) else {
            return;
        };
        match ev {
            // A pane nobody can see does not cost a frame. Its wakeup flag stays
            // set, so it sends no more wakeups until it is shown and drawn.
            TermEvent::Wakeup => self.dirty |= visible,
            TermEvent::Title(t) => {
                pane.title = t.clone();
                self.dirty |= visible;
                if !self.config.rules.is_empty() {
                    let clean = crate::pane::strip_nested_marker(&t).to_string();
                    self.apply_title_rules(id, &clean);
                }
            }
            TermEvent::ResetTitle => {
                pane.title.clear();
                self.dirty |= visible;
            }
            // Replies to queries the program made (device attributes, cursor
            // position): they go back to the program, not to the host.
            TermEvent::PtyWrite(s) => pane.write(s.into_bytes()),
            // A program asking for a colour (OSC 4/10/11/12). One it set itself
            // wins; otherwise the host's, since those are what it is drawn with.
            // Unknown stays unanswered, as before.
            TermEvent::ColorRequest(index, fmt) => {
                let own = pane.term.lock().colors()[index];
                let host =
                    self.host_colors
                        .get(index)
                        .map(|c| alacritty_terminal::vte::ansi::Rgb {
                            r: c.r,
                            g: c.g,
                            b: c.b,
                        });
                if let Some(rgb) = own.or(host) {
                    pane.write(fmt(rgb).into_bytes());
                }
            }
            TermEvent::TextAreaSizeRequest(fmt) => {
                let size = pane.size;
                let reply = fmt(alacritty_terminal::event::WindowSize {
                    num_lines: size.rows,
                    num_cols: size.cols,
                    cell_width: 0,
                    cell_height: 0,
                });
                pane.write(reply.into_bytes());
            }
            TermEvent::Bell if !visible => {
                // A bell where you cannot see it gets a toast saying where.
                let title = pane.label().to_string();
                let place = match (self.locate(id), self.locate_hidden(id)) {
                    (Some(SCRATCHPAD), _) => "the scratchpad".to_string(),
                    (Some(n), _) => format!("workspace {n}"),
                    (None, Some((si, n))) => format!("{} · workspace {n}", self.sessions[si].name),
                    (None, None) => "a pane".to_string(),
                };
                let what = if title.is_empty() {
                    "bell".to_string()
                } else {
                    format!("bell: {title}")
                };
                self.toast(
                    format!("{what} ({place})"),
                    crate::toast::Level::Normal,
                    None,
                );
                if let Some(n) = self.locate(id).filter(|n| *n != SCRATCHPAD) {
                    self.ws_mut(n).urgent = true;
                    self.dirty = true;
                } else if let Some((si, n)) = self.locate_hidden(id)
                    && let Some(ws) = self.sessions[si].workspaces.get_mut(&n)
                {
                    ws.urgent = true;
                }
            }
            // A program copying to the clipboard (OSC 52, e.g. nvim's "+y over
            // SSH): passed on to the host terminal, which owns the clipboard.
            // Reading the clipboard back is refused, alacritty_terminal's default.
            TermEvent::ClipboardStore(_, text) => self.set_host_clipboard(&text),
            TermEvent::ChildExit(_) | TermEvent::Exit => self.close_pane(id),
            // Clipboard (OSC 52) and colour queries: milestone 3.
            _ => {}
        }
    }

    fn handle_input(&mut self, ev: Event) {
        // An open picker takes the keyboard and the mouse, whatever the mode.
        if self.picker.is_some() {
            match ev {
                Event::Key(key) if key.kind != KeyEventKind::Release => self.picker_key(&key),
                Event::Paste(text) => self.picker_paste(&text),
                Event::Mouse(m) => self.picker_mouse(m),
                Event::Resize(w, h) => {
                    self.screen = Rect::new(0, 0, w, h);
                    self.relayout();
                }
                _ => {}
            }
            return;
        }
        if self.mode == Mode::Copy {
            match ev {
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    return self.copy_key(&key);
                }
                Event::Mouse(m) => match m.kind {
                    MouseEventKind::ScrollUp => return self.copy_scroll(3),
                    MouseEventKind::ScrollDown => return self.copy_scroll(-3),
                    // A click anywhere leaves copy mode and then acts as usual.
                    MouseEventKind::Down(_) => self.exit_copy_mode(),
                    _ => return,
                },
                Event::Paste(_) => return,
                _ => {}
            }
        }
        match ev {
            Event::Key(key) => self.handle_key(key),
            Event::Paste(text) => {
                if let Some(p) = self.focused_pane() {
                    p.scroll_to_bottom();
                    p.write(input::encode_paste(&text, p.modes()));
                }
            }
            Event::FocusGained | Event::FocusLost => {
                if let Some(p) = self.focused_pane()
                    && let Some(b) = input::encode_focus(ev == Event::FocusGained, p.modes())
                {
                    p.write(b);
                }
            }
            Event::Resize(w, h) => {
                self.screen = Rect::new(0, 0, w, h);
                self.relayout();
            }
            Event::Mouse(m) => self.handle_mouse(m),
        }
    }

    /// The pane under a point, topmost first (the scratchpad and floats are drawn
    /// last), among the panes of the active workspace.
    fn pane_at(&self, frame: &Frame, x: u16, y: u16) -> Option<PaneView> {
        frame
            .views
            .iter()
            .rev()
            .find(|v| v.outer.contains(x, y) && self.active().contains(v.id))
            .copied()
    }

    /// Clicks on the bar and on tab bars work in any mode: they are ranma's own.
    /// Returns true when the click was one of those.
    fn click_chrome(&mut self, frame: &Frame, x: u16, y: u16) -> bool {
        if let Some(bar) = self.bar_rect()
            && bar.contains(x, y)
        {
            let target = self
                .bar_pieces(bar.w)
                .into_iter()
                .find(|(px, p)| {
                    x - bar.x >= *px && ((x - bar.x - px) as usize) < p.text.chars().count()
                })
                .and_then(|(_, p)| p.click);
            match target {
                Some(Click::Workspace(SCRATCHPAD)) => self.run_action(Action::ScratchpadToggle),
                Some(Click::Workspace(n)) => {
                    self.run_action(Action::Workspace(WorkspaceTarget::Index(n)))
                }
                Some(Click::SessionSwitcher) => self.open_session_switcher(),
                Some(Click::Update) => self.run_action(Action::Update),
                None => {}
            }
            return true;
        }
        if let Some(tb) = frame.tab_bars.iter().find(|t| t.rect.contains(x, y)) {
            let i = ((x - tb.rect.x) as usize * tb.tabs.len()) / tb.rect.w.max(1) as usize;
            if let Some(p) = tb.tabs.get(i) {
                self.focus(*p);
                self.relayout();
            }
            return true;
        }
        false
    }

    fn handle_mouse(&mut self, m: MouseEvent) {
        // A click on a toast dismisses it, in any mode; it never reaches a pane.
        if let MouseEventKind::Down(_) = m.kind
            && let Some(id) = self
                .toast_layout()
                .iter()
                .find(|(_, r, _)| r.contains(m.column, m.row))
                .map(|(t, _, _)| t.id)
        {
            self.toasts.dismiss(id);
            self.dirty = true;
            return;
        }
        // Borders are ranma's in every mode: the top one moves, the others resize.
        let (x, y) = (m.column, m.row);
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) if self.drag.is_none() => {
                let frame = self.frame();
                if let Some(hit) = self.border_hit(&frame, x, y) {
                    self.start_border_drag(hit, x, y);
                    return;
                }
            }
            MouseEventKind::Drag(_) if self.drag.is_some() => {
                self.update_drag(x, y);
                return;
            }
            MouseEventKind::Up(_) if self.drag.is_some() => {
                self.finish_drag();
                return;
            }
            _ => {}
        }
        match self.mode {
            Mode::Wm => self.handle_mouse_wm(m),
            // Copy mode handled its own mouse events before getting here.
            Mode::Normal | Mode::Copy => self.handle_mouse_normal(m),
        }
    }

    /// Outside WM mode: focus by click (or hover), and everything else goes to the
    /// program under the pointer, the way it would in a terminal of its own.
    fn handle_mouse_normal(&mut self, m: MouseEvent) {
        let (x, y) = (m.column, m.row);
        let frame = self.frame();
        if let MouseEventKind::Down(_) = m.kind
            && self.click_chrome(&frame, x, y)
        {
            return;
        }
        let under = self.pane_at(&frame, x, y);
        // A left press in the text of a pane whose program does not use the
        // mouse starts a selection; everything below is for the other cases.
        if let MouseEventKind::Down(MouseButton::Left) = m.kind {
            self.clear_selection();
            if let Some(v) = under
                && v.inner.contains(x, y)
                && self
                    .panes
                    .get(&v.id)
                    .is_some_and(|p| !p.modes().wants_mouse())
            {
                if Some(v.id) != self.focused() {
                    self.focus(v.id);
                    self.relayout();
                }
                self.start_selection(v, x, y);
                return;
            }
        }
        match m.kind {
            MouseEventKind::Down(_) => {
                if let Some(v) = under
                    && Some(v.id) != self.focused()
                {
                    self.focus(v.id);
                    self.relayout();
                }
            }
            MouseEventKind::Moved if self.config.settings.mouse == config::MouseMode::Hover => {
                if let Some(v) = under
                    && Some(v.id) != self.focused()
                {
                    self.focus(v.id);
                    self.relayout();
                }
            }
            _ => {}
        }

        let scroll = match m.kind {
            MouseEventKind::ScrollUp => Some(3i32),
            MouseEventKind::ScrollDown => Some(-3),
            _ => None,
        };
        // The wheel goes to the pane under the pointer; everything else to the
        // focused pane, which is where a drag that started in it belongs even when
        // the pointer leaves it.
        let target = if scroll.is_some() {
            under
        } else {
            let f = self.focused();
            frame.views.iter().find(|v| Some(v.id) == f).copied()
        };
        let Some(v) = target else {
            return;
        };
        let Some(p) = self.panes.get(&v.id) else {
            return;
        };
        let modes = p.modes();
        if modes.wants_mouse() {
            let col = x.clamp(v.inner.x, v.inner.right().saturating_sub(1)) - v.inner.x;
            let row = y.clamp(v.inner.y, v.inner.bottom().saturating_sub(1)) - v.inner.y;
            if let Some(bytes) = input::encode_mouse(m.kind, m.modifiers, col, row, modes) {
                p.write(bytes);
            }
        } else if let Some(lines) = scroll {
            if modes.alt_screen && modes.alternate_scroll {
                // A full-screen program without mouse support (less, man): the
                // wheel becomes arrow keys, as xterm does it.
                let key = KeyEvent::new(
                    if lines > 0 {
                        crossterm::event::KeyCode::Up
                    } else {
                        crossterm::event::KeyCode::Down
                    },
                    crossterm::event::KeyModifiers::NONE,
                );
                if let Some(bytes) = input::encode_key(&key, modes) {
                    p.write(bytes.repeat(lines.unsigned_abs() as usize));
                }
            } else if !modes.alt_screen {
                p.scroll(lines);
                self.dirty = true;
            }
        }
    }

    /// In WM mode: click to focus, drag a floating pane with the left button,
    /// resize it with the right.
    fn handle_mouse_wm(&mut self, m: MouseEvent) {
        let (x, y) = (m.column, m.row);
        // Drags and releases were handled with the borders, in handle_mouse.
        let MouseEventKind::Down(button) = m.kind else {
            return;
        };
        let frame = self.frame();
        if self.click_chrome(&frame, x, y) {
            return;
        }
        let Some(v) = self.pane_at(&frame, x, y) else {
            return;
        };
        self.focus(v.id);
        // In WM mode a float can be grabbed anywhere, not only by its border.
        if v.floating {
            self.drag = match button {
                MouseButton::Left => Some(Drag::Move {
                    id: v.id,
                    dx: x - v.outer.x,
                    dy: y - v.outer.y,
                }),
                MouseButton::Right => Some(Drag::Resize {
                    id: v.id,
                    start: v.outer,
                    x,
                    y,
                }),
                MouseButton::Middle => None,
            };
        }
        self.relayout();
    }

    /// Whether the host terminal should report the mouse: always, unless the
    /// config gives it to the host, in which case only in WM mode.
    pub fn wants_mouse(&self) -> bool {
        self.mode != Mode::Normal
            || self.picker.is_some()
            || self.config.settings.mouse != config::MouseMode::Off
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        let chord = input::chord_of(&key);
        let leader = self.config.settings.leader;

        let outer = self.config.settings.outer_leader;
        if self.mode == Mode::Normal {
            // The outer leader reaches this ranma, even past one inside, unless
            // the one inside is engaged (in WM mode, or deeper): then it is on
            // its way further down, and goes on.
            if chord == Some(outer) {
                if let Some(p) = self.focused_pane()
                    && self.passes_through()
                    && p.inner_engaged()
                {
                    if let Some(bytes) = input::encode_key(&key, p.modes()) {
                        p.write(bytes);
                    }
                    return;
                }
                self.set_mode(Mode::Wm);
                self.status = None;
                return;
            }
            // A ranma in the focused pane gets every other key, the leader and
            // global binds included: keys act on the innermost ranma.
            if self.passes_through() {
                if let Some(p) = self.focused_pane()
                    && let Some(bytes) = input::encode_key(&key, p.modes())
                {
                    p.write(bytes);
                }
                return;
            }
            if chord == Some(leader) {
                self.set_mode(Mode::Wm);
                self.status = None;
                return;
            }
            if let Some(c) = chord
                && self.config.global_binds.contains_key(&c)
            {
                self.run_bind(c, true);
                return;
            }
            // Typing ends a mouse selection, as in any terminal.
            if self.selection_pane.is_some() {
                self.clear_selection();
            }
            if let Some(p) = self.focused_pane()
                && let Some(bytes) = input::encode_key(&key, p.modes())
            {
                p.scroll_to_bottom();
                p.write(bytes);
            }
            return;
        }

        self.status = None;
        self.dirty = true;
        let Some(chord) = chord else {
            return;
        };
        // The outer leader again, unbound: one level down, to the ranma inside.
        if chord == outer && !self.config.binds.contains_key(&chord) {
            if let Some(p) = self.focused_pane()
                && let Some(bytes) = chord_bytes(outer, p.modes())
            {
                p.write(bytes);
            }
            self.set_mode(Mode::Normal);
            return;
        }
        if !self.config.binds.contains_key(&chord) && chord != leader {
            // Unbound keys are swallowed: WM mode is a mode, and typing into a pane
            // by accident while in it is worse than a dead key.
            self.status = Some(format!("{chord} is not bound"));
            return;
        }
        let exits = if chord == leader && !self.config.binds.contains_key(&chord) {
            // The leader pressed again goes through to the program, tmux style.
            self.run_action(Action::SendLeader);
            true
        } else {
            self.run_bind(chord, false)
        };
        // Only from WM mode: the bind may have entered copy mode or opened a picker.
        if (exits || !self.config.settings.wm_mode_sticky) && self.mode == Mode::Wm {
            self.set_mode(Mode::Normal);
        }
    }

    /// Run the bind for `chord` from the WM or the global table. Returns whether
    /// the bind ends WM mode.
    fn run_bind(&mut self, chord: crate::keys::Chord, global: bool) -> bool {
        let table = if global {
            &self.config.global_binds
        } else {
            &self.config.binds
        };
        let Some(bind) = table.get(&chord) else {
            return false;
        };
        let exits = bind.exits_mode;
        match &bind.action {
            BindAction::Builtin(a) => {
                let a = a.clone();
                self.run_action(a);
            }
            BindAction::Lua(key) => match self.config.lua.registry_value::<Function>(key) {
                Ok(f) => {
                    self.call_lua(|_| f.call::<()>(()));
                }
                Err(_) => self.status = Some("lua: bind function is gone".into()),
            },
        }
        exits
    }

    fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.drag = None;
        self.dirty = true;
    }

    fn run_action(&mut self, action: Action) {
        let focused = self.focused();
        let floating = focused.is_some_and(|f| self.active().is_floating(f));
        let area = self.workspace_area();
        match action {
            Action::NewPane => {
                if let Err(e) = self.open_pane(None) {
                    self.status = Some(format!("new pane failed: {e:#}"));
                }
            }
            Action::NewPaneAt(dir) => {
                if let Err(e) = self.open_pane_at(None, Some(dir), None) {
                    self.status = Some(format!("new pane failed: {e:#}"));
                }
            }
            Action::Exec(cmd) => {
                if let Err(e) = self.open_pane(Some(&cmd)) {
                    self.status = Some(format!("exec failed: {e:#}"));
                }
            }
            Action::ClosePane => {
                if let Some(id) = focused {
                    self.close_pane(id);
                }
            }
            Action::Focus(dir) => {
                if let Some(id) = focused {
                    let rects = self.focus_rects();
                    let next = layout::neighbour(&rects, id, dir)
                        .or_else(|| nearest_by_centre(&rects, id, dir));
                    if let Some(n) = next {
                        self.active_mut().fullscreen = false;
                        self.focus(n);
                        self.relayout();
                    }
                }
            }
            Action::Move(dir) if floating => {
                let id = focused.unwrap();
                let (dx, dy): (i32, i32) = match dir {
                    Dir::Left => (-4, 0),
                    Dir::Right => (4, 0),
                    Dir::Up => (0, -2),
                    Dir::Down => (0, 2),
                };
                if let Some(r) = self.active_mut().float_rect_mut(id) {
                    *r = Rect::new(
                        (r.x as i32 + dx).max(0) as u16,
                        (r.y as i32 + dy).max(0) as u16,
                        r.w,
                        r.h,
                    )
                    .clamp_into(area);
                }
                self.relayout();
            }
            Action::Move(dir) => {
                if let Some(id) = focused
                    && let Some(n) = layout::neighbour(&self.focus_rects(), id, dir)
                    && self.active_mut().tree.swap(id, n)
                {
                    self.relayout();
                }
            }
            Action::Resize(dir, cells) if floating => {
                let id = focused.unwrap();
                if let Some(r) = self.active_mut().float_rect_mut(id) {
                    let (w, h) = match dir {
                        Dir::Right => (r.w.saturating_add(cells), r.h),
                        Dir::Left => (r.w.saturating_sub(cells).max(10), r.h),
                        Dir::Down => (r.w, r.h.saturating_add(cells)),
                        Dir::Up => (r.w, r.h.saturating_sub(cells).max(3)),
                    };
                    *r = Rect::new(r.x, r.y, w, h).clamp_into(area);
                }
                self.relayout();
            }
            Action::Resize(dir, cells) => {
                if let Some(id) = focused {
                    let area = if self.scratch_shown {
                        self.scratch_area()
                    } else {
                        area
                    };
                    let gap = self.config.theme.gaps.inner;
                    if self.active_mut().tree.resize(id, dir, cells, area, gap) {
                        self.relayout();
                    }
                }
            }
            Action::ToggleSplit => {
                if let Some(id) = focused
                    && self.active_mut().tree.toggle_split(id)
                {
                    self.relayout();
                }
            }
            Action::ToggleFloating => self.toggle_floating(),
            Action::Detach => self.detach_requested = true,
            Action::Update => {
                // In a float, so the pull and the build can be watched, and the
                // pane stays until a key is pressed so the result can be read.
                let cmd = format!(
                    "{}; printf '\\npress a key to close'; read -rsn1 _",
                    crate::update::install_command()
                );
                match self.open_pane_at(Some(&cmd), None, None) {
                    Ok(id) => {
                        if !self.scratch_shown {
                            self.toggle_floating_pane(id);
                        }
                        self.rename_pane(id, "ranma update");
                        self.update_available = None;
                    }
                    Err(e) => self.status = Some(format!("update failed: {e:#}")),
                }
            }
            Action::CycleFloats => {
                if self.active_mut().cycle_floats().is_some() {
                    self.relayout();
                } else {
                    self.status = Some("no floating panes here".into());
                }
            }
            Action::ToggleGroup => {
                if let Some(id) = focused {
                    if floating {
                        self.status = Some("floating panes cannot be grouped".into());
                    } else if self.active_mut().tree.toggle_group(id) {
                        self.relayout();
                    }
                }
            }
            Action::GroupNext | Action::GroupPrev => {
                let forward = action == Action::GroupNext;
                if let Some(id) = focused
                    && let Some(next) = self.active_mut().tree.cycle_group(id, forward)
                {
                    self.focus(next);
                    self.relayout();
                }
            }
            Action::Fullscreen => {
                if focused.is_some() && !self.scratch_shown {
                    let ws = self.active_mut();
                    ws.fullscreen = !ws.fullscreen;
                    self.relayout();
                }
            }
            Action::Workspace(t) => {
                let n = self.resolve(&t);
                self.switch_workspace(n);
                self.relayout();
            }
            Action::MoveToWorkspace(t) => {
                let n = self.resolve(&t);
                self.move_focused_to(n, true);
            }
            Action::MoveToWorkspaceSilent(t) => {
                let n = self.resolve(&t);
                self.move_focused_to(n, false);
            }
            Action::ScratchpadToggle => {
                if self.scratch_shown {
                    self.scratch_shown = false;
                } else if self.scratch.is_empty() {
                    // An empty scratchpad summons a fresh shell: the point of the
                    // key is a quick terminal, not a message saying there is none.
                    self.scratch_shown = true;
                    if let Err(e) = self.open_pane(None) {
                        self.scratch_shown = false;
                        self.status = Some(format!("scratchpad failed: {e:#}"));
                    }
                } else {
                    self.scratch_shown = true;
                }
                self.relayout();
            }
            Action::MoveToScratchpad => {
                if self.scratch_shown {
                    self.status = Some("already in the scratchpad".into());
                } else {
                    self.move_focused_to(SCRATCHPAD, false);
                }
            }
            Action::ExitMode => {}
            Action::SendLeader => {
                let leader = self.config.settings.leader;
                if let Some(p) = self.focused_pane()
                    && let Some(bytes) = chord_bytes(leader, p.modes())
                {
                    p.write(bytes);
                }
            }
            Action::ReloadConfig => self.reload_config(),
            Action::Quit { now: true } => self.quit = true,
            Action::Quit { now: false } => self.confirm_quit(),
            Action::PaneSwitcher => self.open_pane_switcher(),
            Action::SessionSwitcher => self.open_session_switcher(),
            Action::Help => self.open_palette(crate::picker::PaletteMode::Help),
            Action::CommandPalette => self.open_palette(crate::picker::PaletteMode::Command),
            Action::CopyMode => self.enter_copy_mode(None),
            // Backward: the most recent match first, which is what searching
            // history usually wants.
            Action::Search => self.enter_copy_mode(Some(true)),
            Action::NewSession(name) => self.new_session(name.as_deref()),
            Action::Session(t) => match self.resolve_session(&t) {
                Some(i) => self.switch_session(i),
                None => self.status = Some(format!("no session `{t}`")),
            },
            Action::MoveWorkspaceToSession(None) => self.open_move_workspace(),
            // A name no session has makes one, as `ranma open --session` does.
            Action::MoveWorkspaceToSession(Some(t)) => match self.resolve_session(&t) {
                Some(i) => self.move_workspace_to_session(Some(i), None),
                None => self.move_workspace_to_session(None, Some(&t.to_string())),
            },
            Action::RenameSession(Some(name)) => self.rename_session(self.active_session, &name),
            Action::RenameSession(None) => self.open_rename_prompt(self.active_session),
            Action::RenameWorkspace(Some(name)) => self.rename_workspace(self.current, &name),
            Action::RenameWorkspace(None) => self.open_rename_workspace(),
            Action::RenamePane(Some(name)) => {
                if let Some(id) = self.focused() {
                    self.rename_pane(id, &name);
                }
            }
            Action::RenamePane(None) => self.open_rename_pane(),
        }
    }

    fn reload_config(&mut self) {
        self.reload_at = None;
        // A reload can come from a timer, with no input event to trigger a frame;
        // either outcome puts a message in the bar that must be drawn now.
        self.dirty = true;
        match config::load(config::config_dir().as_deref()) {
            Ok(cfg) => {
                self.config = cfg;
                self.module_generation += 1;
                self.module_values.clear();
                self.module_running.clear();
                self.schedule_modules(Instant::now());
                self.status = Some("config reloaded".into());
                self.relayout();
                self.render_state_modules();
                self.emit(HookEvent::ConfigReload, |_| Ok(()));
            }
            // Keep running on the old config; a typo must never take the session down.
            Err(e) => {
                let msg = format!("{e:#}");
                self.status = Some(format!(
                    "config error, kept the old one: {}",
                    msg.lines().next().unwrap_or("")
                ));
            }
        }
    }

    // ---- Lua ---------------------------------------------------------------------

    fn snapshot(&self) -> config::StateSnapshot {
        config::StateSnapshot {
            session: self.session_name().to_string(),
            sessions: self.sessions.iter().map(|s| s.name.clone()).collect(),
            workspace: if self.scratch_shown {
                SCRATCHPAD
            } else {
                self.current
            },
            workspaces: self
                .workspaces
                .iter()
                .filter(|(_, w)| !w.is_empty())
                .map(|(n, _)| *n)
                .collect(),
            focused: self.focused(),
            title: self.focused_title().unwrap_or("").to_string(),
            mode: match self.mode {
                Mode::Wm => "wm",
                Mode::Normal => "normal",
                Mode::Copy => "copy",
            },
            panes: self.panes.len(),
        }
    }

    /// Run Lua with the runtime API live, then apply what it asked for.
    fn call_lua<R>(&mut self, f: impl FnOnce(&Lua) -> mlua::Result<R>) -> Option<R> {
        let rt = config::Runtime {
            state: self.snapshot(),
            ..Default::default()
        };
        self.config.lua.set_app_data(rt);
        let result = f(&self.config.lua);
        let rt = self
            .config
            .lua
            .remove_app_data::<config::Runtime>()
            .unwrap_or_default();
        if let Some(msg) = rt.notify {
            self.status = Some(msg);
        }
        for (text, urgent, timeout) in rt.toasts {
            let level = if urgent {
                crate::toast::Level::Urgent
            } else {
                crate::toast::Level::Normal
            };
            self.toast(text, level, timeout.map(Duration::from_secs_f64));
        }
        let out = match result {
            Ok(v) => Some(v),
            Err(e) => {
                let msg = e.to_string();
                self.status = Some(format!("lua: {}", msg.lines().next().unwrap_or("")));
                None
            }
        };
        if !rt.actions.is_empty() {
            if self.lua_depth >= MAX_LUA_DEPTH {
                self.status = Some("lua: actions nested too deep; stopped".into());
            } else {
                self.lua_depth += 1;
                for a in rt.actions {
                    self.run_action(a);
                }
                self.lua_depth -= 1;
            }
        }
        self.dirty = true;
        out
    }

    /// Call every hook for `event` with a payload table filled in by `fill`.
    fn emit(&mut self, event: HookEvent, fill: impl FnOnce(&Table) -> mlua::Result<()>) {
        let Some(keys) = self.config.hooks.get(&event) else {
            return;
        };
        let funcs: Vec<Function> = keys
            .iter()
            .filter_map(|k| self.config.lua.registry_value(k).ok())
            .collect();
        if funcs.is_empty() {
            return;
        }
        let payload = match self.config.lua.create_table() {
            Ok(t) => t,
            Err(_) => return,
        };
        if fill(&payload).is_err() {
            return;
        }
        for f in funcs {
            let p = payload.clone();
            self.call_lua(|_| f.call::<()>(p));
        }
    }

    // ---- bar modules ---------------------------------------------------------------

    fn schedule_modules(&mut self, now: Instant) {
        self.module_due = self
            .config
            .modules
            .iter()
            .filter(|(_, m)| m.interval.is_some())
            .map(|(name, _)| (name.clone(), now))
            .collect();
    }

    /// The next moment something needs doing without an event arriving.
    fn next_deadline(&self) -> Option<Instant> {
        self.module_due
            .values()
            .copied()
            .chain(self.reload_at)
            .chain(self.toasts.next_expiry())
            .min()
    }

    pub fn toast(
        &mut self,
        text: impl Into<String>,
        level: crate::toast::Level,
        timeout: Option<Duration>,
    ) {
        self.toasts.push(
            text,
            level,
            timeout.unwrap_or(crate::toast::DEFAULT_TIMEOUT),
            Instant::now(),
        );
        self.dirty = true;
    }

    /// Whether keys go straight to a ranma running in the focused pane.
    pub fn passes_through(&self) -> bool {
        self.config.settings.nested == config::NestedMode::Auto
            && self.focused_pane().is_some_and(|p| p.hosts_ranma())
    }

    /// This ranma is in WM mode, or passes keys to an engaged ranma inside it:
    /// either way the outer leader should come down to here.
    fn engaged(&self) -> bool {
        self.mode == Mode::Wm
            || (self.passes_through() && self.focused_pane().is_some_and(|p| p.inner_engaged()))
    }

    /// Tell the terminal ranma runs in that it is ranma, through its title,
    /// followed by the focused pane's: a ranma around this one finds the marker
    /// and passes keys down; a plain terminal shows a useful window title.
    fn announce(&mut self) {
        if self.config.settings.nested == config::NestedMode::Off {
            return;
        }
        let label: String = self
            .focused_title()
            .unwrap_or("")
            .chars()
            .filter(|c| !c.is_control())
            .collect();
        let marker = if self.engaged() {
            crate::pane::NESTED_MARKER_ENGAGED
        } else {
            crate::pane::NESTED_MARKER
        };
        let title = if label.is_empty() {
            marker.to_string()
        } else {
            format!("{marker} · {label}")
        };
        if title != self.host_title {
            self.host_out
                .push(format!("\x1b]2;{title}\x07").into_bytes());
            self.host_title = title;
        }
    }

    /// The checker found commits this binary lacks: remind, ask, or nothing,
    /// per the `updates` setting (read now, so a reload since startup counts).
    fn update_found(&mut self, b: crate::update::Behind) {
        use crate::config::UpdateMode;
        let n = b.commits();
        let what = format!("{n} new commit{}", if n == 1 { "" } else { "s" });
        match self.config.settings.updates {
            UpdateMode::Off => return,
            // Asking takes the keyboard; if something else already has it
            // (a picker), remind instead of interrupting that.
            UpdateMode::Prompt if self.picker.is_none() => {
                self.picker = Some(crate::picker::Picker::question(
                    crate::picker::Kind::ConfirmUpdate,
                    "ranma update",
                    format!("{what}. Update now?   y or Enter updates · any other key cancels"),
                ));
            }
            _ => self.toast(
                format!("ranma: {what} · leader U updates"),
                crate::toast::Level::Normal,
                Some(Duration::from_secs(20)),
            ),
        }
        self.update_available = Some(b);
        self.dirty = true;
    }

    /// Where toasts go: down the right edge, below a top bar.
    pub fn toast_layout(&self) -> Vec<(&crate::toast::Toast, Rect, Vec<String>)> {
        let top = match self.config.theme.bar.position {
            BarPosition::Top => self.screen.y + 1,
            _ => self.screen.y,
        };
        self.toasts.layout(self.screen, top)
    }

    fn run_timers(&mut self, now: Instant) {
        if self.toasts.expire(now) {
            self.dirty = true;
        }
        if self.reload_at.is_some_and(|t| t <= now) {
            self.reload_config();
        }
        let due: Vec<String> = self
            .module_due
            .iter()
            .filter(|(_, t)| **t <= now)
            .map(|(n, _)| n.clone())
            .collect();
        for name in due {
            let Some(iv) = self.config.modules.get(&name).and_then(|m| m.interval) else {
                continue;
            };
            self.module_due.insert(name.clone(), bar::next_due(now, iv));
            self.run_module(&name);
        }
    }

    fn run_module(&mut self, name: &str) {
        let Some(m) = self.config.modules.get(name) else {
            return;
        };
        match &m.kind {
            ModuleKind::Exec { command, .. } => {
                // One run at a time: a slow command skips ticks instead of piling up.
                if self.module_running.insert(name.to_string()) {
                    bar::spawn_exec(
                        name.to_string(),
                        self.module_generation,
                        command.clone(),
                        self.tx.clone(),
                    );
                }
            }
            ModuleKind::Lua(key) => {
                let Ok(f) = self.config.lua.registry_value::<Function>(key) else {
                    return;
                };
                let seg = match self.call_lua(|_| f.call::<Value>(())) {
                    Some(v) => lua_segment(&v),
                    None => vec![Piece::new(format!("{name}: error"), Style::Urgent)],
                };
                self.set_module_value(name.to_string(), seg);
            }
        }
    }

    /// Lua modules without an interval re-render when observed state changes.
    fn render_state_modules(&mut self) {
        let names: Vec<String> = self
            .config
            .modules
            .iter()
            .filter(|(_, m)| m.interval.is_none() && matches!(m.kind, ModuleKind::Lua(_)))
            .map(|(n, _)| n.clone())
            .collect();
        for n in names {
            self.run_module(&n);
        }
    }

    fn set_module_value(&mut self, name: String, seg: Segment) {
        if self.module_values.get(&name) != Some(&seg) {
            self.module_values.insert(name, seg);
            self.dirty = true;
        }
    }

    /// One module's current output: built-ins from state, others from the cache.
    fn segment(&self, name: &str) -> Segment {
        match name {
            "mode" => match self.mode {
                Mode::Wm => vec![Piece::new(" WM ", Style::Mode)],
                Mode::Copy => {
                    let searching = self
                        .copy
                        .as_ref()
                        .and_then(|c| c.search.as_ref())
                        .is_some_and(|s| s.editing);
                    let label = if searching { " SEARCH " } else { " COPY " };
                    vec![Piece::new(label, Style::Mode)]
                }
                // Keys are going to a ranma inside the focused pane.
                Mode::Normal if self.passes_through() => vec![Piece::new(" ⧉ ", Style::Dim)],
                Mode::Normal => Vec::new(),
            },
            // Only once there is more than one: a lone "main" says nothing.
            "session" if self.session_count() > 1 => {
                vec![
                    Piece::new(self.session_name(), Style::Accent).on_click(Click::SessionSwitcher),
                ]
            }
            "session" => Vec::new(),
            "workspaces" => {
                let mut seg: Segment = self
                    .workspace_list()
                    .into_iter()
                    .map(|(n, current, occupied, urgent, name)| {
                        let style = if current && !self.scratch_shown {
                            Style::WsActive
                        } else if urgent {
                            Style::WsUrgent
                        } else if occupied {
                            Style::WsOccupied
                        } else {
                            Style::WsEmpty
                        };
                        // The number always shows: it is the key that gets you there.
                        let label = match name {
                            Some(name) => format!(" {n}:{name} "),
                            None => format!(" {n} "),
                        };
                        Piece::new(label, style).on_click(Click::Workspace(n))
                    })
                    .collect();
                let (has, shown) = self.scratch_state();
                if has {
                    let style = if shown {
                        Style::WsActive
                    } else {
                        Style::WsOccupied
                    };
                    seg.push(Piece::new(" S ", style).on_click(Click::Workspace(SCRATCHPAD)));
                }
                seg
            }
            "title" => self
                .focused_title()
                .filter(|t| !t.is_empty())
                .map(|t| vec![Piece::new(t, Style::Normal)])
                .unwrap_or_default(),
            "panes" => vec![Piece::new(self.panes.len().to_string(), Style::Dim)],
            "update" => match self.update_available {
                Some(b) => vec![
                    Piece::new(format!("⬆ {}", b.commits()), Style::Accent).on_click(Click::Update),
                ],
                None => Vec::new(),
            },
            _ => self.module_values.get(name).cloned().unwrap_or_default(),
        }
    }

    /// The bar's pieces and where they go, for `cols` columns. A status message
    /// takes the centre while it is up.
    pub fn bar_pieces(&self, cols: u16) -> Vec<(u16, Piece)> {
        let side =
            |names: &[String]| -> Vec<Segment> { names.iter().map(|n| self.segment(n)).collect() };
        let bar = &self.config.bar;
        let center = match &self.status {
            Some(msg) => vec![vec![Piece::new(msg.clone(), Style::Accent)]],
            None => side(&bar.center),
        };
        bar::fit(
            &side(&bar.left),
            &center,
            &side(&bar.right),
            &self.config.theme.bar.separator,
            cols,
        )
    }

    /// Clear wakeup flags of the panes a frame just drew.
    /// Called just before a frame is drawn: re-arm the visible panes' wakeups
    /// and mark the frame clean.
    ///
    /// Before, not after. Output that arrives while the frame is being drawn
    /// may miss it; with the flag already cleared, that output sends a fresh
    /// wakeup and gets the next frame. Cleared after the draw instead, the
    /// flag would swallow that wakeup and the output would sit undrawn until
    /// some unrelated event came along: the "one keypress late" a ranma
    /// nested in another showed, where a frame arrives as several reads.
    fn begin_frame(&mut self) {
        for id in &self.visible {
            if let Some(p) = self.panes.get(id) {
                p.drawn();
            }
        }
        self.dirty = false;
    }
}

/// When nothing lies strictly in a direction (a float over tiles, say), go to the
/// pane whose centre is nearest that way.
fn nearest_by_centre(rects: &[(PaneId, Rect)], from: PaneId, dir: Dir) -> Option<PaneId> {
    let centre = |r: &Rect| (r.x as i32 * 2 + r.w as i32, r.y as i32 * 2 + r.h as i32);
    let (cx, cy) = centre(&rects.iter().find(|(id, _)| *id == from)?.1);
    rects
        .iter()
        .filter(|(id, _)| *id != from)
        .filter_map(|(id, r)| {
            let (x, y) = centre(r);
            let (along, across) = match dir {
                Dir::Left => (cx - x, (y - cy).abs()),
                Dir::Right => (x - cx, (y - cy).abs()),
                Dir::Up => (cy - y, (x - cx).abs()),
                Dir::Down => (y - cy, (x - cx).abs()),
            };
            (along > 0).then_some((*id, along + across * 2))
        })
        .min_by_key(|(_, d)| *d)
        .map(|(id, _)| id)
}

/// What a Lua module returned, as bar pieces.
fn lua_segment(v: &Value) -> Segment {
    match v {
        Value::Nil => Vec::new(),
        Value::String(s) => vec![Piece::new(s.to_string_lossy(), Style::Normal)],
        Value::Integer(i) => vec![Piece::new(i.to_string(), Style::Normal)],
        Value::Number(n) => vec![Piece::new(n.to_string(), Style::Normal)],
        Value::Table(t) => {
            let text: String = t.get("text").unwrap_or_default();
            let style: Option<String> = t.get("style").ok().flatten();
            match style.as_deref().map(|s| (s, Style::from_name(s))) {
                None => vec![Piece::new(text, Style::Normal)],
                Some((_, Some(st))) => vec![Piece::new(text, st)],
                Some((bad, None)) => {
                    vec![Piece::new(format!("unknown style `{bad}`"), Style::Urgent)]
                }
            }
        }
        other => vec![Piece::new(
            format!("module returned a {}", other.type_name()),
            Style::Urgent,
        )],
    }
}

/// The bytes a chord would have sent had it not been the leader.
fn chord_bytes(chord: crate::keys::Chord, modes: input::PaneModes) -> Option<Vec<u8>> {
    use crate::keys::Key;
    use crossterm::event::{KeyCode, KeyModifiers};
    let code = match chord.key {
        Key::Char(c) => KeyCode::Char(c),
        Key::Space => KeyCode::Char(' '),
        Key::Left => KeyCode::Left,
        Key::Right => KeyCode::Right,
        Key::Up => KeyCode::Up,
        Key::Down => KeyCode::Down,
        Key::Return => KeyCode::Enter,
        Key::Tab => KeyCode::Tab,
        Key::Backspace => KeyCode::Backspace,
        Key::Escape => KeyCode::Esc,
        Key::Delete => KeyCode::Delete,
        Key::Home => KeyCode::Home,
        Key::End => KeyCode::End,
        Key::PageUp => KeyCode::PageUp,
        Key::PageDown => KeyCode::PageDown,
        Key::F(n) => KeyCode::F(n),
    };
    let mut mods = KeyModifiers::NONE;
    mods.set(KeyModifiers::CONTROL, chord.mods.ctrl);
    mods.set(KeyModifiers::ALT, chord.mods.alt);
    mods.set(KeyModifiers::SHIFT, chord.mods.shift);
    input::encode_key(&KeyEvent::new(code, mods), modes)
}

pub(super) fn spawn_input_thread(tx: Sender<AppEvent>) {
    std::thread::Builder::new()
        .name("input".into())
        .spawn(move || {
            loop {
                match crossterm::event::read() {
                    Ok(ev) => {
                        if tx.send(AppEvent::Input(ev)).is_err() {
                            return;
                        }
                    }
                    Err(_) => {
                        let _ = tx.send(AppEvent::InputClosed);
                        return;
                    }
                }
            }
        })
        .expect("spawning the input thread");
}

/// Watch the config directory with inotify: no polling, no cost while idle.
/// Returns the watcher, which stops watching when dropped.
///
/// Dotfiles usually make `init.lua` (or `themes/`) a symlink into a repo, and
/// inotify reports changes to a symlink's *target* only to a watch on the
/// target's own directory. So the directories behind any such links are watched
/// too, or saving through the link would never reload.
pub(super) fn watch_config(tx: Sender<AppEvent>) -> Option<notify::RecommendedWatcher> {
    use notify::{RecursiveMode, Watcher};
    let dir = config::config_dir().filter(|d| d.is_dir())?;
    let mut w = notify::recommended_watcher(move |ev: notify::Result<notify::Event>| {
        if let Ok(ev) = ev
            && !ev.kind.is_access()
            && ev.paths.iter().any(|p| {
                matches!(
                    p.extension().and_then(|e| e.to_str()),
                    Some("lua") | Some("toml")
                )
            })
        {
            let _ = tx.send(AppEvent::ConfigChanged);
        }
    })
    .ok()?;
    w.watch(&dir, RecursiveMode::Recursive).ok()?;
    for (entry, mode) in config_link_targets(&dir) {
        // Best effort: a dangling link simply is not watched.
        let _ = w.watch(&entry, mode);
    }
    Some(w)
}

/// Directories behind symlinks in the config dir that need their own watch:
/// the parent of a linked `init.lua`, and a linked `themes/` itself.
fn config_link_targets(dir: &std::path::Path) -> Vec<(std::path::PathBuf, notify::RecursiveMode)> {
    let mut out = Vec::new();
    let init = dir.join("init.lua");
    if init.is_symlink()
        && let Some(parent) = init
            .canonicalize()
            .ok()
            .and_then(|p| p.parent().map(Into::into))
    {
        out.push((parent, notify::RecursiveMode::NonRecursive));
    }
    let themes = dir.join("themes");
    if themes.is_symlink()
        && let Ok(t) = themes.canonicalize()
    {
        out.push((t, notify::RecursiveMode::Recursive));
    }
    out
}

pub(super) fn cursor_style(c: CursorState) -> SetCursorStyle {
    match (c.shape, c.blinking) {
        (CursorShape::Beam, true) => SetCursorStyle::BlinkingBar,
        (CursorShape::Beam, false) => SetCursorStyle::SteadyBar,
        (CursorShape::Underline, true) => SetCursorStyle::BlinkingUnderScore,
        (CursorShape::Underline, false) => SetCursorStyle::SteadyUnderScore,
        (_, true) => SetCursorStyle::BlinkingBlock,
        (_, false) => SetCursorStyle::SteadyBlock,
    }
}

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
use crate::layout::{self, PaneId, Placement, Preset, Rect, Split, TabBar};
use crate::pane::{AppEvent, Pane, Size, SpawnOptions};
use crate::render::CursorState;
use crate::theme::{BarPosition, BorderStyle};
use crate::workspace::Workspace;

mod copy;
mod drag;
mod hints;
mod layouts;
mod nested;
pub use nested::NestLabel;
mod paste;
mod query;
mod rules;
mod run;
mod session;
mod snapshots;
mod switch;
mod touch;

pub use touch::{ButtonState, SheetLayout};
pub(crate) mod upgrade;

pub use copy::CopyState;
pub use drag::drop_half;
pub use hints::HintState;
pub use run::{run, run_server, run_server_resume};
use session::Session;
pub use switch::PickerLayout;
pub use upgrade::check_handover;

/// The frame cap. Output arriving faster than this is coalesced: the pane is drawn
/// at its latest state once per interval, not once per read.
const FRAME: Duration = Duration::from_micros(8_333);
/// Editors save in several steps (write, rename, chmod); one reload for all of them.
const RELOAD_DEBOUNCE: Duration = Duration::from_millis(150);
/// How often, at most, the workspaces module reads which program each pane runs.
const PROGRAMS_EVERY: Duration = Duration::from_millis(500);
/// How long a bar message stays. It was until the next key in WM mode, so an
/// error said once sat in the bar for as long as you typed into programs.
const STATUS_FOR: Duration = Duration::from_secs(5);
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
        /// The grid cell the press landed on, always part of the selection.
        anchor: alacritty_terminal::index::Point,
    },
}

/// The parts of state hooks and state-driven modules react to.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Observed {
    session: String,
    /// Whether this ranma is engaged (see `engaged`); announced to a ranma around it.
    engaged: bool,
    focus: Option<PaneId>,
    /// A ranma under the shown scratchpad that keeps its focus (see `held_focus`).
    held: Option<PaneId>,
    workspace: u8,
    mode_wm: bool,
    title: String,
    /// The host a ranma in the focused pane names: it goes into our own title.
    inner_host: Option<String>,
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
    /// A one-line message for the bar; cleared by the next key in WM mode, or
    /// after `STATUS_FOR`.
    pub status: Option<String>,
    /// The message `run_timers` last saw and when: a new one starts the
    /// clock, wherever it was set from.
    status_seen: Option<(String, Instant)>,
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
    /// The pane a mouse press went to: its drags and its release follow it there,
    /// and nowhere else gets them (see `handle_mouse_normal`).
    mouse_capture: Option<PaneId>,
    /// The title last given to the host terminal (see `announce`).
    host_title: String,
    /// The detach action ran; the event loop sends the client away.
    detach_requested: bool,
    /// `attach NAME` ran; the event loop passes the client on to that server.
    switch_requested: Option<String>,
    /// The socket of the ranma the attached client runs inside (see
    /// `proto::Hello::inside`): the one server it cannot switch to.
    client_inside: Option<String>,
    /// The terminal showing this ranma reached it over SSH (see `title_host`).
    client_remote: bool,
    /// The terminal driving the screen is a phone or a tablet (see
    /// `proto::Hello::mobile`).
    client_mobile: bool,
    /// Modifiers held for the next key (`latch`), and the toolbar face being
    /// pressed (see `touch`).
    latches: touch::Latches,
    pressed: Option<touch::Pressed>,
    /// The toolbar face whose picker (sheet) is open: tapped again, it closes.
    sheet_from: Option<(String, crate::toolbar::Slot)>,
    /// The program in the foreground of each workspace's focused pane, for the
    /// workspaces module (` 3:nvim `). Read from /proc at most every
    /// `PROGRAMS_EVERY`, and only after something happened, so idle stays idle.
    programs: HashMap<PaneId, String>,
    /// Each shown pane's foreground program and directory, for border titles
    /// whose format names them; read with `programs`, never by a frame.
    pane_facts: HashMap<PaneId, (Option<String>, Option<String>)>,
    programs_read: Option<Instant>,
    programs_due: Option<Instant>,
    /// The cpu module's previous /proc/stat sample: usage is the change since.
    cpu_prev: Option<crate::sysstat::CpuSample>,
    /// `ranma wait` requests, answered when their pane ends (see `query`).
    waiters: HashMap<PaneId, Vec<query::Reply>>,
    /// Exit statuses of children that ended, until their pane is closed.
    exit_codes: HashMap<PaneId, i32>,
    /// The last panes to end and their exit statuses, for a late `wait`.
    ended: std::collections::VecDeque<(PaneId, Option<i32>)>,
    /// Popups: when the key pane closes, the value pane gets focus back.
    return_focus: HashMap<PaneId, PaneId>,
    /// Panes marked for synchronized input (see `typed`).
    synced: HashSet<PaneId>,
    /// Labelled links over the focused pane, waiting for a label (see `hints`).
    hints: Option<HintState>,
    /// Where a right-click menu was opened: it is drawn there.
    menu_at: Option<(u16, u16)>,
    /// When the which-key hint shows, if WM mode is still waiting then.
    hint_due: Option<Instant>,
    /// A paste waiting on its upload (see `paste`).
    pending_paste: Option<paste::PendingPaste>,
    paste_seq: u64,
    /// The pane a key was last passed to a ranma in, and when: a ranma there
    /// asking for `paste_image` is only heard just after (see `paste`).
    passed_key: Option<(PaneId, Instant)>,
    /// The which-key hint is up (see `whichkey`).
    hint_on: bool,
    /// The chord that opened WM mode: the hint's title.
    wm_chord: Option<crate::keys::Chord>,
    /// What the ranma in each pane reported (see `nested`).
    reports: HashMap<PaneId, crate::nestbar::Report>,
    /// A ranma around the attached client answered its question: it shows
    /// this ranma's workspaces (see `nested`).
    pub client_outer: bool,
    /// The protocol the ranma around this one answered in (0 with none).
    outer_v: u32,
    /// The terminal showing this ranma has focus (the last focus event).
    host_focused: bool,
    /// The report last sent outward, so only a change is sent.
    last_report: Option<crate::nestbar::Report>,
    /// The colours the ranma around the attached client sent (see `nested`).
    outer_colors: Option<serde_json::Map<String, serde_json::Value>>,
    /// The theme's own colours while the outer's are drawn instead.
    own_colors: Option<crate::theme::Colors>,
    /// The pane drawn without a border last time (see `frameless`).
    frameless_was: Option<PaneId>,
    /// Where `save_layout` writes and `load_layout` reads (see `layouts`).
    layouts_dir: Option<std::path::PathBuf>,
    /// This server's snapshot of itself (see `snapshots`); `None` standalone.
    snapshots: Option<snapshots::Snapshots>,
    snapshot_due: Option<Instant>,
    /// The snapshot a fresh server set aside, while its question is open.
    restore_offer: Option<crate::restore::Server>,
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
                accent: None,
            }],
            active_session: 0,
            scratch: Workspace::default(),
            scratch_shown: false,
            mode: Mode::Normal,
            status: None,
            status_seen: None,
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
            layouts_dir: crate::layouts::dir(),
            snapshots: None,
            snapshot_due: None,
            restore_offer: None,
            selection_pane: None,
            last_click: None,
            mouse_capture: None,
            host_title: String::new(),
            detach_requested: false,
            switch_requested: None,
            client_inside: None,
            client_remote: false,
            client_mobile: false,
            latches: Default::default(),
            pressed: None,
            sheet_from: None,
            programs: HashMap::new(),
            pane_facts: HashMap::new(),
            programs_read: None,
            programs_due: None,
            cpu_prev: None,
            waiters: HashMap::new(),
            exit_codes: HashMap::new(),
            ended: Default::default(),
            return_focus: HashMap::new(),
            synced: HashSet::new(),
            hints: None,
            menu_at: None,
            hint_due: None,
            pending_paste: None,
            paste_seq: 0,
            passed_key: None,
            hint_on: false,
            wm_chord: None,
            reports: HashMap::new(),
            client_outer: false,
            outer_v: 0,
            host_focused: true,
            last_report: None,
            outer_colors: None,
            own_colors: None,
            frameless_was: None,
        };
        app.schedule_modules(Instant::now());
        app.report_plugin_failures();
        app
    }

    /// A plugin that failed to load was dropped and the rest run without it;
    /// say which, and why, so the drop is never silent. Longer than a usual
    /// toast: it may land while nobody is looking yet.
    fn report_plugin_failures(&mut self) {
        let failed: Vec<(String, String)> = self
            .config
            .plugins
            .iter()
            .filter_map(|p| {
                let name = p.path.file_name()?.to_string_lossy().into_owned();
                Some((name, p.error.as_ref()?.lines().next()?.to_string()))
            })
            .collect();
        for (name, why) in failed {
            self.toast(
                format!("plugin {name} not loaded: {why}"),
                crate::toast::Level::Urgent,
                Some(Duration::from_secs(15)),
            );
        }
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

    /// (number, is current, has panes, urgent, name) for each listed workspace.
    /// The name is the one given (rename_workspace), else the program in the
    /// workspace's focused pane, unless the module says `label = "number"`.
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
                    ws.and_then(|w| w.name.clone()).or_else(|| {
                        if self.config.workspaces_numbers_only {
                            return None;
                        }
                        let id = ws?.focused?;
                        self.programs.get(&id).cloned()
                    }),
                )
            })
            .collect()
    }

    // ---- geometry ----------------------------------------------------------

    /// Where the bar is drawn now, if anywhere. A ranma whose workspaces an
    /// outer ranma shows draws none while its terminal has focus, and draws it
    /// over the bottom row when it has not (see `nested`).
    pub fn bar_rect(&self) -> Option<Rect> {
        if self.bar_yielded() && !self.bar_overlaid() {
            return None;
        }
        self.bar_row()
    }

    /// The bar's rows: where the chrome puts them, or, for a yielded bar drawn
    /// over the panes, the edge row the theme names.
    fn bar_row(&self) -> Option<Rect> {
        if !self.bar_yielded() {
            return self.chrome().bar;
        }
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
        // What the bar, toolbars and monocle strip leave (see `chrome`). A
        // yielded bar keeps no row: focus coming and going must not resize the
        // panes, so the bar is drawn over them when it shows at all.
        let a = self.chrome().workspace;
        a.inset_sides(self.config.theme.gaps.outer())
    }

    /// Where the splash goes, and the keys it names: on a shown workspace
    /// with nothing tiled and nothing focused, the state in which Enter opens
    /// a shell (`enter_opens_pane`). `None` when there is no splash to draw.
    pub fn splash(&self) -> Option<(Rect, Vec<(String, &'static str)>)> {
        if !self.config.settings.splash || self.focused().is_some() {
            return None;
        }
        let mut keys = vec![("Enter".to_string(), "open a shell")];
        if let Some(k) = self.help_key() {
            keys.push((k, "every key"));
        }
        Some((self.workspace_area(), keys))
    }

    /// The key that opens help, as typed: a global bind as it is, a WM bind
    /// after the leader. The shortest wins, then the first in order.
    fn help_key(&self) -> Option<String> {
        let is_help = |b: &config::Bind| {
            matches!(
                b.action,
                config::BindAction::Builtin(crate::action::Action::Help)
            )
        };
        let leader = &self.config.settings.leader;
        self.config
            .global_binds
            .iter()
            .filter(|(_, b)| is_help(b))
            .map(|(c, _)| c.to_string())
            .chain(
                self.config
                    .binds
                    .iter()
                    .filter(|(_, b)| is_help(b))
                    .map(|(c, _)| format!("{leader} {c}")),
            )
            .min_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)))
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
        let frameless = self.frameless();
        let view = |id, outer: Rect, floating| {
            let b = if Some(id) == frameless { 0 } else { b };
            PaneView {
                id,
                outer,
                inner: outer.inset(b, b),
                focused: Some(id) == focused,
                floating,
            }
        };

        if let Some(ws) = self.workspaces.get(&self.current) {
            let full = ws.focused.filter(|f| ws.fullscreen && ws.contains(*f));
            let lay = ws.tree.layout_full(area, gap);
            let monocle = self.config.settings.layout == Layout::Monocle;
            match full {
                None if monocle => {
                    let tiles = ws.tree.panes();
                    let shown = self.lone_tile(ws);
                    // Its own row(s) when the chrome gave it some, else
                    // folded into the bar (see `chrome`).
                    if let Some(rect) = self.strip_row() {
                        let order = ws.panes();
                        f.tab_bars.push(TabBar {
                            rect,
                            active: order
                                .iter()
                                .position(|p| Some(*p) == ws.focused)
                                .unwrap_or(0),
                            tabs: order,
                        });
                    }
                    let body = area;
                    for id in tiles {
                        if Some(id) == shown {
                            f.views.push(view(id, body, false));
                        } else {
                            // Sized as if shown: switching does not resize it.
                            f.hidden.push((id, body.inset(b, b)));
                        }
                    }
                    for (id, r) in &ws.floating {
                        f.views.push(view(*id, r.clamp_into(area), true));
                    }
                }
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
        self.note_snapshot(Instant::now());
        // The master layout is a shape kept by policy: whatever changed the tree
        // (a pane opened, closed or moved in) is put back into it here.
        if self.config.settings.layout == Layout::Master {
            let ratio = self.config.settings.master_ratio;
            for ws in self.workspaces.values_mut() {
                ws.tree.arrange_master(ratio);
            }
        }
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

    /// Rebuild the current workspace's tiles into a preset (DESIGN.md,
    /// "Layouts"). Refused where it could not last: the scratchpad has no
    /// tiles, and `layout = "master"` puts its own shape back.
    fn select_layout(&mut self, p: Preset) {
        if self.scratch_shown {
            self.status = Some("scratchpad panes always float".into());
        } else if self.config.settings.layout == Layout::Master && p != Preset::MainVertical {
            self.status = Some(format!(
                "{}: layout \"master\" keeps its own shape",
                p.name()
            ));
        } else {
            let ratio = self.config.settings.master_ratio;
            let ws = self.active_mut();
            ws.preset = Some(p);
            ws.fullscreen = false;
            ws.tree.apply_preset(p, ratio);
            self.relayout();
        }
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
        self.open_pane_with(command, side, cwd, &[])
    }

    /// `open_pane_at` with variables for the child's environment.
    pub(super) fn open_pane_with(
        &mut self,
        command: Option<&str>,
        side: Option<Dir>,
        cwd: Option<std::path::PathBuf>,
        env: &[(String, String)],
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
            // Placed after the focused pane in tree order, then put into the
            // master shape by relayout: it joins the stack after the focused one.
            Layout::Master => Placement::Dwindle,
            // Placed as dwindle would, so the tiling is there to go back to.
            Layout::Monocle => Placement::Dwindle,
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
            env,
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

    /// Where focus goes when `id` leaves a visible workspace: for a float, the
    /// float now on top, the one the eye lands on in a pile (a geometric
    /// neighbour there is usually buried under it); for a tile, the neighbour
    /// the eye lands on (left, up, right, down); else any pane left there.
    fn successor(&self, ws_num: u8, id: PaneId) -> Option<PaneId> {
        let ws = if ws_num == SCRATCHPAD {
            &self.scratch
        } else {
            self.workspaces.get(&ws_num)?
        };
        if ws.is_floating(id)
            && let Some((top, _)) = ws.floating.iter().rev().find(|(p, _)| *p != id)
        {
            return Some(*top);
        }
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
            // A focused float is never left under another: typing into a pane
            // you cannot see is the one thing focus must not do.
            if let Some(f) = ws.focused {
                ws.raise(f);
            }
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
        self.pane_ended(id);
        // A popup gives focus back to the pane it was opened from, if that is
        // still in the workspace shown, rather than to whichever neighbour.
        if let Some(back) = self.return_focus.remove(&id)
            && self.active().contains(back)
        {
            self.focus(back);
        }
        self.return_focus.retain(|_, to| *to != id);
        self.synced.remove(&id);
        self.reports.remove(&id);
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
    /// Focus the next or previous pane of the active layer, wrapping.
    fn focus_cycle(&mut self, forward: bool) {
        let order = self.active().panes();
        let Some(at) = self
            .focused()
            .and_then(|f| order.iter().position(|p| *p == f))
        else {
            return;
        };
        let n = order.len();
        let next = if forward {
            (at + 1) % n
        } else {
            (at + n - 1) % n
        };
        if next != at {
            self.active_mut().fullscreen = false;
            self.focus(order[next]);
            self.relayout();
        }
    }

    /// A pane's name on a tab or strip chip: its label, `◇` before a float.
    pub fn chip_label(&self, id: PaneId) -> String {
        let title = self
            .panes
            .get(&id)
            .map(|p| p.label())
            .filter(|t| !t.is_empty())
            .unwrap_or("shell");
        if self.active().floating.iter().any(|(f, _)| *f == id) {
            format!("◇ {title}")
        } else {
            title.to_string()
        }
    }

    fn focus(&mut self, id: PaneId) {
        let ws = self.active_mut();
        if !ws.contains(id) {
            return;
        }
        ws.focused = Some(id);
        if ws.tree.contains(id) {
            ws.last_tile = Some(id);
        }
        ws.tree.reveal(id);
        ws.raise(id);
        self.dirty = true;
    }

    /// The ranma in the workspace under the shown scratchpad, which keeps its
    /// focus. To a ranma, focus means "the bar around you shows your
    /// workspaces"; told otherwise, it drew its own bar over its bottom row the
    /// moment the scratchpad opened, and took it away again when it closed.
    /// The scratchpad is a layer over the workspace and leaves it as it was.
    fn held_focus(&self) -> Option<PaneId> {
        if !self.scratch_shown {
            return None;
        }
        let id = self.workspaces.get(&self.current)?.focused?;
        self.reports_from(id).then_some(id)
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

    /// Give the focused pane a new float rect, computed from its current one.
    /// A tile is floated first: asking to snap a pane is asking for it to float.
    fn place_float(&mut self, place: impl FnOnce(Rect) -> Rect) {
        let Some(id) = self.focused() else {
            return;
        };
        if !self.active().is_floating(id) {
            self.toggle_floating_pane(id);
        }
        if let Some(r) = self.active_mut().float_rect_mut(id) {
            *r = place(*r);
        }
        self.relayout();
    }

    // ---- events --------------------------------------------------------------

    pub fn handle(&mut self, ev: AppEvent) {
        match ev {
            AppEvent::Pane(..) => self.note_programs(Instant::now(), false),
            AppEvent::Input(_) => {
                let now = Instant::now();
                self.note_programs(now, true);
                self.note_snapshot(now);
            }
            _ => {}
        }
        match ev {
            AppEvent::Input(ev) => self.handle_input(ev),
            AppEvent::InputClosed => self.quit = true,
            AppEvent::Pane(id, ev) => self.handle_pane_event(id, ev),
            AppEvent::Job { id, event } => self.job_event(id, event),
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
            AppEvent::Query(q, reply) => self.answer(q, reply),
            AppEvent::Mark(id, m) => self.pane_mark(id, m),
            AppEvent::UpdateAvailable(b) => self.update_found(b),
            AppEvent::Servers(list) => self.open_server_switcher(list),
            AppEvent::Pasted { id, result } => self.pasted(id, result),
            // The event loop (run.rs) deals with clients itself.
            AppEvent::Attach { .. }
            | AppEvent::Upgrade { .. }
            | AppEvent::ClientInput(..)
            | AppEvent::ClientGone(_)
            | AppEvent::Status(_) => {}
            AppEvent::ConfigChanged => self.reload_at = Some(Instant::now() + RELOAD_DEBOUNCE),
        }
        self.after_event();
    }

    /// Fire hooks for whatever the event changed, and refresh state-driven modules.
    fn after_event(&mut self) {
        // A nested ranma's report, focus or a pane closing can change which
        // pane is drawn without a border, and so the size of that pane.
        let frameless = self.frameless();
        if frameless != self.frameless_was {
            self.frameless_was = frameless;
            self.relayout();
        }
        // Copy mode belongs to one pane; it ends when that pane is no longer the
        // focused one (switched away by a click, a hook, a workspace change).
        if self
            .copy
            .as_ref()
            .is_some_and(|c| Some(c.pane) != self.focused())
        {
            self.exit_copy_mode();
        }
        if self
            .hints
            .as_ref()
            .is_some_and(|h| Some(h.pane) != self.focused())
        {
            self.exit_hints();
        }
        let now = Observed {
            engaged: self.engaged(),
            session: self.session_name().to_string(),
            focus: self.focused(),
            held: self.held_focus(),
            workspace: self.current,
            mode_wm: self.mode == Mode::Wm,
            title: self.focused_title().unwrap_or("").to_string(),
            inner_host: self
                .focused()
                .and_then(|f| self.panes.get(&f))
                .and_then(|p| p.inner_host())
                .map(str::to_string),
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

        // Programs that asked for focus events get them, as in any terminal.
        // A held pane counts as focused: it hears nothing while the scratchpad
        // comes and goes over it.
        let had = [before.focus, before.held];
        let has = [now.focus, now.held];
        for (id, gained) in had
            .iter()
            .flatten()
            .filter(|id| !has.contains(&Some(**id)))
            .map(|id| (*id, false))
            .chain(
                has.iter()
                    .flatten()
                    .filter(|id| !had.contains(&Some(**id)))
                    .map(|id| (*id, true)),
            )
        {
            if let Some(p) = self.panes.get(&id)
                && let Some(b) = input::encode_focus(gained, p.modes())
            {
                p.write(b);
            }
        }
        if now.focus != before.focus {
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
            TermEvent::ChildExit(status) => {
                if let Some(code) = status.code() {
                    self.exit_codes.insert(id, code);
                }
                self.close_pane(id);
            }
            TermEvent::Exit => self.close_pane(id),
            // Clipboard (OSC 52) and colour queries: milestone 3.
            _ => {}
        }
    }

    /// What a pane's program said past the emulator: a command finished (for
    /// the `command_finished` hook), or a notification (a toast, named after
    /// the pane when it gives no title).
    fn pane_mark(&mut self, id: PaneId, m: crate::osc::Mark) {
        let Some(pane) = self.panes.get(&id) else {
            return;
        };
        match m {
            m @ (crate::osc::Mark::RanmaHello | crate::osc::Mark::RanmaReport(_)) => {
                self.nested_mark(id, m);
            }
            crate::osc::Mark::RanmaPasteImage => self.paste_image_asked(id),
            crate::osc::Mark::CommandFinished { exit, duration } => {
                let workspace = self
                    .locate(id)
                    .or_else(|| self.locate_hidden(id).map(|(_, n)| n));
                let visible = self.visible.contains(&id);
                let title = pane.label().to_string();
                self.emit(HookEvent::CommandFinished, |t| {
                    t.set("pane", id)?;
                    t.set("exit", exit)?;
                    t.set("duration", duration.as_secs_f64())?;
                    t.set("workspace", workspace)?;
                    t.set("visible", visible)?;
                    t.set("title", title)
                });
            }
            crate::osc::Mark::Notify { title, body } => {
                let title = if title.is_empty() {
                    pane.label().to_string()
                } else {
                    title
                };
                let (source, text) = match (title.is_empty(), body.is_empty()) {
                    (true, _) => (None, body),
                    (false, true) => (None, title),
                    (false, false) => (Some(title), body),
                };
                self.toasts.push_from(
                    source,
                    text,
                    crate::toast::Level::Normal,
                    crate::toast::DEFAULT_TIMEOUT,
                    Instant::now(),
                );
                self.dirty = true;
            }
        }
    }

    fn handle_input(&mut self, ev: Event) {
        // Toolbars answer taps in every mode, over an open picker too: the
        // button that opened a sheet closes it.
        if let Event::Mouse(m) = ev
            && self.toolbar_mouse(m)
        {
            return;
        }
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
        // Hints take the keyboard until a label is typed or Esc; any click,
        // or focus moving away, ends them.
        if self.hints.is_some() {
            match ev {
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    return self.hint_key(&key);
                }
                Event::Mouse(m) if matches!(m.kind, MouseEventKind::Down(_)) => self.exit_hints(),
                Event::Paste(_) => return,
                _ => {}
            }
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
        if self.hold_for_paste(&ev) {
            return;
        }
        match ev {
            Event::Key(key) => self.handle_key(key),
            Event::Paste(text) => self.paste(text),
            Event::FocusGained | Event::FocusLost => {
                self.host_focused = ev == Event::FocusGained;
                self.dirty = true;
                // Not to a ranma that reports here: to it, focus means "this
                // bar shows your workspaces", which the window losing focus
                // does not change. Told it anyway, it drew its own bar over
                // its bottom row, under this one's still showing it.
                if let Some(id) = self.focused()
                    && !self.reports_from(id)
                    && let Some(p) = self.panes.get(&id)
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
    /// Returns true when the click was one of those. A right click on a
    /// workspace opens its menu; everywhere else it is a click like any other.
    fn click_chrome(&mut self, frame: &Frame, x: u16, y: u16, right: bool) -> bool {
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
                Some(Click::Workspace(n)) if right => self.open_workspace_menu(n, x, y),
                Some(Click::Workspace(n)) => {
                    self.run_action(Action::Workspace(WorkspaceTarget::Index(n)))
                }
                Some(Click::SessionSwitcher) => self.open_session_switcher(),
                Some(Click::Update) => self.run_action(Action::Update),
                Some(Click::Panes) => self.run_action(Action::PaneSwitcher),
                Some(Click::Pane(id)) => {
                    self.active_mut().fullscreen = false;
                    self.focus(id);
                    self.relayout();
                }
                Some(Click::Nested {
                    holder,
                    depth,
                    path,
                }) => self.click_nested(holder, &path[..depth as usize]),
                Some(Click::InPane { pane, n }) => self.click_in_pane(pane, Some(n)),
                None => {}
            }
            return true;
        }
        if let Some(tb) = frame.tab_bars.iter().find(|t| t.rect.contains(x, y)) {
            if let Some(p) = tb.tab_at(x).and_then(|i| tb.tabs.get(i)) {
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
                // A pane's label sits on its border: a click there goes where
                // the label says, and never starts a resize.
                if let Some((pane, n)) = self.nest_label_at(&frame, x, y) {
                    self.click_in_pane(pane, n);
                    return;
                }
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
        if let MouseEventKind::Down(b) = m.kind
            && self.click_chrome(&frame, x, y, b == MouseButton::Right)
        {
            return;
        }
        let under = self.pane_at(&frame, x, y);
        if let MouseEventKind::Down(_) = m.kind
            && self.click_off_scratchpad(under)
        {
            return;
        }
        // A right press opens the pane's menu: on its border or title bar, or
        // in its text when its program does not use the mouse. A program that
        // asked for the mouse keeps its right clicks.
        if let MouseEventKind::Down(MouseButton::Right) = m.kind
            && let Some(v) = under
            && (!v.inner.contains(x, y)
                || self
                    .panes
                    .get(&v.id)
                    .is_some_and(|p| !p.modes().wants_mouse()))
        {
            self.clear_selection();
            self.open_pane_menu(v.id, x, y);
            return;
        }
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
        // The wheel goes to the pane under the pointer. A press goes to the
        // focused pane when it lands in its text, and captures the mouse: the
        // drags and the release that follow go to that pane even once the pointer
        // leaves it, and to no other. Motion with no button held goes to the
        // focused pane only while the pointer is over it. A press ranma kept (a
        // click in the bar) captures nothing, so its release reaches no program:
        // it used to land on the focused pane, clamped to its edge, and click
        // whatever was drawn on the row nearest the bar.
        let focused = || {
            let f = self.focused();
            frame.views.iter().find(|v| Some(v.id) == f).copied()
        };
        let view = |id| frame.views.iter().find(|v| v.id == id).copied();
        let target = match m.kind {
            MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown
            | MouseEventKind::ScrollLeft
            | MouseEventKind::ScrollRight => under,
            MouseEventKind::Down(_) => {
                let v = focused().filter(|v| v.inner.contains(x, y));
                self.mouse_capture = v.map(|v| v.id);
                v
            }
            MouseEventKind::Drag(_) => self.mouse_capture.and_then(view),
            MouseEventKind::Up(_) => self.mouse_capture.take().and_then(view),
            MouseEventKind::Moved => focused().filter(|v| v.inner.contains(x, y)),
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
        if self.click_chrome(&frame, x, y, button == MouseButton::Right) {
            return;
        }
        let under = self.pane_at(&frame, x, y);
        if self.click_off_scratchpad(under) {
            return;
        }
        let Some(v) = under else {
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

    /// A press on none of the scratchpad's panes hides it, the way a click
    /// outside a dropdown closes it. While it is shown only its panes take
    /// clicks, so the press would otherwise do nothing at all. The press is
    /// spent on hiding: it does not also click whatever was under it.
    fn click_off_scratchpad(&mut self, under: Option<PaneView>) -> bool {
        if !self.scratch_shown || under.is_some() {
            return false;
        }
        self.scratch_shown = false;
        self.relayout();
        true
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
        let key = self.latched_key(key);
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
                self.enter_wm(outer);
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
                self.passed_key = self.focused().map(|id| (id, Instant::now()));
                return;
            }
            if chord == Some(leader) {
                self.enter_wm(leader);
                self.status = None;
                return;
            }
            if let Some(c) = chord
                && self.config.global_binds.contains_key(&c)
            {
                self.run_bind(c, true);
                return;
            }
            if self.enter_opens_pane(&key) {
                self.run_action(Action::NewPane);
                return;
            }
            // Typing ends a mouse selection, as in any terminal.
            if self.selection_pane.is_some() {
                self.clear_selection();
            }
            self.typed(|modes| input::encode_key(&key, modes));
            return;
        }

        self.status = None;
        self.dirty = true;
        // A key in WM mode puts the hint away, and a later pause brings it
        // back, after twice the wait: looking between deliberate presses
        // should not make it flash.
        self.hint_on = false;
        self.hint_due = self
            .config
            .settings
            .wm_mode_hint
            .map(|d| Instant::now() + d * 2);
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

    /// Enter on a workspace with nothing in it opens a shell there: the key
    /// would reach no program, and a terminal is what you came for. The
    /// keypad's Enter is CR, the same key, on terminals that send that (ranma
    /// never asks the host for application keypad mode), and LF on some, tmux
    /// among them, which arrives as Ctrl+J. With Ctrl or Alt otherwise it is
    /// somebody's chord.
    fn enter_opens_pane(&self, key: &KeyEvent) -> bool {
        use crossterm::event::{KeyCode, KeyModifiers};
        let line_feed = key.code == KeyCode::Char('j') && key.modifiers == KeyModifiers::CONTROL;
        let enter = key.code == KeyCode::Enter
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        self.focused().is_none() && (enter || line_feed)
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

    /// Input typed at the keyboard: to the focused pane, or, when it is marked
    /// for synchronized input, to every marked pane in the workspace, each
    /// encoded for the modes its own program asked for.
    fn typed(&self, encode: impl Fn(input::PaneModes) -> Option<Vec<u8>>) {
        for id in self.typing_targets() {
            if let Some(p) = self.panes.get(&id)
                && let Some(bytes) = encode(p.modes())
            {
                p.scroll_to_bottom();
                p.write(bytes);
            }
        }
    }

    /// Where typing goes: the focused pane, and with it every marked pane of
    /// the workspace when it is marked itself. Marks elsewhere stay out of it.
    fn typing_targets(&self) -> Vec<PaneId> {
        let Some(focused) = self.focused() else {
            return Vec::new();
        };
        if !self.synced.contains(&focused) {
            return vec![focused];
        }
        self.active()
            .panes()
            .into_iter()
            .filter(|p| self.synced.contains(p))
            .collect()
    }

    /// How many panes of the workspace shown are marked for synchronized input.
    pub fn synced_here(&self) -> usize {
        let ws = self.active();
        self.synced.iter().filter(|p| ws.contains(**p)).count()
    }

    pub fn is_synced(&self, id: PaneId) -> bool {
        self.synced.contains(&id)
    }

    /// WM mode, opened by `chord` (the leader, or the outer leader): the hint
    /// is due after the configured pause.
    fn enter_wm(&mut self, chord: crate::keys::Chord) {
        self.set_mode(Mode::Wm);
        self.wm_chord = Some(chord);
        self.hint_due = self
            .config
            .settings
            .wm_mode_hint
            .map(|d| Instant::now() + d);
    }

    /// Whether the which-key hint is shown now: it is due and WM mode is on,
    /// with nothing else up that takes the keys or the eye.
    pub fn which_key_shown(&self) -> bool {
        self.hint_on
            && self.mode == Mode::Wm
            && self.picker.is_none()
            && self.copy.is_none()
            && self.hints.is_none()
    }

    /// The chord the hint is titled with.
    pub fn wm_chord(&self) -> crate::keys::Chord {
        self.wm_chord.unwrap_or(self.config.settings.leader)
    }

    fn set_mode(&mut self, mode: Mode) {
        if mode != Mode::Wm {
            self.hint_due = None;
            self.hint_on = false;
        }
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
            Action::FocusCycle { forward } => self.focus_cycle(forward),
            // Monocle shows one tile: sideways is through the tabs.
            Action::Focus(dir @ (Dir::Left | Dir::Right))
                if self.config.settings.layout == Layout::Monocle && !self.scratch_shown =>
            {
                self.focus_cycle(dir == Dir::Right);
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
            Action::SyncToggle => {
                if let Some(id) = focused
                    && !self.synced.remove(&id)
                {
                    self.synced.insert(id);
                }
                self.dirty = true;
            }
            Action::SyncClear => {
                self.synced.clear();
                self.dirty = true;
            }
            Action::SwapMaster => {
                if let Some(id) = focused.filter(|_| !floating)
                    && let Some(m) = self.active().tree.master_partner(id)
                    && self.active_mut().tree.swap(id, m)
                {
                    self.relayout();
                }
            }
            Action::Equalize => {
                if self.active_mut().tree.equalize() {
                    self.relayout();
                }
            }
            Action::SelectLayout(p) => self.select_layout(p),
            Action::NextLayout => self.select_layout(Preset::after(self.active().preset)),
            Action::SaveLayout(Some(name)) => self.save_layout(&name),
            Action::SaveLayout(None) => self.open_save_layout_prompt(),
            Action::LoadLayout(Some(name)) => self.load_layout(&name),
            Action::LoadLayout(None) => self.open_layout_picker(),
            Action::Restore { run } => self.restore_last(run),
            Action::ToggleFloating => self.toggle_floating(),
            Action::FloatSize(pw, ph) => self.place_float(|r| r.resized_in(area, pw, ph)),
            Action::Snap(to) => self.place_float(|r| r.snapped(area, to)),
            Action::Detach => self.detach_requested = true,
            Action::ServerSwitcher => self.list_servers(),
            Action::Attach(name) => self.switch_requested = Some(name),
            Action::Profile(name) => self.use_profile(name.as_deref()),
            Action::Toolbar(name, show) => match self.config.show_toolbar(&name, show) {
                Ok(()) => self.relayout(),
                Err(e) => self.status = Some(e),
            },
            Action::Send(chord) => self.send_chord(chord),
            Action::Latch(l) => self.tap_latch(l),
            Action::PaneMenu => {
                if let Some(id) = focused {
                    self.open_pane_menu(id, 0, 0);
                    // No pointer: centred, or a sheet on a touch screen.
                    self.menu_at = None;
                }
            }
            Action::WorkspaceSwitcher => self.open_workspace_switcher(),
            Action::Update => {
                // In a float, so the pull and the build can be watched, and the
                // pane stays until a key is pressed so the result can be read.
                let source = match crate::update::Source::current() {
                    Ok(s) => s,
                    Err(e) => {
                        self.status = Some(format!("update failed: {e:#}"));
                        return;
                    }
                };
                let cmd = format!(
                    "{}; printf '\\npress a key to close'; read -rsn1 _",
                    crate::update::install_command(&source)
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
            // A tap, unlike the key, has no "again goes to the program": the
            // second one leaves, since a toolbar button is a toggle.
            Action::Leader if self.mode == Mode::Wm => self.set_mode(Mode::Normal),
            Action::Leader => {
                self.exit_copy_mode();
                self.exit_hints();
                self.enter_wm(self.config.settings.leader);
            }
            Action::SendLeader => {
                let leader = self.config.settings.leader;
                if let Some(p) = self.focused_pane()
                    && let Some(bytes) = chord_bytes(leader, p.modes())
                {
                    p.write(bytes);
                }
            }
            Action::PasteImage => self.paste_image(),
            Action::ReloadConfig => self.reload_config(),
            Action::Quit { now: true } => self.quit = true,
            Action::Quit { now: false } => self.confirm_quit(),
            Action::PaneSwitcher => self.open_pane_switcher(),
            Action::SessionSwitcher => self.open_session_switcher(),
            Action::Help => self.open_palette(crate::picker::PaletteMode::Help),
            Action::CommandPalette => self.open_palette(crate::picker::PaletteMode::Command),
            Action::CopyMode => self.enter_copy_mode(None),
            Action::Hints => self.enter_hints(),
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
            Action::SessionAccent(c) => {
                self.sessions[self.active_session].accent = c;
                self.dirty = true;
            }
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

    /// Switch profiles (DESIGN.md, "A mobile view"): what the bar, layout and
    /// modules read changes, so the screen is laid out and drawn again.
    fn use_profile(&mut self, name: Option<&str>) {
        if self.config.profile.as_deref() == name {
            return;
        }
        if let Err(e) = self.config.use_profile(name) {
            self.status = Some(e);
            return;
        }
        self.schedule_modules(Instant::now());
        self.relayout();
        self.render_state_modules();
        self.dirty = true;
    }

    fn reload_config(&mut self) {
        self.reload_at = None;
        // A reload can come from a timer, with no input event to trigger a frame;
        // either outcome puts a message in the bar that must be drawn now.
        self.dirty = true;
        match config::load(config::config_dir().as_deref()) {
            Ok(mut cfg) => {
                // The profile in use stays in use, if the new config still has it.
                let kept = self.config.profile.take();
                if let Some(name) = kept.as_deref()
                    && cfg.use_profile(Some(name)).is_err()
                {
                    self.toast(
                        format!("profile `{name}` is gone from the config; using none"),
                        crate::toast::Level::Normal,
                        None,
                    );
                }
                self.config = cfg;
                // The new theme's colours are its own; the outer's go over
                // them again, if the setting still asks for them.
                self.own_colors = None;
                self.adopt_colors();
                self.module_generation += 1;
                self.module_values.clear();
                self.module_running.clear();
                self.schedule_modules(Instant::now());
                self.status = Some("config reloaded".into());
                self.report_plugin_failures();
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
            client: self.client_facts(),
        }
    }

    fn client_facts(&self) -> config::ClientFacts {
        config::ClientFacts {
            cols: self.screen.w,
            rows: self.screen.h,
            mobile: self.client_mobile,
            remote: self.client_remote,
            outer: self.client_outer,
        }
    }

    /// A terminal drives the screen now, at the size it already set: tell
    /// `init.lua`, which may switch profiles for it (DESIGN.md, "A mobile view").
    pub fn driven_by(&mut self, mobile: bool, remote: bool) {
        let previous = self.client_mobile;
        // What the last terminal held or pressed was its own.
        self.latches.release(true);
        self.pressed = None;
        self.client_mobile = mobile;
        self.client_remote = remote;
        let facts = self.client_facts();
        let profile = self.config.profile.clone();
        self.emit(HookEvent::DriverChange, |t| {
            facts.fill(t)?;
            t.set("previous_mobile", previous)
        });
        // A sheet laid out for the other terminal's screen goes with it; within
        // one profile a picker stays open for whoever drives now.
        if self.config.profile != profile && self.picker.is_some() {
            self.close_picker();
        }
        self.dirty = true;
    }

    /// Run Lua with the runtime API live, then apply what it asked for.
    fn call_lua<R>(&mut self, f: impl FnOnce(&Lua) -> mlua::Result<R>) -> Option<R> {
        let rt = config::Runtime {
            state: self.snapshot(),
            panes: self.lua_panes(),
            ..Default::default()
        };
        self.config.lua.set_app_data(rt);
        let result = self
            .config
            .watchdog
            .run(config::CALL_BUDGET, || f(&self.config.lua));
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
        if !rt.ops.is_empty() {
            if self.lua_depth >= MAX_LUA_DEPTH {
                self.status = Some("lua: actions nested too deep; stopped".into());
            } else {
                self.lua_depth += 1;
                for op in rt.ops {
                    match op {
                        config::Op::Action(a) => self.run_action(a),
                        config::Op::Pane(id, req) => {
                            if let Err(e) = self.pane_request(id, req) {
                                self.status = Some(format!("lua: {e}"));
                            }
                        }
                        config::Op::Spawn(id, spec) => {
                            self.config.jobs.start(id, spec, self.tx.clone())
                        }
                        config::Op::Kill(id) => self.config.jobs.kill(id),
                    }
                }
                self.lua_depth -= 1;
            }
        }
        self.dirty = true;
        out
    }

    /// `ranma.defer` and `ranma.every` timers that came due. A repeating
    /// timer whose function fails is stopped: at its rate, an error each
    /// tick would be all the bar ever said.
    fn run_lua_timers(&mut self, now: Instant) {
        for due in self.config.jobs.take_due(now) {
            let Ok(f) = self.config.lua.registry_value::<Function>(&due.f) else {
                continue;
            };
            if self.call_lua(|_| f.call::<()>(())).is_none() && due.repeating {
                self.config.jobs.cancel(due.id);
                let why = self.status.take().unwrap_or_default();
                self.status = Some(format!("{why} (timer {} stopped)", due.id));
            }
        }
    }

    /// What a spawned process did: its lines to `on_line`, one call for the
    /// batch, and its end to `on_exit`. A job from before a reload is
    /// unknown to this configuration, and dropped.
    fn job_event(&mut self, id: u64, event: crate::jobs::JobEvent) {
        use crate::jobs::JobEvent;
        match event {
            JobEvent::Lines(lines) => {
                let Some(key) = self.config.jobs.on_line(id) else {
                    return;
                };
                let Ok(f) = self.config.lua.registry_value::<Function>(&key) else {
                    return;
                };
                self.call_lua(|_| {
                    for line in lines {
                        f.call::<()>(line)?;
                    }
                    Ok(())
                });
            }
            JobEvent::Exit(exit) => {
                let Some(Some(key)) = self.config.jobs.finish(id) else {
                    return;
                };
                let Ok(f) = self.config.lua.registry_value::<Function>(&key) else {
                    return;
                };
                self.call_lua(|lua| {
                    let t = lua.create_table()?;
                    t.set("id", id)?;
                    t.set("code", exit.code)?;
                    t.set("signal", exit.signal)?;
                    t.set("stdout", exit.stdout)?;
                    t.set("stderr", exit.stderr)?;
                    t.set("error", exit.error)?;
                    f.call::<()>(t)
                });
            }
        }
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

    /// Timed modules the bar shows are due now. One defined but not in the bar
    /// never ticks: idle means no wakeups, and nobody would see the result.
    fn schedule_modules(&mut self, now: Instant) {
        let shown: HashSet<&String> = self.config.bar.all().collect();
        self.module_due = self
            .config
            .modules
            .iter()
            .filter(|(name, m)| m.interval.is_some() && shown.contains(name))
            .map(|(name, _)| (name.clone(), now))
            .collect();
    }

    /// A bar message goes `STATUS_FOR` after it first showed. Messages are
    /// set in many places by assignment; noticing a new one here keeps them
    /// all on one clock without each having to start it.
    fn expire_status(&mut self, now: Instant) {
        let Some(text) = &self.status else {
            self.status_seen = None;
            return;
        };
        match &self.status_seen {
            Some((seen, at)) if seen == text => {
                if now >= *at + STATUS_FOR {
                    self.status = None;
                    self.status_seen = None;
                    self.dirty = true;
                }
            }
            _ => self.status_seen = Some((text.clone(), now)),
        }
    }

    /// The next moment something needs doing without an event arriving.
    fn next_deadline(&self) -> Option<Instant> {
        self.module_due
            .values()
            .copied()
            .chain(self.reload_at)
            .chain(self.programs_due)
            .chain(self.snapshot_due)
            .chain(self.toasts.next_expiry())
            .chain(self.status_seen.as_ref().map(|(_, at)| *at + STATUS_FOR))
            .chain(self.hint_due)
            .chain(self.press_due())
            .chain(self.config.jobs.next_due())
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
    /// Something happened that may have changed what runs in a pane: read the
    /// programs now, or once `PROGRAMS_EVERY` has passed since the last read. A
    /// key (`follow_up`) also gets a read after that: the Enter that starts
    /// `ssh vps` arrives before ssh does. Only events call this, never the timer
    /// it sets, so it cannot keep itself awake.
    fn note_programs(&mut self, now: Instant, follow_up: bool) {
        if self.config.workspaces_numbers_only || self.programs_due.is_some() {
            return;
        }
        match self.programs_read {
            Some(t) if now < t + PROGRAMS_EVERY => self.programs_due = Some(t + PROGRAMS_EVERY),
            _ => {
                self.read_programs(now);
                if follow_up {
                    self.programs_due = Some(now + PROGRAMS_EVERY);
                }
            }
        }
    }

    fn read_programs(&mut self, now: Instant) {
        self.programs_due = None;
        self.programs_read = Some(now);
        let programs: HashMap<PaneId, String> = self
            .workspaces
            .values()
            .filter_map(|ws| ws.focused)
            // A pane holding a ranma is named on its border's label even
            // when another pane has the focus (see `nest_labels`).
            .chain(
                self.panes
                    .iter()
                    .filter(|(_, p)| p.hosts_ranma())
                    .map(|(id, _)| *id),
            )
            .filter_map(|id| Some((id, self.panes.get(&id)?.workspace_label()?)))
            .collect();
        if programs != self.programs {
            self.programs = programs;
            self.dirty = true;
            // An `ssh` starting or ending in the focused pane changes the host
            // the title names, whether or not its title changed.
            self.announce();
        }
        // Only what a border title asks for is read, and only for panes on screen.
        let fmt = &self.config.theme.border.title_format;
        let (program, cwd) = (fmt.uses("program"), fmt.uses("cwd"));
        let facts: HashMap<PaneId, (Option<String>, Option<String>)> = if program || cwd {
            let home = dirs::home_dir();
            self.visible
                .iter()
                .filter_map(|id| {
                    let p = self.panes.get(id)?;
                    Some((
                        *id,
                        (
                            p.program().filter(|_| program),
                            p.cwd()
                                .filter(|_| cwd)
                                .map(|c| crate::layouts::tilde(&c, home.as_deref())),
                        ),
                    ))
                })
                .collect()
        } else {
            HashMap::new()
        };
        if facts != self.pane_facts {
            self.pane_facts = facts;
            self.dirty = true;
        }
    }

    /// A pane's title as its border shows it: `border.title_format`, with
    /// what the pane is called, where it is in its workspace, and what the
    /// last read of /proc found running there.
    pub fn border_title(&self, id: PaneId) -> String {
        let Some(p) = self.panes.get(&id) else {
            return String::new();
        };
        let facts = self.pane_facts.get(&id);
        self.config.theme.border.title_format.render(|v| match v {
            "title" => Some(p.label().to_string()),
            "index" => {
                let ws = match self.locate(id)? {
                    SCRATCHPAD => &self.scratch,
                    n => self.workspaces.get(&n)?,
                };
                let at = ws.panes().iter().position(|q| *q == id)?;
                Some((at + 1).to_string())
            }
            "program" => facts.and_then(|f| f.0.clone()),
            "cwd" => facts.and_then(|f| f.1.clone()),
            _ => None,
        })
    }

    pub(super) fn announce(&mut self) {
        if self.config.settings.nested == config::NestedMode::Off {
            return;
        }
        let label: String = self
            .focused_title()
            .unwrap_or("")
            .chars()
            .filter(|c| !c.is_control())
            .collect();
        // The innermost host wins: a ranma in the focused pane (over SSH, say)
        // names where you are, so a terminal tab three levels out still says it.
        // Without one, an `ssh` running in the focused pane names where it went.
        let focused = self.focused().and_then(|f| self.panes.get(&f));
        let inner =
            focused.and_then(|p| p.inner_host().map(str::to_string).or_else(|| p.ssh_host()));
        let own = match self.config.settings.title_host {
            config::TitleHost::Always => Some(crate::pane::hostname()),
            config::TitleHost::Ssh if self.client_remote => Some(crate::pane::hostname()),
            _ => None,
        };
        let title = crate::pane::own_title(self.engaged(), inner.as_deref(), own, &label);
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
        self.expire_press(now);
        self.run_lua_timers(now);
        if self.hint_due.is_some_and(|t| t <= now) {
            self.hint_due = None;
            self.hint_on = self.mode == Mode::Wm;
            self.dirty = true;
        }
        if self.programs_due.is_some_and(|t| t <= now) {
            self.read_programs(now);
        }
        if self.snapshot_due.is_some_and(|t| t <= now) {
            self.write_snapshot();
        }
        if self.toasts.expire(now) {
            self.dirty = true;
        }
        self.expire_status(now);
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
            ModuleKind::Cpu { format } => {
                use crate::sysstat;
                let now = sysstat::read_cpu();
                let pct = self
                    .cpu_prev
                    .zip(now)
                    .and_then(|(a, b)| sysstat::cpu_percent(a, b));
                self.cpu_prev = now;
                // The first tick has nothing to compare with: show nothing yet.
                let seg = pct.map_or_else(Vec::new, |p| {
                    let text = bar::format_output(
                        Some(format.as_deref().unwrap_or("cpu %s")),
                        &format!("{p}%"),
                    );
                    vec![Piece::new(text, sys_style(p))]
                });
                self.set_module_value(name.to_string(), seg);
            }
            ModuleKind::Mem { format } => {
                use crate::sysstat;
                let seg = sysstat::read_mem().map_or_else(Vec::new, |m| {
                    let text = bar::format_output(
                        Some(format.as_deref().unwrap_or("mem %s")),
                        &sysstat::gib(m.used_kib()),
                    );
                    vec![Piece::new(text, sys_style(m.used_percent()))]
                });
                self.set_module_value(name.to_string(), seg);
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
                // Keys are going to a ranma inside the focused pane. One that
                // draws no bar of its own says its mode here.
                Mode::Normal if self.hints.is_some() => vec![Piece::new(" LINK ", Style::Mode)],
                Mode::Normal if self.latches.any() => self
                    .latch_label()
                    .map(|l| vec![Piece::new(format!(" {l} "), Style::Mode)])
                    .unwrap_or_default(),
                Mode::Normal if self.passes_through() => {
                    // ⧉ stays (keys go inside); an inner mode follows it, so
                    // a bare ` WM ` is always this ranma's own.
                    let inner = self
                        .nested_path()
                        .iter()
                        .rev()
                        .map(|r| r.mode.as_str())
                        .find(|m| *m != "normal" && !m.is_empty())
                        .map(|m| Piece::new(format!(" {} ", m.to_uppercase()), Style::Mode));
                    std::iter::once(Piece::new(bar::NESTED_SIGN, Style::Dim))
                        .chain(inner)
                        .collect()
                }
                Mode::Normal => Vec::new(),
            }
            .into_iter()
            // Typing into several panes at once is never something to forget.
            .chain(
                (self.synced_here() > 0)
                    .then(|| Piece::new(format!(" ⇉ sync {} ", self.synced_here()), Style::Urgent)),
            )
            .collect(),
            // Only once there is more than one: a lone "main" says nothing.
            "session" if self.session_count() > 1 => {
                vec![
                    Piece::new(self.session_name(), Style::Accent).on_click(Click::SessionSwitcher),
                ]
            }
            "session" => Vec::new(),
            "workspaces" => {
                let compact = self.chrome().compact_workspaces;
                let mut seg: Segment = self
                    .workspace_list()
                    .into_iter()
                    .flat_map(|(n, current, occupied, urgent, name)| {
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
                        // A short screen keeps only the current one's name.
                        let name = name.filter(|_| current || !compact);
                        let theme_bar = &self.config.theme.bar;
                        let fmt = if current {
                            &theme_bar.workspace_current_format
                        } else {
                            &theme_bar.workspace_format
                        };
                        let label = fmt.render(|v| match v {
                            "n" => Some(n.to_string()),
                            "name" => name.clone(),
                            _ => None,
                        });
                        // A holder drawn here is collapsed (nestbar draws the
                        // bar while one expands), so it gets its count too.
                        match name.and(self.holder_in_use(n)) {
                            Some(k) => vec![
                                Piece::new(label.trim_end().to_string(), style)
                                    .on_click(Click::Workspace(n)),
                                Piece::new(format!("[{k}] "), Style::Dim)
                                    .on_click(Click::Workspace(n)),
                            ],
                            None => vec![Piece::new(label, style).on_click(Click::Workspace(n))],
                        }
                    })
                    .collect();
                let (has, shown) = self.scratch_state();
                if has {
                    let style = if shown {
                        Style::WsActive
                    } else {
                        Style::WsOccupied
                    };
                    // A ranma reached from the scratchpad: drawn here, it
                    // is collapsed, so `S` says how many are inside.
                    let at = Click::Workspace(SCRATCHPAD);
                    match self.scratch_in_use() {
                        Some(k) => {
                            seg.push(Piece::new(" S", style).on_click(at));
                            seg.push(Piece::new(format!("[{k}] "), Style::Dim).on_click(at));
                        }
                        None => seg.push(Piece::new(" S ", style).on_click(at)),
                    }
                }
                seg
            }
            "title" => self
                .focused_title()
                .filter(|t| !t.is_empty())
                .map(|t| vec![Piece::new(t, Style::Normal)])
                .unwrap_or_default(),
            "panes" => vec![Piece::new(self.panes.len().to_string(), Style::Dim)],
            "pane_strip" => {
                // The innermost ranma on the focused path with two panes or
                // more: its bar is this one, so its panes are what focus is
                // among. Its chips focus the pane that holds it.
                let holder = self.focused();
                let chips: Vec<(Option<PaneId>, String, bool)> =
                    match crate::nestbar::strip_of(&self.nested_path()) {
                        Some(inner) => inner
                            .iter()
                            .map(|c| (holder, c.label.clone(), c.focused))
                            .collect(),
                        None => self
                            .strip_panes()
                            .into_iter()
                            .map(|(id, label, focused)| (Some(id), label, focused))
                            .collect(),
                    };
                if chips.len() < 2 {
                    return Vec::new();
                }
                let mut seg = Vec::new();
                for (id, label, focused) in chips {
                    // Filled chips, a column apart, so each reads as one.
                    if !seg.is_empty() {
                        seg.push(Piece::new(" ", Style::Normal));
                    }
                    let style = if focused {
                        Style::TabActive
                    } else {
                        Style::TabInactive
                    };
                    let chip = Piece::new(format!(" {label} "), style);
                    seg.push(match id {
                        Some(id) => chip.on_click(Click::Pane(id)),
                        None => chip,
                    });
                }
                seg
            }
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
        let chrome = self.chrome();
        let large = chrome.bar_size == crate::toolbar::Size::Large;
        // Chips a thumb can hit: padded to five columns and a column apart.
        let theme_bar = &self.config.theme.bar;
        let wrap = |s: Segment| bar::boxed(s, &theme_bar.module_left, &theme_bar.module_right);
        let seg = |n: &String| {
            let s = self.segment(n);
            wrap(
                if large && matches!(n.as_str(), "mode" | "workspaces" | "pane_strip") {
                    bar::enlarge(s)
                } else {
                    s
                },
            )
        };
        let side = |names: &[String]| -> Vec<Segment> { names.iter().map(seg).collect() };
        let bar = &self.config.bar;
        let cols = cols.min(chrome.bar_room);
        // A message of this ranma's, else one from a nested ranma that draws
        // no bar of its own, the deepest first.
        let status = self.status.clone().or_else(|| {
            self.nested_path()
                .iter()
                .rev()
                .find_map(|r| r.status.clone())
        });
        let center = match &status {
            Some(msg) => vec![wrap(vec![Piece::new(msg.clone(), Style::Accent)])],
            None => side(&bar.center),
        };
        let right = side(&bar.right);
        // Chips a column apart, whatever the theme's separator: the large
        // bar's spacing is the design's (the handoff's section 06).
        let sep = if large {
            " "
        } else {
            self.config.theme.bar.separator.as_str()
        };
        // The workspaces module, with the workspaces of the ranmas inside
        // them (see `nestbar`), when there are any to show.
        if self.config.workspaces_nested != config::NestedWorkspaces::Off
            && let Some(at) = bar.left.iter().position(|m| m == "workspaces")
        {
            let set = self.own_report();
            let expand_all = self.config.workspaces_nested == config::NestedWorkspaces::All;
            if crate::nestbar::expands(&set, expand_all) {
                return crate::nestbar::fit_nested(
                    &side(&bar.left[..at]),
                    &set,
                    &side(&bar.left[at + 1..]),
                    &center,
                    &right,
                    sep,
                    cols,
                    expand_all,
                    &wrap,
                    // A message is never left out; the title may be.
                    if status.is_some() {
                        0
                    } else {
                        crate::nestbar::TITLE_FLOOR
                    },
                );
            }
        }
        let mut left = side(&bar.left);
        let mut right = right;
        // The monocle strip folded into the bar (see `chrome`): its chips after
        // the workspaces where they fit, else one `2/4 nvim` chip at the end.
        if chrome.strip == crate::chrome::Strip::InBar {
            // Left-aligned, as the strip's own row draws them, not centred.
            let chips = wrap(self.segment("pane_strip"));
            let mut with = left.clone();
            with.push(chips.clone());
            let width = |segs: &[Segment]| -> usize {
                segs.iter()
                    .filter(|s| !s.is_empty())
                    .map(|s| {
                        use unicode_width::UnicodeWidthStr;
                        s.iter().map(|p| p.text.width()).sum::<usize>() + sep.width()
                    })
                    .sum()
            };
            let used = width(&with) + width(&right) + width(&center);
            if !large {
                left = with;
            } else if used <= cols as usize {
                left.push(bar::stretch(chips, cols as usize - used));
            } else if let Some(chip) = self.strip_count() {
                right.push(vec![
                    Piece::new(chip, Style::TabActive).on_click(Click::Panes),
                ]);
            }
        }
        bar::fit(&left, &center, &right, sep, cols)
    }

    /// The folded strip's one chip: ` 2/4 nvim `, where the focused pane is
    /// among the workspace's and what it is called.
    /// The current workspace's panes as `pane_strip` draws them: each with
    /// its chip label and whether it has focus.
    pub(super) fn strip_panes(&self) -> Vec<(PaneId, String, bool)> {
        let ws = self.active();
        ws.panes()
            .into_iter()
            .map(|id| (id, self.chip_label(id), Some(id) == ws.focused))
            .collect()
    }

    fn strip_count(&self) -> Option<String> {
        let ws = self.active();
        let panes = ws.panes();
        let f = ws.focused?;
        let at = panes.iter().position(|p| *p == f)? + 1;
        Some(format!(" {at}/{} {} ", panes.len(), self.chip_label(f)))
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

/// A cpu or mem reading at or above `URGENT_PERCENT` is urgent.
fn sys_style(percent: u8) -> Style {
    if percent >= crate::sysstat::URGENT_PERCENT {
        Style::Urgent
    } else {
        Style::Normal
    }
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
pub(super) fn chord_bytes(chord: crate::keys::Chord, modes: input::PaneModes) -> Option<Vec<u8>> {
    input::encode_key(&chord_event(chord), modes)
}

/// The key event a terminal would report for `chord`.
pub(super) fn chord_event(chord: crate::keys::Chord) -> KeyEvent {
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
    KeyEvent::new(code, mods)
}

pub(super) fn spawn_input_thread(tx: Sender<AppEvent>) {
    let resizes = tx.clone();
    if let Err(e) = crate::winch::spawn(crossterm::terminal::size, move |cols, rows| {
        resizes
            .send(AppEvent::Input(Event::Resize(cols, rows)))
            .is_ok()
    }) {
        eprintln!("ranma: watching for resizes: {e}");
    }
    std::thread::Builder::new()
        .name("input".into())
        .spawn(move || {
            loop {
                match crossterm::event::read() {
                    Ok(ev) if crate::winch::from_crossterm(&ev) => {}
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
/// the parent of a linked `init.lua`, and a linked `themes/`, `lua/`,
/// `plugin/`, `pack/` or package itself.
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
    // Directories a dotfiles repo or a cloned plugin is linked in as: the
    // plugin layout's three, and each package under `pack/*/start/`.
    let packages = std::fs::read_dir(dir.join("pack"))
        .into_iter()
        .flatten()
        .flatten()
        .flat_map(|pack| {
            std::fs::read_dir(pack.path().join("start"))
                .into_iter()
                .flatten()
        })
        .flatten()
        .map(|e| e.path());
    let linked = ["themes", "lua", "plugin", "pack"]
        .into_iter()
        .map(|d| dir.join(d))
        .chain(packages);
    for d in linked {
        if d.is_symlink()
            && let Ok(t) = d.canonicalize()
        {
            out.push((t, notify::RecursiveMode::Recursive));
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn app(user: Option<&str>) -> App {
        let config = crate::config::load_from(None, None, user).unwrap();
        let (tx, _rx) = std::sync::mpsc::channel();
        App::new(config, tx, 80, 24)
    }

    fn with_pane(app: &mut App, id: PaneId) {
        let ws = app.workspaces.get_mut(&1).unwrap();
        ws.tree
            .insert(id, None, None, crate::layout::Placement::Dwindle);
        ws.focused = Some(id);
    }

    #[test]
    fn closing_a_float_focuses_the_one_left_on_top() {
        let mut a = app(None);
        // A cascaded pile in the scratchpad, 3 on top and focused.
        for id in 1..=3u16 {
            a.scratch
                .floating
                .push((id as PaneId, Rect::new(10 + 3 * id, 5 + id, 30, 10)));
        }
        a.scratch.focused = Some(3);
        a.scratch_shown = true;
        a.detach(3);
        assert_eq!(
            a.scratch.focused,
            Some(2),
            "the float under it, not one buried"
        );
        assert_eq!(a.scratch.floating.last().unwrap().0, 2);

        // Focus on a float that is not on top (a click raises, but a hook may not).
        a.scratch.floating.push((4, Rect::new(40, 10, 30, 10)));
        a.scratch.focused = Some(2);
        a.detach(2);
        assert_eq!(a.scratch.focused, Some(4));
    }

    #[test]
    fn next_layout_steps_through_tmuxs_presets() {
        let mut a = app(None);
        for id in 1..=3 {
            with_pane(&mut a, id);
        }
        a.active_mut().fullscreen = true;
        a.run_action(Action::NextLayout);
        assert_eq!(a.active().preset, Some(Preset::EvenHorizontal));
        assert!(!a.active().fullscreen, "a layout is for seeing every tile");
        let rows: Vec<u16> = a.frame().views.iter().map(|v| v.outer.y).collect();
        assert!(rows.iter().all(|y| *y == rows[0]), "side by side: {rows:?}");
        a.run_action(Action::NextLayout);
        assert_eq!(a.active().preset, Some(Preset::EvenVertical));
        a.run_action("select_layout tiled".parse().unwrap());
        a.run_action(Action::NextLayout);
        assert_eq!(a.active().preset, Some(Preset::EvenHorizontal), "wraps");
        assert_eq!(a.active().tree.panes(), vec![1, 2, 3], "every pane kept");
    }

    #[test]
    fn master_refuses_a_preset_it_would_undo() {
        let mut a = app(Some(r#"ranma.set { layout = "master" }"#));
        for id in 1..=3 {
            with_pane(&mut a, id);
        }
        a.run_action("select_layout even-vertical".parse().unwrap());
        assert_eq!(a.active().preset, None);
        assert!(a.status.as_deref().is_some_and(|s| s.contains("master")));
        a.run_action("select_layout main-vertical".parse().unwrap());
        assert_eq!(a.active().preset, Some(Preset::MainVertical));
    }

    #[test]
    fn a_click_off_the_scratchpad_hides_it() {
        let mut a = app(None);
        with_pane(&mut a, 1);
        a.scratch.floating.push((2, Rect::new(30, 6, 20, 8)));
        a.scratch.focused = Some(2);
        let press = |x, y| {
            AppEvent::Input(Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: x,
                row: y,
                modifiers: crossterm::event::KeyModifiers::NONE,
            }))
        };
        for mode in [Mode::Normal, Mode::Wm] {
            a.mode = mode;
            a.scratch_shown = true;
            a.handle(press(40, 10));
            assert!(a.scratch_shown, "{mode:?}: a click on its pane keeps it");
            a.handle(press(5, 3));
            assert!(!a.scratch_shown, "{mode:?}: a click beside it hides it");
            assert_eq!(a.scratch.len(), 1, "{mode:?}: hidden, not closed");
        }
    }

    #[test]
    fn enter_on_an_empty_workspace_opens_a_pane() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let mut a = app(None);
        assert!(a.focused().is_none(), "a new app starts empty");
        assert!(a.enter_opens_pane(&enter));
        assert!(
            a.enter_opens_pane(&KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT)),
            "shift is still Enter"
        );
        for mods in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
            assert!(!a.enter_opens_pane(&KeyEvent::new(KeyCode::Enter, mods)));
        }
        assert!(!a.enter_opens_pane(&KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)));
        assert!(
            a.enter_opens_pane(&KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL)),
            "a keypad Enter sent as LF"
        );
        // With a pane there, Enter is the program's.
        with_pane(&mut a, 1);
        assert!(!a.enter_opens_pane(&enter));
    }

    #[test]
    fn init_lua_hears_which_terminal_drives() {
        let mut a = app(Some(
            r#"
            ranma.on("driver_change", function(ev)
              ranma.notify(string.format("%dx%d mobile=%s was=%s remote=%s",
                ev.cols, ev.rows, ev.mobile, ev.previous_mobile, ev.remote))
            end)
            ranma.bind("z", function()
              local c = ranma.client()
              ranma.notify(string.format("%dx%d %s", c.cols, c.rows, c.mobile))
            end)
            "#,
        ));
        a.handle(AppEvent::Input(Event::Resize(52, 34)));
        a.driven_by(true, true);
        assert_eq!(
            a.status.as_deref(),
            Some("52x34 mobile=true was=false remote=true")
        );
        a.handle(AppEvent::Input(Event::Resize(200, 50)));
        a.driven_by(false, false);
        assert_eq!(
            a.status.as_deref(),
            Some("200x50 mobile=false was=true remote=false")
        );
        a.run_bind("z".parse().unwrap(), false);
        assert_eq!(a.status.as_deref(), Some("200x50 false"));
    }

    /// `outer` says a ranma is around the terminal driving, as `drive` sets
    /// it before `driver_change`: init.lua can tell nested from not.
    #[test]
    fn init_lua_hears_whether_a_ranma_is_around_the_terminal() {
        let mut a = app(Some(
            r#"
            ranma.on("driver_change", function(ev)
              ranma.notify("outer=" .. tostring(ev.outer))
            end)
            ranma.bind("z", function()
              ranma.notify("client outer=" .. tostring(ranma.client().outer))
            end)
            "#,
        ));
        a.set_outer(Some(crate::nestbar::PROTOCOL));
        a.driven_by(false, true);
        assert_eq!(a.status.as_deref(), Some("outer=true"));
        a.run_bind("z".parse().unwrap(), false);
        assert_eq!(a.status.as_deref(), Some("client outer=true"));
        a.set_outer(None);
        a.driven_by(false, false);
        assert_eq!(a.status.as_deref(), Some("outer=false"));
    }

    #[test]
    fn a_hook_switches_profiles_for_the_terminal_driving() {
        let mut a = app(Some(
            r#"
            ranma.profile("mobile", { set = { layout = "master" } })
            ranma.on("driver_change", function(c)
              ranma.use_profile(c.mobile and "mobile" or nil)
            end)
            "#,
        ));
        a.driven_by(true, false);
        assert_eq!(a.config.settings.layout, Layout::Master);
        assert_eq!(a.config.profile.as_deref(), Some("mobile"));
        a.driven_by(false, false);
        assert_eq!(a.config.settings.layout, Layout::Dwindle);
        assert_eq!(a.config.profile, None);
        a.run_action("profile nope".parse().unwrap());
        assert!(
            a.status
                .as_deref()
                .unwrap_or("")
                .contains("no profile `nope`")
        );
    }

    #[test]
    fn monocle_shows_one_tile_and_the_rest_as_tabs() {
        let mut a = app(Some(
            r#"ranma.set { layout = "monocle" }
               ranma.profile("tile", { set = { layout = "dwindle" } })"#,
        ));
        for id in [1, 2, 3] {
            with_pane(&mut a, id);
        }
        a.workspaces
            .get_mut(&1)
            .unwrap()
            .floating
            .push((4, Rect::new(10, 5, 30, 8)));
        let tree_before = format!("{:?}", a.workspaces[&1].tree);
        a.focus(2);
        let f = a.frame();
        let tiles: Vec<_> = f
            .views
            .iter()
            .filter(|v| !v.floating)
            .map(|v| v.id)
            .collect();
        assert_eq!(tiles, vec![2], "one tile on screen");
        assert!(
            f.views.iter().any(|v| v.id == 4 && v.floating),
            "floats still float"
        );
        let area = a.workspace_area();
        let v = f.views.iter().find(|v| v.id == 2).unwrap();
        assert_eq!(
            v.outer, area,
            "the strip's row is the chrome's, not the pane's"
        );
        assert_eq!(f.tab_bars.len(), 1);
        assert_eq!(f.tab_bars[0].rect, Rect::new(0, 0, 80, 1));
        assert_eq!(area.y, 1);
        assert_eq!(f.tab_bars[0].tabs, vec![1, 2, 3, 4], "tiles, then floats");
        assert_eq!(f.tab_bars[0].active, 1);
        let hidden: Vec<_> = f.hidden.iter().map(|(id, r)| (*id, *r)).collect();
        assert!(
            hidden.iter().all(|(_, r)| *r == v.inner),
            "sized as if shown"
        );
        assert_eq!(hidden.len(), 2);
        // A float focused: the tile last focused stays on screen.
        a.focus(4);
        let f = a.frame();
        assert!(f.views.iter().any(|v| v.id == 2 && !v.floating));
        assert_eq!(f.tab_bars[0].active, 3);
        // Sideways is through the tabs, wrapping.
        a.run_action("focus right".parse().unwrap());
        assert_eq!(a.focused(), Some(1));
        a.run_action("focus prev".parse().unwrap());
        assert_eq!(a.focused(), Some(4));
        // The strip in the bar lists the same panes.
        let strip: Vec<_> = a
            .segment("pane_strip")
            .into_iter()
            .filter(|p| p.click.is_some())
            .collect();
        assert_eq!(strip.len(), 4);
        assert_eq!(strip[3].text, " ◇ shell ");
        assert_eq!(strip[3].style, Style::TabActive);
        assert_eq!(strip[0].click, Some(Click::Pane(1)));
        // Another layout gives the tiling back: the tree was never touched.
        a.run_action("profile tile".parse().unwrap());
        assert_eq!(format!("{:?}", a.workspaces[&1].tree), tree_before);
        assert!(a.frame().tab_bars.is_empty());
    }

    #[test]
    fn monocle_draws_no_strip_for_one_pane() {
        let mut a = app(Some(r#"ranma.set { layout = "monocle" }"#));
        with_pane(&mut a, 1);
        let f = a.frame();
        assert!(f.tab_bars.is_empty());
        assert_eq!(f.views[0].outer, a.workspace_area());
        assert!(a.segment("pane_strip").is_empty());
    }

    /// The mobile view as the handoff draws it: workspaces 1:zsh, 2:vps (four
    /// panes, current) and 3:ai, the scratchpad in use, a phone driving.
    fn phone(rows: u16) -> App {
        let mut a = app(Some(
            r#"
            ranma.toolbar("touch", {
              size = "large",
              buttons = {
                { "≡", "pane_menu", text = "menu" },
                { "+", "new_pane", text = "new" },
                { "◀", "focus prev", text = "prev" },
                { "▶", "focus next", text = "next" },
                { "⌃", "latch ctrl", text = "ctrl" },
                { "⎋", "send esc", text = "esc" },
                { "⊞", "workspace_switcher", text = "spaces" },
                { "✕", "close_pane", text = "close" },
              },
            })
            ranma.profile("mobile", {
              set = { layout = "monocle" },
              bar = { size = "large", left = { "mode", "workspaces" }, center = {}, right = {} },
              toolbars = { "touch" },
            })
            ranma.on("driver_change", function(c)
              ranma.use_profile(c.mobile and "mobile" or nil)
            end)
            "#,
        ));
        let mut id = 1;
        for (n, name, count) in [(1u8, "zsh", 1), (3, "ai", 1), (2, "vps", 4)] {
            a.switch_workspace(n);
            for _ in 0..count {
                with_pane_in(&mut a, n, id);
                id += 1;
            }
            a.rename_workspace(n, name);
        }
        a.focus(5);
        a.scratch.floating.push((99, Rect::new(5, 5, 20, 5)));
        a.handle(AppEvent::Input(Event::Resize(52, rows)));
        a.driven_by(true, true);
        a
    }

    fn with_pane_in(app: &mut App, n: u8, id: PaneId) {
        let ws = app.workspaces.entry(n).or_default();
        ws.tree
            .insert(id, ws.focused, None, crate::layout::Placement::Dwindle);
        ws.focused = Some(id);
    }

    /// The screen as text, each row trimmed at the end.
    fn screen(a: &App) -> Vec<String> {
        use ratatui::backend::TestBackend;
        let (w, h) = (a.screen.w, a.screen.h);
        let mut t = ratatui::Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| {
            crate::render::draw(f, a);
        })
        .unwrap();
        let buf = t.backend().buffer().clone();
        (0..h)
            .map(|y| {
                let mut row = String::new();
                let mut x = 0;
                while x < w {
                    let sym = buf[(x, y)].symbol();
                    row.push_str(sym);
                    x += (unicode_width::UnicodeWidthStr::width(sym) as u16).max(1);
                }
                row.trim_end().to_string()
            })
            .collect()
    }

    /// A theme file in a fresh config directory, and an app using it.
    fn themed(tag: &str, theme: &str, user: &str) -> App {
        let dir = std::env::temp_dir().join(format!("ranma-looks-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("themes")).unwrap();
        std::fs::write(dir.join("themes").join("t.toml"), theme).unwrap();
        let src = format!("ranma.set {{ theme = \"t\" }}\n{user}");
        let config = crate::config::load_from(Some(&dir), None, Some(&src)).unwrap();
        let (tx, _rx) = std::sync::mpsc::channel();
        App::new(config, tx, 60, 6)
    }

    #[test]
    fn workspaces_follow_their_formats_and_modules_sit_between_caps() {
        let mut a = themed(
            "bar",
            r##"
            [colors]
            module_bg = "#313244"
            [bar]
            workspace_format = "<{n}[ {name}]>"
            workspace_current_format = "[[{n}]]"
            module_left = "("
            module_right = ")"
            separator = " "
            "##,
            r#"ranma.bar { left = { "workspaces" }, center = {}, right = { "panes" } }"#,
        );
        with_pane_in(&mut a, 1, 1);
        with_pane_in(&mut a, 2, 2);
        a.rename_workspace(2, "logs");
        let bar = screen(&a).pop().unwrap();
        assert!(bar.starts_with("([1]<2 logs>)"), "{bar:?}");
        // `panes` counts running panes; these have no PTY behind them.
        assert!(bar.ends_with("(0)"), "{bar:?}");
        // The ground is the module's, the caps are its colour on the bar's.
        let pieces = a.bar_pieces(60);
        let cap = pieces.iter().find(|(_, p)| p.text == "(").unwrap();
        assert_eq!(cap.1.style, Style::Cap);
        assert!(
            pieces
                .iter()
                .filter(|(_, p)| p.text.contains('2'))
                .all(|(_, p)| p.boxed)
        );
    }

    /// Section 03, keyboard closed, row for row where the mock's rows are
    /// ranma's. The mock's bar opens with a dim ⧉ that ranma shows only while
    /// keys go to a ranma inside (DESIGN.md, "A mobile view", departures), so
    /// the bar here starts where the mock's workspaces do.
    #[test]
    fn the_phone_matches_the_handoff() {
        let a = phone(34);
        assert_eq!(a.config.profile.as_deref(), Some("mobile"));
        let s = screen(&a);
        let names: Vec<usize> = s[1].match_indices("shell").map(|(i, _)| i).collect();
        assert_eq!(
            names,
            vec![1, 14, 27, 40],
            "the strip's chips, as stripLarge lays them"
        );
        assert_eq!(s[0], "");
        assert_eq!(s[29], " 1:zsh   2:vps   3:ai    S");
        assert_eq!(s[28], "");
        assert_eq!(s[32], "  ≡     +      ◀     ▶      ⌃      ⎋     ⊞      ✕");
        // The desk takes the screen back: the base again, no toolbar.
        let mut a = a;
        a.handle(AppEvent::Input(Event::Resize(200, 50)));
        a.driven_by(false, false);
        assert_eq!(a.config.profile, None);
        assert!(a.shown_toolbars().is_empty());
        assert!(a.frame().tab_bars.is_empty());
    }

    /// Section 03, Gboard open: one bar row with the strip's chips in it and
    /// only the current workspace named; no border; the toolbar still large.
    #[test]
    fn the_phone_with_the_keyboard_open_matches_the_handoff() {
        let a = phone(18);
        let s = screen(&a);
        assert_eq!(s[14], " 1  2:vps  3  S    shell   shell   shell   shell");
        assert_eq!(s[16], "  ≡     +      ◀     ▶      ⌃      ⎋     ⊞      ✕");
        assert_eq!(a.frameless(), Some(5), "the pane on screen has no border");
        assert!(a.frame().tab_bars.is_empty(), "the strip is in the bar");
    }

    /// Section 09: phone landscape. The toolbar sits beside the bar, the
    /// strip's chips between them, and the pane keeps 19 rows and its border.
    #[test]
    fn landscape_matches_the_handoff() {
        let mut a = phone(22);
        a.config.toolbars[0].1.position = crate::toolbar::Position::Beside;
        a.handle(AppEvent::Input(Event::Resize(110, 22)));
        let s = screen(&a);
        let row: Vec<char> = s[20].chars().collect();
        let symbols: String = [65, 71, 77, 83, 89, 95, 101, 107]
            .iter()
            .map(|x| row[*x])
            .collect();
        assert_eq!(symbols, "≡+◀▶⌃⎋⊞✕", "{:?}", s[20]);
        assert!(
            s[20].starts_with(" 1:zsh   2:vps   3:ai    S    shell"),
            "{:?}",
            s[20]
        );
        let chips: Vec<usize> = s[20].match_indices("shell").map(|(i, _)| i).collect();
        assert_eq!(chips.len(), 4, "the strip's chips, in the bar");
        assert!(chips[3] + 6 < 63, "left of the toolbar");
        assert_eq!(a.workspace_area(), Rect::new(0, 0, 110, 19));
        assert_eq!(a.frameless(), None);
    }

    fn tap(a: &mut App, x: u16, y: u16) {
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
        ] {
            a.handle(AppEvent::Input(Event::Mouse(MouseEvent {
                kind,
                column: x,
                row: y,
                modifiers: crossterm::event::KeyModifiers::NONE,
            })));
        }
        a.expire_press(Instant::now() + Duration::from_secs(1));
    }

    /// Section 07: the pane menu as a sheet rising from the toolbar, at 34
    /// rows and with the keyboard open (MOBILE_VIEW_MOCK.txt, menu34, menu18).
    #[test]
    fn the_pane_menu_is_a_sheet_on_a_phone() {
        let mut a = phone(34);
        tap(&mut a, 2, 32);
        assert_eq!(a.sheet_layout().unwrap().outer, Rect::new(0, 4, 52, 27));
        assert_eq!(
            a.button_state("touch", crate::toolbar::Slot::Button(0)),
            ButtonState::Active,
            "the button whose sheet is open"
        );
        let s = screen(&a);
        let rows = [
            (5, "│                                                  │"),
            (7, "│  Float                   Fullscreen              │"),
            (11, "│  Group                   Swap with master        │"),
            (15, "│  Synced input            Links on screen         │"),
            (19, "│  Copy mode               Rename                  │"),
            (23, "│  Move to an empty wor…   Move to the scratchpad  │"),
            (27, "│  Close                                           │"),
            (30, "╰──────────────────────────────────────────────────╯"),
        ];
        for (y, want) in rows {
            assert_eq!(s[y], want, "row {y}");
        }
        // Its button again closes it.
        tap(&mut a, 2, 32);
        assert!(a.picker.is_none());

        let mut a = phone(18);
        tap(&mut a, 2, 16);
        assert_eq!(a.sheet_layout().unwrap().outer, Rect::new(0, 1, 52, 14));
        let s = screen(&a);
        assert_eq!(s[4], "│  Float                   Fullscreen              │");
        assert_eq!(
            s[12],
            "│  Synced input            Links on screen         │"
        );
        assert_eq!(
            s[14],
            "╰─────────────────────────────────────── ▾ 5 more ─╯"
        );
        // Swiped up, the rest show; tapped outside, it closes.
        a.handle(AppEvent::Input(Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 10,
            row: 8,
            modifiers: crossterm::event::KeyModifiers::NONE,
        })));
        assert_eq!(
            screen(&a)[4],
            "│  Group                   Swap with master        │"
        );
        tap(&mut a, 10, 0);
        assert!(a.picker.is_none());
    }

    /// Section 07: the workspace switcher, with its counts, the current one
    /// marked, and a typed name offering a new workspace.
    #[test]
    fn the_workspace_switcher_is_a_sheet_on_a_phone() {
        let mut a = phone(34);
        tap(&mut a, 44, 32);
        assert_eq!(a.sheet_layout().unwrap().outer, Rect::new(0, 15, 52, 16));
        let s = screen(&a);
        assert_eq!(
            s[16],
            "│>  type to filter                                 │"
        );
        assert_eq!(
            s[19],
            "│    1:zsh        1 pane   ● 2:vps        4 panes  │"
        );
        assert_eq!(
            s[23],
            "│    3:ai         1 pane     S scratchpad  1 pane  │"
        );
        assert_eq!(
            s[27],
            "│    + new workspace                               │"
        );
        // A tap on a face goes there.
        tap(&mut a, 10, 23);
        assert_eq!(a.current, 3);
        assert!(a.picker.is_none());

        let mut a = phone(18);
        a.run_action(Action::WorkspaceSwitcher);
        for c in "notes".chars() {
            a.handle(AppEvent::Input(Event::Key(KeyEvent::new(
                crossterm::event::KeyCode::Char(c),
                crossterm::event::KeyModifiers::NONE,
            ))));
        }
        let l = a.sheet_layout().unwrap();
        assert_eq!(l.faces.len(), 1);
        assert_eq!(
            screen(&a)[l.faces[0].1.y as usize + 1],
            "│  new workspace: notes                            │"
        );
        a.handle(AppEvent::Input(Event::Key(KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ))));
        assert_eq!(a.workspaces[&a.current].name.as_deref(), Some("notes"));
    }

    /// Section 08: Ctrl latched shows in the mode slot and on its button,
    /// goes with the next key only, and belongs to the terminal that latched.
    #[test]
    fn a_latched_ctrl_shows_and_goes_with_the_next_key() {
        let mut a = phone(34);
        let tap = |a: &mut App, x: u16, y: u16| {
            for kind in [
                MouseEventKind::Down(MouseButton::Left),
                MouseEventKind::Up(MouseButton::Left),
            ] {
                a.handle(AppEvent::Input(Event::Mouse(MouseEvent {
                    kind,
                    column: x,
                    row: y,
                    modifiers: crossterm::event::KeyModifiers::NONE,
                })));
            }
        };
        // The fifth face, ⌃, over its gap column too.
        tap(&mut a, 31, 32);
        assert_eq!(a.latch_label().as_deref(), Some("CTRL"));
        assert_eq!(
            a.button_state("touch", crate::toolbar::Slot::Button(4)),
            ButtonState::Pressed,
            "held a moment after the release"
        );
        a.expire_press(Instant::now() + Duration::from_secs(1));
        assert_eq!(
            a.button_state("touch", crate::toolbar::Slot::Button(4)),
            ButtonState::Latched
        );
        let s = screen(&a);
        // The mock's p34ctrl bar row, exactly (CTRL takes the ⧉ slot).
        assert_eq!(s[29], " CTRL   1:zsh   2:vps   3:ai    S");
        // The desk types: its keys are not the phone's Ctrl.
        a.driven_by(false, false);
        assert_eq!(a.latch_label(), None);
    }

    fn uploading(a: &mut App, id: u64) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        a.paste_seq = id;
        a.pending_paste = Some(super::paste::PendingPaste {
            id,
            pane: 1,
            fallback: "/home/me/a.png".into(),
            held: Vec::new(),
            cancel: cancel.clone(),
        });
        cancel
    }

    fn press(a: &mut App, code: crossterm::event::KeyCode) {
        a.handle(AppEvent::Input(Event::Key(KeyEvent::new(
            code,
            crossterm::event::KeyModifiers::NONE,
        ))));
    }

    /// Keys typed while a paste uploads wait for it, so they cannot land
    /// before its path; an answer for an earlier paste is dropped.
    #[test]
    fn keys_typed_during_an_upload_wait_for_it() {
        let mut a = app(None);
        with_pane(&mut a, 1);
        uploading(&mut a, 7);
        press(&mut a, crossterm::event::KeyCode::Char('x'));
        a.handle(AppEvent::Input(Event::Paste("more".into())));
        assert_eq!(a.pending_paste.as_ref().unwrap().held.len(), 2);
        a.handle(AppEvent::Pasted {
            id: 6,
            result: Ok("/tmp/old.png".into()),
        });
        assert!(
            a.pending_paste.is_some(),
            "a late answer is not this paste's"
        );
        a.handle(AppEvent::Pasted {
            id: 7,
            result: Ok("/tmp/ranma-paste-1000/f.png".into()),
        });
        assert!(a.pending_paste.is_none());
        assert!(a.toasts.is_empty(), "nothing to say when it worked");
    }

    /// The report carries the current workspace's panes, as the strip
    /// names them, for an outer's `pane_strip`.
    #[test]
    fn the_report_carries_the_strip() {
        let mut a = app(None);
        with_pane(&mut a, 1);
        with_pane(&mut a, 2);
        let r = a.own_report();
        let got: Vec<_> = r
            .panes
            .iter()
            .map(|c| (c.label.as_str(), c.focused))
            .collect();
        assert_eq!(got, [("shell", false), ("shell", true)]);
    }

    /// Esc cancels an upload: the thread is told, and the paste is typed as
    /// it came, with a toast saying why.
    #[test]
    fn esc_cancels_an_upload() {
        let mut a = app(None);
        with_pane(&mut a, 1);
        let cancel = uploading(&mut a, 1);
        press(&mut a, crossterm::event::KeyCode::Esc);
        assert!(a.pending_paste.is_none());
        assert!(cancel.load(std::sync::atomic::Ordering::Relaxed));
        assert!(!a.toasts.is_empty());
        // The thread's answer, arriving after, changes nothing.
        a.handle(AppEvent::Pasted {
            id: 1,
            result: Ok("/tmp/x.png".into()),
        });
        assert!(a.pending_paste.is_none());
    }

    /// Under an outer of protocol 1 the unfocused inner draws its bar over its
    /// bottom row, as it always did, and reports in version 1, the only one
    /// that outer reads. A newer outer labels its border instead, so the inner
    /// draws no bar, focused or not.
    #[test]
    fn the_inner_bar_follows_what_the_outer_speaks() {
        let mut a = app(None);
        with_pane(&mut a, 1);
        a.set_outer(Some(1));
        assert!(a.bar_yielded());
        a.host_focused = false;
        assert!(a.bar_overlaid(), "a version-1 outer shows nothing for it");
        assert!(a.bar_rect().is_some());
        assert_eq!(a.own_report().v, 1);

        a.set_outer(Some(crate::nestbar::PROTOCOL));
        a.host_focused = false;
        assert!(!a.bar_overlaid(), "the outer labels the border instead");
        assert_eq!(a.bar_rect(), None);
        assert_eq!(a.own_report().v, crate::nestbar::PROTOCOL);

        // An answer this build does not speak is no outer at all.
        a.set_outer(Some(crate::nestbar::PROTOCOL + 1));
        assert!(!a.bar_yielded());
        assert!(a.bar_rect().is_some());
    }

    /// `theme_colors = "outer"` draws with the colours the outer sent at
    /// attach, and the theme's own again when a terminal without them
    /// attaches; the default, `"own"`, never takes them.
    #[test]
    fn the_outers_colours_are_taken_only_when_asked_for() {
        use crate::theme::Color;
        let mut theirs = app(None).config.theme.colors;
        theirs.border_active = Color::Rgb(9, 8, 7);
        let wire = serde_json::to_value(&theirs).unwrap().as_object().cloned();

        let mut own = app(None);
        let mine = own.config.theme.colors.border_active;
        own.set_outer_colors(wire.clone());
        assert_eq!(own.config.theme.colors.border_active, mine);

        let mut a = app(Some("ranma.set { theme_colors = 'outer' }"));
        a.set_outer_colors(wire.clone());
        assert_eq!(a.config.theme.colors.border_active, Color::Rgb(9, 8, 7));
        // What this ranma tells a ranma inside it is what it draws with.
        let sent = crate::nestbar::colors_osc(&a.config.theme.colors);
        let sent = crate::nestbar::outer_colors_in(&sent).unwrap();
        assert_eq!(sent["border_active"], "#090807");
        a.dirty = false;
        // A plain terminal attaches: its own colours come back.
        a.set_outer_colors(None);
        assert_eq!(a.config.theme.colors.border_active, mine);
        assert!(a.dirty);
    }

    /// Inside another ranma, `paste_image` is the outer one's to do: the
    /// clipboard is on the machine at the keyboard. This one only asks.
    #[test]
    fn a_nested_ranma_asks_the_outer_one_to_paste_the_image() {
        let mut a = app(None);
        with_pane(&mut a, 1);
        a.set_outer(Some(crate::nestbar::PROTOCOL));
        a.host_out.clear();
        a.run_action(Action::PasteImage);
        assert!(a.pending_paste.is_none(), "nothing read here");
        assert_eq!(
            a.host_out,
            vec![crate::nestbar::PASTE_IMAGE.as_bytes().to_vec()]
        );
    }

    /// The request is heard only from a pane a key was just passed to:
    /// printed out of the blue, it would send the clipboard's image there.
    #[test]
    fn a_paste_image_request_nobody_typed_for_is_ignored() {
        let mut a = app(Some(
            r#"ranma.set { paste = { image_command = "exit 1" } }"#,
        ));
        with_pane(&mut a, 1);
        a.paste_image_asked(1);
        assert!(a.pending_paste.is_none());
        a.passed_key = Some((1, Instant::now() - Duration::from_secs(5)));
        a.paste_image_asked(1);
        assert!(a.pending_paste.is_none(), "too long after the key");
        a.passed_key = Some((2, Instant::now()));
        a.paste_image_asked(1);
        assert!(a.pending_paste.is_none(), "a key to another pane");
    }

    /// Pasted paths into a pane that runs no ssh are only text.
    #[test]
    fn an_image_path_into_a_local_pane_is_typed() {
        let mut a = app(None);
        with_pane(&mut a, 1);
        a.handle(AppEvent::Input(Event::Paste("/tmp/a.png".into())));
        assert!(a.pending_paste.is_none());
    }

    /// A leader button does what the leader does, and a second tap undoes it:
    /// Termux has its own Ctrl key, but nothing that types ranma's leader.
    #[test]
    fn a_leader_button_enters_wm_mode_and_leaves_it() {
        let mut a = app(Some(
            r#"ranma.toolbar("t", { show = true, buttons = { { "◆", "leader", text = "leader" } } })"#,
        ));
        with_pane(&mut a, 1);
        let leader = crate::toolbar::Slot::Button(0);
        assert_eq!(a.button_state("t", leader), ButtonState::Normal);
        a.run_button("t", 0);
        assert_eq!(a.mode, Mode::Wm);
        assert_eq!(a.button_state("t", leader), ButtonState::Active);
        assert_eq!(a.wm_chord(), a.config.settings.leader);
        a.run_button("t", 0);
        assert_eq!(a.mode, Mode::Normal);
        // Scripts reach it too: `ranma action leader`.
        assert_eq!("leader".parse::<Action>().unwrap(), Action::Leader);
    }

    /// A sheet open on the phone closes when the desk takes the screen (its
    /// profile changes); a second phone driving keeps it.
    #[test]
    fn a_sheet_goes_with_the_profile_it_was_drawn_for() {
        let mut a = phone(34);
        a.run_action(Action::WorkspaceSwitcher);
        a.driven_by(true, true);
        assert!(a.picker.is_some(), "the same profile");
        a.driven_by(false, false);
        assert!(a.picker.is_none());
    }

    #[test]
    fn a_session_accent_colours_its_border_workspace_and_name() {
        use crate::theme::Color;
        let mut a = app(Some("ranma.session('main', { accent = '#ff0000' })"));
        let base = a.config.theme.colors.clone();
        assert_eq!(a.colors().border_active, Color::Rgb(0xff, 0, 0));
        assert_eq!(a.colors().ws_active_bg, Color::Rgb(0xff, 0, 0));
        assert_eq!(a.colors().mode_bg, base.mode_bg, "WM mode keeps its colour");
        // Set at run time, it wins over the config; `none` gives the config back.
        a.run_action("session_accent 4".parse().unwrap());
        assert_eq!(a.colors().bar_accent, Color::Indexed(4));
        a.run_action("session_accent none".parse().unwrap());
        assert_eq!(a.colors().bar_accent, Color::Rgb(0xff, 0, 0));
        // A session with no accent anywhere draws with the theme.
        assert_eq!(app(None).colors().border_active, base.border_active);
    }

    #[test]
    fn pickers_fit_any_screen_however_small() {
        // A nested ranma in a narrow pane asked to quit panicked here: the
        // question's minimum width was larger than the screen.
        for w in 0..40u16 {
            for h in 0..8u16 {
                let mut a = app(None);
                a.screen = Rect::new(0, 0, w, h);
                a.confirm_quit();
                let l = a.picker_layout().unwrap();
                assert!(l.outer.w <= w && l.outer.h <= h, "{w}x{h}: {:?}", l.outer);
                a.picker = None;
                a.open_palette(crate::picker::PaletteMode::Help);
                let l = a.picker_layout().unwrap();
                assert!(l.outer.w <= w && l.outer.h <= h, "{w}x{h}: {:?}", l.outer);
            }
        }
    }

    #[test]
    fn typing_goes_to_every_marked_pane_only_from_a_marked_one() {
        let mut a = app(None);
        for id in [1, 2, 3] {
            with_pane(&mut a, id);
        }
        a.workspaces.get_mut(&1).unwrap().focused = Some(1);
        assert_eq!(a.typing_targets(), vec![1]);
        a.run_action(Action::SyncToggle);
        a.workspaces.get_mut(&1).unwrap().focused = Some(3);
        a.run_action(Action::SyncToggle);
        // Focused on a marked pane: both marked ones, not the unmarked 2.
        let mut t = a.typing_targets();
        t.sort();
        assert_eq!(t, vec![1, 3]);
        assert_eq!(a.synced_here(), 2);
        // Focused on the unmarked one: only it.
        a.workspaces.get_mut(&1).unwrap().focused = Some(2);
        assert_eq!(a.typing_targets(), vec![2]);
        a.run_action(Action::SyncClear);
        assert_eq!(a.synced_here(), 0);
    }

    #[test]
    fn the_pane_menu_opens_at_the_pointer_on_that_pane() {
        let mut a = app(None);
        with_pane(&mut a, 1);
        with_pane(&mut a, 2);
        a.workspaces.get_mut(&1).unwrap().focused = Some(1);
        a.open_pane_menu(2, 70, 20);
        assert_eq!(a.focused(), Some(2), "the menu is about the pane clicked");
        let p = a.picker.as_ref().unwrap();
        let labels: Vec<String> = p.visible().into_iter().map(|i| i.label).collect();
        assert_eq!(labels.first().map(String::as_str), Some("Float"));
        assert!(labels.iter().any(|l| l == "Close"));
        // Kept on the 80x24 screen however near the corner it was opened.
        let l = a.picker_layout().unwrap();
        assert!(
            l.outer.right() <= 80 && l.outer.bottom() <= 24,
            "{:?}",
            l.outer
        );
        assert!(l.outer.x <= 70 && l.outer.y <= 20);
        // Every entry is an action that parses.
        for it in p.visible() {
            match it.target {
                crate::picker::Target::Run(line) => {
                    assert!(line.parse::<Action>().is_ok(), "{line}")
                }
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn a_right_click_on_a_workspace_opens_its_menu() {
        let mut a = app(None);
        with_pane(&mut a, 1);
        a.workspaces.entry(3).or_default().name = Some("web".into());
        let bar = a.bar_rect().unwrap();
        let (px, _) = a
            .bar_pieces(bar.w)
            .into_iter()
            .find(|(_, p)| p.click == Some(Click::Workspace(3)))
            .expect("workspace 3 has a chip");
        let right = |kind| {
            AppEvent::Input(Event::Mouse(MouseEvent {
                kind,
                column: bar.x + px,
                row: bar.y,
                modifiers: crossterm::event::KeyModifiers::NONE,
            }))
        };
        a.handle(right(MouseEventKind::Down(MouseButton::Right)));
        a.handle(right(MouseEventKind::Up(MouseButton::Right)));
        assert_eq!(a.current, 3, "the menu is about the workspace clicked");
        let p = a.picker.as_ref().expect("a menu");
        assert_eq!(p.title, "workspace 3:web");
        let labels: Vec<String> = p.visible().into_iter().map(|i| i.label).collect();
        assert!(labels.iter().any(|l| l == "Rename"), "{labels:?}");
        assert!(!labels.iter().any(|l| l == "Float"), "not the pane menu");
        // Empty, it has nothing to equalize and nothing to send away.
        assert!(
            !labels
                .iter()
                .any(|l| l == "Equalize" || l.starts_with("Send"))
        );
        for it in p.visible() {
            match it.target {
                crate::picker::Target::Run(line) => {
                    assert!(line.parse::<Action>().is_ok(), "{line}")
                }
                other => panic!("{other:?}"),
            }
        }
        // An occupied workspace can be equalized and sent away.
        a.picker = None;
        a.open_workspace_menu(1, 0, 0);
        let labels: Vec<String> = a
            .picker
            .as_ref()
            .unwrap()
            .visible()
            .into_iter()
            .map(|i| i.label)
            .collect();
        assert!(labels.iter().any(|l| l == "Equalize"), "{labels:?}");
        assert!(labels.iter().any(|l| l.starts_with("Send")), "{labels:?}");
    }

    #[test]
    fn a_bar_message_goes_after_five_seconds() {
        let mut a = app(None);
        let t = Instant::now();
        a.status = Some("paste_image: no clipboard here".into());
        a.run_timers(t);
        // Other timers (the bar's clock) may be due sooner; the loop wakes by then.
        assert!(
            a.next_deadline().is_some_and(|d| d <= t + STATUS_FOR),
            "the loop wakes for it"
        );
        a.run_timers(t + STATUS_FOR - Duration::from_millis(1));
        assert!(a.status.is_some());
        a.run_timers(t + STATUS_FOR);
        assert_eq!(a.status, None);
        assert_eq!(a.status_seen, None);
        // A different message starts its own five seconds.
        a.status = Some("one".into());
        a.run_timers(t);
        a.status = Some("two".into());
        a.run_timers(t + Duration::from_secs(4));
        a.run_timers(t + STATUS_FOR);
        assert_eq!(a.status.as_deref(), Some("two"));
        a.run_timers(t + Duration::from_secs(9));
        assert_eq!(a.status, None);
    }

    #[test]
    fn the_which_key_hint_waits_for_a_pause_and_steps_aside() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut a = app(None);
        let leader = a.config.settings.leader;
        a.enter_wm(leader);
        let due = a.hint_due.expect("due after the pause");
        a.run_timers(due - Duration::from_millis(1));
        assert!(!a.which_key_shown(), "not before the pause");
        a.run_timers(due);
        assert!(a.which_key_shown());
        // A key puts it away; the next pause is twice the first.
        let t = Instant::now();
        a.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));
        assert!(!a.which_key_shown());
        assert!(a.hint_due.unwrap() >= t + Duration::from_millis(999));
        // Something else up (a picker) keeps it away even when due.
        a.run_timers(a.hint_due.unwrap());
        a.open_palette(crate::picker::PaletteMode::Help);
        assert!(!a.which_key_shown());
        // Off in the config: never due.
        let mut off = app(Some("ranma.set { wm_mode = { hint = false } }"));
        off.enter_wm(leader);
        assert!(off.hint_due.is_none());
    }

    fn names(app: &App) -> Vec<Option<String>> {
        app.workspace_list().into_iter().map(|w| w.4).collect()
    }

    #[test]
    fn a_workspace_is_named_after_its_program_until_given_a_name() {
        let mut a = app(None);
        with_pane(&mut a, 5);
        assert_eq!(names(&a), [None], "nothing read yet");
        a.programs.insert(5, "nvim".into());
        assert_eq!(names(&a), [Some("nvim".into())]);
        a.rename_workspace(1, "web");
        assert_eq!(names(&a), [Some("web".into())]);

        let mut a = app(Some("ranma.module('workspaces', { label = 'number' })"));
        with_pane(&mut a, 5);
        a.programs.insert(5, "nvim".into());
        assert_eq!(names(&a), [None]);
    }

    #[test]
    fn programs_are_read_after_events_at_most_every_interval() {
        let mut a = app(None);
        let t = Instant::now();
        a.note_programs(t, false);
        assert_eq!(a.programs_read, Some(t));
        assert_eq!(a.programs_due, None, "a read now needs no timer");
        // Soon after: not read again, but once the interval is up.
        a.note_programs(t + Duration::from_millis(100), false);
        assert_eq!(a.programs_read, Some(t));
        assert_eq!(a.programs_due, Some(t + PROGRAMS_EVERY));
        a.run_timers(t + PROGRAMS_EVERY);
        assert_eq!(a.programs_read, Some(t + PROGRAMS_EVERY));
        // The timer's read sets no new timer: idle stays idle.
        assert_eq!(a.programs_due, None);
        // A key reads now and once more after the interval, and no more.
        let k = t + PROGRAMS_EVERY * 4;
        a.note_programs(k, true);
        assert_eq!(a.programs_read, Some(k));
        assert_eq!(a.programs_due, Some(k + PROGRAMS_EVERY));
        a.run_timers(k + PROGRAMS_EVERY);
        assert_eq!(a.programs_due, None);
    }

    fn title(a: &mut App) -> String {
        a.host_title.clear();
        a.host_out.clear();
        a.announce();
        a.host_title.clone()
    }

    #[test]
    fn the_title_names_this_host_when_the_client_came_over_ssh() {
        let host = crate::pane::hostname();
        let mut a = app(None);
        assert_eq!(title(&mut a), crate::pane::NESTED_MARKER);
        a.client_remote = true;
        assert_eq!(title(&mut a), format!("⧉ ranma@{host}"));
        assert!(String::from_utf8_lossy(&a.host_out.concat()).contains(&format!("ranma@{host}")));

        let mut a = app(Some("ranma.set { title_host = 'always' }"));
        assert_eq!(title(&mut a), format!("⧉ ranma@{host}"));
        let mut a = app(Some("ranma.set { title_host = 'never' }"));
        a.client_remote = true;
        assert_eq!(title(&mut a), crate::pane::NESTED_MARKER);
    }

    #[test]
    fn timers_fire_and_a_failing_repeating_one_stops() {
        let mut a = app(Some(
            r#"
            ranma.defer(0, function() ranma.notify("deferred") end)
            local dropped = ranma.defer(0, function() ranma.notify("cancelled ran") end)
            ranma.cancel(dropped)
            "#,
        ));
        a.run_timers(Instant::now() + Duration::from_millis(1));
        assert_eq!(a.status.as_deref(), Some("deferred"));
        assert_eq!(a.config.jobs.next_due(), None, "a deferred timer runs once");

        let mut a = app(Some(r#"ranma.every(50, function() error("broken") end)"#));
        a.run_timers(Instant::now() + Duration::from_millis(60));
        let status = a.status.clone().unwrap();
        assert!(
            status.contains("broken") && status.contains("stopped"),
            "{status}"
        );
        assert_eq!(a.config.jobs.next_due(), None);
    }

    #[test]
    fn a_spawned_process_calls_back_with_its_lines_and_its_end() {
        let config = crate::config::load_from(None, None, None).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut a = App::new(config, tx, 80, 24);
        a.call_lua(|lua| {
            lua.load(
                r#"
                got = {}
                ranma.spawn({ "sh", "-c", "echo one; echo two; exit 4" }, {
                  on_line = function(l) table.insert(got, l) end,
                  on_exit = function(r)
                    ranma.notify(table.concat(got, ",") .. " exit " .. r.code)
                  end,
                })
                "#,
            )
            .exec()
        })
        .unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        while a.status.as_deref().is_none_or(|s| !s.contains("exit")) && Instant::now() < until {
            if let Ok(ev) = rx.recv_timeout(Duration::from_millis(100)) {
                a.handle(ev);
            }
        }
        assert_eq!(a.status.as_deref(), Some("one,two exit 4"));
    }
}

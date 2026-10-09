//! Loading `init.lua`: the built-in defaults first, then the user's file on top.
//!
//! The Lua state outlives loading. Binds and hooks can be Lua functions, and those
//! live in the state's registry, so [`Config`] owns the `Lua` that created them.
//! That also means Lua only ever runs on events, never per frame: the renderer reads
//! the plain Rust fields below and never calls into Lua.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result};
use mlua::{Function, Lua, LuaSerdeExt, RegistryKey, Table, Value};
use serde::Deserialize;
use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::action::Action;
use crate::keys::Chord;
use crate::theme::{self, Theme};

pub const DEFAULT_INIT_LUA: &str = include_str!("../assets/init.lua");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    Dwindle,
    Manual,
    /// One master pane on the left, the rest stacked on the right.
    Master,
    /// One tiled pane on screen at a time, the others as tabs above it: the
    /// whole workspace drawn as a tabbed group. The tree is kept as it is, so
    /// another layout gives the tiling back.
    Monocle,
    /// niri's: a strip of columns wider than the screen, the view scrolling
    /// to the focused one (`crate::strip`).
    Scrolling,
}

/// What the mouse does outside WM mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MouseMode {
    /// Clicking a pane focuses it; clicks and wheel go on to programs that ask.
    Click,
    /// Focus follows the pointer.
    Hover,
    /// The mouse belongs to the host terminal (its selection, its scrolling);
    /// ranma only sees it in WM mode.
    Off,
}

/// Whether ranma steps aside for a ranma running in the focused pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NestedMode {
    /// Announce ourselves in the host's title, and pass every key to a ranma
    /// found in the focused pane.
    Auto,
    Off,
}

/// Which workspaces the workspaces module expands into the workspaces of the
/// ranma in their focused pane (see `nestbar`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NestedWorkspaces {
    /// Only the one on your path: the current workspace's.
    #[default]
    Focused,
    All,
    /// None: every workspace as it always was.
    Off,
}

/// Whose colours this ranma draws with when it runs inside another ranma
/// (DESIGN.md, "An inner ranma in the outer's colours").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeColors {
    /// Its own theme's, always.
    Own,
    /// The `[colors]` of the ranma around it, sent when the terminal
    /// attached; its own theme's anywhere else.
    Outer,
}

/// When ranma's title names the host it runs on (`⧉ ranma@vps · nvim`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TitleHost {
    /// When the terminal showing this ranma reached it over SSH.
    Ssh,
    Always,
    Never,
}

/// Whether a server keeps a snapshot of itself and offers it after a reboot
/// (DESIGN.md, "Layouts").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RestoreMode {
    /// Write snapshots; a fresh server asks whether to bring the last back.
    Ask,
    /// Neither write nor ask.
    Off,
}

/// Whether a pane stays when its program ends (tmux's `remain-on-exit`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RemainOnExit {
    /// The pane closes with its program, as it always did.
    Off,
    /// It stays when the program failed: a non-zero status, or a signal.
    Failed,
    /// It always stays.
    On,
}

impl RemainOnExit {
    /// Whether a program that ended with this status leaves its pane.
    pub fn keeps(self, code: Option<i32>) -> bool {
        match self {
            RemainOnExit::Off => false,
            RemainOnExit::Failed => code != Some(0),
            RemainOnExit::On => true,
        }
    }
}

/// What ranma does when its source has moved on (see `update`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateMode {
    /// A toast and a bar marker; `leader U` installs.
    Remind,
    /// Ask y/n, as oh-my-zsh does.
    Prompt,
    /// Never check.
    Off,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub leader: Chord,
    pub theme: String,
    pub layout: Layout,
    /// With `layout = "master"`: the master's share of the width, for a new
    /// master area (a resized one keeps its size).
    pub master_ratio: f32,
    /// With `layout = "scrolling"`: the widths `column_width next` steps
    /// through, a new column's width, the narrowest column, and when the
    /// view centres the focused column.
    pub scroll_widths: Vec<crate::strip::Width>,
    pub scroll_width: crate::strip::Width,
    pub scroll_min: u16,
    pub scroll_center: crate::strip::Center,
    pub preserve_split: bool,
    pub shell: Option<String>,
    pub scrollback_lines: usize,
    pub wm_mode_sticky: bool,
    /// How long WM mode waits before showing the which-key hint; `None` is off.
    pub wm_mode_hint: Option<std::time::Duration>,
    pub mouse: MouseMode,
    pub updates: UpdateMode,
    pub update_check_hours: f64,
    pub nested: NestedMode,
    /// Reaches this ranma even while it passes keys to one inside it; pressed
    /// again in WM mode, it goes one level down.
    pub outer_leader: Chord,
    pub title_host: TitleHost,
    pub theme_colors: ThemeColors,
    pub restore: RestoreMode,
    /// A paste that is nothing but paths of local files, into a pane running
    /// ssh, is uploaded first.
    pub paste_upload: bool,
    /// Writes the clipboard's image as PNG to stdout; `None` is the platform's.
    pub paste_image_command: Option<String>,
    /// An empty workspace shows the logo and how to start (Enter, help).
    pub splash: bool,
    /// How long a pane that printed must stay quiet for `pane_idle`.
    pub pane_idle: std::time::Duration,
    /// Mark a workspace in the bar when a pane in it prints while it is not
    /// shown (tmux's `monitor-activity`).
    pub monitor_activity: bool,
    /// How long a pane watched with `monitor_silence` must stay quiet after
    /// printing before it says so.
    pub monitor_silence: std::time::Duration,
    pub remain_on_exit: RemainOnExit,
}

impl Default for Settings {
    // Only a starting point for the builder: assets/init.lua sets every field, and
    // that file, not this impl, is the documented default.
    fn default() -> Self {
        Settings {
            leader: "ctrl+b".parse().unwrap(),
            theme: theme::DEFAULT_THEME_NAME.into(),
            layout: Layout::Dwindle,
            master_ratio: 0.55,
            scroll_widths: crate::strip::DEFAULT_WIDTHS.to_vec(),
            scroll_width: crate::strip::Width::Frac(1, 2),
            scroll_min: 40,
            scroll_center: crate::strip::Center::Never,
            preserve_split: true,
            shell: None,
            scrollback_lines: 10_000,
            wm_mode_sticky: true,
            wm_mode_hint: Some(std::time::Duration::from_millis(500)),
            mouse: MouseMode::Click,
            updates: UpdateMode::Remind,
            update_check_hours: 24.0,
            nested: NestedMode::Auto,
            outer_leader: "ctrl+alt+b".parse().unwrap(),
            title_host: TitleHost::Ssh,
            theme_colors: ThemeColors::Own,
            restore: RestoreMode::Ask,
            paste_upload: true,
            paste_image_command: None,
            splash: true,
            pane_idle: std::time::Duration::from_secs(5),
            monitor_activity: false,
            monitor_silence: std::time::Duration::from_secs(10),
            remain_on_exit: RemainOnExit::Off,
        }
    }
}

/// One `ranma.set { ... }` call. Every field is optional because each call only
/// changes what it names; unknown fields are errors so typos surface at load.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsPatch {
    leader: Option<String>,
    theme: Option<String>,
    layout: Option<Layout>,
    master_ratio: Option<f32>,
    scroll_widths: Option<Vec<crate::strip::WidthSetting>>,
    scroll_width: Option<crate::strip::WidthSetting>,
    scroll_min: Option<u16>,
    scroll_center: Option<crate::strip::Center>,
    preserve_split: Option<bool>,
    shell: Option<String>,
    scrollback_lines: Option<usize>,
    wm_mode: Option<WmModePatch>,
    mouse: Option<MouseMode>,
    updates: Option<UpdateMode>,
    update_check_hours: Option<f64>,
    nested: Option<NestedMode>,
    outer_leader: Option<String>,
    title_host: Option<TitleHost>,
    theme_colors: Option<ThemeColors>,
    restore: Option<RestoreMode>,
    paste: Option<PastePatch>,
    splash: Option<bool>,
    pane_idle: Option<f64>,
    monitor_activity: Option<bool>,
    monitor_silence: Option<f64>,
    remain_on_exit: Option<RemainOnExit>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PastePatch {
    upload: Option<bool>,
    image_command: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct WmModePatch {
    sticky: Option<bool>,
    /// Seconds before the hint shows, or false for none.
    hint: Option<HintSetting>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum HintSetting {
    On(bool),
    After(f64),
}

#[derive(Debug, Clone)]
pub enum BindAction {
    Builtin(Action),
    /// Shared, so the builder can be copied before a plugin runs and the
    /// copy put back if the plugin fails.
    Lua(Rc<RegistryKey>),
    /// A key with keys behind it (`{ folder = NAME }`): pressing it opens
    /// them in WM mode, and the which-key hint shows them instead of the
    /// top level.
    Folder(Folder),
}

/// A folder's name and its keys, which may be folders in turn.
#[derive(Debug, Clone)]
pub struct Folder {
    pub name: String,
    pub binds: HashMap<Chord, Bind>,
}

#[derive(Debug, Clone)]
pub struct Bind {
    pub action: BindAction,
    /// Whether WM mode ends after this bind fires.
    pub exits_mode: bool,
    /// `{ exit = ... }` as written: when given, it decides alone, over
    /// `wm_mode.sticky` too (a resize that stays in WM mode while every other
    /// bind is one-shot).
    pub exit: Option<bool>,
    /// The action as written, for `--check-config` and error messages.
    pub label: String,
    /// A short name for the which-key hint (`{ desc = "..." }`); a Lua bind
    /// has no action to name it by otherwise.
    pub desc: Option<String>,
    /// The which-key heading it is listed under (`{ group = "..." }`).
    pub group: Option<String>,
}

impl Bind {
    /// The folder this key opens, when it is one.
    pub fn folder(&self) -> Option<&Folder> {
        match &self.action {
            BindAction::Folder(f) => Some(f),
            _ => None,
        }
    }
}

/// The keys behind `path` (chords after the leader, each but the last a
/// folder): the folder the path ends at, or `None` when it does not lead to
/// one. An empty path is `top` itself.
pub fn folder_binds<'a>(
    top: &'a HashMap<Chord, Bind>,
    path: &[Chord],
) -> Option<&'a HashMap<Chord, Bind>> {
    let mut table = top;
    for c in path {
        table = &table.get(c)?.folder()?.binds;
    }
    Some(table)
}

/// A mode of the user's (`ranma.mode`): WM mode with a key table of its own,
/// entered with the `mode` action.
#[derive(Clone)]
pub struct UserMode {
    /// What the bar's mode module says while it is on (` RESIZE `).
    pub label: String,
    pub binds: HashMap<Chord, Bind>,
    /// Stay in it after a bind fires (unless the bind says `exit`); Esc and
    /// Enter leave.
    pub sticky: bool,
    pub on_enter: Option<Rc<RegistryKey>>,
    pub on_exit: Option<Rc<RegistryKey>>,
}

/// A command of the user's (`ranma.command`), run from the palette.
#[derive(Clone)]
pub struct UserCommand {
    pub func: Rc<RegistryKey>,
    pub desc: Option<String>,
    /// What it takes, as the palette shows it (`<name>`, `[n]`).
    pub args: Option<String>,
    /// Its argument's values: a list, or a function returning one.
    pub complete: Option<Rc<RegistryKey>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Event {
    PaneOpen,
    PaneClose,
    FocusChange,
    WorkspaceChange,
    SessionSwitch,
    ModeChange,
    ConfigReload,
    /// A shell said a command finished (OSC 133; see `osc`).
    CommandFinished,
    /// A terminal started driving the screen (DESIGN.md, "Several terminals on
    /// one server"): its size, and whether it is a phone.
    DriverChange,
    /// A shell said a command started (OSC 133 C).
    CommandStarted,
    /// A pane's shell moved to another directory (OSC 7, or seen in /proc
    /// when a command finished).
    CwdChange,
    /// A pane's program set a different title.
    TitleChange,
    /// A pane rang the bell.
    Bell,
    /// A pane that was printing has printed nothing for `pane_idle` seconds.
    PaneIdle,
    /// The settings panel changed an option's value (live, saved or not).
    OptionChange,
    /// The pointer came to rest on another cell of a pane.
    Hover,
    /// A click with Ctrl or Alt held in a pane's text: the hook's, not the
    /// program's.
    Click,
}

/// Every event by the name `ranma.on` takes.
pub const EVENTS: [(&str, Event); 17] = [
    ("pane_open", Event::PaneOpen),
    ("pane_close", Event::PaneClose),
    ("focus_change", Event::FocusChange),
    ("workspace_change", Event::WorkspaceChange),
    ("session_switch", Event::SessionSwitch),
    ("mode_change", Event::ModeChange),
    ("config_reload", Event::ConfigReload),
    ("command_finished", Event::CommandFinished),
    ("driver_change", Event::DriverChange),
    ("command_started", Event::CommandStarted),
    ("cwd_change", Event::CwdChange),
    ("title_change", Event::TitleChange),
    ("bell", Event::Bell),
    ("pane_idle", Event::PaneIdle),
    ("hover", Event::Hover),
    ("option_change", Event::OptionChange),
    ("click", Event::Click),
];

impl Event {
    pub fn name(self) -> &'static str {
        EVENTS
            .iter()
            .find(|(_, e)| *e == self)
            .map(|(n, _)| *n)
            .expect("every event is in EVENTS")
    }
}

impl FromStr for Event {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        EVENTS
            .iter()
            .find(|(n, _)| *n == s)
            .map(|(_, e)| *e)
            .ok_or_else(|| format!("unknown event `{s}`"))
    }
}

/// Modules ranma draws itself. Anything else in a bar list must be defined with
/// `ranma.module`.
pub const BUILTIN_MODULES: [&str; 7] = [
    "mode",
    "session",
    "workspaces",
    "title",
    "panes",
    "pane_strip",
    "update",
];

/// Built-in modules that read the machine on a timer (see `sysstat`), with
/// their default interval in seconds. Named in the bar, they are defined with
/// these defaults; `ranma.module` changes only `interval` and `format`.
pub const SYSTEM_MODULES: [(&str, f64); 2] = [("cpu", 2.0), ("mem", 5.0)];

/// Which modules the bar shows, in order, on each side.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BarLayout {
    pub left: Vec<String>,
    pub center: Vec<String>,
    pub right: Vec<String>,
    /// `large`: three rows, chips on the middle one (DESIGN.md, "A mobile view").
    pub size: crate::toolbar::Size,
}

impl BarLayout {
    pub fn all(&self) -> impl Iterator<Item = &String> {
        self.left.iter().chain(&self.center).chain(&self.right)
    }
}

#[derive(Debug, Clone)]
pub enum ModuleKind {
    /// A Lua function returning a string or `{ text = ..., style = ... }`.
    Lua(Rc<RegistryKey>),
    /// A shell command; its first line of output is the text.
    Exec {
        command: String,
        /// `%s` is replaced by the output line.
        format: Option<String>,
    },
    /// CPU usage since the last tick, from /proc/stat. `%s` is `12%`.
    Cpu { format: Option<String> },
    /// Memory in use, from /proc/meminfo. `%s` is `24.1G`.
    Mem { format: Option<String> },
}

impl ModuleKind {
    fn system(name: &str, format: Option<String>) -> Option<ModuleKind> {
        match name {
            "cpu" => Some(ModuleKind::Cpu { format }),
            "mem" => Some(ModuleKind::Mem { format }),
            _ => None,
        }
    }
}

fn system_module(name: &str, interval: Option<f64>, format: Option<String>) -> Option<ModuleDef> {
    let default = SYSTEM_MODULES.iter().find(|(n, _)| *n == name)?.1;
    Some(ModuleDef {
        interval: Some(std::time::Duration::from_secs_f64(
            interval.unwrap_or(default),
        )),
        kind: ModuleKind::system(name, format)?,
    })
}

#[derive(Debug, Clone)]
pub struct ModuleDef {
    /// How often to refresh. `None` for a Lua module means "when ranma's state
    /// changes" (focus, workspace, title, mode) instead of on a timer.
    pub interval: Option<std::time::Duration>,
    pub kind: ModuleKind,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct BarPatch {
    left: Option<Vec<String>>,
    center: Option<Vec<String>>,
    right: Option<Vec<String>>,
    size: Option<crate::toolbar::Size>,
}

/// `ranma.profile(name, def)`: overrides applied over the base configuration
/// while the profile is in use (DESIGN.md, "A mobile view"). An overlay, never
/// an edit of the base, so going back is exact.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    #[serde(default)]
    set: SettingsPatch,
    #[serde(default)]
    bar: BarPatch,
    /// The toolbars shown while it is in use, instead of the base's.
    #[serde(default)]
    toolbars: Option<Vec<String>>,
}

impl Profile {
    fn modules(&self) -> impl Iterator<Item = &String> {
        [&self.bar.left, &self.bar.center, &self.bar.right]
            .into_iter()
            .flatten()
            .flatten()
    }
}

/// `ranma.toolbar(name, def)`: a row of buttons (DESIGN.md, "A mobile view").
#[derive(Debug, Clone)]
pub struct ToolbarDef {
    pub position: crate::toolbar::Position,
    pub size: crate::toolbar::Size,
    pub buttons: Vec<Button>,
}

#[derive(Debug, Clone)]
pub struct Button {
    pub label: crate::toolbar::Label,
    pub action: BindAction,
}

impl Button {
    /// The action a button runs, when it is one ranma knows (not Lua).
    pub fn builtin(&self) -> Option<&Action> {
        match &self.action {
            BindAction::Builtin(a) => Some(a),
            BindAction::Lua(_) | BindAction::Folder(_) => None,
        }
    }
}

/// Parse `ranma.toolbar`'s definition, strictly.
fn parse_toolbar(lua: &Lua, name: &str, def: &Table) -> mlua::Result<(ToolbarDef, bool)> {
    let who = format!("ranma.toolbar(\"{name}\")");
    let mut position = crate::toolbar::Position::Bottom;
    let mut size = crate::toolbar::Size::Normal;
    let mut show = false;
    let mut buttons = None;
    for pair in def.pairs::<String, Value>() {
        let (k, v) = pair?;
        match k.as_str() {
            "position" | "size" => {
                let v: String = lua
                    .from_value(v)
                    .map_err(|_| rt_err(format!("{who}: `{k}` must be a string")))?;
                let bad =
                    |expected: &str| rt_err(format!("{who}: {k} = \"{v}\" (expected {expected})"));
                if k == "position" {
                    position = lua
                        .from_value(Value::String(lua.create_string(&v)?))
                        .map_err(|_| bad("top, bottom or beside"))?;
                } else {
                    size = lua
                        .from_value(Value::String(lua.create_string(&v)?))
                        .map_err(|_| bad("normal or large"))?;
                }
            }
            "show" => match v {
                Value::Boolean(b) => show = b,
                other => {
                    return Err(rt_err(format!(
                        "{who}: `show` must be true or false, not {}",
                        other.type_name()
                    )));
                }
            },
            "buttons" => match v {
                Value::Table(t) => buttons = Some(t),
                other => {
                    return Err(rt_err(format!(
                        "{who}: `buttons` must be a list, not {}",
                        other.type_name()
                    )));
                }
            },
            _ => {
                return Err(rt_err(format!(
                    "{who}: unknown option `{k}` (expected position, size, show, buttons)"
                )));
            }
        }
    }
    let list = buttons.ok_or_else(|| rt_err(format!("{who}: `buttons` is required")))?;
    let mut out = Vec::new();
    for (i, entry) in list.sequence_values::<Value>().enumerate() {
        let n = i + 1;
        let Value::Table(b) = entry? else {
            return Err(rt_err(format!(
                "{who}: button {n} must be a table like {{ \"+\", \"new_pane\" }}"
            )));
        };
        let mut label = None;
        let mut action = None;
        let mut text = None;
        for pair in b.pairs::<Value, Value>() {
            let (k, v) = pair?;
            match (&k, v) {
                (Value::Integer(1), Value::String(s)) => label = Some(s.to_str()?.to_string()),
                (Value::Integer(2), Value::String(s)) => {
                    let s = s.to_str()?.to_string();
                    let a: Action = s
                        .parse()
                        .map_err(|e| rt_err(format!("{who}: button {n}: {e}")))?;
                    action = Some(BindAction::Builtin(a));
                }
                (Value::Integer(2), Value::Function(f)) => {
                    action = Some(BindAction::Lua(Rc::new(lua.create_registry_value(f)?)));
                }
                (Value::String(s), Value::String(t)) if s.to_str()? == "text" => {
                    text = Some(t.to_str()?.to_string());
                }
                (k, v) => {
                    let k = match k {
                        Value::String(s) => s.to_str()?.to_string(),
                        other => format!("{other:?}"),
                    };
                    return Err(rt_err(format!(
                        "{who}: button {n}: unexpected `{k}` ({}); a button is {{ label, action, text = \"...\" }}",
                        v.type_name()
                    )));
                }
            }
        }
        let (Some(label), Some(action)) = (label, action) else {
            return Err(rt_err(format!(
                "{who}: button {n} needs a label and an action (a string or a function)"
            )));
        };
        if label.is_empty() {
            return Err(rt_err(format!("{who}: button {n} has an empty label")));
        }
        out.push(Button {
            label: crate::toolbar::Label { label, text },
            action,
        });
    }
    if out.is_empty() {
        return Err(rt_err(format!("{who}: `buttons` is empty")));
    }
    Ok((
        ToolbarDef {
            position,
            size,
            buttons: out,
        },
        show,
    ))
}

/// A window rule: what to do with a pane whose command or title matches.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    /// Glob (`*`, `?`) matched against the command an `exec` pane was opened with.
    pub command: Option<String>,
    /// Glob matched against the pane's title, the first time the title matches.
    pub title: Option<String>,
    pub float: bool,
    /// Floating size in percent of the workspace, width and height.
    pub size: Option<(u16, u16)>,
    pub workspace: Option<u8>,
    /// With `workspace`: send it there without following.
    pub silent: bool,
}

impl Rule {
    pub fn matches_command(&self, cmd: &str) -> bool {
        self.command.as_deref().is_some_and(|g| glob(g, cmd))
    }
    pub fn matches_title(&self, title: &str) -> bool {
        self.title.as_deref().is_some_and(|g| glob(g, title))
    }
}

/// `*` matches any run of characters, `?` any one; everything else is literal.
/// Case-sensitive, like the commands and titles it matches.
pub fn glob(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    // Iterative matcher with one backtrack point: linear enough for short globs.
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(sp) = star {
            pi = sp + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleSpec {
    command: Option<String>,
    title: Option<String>,
    float: Option<bool>,
    size: Option<Vec<u16>>,
    workspace: Option<u8>,
    silent: Option<bool>,
}

/// What `ranma.state()` answers with, filled in by the window manager before it
/// calls into Lua.
#[derive(Debug, Clone, Default)]
pub struct StateSnapshot {
    pub session: String,
    pub sessions: Vec<String>,
    pub workspace: u8,
    pub workspaces: Vec<u8>,
    pub focused: Option<u64>,
    pub title: String,
    pub mode: &'static str,
    pub panes: usize,
    pub client: ClientFacts,
}

/// What `ranma.client()` answers with: the terminal driving the screen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClientFacts {
    pub cols: u16,
    pub rows: u16,
    pub mobile: bool,
    pub remote: bool,
    /// A ranma around that terminal answered at attach (see `nestbar`).
    pub outer: bool,
}

impl ClientFacts {
    pub fn fill(&self, t: &Table) -> mlua::Result<()> {
        t.set("cols", self.cols)?;
        t.set("rows", self.rows)?;
        t.set("mobile", self.mobile)?;
        t.set("remote", self.remote)?;
        t.set("outer", self.outer)
    }
}

/// Present in the Lua state only while ranma is calling a bind, hook or module:
/// what `ranma.action`, `ranma.notify` and `ranma.state` read and write.
#[derive(Debug, Default)]
pub struct Runtime {
    /// What the call asked ranma to do, in the order asked.
    pub ops: Vec<Op>,
    /// Every pane, for `ranma.pane` and `ranma.panes`.
    pub panes: Vec<crate::luapane::PaneEntry>,
    pub notify: Option<String>,
    /// `ranma.toast` calls: text, urgent, timeout in seconds.
    pub toasts: Vec<(String, bool, Option<f64>)>,
    pub state: StateSnapshot,
}

/// One thing a Lua call asked for, done after it returns.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Action(Action),
    Pane(crate::layout::PaneId, crate::luapane::PaneRequest),
    Spawn(u64, crate::jobs::SpawnSpec),
    Kill(u64),
    Picker(crate::luaui::PickerSpec),
    Input(crate::luaui::InputSpec),
    Screen(Box<crate::luascreen::ScreenSpec>),
    ScreenSet(u64, Box<crate::luascreen::Update>),
    ScreenClose(u64),
    Tooltip(Option<crate::luaui::TooltipSpec>),
    /// Put text on the clipboard of the terminal driving ranma (OSC 52).
    Copy(String),
    /// Stop a `pane:watch`.
    Unwatch(u64),
}

/// What the `ranma` global writes into while the config runs.
#[derive(Default, Clone)]
struct Builder {
    modes: HashMap<String, UserMode>,
    commands: std::collections::BTreeMap<String, UserCommand>,
    settings: Settings,
    binds: HashMap<Chord, Bind>,
    /// Binds and unbinds inside folders (`"g s"`), in the order made: put
    /// in their folders when the whole configuration has loaded.
    nested: Vec<(String, Vec<Chord>, Option<Bind>)>,
    /// Group names, in the order binds first named them.
    group_order: Vec<String>,
    global_binds: HashMap<Chord, Bind>,
    hooks: HashMap<Event, Vec<Rc<RegistryKey>>>,
    bar: BarLayout,
    modules: HashMap<String, ModuleDef>,
    workspaces_show_all: bool,
    workspaces_numbers_only: bool,
    workspaces_nested: NestedWorkspaces,
    title_nested: bool,
    rules: Vec<Rule>,
    session_accents: HashMap<String, theme::Color>,
    profiles: HashMap<String, Profile>,
    toolbars: Vec<(String, ToolbarDef)>,
    toolbars_shown: Vec<String>,
    layouts: std::collections::BTreeMap<String, crate::layouts::Spec>,
    /// `ranma.defer` and `ranma.every` made while loading, started with it.
    timers: crate::jobs::PendingTimers,
    /// `ranma.on("user:<name>", fn)`, by name.
    user_hooks: HashMap<String, Vec<Rc<RegistryKey>>>,
    /// Every `ranma.set` so far, merged as TOML: what the settings panel
    /// reads values from (`crate::options`).
    set_table: toml::Table,
    /// `set_table` as the built-in defaults left it, plus each plugin
    /// option's default.
    default_set: toml::Table,
    /// `ranma.option` declarations.
    plugin_options: Vec<crate::options::Opt>,
}

pub struct Config {
    /// `ranma.mode`s by name.
    pub modes: HashMap<String, UserMode>,
    /// `ranma.command`s by name.
    pub commands: std::collections::BTreeMap<String, UserCommand>,
    pub settings: Settings,
    /// Keys looked up in WM mode, after the leader.
    pub binds: HashMap<Chord, Bind>,
    /// The which-key groups binds name (`{ group = "..." }`), in the order
    /// the configuration first names them.
    pub group_order: Vec<String>,
    /// Keys looked up outside WM mode, before the program sees them.
    pub global_binds: HashMap<Chord, Bind>,
    pub hooks: HashMap<Event, Vec<Rc<RegistryKey>>>,
    pub bar: BarLayout,
    pub modules: HashMap<String, ModuleDef>,
    /// The workspaces module shows 1-10 even when empty.
    pub workspaces_show_all: bool,
    /// The workspaces module shows ` 3 ` for an unnamed workspace, not
    /// ` 3:nvim ` (the program in its focused pane).
    pub workspaces_numbers_only: bool,
    /// Which workspaces show the workspaces of a ranma inside them.
    pub workspaces_nested: NestedWorkspaces,
    /// The `title` module shows the focused pane's title when that pane is a
    /// ranma reporting to this one (which shows it on its own border too).
    pub title_nested: bool,
    /// In the order written; every matching rule applies, later ones last.
    pub rules: Vec<Rule>,
    /// `ranma.session(name, { accent = ... })`: a session's colour, by name.
    pub session_accents: HashMap<String, theme::Color>,
    /// `ranma.layout` definitions, by name (DESIGN.md, "Layouts").
    pub layouts: std::collections::BTreeMap<String, crate::layouts::Spec>,
    pub theme: Theme,
    /// `ranma.profile` definitions, by name.
    pub profiles: HashMap<String, Profile>,
    /// The profile in use, applied over `base`.
    pub profile: Option<String>,
    /// The settings, bar and shown toolbars as `init.lua` left them, before
    /// any profile.
    base: (Settings, BarLayout, Vec<String>),
    /// `ranma.toolbar` definitions, in the order written.
    pub toolbars: Vec<(String, ToolbarDef)>,
    /// The toolbars shown now, by name, in definition order.
    pub toolbars_shown: Vec<String>,
    /// The user's init.lua, if one was found and run.
    pub source: Option<PathBuf>,
    /// Every plugin file found, in the order sourced, failed ones included.
    pub plugins: Vec<PluginLoad>,
    /// What loaded but deserves a word: settings.toml naming an option
    /// nothing declares.
    pub warnings: Vec<String>,
    /// Every option and the layers its value comes from (`crate::options`).
    pub values: Values,
    /// Stops a Lua call that runs too long; armed by every call into `lua`.
    pub watchdog: Watchdog,
    /// Its timers and the processes it spawned (`crate::jobs`).
    pub jobs: crate::jobs::Jobs,
    /// `ranma.on("user:<name>")` listeners.
    user_events: UserEvents,
    /// Owns every Lua function referenced by `binds` and `hooks`.
    pub lua: Lua,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("settings", &self.settings)
            .field("binds", &self.binds.len())
            .field("theme", &self.theme.name)
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

impl Config {
    /// Each plugin event with listeners, and how many, by name.
    pub fn user_hooks(&self) -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = self
            .user_events
            .hooks
            .borrow()
            .iter()
            .map(|(n, v)| (n.clone(), v.len()))
            .collect();
        out.sort();
        out
    }

    /// Use the profile `name` over the base configuration, or none: the
    /// settings and bar are rebuilt from the base each time, so nothing a
    /// profile changed outlives it.
    pub fn toolbar(&self, name: &str) -> Option<&ToolbarDef> {
        self.toolbars
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, t)| t)
    }

    /// Show, hide or flip a toolbar until the next profile switch.
    pub fn show_toolbar(&mut self, name: &str, show: Option<bool>) -> Result<(), String> {
        if self.toolbar(name).is_none() {
            let known: Vec<&str> = self.toolbars.iter().map(|(n, _)| n.as_str()).collect();
            return Err(format!(
                "no toolbar `{name}` (defined: {})",
                if known.is_empty() {
                    "none".into()
                } else {
                    known.join(", ")
                }
            ));
        }
        let shown = self.toolbars_shown.iter().any(|n| n == name);
        let want = show.unwrap_or(!shown);
        self.toolbars_shown.retain(|n| n != name);
        if want {
            self.toolbars_shown.push(name.to_string());
            let defs = &self.toolbars;
            self.toolbars_shown
                .sort_by_key(|s| defs.iter().position(|(n, _)| n == s));
        }
        Ok(())
    }

    pub fn use_profile(&mut self, name: Option<&str>) -> Result<(), String> {
        let (mut settings, mut bar, mut toolbars) = self.base.clone();
        if let Some(name) = name {
            let Some(p) = self.profiles.get(name) else {
                let mut known: Vec<&str> = self.profiles.keys().map(String::as_str).collect();
                known.sort_unstable();
                return Err(format!(
                    "no profile `{name}` (defined: {})",
                    if known.is_empty() {
                        "none".into()
                    } else {
                        known.join(", ")
                    }
                ));
            };
            apply_settings(&mut settings, p.set.clone(), &format!("profile `{name}`"))?;
            apply_bar(&mut bar, p.bar.clone());
            if let Some(t) = &p.toolbars {
                toolbars = t.clone();
            }
        }
        self.settings = settings;
        self.bar = bar;
        self.toolbars_shown = toolbars;
        self.profile = name.map(str::to_string);
        Ok(())
    }
}

/// `$RANMA_CONFIG_DIR`, else `$XDG_CONFIG_HOME/ranma` (`~/.config/ranma`).
pub fn config_dir() -> Option<PathBuf> {
    std::env::var_os("RANMA_CONFIG_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::config_dir().map(|d| d.join("ranma")))
}

fn rt_err(msg: impl Into<String>) -> mlua::Error {
    mlua::Error::RuntimeError(msg.into())
}

/// The error for a function that builds the configuration called once it has
/// been built: from a bind, hook, module or timer. The traceback names which.
/// One bind as `ranma.bind` spells it, and each of a `ranma.mode`'s: the
/// keys (one chord, or `"g s"`: chords inside the folder on `g`), the bind,
/// and whether it is global (`global_ok`: only `ranma.bind` takes that
/// option, and folders).
fn parse_bind(
    lua: &Lua,
    keys: &str,
    action: Value,
    opts: Option<&Table>,
    who: &str,
    global_ok: bool,
) -> mlua::Result<(Vec<Chord>, Bind, bool)> {
    let path: Vec<Chord> = keys
        .split_whitespace()
        .map(|k| {
            k.parse()
                .map_err(|e| rt_err(format!("{who}: key `{keys}`: {e}")))
        })
        .collect::<mlua::Result<_>>()?;
    if path.is_empty() {
        return Err(rt_err(format!("{who}: no key given")));
    }
    if path.len() > 1 && !global_ok {
        return Err(rt_err(format!(
            "{who}: key `{keys}`: keys inside a folder are bound with ranma.bind"
        )));
    }
    let backspace = Chord {
        key: crate::keys::Key::Backspace,
        mods: crate::keys::Mods::default(),
    };
    if path.len() > 1 && path.last() == Some(&backspace) {
        return Err(rt_err(format!(
            "{who}(\"{keys}\"): backspace goes up a level inside a folder and cannot be bound there"
        )));
    }
    let mut exit_override: Option<bool> = None;
    let mut global = false;
    let mut desc: Option<String> = None;
    let mut group: Option<String> = None;
    // A folder is a table in the action's place, which takes its group too.
    let folder = match &action {
        Value::Table(t) if global_ok => {
            let mut name: Option<String> = None;
            for pair in t.pairs::<String, Value>() {
                let (k, v) = pair?;
                match (k.as_str(), v) {
                    ("folder", Value::String(n)) => name = Some(n.to_str()?.to_string()),
                    ("group", Value::String(g)) => group = Some(g.to_str()?.to_string()),
                    (k @ ("folder" | "group"), other) => {
                        return Err(rt_err(format!(
                            "{who}(\"{keys}\"): `{k}` must be a string, not {}",
                            other.type_name()
                        )));
                    }
                    (k, _) => {
                        return Err(rt_err(format!(
                            "{who}(\"{keys}\"): a folder takes `folder` and `group`, not `{k}` (its name is its description)"
                        )));
                    }
                }
            }
            let name = name.ok_or_else(|| {
                rt_err(format!(
                    "{who}(\"{keys}\"): a table in the action's place is a folder: {{ folder = NAME }}"
                ))
            })?;
            check_heading(who, keys, "folder", &name)?;
            Some(name)
        }
        _ => None,
    };
    let expected = match (folder.is_some(), global_ok) {
        (true, _) => "group",
        (false, true) => "exit, global, desc, group",
        (false, false) => "exit, desc",
    };
    if let Some(t) = opts {
        for pair in t.pairs::<String, Value>() {
            let (k, v) = pair?;
            match (k.as_str(), v) {
                ("group", Value::String(g)) if global_ok => {
                    group = Some(g.to_str()?.to_string());
                }
                ("exit", Value::Boolean(b)) if folder.is_none() => exit_override = Some(b),
                ("global", Value::Boolean(b)) if global_ok && folder.is_none() => global = b,
                ("desc", Value::String(d)) if folder.is_none() => {
                    desc = Some(d.to_str()?.to_string());
                }
                (k @ ("desc" | "group"), other)
                    if folder.is_none() && (k == "desc" || global_ok) =>
                {
                    return Err(rt_err(format!(
                        "{who}(\"{keys}\"): `{k}` must be a string, not {}",
                        other.type_name()
                    )));
                }
                ("exit", other) | ("global", other)
                    if folder.is_none() && (k != "global" || global_ok) =>
                {
                    return Err(rt_err(format!(
                        "{who}(\"{keys}\"): `{k}` must be true or false, not {}",
                        other.type_name()
                    )));
                }
                _ => {
                    return Err(rt_err(format!(
                        "{who}: unknown option `{k}` (expected {expected})"
                    )));
                }
            }
        }
    }
    if let Some(g) = &group {
        check_heading(who, keys, "group", g)?;
    }
    if global && path.len() > 1 {
        return Err(rt_err(format!(
            "{who}(\"{keys}\"): a key inside a folder cannot be global"
        )));
    }
    let bind = match (folder, action) {
        (Some(name), _) => Bind {
            action: BindAction::Folder(Folder {
                name,
                binds: HashMap::new(),
            }),
            exits_mode: false,
            exit: None,
            label: "folder".into(),
            desc: None,
            group,
        },
        (None, Value::String(s)) => {
            let s = s.to_str()?.to_string();
            let parsed: Action = s
                .parse()
                .map_err(|e| rt_err(format!("{who}(\"{keys}\"): {e}")))?;
            Bind {
                exits_mode: exit_override.unwrap_or(parsed.exits_mode_by_default()),
                exit: exit_override,
                action: BindAction::Builtin(parsed),
                label: s,
                desc,
                group,
            }
        }
        (None, Value::Function(f)) => Bind {
            action: BindAction::Lua(Rc::new(lua.create_registry_value(f)?)),
            exits_mode: exit_override.unwrap_or(false),
            exit: exit_override,
            label: "<lua function>".into(),
            desc,
            group,
        },
        (None, other) => {
            return Err(rt_err(format!(
                "{who}(\"{keys}\"): action must be a string or a function, not {}",
                other.type_name()
            )));
        }
    };
    Ok((path, bind, global))
}

/// A folder's or a group's name: a heading in the which-key hint, so
/// something to read and at most as wide as a name there.
fn check_heading(who: &str, keys: &str, what: &str, name: &str) -> mlua::Result<()> {
    let w = unicode_width::UnicodeWidthStr::width(name);
    if name.trim().is_empty() || w > crate::whichkey::NAME_MAX {
        return Err(rt_err(format!(
            "{who}(\"{keys}\"): a {what} name is 1 to {} cells, not `{name}`",
            crate::whichkey::NAME_MAX
        )));
    }
    Ok(())
}

fn loading_only() -> mlua::Error {
    rt_err(
        "this only works while the configuration loads (init.lua or a plugin), \
         not from a bind, hook, module or timer",
    )
}

/// Apply what `ranma.set` (or a profile's `set`, as `who`) names to `s`.
fn apply_settings(s: &mut Settings, patch: SettingsPatch, who: &str) -> Result<(), String> {
    if let Some(leader) = patch.leader {
        s.leader = leader
            .parse()
            .map_err(|e| format!("{who}: leader `{leader}`: {e}"))?;
    }
    if let Some(t) = patch.theme {
        s.theme = t;
    }
    if let Some(l) = patch.layout {
        s.layout = l;
    }
    if let Some(r) = patch.master_ratio {
        if !(0.1..=0.9).contains(&r) {
            return Err(format!(
                "{who}: master_ratio must be between 0.1 and 0.9, not {r}"
            ));
        }
        s.master_ratio = r;
    }
    if let Some(m) = patch.scroll_min {
        if !(20..=500).contains(&m) {
            return Err(format!(
                "{who}: scroll_min must be between 20 and 500 cells, not {m}"
            ));
        }
        s.scroll_min = m;
    }
    if let Some(ws) = patch.scroll_widths {
        if !(1..=6).contains(&ws.len()) {
            return Err(format!(
                "{who}: scroll_widths takes 1 to 6 widths, not {}",
                ws.len()
            ));
        }
        s.scroll_widths = ws
            .iter()
            .map(|w| w.parse(s.scroll_min, &format!("{who}: scroll_widths")))
            .collect::<Result<_, _>>()?;
    }
    if let Some(w) = patch.scroll_width {
        s.scroll_width = w.parse(s.scroll_min, &format!("{who}: scroll_width"))?;
    }
    if let Some(c) = patch.scroll_center {
        s.scroll_center = c;
    }
    if let Some(p) = patch.preserve_split {
        s.preserve_split = p;
    }
    if let Some(on) = patch.splash {
        s.splash = on;
    }
    if let Some(secs) = patch.pane_idle {
        if !(0.5..=3600.0).contains(&secs) {
            return Err(format!(
                "{who}: pane_idle must be 0.5 to 3600 seconds, not {secs}"
            ));
        }
        s.pane_idle = std::time::Duration::from_secs_f64(secs);
    }
    if let Some(r) = patch.remain_on_exit {
        s.remain_on_exit = r;
    }
    if let Some(on) = patch.monitor_activity {
        s.monitor_activity = on;
    }
    if let Some(secs) = patch.monitor_silence {
        if !(1.0..=86400.0).contains(&secs) {
            return Err(format!(
                "{who}: monitor_silence must be 1 to 86400 seconds, not {secs}"
            ));
        }
        s.monitor_silence = std::time::Duration::from_secs_f64(secs);
    }
    if patch.shell.is_some() {
        s.shell = patch.shell;
    }
    if let Some(n) = patch.scrollback_lines {
        s.scrollback_lines = n;
    }
    if let Some(w) = patch.wm_mode {
        if let Some(sticky) = w.sticky {
            s.wm_mode_sticky = sticky;
        }
        match w.hint {
            None => {}
            Some(HintSetting::On(false)) => s.wm_mode_hint = None,
            Some(HintSetting::On(true)) => {
                s.wm_mode_hint = Some(std::time::Duration::from_millis(500))
            }
            Some(HintSetting::After(secs)) if (0.0..=10.0).contains(&secs) => {
                s.wm_mode_hint = Some(std::time::Duration::from_secs_f64(secs))
            }
            Some(HintSetting::After(secs)) => {
                return Err(format!(
                    "{who}: wm_mode.hint must be false or seconds from 0 to 10, not {secs}"
                ));
            }
        }
    }
    if let Some(m) = patch.mouse {
        s.mouse = m;
    }
    if let Some(u) = patch.updates {
        s.updates = u;
    }
    if let Some(n) = patch.nested {
        s.nested = n;
    }
    if let Some(h) = patch.title_host {
        s.title_host = h;
    }
    if let Some(c) = patch.theme_colors {
        s.theme_colors = c;
    }
    if let Some(r) = patch.restore {
        s.restore = r;
    }
    if let Some(p) = patch.paste {
        if let Some(u) = p.upload {
            s.paste_upload = u;
        }
        if let Some(c) = p.image_command {
            if c.trim().is_empty() {
                return Err(format!("{who}: paste.image_command is empty"));
            }
            s.paste_image_command = Some(c);
        }
    }
    if let Some(k) = patch.outer_leader {
        s.outer_leader = k
            .parse()
            .map_err(|e| format!("{who}: outer_leader `{k}`: {e}"))?;
    }
    if s.outer_leader == s.leader {
        return Err(format!(
            "{who}: outer_leader must differ from leader (it reaches past a nested ranma)"
        ));
    }
    if let Some(h) = patch.update_check_hours {
        if h.is_nan() || h <= 0.0 {
            return Err(format!(
                "{who}: update_check_hours must be positive, not {h}"
            ));
        }
        s.update_check_hours = h;
    }
    Ok(())
}

/// Apply what `ranma.bar` (or a profile's `bar`) names to `bar`.
fn apply_bar(bar: &mut BarLayout, patch: BarPatch) {
    if let Some(v) = patch.left {
        bar.left = v;
    }
    if let Some(v) = patch.center {
        bar.center = v;
    }
    if let Some(v) = patch.right {
        bar.right = v;
    }
    if let Some(s) = patch.size {
        bar.size = s;
    }
}

fn install_api(
    lua: &Lua,
    config_dir: Option<&Path>,
    jobs: &crate::jobs::Jobs,
    user: &UserEvents,
    values: &Values,
) -> mlua::Result<()> {
    let ranma = lua.create_table()?;
    ranma.set("version", env!("CARGO_PKG_VERSION"))?;
    if let Some(dir) = config_dir {
        ranma.set("config_dir", dir.display().to_string())?;
    }

    ranma.set(
        "set",
        lua.create_function(|lua, value: Value| {
            let table = match lua.from_value::<toml::Value>(value) {
                Ok(toml::Value::Table(t)) => t,
                Ok(other) => {
                    return Err(rt_err(format!(
                        "ranma.set: takes a table of settings, not {}",
                        other.type_str()
                    )));
                }
                Err(e) => return Err(rt_err(format!("ranma.set: {e}"))),
            };
            let mut b = lua.app_data_mut::<Builder>().ok_or_else(loading_only)?;
            apply_set(&mut b, table, "ranma.set").map_err(rt_err)
        })?,
    )?;

    ranma.set(
        "bind",
        lua.create_function(
            |lua, (keys, action, opts): (String, Value, Option<Table>)| {
                let (path, bind, global) =
                    parse_bind(lua, &keys, action, opts.as_ref(), "ranma.bind", true)?;
                let mut b = lua.app_data_mut::<Builder>().ok_or_else(loading_only)?;
                if let Some(g) = &bind.group
                    && !b.group_order.contains(g)
                {
                    b.group_order.push(g.clone());
                }
                // A key that stops being a folder takes the keys recorded
                // behind it so far with it: rebinding a default folder's key
                // must not leave its keys pointing into nothing.
                if !global && !matches!(bind.action, BindAction::Folder(_)) {
                    b.forget_behind(&path);
                }
                if path.len() > 1 {
                    // Put in its folder once the file is done: the folder
                    // may be declared after its keys.
                    b.nested.push((keys, path, Some(bind)));
                    return Ok(());
                }
                drop(b);
                let chord = path[0];
                if global
                    && chord
                        == lua
                            .app_data_ref::<Builder>()
                            .ok_or_else(loading_only)?
                            .settings
                            .leader
                {
                    return Err(rt_err(format!(
                        "ranma.bind(\"{keys}\"): the leader cannot also be a global bind"
                    )));
                }
                let mut b = lua.app_data_mut::<Builder>().ok_or_else(loading_only)?;
                if global {
                    b.global_binds.insert(chord, bind);
                } else {
                    b.binds.insert(chord, bind);
                }
                Ok(())
            },
        )?,
    )?;

    ranma.set(
        "mode",
        lua.create_function(|lua, (name, def): (String, Table)| {
            let who = format!("ranma.mode(\"{name}\")");
            if name.is_empty() || name.contains(char::is_whitespace) {
                return Err(rt_err(format!("{who}: a mode's name is one word")));
            }
            let mut mode = UserMode {
                label: name.to_uppercase(),
                binds: HashMap::new(),
                sticky: true,
                on_enter: None,
                on_exit: None,
            };
            for pair in def.pairs::<String, Value>() {
                let (k, v) = pair?;
                match (k.as_str(), v) {
                    ("label", Value::String(s)) => mode.label = s.to_str()?.to_string(),
                    ("sticky", Value::Boolean(b)) => mode.sticky = b,
                    ("on_enter", Value::Function(f)) => {
                        mode.on_enter = Some(Rc::new(lua.create_registry_value(f)?))
                    }
                    ("on_exit", Value::Function(f)) => {
                        mode.on_exit = Some(Rc::new(lua.create_registry_value(f)?))
                    }
                    ("binds", Value::Table(t)) => {
                        for pair in t.pairs::<String, Value>() {
                            let (keys, v) = pair?;
                            // A key's value is its action, or a table with the
                            // action first and the bind's options after it.
                            let (action, opts) = match v {
                                Value::Table(t) => (t.get::<Value>(1)?, Some(t)),
                                other => (other, None),
                            };
                            if let Some(t) = &opts {
                                t.raw_remove(1)?;
                            }
                            let (path, bind, global) =
                                parse_bind(lua, &keys, action, opts.as_ref(), &who, false)?;
                            debug_assert!(!global && path.len() == 1);
                            mode.binds.insert(path[0], bind);
                        }
                    }
                    (k @ ("label" | "sticky" | "on_enter" | "on_exit" | "binds"), other) => {
                        return Err(rt_err(format!(
                            "{who}: `{k}` cannot be {}",
                            other.type_name()
                        )));
                    }
                    (k, _) => {
                        return Err(rt_err(format!(
                            "{who}: unknown key `{k}` (expected binds, label, sticky, on_enter, on_exit)"
                        )));
                    }
                }
            }
            let mut b = lua.app_data_mut::<Builder>().ok_or_else(loading_only)?;
            b.modes.insert(name, mode);
            Ok(())
        })?,
    )?;

    ranma.set(
        "command",
        lua.create_function(
            |lua, (name, func, opts): (String, Function, Option<Table>)| {
                let who = format!("ranma.command(\"{name}\")");
                let ok = !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
                if !ok {
                    return Err(rt_err(format!(
                        "{who}: a command's name is letters, digits, _ - and ."
                    )));
                }
                if crate::action::CATALOGUE.iter().any(|(n, _)| *n == name) {
                    return Err(rt_err(format!(
                        "{who}: `{name}` is a built-in action already"
                    )));
                }
                let mut cmd = UserCommand {
                    func: Rc::new(lua.create_registry_value(func)?),
                    desc: None,
                    args: None,
                    complete: None,
                };
                if let Some(t) = opts {
                    for pair in t.pairs::<String, Value>() {
                        let (k, v) = pair?;
                        match (k.as_str(), v) {
                            ("desc", Value::String(s)) => cmd.desc = Some(s.to_str()?.to_string()),
                            ("args", Value::String(s)) => cmd.args = Some(s.to_str()?.to_string()),
                            ("complete", v @ (Value::Function(_) | Value::Table(_))) => {
                                cmd.complete = Some(Rc::new(lua.create_registry_value(v)?))
                            }
                            (k @ ("desc" | "args" | "complete"), other) => {
                                return Err(rt_err(format!(
                                    "{who}: `{k}` cannot be {}",
                                    other.type_name()
                                )));
                            }
                            (k, _) => {
                                return Err(rt_err(format!(
                                    "{who}: unknown option `{k}` (expected desc, args, complete)"
                                )));
                            }
                        }
                    }
                }
                let mut b = lua.app_data_mut::<Builder>().ok_or_else(loading_only)?;
                b.commands.insert(name, cmd);
                Ok(())
            },
        )?,
    )?;

    ranma.set(
        "unbind",
        lua.create_function(|lua, keys: String| {
            let path: Vec<Chord> = keys
                .split_whitespace()
                .map(|k| {
                    k.parse()
                        .map_err(|e| rt_err(format!("ranma.unbind: key `{keys}`: {e}")))
                })
                .collect::<mlua::Result<_>>()?;
            let mut b = lua.app_data_mut::<Builder>().ok_or_else(loading_only)?;
            b.forget_behind(&path);
            match path.as_slice() {
                [] => return Err(rt_err("ranma.unbind: no key given")),
                [chord] => {
                    b.binds.remove(chord);
                    b.global_binds.remove(chord);
                }
                _ => b.nested.push((keys, path, None)),
            }
            Ok(())
        })?,
    )?;

    ranma.set(
        "unbind_all",
        lua.create_function(|lua, ()| {
            let mut b = lua.app_data_mut::<Builder>().ok_or_else(loading_only)?;
            b.binds.clear();
            b.global_binds.clear();
            b.nested.clear();
            Ok(())
        })?,
    )?;

    ranma.set(
        "on",
        lua.create_function(|lua, (event, f): (String, Function)| {
            // A plugin's own event: whatever name it likes, after `user:`.
            if let Some(name) = event.strip_prefix("user:") {
                if name.is_empty() {
                    return Err(rt_err("ranma.on: `user:` needs a name after it"));
                }
                let key = Rc::new(lua.create_registry_value(f)?);
                lua.app_data_mut::<Builder>()
                    .ok_or_else(loading_only)?
                    .user_hooks
                    .entry(name.to_string())
                    .or_default()
                    .push(key);
                return Ok(());
            }
            let ev: Event = event.parse().map_err(|e| {
                rt_err(format!(
                    "ranma.on: {e} (a plugin's own event is named `user:<name>`)"
                ))
            })?;
            let key = Rc::new(lua.create_registry_value(f)?);
            lua.app_data_mut::<Builder>()
                .ok_or_else(loading_only)?
                .hooks
                .entry(ev)
                .or_default()
                .push(key);
            Ok(())
        })?,
    )?;

    ranma.set(
        "bar",
        lua.create_function(|lua, value: Value| {
            let patch: BarPatch = lua
                .from_value(value)
                .map_err(|e| rt_err(format!("ranma.bar: {e}")))?;
            let mut b = lua.app_data_mut::<Builder>().ok_or_else(loading_only)?;
            apply_bar(&mut b.bar, patch);
            Ok(())
        })?,
    )?;

    ranma.set(
        "profile",
        lua.create_function(|lua, (name, value): (String, Value)| {
            let who = format!("ranma.profile(\"{name}\")");
            let def: Profile = lua.from_value(value).map_err(|e| {
                rt_err(format!(
                    "{who}: {e} (a profile takes set, bar and toolbars)"
                ))
            })?;
            if def.set.theme.is_some() {
                return Err(rt_err(format!(
                    "{who}: a profile cannot change the theme; the theme is read once at load"
                )));
            }
            let mut b = lua.app_data_mut::<Builder>().ok_or_else(loading_only)?;
            // Checked now, against what init.lua has set so far, so a bad value
            // is an error at load and not when the profile is first used.
            let mut probe = b.settings.clone();
            apply_settings(&mut probe, def.set.clone(), &format!("{who} set")).map_err(rt_err)?;
            b.profiles.insert(name, def);
            Ok(())
        })?,
    )?;

    ranma.set(
        "toolbar",
        lua.create_function(|lua, (name, def): (String, Table)| {
            let (def, show) = parse_toolbar(lua, &name, &def)?;
            let mut b = lua.app_data_mut::<Builder>().ok_or_else(loading_only)?;
            b.toolbars.retain(|(n, _)| *n != name);
            b.toolbars_shown.retain(|n| *n != name);
            if show {
                b.toolbars_shown.push(name.clone());
            }
            b.toolbars.push((name, def));
            Ok(())
        })?,
    )?;

    ranma.set(
        "use_profile",
        lua.create_function(|lua, name: Option<String>| {
            let mut rt = lua.app_data_mut::<Runtime>().ok_or_else(|| {
                rt_err("ranma.use_profile only works inside binds and hooks, not at config load")
            })?;
            rt.ops.push(Op::Action(Action::Profile(name)));
            Ok(())
        })?,
    )?;

    ranma.set(
        "module",
        lua.create_function(|lua, (name, opts): (String, Table)| {
            let mut keys = Vec::new();
            for pair in opts.pairs::<String, Value>() {
                keys.push(pair?.0);
            }
            if BUILTIN_MODULES.contains(&name.as_str()) {
                // Built-ins are configured, not replaced: their only knobs are listed
                // here, so a typo is an error instead of a silently ignored option.
                let allowed: &[&str] = match name.as_str() {
                    "workspaces" => &["show", "label", "nested"],
                    "title" => &["nested"],
                    _ => &[],
                };
                if let Some(k) = keys.iter().find(|k| !allowed.contains(&k.as_str())) {
                    return Err(rt_err(format!(
                        "ranma.module(\"{name}\"): `{name}` is built in and takes {}; `{k}` is not one",
                        if allowed.is_empty() { "no options".to_string() } else { format!("only {}", allowed.join(", ")) }
                    )));
                }
                if name == "workspaces"
                    && let Some(show) = opts.get::<Option<String>>("show")?
                {
                    let all = match show.as_str() {
                        "all" => true,
                        "occupied" => false,
                        other => {
                            return Err(rt_err(format!(
                                "ranma.module(\"workspaces\"): show = \"{other}\" (expected \"all\" or \"occupied\")"
                            )));
                        }
                    };
                    lua.app_data_mut::<Builder>()
                        .ok_or_else(loading_only)?
                        .workspaces_show_all = all;
                }
                if name == "workspaces"
                    && let Some(label) = opts.get::<Option<String>>("label")?
                {
                    let numbers_only = match label.as_str() {
                        "program" => false,
                        "number" => true,
                        other => {
                            return Err(rt_err(format!(
                                "ranma.module(\"workspaces\"): label = \"{other}\" (expected \"program\" or \"number\")"
                            )));
                        }
                    };
                    lua.app_data_mut::<Builder>()
                        .ok_or_else(loading_only)?
                        .workspaces_numbers_only = numbers_only;
                }
                if name == "workspaces"
                    && let Some(nested) = opts.get::<Option<String>>("nested")?
                {
                    let n = match nested.as_str() {
                        "focused" => NestedWorkspaces::Focused,
                        "all" => NestedWorkspaces::All,
                        "off" => NestedWorkspaces::Off,
                        other => {
                            return Err(rt_err(format!(
                                "ranma.module(\"workspaces\"): nested = \"{other}\" (expected \"focused\", \"all\" or \"off\")"
                            )));
                        }
                    };
                    lua.app_data_mut::<Builder>()
                        .ok_or_else(loading_only)?
                        .workspaces_nested = n;
                }
                if name == "title"
                    && let Some(nested) = opts.get::<Option<String>>("nested")?
                {
                    let show = match nested.as_str() {
                        "show" => true,
                        "hide" => false,
                        other => {
                            return Err(rt_err(format!(
                                "ranma.module(\"title\"): nested = \"{other}\" (expected \"hide\" or \"show\")"
                            )));
                        }
                    };
                    lua.app_data_mut::<Builder>()
                        .ok_or_else(loading_only)?
                        .title_nested = show;
                }
                return Ok(());
            }
            if SYSTEM_MODULES.iter().any(|(n, _)| *n == name) {
                if let Some(k) = keys.iter().find(|k| !["interval", "format"].contains(&k.as_str())) {
                    return Err(rt_err(format!(
                        "ranma.module(\"{name}\"): `{name}` is built in and takes only interval, format; `{k}` is not one"
                    )));
                }
                let interval = opts.get::<Option<f64>>("interval")?;
                if let Some(secs) = interval.filter(|s| *s <= 0.0) {
                    return Err(rt_err(format!(
                        "ranma.module(\"{name}\"): interval must be positive, not {secs}"
                    )));
                }
                let def = system_module(&name, interval, opts.get("format")?).expect("listed above");
                lua.app_data_mut::<Builder>()
                    .ok_or_else(loading_only)?
                    .modules
                    .insert(name, def);
                return Ok(());
            }
            if let Some(k) = keys
                .iter()
                .find(|k| !["render", "exec", "format", "interval"].contains(&k.as_str()))
            {
                return Err(rt_err(format!(
                    "ranma.module(\"{name}\"): unknown option `{k}` (expected render, exec, format, interval)"
                )));
            }
            let interval = match opts.get::<Option<f64>>("interval")? {
                Some(secs) if secs > 0.0 => Some(std::time::Duration::from_secs_f64(secs)),
                Some(secs) => {
                    return Err(rt_err(format!(
                        "ranma.module(\"{name}\"): interval must be positive, not {secs}"
                    )));
                }
                None => None,
            };
            let render: Option<Function> = opts.get("render")?;
            let exec: Option<String> = opts.get("exec")?;
            let format: Option<String> = opts.get("format")?;
            let kind = match (render, exec) {
                (Some(f), None) => {
                    if format.is_some() {
                        return Err(rt_err(format!(
                            "ranma.module(\"{name}\"): format only applies to exec modules"
                        )));
                    }
                    ModuleKind::Lua(Rc::new(lua.create_registry_value(f)?))
                }
                (None, Some(command)) => {
                    if interval.is_none() {
                        return Err(rt_err(format!(
                            "ranma.module(\"{name}\"): an exec module needs an interval"
                        )));
                    }
                    ModuleKind::Exec { command, format }
                }
                _ => {
                    return Err(rt_err(format!(
                        "ranma.module(\"{name}\"): give exactly one of render or exec"
                    )));
                }
            };
            lua.app_data_mut::<Builder>()
                .ok_or_else(loading_only)?
                .modules
                .insert(name, ModuleDef { interval, kind });
            Ok(())
        })?,
    )?;

    ranma.set(
        "session",
        lua.create_function(|lua, (name, opts): (String, Table)| {
            for pair in opts.pairs::<String, Value>() {
                let (k, _) = pair?;
                if k != "accent" {
                    return Err(rt_err(format!(
                        "ranma.session(\"{name}\"): unknown option `{k}` (expected accent)"
                    )));
                }
            }
            let accent: Option<String> = opts.get("accent")?;
            let b = lua.app_data_mut::<Builder>();
            let mut b = b.ok_or_else(loading_only)?;
            match accent {
                Some(a) => {
                    let c = a
                        .parse::<theme::Color>()
                        .map_err(|e| rt_err(format!("ranma.session(\"{name}\"): accent: {e}")))?;
                    b.session_accents.insert(name, c);
                }
                None => {
                    b.session_accents.remove(&name);
                }
            }
            Ok(())
        })?,
    )?;

    ranma.set(
        "layout",
        lua.create_function(|lua, (name, def): (String, Table)| {
            let who = format!("ranma.layout(\"{name}\")");
            if !crate::layouts::valid_name(&name) {
                return Err(rt_err(format!(
                    "{who}: a layout name has no slashes or spaces"
                )));
            }
            let spec = crate::layouts::Spec::from_lua(&def, &who)
                .and_then(|s| s.check().map(|()| s).map_err(|e| format!("{who}: {e}")))
                .map_err(rt_err)?;
            lua.app_data_mut::<Builder>()
                .ok_or_else(loading_only)?
                .layouts
                .insert(name, spec);
            Ok(())
        })?,
    )?;

    ranma.set(
        "rule",
        lua.create_function(|lua, value: Value| {
            let spec: RuleSpec = lua
                .from_value(value)
                .map_err(|e| rt_err(format!("ranma.rule: {e}")))?;
            if spec.command.is_none() && spec.title.is_none() {
                return Err(rt_err("ranma.rule: give command or title to match on"));
            }
            let size = match spec.size.as_deref() {
                None => None,
                Some([w, h]) if (10..=100).contains(w) && (10..=100).contains(h) => Some((*w, *h)),
                Some(other) => {
                    return Err(rt_err(format!(
                        "ranma.rule: size is {{ width%, height% }}, each 10-100, not {other:?}"
                    )));
                }
            };
            if spec.workspace == Some(0) {
                return Err(rt_err("ranma.rule: workspaces are numbered from 1"));
            }
            let float = spec.float.unwrap_or(size.is_some());
            if !float && size.is_none() && spec.workspace.is_none() {
                return Err(rt_err(
                    "ranma.rule: the rule does nothing (give float, size or workspace)",
                ));
            }
            if spec.silent.is_some() && spec.workspace.is_none() {
                return Err(rt_err("ranma.rule: silent only applies with workspace"));
            }
            lua.app_data_mut::<Builder>()
                .ok_or_else(loading_only)?
                .rules
                .push(Rule {
                    command: spec.command,
                    title: spec.title,
                    float,
                    size,
                    workspace: spec.workspace,
                    silent: spec.silent.unwrap_or(false),
                });
            Ok(())
        })?,
    )?;

    // The runtime half: only meaningful while ranma is calling into Lua. During
    // config load there is no window manager to act on, so these refuse.
    // JSON for plugins that talk to command-line tools (`ai peers --json`,
    // `gh ... --json`): serde both ways, null as nil.
    let json = lua.create_table()?;
    json.set(
        "decode",
        lua.create_function(|lua, text: String| {
            let v: serde_json::Value = serde_json::from_str(&text)
                .map_err(|e| rt_err(format!("ranma.json.decode: {e}")))?;
            lua.to_value(&v)
        })?,
    )?;
    json.set(
        "encode",
        lua.create_function(|lua, v: Value| {
            let j: serde_json::Value = lua
                .from_value(v)
                .map_err(|e| rt_err(format!("ranma.json.encode: {e}")))?;
            Ok(j.to_string())
        })?,
    )?;
    ranma.set("json", json)?;

    ranma.set(
        "copy",
        lua.create_function(|lua, text: String| {
            let mut rt = lua.app_data_mut::<Runtime>().ok_or_else(|| {
                rt_err("ranma.copy only works inside binds, hooks, modules and timers, not at config load")
            })?;
            rt.ops.push(Op::Copy(text));
            Ok(())
        })?,
    )?;

    ranma.set(
        "action",
        lua.create_function(|lua, spec: String| {
            let action: Action = spec
                .parse()
                .map_err(|e| rt_err(format!("ranma.action: {e}")))?;
            let mut rt = lua.app_data_mut::<Runtime>().ok_or_else(|| {
                rt_err(
                    "ranma.action only works inside binds, hooks and modules, not at config load",
                )
            })?;
            rt.ops.push(Op::Action(action));
            Ok(())
        })?,
    )?;

    ranma.set(
        "notify",
        lua.create_function(|lua, msg: String| {
            let mut rt = lua.app_data_mut::<Runtime>().ok_or_else(|| {
                rt_err(
                    "ranma.notify only works inside binds, hooks and modules, not at config load",
                )
            })?;
            rt.notify = Some(msg);
            Ok(())
        })?,
    )?;

    ranma.set(
        "toast",
        lua.create_function(|lua, (text, opts): (String, Option<Table>)| {
            let mut urgent = false;
            let mut timeout = None;
            if let Some(t) = &opts {
                for pair in t.pairs::<String, Value>() {
                    let (k, v) = pair?;
                    match (k.as_str(), v) {
                        ("urgent", Value::Boolean(b)) => urgent = b,
                        ("timeout", Value::Integer(n)) if n > 0 => timeout = Some(n as f64),
                        ("timeout", Value::Number(n)) if n > 0.0 => timeout = Some(n),
                        ("urgent" | "timeout", other) => {
                            return Err(rt_err(format!(
                                "ranma.toast: bad `{k}` ({}); urgent is true/false, timeout positive seconds",
                                other.type_name()
                            )));
                        }
                        _ => {
                            return Err(rt_err(format!(
                                "ranma.toast: unknown option `{k}` (expected urgent, timeout)"
                            )));
                        }
                    }
                }
            }
            let mut rt = lua.app_data_mut::<Runtime>().ok_or_else(|| {
                rt_err("ranma.toast only works inside binds, hooks and modules, not at config load")
            })?;
            rt.toasts.push((text, urgent, timeout));
            Ok(())
        })?,
    )?;

    ranma.set(
        "state",
        lua.create_function(|lua, ()| {
            let st = lua
                .app_data_ref::<Runtime>()
                .ok_or_else(|| {
                    rt_err("ranma.state only works inside binds, hooks and modules, not at config load")
                })?
                .state
                .clone();
            let t = lua.create_table()?;
            t.set("session", st.session)?;
            t.set("sessions", st.sessions)?;
            t.set("workspace", st.workspace)?;
            t.set("workspaces", st.workspaces)?;
            t.set("focused", st.focused)?;
            t.set("title", st.title)?;
            t.set("mode", st.mode)?;
            t.set("panes", st.panes)?;
            Ok(t)
        })?,
    )?;

    ranma.set(
        "client",
        lua.create_function(|lua, ()| {
            let c = lua
                .app_data_ref::<Runtime>()
                .ok_or_else(|| {
                    rt_err("ranma.client only works inside binds, hooks and modules, not at config load")
                })?
                .state
                .client;
            let t = lua.create_table()?;
            c.fill(&t)?;
            Ok(t)
        })?,
    )?;

    crate::luapane::install(lua, &ranma)?;
    ranma.set(
        "option",
        lua.create_function(|lua, (key, spec): (String, Value)| {
            let spec = match lua.from_value::<toml::Value>(spec) {
                Ok(toml::Value::Table(t)) => t,
                _ => {
                    return Err(rt_err(format!(
                        "ranma.option(\"{key}\"): the second argument is a table: {{ type, default, ... }}"
                    )));
                }
            };
            let (opt, default) = crate::options::from_spec(&key, &spec).map_err(rt_err)?;
            let mut b = lua.app_data_mut::<Builder>().ok_or_else(loading_only)?;
            let builtin_top = crate::options::get(&b.default_set, &opt.group).is_some()
                && !b.plugin_options.iter().any(|o| o.group == opt.group);
            if builtin_top || crate::options::GROUPS.iter().any(|(g, _)| *g == opt.group) {
                return Err(rt_err(format!(
                    "ranma.option(\"{key}\"): `{}` is one of ranma's own settings; name the option after your plugin",
                    opt.group
                )));
            }
            if b.plugin_options.iter().any(|o| o.key == key) {
                return Err(rt_err(format!("ranma.option(\"{key}\"): declared twice")));
            }
            crate::options::set(&mut b.default_set, &key, default);
            b.plugin_options.push(opt);
            Ok(())
        })?,
    )?;
    {
        let values = values.clone();
        ranma.set(
            "get",
            lua.create_function(move |lua, key: String| {
                if let Some(b) = lua.app_data_ref::<Builder>() {
                    let is_theme = crate::options::builtin()
                        .iter()
                        .any(|o| o.key == key && o.home == crate::options::Home::Theme);
                    if is_theme {
                        return Err(rt_err(format!(
                            "ranma.get(\"{key}\"): the theme is not loaded yet while the configuration loads"
                        )));
                    }
                    let v = crate::options::get(&b.set_table, &key)
                        .or_else(|| crate::options::get(&b.default_set, &key));
                    return match v {
                        Some(v) => lua.to_value(v),
                        None if known_key(&b, &key) => Ok(Value::Nil),
                        None => Err(rt_err(format!("ranma.get: no option `{key}`"))),
                    };
                }
                let v = values.borrow();
                let Some(o) = v.options.iter().find(|o| o.key == key) else {
                    return Err(rt_err(format!("ranma.get: no option `{key}`")));
                };
                match v.layers.effective(o) {
                    Some(val) => lua.to_value(&val),
                    None => Ok(Value::Nil),
                }
            })?,
        )?;
    }
    crate::luaui::install(lua, &ranma)?;
    crate::luascreen::install(lua, &ranma)?;
    crate::store::Stores::new(crate::store::dir()).install(lua, &ranma)?;
    {
        let user = user.clone();
        ranma.set(
            "emit",
            lua.create_function(move |lua, (name, data): (String, Value)| {
                if lua.app_data_ref::<Runtime>().is_none() {
                    return Err(rt_err(
                        "ranma.emit only works inside binds, hooks, modules and timers, \
                         not at config load",
                    ));
                }
                user.emit(lua, &name, data)
            })?,
        )?;
    }
    crate::jobs::install(lua, &ranma, jobs, |lua, f| {
        match lua.app_data_mut::<Builder>() {
            Some(mut b) => {
                f(&mut b.timers);
                true
            }
            None => false,
        }
    })?;
    lua.globals().set("ranma", ranma)
}

/// Load the defaults, then `<config_dir>/init.lua` if it exists, then the theme.
pub fn load(config_dir: Option<&Path>) -> Result<Config> {
    let user_file = config_dir
        .map(|d| d.join("init.lua"))
        .filter(|p| p.is_file());
    let user_src = match &user_file {
        Some(p) => {
            Some(std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?)
        }
        None => None,
    };
    load_from(config_dir, user_file, user_src.as_deref())
}

/// [`load`] with the user source passed in, so tests need no files for it.
pub fn load_from(
    config_dir: Option<&Path>,
    user_file: Option<PathBuf>,
    user_src: Option<&str>,
) -> Result<Config> {
    let lua = Lua::new();
    lua.set_app_data(Builder::default());
    let watchdog = Watchdog::default();
    let jobs = crate::jobs::Jobs::default();
    let user_events = UserEvents::default();
    let values = Values::default();
    // mlua's error is not Send without its `send` feature, so it cannot go through
    // anyhow's `.context` directly; its Display is all we need from it anyway.
    install_api(&lua, config_dir, &jobs, &user_events, &values)
        .and_then(|()| watchdog.install(&lua))
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("installing the ranma Lua API")?;

    watchdog
        .run(LOAD_BUDGET, || {
            lua.load(DEFAULT_INIT_LUA)
                .set_name("@<built-in init.lua>")
                .exec()
        })
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("the built-in default config failed (this is a ranma bug)")?;
    {
        let mut b = lua
            .app_data_mut::<Builder>()
            .expect("builder installed above");
        b.default_set = b.set_table.clone();
    }

    let mut plugins = Vec::new();
    if let Some(dir) = config_dir {
        let found = PluginDirs::find(dir);
        found
            .put_on_path(&lua)
            .map_err(|e| anyhow::anyhow!("{e}"))
            .context("setting `require`'s path")?;
        for file in &found.files {
            plugins.push(source_plugin(&lua, &watchdog, file));
        }
    }

    if let Some(src) = user_src {
        let name = user_file
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "init.lua".into());
        // Lua errors carry their own "file:line:" prefix and traceback, which anyhow's
        // chain would otherwise bury under a generic "callback error".
        watchdog
            .run(LOAD_BUDGET, || {
                lua.load(src).set_name(format!("@{name}")).exec()
            })
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    }

    // The settings panel's file, over everything a person wrote.
    let mut warnings = Vec::new();
    let panel = match config_dir {
        Some(dir) => read_settings_file(&dir.join(SETTINGS_FILE))?,
        None => PanelFile::default(),
    };
    {
        let mut b = lua
            .app_data_mut::<Builder>()
            .expect("builder installed above");
        let file_set = b.set_table.clone();
        let mut apply = panel.set.clone();
        // A plugin that did not load leaves its saved options with nothing to
        // apply to. They are kept, for when it loads again, and said; one
        // broken plugin must not stop ranma from starting.
        let groups: Vec<String> = apply.keys().cloned().collect();
        for g in groups {
            let builtin = crate::options::get(&b.default_set, &g).is_some()
                || SETTINGS_ONLY_IN_LUA.contains(&g.as_str());
            if !builtin {
                apply.remove(&g);
                warnings.push(format!(
                    "{SETTINGS_FILE}: `{g}` is no option ranma knows now (a plugin that did not load?); kept, not applied"
                ));
            }
        }
        apply_set(&mut b, apply, SETTINGS_FILE).map_err(|e| anyhow::anyhow!("{e}"))?;
        b.set_table = file_set;
    }

    let mut builder = lua
        .remove_app_data::<Builder>()
        .expect("builder installed above");
    // A system module the bar names but nobody configured gets its defaults.
    let named: Vec<String> = builder.bar.all().cloned().collect();
    for name in named {
        if !builder.modules.contains_key(&name)
            && let Some(def) = system_module(&name, None, None)
        {
            builder.modules.insert(name, def);
        }
    }
    // Checked after the whole file ran, so a module may be defined after the
    // ranma.bar call that uses it.
    if let Some(unknown) = builder
        .bar
        .all()
        .chain(builder.profiles.values().flat_map(Profile::modules))
        .find(|m| !BUILTIN_MODULES.contains(&m.as_str()) && !builder.modules.contains_key(*m))
    {
        let builtins: Vec<&str> = BUILTIN_MODULES
            .iter()
            .copied()
            .chain(SYSTEM_MODULES.iter().map(|(n, _)| *n))
            .collect();
        anyhow::bail!(
            "ranma.bar: unknown module `{unknown}` (define it with ranma.module, or use a built-in: {})",
            builtins.join(", ")
        );
    }
    // Also after the whole file: a profile may name a toolbar defined later.
    for (pname, p) in &builder.profiles {
        if let Some(t) = p
            .toolbars
            .iter()
            .flatten()
            .find(|t| !builder.toolbars.iter().any(|(n, _)| n == *t))
        {
            anyhow::bail!(
                "ranma.profile(\"{pname}\"): unknown toolbar `{t}` (define it with ranma.toolbar)"
            );
        }
    }
    // Shown in the order they were defined, whatever order they were named in.
    let order = |shown: &mut Vec<String>, defs: &[(String, ToolbarDef)]| {
        shown.sort_by_key(|s| defs.iter().position(|(n, _)| n == s));
        shown.dedup();
    };
    order(&mut builder.toolbars_shown, &builder.toolbars);
    for p in builder.profiles.values_mut() {
        if let Some(t) = &mut p.toolbars {
            order(t, &builder.toolbars);
        }
    }
    let (theme, file_theme) = theme::load_over(
        &builder.settings.theme,
        &theme::theme_dirs(config_dir),
        Some(&panel.theme),
    )?;
    {
        let mut options = crate::options::builtin();
        options.extend(builder.plugin_options.iter().cloned());
        let mut v = values.0.borrow_mut();
        v.options = options;
        v.layers = crate::options::Layers {
            default_set: std::mem::take(&mut builder.default_set),
            file_set: std::mem::take(&mut builder.set_table),
            panel_set: panel.set,
            default_theme: theme::resolved(theme::DEFAULT_THEME_NAME, &[])?,
            file_theme,
            panel_theme: panel.theme,
            ..Default::default()
        };
    }
    // Keys inside folders, now that every folder is declared; then folders
    // left with no keys. One a failed plugin emptied stays, drawn as empty:
    // that plugin's error is already reported, and the folder is better shown
    // than a key that silently does nothing.
    for (keys, path, bind) in std::mem::take(&mut builder.nested) {
        put_nested(&mut builder.binds, &keys, &path, bind)?;
    }
    if plugins.iter().all(|p| p.error.is_none())
        && let Some((keys, name)) = empty_folder(&builder.binds, "")
    {
        anyhow::bail!("folder \"{name}\" on {keys} has no keys");
    }
    // A bind may name a mode declared after it; by now all are, so a name
    // that is none of them is an error here, as an unknown action is.
    {
        let tables = builder
            .binds
            .values()
            .chain(builder.global_binds.values())
            .chain(builder.modes.values().flat_map(|m| m.binds.values()));
        for b in tables {
            if let BindAction::Builtin(Action::Mode(m)) = &b.action
                && !builder.modes.contains_key(m)
            {
                anyhow::bail!("`mode {m}`: no ranma.mode is called {m}");
            }
        }
    }
    jobs.adopt(std::mem::take(&mut builder.timers));
    *user_events.hooks.borrow_mut() = std::mem::take(&mut builder.user_hooks);

    Ok(Config {
        modes: std::mem::take(&mut builder.modes),
        commands: std::mem::take(&mut builder.commands),
        base: (
            builder.settings.clone(),
            builder.bar.clone(),
            builder.toolbars_shown.clone(),
        ),
        toolbars: builder.toolbars,
        toolbars_shown: builder.toolbars_shown,
        profiles: builder.profiles,
        profile: None,
        settings: builder.settings,
        binds: builder.binds,
        group_order: builder.group_order,
        global_binds: builder.global_binds,
        hooks: builder.hooks,
        bar: builder.bar,
        modules: builder.modules,
        workspaces_show_all: builder.workspaces_show_all,
        workspaces_numbers_only: builder.workspaces_numbers_only,
        workspaces_nested: builder.workspaces_nested,
        title_nested: builder.title_nested,
        rules: builder.rules,
        session_accents: builder.session_accents,
        layouts: builder.layouts,
        theme,
        source: user_file,
        plugins,
        warnings,
        watchdog,
        jobs,
        user_events,
        values,
        lua,
    })
}

/// Where the settings panel saves, in the config directory.
pub const SETTINGS_FILE: &str = "settings.toml";

/// Top-level settings that exist but have no default in the built-in
/// init.lua (so `default_set` cannot vouch for them).
const SETTINGS_ONLY_IN_LUA: [&str; 2] = ["shell", "paste"];

/// What `settings.toml` holds: `[set]`, as `ranma.set` takes it, and
/// `[theme]`, merged over the theme in use.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PanelFile {
    pub set: toml::Table,
    pub theme: toml::Table,
}

/// Read `settings.toml`, strictly: only `[set]` and `[theme]`. No file is an
/// empty one.
pub fn read_settings_file(path: &Path) -> Result<PanelFile> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(PanelFile::default()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let mut t: toml::Table = text
        .parse()
        .with_context(|| format!("parsing {}", path.display()))?;
    let mut table = |k: &str| -> Result<toml::Table> {
        match t.remove(k) {
            None => Ok(toml::Table::new()),
            Some(toml::Value::Table(t)) => Ok(t),
            Some(_) => anyhow::bail!("{}: `{k}` must be a table", path.display()),
        }
    };
    let out = PanelFile {
        set: table("set")?,
        theme: table("theme")?,
    };
    if let Some(k) = t.keys().next() {
        anyhow::bail!(
            "{}: unknown table `{k}` (expected [set] and [theme])",
            path.display()
        );
    }
    Ok(out)
}

/// Write `settings.toml` whole, by a write and a rename: a crash leaves the
/// old file or the new one.
pub fn write_settings_file(path: &Path, f: &PanelFile) -> Result<()> {
    let mut root = toml::Table::new();
    if !f.set.is_empty() {
        root.insert("set".into(), toml::Value::Table(f.set.clone()));
    }
    if !f.theme.is_empty() {
        root.insert("theme".into(), toml::Value::Table(f.theme.clone()));
    }
    let text = format!(
        "# Written by ranma's settings panel. init.lua and the theme say the rest;\n\
         # what is here wins over them. Edit it by hand if you like: it is read\n\
         # as strictly as they are.\n{}",
        toml::to_string(&root).context("writing the settings")?
    );
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))
}

/// Every option and where each value comes from, shared between the
/// configuration and `ranma.get`.
#[derive(Debug, Clone, Default)]
pub struct Values(Rc<std::cell::RefCell<ValueState>>);

#[derive(Debug, Clone, Default)]
pub struct ValueState {
    pub options: Vec<crate::options::Opt>,
    pub layers: crate::options::Layers,
}

impl Values {
    pub fn borrow(&self) -> std::cell::Ref<'_, ValueState> {
        self.0.borrow()
    }
    pub fn borrow_mut(&self) -> std::cell::RefMut<'_, ValueState> {
        self.0.borrow_mut()
    }
}

/// Apply ranma's own settings from a table, as `ranma.set` would: the
/// settings panel's live edits go through here, so they are checked exactly
/// as a file's are. A plugin's group is not ranma's own and must not be in it.
pub fn patch_settings(s: &mut Settings, t: toml::Table, who: &str) -> Result<(), String> {
    let patch: SettingsPatch = toml::Value::Table(t)
        .try_into()
        .map_err(|e| format!("{who}: {e}"))?;
    apply_settings(s, patch, who)
}

fn known_key(b: &Builder, key: &str) -> bool {
    crate::options::builtin().iter().any(|o| o.key == key)
        || b.plugin_options.iter().any(|o| o.key == key)
}

/// Apply a `ranma.set` table: a plugin's group to its declared options,
/// checked; the rest as ranma's own settings, parsed as strictly as ever.
/// Both are kept in the builder's TOML, for the settings panel.
fn apply_set(b: &mut Builder, mut t: toml::Table, who: &str) -> Result<(), String> {
    let mut plugin = toml::Table::new();
    let groups: Vec<String> = b.plugin_options.iter().map(|o| o.group.clone()).collect();
    for g in groups {
        if let Some(v) = t.remove(&g) {
            plugin.insert(g, v);
        }
    }
    fn leaves(prefix: &str, v: &toml::Value, out: &mut Vec<(String, toml::Value)>) {
        match v {
            toml::Value::Table(t) => {
                for (k, v) in t {
                    leaves(&format!("{prefix}.{k}"), v, out)
                }
            }
            v => out.push((prefix.to_string(), v.clone())),
        }
    }
    let mut set = Vec::new();
    for (g, v) in &plugin {
        if !v.is_table() {
            return Err(format!(
                "{who}: `{g}` is a plugin's options; give a table: {g} = {{ ... }}"
            ));
        }
        leaves(g, v, &mut set);
    }
    for (key, v) in &set {
        let Some(o) = b.plugin_options.iter().find(|o| &o.key == key) else {
            let known: Vec<&str> = b
                .plugin_options
                .iter()
                .filter(|o| key.starts_with(&format!("{}.", o.group)))
                .map(|o| o.key.as_str())
                .collect();
            return Err(format!(
                "{who}: no option `{key}` (declared: {})",
                known.join(", ")
            ));
        };
        crate::options::check(o, v).map_err(|e| format!("{who}: {e}"))?;
    }
    let patch: SettingsPatch = toml::Value::Table(t.clone())
        .try_into()
        .map_err(|e| format!("{who}: {e}"))?;
    apply_settings(&mut b.settings, patch, who)?;
    crate::options::merge(&mut b.set_table, t);
    crate::options::merge(&mut b.set_table, plugin);
    Ok(())
}

/// How deep `ranma.emit` may nest (a handler emitting, whose handler emits,
/// ...) before it is refused: past this it is a loop, not a design.
const MAX_EMIT_DEPTH: usize = 8;

/// Plugins' own events (`ranma.on("user:<name>")`, `ranma.emit`): handlers
/// run there and then, inside the call that emitted, as Neovim's `User`
/// autocommands do.
#[derive(Debug, Clone, Default)]
struct UserEvents {
    hooks: Rc<std::cell::RefCell<HashMap<String, Vec<Rc<RegistryKey>>>>>,
    depth: Rc<Cell<usize>>,
}

impl UserEvents {
    fn emit(&self, lua: &Lua, name: &str, data: Value) -> mlua::Result<()> {
        let keys = self.hooks.borrow().get(name).cloned().unwrap_or_default();
        if keys.is_empty() {
            return Ok(());
        }
        if self.depth.get() >= MAX_EMIT_DEPTH {
            return Err(rt_err(format!(
                "ranma.emit(\"{name}\"): events nested {MAX_EMIT_DEPTH} deep; stopped"
            )));
        }
        self.depth.set(self.depth.get() + 1);
        let result = keys
            .iter()
            .try_for_each(|k| lua.registry_value::<Function>(k)?.call::<()>(data.clone()));
        self.depth.set(self.depth.get() - 1);
        result
    }
}

/// How long the built-in defaults, one plugin file, or `init.lua` may run
/// while loading. Generous: a load is rare and may `require` a lot.
pub const LOAD_BUDGET: Duration = Duration::from_secs(1);
/// How long one call into Lua may run once ranma is up: a bind, a hook, a
/// module's `render`. Past it the callback is stopped with an error.
pub const CALL_BUDGET: Duration = Duration::from_millis(200);

/// A deadline that Lua's instruction hook checks, so a loop in a plugin costs
/// an error instead of the terminal (DESIGN.md, "Plugins: Neovim's shape").
#[derive(Debug, Clone, Default)]
pub struct Watchdog(Rc<Cell<Option<(Instant, Duration)>>>);

impl Watchdog {
    fn install(&self, lua: &Lua) -> mlua::Result<()> {
        let w = self.clone();
        let hook = move |_: &Lua, _: &mlua::debug::Debug| match w.0.get() {
            Some((_, budget)) if w.overdue() => Err(rt_err(format!(
                "stopped: ran longer than {} ms",
                budget.as_millis()
            ))),
            _ => Ok(mlua::VmState::Continue),
        };
        // Often enough to stop a loop within a few ms of its budget, rarely
        // enough that the clock read is lost in the noise.
        let every = mlua::HookTriggers::new().every_nth_instruction(10_000);
        // The global hook, not `set_hook`: it is set on the main thread, and
        // Lua's own `coroutine.create` copies the hook of the thread that made
        // it. A per-thread hook would be copied too, but mlua finds its
        // callback by thread, and a coroutine it did not make has none.
        lua.set_global_hook(every, hook)?;
        // The hook's error can be caught, and a loop around a `pcall` would
        // catch it forever: the hook nearly always fires inside the call. So
        // past the deadline, whatever catches errors passes them on.
        let w = self.clone();
        let overdue = lua.create_function(move |_, ()| Ok(w.overdue()))?;
        lua.load(
            r#"
            local overdue = ...
            local error, pcall, xpcall, resume = error, pcall, xpcall, coroutine.resume
            local function pass(ok, ...)
              if not ok and overdue() then error((...), 0) end
              return ok, ...
            end
            _G.pcall = function(...) return pass(pcall(...)) end
            _G.xpcall = function(...) return pass(xpcall(...)) end
            coroutine.resume = function(...) return pass(resume(...)) end
            "#,
        )
        .set_name("@<ranma watchdog>")
        .call::<()>(overdue)
    }

    fn overdue(&self) -> bool {
        self.0
            .get()
            .is_some_and(|(start, budget)| start.elapsed() > budget)
    }

    /// Run `f` with `budget` to spend. A run inside another (a hook fired by
    /// an action a bind ran) shares the outer deadline instead of resetting it.
    pub fn run<R>(&self, budget: Duration, f: impl FnOnce() -> R) -> R {
        let outer = self.0.get();
        if outer.is_none() {
            self.0.set(Some((Instant::now(), budget)));
        }
        let out = f();
        if outer.is_none() {
            self.0.set(None);
        }
        out
    }
}

/// One plugin file sourced at load, for the bar and `--check-config`.
#[derive(Debug, Clone)]
pub struct PluginLoad {
    pub path: PathBuf,
    pub took: Duration,
    /// Why it was dropped; `None` when it loaded.
    pub error: Option<String>,
}

/// Neovim's layout under the config directory (DESIGN.md, "Plugins: Neovim's
/// shape"): `lua/` on `require`'s path, `plugin/*.lua` sourced, and each
/// `pack/*/start/*/` a package carrying its own `lua/` and `plugin/`.
#[derive(Debug, Default, PartialEq)]
struct PluginDirs {
    /// Searched by `require` in this order: the user's own `lua/` first, so a
    /// module of theirs shadows a package's.
    lua: Vec<PathBuf>,
    /// Sourced in this order: packages first, then the user's own
    /// `plugin/`, so their own plugins run last and win.
    files: Vec<PathBuf>,
}

impl PluginDirs {
    fn find(config_dir: &Path) -> PluginDirs {
        let mut packages: Vec<PathBuf> = sorted_entries(&config_dir.join("pack"))
            .into_iter()
            .flat_map(|pack| sorted_entries(&pack.join("start")))
            .filter(|p| p.is_dir())
            .collect();
        packages.push(config_dir.to_path_buf());
        let mut out = PluginDirs::default();
        for root in packages.iter().rev() {
            let lua = root.join("lua");
            if lua.is_dir() {
                out.lua.push(lua);
            }
        }
        for root in &packages {
            out.files.extend(
                sorted_entries(&root.join("plugin"))
                    .into_iter()
                    .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "lua")),
            );
        }
        out
    }

    /// Put the `lua/` directories in front of `package.path`, keeping the
    /// system's after them.
    fn put_on_path(&self, lua: &Lua) -> mlua::Result<()> {
        if self.lua.is_empty() {
            return Ok(());
        }
        let package: Table = lua.globals().get("package")?;
        let rest: String = package.get("path")?;
        let mut path = String::new();
        for d in &self.lua {
            let d = d.display();
            path.push_str(&format!("{d}/?.lua;{d}/?/init.lua;"));
        }
        path.push_str(&rest);
        package.set("path", path)
    }
}

/// A directory's entries, sorted by name; nothing if it does not exist.
fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    out.sort();
    out
}

impl Builder {
    /// Drop the folder keys recorded so far behind `path` (`"y s"` behind
    /// `y`): the key at `path` is no longer the folder they were bound in.
    fn forget_behind(&mut self, path: &[Chord]) {
        self.nested
            .retain(|(_, p, _)| !(p.len() > path.len() && p.starts_with(path)));
    }
}

/// Bind (or, with `None`, unbind) `path` inside its folders: every chord but
/// the last must be a folder by now.
fn put_nested(
    top: &mut HashMap<Chord, Bind>,
    keys: &str,
    path: &[Chord],
    bind: Option<Bind>,
) -> Result<()> {
    let (last, folders) = path.split_last().expect("two chords at least");
    let mut table = top;
    for (i, c) in folders.iter().enumerate() {
        let spelled = path[..=i]
            .iter()
            .map(Chord::to_string)
            .collect::<Vec<_>>()
            .join(" ");
        table = match table.get_mut(c).map(|b| &mut b.action) {
            Some(BindAction::Folder(f)) => &mut f.binds,
            _ => anyhow::bail!(
                "ranma.bind(\"{keys}\"): `{spelled}` is not a folder (make it one with ranma.bind(\"{spelled}\", {{ folder = NAME }}))"
            ),
        };
    }
    match bind {
        Some(b) => {
            table.insert(*last, b);
        }
        None => {
            table.remove(last);
        }
    }
    Ok(())
}

/// The first folder with no keys, depth first in key order: its keys as
/// bound and its name.
fn empty_folder(table: &HashMap<Chord, Bind>, prefix: &str) -> Option<(String, String)> {
    let mut chords: Vec<&Chord> = table.keys().collect();
    chords.sort_by_key(|c| c.to_string());
    for c in chords {
        if let Some(f) = table[c].folder() {
            let keys = format!("{prefix}{c}");
            if f.binds.is_empty() {
                return Some((keys, f.name.clone()));
            }
            if let Some(found) = empty_folder(&f.binds, &format!("{keys} ")) {
                return Some(found);
            }
        }
    }
    None
}

/// Source one plugin file against a copy of the configuration built so far,
/// keeping its effects only if it finishes: a failing plugin is dropped
/// whole, never half-applied, and the others still load.
fn source_plugin(lua: &Lua, watchdog: &Watchdog, path: &Path) -> PluginLoad {
    let before = lua
        .app_data_ref::<Builder>()
        .expect("builder installed")
        .clone();
    let start = Instant::now();
    let result = std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|src| {
            watchdog
                .run(LOAD_BUDGET, || {
                    lua.load(src)
                        .set_name(format!("@{}", path.display()))
                        .exec()
                })
                .map_err(|e| e.to_string())
        });
    if result.is_err() {
        lua.set_app_data(before);
    }
    PluginLoad {
        path: path.to_path_buf(),
        took: start.elapsed(),
        error: result.err(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{Dir, WorkspaceTarget};

    fn with_user(src: &str) -> Result<Config> {
        load_from(None, None, Some(src))
    }

    #[test]
    fn a_profile_overlays_the_base_and_goes_back_exactly() {
        let mut cfg = with_user(
            r#"
            ranma.set { layout = "master", mouse = "hover" }
            ranma.profile("mobile", {
              set = { layout = "dwindle", wm_mode = { hint = false } },
              bar = { center = {}, right = { "mode" } },
            })
            "#,
        )
        .unwrap();
        let (settings, bar) = (cfg.settings.clone(), cfg.bar.clone());
        assert_eq!(cfg.profile, None);
        cfg.use_profile(Some("mobile")).unwrap();
        assert_eq!(cfg.settings.layout, Layout::Dwindle);
        assert_eq!(cfg.settings.wm_mode_hint, None);
        assert_eq!(
            cfg.settings.mouse,
            MouseMode::Hover,
            "what it does not name stays"
        );
        assert!(cfg.bar.center.is_empty());
        assert_eq!(cfg.bar.right, vec!["mode".to_string()]);
        assert_eq!(cfg.bar.left, bar.left, "a side it does not name stays");
        assert_eq!(cfg.profile.as_deref(), Some("mobile"));
        cfg.use_profile(None).unwrap();
        assert_eq!(cfg.settings, settings);
        assert_eq!(cfg.bar, bar);
        assert_eq!(cfg.profile, None);
        let e = cfg.use_profile(Some("tablet")).unwrap_err();
        assert!(
            e.contains("no profile `tablet`") && e.contains("mobile"),
            "{e}"
        );
        assert_eq!(cfg.settings, settings, "a failed switch changes nothing");
    }

    /// The defaults ship the mobile view defined and unused (assets/init.lua).
    #[test]
    fn the_defaults_define_the_mobile_view_without_using_it() {
        let mut cfg = with_user("").unwrap();
        assert!(cfg.toolbars_shown.is_empty());
        assert_eq!(cfg.profile, None);
        assert_eq!(cfg.bar.size, crate::toolbar::Size::Normal);
        let t = cfg.toolbar("touch").unwrap();
        assert_eq!(t.buttons.len(), 8);
        assert_eq!(t.size, crate::toolbar::Size::Large);
        cfg.use_profile(Some("mobile")).unwrap();
        assert_eq!(cfg.settings.layout, Layout::Monocle);
        assert_eq!(cfg.toolbars_shown, vec!["touch".to_string()]);
        assert_eq!(cfg.bar.size, crate::toolbar::Size::Large);
    }

    #[test]
    fn a_toolbar_is_parsed_strictly() {
        let err = |src: &str| format!("{:#}", with_user(src).unwrap_err());
        let ok = with_user(
            r#"ranma.toolbar("t", { position = "top", size = "large", show = true,
                 buttons = { { "+", "new_pane", text = "new" }, { "f", function() end } } })"#,
        )
        .unwrap();
        let t = ok.toolbar("t").unwrap();
        assert_eq!(t.position, crate::toolbar::Position::Top);
        assert_eq!(t.buttons.len(), 2);
        assert_eq!(t.buttons[0].label.text.as_deref(), Some("new"));
        assert_eq!(ok.toolbars_shown, vec!["t".to_string()]);
        assert!(err(r#"ranma.toolbar("t", { buttons = {}, colour = 1 })"#).contains("colour"));
        assert!(
            err(r#"ranma.toolbar("t", { position = "left", buttons = { { "+", "new_pane" } } })"#)
                .contains("top, bottom or beside")
        );
        assert!(
            err(r#"ranma.toolbar("t", { buttons = { { "+", "new_pnae" } } })"#)
                .contains("new_pnae")
        );
        assert!(
            err(r#"ranma.toolbar("t", { buttons = { { "+" } } })"#)
                .contains("needs a label and an action")
        );
        assert!(
            err(r#"ranma.toolbar("t", { buttons = { { "+", "new_pane", colour = "x" } } })"#)
                .contains("colour")
        );
        assert!(err(r#"ranma.toolbar("t", { buttons = {} })"#).contains("empty"));
        assert!(
            err(r#"ranma.profile("m", { toolbars = { "nope" } })"#)
                .contains("unknown toolbar `nope`")
        );
        assert!(err(r#"ranma.bar { size = "huge" }"#).contains("huge"));
    }

    #[test]
    fn toolbars_show_and_hide_and_a_profile_brings_its_own() {
        let mut cfg = with_user(
            r#"ranma.toolbar("a", { buttons = { { "a", "help" } } })
               ranma.toolbar("b", { show = true, buttons = { { "b", "help" } } })
               ranma.profile("p", { toolbars = { "b", "a" } })"#,
        )
        .unwrap();
        assert_eq!(cfg.toolbars_shown, vec!["b".to_string()]);
        cfg.show_toolbar("a", None).unwrap();
        assert_eq!(
            cfg.toolbars_shown,
            vec!["a".to_string(), "b".to_string()],
            "in definition order"
        );
        cfg.show_toolbar("b", Some(false)).unwrap();
        assert_eq!(cfg.toolbars_shown, vec!["a".to_string()]);
        assert!(
            cfg.show_toolbar("c", None)
                .unwrap_err()
                .contains("no toolbar `c`")
        );
        cfg.use_profile(Some("p")).unwrap();
        assert_eq!(cfg.toolbars_shown, vec!["a".to_string(), "b".to_string()]);
        cfg.use_profile(None).unwrap();
        assert_eq!(
            cfg.toolbars_shown,
            vec!["b".to_string()],
            "the base's, not what was toggled"
        );
    }

    #[test]
    fn a_profile_is_parsed_as_strictly_as_the_base() {
        let err = |src: &str| format!("{:#}", with_user(src).unwrap_err());
        assert!(err(r#"ranma.profile("m", { toolbar = {} })"#).contains("toolbar"));
        assert!(err(r#"ranma.profile("m", { set = { layuot = "dwindle" } })"#).contains("layuot"));
        assert!(
            err(r#"ranma.profile("m", { set = { master_ratio = 2 } })"#)
                .contains(r#"ranma.profile("m") set: master_ratio"#)
        );
        assert!(err(r#"ranma.profile("m", { set = { theme = "x" } })"#).contains("theme"));
        assert!(err(r#"ranma.profile("m", { bar = { left = { "nope" } } })"#).contains("`nope`"));
        assert!(
            err(r#"ranma.use_profile("m")"#).contains("only works inside"),
            "switching is for run time"
        );
    }

    fn builtin(cfg: &Config, keys: &str) -> Option<Action> {
        match &cfg.binds.get(&keys.parse().unwrap())?.action {
            BindAction::Builtin(a) => Some(a.clone()),
            BindAction::Lua(_) | BindAction::Folder(_) => None,
        }
    }

    #[test]
    fn folders_hold_keys_in_any_order_and_nest() {
        let cfg = with_user(
            r#"
            ranma.bind("g s", "exec lazygit", { desc = "status" })
            ranma.bind("g", { folder = "git", group = "tools" })
            ranma.bind("g b", { folder = "branches" })
            ranma.bind("g b n", function() end, { desc = "new" })
            ranma.bind("g b d", "exec git branch -d")
            ranma.unbind("g b d")
            ranma.bind("h", function() end, { desc = "history", group = "plugins" })
            "#,
        )
        .unwrap();
        let g: Chord = "g".parse().unwrap();
        let git = cfg.binds[&g].folder().unwrap();
        assert_eq!(git.name, "git");
        assert_eq!(cfg.binds[&g].group.as_deref(), Some("tools"));
        assert_eq!(git.binds.len(), 2, "s and b");
        let path: Vec<Chord> = ["g", "b"].iter().map(|k| k.parse().unwrap()).collect();
        let branches = folder_binds(&cfg.binds, &path).unwrap();
        assert_eq!(branches.len(), 1, "d was unbound");
        // After the groups the default folders name (ranma, panes).
        assert!(
            cfg.group_order
                .ends_with(&["tools".into(), "plugins".into()])
        );
        assert!(
            !cfg.binds[&g].exits_mode,
            "opening a folder stays in WM mode"
        );
    }

    /// The defaults bind keys inside their folders (`x`, `y`); a user's own
    /// bind on the folder's key, or an unbind of it, takes those keys with it
    /// instead of leaving them in a folder that no longer exists.
    #[test]
    fn a_default_folder_key_can_be_rebound_or_unbound() {
        let x: Chord = "x".parse().unwrap();
        let y: Chord = "y".parse().unwrap();
        let cfg = with_user(r#"ranma.bind("y", "detach") ranma.unbind("x")"#).unwrap();
        assert_eq!(builtin(&cfg, "y"), Some(Action::Detach));
        assert!(!cfg.binds.contains_key(&x));
        // A folder of the user's own on the key keeps the defaults' keys in
        // it; unbinding the key first starts it empty.
        let cfg = with_user(r#"ranma.bind("y", { folder = "mine" }) ranma.bind("y n", "detach")"#)
            .unwrap();
        let mine = cfg.binds[&y].folder().unwrap();
        assert_eq!((mine.name.as_str(), mine.binds.len()), ("mine", 3));
        let cfg = with_user(
            r#"ranma.unbind("y") ranma.bind("y", { folder = "mine" }) ranma.bind("y n", "detach")"#,
        )
        .unwrap();
        assert_eq!(cfg.binds[&y].folder().unwrap().binds.len(), 1);
        // Inside a folder too: "x c" rebound as a key drops nothing else.
        let cfg = with_user(r#"ranma.bind("x c", "detach")"#).unwrap();
        assert_eq!(cfg.binds[&x].folder().unwrap().binds.len(), 12);
    }

    #[test]
    fn folders_and_groups_are_parsed_strictly() {
        let err = |src: &str| format!("{:#}", with_user(src).unwrap_err());
        assert!(
            err(r#"ranma.bind("z", { folder = "scratch" })"#)
                .contains(r#"folder "scratch" on z has no keys"#)
        );
        assert!(
            err(r#"ranma.bind("g", "detach") ranma.bind("g s", "detach")"#)
                .contains("`g` is not a folder")
        );
        assert!(
            err(r#"ranma.bind("g s", "detach")"#).contains("`g` is not a folder"),
            "a folder never declared"
        );
        assert!(err(r#"ranma.bind("g", { folder = "git", desc = "x" })"#).contains("not `desc`"));
        assert!(
            err(r#"ranma.bind("g", { folder = "git" }, { exit = true })"#)
                .contains("unknown option `exit` (expected group)")
        );
        assert!(err(r#"ranma.bind("g", { group = "git" })"#).contains("{ folder = NAME }"));
        assert!(
            err(r#"ranma.bind("g", { folder = "git" }) ranma.bind("g backspace", "detach")"#)
                .contains("backspace goes up a level")
        );
        assert!(
            err(r#"ranma.bind("g", { folder = "git" }) ranma.bind("g s", "detach", { global = true })"#)
                .contains("cannot be global")
        );
        assert!(
            err(r#"ranma.bind("h", "detach", { group = "a name far too long" })"#)
                .contains("1 to 16 cells")
        );
        assert!(err(r#"ranma.bind("h", "detach", { group = 3 })"#).contains("must be a string"));
        assert!(
            err(r#"ranma.mode("m", { binds = { ["a b"] = "detach" } })"#)
                .contains("keys inside a folder are bound with ranma.bind")
        );
    }

    #[test]
    fn defaults_load_and_mirror_the_hyprland_keymap() {
        let cfg = load_from(None, None, None).unwrap();
        assert_eq!(cfg.settings.leader, "ctrl+b".parse().unwrap());
        assert_eq!(cfg.settings.layout, Layout::Dwindle);
        assert_eq!(builtin(&cfg, "t"), Some(Action::NewPane));
        assert_eq!(builtin(&cfg, "left"), Some(Action::Focus(Dir::Left)));
        assert_eq!(builtin(&cfg, "shift+up"), Some(Action::Resize(Dir::Up, 3)));
        assert_eq!(
            builtin(&cfg, "0"),
            Some(Action::Workspace(WorkspaceTarget::Index(10)))
        );
        assert_eq!(builtin(&cfg, "backspace"), Some(Action::SessionSwitcher));
        assert_eq!(builtin(&cfg, "shift+s"), Some(Action::ServerSwitcher));
        assert_eq!(builtin(&cfg, "?"), Some(Action::Help));
        assert_eq!(builtin(&cfg, ":"), Some(Action::CommandPalette));
        // Alt+S: the scratchpad outside WM mode, sending the pane there inside it.
        assert_eq!(builtin(&cfg, "alt+s"), Some(Action::MoveToScratchpad));
        match &cfg.global_binds[&"alt+s".parse().unwrap()].action {
            BindAction::Builtin(a) => assert_eq!(*a, Action::ScratchpadToggle),
            _ => panic!("alt+s is a builtin"),
        }
        assert_eq!(
            builtin(&cfg, "m"),
            Some(Action::MoveWorkspaceToSession(None))
        );
        assert!(cfg.binds[&"t".parse().unwrap()].exits_mode);
        assert!(!cfg.binds[&"left".parse().unwrap()].exits_mode);
        // Everything ranma has gets a key, the rare ones in folders.
        assert_eq!(builtin(&cfg, "p"), Some(Action::Settings));
        assert_eq!(builtin(&cfg, "i"), Some(Action::DisplayPanes));
        assert_eq!(builtin(&cfg, ";"), Some(Action::FocusLast));
        assert_eq!(
            builtin(&cfg, "l"),
            Some(Action::Workspace(WorkspaceTarget::Last))
        );
        assert_eq!(builtin(&cfg, "]"), Some(Action::PasteBuffer(1)));
        assert_eq!(builtin(&cfg, "shift+v"), Some(Action::ChooseBuffer));
        let inside = |folder: &str, key: &str| {
            let path = [folder.parse().unwrap()];
            match &folder_binds(&cfg.binds, &path).unwrap()[&key.parse().unwrap()].action {
                BindAction::Builtin(a) => a.clone(),
                _ => panic!("{folder} {key} is a builtin"),
            }
        };
        assert_eq!(inside("x", "a"), Action::SyncToggle);
        assert_eq!(inside("x", "shift+a"), Action::SyncClear);
        assert_eq!(inside("x", "r"), Action::RespawnPane);
        assert_eq!(inside("x", "c"), Action::Snap(crate::action::Snap::Center));
        assert_eq!(inside("y", "s"), Action::SaveLayout(None));
        assert!(!cfg.binds.contains_key(&"a".parse().unwrap()));
    }

    #[test]
    fn user_config_overrides_and_extends() {
        let cfg = with_user(
            r#"
            ranma.set { leader = "ctrl+a", wm_mode = { sticky = false } }
            ranma.bind("t", "exec nvim")
            ranma.unbind("q")
            ranma.bind("x", function() end, { exit = true })
            ranma.on("pane_open", function(ev) end)
            "#,
        )
        .unwrap();
        assert_eq!(cfg.settings.leader, "ctrl+a".parse().unwrap());
        assert!(!cfg.settings.wm_mode_sticky);
        assert_eq!(builtin(&cfg, "t"), Some(Action::Exec("nvim".into())));
        assert!(!cfg.binds.contains_key(&"q".parse().unwrap()));
        let x = &cfg.binds[&"x".parse().unwrap()];
        assert!(matches!(x.action, BindAction::Lua(_)) && x.exits_mode);
        assert_eq!(cfg.hooks[&Event::PaneOpen].len(), 1);
    }

    #[test]
    fn an_explicit_exit_is_kept_apart_from_the_default() {
        let cfg = with_user(
            r#"
            ranma.set { wm_mode = { sticky = false } }
            ranma.bind("shift+left", "resize left", { exit = false })
            ranma.bind("t", "new_pane", { exit = true })
            "#,
        )
        .unwrap();
        let b = |k: &str| &cfg.binds[&k.parse().unwrap()];
        assert_eq!(
            (b("shift+left").exit, b("shift+left").exits_mode),
            (Some(false), false)
        );
        assert_eq!(b("t").exit, Some(true));
        assert_eq!(
            b("q").exit,
            None,
            "unset: the action's default and sticky decide"
        );
    }

    #[test]
    fn unbind_all_starts_from_nothing() {
        let cfg = with_user("ranma.unbind_all(); ranma.bind('t', 'new_pane')").unwrap();
        assert_eq!(cfg.binds.len(), 1);
    }

    #[test]
    fn errors_name_the_problem_and_the_line() {
        let err = format!(
            "{:#}",
            with_user("\n\nranma.bind('t', 'new_pain')").unwrap_err()
        );
        assert!(err.contains("new_pain") && err.contains(":3:"), "{err}");

        let err = format!(
            "{:#}",
            with_user("ranma.set { leeder = 'ctrl+a' }").unwrap_err()
        );
        assert!(err.contains("leeder"), "{err}");

        let err = format!(
            "{:#}",
            with_user("ranma.bind('ctlr+x', 'quit')").unwrap_err()
        );
        assert!(err.contains("ctlr"), "{err}");

        let err = format!(
            "{:#}",
            with_user("ranma.on('pane_opne', function() end)").unwrap_err()
        );
        assert!(err.contains("pane_opne"), "{err}");

        let err = format!(
            "{:#}",
            with_user("ranma.bind('x', 'quit', { exti = true })").unwrap_err()
        );
        assert!(err.contains("exti"), "{err}");

        let err = format!(
            "{:#}",
            with_user("ranma.set { theme = 'nope' }").unwrap_err()
        );
        assert!(err.contains("nope"), "{err}");
    }

    #[test]
    fn global_binds_are_separate_and_alt_arrows_are_default() {
        let cfg = load_from(None, None, None).unwrap();
        let alt_left = "alt+left".parse().unwrap();
        assert!(cfg.global_binds.contains_key(&alt_left));
        // The same chord means something else in each table: focus outside WM
        // mode, open a pane on that side inside it.
        assert_eq!(
            builtin(&cfg, "alt+left"),
            Some(Action::NewPaneAt(Dir::Left))
        );
        assert_eq!(builtin(&cfg, "return"), Some(Action::ExitMode));
        assert_eq!(
            builtin(&cfg, "alt+3"),
            Some(Action::MoveToWorkspace(WorkspaceTarget::Index(3)))
        );
        assert!(builtin(&cfg, "shift+3").is_none());
        // Alt+Shift+digit arrives as Alt+<symbol>; US and ABNT2 symbols are bound.
        for (sym, n) in [("!", 1), ("@", 2), ("^", 6), ("¨", 6), (")", 10)] {
            let chord: crate::keys::Chord = format!("alt+{sym}").parse().unwrap();
            assert!(
                matches!(
                    cfg.global_binds[&chord].action,
                    BindAction::Builtin(Action::MoveToWorkspace(WorkspaceTarget::Index(m))) if m == n
                ),
                "{sym}"
            );
        }
        assert!(matches!(
            cfg.global_binds[&"alt+shift+left".parse().unwrap()].action,
            BindAction::Builtin(Action::Move(Dir::Left))
        ));
        // Bare Alt+digit (global) goes to the workspace; in WM mode it moves the pane.
        let alt3 = "alt+3".parse().unwrap();
        assert!(matches!(
            cfg.global_binds[&alt3].action,
            BindAction::Builtin(Action::Workspace(WorkspaceTarget::Index(3)))
        ));
        assert!(matches!(
            cfg.global_binds[&"alt+0".parse().unwrap()].action,
            BindAction::Builtin(Action::Workspace(WorkspaceTarget::Index(10)))
        ));
        assert_eq!(cfg.settings.mouse, MouseMode::Click);

        let cfg = with_user("ranma.unbind('alt+left'); ranma.set { mouse = 'hover' }").unwrap();
        assert!(!cfg.global_binds.contains_key(&alt_left));
        assert!(!cfg.binds.contains_key(&alt_left));
        assert_eq!(cfg.settings.mouse, MouseMode::Hover);

        let err = format!(
            "{:#}",
            with_user("ranma.bind('ctrl+b', 'quit', { global = true })").unwrap_err()
        );
        assert!(err.contains("leader"), "{err}");
        let err = format!(
            "{:#}",
            with_user("ranma.bind('x', 'quit', { global = 1 })").unwrap_err()
        );
        assert!(err.contains("true or false"), "{err}");
        let err = format!(
            "{:#}",
            with_user("ranma.set { mouse = 'always' }").unwrap_err()
        );
        assert!(err.contains("always"), "{err}");
    }

    #[test]
    fn bar_and_modules() {
        let cfg = load_from(None, None, None).unwrap();
        assert_eq!(cfg.bar.left, vec!["mode", "session", "workspaces"]);
        assert!(matches!(cfg.modules["clock"].kind, ModuleKind::Lua(_)));

        let cfg = with_user(
            r#"
            ranma.bar { right = { "load", "clock" } }
            ranma.module("load", { interval = 5, exec = "cat /proc/loadavg", format = "L %s" })
            ranma.module("workspaces", { show = "all" })
            "#,
        )
        .unwrap();
        assert_eq!(cfg.bar.right, vec!["load", "clock"]);
        // Only the side given changes.
        assert_eq!(cfg.bar.left, vec!["mode", "session", "workspaces"]);
        assert!(cfg.workspaces_show_all);
        assert!(
            !cfg.workspaces_numbers_only,
            "program names are the default"
        );
        assert_eq!(cfg.workspaces_nested, NestedWorkspaces::Focused);
        let all = with_user("ranma.module('workspaces', { nested = 'all' })").unwrap();
        assert_eq!(all.workspaces_nested, NestedWorkspaces::All);
        assert!(with_user("ranma.module('workspaces', { nested = 'yes' })").is_err());
        let numbers = with_user("ranma.module('workspaces', { label = 'number' })").unwrap();
        assert!(numbers.workspaces_numbers_only);
        let e = with_user("ranma.module('workspaces', { label = 'title' })").unwrap_err();
        assert!(e.to_string().contains("label = \"title\""), "{e}");
        match &cfg.modules["load"].kind {
            ModuleKind::Exec { command, format } => {
                assert_eq!(command, "cat /proc/loadavg");
                assert_eq!(format.as_deref(), Some("L %s"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn paste_settings_are_read_strictly() {
        let d = with_user("").unwrap();
        assert!(d.settings.paste_upload, "uploading is the default");
        assert_eq!(d.settings.paste_image_command, None);
        let c =
            with_user(r#"ranma.set { paste = { upload = false, image_command = "pngpaste -" } }"#)
                .unwrap();
        assert!(!c.settings.paste_upload);
        assert_eq!(
            c.settings.paste_image_command.as_deref(),
            Some("pngpaste -")
        );
        let err = format!(
            "{:#}",
            with_user("ranma.set { paste = { uplaod = true } }").unwrap_err()
        );
        assert!(err.contains("uplaod"), "{err}");
        assert!(with_user(r#"ranma.set { paste = { image_command = " " } }"#).is_err());
    }

    #[test]
    fn the_which_key_hint_setting() {
        let d = load_from(None, None, None).unwrap();
        assert_eq!(
            d.settings.wm_mode_hint,
            Some(std::time::Duration::from_millis(500))
        );
        let off = with_user("ranma.set { wm_mode = { hint = false } }").unwrap();
        assert_eq!(off.settings.wm_mode_hint, None);
        let slow = with_user("ranma.set { wm_mode = { hint = 1.5 } }").unwrap();
        assert_eq!(
            slow.settings.wm_mode_hint,
            Some(std::time::Duration::from_millis(1500))
        );
        let e = format!(
            "{:#}",
            with_user("ranma.set { wm_mode = { hint = -1 } }").unwrap_err()
        );
        assert!(e.contains("wm_mode.hint"), "{e}");
        assert!(with_user("ranma.set { wm_mode = { hint = 'soon' } }").is_err());
    }

    #[test]
    fn master_layout_and_its_ratio() {
        let cfg = with_user("ranma.set { layout = 'master', master_ratio = 0.6 }").unwrap();
        assert_eq!(cfg.settings.layout, Layout::Master);
        assert_eq!(cfg.settings.master_ratio, 0.6);
        let e = format!(
            "{:#}",
            with_user("ranma.set { master_ratio = 0.95 }").unwrap_err()
        );
        assert!(e.contains("master_ratio"), "{e}");
    }

    #[test]
    fn layouts_are_declared_in_lua_and_checked() {
        use crate::layouts::SplitName;
        let cfg = with_user(
            r#"
            ranma.layout("dev", {
              split = "horizontal",
              { cwd = "~/projects/kumiko", command = "nvim", size = 2 },
              { split = "vertical", { command = "yarn run dev" }, {} },
            })
            "#,
        )
        .unwrap();
        let dev = &cfg.layouts["dev"];
        assert_eq!(dev.split, Some(SplitName::Horizontal));
        assert_eq!(dev.children[0].command.as_deref(), Some("nvim"));
        assert_eq!(dev.children[0].size, Some(2.0));
        assert_eq!(dev.children[1].children.len(), 2);
        assert_eq!(dev.panes().len(), 3);

        let err = |src: &str| format!("{:#}", with_user(src).unwrap_err());
        assert!(
            err(r#"ranma.layout("a", { split = "horizontal", { comand = "x" } })"#)
                .contains("unknown key `comand`")
        );
        assert!(err(r#"ranma.layout("a", { split = "diagonal", {}, {} })"#).contains("diagonal"));
        assert!(err(r#"ranma.layout("a", { {}, {} })"#).contains("needs split"));
        assert!(err(r#"ranma.layout("two words", {})"#).contains("no slashes or spaces"));
    }

    #[test]
    fn session_accents_by_name() {
        let cfg = with_user("ranma.session('kumiko', { accent = '#ff6a6a' })").unwrap();
        assert_eq!(
            cfg.session_accents.get("kumiko"),
            Some(&theme::Color::Rgb(0xff, 0x6a, 0x6a))
        );
        for (src, needle) in [
            ("ranma.session('x', { colour = 'red' })", "colour"),
            ("ranma.session('x', { accent = 'pink' })", "pink"),
        ] {
            let err = format!("{:#}", with_user(src).unwrap_err());
            assert!(err.contains(needle), "{src}: {err}");
        }
    }

    #[test]
    fn system_modules_are_defined_by_naming_them() {
        let cfg = with_user("ranma.bar { right = { 'cpu', 'mem' } }").unwrap();
        assert_eq!(
            cfg.modules["cpu"].interval,
            Some(std::time::Duration::from_secs(2))
        );
        assert!(matches!(
            cfg.modules["mem"].kind,
            ModuleKind::Mem { format: None }
        ));
        // Configured, before or after the bar names it.
        let cfg = with_user(
            "ranma.module('cpu', { interval = 1, format = 'C %s' })\nranma.bar { right = { 'cpu' } }",
        )
        .unwrap();
        assert_eq!(
            cfg.modules["cpu"].interval,
            Some(std::time::Duration::from_secs(1))
        );
        assert!(
            matches!(&cfg.modules["cpu"].kind, ModuleKind::Cpu { format: Some(f) } if f == "C %s")
        );
        // Not named, not defined: nothing ticks for a module nobody sees.
        assert!(
            !load_from(None, None, None)
                .unwrap()
                .modules
                .contains_key("cpu")
        );
    }

    #[test]
    fn bar_mistakes_are_named() {
        for (src, needle) in [
            ("ranma.bar { right = { 'nope' } }", "nope"),
            ("ranma.bar { middle = { 'title' } }", "middle"),
            ("ranma.module('x', { interval = 5 })", "exactly one"),
            ("ranma.module('x', { exec = 'true' })", "needs an interval"),
            (
                "ranma.module('x', { render = function() end, format = '%s' })",
                "format only",
            ),
            (
                "ranma.module('x', { render = function() end, every = 5 })",
                "every",
            ),
            (
                "ranma.module('x', { render = function() end, interval = 0 })",
                "positive",
            ),
            ("ranma.module('title', { show = 'all' })", "built in"),
            ("ranma.module('workspaces', { show = 'some' })", "some"),
            (
                "ranma.module('cpu', { exec = 'true' })",
                "only interval, format",
            ),
            ("ranma.module('mem', { interval = -1 })", "positive"),
        ] {
            let err = format!("{:#}", with_user(src).unwrap_err());
            assert!(err.contains(needle), "{src}: {err}");
        }
    }

    #[test]
    fn runtime_api_refuses_at_config_load() {
        for src in [
            "ranma.action('quit')",
            "ranma.notify('x')",
            "ranma.state()",
            "ranma.toast('x')",
        ] {
            let err = format!("{:#}", with_user(src).unwrap_err());
            assert!(err.contains("not at config load"), "{src}: {err}");
        }
        // A bad action is caught where it is written, even inside a function body
        // that only runs later: at call time, with the line.
        let err = format!("{:#}", with_user("ranma.action('fly')").unwrap_err());
        assert!(err.contains("fly"), "{err}");
    }

    #[test]
    fn toast_options_are_checked() {
        for (src, needle) in [
            ("ranma.toast('x', { loud = true })", "loud"),
            ("ranma.toast('x', { timeout = -1 })", "timeout"),
            ("ranma.toast('x', { urgent = 'yes' })", "urgent"),
        ] {
            let err = format!("{:#}", with_user(src).unwrap_err());
            assert!(err.contains(needle), "{src}: {err}");
        }
    }

    #[test]
    fn update_settings() {
        let cfg = load_from(None, None, None).unwrap();
        assert_eq!(cfg.settings.updates, UpdateMode::Remind);
        assert_eq!(cfg.settings.update_check_hours, 24.0);
        assert_eq!(builtin(&cfg, "shift+u"), Some(Action::Update));
        let cfg = with_user("ranma.set { updates = 'prompt', update_check_hours = 6 }").unwrap();
        assert_eq!(cfg.settings.updates, UpdateMode::Prompt);
        assert_eq!(cfg.settings.update_check_hours, 6.0);
        for (src, needle) in [
            ("ranma.set { updates = 'always' }", "always"),
            ("ranma.set { update_check_hours = 0 }", "positive"),
        ] {
            let err = format!("{:#}", with_user(src).unwrap_err());
            assert!(err.contains(needle), "{src}: {err}");
        }
    }

    #[test]
    fn nested_settings() {
        let cfg = load_from(None, None, None).unwrap();
        assert_eq!(cfg.settings.nested, NestedMode::Auto);
        assert_eq!(cfg.settings.outer_leader, "ctrl+alt+b".parse().unwrap());
        assert_eq!(cfg.settings.title_host, TitleHost::Ssh);
        assert!(cfg.settings.splash, "the splash is on by default");
        assert!(
            !with_user("ranma.set { splash = false }")
                .unwrap()
                .settings
                .splash
        );
        assert!(with_user("ranma.set { splash = 'yes' }").is_err());
        let cfg = with_user("ranma.set { title_host = 'always' }").unwrap();
        assert_eq!(cfg.settings.title_host, TitleHost::Always);
        assert!(with_user("ranma.set { title_host = 'sometimes' }").is_err());
        assert_eq!(cfg.settings.theme_colors, ThemeColors::Own);
        let cfg = with_user("ranma.set { theme_colors = 'outer' }").unwrap();
        assert_eq!(cfg.settings.theme_colors, ThemeColors::Outer);
        assert!(with_user("ranma.set { theme_colors = 'inherit' }").is_err());
        assert_eq!(with_user("").unwrap().settings.restore, RestoreMode::Ask);
        let cfg = with_user("ranma.set { restore = 'off' }").unwrap();
        assert_eq!(cfg.settings.restore, RestoreMode::Off);
        assert!(with_user("ranma.set { restore = 'always' }").is_err());
        let cfg = with_user("ranma.set { nested = 'off', outer_leader = 'ctrl+alt+a' }").unwrap();
        assert_eq!(cfg.settings.nested, NestedMode::Off);
        let err = format!(
            "{:#}",
            with_user("ranma.set { outer_leader = 'ctrl+b' }").unwrap_err()
        );
        assert!(err.contains("differ"), "{err}");
    }

    #[test]
    fn globs() {
        assert!(glob("htop", "htop"));
        assert!(!glob("htop", "htop -d 5"));
        assert!(glob("htop*", "htop -d 5"));
        assert!(glob("*NVIM*", "keys.rs - NVIM"));
        assert!(glob("?vim", "nvim"));
        assert!(glob("*", ""));
        assert!(!glob("a*b", "acd"));
        assert!(glob("a*b*c", "a-b-b-c"));
    }

    #[test]
    fn rules() {
        let cfg = with_user(
            r#"
            ranma.rule { command = "htop*", float = true, size = { 70, 60 } }
            ranma.rule { title = "*NVIM*", workspace = 2, silent = true }
            ranma.rule { command = "btop", size = { 50, 50 } }
            "#,
        )
        .unwrap();
        assert_eq!(cfg.rules.len(), 3);
        assert!(cfg.rules[0].matches_command("htop -d 5"));
        assert!(!cfg.rules[0].matches_title("htop"));
        assert_eq!(cfg.rules[1].workspace, Some(2));
        // A size implies floating.
        assert!(cfg.rules[2].float);
        for (src, needle) in [
            ("ranma.rule { float = true }", "command or title"),
            ("ranma.rule { command = 'x' }", "does nothing"),
            ("ranma.rule { command = 'x', size = { 5, 50 } }", "10-100"),
            ("ranma.rule { command = 'x', workspace = 0 }", "from 1"),
            (
                "ranma.rule { command = 'x', float = true, silent = true }",
                "silent",
            ),
            ("ranma.rule { command = 'x', floating = true }", "floating"),
        ] {
            let err = format!("{:#}", with_user(src).unwrap_err());
            assert!(err.contains(needle), "{src}: {err}");
        }
    }

    #[test]
    fn lua_syntax_errors_surface() {
        assert!(with_user("ranma.bind(").is_err());
    }

    /// A config directory with these files, fresh for each test.
    fn config_tree(tag: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ranma-plugins-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (path, src) in files {
            let p = dir.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, src).unwrap();
        }
        dir
    }

    fn bound(cfg: &Config, keys: &str) -> Option<String> {
        Some(cfg.binds.get(&keys.parse().unwrap())?.label.clone())
    }

    #[test]
    fn plugins_load_before_init_lua_and_require_finds_lua_dir() {
        let dir = config_tree(
            "order",
            &[
                ("lua/util.lua", "return { action = 'equalize' }"),
                ("lua/deep/init.lua", "return { action = 'help' }"),
                (
                    "plugin/a.lua",
                    "ranma.bind('f5', require('util').action)\n\
                     ranma.bind('f6', require('deep').action)\n\
                     ranma.bind('f7', 'help')",
                ),
                ("init.lua", "ranma.bind('f7', 'equalize')"),
            ],
        );
        let cfg = load(Some(&dir)).unwrap();
        assert_eq!(bound(&cfg, "f5").as_deref(), Some("equalize"));
        assert_eq!(bound(&cfg, "f6").as_deref(), Some("help"), "?/init.lua");
        assert_eq!(
            bound(&cfg, "f7").as_deref(),
            Some("equalize"),
            "init.lua runs after plugins and has the last word"
        );
        assert_eq!(cfg.plugins.len(), 1);
        assert!(cfg.plugins[0].error.is_none());
    }

    #[test]
    fn a_failing_plugin_is_dropped_whole_and_the_rest_load() {
        let dir = config_tree(
            "drop",
            &[
                (
                    "plugin/a_bad.lua",
                    "ranma.bind('f5', 'help')\nerror('boom')",
                ),
                ("plugin/b_good.lua", "ranma.bind('f6', 'help')"),
                ("plugin/c_typo.lua", "ranma.set { no_such_setting = 1 }"),
            ],
        );
        let cfg = load(Some(&dir)).unwrap();
        assert_eq!(bound(&cfg, "f5"), None, "its bind went with it");
        assert_eq!(bound(&cfg, "f6").as_deref(), Some("help"));
        let errors: Vec<_> = cfg.plugins.iter().map(|p| p.error.is_some()).collect();
        assert_eq!(errors, [true, false, true], "sourced in name order");
        assert!(cfg.plugins[0].error.as_ref().unwrap().contains("boom"));
        assert!(
            cfg.plugins[2]
                .error
                .as_ref()
                .unwrap()
                .contains("no_such_setting"),
            "a plugin is parsed as strictly as init.lua"
        );
    }

    #[test]
    fn a_folder_a_failed_plugin_emptied_stays_empty() {
        let dir = config_tree(
            "emptied",
            &[
                (
                    "plugin/agents.lua",
                    "ranma.bind('i a', 'help')\nerror('boom')",
                ),
                ("init.lua", "ranma.bind('i', { folder = 'agents' })"),
            ],
        );
        let cfg = load(Some(&dir)).expect("the plugin's error is reported, not the folder");
        let i: Chord = "i".parse().unwrap();
        assert!(cfg.binds[&i].folder().unwrap().binds.is_empty());
        assert!(cfg.plugins[0].error.is_some());
    }

    #[test]
    fn packages_carry_their_own_lua_and_plugin_dirs() {
        let dir = config_tree(
            "pack",
            &[
                ("pack/ext/start/hist/lua/hist.lua", "return 'help'"),
                (
                    "pack/ext/start/hist/plugin/hist.lua",
                    "ranma.bind('f5', (require('hist')))\nranma.bind('f6', 'help')",
                ),
                (
                    "pack/ext/opt/lazy/plugin/lazy.lua",
                    "error('opt/ is not sourced')",
                ),
                ("plugin/mine.lua", "ranma.bind('f6', 'equalize')"),
            ],
        );
        let cfg = load(Some(&dir)).unwrap();
        assert_eq!(bound(&cfg, "f5").as_deref(), Some("help"));
        assert_eq!(
            bound(&cfg, "f6").as_deref(),
            Some("equalize"),
            "the user's own plugin/ runs after packages"
        );
        assert_eq!(cfg.plugins.len(), 2);
        assert!(cfg.plugins.iter().all(|p| p.error.is_none()));
    }

    #[test]
    fn the_users_lua_dir_shadows_a_packages() {
        let dir = config_tree(
            "shadow",
            &[
                ("pack/ext/start/p/lua/m.lua", "return 'help'"),
                ("lua/m.lua", "return 'equalize'"),
                ("init.lua", "ranma.bind('f5', (require('m')))"),
            ],
        );
        let cfg = load(Some(&dir)).unwrap();
        assert_eq!(bound(&cfg, "f5").as_deref(), Some("equalize"));
    }

    #[test]
    fn a_plugin_that_never_returns_is_stopped() {
        let dir = config_tree(
            "hang",
            &[
                (
                    "plugin/a_loop.lua",
                    "ranma.bind('f5', 'help')\nwhile true do end",
                ),
                ("plugin/b_ok.lua", "ranma.bind('f6', 'help')"),
            ],
        );
        let start = Instant::now();
        let cfg = load(Some(&dir)).unwrap();
        assert!(start.elapsed() < LOAD_BUDGET * 3, "{:?}", start.elapsed());
        assert!(cfg.plugins[0].error.as_ref().unwrap().contains("stopped"));
        assert_eq!(bound(&cfg, "f5"), None);
        assert_eq!(bound(&cfg, "f6").as_deref(), Some("help"));
    }

    #[test]
    fn a_callback_is_stopped_at_its_budget_even_under_pcall() {
        let cfg = load_from(None, None, None).unwrap();
        let run = |src: &str| {
            let start = Instant::now();
            let r = cfg.watchdog.run(CALL_BUDGET, || cfg.lua.load(src).exec());
            (r, start.elapsed())
        };
        let (r, took) = run("while true do end");
        assert!(r.unwrap_err().to_string().contains("stopped"));
        assert!(took < CALL_BUDGET * 3, "{took:?}");
        // pcall catches the hook's error, but the loop around it runs on and
        // trips the hook again outside any pcall.
        let (r, _) = run("while true do pcall(function() while true do end end) end");
        assert!(r.is_err());
        let (r, _) = run(
            "local co = coroutine.create(function() while true do end end)\n\
             assert(coroutine.resume(co))",
        );
        assert!(r.is_err(), "a coroutine inherits the hook");
        // Disarmed after a run: quick code afterwards is untouched.
        assert!(cfg.lua.load("for i = 1, 1e6 do end").exec().is_ok());
    }

    #[test]
    fn timers_made_at_load_start_with_the_config_and_go_with_a_failing_plugin() {
        let dir = config_tree(
            "timers",
            &[
                (
                    "plugin/a_bad.lua",
                    "ranma.every(100, function() end)\nerror('x')",
                ),
                ("plugin/b_ok.lua", "ranma.defer(10, function() end)"),
            ],
        );
        let cfg = load(Some(&dir)).unwrap();
        assert_eq!(
            cfg.jobs.timer_count(),
            1,
            "the failed plugin's timer went with it"
        );
        let err = |src: &str| with_user(src).unwrap_err().to_string();
        assert!(err("ranma.every(10, function() end)").contains("too often"));
        assert!(err("ranma.defer(-1, function() end)").contains("0 or more"));
        assert!(err("ranma.spawn('true')").contains("not at config load"));
    }

    #[test]
    fn spawn_options_are_strict() {
        let cfg = load_from(None, None, None).unwrap();
        cfg.lua.set_app_data(Runtime::default());
        let err = |src: &str| cfg.lua.load(src).exec().unwrap_err().to_string();
        assert!(err("ranma.spawn('x', { on_exti = print })").contains("unknown option `on_exti`"));
        assert!(err("ranma.spawn('x', { timeout = 'soon' })").contains("`timeout` must be"));
        assert!(err("ranma.spawn({})").contains("empty command"));
        assert!(err("ranma.spawn(3)").contains("string or a list"));
        cfg.lua
            .load("ranma.spawn({ 'ls', '-l' }, { cwd = '/' })")
            .exec()
            .unwrap();
        let rt = cfg.lua.remove_app_data::<Runtime>().unwrap();
        assert!(matches!(&rt.ops[..], [Op::Spawn(_, s)] if s.argv == ["ls", "-l"] && !s.lines));
    }

    #[test]
    fn building_the_config_from_a_bind_is_an_error_not_a_crash() {
        let cfg = load_from(None, None, None).unwrap();
        cfg.lua.set_app_data(Runtime::default());
        for call in [
            "ranma.set { splash = false }",
            "ranma.bind('f5', 'help')",
            "ranma.unbind('t')",
            "ranma.unbind_all()",
            "ranma.on('pane_open', function() end)",
            "ranma.bar { left = {} }",
            "ranma.module('m', { render = function() return '' end })",
            "ranma.rule { title = 'x', workspace = 2 }",
            "ranma.session('s', { accent = '#ff0000' })",
            "ranma.profile('p', {})",
            "ranma.toolbar('t', { buttons = { { 'x', 'help' } } })",
            "ranma.layout('l', { split = 'horizontal', {}, {} })",
        ] {
            let e = cfg.lua.load(call).exec().unwrap_err().to_string();
            assert!(
                e.contains("only works while the configuration loads"),
                "{call}: {e}"
            );
        }
    }

    #[test]
    fn plugins_have_events_of_their_own() {
        let cfg = with_user(
            r#"
            got = {}
            ranma.on("user:build", function(d) table.insert(got, "a" .. d.n) end)
            ranma.on("user:build", function(d) table.insert(got, "b" .. d.n) end)
            ranma.on("user:loop", function() ranma.emit("loop") end)
            "#,
        )
        .unwrap();
        cfg.lua.set_app_data(Runtime::default());
        cfg.lua
            .load("ranma.emit('build', { n = 1 }) ranma.emit('nobody', 2)")
            .exec()
            .unwrap();
        let got: Vec<String> = cfg.lua.load("return got").eval().unwrap();
        assert_eq!(got, ["a1", "b1"], "in order, there and then");
        let e = cfg
            .lua
            .load("ranma.emit('loop')")
            .exec()
            .unwrap_err()
            .to_string();
        assert!(e.contains("nested 8 deep"), "{e}");
        cfg.lua.remove_app_data::<Runtime>();
        assert!(
            cfg.lua
                .load("ranma.emit('build', {})")
                .exec()
                .unwrap_err()
                .to_string()
                .contains("not at config load")
        );

        let err = |src: &str| with_user(src).unwrap_err().to_string();
        assert!(err("ranma.on('user:', print)").contains("needs a name"));
        assert!(err("ranma.on('buld', print)").contains("user:<name>"));
    }

    #[test]
    fn the_new_events_parse_and_pane_idle_is_a_range() {
        for e in [
            "command_started",
            "cwd_change",
            "title_change",
            "bell",
            "pane_idle",
            "hover",
        ] {
            assert!(e.parse::<Event>().is_ok(), "{e}");
        }
        let cfg = with_user("ranma.set { pane_idle = 2.5 }").unwrap();
        assert_eq!(
            cfg.settings.pane_idle,
            std::time::Duration::from_millis(2500)
        );
        assert_eq!(
            load_from(None, None, None)
                .unwrap()
                .settings
                .pane_idle
                .as_secs(),
            5
        );
        assert!(
            with_user("ranma.set { pane_idle = 0 }")
                .unwrap_err()
                .to_string()
                .contains("0.5 to 3600")
        );
    }

    /// Every key the defaults leave in a table, as dotted leaves.
    fn leaf_keys(prefix: &str, t: &toml::Table, out: &mut Vec<String>) {
        for (k, v) in t {
            let key = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{prefix}.{k}")
            };
            match v {
                toml::Value::Table(sub) => leaf_keys(&key, sub, out),
                _ => out.push(key),
            }
        }
    }

    #[test]
    fn the_registry_covers_every_setting_and_theme_key_or_says_why_not() {
        let cfg = load_from(None, None, None).unwrap();
        let v = cfg.values.borrow();
        let has = |k: &str| v.options.iter().any(|o| o.key == k);
        let mut keys = Vec::new();
        leaf_keys("", &v.layers.default_set, &mut keys);
        for k in &keys {
            assert!(
                has(k) || crate::options::SET_NOT_IN_PANEL.contains(&k.as_str()),
                "ranma.set's `{k}` is neither in the options registry nor in SET_NOT_IN_PANEL"
            );
        }
        let mut keys = Vec::new();
        leaf_keys("", &v.layers.default_theme, &mut keys);
        for k in &keys {
            let left_out = crate::options::NOT_IN_PANEL
                .iter()
                .any(|n| k == n || k.starts_with(&format!("{n}.")));
            assert!(
                has(k) || left_out,
                "the theme's `{k}` is neither an option nor in NOT_IN_PANEL"
            );
        }
        // And every option names something real.
        for o in v.options.iter() {
            let found = match o.home {
                crate::options::Home::Init => {
                    crate::options::get(&v.layers.default_set, &o.key).is_some()
                }
                crate::options::Home::Theme => {
                    crate::options::get(&v.layers.default_theme, &o.key).is_some()
                }
            };
            assert!(
                found || o.unset.is_some(),
                "option `{}` is in no default layer",
                o.key
            );
        }
    }

    #[test]
    fn plugins_declare_options_set_them_and_read_them() {
        let dir = config_tree(
            "options",
            &[
                (
                    "plugin/history.lua",
                    "ranma.option('history.max_results', { type = 'int', min = 1, max = 500, default = 100 })\n\
                     ranma.option('history.case', { type = 'bool', default = false })\n\
                     seen = ranma.get('history.max_results')",
                ),
                (
                    "init.lua",
                    "ranma.set { history = { max_results = 20 }, splash = false }",
                ),
            ],
        );
        let cfg = load(Some(&dir)).unwrap();
        assert!(!cfg.settings.splash);
        cfg.lua.set_app_data(Runtime::default());
        let get = |k: &str| {
            cfg.lua
                .load(format!("return ranma.get('{k}')"))
                .eval::<Value>()
                .unwrap()
        };
        assert_eq!(get("history.max_results").as_integer(), Some(20));
        assert_eq!(get("history.case").as_boolean(), Some(false));
        assert_eq!(get("splash").as_boolean(), Some(false));
        assert_eq!(
            get("border.style").as_string().unwrap().to_str().unwrap(),
            "rounded"
        );
        let seen: i64 = cfg.lua.load("return seen").eval().unwrap();
        assert_eq!(seen, 100, "at load, a plugin reads its default");
        let v = cfg.values.borrow();
        let o = v
            .options
            .iter()
            .find(|o| o.key == "history.max_results")
            .unwrap();
        assert!(o.plugin && o.group == "history");
        assert_eq!(
            v.layers.value(o, crate::options::Layer::File),
            Some(toml::Value::Integer(20))
        );

        let err = |src: &str| with_user(src).unwrap_err().to_string();
        assert!(
            err("ranma.option('wm_mode.x', { type = 'bool', default = true })")
                .contains("ranma's own")
        );
        assert!(err("ranma.option('p.x', { type = 'bool', default = true }) ranma.option('p.x', { type = 'bool', default = true })").contains("declared twice"));
        assert!(
            err(
                "ranma.option('p.x', { type = 'bool', default = true }) ranma.set { p = { y = 1 } }"
            )
            .contains("no option `p.y`")
        );
        assert!(
            err("ranma.option('p.x', { type = 'int', default = 1 }) ranma.set { p = { x = 'a' } }")
                .contains("whole number")
        );
        assert!(err("ranma.set { nope = 1 }").contains("unknown field `nope`"));
        assert!(err("ranma.get('nope.x')").contains("no option"));
        assert!(err("ranma.get('border.style')").contains("not loaded yet"));
    }

    #[test]
    fn settings_toml_wins_over_init_lua_and_the_theme() {
        let dir = config_tree(
            "panelfile",
            &[
                ("init.lua", "ranma.set { splash = false, mouse = 'hover' }"),
                (
                    "settings.toml",
                    "[set]\nmouse = 'off'\n[set.wm_mode]\nhint = false\n\n[set.gone]\nx = 1\n\n[theme.panes]\ndim_unfocused = 0.4\n",
                ),
            ],
        );
        let cfg = load(Some(&dir)).unwrap();
        assert_eq!(cfg.settings.mouse, MouseMode::Off);
        assert!(!cfg.settings.splash, "init.lua still says the rest");
        assert_eq!(cfg.settings.wm_mode_hint, None);
        assert_eq!(cfg.theme.panes.dim_unfocused, 0.4);
        assert!(cfg.warnings[0].contains("`gone`"), "{:?}", cfg.warnings);
        let v = cfg.values.borrow();
        let o = |k: &str| v.options.iter().find(|o| o.key == k).unwrap().clone();
        use crate::options::Layer;
        let mouse = o("mouse");
        assert_eq!(
            v.layers.value(&mouse, Layer::Default),
            Some(toml::Value::String("click".into()))
        );
        assert_eq!(
            v.layers.value(&mouse, Layer::File),
            Some(toml::Value::String("hover".into()))
        );
        assert_eq!(
            v.layers.value(&mouse, Layer::Panel),
            Some(toml::Value::String("off".into()))
        );
        let dim = o("panes.dim_unfocused");
        assert_eq!(
            v.layers.value(&dim, Layer::File),
            None,
            "the theme says 0, as the default does"
        );
        assert_eq!(v.layers.effective(&dim), Some(toml::Value::Float(0.4)));

        // Written back, it reads the same.
        let path = dir.join("settings.toml");
        let f = read_settings_file(&path).unwrap();
        write_settings_file(&path, &f).unwrap();
        assert_eq!(read_settings_file(&path).unwrap(), f);
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .starts_with("# Written by ranma's settings panel")
        );

        let bad = |text: &str| {
            let d = config_tree("panelbad", &[("settings.toml", text)]);
            load(Some(&d)).unwrap_err().to_string()
        };
        assert!(bad("[sett]\nx = 1\n").contains("unknown table `sett`"));
        assert!(bad("[set]\nmouse = 'sideways'\n").contains("settings.toml"));
        assert!(
            format!(
                "{:#}",
                load(Some(&config_tree(
                    "panelbad2",
                    &[("settings.toml", "[theme.panes]\ndim_unfocused = 'x'\n")]
                )))
                .unwrap_err()
            )
            .contains("settings.toml")
        );
    }

    #[test]
    fn plugins_read_and_write_json() {
        let cfg = load_from(None, None, None).unwrap();
        let v: i64 = cfg
            .lua
            .load(r#"local t = ranma.json.decode('[{"pane": "5", "n": 3, "busy": true}]') return t[1].n"#)
            .eval()
            .unwrap();
        assert_eq!(v, 3);
        let s: String = cfg
            .lua
            .load(r#"return ranma.json.encode({ a = 1 })"#)
            .eval()
            .unwrap();
        assert_eq!(s, r#"{"a":1}"#);
        let e = cfg
            .lua
            .load("ranma.json.decode('{oops')")
            .exec()
            .unwrap_err()
            .to_string();
        assert!(e.contains("ranma.json.decode"), "{e}");
    }
}

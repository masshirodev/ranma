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
}

#[derive(Debug, Clone)]
pub struct Bind {
    pub action: BindAction,
    /// Whether WM mode ends after this bind fires.
    pub exits_mode: bool,
    /// The action as written, for `--check-config` and error messages.
    pub label: String,
    /// A short name for the which-key hint (`{ desc = "..." }`); a Lua bind
    /// has no action to name it by otherwise.
    pub desc: Option<String>,
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
}

impl FromStr for Event {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "pane_open" => Event::PaneOpen,
            "pane_close" => Event::PaneClose,
            "focus_change" => Event::FocusChange,
            "workspace_change" => Event::WorkspaceChange,
            "session_switch" => Event::SessionSwitch,
            "mode_change" => Event::ModeChange,
            "config_reload" => Event::ConfigReload,
            "command_finished" => Event::CommandFinished,
            "driver_change" => Event::DriverChange,
            _ => return Err(format!("unknown event `{s}`")),
        })
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
            BindAction::Lua(_) => None,
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
}

/// What the `ranma` global writes into while the config runs.
#[derive(Default, Clone)]
struct Builder {
    settings: Settings,
    binds: HashMap<Chord, Bind>,
    global_binds: HashMap<Chord, Bind>,
    hooks: HashMap<Event, Vec<Rc<RegistryKey>>>,
    bar: BarLayout,
    modules: HashMap<String, ModuleDef>,
    workspaces_show_all: bool,
    workspaces_numbers_only: bool,
    workspaces_nested: NestedWorkspaces,
    rules: Vec<Rule>,
    session_accents: HashMap<String, theme::Color>,
    profiles: HashMap<String, Profile>,
    toolbars: Vec<(String, ToolbarDef)>,
    toolbars_shown: Vec<String>,
    layouts: std::collections::BTreeMap<String, crate::layouts::Spec>,
    /// `ranma.defer` and `ranma.every` made while loading, started with it.
    timers: crate::jobs::PendingTimers,
}

pub struct Config {
    pub settings: Settings,
    /// Keys looked up in WM mode, after the leader.
    pub binds: HashMap<Chord, Bind>,
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
    /// Stops a Lua call that runs too long; armed by every call into `lua`.
    pub watchdog: Watchdog,
    /// Its timers and the processes it spawned (`crate::jobs`).
    pub jobs: crate::jobs::Jobs,
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
    if let Some(p) = patch.preserve_split {
        s.preserve_split = p;
    }
    if let Some(on) = patch.splash {
        s.splash = on;
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

fn install_api(lua: &Lua, config_dir: Option<&Path>, jobs: &crate::jobs::Jobs) -> mlua::Result<()> {
    let ranma = lua.create_table()?;
    ranma.set("version", env!("CARGO_PKG_VERSION"))?;
    if let Some(dir) = config_dir {
        ranma.set("config_dir", dir.display().to_string())?;
    }

    ranma.set(
        "set",
        lua.create_function(|lua, value: Value| {
            let patch: SettingsPatch = lua
                .from_value(value)
                .map_err(|e| rt_err(format!("ranma.set: {e}")))?;
            let mut b = lua.app_data_mut::<Builder>().ok_or_else(loading_only)?;
            apply_settings(&mut b.settings, patch, "ranma.set").map_err(rt_err)
        })?,
    )?;

    ranma.set(
        "bind",
        lua.create_function(
            |lua, (keys, action, opts): (String, Value, Option<Table>)| {
                let chord: Chord = keys
                    .parse()
                    .map_err(|e| rt_err(format!("ranma.bind: key `{keys}`: {e}")))?;
                let mut exit_override: Option<bool> = None;
                let mut global = false;
                let mut desc: Option<String> = None;
                if let Some(t) = &opts {
                    for pair in t.pairs::<String, Value>() {
                        let (k, v) = pair?;
                        match (k.as_str(), v) {
                            ("exit", Value::Boolean(b)) => exit_override = Some(b),
                            ("global", Value::Boolean(b)) => global = b,
                            ("desc", Value::String(d)) => desc = Some(d.to_str()?.to_string()),
                            ("desc", other) => {
                                return Err(rt_err(format!(
                                    "ranma.bind(\"{keys}\"): `desc` must be a string, not {}",
                                    other.type_name()
                                )));
                            }
                            ("exit" | "global", other) => {
                                return Err(rt_err(format!(
                                    "ranma.bind(\"{keys}\"): `{k}` must be true or false, not {}",
                                    other.type_name()
                                )));
                            }
                            _ => {
                                return Err(rt_err(format!(
                                    "ranma.bind: unknown option `{k}` (expected exit, global, desc)"
                                )));
                            }
                        }
                    }
                }
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
                let bind = match action {
                    Value::String(s) => {
                        let s = s.to_str()?.to_string();
                        let parsed: Action = s
                            .parse()
                            .map_err(|e| rt_err(format!("ranma.bind(\"{keys}\"): {e}")))?;
                        Bind {
                            exits_mode: exit_override.unwrap_or(parsed.exits_mode_by_default()),
                            action: BindAction::Builtin(parsed),
                            label: s,
                            desc,
                        }
                    }
                    Value::Function(f) => Bind {
                        action: BindAction::Lua(Rc::new(lua.create_registry_value(f)?)),
                        exits_mode: exit_override.unwrap_or(false),
                        label: "<lua function>".into(),
                        desc,
                    },
                    other => {
                        return Err(rt_err(format!(
                            "ranma.bind(\"{keys}\"): action must be a string or a function, not {}",
                            other.type_name()
                        )));
                    }
                };
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
        "unbind",
        lua.create_function(|lua, keys: String| {
            let chord: Chord = keys
                .parse()
                .map_err(|e| rt_err(format!("ranma.unbind: key `{keys}`: {e}")))?;
            let mut b = lua.app_data_mut::<Builder>().ok_or_else(loading_only)?;
            b.binds.remove(&chord);
            b.global_binds.remove(&chord);
            Ok(())
        })?,
    )?;

    ranma.set(
        "unbind_all",
        lua.create_function(|lua, ()| {
            let mut b = lua.app_data_mut::<Builder>().ok_or_else(loading_only)?;
            b.binds.clear();
            b.global_binds.clear();
            Ok(())
        })?,
    )?;

    ranma.set(
        "on",
        lua.create_function(|lua, (event, f): (String, Function)| {
            let ev: Event = event
                .parse()
                .map_err(|e| rt_err(format!("ranma.on: {e}")))?;
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
                let allowed: &[&str] = if name == "workspaces" { &["show", "label", "nested"] } else { &[] };
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
    // mlua's error is not Send without its `send` feature, so it cannot go through
    // anyhow's `.context` directly; its Display is all we need from it anyway.
    install_api(&lua, config_dir, &jobs)
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
    let theme = theme::load(&builder.settings.theme, &theme::theme_dirs(config_dir))?;
    jobs.adopt(std::mem::take(&mut builder.timers));

    Ok(Config {
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
        global_binds: builder.global_binds,
        hooks: builder.hooks,
        bar: builder.bar,
        modules: builder.modules,
        workspaces_show_all: builder.workspaces_show_all,
        workspaces_numbers_only: builder.workspaces_numbers_only,
        workspaces_nested: builder.workspaces_nested,
        rules: builder.rules,
        session_accents: builder.session_accents,
        layouts: builder.layouts,
        theme,
        source: user_file,
        plugins,
        watchdog,
        jobs,
        lua,
    })
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
            BindAction::Lua(_) => None,
        }
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
            BindAction::Lua(_) => panic!("alt+s is a builtin"),
        }
        assert_eq!(
            builtin(&cfg, "m"),
            Some(Action::MoveWorkspaceToSession(None))
        );
        assert!(cfg.binds[&"t".parse().unwrap()].exits_mode);
        assert!(!cfg.binds[&"left".parse().unwrap()].exits_mode);
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
}

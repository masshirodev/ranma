//! Actions a bind can trigger, spelled like Hyprland dispatchers: `"focus left"`,
//! `"workspace 3"`, `"resize right 5"`, `"exec nvim"`.
//!
//! Every name is checked at config load. An action that only fails when pressed is a
//! bind the user finds broken weeks later, in the middle of something else.

use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

/// Where `snap` puts a float: a half, a quarter, or the middle at its own size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Snap {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}

impl Snap {
    const NAMES: [(&'static str, Snap); 9] = [
        ("left", Snap::Left),
        ("right", Snap::Right),
        ("top", Snap::Top),
        ("bottom", Snap::Bottom),
        ("top_left", Snap::TopLeft),
        ("top_right", Snap::TopRight),
        ("bottom_left", Snap::BottomLeft),
        ("bottom_right", Snap::BottomRight),
        ("center", Snap::Center),
    ];
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceTarget {
    /// 1-based, as the user counts them: `workspace 1` is the first.
    Index(u8),
    Next,
    Prev,
    /// The first workspace with no panes, like Hyprland's `workspace empty`.
    Empty,
    /// The one shown before this, in this session (tmux's `last-window`).
    Last,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionTarget {
    Name(String),
    Next,
    Prev,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    NewPane,
    /// Open a pane on this side of the focused one.
    NewPaneAt(Dir),
    ClosePane,
    Focus(Dir),
    /// Focus the next (or previous) pane of the workspace: its tiles in tree
    /// order, then its floats, wrapping.
    FocusCycle {
        forward: bool,
    },
    /// Focus the pane focused before this one, here (tmux's `last-pane`).
    FocusLast,
    /// Swap the focused pane with its neighbour in that direction.
    Move(Dir),
    /// Grow the focused pane's edge in that direction by this many cells.
    Resize(Dir, u16),
    ToggleSplit,
    /// Trade places with the master: the first pane of the tree (the one on the
    /// left in layout "master"); the master trades with the next one.
    SwapMaster,
    /// Mark or unmark the focused pane for synchronized input.
    SyncToggle,
    /// Unmark every pane.
    SyncClear,
    /// Give every split in the workspace equal shares.
    Equalize,
    /// Rebuild the workspace's tiles into one of tmux's preset layouts.
    SelectLayout(crate::layout::Preset),
    /// The preset after the one the workspace showed last (tmux's `Space`).
    NextLayout,
    /// Save the workspace's tiles as a layout; without a name, ask for one.
    SaveLayout(Option<String>),
    /// Apply a layout to the workspace; without a name, pick one.
    LoadLayout(Option<String>),
    /// Bring back the snapshot this server set aside when it started: each
    /// command typed and waiting, or run (`restore run`).
    Restore {
        run: bool,
    },
    ToggleFloating,
    /// Size the focused pane as a float, in percent of the workspace (width,
    /// height), keeping its centre. A tile is floated first.
    FloatSize(u8, u8),
    /// Put the focused pane, floated first if it tiles, on a half, a quarter, or
    /// in the middle.
    Snap(Snap),
    /// Raise the next floating pane, cycling through the pile.
    CycleFloats,
    /// Pull ranma's source and install it, in a floating pane.
    Update,
    /// Leave this terminal; the server and everything in it keep running.
    Detach,
    /// List the ranma servers; Enter moves this terminal to one.
    ServerSwitcher,
    /// Move this terminal to the server with this name (`ranma ls` lists them).
    Attach(String),
    /// Use a `ranma.profile` over the base configuration; `None` goes back to it.
    Profile(Option<String>),
    /// Show (`Some(true)`), hide or flip a `ranma.toolbar`.
    Toolbar(String, Option<bool>),
    /// Type this chord into the focused pane, as if pressed there: what a
    /// phone's keyboard cannot type. Binds are not looked up.
    Send(crate::keys::Chord),
    /// Hold a modifier for the next key; tapped twice quickly, until tapped
    /// again.
    Latch(Latch),
    /// The focused pane's menu, with no pointer to open it at (a button, a key).
    PaneMenu,
    /// The workspaces of the shown session, filterable; a new name opens one.
    WorkspaceSwitcher,
    ToggleGroup,
    GroupNext,
    GroupPrev,
    Fullscreen,
    Workspace(WorkspaceTarget),
    MoveToWorkspace(WorkspaceTarget),
    /// Move the pane without following it.
    MoveToWorkspaceSilent(WorkspaceTarget),
    ScratchpadToggle,
    MoveToScratchpad,
    PaneSwitcher,
    SessionSwitcher,
    /// Every bind, filterable; Enter runs the selected one.
    Help,
    /// The same picker listing every action, bound or not, and running a typed one.
    CommandPalette,
    /// The settings panel: every option, edited in place.
    Settings,
    /// Keyboard scrollback and selection (vi keys) in the focused pane.
    CopyMode,
    /// Label the links on the focused pane's screen; typing a label copies it,
    /// in capitals opens it.
    Hints,
    /// Watch the focused pane for silence: when it has printed and then stays
    /// quiet, say so (tmux's `monitor-silence`). Bare toggles with the
    /// `monitor_silence` setting; a number of seconds watches for that long a
    /// quiet; `off` stops.
    MonitorSilence(Option<Option<u32>>),
    /// Number the panes on screen; typing a number focuses that pane (tmux's
    /// `display-panes`).
    DisplayPanes,
    /// Search the focused pane's history (copy mode with the search prompt open).
    Search,
    /// Create a session (named, or numbered) and switch to it.
    NewSession(Option<String>),
    Session(SessionTarget),
    /// Send the current workspace, whole, to another session and follow it;
    /// without a target, pick the session (or type a new one).
    MoveWorkspaceToSession(Option<SessionTarget>),
    /// Colour the current session's focused border, active workspace and name
    /// in the bar; `None` goes back to the config's accent, else the theme's.
    SessionAccent(Option<crate::theme::Color>),
    /// Rename the current session; without a name, ask for one.
    RenameSession(Option<String>),
    /// Name the current workspace (an empty name clears it); without one, ask.
    RenameWorkspace(Option<String>),
    /// Name the focused pane, overriding its title (empty clears); without one, ask.
    RenamePane(Option<String>),
    /// Open a new pane running this command line.
    Exec(String),
    ExitMode,
    /// Enter WM mode as the leader does, or leave it when in it: the leader
    /// for a toolbar button, a mouse or a script.
    Leader,
    /// Send the leader chord itself to the focused program.
    SendLeader,
    /// Type the paths of the clipboard's copied files, else of its image, into
    /// the focused pane, uploaded first when the pane runs ssh (DESIGN.md,
    /// "Pasting files").
    PasteImage,
    ReloadConfig,
    /// Quit ranma, closing every pane. Asks first unless `now` (`quit now`).
    Quit {
        now: bool,
    },
}

impl Action {
    /// Whether WM mode should end after this action when the bind does not say.
    ///
    /// Opening a pane, or summoning the scratchpad, ends it because the next thing
    /// you do is type there; focus, resize and move keep it because they come in runs.
    pub fn exits_mode_by_default(&self) -> bool {
        matches!(
            self,
            Action::NewPane
                | Action::NewPaneAt(_)
                | Action::ScratchpadToggle
                | Action::Exec(_)
                | Action::ExitMode
                | Action::SendLeader
                | Action::PasteImage
                | Action::PaneSwitcher
                | Action::WorkspaceSwitcher
                | Action::SaveLayout(_)
                | Action::LoadLayout(_)
                | Action::PaneMenu
                | Action::SessionSwitcher
                | Action::Help
                | Action::CommandPalette
                | Action::Settings
                | Action::CopyMode
                | Action::Hints
                | Action::DisplayPanes
                | Action::Search
                | Action::NewSession(_)
                | Action::MoveWorkspaceToSession(_)
                | Action::RenameSession(_)
                | Action::RenameWorkspace(_)
                | Action::RenamePane(_)
                | Action::Quit { .. }
                | Action::Update
                | Action::Detach
                | Action::ServerSwitcher
                | Action::Attach(_)
        )
    }
}

/// A modifier a `latch` holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Latch {
    Ctrl,
    Alt,
    Shift,
}

impl Latch {
    pub fn name(self) -> &'static str {
        match self {
            Latch::Ctrl => "ctrl",
            Latch::Alt => "alt",
            Latch::Shift => "shift",
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ActionError {
    #[error("unknown action `{0}`")]
    Unknown(String),
    #[error("`{action}` needs {expected}")]
    MissingArg {
        action: String,
        expected: &'static str,
    },
    #[error("`{action}`: `{arg}` is not {expected}")]
    BadArg {
        action: String,
        arg: String,
        expected: &'static str,
    },
    #[error("`{0}` takes no argument")]
    UnexpectedArg(String),
}

/// Every action by name, with what it takes: `<...>` is required, `[...]`
/// optional, empty is nothing. The command palette lists these, so an action
/// missing here can be bound but not found; a test holds the two together.
pub const CATALOGUE: &[(&str, &str)] = &[
    ("new_pane", "[left|right|up|down]"),
    ("close_pane", ""),
    ("focus", "<left|right|up|down|next|prev|last>"),
    ("move", "<left|right|up|down>"),
    ("resize", "<left|right|up|down> [cells]"),
    ("toggle_split", ""),
    ("equalize", ""),
    (
        "select_layout",
        "<even-horizontal|even-vertical|main-horizontal|main-vertical|tiled>",
    ),
    ("next_layout", ""),
    ("save_layout", "[name]"),
    ("load_layout", "[name]"),
    ("restore", "[run]"),
    ("swap_master", ""),
    ("sync_toggle", ""),
    ("sync_clear", ""),
    ("toggle_floating", ""),
    ("float_size", "<width%> [height%]"),
    (
        "snap",
        "<left|right|top|bottom|top_left|top_right|bottom_left|bottom_right|center>",
    ),
    ("cycle_floats", ""),
    ("toggle_group", ""),
    ("group_next", ""),
    ("group_prev", ""),
    ("fullscreen", ""),
    ("workspace", "<1-99|next|prev|empty|last>"),
    ("move_to_workspace", "<1-99|next|prev|empty|last>"),
    ("move_to_workspace_silent", "<1-99|next|prev|empty|last>"),
    ("scratchpad_toggle", ""),
    ("move_to_scratchpad", ""),
    ("pane_switcher", ""),
    ("session_switcher", ""),
    ("help", ""),
    ("command_palette", ""),
    ("settings", ""),
    ("copy_mode", ""),
    ("hints", ""),
    ("display_panes", ""),
    ("monitor_silence", "[seconds|off]"),
    ("search", ""),
    ("new_session", "[name]"),
    ("session", "<name|next|prev>"),
    ("rename_session", "[name]"),
    ("session_accent", "<#rrggbb|colour|none>"),
    ("move_workspace_to_session", "[name|next|prev]"),
    ("rename_workspace", "[name]"),
    ("rename_pane", "[name]"),
    ("exec", "<command line>"),
    ("exit_mode", ""),
    ("leader", ""),
    ("send_leader", ""),
    ("paste_image", ""),
    ("reload_config", ""),
    ("update", ""),
    ("detach", ""),
    ("server_switcher", ""),
    ("attach", "<server>"),
    ("profile", "<name|none>"),
    ("toolbar", "<name> [on|off|toggle]"),
    ("send", "<key chord>"),
    ("pane_menu", ""),
    ("workspace_switcher", ""),
    ("latch", "<ctrl|alt|shift>"),
    ("quit", "[now]"),
];

/// Whether an action from the catalogue cannot run without an argument.
pub fn needs_arg(hint: &str) -> bool {
    hint.starts_with('<')
}

const DIR: &str = "a direction (left, right, up, down)";
const WS: &str = "a workspace (1-99, next, prev, empty, last)";

fn parse_dir(action: &str, arg: Option<&str>) -> Result<Dir, ActionError> {
    let arg = arg.ok_or(ActionError::MissingArg {
        action: action.into(),
        expected: DIR,
    })?;
    match arg {
        "left" | "l" => Ok(Dir::Left),
        "right" | "r" => Ok(Dir::Right),
        "up" | "u" => Ok(Dir::Up),
        "down" | "d" => Ok(Dir::Down),
        _ => Err(ActionError::BadArg {
            action: action.into(),
            arg: arg.into(),
            expected: DIR,
        }),
    }
}

/// A workspace as a bind spells it: `3`, `next`, `prev`, `empty`.
pub fn parse_workspace(s: &str) -> Result<WorkspaceTarget, ActionError> {
    parse_ws("workspace", Some(s))
}

fn parse_ws(action: &str, arg: Option<&str>) -> Result<WorkspaceTarget, ActionError> {
    let arg = arg.ok_or(ActionError::MissingArg {
        action: action.into(),
        expected: WS,
    })?;
    match arg {
        "next" | "+1" => Ok(WorkspaceTarget::Next),
        "prev" | "-1" => Ok(WorkspaceTarget::Prev),
        "empty" => Ok(WorkspaceTarget::Empty),
        "last" => Ok(WorkspaceTarget::Last),
        n => match n.parse::<u8>() {
            Ok(i @ 1..=99) => Ok(WorkspaceTarget::Index(i)),
            _ => Err(ActionError::BadArg {
                action: action.into(),
                arg: arg.into(),
                expected: WS,
            }),
        },
    }
}

impl FromStr for Action {
    type Err = ActionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let (name, rest) = match s.split_once(char::is_whitespace) {
            Some((n, r)) => (n, Some(r.trim()).filter(|r| !r.is_empty())),
            None => (s, None),
        };
        let mut words = rest.map(|r| r.split_whitespace()).into_iter().flatten();
        let first = words.next();
        let second = words.next();
        let no_arg = |a: Action| {
            if rest.is_some() {
                Err(ActionError::UnexpectedArg(name.into()))
            } else {
                Ok(a)
            }
        };

        match name {
            "new_pane" => match first {
                None => Ok(Action::NewPane),
                Some(_) => Ok(Action::NewPaneAt(parse_dir(name, first)?)),
            },
            "close_pane" => no_arg(Action::ClosePane),
            "focus" => match first {
                Some("next") => Ok(Action::FocusCycle { forward: true }),
                Some("prev") => Ok(Action::FocusCycle { forward: false }),
                Some("last") => Ok(Action::FocusLast),
                _ => Ok(Action::Focus(parse_dir(name, first)?)),
            },
            "move" => Ok(Action::Move(parse_dir(name, first)?)),
            "resize" => {
                let dir = parse_dir(name, first)?;
                let amount = match second {
                    None => 2,
                    Some(n) => {
                        n.parse::<u16>()
                            .ok()
                            .filter(|n| *n > 0)
                            .ok_or(ActionError::BadArg {
                                action: name.into(),
                                arg: n.into(),
                                expected: "a positive cell count",
                            })?
                    }
                };
                Ok(Action::Resize(dir, amount))
            }
            "toggle_split" => no_arg(Action::ToggleSplit),
            "equalize" => no_arg(Action::Equalize),
            "select_layout" => {
                const PRESETS: &str =
                    "even-horizontal, even-vertical, main-horizontal, main-vertical or tiled";
                match (first, second) {
                    (None, _) => Err(ActionError::MissingArg {
                        action: name.into(),
                        expected: PRESETS,
                    }),
                    (Some(p), None) => crate::layout::Preset::from_name(p)
                        .map(Action::SelectLayout)
                        .ok_or(ActionError::BadArg {
                            action: name.into(),
                            arg: p.into(),
                            expected: PRESETS,
                        }),
                    (Some(_), Some(_)) => Err(ActionError::BadArg {
                        action: name.into(),
                        arg: rest.unwrap_or_default().into(),
                        expected: PRESETS,
                    }),
                }
            }
            "next_layout" => no_arg(Action::NextLayout),
            "restore" => match rest {
                None => Ok(Action::Restore { run: false }),
                Some("run") => Ok(Action::Restore { run: true }),
                Some(other) => Err(ActionError::BadArg {
                    action: name.into(),
                    arg: other.into(),
                    expected: "nothing (commands wait on the prompt), or `run`",
                }),
            },
            "save_layout" | "load_layout" => {
                let named = match (first, second) {
                    (None, _) => None,
                    (Some(n), None) if crate::layouts::valid_name(n) => Some(n.to_string()),
                    _ => {
                        return Err(ActionError::BadArg {
                            action: name.into(),
                            arg: rest.unwrap_or_default().into(),
                            expected: "one layout name, without slashes or spaces",
                        });
                    }
                };
                Ok(if name == "save_layout" {
                    Action::SaveLayout(named)
                } else {
                    Action::LoadLayout(named)
                })
            }
            "swap_master" => no_arg(Action::SwapMaster),
            "sync_toggle" => no_arg(Action::SyncToggle),
            "sync_clear" => no_arg(Action::SyncClear),
            "toggle_floating" => no_arg(Action::ToggleFloating),
            "float_size" => {
                let pct = |arg: Option<&str>| -> Result<Option<u8>, ActionError> {
                    let Some(a) = arg else { return Ok(None) };
                    a.trim_end_matches('%')
                        .parse::<u8>()
                        .ok()
                        .filter(|n| (10..=100).contains(n))
                        .map(Some)
                        .ok_or(ActionError::BadArg {
                            action: name.into(),
                            arg: a.into(),
                            expected: "a percentage from 10 to 100",
                        })
                };
                let w = pct(first)?.ok_or(ActionError::MissingArg {
                    action: name.into(),
                    expected: "a width in percent (and optionally a height)",
                })?;
                let h = pct(second)?.unwrap_or(w);
                if words.next().is_some() {
                    return Err(ActionError::BadArg {
                        action: name.into(),
                        arg: rest.unwrap_or_default().into(),
                        expected: "a width and a height, nothing more",
                    });
                }
                Ok(Action::FloatSize(w, h))
            }
            "snap" => {
                const WHERE: &str = "left, right, top, bottom, top_left, top_right, bottom_left, bottom_right or center";
                let arg = first.ok_or(ActionError::MissingArg {
                    action: name.into(),
                    expected: WHERE,
                })?;
                Snap::NAMES
                    .iter()
                    .find(|(n, _)| *n == arg && second.is_none())
                    .map(|(_, s)| Action::Snap(*s))
                    .ok_or(ActionError::BadArg {
                        action: name.into(),
                        arg: rest.unwrap_or_default().into(),
                        expected: WHERE,
                    })
            }
            "cycle_floats" => no_arg(Action::CycleFloats),
            "update" => no_arg(Action::Update),
            "detach" => no_arg(Action::Detach),
            "server_switcher" => no_arg(Action::ServerSwitcher),
            "pane_menu" => no_arg(Action::PaneMenu),
            "workspace_switcher" => no_arg(Action::WorkspaceSwitcher),
            "toolbar" => match (first, second) {
                (Some(n), op) => {
                    let show = match op {
                        Some("on") => Some(true),
                        Some("off") => Some(false),
                        None | Some("toggle") => None,
                        Some(other) => {
                            return Err(ActionError::BadArg {
                                action: name.into(),
                                arg: other.into(),
                                expected: "on, off or toggle",
                            });
                        }
                    };
                    Ok(Action::Toolbar(n.to_string(), show))
                }
                (None, _) => Err(ActionError::MissingArg {
                    action: name.into(),
                    expected: "a toolbar name (from ranma.toolbar), then on, off or toggle",
                }),
            },
            "send" => match (first, second) {
                (Some(k), None) => k
                    .parse()
                    .map(Action::Send)
                    .map_err(|_| ActionError::BadArg {
                        action: name.into(),
                        arg: k.into(),
                        expected: "one key chord, as binds spell them (ctrl+c, esc, alt+.)",
                    }),
                (None, _) => Err(ActionError::MissingArg {
                    action: name.into(),
                    expected: "a key chord (ctrl+c, esc, alt+.)",
                }),
                (Some(_), Some(_)) => Err(ActionError::BadArg {
                    action: name.into(),
                    arg: rest.unwrap_or_default().into(),
                    expected: "one key chord",
                }),
            },
            "latch" => match (first, second) {
                (Some("ctrl"), None) => Ok(Action::Latch(Latch::Ctrl)),
                (Some("alt"), None) => Ok(Action::Latch(Latch::Alt)),
                (Some("shift"), None) => Ok(Action::Latch(Latch::Shift)),
                (None, _) => Err(ActionError::MissingArg {
                    action: name.into(),
                    expected: "ctrl, alt or shift",
                }),
                (Some(_), _) => Err(ActionError::BadArg {
                    action: name.into(),
                    arg: rest.unwrap_or_default().into(),
                    expected: "ctrl, alt or shift",
                }),
            },
            "profile" => match (first, second) {
                (Some("none"), None) => Ok(Action::Profile(None)),
                (Some(n), None) => Ok(Action::Profile(Some(n.to_string()))),
                (None, _) => Err(ActionError::MissingArg {
                    action: name.into(),
                    expected: "a profile name (from ranma.profile) or none",
                }),
                (Some(_), Some(_)) => Err(ActionError::BadArg {
                    action: name.into(),
                    arg: rest.unwrap_or_default().into(),
                    expected: "one profile name, or none",
                }),
            },
            "attach" => match (first, second) {
                (Some(n), None) => Ok(Action::Attach(n.to_string())),
                (None, _) => Err(ActionError::MissingArg {
                    action: name.into(),
                    expected: "a server name (as `ranma ls` shows)",
                }),
                (Some(_), Some(_)) => Err(ActionError::BadArg {
                    action: name.into(),
                    arg: rest.unwrap_or_default().into(),
                    expected: "one server name",
                }),
            },
            "toggle_group" => no_arg(Action::ToggleGroup),
            "group_next" => no_arg(Action::GroupNext),
            "group_prev" => no_arg(Action::GroupPrev),
            "fullscreen" => no_arg(Action::Fullscreen),
            "workspace" => Ok(Action::Workspace(parse_ws(name, first)?)),
            "move_to_workspace" => Ok(Action::MoveToWorkspace(parse_ws(name, first)?)),
            "move_to_workspace_silent" => Ok(Action::MoveToWorkspaceSilent(parse_ws(name, first)?)),
            "scratchpad_toggle" => no_arg(Action::ScratchpadToggle),
            "move_to_scratchpad" => no_arg(Action::MoveToScratchpad),
            "pane_switcher" => no_arg(Action::PaneSwitcher),
            "session_switcher" => no_arg(Action::SessionSwitcher),
            "help" => no_arg(Action::Help),
            "command_palette" => no_arg(Action::CommandPalette),
            "settings" => no_arg(Action::Settings),
            "copy_mode" => no_arg(Action::CopyMode),
            "hints" => no_arg(Action::Hints),
            "display_panes" => no_arg(Action::DisplayPanes),
            "monitor_silence" => match (first, second) {
                (None, _) => Ok(Action::MonitorSilence(None)),
                (Some("off"), None) => Ok(Action::MonitorSilence(Some(None))),
                (Some(n), None) => n
                    .parse::<u32>()
                    .ok()
                    .filter(|n| (1..=86_400).contains(n))
                    .map(|n| Action::MonitorSilence(Some(Some(n))))
                    .ok_or(ActionError::BadArg {
                        action: name.into(),
                        arg: n.into(),
                        expected: "seconds (1-86400) or off",
                    }),
                (Some(_), Some(_)) => Err(ActionError::BadArg {
                    action: name.into(),
                    arg: rest.unwrap_or_default().into(),
                    expected: "seconds (1-86400) or off",
                }),
            },
            "search" => no_arg(Action::Search),
            "new_session" => Ok(Action::NewSession(rest.map(str::to_string))),
            "rename_session" => Ok(Action::RenameSession(rest.map(str::to_string))),
            "session_accent" => match rest {
                None => Err(ActionError::MissingArg {
                    action: name.into(),
                    expected: "a colour (#rrggbb, 0-255, an ANSI name) or none",
                }),
                Some("none") => Ok(Action::SessionAccent(None)),
                Some(c) => c
                    .parse()
                    .map(|c| Action::SessionAccent(Some(c)))
                    .map_err(|_| ActionError::BadArg {
                        action: name.into(),
                        arg: c.into(),
                        expected: "a colour (#rrggbb, 0-255, an ANSI name) or none",
                    }),
            },
            "rename_workspace" => Ok(Action::RenameWorkspace(rest.map(str::to_string))),
            "rename_pane" => Ok(Action::RenamePane(rest.map(str::to_string))),
            "session" => {
                rest.map(|r| Action::Session(parse_session(r)))
                    .ok_or(ActionError::MissingArg {
                        action: name.into(),
                        expected: "a session name, next or prev",
                    })
            }
            "move_workspace_to_session" => {
                Ok(Action::MoveWorkspaceToSession(rest.map(parse_session)))
            }
            "exec" => {
                rest.map(|cmd| Action::Exec(cmd.to_string()))
                    .ok_or(ActionError::MissingArg {
                        action: name.into(),
                        expected: "a command line",
                    })
            }
            "exit_mode" => no_arg(Action::ExitMode),
            "leader" => no_arg(Action::Leader),
            "send_leader" => no_arg(Action::SendLeader),
            "paste_image" => no_arg(Action::PasteImage),
            "reload_config" => no_arg(Action::ReloadConfig),
            "quit" => match rest {
                None => Ok(Action::Quit { now: false }),
                Some("now") => Ok(Action::Quit { now: true }),
                Some(other) => Err(ActionError::BadArg {
                    action: name.into(),
                    arg: other.into(),
                    expected: "nothing, or `now` to skip the question",
                }),
            },
            _ => Err(ActionError::Unknown(name.into())),
        }
    }
}

fn parse_session(s: &str) -> SessionTarget {
    match s {
        "next" => SessionTarget::Next,
        "prev" => SessionTarget::Prev,
        name => SessionTarget::Name(name.to_string()),
    }
}

impl fmt::Display for SessionTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionTarget::Name(n) => f.write_str(n),
            SessionTarget::Next => f.write_str("next"),
            SessionTarget::Prev => f.write_str("prev"),
        }
    }
}

impl fmt::Display for Dir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Dir::Left => "left",
            Dir::Right => "right",
            Dir::Up => "up",
            Dir::Down => "down",
        })
    }
}

impl fmt::Display for WorkspaceTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkspaceTarget::Index(i) => write!(f, "{i}"),
            WorkspaceTarget::Next => f.write_str("next"),
            WorkspaceTarget::Prev => f.write_str("prev"),
            WorkspaceTarget::Empty => f.write_str("empty"),
            WorkspaceTarget::Last => f.write_str("last"),
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Action::NewPane => f.write_str("new_pane"),
            Action::NewPaneAt(d) => write!(f, "new_pane {d}"),
            Action::ClosePane => f.write_str("close_pane"),
            Action::Focus(d) => write!(f, "focus {d}"),
            Action::FocusCycle { forward: true } => f.write_str("focus next"),
            Action::FocusCycle { forward: false } => f.write_str("focus prev"),
            Action::FocusLast => f.write_str("focus last"),
            Action::Move(d) => write!(f, "move {d}"),
            Action::Resize(d, n) => write!(f, "resize {d} {n}"),
            Action::ToggleSplit => f.write_str("toggle_split"),
            Action::Equalize => f.write_str("equalize"),
            Action::SelectLayout(p) => write!(f, "select_layout {}", p.name()),
            Action::NextLayout => f.write_str("next_layout"),
            Action::SaveLayout(None) => f.write_str("save_layout"),
            Action::SaveLayout(Some(n)) => write!(f, "save_layout {n}"),
            Action::LoadLayout(None) => f.write_str("load_layout"),
            Action::LoadLayout(Some(n)) => write!(f, "load_layout {n}"),
            Action::Restore { run: false } => f.write_str("restore"),
            Action::Restore { run: true } => f.write_str("restore run"),
            Action::SwapMaster => f.write_str("swap_master"),
            Action::SyncToggle => f.write_str("sync_toggle"),
            Action::SyncClear => f.write_str("sync_clear"),
            Action::ToggleFloating => f.write_str("toggle_floating"),
            Action::FloatSize(w, h) => write!(f, "float_size {w} {h}"),
            Action::Snap(s) => {
                let name = Snap::NAMES.iter().find(|(_, v)| v == s).map(|(n, _)| *n);
                write!(f, "snap {}", name.unwrap_or("center"))
            }
            Action::CycleFloats => f.write_str("cycle_floats"),
            Action::Update => f.write_str("update"),
            Action::Detach => f.write_str("detach"),
            Action::ServerSwitcher => f.write_str("server_switcher"),
            Action::Attach(n) => write!(f, "attach {n}"),
            Action::Toolbar(n, None) => write!(f, "toolbar {n} toggle"),
            Action::Toolbar(n, Some(true)) => write!(f, "toolbar {n} on"),
            Action::Toolbar(n, Some(false)) => write!(f, "toolbar {n} off"),
            Action::Send(c) => write!(f, "send {c}"),
            Action::PaneMenu => f.write_str("pane_menu"),
            Action::WorkspaceSwitcher => f.write_str("workspace_switcher"),
            Action::Latch(m) => write!(f, "latch {}", m.name()),
            Action::Profile(None) => f.write_str("profile none"),
            Action::Profile(Some(n)) => write!(f, "profile {n}"),
            Action::ToggleGroup => f.write_str("toggle_group"),
            Action::GroupNext => f.write_str("group_next"),
            Action::GroupPrev => f.write_str("group_prev"),
            Action::Fullscreen => f.write_str("fullscreen"),
            Action::Workspace(w) => write!(f, "workspace {w}"),
            Action::MoveToWorkspace(w) => write!(f, "move_to_workspace {w}"),
            Action::MoveToWorkspaceSilent(w) => write!(f, "move_to_workspace_silent {w}"),
            Action::ScratchpadToggle => f.write_str("scratchpad_toggle"),
            Action::MoveToScratchpad => f.write_str("move_to_scratchpad"),
            Action::PaneSwitcher => f.write_str("pane_switcher"),
            Action::SessionSwitcher => f.write_str("session_switcher"),
            Action::Help => f.write_str("help"),
            Action::CommandPalette => f.write_str("command_palette"),
            Action::Settings => f.write_str("settings"),
            Action::CopyMode => f.write_str("copy_mode"),
            Action::Hints => f.write_str("hints"),
            Action::DisplayPanes => f.write_str("display_panes"),
            Action::MonitorSilence(None) => f.write_str("monitor_silence"),
            Action::MonitorSilence(Some(None)) => f.write_str("monitor_silence off"),
            Action::MonitorSilence(Some(Some(n))) => write!(f, "monitor_silence {n}"),
            Action::Search => f.write_str("search"),
            Action::NewSession(None) => f.write_str("new_session"),
            Action::NewSession(Some(n)) => write!(f, "new_session {n}"),
            Action::SessionAccent(None) => f.write_str("session_accent none"),
            Action::SessionAccent(Some(c)) => write!(f, "session_accent {c}"),
            Action::RenameSession(None) => f.write_str("rename_session"),
            Action::RenameSession(Some(n)) => write!(f, "rename_session {n}"),
            Action::RenameWorkspace(None) => f.write_str("rename_workspace"),
            Action::RenameWorkspace(Some(n)) => write!(f, "rename_workspace {n}"),
            Action::RenamePane(None) => f.write_str("rename_pane"),
            Action::RenamePane(Some(n)) => write!(f, "rename_pane {n}"),
            Action::Session(t) => write!(f, "session {t}"),
            Action::MoveWorkspaceToSession(None) => f.write_str("move_workspace_to_session"),
            Action::MoveWorkspaceToSession(Some(t)) => write!(f, "move_workspace_to_session {t}"),
            Action::Exec(cmd) => write!(f, "exec {cmd}"),
            Action::ExitMode => f.write_str("exit_mode"),
            Action::Leader => f.write_str("leader"),
            Action::SendLeader => f.write_str("send_leader"),
            Action::PasteImage => f.write_str("paste_image"),
            Action::ReloadConfig => f.write_str("reload_config"),
            Action::Quit { now: false } => f.write_str("quit"),
            Action::Quit { now: true } => f.write_str("quit now"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(s: &str) -> Action {
        s.parse().unwrap()
    }

    #[test]
    fn parses_dispatchers_with_arguments() {
        assert_eq!(a("focus left"), Action::Focus(Dir::Left));
        assert_eq!(a("resize right"), Action::Resize(Dir::Right, 2));
        assert_eq!(a("resize up 5"), Action::Resize(Dir::Up, 5));
        assert_eq!(
            a("workspace 3"),
            Action::Workspace(WorkspaceTarget::Index(3))
        );
        assert_eq!(
            a("move_to_workspace_silent next"),
            Action::MoveToWorkspaceSilent(WorkspaceTarget::Next)
        );
        assert_eq!(
            a("exec nvim  -c 'set nu'"),
            Action::Exec("nvim  -c 'set nu'".into())
        );
    }

    #[test]
    fn layouts_are_tmuxs_presets_by_name() {
        use crate::layout::Preset;
        assert_eq!(
            a("select_layout main-vertical"),
            Action::SelectLayout(Preset::MainVertical)
        );
        assert_eq!(a("next_layout"), Action::NextLayout);
        assert_eq!(a("load_layout"), Action::LoadLayout(None));
        assert_eq!(a("restore"), Action::Restore { run: false });
        assert_eq!(a("restore run"), Action::Restore { run: true });
        assert!("restore now".parse::<Action>().is_err());
        assert_eq!(a("save_layout dev"), Action::SaveLayout(Some("dev".into())));
        assert!(matches!(
            "load_layout a/b".parse::<Action>(),
            Err(ActionError::BadArg { .. })
        ));
        assert!(matches!(
            "save_layout two words".parse::<Action>(),
            Err(ActionError::BadArg { .. })
        ));
        assert!(matches!(
            "select_layout".parse::<Action>(),
            Err(ActionError::MissingArg { .. })
        ));
        assert!(matches!(
            "select_layout main".parse::<Action>(),
            Err(ActionError::BadArg { .. })
        ));
        for p in Preset::ALL {
            let act = Action::SelectLayout(p);
            assert_eq!(a(&act.to_string()), act);
        }
    }

    #[test]
    fn rejects_bad_actions_with_the_reason() {
        assert_eq!(
            "fcous left".parse::<Action>(),
            Err(ActionError::Unknown("fcous".into()))
        );
        assert!(matches!(
            "focus".parse::<Action>(),
            Err(ActionError::MissingArg { .. })
        ));
        assert!(matches!(
            "focus sideways".parse::<Action>(),
            Err(ActionError::BadArg { .. })
        ));
        assert!(matches!(
            "workspace 0".parse::<Action>(),
            Err(ActionError::BadArg { .. })
        ));
        assert!(matches!(
            "resize left 0".parse::<Action>(),
            Err(ActionError::BadArg { .. })
        ));
        assert!(matches!(
            "new_pane now".parse::<Action>(),
            Err(ActionError::BadArg { .. })
        ));
        assert_eq!(
            "close_pane now".parse::<Action>(),
            Err(ActionError::UnexpectedArg("close_pane".into()))
        );
        assert!(matches!(
            "exec".parse::<Action>(),
            Err(ActionError::MissingArg { .. })
        ));
        assert_eq!(
            a("float_size 60%"),
            Action::FloatSize(60, 60),
            "one number is both sides"
        );
        for bad in [
            "float_size 5",
            "float_size 60 101",
            "float_size 50 50 50",
            "snap middle",
            "snap left right",
        ] {
            assert!(
                matches!(bad.parse::<Action>(), Err(ActionError::BadArg { .. })),
                "{bad}"
            );
        }
        assert!(matches!(
            "session_accent pink".parse::<Action>(),
            Err(ActionError::BadArg { .. })
        ));
        assert!(matches!(
            "attach".parse::<Action>(),
            Err(ActionError::MissingArg { .. })
        ));
        assert!(matches!(
            "attach 1 2".parse::<Action>(),
            Err(ActionError::BadArg { .. })
        ));
    }

    #[test]
    fn display_round_trips() {
        for s in [
            "new_pane",
            "new_pane down",
            "focus down",
            "resize left 3",
            "workspace empty",
            "move_to_workspace 10",
            "exec htop",
            "help",
            "equalize",
            "sync_toggle",
            "sync_clear",
            "swap_master",
            "float_size 60 40",
            "snap top_right",
            "snap center",
            "command_palette",
            "settings",
            "copy_mode",
            "hints",
            "search",
            "new_session",
            "new_session work",
            "session next",
            "session kumiko",
            "move_workspace_to_session",
            "move_workspace_to_session prev",
            "move_workspace_to_session ai-projects",
            "rename_session",
            "session_accent #ff6a6a",
            "session_accent bright-red",
            "session_accent none",
            "rename_workspace web",
            "rename_pane",
            "quit",
            "quit now",
            "server_switcher",
            "attach 2",
        ] {
            assert_eq!(a(&a(s).to_string()), a(s), "{s}");
        }
    }

    #[test]
    fn the_catalogue_is_every_action() {
        for (name, hint) in CATALOGUE {
            match name.parse::<Action>() {
                Ok(a) => {
                    assert!(
                        !needs_arg(hint),
                        "{name} parses bare but says it needs {hint}"
                    );
                    assert_eq!(a.to_string().split(' ').next(), Some(*name));
                }
                Err(ActionError::MissingArg { .. }) => assert!(needs_arg(hint), "{name}"),
                Err(e) => panic!("{name}: {e}"),
            }
        }
        // And the other way: every action the default config binds is listed.
        let cfg = crate::config::load_from(None, None, None).unwrap();
        for bind in cfg.binds.values().chain(cfg.global_binds.values()) {
            if let crate::config::BindAction::Builtin(a) = &bind.action {
                let s = a.to_string();
                let name = s.split(' ').next().unwrap();
                assert!(CATALOGUE.iter().any(|(n, _)| *n == name), "{name}");
            }
        }
    }
}

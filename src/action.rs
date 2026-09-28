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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceTarget {
    /// 1-based, as the user counts them: `workspace 1` is the first.
    Index(u8),
    Next,
    Prev,
    /// The first workspace with no panes, like Hyprland's `workspace empty`.
    Empty,
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
    ClosePane,
    Focus(Dir),
    /// Swap the focused pane with its neighbour in that direction.
    Move(Dir),
    /// Grow the focused pane's edge in that direction by this many cells.
    Resize(Dir, u16),
    ToggleSplit,
    ToggleFloating,
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
    /// Keyboard scrollback and selection (vi keys) in the focused pane.
    CopyMode,
    /// Search the focused pane's history (copy mode with the search prompt open).
    Search,
    /// Create a session (named, or numbered) and switch to it.
    NewSession(Option<String>),
    Session(SessionTarget),
    /// Rename the current session; without a name, ask for one.
    RenameSession(Option<String>),
    /// Name the current workspace (an empty name clears it); without one, ask.
    RenameWorkspace(Option<String>),
    /// Name the focused pane, overriding its title (empty clears); without one, ask.
    RenamePane(Option<String>),
    /// Open a new pane running this command line.
    Exec(String),
    ExitMode,
    /// Send the leader chord itself to the focused program.
    SendLeader,
    ReloadConfig,
    Quit,
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
                | Action::ScratchpadToggle
                | Action::Exec(_)
                | Action::ExitMode
                | Action::SendLeader
                | Action::PaneSwitcher
                | Action::SessionSwitcher
                | Action::Help
                | Action::CopyMode
                | Action::Search
                | Action::NewSession(_)
                | Action::RenameSession(_)
                | Action::RenameWorkspace(_)
                | Action::RenamePane(_)
                | Action::Quit
        )
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

const DIR: &str = "a direction (left, right, up, down)";
const WS: &str = "a workspace (1-99, next, prev, empty)";

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

fn parse_ws(action: &str, arg: Option<&str>) -> Result<WorkspaceTarget, ActionError> {
    let arg = arg.ok_or(ActionError::MissingArg {
        action: action.into(),
        expected: WS,
    })?;
    match arg {
        "next" | "+1" => Ok(WorkspaceTarget::Next),
        "prev" | "-1" => Ok(WorkspaceTarget::Prev),
        "empty" => Ok(WorkspaceTarget::Empty),
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
            "new_pane" => no_arg(Action::NewPane),
            "close_pane" => no_arg(Action::ClosePane),
            "focus" => Ok(Action::Focus(parse_dir(name, first)?)),
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
            "toggle_floating" => no_arg(Action::ToggleFloating),
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
            "copy_mode" => no_arg(Action::CopyMode),
            "search" => no_arg(Action::Search),
            "new_session" => Ok(Action::NewSession(rest.map(str::to_string))),
            "rename_session" => Ok(Action::RenameSession(rest.map(str::to_string))),
            "rename_workspace" => Ok(Action::RenameWorkspace(rest.map(str::to_string))),
            "rename_pane" => Ok(Action::RenamePane(rest.map(str::to_string))),
            "session" => match rest {
                Some("next") => Ok(Action::Session(SessionTarget::Next)),
                Some("prev") => Ok(Action::Session(SessionTarget::Prev)),
                Some(name) => Ok(Action::Session(SessionTarget::Name(name.to_string()))),
                None => Err(ActionError::MissingArg {
                    action: name.into(),
                    expected: "a session name, next or prev",
                }),
            },
            "exec" => {
                rest.map(|cmd| Action::Exec(cmd.to_string()))
                    .ok_or(ActionError::MissingArg {
                        action: name.into(),
                        expected: "a command line",
                    })
            }
            "exit_mode" => no_arg(Action::ExitMode),
            "send_leader" => no_arg(Action::SendLeader),
            "reload_config" => no_arg(Action::ReloadConfig),
            "quit" => no_arg(Action::Quit),
            _ => Err(ActionError::Unknown(name.into())),
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
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Action::NewPane => f.write_str("new_pane"),
            Action::ClosePane => f.write_str("close_pane"),
            Action::Focus(d) => write!(f, "focus {d}"),
            Action::Move(d) => write!(f, "move {d}"),
            Action::Resize(d, n) => write!(f, "resize {d} {n}"),
            Action::ToggleSplit => f.write_str("toggle_split"),
            Action::ToggleFloating => f.write_str("toggle_floating"),
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
            Action::CopyMode => f.write_str("copy_mode"),
            Action::Search => f.write_str("search"),
            Action::NewSession(None) => f.write_str("new_session"),
            Action::NewSession(Some(n)) => write!(f, "new_session {n}"),
            Action::RenameSession(None) => f.write_str("rename_session"),
            Action::RenameSession(Some(n)) => write!(f, "rename_session {n}"),
            Action::RenameWorkspace(None) => f.write_str("rename_workspace"),
            Action::RenameWorkspace(Some(n)) => write!(f, "rename_workspace {n}"),
            Action::RenamePane(None) => f.write_str("rename_pane"),
            Action::RenamePane(Some(n)) => write!(f, "rename_pane {n}"),
            Action::Session(SessionTarget::Next) => f.write_str("session next"),
            Action::Session(SessionTarget::Prev) => f.write_str("session prev"),
            Action::Session(SessionTarget::Name(n)) => write!(f, "session {n}"),
            Action::Exec(cmd) => write!(f, "exec {cmd}"),
            Action::ExitMode => f.write_str("exit_mode"),
            Action::SendLeader => f.write_str("send_leader"),
            Action::ReloadConfig => f.write_str("reload_config"),
            Action::Quit => f.write_str("quit"),
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
        assert_eq!(
            "new_pane now".parse::<Action>(),
            Err(ActionError::UnexpectedArg("new_pane".into()))
        );
        assert!(matches!(
            "exec".parse::<Action>(),
            Err(ActionError::MissingArg { .. })
        ));
    }

    #[test]
    fn display_round_trips() {
        for s in [
            "new_pane",
            "focus down",
            "resize left 3",
            "workspace empty",
            "move_to_workspace 10",
            "exec htop",
            "help",
            "copy_mode",
            "search",
            "new_session",
            "new_session work",
            "session next",
            "session kumiko",
            "rename_session",
            "rename_workspace web",
            "rename_pane",
        ] {
            assert_eq!(a(&a(s).to_string()), a(s), "{s}");
        }
    }
}

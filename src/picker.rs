//! The picker: a filtered list with a query line, drawn over everything.
//!
//! One component serves the pane switcher, the session switcher, the palette
//! and the prompts (rename a session). The logic is here and pure; drawing is in
//! `render`, and what an accepted item does is the app's business.
//!
//! The palette has two modes, chosen by the first character of the query and
//! changed by typing it: `?` lists the binds (help), `:` or `>` lists every
//! action and runs a typed one. No prefix is help.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::keys::Chord;
use crate::layout::PaneId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Pane(PaneId),
    Session(usize),
    /// Create a session named by the query.
    NewSession,
    /// A ranma server, by name (the server switcher).
    Server(String),
    /// Run this bind: from the global table if the flag is set.
    Bind(Chord, bool),
    /// An action from the catalogue. One that needs an argument is completed
    /// into the query instead of run.
    Action {
        name: String,
        needs_arg: bool,
    },
    /// A typed command line that parses as an action.
    Run(String),
    /// A typed command line that does not parse; the label says why.
    Invalid,
    /// A toolbar's button, by the toolbar's name and its index (`⋯`).
    Button(String, usize),
    /// Go to this workspace (0 is the scratchpad).
    Workspace(u8),
    /// Open an empty workspace, named by the query if there is one.
    NewWorkspace,
}

/// What the palette shows, by the query's first character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteMode {
    Help,
    Command,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    /// Shown dimmed to the right; not matched against.
    pub detail: String,
    pub target: Target,
    /// Where you are now (the shown session, the focused pane): marked in the list.
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Panes,
    Sessions,
    /// The sessions again, choosing where the current workspace goes.
    MoveWorkspace,
    /// Help and the command palette: one list, two modes (see [`PaletteMode`]).
    Palette,
    /// A one-line prompt renaming the session at this index.
    RenameSession(usize),
    RenameWorkspace(u8),
    RenamePane(PaneId),
    /// "Quit ranma?": y or Enter confirms, anything else cancels.
    ConfirmQuit,
    /// "Update ranma?", answered the same way.
    ConfirmUpdate,
    /// The ranma servers: Enter moves this terminal to one, Ctrl+X kills one.
    Servers,
    /// "Kill server NAME?", answered as ConfirmQuit.
    ConfirmKill(String),
    /// A pane's right-click menu: entries that run an action on it.
    Menu,
    /// The buttons of a toolbar that did not fit on it (its `⋯`).
    ToolbarMore,
    /// The workspaces of the shown session (`workspace_switcher`).
    Workspaces,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Still open.
    Open,
    Cancel,
    Accept(Target),
    /// Enter on a prompt, with its text.
    Submit(String),
    /// Ctrl+R on a session: rename it.
    Rename(usize),
    /// Ctrl+X on a server: kill it (after asking).
    Kill(String),
}

#[derive(Debug, Clone)]
pub struct Picker {
    pub kind: Kind,
    pub title: String,
    /// Shown in place of the query line when there is nothing to type (a
    /// yes/no question).
    pub message: Option<String>,
    pub query: String,
    items: Vec<Item>,
    /// Index into `visible()`, not into `items`.
    pub selected: usize,
    /// A key moved the selection or typed: until then a sheet highlights
    /// nothing, since a tap, not Enter, is what picks there.
    pub touched: bool,
    /// Rows of faces a sheet is scrolled down by (the wheel, a swipe).
    pub scroll: usize,
}

impl Picker {
    pub fn new(kind: Kind, title: impl Into<String>, items: Vec<Item>) -> Picker {
        Picker {
            kind,
            title: title.into(),
            message: None,
            query: String::new(),
            items,
            selected: 0,
            touched: false,
            scroll: 0,
        }
    }

    pub fn prompt(kind: Kind, title: impl Into<String>, initial: &str) -> Picker {
        let mut p = Picker::new(kind, title, Vec::new());
        p.query = initial.to_string();
        p
    }

    /// A question answered with one key: `message` is shown, nothing is typed.
    pub fn question(kind: Kind, title: impl Into<String>, message: impl Into<String>) -> Picker {
        let mut p = Picker::new(kind, title, Vec::new());
        p.message = Some(message.into());
        p
    }

    /// The palette, opened in a mode: its prefix is already typed.
    pub fn palette(items: Vec<Item>, mode: PaletteMode) -> Picker {
        let mut p = Picker::new(Kind::Palette, "", items);
        p.query = match mode {
            PaletteMode::Help => "?".into(),
            PaletteMode::Command => ":".into(),
        };
        p
    }

    /// The palette's mode and the query without its prefix.
    pub fn palette_mode(&self) -> Option<(PaletteMode, &str)> {
        if self.kind != Kind::Palette {
            return None;
        }
        let q = self.query.as_str();
        Some(match q.chars().next() {
            Some(':' | '>') => (PaletteMode::Command, &q[1..]),
            Some('?') => (PaletteMode::Help, &q[1..]),
            _ => (PaletteMode::Help, q),
        })
    }

    /// The title over the box. The palette's says which mode it is in and how to
    /// get to the other one, since the mode is one keystroke away.
    pub fn heading(&self) -> &str {
        match self.palette_mode() {
            Some((PaletteMode::Help, _)) => {
                "keys  (type to filter · enter runs it · : for commands)"
            }
            Some((PaletteMode::Command, _)) => {
                "commands  (tab completes · enter runs · ? for keys)"
            }
            None => &self.title,
        }
    }

    /// Start on the item marked current, so Enter stays put and the arrows move
    /// from where you are.
    pub fn select_current(&mut self) -> &mut Self {
        self.selected = self.visible().iter().position(|it| it.current).unwrap_or(0);
        self
    }

    pub fn is_prompt(&self) -> bool {
        matches!(
            self.kind,
            Kind::RenameSession(_)
                | Kind::RenameWorkspace(_)
                | Kind::RenamePane(_)
                | Kind::ConfirmQuit
                | Kind::ConfirmUpdate
                | Kind::ConfirmKill(_)
        )
    }

    /// The items matching the query, best first, plus "new session" when the
    /// session switcher's query names no existing session.
    pub fn visible(&self) -> Vec<Item> {
        if self.is_prompt() {
            return Vec::new();
        }
        let mode = self.palette_mode();
        // In command mode only the action's name filters: what follows it is
        // the argument being typed, not part of the search.
        let query = match mode {
            Some((PaletteMode::Command, rest)) => rest.split_whitespace().next().unwrap_or(""),
            Some((PaletteMode::Help, rest)) => rest,
            None => &self.query,
        };
        let shown = |t: &Target| match mode {
            Some((PaletteMode::Help, _)) => matches!(t, Target::Bind(..)),
            Some((PaletteMode::Command, _)) => matches!(t, Target::Action { .. }),
            None => true,
        };
        let mut scored: Vec<(i64, usize)> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, it)| shown(&it.target))
            .filter_map(|(i, it)| {
                // The label counts most; the detail ("workspace 2", "global") still
                // lets a query narrow by it.
                fuzzy_score(query, &it.label)
                    .map(|s| s + 1000)
                    .or_else(|| fuzzy_score(query, &format!("{} {}", it.label, it.detail)))
                    .map(|s| (s, i))
            })
            .collect();
        // Stable on ties: the order the app listed them in (recency, position).
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut out: Vec<Item> = scored
            .into_iter()
            .map(|(_, i)| self.items[i].clone())
            .collect();
        let q = self.query.trim();
        if self.kind == Kind::Workspaces {
            // A name no workspace has offers one; with nothing typed, an
            // empty one.
            if q.is_empty() {
                out.push(Item {
                    label: "+ new workspace".into(),
                    detail: String::new(),
                    target: Target::NewWorkspace,
                    current: false,
                });
            } else if !self
                .items
                .iter()
                .any(|it| it.label.split_once(':').is_some_and(|(_, n)| n == q))
            {
                out.push(Item {
                    label: format!("new workspace: {q}"),
                    detail: String::new(),
                    target: Target::NewWorkspace,
                    current: false,
                });
            }
        }
        if matches!(self.kind, Kind::Sessions | Kind::MoveWorkspace)
            && !q.is_empty()
            && !self.items.iter().any(|it| it.label == q)
        {
            out.push(Item {
                label: format!("new session: {q}"),
                detail: String::new(),
                target: Target::NewSession,
                current: false,
            });
        }
        // An argument typed: the line itself is first, parsed as a bind would be,
        // so a mistake says what is wrong before anything runs.
        if let Some((PaletteMode::Command, rest)) = mode {
            let line = rest.trim();
            if line.contains(char::is_whitespace) {
                let item = match line.parse::<crate::action::Action>() {
                    Ok(_) => Item {
                        label: format!("run: {line}"),
                        detail: String::new(),
                        target: Target::Run(line.to_string()),
                        current: false,
                    },
                    Err(e) => Item {
                        label: format!("✗ {e}"),
                        detail: String::new(),
                        target: Target::Invalid,
                        current: false,
                    },
                };
                out.insert(0, item);
            }
        }
        out
    }

    /// Put an action's name in the query, ready for its argument.
    fn complete(&mut self, name: &str) {
        let prefix = self.query.chars().next().filter(|c| matches!(c, ':' | '>'));
        self.query = format!("{}{name} ", prefix.unwrap_or(':'));
        self.selected = 0;
    }

    /// What Ctrl+U leaves: the palette keeps its mode.
    fn cleared(&self) -> String {
        match self.palette_mode() {
            Some(_) => self
                .query
                .chars()
                .next()
                .filter(|c| matches!(c, ':' | '>' | '?'))
                .map(String::from)
                .unwrap_or_default(),
            None => String::new(),
        }
    }

    pub fn key(&mut self, key: &KeyEvent) -> Outcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        // A yes/no question: one key answers it, and only yes is yes.
        if matches!(
            self.kind,
            Kind::ConfirmQuit | Kind::ConfirmUpdate | Kind::ConfirmKill(_)
        ) {
            return match key.code {
                KeyCode::Enter | KeyCode::Char('y' | 'Y') if !ctrl => Outcome::Submit("y".into()),
                _ => Outcome::Cancel,
            };
        }
        let n = self.visible().len();
        match key.code {
            KeyCode::Esc => return Outcome::Cancel,
            KeyCode::Char('c' | 'g') if ctrl => return Outcome::Cancel,
            KeyCode::Enter => {
                if self.is_prompt() {
                    return Outcome::Submit(self.query.trim().to_string());
                }
                return match self
                    .visible()
                    .get(self.selected)
                    .map(|it| it.target.clone())
                {
                    Some(Target::Action {
                        name,
                        needs_arg: true,
                    }) => {
                        self.complete(&name);
                        Outcome::Open
                    }
                    Some(Target::Invalid) | None => Outcome::Open,
                    Some(t) => Outcome::Accept(t),
                };
            }
            KeyCode::Tab if matches!(self.palette_mode(), Some((PaletteMode::Command, _))) => {
                if let Some(Target::Action { name, .. }) = self
                    .visible()
                    .get(self.selected)
                    .map(|it| it.target.clone())
                {
                    self.complete(&name);
                }
            }
            KeyCode::Char('r') if ctrl && self.kind == Kind::Sessions => {
                if let Some(Item {
                    target: Target::Session(i),
                    ..
                }) = self.visible().get(self.selected)
                {
                    return Outcome::Rename(*i);
                }
            }
            KeyCode::Char('x') if ctrl && self.kind == Kind::Servers => {
                if let Some(Item {
                    target: Target::Server(name),
                    ..
                }) = self.visible().get(self.selected)
                {
                    return Outcome::Kill(name.clone());
                }
            }
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Char('p' | 'k') if ctrl => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Tab => {
                self.selected = (self.selected + 1).min(n.saturating_sub(1))
            }
            KeyCode::Char('n' | 'j') if ctrl => {
                self.selected = (self.selected + 1).min(n.saturating_sub(1))
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.selected = 0;
            }
            KeyCode::Char('u') if ctrl => {
                self.query = self.cleared();
                self.selected = 0;
            }
            KeyCode::Char(c) if !ctrl => {
                self.query.push(c);
                self.selected = 0;
            }
            _ => {}
        }
        Outcome::Open
    }

    pub fn paste(&mut self, text: &str) {
        self.query.push_str(text.lines().next().unwrap_or(""));
        self.selected = 0;
    }
}

/// Subsequence fuzzy match, case-insensitive. `None` if `query` is not a
/// subsequence of `label`. Higher is better: consecutive characters, matches at
/// the start of a word, and a match at the very start all score; long labels and
/// gaps cost a little.
pub fn fuzzy_score(query: &str, label: &str) -> Option<i64> {
    let q: Vec<char> = query
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if q.is_empty() {
        return Some(0);
    }
    let l: Vec<char> = label.to_lowercase().chars().collect();
    let mut score = 0i64;
    let mut qi = 0;
    let mut prev: Option<usize> = None;
    for (i, c) in l.iter().enumerate() {
        if qi < q.len() && *c == q[qi] {
            score += 1;
            if prev == Some(i.wrapping_sub(1)) {
                score += 5;
            }
            if i == 0 {
                score += 8;
            } else if !l[i - 1].is_alphanumeric() {
                score += 4;
            }
            if let Some(p) = prev {
                score -= (i - p - 1).min(3) as i64;
            }
            prev = Some(i);
            qi += 1;
        }
    }
    (qi == q.len()).then(|| score * 10 - l.len() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(labels: &[&str]) -> Vec<Item> {
        labels
            .iter()
            .enumerate()
            .map(|(i, l)| Item {
                label: l.to_string(),
                detail: String::new(),
                target: Target::Session(i),
                current: false,
            })
            .collect()
    }

    fn typed(p: &mut Picker, s: &str) {
        for c in s.chars() {
            p.key(&KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
    }

    #[test]
    fn fuzzy_prefers_prefixes_and_runs() {
        assert!(fuzzy_score("kum", "kumiko").unwrap() > fuzzy_score("kum", "work-kum-x").unwrap());
        assert!(fuzzy_score("nv", "nvim src").unwrap() > fuzzy_score("nv", "a n b v").unwrap());
        assert_eq!(fuzzy_score("xyz", "kumiko"), None);
        assert_eq!(fuzzy_score("", "anything"), Some(0));
        assert!(fuzzy_score("KUM", "kumiko").is_some());
    }

    #[test]
    fn typing_filters_and_resets_selection() {
        let mut p = Picker::new(Kind::Panes, "panes", items(&["htop", "nvim", "zsh"]));
        p.key(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(p.selected, 1);
        typed(&mut p, "nv");
        assert_eq!(p.selected, 0);
        let v = p.visible();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].label, "nvim");
        assert_eq!(
            p.key(&KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Outcome::Accept(Target::Session(1))
        );
    }

    #[test]
    fn session_picker_offers_to_create() {
        let mut p = Picker::new(Kind::Sessions, "sessions", items(&["main"]));
        typed(&mut p, "work");
        let v = p.visible();
        assert_eq!(v.last().unwrap().target, Target::NewSession);
        // An exact name does not offer a duplicate.
        let mut p = Picker::new(Kind::Sessions, "sessions", items(&["main"]));
        typed(&mut p, "main");
        assert!(p.visible().iter().all(|i| i.target != Target::NewSession));
    }

    #[test]
    fn opens_on_the_current_item() {
        let mut list = items(&["main", "ai-projects", "kumiko"]);
        list[1].current = true;
        let mut p = Picker::new(Kind::Sessions, "sessions", list);
        p.select_current();
        assert_eq!(p.selected, 1);
        assert_eq!(
            p.key(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            Outcome::Open
        );
        assert_eq!(p.selected, 2);
        // With nothing marked it starts at the top.
        let mut p = Picker::new(Kind::Sessions, "sessions", items(&["a", "b"]));
        p.select_current();
        assert_eq!(p.selected, 0);
    }

    #[test]
    fn moving_a_workspace_offers_a_new_session_too() {
        let mut p = Picker::new(Kind::MoveWorkspace, "move", items(&["main"]));
        typed(&mut p, "ai");
        assert_eq!(p.visible().last().unwrap().target, Target::NewSession);
    }

    #[test]
    fn prompt_submits_text_and_esc_cancels() {
        let mut p = Picker::prompt(Kind::RenameSession(0), "rename", "main");
        p.key(&KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        typed(&mut p, "x");
        assert_eq!(
            p.key(&KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Outcome::Submit("maix".into())
        );
        assert_eq!(
            p.key(&KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Outcome::Cancel
        );
    }

    #[test]
    fn ctrl_r_renames_the_selected_session() {
        let mut p = Picker::new(Kind::Sessions, "sessions", items(&["a", "b"]));
        p.key(&KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL));
        assert_eq!(
            p.key(&KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL)),
            Outcome::Rename(1)
        );
    }

    #[test]
    fn quit_confirmation_takes_only_yes() {
        for (code, out) in [
            (KeyCode::Char('y'), Outcome::Submit("y".into())),
            (KeyCode::Enter, Outcome::Submit("y".into())),
            (KeyCode::Char('n'), Outcome::Cancel),
            (KeyCode::Char('q'), Outcome::Cancel),
            (KeyCode::Esc, Outcome::Cancel),
        ] {
            let mut p = Picker::prompt(Kind::ConfirmQuit, "quit?", "");
            assert_eq!(
                p.key(&KeyEvent::new(code, KeyModifiers::NONE)),
                out,
                "{code:?}"
            );
        }
    }

    #[test]
    fn selection_stays_in_range() {
        let mut p = Picker::new(Kind::Panes, "panes", items(&["a"]));
        for _ in 0..5 {
            p.key(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        assert_eq!(p.selected, 0);
        let mut empty = Picker::new(Kind::Panes, "panes", Vec::new());
        assert_eq!(
            empty.key(&KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Outcome::Open
        );
    }

    fn key(p: &mut Picker, code: KeyCode) -> Outcome {
        p.key(&KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn palette(mode: PaletteMode) -> Picker {
        let mut items = vec![Item {
            label: "ctrl+b t               new_pane".into(),
            detail: String::new(),
            target: Target::Bind("t".parse().unwrap(), false),
            current: false,
        }];
        for (name, needs_arg) in [("new_pane", false), ("workspace", true), ("detach", false)] {
            items.push(Item {
                label: name.into(),
                detail: String::new(),
                target: Target::Action {
                    name: name.into(),
                    needs_arg,
                },
                current: false,
            });
        }
        Picker::palette(items, mode)
    }

    #[test]
    fn the_prefix_is_the_mode() {
        let mut p = palette(PaletteMode::Help);
        assert_eq!(p.query, "?");
        assert!(
            p.visible()
                .iter()
                .all(|i| matches!(i.target, Target::Bind(..)))
        );
        assert!(p.heading().starts_with("keys"));
        // Over the prefix and back: nothing typed is help, ":" and ">" commands.
        key(&mut p, KeyCode::Backspace);
        assert_eq!(p.palette_mode(), Some((PaletteMode::Help, "")));
        for prefix in [':', '>'] {
            p.query.clear();
            key(&mut p, KeyCode::Char(prefix));
            assert_eq!(p.palette_mode(), Some((PaletteMode::Command, "")));
            assert_eq!(p.visible().len(), 3);
            assert!(p.heading().starts_with("commands"));
        }
        // Ctrl+U clears what was typed, not the mode.
        typed(&mut p, "det");
        p.key(&KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!(p.query, ">");
    }

    #[test]
    fn commands_run_bare_or_complete_for_an_argument() {
        let mut p = palette(PaletteMode::Command);
        typed(&mut p, "det");
        assert_eq!(
            key(&mut p, KeyCode::Enter),
            Outcome::Accept(Target::Action {
                name: "detach".into(),
                needs_arg: false
            })
        );
        // One that needs an argument is completed instead of run...
        let mut p = palette(PaletteMode::Command);
        typed(&mut p, "works");
        assert_eq!(key(&mut p, KeyCode::Enter), Outcome::Open);
        assert_eq!(p.query, ":workspace ");
        // ...and Tab completes any of them.
        let mut p = palette(PaletteMode::Command);
        typed(&mut p, "new");
        key(&mut p, KeyCode::Tab);
        assert_eq!(p.query, ":new_pane ");
    }

    #[test]
    fn a_typed_line_runs_or_says_why_not() {
        let mut p = palette(PaletteMode::Command);
        typed(&mut p, "workspace 7");
        let v = p.visible();
        assert_eq!(v[0].target, Target::Run("workspace 7".into()));
        // The name still filters the list under it.
        assert_eq!(v.len(), 2);
        assert_eq!(
            key(&mut p, KeyCode::Enter),
            Outcome::Accept(Target::Run("workspace 7".into()))
        );
        let mut p = palette(PaletteMode::Command);
        typed(&mut p, "workspace 0");
        let v = p.visible();
        assert_eq!(v[0].target, Target::Invalid);
        assert!(v[0].label.contains("is not a workspace"), "{}", v[0].label);
        assert_eq!(key(&mut p, KeyCode::Enter), Outcome::Open);
        // An action outside the catalogue is refused by name.
        let mut p = palette(PaletteMode::Command);
        typed(&mut p, "fcous left");
        assert!(p.visible()[0].label.contains("unknown action `fcous`"));
    }
}

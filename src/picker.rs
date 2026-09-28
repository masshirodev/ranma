//! The picker: a filtered list with a query line, drawn over everything.
//!
//! One component serves the pane switcher, the session switcher and the prompts
//! (rename a session). The logic is here and pure; drawing is in `render`, and
//! what an accepted item does is the app's business.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::keys::Chord;
use crate::layout::PaneId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Pane(PaneId),
    Session(usize),
    /// Create a session named by the query.
    NewSession,
    /// Run this bind: from the global table if the flag is set.
    Bind(Chord, bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    /// Shown dimmed to the right; not matched against.
    pub detail: String,
    pub target: Target,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Panes,
    Sessions,
    Help,
    /// A one-line prompt renaming the session at this index.
    RenameSession(usize),
    RenameWorkspace(u8),
    RenamePane(PaneId),
    /// "Quit ranma?": y or Enter confirms, anything else cancels.
    ConfirmQuit,
    /// "Update ranma?", answered the same way.
    ConfirmUpdate,
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

    pub fn is_prompt(&self) -> bool {
        matches!(
            self.kind,
            Kind::RenameSession(_)
                | Kind::RenameWorkspace(_)
                | Kind::RenamePane(_)
                | Kind::ConfirmQuit
                | Kind::ConfirmUpdate
        )
    }

    /// The items matching the query, best first, plus "new session" when the
    /// session switcher's query names no existing session.
    pub fn visible(&self) -> Vec<Item> {
        if self.is_prompt() {
            return Vec::new();
        }
        let mut scored: Vec<(i64, usize)> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, it)| {
                // The label counts most; the detail ("workspace 2", "global") still
                // lets a query narrow by it.
                fuzzy_score(&self.query, &it.label)
                    .map(|s| s + 1000)
                    .or_else(|| fuzzy_score(&self.query, &format!("{} {}", it.label, it.detail)))
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
        if self.kind == Kind::Sessions
            && !q.is_empty()
            && !self.items.iter().any(|it| it.label == q)
        {
            out.push(Item {
                label: format!("new session: {q}"),
                detail: String::new(),
                target: Target::NewSession,
            });
        }
        out
    }

    pub fn key(&mut self, key: &KeyEvent) -> Outcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        // A yes/no question: one key answers it, and only yes is yes.
        if matches!(self.kind, Kind::ConfirmQuit | Kind::ConfirmUpdate) {
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
                return match self.visible().get(self.selected) {
                    Some(it) => Outcome::Accept(it.target.clone()),
                    None => Outcome::Open,
                };
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
                self.query.clear();
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
}

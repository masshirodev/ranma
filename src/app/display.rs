//! Pane numbers (`leader i`, tmux's `display-panes`): a number drawn large
//! over every pane of the active layer; typing one focuses that pane.
//! Anything else, or a click, puts them away.

use crossterm::event::{KeyCode, KeyEvent};

use super::App;
use crate::layout::PaneId;

pub struct PaneNumbers {
    /// The panes on screen in drawing order (tiles, then floats bottom to
    /// top), each with its number.
    pub panes: Vec<(String, PaneId)>,
    /// What has been typed of a number so far.
    pub typed: String,
    /// The pane focused when they were shown: focus moving any other way
    /// (a hook, a workspace switch) puts them away.
    pub from: Option<PaneId>,
}

/// `n` numbers from 1, as wide as the largest so none is the start of
/// another: `1`-`9` for up to nine panes, `01`-`12` for twelve.
pub fn numbers(n: usize) -> Vec<String> {
    let width = n.to_string().len();
    (1..=n).map(|i| format!("{i:0width$}")).collect()
}

impl App {
    pub fn pane_numbers(&self) -> Option<&PaneNumbers> {
        self.numbers.as_ref()
    }

    pub(super) fn show_pane_numbers(&mut self) {
        let panes: Vec<PaneId> = self.focus_rects().into_iter().map(|(id, _)| id).collect();
        if panes.is_empty() {
            self.status = Some("no panes to number".into());
            return;
        }
        self.numbers = Some(PaneNumbers {
            panes: numbers(panes.len()).into_iter().zip(panes).collect(),
            typed: String::new(),
            from: self.focused(),
        });
        self.dirty = true;
    }

    pub(super) fn hide_pane_numbers(&mut self) {
        if self.numbers.take().is_some() {
            self.dirty = true;
        }
    }

    /// A digit adds to the number; a complete one focuses its pane. Any other
    /// key only puts the numbers away: it was meant for them, not the program.
    pub(super) fn pane_number_key(&mut self, key: &KeyEvent) {
        let Some(n) = self.numbers.as_mut() else {
            return;
        };
        let KeyCode::Char(c @ '0'..='9') = key.code else {
            return self.hide_pane_numbers();
        };
        n.typed.push(c);
        self.dirty = true;
        let n = self.numbers.as_ref().expect("checked above");
        if let Some((_, id)) = n.panes.iter().find(|(l, _)| *l == n.typed) {
            let id = *id;
            self.numbers = None;
            self.active_mut().fullscreen = false;
            self.focus(id);
            self.relayout();
        } else if !n.panes.iter().any(|(l, _)| l.starts_with(n.typed.as_str())) {
            self.status = Some(format!("no pane {}", n.typed));
            self.numbers = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_as_wide_as_the_largest() {
        assert_eq!(numbers(3), vec!["1", "2", "3"]);
        let twelve = numbers(12);
        assert_eq!(twelve.first().unwrap(), "01");
        assert_eq!(twelve.last().unwrap(), "12");
        for a in &twelve {
            for b in &twelve {
                assert!(a == b || !b.starts_with(a.as_str()), "{a} {b}");
            }
        }
    }
}

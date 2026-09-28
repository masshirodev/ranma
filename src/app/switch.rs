//! The pickers ranma opens: the pane switcher, the session switcher, the rename
//! prompt, and help (every bind, runnable).

use crossterm::event::{KeyEvent, MouseEvent, MouseEventKind};

use super::{App, SCRATCHPAD};
use crate::keys::Chord;
use crate::layout::Rect;
use crate::picker::{Item, Kind, Outcome, Picker, Target};

/// Where the picker is drawn, shared by the renderer and mouse hit-testing.
#[derive(Debug, Clone, Copy)]
pub struct PickerLayout {
    pub outer: Rect,
    pub query: Rect,
    pub list: Rect,
    /// Index of the first visible item (the list scrolls to keep the selection).
    pub offset: usize,
}

impl App {
    pub fn picker(&self) -> Option<&Picker> {
        self.picker.as_ref()
    }

    pub fn picker_layout(&self) -> Option<PickerLayout> {
        let p = self.picker.as_ref()?;
        let s = self.screen;
        let items = p.visible().len() as u16;
        let w = s.w.saturating_sub(4).clamp(10, 72);
        let max_h = (s.h * 3 / 5).max(5);
        // Border, query row, the items (at least one row), border.
        let h = (items.max(1) + 3).min(max_h);
        let outer = Rect::new(s.x + (s.w - w) / 2, s.y + (s.h.saturating_sub(h)) / 3, w, h);
        let inner = outer.inset(1, 1);
        let query = Rect::new(inner.x, inner.y, inner.w, inner.h.min(1));
        let list = Rect::new(inner.x, inner.y + 1, inner.w, inner.h.saturating_sub(1));
        let offset = p.selected.saturating_sub(list.h.saturating_sub(1) as usize);
        Some(PickerLayout {
            outer,
            query,
            list,
            offset,
        })
    }

    pub(super) fn open_pane_switcher(&mut self) {
        let mut items = Vec::new();
        let sessions = self.session_count() > 1;
        // The shown session first, then the others: the pane you want is usually
        // close by.
        let mut order: Vec<usize> = vec![self.active_session];
        order.extend((0..self.sessions.len()).filter(|i| *i != self.active_session));
        for si in order {
            let map = if si == self.active_session {
                &self.workspaces
            } else {
                &self.sessions[si].workspaces
            };
            for (n, ws) in map {
                for id in ws.panes() {
                    let title = self.pane_title(id);
                    let place = if sessions {
                        format!("{} · {n}", self.sessions[si].name)
                    } else {
                        format!("workspace {n}")
                    };
                    items.push(Item {
                        label: title,
                        detail: place,
                        target: Target::Pane(id),
                    });
                }
            }
        }
        for id in self.scratch.panes() {
            items.push(Item {
                label: self.pane_title(id),
                detail: "scratchpad".into(),
                target: Target::Pane(id),
            });
        }
        self.picker = Some(Picker::new(Kind::Panes, "panes", items));
        self.dirty = true;
    }

    pub(super) fn open_session_switcher(&mut self) {
        let items = self
            .session_list()
            .into_iter()
            .map(|(i, name, panes, shown)| Item {
                label: name,
                detail: format!(
                    "{panes} pane{}{}",
                    if panes == 1 { "" } else { "s" },
                    if shown { " · shown" } else { "" }
                ),
                target: Target::Session(i),
            })
            .collect();
        self.picker = Some(Picker::new(
            Kind::Sessions,
            "sessions  (type a new name to create · ctrl+r renames)",
            items,
        ));
        self.dirty = true;
    }

    pub(super) fn open_rename_prompt(&mut self, i: usize) {
        let name = self.sessions[i].name.clone();
        self.picker = Some(Picker::prompt(
            Kind::RenameSession(i),
            format!("rename session {name}"),
            &name,
        ));
        self.dirty = true;
    }

    pub(super) fn open_rename_workspace(&mut self) {
        let n = self.current;
        let name = self
            .workspaces
            .get(&n)
            .and_then(|w| w.name.clone())
            .unwrap_or_default();
        self.picker = Some(Picker::prompt(
            Kind::RenameWorkspace(n),
            format!("name workspace {n}  (empty clears)"),
            &name,
        ));
        self.dirty = true;
    }

    pub(super) fn open_rename_pane(&mut self) {
        let Some(id) = self.focused() else {
            return;
        };
        let current = self
            .panes
            .get(&id)
            .map(|p| p.label().to_string())
            .unwrap_or_default();
        self.picker = Some(Picker::prompt(
            Kind::RenamePane(id),
            "name this pane  (empty goes back to its title)",
            &current,
        ));
        self.dirty = true;
    }

    pub(super) fn confirm_quit(&mut self) {
        let n = self.panes.len();
        let s = self.session_count();
        let what = match (n, s) {
            (1, _) => "the last pane".to_string(),
            (n, 1) => format!("{n} panes"),
            (n, s) => format!("{n} panes in {s} sessions"),
        };
        self.picker = Some(Picker::prompt(
            Kind::ConfirmQuit,
            format!("quit ranma, closing {what}?  y / Enter quits, anything else cancels"),
            "",
        ));
        self.dirty = true;
    }

    pub(super) fn rename_workspace(&mut self, n: u8, name: &str) {
        let name = name.trim();
        if let Some(ws) = self.workspaces.get_mut(&n) {
            ws.name = (!name.is_empty()).then(|| name.to_string());
        }
        self.dirty = true;
    }

    pub(super) fn rename_pane(&mut self, id: crate::layout::PaneId, name: &str) {
        let name = name.trim();
        if let Some(p) = self.panes.get_mut(&id) {
            p.name = (!name.is_empty()).then(|| name.to_string());
        }
        self.dirty = true;
    }

    pub(super) fn open_help(&mut self) {
        let mut items: Vec<Item> = Vec::new();
        let leader = self.config.settings.leader;
        for (global, table) in [
            (false, &self.config.binds),
            (true, &self.config.global_binds),
        ] {
            for (chord, bind) in table {
                let keys = if global {
                    chord.to_string()
                } else {
                    format!("{leader} {chord}")
                };
                items.push(Item {
                    label: format!("{keys:<22} {}", bind.label),
                    detail: if global {
                        "global".into()
                    } else {
                        String::new()
                    },
                    target: Target::Bind(*chord, global),
                });
            }
        }
        // Grouped by what they do, which is how you look for one: every "focus"
        // together, every "workspace" together.
        items.sort_by(|a, b| {
            let action = |i: &Item| i.label[22.min(i.label.len())..].to_string();
            action(a).cmp(&action(b)).then(a.label.cmp(&b.label))
        });
        self.picker = Some(Picker::new(
            Kind::Help,
            "keys  (type to filter · enter runs it)",
            items,
        ));
        self.dirty = true;
    }

    fn pane_title(&self, id: crate::layout::PaneId) -> String {
        self.panes
            .get(&id)
            .map(|p| p.label().to_string())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| format!("pane {id}"))
    }

    pub(super) fn picker_key(&mut self, key: &KeyEvent) {
        let Some(p) = self.picker.as_mut() else {
            return;
        };
        let outcome = p.key(key);
        self.dirty = true;
        self.picker_outcome(outcome);
    }

    pub(super) fn picker_paste(&mut self, text: &str) {
        if let Some(p) = self.picker.as_mut() {
            p.paste(text);
            self.dirty = true;
        }
    }

    /// Clicks pick an item; a click outside closes the picker; the wheel moves.
    pub(super) fn picker_mouse(&mut self, m: MouseEvent) {
        let Some(l) = self.picker_layout() else {
            return;
        };
        let (x, y) = (m.column, m.row);
        match m.kind {
            MouseEventKind::Down(_) if l.list.contains(x, y) => {
                let i = l.offset + (y - l.list.y) as usize;
                let target = self
                    .picker
                    .as_ref()
                    .and_then(|p| p.visible().get(i).map(|it| it.target.clone()));
                if let Some(t) = target {
                    self.picker_outcome(Outcome::Accept(t));
                }
            }
            MouseEventKind::Down(_) if !l.outer.contains(x, y) => {
                self.picker_outcome(Outcome::Cancel)
            }
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                if let Some(p) = self.picker.as_mut() {
                    let n = p.visible().len();
                    p.selected = if m.kind == MouseEventKind::ScrollDown {
                        (p.selected + 1).min(n.saturating_sub(1))
                    } else {
                        p.selected.saturating_sub(1)
                    };
                }
                self.dirty = true;
            }
            _ => {}
        }
    }

    fn picker_outcome(&mut self, outcome: Outcome) {
        let kind = self.picker.as_ref().map(|p| p.kind.clone());
        match outcome {
            Outcome::Open => {}
            Outcome::Cancel => self.picker = None,
            Outcome::Rename(i) => self.open_rename_prompt(i),
            Outcome::Submit(text) => {
                self.picker = None;
                match kind {
                    Some(Kind::RenameSession(i)) => self.rename_session(i, &text),
                    Some(Kind::RenameWorkspace(n)) => self.rename_workspace(n, &text),
                    Some(Kind::RenamePane(id)) => self.rename_pane(id, &text),
                    Some(Kind::ConfirmQuit) => self.quit = true,
                    _ => {}
                }
            }
            Outcome::Accept(target) => {
                let query = self
                    .picker
                    .take()
                    .map(|p| p.query.trim().to_string())
                    .unwrap_or_default();
                match target {
                    Target::Session(i) => self.switch_session(i),
                    Target::NewSession => self.new_session(Some(&query)),
                    Target::Pane(id) => self.reveal_pane(id),
                    Target::Bind(chord, global) => self.run_help_bind(chord, global),
                }
            }
        }
        self.dirty = true;
    }

    /// Show a pane wherever it is: its session, its workspace or the scratchpad.
    fn reveal_pane(&mut self, id: crate::layout::PaneId) {
        if let Some((si, _)) = self.locate_hidden(id) {
            self.switch_session(si);
        }
        match self.locate(id) {
            Some(SCRATCHPAD) => {
                self.scratch_shown = true;
            }
            Some(n) => self.switch_workspace(n),
            None => return,
        }
        self.focus(id);
        self.relayout();
    }

    /// Run a bind picked from help. A WM bind runs as if pressed in WM mode, but
    /// ranma returns to normal mode afterwards: help is a palette, not a mode.
    fn run_help_bind(&mut self, chord: Chord, global: bool) {
        self.run_bind(chord, global);
        if self.mode == super::Mode::Wm {
            self.set_mode(super::Mode::Normal);
        }
    }
}

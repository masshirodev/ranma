//! One bar for nested ranmas, the window manager's side (the bar itself is
//! `nestbar`). Two directions:
//!
//! - **Inward**, as the outer ranma: a ranma in a pane asks at start whether a
//!   ranma draws around it (answered here), then reports its workspaces, which
//!   are kept per pane and drawn in this ranma's bar. A pane whose ranma
//!   reports loses its border's title, and its border when it fills the
//!   workspace: the ranma inside draws both.
//! - **Outward**, as the inner ranma: when the client said a ranma answered its
//!   question, this ranma reports its own workspaces (and what it heard from
//!   the ranmas inside it) on every change, and draws no bar while its terminal
//!   has focus, which is exactly while the outer bar shows its workspaces.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{App, Mode, SCRATCHPAD, chord_bytes};
use crate::action::{Action, WorkspaceTarget};
use crate::config::{BindAction, NestedMode};
use crate::layout::PaneId;
use crate::nestbar::{self, Report, Ws};

impl App {
    /// The report of the ranma in a pane: only while one runs there (its
    /// title carries the mark), so a ranma that ended leaves nothing behind.
    pub(super) fn report_of(&self, id: PaneId) -> Option<&Report> {
        let p = self.panes.get(&id)?;
        if !p.hosts_ranma() {
            return None;
        }
        self.reports.get(&id)
    }

    /// How many workspaces the ranma in workspace `n`'s focused pane has in
    /// use, for the workspaces module to put after a collapsed holder. None
    /// with `nested = "off"`, where the bar says nothing of inner ranmas.
    pub(super) fn holder_in_use(&self, n: u8) -> Option<usize> {
        if self.config.workspaces_nested == crate::config::NestedWorkspaces::Off {
            return None;
        }
        let focused = self.workspaces.get(&n)?.focused?;
        nestbar::in_use(self.report_of(focused)?)
    }

    /// Whether the ranma in this pane reports: its border then carries no
    /// title, since the ranma inside draws its own.
    pub fn reports_from(&self, id: PaneId) -> bool {
        self.report_of(id).is_some()
    }

    pub(super) fn nested_mark(&mut self, id: PaneId, m: crate::osc::Mark) {
        match m {
            crate::osc::Mark::RanmaHello => {
                // Answered only by a ranma that will show the workspaces: one
                // with a bar of its own, or one that passes them further out.
                let shows = self.config.theme.bar.position != crate::theme::BarPosition::Hidden
                    || self.client_outer;
                if self.config.settings.nested == NestedMode::Auto
                    && shows
                    && let Some(p) = self.panes.get(&id)
                {
                    p.write(nestbar::hello_reply());
                }
            }
            crate::osc::Mark::RanmaReport(json) => {
                match Report::parse(&json) {
                    Some(r) => {
                        if self.reports.get(&id) != Some(&r) {
                            self.reports.insert(id, r);
                            self.dirty = true;
                        }
                    }
                    // A protocol this build does not speak: as if none came.
                    None => {
                        if self.reports.remove(&id).is_some() {
                            self.dirty = true;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// This ranma as a report: what an outer ranma draws, and the top level
    /// of this ranma's own nested bar.
    pub(super) fn own_report(&self) -> Report {
        let keys = self.workspace_keys();
        let ws = self
            .workspace_list()
            .into_iter()
            .map(|(n, _current, occ, urgent, name)| {
                let focused = self.workspaces.get(&n).and_then(|w| w.focused);
                Ws {
                    n,
                    name,
                    occ,
                    urgent,
                    key: keys.get(&n).cloned(),
                    nest: focused
                        .and_then(|f| self.report_of(f))
                        .cloned()
                        .map(Box::new),
                }
            })
            .collect();
        let (scratch, scratch_shown) = self.scratch_state();
        Report {
            v: nestbar::PROTOCOL,
            session: self.session_name().to_string(),
            sessions: self.session_count(),
            current: if scratch_shown {
                SCRATCHPAD
            } else {
                self.current
            },
            accent: self.session_accent().and_then(|c| self.rgb_of(c)),
            scratch,
            scratch_shown,
            scratch_key: self.bound_key(|a| *a == Action::ScratchpadToggle),
            mode: match (self.mode, self.hints.is_some()) {
                (Mode::Wm, _) => "wm",
                (Mode::Copy, _) => {
                    if self
                        .copy
                        .as_ref()
                        .and_then(|c| c.search.as_ref())
                        .is_some_and(|s| s.editing)
                    {
                        "search"
                    } else {
                        "copy"
                    }
                }
                (Mode::Normal, true) => "link",
                (Mode::Normal, false) => "normal",
            }
            .into(),
            status: self.status.clone(),
            outer_leader: Some(self.config.settings.outer_leader.to_string()),
            sticky: self.config.settings.wm_mode_sticky,
            ws,
        }
    }

    /// The key each workspace is bound to in WM mode, as a bind spells it.
    fn workspace_keys(&self) -> std::collections::HashMap<u8, String> {
        let mut keys = std::collections::HashMap::new();
        for (chord, b) in &self.config.binds {
            if let BindAction::Builtin(Action::Workspace(WorkspaceTarget::Index(n))) = &b.action {
                keys.entry(*n)
                    .and_modify(|k: &mut String| {
                        let c = chord.to_string();
                        if c.len() < k.len() {
                            *k = c;
                        }
                    })
                    .or_insert_with(|| chord.to_string());
            }
        }
        keys
    }

    fn bound_key(&self, is: impl Fn(&Action) -> bool) -> Option<String> {
        self.config
            .binds
            .iter()
            .filter(|(_, b)| matches!(&b.action, BindAction::Builtin(a) if is(a)))
            .map(|(c, _)| c.to_string())
            .min_by_key(|k| k.len())
    }

    /// A theme colour as RGB, through the host's palette for an indexed one.
    fn rgb_of(&self, c: crate::theme::Color) -> Option<[u8; 3]> {
        match c {
            crate::theme::Color::Rgb(r, g, b) => Some([r, g, b]),
            crate::theme::Color::Indexed(i) => {
                self.host_colors.get(i as usize).map(|c| [c.r, c.g, c.b])
            }
            crate::theme::Color::Default => None,
        }
    }

    /// Send this ranma's report outward when it changed, if a ranma around
    /// it answered. Called once per round of events, before output is flushed.
    pub fn report_outward(&mut self) {
        if !self.client_outer || self.config.settings.nested != NestedMode::Auto {
            return;
        }
        let r = self.own_report();
        if self.last_report.as_ref() != Some(&r) {
            self.host_out.push(nestbar::report_osc(&r));
            self.last_report = Some(r);
        }
    }

    /// A (new) terminal is showing this ranma: whether a ranma around it
    /// answered, in a protocol this build speaks. The report goes out afresh.
    pub fn set_outer(&mut self, outer: Option<u32>) {
        let was = self.bar_yielded();
        self.client_outer = outer == Some(nestbar::PROTOCOL);
        self.host_focused = true;
        self.last_report = None;
        if self.bar_yielded() != was {
            self.relayout();
        }
    }

    /// This ranma draws no bar of its own: an outer one shows its workspaces,
    /// and does so right now (this terminal has focus there).
    pub fn bar_yielded(&self) -> bool {
        self.client_outer && self.config.settings.nested == NestedMode::Auto
    }

    /// The bar is drawn over the bottom row instead of beside the panes: a
    /// ranma around this one shows the workspaces while this terminal has
    /// focus, and keeping the row free means focus coming and going never
    /// resizes the panes.
    pub fn bar_overlaid(&self) -> bool {
        self.bar_yielded() && !self.host_focused
    }

    /// The pane that is drawn without a border: one whose ranma reports and
    /// that fills the workspace shown (alone there, or fullscreen), so the
    /// ranma inside draws the only frame.
    pub(super) fn frameless(&self) -> Option<PaneId> {
        if self.scratch_shown {
            return None;
        }
        let ws = self.workspaces.get(&self.current)?;
        let id = ws.focused?;
        self.report_of(id)?;
        let alone = ws.fullscreen
            || (ws.floating.is_empty() && ws.tree.panes().len() == 1 && ws.tree.contains(id));
        alone.then_some(id)
    }

    /// The deepest ranma on the path of the focused pane, and every one
    /// between: whose mode and messages the bar says.
    pub(super) fn nested_path(&self) -> Vec<&Report> {
        let mut out = Vec::new();
        let mut r = self.focused().and_then(|f| self.report_of(f));
        while let Some(rep) = r {
            out.push(rep);
            r = rep.on_path();
        }
        out
    }

    /// A click on a workspace inside a nested ranma: show its holder, then
    /// type what reaches it. The outer leader once puts the ranma in the pane
    /// in WM mode; each time more goes one level down (DESIGN.md, "ranma
    /// inside ranma"); then that level's key for the workspace, then Esc.
    pub(super) fn click_nested(&mut self, holder: u8, path: &[u8]) {
        self.run_action(Action::Workspace(WorkspaceTarget::Index(holder)));
        let Some(pane) = self.workspaces.get(&holder).and_then(|w| w.focused) else {
            return;
        };
        let mut level = self.report_of(pane).cloned();
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let mut bytes = Vec::new();
        let modes = match self.panes.get(&pane) {
            Some(p) => p.modes(),
            None => return,
        };
        for (depth, n) in path.iter().enumerate() {
            let Some(r) = level else { break };
            let key = if *n == SCRATCHPAD {
                r.scratch_key.clone()
            } else {
                r.ws.iter().find(|w| w.n == *n).and_then(|w| w.key.clone())
            };
            let outer = r
                .outer_leader
                .as_deref()
                .and_then(|k| k.parse::<crate::keys::Chord>().ok());
            let (Some(key), Some(outer)) = (key.and_then(|k| k.parse().ok()), outer) else {
                break;
            };
            for _ in 0..=depth {
                bytes.extend(chord_bytes(outer, modes).unwrap_or_default());
            }
            bytes.extend(chord_bytes(key, modes).unwrap_or_default());
            if r.sticky {
                bytes.extend(crate::input::encode_key(&esc, modes).unwrap_or_default());
            }
            level =
                r.ws.iter()
                    .find(|w| w.n == *n)
                    .and_then(|w| w.nest.as_deref().cloned());
        }
        if let Some(p) = self.panes.get(&pane)
            && !bytes.is_empty()
        {
            p.write(bytes);
        }
    }
}

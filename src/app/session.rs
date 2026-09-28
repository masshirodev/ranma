//! Sessions: independent sets of workspaces, one shown at a time.
//!
//! The shown session's workspaces live in `App::workspaces` and `App::current`,
//! so everything that deals with workspaces keeps working on plain fields.
//! Switching swaps them with the stored copy. A session's slot in `sessions`
//! keeps its name; while shown, its own `workspaces` map is empty.

use std::collections::BTreeMap;

use super::App;
use crate::action::SessionTarget;
use crate::config::Event as HookEvent;
use crate::layout::PaneId;
use crate::workspace::Workspace;

#[derive(Debug, Default)]
pub struct Session {
    pub name: String,
    pub workspaces: BTreeMap<u8, Workspace>,
    pub current: u8,
}

impl Session {
    pub fn new(name: impl Into<String>) -> Session {
        let mut workspaces = BTreeMap::new();
        workspaces.insert(1, Workspace::default());
        Session {
            name: name.into(),
            workspaces,
            current: 1,
        }
    }
}

impl App {
    pub fn session_name(&self) -> &str {
        &self.sessions[self.active_session].name
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// How many panes a session holds, shown or not.
    pub(super) fn session_panes(&self, i: usize) -> usize {
        let map = if i == self.active_session {
            &self.workspaces
        } else {
            &self.sessions[i].workspaces
        };
        map.values().map(Workspace::len).sum()
    }

    /// Every session: index, name, pane count, whether it is shown.
    pub(super) fn session_list(&self) -> Vec<(usize, String, usize, bool)> {
        (0..self.sessions.len())
            .map(|i| {
                (
                    i,
                    self.sessions[i].name.clone(),
                    self.session_panes(i),
                    i == self.active_session,
                )
            })
            .collect()
    }

    pub(super) fn switch_session(&mut self, i: usize) {
        if i == self.active_session || i >= self.sessions.len() {
            return;
        }
        self.exit_copy_mode();
        let prev = self.active_session;
        let slot = &mut self.sessions[prev];
        std::mem::swap(&mut slot.workspaces, &mut self.workspaces);
        slot.current = self.current;
        let next = &mut self.sessions[i];
        std::mem::swap(&mut next.workspaces, &mut self.workspaces);
        self.current = next.current;
        self.active_session = i;
        self.scratch_shown = false;
        self.workspaces.entry(self.current).or_default();
        self.relayout();
    }

    /// A name no session has: the one asked for, or the lowest free number.
    fn free_session_name(&self, wanted: Option<&str>) -> Result<String, String> {
        let taken = |n: &str| self.sessions.iter().any(|s| s.name == n);
        match wanted.map(str::trim).filter(|n| !n.is_empty()) {
            Some(n) if taken(n) => Err(format!("a session called `{n}` exists")),
            Some(n) => Ok(n.to_string()),
            None => Ok((2..)
                .map(|i: u32| i.to_string())
                .find(|n| !taken(n))
                .expect("some number is free")),
        }
    }

    pub(super) fn new_session(&mut self, name: Option<&str>) {
        let name = match self.free_session_name(name) {
            Ok(n) => n,
            Err(e) => {
                self.status = Some(e);
                return;
            }
        };
        self.sessions.push(Session::new(name));
        let i = self.sessions.len() - 1;
        self.switch_session(i);
        if let Err(e) = self.open_pane(None) {
            self.status = Some(format!("new session failed: {e:#}"));
        }
    }

    pub(super) fn rename_session(&mut self, i: usize, name: &str) {
        let name = name.trim();
        if self.sessions.get(i).is_some_and(|s| s.name == name) {
            return;
        }
        match self.free_session_name(Some(name)) {
            Ok(n) if !name.is_empty() => {
                let old = std::mem::replace(&mut self.sessions[i].name, n);
                self.status = Some(format!("renamed {old} to {name}"));
            }
            Ok(_) => self.status = Some("a session needs a name".into()),
            Err(e) => self.status = Some(e),
        }
        self.dirty = true;
    }

    pub(super) fn resolve_session(&self, t: &SessionTarget) -> Option<usize> {
        let n = self.sessions.len();
        match t {
            SessionTarget::Next => Some((self.active_session + 1) % n),
            SessionTarget::Prev => Some((self.active_session + n - 1) % n),
            SessionTarget::Name(name) => self.sessions.iter().position(|s| &s.name == name),
        }
    }

    /// A pane in a session that is not shown: its session and workspace.
    pub(super) fn locate_hidden(&self, id: PaneId) -> Option<(usize, u8)> {
        self.sessions.iter().enumerate().find_map(|(i, s)| {
            s.workspaces
                .iter()
                .find(|(_, ws)| ws.contains(id))
                .map(|(n, _)| (i, *n))
        })
    }

    /// Take a pane out of a session that is not shown. Focus there moves to any
    /// other pane of that workspace; nobody is looking at it.
    pub(super) fn detach_hidden(&mut self, id: PaneId) -> bool {
        let Some((si, n)) = self.locate_hidden(id) else {
            return false;
        };
        let s = &mut self.sessions[si];
        let ws = s.workspaces.get_mut(&n).expect("located");
        ws.take(id);
        if ws.focused == Some(id) {
            ws.focused = ws.panes().first().copied();
        }
        let current = s.current;
        s.workspaces.retain(|k, w| *k == current || !w.is_empty());
        true
    }

    /// Sessions without panes go away. If that is the shown one, the next
    /// session with panes is shown instead, before it goes.
    pub(super) fn drop_empty_sessions(&mut self) {
        if self.session_panes(self.active_session) == 0
            && let Some(other) = (0..self.sessions.len())
                .find(|i| *i != self.active_session && self.session_panes(*i) > 0)
        {
            let prev = self.session_name().to_string();
            self.switch_session(other);
            self.status = Some(format!("session {prev} ended"));
        }
        let active = self.active_session;
        let keep: Vec<bool> = (0..self.sessions.len())
            .map(|i| i == active || self.session_panes(i) > 0)
            .collect();
        let active_name = self.sessions[active].name.clone();
        let mut i = 0;
        self.sessions.retain(|_| {
            let k = keep[i];
            i += 1;
            k
        });
        self.active_session = self
            .sessions
            .iter()
            .position(|s| s.name == active_name)
            .unwrap_or(0);
    }

    pub(super) fn emit_session_switch(&mut self, session: String, previous: String) {
        self.emit(HookEvent::SessionSwitch, |t| {
            t.set("session", session)?;
            t.set("previous", previous)
        });
    }
}

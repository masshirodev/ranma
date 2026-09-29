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
    /// Set by `session_accent` or `ranma open --accent`; over the config's.
    pub accent: Option<crate::theme::Color>,
}

impl Session {
    pub fn new(name: impl Into<String>) -> Session {
        let mut workspaces = BTreeMap::new();
        workspaces.insert(1, Workspace::default());
        Session {
            name: name.into(),
            workspaces,
            current: 1,
            accent: None,
        }
    }
}

impl App {
    pub fn session_name(&self) -> &str {
        &self.sessions[self.active_session].name
    }

    /// The shown session's accent: its own, else the config's for its name.
    pub fn session_accent(&self) -> Option<crate::theme::Color> {
        let s = &self.sessions[self.active_session];
        s.accent
            .or_else(|| self.config.session_accents.get(&s.name).copied())
    }

    /// The theme's colours, with the shown session's accent on the focused
    /// border, the active workspace and the session name. The WM-mode colour
    /// stays: it says which mode keys are in, whatever the session.
    pub fn colors(&self) -> std::borrow::Cow<'_, crate::theme::Colors> {
        let base = &self.config.theme.colors;
        match self.session_accent() {
            None => std::borrow::Cow::Borrowed(base),
            Some(a) => {
                let mut c = base.clone();
                c.border_active = a;
                c.ws_active_bg = a;
                c.bar_accent = a;
                std::borrow::Cow::Owned(c)
            }
        }
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

    /// `ranma open`: go to (or create) the session and workspace, open the pane
    /// there, and name what was asked. A session created here gets this pane as
    /// its first, not a shell next to it.
    /// `ranma open`: a pane in a given session and workspace, or beside a given
    /// pane, named and coloured as asked. Returns the new pane. In the
    /// background, focus and the shown session and workspace stay as they were.
    pub(super) fn open_spec(&mut self, spec: crate::ipc::OpenSpec) -> Result<PaneId, String> {
        let back = (self.active_session, self.current, self.scratch_shown);
        let mut side = None;
        if let Some((target, dir)) = spec.beside {
            if spec.session.is_some() || spec.workspace.is_some() {
                return Err("beside a pane is in that pane's session and workspace; \
                            --session and --workspace do not go with it"
                    .into());
            }
            if let Some((si, _)) = self.locate_hidden(target) {
                self.switch_session(si);
            }
            match self.locate(target) {
                Some(super::SCRATCHPAD) => self.scratch_shown = true,
                Some(n) => self.switch_workspace(n),
                None => return Err(format!("no pane {target} (see `ranma panes`)")),
            }
            side = Some(dir);
        } else {
            if let Some(name) = spec.session.as_deref() {
                match self.sessions.iter().position(|s| s.name == name) {
                    Some(i) => self.switch_session(i),
                    None => {
                        self.sessions.push(Session::new(name));
                        let i = self.sessions.len() - 1;
                        self.switch_session(i);
                    }
                }
            }
            if let Some(t) = &spec.workspace {
                let n = self.resolve(t);
                self.switch_workspace(n);
            }
            self.scratch_shown = false;
        }
        let prior = self.focused();
        if let Some((target, _)) = spec.beside {
            self.focus(target);
        }
        let result =
            self.open_pane_with(spec.command.as_deref(), side, spec.cwd.clone(), &spec.env);
        match result {
            Ok(id) => {
                if let Some(name) = spec.name.as_deref() {
                    self.rename_pane(id, name);
                }
                if let Some(name) = spec.workspace_name.as_deref() {
                    self.rename_workspace(self.current, name);
                }
                if spec.accent.is_some() {
                    self.sessions[self.active_session].accent = spec.accent;
                }
                if let Some((w, h)) = spec.float {
                    let area = self.workspace_area();
                    if !self.active().is_floating(id) {
                        self.toggle_floating_pane(id);
                    }
                    if let Some(r) = self.active_mut().float_rect_mut(id) {
                        *r = area.centered(w as u16, h as u16);
                    }
                }
                if spec.return_focus
                    && let Some(p) = prior
                {
                    self.return_focus.insert(id, p);
                }
                if spec.background {
                    if let Some(p) = prior {
                        self.focus(p);
                    }
                    self.switch_session(back.0);
                    if self.current != back.1 && !self.sessions.is_empty() {
                        self.switch_workspace(back.1);
                    }
                    self.scratch_shown = back.2;
                }
                self.relayout();
                Ok(id)
            }
            Err(e) => {
                // A session made for this pane and left empty goes again.
                self.drop_empty_sessions();
                self.relayout();
                Err(format!("{e:#}"))
            }
        }
    }

    /// The workspace `move_workspace_to_session` would send: the current one, if
    /// it has panes. The scratchpad is every session's, so it never moves.
    pub(super) fn movable_workspace(&self) -> Result<u8, String> {
        if self.scratch_shown {
            return Err("the scratchpad belongs to every session; it does not move".into());
        }
        if self
            .workspaces
            .get(&self.current)
            .is_none_or(Workspace::is_empty)
        {
            return Err(format!("workspace {} is empty", self.current));
        }
        Ok(self.current)
    }

    /// Send the current workspace, whole, to session `to` (or to a new session
    /// called `new`) and follow it. It keeps its number unless that session
    /// already uses it, then takes the lowest free one. A session left without
    /// panes ends, as when its last pane closes.
    pub(super) fn move_workspace_to_session(&mut self, to: Option<usize>, new: Option<&str>) {
        let n = match self.movable_workspace() {
            Ok(n) => n,
            Err(e) => {
                self.status = Some(e);
                return;
            }
        };
        let to = match to {
            Some(i) if i == self.active_session => {
                self.status = Some(format!("already in {}", self.session_name()));
                return;
            }
            Some(i) if i < self.sessions.len() => i,
            Some(_) => return,
            None => match self.free_session_name(new) {
                Ok(name) => {
                    self.sessions.push(Session {
                        name,
                        workspaces: BTreeMap::new(),
                        current: n,
                        accent: None,
                    });
                    self.sessions.len() - 1
                }
                Err(e) => {
                    self.status = Some(e);
                    return;
                }
            },
        };
        let target = &mut self.sessions[to];
        // An empty, unnamed workspace is only a placeholder; a named one is wanted.
        let free = |k: u8| {
            target
                .workspaces
                .get(&k)
                .is_none_or(|w| w.is_empty() && w.name.is_none())
        };
        let Some(slot) = std::iter::once(n).chain(1..=99).find(|k| free(*k)) else {
            self.status = Some(format!("{} has no free workspace", target.name));
            return;
        };
        let ws = self.workspaces.remove(&n).expect("movable");
        target.workspaces.insert(slot, ws);
        target.current = slot;
        let name = target.name.clone();
        // What stays behind is shown from its first workspace next time.
        if let Some(&k) = self.workspaces.keys().next() {
            self.current = k;
        }
        self.switch_session(to);
        self.tidy();
        self.drop_empty_sessions();
        self.status = Some(if slot == n {
            format!("workspace {n} sent to {name}")
        } else {
            format!("workspace {n} sent to {name} as {slot}")
        });
        self.relayout();
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

#[cfg(test)]
mod tests {
    use super::*;

    /// An app with sessions of pane ids and no processes behind them: moving
    /// workspaces is bookkeeping, and bookkeeping is what is under test.
    type Spec<'a> = [(&'a str, &'a [(u8, &'a [PaneId])])];

    fn app(sessions: &Spec) -> App {
        let config = crate::config::load_from(None, None, None).unwrap();
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut app = App::new(config, tx, 80, 24);
        app.sessions.clear();
        for (name, spaces) in sessions {
            let mut s = Session::new(*name);
            s.workspaces.clear();
            for (n, panes) in *spaces {
                let mut ws = Workspace::default();
                for id in *panes {
                    ws.tree
                        .insert(*id, None, None, crate::layout::Placement::Dwindle);
                }
                ws.focused = panes.first().copied();
                s.workspaces.insert(*n, ws);
            }
            s.current = *s.workspaces.keys().next().unwrap_or(&1);
            app.sessions.push(s);
        }
        // Show the first, the way switch_session leaves things.
        app.active_session = 0;
        app.workspaces = std::mem::take(&mut app.sessions[0].workspaces);
        app.current = app.sessions[0].current;
        app
    }

    fn names(app: &App) -> Vec<&str> {
        app.sessions.iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn a_workspace_moves_whole_and_is_followed() {
        let mut a = app(&[("main", &[(1, &[1]), (2, &[2, 3])]), ("ai", &[(1, &[4])])]);
        a.switch_workspace(2);
        a.move_workspace_to_session(Some(1), None);
        assert_eq!(a.session_name(), "ai");
        assert_eq!(a.current, 2);
        assert_eq!(a.workspaces[&2].panes(), vec![2, 3]);
        assert_eq!(a.workspaces[&1].panes(), vec![4]);
        // What stayed behind is intact and shown from its remaining workspace.
        assert_eq!(a.sessions[0].workspaces[&1].panes(), vec![1]);
        assert_eq!(a.sessions[0].current, 1);
        assert!(!a.sessions[0].workspaces.contains_key(&2));
    }

    #[test]
    fn a_taken_number_gives_way_to_the_lowest_free_one() {
        let mut a = app(&[
            ("main", &[(1, &[1]), (2, &[2])]),
            ("ai", &[(1, &[3]), (2, &[4])]),
        ]);
        a.move_workspace_to_session(Some(1), None);
        assert_eq!(a.current, 3);
        assert_eq!(a.workspaces[&3].panes(), vec![1]);
        assert_eq!(a.status.as_deref(), Some("workspace 1 sent to ai as 3"));
    }

    #[test]
    fn the_last_workspace_leaving_ends_its_session() {
        let mut a = app(&[("main", &[(1, &[1, 2])]), ("ai", &[(1, &[3])])]);
        a.move_workspace_to_session(Some(1), None);
        assert_eq!(names(&a), vec!["ai"]);
        assert_eq!(a.active_session, 0);
        assert_eq!(a.workspaces[&2].panes(), vec![1, 2]);
    }

    #[test]
    fn a_new_name_makes_the_session() {
        let mut a = app(&[("main", &[(1, &[1]), (4, &[2])])]);
        a.switch_workspace(4);
        a.move_workspace_to_session(None, Some("ai-projects"));
        assert_eq!(names(&a), vec!["main", "ai-projects"]);
        assert_eq!(a.session_name(), "ai-projects");
        assert_eq!(a.workspaces.keys().copied().collect::<Vec<_>>(), vec![4]);
        a.move_workspace_to_session(None, Some("main"));
        assert_eq!(a.status.as_deref(), Some("a session called `main` exists"));
    }

    #[test]
    fn empty_workspaces_the_scratchpad_and_here_stay_put() {
        let mut a = app(&[("main", &[(1, &[1])]), ("ai", &[(1, &[2])])]);
        a.move_workspace_to_session(Some(0), None);
        assert_eq!(a.status.as_deref(), Some("already in main"));
        a.switch_workspace(5);
        a.move_workspace_to_session(Some(1), None);
        assert_eq!(a.status.as_deref(), Some("workspace 5 is empty"));
        a.switch_workspace(1);
        a.scratch_shown = true;
        a.move_workspace_to_session(Some(1), None);
        assert!(a.status.as_deref().unwrap().contains("scratchpad"));
        assert_eq!(a.session_name(), "main");
    }

    #[test]
    fn the_action_by_name_creates_like_ranma_open() {
        let mut a = app(&[("main", &[(1, &[1]), (2, &[2])])]);
        a.run_action("move_workspace_to_session work".parse().unwrap());
        assert_eq!(a.session_name(), "work");
        a.run_action("move_workspace_to_session prev".parse().unwrap());
        assert_eq!(a.session_name(), "main");
        assert_eq!(names(&a), vec!["main"]);
    }

    #[test]
    fn the_switchers_open_on_where_you_are() {
        let mut a = app(&[
            ("main", &[(1, &[1])]),
            ("ai", &[(1, &[2])]),
            ("x", &[(1, &[3])]),
        ]);
        a.switch_session(1);
        a.open_session_switcher();
        let p = a.picker.as_ref().unwrap();
        assert_eq!(p.selected, 1);
        assert!(p.visible()[1].current);
        assert_eq!(p.visible().iter().filter(|i| i.current).count(), 1);
        // Moving starts on a session that is not this one.
        a.picker = None;
        a.open_move_workspace();
        let p = a.picker.as_ref().unwrap();
        assert_eq!(p.kind, crate::picker::Kind::MoveWorkspace);
        assert_eq!(p.selected, 0);
    }
}

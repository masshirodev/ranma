//! A server's snapshot of itself, and bringing the last one back after a
//! reboot (DESIGN.md, "Layouts", the snapshot part). The format and the files
//! are `crate::restore`; this captures panes and respawns them.

use super::layouts::Typing;
use super::*;
use crate::layouts::Spec;
use crate::picker::{Kind, Picker};
use crate::restore::{self, Float};

/// How long after something happens the snapshot is written: activity
/// coalesces into one write, and nothing is written while idle.
pub(super) const SNAPSHOT_AFTER: Duration = Duration::from_secs(5);

/// Where this server's snapshots go, and what was last written there.
pub(super) struct Snapshots {
    pub(super) dir: std::path::PathBuf,
    pub(super) name: String,
    last_text: Option<String>,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl App {
    /// Start keeping snapshots as the server `name`. A fresh server (not one
    /// taking a new build in place) first sets its name's last snapshot aside
    /// and offers it.
    pub fn start_snapshots(&mut self, name: &str, fresh: bool) {
        if self.config.settings.restore == config::RestoreMode::Off
            || !crate::layouts::valid_name(name)
        {
            return;
        }
        let Some(dir) = restore::dir() else {
            return;
        };
        self.start_snapshots_in(dir, name, fresh);
    }

    pub(super) fn start_snapshots_in(&mut self, dir: std::path::PathBuf, name: &str, fresh: bool) {
        if fresh {
            match restore::set_aside(&dir, name) {
                Ok(Some(snap)) => self.offer_restore(snap),
                Ok(None) => {}
                Err(e) => self.status = Some(format!("restore: {e:#}")),
            }
        }
        self.snapshots = Some(Snapshots {
            dir,
            name: name.to_string(),
            last_text: None,
        });
    }

    fn offer_restore(&mut self, snap: restore::Server) {
        let message = format!(
            "{}.  Enter: restore, commands waiting on the prompt · r: restore and run them · Esc: not now",
            snap.summary(now_secs())
        );
        self.picker = Some(Picker::question(
            Kind::ConfirmRestore,
            "bring back the last session?",
            message,
        ));
        self.restore_offer = Some(snap);
        self.dirty = true;
    }

    /// The question's answer: `run` types each command and Enter.
    pub(super) fn accept_restore(&mut self, run: bool) {
        if let Some(snap) = self.restore_offer.take() {
            self.restore_from(&snap, run);
        }
    }

    /// The `restore` action: the snapshot set aside at this server's start.
    pub(super) fn restore_last(&mut self, run: bool) {
        self.dirty = true;
        let Some(s) = &self.snapshots else {
            self.status = Some(match self.config.settings.restore {
                config::RestoreMode::Off => {
                    "restore is off (ranma.set { restore = \"ask\" })".into()
                }
                config::RestoreMode::Ask => {
                    "no snapshots here: a server keeps them, not --standalone".into()
                }
            });
            return;
        };
        match restore::read(&restore::last(&s.dir, &s.name)) {
            Ok(Some(snap)) if snap.panes() > 0 => self.restore_from(&snap, run),
            Ok(_) => self.status = Some("nothing to restore".into()),
            Err(e) => self.status = Some(format!("restore: {e:#}")),
        }
    }

    fn restore_from(&mut self, snap: &restore::Server, run: bool) {
        let typing = if run { Typing::Run } else { Typing::Wait };
        let (opened, failed) = self.restore(snap, typing);
        self.status = Some(match failed {
            Some(e) => format!("restored {opened} panes; one failed to start: {e}"),
            None => format!(
                "restored {opened} pane{}{}",
                if opened == 1 { "" } else { "s" },
                if run {
                    ""
                } else {
                    ": Enter runs what is on each prompt"
                }
            ),
        });
        self.note_snapshot(Instant::now());
    }

    /// Bring a snapshot back: sessions and workspaces by name and number,
    /// filled as `load_layout` fills one, so panes already there take places
    /// in it. Ends where the snapshot was looking.
    pub(super) fn restore(
        &mut self,
        snap: &restore::Server,
        typing: Typing,
    ) -> (usize, Option<String>) {
        let mut opened = 0;
        let mut failed = None;
        let mut note = |r: (usize, Option<String>)| {
            opened += r.0;
            if failed.is_none() {
                failed = r.1;
            }
        };
        self.exit_copy_mode();
        self.scratch_shown = false;
        for s in &snap.sessions {
            let i = match self.sessions.iter().position(|x| x.name == s.name) {
                Some(i) => i,
                None => {
                    // An empty slot: switching to it gives it workspace 1.
                    self.sessions.push(Session {
                        name: s.name.clone(),
                        ..Session::default()
                    });
                    self.sessions.len() - 1
                }
            };
            if let Some(a) = s.accent.as_deref().and_then(|a| a.parse().ok()) {
                self.sessions[i].accent = Some(a);
            }
            self.switch_session(i);
            for w in &s.workspaces {
                self.switch_workspace(w.n);
                if let Some(name) = &w.name {
                    self.active_mut().name = Some(name.clone());
                }
                if let Some(spec) = &w.layout {
                    note(self.apply_spec(spec, typing));
                }
                for f in &w.floats {
                    note(self.spawn_float(f, typing));
                }
                if let Some(id) = w.focus.and_then(|k| self.active().panes().get(k).copied()) {
                    self.focus(id);
                }
                self.relayout();
            }
            self.switch_workspace(s.current);
        }
        if !snap.scratchpad.is_empty() {
            self.scratch_shown = true;
            for f in &snap.scratchpad {
                note(self.spawn_float(f, typing));
            }
            self.scratch_shown = false;
        }
        if let Some(i) = self.sessions.iter().position(|x| x.name == snap.active) {
            self.switch_session(i);
        }
        self.tidy();
        self.relayout();
        (opened, failed)
    }

    /// A float from a snapshot, on the shown layer, at its share of the area.
    fn spawn_float(&mut self, f: &Float, typing: Typing) -> (usize, Option<String>) {
        let area = self.workspace_area();
        let at = |frac: f32, len: u16| (frac.clamp(0.0, 1.0) * len as f32).round() as u16;
        let r = Rect::new(
            area.x + at(f.x, area.w),
            area.y + at(f.y, area.h),
            at(f.w, area.w).max(10),
            at(f.h, area.h).max(4),
        )
        .clamp_into(area);
        let id = self.next_id;
        self.next_id += 1;
        self.active_mut().floating.push((id, r));
        match self.spawn_into(id, f.cwd.as_deref(), f.command.as_deref(), typing) {
            Ok(()) => (1, None),
            Err(e) => {
                self.active_mut().take(id);
                (0, Some(e))
            }
        }
    }

    /// Something happened that the snapshot may not have yet: write it once
    /// `SNAPSHOT_AFTER` has passed. Only events call this, never the timer.
    pub(super) fn note_snapshot(&mut self, now: Instant) {
        if self.snapshots.is_some() && self.snapshot_due.is_none() {
            self.snapshot_due = Some(now + SNAPSHOT_AFTER);
        }
    }

    /// Write the snapshot if it changed; an empty server removes its own.
    pub fn write_snapshot(&mut self) {
        self.snapshot_due = None;
        let snap = self.server_snapshot();
        let Some(s) = &mut self.snapshots else {
            return;
        };
        let path = restore::current(&s.dir, &s.name);
        if snap.panes() == 0 {
            let _ = std::fs::remove_file(&path);
            s.last_text = None;
            return;
        }
        // The time is left out of the comparison: only a change is written.
        let text = match restore::render(&restore::Server {
            saved: 0,
            ..snap.clone()
        }) {
            Ok(t) => t,
            Err(_) => return,
        };
        if s.last_text.as_deref() == Some(text.as_str()) {
            return;
        }
        if let Ok(full) = restore::render(&snap)
            && restore::write(&path, &full).is_ok()
        {
            s.last_text = Some(text);
        }
    }

    /// Everything a restore would need, as it is now.
    pub(super) fn server_snapshot(&self) -> restore::Server {
        let about = self.pane_about();
        let area = self.workspace_area();
        let float = |(id, r): &(PaneId, Rect)| {
            let (cwd, command) = about(*id);
            let frac = |v: u16, origin: u16, len: u16| {
                ((v.saturating_sub(origin)) as f32 / len.max(1) as f32 * 1000.0).round() / 1000.0
            };
            Float {
                x: frac(r.x, area.x, area.w),
                y: frac(r.y, area.y, area.h),
                w: frac(r.w, 0, area.w),
                h: frac(r.h, 0, area.h),
                cwd,
                command,
            }
        };
        let workspace = |n: u8, ws: &Workspace| -> Option<restore::Workspace> {
            if ws.is_empty() && ws.name.is_none() {
                return None;
            }
            Some(restore::Workspace {
                n,
                name: ws.name.clone(),
                focus: ws
                    .focused
                    .and_then(|f| ws.panes().iter().position(|p| *p == f)),
                layout: ws.tree.root.as_ref().map(|r| Spec::from_node(r, &about)),
                floats: ws.floating.iter().map(float).collect(),
            })
        };
        let sessions = self
            .sessions
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let (map, current) = if i == self.active_session {
                    (&self.workspaces, self.current)
                } else {
                    (&s.workspaces, s.current)
                };
                restore::Session {
                    name: s.name.clone(),
                    current,
                    accent: s.accent.map(|c| c.to_string()),
                    workspaces: map.iter().filter_map(|(n, w)| workspace(*n, w)).collect(),
                }
            })
            .collect();
        restore::Server {
            version: restore::VERSION,
            saved: now_secs(),
            active: self.session_name().to_string(),
            sessions,
            scratchpad: self.scratch.floating.iter().map(float).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(tag: &str) -> (App, std::path::PathBuf) {
        let config = crate::config::load_from(None, None, None).unwrap();
        let (tx, _rx) = std::sync::mpsc::channel();
        let a = App::new(config, tx, 120, 40);
        let dir =
            std::env::temp_dir().join(format!("ranma-app-restore-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        (a, dir)
    }

    fn with_pane_in(a: &mut App, n: u8, id: PaneId) {
        let ws = a.workspaces.entry(n).or_default();
        ws.tree.insert(id, ws.focused, None, Placement::Dwindle);
        ws.focused = Some(id);
    }

    #[test]
    fn a_capture_holds_sessions_workspaces_and_floats() {
        let (mut a, _) = app("capture");
        with_pane_in(&mut a, 1, 1);
        with_pane_in(&mut a, 1, 2);
        a.workspaces.get_mut(&1).unwrap().focused = Some(1);
        a.workspaces.entry(3).or_default().name = Some("notes".into());
        let area = a.workspace_area();
        a.workspaces
            .get_mut(&1)
            .unwrap()
            .floating
            .push((3, Rect::new(area.x, area.y, area.w / 2, area.h / 2)));
        let snap = a.server_snapshot();
        assert_eq!(snap.version, restore::VERSION);
        assert_eq!(snap.active, a.session_name());
        let s = &snap.sessions[0];
        let ns: Vec<u8> = s.workspaces.iter().map(|w| w.n).collect();
        assert_eq!(
            ns,
            vec![1, 3],
            "an empty named workspace is kept, an empty one not"
        );
        let w1 = &s.workspaces[0];
        assert_eq!(w1.layout.as_ref().unwrap().panes().len(), 2);
        assert_eq!(w1.focus, Some(0));
        assert_eq!((w1.floats[0].x, w1.floats[0].w), (0.0, 0.5));
        assert_eq!(snap.panes(), 3);
    }

    #[test]
    fn snapshots_wait_for_activity_and_write_only_changes() {
        let (mut a, dir) = app("write");
        a.start_snapshots_in(dir.clone(), "1", true);
        assert!(a.picker.is_none(), "nothing to offer the first time");
        let t = Instant::now();
        a.note_snapshot(t);
        assert_eq!(a.snapshot_due, Some(t + SNAPSHOT_AFTER));
        a.note_snapshot(t + Duration::from_secs(1));
        assert_eq!(a.snapshot_due, Some(t + SNAPSHOT_AFTER), "coalesced");
        assert!(a.next_deadline().is_some_and(|d| d <= t + SNAPSHOT_AFTER));
        // Empty: nothing is written.
        a.write_snapshot();
        let path = restore::current(&dir, "1");
        assert!(!path.exists());
        with_pane_in(&mut a, 1, 1);
        a.write_snapshot();
        assert!(path.exists());
        let first = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(Duration::from_millis(20));
        a.write_snapshot();
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            first,
            "unchanged, not rewritten"
        );
        // A server that ends empty leaves nothing to offer.
        a.workspaces.get_mut(&1).unwrap().tree = Default::default();
        a.write_snapshot();
        assert!(!path.exists());
    }

    #[test]
    fn a_fresh_server_offers_the_last_and_the_question_takes_three_answers() {
        let (mut a, dir) = app("offer");
        with_pane_in(&mut a, 1, 1);
        a.start_snapshots_in(dir.clone(), "1", false);
        a.write_snapshot();
        let (mut b, _) = app("offer-b");
        b.start_snapshots_in(dir.clone(), "1", true);
        let p = b.picker.as_mut().expect("the question");
        assert_eq!(p.kind, Kind::ConfirmRestore);
        assert!(
            p.message
                .as_deref()
                .unwrap()
                .starts_with("1 pane in 1 session")
        );
        use crossterm::event::{KeyCode, KeyModifiers};
        let key = |c| KeyEvent::new(c, KeyModifiers::NONE);
        assert_eq!(
            p.key(&key(KeyCode::Char('r'))),
            crate::picker::Outcome::Submit("run".into())
        );
        assert_eq!(
            p.key(&key(KeyCode::Enter)),
            crate::picker::Outcome::Submit("y".into())
        );
        assert_eq!(p.key(&key(KeyCode::Esc)), crate::picker::Outcome::Cancel);
        assert!(b.restore_offer.is_some());
        // An upgrade in place asks nothing.
        let (mut c, _) = app("offer-c");
        c.start_snapshots_in(dir, "1", false);
        assert!(c.picker.is_none());
    }

    #[test]
    fn restoring_reuses_the_panes_there_and_names_what_was_named() {
        let (mut a, _) = app("restore");
        // The shell a new server opens first.
        with_pane_in(&mut a, 1, 1);
        let snap = restore::Server {
            version: restore::VERSION,
            saved: 0,
            active: a.session_name().to_string(),
            sessions: vec![restore::Session {
                name: a.session_name().to_string(),
                current: 1,
                accent: Some("#ff6a6a".into()),
                workspaces: vec![
                    restore::Workspace {
                        n: 1,
                        layout: Some(Spec::default()),
                        ..Default::default()
                    },
                    restore::Workspace {
                        n: 4,
                        name: Some("notes".into()),
                        ..Default::default()
                    },
                ],
            }],
            scratchpad: vec![],
        };
        let (opened, failed) = a.restore(&snap, Typing::Wait);
        assert_eq!(
            (opened, failed),
            (0, None),
            "the one pane took the one place"
        );
        assert_eq!(a.active().tree.panes(), vec![1]);
        assert_eq!(a.current, 1);
        assert_eq!(
            a.workspaces.get(&4).and_then(|w| w.name.as_deref()),
            Some("notes")
        );
        assert_eq!(
            a.session_accent(),
            Some(crate::theme::Color::Rgb(255, 106, 106))
        );
    }
}

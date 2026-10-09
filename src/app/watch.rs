//! `pane:watch(pattern, fn)`: a plugin hears when a regex newly matches on a
//! pane's screen (DESIGN.md, "Watching a pane's screen").
//!
//! It stays off the PTY path. Output only marks the pane for a look, and the
//! look happens at most every [`WATCH_LOOK`], over the screen's rows alone
//! (never the scrollback), in the app's own loop. A pane with no watch is
//! never looked at. A watched pane out of sight gets its wakeups back at
//! each look, so a hidden agent's prompt is still seen, at most ten times a
//! second while it prints.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use alacritty_terminal::term::search::RegexSearch;
use mlua::Function;

use super::App;
use crate::layout::PaneId;
use crate::panetext;

/// How long after output a watched pane is looked at: what arrives within it
/// is one look.
pub(super) const WATCH_LOOK: Duration = Duration::from_millis(100);

pub(super) struct Watch {
    pane: PaneId,
    regex: RegexSearch,
    f: std::rc::Rc<mlua::RegistryKey>,
    /// The rows matched at the last look.
    seen: HashSet<String>,
}

impl App {
    pub(super) fn add_watch(
        &mut self,
        id: u64,
        pane: PaneId,
        pattern: &str,
        f: std::rc::Rc<mlua::RegistryKey>,
    ) -> Result<(), String> {
        if !self.panes.contains_key(&pane) {
            return Err(format!("no pane {pane}"));
        }
        if self.watches.len() >= crate::luapane::MAX_WATCHES {
            return Err(format!(
                "pane:watch: {} watches already",
                crate::luapane::MAX_WATCHES
            ));
        }
        let regex = RegexSearch::new(pattern).map_err(|e| format!("pane:watch: {e}"))?;
        self.watches.insert(
            id,
            Watch {
                pane,
                regex,
                f,
                seen: HashSet::new(),
            },
        );
        // A first look now: a match already on screen is news too.
        self.watch_pending.insert(pane);
        self.watch_due = Some(Instant::now());
        Ok(())
    }

    pub(super) fn unwatch(&mut self, id: u64) {
        self.watches.remove(&id);
    }

    /// A pane printed: if something watches it, look at it soon.
    pub(super) fn watch_output(&mut self, pane: PaneId) {
        if self.watches.values().any(|w| w.pane == pane) {
            self.watch_pending.insert(pane);
            if self.watch_due.is_none() {
                self.watch_due = Some(Instant::now() + WATCH_LOOK);
            }
        }
    }

    pub(super) fn forget_watches(&mut self, pane: PaneId) {
        self.watches.retain(|_, w| w.pane != pane);
        self.watch_pending.remove(&pane);
    }

    /// Look at the panes that printed, and call each watch whose pattern
    /// newly matches.
    pub(super) fn look(&mut self, now: Instant) {
        if self.watch_due.is_none_or(|t| t > now) {
            return;
        }
        self.watch_due = None;
        let pending: Vec<PaneId> = self.watch_pending.drain().collect();
        let mut calls: Vec<(std::rc::Rc<mlua::RegistryKey>, PaneId, panetext::Seen)> = Vec::new();
        for pane in pending {
            let Some(p) = self.panes.get(&pane) else {
                continue;
            };
            {
                let term = p.term.lock();
                for w in self.watches.values_mut().filter(|w| w.pane == pane) {
                    let now = panetext::screen_matches(&term, &mut w.regex);
                    for s in panetext::new_matches(&w.seen, &now) {
                        calls.push((w.f.clone(), pane, s));
                    }
                    w.seen = now.into_iter().map(|s| s.row).collect();
                }
            }
            // Out of sight, a pane sends one wakeup and then none until it is
            // drawn: ask for the next, so the watch keeps seeing it.
            if !self.visible.contains(&pane) {
                p.drawn();
            }
        }
        for (f, pane, s) in calls {
            let Ok(f) = self.config.lua.registry_value::<Function>(&f) else {
                continue;
            };
            self.call_lua(|lua| {
                let t = lua.create_table()?;
                t.set("pane", pane)?;
                t.set("line", s.line)?;
                t.set("col", s.col)?;
                t.set("text", s.text)?;
                t.set("row", s.row)?;
                f.call::<()>(t)
            });
        }
    }
}

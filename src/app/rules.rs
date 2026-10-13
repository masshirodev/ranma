//! Window rules: float, size or send a pane somewhere when its command or title
//! matches. Command rules apply when an `exec` pane opens; title rules apply the
//! first time a pane's title matches, once per pane and rule, so a program that
//! keeps setting its title is not moved again and again.

use super::{App, SCRATCHPAD};
use crate::config::Rule;
use crate::layout::{PaneId, Placement};

/// Whose badges a rule's are, beside plugins'.
pub(super) const RULES_OWNER: &str = "rules";

impl App {
    pub(super) fn apply_command_rules(&mut self, id: PaneId, command: &str) {
        let rules: Vec<(usize, Rule)> = self
            .config
            .rules
            .iter()
            .enumerate()
            .filter(|(_, r)| r.matches_command(command))
            .map(|(i, r)| (i, r.clone()))
            .collect();
        for (i, r) in rules {
            self.rules_applied.insert((id, i));
            self.apply_rule(id, &r);
        }
    }

    pub(super) fn apply_title_rules(&mut self, id: PaneId, title: &str) {
        let rules: Vec<(usize, Rule)> = self
            .config
            .rules
            .iter()
            .enumerate()
            .filter(|(i, r)| r.matches_title(title) && !self.rules_applied.contains(&(id, *i)))
            .map(|(i, r)| (i, r.clone()))
            .collect();
        for (i, r) in rules {
            self.rules_applied.insert((id, i));
            self.apply_rule(id, &r);
        }
    }

    /// Rules on what runs in a pane (`program`, `cwd`, `host`), checked
    /// where the bar reads the panes' programs: after output or a key, at
    /// most every 500 ms. Only panes some such rule has not matched yet are
    /// read, and nothing at all without such rules.
    pub(super) fn apply_late_rules(&mut self) {
        let late: Vec<usize> = (0..self.config.rules.len())
            .filter(|i| self.config.rules[*i].is_late())
            .collect();
        if late.is_empty() {
            return;
        }
        let home = dirs::home_dir();
        let ids: Vec<PaneId> = self.workspaces.values().flat_map(|ws| ws.panes()).collect();
        for id in ids {
            let pending: Vec<usize> = late
                .iter()
                .copied()
                .filter(|i| !self.rules_applied.contains(&(id, *i)))
                .collect();
            if pending.is_empty() {
                continue;
            }
            let Some(p) = self.panes.get(&id) else {
                continue;
            };
            let wants = |f: fn(&Rule) -> bool| pending.iter().any(|i| f(&self.config.rules[*i]));
            let program = wants(|r| r.program.is_some())
                .then(|| p.program())
                .flatten();
            let cwd = wants(|r| r.cwd.is_some())
                .then(|| p.cwd())
                .flatten()
                .map(|c| crate::layouts::tilde(&c, home.as_deref()));
            let host = wants(|r| r.host.is_some())
                .then(|| p.ssh_host().or_else(|| p.inner_host().map(str::to_string)))
                .flatten();
            let title = p.label().to_string();
            let matched: Vec<(usize, Rule)> = pending
                .into_iter()
                .filter(|i| {
                    self.config.rules[*i].matches_late(
                        &title,
                        program.as_deref(),
                        cwd.as_deref(),
                        host.as_deref(),
                    )
                })
                .map(|i| (i, self.config.rules[i].clone()))
                .collect();
            for (i, r) in matched {
                self.rules_applied.insert((id, i));
                self.apply_rule(id, &r);
            }
        }
    }

    fn apply_rule(&mut self, id: PaneId, rule: &Rule) {
        if let Some(name) = &rule.name {
            self.rename_pane(id, name);
        }
        if let Some(b) = &rule.badge {
            let owner = RULES_OWNER.to_string();
            if !self.badge_owners.contains(&owner) {
                self.badge_owners.push(owner.clone());
            }
            let list = self.badges.entry(id).or_default();
            list.retain(|(o, _)| *o != owner);
            list.push((owner, b.clone()));
            self.dirty = true;
        }
        // Rules act on panes of the shown session; the scratchpad keeps its own
        // tiling (it is already a floating layer).
        let Some(n) = self.locate(id).filter(|n| *n != SCRATCHPAD) else {
            return;
        };
        if rule.float {
            let area = self.workspace_area();
            let (pw, ph) = rule.size.unwrap_or((60, 60));
            let ws = self.ws_mut(n);
            if ws.tree.remove(id) {
                let r = ws.cascade(area, pw, ph);
                ws.floating.push((id, r));
            } else if let Some(r) = ws.float_rect_mut(id) {
                *r = area.centered(pw, ph);
            }
            if ws.focused.is_none() {
                ws.focused = Some(id);
            }
        }
        if let Some(target) = rule.workspace
            && target != n
        {
            self.move_pane_to(id, target, !rule.silent);
        }
        if let Some(name) = &rule.group {
            self.join_named_group(id, name);
        }
        self.relayout();
    }

    /// Tab `id` into the group called `name` in its workspace (a pane a rule
    /// put there that is still in a group), or start that group with it.
    /// A floating pane is tiled first: a group is part of the tiling.
    fn join_named_group(&mut self, id: PaneId, name: &str) {
        let Some(n) = self.locate(id).filter(|n| *n != SCRATCHPAD) else {
            return;
        };
        let member = self
            .pane_groups
            .iter()
            .filter(|(p, g)| **p != id && g.as_str() == name)
            .map(|(p, _)| *p)
            .find(|p| {
                self.locate(*p) == Some(n)
                    && self
                        .workspaces
                        .get(&n)
                        .is_some_and(|w| w.tree.is_grouped(*p))
            });
        let ws = self.ws_mut(n);
        let floated = ws.floating.iter().any(|(p, _)| *p == id);
        ws.floating.retain(|(p, _)| *p != id);
        match member {
            Some(m) => {
                ws.tree.remove(id);
                if !ws.tree.join_group(id, m) {
                    ws.tree.insert(id, Some(m), None, Placement::Dwindle);
                }
            }
            None => {
                if floated {
                    let f = ws.focused.filter(|f| *f != id);
                    ws.tree.insert(id, f, None, Placement::Dwindle);
                }
                ws.tree.group_alone(id);
            }
        }
        self.pane_groups.insert(id, name.to_string());
    }

    /// Toggle floating on a pane, remembering where it floated: a pane floated
    /// again returns to that spot instead of the centre.
    pub(super) fn toggle_floating_pane(&mut self, id: PaneId) {
        let area = self.workspace_area();
        let gap = self.config.theme.gaps.inner;
        let ws = self.active_mut();
        match ws.take(id) {
            Some(None) => {
                let cascade = ws.cascade(area, 60, 60);
                let r = ws
                    .float_memory
                    .remove(&id)
                    .map(|r| r.clamp_into(area))
                    .unwrap_or(cascade);
                ws.floating.push((id, r));
                ws.focused = Some(id);
            }
            Some(Some(r)) => {
                ws.float_memory.insert(id, r);
                // Tile it next to whatever it was floating over, so it lands where
                // the eye already is.
                let (cx, cy) = (r.x + r.w / 2, r.y + r.h / 2);
                let under = ws
                    .tree
                    .layout(area, gap)
                    .into_iter()
                    .find(|(_, t)| t.contains(cx, cy));
                ws.tree.insert(
                    id,
                    under.map(|(p, _)| p),
                    under.map(|(_, t)| t),
                    Placement::Dwindle,
                );
                ws.focused = Some(id);
            }
            None => {}
        }
        self.relayout();
    }
}

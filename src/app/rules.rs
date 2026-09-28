//! Window rules: float, size or send a pane somewhere when its command or title
//! matches. Command rules apply when an `exec` pane opens; title rules apply the
//! first time a pane's title matches, once per pane and rule, so a program that
//! keeps setting its title is not moved again and again.

use super::{App, SCRATCHPAD};
use crate::config::Rule;
use crate::layout::{PaneId, Placement};

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

    fn apply_rule(&mut self, id: PaneId, rule: &Rule) {
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
        self.relayout();
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

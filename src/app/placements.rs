//! Workspaces laid out by a placement policy in Lua (DESIGN.md, "Placement
//! policies in Lua"; the checks are `crate::placement`). The policy runs on
//! structural change only: a pane in or out, the area resized, the layout
//! chosen, or a message from a resize or move key. Never per frame.

use super::App;
use crate::config::Layout;
use crate::layout::PaneId;
use crate::placement::PolicyId;

/// What a policy last arranged in a workspace: when this changes, it runs
/// again.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Shape {
    policy: PolicyId,
    panes: Vec<PaneId>,
    size: (u16, u16),
}

impl App {
    /// Give every workspace laid out by a policy whose shape changed the
    /// tree its policy returns. Called from `relayout`.
    pub(super) fn arrange_placements(&mut self) {
        let setting = self.config.settings.layout;
        let area = self.workspace_area();
        let due: Vec<(u8, PolicyId)> = self
            .workspaces
            .iter()
            .filter_map(|(n, ws)| match ws.layout.unwrap_or(setting) {
                Layout::Lua(id) => {
                    let mut panes = ws.tree.panes();
                    panes.sort_unstable();
                    let shape = Shape {
                        policy: id,
                        panes,
                        size: (area.w, area.h),
                    };
                    (self.placement_shapes.get(n) != Some(&shape)).then_some((*n, id))
                }
                _ => None,
            })
            .collect();
        for (n, id) in due {
            self.run_placement(n, id, None);
        }
    }

    /// A resize or move key on a workspace laid out by a policy: the policy
    /// gets it as a message (`resize right 4`) and answers with a tree, or
    /// with nothing to leave it be.
    pub(super) fn placement_message(&mut self, message: &str) -> bool {
        let n = self.current;
        let Layout::Lua(id) = self.layout_here() else {
            return false;
        };
        if self.scratch_shown {
            return false;
        }
        self.run_placement(n, id, Some(message));
        self.relayout();
        true
    }

    fn run_placement(&mut self, n: u8, id: PolicyId, message: Option<&str>) {
        let area = self.workspace_area();
        let Some(ws) = self.workspaces.get(&n) else {
            return;
        };
        let panes = ws.tree.panes();
        let focused = ws.focused.filter(|f| panes.contains(f));
        let mut sorted = panes.clone();
        sorted.sort_unstable();
        // Noted before the call: a policy that errors is not called again
        // until the shape changes, rather than on every relayout.
        self.placement_shapes.insert(
            n,
            Shape {
                policy: id,
                panes: sorted,
                size: (area.w, area.h),
            },
        );
        if panes.is_empty() {
            return;
        }
        let name = crate::placement::name(id);
        let Some(key) = self
            .config
            .placements
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, k)| k.clone())
        else {
            self.toast(
                format!("no placement `{name}` in this configuration; dwindle meanwhile"),
                crate::toast::Level::Urgent,
                None,
            );
            return;
        };
        let state = self.placement_state.get(&n).cloned();
        let panes_arg = panes.clone();
        let out = self.call_lua(|lua| {
            let f: mlua::Function = lua.registry_value(&key)?;
            let state: mlua::Table = match &state {
                Some(k) => lua.registry_value(k)?,
                None => lua.create_table()?,
            };
            let ws = lua.create_table()?;
            ws.set("panes", panes_arg)?;
            ws.set("focused", focused)?;
            ws.set("width", area.w)?;
            ws.set("height", area.h)?;
            ws.set("workspace", n)?;
            ws.set("state", state.clone())?;
            ws.set("message", message)?;
            let v: mlua::Value = f.call(ws)?;
            Ok((v, state))
        });
        let Some((v, state)) = out else {
            // call_lua said why, on the status line; the tree stays dwindle's.
            return;
        };
        if !self.placement_state.contains_key(&n)
            && let Ok(k) = self.config.lua.create_registry_value(state)
        {
            self.placement_state.insert(n, std::rc::Rc::new(k));
        }
        if v.is_nil() {
            return;
        }
        match crate::placement::tree(&v, &panes) {
            Ok(node) => {
                if let Some(ws) = self.workspaces.get_mut(&n) {
                    ws.tree.root = Some(node);
                }
            }
            Err(e) => self.toast(
                format!("placement `{name}`: {e}; dwindle meanwhile"),
                crate::toast::Level::Urgent,
                None,
            ),
        }
    }
}

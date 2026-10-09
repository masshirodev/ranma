//! The pickers ranma opens: the pane switcher, the session switcher, the server
//! switcher, the rename prompt, and the palette (help, every bind runnable, and
//! every action).

use crossterm::event::{KeyEvent, MouseEvent, MouseEventKind};
use unicode_width::UnicodeWidthStr;

use super::{App, SCRATCHPAD};
use crate::keys::Chord;
use crate::layout::Rect;
use crate::picker::{Item, Kind, Outcome, PaletteMode, Picker, Target};

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
        // Never wider or taller than the screen: a terminal a few cells wide
        // (a nested ranma in a narrow pane) still gets a picker, cut short,
        // rather than a panic.
        let fit = s.w.saturating_sub(4).max(10).min(s.w);
        // A menu opens at the pointer, as small as its entries, kept on screen.
        if p.kind == Kind::Menu
            && let Some((mx, my)) = self.menu_at
        {
            let widest = p
                .visible()
                .iter()
                .map(|it| {
                    UnicodeWidthStr::width(it.label.as_str())
                        + UnicodeWidthStr::width(it.detail.as_str())
                })
                .max()
                .unwrap_or(10)
                .max(UnicodeWidthStr::width(p.title.as_str()));
            let w = (widest as u16 + 7).min(s.w);
            let h = (items.max(1) + 3).min(s.h);
            let outer = Rect::new(
                mx.min(s.right().saturating_sub(w)),
                my.min(s.bottom().saturating_sub(h)),
                w,
                h,
            );
            let inner = outer.inset(1, 1);
            let list = Rect::new(inner.x, inner.y + 1, inner.w, inner.h.saturating_sub(1));
            return Some(PickerLayout {
                outer,
                query: Rect::new(inner.x, inner.y, inner.w, inner.h.min(1)),
                list,
                offset: p.selected.saturating_sub(list.h.saturating_sub(1) as usize),
            });
        }
        let (w, h) = if p.is_prompt() {
            // A prompt is one row between borders, as wide as its text needs:
            // cutting a question short is worse than a wide box.
            let text = p
                .message
                .as_deref()
                .map(UnicodeWidthStr::width)
                .unwrap_or(40)
                .max(UnicodeWidthStr::width(p.title.as_str()) + 2);
            ((text as u16 + 4).max(30).min(fit), 3.min(s.h))
        } else {
            // Border, query row, the items (at least one row), border.
            let max_h = (s.h * 3 / 5).max(5);
            (fit.min(72), (items.max(1) + 3).min(max_h).min(s.h))
        };
        let outer = Rect::new(
            s.x + s.w.saturating_sub(w) / 2,
            s.y + (s.h.saturating_sub(h)) / 3,
            w,
            h,
        );
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
        let focused = self.focused();
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
                        current: Some(id) == focused,
                    });
                }
            }
        }
        for id in self.scratch.panes() {
            items.push(Item {
                label: self.pane_title(id),
                detail: "scratchpad".into(),
                target: Target::Pane(id),
                current: Some(id) == focused,
            });
        }
        self.picker = Some(Picker::new(Kind::Panes, "panes", items));
        self.dirty = true;
    }

    fn session_items(&self) -> Vec<Item> {
        self.session_list()
            .into_iter()
            .map(|(i, name, panes, shown)| Item {
                label: name,
                detail: format!(
                    "{panes} pane{}{}",
                    if panes == 1 { "" } else { "s" },
                    if shown { " · current" } else { "" }
                ),
                target: Target::Session(i),
                current: shown,
            })
            .collect()
    }

    pub(super) fn open_session_switcher(&mut self) {
        let mut p = Picker::new(
            Kind::Sessions,
            "sessions  (type a new name to create · ctrl+r renames)",
            self.session_items(),
        );
        p.select_current();
        self.picker = Some(p);
        self.dirty = true;
    }

    /// Choose where the current workspace goes. It starts on the first session
    /// that is not this one: staying put is never what the key was pressed for.
    pub(super) fn open_move_workspace(&mut self) {
        if let Err(e) = self.movable_workspace() {
            self.status = Some(e);
            return;
        }
        let mut p = Picker::new(
            Kind::MoveWorkspace,
            format!(
                "send workspace {} to  (type a new name to create)",
                self.current
            ),
            self.session_items(),
        );
        p.selected = p.visible().iter().position(|it| !it.current).unwrap_or(0);
        self.picker = Some(p);
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

    /// Ask every server how it is, off the UI thread: one of them may be slow
    /// to answer, and this server's own answer comes from this very loop. The
    /// switcher opens when the list arrives (`AppEvent::Servers`).
    pub(super) fn list_servers(&mut self) {
        let tx = self.tx.clone();
        let _ = std::thread::Builder::new()
            .name("servers".into())
            .spawn(move || {
                let _ = tx.send(crate::pane::AppEvent::Servers(crate::client::servers()));
            });
    }

    pub(super) fn open_server_switcher(&mut self, list: Vec<crate::proto::Status>) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let items = server_items(
            &list,
            crate::ipc::socket_path(),
            self.client_inside.as_deref(),
            now,
        );
        let mut p = Picker::new(
            Kind::Servers,
            "servers  (Enter moves this terminal there · ctrl+x kills)",
            items,
        );
        p.select_current();
        self.picker = Some(p);
        self.dirty = true;
    }

    /// Enter on a server: this one is a no-op, the one this terminal runs
    /// inside is refused, any other is where the client goes.
    fn pick_server(&mut self, name: String) {
        let sock = crate::ipc::server_socket(&name);
        if crate::ipc::socket_path() == Some(sock.as_path()) {
            return;
        }
        self.switch_requested = Some(name);
    }

    fn confirm_kill(&mut self, name: String) {
        if crate::ipc::socket_path() == Some(crate::ipc::server_socket(&name).as_path()) {
            self.status = Some(format!(
                "server {name} is this one: quit (leader Delete) ends it"
            ));
            return;
        }
        self.picker = Some(Picker::question(
            Kind::ConfirmKill(name.clone()),
            "kill server",
            format!(
                "Kill server {name}, closing every shell in it?   y or Enter kills · any other key cancels"
            ),
        ));
        self.dirty = true;
    }

    /// `ranma kill NAME`, from inside: the other server quits on its own loop,
    /// so this one only sends the word.
    fn kill_server(&mut self, name: String) {
        let tx = self.tx.clone();
        let _ = std::thread::Builder::new()
            .name("kill-server".into())
            .spawn(move || {
                let (text, level) = match crate::client::kill(&name) {
                    Ok(()) => (format!("server {name} quit"), crate::toast::Level::Normal),
                    Err(e) => (
                        format!("could not kill server {name}: {e}"),
                        crate::toast::Level::Urgent,
                    ),
                };
                let _ = tx.send(crate::pane::AppEvent::Toast {
                    text,
                    level,
                    timeout: None,
                });
            });
    }

    pub(super) fn confirm_quit(&mut self) {
        let n = self.panes.len();
        let s = self.session_count();
        let what = match (n, s) {
            (1, _) => "the last pane".to_string(),
            (n, 1) => format!("{n} panes"),
            (n, s) => format!("{n} panes in {s} sessions"),
        };
        self.picker = Some(Picker::question(
            Kind::ConfirmQuit,
            "quit ranma",
            format!("Close {what}?   y or Enter quits · any other key cancels"),
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

    /// Help (`?`) and the command palette (`:`) are one picker: both lists are
    /// in it, and the query's prefix says which one shows.
    pub(super) fn open_palette(&mut self, mode: PaletteMode) {
        let mut items = self.help_items();
        items.extend(self.command_items());
        let completions = self.user_completions();
        let mut p = Picker::palette(items, mode);
        p.user_commands = completions;
        self.picker = Some(p);
        self.dirty = true;
    }

    /// Each user command's name with what its `complete` offers now: a list
    /// as given, or what its function returns (called once, as the palette
    /// opens).
    fn user_completions(&mut self) -> Vec<(String, Vec<String>)> {
        let specs: Vec<(String, Option<std::rc::Rc<mlua::RegistryKey>>)> = self
            .config
            .commands
            .iter()
            .map(|(n, c)| (n.clone(), c.complete.clone()))
            .collect();
        specs
            .into_iter()
            .map(|(name, complete)| {
                let values = complete
                    .and_then(|key| {
                        let v = self.config.lua.registry_value::<mlua::Value>(&key).ok()?;
                        match v {
                            mlua::Value::Function(f) => {
                                self.call_lua(|_| f.call::<Vec<String>>(()))
                            }
                            mlua::Value::Table(t) => t
                                .sequence_values::<String>()
                                .collect::<mlua::Result<Vec<_>>>()
                                .ok(),
                            _ => None,
                        }
                    })
                    .unwrap_or_default();
                (name, values)
            })
            .collect()
    }

    /// Every action in the catalogue, with its key when one runs it exactly,
    /// and the user's commands with what they say they do.
    fn command_items(&self) -> Vec<Item> {
        let keys = self.bound_keys();
        let user = self.config.commands.iter().map(|(name, c)| {
            let args = c.args.as_deref().unwrap_or("");
            Item {
                label: if args.is_empty() {
                    name.clone()
                } else {
                    format!("{name} {args}")
                },
                detail: c.desc.clone().unwrap_or_default(),
                target: Target::Action {
                    name: name.clone(),
                    needs_arg: crate::action::needs_arg(args),
                },
                current: false,
            }
        });
        crate::action::CATALOGUE
            .iter()
            .map(|(name, hint)| Item {
                label: if hint.is_empty() {
                    name.to_string()
                } else {
                    format!("{name} {hint}")
                },
                detail: keys.get(*name).cloned().unwrap_or_default(),
                target: Target::Action {
                    name: name.to_string(),
                    needs_arg: crate::action::needs_arg(hint),
                },
                current: false,
            })
            .chain(user)
            .collect()
    }

    /// The key each bound action is on, as `leader t` or `alt+left`, by the
    /// action as written (`toggle_floating`, `workspace 3`).
    fn bound_keys(&self) -> std::collections::HashMap<String, String> {
        let leader = self.config.settings.leader;
        let mut keys: std::collections::HashMap<String, String> = Default::default();
        for (global, table) in [
            (false, &self.config.binds),
            (true, &self.config.global_binds),
        ] {
            for (chord, bind) in table {
                if let crate::config::BindAction::Builtin(a) = &bind.action {
                    let k = if global {
                        chord.to_string()
                    } else {
                        format!("{leader} {chord}")
                    };
                    // One key is enough to say it is bound; the shortest reads best.
                    keys.entry(a.to_string())
                        .and_modify(|e| {
                            if k.len() < e.len() {
                                e.clone_from(&k)
                            }
                        })
                        .or_insert(k);
                }
            }
        }
        keys
    }

    /// `workspace_switcher`: the shown session's workspaces, the current one
    /// marked, each with its pane count; a name no workspace has opens one.
    pub(super) fn open_workspace_switcher(&mut self) {
        let count = |n: usize| {
            if n == 1 {
                "1 pane".to_string()
            } else {
                format!("{n} panes")
            }
        };
        let mut items: Vec<Item> = self
            .workspace_list()
            .into_iter()
            .map(|(n, current, _, _, _, name)| Item {
                label: match name {
                    Some(name) => format!("{n}:{name}"),
                    None => n.to_string(),
                },
                detail: count(self.workspaces.get(&n).map_or(0, |w| w.len())),
                target: Target::Workspace(n),
                current: current && !self.scratch_shown,
            })
            .collect();
        if !self.scratch.is_empty() {
            items.push(Item {
                label: "S scratchpad".into(),
                detail: count(self.scratch.len()),
                target: Target::Workspace(crate::app::SCRATCHPAD),
                current: self.scratch_shown,
            });
        }
        let mut p = Picker::new(Kind::Workspaces, "workspaces", items);
        p.select_current();
        self.picker = Some(p);
        self.dirty = true;
    }

    /// The right-click menu of a pane, at the pointer: what can be done to it,
    /// with the key that does it. It focuses the pane first, so every entry is
    /// the ordinary action on the focused pane.
    pub(super) fn open_pane_menu(&mut self, id: crate::layout::PaneId, x: u16, y: u16) {
        if !self.active().contains(id) {
            return;
        }
        if self.focused() != Some(id) {
            self.focus(id);
            self.relayout();
        }
        let ws = self.active();
        let floating = ws.is_floating(id);
        let grouped = ws.tree.is_grouped(id);
        let synced = self.is_synced(id);
        let mut entries: Vec<(&str, &str)> = vec![
            (if floating { "Tile" } else { "Float" }, "toggle_floating"),
            ("Fullscreen", "fullscreen"),
        ];
        if !floating && !self.scratch_shown {
            entries.push((if grouped { "Ungroup" } else { "Group" }, "toggle_group"));
            entries.push(("Swap with master", "swap_master"));
        }
        entries.extend([
            (
                if synced {
                    "Stop synced input"
                } else {
                    "Synced input"
                },
                "sync_toggle",
            ),
            ("Links on screen", "hints"),
            ("Copy mode", "copy_mode"),
            ("Rename", "rename_pane"),
            ("Move to an empty workspace", "move_to_workspace empty"),
            ("Move to the scratchpad", "move_to_scratchpad"),
            ("Close", "close_pane"),
        ]);
        if self.scratch_shown {
            entries.retain(|(_, a)| *a != "move_to_scratchpad");
        }
        let title = self.pane_title(id);
        self.open_menu(title, &entries, x, y);
    }

    /// The right-click menu of a workspace chip in the bar, at the pointer. As
    /// the pane menu focuses its pane, this one goes to its workspace first, so
    /// every entry is the ordinary action on the current workspace.
    pub(super) fn open_workspace_menu(&mut self, n: u8, x: u16, y: u16) {
        if n != self.current || self.scratch_shown {
            self.run_action(crate::action::Action::Workspace(
                crate::action::WorkspaceTarget::Index(n),
            ));
        }
        let mut entries: Vec<(&str, &str)> =
            vec![("New pane", "new_pane"), ("Rename", "rename_workspace")];
        let occupied = self.workspaces.get(&n).is_some_and(|w| !w.is_empty());
        if occupied {
            entries.push(("Equalize", "equalize"));
        }
        if self.movable_workspace().is_ok() {
            entries.push(("Send to another session", "move_workspace_to_session"));
        }
        entries.push(("All workspaces", "workspace_switcher"));
        let title = match self.workspaces.get(&n).and_then(|w| w.name.as_deref()) {
            Some(name) => format!("workspace {n}:{name}"),
            None => format!("workspace {n}"),
        };
        self.open_menu(title, &entries, x, y);
    }

    /// A menu at the pointer of `(label, action)` entries, each shown with the
    /// key bound to its action, so the menu also teaches the keys.
    fn open_menu(&mut self, title: String, entries: &[(&str, &str)], x: u16, y: u16) {
        let keys = self.bound_keys();
        let items = entries
            .iter()
            .map(|(label, action)| Item {
                label: label.to_string(),
                detail: keys.get(*action).cloned().unwrap_or_default(),
                target: Target::Run(action.to_string()),
                current: false,
            })
            .collect();
        self.picker = Some(Picker::new(Kind::Menu, title, items));
        self.menu_at = Some((x, y));
        self.dirty = true;
    }

    fn help_items(&self) -> Vec<Item> {
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
                // A Lua bind has no action text to show; its `desc` says
                // what it does, as it does in the which-key hint.
                let what = match (&bind.action, &bind.desc) {
                    (crate::config::BindAction::Lua(_), Some(d)) => d.as_str(),
                    _ => bind.label.as_str(),
                };
                items.push(Item {
                    label: format!("{keys:<22} {what}"),
                    detail: if global {
                        "global".into()
                    } else {
                        String::new()
                    },
                    target: Target::Bind(*chord, global),
                    current: false,
                });
            }
        }
        // Grouped by what they do, which is how you look for one: every "focus"
        // together, every "workspace" together.
        items.sort_by(|a, b| {
            let action = |i: &Item| i.label[22.min(i.label.len())..].to_string();
            action(a).cmp(&action(b)).then(a.label.cmp(&b.label))
        });
        items
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
        p.touched = true;
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
        if let Some(l) = self.sheet_layout() {
            return self.sheet_mouse(m, &l);
        }
        let Some(l) = self.picker_layout() else {
            return;
        };
        let (x, y) = (m.column, m.row);
        match m.kind {
            // A click is Enter on that row, so it does what Enter would: an action
            // that needs an argument completes rather than runs.
            MouseEventKind::Down(_) if l.list.contains(x, y) => {
                let i = l.offset + (y - l.list.y) as usize;
                let Some(p) = self.picker.as_mut() else {
                    return;
                };
                if i < p.visible().len() {
                    p.selected = i;
                    let enter = KeyEvent::new(
                        crossterm::event::KeyCode::Enter,
                        crossterm::event::KeyModifiers::NONE,
                    );
                    let outcome = p.key(&enter);
                    self.dirty = true;
                    self.picker_outcome(outcome);
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

    pub(super) fn sheet_outcome(&mut self, outcome: Outcome) {
        self.picker_outcome(outcome);
    }

    fn picker_outcome(&mut self, outcome: Outcome) {
        let kind = self.picker.as_ref().map(|p| p.kind.clone());
        match outcome {
            Outcome::Open => {}
            Outcome::Cancel => {
                self.close_picker();
                if matches!(kind, Some(Kind::Lua | Kind::LuaInput)) {
                    self.lua_ui_done(None);
                }
            }
            Outcome::Rename(i) => self.open_rename_prompt(i),
            Outcome::Kill(name) => self.confirm_kill(name),
            Outcome::Submit(text) => {
                self.picker = None;
                match kind {
                    Some(Kind::RenameSession(i)) => self.rename_session(i, &text),
                    Some(Kind::RenameWorkspace(n)) => self.rename_workspace(n, &text),
                    Some(Kind::RenamePane(id)) => self.rename_pane(id, &text),
                    Some(Kind::SaveLayout) => self.save_layout(&text),
                    Some(Kind::ConfirmRestore) => self.accept_restore(text == "run"),
                    Some(Kind::ConfirmQuit) => self.quit = true,
                    Some(Kind::ConfirmUpdate) => self.run_action(crate::action::Action::Update),
                    Some(Kind::ConfirmKill(name)) => self.kill_server(name),
                    Some(Kind::LuaInput) => self.lua_ui_done(Some(LuaChoice::Text(text))),
                    _ => {}
                }
            }
            Outcome::Accept(target) => {
                self.sheet_from = None;
                let query = self
                    .picker
                    .take()
                    .map(|p| p.query.trim().to_string())
                    .unwrap_or_default();
                match (kind, target) {
                    (Some(Kind::MoveWorkspace), Target::Session(i)) => {
                        self.move_workspace_to_session(Some(i), None)
                    }
                    (Some(Kind::MoveWorkspace), Target::NewSession) => {
                        self.move_workspace_to_session(None, Some(&query))
                    }
                    (_, Target::Session(i)) => self.switch_session(i),
                    (_, Target::NewSession) => self.new_session(Some(&query)),
                    (_, Target::Pane(id)) => self.reveal_pane(id),
                    (_, Target::Server(name)) => self.pick_server(name),
                    (_, Target::Bind(chord, global)) => self.run_help_bind(chord, global),
                    (_, Target::Action { name: line, .. } | Target::Run(line)) => {
                        self.run_command(&line)
                    }
                    (_, Target::Button(toolbar, i)) => self.run_button(&toolbar, i),
                    (_, Target::Workspace(crate::app::SCRATCHPAD)) => {
                        if !self.scratch_shown {
                            self.run_action(crate::action::Action::ScratchpadToggle);
                        }
                    }
                    (_, Target::Workspace(n)) => self.run_action(crate::action::Action::Workspace(
                        crate::action::WorkspaceTarget::Index(n),
                    )),
                    (_, Target::NewWorkspace) => {
                        self.run_action(crate::action::Action::Workspace(
                            crate::action::WorkspaceTarget::Empty,
                        ));
                        if !query.is_empty() {
                            self.rename_workspace(self.current, &query);
                        }
                    }
                    (_, Target::Lua(i)) => self.lua_ui_done(Some(LuaChoice::Item(i, query))),
                    (_, Target::Invalid) => {}
                }
            }
        }
        self.dirty = true;
    }

    /// A plugin's picker (`ranma.picker`): its items, matched as any picker's.
    pub(super) fn open_lua_picker(&mut self, spec: crate::luaui::PickerSpec) {
        let items = spec
            .items
            .into_iter()
            .enumerate()
            .map(|(i, (label, detail))| Item {
                label,
                detail,
                target: Target::Lua(i),
                current: false,
            })
            .collect();
        self.lua_ui = Some(spec.hooks);
        self.picker = Some(Picker::new(Kind::Lua, spec.title, items));
        self.dirty = true;
    }

    /// A plugin's prompt (`ranma.input`).
    pub(super) fn open_lua_input(&mut self, spec: crate::luaui::InputSpec) {
        self.lua_ui = Some(spec.hooks);
        self.picker = Some(Picker::prompt(Kind::LuaInput, spec.title, &spec.text));
        self.dirty = true;
    }

    /// A plugin's picker or prompt closed: call it back with what was chosen,
    /// or `on_cancel` with nothing. The hooks are taken first, so a callback
    /// can open the next picker.
    fn lua_ui_done(&mut self, choice: Option<LuaChoice>) {
        let Some(hooks) = self.lua_ui.take() else {
            return;
        };
        let lua = &self.config.lua;
        let get = |k: &Option<std::rc::Rc<mlua::RegistryKey>>| {
            k.as_ref()
                .and_then(|k| lua.registry_value::<mlua::Function>(k).ok())
        };
        match choice {
            None => {
                if let Some(f) = get(&hooks.on_cancel) {
                    self.call_lua(|_| f.call::<()>(()));
                }
            }
            Some(LuaChoice::Text(text)) => {
                if let Some(f) = get(&hooks.on_select) {
                    self.call_lua(|_| f.call::<()>(text));
                }
            }
            Some(LuaChoice::Item(i, query)) => {
                let items = hooks
                    .items
                    .as_ref()
                    .and_then(|k| lua.registry_value::<mlua::Table>(k).ok());
                if let (Some(f), Some(items)) = (get(&hooks.on_select), items) {
                    self.call_lua(|_| f.call::<()>((items.raw_get::<mlua::Value>(i + 1)?, query)));
                }
            }
        }
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

    /// Run a command line from the palette, as `ranma action` would. Like a bind
    /// from help, it leaves ranma in normal mode: the palette is not a mode.
    fn run_command(&mut self, line: &str) {
        let line = line.trim();
        let (name, arg) = match line.split_once(char::is_whitespace) {
            Some((n, a)) => (n, Some(a.trim())),
            None => (line, None),
        };
        if let Some(key) = self.config.commands.get(name).map(|c| c.func.clone()) {
            // A user command gets what follows its name, or nil.
            if let Ok(f) = self.config.lua.registry_value::<mlua::Function>(&key) {
                let arg = arg.filter(|a| !a.is_empty()).map(str::to_string);
                self.call_lua(|_| f.call::<()>(arg));
            }
            if self.mode == super::Mode::Wm {
                self.set_mode(super::Mode::Normal);
            }
            return;
        }
        match line.parse::<crate::action::Action>() {
            Ok(a) => self.run_action(a),
            Err(e) => self.status = Some(e.to_string()),
        }
        if self.mode == super::Mode::Wm {
            self.set_mode(super::Mode::Normal);
        }
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

/// The server switcher's rows, from what `status` answered. `own` is this
/// server's socket and `inside` the socket of the ranma the client runs inside;
/// `now` is seconds since the epoch.
pub(super) fn server_items(
    list: &[crate::proto::Status],
    own: Option<&std::path::Path>,
    inside: Option<&str>,
    now: u64,
) -> Vec<Item> {
    list.iter()
        .map(|s| {
            let sock = crate::ipc::server_socket(&s.name);
            let current = own == Some(sock.as_path());
            let mut detail = vec![
                if current {
                    "this one".to_string()
                } else if inside == sock.to_str() {
                    "this terminal runs inside it".to_string()
                } else if s.attached {
                    "attached elsewhere".to_string()
                } else {
                    "detached".to_string()
                },
                format!("{} pane{}", s.panes, if s.panes == 1 { "" } else { "s" }),
                s.sessions.join(", "),
                crate::client::ago(now.saturating_sub(s.last_active)),
            ];
            if s.build != crate::update::BUILD_SHA {
                detail.push("other build".into());
            }
            Item {
                label: s.name.clone(),
                detail: detail.join(" · "),
                target: Target::Server(s.name.clone()),
                current,
            }
        })
        .collect()
}

/// What a plugin's picker or prompt ended with.
enum LuaChoice {
    /// The item at this index (from 0), and the query typed.
    Item(usize, String),
    /// The text a prompt was submitted with.
    Text(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::Status;

    fn status(name: &str, attached: bool, build: &str) -> Status {
        Status {
            name: name.into(),
            attached,
            clients: usize::from(attached),
            panes: if name == "1" { 1 } else { 6 },
            sessions: vec!["main".into(), "ai".into()],
            last_active: 1000,
            build: build.into(),
        }
    }

    #[test]
    fn server_rows_say_where_you_are_and_what_each_holds() {
        let here = crate::update::BUILD_SHA;
        let list = [
            status("1", true, here),
            status("2", true, "older"),
            status("3", false, here),
            status("4", false, here),
        ];
        let own = crate::ipc::server_socket("1");
        let inside = crate::ipc::server_socket("4");
        let items = server_items(&list, Some(&own), inside.to_str(), 1000 + 46 * 60);
        let details: Vec<&str> = items.iter().map(|i| i.detail.as_str()).collect();
        assert_eq!(
            details,
            [
                "this one · 1 pane · main, ai · 46m ago",
                "attached elsewhere · 6 panes · main, ai · 46m ago · other build",
                "detached · 6 panes · main, ai · 46m ago",
                "this terminal runs inside it · 6 panes · main, ai · 46m ago",
            ]
        );
        assert!(items[0].current && !items[1].current);
        assert_eq!(items[2].target, Target::Server("3".into()));
    }

    #[test]
    fn ctrl_x_asks_before_killing_and_only_a_yes_kills() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let items = server_items(&[status("2", false, "x")], None, None, 1000);
        let mut p = Picker::new(Kind::Servers, "servers", items);
        let ctrl_x = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL);
        assert_eq!(p.key(&ctrl_x), Outcome::Kill("2".into()));
        // Elsewhere, Ctrl+X is not a kill.
        let mut sessions = Picker::new(Kind::Sessions, "sessions", Vec::new());
        assert_eq!(sessions.key(&ctrl_x), Outcome::Open);

        let config = crate::config::load_from(None, None, None).unwrap();
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut app = App::new(config, tx, 80, 24);
        app.confirm_kill("2".into());
        assert_eq!(
            app.picker().map(|p| p.kind.clone()),
            Some(Kind::ConfirmKill("2".into()))
        );
        app.picker_key(&KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(app.picker().is_none(), "anything but yes cancels");
    }

    #[test]
    fn enter_on_a_server_asks_the_event_loop_to_move_there() {
        let config = crate::config::load_from(None, None, None).unwrap();
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut app = App::new(config, tx, 80, 24);
        app.open_server_switcher(vec![status("2", true, "x"), status("3", false, "x")]);
        app.picker_key(&KeyEvent::new(
            crossterm::event::KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        ));
        app.picker_key(&KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(app.switch_requested.as_deref(), Some("3"));
        app.run_action("attach 7".parse().unwrap());
        assert_eq!(app.switch_requested.as_deref(), Some("7"));
    }

    #[test]
    fn help_names_a_lua_bind_by_its_desc() {
        let config = crate::config::load_from(
            None,
            None,
            Some(
                r#"ranma.bind("h", function() end, { desc = "history" })
ranma.bind("y", function() end)"#,
            ),
        )
        .unwrap();
        let (tx, _rx) = std::sync::mpsc::channel();
        let a = App::new(config, tx, 80, 24);
        let labels: Vec<String> = a.help_items().into_iter().map(|i| i.label).collect();
        assert!(
            labels
                .iter()
                .any(|l| l.contains(" h ") && l.ends_with("history")),
            "{labels:?}"
        );
        assert!(
            labels
                .iter()
                .any(|l| l.contains(" y ") && l.ends_with("<lua function>"))
        );
    }
}

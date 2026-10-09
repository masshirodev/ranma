//! Saving the current workspace as a layout and bringing one back (DESIGN.md,
//! "Layouts: tmux's presets, and saved ones"). The schema and the files are
//! `crate::layouts`; this is the part that reads panes and spawns them.

use super::*;
use crate::layouts::{self, Spec};
use crate::picker::{Item, Kind, Picker, Target};

/// Where a layout of that name comes from.
enum Source {
    Declared,
    Saved,
}

impl App {
    /// The layout called `name`: declared in `init.lua` first, then saved.
    fn find_layout(&self, name: &str) -> Result<(Spec, Source), String> {
        if let Some(spec) = self.config.layouts.get(name) {
            return Ok((spec.clone(), Source::Declared));
        }
        let dir = self
            .layouts_dir
            .as_deref()
            .ok_or("no directory for saved layouts")?;
        match layouts::load(dir, name) {
            Ok(Some(spec)) => Ok((spec, Source::Saved)),
            Ok(None) => Err(format!("no layout named {name}")),
            Err(e) => Err(format!("{e:#}")),
        }
    }

    /// Every layout by name, declared and saved, for the picker.
    fn layout_names(&self) -> Vec<(String, Source)> {
        let mut out: Vec<(String, Source)> = self
            .config
            .layouts
            .keys()
            .map(|n| (n.clone(), Source::Declared))
            .collect();
        if let Some(dir) = &self.layouts_dir {
            for n in layouts::saved(dir) {
                // A declared one wins, so a file of the same name is not offered.
                if !self.config.layouts.contains_key(&n) {
                    out.push((n, Source::Saved));
                }
            }
        }
        out
    }

    pub(super) fn open_layout_picker(&mut self) {
        let items: Vec<Item> = self
            .layout_names()
            .into_iter()
            .map(|(name, src)| Item {
                detail: match src {
                    Source::Declared => "init.lua".into(),
                    Source::Saved => "saved".into(),
                },
                target: Target::Run(format!("load_layout {name}")),
                label: name,
                current: false,
            })
            .collect();
        if items.is_empty() {
            self.status =
                Some("no layouts yet: save_layout NAME, or ranma.layout in init.lua".into());
        } else {
            self.picker = Some(Picker::new(Kind::Layouts, "load a layout", items));
        }
        self.dirty = true;
    }

    pub(super) fn open_save_layout_prompt(&mut self) {
        let name = self.active().name.clone().unwrap_or_default();
        self.picker = Some(Picker::prompt(
            Kind::SaveLayout,
            "save this workspace as the layout",
            &name,
        ));
        self.dirty = true;
    }

    /// The shell new panes run, by the name its process shows.
    fn shell_name(&self) -> String {
        self.config
            .settings
            .shell
            .clone()
            .or_else(|| std::env::var("SHELL").ok().filter(|s| !s.is_empty()))
            .unwrap_or_else(|| "/bin/sh".into())
    }

    /// Write the current workspace's tiles to the layout `name`.
    pub(super) fn save_layout(&mut self, name: &str) {
        self.dirty = true;
        let name = name.trim();
        if !layouts::valid_name(name) {
            self.status = Some(format!(
                "`{name}` is not a layout name (no slashes or spaces)"
            ));
            return;
        }
        if self.config.layouts.contains_key(name) {
            self.status = Some(format!(
                "{name} is declared in init.lua: change it there, or save under another name"
            ));
            return;
        }
        if self.scratch_shown {
            self.status = Some("scratchpad panes always float: nothing to save".into());
            return;
        }
        let Some(root) = self.active().tree.root.clone() else {
            self.status = Some("no tiled panes here to save".into());
            return;
        };
        let Some(dir) = self.layouts_dir.clone() else {
            self.status = Some("no directory for saved layouts".into());
            return;
        };
        let home = dirs::home_dir();
        let spec = if self.in_strip() {
            Spec::from_strip(&root, &self.pane_about())
        } else {
            Spec::from_node(&root, &self.pane_about())
        };
        let n = spec.panes().len();
        self.status = Some(match layouts::save(&dir, name, &spec) {
            Ok(path) => format!(
                "saved {n} pane{} as {name} ({})",
                if n == 1 { "" } else { "s" },
                layouts::tilde(&path, home.as_deref())
            ),
            Err(e) => format!("save_layout: {e:#}"),
        });
    }

    /// Apply the layout `name` to the current workspace: its panes fill the
    /// layout in tree order, what it lacks is spawned, what it has beyond the
    /// layout is placed after it. Nothing is closed.
    pub(super) fn load_layout(&mut self, name: &str) {
        self.dirty = true;
        if self.scratch_shown {
            self.status = Some("scratchpad panes always float: no layout there".into());
            return;
        }
        let spec = match self.find_layout(name) {
            Ok((spec, _)) => spec,
            Err(e) => {
                self.status = Some(e);
                return;
            }
        };
        let (opened, failed) = self.apply_spec(&spec, Typing::Run);
        self.relayout();
        self.status = Some(match failed {
            Some(e) => format!("layout {name}: a pane failed to start: {e}"),
            None if opened > 0 => format!(
                "layout {name}: {opened} pane{} opened",
                if opened == 1 { "" } else { "s" }
            ),
            None => format!("layout {name}"),
        });
    }

    /// What a pane is doing, as a layout keeps it: its directory, and the
    /// command in its foreground when that is not its shell.
    pub(super) fn pane_about(&self) -> impl Fn(PaneId) -> (Option<String>, Option<String>) + '_ {
        let home = dirs::home_dir();
        let shell = self.shell_name();
        move |id| {
            let Some(p) = self.panes.get(&id) else {
                return (None, None);
            };
            (
                p.cwd().map(|c| layouts::tilde(&c, home.as_deref())),
                p.foreground_command(&shell)
                    .map(|argv| layouts::shell_join(&argv)),
            )
        }
    }

    /// Put `spec` on the shown workspace (not the scratchpad): its panes fill
    /// the layout in tree order, the rest is spawned. Returns how many were
    /// opened and the first failure. The caller relayouts.
    pub(super) fn apply_spec(&mut self, spec: &Spec, typing: Typing) -> (usize, Option<String>) {
        let existing = self.active().tree.panes();
        let leaves = spec.panes();
        let mut ids = Vec::with_capacity(leaves.len());
        let mut fresh: Vec<(PaneId, Spec)> = Vec::new();
        for (i, leaf) in leaves.iter().enumerate() {
            match existing.get(i) {
                Some(id) => ids.push(*id),
                None => {
                    let id = self.next_id;
                    self.next_id += 1;
                    ids.push(id);
                    fresh.push((id, (*leaf).clone()));
                }
            }
        }
        let strip = self.strip_here();
        let (screen, min) = (self.workspace_area().w, self.config.settings.scroll_min);
        let ws = self.active_mut();
        ws.tree.root = Some(spec.to_node(&mut ids.into_iter()));
        // A strip loaded into a strip keeps its columns; a plain tree becomes
        // a column per pane; a strip loaded elsewhere has its widths read as
        // weights.
        match (spec.scroll, strip) {
            (true, true) => ws.tree.mark_strip(true),
            (false, true) => ws.tree.mark_strip(false),
            (true, false) => {
                ws.tree.mark_strip(true);
                ws.tree.leave_strip(screen, min);
            }
            (false, false) => {}
        }
        ws.fullscreen = false;
        ws.preset = None;
        for extra in existing.iter().skip(leaves.len()) {
            let last = ws.tree.panes().last().copied();
            ws.tree.insert(*extra, last, None, Placement::Dwindle);
        }
        let mut failed = None;
        let mut opened = 0;
        for (id, leaf) in fresh {
            match self.spawn_into(id, leaf.cwd.as_deref(), leaf.command.as_deref(), typing) {
                Ok(()) => opened += 1,
                Err(e) => {
                    self.active_mut().tree.remove(id);
                    failed.get_or_insert(e);
                }
            }
        }
        let focus = self
            .active()
            .focused
            .filter(|f| self.active().contains(*f))
            .or_else(|| self.active().tree.panes().first().copied());
        if let Some(f) = focus {
            self.focus(f);
        }
        (opened, failed)
    }

    /// Spawn pane `id`, already placed in the shown workspace, at the size it
    /// is laid out at: a shell in `cwd` (where new_pane would start one when
    /// `None`, home when it is gone), `command` typed into it.
    pub(super) fn spawn_into(
        &mut self,
        id: PaneId,
        cwd: Option<&str>,
        command: Option<&str>,
        typing: Typing,
    ) -> Result<(), String> {
        let home = dirs::home_dir();
        let cwd = match cwd {
            Some(c) => Some(layouts::expand(c, home.as_deref()))
                .filter(|p| p.is_dir())
                .or(home),
            None => self
                .focused()
                .or(self.last_focused)
                .and_then(|f| self.panes.get(&f))
                .and_then(|p| p.cwd()),
        };
        let frame = self.frame();
        let size = frame
            .views
            .iter()
            .find(|v| v.id == id)
            .map(|v| v.inner)
            .or_else(|| frame.hidden.iter().find(|(h, _)| *h == id).map(|(_, r)| *r))
            .map_or(Size { cols: 80, rows: 24 }, |r| Size {
                cols: r.w,
                rows: r.h,
            });
        let s = &self.config.settings;
        let opts = SpawnOptions {
            shell: s.shell.as_deref(),
            command: None,
            scrollback_lines: s.scrollback_lines,
            cwd,
            env: &[],
        };
        let pane = Pane::spawn(id, size, &opts, self.tx.clone()).map_err(|e| format!("{e:#}"))?;
        // Typed, not run in the shell's place: a command that ends leaves its
        // shell, and is in its history to run again. Waiting, it is on the
        // prompt for one Enter.
        if let Some(cmd) = command {
            let line = match typing {
                Typing::Run => format!("{cmd}\r"),
                Typing::Wait => cmd.to_string(),
            };
            pane.write(line.into_bytes());
        }
        self.panes.insert(id, pane);
        let ws = if self.scratch_shown {
            SCRATCHPAD
        } else {
            self.current
        };
        self.emit(HookEvent::PaneOpen, |t| {
            t.set("pane", id)?;
            t.set("workspace", ws)
        });
        Ok(())
    }
}

/// What a respawned pane does with its command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Typing {
    /// Typed and run, as if Enter followed.
    Run,
    /// Typed and left on the prompt.
    Wait,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(user: Option<&str>, tag: &str) -> App {
        let config = crate::config::load_from(None, None, user).unwrap();
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut a = App::new(config, tx, 120, 40);
        let dir =
            std::env::temp_dir().join(format!("ranma-app-layouts-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        a.layouts_dir = Some(dir);
        a
    }

    fn with_pane(a: &mut App, id: PaneId) {
        let ws = a.workspaces.get_mut(&1).unwrap();
        ws.tree.insert(id, None, None, Placement::Dwindle);
        ws.focused = Some(id);
    }

    const DEV: &str = r#"
        ranma.layout("dev", {
          split = "horizontal",
          { size = 3 },
          { split = "vertical", {}, {} },
        })
    "#;

    #[test]
    fn existing_panes_fill_a_layout_in_order_and_none_are_closed() {
        let mut a = app(Some(DEV), "fill");
        for id in 1..=4 {
            with_pane(&mut a, id);
        }
        a.load_layout("dev");
        let tree = &a.active().tree;
        assert_eq!(tree.panes(), vec![1, 2, 3, 4], "every pane kept, in order");
        // 1, 2, 3 took the layout's places; 1 is the wide one.
        let w = |id| {
            a.frame()
                .views
                .iter()
                .find(|v| v.id == id)
                .map(|v| v.outer.w)
                .unwrap()
        };
        assert!(w(1) > w(2), "{} vs {}", w(1), w(2));
        assert_eq!(a.status.as_deref(), Some("layout dev"));
    }

    #[test]
    fn unknown_and_declared_names_are_said_so() {
        let mut a = app(Some(DEV), "names");
        a.load_layout("nope");
        assert_eq!(a.status.as_deref(), Some("no layout named nope"));
        with_pane(&mut a, 1);
        a.save_layout("dev");
        assert!(
            a.status
                .as_deref()
                .unwrap()
                .contains("declared in init.lua")
        );
        a.save_layout("two words");
        assert!(a.status.as_deref().unwrap().contains("not a layout name"));
    }

    #[test]
    fn a_saved_layout_is_offered_beside_the_declared_ones() {
        let mut a = app(Some(DEV), "picker");
        with_pane(&mut a, 1);
        with_pane(&mut a, 2);
        a.save_layout("mine");
        assert!(
            a.status
                .as_deref()
                .unwrap()
                .starts_with("saved 2 panes as mine"),
            "{:?}",
            a.status
        );
        a.open_layout_picker();
        let p = a.picker.as_ref().expect("a picker");
        let items = p.visible();
        let labels: Vec<(&str, &str)> = items
            .iter()
            .map(|i| (i.label.as_str(), i.detail.as_str()))
            .collect();
        assert_eq!(labels, vec![("dev", "init.lua"), ("mine", "saved")]);
        assert_eq!(items[1].target, Target::Run("load_layout mine".into()));
        // Loaded back onto the same two panes, the shape is the saved one.
        let before = a.active().tree.root.clone();
        a.active_mut().tree.apply_preset(Preset::EvenVertical, 0.5);
        a.load_layout("mine");
        assert_eq!(a.active().tree.root, before);
    }

    #[test]
    fn nothing_to_save_and_nowhere_to_load() {
        let mut a = app(None, "empty");
        a.save_layout("x");
        assert_eq!(a.status.as_deref(), Some("no tiled panes here to save"));
        a.open_layout_picker();
        assert!(a.picker.is_none());
        assert!(a.status.as_deref().unwrap().starts_with("no layouts yet"));
        a.scratch_shown = true;
        a.load_layout("x");
        assert!(a.status.as_deref().unwrap().contains("scratchpad"));
    }
}

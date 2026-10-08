//! The settings panel in the app (`leader p`, the `settings` action): what it
//! changes and when. The panel itself, its drawing and its keys, is
//! `crate::settings`; this is where an edit becomes ranma's: applied live
//! (the workspace beside the panel shows it), saved to `settings.toml`, or
//! put back.
//!
//! An edit is applied by rebuilding the settings and the theme from what
//! they were when the panel opened, with every unsaved edit over them,
//! through the same strict parsing a config file gets. An edit that does not
//! parse is refused there and taken back, and the panel says why.

use std::time::Instant;

use crossterm::event::{Event, KeyEvent, KeyEventKind};
use toml::Value;

use super::App;
use crate::config::{self, Event as HookEvent};
use crate::options::Home;
use crate::settings::{self, Outcome, Panel};

/// The panel, and ranma as it was when the panel opened.
pub struct SettingsState {
    pub panel: Panel,
    settings: config::Settings,
    /// The theme as it was: the panel's border is drawn in its style.
    pub theme: crate::theme::Theme,
    /// The colours the panel draws itself with: the ones it opened with, so
    /// editing a colour the panel uses does not repaint the panel under you.
    pub colors: crate::theme::Colors,
}

/// The smallest screen the panel opens on.
const MIN_W: u16 = 30;
const MIN_H: u16 = 14;

impl App {
    pub fn settings_panel(&self) -> Option<&SettingsState> {
        self.settings.as_ref()
    }

    pub(super) fn open_settings(&mut self) {
        if self.screen.w < MIN_W || self.screen.h < MIN_H {
            self.status = Some(format!(
                "settings needs a screen of {MIN_W}×{MIN_H} at least"
            ));
            return;
        }
        let values = self.config.values.borrow();
        let entries = settings::entries(&values.options, &values.layers);
        drop(values);
        let panel = Panel::new(entries, self.palette(), theme_names());
        self.settings = Some(SettingsState {
            panel,
            settings: self.config.settings.clone(),
            theme: self.config.theme.clone(),
            colors: self.config.theme.colors.clone(),
        });
        self.picker = None;
        self.relayout();
    }

    /// The theme's own colours, each once, in role order: what ←→ steps a
    /// colour option through.
    fn palette(&self) -> Vec<crate::theme::Color> {
        let c = toml::Value::try_from(&self.config.theme.colors).unwrap_or(Value::Boolean(false));
        let mut out: Vec<crate::theme::Color> = Vec::new();
        for (role, _, _) in crate::options::COLOR_ROLES {
            if let Some(col) = c
                .get(role)
                .and_then(Value::as_str)
                .and_then(|s| s.parse().ok())
                && col != crate::theme::Color::Default
                && !out.contains(&col)
            {
                out.push(col);
            }
        }
        out
    }

    /// The panel has the keyboard while it is open; the mouse is left alone.
    pub(super) fn settings_input(&mut self, ev: Event) {
        match ev {
            Event::Key(k) if k.kind != KeyEventKind::Release => self.settings_key(&k),
            Event::Paste(text) => {
                if let Some((t, _)) = self.settings.as_mut().and_then(|s| s.panel.edit.as_mut()) {
                    t.push_str(text.trim_end_matches('\n'));
                    self.dirty = true;
                }
            }
            Event::Resize(w, h) => {
                self.screen = crate::layout::Rect::new(0, 0, w, h);
                self.relayout();
            }
            _ => {}
        }
    }

    fn settings_key(&mut self, k: &KeyEvent) {
        let Some(st) = self.settings.as_mut() else {
            return;
        };
        let before = st.panel.entries.clone();
        let outcome = st.panel.key(k);
        self.dirty = true;
        match outcome {
            Outcome::Stay => {}
            Outcome::Relayout => self.relayout(),
            Outcome::Changed => {
                let changed: Vec<(String, Option<Value>, Option<Value>)> = {
                    let st = self.settings.as_ref().expect("checked above");
                    st.panel
                        .entries
                        .iter()
                        .zip(&before)
                        .filter(|(a, b)| a.eff() != b.eff())
                        .map(|(a, b)| (a.opt.key.clone(), a.eff(), b.eff()))
                        .collect()
                };
                match self.apply_settings_edits() {
                    Ok(()) => {
                        for (key, value, previous) in changed {
                            self.emit_option_change(&key, value, previous, false);
                        }
                    }
                    Err(e) => {
                        let st = self.settings.as_mut().expect("checked above");
                        st.panel.entries = before;
                        st.panel.error = Some(e);
                        let _ = self.apply_settings_edits();
                    }
                }
            }
            Outcome::Save => self.save_settings(false),
            Outcome::SaveClose => self.save_settings(true),
            Outcome::Discard => {
                let reverted: Vec<(String, Option<Value>, Option<Value>)> = self
                    .settings
                    .as_ref()
                    .map(|st| {
                        st.panel
                            .unsaved()
                            .iter()
                            .map(|e| (e.opt.key.clone(), e.saved(), e.eff()))
                            .collect()
                    })
                    .unwrap_or_default();
                if let Some(st) = self.settings.take() {
                    self.config.settings = st.settings;
                    self.own_colors = None;
                    self.config.theme = st.theme;
                    self.adopt_colors();
                }
                self.clear_pending_values();
                for (key, value, previous) in reverted {
                    self.emit_option_change(&key, value, previous, false);
                }
                self.relayout();
            }
            Outcome::Close => {
                self.settings = None;
                self.clear_pending_values();
                self.relayout();
            }
        }
    }

    fn clear_pending_values(&self) {
        let mut v = self.config.values.borrow_mut();
        v.layers.pending_set.clear();
        v.layers.pending_theme.clear();
    }

    /// Rebuild the settings and the theme from what they were when the panel
    /// opened, with every unsaved edit over them.
    fn apply_settings_edits(&mut self) -> Result<(), String> {
        let st = self.settings.as_ref().expect("the panel is open");
        let mut set = toml::Table::new();
        let mut plugin = toml::Table::new();
        let mut theme = st_panel_theme(self);
        let mut unset_theme: Vec<String> = Vec::new();
        let mut unset_set: Vec<String> = Vec::new();
        for e in st.panel.entries.iter().filter(|e| e.pend.is_some()) {
            let v = e.eff();
            match (e.opt.home, v, e.opt.plugin) {
                (Home::Init, Some(v), true) => crate::options::set(&mut plugin, &e.opt.key, v),
                (Home::Init, Some(v), false) => crate::options::set(&mut set, &e.opt.key, v),
                (Home::Init, None, _) => unset_set.push(e.opt.key.clone()),
                (Home::Theme, Some(v), _) => crate::options::set(&mut theme, &e.opt.key, v),
                (Home::Theme, None, _) => {
                    crate::options::remove(&mut theme, &e.opt.key);
                    unset_theme.push(e.opt.key.clone());
                }
            }
        }
        let mut settings = st.settings.clone();
        config::patch_settings(&mut settings, set.clone(), "settings").map_err(strip_who)?;
        for k in &unset_set {
            match k.as_str() {
                "shell" => settings.shell = None,
                "paste.image_command" => settings.paste_image_command = None,
                _ => {}
            }
        }
        let dirs = crate::theme::theme_dirs(config::config_dir().as_deref());
        let (new_theme, _) =
            crate::theme::load_over_unset(&settings.theme, &dirs, Some(&theme), &unset_theme)
                .map_err(|e| format!("{e:#}"))?;
        self.config.settings = settings;
        self.own_colors = None;
        self.config.theme = new_theme;
        self.adopt_colors();
        {
            let mut v = self.config.values.borrow_mut();
            let mut pending = set;
            crate::options::merge(&mut pending, plugin);
            v.layers.pending_set = pending;
            let st = self.settings.as_ref().expect("the panel is open");
            let mut pt = toml::Table::new();
            for e in st
                .panel
                .entries
                .iter()
                .filter(|e| e.pend.is_some() && e.opt.home == Home::Theme)
            {
                if let Some(val) = e.eff() {
                    crate::options::set(&mut pt, &e.opt.key, val);
                }
            }
            v.layers.pending_theme = pt;
        }
        self.module_generation += 1;
        self.module_values.clear();
        self.schedule_modules(Instant::now());
        self.relayout();
        Ok(())
    }

    /// Write the edits to `settings.toml`. The config watcher then reloads,
    /// which reads them back the way a start does; until it does, they are
    /// already in force, since they were applied as they were made.
    fn save_settings(&mut self, close: bool) {
        let Some(dir) = config::config_dir() else {
            self.settings_error("no config directory to save settings.toml in");
            return;
        };
        let path = dir.join(config::SETTINGS_FILE);
        let st = self.settings.as_ref().expect("the panel is open");
        // An optional value set back to unset over a file that sets it: TOML
        // cannot say "unset", so the file would win again on the next load.
        if let Some(e) = st
            .panel
            .unsaved()
            .into_iter()
            .find(|e| e.eff().is_none() && e.file.is_some())
        {
            let msg = format!(
                "{}: unset it in {}; settings.toml can only set values",
                e.opt.name,
                e.file_name()
            );
            self.settings_error(&msg);
            return;
        }
        let mut file = match config::read_settings_file(&path) {
            Ok(f) => f,
            Err(e) => {
                self.settings_error(&format!("{e:#}"));
                return;
            }
        };
        settings::saved_layers(&st.panel.entries, &mut file.set, &mut file.theme);
        if let Err(e) = std::fs::create_dir_all(&dir)
            .map_err(anyhow::Error::from)
            .and_then(|()| config::write_settings_file(&path, &file))
        {
            self.settings_error(&format!("{e:#}"));
            return;
        }
        let saved: Vec<(String, Option<Value>)> = st
            .panel
            .unsaved()
            .iter()
            .map(|e| (e.opt.key.clone(), e.eff()))
            .collect();
        {
            let mut v = self.config.values.borrow_mut();
            v.layers.panel_set = file.set;
            v.layers.panel_theme = file.theme;
            v.layers.pending_set.clear();
            v.layers.pending_theme.clear();
        }
        for (key, value) in saved {
            self.emit_option_change(&key, value.clone(), value, true);
        }
        if close {
            self.settings = None;
            self.relayout();
        } else {
            self.refresh_settings_panel();
        }
        self.status = Some(format!("saved {}", path.display()));
    }

    fn settings_error(&mut self, msg: &str) {
        match self.settings.as_mut() {
            Some(st) => st.panel.error = Some(msg.to_string()),
            None => self.status = Some(msg.to_string()),
        }
    }

    /// The panel's rows again from the configuration's layers, keeping the
    /// selection, the filter and the view: after a save, and after a reload.
    pub(super) fn refresh_settings_panel(&mut self) {
        let Some(st) = self.settings.as_mut() else {
            return;
        };
        let values = self.config.values.borrow();
        let entries = settings::entries(&values.options, &values.layers);
        drop(values);
        st.panel.entries = entries;
        st.settings = self.config.settings.clone();
        st.theme = self.config.theme.clone();
        if st.panel.selected().is_none() {
            st.panel.sel = st
                .panel
                .entries
                .first()
                .map(|e| e.opt.key.clone())
                .unwrap_or_default();
        }
        self.dirty = true;
    }

    fn emit_option_change(
        &mut self,
        key: &str,
        value: Option<Value>,
        previous: Option<Value>,
        saved: bool,
    ) {
        let lua = &self.config.lua;
        let to_lua = |v: &Option<Value>| -> mlua::Value {
            v.as_ref()
                .and_then(|v| mlua::LuaSerdeExt::to_value(lua, v).ok())
                .unwrap_or(mlua::Value::Nil)
        };
        let (value, previous) = (to_lua(&value), to_lua(&previous));
        let key = key.to_string();
        self.emit(HookEvent::OptionChange, |t| {
            t.set("key", key)?;
            t.set("value", value)?;
            t.set("previous", previous)?;
            t.set("saved", saved)
        });
    }
}

/// The panel's saved `[theme]` layer, which edits go over.
fn st_panel_theme(app: &App) -> toml::Table {
    app.config.values.borrow().layers.panel_theme.clone()
}

/// "settings: wm_mode: ..." reads as "wm_mode: ..." in the panel.
fn strip_who(e: String) -> String {
    e.strip_prefix("settings: ")
        .map(str::to_string)
        .unwrap_or(e)
}

/// The themes the panel's `theme` option cycles through: the built-in and
/// every file in the config's `themes/`.
fn theme_names() -> Vec<String> {
    let mut out = vec![crate::theme::DEFAULT_THEME_NAME.to_string()];
    for dir in crate::theme::theme_dirs(config::config_dir().as_deref()) {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                if p.extension()? != "toml" {
                    return None;
                }
                Some(p.file_stem()?.to_string_lossy().into_owned())
            })
            .filter(|n| !out.contains(n))
            .collect();
        names.sort();
        out.extend(names);
    }
    out
}

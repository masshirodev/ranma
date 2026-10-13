//! `choose_theme`: a picker over the themes there are, showing each as the
//! selection moves onto it (DESIGN.md, "A theme picker that shows each
//! theme"). Enter keeps it, written to `settings.toml` as the settings panel
//! writes its own changes; Esc, or anything else that closes the picker,
//! puts the theme back as it was.

use super::App;
use crate::config;
use crate::picker::{Item, Kind, Picker, Target};
use crate::theme::Theme;

/// The theme in force when the picker opened, to go back to.
pub(super) struct Preview {
    theme: Theme,
    name: String,
}

impl App {
    /// A theme as the configuration builds it: the named file, with
    /// `settings.toml`'s `[theme]` over it.
    fn theme_over(&self, name: &str) -> anyhow::Result<Theme> {
        let dirs = crate::theme::theme_dirs(config::config_dir().as_deref());
        let overlay = self.config.values.borrow().layers.panel_theme.clone();
        Ok(crate::theme::load_over(name, &dirs, Some(&overlay))?.0)
    }

    pub(super) fn open_theme_picker(&mut self) {
        let current = self.config.settings.theme.clone();
        let items = crate::theme::names(config::config_dir().as_deref())
            .into_iter()
            .map(|name| {
                // A theme that would not load says why, and is not shown.
                let detail = match self.theme_over(&name) {
                    Ok(_) => String::new(),
                    Err(e) => format!("✗ {}", first_line(&format!("{e:#}"))),
                };
                Item {
                    current: name == current,
                    label: name.clone(),
                    detail,
                    target: Target::Theme(name),
                }
            })
            .collect();
        let mut p = Picker::new(Kind::Themes, "themes", items);
        p.select_current();
        self.theme_preview = Some(Preview {
            theme: self.config.theme.clone(),
            name: current,
        });
        self.picker = Some(p);
        self.dirty = true;
    }

    /// The selection moved: show the theme under it.
    pub(super) fn preview_theme(&mut self) {
        let Some(p) = self.picker.as_ref().filter(|p| p.kind == Kind::Themes) else {
            return;
        };
        let Some(Target::Theme(name)) = p.visible().get(p.selected).map(|it| it.target.clone())
        else {
            return;
        };
        if let Ok(t) = self.theme_over(&name) {
            self.show_theme(t);
        }
    }

    fn show_theme(&mut self, theme: Theme) {
        self.own_colors = None;
        self.config.theme = theme;
        self.adopt_colors();
        self.module_generation += 1;
        self.module_values.clear();
        self.schedule_modules(std::time::Instant::now());
        self.relayout();
        self.dirty = true;
    }

    /// The picker closed without a choice: the theme it opened on again.
    pub(super) fn end_theme_preview(&mut self) {
        if let Some(prev) = self.theme_preview.take()
            && self.config.theme != prev.theme
        {
            self.show_theme(prev.theme);
            self.config.settings.theme = prev.name;
        }
    }

    /// Enter on a theme: keep it, and write it to `settings.toml` so the next
    /// start (and the reload the write sets off) has it too.
    pub(super) fn choose_theme(&mut self, name: String) {
        let previous = self
            .theme_preview
            .take()
            .map_or_else(|| self.config.settings.theme.clone(), |p| p.name);
        let theme = match self.theme_over(&name) {
            Ok(t) => t,
            Err(e) => {
                self.status = Some(format!("theme {name}: {e:#}"));
                return;
            }
        };
        self.show_theme(theme);
        self.config.settings.theme = name.clone();
        let Some(dir) = config::config_dir() else {
            self.status = Some("no config directory to save settings.toml in".into());
            return;
        };
        let path = dir.join(config::SETTINGS_FILE);
        let saved = config::read_settings_file(&path).and_then(|mut file| {
            file.set
                .insert("theme".into(), toml::Value::String(name.clone()));
            std::fs::create_dir_all(&dir)?;
            config::write_settings_file(&path, &file)?;
            Ok(file)
        });
        match saved {
            Ok(file) => {
                self.config.values.borrow_mut().layers.panel_set = file.set;
                self.emit_option_change(
                    "theme",
                    Some(toml::Value::String(name)),
                    Some(toml::Value::String(previous)),
                    true,
                );
            }
            Err(e) => self.status = Some(format!("saving the theme: {e:#}")),
        }
    }
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").to_string()
}
